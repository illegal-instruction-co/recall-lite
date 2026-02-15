use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use futures::TryStreamExt;
use lancedb::connection::Connection;
use lancedb::index::Index;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{DistanceType, Table};
use walkdir::WalkDir;

use arrow_array::{
    Float32Array, FixedSizeListArray, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};

const QUERY_PREFIX: &str = "query: ";
const PASSAGE_PREFIX: &str = "passage: ";
const CHUNK_MAX_BYTES: usize = 800;
const CHUNK_OVERLAP_BYTES: usize = 200;
const ANN_INDEX_THRESHOLD: usize = 256;

struct Record {
    path: String,
    content: String,
    vector: Vec<f32>,
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

pub fn load_model(model: EmbeddingModel, cache_dir: std::path::PathBuf) -> Result<TextEmbedding> {
    let mut options = InitOptions::default();
    options.model_name = model;
    options.cache_dir = cache_dir;
    options.show_download_progress = cfg!(debug_assertions);
    TextEmbedding::try_new(options)
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
    let mut files_indexed = 0;

    for entry in WalkDir::new(root_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let text = match read_file_content(path) {
            Some(t) if !t.trim().is_empty() => t,
            _ => continue,
        };

        let chunks = chunk_with_overlap(&text, CHUNK_MAX_BYTES, CHUNK_OVERLAP_BYTES);
        if chunks.is_empty() {
            continue;
        }

        let embeddings = embed_passages(model, chunks.clone())?;

        let records: Vec<Record> = chunks
            .into_iter()
            .zip(embeddings)
            .map(|(content, vector)| Record {
                path: path.to_string_lossy().to_string(),
                content,
                vector,
            })
            .collect();

        let batch = create_record_batch(records)?;

        let safe_path = path.to_string_lossy().replace('\'', "''");
        let _ = table.delete(&format!("path = '{}'", safe_path)).await;

        let schema = batch.schema();
        table
            .add(RecordBatchIterator::new(vec![Ok(batch)], schema))
            .execute()
            .await?;

        progress_callback(path.to_string_lossy().to_string());
        files_indexed += 1;
    }

    if files_indexed >= ANN_INDEX_THRESHOLD {
        let _ = build_ann_index(&table).await;
    }

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

    let mut matches = Vec::new();
    let mut seen_paths = HashSet::new();

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
            if seen_paths.contains(&path) {
                continue;
            }

            seen_paths.insert(path.clone());
            matches.push((path, content_array.value(i).to_string(), dist_array.value(i)));

            if matches.len() >= limit {
                return Ok(matches);
            }
        }
    }

    Ok(matches)
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

async fn get_or_create_table(db: &Connection, table_name: &str, dim: usize) -> Result<Table> {
    match db.open_table(table_name).execute().await {
        Ok(table) => {
            let schema = table.schema().await?;
            if let Ok(field) = schema.field_with_name("vector") {
                if let DataType::FixedSizeList(_, size) = field.data_type() {
                    if *size == dim as i32 {
                        return Ok(table);
                    }
                }
            }
            let _ = db.drop_table(table_name, &[]).await;
        }
        Err(_) => {}
    }

    let schema = Arc::new(Schema::new(vec![
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
    ]));

    let table = db
        .create_table(table_name, RecordBatchIterator::new(vec![], schema))
        .execute()
        .await?;

    Ok(table)
}

fn create_record_batch(records: Vec<Record>) -> Result<RecordBatch> {
    if records.is_empty() {
        return Err(anyhow!("No records to convert"));
    }

    let dim = records[0].vector.len();
    let schema = Arc::new(Schema::new(vec![
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
    ]));

    let paths: Vec<String> = records.iter().map(|r| r.path.clone()).collect();
    let contents: Vec<String> = records.iter().map(|r| r.content.clone()).collect();

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
}
