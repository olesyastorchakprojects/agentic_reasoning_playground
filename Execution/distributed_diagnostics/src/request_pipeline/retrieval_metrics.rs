use crate::shared_types::RetrievalEvaluationMetrics;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub(crate) struct GoldenRetrievalTargetsById {
    pub strict_positive_ids: Vec<String>,
    pub soft_positive_ids: Vec<String>,
    pub graded_relevance: Vec<GoldenRetrievalRelevanceById>,
}

#[derive(Debug, Clone)]
pub(crate) struct GoldenRetrievalRelevanceById {
    pub id: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub(crate) enum RetrievalMetricsError {
    InvalidGoldenTargets { reason: String },
    InvalidK { reason: String },
    InconsistentGradedRelevance { reason: String },
    #[allow(dead_code)]
    UnexpectedComputationState { reason: String },
}

impl std::fmt::Display for RetrievalMetricsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RetrievalMetricsError::InvalidGoldenTargets { reason } => {
                write!(f, "invalid golden targets: {}", reason)
            }
            RetrievalMetricsError::InvalidK { reason } => {
                write!(f, "invalid k: {}", reason)
            }
            RetrievalMetricsError::InconsistentGradedRelevance { reason } => {
                write!(f, "inconsistent graded relevance: {}", reason)
            }
            RetrievalMetricsError::UnexpectedComputationState { reason } => {
                write!(f, "unexpected computation state: {}", reason)
            }
        }
    }
}

impl std::error::Error for RetrievalMetricsError {}

pub(crate) fn compute_retrieval_metrics(
    golden_targets: &GoldenRetrievalTargetsById,
    actual_ranked_ids: &[String],
    k: usize,
) -> Result<RetrievalEvaluationMetrics, RetrievalMetricsError> {
    if k == 0 {
        return Err(RetrievalMetricsError::InvalidK {
            reason: "k must be positive".to_string(),
        });
    }

    let normalized_targets = validate_and_normalize_golden_targets(golden_targets)?;

    let top_k_dedup = deduplicate_ranked_ids(&actual_ranked_ids[..std::cmp::min(k, actual_ranked_ids.len())]);
    let top_k_set: HashSet<_> = top_k_dedup.iter().cloned().collect();

    let graded_map: HashMap<String, f32> = normalized_targets
        .graded_relevance
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect();

    let recall_soft = compute_recall_metric(
        &normalized_targets.soft_positive_ids,
        &top_k_set,
    );
    let recall_strict = compute_recall_metric(
        &normalized_targets.strict_positive_ids,
        &top_k_set,
    );

    let (rr_soft, first_relevant_rank_soft) =
        compute_reciprocal_rank_metric(&normalized_targets.soft_positive_ids, &top_k_dedup);
    let (rr_strict, first_relevant_rank_strict) =
        compute_reciprocal_rank_metric(&normalized_targets.strict_positive_ids, &top_k_dedup);

    let num_relevant_soft =
        compute_relevant_count(&normalized_targets.soft_positive_ids, &top_k_set);
    let num_relevant_strict =
        compute_relevant_count(&normalized_targets.strict_positive_ids, &top_k_set);

    let ndcg = compute_ndcg_metric(
        &top_k_dedup,
        &graded_map,
        &normalized_targets.graded_relevance,
        k,
    )?;

    Ok(RetrievalEvaluationMetrics {
        evaluated_k: k as u32,
        recall_soft,
        recall_strict,
        rr_soft,
        rr_strict,
        ndcg,
        first_relevant_rank_soft,
        first_relevant_rank_strict,
        num_relevant_soft: num_relevant_soft as u32,
        num_relevant_strict: num_relevant_strict as u32,
    })
}

fn validate_and_normalize_golden_targets(
    targets: &GoldenRetrievalTargetsById,
) -> Result<GoldenRetrievalTargetsById, RetrievalMetricsError> {
    if targets.strict_positive_ids.is_empty() || targets.soft_positive_ids.is_empty() {
        return Err(RetrievalMetricsError::InvalidGoldenTargets {
            reason: "both strict and soft positive ids must be non-empty".to_string(),
        });
    }

    let mut strict_set = HashSet::new();
    let mut normalized_strict = Vec::with_capacity(targets.strict_positive_ids.len());
    for id in &targets.strict_positive_ids {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err(RetrievalMetricsError::InvalidGoldenTargets {
                reason: "strict positive ids must not be empty after trimming".to_string(),
            });
        }
        if !strict_set.insert(trimmed.to_string()) {
            return Err(RetrievalMetricsError::InvalidGoldenTargets {
                reason: "duplicate strict positive ids".to_string(),
            });
        }
        normalized_strict.push(trimmed.to_string());
    }

    let mut soft_set = HashSet::new();
    let mut normalized_soft = Vec::with_capacity(targets.soft_positive_ids.len());
    for id in &targets.soft_positive_ids {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err(RetrievalMetricsError::InvalidGoldenTargets {
                reason: "soft positive ids must not be empty after trimming".to_string(),
            });
        }
        if !soft_set.insert(trimmed.to_string()) {
            return Err(RetrievalMetricsError::InvalidGoldenTargets {
                reason: "duplicate soft positive ids".to_string(),
            });
        }
        normalized_soft.push(trimmed.to_string());
    }

    if !strict_set.is_subset(&soft_set) {
        return Err(RetrievalMetricsError::InvalidGoldenTargets {
            reason: "strict positive ids must be a subset of soft positive ids".to_string(),
        });
    }

    let mut graded_ids = HashSet::new();
    let mut normalized_graded = Vec::with_capacity(targets.graded_relevance.len());
    for rel in &targets.graded_relevance {
        let trimmed = rel.id.trim();
        if trimmed.is_empty() {
            return Err(RetrievalMetricsError::InvalidGoldenTargets {
                reason: "graded relevance ids must not be empty after trimming".to_string(),
            });
        }
        if !graded_ids.insert(trimmed.to_string()) {
            return Err(RetrievalMetricsError::InvalidGoldenTargets {
                reason: "duplicate graded relevance ids".to_string(),
            });
        }

        match rel.score {
            0.0 | 0.5 | 1.0 => {}
            _ => {
                return Err(RetrievalMetricsError::InconsistentGradedRelevance {
                    reason: format!(
                        "graded relevance score must be 0.0, 0.5, or 1.0, got {}",
                        rel.score
                    ),
                });
            }
        }

        normalized_graded.push(GoldenRetrievalRelevanceById {
            id: trimmed.to_string(),
            score: rel.score,
        });
    }

    let graded_map: HashMap<String, f32> = normalized_graded
        .iter()
        .map(|rel| (rel.id.clone(), rel.score))
        .collect();

    for id in &normalized_strict {
        match graded_map.get(id) {
            Some(score) if (*score - 1.0).abs() < f32::EPSILON => {}
            _ => {
                return Err(RetrievalMetricsError::InconsistentGradedRelevance {
                    reason: format!(
                        "strict positive id '{}' must have graded relevance score 1.0",
                        id
                    ),
                });
            }
        }
    }

    for id in &normalized_soft {
        if !strict_set.contains(id) {
            match graded_map.get(id) {
                Some(score) if (*score - 0.5).abs() < f32::EPSILON => {}
                _ => {
                    return Err(RetrievalMetricsError::InconsistentGradedRelevance {
                        reason: format!(
                            "soft-only positive id '{}' must have graded relevance score 0.5",
                            id
                        ),
                    });
                }
            }
        }
    }

    for rel in &normalized_graded {
        let is_in_strict = strict_set.contains(&rel.id);
        let is_in_soft = soft_set.contains(&rel.id);
        match rel.score {
            1.0 if !is_in_strict => {
                return Err(RetrievalMetricsError::InconsistentGradedRelevance {
                    reason: format!(
                        "graded relevance id '{}' scored 1.0 must belong to strict positive ids",
                        rel.id
                    ),
                });
            }
            0.5 if !is_in_soft || is_in_strict => {
                return Err(RetrievalMetricsError::InconsistentGradedRelevance {
                    reason: format!(
                        "graded relevance id '{}' scored 0.5 must belong to soft-only positive ids",
                        rel.id
                    ),
                });
            }
            0.0 if is_in_soft => {
                return Err(RetrievalMetricsError::InconsistentGradedRelevance {
                    reason: format!(
                        "graded relevance id '{}' scored 0.0 must not belong to soft positive ids",
                        rel.id
                    ),
                });
            }
            _ => {}
        }
    }

    Ok(GoldenRetrievalTargetsById {
        strict_positive_ids: normalized_strict,
        soft_positive_ids: normalized_soft,
        graded_relevance: normalized_graded,
    })
}

fn deduplicate_ranked_ids(ranked_ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for id in ranked_ids {
        if seen.insert(id.clone()) {
            deduped.push(id.clone());
        }
    }
    deduped
}

fn compute_recall_metric(
    relevant_ids: &[String],
    top_k_set: &HashSet<String>,
) -> f32 {
    if relevant_ids.is_empty() {
        return 0.0;
    }
    let found_count = relevant_ids.iter().filter(|id| top_k_set.contains(*id)).count();
    found_count as f32 / relevant_ids.len() as f32
}

fn compute_reciprocal_rank_metric(
    relevant_ids: &[String],
    top_k_dedup: &[String],
) -> (f32, Option<u32>) {
    let relevant_set: HashSet<_> = relevant_ids.iter().cloned().collect();
    for (rank_zero, id) in top_k_dedup.iter().enumerate() {
        if relevant_set.contains(id) {
            let rank_one = (rank_zero + 1) as u32;
            return (1.0 / rank_one as f32, Some(rank_one));
        }
    }
    (0.0, None)
}

fn compute_relevant_count(relevant_ids: &[String], top_k_set: &HashSet<String>) -> usize {
    relevant_ids.iter().filter(|id| top_k_set.contains(*id)).count()
}

fn compute_ndcg_metric(
    top_k_dedup: &[String],
    graded_map: &HashMap<String, f32>,
    graded_relevance: &[GoldenRetrievalRelevanceById],
    k: usize,
) -> Result<f32, RetrievalMetricsError> {
    let dcg = compute_dcg(top_k_dedup, graded_map);

    let idcg = compute_idcg(graded_relevance, k)?;

    if idcg > 0.0 {
        Ok(dcg / idcg)
    } else {
        Ok(0.0)
    }
}

fn compute_dcg(top_k_dedup: &[String], graded_map: &HashMap<String, f32>) -> f32 {
    let mut dcg = 0.0;
    for (rank_zero, id) in top_k_dedup.iter().enumerate() {
        let rank_one = rank_zero + 1;
        let grade = graded_map.get(id).copied().unwrap_or(0.0);
        let discount = (rank_one as f32 + 1.0).log2();
        dcg += grade / discount;
    }
    dcg
}

fn compute_idcg(
    graded_relevance: &[GoldenRetrievalRelevanceById],
    k: usize,
) -> Result<f32, RetrievalMetricsError> {
    let mut ideal_grades: Vec<f32> = graded_relevance
        .iter()
        .filter(|r| r.score > 0.0)
        .map(|r| r.score)
        .collect();

    ideal_grades.sort_by(|a, b| {
        let cmp = b.partial_cmp(a).ok_or(()).unwrap_or(std::cmp::Ordering::Equal);
        if cmp == std::cmp::Ordering::Equal {
            std::cmp::Ordering::Equal
        } else {
            cmp
        }
    });

    let ideal_top_k: Vec<f32> = ideal_grades.into_iter().take(k).collect();

    let mut idcg = 0.0;
    for (rank_zero, grade) in ideal_top_k.iter().enumerate() {
        let rank_one = rank_zero + 1;
        let discount = (rank_one as f32 + 1.0).log2();
        idcg += grade / discount;
    }

    Ok(idcg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_empty_strict_ids() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec![],
            soft_positive_ids: vec!["soft1".to_string()],
            graded_relevance: vec![GoldenRetrievalRelevanceById {
                id: "soft1".to_string(),
                score: 0.5,
            }],
        };
        let result = validate_and_normalize_golden_targets(&targets);
        assert!(matches!(
            result,
            Err(RetrievalMetricsError::InvalidGoldenTargets { .. })
        ));
    }

    #[test]
    fn test_validate_empty_soft_ids() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["strict1".to_string()],
            soft_positive_ids: vec![],
            graded_relevance: vec![GoldenRetrievalRelevanceById {
                id: "strict1".to_string(),
                score: 1.0,
            }],
        };
        let result = validate_and_normalize_golden_targets(&targets);
        assert!(matches!(
            result,
            Err(RetrievalMetricsError::InvalidGoldenTargets { .. })
        ));
    }

    #[test]
    fn test_validate_duplicate_strict_ids() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["strict1".to_string(), "strict1".to_string()],
            soft_positive_ids: vec!["strict1".to_string(), "soft1".to_string()],
            graded_relevance: vec![
                GoldenRetrievalRelevanceById {
                    id: "strict1".to_string(),
                    score: 1.0,
                },
                GoldenRetrievalRelevanceById {
                    id: "soft1".to_string(),
                    score: 0.5,
                },
            ],
        };
        let result = validate_and_normalize_golden_targets(&targets);
        assert!(matches!(
            result,
            Err(RetrievalMetricsError::InvalidGoldenTargets { .. })
        ));
    }

    #[test]
    fn test_validate_strict_not_subset_of_soft() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["strict1".to_string()],
            soft_positive_ids: vec!["soft1".to_string()],
            graded_relevance: vec![
                GoldenRetrievalRelevanceById {
                    id: "strict1".to_string(),
                    score: 1.0,
                },
                GoldenRetrievalRelevanceById {
                    id: "soft1".to_string(),
                    score: 0.5,
                },
            ],
        };
        let result = validate_and_normalize_golden_targets(&targets);
        assert!(matches!(
            result,
            Err(RetrievalMetricsError::InvalidGoldenTargets { .. })
        ));
    }

    #[test]
    fn test_validate_invalid_grade_value() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["strict1".to_string()],
            soft_positive_ids: vec!["strict1".to_string(), "soft1".to_string()],
            graded_relevance: vec![
                GoldenRetrievalRelevanceById {
                    id: "strict1".to_string(),
                    score: 1.0,
                },
                GoldenRetrievalRelevanceById {
                    id: "soft1".to_string(),
                    score: 0.7,
                },
            ],
        };
        let result = validate_and_normalize_golden_targets(&targets);
        assert!(matches!(
            result,
            Err(RetrievalMetricsError::InconsistentGradedRelevance { .. })
        ));
    }

    #[test]
    fn test_validate_strict_missing_grade_1_0() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["strict1".to_string()],
            soft_positive_ids: vec!["strict1".to_string()],
            graded_relevance: vec![GoldenRetrievalRelevanceById {
                id: "strict1".to_string(),
                score: 0.5,
            }],
        };
        let result = validate_and_normalize_golden_targets(&targets);
        assert!(matches!(
            result,
            Err(RetrievalMetricsError::InconsistentGradedRelevance { .. })
        ));
    }

    #[test]
    fn test_validate_soft_only_missing_grade_0_5() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["strict1".to_string()],
            soft_positive_ids: vec!["strict1".to_string(), "soft1".to_string()],
            graded_relevance: vec![
                GoldenRetrievalRelevanceById {
                    id: "strict1".to_string(),
                    score: 1.0,
                },
                GoldenRetrievalRelevanceById {
                    id: "soft1".to_string(),
                    score: 1.0,
                },
            ],
        };
        let result = validate_and_normalize_golden_targets(&targets);
        assert!(matches!(
            result,
            Err(RetrievalMetricsError::InconsistentGradedRelevance { .. })
        ));
    }

    #[test]
    fn test_validate_whitespace_trimming() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["  strict1  ".to_string()],
            soft_positive_ids: vec!["  strict1  ".to_string()],
            graded_relevance: vec![GoldenRetrievalRelevanceById {
                id: "strict1".to_string(),
                score: 1.0,
            }],
        };
        let result = validate_and_normalize_golden_targets(&targets);
        let normalized = result.expect("normalization must succeed");
        assert_eq!(normalized.strict_positive_ids, vec!["strict1"]);
        assert_eq!(normalized.soft_positive_ids, vec!["strict1"]);
    }

    #[test]
    fn test_validate_reverse_implication_score_1_requires_strict() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["strict1".to_string()],
            soft_positive_ids: vec!["strict1".to_string(), "soft1".to_string()],
            graded_relevance: vec![
                GoldenRetrievalRelevanceById {
                    id: "strict1".to_string(),
                    score: 1.0,
                },
                GoldenRetrievalRelevanceById {
                    id: "soft1".to_string(),
                    score: 1.0,
                },
            ],
        };
        let result = validate_and_normalize_golden_targets(&targets);
        assert!(matches!(
            result,
            Err(RetrievalMetricsError::InconsistentGradedRelevance { .. })
        ));
    }

    #[test]
    fn test_validate_reverse_implication_score_05_requires_soft_only() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["strict1".to_string()],
            soft_positive_ids: vec!["strict1".to_string()],
            graded_relevance: vec![
                GoldenRetrievalRelevanceById {
                    id: "strict1".to_string(),
                    score: 1.0,
                },
                GoldenRetrievalRelevanceById {
                    id: "other".to_string(),
                    score: 0.5,
                },
            ],
        };
        let result = validate_and_normalize_golden_targets(&targets);
        assert!(matches!(
            result,
            Err(RetrievalMetricsError::InconsistentGradedRelevance { .. })
        ));
    }

    #[test]
    fn test_validate_reverse_implication_score_0_forbidden_in_soft() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["strict1".to_string()],
            soft_positive_ids: vec!["strict1".to_string(), "soft1".to_string()],
            graded_relevance: vec![
                GoldenRetrievalRelevanceById {
                    id: "strict1".to_string(),
                    score: 1.0,
                },
                GoldenRetrievalRelevanceById {
                    id: "soft1".to_string(),
                    score: 0.0,
                },
            ],
        };
        let result = validate_and_normalize_golden_targets(&targets);
        assert!(matches!(
            result,
            Err(RetrievalMetricsError::InconsistentGradedRelevance { .. })
        ));
    }

    #[test]
    fn test_compute_metrics_uses_normalized_trimmed_ids() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["  id1  ".to_string()],
            soft_positive_ids: vec!["  id1  ".to_string()],
            graded_relevance: vec![GoldenRetrievalRelevanceById {
                id: "id1".to_string(),
                score: 1.0,
            }],
        };
        let actual_ranked = vec!["id1".to_string()];
        let result = compute_retrieval_metrics(&targets, &actual_ranked, 1).unwrap();
        assert!((result.recall_soft - 1.0).abs() < f32::EPSILON);
        assert_eq!(result.first_relevant_rank_soft, Some(1));
    }

    #[test]
    fn test_deduplicate_no_duplicates() {
        let ranked = vec!["id1".to_string(), "id2".to_string(), "id3".to_string()];
        let deduped = deduplicate_ranked_ids(&ranked);
        assert_eq!(deduped, ranked);
    }

    #[test]
    fn test_deduplicate_with_duplicates() {
        let ranked = vec!["id1".to_string(), "id2".to_string(), "id1".to_string(), "id3".to_string()];
        let deduped = deduplicate_ranked_ids(&ranked);
        assert_eq!(deduped, vec!["id1", "id2", "id3"]);
    }

    #[test]
    fn test_deduplicate_all_duplicates() {
        let ranked = vec!["id1".to_string(), "id1".to_string(), "id1".to_string()];
        let deduped = deduplicate_ranked_ids(&ranked);
        assert_eq!(deduped, vec!["id1"]);
    }

    #[test]
    fn test_recall_soft_all_found() {
        let relevant = vec!["id1".to_string(), "id2".to_string()];
        let top_k: HashSet<_> = vec!["id1".to_string(), "id2".to_string()].into_iter().collect();
        let recall = compute_recall_metric(&relevant, &top_k);
        assert!((recall - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_recall_soft_partial_found() {
        let relevant = vec!["id1".to_string(), "id2".to_string()];
        let top_k: HashSet<_> = vec!["id1".to_string()].into_iter().collect();
        let recall = compute_recall_metric(&relevant, &top_k);
        assert!((recall - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_recall_soft_none_found() {
        let relevant = vec!["id1".to_string(), "id2".to_string()];
        let top_k: HashSet<_> = vec!["id3".to_string()].into_iter().collect();
        let recall = compute_recall_metric(&relevant, &top_k);
        assert!(recall < f32::EPSILON);
    }

    #[test]
    fn test_recall_empty_relevant() {
        let relevant = vec![];
        let top_k: HashSet<_> = vec!["id1".to_string()].into_iter().collect();
        let recall = compute_recall_metric(&relevant, &top_k);
        assert!(recall < f32::EPSILON);
    }

    #[test]
    fn test_reciprocal_rank_first_position() {
        let relevant = vec!["id1".to_string()];
        let top_k = vec!["id1".to_string(), "id2".to_string()];
        let (rr, rank) = compute_reciprocal_rank_metric(&relevant, &top_k);
        assert!((rr - 1.0).abs() < f32::EPSILON);
        assert_eq!(rank, Some(1));
    }

    #[test]
    fn test_reciprocal_rank_second_position() {
        let relevant = vec!["id2".to_string()];
        let top_k = vec!["id1".to_string(), "id2".to_string()];
        let (rr, rank) = compute_reciprocal_rank_metric(&relevant, &top_k);
        assert!((rr - 0.5).abs() < f32::EPSILON);
        assert_eq!(rank, Some(2));
    }

    #[test]
    fn test_reciprocal_rank_not_found() {
        let relevant = vec!["id3".to_string()];
        let top_k = vec!["id1".to_string(), "id2".to_string()];
        let (rr, rank) = compute_reciprocal_rank_metric(&relevant, &top_k);
        assert!(rr < f32::EPSILON);
        assert_eq!(rank, None);
    }

    #[test]
    fn test_relevant_count() {
        let relevant = vec!["id1".to_string(), "id2".to_string(), "id3".to_string()];
        let top_k: HashSet<_> = vec!["id1".to_string(), "id3".to_string()].into_iter().collect();
        let count = compute_relevant_count(&relevant, &top_k);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_compute_dcg_perfect() {
        let top_k = vec!["id1".to_string(), "id2".to_string()];
        let mut graded_map = HashMap::new();
        graded_map.insert("id1".to_string(), 1.0);
        graded_map.insert("id2".to_string(), 1.0);
        let dcg = compute_dcg(&top_k, &graded_map);
        let expected = 1.0 / 2.0_f32.log2() + 1.0 / 3.0_f32.log2();
        assert!((dcg - expected).abs() < 1e-5);
    }

    #[test]
    fn test_compute_dcg_partial() {
        let top_k = vec!["id1".to_string(), "id2".to_string()];
        let mut graded_map = HashMap::new();
        graded_map.insert("id1".to_string(), 1.0);
        graded_map.insert("id2".to_string(), 0.5);
        let dcg = compute_dcg(&top_k, &graded_map);
        let expected = 1.0 / 2.0_f32.log2() + 0.5 / 3.0_f32.log2();
        assert!((dcg - expected).abs() < 1e-5);
    }

    #[test]
    fn test_compute_dcg_with_missing_ids() {
        let top_k = vec!["id1".to_string(), "id3".to_string()];
        let mut graded_map = HashMap::new();
        graded_map.insert("id1".to_string(), 1.0);
        let dcg = compute_dcg(&top_k, &graded_map);
        let expected = 1.0 / 2.0_f32.log2() + 0.0 / 3.0_f32.log2();
        assert!((dcg - expected).abs() < 1e-5);
    }

    #[test]
    fn test_compute_idcg_perfect() {
        let graded_relevance = vec![
            GoldenRetrievalRelevanceById {
                id: "id1".to_string(),
                score: 1.0,
            },
            GoldenRetrievalRelevanceById {
                id: "id2".to_string(),
                score: 1.0,
            },
        ];
        let idcg = compute_idcg(&graded_relevance, 2).unwrap();
        let expected = 1.0 / 2.0_f32.log2() + 1.0 / 3.0_f32.log2();
        assert!((idcg - expected).abs() < 1e-5);
    }

    #[test]
    fn test_compute_idcg_with_zero_grades() {
        let graded_relevance = vec![
            GoldenRetrievalRelevanceById {
                id: "id1".to_string(),
                score: 1.0,
            },
            GoldenRetrievalRelevanceById {
                id: "id2".to_string(),
                score: 0.0,
            },
        ];
        let idcg = compute_idcg(&graded_relevance, 2).unwrap();
        let expected = 1.0 / 2.0_f32.log2();
        assert!((idcg - expected).abs() < 1e-5);
    }

    #[test]
    fn test_compute_idcg_k_cutoff() {
        let graded_relevance = vec![
            GoldenRetrievalRelevanceById {
                id: "id1".to_string(),
                score: 1.0,
            },
            GoldenRetrievalRelevanceById {
                id: "id2".to_string(),
                score: 1.0,
            },
            GoldenRetrievalRelevanceById {
                id: "id3".to_string(),
                score: 1.0,
            },
        ];
        let idcg = compute_idcg(&graded_relevance, 2).unwrap();
        let expected = 1.0 / 2.0_f32.log2() + 1.0 / 3.0_f32.log2();
        assert!((idcg - expected).abs() < 1e-5);
    }

    #[test]
    fn test_compute_metrics_invalid_k_zero() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["id1".to_string()],
            soft_positive_ids: vec!["id1".to_string()],
            graded_relevance: vec![GoldenRetrievalRelevanceById {
                id: "id1".to_string(),
                score: 1.0,
            }],
        };
        let result = compute_retrieval_metrics(&targets, &vec!["id1".to_string()], 0);
        assert!(matches!(
            result,
            Err(RetrievalMetricsError::InvalidK { .. })
        ));
    }

    #[test]
    fn test_compute_metrics_perfect_ranking() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["id1".to_string()],
            soft_positive_ids: vec!["id1".to_string()],
            graded_relevance: vec![GoldenRetrievalRelevanceById {
                id: "id1".to_string(),
                score: 1.0,
            }],
        };
        let actual_ranked = vec!["id1".to_string()];
        let result = compute_retrieval_metrics(&targets, &actual_ranked, 5).unwrap();

        assert_eq!(result.evaluated_k, 5);
        assert!((result.recall_soft - 1.0).abs() < f32::EPSILON);
        assert!((result.recall_strict - 1.0).abs() < f32::EPSILON);
        assert!((result.rr_soft - 1.0).abs() < f32::EPSILON);
        assert!((result.rr_strict - 1.0).abs() < f32::EPSILON);
        assert!(result.first_relevant_rank_soft.is_some());
        assert!(result.first_relevant_rank_strict.is_some());
        assert_eq!(result.num_relevant_soft, 1);
        assert_eq!(result.num_relevant_strict, 1);
        assert!((result.ndcg - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compute_metrics_no_relevant_found() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["id1".to_string()],
            soft_positive_ids: vec!["id1".to_string()],
            graded_relevance: vec![GoldenRetrievalRelevanceById {
                id: "id1".to_string(),
                score: 1.0,
            }],
        };
        let actual_ranked = vec!["id2".to_string(), "id3".to_string()];
        let result = compute_retrieval_metrics(&targets, &actual_ranked, 5).unwrap();

        assert!((result.recall_soft - 0.0).abs() < f32::EPSILON);
        assert!((result.recall_strict - 0.0).abs() < f32::EPSILON);
        assert!((result.rr_soft - 0.0).abs() < f32::EPSILON);
        assert!((result.rr_strict - 0.0).abs() < f32::EPSILON);
        assert_eq!(result.first_relevant_rank_soft, None);
        assert_eq!(result.first_relevant_rank_strict, None);
        assert_eq!(result.num_relevant_soft, 0);
        assert_eq!(result.num_relevant_strict, 0);
        assert!((result.ndcg - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compute_metrics_with_duplicates_in_ranked() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["id1".to_string()],
            soft_positive_ids: vec!["id1".to_string()],
            graded_relevance: vec![GoldenRetrievalRelevanceById {
                id: "id1".to_string(),
                score: 1.0,
            }],
        };
        let actual_ranked = vec!["id1".to_string(), "id1".to_string(), "id2".to_string()];
        let result = compute_retrieval_metrics(&targets, &actual_ranked, 5).unwrap();

        assert!((result.recall_soft - 1.0).abs() < f32::EPSILON);
        assert!(result.first_relevant_rank_soft == Some(1));
        assert_eq!(result.num_relevant_soft, 1);
    }

    #[test]
    fn test_compute_metrics_soft_vs_strict() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["id1".to_string()],
            soft_positive_ids: vec!["id1".to_string(), "id2".to_string()],
            graded_relevance: vec![
                GoldenRetrievalRelevanceById {
                    id: "id1".to_string(),
                    score: 1.0,
                },
                GoldenRetrievalRelevanceById {
                    id: "id2".to_string(),
                    score: 0.5,
                },
            ],
        };
        let actual_ranked = vec!["id2".to_string(), "id1".to_string()];
        let result = compute_retrieval_metrics(&targets, &actual_ranked, 5).unwrap();

        assert!((result.recall_soft - 1.0).abs() < f32::EPSILON);
        assert!((result.recall_strict - 1.0).abs() < f32::EPSILON);
        assert!(result.first_relevant_rank_soft == Some(1));
        assert!(result.first_relevant_rank_strict == Some(2));
        assert_eq!(result.num_relevant_soft, 2);
        assert_eq!(result.num_relevant_strict, 1);
    }

    #[test]
    fn test_compute_metrics_k_limit() {
        let targets = GoldenRetrievalTargetsById {
            strict_positive_ids: vec!["id1".to_string(), "id2".to_string()],
            soft_positive_ids: vec!["id1".to_string(), "id2".to_string()],
            graded_relevance: vec![
                GoldenRetrievalRelevanceById {
                    id: "id1".to_string(),
                    score: 1.0,
                },
                GoldenRetrievalRelevanceById {
                    id: "id2".to_string(),
                    score: 1.0,
                },
            ],
        };
        let actual_ranked = vec!["id1".to_string(), "id2".to_string(), "id3".to_string()];
        let result = compute_retrieval_metrics(&targets, &actual_ranked, 1).unwrap();

        assert!((result.recall_soft - 0.5).abs() < f32::EPSILON);
        assert_eq!(result.num_relevant_soft, 1);
    }
}
