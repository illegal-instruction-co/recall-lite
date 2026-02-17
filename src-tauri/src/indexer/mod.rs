pub mod chunking;
pub mod db;
pub mod embedding;
pub mod file_io;
pub mod ocr;
pub mod search;

use std::sync::Arc;

use anyhow::{anyhow, Result};
use arrow_array::RecordBatchIterator;
use lancedb::connection::Connection;
use tokio::sync::Mutex;

use crate::state::ModelState;

use walkdir::WalkDir;

pub use chunking::expand_query;
pub use db::reset_index;
pub use embedding::{embed_query, load_model, load_reranker, rerank_results};
pub use search::{hybrid_merge, search_files, search_fts};

const ANN_INDEX_THRESHOLD: usize = 256;
const EMBED_BATCH_SIZE: usize = 64;

async fn embed_batch(
    model_state: &Arc<Mutex<ModelState>>,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>> {
    let mut guard = model_state.lock().await;
    let model = guard
        .model
        .as_mut()
        .ok_or_else(|| anyhow!("Model not loaded"))?;
    embedding::embed_passages(model, texts)
}

async fn get_model_dim(model_state: &Arc<Mutex<ModelState>>) -> Result<usize> {
    let mut guard = model_state.lock().await;
    let model = guard
        .model
        .as_mut()
        .ok_or_else(|| anyhow!("Model not loaded"))?;
    embedding::get_model_dimension(model)
}

pub async fn index_directory<F>(
    root_dir: &str,
    table_name: &str,
    db: &Connection,
    model_state: &Arc<Mutex<ModelState>>,
    progress_callback: F,
) -> Result<usize>
where
    F: Fn(usize, usize, String) + Send + 'static,
{
    let dim = get_model_dim(model_state).await?;
    let table = db::get_or_create_table(db, table_name, dim).await?;

    let existing_mtimes = db::get_indexed_mtimes(&table).await.unwrap_or_default();

    let all_files: Vec<_> = WalkDir::new(root_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();
    let total_files = all_files.len();

    let mut pending_chunks: Vec<db::PendingChunk> = Vec::new();
    let mut files_indexed_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut files_seen = 0;
    let mut current_file = 0;
    let mut batches_written = 0;

    for path in &all_files {
        current_file += 1;
        let path_str = path.to_string_lossy().to_string();
        let mtime = file_io::get_file_mtime(path);

        if let Some(&existing_mtime) = existing_mtimes.get(&path_str) {
            if existing_mtime == mtime {
                files_seen += 1;
                progress_callback(current_file, total_files, path_str);
                continue;
            }
        }

        let text = match file_io::read_file_content(path) {
            Some(t) if !t.trim().is_empty() => t,
            _ => {
                progress_callback(current_file, total_files, path_str);
                continue;
            }
        };

        let safe_path = path_str.replace('\'', "''");
        let _ = table.delete(&format!("path = '{}'", safe_path)).await;

        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let chunks = chunking::semantic_chunk(&text, &ext);
        files_indexed_set.insert(path_str.clone());
        for chunk in chunks {
            pending_chunks.push(db::PendingChunk {
                path: path_str.clone(),
                content: chunk,
                mtime,
            });
        }

        progress_callback(current_file, total_files, path_str);
        files_seen += 1;

        if pending_chunks.len() >= EMBED_BATCH_SIZE {
            batches_written += 1;
            progress_callback(
                current_file,
                total_files,
                format!("Embedding batch {}", batches_written),
            );

            let batch_chunks: Vec<db::PendingChunk> = pending_chunks.drain(..).collect();
            let texts: Vec<String> = batch_chunks.iter().map(|c| c.content.clone()).collect();
            let embeddings = embed_batch(model_state, texts).await?;

            let records: Vec<db::Record> = batch_chunks
                .into_iter()
                .zip(embeddings)
                .map(|(chunk, vector)| db::Record {
                    path: chunk.path,
                    content: chunk.content,
                    vector,
                    mtime: chunk.mtime,
                })
                .collect();

            let batch = db::create_record_batch(records)?;
            let schema = batch.schema();
            table
                .add(RecordBatchIterator::new(vec![Ok(batch)], schema))
                .execute()
                .await?;
        }
    }

    if !pending_chunks.is_empty() {
        batches_written += 1;
        progress_callback(
            total_files,
            total_files,
            format!("Embedding batch {}", batches_written),
        );

        let texts: Vec<String> = pending_chunks.iter().map(|c| c.content.clone()).collect();
        let embeddings = embed_batch(model_state, texts).await?;

        let records: Vec<db::Record> = pending_chunks
            .into_iter()
            .zip(embeddings)
            .map(|(chunk, vector)| db::Record {
                path: chunk.path,
                content: chunk.content,
                vector,
                mtime: chunk.mtime,
            })
            .collect();

        let batch = db::create_record_batch(records)?;
        let schema = batch.schema();
        table
            .add(RecordBatchIterator::new(vec![Ok(batch)], schema))
            .execute()
            .await?;
    }

    let files_indexed = files_indexed_set.len();

    if files_indexed == 0 {
        progress_callback(total_files, total_files, "Done -- no new files".to_string());
        return Ok(0);
    }

    if files_seen >= ANN_INDEX_THRESHOLD {
        progress_callback(total_files, total_files, "Building vector index...".to_string());
        let _ = db::build_ann_index(&table).await;
    }

    progress_callback(total_files, total_files, "Building search index...".to_string());
    let _ = db::build_fts_index(&table).await;

    Ok(files_indexed)
}
