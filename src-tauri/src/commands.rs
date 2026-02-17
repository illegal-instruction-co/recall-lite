use std::sync::Arc;

use tauri::Emitter;
use tokio::sync::Mutex;

use crate::config::{get_table_name, ConfigState};
use crate::indexer;
use crate::state::{
    ContainerListItem, DbState, IndexingProgress, ModelState, RerankerState, SearchResult,
};

#[tauri::command]
pub async fn get_containers(
    config_state: tauri::State<'_, ConfigState>,
) -> Result<(Vec<ContainerListItem>, String), String> {
    let config = config_state.config.lock().await;
    let list: Vec<ContainerListItem> = config.containers.iter().map(|(name, info)| {
        ContainerListItem {
            name: name.clone(),
            description: info.description.clone(),
            indexed_paths: info.indexed_paths.clone(),
        }
    }).collect();
    Ok((list, config.active_container.clone()))
}

#[tauri::command]
pub async fn create_container(
    name: String,
    description: String,
    config_state: tauri::State<'_, ConfigState>,
) -> Result<(), String> {
    let mut config = config_state.config.lock().await;
    if config.containers.contains_key(&name) {
        return Err("Container already exists".to_string());
    }
    config.containers.insert(name, crate::config::ContainerInfo {
        description,
        indexed_paths: Vec::new(),
    });
    drop(config);
    config_state.save().await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_container(
    name: String,
    config_state: tauri::State<'_, ConfigState>,
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
) -> Result<(), String> {
    {
        let mut config = config_state.config.lock().await;
        if name == "Default" {
            return Err("Cannot delete Default container".to_string());
        }
        if config.active_container == name {
            config.active_container = "Default".to_string();
        }
        config.containers.remove(&name);
    }

    config_state.save().await?;

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };
    let table_name = get_table_name(&name);
    let _ = db.drop_table(&table_name, &[]).await;

    Ok(())
}

#[tauri::command]
pub async fn set_active_container(
    name: String,
    config_state: tauri::State<'_, ConfigState>,
) -> Result<(), String> {
    let mut config = config_state.config.lock().await;
    if !config.containers.contains_key(&name) {
        return Err("Container does not exist".to_string());
    }
    config.active_container = name;
    drop(config);
    config_state.save().await?;
    Ok(())
}

#[tauri::command]
pub async fn search(
    query: String,
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
    model_state: tauri::State<'_, Arc<Mutex<ModelState>>>,
    reranker_state: tauri::State<'_, Arc<Mutex<RerankerState>>>,
    config_state: tauri::State<'_, ConfigState>,
) -> Result<Vec<SearchResult>, String> {
    let table_name = {
        let config = config_state.config.lock().await;
        get_table_name(&config.active_container)
    };

    let query_vector = {
        let guard = match model_state.try_lock() {
            Ok(g) => g,
            Err(_) => return Err("Indexing in progress — search will be available when it finishes.".to_string()),
        };
        if let Some(err) = &guard.init_error {
            return Err(format!("Model failed to load: {}", err));
        }
        let mut guard = guard;
        let model = guard.model.as_mut().ok_or("AI model is loading... Please wait a moment.")?;
        indexer::embed_query(model, &query)
            .map_err(|e| e.to_string())?
    };

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };

    let query_variants = indexer::expand_query(&query);

    let vector_fut = indexer::search_files(&db, &table_name, &query_vector, 50);

    let fts_db = db.clone();
    let fts_table_name = table_name.clone();
    let fts_fut = async move {
        let futs: Vec<_> = query_variants
            .iter()
            .map(|variant| indexer::search_fts(&fts_db, &fts_table_name, variant, 30))
            .collect();
        let results = futures::future::join_all(futs).await;
        let mut all_fts: Vec<(String, String)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for result in results.into_iter().flatten() {
            for item in result {
                if seen.insert(item.0.clone()) {
                    all_fts.push(item);
                }
            }
        }
        all_fts
    };

    let (vector_result, fts_results) = tokio::join!(vector_fut, fts_fut);

    let vector_results = vector_result.map_err(|e| e.to_string())?;

    let merged = if fts_results.is_empty() {
        vector_results
    } else {
        indexer::hybrid_merge(&vector_results, &fts_results, 50)
    };

    let rerank_input: Vec<(String, String, f32)> = merged.into_iter().take(15).collect();

    let (final_results, used_reranker) = {
        let mut guard = reranker_state.lock().await;
        if let Some(reranker) = guard.reranker.take() {
            let query_clone = query.clone();
            let input_clone = rerank_input.clone();
            let result = tokio::task::spawn_blocking(move || {
                let mut r = reranker;
                let res = indexer::rerank_results(&mut r, &query_clone, &input_clone);
                (r, res)
            })
            .await
            .map_err(|e| e.to_string())?;
            let (reranker_back, rerank_res) = result;
            guard.reranker = Some(reranker_back);
            match rerank_res {
                Ok(reranked) => (reranked, true),
                Err(_) => (rerank_input, false),
            }
        } else {
            (rerank_input, false)
        }
    };

    let used_hybrid = !fts_results.is_empty();

    let mut scored: Vec<SearchResult> = if used_reranker {
        final_results
            .into_iter()
            .map(|(path, snippet, raw_score)| {
                let sigmoid = 1.0 / (1.0 + (-raw_score).exp());
                let pct = sigmoid * 100.0;
                SearchResult { path, snippet, score: pct }
            })
            .collect()
    } else if used_hybrid {
        let max_rrf = final_results.first().map(|(_, _, s)| *s).unwrap_or(1.0);
        final_results
            .into_iter()
            .map(|(path, snippet, rrf_score)| {
                let pct = if max_rrf > 0.0 { (rrf_score / max_rrf) * 100.0 } else { 0.0 };
                SearchResult { path, snippet, score: pct }
            })
            .collect()
    } else {
        final_results
            .into_iter()
            .map(|(path, snippet, cosine_dist)| {
                let similarity = (1.0 - cosine_dist).clamp(0.0, 1.0);
                let pct = similarity * 100.0;
                SearchResult { path, snippet, score: pct }
            })
            .collect()
    };

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    if used_reranker {
        scored.retain(|r| r.score >= 25.0);
    }
    scored.truncate(20);

    Ok(scored)
}

#[tauri::command]
pub async fn index_folder(
    app: tauri::AppHandle,
    dir: String,
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
    model_state: tauri::State<'_, Arc<Mutex<ModelState>>>,
    config_state: tauri::State<'_, ConfigState>,
) -> Result<String, String> {
    let table_name = {
        let config = config_state.config.lock().await;
        get_table_name(&config.active_container)
    };

    {
        let mut config = config_state.config.lock().await;
        let active = config.active_container.clone();
        if let Some(info) = config.containers.get_mut(&active) {
            if !info.indexed_paths.contains(&dir) {
                info.indexed_paths.push(dir.clone());
            }
        }
        drop(config);
        config_state.save().await?;
    }

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };

    let mut guard = model_state.lock().await;
    if let Some(err) = &guard.init_error {
        return Err(format!("Model init failed: {}", err));
    }
    let model = guard.model.as_mut().ok_or("Model is still loading...")?;

    let app_handle = app.clone();

    let count = indexer::index_directory(&dir, &table_name, &db, model, move |current, total, path| {
        let _ = app_handle.emit("indexing-progress", IndexingProgress { current, total, path });
    })
    .await
    .map_err(|e| e.to_string())?;

    let _ = app.emit("indexing-complete", format!("{} files indexed", count));

    Ok(format!("Indexed {} files", count))
}

#[tauri::command]
pub async fn reset_index(
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
    config_state: tauri::State<'_, ConfigState>,
) -> Result<String, String> {
    let table_name = {
        let config = config_state.config.lock().await;
        get_table_name(&config.active_container)
    };

    let path = {
        let guard = db_state.lock().await;
        guard.path.clone()
    };
    indexer::reset_index(&path, &table_name)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Index cleared successfully".to_string())
}

#[tauri::command]
pub async fn reindex_all(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
    model_state: tauri::State<'_, Arc<Mutex<ModelState>>>,
    config_state: tauri::State<'_, ConfigState>,
) -> Result<String, String> {
    let (table_name, paths) = {
        let config = config_state.config.lock().await;
        let info = config.containers.get(&config.active_container)
            .ok_or("Active container not found")?;
        (get_table_name(&config.active_container), info.indexed_paths.clone())
    };

    if paths.is_empty() {
        return Err("No folders to reindex".to_string());
    }

    let db_path = {
        let guard = db_state.lock().await;
        guard.path.clone()
    };
    indexer::reset_index(&db_path, &table_name)
        .await
        .map_err(|e| e.to_string())?;

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };

    let mut guard = model_state.lock().await;
    if let Some(err) = &guard.init_error {
        return Err(format!("Model init failed: {}", err));
    }
    let model = guard.model.as_mut().ok_or("Model is still loading...")?;

    let mut total = 0;
    for dir in &paths {
        let app_handle = app.clone();
        let count = indexer::index_directory(dir, &table_name, &db, model, move |current, total, path| {
            let _ = app_handle.emit("indexing-progress", IndexingProgress { current, total, path });
        })
        .await
        .map_err(|e| e.to_string())?;
        total += count;
    }

    let _ = app.emit("indexing-complete", format!("{} files reindexed from {} folders", total, paths.len()));

    Ok(format!("Reindexed {} files from {} folders", total, paths.len()))
}
