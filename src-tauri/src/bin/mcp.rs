use std::sync::Arc;

use mimalloc::MiMalloc;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::model::*;
use rmcp::tool;
use rmcp::transport::stdio;
use rmcp::{tool_handler, tool_router, schemars, ErrorData as McpError, ServerHandler, ServiceExt};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use recall_lite_lib::config::{get_embedding_model, get_table_name, load_config, Config};
use recall_lite_lib::indexer;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

struct Models {
    model: fastembed::TextEmbedding,
    reranker: Option<fastembed::TextRerank>,
}

struct AppState {
    db: lancedb::Connection,
    models: Arc<Mutex<Models>>,
    config: Config,
}

#[derive(Clone)]
pub struct RecallServer {
    state: Arc<AppState>,
    tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SearchParams {
    query: String,
    container: Option<String>,
}

#[derive(Serialize)]
struct SearchResultItem {
    path: String,
    snippet: String,
    score: f32,
}

#[tool_router]
impl RecallServer {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Search indexed files using semantic + keyword hybrid search. Returns ranked results with file paths, relevant snippets, and relevance scores."
    )]
    async fn recall_search(
        &self,
        Parameters(SearchParams { query, container }): Parameters<SearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let container =
            container.unwrap_or_else(|| self.state.config.active_container.clone());
        let table_name = get_table_name(&container);

        let table_check = self.state.db.table_names().execute().await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        if !table_check.iter().any(|t| t == &table_name) {
            return Ok(CallToolResult::success(vec![Content::text(
                format!("no index found for container '{}'. open Recall Lite and index some folders first.", container),
            )]));
        }

        let query_vector = {
            let mut guard = self.state.models.lock().await;
            indexer::embed_query(&mut guard.model, &query)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        };

        let query_variants = indexer::expand_query(&query);
        let vector_fut =
            indexer::search_files(&self.state.db, &table_name, &query_vector, 50);

        let fts_db = self.state.db.clone();
        let fts_table = table_name.clone();
        let fts_fut = async move {
            let futs: Vec<_> = query_variants
                .iter()
                .map(|v| indexer::search_fts(&fts_db, &fts_table, v, 30))
                .collect();
            let results = futures::future::join_all(futs).await;
            let mut all: Vec<(String, String)> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for result in results.into_iter().flatten() {
                for item in result {
                    if seen.insert(item.0.clone()) {
                        all.push(item);
                    }
                }
            }
            all
        };

        let (vector_result, fts_results) = tokio::join!(vector_fut, fts_fut);
        let vector_results = vector_result
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let merged = if fts_results.is_empty() {
            vector_results
        } else {
            indexer::hybrid_merge(&vector_results, &fts_results, 50)
        };

        let rerank_input: Vec<(String, String, f32)> =
            merged.into_iter().take(15).collect();

        let (final_results, used_reranker) = {
            let mut guard = self.state.models.lock().await;
            if let Some(reranker) = guard.reranker.take() {
                let query_clone = query.clone();
                let input_clone = rerank_input.clone();
                match tokio::task::spawn_blocking(move || {
                    let mut r = reranker;
                    let res =
                        indexer::rerank_results(&mut r, &query_clone, &input_clone);
                    (r, res)
                })
                .await
                {
                    Ok((reranker_back, rerank_res)) => {
                        guard.reranker = Some(reranker_back);
                        match rerank_res {
                            Ok(reranked) => (reranked, true),
                            Err(_) => (rerank_input, false),
                        }
                    }
                    Err(_) => (rerank_input, false),
                }
            } else {
                (rerank_input, false)
            }
        };

        let used_hybrid = !fts_results.is_empty();

        let mut scored: Vec<SearchResultItem> = if used_reranker {
            final_results
                .into_iter()
                .map(|(path, snippet, raw_score)| {
                    let sigmoid = 1.0 / (1.0 + (-raw_score).exp());
                    SearchResultItem {
                        path,
                        snippet,
                        score: sigmoid * 100.0,
                    }
                })
                .collect()
        } else if used_hybrid {
            let max_rrf = final_results.first().map(|(_, _, s)| *s).unwrap_or(1.0);
            final_results
                .into_iter()
                .map(|(path, snippet, rrf_score)| {
                    let pct = if max_rrf > 0.0 {
                        (rrf_score / max_rrf) * 100.0
                    } else {
                        0.0
                    };
                    SearchResultItem {
                        path,
                        snippet,
                        score: pct,
                    }
                })
                .collect()
        } else {
            final_results
                .into_iter()
                .map(|(path, snippet, cosine_dist)| {
                    let similarity = (1.0 - cosine_dist).clamp(0.0, 1.0);
                    SearchResultItem {
                        path,
                        snippet,
                        score: similarity * 100.0,
                    }
                })
                .collect()
        };

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if used_reranker {
            scored.retain(|r| r.score >= 25.0);
        }
        scored.truncate(20);

        let json = serde_json::to_string_pretty(&scored)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "List all search containers (collections of indexed folders) with their names, descriptions, and indexed paths."
    )]
    async fn recall_list_containers(
        &self,
    ) -> Result<CallToolResult, McpError> {
        let containers: Vec<serde_json::Value> = self
            .state
            .config
            .containers
            .iter()
            .map(|(name, info)| {
                serde_json::json!({
                    "name": name,
                    "description": info.description,
                    "indexed_paths": info.indexed_paths,
                    "active": name == &self.state.config.active_container
                })
            })
            .collect();

        let json = serde_json::to_string_pretty(&containers)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler]
impl ServerHandler for RecallServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            server_info: Implementation {
                name: "recall-lite".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "Recall-Lite: local semantic file search. \
                 Use recall_search to find files by meaning, not just keywords. \
                 Use recall_list_containers to see available search scopes."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
        }
    }
}

fn get_app_data_dir() -> std::path::PathBuf {
    let base = std::env::var("APPDATA")
        .or_else(|_| std::env::var("XDG_DATA_HOME"))
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{}/.local/share", home)
        });
    std::path::PathBuf::from(base).join("com.recall-lite.app")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app_data = get_app_data_dir();
    let models_path = app_data.join("models");

    let db_path = app_data.join("lancedb");
    let db = lancedb::connect(db_path.to_string_lossy().as_ref())
        .execute()
        .await?;

    let config_path = app_data.join("config.json");
    let config = load_config(&config_path);

    let model_enum = get_embedding_model(&config.embedding_model);
    let model = indexer::load_model(model_enum, models_path.clone())?;
    let reranker = indexer::load_reranker(models_path).ok();

    let state = Arc::new(AppState {
        db,
        models: Arc::new(Mutex::new(Models { model, reranker })),
        config,
    });

    let server = RecallServer::new(state);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}
