#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use mimalloc::MiMalloc;
use tokio::sync::Mutex;

use recall_lite_lib::config::{self, get_embedding_model, ConfigState};
use recall_lite_lib::events::{self, AppEvent};
use recall_lite_lib::i18n::{self, Language};
use recall_lite_lib::indexer;
use recall_lite_lib::state::{DbState, ModelState, RerankerState};
use recall_lite_lib::ui::RecallApp;
use recall_lite_lib::watcher;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn get_app_data_dir() -> std::path::PathBuf {
    let base = std::env::var("APPDATA")
        .or_else(|_| std::env::var("XDG_DATA_HOME"))
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{}/.local/share", home)
        });
    std::path::PathBuf::from(base).join("com.recall-lite.app")
}

fn main() {
    // ── Config ──
    let config_dir = get_app_data_dir();
    std::fs::create_dir_all(&config_dir).ok();
    let config_path = config_dir.join("config.json");
    let config = config::load_config(&config_path);

    let _always_on_top = config.always_on_top;
    let launch_at_startup = config.launch_at_startup;
    let hotkey_str = config.hotkey.clone();
    let model_enum = get_embedding_model(&config.embedding_model);

    // Determine locale
    let locale = if config.locale == "auto" {
        i18n::detect_system_language()
    } else {
        Language::from_code(&config.locale)
    };

    // ── Auto-start ──
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(auto) = auto_launch::AutoLaunchBuilder::new()
            .set_app_name("Recall-Lite")
            .set_app_path(&exe.to_string_lossy())
            .build()
        {
            if launch_at_startup {
                let _ = auto.enable();
            } else {
                let _ = auto.disable();
            }
        }
    }

    // ── Tokio runtime ──
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    // ── LanceDB ──
    let app_data = config_dir.clone();
    let db_path = app_data.join("lancedb");
    let db = runtime.block_on(async {
        lancedb::connect(&db_path.to_string_lossy())
            .execute()
            .await
            .expect("Failed to connect to LanceDB")
    });

    // ── State ──
    let config_state = ConfigState {
        config: Arc::new(Mutex::new(config)),
        path: config_path,
    };

    let model_state = Arc::new(Mutex::new(ModelState {
        model: None,
        init_error: None,
    }));
    let reranker_state = Arc::new(Mutex::new(RerankerState {
        reranker: None,
        init_error: None,
    }));
    let db_state = Arc::new(Mutex::new(DbState {
        db: db.clone(),
        path: db_path,
    }));
    let watcher_state = watcher::new_state();

    // ── Event channel ──
    let (event_tx, event_rx) = events::channel();

    // ── Initial container list ──
    let (initial_containers, initial_active) = runtime.block_on(async {
        match recall_lite_lib::commands::get_containers(&config_state).await {
            Ok((list, active)) => (list, active),
            Err(_) => (vec![], "Default".to_string()),
        }
    });

    // ── Global Hotkey ──
    let hotkey_manager = global_hotkey::GlobalHotKeyManager::new()
        .expect("Failed to create hotkey manager");
    let hotkey = config::parse_hotkey(&hotkey_str);
    hotkey_manager
        .register(hotkey)
        .expect("Failed to register global hotkey");

    // ── Tray Icon ──
    let show_item = tray_icon::menu::MenuItem::with_id("show", "Show Recall", true, None);
    let quit_item = tray_icon::menu::MenuItem::with_id("quit", "Quit", true, None);
    let menu = tray_icon::menu::Menu::new();
    let _ = menu.append(&show_item);
    let _ = menu.append(&quit_item);

    // Load icon from embedded bytes
    let icon = load_tray_icon();

    let _tray = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Recall-Lite")
        .with_icon(icon)
        .with_menu_on_left_click(false)
        .build()
        .expect("Failed to create tray icon");

    // ── Spawn model loading ──
    let model_state_clone = model_state.clone();
    let event_tx_clone = event_tx.clone();
    let watcher_state_clone = watcher_state.clone();
    let config_state_for_watcher = ConfigState {
        config: config_state.config.clone(),
        path: config_state.path.clone(),
    };
    let watcher_db = db.clone();
    let watcher_model = model_state.clone();
    let models_path = app_data.join("models");
    std::fs::create_dir_all(&models_path).ok();

    let reranker_state_clone = reranker_state.clone();
    let reranker_models = models_path.clone();
    let reranker_tx = event_tx.clone();

    runtime.spawn(async move {
        let mut attempts = 0;
        let max_attempts = 3;
        let mut last_error = None;
        let mut loaded = false;

        while attempts < max_attempts {
            attempts += 1;
            match indexer::load_model(model_enum.clone(), models_path.clone()) {
                Ok(model) => {
                    let mut state = model_state_clone.lock().await;
                    state.model = Some(model);
                    state.init_error = None;
                    let _ = event_tx_clone.send(AppEvent::ModelLoaded);
                    loaded = true;

                    // Start watcher after model is loaded
                    watcher::restart(
                        &watcher_state_clone,
                        &config_state_for_watcher,
                        watcher_db.clone(),
                        watcher_model.clone(),
                        event_tx_clone.clone(),
                    )
                    .await;

                    break;
                }
                Err(e) => {
                    last_error = Some(e);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }

        if !loaded {
            if let Some(e) = last_error {
                let mut state = model_state_clone.lock().await;
                state.init_error = Some(e.to_string());
                let _ = event_tx_clone.send(AppEvent::ModelLoadError(e.to_string()));
            }
        }
    });

    // Spawn reranker loading
    runtime.spawn(async move {
        match indexer::load_reranker(reranker_models) {
            Ok(reranker) => {
                let mut state = reranker_state_clone.lock().await;
                state.reranker = Some(reranker);
                let _ = reranker_tx.send(AppEvent::RerankerLoaded);
            }
            Err(e) => {
                let mut state = reranker_state_clone.lock().await;
                state.init_error = Some(e.to_string());
                let _ = reranker_tx.send(AppEvent::RerankerLoadError(e.to_string()));
            }
        }
    });

    // Clean up legacy cache
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let legacy_cache = std::path::PathBuf::from(home).join(".fastembed_cache");
        if legacy_cache.exists() {
            let _ = std::fs::remove_dir_all(&legacy_cache);
        }
    }

    // ── eframe window ──
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_transparent(true)
            .with_decorations(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_inner_size([800.0, 600.0])
            .with_resizable(false)
            .with_visible(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    let rt_handle = runtime.handle().clone();

    eframe::run_native(
        "Recall-Lite",
        options,
        Box::new(move |cc| {
            // Apply Mica on Windows
            #[cfg(target_os = "windows")]
            {
                use window_vibrancy::apply_mica;
                let _ = apply_mica(&cc, Some(true));
            }

            Ok(Box::new(RecallApp::new(
                cc,
                db_state,
                model_state,
                reranker_state,
                config_state,
                watcher_state,
                event_tx,
                event_rx,
                rt_handle,
                locale,
                initial_containers,
                initial_active,
            )))
        }),
    )
    .expect("Failed to run eframe application");
}

fn load_tray_icon() -> tray_icon::Icon {
    // Try to load from icons directory, fallback to a simple generated icon
    let icon_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("icons").join("32x32.png")));

    if let Some(path) = icon_path {
        if path.exists() {
            if let Ok(img) = image::open(&path) {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                if let Ok(icon) = tray_icon::Icon::from_rgba(rgba.into_raw(), w, h) {
                    return icon;
                }
            }
        }
    }

    // Fallback: generate a simple 16x16 blue icon
    let size = 16u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            rgba[idx] = 96;      // R
            rgba[idx + 1] = 205; // G
            rgba[idx + 2] = 255; // B
            rgba[idx + 3] = 255; // A
        }
    }
    tray_icon::Icon::from_rgba(rgba, size, size).expect("Failed to create fallback icon")
}

use eframe::egui;
