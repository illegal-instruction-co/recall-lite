mod sidebar;
mod search_bar;
mod results_list;
mod status_bar;
mod modal;
mod style;

use std::sync::Arc;
use std::time::Instant;

use eframe::egui;
use global_hotkey::GlobalHotKeyEvent;
use tokio::sync::Mutex;
use tray_icon::TrayIconEvent;
use tray_icon::menu::MenuEvent;

use crate::commands;
use crate::config::ConfigState;
use crate::events::{AppEvent, EventReceiver, EventSender};
use crate::i18n::{self, Language};
use crate::state::{
    ContainerListItem, DbState, IndexingProgress, ModelState, RerankerState, SearchResult,
};
use crate::watcher;

use self::modal::ModalState;

/// Async response types sent back from spawned tasks
enum AsyncResponse {
    SearchResults {
        generation: u64,
        results: Result<Vec<SearchResult>, String>,
    },
    IndexResult(Result<String, String>),
    ClearResult(Result<(), String>),
    ContainerList(Result<(Vec<ContainerListItem>, String), String>),
    ContainerAction(Result<(), String>),
}

pub struct RecallApp {
    // UI state
    query: String,
    results: Vec<SearchResult>,
    selected_index: usize,
    status: String,
    status_clear_at: Option<Instant>,
    is_indexing: bool,
    index_progress: Option<IndexingProgress>,

    // Containers
    containers: Vec<ContainerListItem>,
    active_container: String,
    sidebar_open: bool,

    // Modal
    modal: ModalState,

    // i18n
    locale: Language,

    // Backend state (shared with async tasks)
    db_state: Arc<Mutex<DbState>>,
    model_state: Arc<Mutex<ModelState>>,
    reranker_state: Arc<Mutex<RerankerState>>,
    config_state: ConfigState,
    watcher_state: watcher::WatcherState,

    // Event channels
    event_tx: EventSender,
    event_rx: EventReceiver,

    // Async response channel
    async_tx: std::sync::mpsc::Sender<AsyncResponse>,
    async_rx: std::sync::mpsc::Receiver<AsyncResponse>,

    // Search debounce
    last_query_change: Instant,
    last_searched_query: String,
    search_generation: u64,

    // Tokio runtime
    runtime: tokio::runtime::Handle,

    // Window visibility
    visible: bool,
    /// Instant auquel la fenêtre a été rendue visible (debounce hide-on-unfocus)
    shown_at: Option<Instant>,
    /// Supprime le hide-on-unfocus jusqu'à cet instant (ex : après rfd dialog)
    suppress_hide_until: Option<Instant>,
}

impl RecallApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        db_state: Arc<Mutex<DbState>>,
        model_state: Arc<Mutex<ModelState>>,
        reranker_state: Arc<Mutex<RerankerState>>,
        config_state: ConfigState,
        watcher_state: watcher::WatcherState,
        event_tx: EventSender,
        event_rx: EventReceiver,
        runtime: tokio::runtime::Handle,
        locale: Language,
        initial_containers: Vec<ContainerListItem>,
        initial_active: String,
    ) -> Self {
        let (async_tx, async_rx) = std::sync::mpsc::channel();

        Self {
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
            status: i18n::ts(locale, "status_model_loading"),
            status_clear_at: None,
            is_indexing: false,
            index_progress: None,

            containers: initial_containers,
            active_container: initial_active,
            sidebar_open: true,

            modal: ModalState::None,

            locale,

            db_state,
            model_state,
            reranker_state,
            config_state,
            watcher_state,

            event_tx,
            event_rx,

            async_tx,
            async_rx,

            last_query_change: Instant::now(),
            last_searched_query: String::new(),
            search_generation: 0,

            runtime,

            // La fenêtre démarre cachée ; l'utilisateur l'ouvre via le hotkey
            // ou l'icône de tray.
            visible: false,
            shown_at: None,
            suppress_hide_until: None,
        }
    }

    /// Affiche la fenêtre, la centre (au premier affichage) et lui donne le focus.
    fn show_window(&mut self, ctx: &egui::Context) {
        let first_time = self.shown_at.is_none();
        self.shown_at = Some(Instant::now());
        self.suppress_hide_until = None;

        if first_time {
            // Centrer sur le moniteur courant via egui::ViewportInfo::monitor_size.
            if let Some(monitor) = ctx.input(|i| i.viewport().monitor_size) {
                let x = (monitor.x - 800.0).max(0.0) / 2.0;
                let y = (monitor.y - 600.0).max(0.0) / 2.0;
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                    egui::pos2(x, y),
                ));
            }
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn poll_events(&mut self, ctx: &egui::Context) {
        // Poll backend events (indexing progress, model loaded, etc.)
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::IndexingProgress {
                    current,
                    total,
                    path,
                } => {
                    self.is_indexing = true;
                    self.index_progress = Some(IndexingProgress {
                        current,
                        total,
                        path: path.clone(),
                    });
                    let filename = path.rsplit(['/', '\\']).next().unwrap_or(&path);
                    self.status = i18n::t(self.locale, "status_indexing_file", &[("filename", filename)]);
                }
                AppEvent::IndexingComplete(msg) => {
                    self.status = i18n::t(self.locale, "status_done", &[("message", &msg)]);
                    self.is_indexing = false;
                    self.index_progress = None;
                    self.status_clear_at = Some(Instant::now() + std::time::Duration::from_secs(5));
                    self.refresh_containers(ctx);
                }
                AppEvent::ModelLoaded => {
                    self.status.clear();
                    self.is_indexing = false;
                    self.index_progress = None;
                }
                AppEvent::ModelLoadError(err) => {
                    self.status =
                        i18n::t(self.locale, "status_model_error", &[("error", &err)]);
                    self.is_indexing = false;
                    self.index_progress = None;
                }
                AppEvent::RerankerLoaded | AppEvent::RerankerLoadError(_) => {}
            }
            ctx.request_repaint();
        }

        // Poll async responses
        while let Ok(resp) = self.async_rx.try_recv() {
            match resp {
                AsyncResponse::SearchResults {
                    generation,
                    results,
                } => {
                    if generation == self.search_generation {
                        match results {
                            Ok(res) => {
                                self.results = res;
                                self.selected_index = 0;
                            }
                            Err(msg) => {
                                if msg.contains("rebuild") || msg.contains("Model changed") {
                                    self.status =
                                        i18n::ts(self.locale, "status_rebuild_needed");
                                } else {
                                    self.status = msg;
                                }
                            }
                        }
                    }
                }
                AsyncResponse::IndexResult(result) => {
                    match result {
                        Ok(msg) => {
                            self.status = msg;
                        }
                        Err(msg) => {
                            self.status = msg;
                        }
                    }
                    self.is_indexing = false;
                }
                AsyncResponse::ClearResult(result) => {
                    match result {
                        Ok(()) => {
                            self.status = i18n::ts(self.locale, "status_cleared");
                            self.status_clear_at =
                                Some(Instant::now() + std::time::Duration::from_secs(4));
                        }
                        Err(msg) => {
                            self.status = msg;
                        }
                    }
                    self.is_indexing = false;
                    self.refresh_containers(ctx);
                }
                AsyncResponse::ContainerList(result) => {
                    if let Ok((list, active)) = result {
                        self.containers = list;
                        self.active_container = active;
                    }
                }
                AsyncResponse::ContainerAction(result) => {
                    if let Err(msg) = result {
                        self.status = msg;
                    }
                    self.refresh_containers(ctx);
                }
            }
            ctx.request_repaint();
        }

        // Clear status after timeout
        if let Some(clear_at) = self.status_clear_at {
            if Instant::now() >= clear_at {
                self.status.clear();
                self.status_clear_at = None;
                ctx.request_repaint();
            }
        }

        // Poll global hotkey
        if let Ok(_event) = GlobalHotKeyEvent::receiver().try_recv() {
            self.visible = !self.visible;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(self.visible));
            if self.visible {
                self.show_window(ctx);
            }
        }

        // Poll tray icon click
        if let Ok(TrayIconEvent::Click { .. }) = TrayIconEvent::receiver().try_recv() {
            self.visible = !self.visible;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(self.visible));
            if self.visible {
                self.show_window(ctx);
            }
        }

        // Poll tray menu events
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id().0.as_str() {
                "quit" => std::process::exit(0),
                "show" => {
                    self.visible = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    self.show_window(ctx);
                }
                _ => {}
            }
        }

        // Hide-on-unfocus (comportement Spotlight) : masquer la fenêtre dès
        // que l'OS lui retire le focus, sauf pendant les 300 ms qui suivent
        // un affichage (debounce) ou pendant une suppression explicite.
        if self.visible {
            let has_focus = ctx.input(|i| i.focused);
            let debounced = self
                .shown_at
                .map_or(false, |t| t.elapsed() >= std::time::Duration::from_millis(300));
            let suppressed = self
                .suppress_hide_until
                .map_or(false, |t| t > Instant::now());
            if !has_focus && debounced && !suppressed && matches!(self.modal, ModalState::None) {
                self.visible = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::ArrowDown) {
                if !self.results.is_empty() {
                    self.selected_index =
                        (self.selected_index + 1).min(self.results.len() - 1);
                }
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                self.selected_index = self.selected_index.saturating_sub(1);
            }
            if i.key_pressed(egui::Key::Enter) && !self.results.is_empty() {
                if let Some(result) = self.results.get(self.selected_index) {
                    let _ = open::that(&result.path);
                }
            }
            if i.key_pressed(egui::Key::Escape) {
                if !self.query.is_empty() {
                    self.query.clear();
                    self.results.clear();
                } else if matches!(self.modal, ModalState::None) {
                    self.visible = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
            }
            if i.modifiers.ctrl && i.key_pressed(egui::Key::O) {
                self.pick_folder(ctx);
            }
        });
    }

    fn maybe_search(&mut self, ctx: &egui::Context) {
        let query = self.query.trim().to_string();
        if query.is_empty() {
            if !self.results.is_empty() {
                self.results.clear();
            }
            self.last_searched_query.clear();
            return;
        }

        if query == self.last_searched_query {
            return;
        }

        let elapsed = self.last_query_change.elapsed();
        if elapsed < std::time::Duration::from_millis(300) {
            // Request repaint after debounce period
            ctx.request_repaint_after(std::time::Duration::from_millis(300) - elapsed);
            return;
        }

        // Fire search
        self.search_generation += 1;
        self.last_searched_query = query.clone();
        let gen = self.search_generation;

        let db = self.db_state.clone();
        let model = self.model_state.clone();
        let reranker = self.reranker_state.clone();
        let config = ConfigState {
            config: self.config_state.config.clone(),
            path: self.config_state.path.clone(),
        };
        let tx = self.async_tx.clone();
        let repaint = ctx.clone();

        self.runtime.spawn(async move {
            let result = commands::search(query, &db, &model, &reranker, &config).await;
            let _ = tx.send(AsyncResponse::SearchResults {
                generation: gen,
                results: result,
            });
            repaint.request_repaint();
        });
    }

    fn pick_folder(&mut self, ctx: &egui::Context) {
        let title = i18n::t(
            self.locale,
            "index_folder_title",
            &[("container", &self.active_container)],
        );

        let selected = rfd::FileDialog::new().set_title(&title).pick_folder();
        // Supprimer le hide-on-unfocus 500 ms après la fermeture du dialog
        // natif, le temps que l'OS retourne le focus à notre fenêtre.
        self.suppress_hide_until =
            Some(Instant::now() + std::time::Duration::from_millis(500));

        if let Some(path) = selected {
            let dir = path.to_string_lossy().to_string();
            self.status = i18n::ts(self.locale, "status_starting");
            self.is_indexing = true;

            let db = self.db_state.clone();
            let model = self.model_state.clone();
            let config = ConfigState {
                config: self.config_state.config.clone(),
                path: self.config_state.path.clone(),
            };
            let ws = self.watcher_state.clone();
            let event_tx = self.event_tx.clone();
            let async_tx = self.async_tx.clone();
            let repaint = ctx.clone();

            self.runtime.spawn(async move {
                let result =
                    commands::index_folder(dir, &db, &model, &config, &ws, event_tx).await;
                let _ = async_tx.send(AsyncResponse::IndexResult(result));
                repaint.request_repaint();
            });
        }
    }

    fn refresh_containers(&self, ctx: &egui::Context) {
        let config = ConfigState {
            config: self.config_state.config.clone(),
            path: self.config_state.path.clone(),
        };
        let tx = self.async_tx.clone();
        let repaint = ctx.clone();
        self.runtime.spawn(async move {
            let result = commands::get_containers(&config).await;
            let _ = tx.send(AsyncResponse::ContainerList(result));
            repaint.request_repaint();
        });
    }

    fn switch_container(&mut self, name: String, ctx: &egui::Context) {
        if name == self.active_container {
            return;
        }
        self.active_container = name.clone();
        self.results.clear();
        self.query.clear();
        self.status = i18n::t(self.locale, "status_switched", &[("name", &name)]);
        self.status_clear_at = Some(Instant::now() + std::time::Duration::from_secs(3));

        let config = ConfigState {
            config: self.config_state.config.clone(),
            path: self.config_state.path.clone(),
        };
        let db = self.db_state.clone();
        let model = self.model_state.clone();
        let ws = self.watcher_state.clone();
        let event_tx = self.event_tx.clone();
        let tx = self.async_tx.clone();
        let repaint = ctx.clone();
        self.runtime.spawn(async move {
            let result = commands::set_active_container(
                name, &config, &db, &model, &ws, event_tx,
            )
            .await;
            if result.is_err() {
                // Revert in the response handler isn't easy; just report error
            }
            let _ = tx.send(AsyncResponse::ContainerAction(result));
            repaint.request_repaint();
        });
    }

    fn create_container(&mut self, name: String, description: String, ctx: &egui::Context) {
        let config = ConfigState {
            config: self.config_state.config.clone(),
            path: self.config_state.path.clone(),
        };
        let tx = self.async_tx.clone();
        let repaint = ctx.clone();
        self.runtime.spawn(async move {
            let result = commands::create_container(name, description, &config).await;
            let _ = tx.send(AsyncResponse::ContainerAction(result));
            repaint.request_repaint();
        });
    }

    fn delete_container(&mut self, ctx: &egui::Context) {
        if self.active_container == "Default" {
            return;
        }
        let name = self.active_container.clone();
        let config = ConfigState {
            config: self.config_state.config.clone(),
            path: self.config_state.path.clone(),
        };
        let db = self.db_state.clone();
        let tx = self.async_tx.clone();
        let repaint = ctx.clone();
        self.active_container = "Default".to_string();
        self.results.clear();
        self.runtime.spawn(async move {
            let result = commands::delete_container(name, &config, &db).await;
            let _ = tx.send(AsyncResponse::ContainerAction(result));
            repaint.request_repaint();
        });
    }

    fn reset_index(&mut self, ctx: &egui::Context) {
        self.status = i18n::ts(self.locale, "status_clearing");
        self.is_indexing = true;
        self.results.clear();

        let db = self.db_state.clone();
        let config = ConfigState {
            config: self.config_state.config.clone(),
            path: self.config_state.path.clone(),
        };
        let tx = self.async_tx.clone();
        let repaint = ctx.clone();
        self.runtime.spawn(async move {
            let result = commands::reset_index(&db, &config).await;
            let _ = tx.send(AsyncResponse::ClearResult(result));
            repaint.request_repaint();
        });
    }

    fn reindex_all(&mut self, ctx: &egui::Context) {
        self.status = i18n::ts(self.locale, "status_rebuilding");
        self.is_indexing = true;
        self.results.clear();

        let db = self.db_state.clone();
        let model = self.model_state.clone();
        let config = ConfigState {
            config: self.config_state.config.clone(),
            path: self.config_state.path.clone(),
        };
        let event_tx = self.event_tx.clone();
        let tx = self.async_tx.clone();
        let repaint = ctx.clone();
        self.runtime.spawn(async move {
            let result = commands::reindex_all(&db, &model, &config, event_tx).await;
            let _ = tx.send(AsyncResponse::IndexResult(result));
            repaint.request_repaint();
        });
    }
}

impl eframe::App for RecallApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // Transparent for Mica
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events(ctx);
        self.handle_keyboard(ctx);

        // Process modal actions
        let modal_action = self.modal.take_action();

        // Apply style
        style::apply(ctx);

        // Background fill - semi-transparent dark for when Mica isn't available
        // Fond semi-transparent pour laisser l'effet Mica transpirer.
        let frame = egui::Frame::new()
            .fill(egui::Color32::from_rgba_unmultiplied(18, 18, 18, 200))
            .inner_margin(egui::Margin::ZERO)
            .outer_margin(egui::Margin::ZERO)
            .stroke(egui::Stroke::new(
                1.0,
                egui::Color32::from_white_alpha(20),
            ))
            .corner_radius(8.0);

        egui::CentralPanel::default()
            .frame(frame)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Sidebar
                    let sidebar_action = sidebar::show(
                        ui,
                        &self.containers,
                        &self.active_container,
                        self.sidebar_open,
                        self.is_indexing,
                        self.locale,
                    );
                    match sidebar_action {
                        sidebar::SidebarAction::None => {}
                        sidebar::SidebarAction::ToggleSidebar => {
                            self.sidebar_open = !self.sidebar_open;
                        }
                        sidebar::SidebarAction::SwitchContainer(name) => {
                            self.switch_container(name, ctx);
                        }
                        sidebar::SidebarAction::CreateContainer => {
                            self.modal = ModalState::CreateContainer {
                                name: String::new(),
                                description: String::new(),
                            };
                        }
                        sidebar::SidebarAction::DeleteContainer => {
                            self.modal = ModalState::ConfirmDelete {
                                container_name: self.active_container.clone(),
                            };
                        }
                        sidebar::SidebarAction::ClearIndex => {
                            self.modal = ModalState::ConfirmClear {
                                container_name: self.active_container.clone(),
                            };
                        }
                        sidebar::SidebarAction::ReindexAll => {
                            let folder_count = self
                                .containers
                                .iter()
                                .find(|c| c.name == self.active_container)
                                .map(|c| c.indexed_paths.len())
                                .unwrap_or(0);
                            self.modal = ModalState::ConfirmReindex {
                                container_name: self.active_container.clone(),
                                folder_count,
                            };
                        }
                        sidebar::SidebarAction::CycleLocale => {
                            self.locale = self.locale.cycle();
                            // Save locale preference
                            let config = self.config_state.config.clone();
                            let path = self.config_state.path.clone();
                            let code = self.locale.code().to_string();
                            self.runtime.spawn(async move {
                                let mut c = config.lock().await;
                                c.locale = code;
                                drop(c);
                                let cs = ConfigState { config, path };
                                let _ = cs.save().await;
                            });
                        }
                    }

                    // Separator
                    ui.add(egui::Separator::default().vertical());

                    // Main content
                    ui.vertical(|ui| {
                        // Search bar
                        let old_query = self.query.clone();
                        search_bar::show(
                            ui,
                            &mut self.query,
                            &self.active_container,
                            self.is_indexing,
                            self.locale,
                            // Focus uniquement quand aucune modale n'est ouverte
                            matches!(self.modal, ModalState::None),
                        );
                        if self.query != old_query {
                            self.last_query_change = Instant::now();
                        }

                        // Request folder pick if search bar button clicked
                        // (handled via keyboard Ctrl+O above)

                        ui.add_space(4.0);

                        // Results list
                        let result_action = results_list::show(
                            ui,
                            &self.results,
                            self.selected_index,
                            &self.active_container,
                            &self.query,
                            self.locale,
                        );
                        match result_action {
                            results_list::ResultAction::None => {}
                            results_list::ResultAction::Select(idx) => {
                                self.selected_index = idx;
                            }
                            results_list::ResultAction::Open(idx) => {
                                if let Some(r) = self.results.get(idx) {
                                    let _ = open::that(&r.path);
                                }
                            }
                        }

                        // Status bar
                        let active_info = self
                            .containers
                            .iter()
                            .find(|c| c.name == self.active_container);
                        let folder_count = active_info
                            .map(|i| i.indexed_paths.len())
                            .unwrap_or(0);
                        status_bar::show(
                            ui,
                            &self.status,
                            self.is_indexing,
                            self.index_progress.as_ref(),
                            &self.active_container,
                            folder_count,
                            self.results.len(),
                            self.locale,
                        );
                    });
                });
            });

        // Modals (rendered on top)
        let modal_result = modal::show(ctx, &mut self.modal, self.locale);
        match modal_result {
            modal::ModalResult::None => {}
            modal::ModalResult::CreateContainer { name, description } => {
                self.create_container(name, description, ctx);
            }
            modal::ModalResult::ConfirmDelete => {
                self.delete_container(ctx);
            }
            modal::ModalResult::ConfirmClear => {
                self.reset_index(ctx);
            }
            modal::ModalResult::ConfirmReindex => {
                self.reindex_all(ctx);
            }
        }

        // Handle modal actions from keyboard shortcuts
        if let Some(action) = modal_action {
            match action.as_str() {
                "clear_index" => {
                    self.modal = ModalState::ConfirmClear {
                        container_name: self.active_container.clone(),
                    };
                }
                _ => {}
            }
        }

        // Search debounce
        self.maybe_search(ctx);

        // Continuous repaint for event polling
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}
