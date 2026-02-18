use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

use crate::config::{get_table_name, ConfigState};
use crate::indexer;
use crate::state::{
    ContainerListItem, DbState, IndexingProgress, ModelState, RerankerState, SearchResult,
};
use crate::watcher;

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
    app: tauri::AppHandle,
    name: String,
    config_state: tauri::State<'_, ConfigState>,
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
    model_state: tauri::State<'_, Arc<Mutex<ModelState>>>,
    watcher_state: tauri::State<'_, watcher::WatcherState>,
) -> Result<(), String> {
    let mut config = config_state.config.lock().await;
    if !config.containers.contains_key(&name) {
        return Err("Container does not exist".to_string());
    }
    config.active_container = name;
    drop(config);
    config_state.save().await?;

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };
    watcher::restart(
        watcher_state.inner(),
        config_state.inner(),
        db,
        model_state.inner().clone(),
        app,
    ).await;

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
        let mut guard = model_state.lock().await;
        if let Some(err) = &guard.init_error {
            return Err(format!("Model failed to load: {}", err));
        }
        let model = guard.model.as_mut().ok_or("AI model is loading... Please wait a moment.")?;
        indexer::embed_query(model, &query)
            .map_err(|e| e.to_string())?
    };

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };

    let (merged, used_hybrid) = indexer::search_pipeline(
        &db, &table_name, &query, &query_vector, 50, None, None,
    )
    .await
    .map_err(|e| e.to_string())?;

    let rerank_input: Vec<(String, String, f32)> = merged.into_iter().take(15).collect();

    let (final_results, used_reranker) = {
        let mut guard = reranker_state.lock().await;
        if let Some(reranker) = guard.reranker.take() {
            let (reranker_back, results, used) =
                indexer::safe_rerank(reranker, query.clone(), rerank_input.clone()).await;
            guard.reranker = reranker_back;
            if used {
                (results, true)
            } else {
                (rerank_input, false)
            }
        } else {
            (rerank_input, false)
        }
    };

    let scored = indexer::pipeline::score_results(final_results, used_reranker, used_hybrid, 20);

    Ok(scored
        .into_iter()
        .map(|r| SearchResult {
            path: r.path,
            snippet: r.snippet,
            score: r.score,
        })
        .collect())
}

#[tauri::command]
pub async fn index_folder(
    app: tauri::AppHandle,
    dir: String,
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
    model_state: tauri::State<'_, Arc<Mutex<ModelState>>>,
    config_state: tauri::State<'_, ConfigState>,
    watcher_state: tauri::State<'_, watcher::WatcherState>,
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

    let ms = model_state.inner().clone();
    let app_handle = app.clone();

    let indexing_config = {
        let config = config_state.config.lock().await;
        config.indexing.clone()
    };

    let count = indexer::index_directory(&dir, &table_name, &db, &ms, &indexing_config, move |current, total, path| {
        let _ = app_handle.emit("indexing-progress", IndexingProgress { current, total, path });
    })
    .await
    .map_err(|e| e.to_string())?;

    let _ = app.emit("indexing-complete", format!("{} files indexed", count));

    let db2 = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };
    watcher::restart(
        watcher_state.inner(),
        config_state.inner(),
        db2,
        model_state.inner().clone(),
        app,
    ).await;

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

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };

    let ms = model_state.inner().clone();

    let indexing_config = {
        let config = config_state.config.lock().await;
        config.indexing.clone()
    };

    let mut total = 0;
    for dir in &paths {
        let app_handle = app.clone();
        let count = indexer::index_directory(dir, &table_name, &db, &ms, &indexing_config, move |current, total, path| {
            let _ = app_handle.emit("indexing-progress", IndexingProgress { current, total, path });
        })
        .await
        .map_err(|e| e.to_string())?;
        total += count;
    }

    let _ = app.emit("indexing-complete", format!("{} files reindexed from {} folders", total, paths.len()));

    Ok(format!("Reindexed {} files from {} folders", total, paths.len()))
}

#[derive(Serialize)]
pub struct AppConfig {
    pub always_on_top: bool,
    pub launch_at_startup: bool,
    pub hotkey: String,
    pub use_git_history: bool,
    pub embedding_model: String,
    pub chunk_size: Option<usize>,
    pub chunk_overlap: Option<usize>,
    pub extra_extensions: Vec<String>,
    pub excluded_extensions: Vec<String>,
}

#[tauri::command]
pub async fn get_config(
    config_state: tauri::State<'_, ConfigState>,
) -> Result<AppConfig, String> {
    let config = config_state.config.lock().await;
    Ok(AppConfig {
        always_on_top: config.always_on_top,
        launch_at_startup: config.launch_at_startup,
        hotkey: config.hotkey.clone(),
        use_git_history: config.indexing.use_git_history,
        embedding_model: config.embedding_model.clone(),
        chunk_size: config.indexing.chunk_size,
        chunk_overlap: config.indexing.chunk_overlap,
        extra_extensions: config.indexing.extra_extensions.clone(),
        excluded_extensions: config.indexing.excluded_extensions.clone(),
    })
}

#[derive(Deserialize)]
pub struct ConfigUpdate {
    pub always_on_top: Option<bool>,
    pub launch_at_startup: Option<bool>,
    pub hotkey: Option<String>,
    pub use_git_history: Option<bool>,
    pub embedding_model: Option<String>,
    pub chunk_size: Option<Option<usize>>,
    pub chunk_overlap: Option<Option<usize>>,
    pub extra_extensions: Option<Vec<String>>,
    pub excluded_extensions: Option<Vec<String>>,
}

#[tauri::command]
pub async fn update_config(
    app: tauri::AppHandle,
    updates: ConfigUpdate,
    config_state: tauri::State<'_, ConfigState>,
) -> Result<(), String> {
    {
        let mut config = config_state.config.lock().await;

        if let Some(v) = updates.always_on_top {
            config.always_on_top = v;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_always_on_top(v);
            }
        }

        if let Some(v) = updates.launch_at_startup {
            config.launch_at_startup = v;
            use tauri_plugin_autostart::ManagerExt;
            let autostart = app.autolaunch();
            if v {
                let _ = autostart.enable();
            } else {
                let _ = autostart.disable();
            }
        }

        if let Some(ref v) = updates.hotkey {
            config.hotkey = v.clone();
        }

        if let Some(v) = updates.use_git_history {
            config.indexing.use_git_history = v;
        }

        if let Some(ref v) = updates.embedding_model {
            config.embedding_model = v.clone();
        }

        if let Some(v) = updates.chunk_size {
            config.indexing.chunk_size = v;
        }

        if let Some(v) = updates.chunk_overlap {
            config.indexing.chunk_overlap = v;
        }

        if let Some(ref v) = updates.extra_extensions {
            config.indexing.extra_extensions = v.clone();
        }

        if let Some(ref v) = updates.excluded_extensions {
            config.indexing.excluded_extensions = v.clone();
        }
    }

    config_state.save().await?;
    Ok(())
}
