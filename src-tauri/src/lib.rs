mod commands;
pub mod config;
pub mod indexer;
pub mod state;


use std::sync::Arc;
use std::fs;
use std::io::Write;


use tauri::{Emitter, Manager};
use tauri::menu::{Menu, MenuItem, MenuEvent};
use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};
use tokio::sync::Mutex;

use config::{ConfigState, get_embedding_model};
use state::{DbState, ModelState, RerankerState};

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
            let config = config::load_config(&config_path);

            let model_enum = get_embedding_model(&config.embedding_model);

            app.manage(ConfigState {
                config: Arc::new(Mutex::new(config)),
                path: config_path,
            });

            let model_state = Arc::new(Mutex::new(ModelState { model: None, init_error: None }));
            app.manage(model_state.clone());

            let reranker_state = Arc::new(Mutex::new(RerankerState { reranker: None, init_error: None }));
            app.manage(reranker_state.clone());
            app.manage(Arc::new(Mutex::new(DbState { db, path: db_path })));

            let models_path = app_data.join("models");
            std::fs::create_dir_all(&models_path).ok();

            let log_path = app_data.join("recall.log");
            let _ = fs::write(&log_path, "Starting model load...\n");

            let app_handle = app.handle().clone();

            let reranker_models_path = models_path.clone();
            let reranker_log = log_path.clone();

            tauri::async_runtime::spawn(async move {
                if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
                    let _ = writeln!(file, "Loading model to: {:?}", models_path);
                }

                let mut attempts = 0;
                let max_attempts = 3;
                let mut last_error = None;
                let mut loaded = false;

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
                            loaded = true;
                            break;
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

                if !loaded {
                    if let Some(e) = last_error {
                        let mut state = model_state.lock().await;
                        state.init_error = Some(e.to_string());
                        let _ = app_handle.emit("model-load-error", e.to_string());
                    }
                }
            });

            tauri::async_runtime::spawn(async move {
                if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&reranker_log) {
                    let _ = writeln!(file, "Loading reranker model...");
                }
                match indexer::load_reranker(reranker_models_path) {
                    Ok(reranker) => {
                        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&reranker_log) {
                            let _ = writeln!(file, "Reranker loaded successfully");
                        }
                        let mut state = reranker_state.lock().await;
                        state.reranker = Some(reranker);
                    }
                    Err(e) => {
                        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&reranker_log) {
                            let _ = writeln!(file, "Reranker load failed (non-fatal): {}", e);
                        }
                        let mut state = reranker_state.lock().await;
                        state.init_error = Some(e.to_string());
                    }
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
            commands::search,
            commands::index_folder,
            commands::reset_index,
            commands::reindex_all,
            commands::get_containers,
            commands::create_container,
            commands::delete_container,
            commands::set_active_container
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
