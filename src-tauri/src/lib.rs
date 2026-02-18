mod commands;
pub mod config;
pub mod indexer;
pub mod state;
mod watcher;

use std::sync::Arc;

use log::{info, error, debug, warn};


use tauri::{Emitter, Manager};
use tauri::menu::{Menu, MenuItem, MenuEvent};
use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri_plugin_global_shortcut::ShortcutState;
use tokio::sync::Mutex;

use config::{ConfigState, EmbeddingProviderConfig, get_embedding_model, parse_hotkey};
use state::{DbState, ModelState, ProviderState, RerankerState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config_dir = std::path::PathBuf::from(
        std::env::var("APPDATA").expect("APPDATA not set")
    ).join("com.rememex.app");
    std::fs::create_dir_all(&config_dir).ok();
    let config_path = config_dir.join("config.json");
    let config = config::load_config(&config_path);

    let shortcut = parse_hotkey(&config.hotkey);
    let always_on_top = config.always_on_top;
    let launch_at_startup = config.launch_at_startup;

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name: Some("rememex".into()) }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                ])
                .max_file_size(5_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
                .level(log::LevelFilter::Debug)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())

        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcut(shortcut)
                .unwrap()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
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
                .build(),
        )
        .setup(move |app| {
            let app_data = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");

            std::fs::create_dir_all(&app_data).ok();

            let db_path = app_data.join("lancedb");
            let db_path_str = db_path.to_string_lossy().to_string();

            debug!("Connecting to LanceDB at {:?}", db_path);
            let db = tauri::async_runtime::block_on(async {
                lancedb::connect(&db_path_str)
                    .execute()
                    .await
                    .expect("Failed to connect to LanceDB")
            });
            info!("LanceDB connected");

            #[cfg(target_os = "windows")]
            {
                use window_vibrancy::apply_mica;
                if let Some(window) = app.get_webview_window("main") {
                    let _ = apply_mica(&window, Some(true));
                    let _ = window.set_always_on_top(always_on_top);
                }
            }

            {
                use tauri_plugin_autostart::ManagerExt;
                let autostart = app.autolaunch();
                if launch_at_startup {
                    let _ = autostart.enable();
                } else {
                    let _ = autostart.disable();
                }
            }

            let show_i = MenuItem::with_id(app, "show", "Show Rememex", true, None::<&str>)?;
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

            let provider_state = Arc::new(Mutex::new(ProviderState { provider: None, init_error: None }));
            app.manage(provider_state.clone());

            let reranker_state = Arc::new(Mutex::new(RerankerState { reranker: None, init_error: None }));
            app.manage(reranker_state.clone());
            app.manage(Arc::new(Mutex::new(DbState { db, path: db_path })));

            let watcher_state = watcher::new_state();
            app.manage(watcher_state.clone());

            let models_path = app_data.join("models");
            std::fs::create_dir_all(&models_path).ok();

            info!("Starting provider init...");

            let app_handle = app.handle().clone();

            let reranker_models_path = models_path.clone();
            let watcher_provider_state = provider_state.clone();
            let watcher_state_for_model = watcher_state.clone();
            let watcher_app = app.handle().clone();

            let embedding_provider_config = config.embedding_provider.clone();
            let is_first_run = config.first_run;

            app.manage(ConfigState {
                config: Arc::new(Mutex::new(config)),
                path: config_path,
            });

            let watcher_config: ConfigState = {
                let cs: tauri::State<ConfigState> = app.state();
                ConfigState { config: cs.config.clone(), path: cs.path.clone() }
            };
            let watcher_db = {
                let guard: tauri::State<Arc<Mutex<state::DbState>>> = app.state();
                let g = guard.blocking_lock();
                g.db.clone()
            };

            if is_first_run {
                info!("First run detected — deferring provider init until user configures settings");
            } else {
                match embedding_provider_config {
                    EmbeddingProviderConfig::Local { ref model } => {
                        let model_enum = get_embedding_model(model);

                        tauri::async_runtime::spawn(async move {
                            info!("Loading local model to: {:?}", models_path);

                            let mut attempts = 0;
                            let max_attempts = 3;
                            let mut last_error = None;
                            let mut loaded = false;

                            while attempts < max_attempts {
                                attempts += 1;
                                match indexer::load_model(model_enum.clone(), models_path.clone()) {
                                    Ok(model) => {
                                        info!("Local embedding model loaded successfully");
                                        let model_state = Arc::new(Mutex::new(ModelState {
                                            model: Some(model),
                                            init_error: None,
                                            cached_dim: None,
                                        }));
                                        let local_provider = indexer::embedding_provider::LocalProvider { model_state };
                                        let mut guard = provider_state.lock().await;
                                        guard.provider = Some(Box::new(local_provider));
                                        guard.init_error = None;
                                        drop(guard);
                                        let _ = app_handle.emit("model-loaded", ());
                                        loaded = true;

                                        watcher::restart(
                                            &watcher_state_for_model,
                                            &watcher_config,
                                            watcher_db.clone(),
                                            watcher_provider_state.clone(),
                                            watcher_app.clone(),
                                        ).await;

                                        break;
                                    }
                                    Err(e) => {
                                        error!("Model load attempt {} failed: {}", attempts, e);
                                        last_error = Some(e);
                                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                    }
                                }
                            }

                            if !loaded {
                                if let Some(e) = last_error {
                                    let mut guard = provider_state.lock().await;
                                    guard.init_error = Some(e.to_string());
                                    let _ = app_handle.emit("model-load-error", e.to_string());
                                }
                            }
                        });
                    }
                    EmbeddingProviderConfig::Remote(ref rc) => {
                        info!("Initializing remote embedding provider: {}", rc.endpoint);
                        let remote_provider = indexer::embedding_provider::RemoteProvider::new(rc.clone());
                        let mut guard = provider_state.blocking_lock();
                        guard.provider = Some(Box::new(remote_provider));
                        guard.init_error = None;
                        drop(guard);
                        let _ = app_handle.emit("model-loaded", ());

                        tauri::async_runtime::spawn(async move {
                            watcher::restart(
                                &watcher_state_for_model,
                                &watcher_config,
                                watcher_db.clone(),
                                watcher_provider_state.clone(),
                                watcher_app.clone(),
                            ).await;
                        });
                    }
                }
            }

            tauri::async_runtime::spawn(async move {
                info!("Loading reranker model...");
                match indexer::load_reranker(reranker_models_path) {
                    Ok(reranker) => {
                        info!("Reranker loaded successfully");
                        let mut state = reranker_state.lock().await;
                        state.reranker = Some(reranker);
                    }
                    Err(e) => {
                        warn!("Reranker load failed (non-fatal): {}", e);
                        let mut state = reranker_state.lock().await;
                        state.init_error = Some(e.to_string());
                    }
                }
            });

            if let Ok(home_dir) = app.path().home_dir() {
                tauri::async_runtime::spawn(async move {
                    let legacy_cache = home_dir.join(".fastembed_cache");
                    if legacy_cache.exists() {
                        let _ = std::fs::remove_dir_all(&legacy_cache);
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
            commands::set_active_container,
            commands::get_config,
            commands::update_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
