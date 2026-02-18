use std::path::{Path, PathBuf};
use std::sync::Arc;

use mimalloc::MiMalloc;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::model::*;
use rmcp::tool;
use rmcp::transport::stdio;
use rmcp::{tool_handler, tool_router, schemars, ErrorData as McpError, ServerHandler, ServiceExt};

use serde::Deserialize;
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
    #[schemars(description = "Number of results to return (default 10, max 50)")]
    top_k: Option<usize>,
    #[schemars(description = "Filter by file extensions, e.g. [\"rs\", \"ts\", \"py\"]")]
    file_extensions: Option<Vec<String>>,
    #[schemars(description = "Filter by path prefix, e.g. \"src/indexer\"")]
    path_prefix: Option<String>,
    #[schemars(description = "Max snippet size in bytes (default 1500, max 10000)")]
    context_bytes: Option<usize>,
}


#[derive(Deserialize, schemars::JsonSchema)]
struct ReadFileParams {
    #[schemars(description = "Absolute path to the file to read. Must be within an indexed container.")]
    path: String,
    #[schemars(description = "Start line (1-indexed, inclusive). Omit to read from beginning.")]
    start_line: Option<u32>,
    #[schemars(description = "End line (1-indexed, inclusive). Omit to read to end.")]
    end_line: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ListFilesParams {
    container: Option<String>,
    #[schemars(description = "Filter files by path prefix, e.g. \"src/indexer\"")]
    path_prefix: Option<String>,
    #[schemars(description = "Filter by file extensions, e.g. [\"rs\", \"ts\"]")]
    extensions: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct IndexStatusParams {
    container: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct DiffParams {
    #[schemars(description = "Time window like '2h', '30m', '1d', '7d'. Finds files changed within this period.")]
    since: String,
    container: Option<String>,
    #[schemars(description = "Show git-style unified diff for each changed file (default true)")]
    show_diff: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct RelatedParams {
    #[schemars(description = "Absolute path to the file. Finds semantically similar files via vector proximity.")]
    path: String,
    container: Option<String>,
    #[schemars(description = "Number of related files to return (default 10, max 30)")]
    top_k: Option<usize>,
}

fn is_path_within_container(file_path: &Path, config: &Config, container_name: &str) -> bool {
    let canonical = match std::fs::canonicalize(file_path) {
        Ok(p) => p,
        Err(_) => return false,
    };
    if let Some(info) = config.containers.get(container_name) {
        for indexed_path in &info.indexed_paths {
            if let Ok(indexed_canonical) = std::fs::canonicalize(indexed_path) {
                if canonical.starts_with(&indexed_canonical) {
                    return true;
                }
            }
        }
    }
    false
}

fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    let (num_str, multiplier) = if let Some(n) = s.strip_suffix('s') {
        (n, 1u64)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60)
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 3600)
    } else if let Some(n) = s.strip_suffix('d') {
        (n, 86400)
    } else if let Some(n) = s.strip_suffix('w') {
        (n, 604800)
    } else {
        return None;
    };
    num_str.trim().parse::<u64>().ok().map(|n| n * multiplier)
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
        Parameters(SearchParams { query, container, top_k, file_extensions, path_prefix, context_bytes }): Parameters<SearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let container =
            container.unwrap_or_else(|| self.state.config.active_container.clone());
        let table_name = get_table_name(&container);

        let top_k = top_k.unwrap_or(10).min(50).max(1);
        let context_bytes = context_bytes.unwrap_or(1500).min(10000).max(100);

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

        let search_limit = top_k * 3;

        let pp_ref = path_prefix.as_deref();
        let fe_ref = file_extensions.as_deref();

        let (merged, used_hybrid) = indexer::search_pipeline(
            &self.state.db, &table_name, &query, &query_vector, search_limit, pp_ref, fe_ref,
        )
        .await
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let rerank_input: Vec<(String, String, f32)> =
            merged.into_iter().take(top_k * 2).collect();

        let (final_results, used_reranker) = {
            let mut guard = self.state.models.lock().await;
            if let Some(reranker) = guard.reranker.take() {
                let (reranker_back, results, used) =
                    indexer::safe_rerank(reranker, query.clone(), rerank_input.clone()).await;
                guard.reranker = reranker_back;
                if used {
                    (results, true)
                } else {
                    (rerank_input, false)
                }
            } else {
                (rerank_input, false)
            }
        };

        let mut scored = indexer::pipeline::score_results(final_results, used_reranker, used_hybrid, top_k);

        for item in &mut scored {
            if item.snippet.len() > context_bytes {
                let mut end = context_bytes;
                while end > 0 && !item.snippet.is_char_boundary(end) {
                    end -= 1;
                }
                item.snippet = item.snippet[..end].to_string();
            }
        }

        let json = serde_json::to_string_pretty(&scored)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Read file content by path. Supports optional line range. The file must be within an indexed container."
    )]
    async fn recall_read_file(
        &self,
        Parameters(ReadFileParams { path, start_line, end_line }): Parameters<ReadFileParams>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = PathBuf::from(&path);

        let mut authorized = false;
        for (name, _) in &self.state.config.containers {
            if is_path_within_container(&file_path, &self.state.config, name) {
                authorized = true;
                break;
            }
        }
        if !authorized {
            return Ok(CallToolResult::success(vec![Content::text(
                "access denied: file is not within any indexed container path.",
            )]));
        }

        if !file_path.is_file() {
            return Ok(CallToolResult::success(vec![Content::text(
                format!("file not found: {}", path),
            )]));
        }

        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| McpError::internal_error(format!("failed to read file: {}", e), None))?;

        let output = match (start_line, end_line) {
            (Some(start), Some(end)) => {
                let start = (start as usize).max(1);
                let end = end as usize;
                content
                    .lines()
                    .enumerate()
                    .filter(|(i, _)| {
                        let line_num = i + 1;
                        line_num >= start && line_num <= end
                    })
                    .map(|(_, line)| line)
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            (Some(start), None) => {
                let start = (start as usize).max(1);
                content
                    .lines()
                    .enumerate()
                    .filter(|(i, _)| (i + 1) >= start)
                    .map(|(_, line)| line)
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            (None, Some(end)) => {
                let end = end as usize;
                content
                    .lines()
                    .enumerate()
                    .filter(|(i, _)| (i + 1) <= end)
                    .map(|(_, line)| line)
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            (None, None) => content,
        };

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        description = "List indexed file paths with metadata. Returns deduplicated file list from the search index."
    )]
    async fn recall_list_files(
        &self,
        Parameters(ListFilesParams { container, path_prefix, extensions }): Parameters<ListFilesParams>,
    ) -> Result<CallToolResult, McpError> {
        use arrow_array::StringArray;
        use futures::TryStreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase};

        let container =
            container.unwrap_or_else(|| self.state.config.active_container.clone());
        let table_name = get_table_name(&container);

        let table = match self.state.db.open_table(&table_name).execute().await {
            Ok(t) => t,
            Err(_) => {
                return Ok(CallToolResult::success(vec![Content::text(
                    format!("no index found for container '{}'.", container),
                )]));
            }
        };

        let mut query = table.query().select(lancedb::query::Select::Columns(vec!["path".to_string()]));

        if let Some(filter) = indexer::build_filter_expr(path_prefix.as_deref(), extensions.as_deref()) {
            query = query.only_if(filter);
        }

        let results = query
            .execute()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut paths = std::collections::BTreeSet::new();
        for batch in results {
            if let Some(path_array) = batch
                .column_by_name("path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            {
                for i in 0..batch.num_rows() {
                    paths.insert(path_array.value(i).to_string());
                }
            }
        }

        let file_list: Vec<serde_json::Value> = paths
            .iter()
            .map(|p| {
                let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                serde_json::json!({
                    "path": p,
                    "size_bytes": size,
                })
            })
            .collect();

        let json = serde_json::to_string_pretty(&serde_json::json!({
            "container": container,
            "total_files": file_list.len(),
            "files": file_list,
        }))
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Get index status: total files, total chunks, and container metadata. Use this to check if the index is populated before searching."
    )]
    async fn recall_index_status(
        &self,
        Parameters(IndexStatusParams { container }): Parameters<IndexStatusParams>,
    ) -> Result<CallToolResult, McpError> {
        use arrow_array::StringArray;
        use futures::TryStreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase};

        let container =
            container.unwrap_or_else(|| self.state.config.active_container.clone());
        let table_name = get_table_name(&container);

        let container_info = self.state.config.containers.get(&container);
        let indexed_paths: Vec<String> = container_info
            .map(|info| info.indexed_paths.clone())
            .unwrap_or_default();
        let description = container_info
            .map(|info| info.description.clone())
            .unwrap_or_default();

        let table = match self.state.db.open_table(&table_name).execute().await {
            Ok(t) => t,
            Err(_) => {
                let json = serde_json::to_string_pretty(&serde_json::json!({
                    "container": container,
                    "description": description,
                    "indexed_paths": indexed_paths,
                    "total_files": 0,
                    "total_chunks": 0,
                    "has_index": false,
                }))
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                return Ok(CallToolResult::success(vec![Content::text(json)]));
            }
        };

        let results = table
            .query()
            .select(lancedb::query::Select::Columns(vec!["path".to_string()]))
            .execute()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut total_chunks: usize = 0;
        let mut unique_paths = std::collections::HashSet::new();

        for batch in results {
            total_chunks += batch.num_rows();
            if let Some(path_array) = batch
                .column_by_name("path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            {
                for i in 0..batch.num_rows() {
                    unique_paths.insert(path_array.value(i).to_string());
                }
            }
        }

        let json = serde_json::to_string_pretty(&serde_json::json!({
            "container": container,
            "description": description,
            "indexed_paths": indexed_paths,
            "total_files": unique_paths.len(),
            "total_chunks": total_chunks,
            "has_index": true,
        }))
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Find files that changed recently. Returns paths, timestamps, and optionally git-style diffs. Use at conversation start to understand what's been modified."
    )]
    async fn recall_diff(
        &self,
        Parameters(DiffParams { since, container, show_diff }): Parameters<DiffParams>,
    ) -> Result<CallToolResult, McpError> {
        use arrow_array::{Int64Array, StringArray};
        use futures::TryStreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase};

        let container =
            container.unwrap_or_else(|| self.state.config.active_container.clone());
        let table_name = get_table_name(&container);
        let show_diff = show_diff.unwrap_or(true);

        let seconds = parse_duration(&since).ok_or_else(|| {
            McpError::invalid_params(format!("invalid duration '{}'. use format like '2h', '30m', '1d'", since), None)
        })?;

        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - seconds as i64;

        let table = match self.state.db.open_table(&table_name).execute().await {
            Ok(t) => t,
            Err(_) => {
                return Ok(CallToolResult::success(vec![Content::text(
                    format!("no index found for container '{}'.", container),
                )]));
            }
        };

        let results = table
            .query()
            .only_if(format!("mtime >= {}", cutoff))
            .select(lancedb::query::Select::Columns(vec!["path".to_string(), "mtime".to_string()]))
            .execute()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut file_mtimes: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
        for batch in results {
            let path_array = batch
                .column_by_name("path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let mtime_array = batch
                .column_by_name("mtime")
                .and_then(|c| c.as_any().downcast_ref::<Int64Array>());
            if let (Some(paths), Some(mtimes)) = (path_array, mtime_array) {
                for i in 0..batch.num_rows() {
                    let path = paths.value(i).to_string();
                    let mtime = mtimes.value(i);
                    file_mtimes
                        .entry(path)
                        .and_modify(|existing| {
                            if mtime > *existing {
                                *existing = mtime;
                            }
                        })
                        .or_insert(mtime);
                }
            }
        }

        if file_mtimes.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                format!("no files changed in the last {}.", since),
            )]));
        }

        let mut changed_files: Vec<serde_json::Value> = Vec::new();
        for (path, mtime) in &file_mtimes {
            let mut entry = serde_json::json!({
                "path": path,
                "modified_unix": mtime,
            });

            if show_diff {
                let file_path = PathBuf::from(path);
                if file_path.is_file() {
                    if let Ok(content) = std::fs::read_to_string(&file_path) {
                        let preview: String = content.lines().take(50).collect::<Vec<_>>().join("\n");
                        let total_lines = content.lines().count();
                        entry["preview"] = serde_json::json!(preview);
                        entry["total_lines"] = serde_json::json!(total_lines);
                    }
                } else {
                    entry["status"] = serde_json::json!("deleted");
                }
            }

            changed_files.push(entry);
        }

        let json = serde_json::to_string_pretty(&serde_json::json!({
            "since": since,
            "total_changed": changed_files.len(),
            "files": changed_files,
        }))
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Find files semantically related to a given file. Uses vector proximity in the embedding space -- finds files with similar meaning, not just similar names."
    )]
    async fn recall_related(
        &self,
        Parameters(RelatedParams { path, container, top_k }): Parameters<RelatedParams>,
    ) -> Result<CallToolResult, McpError> {
        use arrow_array::{Float32Array, StringArray};
        use futures::TryStreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase};

        let container =
            container.unwrap_or_else(|| self.state.config.active_container.clone());
        let table_name = get_table_name(&container);
        let top_k = top_k.unwrap_or(10).min(30).max(1);

        let table = match self.state.db.open_table(&table_name).execute().await {
            Ok(t) => t,
            Err(_) => {
                return Ok(CallToolResult::success(vec![Content::text(
                    format!("no index found for container '{}'.", container),
                )]));
            }
        };

        let safe_path = path.replace('\'', "''");
        let chunks = table
            .query()
            .only_if(format!("path = '{}'", safe_path))
            .execute()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut avg_vector: Option<Vec<f32>> = None;
        let mut chunk_count = 0usize;

        for batch in &chunks {
            if let Some(vector_col) = batch.column_by_name("vector") {
                use arrow_array::FixedSizeListArray;
                if let Some(fsl) = vector_col.as_any().downcast_ref::<FixedSizeListArray>() {
                    for i in 0..batch.num_rows() {
                        let values = fsl.value(i);
                        if let Some(float_arr) = values.as_any().downcast_ref::<Float32Array>() {
                            let vec: Vec<f32> = (0..float_arr.len()).map(|j| float_arr.value(j)).collect();
                            match &mut avg_vector {
                                Some(avg) => {
                                    for (k, v) in avg.iter_mut().enumerate() {
                                        *v += vec[k];
                                    }
                                }
                                None => avg_vector = Some(vec),
                            }
                            chunk_count += 1;
                        }
                    }
                }
            }
        }

        let query_vector = match avg_vector {
            Some(mut avg) if chunk_count > 0 => {
                for v in avg.iter_mut() {
                    *v /= chunk_count as f32;
                }
                avg
            }
            _ => {
                return Ok(CallToolResult::success(vec![Content::text(
                    format!("file '{}' not found in index. make sure it's been indexed.", path),
                )]));
            }
        };

        let search_limit = (top_k + 1) * 3;
        let vq = table
            .vector_search(query_vector.as_slice())
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let results = vq
            .distance_type(lancedb::DistanceType::Cosine)
            .select(lancedb::query::Select::Columns(vec!["path".to_string(), "content".to_string()]))
            .limit(search_limit)
            .execute()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut best_per_file: std::collections::HashMap<String, (String, f32)> = std::collections::HashMap::new();
        for batch in results {
            let path_array = batch
                .column_by_name("path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let content_array = batch
                .column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let dist_array = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

            if let (Some(paths), Some(contents), Some(dists)) = (path_array, content_array, dist_array) {
                for i in 0..batch.num_rows() {
                    let p = paths.value(i).to_string();
                    if p == path {
                        continue;
                    }
                    let dist = dists.value(i);
                    match best_per_file.get(&p) {
                        Some((_, existing_dist)) if *existing_dist <= dist => {}
                        _ => {
                            best_per_file.insert(p, (contents.value(i).to_string(), dist));
                        }
                    }
                }
            }
        }

        let mut related: Vec<(String, String, f32)> = best_per_file
            .into_iter()
            .map(|(p, (snippet, dist))| (p, snippet, dist))
            .collect();
        related.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        related.truncate(top_k);

        let items: Vec<serde_json::Value> = related
            .into_iter()
            .map(|(p, snippet, dist)| {
                let similarity = ((1.0 - dist).clamp(0.0, 1.0) * 100.0) as u32;
                serde_json::json!({
                    "path": p,
                    "snippet": snippet,
                    "similarity": similarity,
                })
            })
            .collect();

        let json = serde_json::to_string_pretty(&serde_json::json!({
            "source": path,
            "total_related": items.len(),
            "related_files": items,
        }))
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
                "Recall-Lite: local semantic file search for AI agents. \
                 Use recall_search to find files by meaning with filtering (top_k, file_extensions, path_prefix, context_bytes). \
                 Use recall_read_file to read file content by path (with optional line range). \
                 Use recall_list_files to browse indexed file paths. \
                 Use recall_index_status to check index health and stats. \
                 Use recall_diff to see what files changed recently (e.g. '2h', '1d'). Start conversations with this. \
                 Use recall_related to find semantically similar files to a given file path. \
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
