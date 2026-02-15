mod indexer;

use std::sync::Arc;
use serde::Serialize;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};
use tokio::sync::Mutex;
use fastembed::EmbeddingModel;
use std::fs;
use std::io::Write;
use serde::Deserialize;

#[derive(Serialize, Deserialize)]
struct Config {
    embedding_model: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            embedding_model: "MultilingualE5Small".to_string(),
        }
    }
}

fn get_embedding_model(name: &str) -> EmbeddingModel {
    match name {
        "AllMiniLML6V2" => EmbeddingModel::AllMiniLML6V2,
        "MultilingualE5Small" => EmbeddingModel::MultilingualE5Small,
         _ => EmbeddingModel::MultilingualE5Small,
    }
}

struct DbState {
    db: lancedb::Connection,
    path: std::path::PathBuf,
}

struct ModelState {
    model: Option<fastembed::TextEmbedding>,
    init_error: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct SearchResult {
    path: String,
    snippet: String,
    score: f32,
}

#[tauri::command]
async fn search(
    query: String,
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
    model_state: tauri::State<'_, Arc<Mutex<ModelState>>>,
) -> Result<Vec<SearchResult>, String> {
    let db = db_state.lock().await;
    let mut model_guard = model_state.lock().await;

    if let Some(err) = &model_guard.init_error {
        return Err(format!("Model init failed: {}", err));
    }

    let model = model_guard.model.as_mut().ok_or("Model is still loading...")?;
    
    let results = indexer::search_files(&db.db, model, &query, 5)
        .await
        .map_err(|e| e.to_string())?;

    Ok(results
        .into_iter()
        .map(|(path, snippet, dist)| SearchResult { 
            path, 
            snippet, 
            score: (1.0 - dist).max(0.0) * 100.0 
        })
        .filter(|r| r.score >= 55.0)
        .collect())
}

#[tauri::command]
async fn index_folder(
    app: tauri::AppHandle,
    dir: String,
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
    model_state: tauri::State<'_, Arc<Mutex<ModelState>>>,
) -> Result<String, String> {
    let db = db_state.lock().await;
    let mut model_guard = model_state.lock().await;

    if let Some(err) = &model_guard.init_error {
        return Err(format!("Model init failed: {}", err));
    }

    let model = model_guard.model.as_mut().ok_or("Model is still loading...")?;
    
    let app_handle = app.clone();

    let count = indexer::index_directory(&dir, &db.db, model, move |path| {
        let _ = app_handle.emit("indexing-progress", path);
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(format!("Indexed {} files", count))
}

#[tauri::command]
async fn reset_index(
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
) -> Result<String, String> {
    let db = db_state.lock().await;
    indexer::reset_index(&db.path)
        .await
        .map_err(|e| e.to_string())?;
    Ok("Index cleared successfully".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcut(Shortcut::new(Some(Modifiers::ALT), Code::Space))
                .unwrap()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if shortcut.matches(Modifiers::ALT, Code::Space) {
                            if let Some(window) = app.get_webview_window("main") {
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            let app_data = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");

            std::fs::create_dir_all(&app_data).ok();

            let db_path = app_data.join("lancedb");
            let db_path_str = db_path.to_string_lossy().to_string();

            let db = tauri::async_runtime::block_on(async {
                lancedb::connect(&db_path_str)
                    .execute()
                    .await
                    .expect("Failed to connect to LanceDB")
            });

            #[cfg(target_os = "windows")]
            {
                use window_vibrancy::apply_mica;
                if let Some(window) = app.get_webview_window("main") {
                    // Apply Mica effect (Windows 11 default)
                    // The second argument is for Dark Mode (Some(true) = Dark, Some(false) = Light, None = System)
                    let _ = apply_mica(&window, Some(true));
                }
            }

            let config_path = app_data.join("config.json");
            let config = if config_path.exists() {
                let content = fs::read_to_string(&config_path).unwrap_or_default();
                serde_json::from_str(&content).unwrap_or_default()
            } else {
                let config = Config::default();
                let content = serde_json::to_string_pretty(&config).unwrap();
                fs::write(&config_path, content).ok();
                config
            };

            let model_enum = get_embedding_model(&config.embedding_model);
            
            // Initialize with None
            let model_state = Arc::new(Mutex::new(ModelState { model: None, init_error: None }));
            app.manage(model_state.clone());
            app.manage(Arc::new(Mutex::new(DbState { db, path: db_path })));

            let models_path = app_data.join("models");
            std::fs::create_dir_all(&models_path).ok();

            let log_path = app_data.join("recall.log");
            let _ = fs::write(&log_path, "Starting model load...\n");

            let app_handle = app.handle().clone();

            // Spawn background task to load model
            tauri::async_runtime::spawn(async move {
                if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
                    let _ = writeln!(file, "Loading model to: {:?}", models_path);
                }
                
                match indexer::load_model(model_enum, models_path) {
                    Ok(model) => {
                        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
                            let _ = writeln!(file, "Model loaded successfully");
                        }
                        let mut state = model_state.lock().await;
                        state.model = Some(model);
                        state.init_error = None;
                        let _ = app_handle.emit("model-loaded", ());
                    }
                    Err(e) => {
                         if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
                            let _ = writeln!(file, "Model load failed: {}", e);
                         }
                         let mut state = model_state.lock().await;
                         state.init_error = Some(e.to_string());
                         let _ = app_handle.emit("model-load-error", e.to_string());
                    }
                }
            });

            // Cleanup legacy cache
            if let Ok(home_dir) = app.path().home_dir() {
                 let log_path_cleanup = app_data.join("recall.log");
                 tauri::async_runtime::spawn(async move {
                     let legacy_cache = home_dir.join(".fastembed_cache");
                     if legacy_cache.exists() {
                         if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&log_path_cleanup) {
                             let _ = writeln!(file, "Removing legacy cache: {:?}", legacy_cache);
                         }
                         let _ = fs::remove_dir_all(&legacy_cache);
                     }
                 });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![search, index_folder, reset_index])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
