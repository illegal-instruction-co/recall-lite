use std::path::{Path, PathBuf};
use std::sync::Arc;

use log::{info, debug};

use mimalloc::MiMalloc;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::model::*;
use rmcp::tool;
use rmcp::transport::stdio;
use rmcp::{tool_handler, tool_router, schemars, ErrorData as McpError, ServerHandler, ServiceExt};

use serde::Deserialize;
use tokio::sync::Mutex;

use rememex_lib::config::{get_embedding_model, get_table_name, load_config, Config, EmbeddingProviderConfig};
use rememex_lib::indexer;
use rememex_lib::indexer::annotations;
use rememex_lib::indexer::embedding_provider::{EmbeddingProvider, LocalProvider, RemoteProvider};
use rememex_lib::state::ModelState;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

struct AppState {
    db: lancedb::Connection,
    provider: Arc<Mutex<Box<dyn EmbeddingProvider>>>,
    reranker: Arc<Mutex<Option<fastembed::TextRerank>>>,
    config: Config,
}

#[derive(Clone)]
pub struct RememexServer {
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
    #[schemars(description = "Minimum relevance score (0-100). Results below this are filtered out. Default: no filtering.")]
    min_score: Option<f32>,
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

#[derive(Deserialize, schemars::JsonSchema)]
struct AnnotateParams {
    #[schemars(description = "Absolute path to the file to annotate.")]
    path: String,
    #[schemars(description = "The annotation note to attach to the file. This text is embedded and searchable.")]
    note: String,
    container: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct AnnotationsParams {
    #[schemars(description = "Optional. Absolute file path to filter annotations for.")]
    path: Option<String>,
    container: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct DeleteAnnotationParams {
    #[schemars(description = "The annotation ID to delete (e.g. 'ann_...'). Get IDs from rememex_annotations.")]
    annotation_id: String,
    container: Option<String>,
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
impl RememexServer {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Search indexed files using semantic + keyword hybrid search. Returns ranked results with file paths, relevant snippets, and relevance scores."
    )]
    async fn rememex_search(
        &self,
        Parameters(SearchParams { query, container, top_k, file_extensions, path_prefix, context_bytes, min_score }): Parameters<SearchParams>,
    ) -> Result<CallToolResult, McpError> {
        debug!("rememex_search: query=\"{}\", container={:?}, top_k={:?}", query, container, top_k);
        let container =
            container.unwrap_or_else(|| self.state.config.active_container.clone());
        let table_name = get_table_name(&container);

        let top_k = top_k.unwrap_or(10).clamp(1, 50);
        let context_bytes = context_bytes.unwrap_or(1500).clamp(100, 10000);

        let table_check = self.state.db.table_names().execute().await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        if !table_check.iter().any(|t| t == &table_name) {
            return Ok(CallToolResult::success(vec![Content::text(
                format!("no index found for container '{}'. open Rememex and index some folders first.", container),
            )]));
        }

        let query_vector = {
            let guard = self.state.provider.lock().await;
            guard.embed_query(&query).await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        };

        let search_limit = top_k * 3;

        let pp_ref = path_prefix.as_deref();
        let fe_ref = file_extensions.as_deref();

        let (mut merged, used_hybrid) = indexer::search_pipeline(
            &self.state.db, &table_name, &query, &query_vector, search_limit, pp_ref, fe_ref,
        )
        .await
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        if let Ok(ann_results) = annotations::search_annotations(&self.state.db, &table_name, &query_vector, 10).await {
            if used_hybrid {
                for (rank, (path, note, _dist)) in ann_results.into_iter().enumerate() {
                    let rrf_score = 1.0 / (60.0 + rank as f32 + 1.0);
                    merged.push((path, note, rrf_score));
                }
                merged.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
            } else {
                for (path, note, dist) in ann_results {
                    merged.push((path, note, dist));
                }
                merged.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            }
        }

        let rerank_input: Vec<(String, String, f32)> =
            merged.into_iter().take(top_k * 2).collect();

        let (final_results, used_reranker) = {
            let reranker = {
                let mut guard = self.state.reranker.lock().await;
                guard.take()
            };
            if let Some(reranker) = reranker {
                let (reranker_back, results, used) =
                    indexer::safe_rerank(reranker, query.clone(), rerank_input.clone()).await;
                {
                    let mut guard = self.state.reranker.lock().await;
                    *guard = reranker_back;
                }
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
        scored.retain(|item| item.score >= min_score.unwrap_or(0.0));

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
    async fn rememex_read_file(
        &self,
        Parameters(ReadFileParams { path, start_line, end_line }): Parameters<ReadFileParams>,
    ) -> Result<CallToolResult, McpError> {
        debug!("rememex_read_file: path={}, lines={:?}-{:?}", path, start_line, end_line);
        let file_path = PathBuf::from(&path);

        let mut authorized = false;
        for name in self.state.config.containers.keys() {
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
    async fn rememex_list_files(
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
    async fn rememex_index_status(
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
    async fn rememex_diff(
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
    async fn rememex_related(
        &self,
        Parameters(RelatedParams { path, container, top_k }): Parameters<RelatedParams>,
    ) -> Result<CallToolResult, McpError> {
        use arrow_array::{Float32Array, StringArray};
        use futures::TryStreamExt;
        use lancedb::query::{ExecutableQuery, QueryBase};

        let container =
            container.unwrap_or_else(|| self.state.config.active_container.clone());
        let table_name = get_table_name(&container);
        let top_k = top_k.unwrap_or(10).clamp(1, 30);

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
    async fn rememex_list_containers(
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

    #[tool(
        description = "Add a searchable annotation (note) to a file. The note is embedded and will appear in future search results. Use this to leave context, warnings, or explanations for yourself or other agents."
    )]
    async fn rememex_annotate(
        &self,
        Parameters(AnnotateParams { path, note, container }): Parameters<AnnotateParams>,
    ) -> Result<CallToolResult, McpError> {
        let container_name = container
            .as_deref()
            .unwrap_or(&self.state.config.active_container);
        let table_name = get_table_name(container_name);

        let vector = {
            let provider = self.state.provider.lock().await;
            provider.embed_passages(vec![note.clone()]).await
                .map_err(|e| McpError::internal_error(format!("Embedding failed: {}", e), None))?
                .into_iter()
                .next()
                .ok_or_else(|| McpError::internal_error("Empty embedding result", None))?
        };

        let dim = vector.len();
        let ann_table_name = format!("{}_annotations", table_name);
        let ann_table = if let Ok(t) = self.state.db.open_table(&ann_table_name).execute().await {
            t
        } else {
            use arrow_schema::{DataType, Field, Schema};
            use arrow_array::RecordBatchIterator;
            let schema = std::sync::Arc::new(Schema::new(vec![
                Field::new("id", DataType::Utf8, false),
                Field::new("path", DataType::Utf8, false),
                Field::new("note", DataType::Utf8, false),
                Field::new("source", DataType::Utf8, false),
                Field::new("vector", DataType::FixedSizeList(
                    std::sync::Arc::new(Field::new("item", DataType::Float32, true)),
                    dim as i32,
                ), false),
                Field::new("created_at", DataType::Int64, false),
            ]));
            self.state.db.create_table(&ann_table_name, RecordBatchIterator::new(vec![], schema))
                .execute().await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        };

        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let id = format!("ann_{}", now.as_nanos());
        let created_at = now.as_secs() as i64;

        use arrow_array::*;
        use arrow_schema::{DataType, Field, Schema};
        let schema = std::sync::Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("path", DataType::Utf8, false),
            Field::new("note", DataType::Utf8, false),
            Field::new("source", DataType::Utf8, false),
            Field::new("vector", DataType::FixedSizeList(
                std::sync::Arc::new(Field::new("item", DataType::Float32, true)),
                dim as i32,
            ), false),
            Field::new("created_at", DataType::Int64, false),
        ]));
        let vector_array = FixedSizeListArray::try_new(
            std::sync::Arc::new(Field::new("item", DataType::Float32, true)),
            dim as i32,
            std::sync::Arc::new(Float32Array::from(vector)),
            None,
        ).map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                std::sync::Arc::new(StringArray::from(vec![id.as_str()])),
                std::sync::Arc::new(StringArray::from(vec![path.as_str()])),
                std::sync::Arc::new(StringArray::from(vec![note.as_str()])),
                std::sync::Arc::new(StringArray::from(vec!["agent"])),
                std::sync::Arc::new(vector_array),
                std::sync::Arc::new(Int64Array::from(vec![created_at])),
            ],
        ).map_err(|e| McpError::internal_error(e.to_string(), None))?;

        ann_table.add(RecordBatchIterator::new(vec![Ok(batch)], schema))
            .execute().await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let result = serde_json::json!({
            "id": id,
            "path": path,
            "note": note,
            "source": "agent",
            "created_at": created_at
        });
        Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
    }

    #[tool(
        description = "List annotations for a file or all annotations in a container. Returns id, path, note, and timestamp for each."
    )]
    async fn rememex_annotations(
        &self,
        Parameters(AnnotationsParams { path, container }): Parameters<AnnotationsParams>,
    ) -> Result<CallToolResult, McpError> {
        let container_name = container
            .as_deref()
            .unwrap_or(&self.state.config.active_container);
        let table_name = get_table_name(container_name);

        let result = annotations::get_annotations(&self.state.db, &table_name, path.as_deref())
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Delete an agent-created annotation by ID. Only annotations with source 'agent' can be deleted via MCP -- user-created annotations are protected. Use rememex_annotations to get annotation IDs first."
    )]
    async fn rememex_delete_annotation(
        &self,
        Parameters(DeleteAnnotationParams { annotation_id, container }): Parameters<DeleteAnnotationParams>,
    ) -> Result<CallToolResult, McpError> {
        let container_name = container
            .as_deref()
            .unwrap_or(&self.state.config.active_container);
        let table_name = get_table_name(container_name);

        let all = annotations::get_annotations(&self.state.db, &table_name, None)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let target = all.iter().find(|a| a.id == annotation_id);

        let annotation = match target {
            Some(a) => a,
            None => {
                return Ok(CallToolResult::success(vec![Content::text(
                    format!("annotation '{}' not found.", annotation_id),
                )]));
            }
        };

        if annotation.source != "agent" {
            return Ok(CallToolResult::success(vec![Content::text(
                format!("cannot delete annotation '{}': source is '{}', not 'agent'. only agent-created annotations can be deleted via MCP.", annotation_id, annotation.source),
            )]));
        }

        annotations::delete_annotation(&self.state.db, &table_name, &annotation_id)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let result = serde_json::json!({
            "deleted": true,
            "id": annotation_id,
            "path": annotation.path,
            "note": annotation.note,
        });
        Ok(CallToolResult::success(vec![Content::text(result.to_string())]))
    }
}

#[tool_handler]
impl ServerHandler for RememexServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            server_info: Implementation {
                name: "rememex".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "Rememex: local semantic file search for AI agents. \
                 Use rememex_search to find files by meaning with filtering (top_k, file_extensions, path_prefix, context_bytes, min_score). \
                 Use rememex_read_file to read file content by path (with optional line range). \
                 Use rememex_list_files to browse indexed file paths. \
                 Use rememex_index_status to check index health and stats. \
                 Use rememex_diff to see what files changed recently (e.g. '2h', '1d'). Start conversations with this. \
                 Use rememex_related to find semantically similar files to a given file path. \
                 Use rememex_annotate to add searchable notes to files (they appear in future searches). \
                 Use rememex_annotations to list existing annotations. \
                 Use rememex_delete_annotation to remove outdated agent-created annotations by ID (user annotations are protected). \
                 Use rememex_list_containers to see available search scopes."
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
    std::path::PathBuf::from(base).join("com.rememex.app")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    info!("MCP server starting...");
    let app_data = get_app_data_dir();
    let models_path = app_data.join("models");

    let db_path = app_data.join("lancedb");
    let db = lancedb::connect(db_path.to_string_lossy().as_ref())
        .execute()
        .await?;

    let config_path = app_data.join("config.json");
    let config = load_config(&config_path);
    info!("Config loaded, active container: {}", config.active_container);

    let provider: Box<dyn EmbeddingProvider> = match &config.embedding_provider {
        EmbeddingProviderConfig::Local { model } => {
            let model_enum = get_embedding_model(model);
            let model = indexer::load_model(model_enum, models_path.clone())?;
            let model_state = Arc::new(Mutex::new(ModelState {
                model: Some(model),
                init_error: None,
                cached_dim: None,
            }));
            Box::new(LocalProvider { model_state })
        }
        EmbeddingProviderConfig::Remote(rc) => {
            Box::new(RemoteProvider::new(rc.clone()))
        }
    };
    info!("Embedding provider ready");

    let reranker = indexer::load_reranker(models_path).ok();

    let state = Arc::new(AppState {
        db,
        provider: Arc::new(Mutex::new(provider)),
        reranker: Arc::new(Mutex::new(reranker)),
        config,
    });

    let server = RememexServer::new(state);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}
