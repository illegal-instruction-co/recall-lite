use std::collections::HashMap;

use anyhow::{anyhow, Result};
use arrow_array::{Float32Array, StringArray};
use futures::TryStreamExt;
use lancedb::connection::Connection;
use lancedb::index::scalar::FullTextSearchQuery;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::DistanceType;

pub fn build_filter_expr(
    path_prefix: Option<&str>,
    file_extensions: Option<&[String]>,
) -> Option<String> {
    let mut clauses = Vec::new();

    if let Some(prefix) = path_prefix {
        let escaped = prefix
            .replace('\\', "\\\\")
            .replace('\'', "''")
            .replace('%', "\\%")
            .replace('_', "\\_");
        clauses.push(format!("path LIKE '{}%' ESCAPE '\\'", escaped));
    }

    if let Some(exts) = file_extensions {
        if !exts.is_empty() {
            let ext_clauses: Vec<String> = exts
                .iter()
                .map(|ext| {
                    let clean = ext
                        .trim_start_matches('.')
                        .replace('\\', "\\\\")
                        .replace('\'', "''")
                        .replace('%', "\\%")
                        .replace('_', "\\_");
                    format!("path LIKE '%.{}' ESCAPE '\\'", clean)
                })
                .collect();
            clauses.push(format!("({})", ext_clauses.join(" OR ")));
        }
    }

    if clauses.is_empty() {
        None
    } else {
        Some(clauses.join(" AND "))
    }
}

pub async fn search_files(
    db: &Connection,
    table_name: &str,
    query_vector: &[f32],
    limit: usize,
    path_prefix: Option<&str>,
    file_extensions: Option<&[String]>,
    multi_chunk: bool,
) -> Result<Vec<(String, String, f32)>> {
    let table = match db.open_table(table_name).execute().await {
        Ok(t) => t,
        Err(_) => return Err(anyhow!("No index found for '{}'. Index some folders first.", table_name)),
    };

    let schema = table.schema().await?;
    if let Ok(field) = schema.field_with_name("vector") {
        if let arrow_schema::DataType::FixedSizeList(_, size) = field.data_type() {
            if *size != query_vector.len() as i32 {
                return Err(anyhow!(
                    "Model changed: index has {}-dim vectors but current model produces {}-dim. Please rebuild the index.",
                    size, query_vector.len()
                ));
            }
        }
    }

    let search_limit = if multi_chunk { limit * 3 } else { limit * 2 };

    let mut query = table
        .vector_search(query_vector)?
        .distance_type(DistanceType::Cosine)
        .select(lancedb::query::Select::Columns(vec!["path".to_string(), "content".to_string()]))
        .limit(search_limit);

    if let Some(filter) = build_filter_expr(path_prefix, file_extensions) {
        query = query.only_if(filter);
    }

    let results = query
        .execute()
        .await?
        .try_collect::<Vec<_>>()
        .await?;

    if multi_chunk {
        let mut matches = Vec::new();

        for batch in results {
            let path_array = batch
                .column_by_name("path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow!("Missing or invalid 'path' column"))?;

            let content_array = batch
                .column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow!("Missing or invalid 'content' column"))?;

            let dist_array = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
                .ok_or_else(|| anyhow!("Missing or invalid '_distance' column"))?;

            for i in 0..batch.num_rows() {
                matches.push((
                    path_array.value(i).to_string(),
                    content_array.value(i).to_string(),
                    dist_array.value(i),
                ));
            }
        }

        matches.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        matches.truncate(limit);
        Ok(matches)
    } else {
        let mut best_per_file: HashMap<String, (String, f32)> = HashMap::new();

        for batch in results {
            let path_array = batch
                .column_by_name("path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow!("Missing or invalid 'path' column"))?;

            let content_array = batch
                .column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow!("Missing or invalid 'content' column"))?;

            let dist_array = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
                .ok_or_else(|| anyhow!("Missing or invalid '_distance' column"))?;

            for i in 0..batch.num_rows() {
                let path = path_array.value(i).to_string();
                let content = content_array.value(i).to_string();
                let dist = dist_array.value(i);

                match best_per_file.get(&path) {
                    Some((_, existing_dist)) if *existing_dist <= dist => {}
                    _ => {
                        best_per_file.insert(path, (content, dist));
                    }
                }
            }
        }

        let mut matches: Vec<(String, String, f32)> = best_per_file
            .into_iter()
            .map(|(path, (content, dist))| (path, content, dist))
            .collect();

        matches.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        matches.truncate(limit);

        Ok(matches)
    }
}

pub async fn search_fts(
    db: &Connection,
    table_name: &str,
    query: &str,
    limit: usize,
    path_prefix: Option<&str>,
    file_extensions: Option<&[String]>,
    multi_chunk: bool,
) -> Result<Vec<(String, String)>> {
    let table = match db.open_table(table_name).execute().await {
        Ok(t) => t,
        Err(_) => return Err(anyhow!("No index found for '{}'. Index some folders first.", table_name)),
    };

    let fts_query = FullTextSearchQuery::new(query.to_string());
    let search_limit = if multi_chunk { limit * 3 } else { limit * 2 };
    let mut q = table
        .query()
        .full_text_search(fts_query)
        .limit(search_limit);

    if let Some(filter) = build_filter_expr(path_prefix, file_extensions) {
        q = q.only_if(filter);
    }

    let results = q
        .execute()
        .await?
        .try_collect::<Vec<_>>()
        .await?;

    let mut matches = Vec::new();

    if multi_chunk {
        for batch in results {
            let path_array = batch
                .column_by_name("path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let content_array = batch
                .column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            if let (Some(paths), Some(contents)) = (path_array, content_array) {
                for i in 0..batch.num_rows() {
                    matches.push((paths.value(i).to_string(), contents.value(i).to_string()));
                    if matches.len() >= limit {
                        return Ok(matches);
                    }
                }
            }
        }
    } else {
        let mut seen_paths = std::collections::HashSet::new();

        for batch in results {
            let path_array = batch
                .column_by_name("path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let content_array = batch
                .column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            if let (Some(paths), Some(contents)) = (path_array, content_array) {
                for i in 0..batch.num_rows() {
                    let path = paths.value(i).to_string();
                    if seen_paths.insert(path.clone()) {
                        matches.push((path, contents.value(i).to_string()));
                    }
                    if matches.len() >= limit {
                        return Ok(matches);
                    }
                }
            }
        }
    }

    Ok(matches)
}

pub fn hybrid_merge(
    vector_results: &[(String, String, f32)],
    fts_results: &[(String, String)],
    limit: usize,
) -> Vec<(String, String, f32)> {
    let k = 60.0_f32;

    let mut rrf_scores: HashMap<String, (String, f32)> = HashMap::new();

    for (rank, (path, snippet, _)) in vector_results.iter().enumerate() {
        let score = 1.0 / (k + rank as f32 + 1.0);
        rrf_scores.insert(path.clone(), (snippet.clone(), score));
    }

    for (rank, (path, snippet)) in fts_results.iter().enumerate() {
        let score = 1.0 / (k + rank as f32 + 1.0);
        rrf_scores
            .entry(path.clone())
            .and_modify(|(_, s)| *s += score)
            .or_insert_with(|| (snippet.clone(), score));
    }

    let mut merged: Vec<(String, String, f32)> = rrf_scores
        .into_iter()
        .map(|(path, (snippet, score))| (path, snippet, score))
        .collect();

    merged.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(limit);
    merged
}

pub async fn search_pipeline(
    db: &Connection,
    table_name: &str,
    query: &str,
    query_vector: &[f32],
    search_limit: usize,
    path_prefix: Option<&str>,
    file_extensions: Option<&[String]>,
) -> Result<(Vec<(String, String, f32)>, bool)> {
    let query_variants = super::chunking::expand_query(query);

    let vector_fut = search_files(db, table_name, query_vector, search_limit, path_prefix, file_extensions, false);

    let fts_db = db.clone();
    let fts_table = table_name.to_string();
    let fe_clone: Option<Vec<String>> = file_extensions.map(|s| s.to_vec());
    let pp_clone: Option<String> = path_prefix.map(|s| s.to_string());
    let fts_fut = async move {
        let pp_ref = pp_clone.as_deref();
        let fe_ref = fe_clone.as_deref();
        let futs: Vec<_> = query_variants
            .iter()
            .map(|v| search_fts(&fts_db, &fts_table, v, 30, pp_ref, fe_ref, false))
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
    let vector_results = vector_result?;

    let used_hybrid = !fts_results.is_empty();
    let merged = if fts_results.is_empty() {
        vector_results
    } else {
        hybrid_merge(&vector_results, &fts_results, search_limit)
    };

    Ok((merged, used_hybrid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_merge() {
        let vector = vec![
            ("a.txt".to_string(), "hello".to_string(), 0.1),
            ("b.txt".to_string(), "world".to_string(), 0.2),
        ];
        let fts = vec![
            ("b.txt".to_string(), "world".to_string()),
            ("c.txt".to_string(), "new".to_string()),
        ];
        let merged = hybrid_merge(&vector, &fts, 10);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].0, "b.txt");
    }

    #[test]
    fn test_build_filter_expr_none() {
        assert_eq!(build_filter_expr(None, None), None);
    }

    #[test]
    fn test_build_filter_expr_prefix_only() {
        let result = build_filter_expr(Some("src/indexer"), None);
        assert_eq!(result, Some("path LIKE 'src/indexer%' ESCAPE '\\'".to_string()));
    }

    #[test]
    fn test_build_filter_expr_extensions_only() {
        let exts = vec!["rs".to_string(), "ts".to_string()];
        let result = build_filter_expr(None, Some(&exts));
        assert_eq!(result, Some("(path LIKE '%.rs' ESCAPE '\\' OR path LIKE '%.ts' ESCAPE '\\')".to_string()));
    }

    #[test]
    fn test_build_filter_expr_both() {
        let exts = vec!["py".to_string()];
        let result = build_filter_expr(Some("lib/"), Some(&exts));
        assert_eq!(result, Some("path LIKE 'lib/%' ESCAPE '\\' AND (path LIKE '%.py' ESCAPE '\\')".to_string()));
    }

    #[test]
    fn test_build_filter_expr_dot_prefix_stripped() {
        let exts = vec![".rs".to_string()];
        let result = build_filter_expr(None, Some(&exts));
        assert_eq!(result, Some("(path LIKE '%.rs' ESCAPE '\\')".to_string()));
    }

    #[test]
    fn test_build_filter_expr_empty_extensions() {
        let exts: Vec<String> = vec![];
        assert_eq!(build_filter_expr(None, Some(&exts)), None);
    }

    #[test]
    fn test_build_filter_expr_underscore_escaped() {
        let result = build_filter_expr(Some("src/my_module"), None);
        assert_eq!(result, Some("path LIKE 'src/my\\_module%' ESCAPE '\\'".to_string()));
    }

    #[test]
    fn test_build_filter_expr_percent_escaped() {
        let result = build_filter_expr(Some("100%done"), None);
        assert_eq!(result, Some("path LIKE '100\\%done%' ESCAPE '\\'".to_string()));
    }
}
