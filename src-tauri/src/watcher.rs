use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use notify_debouncer_full::notify::{self, RecursiveMode};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use crate::config::{get_table_name, ConfigState};
use crate::indexer;
use crate::state::{IndexingProgress, ModelState};

fn build_gitignore(roots: &[String]) -> Option<ignore::gitignore::Gitignore> {
    if roots.is_empty() { return None; }
    let mut builder = ignore::gitignore::GitignoreBuilder::new(&roots[0]);
    for root in roots {
        let gi = std::path::Path::new(root).join(".gitignore");
        if gi.exists() {
            let _ = builder.add(gi);
        }
        let rc = std::path::Path::new(root).join(".rcignore");
        if rc.exists() {
            let _ = builder.add(rc);
        }
    }
    builder.build().ok()
}

pub struct WatcherHandle {
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
}

pub type WatcherState = Arc<Mutex<Option<WatcherHandle>>>;

pub fn new_state() -> WatcherState {
    Arc::new(Mutex::new(None))
}

pub async fn restart(
    watcher_state: &WatcherState,
    config_state: &ConfigState,
    db: lancedb::Connection,
    model_state: Arc<Mutex<ModelState>>,
    app: AppHandle,
) {
    let handle = {
        let config = config_state.config.lock().await;
        let table_name = get_table_name(&config.active_container);
        let paths = config
            .containers
            .get(&config.active_container)
            .map(|info| info.indexed_paths.clone())
            .unwrap_or_default();
        let use_git_history = config.indexing.use_git_history;
        let chunk_size = config.indexing.chunk_size;
        let chunk_overlap = config.indexing.chunk_overlap;
        drop(config);
        start_watcher(paths, db, model_state, table_name, app, use_git_history, chunk_size, chunk_overlap)
    };

    let mut guard = watcher_state.lock().await;
    *guard = handle;
}

fn start_watcher(
    paths: Vec<String>,
    db: lancedb::Connection,
    model_state: Arc<Mutex<ModelState>>,
    table_name: String,
    app: AppHandle,
    use_git_history: bool,
    chunk_size: Option<usize>,
    chunk_overlap: Option<usize>,
) -> Option<WatcherHandle> {
    if paths.is_empty() {
        return None;
    }

    let (tx, rx) = std::sync::mpsc::channel();

    let mut debouncer = match new_debouncer(Duration::from_millis(500), None, move |result: DebounceEventResult| {
        if let Ok(events) = result {
            let _ = tx.send(events);
        }
    }) {
        Ok(d) => d,
        Err(_) => return None,
    };

    for path in &paths {
        let p = std::path::Path::new(path);
        let _ = debouncer.watch(p, RecursiveMode::Recursive);
    }

    let gitignore = build_gitignore(&paths);

    let rt = tokio::runtime::Handle::current();
    let indexing_lock = Arc::new(Mutex::new(()));
    std::thread::spawn(move || {
        while let Ok(events) = rx.recv() {
            let mut changed: HashSet<PathBuf> = HashSet::new();
            let mut deleted: HashSet<PathBuf> = HashSet::new();

            for event in &events {
                use notify::EventKind;
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        for p in &event.paths {
                            let dominated = gitignore.as_ref().map_or(false, |gi| {
                                gi.matched_path_or_any_parents(p, false).is_ignore()
                            });
                            if p.is_file() && !dominated {
                                changed.insert(p.clone());
                            }
                        }
                    }
                    EventKind::Remove(_) => {
                        for p in &event.paths {
                            let dominated = gitignore.as_ref().map_or(false, |gi| {
                                gi.matched_path_or_any_parents(p, false).is_ignore()
                            });
                            if !dominated {
                                deleted.insert(p.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }

            if changed.is_empty() && deleted.is_empty() {
                continue;
            }

            let db = db.clone();
            let ms = model_state.clone();
            let tn = table_name.clone();
            let app = app.clone();
            let lock = indexing_lock.clone();
            let changed: Vec<PathBuf> = changed.into_iter().collect();
            let deleted: Vec<PathBuf> = deleted.into_iter().collect();
            let total = changed.len() + deleted.len();

            rt.spawn(async move {
                let _guard = lock.lock().await;

                let _ = app.emit("indexing-progress", IndexingProgress {
                    current: 0,
                    total,
                    path: format!("Auto-reindexing {} files...", total),
                });

                let mut count = 0usize;

                for path in &deleted {
                    let path_str = path.to_string_lossy().to_string();
                    if let Err(e) = indexer::delete_file_from_index(&path_str, &tn, &db).await {
                        let _ = app.emit("watcher-error", format!("Failed to remove {}: {}", path_str, e));
                    }
                    count += 1;
                }

                for path in &changed {
                    if let Err(e) = indexer::index_single_file(path, &tn, &db, &ms, use_git_history, chunk_size, chunk_overlap).await {
                        let _ = app.emit("watcher-error", format!("Failed to index {}: {}", path.display(), e));
                    }
                    count += 1;
                    let _ = app.emit("indexing-progress", IndexingProgress {
                        current: count,
                        total,
                        path: path.to_string_lossy().to_string(),
                    });
                }

                let _ = app.emit("indexing-complete", format!("{} files auto-reindexed", count));
            });
        }
    });

    Some(WatcherHandle { _debouncer: debouncer })
}
