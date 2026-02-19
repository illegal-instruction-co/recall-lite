use log::debug;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct ScoredResult {
    pub path: String,
    pub snippet: String,
    pub score: f32,
}

pub fn score_results(
    final_results: Vec<(String, String, f32)>,
    used_reranker: bool,
    used_hybrid: bool,
    max_results: usize,
) -> Vec<ScoredResult> {
    let mut scored: Vec<ScoredResult> = if used_reranker {
        final_results
            .into_iter()
            .map(|(path, snippet, raw_score)| {
                let sigmoid = 1.0 / (1.0 + (-raw_score).exp());
                let score = sigmoid * 100.0;
                debug!(
                    "reranker score: raw={:.4} → normalized={:.1} for {}",
                    raw_score, score, path
                );
                ScoredResult {
                    path,
                    snippet,
                    score,
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
                ScoredResult {
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
                ScoredResult {
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
        scored.retain(|r| r.score >= 1.0);
    }
    scored.truncate(max_results);
    scored
}
