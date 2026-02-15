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
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use arrow_array::{RecordBatch, RecordBatchIterator, StringArray, FixedSizeListArray, Float32Array, Array};
use arrow_schema::{Field, Schema, DataType};

const TABLE_NAME: &str = "file_embeddings";

#[derive(Debug, Serialize, Deserialize)]
struct Record {
    path: String,
    content: String,
    vector: Vec<f32>,
}

pub fn load_model(model: EmbeddingModel) -> Result<TextEmbedding> {
    let mut options = InitOptions::default();
    options.model_name = model;
    options.show_download_progress = true;
    TextEmbedding::try_new(options)
}

pub async fn index_directory<F>(
    root_dir: &str,
    db: &Connection,
    model: &mut TextEmbedding,
    progress_callback: F,
) -> Result<usize>
where
    F: Fn(String) + Send + 'static,
{
    let mut files_indexed = 0;
    
    // Ensure table exists
    let table = get_or_create_table(db).await?;

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

            // Simple chunking - in a real app use a proper splitter
            // For now, we just index the whole file if small, or first 8000 chars
            let chunk_size = 2000;
            let chunks: Vec<String> = text
                .chars()
                .collect::<Vec<char>>()
                .chunks(chunk_size)
                .map(|c| c.iter().collect::<String>())
                .collect();

            // Embed chunks
            // We only use the first few chunks to avoid massive processing for this lite version
            let chunks_to_embed = chunks.into_iter().take(5).collect::<Vec<_>>();
            
            if chunks_to_embed.is_empty() {
                continue;
            }

            let embeddings = model.embed(chunks_to_embed.clone(), None)?;

            let records: Vec<Record> = chunks_to_embed
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
    model: &mut TextEmbedding,
    query: &str,
    limit: usize,
) -> Result<Vec<(String, String, f32)>> {
    let table = db.open_table(TABLE_NAME).execute().await;
    
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

pub async fn reset_index(db_path: &Path) -> Result<()> {
     let db = lancedb::connect(&db_path.to_string_lossy()).execute().await?;
     let _ = db.drop_table(TABLE_NAME, &[]).await;
     Ok(())
}

async fn get_or_create_table(db: &Connection) -> Result<Table> {
    if db.table_names().execute().await?.contains(&TABLE_NAME.to_string()) {
        Ok(db.open_table(TABLE_NAME).execute().await?)
    } else {
        let schema = Arc::new(Schema::new(vec![
            Field::new("path", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new("vector", DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                 384 // MultilingualE5Small dim
            ), false),
        ]));
        
         let table = db.create_table(TABLE_NAME, 
            RecordBatchIterator::new(vec![], schema)
        ).execute().await?;
        
        Ok(table)
    }
}
