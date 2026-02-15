use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use futures::TryStreamExt;
use lancedb::connection::Connection;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Table;
use walkdir::WalkDir;

use arrow_array::{RecordBatch, RecordBatchIterator, StringArray, FixedSizeListArray, Float32Array};
use arrow_schema::{Field, Schema, DataType};

#[derive(Debug)]
struct Record {
    path: String,
    content: String,
    vector: Vec<f32>,
}

pub fn load_model(model: EmbeddingModel, cache_dir: std::path::PathBuf) -> Result<TextEmbedding> {
    let mut options = InitOptions::default();
    options.model_name = model;
    options.cache_dir = cache_dir;
    
    options.show_download_progress = cfg!(debug_assertions);

    TextEmbedding::try_new(options)
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
    let mut files_indexed = 0;
    
    // Determine vector dimension dynamically
    let dummy_embedding = model.embed(vec!["test".to_string()], None)?;
    let dim = dummy_embedding.first().map(|v| v.len()).ok_or(anyhow!("Failed to determine model dimension"))?;

    // Ensure table exists with correct dimension
    let table = get_or_create_table(db, table_name, dim).await?;

    for entry in WalkDir::new(root_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file()) 
    {
        let path = entry.path();
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        
        let content = match extension.to_lowercase().as_str() {
            "txt" | "md" | "rs" | "toml" | "json" | "js" | "ts" => fs::read_to_string(path).ok(),
            "pdf" => pdf_extract::extract_text(path).ok(),
            _ => None,
        };

        if let Some(text) = content {
            if text.trim().is_empty() {
                continue;
            }

            let chunk_size = 2000;
            let chunks: Vec<String> = chunk_on_boundaries(&text, chunk_size);
            
            // Embed ALL chunks (removed .take(5) limit)
            if chunks.is_empty() {
                continue;
            }

            let embeddings = model.embed(chunks.clone(), None)?;

            let records: Vec<Record> = chunks
                .into_iter()
                .zip(embeddings)
                .map(|(text_chunk, vector)| Record {
                    path: path.to_string_lossy().to_string(),
                    content: text_chunk,
                    vector,
                })
                .collect();

            // Create RecordBatch manually
            let batch = create_record_batch(records)?;

            // Idempotency: Remove existing entries for this file
            // Escape single quotes in path for SQL-like filter
            let safe_path = path.to_string_lossy().replace("'", "''");
            let _ = table.delete(&format!("path = '{}'", safe_path)).await;

            // Add to LanceDB
            let schema = batch.schema();
            table.add(
                RecordBatchIterator::new(vec![Ok(batch)], schema)
            ).execute().await?;

            progress_callback(path.to_string_lossy().to_string());
            files_indexed += 1;
        }
    }

    Ok(files_indexed)
}

fn create_record_batch(records: Vec<Record>) -> Result<RecordBatch> {
    if records.is_empty() {
        return Err(anyhow!("No records to convert to batch"));
    }

    let dim = records[0].vector.len();
    let schema = Arc::new(Schema::new(vec![
        Field::new("path", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new("vector", DataType::FixedSizeList(
            Arc::new(Field::new("item", DataType::Float32, true)),
            dim as i32
        ), false),
    ]));

    let paths: Vec<String> = records.iter().map(|r| r.path.clone()).collect();
    let contents: Vec<String> = records.iter().map(|r| r.content.clone()).collect();
    
    let path_array = StringArray::from(paths);
    let content_array = StringArray::from(contents);

    // Flatten vectors
    let mut flat_vectors = Vec::with_capacity(records.len() * dim);
    for r in &records {
        flat_vectors.extend_from_slice(&r.vector);
    }
    
    let value_data = Float32Array::from(flat_vectors);
    let vector_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        dim as i32,
        Arc::new(value_data),
        None,
    )?;

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(path_array),
            Arc::new(content_array),
            Arc::new(vector_array),
        ]
    ).map_err(|e| anyhow!(e))
}

pub async fn search_files(
    db: &Connection,
    table_name: &str,
    model: &mut TextEmbedding,
    query: &str,
    limit: usize,
) -> Result<Vec<(String, String, f32)>> {
    let table = db.open_table(table_name).execute().await;
    
    // If table doesn't exist yet, return empty
    if table.is_err() {
        return Ok(vec![]);
    }
    let table = table.unwrap();

    let query_embedding = model.embed(vec![query.to_string()], None)?;
    let query_vector = query_embedding.first().ok_or(anyhow!("Failed to generate embedding"))?;

    // Request more results than limit to allow for deduplication
    let search_limit = limit * 3;

    let results = table
        .query()
        .nearest_to(query_vector.clone())?
        .limit(search_limit)
        .execute()
        .await?
        .try_collect::<Vec<_>>()
        .await?;

    let mut matches = Vec::new();
    let mut seen_paths = HashSet::new();

    for batch in results {
        let path_col = batch.column_by_name("path").ok_or(anyhow!("Missing path column"))?;
        let content_col = batch.column_by_name("content").ok_or(anyhow!("Missing content column"))?;
        let dist_col = batch.column_by_name("_distance").ok_or(anyhow!("Missing _distance column"))?;

        let path_array = path_col.as_any().downcast_ref::<StringArray>().ok_or(anyhow!("Invalid path array"))?;
        let content_array = content_col.as_any().downcast_ref::<StringArray>().ok_or(anyhow!("Invalid content array"))?;
        let dist_array = dist_col.as_any().downcast_ref::<Float32Array>().ok_or(anyhow!("Invalid distance array"))?;

        for i in 0..batch.num_rows() {
            let path = path_array.value(i).to_string();
            
            // Deduplication: Only take the first (best matching) chunk for each file
            // Note: LanceDB returns results sorted by distance/score within the query limit,
            // but batches might not be perfectly ordered if multiple chunks of same file exist?
            // "nearest_to" implies global usage, so yes, result stream should be ordered.
            if seen_paths.contains(&path) {
                continue;
            }

            let content = content_array.value(i).to_string();
            let dist = dist_array.value(i);
            
            seen_paths.insert(path.clone());
            matches.push((path, content, dist));

            if matches.len() >= limit {
                break;
            }
        }
        if matches.len() >= limit {
            break;
        }
    }

    Ok(matches)
}

pub async fn reset_index(db_path: &Path, table_name: &str) -> Result<()> {
     let db = lancedb::connect(&db_path.to_string_lossy()).execute().await?;
     let _ = db.drop_table(table_name, &[]).await;
     Ok(())
}

async fn get_or_create_table(db: &Connection, table_name: &str, dim: usize) -> Result<Table> {
    match db.open_table(table_name).execute().await {
        Ok(table) => {
            let schema = table.schema().await?;
            
            let mut schema_valid = false;
            if let Some(field) = schema.field_with_name("vector").ok() {
                if let DataType::FixedSizeList(_, size) = field.data_type() {
                    if *size == dim as i32 {
                        schema_valid = true;
                    }
                }
            }

            if schema_valid {
                return Ok(table);
            } else {
                let _ = db.drop_table(table_name, &[]).await;
            }
        }
        Err(_) => {}
    }

    // Create new table
    let schema = Arc::new(Schema::new(vec![
        Field::new("path", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new("vector", DataType::FixedSizeList(
            Arc::new(Field::new("item", DataType::Float32, true)),
            dim as i32
        ), false),
    ]));
    
    let table = db.create_table(table_name, 
        RecordBatchIterator::new(vec![], schema)
    ).execute().await?;
    
    Ok(table)
}

fn chunk_on_boundaries(text: &str, max_bytes: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + max_bytes).min(text.len());
        // Don't slice in the middle of a multi-byte UTF-8 character
        while end < text.len() && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == text.len() {
            chunks.push(text[start..].to_string());
            break;
        }
        // Walk back to nearest newline or space
        let slice = &text[start..end];
        let split_at = slice.rfind('\n')
            .or_else(|| slice.rfind(' '))
            .map(|i| start + i + 1)
            .unwrap_or(end);
        chunks.push(text[start..split_at].to_string());
        start = split_at;
    }
    chunks
}
