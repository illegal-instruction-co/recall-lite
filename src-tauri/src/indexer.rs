use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use anyhow::{anyhow, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use fastembed::{RerankInitOptions, RerankResult, RerankerModel, TextRerank};
use futures::TryStreamExt;
use lancedb::connection::Connection;
use lancedb::index::scalar::FullTextSearchQuery;
use lancedb::index::Index;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{DistanceType, Table};
use walkdir::WalkDir;

use arrow_array::{
    Float32Array, FixedSizeListArray, Int64Array, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};

const QUERY_PREFIX: &str = "query: ";
const PASSAGE_PREFIX: &str = "passage: ";
const CHUNK_MAX_BYTES: usize = 800;
const CHUNK_OVERLAP_BYTES: usize = 200;
const ANN_INDEX_THRESHOLD: usize = 256;
const EMBED_BATCH_SIZE: usize = 64;

struct Record {
    path: String,
    content: String,
    vector: Vec<f32>,
    mtime: i64,
}

fn is_text_extension(ext: &str) -> bool {
    matches!(
        ext,
        "txt" | "md" | "markdown"
            | "rs" | "toml" | "json" | "yaml" | "yml"
            | "js" | "ts" | "jsx" | "tsx"
            | "py" | "rb" | "go" | "java" | "c" | "cpp" | "h" | "hpp" | "cs"
            | "html" | "htm" | "xml" | "svg"
            | "css" | "scss" | "less"
            | "sql" | "sh" | "bash" | "ps1" | "bat" | "cmd"
            | "csv" | "tsv" | "log"
            | "ini" | "cfg" | "conf" | "env"
            | "dockerfile" | "makefile"
            | "tex" | "bib" | "rst" | "adoc"
    )
}

fn read_file_content(path: &Path) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    let is_dotfile = matches!(
        file_name.as_str(),
        "dockerfile" | "makefile" | ".gitignore" | ".env" | ".editorconfig"
    );

    if is_text_extension(&ext) || is_dotfile {
        fs::read_to_string(path).ok()
    } else if ext == "pdf" {
        pdf_extract::extract_text(path).ok()
    } else {
        None
    }
}

fn get_file_mtime(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn load_model(model: EmbeddingModel, cache_dir: std::path::PathBuf) -> Result<TextEmbedding> {
    let mut options = InitOptions::default();
    options.model_name = model;
    options.cache_dir = cache_dir.clone();
    options.show_download_progress = cfg!(debug_assertions);
    TextEmbedding::try_new(options)
}

pub fn load_reranker(cache_dir: std::path::PathBuf) -> Result<TextRerank> {
    let mut options = RerankInitOptions::default();
    options.model_name = RerankerModel::JINARerankerV2BaseMultiligual;
    options.cache_dir = cache_dir;
    options.show_download_progress = cfg!(debug_assertions);
    TextRerank::try_new(options).map_err(|e| anyhow!("Failed to load reranker: {}", e))
}

pub fn embed_passages(model: &mut TextEmbedding, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
    let prefixed: Vec<String> = texts
        .into_iter()
        .map(|t| format!("{}{}", PASSAGE_PREFIX, t))
        .collect();
    model
        .embed(prefixed, None)
        .map_err(|e| anyhow!("Embedding failed: {}", e))
}

pub fn embed_query(model: &mut TextEmbedding, query: &str) -> Result<Vec<f32>> {
    let prefixed = format!("{}{}", QUERY_PREFIX, query);
    let embeddings = model
        .embed(vec![prefixed], None)
        .map_err(|e| anyhow!("Embedding failed: {}", e))?;
    embeddings
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Empty embedding result"))
}

pub fn get_model_dimension(model: &mut TextEmbedding) -> Result<usize> {
    let probe = model
        .embed(vec!["dimension probe".to_string()], None)
        .map_err(|e| anyhow!("Dimension probe failed: {}", e))?;
    probe
        .first()
        .map(|v| v.len())
        .ok_or_else(|| anyhow!("No vector returned from dimension probe"))
}

pub fn rerank_results(
    reranker: &mut TextRerank,
    query: &str,
    results: &[(String, String, f32)],
) -> Result<Vec<(String, String, f32)>> {
    if results.is_empty() {
        return Ok(vec![]);
    }

    let documents: Vec<&str> = results.iter().map(|(_, snippet, _)| snippet.as_str()).collect();
    let reranked = reranker
        .rerank(query, &documents, false, None)
        .map_err(|e| anyhow!("Reranking failed: {}", e))?;

    Ok(reranked
        .into_iter()
        .map(|RerankResult { index, score, .. }| {
            let (path, snippet, _) = &results[index];
            (path.clone(), snippet.clone(), score)
        })
        .collect())
}

struct PendingChunk {
    path: String,
    content: String,
    mtime: i64,
}

pub async fn index_directory<F>(
    root_dir: &str,
    table_name: &str,
    db: &Connection,
    model: &mut TextEmbedding,
    progress_callback: F,
) -> Result<usize>
where
    F: Fn(String) + Send + 'static,
{
    let dim = get_model_dimension(model)?;
    let table = get_or_create_table(db, table_name, dim).await?;

    let existing_mtimes = get_indexed_mtimes(&table).await.unwrap_or_default();

    let mut pending_chunks: Vec<PendingChunk> = Vec::new();
    let mut files_seen = 0;

    for entry in WalkDir::new(root_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();
        let mtime = get_file_mtime(path);

        if let Some(&existing_mtime) = existing_mtimes.get(&path_str) {
            if existing_mtime == mtime {
                files_seen += 1;
                continue;
            }
        }

        let text = match read_file_content(path) {
            Some(t) if !t.trim().is_empty() => t,
            _ => continue,
        };

        let safe_path = path_str.replace('\'', "''");
        let _ = table.delete(&format!("path = '{}'", safe_path)).await;

        let chunks = chunk_with_overlap(&text, CHUNK_MAX_BYTES, CHUNK_OVERLAP_BYTES);
        for chunk in chunks {
            pending_chunks.push(PendingChunk {
                path: path_str.clone(),
                content: chunk,
                mtime,
            });
        }

        progress_callback(path_str);
        files_seen += 1;
    }

    if pending_chunks.is_empty() {
        return Ok(0);
    }

    let mut files_indexed = 0;
    let file_set: std::collections::HashSet<&str> = pending_chunks.iter().map(|c| c.path.as_str()).collect();
    files_indexed = file_set.len();

    for batch_start in (0..pending_chunks.len()).step_by(EMBED_BATCH_SIZE) {
        let batch_end = (batch_start + EMBED_BATCH_SIZE).min(pending_chunks.len());
        let batch_chunks = &pending_chunks[batch_start..batch_end];

        let texts: Vec<String> = batch_chunks.iter().map(|c| c.content.clone()).collect();
        let embeddings = embed_passages(model, texts)?;

        let records: Vec<Record> = batch_chunks
            .iter()
            .zip(embeddings)
            .map(|(chunk, vector)| Record {
                path: chunk.path.clone(),
                content: chunk.content.clone(),
                vector,
                mtime: chunk.mtime,
            })
            .collect();

        let batch = create_record_batch(records)?;
        let schema = batch.schema();
        table
            .add(RecordBatchIterator::new(vec![Ok(batch)], schema))
            .execute()
            .await?;
    }

    if files_seen >= ANN_INDEX_THRESHOLD {
        let _ = build_ann_index(&table).await;
    }

    let _ = build_fts_index(&table).await;

    Ok(files_indexed)
}

pub async fn search_files(
    db: &Connection,
    table_name: &str,
    query_vector: &[f32],
    limit: usize,
) -> Result<Vec<(String, String, f32)>> {
    let table = match db.open_table(table_name).execute().await {
        Ok(t) => t,
        Err(_) => return Ok(vec![]),
    };

    let search_limit = limit * 3;

    let results = table
        .vector_search(query_vector)?
        .distance_type(DistanceType::Cosine)
        .limit(search_limit)
        .execute()
        .await?
        .try_collect::<Vec<_>>()
        .await?;

    let mut best_per_file: HashMap<String, (String, f32)> = HashMap::new();

    for batch in results {
        let path_array = batch
            .column_by_name("path")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| anyhow!("Missing or invalid 'path' column"))?;

        let content_array = batch
            .column_by_name("content")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| anyhow!("Missing or invalid 'content' column"))?;

        let dist_array = batch
            .column_by_name("_distance")
            .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
            .ok_or_else(|| anyhow!("Missing or invalid '_distance' column"))?;

        for i in 0..batch.num_rows() {
            let path = path_array.value(i).to_string();
            let content = content_array.value(i).to_string();
            let dist = dist_array.value(i);

            match best_per_file.get(&path) {
                Some((_, existing_dist)) if *existing_dist <= dist => {}
                _ => {
                    best_per_file.insert(path, (content, dist));
                }
            }
        }
    }

    let mut matches: Vec<(String, String, f32)> = best_per_file
        .into_iter()
        .map(|(path, (content, dist))| (path, content, dist))
        .collect();

    matches.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
    matches.truncate(limit);

    Ok(matches)
}

pub async fn search_fts(
    db: &Connection,
    table_name: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<(String, String)>> {
    let table = match db.open_table(table_name).execute().await {
        Ok(t) => t,
        Err(_) => return Ok(vec![]),
    };

    let fts_query = FullTextSearchQuery::new(query.to_string());
    let results = table
        .query()
        .full_text_search(fts_query)
        .limit(limit)
        .execute()
        .await?
        .try_collect::<Vec<_>>()
        .await?;

    let mut matches = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for batch in results {
        let path_array = batch
            .column_by_name("path")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let content_array = batch
            .column_by_name("content")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());

        if let (Some(paths), Some(contents)) = (path_array, content_array) {
            for i in 0..batch.num_rows() {
                let path = paths.value(i).to_string();
                if seen_paths.insert(path.clone()) {
                    matches.push((path, contents.value(i).to_string()));
                }
                if matches.len() >= limit {
                    return Ok(matches);
                }
            }
        }
    }

    Ok(matches)
}

pub fn hybrid_merge(
    vector_results: &[(String, String, f32)],
    fts_results: &[(String, String)],
    limit: usize,
) -> Vec<(String, String, f32)> {
    let k = 60.0_f32;

    let mut rrf_scores: HashMap<String, (String, f32)> = HashMap::new();

    for (rank, (path, snippet, _)) in vector_results.iter().enumerate() {
        let score = 1.0 / (k + rank as f32 + 1.0);
        rrf_scores.insert(path.clone(), (snippet.clone(), score));
    }

    for (rank, (path, snippet)) in fts_results.iter().enumerate() {
        let score = 1.0 / (k + rank as f32 + 1.0);
        rrf_scores
            .entry(path.clone())
            .and_modify(|(_, s)| *s += score)
            .or_insert_with(|| (snippet.clone(), score));
    }

    let mut merged: Vec<(String, String, f32)> = rrf_scores
        .into_iter()
        .map(|(path, (snippet, score))| (path, snippet, score))
        .collect();

    merged.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(limit);
    merged
}

pub async fn reset_index(db_path: &Path, table_name: &str) -> Result<()> {
    let db = lancedb::connect(&db_path.to_string_lossy())
        .execute()
        .await?;
    let _ = db.drop_table(table_name, &[]).await;
    Ok(())
}

async fn build_ann_index(table: &Table) -> Result<()> {
    table
        .create_index(&["vector"], Index::Auto)
        .execute()
        .await?;
    Ok(())
}

async fn build_fts_index(table: &Table) -> Result<()> {
    let _ = table
        .create_index(&["content"], Index::FTS(Default::default()))
        .execute()
        .await;
    Ok(())
}

async fn get_indexed_mtimes(table: &Table) -> Result<HashMap<String, i64>> {
    let mut mtimes = HashMap::new();

    let results = table
        .query()
        .select(lancedb::query::Select::Columns(vec![
            "path".to_string(),
            "mtime".to_string(),
        ]))
        .execute()
        .await?
        .try_collect::<Vec<_>>()
        .await?;

    for batch in results {
        let path_array = batch
            .column_by_name("path")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let mtime_array = batch
            .column_by_name("mtime")
            .and_then(|c| c.as_any().downcast_ref::<Int64Array>());

        if let (Some(paths), Some(mtimes_col)) = (path_array, mtime_array) {
            for i in 0..batch.num_rows() {
                mtimes.insert(paths.value(i).to_string(), mtimes_col.value(i));
            }
        }
    }

    Ok(mtimes)
}

async fn get_or_create_table(db: &Connection, table_name: &str, dim: usize) -> Result<Table> {
    match db.open_table(table_name).execute().await {
        Ok(table) => {
            let schema = table.schema().await?;
            let has_mtime = schema.field_with_name("mtime").is_ok();
            if let Ok(field) = schema.field_with_name("vector") {
                if let DataType::FixedSizeList(_, size) = field.data_type() {
                    if *size == dim as i32 && has_mtime {
                        return Ok(table);
                    }
                }
            }
            let _ = db.drop_table(table_name, &[]).await;
        }
        Err(_) => {}
    }

    let schema = Arc::new(make_schema(dim));

    let table = db
        .create_table(table_name, RecordBatchIterator::new(vec![], schema))
        .execute()
        .await?;

    Ok(table)
}

fn make_schema(dim: usize) -> Schema {
    Schema::new(vec![
        Field::new("path", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dim as i32,
            ),
            false,
        ),
        Field::new("mtime", DataType::Int64, false),
    ])
}

fn create_record_batch(records: Vec<Record>) -> Result<RecordBatch> {
    if records.is_empty() {
        return Err(anyhow!("No records to convert"));
    }

    let dim = records[0].vector.len();
    let schema = Arc::new(make_schema(dim));

    let paths: Vec<String> = records.iter().map(|r| r.path.clone()).collect();
    let contents: Vec<String> = records.iter().map(|r| r.content.clone()).collect();
    let mtimes: Vec<i64> = records.iter().map(|r| r.mtime).collect();

    let mut flat_vectors = Vec::with_capacity(records.len() * dim);
    for r in &records {
        flat_vectors.extend_from_slice(&r.vector);
    }

    let vector_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        dim as i32,
        Arc::new(Float32Array::from(flat_vectors)),
        None,
    )?;

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(paths)),
            Arc::new(StringArray::from(contents)),
            Arc::new(vector_array),
            Arc::new(Int64Array::from(mtimes)),
        ],
    )
    .map_err(|e| anyhow!(e))
}

fn chunk_with_overlap(text: &str, max_bytes: usize, overlap_bytes: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let mut end = (start + max_bytes).min(text.len());

        while end < text.len() && !text.is_char_boundary(end) {
            end -= 1;
        }

        if end >= text.len() {
            chunks.push(text[start..].to_string());
            break;
        }

        let slice = &text[start..end];
        let split_at = slice
            .rfind('\n')
            .or_else(|| slice.rfind(". "))
            .or_else(|| slice.rfind(' '))
            .map(|i| start + i + 1)
            .unwrap_or(end);

        chunks.push(text[start..split_at].to_string());

        let rewind = overlap_bytes.min(split_at - start);
        let mut overlap_start = split_at - rewind;
        while overlap_start > start && !text.is_char_boundary(overlap_start) {
            overlap_start += 1;
        }
        start = overlap_start;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_with_overlap_basic() {
        let text = "Hello world. This is a test. Another sentence here.";
        let chunks = chunk_with_overlap(text, 30, 10);
        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| c.len() <= 31));
    }

    #[test]
    fn test_chunk_with_overlap_preserves_content() {
        let text = "ABCDEFGHIJ";
        let chunks = chunk_with_overlap(text, 5, 2);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn test_chunk_short_text() {
        let text = "Short";
        let chunks = chunk_with_overlap(text, 800, 200);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Short");
    }

    #[test]
    fn test_is_text_extension() {
        assert!(is_text_extension("py"));
        assert!(is_text_extension("tsx"));
        assert!(is_text_extension("rs"));
        assert!(is_text_extension("sql"));
        assert!(!is_text_extension("exe"));
        assert!(!is_text_extension("png"));
    }

    #[test]
    fn test_hybrid_merge() {
        let vector = vec![
            ("a.txt".to_string(), "hello".to_string(), 0.1),
            ("b.txt".to_string(), "world".to_string(), 0.2),
        ];
        let fts = vec![
            ("b.txt".to_string(), "world".to_string()),
            ("c.txt".to_string(), "new".to_string()),
        ];
        let merged = hybrid_merge(&vector, &fts, 10);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].0, "b.txt");
    }
}
