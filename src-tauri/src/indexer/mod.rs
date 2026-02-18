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
use rayon::prelude::*;
use tokio::sync::Mutex;

use crate::config::IndexingConfig;
use crate::state::ModelState;

use ignore::WalkBuilder;

pub use chunking::expand_query;
pub use db::reset_index;
pub use embedding::{embed_query, load_model, load_reranker, rerank_results};
pub use search::{build_filter_expr, hybrid_merge, search_files, search_fts};

const ANN_INDEX_THRESHOLD: usize = 256;
const EMBED_BATCH_SIZE: usize = 256;

struct ExtractedFile {
    path: String,
    chunks: Vec<String>,
    mtime: i64,
}

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
    indexing_config: &IndexingConfig,
    progress_callback: F,
) -> Result<usize>
where
    F: Fn(usize, usize, String) + Send + Sync + 'static,
{
    let dim = get_model_dim(model_state).await?;
    let table = db::get_or_create_table(db, table_name, dim).await?;

    let existing_mtimes = db::get_indexed_mtimes(&table).await.unwrap_or_default();

    let all_files: Vec<_> = WalkBuilder::new(root_dir)
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .add_custom_ignore_filename(".rcignore")
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
        .map(|e| e.into_path())
        .collect();
    let total_files = all_files.len();

    progress_callback(0, total_files, "Scanning files...".to_string());

    let image_files: Vec<_> = all_files
        .iter()
        .filter(|p| {
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            ocr::is_image_extension(&ext)
        })
        .cloned()
        .collect();

    let non_image_files: Vec<_> = all_files
        .iter()
        .filter(|p| {
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            !ocr::is_image_extension(&ext)
        })
        .cloned()
        .collect();

    let extracted: Vec<ExtractedFile> = non_image_files
        .par_iter()
        .filter_map(|path| {
            let path_str = path.to_string_lossy().to_string();
            let mtime = file_io::get_file_mtime(path);

            if let Some(&existing_mtime) = existing_mtimes.get(&path_str) {
                if existing_mtime == mtime {
                    return None;
                }
            }

            let text = file_io::read_file_content_with_config(path, indexing_config)?;
            if text.trim().is_empty() {
                return None;
            }

            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            let chunks = chunking::semantic_chunk_with_overrides(
                &text,
                &ext,
                indexing_config.chunk_size,
                indexing_config.chunk_overlap,
            );

            Some(ExtractedFile {
                path: path_str,
                chunks,
                mtime,
            })
        })
        .collect();

    let mut image_extracted: Vec<ExtractedFile> = Vec::new();
    for path in &image_files {
        let path_str = path.to_string_lossy().to_string();
        let mtime = file_io::get_file_mtime(path);

        if let Some(&existing_mtime) = existing_mtimes.get(&path_str) {
            if existing_mtime == mtime {
                continue;
            }
        }

        if let Some(text) = file_io::read_file_content_with_ocr(path) {
            if !text.trim().is_empty() {
                let ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                let chunks = chunking::semantic_chunk(&text, &ext);
                image_extracted.push(ExtractedFile {
                    path: path_str,
                    chunks,
                    mtime,
                });
            }
        }
    }

    let mut all_extracted = extracted;
    all_extracted.extend(image_extracted);
    let files_indexed = all_extracted.len();

    if files_indexed == 0 {
        progress_callback(total_files, total_files, "Done -- no new files".to_string());
        return Ok(0);
    }

    progress_callback(
        0,
        files_indexed,
        format!("Extracted {} files, starting embedding...", files_indexed),
    );

    let mut pending_chunks: Vec<db::PendingChunk> = Vec::new();
    let mut batches_written = 0;

    for (idx, ef) in all_extracted.iter().enumerate() {
        let safe_path = ef.path.replace('\'', "''");
        let _ = table.delete(&format!("path = '{}'", safe_path)).await;

        for chunk in &ef.chunks {
            pending_chunks.push(db::PendingChunk {
                path: ef.path.clone(),
                content: chunk.clone(),
                mtime: ef.mtime,
            });
        }

        if pending_chunks.len() >= EMBED_BATCH_SIZE {
            batches_written += 1;
            progress_callback(
                idx + 1,
                files_indexed,
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
            files_indexed,
            files_indexed,
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

    // Utiliser files_indexed comme approximation du volume total dans la table
    // pour décider si l'index ANN vaut le coup d'être construit.
    let total_indexed = files_indexed;

    if total_indexed >= ANN_INDEX_THRESHOLD {
        progress_callback(files_indexed, files_indexed, "Building vector index...".to_string());
        let _ = db::build_ann_index(&table).await;
    }

    progress_callback(files_indexed, files_indexed, "Building search index...".to_string());
    let _ = db::build_fts_index(&table).await;

    Ok(files_indexed)
}

pub async fn index_single_file(
    file_path: &std::path::Path,
    table_name: &str,
    db: &Connection,
    model_state: &Arc<Mutex<ModelState>>,
) -> Result<bool> {
    if !file_path.is_file() {
        return Ok(false);
    }

    let dim = get_model_dim(model_state).await?;
    let table = db::get_or_create_table(db, table_name, dim).await?;
    let path_str = file_path.to_string_lossy().to_string();
    let mtime = file_io::get_file_mtime(file_path);

    let existing_mtimes = db::get_indexed_mtimes(&table).await.unwrap_or_default();
    if let Some(&existing_mtime) = existing_mtimes.get(&path_str) {
        if existing_mtime == mtime {
            return Ok(false);
        }
    }

    let safe_path = path_str.replace('\'', "''");
    let _ = table.delete(&format!("path = '{}'", safe_path)).await;

    let ext = file_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    let text = if ocr::is_image_extension(&ext) {
        file_io::read_file_content_with_ocr(file_path)
    } else {
        file_io::read_file_content(file_path)
    };

    let text = match text {
        Some(t) if !t.trim().is_empty() => t,
        _ => return Ok(false),
    };

    let chunks = chunking::semantic_chunk(&text, &ext);
    if chunks.is_empty() {
        return Ok(false);
    }

    let texts: Vec<String> = chunks.clone();
    let embeddings = embed_batch(model_state, texts).await?;

    let records: Vec<db::Record> = chunks
        .into_iter()
        .zip(embeddings)
        .map(|(content, vector)| db::Record {
            path: path_str.clone(),
            content,
            vector,
            mtime,
        })
        .collect();

    let batch = db::create_record_batch(records)?;
    let schema = batch.schema();
    table
        .add(RecordBatchIterator::new(vec![Ok(batch)], schema))
        .execute()
        .await?;

    Ok(true)
}

pub async fn delete_file_from_index(
    file_path: &str,
    table_name: &str,
    db: &Connection,
) -> Result<()> {
    // Ouvrir la table existante sans créer ni recréer (évite la destruction
    // de l'index si la dimension ne correspond pas au modèle actuel).
    let table = match db.open_table(table_name).execute().await {
        Ok(t) => t,
        Err(_) => return Ok(()),
    };
    let safe_path = file_path.replace('\'', "''");
    let _ = table.delete(&format!("path = '{}'", safe_path)).await;
    Ok(())
}
