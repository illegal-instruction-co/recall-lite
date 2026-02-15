mod indexer;

use std::collections::HashMap;
use std::sync::Arc;
use serde::Serialize;
use tauri::{Emitter, Manager};
use tauri::menu::{Menu, MenuItem, MenuEvent};
use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};
use tokio::sync::Mutex;
use fastembed::EmbeddingModel;
use std::fs;
use std::io::Write;
use serde::Deserialize;

#[derive(Serialize, Deserialize, Clone)]
struct ContainerInfo {
    description: String,
    indexed_paths: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Config {
    embedding_model: String,
    containers: HashMap<String, ContainerInfo>,
    active_container: String,
}

impl Default for Config {
    fn default() -> Self {
        let mut containers = HashMap::new();
        containers.insert("Default".to_string(), ContainerInfo {
            description: String::new(),
            indexed_paths: Vec::new(),
        });
        Self {
            embedding_model: "MultilingualE5Small".to_string(),
            containers,
            active_container: "Default".to_string(),
        }
    }
}

struct ConfigState {
    config: Arc<Mutex<Config>>,
    path: std::path::PathBuf,
}

impl ConfigState {
    async fn save(&self) -> Result<(), String> {
        let config = self.config.lock().await;
        let content = serde_json::to_string_pretty(&*config).map_err(|e| e.to_string())?;
        fs::write(&self.path, content).map_err(|e| e.to_string())
    }
}

fn get_embedding_model(name: &str) -> EmbeddingModel {
    match name {
        "AllMiniLML6V2" => EmbeddingModel::AllMiniLML6V2,
        "MultilingualE5Small" => EmbeddingModel::MultilingualE5Small,
        _ => EmbeddingModel::MultilingualE5Small,
    }
}

fn get_table_name(container: &str) -> String {
    let sanitized: String = container.chars().map(|c| {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
            c.to_string()
        } else {
            format!("{:04x}", c as u32)
        }
    }).collect();
    format!("c_{}", sanitized)
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

#[derive(Serialize, Clone)]
struct ContainerListItem {
    name: String,
    description: String,
    indexed_paths: Vec<String>,
}

#[tauri::command]
async fn get_containers(
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
async fn create_container(
    name: String,
    description: String,
    config_state: tauri::State<'_, ConfigState>,
) -> Result<(), String> {
    let mut config = config_state.config.lock().await;
    if config.containers.contains_key(&name) {
        return Err("Container already exists".to_string());
    }
    config.containers.insert(name, ContainerInfo {
        description,
        indexed_paths: Vec::new(),
    });
    drop(config);
    config_state.save().await?;
    Ok(())
}

#[tauri::command]
async fn delete_container(
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
async fn set_active_container(
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
async fn search(
    query: String,
    db_state: tauri::State<'_, Arc<Mutex<DbState>>>,
    model_state: tauri::State<'_, Arc<Mutex<ModelState>>>,
    config_state: tauri::State<'_, ConfigState>,
) -> Result<Vec<SearchResult>, String> {
    let (table_name, description) = {
        let config = config_state.config.lock().await;
        let desc = config.containers.get(&config.active_container)
            .map(|c| c.description.clone())
            .unwrap_or_default();
        (get_table_name(&config.active_container), desc)
    };

    let contextualized_query = if description.is_empty() {
        query.clone()
    } else {
        format!("{}: {}", description, query)
    };

    let query_vector = {
        let mut guard = model_state.lock().await;
        if let Some(err) = &guard.init_error {
            return Err(format!("Model init failed: {}", err));
        }
        let model = guard.model.as_mut().ok_or("Model is still loading...")?;
        indexer::embed_query(model, &contextualized_query)
            .map_err(|e| e.to_string())?
    };

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };

    let results = indexer::search_files(&db, &table_name, &query_vector, 50)
        .await
        .map_err(|e| e.to_string())?;

    Ok(results
        .into_iter()
        .map(|(path, snippet, cosine_dist)| {
            let similarity = (1.0 - cosine_dist).clamp(0.0, 1.0);
            SearchResult {
                path,
                snippet,
                score: similarity * 100.0,
            }
        })
        .filter(|r| r.score >= 40.0)
        .collect())
}

#[tauri::command]
async fn index_folder(
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

    let count = indexer::index_directory(&dir, &table_name, &db, model, move |path| {
        let _ = app_handle.emit("indexing-progress", path);
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(format!("Indexed {} files", count))
}

#[tauri::command]
async fn reset_index(
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())

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
                    let _ = apply_mica(&window, Some(true));
                }
            }

            let show_i = MenuItem::with_id(app, "show", "Show Recall", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::with_id("tray")
                .menu(&menu)
                .icon(app.default_window_icon().unwrap().clone())
                .show_menu_on_left_click(false)
                .on_menu_event(move |app: &tauri::AppHandle, event: MenuEvent| {
                    match event.id().as_ref() {
                        "quit" => app.exit(0),
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray: &TrayIcon, event: TrayIconEvent| {
                    if let TrayIconEvent::Click { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            let config_path = app_data.join("config.json");
            let config: Config = if config_path.exists() {
                let content = fs::read_to_string(&config_path).unwrap_or_default();
                match serde_json::from_str::<Config>(&content) {
                    Ok(c) => c,
                    Err(_) => {
                        #[derive(Deserialize)]
                        struct OldConfig {
                            embedding_model: Option<String>,
                            containers: Option<Vec<String>>,
                            active_container: Option<String>,
                        }
                        let migrated = if let Ok(old) = serde_json::from_str::<OldConfig>(&content) {
                            let mut containers = HashMap::new();
                            if let Some(names) = old.containers {
                                for name in names {
                                    containers.insert(name, ContainerInfo {
                                        description: String::new(),
                                        indexed_paths: Vec::new(),
                                    });
                                }
                            }
                            if containers.is_empty() {
                                containers.insert("Default".to_string(), ContainerInfo {
                                    description: String::new(),
                                    indexed_paths: Vec::new(),
                                });
                            }
                            Config {
                                embedding_model: old.embedding_model.unwrap_or_else(|| "MultilingualE5Small".to_string()),
                                active_container: old.active_container.unwrap_or_else(|| "Default".to_string()),
                                containers,
                            }
                        } else {
                            Config::default()
                        };
                        if let Ok(json) = serde_json::to_string_pretty(&migrated) {
                            let _ = fs::write(&config_path, json);
                        }
                        migrated
                    }
                }
            } else {
                let config = Config::default();
                let content = serde_json::to_string_pretty(&config).unwrap();
                fs::write(&config_path, content).ok();
                config
            };

            let model_enum = get_embedding_model(&config.embedding_model);

            app.manage(ConfigState {
                config: Arc::new(Mutex::new(config)),
                path: config_path,
            });

            let model_state = Arc::new(Mutex::new(ModelState { model: None, init_error: None }));
            app.manage(model_state.clone());
            app.manage(Arc::new(Mutex::new(DbState { db, path: db_path })));

            let models_path = app_data.join("models");
            std::fs::create_dir_all(&models_path).ok();

            let log_path = app_data.join("recall.log");
            let _ = fs::write(&log_path, "Starting model load...\n");

            let app_handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
                    let _ = writeln!(file, "Loading model to: {:?}", models_path);
                }

                let mut attempts = 0;
                let max_attempts = 3;
                let mut last_error = None;

                while attempts < max_attempts {
                    attempts += 1;
                    match indexer::load_model(model_enum.clone(), models_path.clone()) {
                        Ok(model) => {
                            if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
                                let _ = writeln!(file, "Model loaded successfully");
                            }
                            let mut state = model_state.lock().await;
                            state.model = Some(model);
                            state.init_error = None;
                            let _ = app_handle.emit("model-loaded", ());
                            return;
                        }
                        Err(e) => {
                            if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
                                let _ = writeln!(file, "Attempt {} failed: {}", attempts, e);
                            }
                            last_error = Some(e);
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        }
                    }
                }

                if let Some(e) = last_error {
                    let mut state = model_state.lock().await;
                    state.init_error = Some(e.to_string());
                    let _ = app_handle.emit("model-load-error", e.to_string());
                }
            });

            if let Ok(home_dir) = app.path().home_dir() {
                tauri::async_runtime::spawn(async move {
                    let legacy_cache = home_dir.join(".fastembed_cache");
                    if legacy_cache.exists() {
                        let _ = fs::remove_dir_all(&legacy_cache);
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search,
            index_folder,
            reset_index,
            get_containers,
            create_container,
            delete_container,
            set_active_container
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
