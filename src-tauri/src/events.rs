use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub enum AppEvent {
    IndexingProgress {
        current: usize,
        total: usize,
        path: String,
    },
    IndexingComplete(String),
    ModelLoaded,
    ModelLoadError(String),
    RerankerLoaded,
    RerankerLoadError(String),
}

pub type EventSender = std::sync::mpsc::Sender<AppEvent>;
pub type EventReceiver = std::sync::mpsc::Receiver<AppEvent>;

pub fn channel() -> (EventSender, EventReceiver) {
    std::sync::mpsc::channel()
}
