use serde::Serialize;

pub struct DbState {
    pub db: lancedb::Connection,
    pub path: std::path::PathBuf,
}

pub struct ModelState {
    pub model: Option<fastembed::TextEmbedding>,
    pub init_error: Option<String>,
}

pub struct RerankerState {
    pub reranker: Option<fastembed::TextRerank>,
    pub init_error: Option<String>,
}

#[derive(Clone)]
pub struct SearchResult {
    pub path: String,
    pub snippet: String,
    pub score: f32,
}

#[derive(Clone)]
pub struct IndexingProgress {
    pub current: usize,
    pub total: usize,
    pub path: String,
}

#[derive(Serialize, Clone)]
pub struct ContainerListItem {
    pub name: String,
    pub description: String,
    pub indexed_paths: Vec<String>,
}
