use std::sync::Arc;

use log::{info, error, debug};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

use crate::config::{get_table_name, ConfigState, EmbeddingProviderConfig};
use crate::indexer;
use crate::indexer::embedding_provider::RemoteProviderConfig;
use crate::state::{
    ContainerListItem, DbState, IndexingProgress, ProviderState, RerankerState, SearchResult,
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
    info!("create_container: name=\"{}\"", name);
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
    info!("delete_container: name=\"{}\"", name);
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
    provider_state: tauri::State<'_, Arc<Mutex<ProviderState>>>,
    watcher_state: tauri::State<'_, watcher::WatcherState>,
) -> Result<(), String> {
    info!("set_active_container: name=\"{}\"", name);
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
        provider_state.inner().clone(),
        app,
    ).await;

    Ok(())
}

#[tauri::command]
pub async fn search(
    query: String,
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
    provider_state: tauri::State<'_, Arc<Mutex<ProviderState>>>,
    reranker_state: tauri::State<'_, Arc<Mutex<RerankerState>>>,
    config_state: tauri::State<'_, ConfigState>,
) -> Result<Vec<SearchResult>, String> {
    debug!("search: query=\"{}\"", query);
    let table_name = {
        let config = config_state.config.lock().await;
        get_table_name(&config.active_container)
    };

    let query_vector = {
        let guard = provider_state.lock().await;
        if let Some(err) = &guard.init_error {
            return Err(format!("Embedding provider failed: {}", err));
        }
        let provider = guard.provider.as_ref().ok_or("Embedding provider is loading... Please wait a moment.")?;
        provider.embed_query(&query).await
            .map_err(|e| {
                error!("Query embedding failed: {}", e);
                e.to_string()
            })?
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
    debug!("search: {} results, hybrid={}, reranker={}", scored.len(), used_hybrid, used_reranker);

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
    provider_state: tauri::State<'_, Arc<Mutex<ProviderState>>>,
    config_state: tauri::State<'_, ConfigState>,
    watcher_state: tauri::State<'_, watcher::WatcherState>,
) -> Result<String, String> {
    info!("index_folder: dir=\"{}\"", dir);
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

    let ps = provider_state.inner().clone();
    let app_handle = app.clone();

    let indexing_config = {
        let config = config_state.config.lock().await;
        config.indexing.clone()
    };

    let count = indexer::index_directory(&dir, &table_name, &db, &ps, &indexing_config, move |current, total, path| {
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
        provider_state.inner().clone(),
        app,
    ).await;

    Ok(format!("Indexed {} files", count))
}

#[tauri::command]
pub async fn reset_index(
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
    config_state: tauri::State<'_, ConfigState>,
) -> Result<String, String> {
    info!("reset_index");
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
    provider_state: tauri::State<'_, Arc<Mutex<ProviderState>>>,
    config_state: tauri::State<'_, ConfigState>,
) -> Result<String, String> {
    info!("reindex_all");
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

    let ps = provider_state.inner().clone();

    let indexing_config = {
        let config = config_state.config.lock().await;
        config.indexing.clone()
    };

    let mut total = 0;
    for dir in &paths {
        let app_handle = app.clone();
        let count = indexer::index_directory(dir, &table_name, &db, &ps, &indexing_config, move |current, total, path| {
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
    pub provider_type: String,
    pub remote_endpoint: String,
    pub remote_api_key: String,
    pub remote_model: String,
    pub remote_dimensions: usize,
    pub first_run: bool,
}

#[tauri::command]
pub async fn get_config(
    config_state: tauri::State<'_, ConfigState>,
) -> Result<AppConfig, String> {
    let config = config_state.config.lock().await;
    let (provider_type, remote_endpoint, remote_api_key, remote_model, remote_dimensions) =
        match &config.embedding_provider {
            EmbeddingProviderConfig::Local { .. } => (
                "local".to_string(),
                String::new(),
                String::new(),
                String::new(),
                0,
            ),
            EmbeddingProviderConfig::Remote(rc) => (
                "remote".to_string(),
                rc.endpoint.clone(),
                rc.api_key.clone().unwrap_or_default(),
                rc.model.clone(),
                rc.dimensions,
            ),
        };
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
        provider_type,
        remote_endpoint,
        remote_api_key,
        remote_model,
        remote_dimensions,
        first_run: config.first_run,
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
    pub provider_type: Option<String>,
    pub remote_endpoint: Option<String>,
    pub remote_api_key: Option<String>,
    pub remote_model: Option<String>,
    pub remote_dimensions: Option<usize>,
    pub first_run: Option<bool>,
}

#[tauri::command]
pub async fn update_config(
    app: tauri::AppHandle,
    updates: ConfigUpdate,
    config_state: tauri::State<'_, ConfigState>,
    provider_state: tauri::State<'_, Arc<Mutex<ProviderState>>>,
) -> Result<(), String> {
    info!("update_config");
    let mut provider_changed = false;

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
            if let EmbeddingProviderConfig::Local { ref mut model } = config.embedding_provider {
                *model = v.clone();
                provider_changed = true;
            }
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

        if let Some(ref pt) = updates.provider_type {
            match pt.as_str() {
                "local" => {
                    let model_name = config.embedding_model.clone();
                    config.embedding_provider = EmbeddingProviderConfig::Local { model: model_name };
                    provider_changed = true;
                }
                "remote" => {
                    let endpoint = updates.remote_endpoint.clone().unwrap_or_default();
                    let api_key = updates.remote_api_key.clone().filter(|k| !k.is_empty());
                    let model = updates.remote_model.clone().unwrap_or_default();
                    let dimensions = updates.remote_dimensions.unwrap_or(1024);
                    config.embedding_provider = EmbeddingProviderConfig::Remote(RemoteProviderConfig {
                        endpoint,
                        api_key,
                        model,
                        dimensions,
                    });
                    provider_changed = true;
                }
                _ => {}
            }
        } else if let EmbeddingProviderConfig::Remote(ref mut rc) = config.embedding_provider {
            if let Some(ref v) = updates.remote_endpoint {
                rc.endpoint = v.clone();
                provider_changed = true;
            }
            if let Some(ref v) = updates.remote_api_key {
                rc.api_key = if v.is_empty() { None } else { Some(v.clone()) };
                provider_changed = true;
            }
            if let Some(ref v) = updates.remote_model {
                rc.model = v.clone();
                provider_changed = true;
            }
            if let Some(v) = updates.remote_dimensions {
                rc.dimensions = v;
                provider_changed = true;
            }
        }

        if let Some(v) = updates.first_run {
            config.first_run = v;
        }
    }

    config_state.save().await?;

    if provider_changed {
        let config = config_state.config.lock().await;
        match &config.embedding_provider {
            EmbeddingProviderConfig::Local { model } => {
                let model_enum = crate::config::get_embedding_model(model);
                let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
                let models_path = app_data.join("models");
                drop(config);

                let ps = provider_state.inner().clone();
                tauri::async_runtime::spawn(async move {
                    match indexer::load_model(model_enum, models_path) {
                        Ok(model) => {
                            use crate::indexer::embedding_provider::LocalProvider;
                            use crate::state::ModelState;
                            let model_state = Arc::new(Mutex::new(ModelState {
                                model: Some(model),
                                init_error: None,
                                cached_dim: None,
                            }));
                            let mut guard = ps.lock().await;
                            guard.provider = Some(Box::new(LocalProvider { model_state }));
                            guard.init_error = None;
                            let _ = app.emit("model-loaded", ());
                        }
                        Err(e) => {
                            let mut guard = ps.lock().await;
                            guard.init_error = Some(e.to_string());
                            let _ = app.emit("model-load-error", e.to_string());
                        }
                    }
                });
            }
            EmbeddingProviderConfig::Remote(rc) => {
                use crate::indexer::embedding_provider::RemoteProvider;
                let provider = RemoteProvider::new(rc.clone());
                let mut guard = provider_state.lock().await;
                guard.provider = Some(Box::new(provider));
                guard.init_error = None;
                drop(config);
                let _ = app.emit("model-loaded", ());
            }
        }
    }

    Ok(())
}
