//! Information Retrieval (IR) evaluation metrics.

use std::collections::HashMap;

use ctxvault_common::types::SearchResult;
use serde::{Deserialize, Serialize};

use crate::dataset::schema::RelevanceJudgment;

/// Evaluation metrics for a single query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryEvaluationMetrics {
    /// Recall at cutoff K.
    pub recall_at_k: f64,
    /// Precision at cutoff K.
    pub precision_at_k: f64,
    /// Mean Reciprocal Rank at cutoff K.
    pub mrr_at_k: f64,
    /// Normalized Discounted Cumulative Gain at cutoff K.
    pub ndcg_at_k: f64,
    /// Score separation: ratio of top-1 score to top-K score (or 0.0 if not applicable).
    pub score_separation: f64,
    /// Number of ground-truth hits present in the top-K.
    pub hits_at_k: usize,
    /// Total ground-truth relevant documents for this query.
    pub total_relevant: usize,
}

/// Evaluator computing standard Information Retrieval metrics.
pub struct IrEvaluator;

impl IrEvaluator {
    /// Evaluate a ranked result list against expected judgments at cutoff `k`.
    pub fn evaluate(
        results: &[SearchResult],
        judgments: &[RelevanceJudgment],
        k: usize,
    ) -> QueryEvaluationMetrics {
        if judgments.is_empty() {
            return QueryEvaluationMetrics {
                recall_at_k: 0.0,
                precision_at_k: 0.0,
                mrr_at_k: 0.0,
                ndcg_at_k: 0.0,
                score_separation: 0.0,
                hits_at_k: 0,
                total_relevant: 0,
            };
        }

        let grade_map: HashMap<&str, u8> =
            judgments.iter().map(|j| (j.path.as_str(), j.grade)).collect();

        let top_k: Vec<&SearchResult> = results.iter().take(k).collect();
        let mut hits = 0;
        let mut first_hit_rank: Option<usize> = None;
        let mut dcg = 0.0;

        for (idx, res) in top_k.iter().enumerate() {
            let rank = idx + 1;
            if let Some(&grade) = grade_map.get(res.path.as_str()) {
                if grade > 0 {
                    hits += 1;
                    if first_hit_rank.is_none() {
                        first_hit_rank = Some(rank);
                    }
                    // Graded DCG: (2^rel - 1) / log2(rank + 1)
                    let gain = 2.0f64.powi(grade as i32) - 1.0;
                    dcg += gain / (rank as f64 + 1.0).log2();
                }
            }
        }

        let recall_at_k = hits as f64 / judgments.len() as f64;
        let precision_at_k = if k > 0 { hits as f64 / k as f64 } else { 0.0 };
        let mrr_at_k = match first_hit_rank {
            Some(r) => 1.0 / r as f64,
            None => 0.0,
        };

        // Compute IDCG (Ideal DCG)
        let mut ideal_grades: Vec<u8> = judgments.iter().map(|j| j.grade).collect();
        ideal_grades.sort_by(|a, b| b.cmp(a));
        let mut idcg = 0.0;
        for (idx, &grade) in ideal_grades.iter().take(k).enumerate() {
            let rank = idx + 1;
            let gain = 2.0f64.powi(grade as i32) - 1.0;
            idcg += gain / (rank as f64 + 1.0).log2();
        }

        let ndcg_at_k = if idcg > 0.0 { dcg / idcg } else { 0.0 };

        let score_separation = if top_k.len() >= 2 && top_k[0].score > 0.0 {
            let last_score = top_k.last().map(|r| r.score).unwrap_or(1.0);
            if last_score > 0.0 {
                top_k[0].score / last_score
            } else {
                top_k[0].score
            }
        } else {
            1.0
        };

        QueryEvaluationMetrics {
            recall_at_k,
            precision_at_k,
            mrr_at_k,
            ndcg_at_k,
            score_separation,
            hits_at_k: hits,
            total_relevant: judgments.len(),
        }
    }
}
