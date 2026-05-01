use std::collections::{HashMap, HashSet};

use thiserror::Error;

use crate::shared_types::{
    GoldenQueryStructuringTargets, GoldenVocabularyFieldTargets, QueryStructuringAggregateMetrics,
    QueryStructuringControlledVocabulary, QueryStructuringMetrics,
    QueryStructuringNonVocabularyFieldMetrics, QueryStructuringTopLevelMetrics,
    QueryStructuringVocabularyFieldMetricSet, QueryStructuringVocabularyFieldMetrics,
    StructuredUserQuery, StructuredUserQuerySupportLevel, StructuredUserQueryTerm,
};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub(crate) enum QueryStructuringMetricsError {
    #[error("invalid golden targets for field '{field}': {reason}")]
    InvalidGoldenTargets { field: String, reason: String },

    #[error("inconsistent vocabulary mapping for field '{field}': {reason}")]
    InconsistentVocabularyMapping { field: String, reason: String },

    #[error("inconsistent graded relevance for field '{field}': {reason}")]
    InconsistentGradedRelevance { field: String, reason: String },

    #[error("invalid raw user query: {reason}")]
    InvalidRawUserQuery { reason: String },

    #[error("unexpected computation state for field '{field}': {reason}")]
    UnexpectedComputationState { field: String, reason: String },
}

// ---------------------------------------------------------------------------
// Public entrypoint
// ---------------------------------------------------------------------------

pub(crate) fn compute_query_structuring_metrics(
    structured_query: &StructuredUserQuery,
    golden_targets: &GoldenQueryStructuringTargets,
    controlled_vocabulary: &QueryStructuringControlledVocabulary,
    raw_user_query: &str,
) -> Result<QueryStructuringMetrics, QueryStructuringMetricsError> {
    if raw_user_query.trim().is_empty() {
        return Err(QueryStructuringMetricsError::InvalidRawUserQuery {
            reason: "raw_user_query must not be empty or all whitespace".to_string(),
        });
    }

    let normalized_query = normalize_for_grounding(raw_user_query);

    let symptoms = compute_field_metrics(
        "symptoms",
        &structured_query.symptoms,
        &golden_targets.symptoms,
        &controlled_vocabulary.canonical_symptoms,
        &normalized_query,
    )?;

    let affected_subsystems = compute_field_metrics(
        "affected_subsystems",
        &structured_query.affected_subsystems,
        &golden_targets.affected_subsystems,
        &controlled_vocabulary.affected_components,
        &normalized_query,
    )?;

    let failure_modes = compute_field_metrics(
        "failure_modes",
        &structured_query.failure_modes,
        &golden_targets.failure_modes,
        &controlled_vocabulary.failure_mode_candidates,
        &normalized_query,
    )?;

    let system_properties = compute_field_metrics(
        "system_properties",
        &structured_query.system_properties,
        &golden_targets.system_properties,
        &controlled_vocabulary.violated_properties,
        &normalized_query,
    )?;

    let non_vocab_fields = compute_non_vocab_metrics(structured_query);

    let aggregates = compute_aggregates(&[
        &symptoms,
        &affected_subsystems,
        &failure_modes,
        &system_properties,
    ]);

    let top_level = QueryStructuringTopLevelMetrics {
        macro_precision_soft: aggregates.macro_precision_soft,
        macro_recall_strict: aggregates.macro_recall_strict,
        macro_recall_soft: aggregates.macro_recall_soft,
        overall_grounded_strict_recall: aggregates.overall_grounded_strict_recall,
        all_fields_core_success_rate: aggregates.all_fields_core_success_rate,
    };

    Ok(QueryStructuringMetrics {
        top_level,
        vocab_fields: QueryStructuringVocabularyFieldMetrics {
            symptoms: symptoms.metric_set,
            affected_subsystems: affected_subsystems.metric_set,
            failure_modes: failure_modes.metric_set,
            system_properties: system_properties.metric_set,
        },
        non_vocab_fields,
        aggregates,
    })
}

// ---------------------------------------------------------------------------
// Internal result carrier
// ---------------------------------------------------------------------------

struct FieldMetricsResult {
    metric_set: QueryStructuringVocabularyFieldMetricSet,
    grounded_strict_hits: u32,
    strict_gold_size: u32,
}

// ---------------------------------------------------------------------------
// Per-field computation (sections 5-10)
// ---------------------------------------------------------------------------

fn compute_field_metrics(
    field_name: &str,
    field_terms: &[StructuredUserQueryTerm],
    golden_field: &GoldenVocabularyFieldTargets,
    vocabulary: &[String],
    normalized_query: &str,
) -> Result<FieldMetricsResult, QueryStructuringMetricsError> {
    let (strict_set, soft_set, grade_map) =
        validate_field_targets(field_name, golden_field, vocabulary)?;

    let vocab_set: HashSet<&str> = vocabulary.iter().map(String::as_str).collect();

    let (predicted_dedup, duplicate_term_count) = build_predicted_dedup(field_terms);

    // PredictedSet — unique terms as a set for O(1) lookups
    let predicted_set: HashSet<&str> =
        predicted_dedup.iter().map(|t| t.term.as_str()).collect();

    // -----------------------------------------------------------------------
    // Section 5 — contract/vocabulary counts
    // -----------------------------------------------------------------------

    let invalid_vocab_count = predicted_set
        .iter()
        .filter(|&&t| !vocab_set.contains(t))
        .count() as u32;

    // -----------------------------------------------------------------------
    // Section 6 — set-based term selection metrics
    // -----------------------------------------------------------------------

    let num_predicted_terms = predicted_set.len() as u32;

    let predicted_in_soft = predicted_set
        .iter()
        .filter(|&&t| soft_set.contains(t))
        .count();
    let predicted_in_strict = predicted_set
        .iter()
        .filter(|&&t| strict_set.contains(t))
        .count();

    let num_false_positive = predicted_set
        .iter()
        .filter(|&&t| !soft_set.contains(t))
        .count() as u32;
    let num_false_negative_strict = strict_set
        .iter()
        .filter(|t| !predicted_set.contains(t.as_str()))
        .count() as u32;

    let precision_soft = if predicted_set.is_empty() {
        if soft_set.is_empty() { 1.0 } else { 0.0 }
    } else if soft_set.is_empty() {
        0.0
    } else {
        predicted_in_soft as f32 / predicted_set.len() as f32
    };

    let recall_strict = if strict_set.is_empty() {
        1.0
    } else {
        predicted_in_strict as f32 / strict_set.len() as f32
    };

    let recall_soft = if soft_set.is_empty() {
        1.0
    } else {
        predicted_in_soft as f32 / soft_set.len() as f32
    };

    // -----------------------------------------------------------------------
    // Section 7 — graded relevance metrics
    // -----------------------------------------------------------------------

    let grade = |term: &str| -> f32 { grade_map.get(term).copied().unwrap_or(0.0) };

    let positive_grade_sum: f32 = grade_map.values().filter(|&&s| s > 0.0).sum();

    let selected_positive_grade_sum: f32 = predicted_set
        .iter()
        .map(|&t| grade(t))
        .filter(|&s| s > 0.0)
        .sum();

    let selected_grade_sum: f32 = predicted_set.iter().map(|&t| grade(t)).sum();

    let graded_coverage = if positive_grade_sum == 0.0 {
        if predicted_set.is_empty() { 1.0 } else { 0.0 }
    } else {
        selected_positive_grade_sum / positive_grade_sum
    };

    let average_selected_score = if predicted_set.is_empty() {
        0.0
    } else {
        selected_grade_sum / predicted_set.len() as f32
    };

    let zero_score_selection_count = predicted_set
        .iter()
        .filter(|&&t| grade(t) == 0.0)
        .count() as u32;

    // -----------------------------------------------------------------------
    // Section 8 — evidence-span and grounding metrics
    // Evidence/support is evaluated on PredictedDedup (first occurrence per term)
    // -----------------------------------------------------------------------

    let mut grounded_strict_hits: u32 = 0;
    let mut missing_evidence_span_count: u32 = 0;
    let mut invalid_evidence_span_count: u32 = 0;
    let mut near_substring_count: u32 = 0;
    let mut unsupported_count: u32 = 0;

    for term in &predicted_dedup {
        let normalized_span = normalize_for_grounding(&term.evidence_span);
        let span_empty = normalized_span.is_empty();
        let span_near_sub = !span_empty && normalized_query.contains(normalized_span.as_str());

        if span_empty {
            missing_evidence_span_count += 1;
        } else if !span_near_sub {
            invalid_evidence_span_count += 1;
        }

        if span_near_sub {
            near_substring_count += 1;
        }

        let is_weak = term.support_level == StructuredUserQuerySupportLevel::WeakInference;

        if span_empty || !span_near_sub || is_weak {
            unsupported_count += 1;
        }

        // Valid strict grounding: strict gold + non-empty near-substring span + not WeakInference
        if strict_set.contains(term.term.as_str()) && span_near_sub && !is_weak {
            grounded_strict_hits += 1;
        }
    }

    let n_dedup = predicted_dedup.len() as f32;

    let grounded_strict_recall = if strict_set.is_empty() {
        1.0
    } else {
        grounded_strict_hits as f32 / strict_set.len() as f32
    };

    let unsupported_selected_term_rate = if predicted_set.is_empty() {
        0.0
    } else {
        unsupported_count as f32 / n_dedup
    };

    let evidence_span_near_substring_rate = if predicted_set.is_empty() {
        0.0
    } else {
        near_substring_count as f32 / n_dedup
    };

    // -----------------------------------------------------------------------
    // Section 9 — support-level metrics
    // -----------------------------------------------------------------------

    let weak_selected_terms = predicted_dedup
        .iter()
        .filter(|t| t.support_level == StructuredUserQuerySupportLevel::WeakInference)
        .count() as u32;

    let strict_selected_terms = predicted_dedup
        .iter()
        .filter(|t| strict_set.contains(t.term.as_str()))
        .count() as u32;

    let weak_strict_selected_terms = predicted_dedup
        .iter()
        .filter(|t| {
            strict_set.contains(t.term.as_str())
                && t.support_level == StructuredUserQuerySupportLevel::WeakInference
        })
        .count() as u32;

    let false_positive_term_count = predicted_dedup
        .iter()
        .filter(|t| !soft_set.contains(t.term.as_str()))
        .count() as u32;

    let weak_false_positive_terms = predicted_dedup
        .iter()
        .filter(|t| {
            !soft_set.contains(t.term.as_str())
                && t.support_level == StructuredUserQuerySupportLevel::WeakInference
        })
        .count() as u32;

    let weak_inference_rate = if predicted_set.is_empty() {
        0.0
    } else {
        weak_selected_terms as f32 / n_dedup
    };

    let strict_terms_weak_inference_rate = if strict_selected_terms == 0 {
        0.0
    } else {
        weak_strict_selected_terms as f32 / strict_selected_terms as f32
    };

    let weak_false_positive_rate = if false_positive_term_count == 0 {
        0.0
    } else {
        weak_false_positive_terms as f32 / false_positive_term_count as f32
    };

    // -----------------------------------------------------------------------
    // Section 10 — field-level success metrics
    // Integer conditions avoid floating-point equality pitfalls
    // -----------------------------------------------------------------------

    let field_core_success =
        (predicted_in_strict as u32 == strict_set.len() as u32) && invalid_vocab_count == 0;

    let field_grounded_success =
        (strict_set.is_empty() || grounded_strict_hits == strict_set.len() as u32)
            && unsupported_count == 0;

    let empty_when_gold_exists = predicted_set.is_empty() && !strict_set.is_empty();

    Ok(FieldMetricsResult {
        metric_set: QueryStructuringVocabularyFieldMetricSet {
            invalid_vocab_count,
            duplicate_term_count,
            precision_soft,
            recall_strict,
            recall_soft,
            num_false_positive,
            num_false_negative_strict,
            num_predicted_terms,
            graded_coverage,
            average_selected_score,
            zero_score_selection_count,
            grounded_strict_recall,
            unsupported_selected_term_rate,
            missing_evidence_span_count,
            invalid_evidence_span_count,
            evidence_span_near_substring_rate,
            weak_inference_rate,
            strict_terms_weak_inference_rate,
            weak_false_positive_rate,
            field_core_success,
            field_grounded_success,
            empty_when_gold_exists,
        },
        grounded_strict_hits,
        strict_gold_size: strict_set.len() as u32,
    })
}

// ---------------------------------------------------------------------------
// Section 11 — non-vocabulary field metrics
// ---------------------------------------------------------------------------

fn compute_non_vocab_metrics(q: &StructuredUserQuery) -> QueryStructuringNonVocabularyFieldMetrics {
    QueryStructuringNonVocabularyFieldMetrics {
        entities_count: q.entities.len() as u32,
        constraints_count: q.constraints.len() as u32,
        triggers_count: q.triggers.len() as u32,
        observability_signals_count: q.observability_signals.len() as u32,
        unresolved_terms_count: q.unresolved_terms.len() as u32,
        intent_present: !q.intent.trim().is_empty(),
        scenario_present: !q.scenario.trim().is_empty(),
    }
}

// ---------------------------------------------------------------------------
// Section 12 — cross-field aggregates
// ---------------------------------------------------------------------------

fn compute_aggregates(fields: &[&FieldMetricsResult; 4]) -> QueryStructuringAggregateMetrics {
    let macro_precision_soft =
        fields.iter().map(|f| f.metric_set.precision_soft).sum::<f32>() / 4.0;
    let macro_recall_strict =
        fields.iter().map(|f| f.metric_set.recall_strict).sum::<f32>() / 4.0;
    let macro_recall_soft =
        fields.iter().map(|f| f.metric_set.recall_soft).sum::<f32>() / 4.0;

    let total_grounded_hits: u32 = fields.iter().map(|f| f.grounded_strict_hits).sum();
    let total_strict_gold: u32 = fields.iter().map(|f| f.strict_gold_size).sum();

    let overall_grounded_strict_recall = if total_strict_gold == 0 {
        1.0
    } else {
        total_grounded_hits as f32 / total_strict_gold as f32
    };

    let passed_core = fields.iter().filter(|f| f.metric_set.field_core_success).count() as f32;
    let all_fields_core_success_rate = passed_core / 4.0;

    QueryStructuringAggregateMetrics {
        macro_precision_soft,
        macro_recall_strict,
        macro_recall_soft,
        overall_grounded_strict_recall,
        all_fields_core_success_rate,
    }
}

// ---------------------------------------------------------------------------
// Golden-target validation (Section 3)
// Returns (strict_set, soft_set, grade_map) all keyed by trimmed term strings
// ---------------------------------------------------------------------------

fn validate_field_targets(
    field_name: &str,
    golden: &GoldenVocabularyFieldTargets,
    vocabulary: &[String],
) -> Result<
    (HashSet<String>, HashSet<String>, HashMap<String, f32>),
    QueryStructuringMetricsError,
> {
    let vocab_set: HashSet<&str> = vocabulary.iter().map(String::as_str).collect();

    let bad_targets = |reason: String| QueryStructuringMetricsError::InvalidGoldenTargets {
        field: field_name.to_string(),
        reason,
    };
    let bad_vocab = |reason: String| QueryStructuringMetricsError::InconsistentVocabularyMapping {
        field: field_name.to_string(),
        reason,
    };
    let bad_graded = |reason: String| QueryStructuringMetricsError::InconsistentGradedRelevance {
        field: field_name.to_string(),
        reason,
    };

    // Build strict_set
    let mut strict_set: HashSet<String> = HashSet::new();
    for raw in &golden.strict_vocabulary_terms {
        let t = raw.trim();
        if t.is_empty() {
            return Err(bad_targets(
                "strict_vocabulary_terms contains empty string after trimming".to_string(),
            ));
        }
        if !vocab_set.contains(t) {
            return Err(bad_vocab(format!(
                "strict term '{t}' is not in the controlled vocabulary"
            )));
        }
        if !strict_set.insert(t.to_string()) {
            return Err(bad_targets(format!(
                "duplicate term '{t}' in strict_vocabulary_terms"
            )));
        }
    }

    // Build soft_set
    let mut soft_set: HashSet<String> = HashSet::new();
    for raw in &golden.soft_vocabulary_terms {
        let t = raw.trim();
        if t.is_empty() {
            return Err(bad_targets(
                "soft_vocabulary_terms contains empty string after trimming".to_string(),
            ));
        }
        if !vocab_set.contains(t) {
            return Err(bad_vocab(format!(
                "soft term '{t}' is not in the controlled vocabulary"
            )));
        }
        if !soft_set.insert(t.to_string()) {
            return Err(bad_targets(format!(
                "duplicate term '{t}' in soft_vocabulary_terms"
            )));
        }
    }

    // StrictGold ⊆ SoftGold
    for t in &strict_set {
        if !soft_set.contains(t.as_str()) {
            return Err(bad_targets(format!(
                "strict term '{t}' is absent from soft_vocabulary_terms \
                 (StrictGold must be a subset of SoftGold)"
            )));
        }
    }

    // Build grade_map
    let mut grade_map: HashMap<String, f32> = HashMap::new();
    for entry in &golden.graded_relevance {
        let t = entry.term.trim();
        if t.is_empty() {
            return Err(bad_targets(
                "graded_relevance contains empty term string after trimming".to_string(),
            ));
        }
        if grade_map.contains_key(t) {
            return Err(bad_targets(format!(
                "duplicate term '{t}' in graded_relevance"
            )));
        }
        let s = entry.score;
        if s != 0.0 && s != 0.5 && s != 1.0 {
            return Err(bad_targets(format!(
                "term '{t}' has invalid graded score {s}; must be 0.0, 0.5, or 1.0"
            )));
        }
        if !vocab_set.contains(t) {
            return Err(bad_vocab(format!(
                "graded_relevance term '{t}' is not in the controlled vocabulary"
            )));
        }
        grade_map.insert(t.to_string(), s);
    }

    // Every strict term must appear in graded_relevance with score 1.0
    for t in &strict_set {
        match grade_map.get(t.as_str()) {
            None => {
                return Err(bad_graded(format!(
                    "strict term '{t}' has no graded_relevance entry"
                )))
            }
            Some(&s) if s != 1.0 => {
                return Err(bad_graded(format!(
                    "strict term '{t}' must have grade 1.0, got {s}"
                )))
            }
            _ => {}
        }
    }

    // Every soft term must appear in graded_relevance;
    // soft-only terms must have grade 0.5
    for t in &soft_set {
        match grade_map.get(t.as_str()) {
            None => {
                return Err(bad_graded(format!(
                    "soft term '{t}' has no graded_relevance entry"
                )))
            }
            Some(&s) if !strict_set.contains(t.as_str()) && s != 0.5 => {
                return Err(bad_graded(format!(
                    "soft-only term '{t}' must have grade 0.5, got {s}"
                )))
            }
            _ => {}
        }
    }

    // Cross-check all grade_map entries
    for (t, &s) in &grade_map {
        if s == 1.0 && !strict_set.contains(t.as_str()) {
            return Err(bad_graded(format!(
                "term '{t}' has grade 1.0 but is not in strict_vocabulary_terms"
            )));
        }
        if s == 0.5 {
            if !soft_set.contains(t.as_str()) {
                return Err(bad_graded(format!(
                    "term '{t}' has grade 0.5 but is not in soft_vocabulary_terms"
                )));
            }
            if strict_set.contains(t.as_str()) {
                return Err(bad_graded(format!(
                    "term '{t}' has grade 0.5 but also appears in strict_vocabulary_terms"
                )));
            }
        }
        if s == 0.0 && soft_set.contains(t.as_str()) {
            return Err(bad_graded(format!(
                "term '{t}' has grade 0.0 but is in soft_vocabulary_terms"
            )));
        }
    }

    Ok((strict_set, soft_set, grade_map))
}

// ---------------------------------------------------------------------------
// Predicted-set deduplication (Section 4)
// Returns (PredictedDedup, duplicate_term_count)
// ---------------------------------------------------------------------------

fn build_predicted_dedup(
    terms: &[StructuredUserQueryTerm],
) -> (Vec<&StructuredUserQueryTerm>, u32) {
    let mut dedup: Vec<&StructuredUserQueryTerm> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut duplicate_count: u32 = 0;
    for term in terms {
        if seen.contains(term.term.as_str()) {
            duplicate_count += 1;
        } else {
            seen.insert(term.term.as_str());
            dedup.push(term);
        }
    }
    (dedup, duplicate_count)
}

// ---------------------------------------------------------------------------
// Evidence-span normalization (Section 8)
// Lowercase → split on Unicode whitespace → strip ASCII punctuation from token
// boundaries → rejoin with single space
// ---------------------------------------------------------------------------

fn normalize_for_grounding(s: &str) -> String {
    s.split_whitespace()
        .filter_map(|token| {
            let lower = token.to_lowercase();
            let trimmed = lower
                .trim_matches(|c: char| c.is_ascii_punctuation())
                .to_string();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_types::{
        GoldenTermRelevance, StructuredUserQueryConfidence,
    };

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn vocab() -> QueryStructuringControlledVocabulary {
        QueryStructuringControlledVocabulary {
            canonical_symptoms: vec![
                "slow_writes".to_string(),
                "high_latency".to_string(),
                "timeout".to_string(),
                "packet_loss".to_string(),
            ],
            affected_components: vec![
                "raft_leader".to_string(),
                "wal_writer".to_string(),
                "network_layer".to_string(),
            ],
            failure_mode_candidates: vec![
                "lock_contention".to_string(),
                "disk_saturation".to_string(),
                "memory_pressure".to_string(),
            ],
            violated_properties: vec![
                "linearizability".to_string(),
                "durability".to_string(),
                "availability".to_string(),
            ],
        }
    }

    fn empty_field() -> GoldenVocabularyFieldTargets {
        GoldenVocabularyFieldTargets {
            strict_vocabulary_terms: vec![],
            soft_vocabulary_terms: vec![],
            graded_relevance: vec![],
        }
    }

    fn field_targets(
        strict: &[&str],
        soft_only: &[&str],
        zero_grade: &[&str],
    ) -> GoldenVocabularyFieldTargets {
        let soft_vocab: Vec<String> = strict
            .iter()
            .chain(soft_only.iter())
            .map(|s| s.to_string())
            .collect();
        let mut graded: Vec<GoldenTermRelevance> = strict
            .iter()
            .map(|&t| GoldenTermRelevance { term: t.to_string(), score: 1.0 })
            .collect();
        graded.extend(soft_only.iter().map(|&t| GoldenTermRelevance {
            term: t.to_string(),
            score: 0.5,
        }));
        graded.extend(zero_grade.iter().map(|&t| GoldenTermRelevance {
            term: t.to_string(),
            score: 0.0,
        }));
        GoldenVocabularyFieldTargets {
            strict_vocabulary_terms: strict.iter().map(|s| s.to_string()).collect(),
            soft_vocabulary_terms: soft_vocab,
            graded_relevance: graded,
        }
    }

    fn make_term(
        term: &str,
        evidence_span: &str,
        support_level: StructuredUserQuerySupportLevel,
    ) -> StructuredUserQueryTerm {
        StructuredUserQueryTerm {
            term: term.to_string(),
            evidence_span: evidence_span.to_string(),
            support_level,
        }
    }

    fn explicit(term: &str, span: &str) -> StructuredUserQueryTerm {
        make_term(term, span, StructuredUserQuerySupportLevel::Explicit)
    }

    fn weak(term: &str, span: &str) -> StructuredUserQueryTerm {
        make_term(term, span, StructuredUserQuerySupportLevel::WeakInference)
    }

    fn empty_query(symptoms: Vec<StructuredUserQueryTerm>) -> StructuredUserQuery {
        StructuredUserQuery {
            intent: String::new(),
            scenario: String::new(),
            symptoms,
            affected_subsystems: vec![],
            failure_modes: vec![],
            system_properties: vec![],
            entities: vec![],
            constraints: vec![],
            triggers: vec![],
            observability_signals: vec![],
            unresolved_terms: vec![],
            rejected_nearby_terms: vec![],
            confidence: StructuredUserQueryConfidence::Low,
        }
    }

    fn all_empty_golden() -> GoldenQueryStructuringTargets {
        GoldenQueryStructuringTargets {
            symptoms: empty_field(),
            affected_subsystems: empty_field(),
            failure_modes: empty_field(),
            system_properties: empty_field(),
        }
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    // -----------------------------------------------------------------------
    // Per-field routing tests (Section 3 vocabulary field mapping)
    // -----------------------------------------------------------------------

    #[test]
    fn symptoms_field_uses_canonical_symptoms_vocabulary() {
        let mut v = vocab();
        v.canonical_symptoms = vec!["only_sym".to_string()];
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["only_sym"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![explicit("only_sym", "only sym")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &v, "only sym").unwrap();
        assert_eq!(m.vocab_fields.symptoms.num_predicted_terms, 1);
        assert_eq!(m.vocab_fields.symptoms.invalid_vocab_count, 0);
    }

    #[test]
    fn affected_subsystems_field_uses_affected_components_vocabulary() {
        let mut v = vocab();
        v.affected_components = vec!["only_comp".to_string()];
        let golden = GoldenQueryStructuringTargets {
            affected_subsystems: field_targets(&["only_comp"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = StructuredUserQuery {
            affected_subsystems: vec![explicit("only_comp", "only comp")],
            ..empty_query(vec![])
        };
        let m = compute_query_structuring_metrics(&sq, &golden, &v, "only comp").unwrap();
        assert_eq!(m.vocab_fields.affected_subsystems.invalid_vocab_count, 0);
        assert_eq!(m.vocab_fields.affected_subsystems.num_predicted_terms, 1);
    }

    #[test]
    fn failure_modes_field_uses_failure_mode_candidates_vocabulary() {
        let mut v = vocab();
        v.failure_mode_candidates = vec!["only_fm".to_string()];
        let golden = GoldenQueryStructuringTargets {
            failure_modes: field_targets(&["only_fm"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = StructuredUserQuery {
            failure_modes: vec![explicit("only_fm", "only fm")],
            ..empty_query(vec![])
        };
        let m = compute_query_structuring_metrics(&sq, &golden, &v, "only fm").unwrap();
        assert_eq!(m.vocab_fields.failure_modes.invalid_vocab_count, 0);
        assert_eq!(m.vocab_fields.failure_modes.num_predicted_terms, 1);
    }

    #[test]
    fn system_properties_field_uses_violated_properties_vocabulary() {
        let mut v = vocab();
        v.violated_properties = vec!["only_prop".to_string()];
        let golden = GoldenQueryStructuringTargets {
            system_properties: field_targets(&["only_prop"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = StructuredUserQuery {
            system_properties: vec![explicit("only_prop", "only prop")],
            ..empty_query(vec![])
        };
        let m = compute_query_structuring_metrics(&sq, &golden, &v, "only prop").unwrap();
        assert_eq!(m.vocab_fields.system_properties.invalid_vocab_count, 0);
        assert_eq!(m.vocab_fields.system_properties.num_predicted_terms, 1);
    }

    // -----------------------------------------------------------------------
    // Section 4 — deduplication
    // -----------------------------------------------------------------------

    #[test]
    fn duplicate_terms_counted_by_duplicate_term_count() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &["high_latency"], &[]),
            ..all_empty_golden()
        };
        // "slow_writes" appears twice; second occurrence is a duplicate
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            explicit("slow_writes", "writes are slow"),
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert_eq!(m.vocab_fields.symptoms.duplicate_term_count, 1);
        assert_eq!(m.vocab_fields.symptoms.num_predicted_terms, 1);
    }

    #[test]
    fn duplicates_do_not_increase_recall_or_precision() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes", "high_latency"], &[], &[]),
            ..all_empty_golden()
        };
        let sq_dedup = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            explicit("high_latency", "high latency"),
        ]);
        let sq_dup = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            explicit("slow_writes", "writes are slow"),
            explicit("high_latency", "high latency"),
        ]);
        let raw = "slow writes high latency";
        let m_dedup =
            compute_query_structuring_metrics(&sq_dedup, &golden, &vocab(), raw).unwrap();
        let m_dup =
            compute_query_structuring_metrics(&sq_dup, &golden, &vocab(), raw).unwrap();

        assert!(approx(m_dedup.vocab_fields.symptoms.recall_strict, m_dup.vocab_fields.symptoms.recall_strict));
        assert!(approx(m_dedup.vocab_fields.symptoms.precision_soft, m_dup.vocab_fields.symptoms.precision_soft));
        assert_eq!(m_dup.vocab_fields.symptoms.duplicate_term_count, 1);
    }

    #[test]
    fn evidence_diagnostics_use_first_occurrence_for_duplicate_terms() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        // First occurrence has a valid evidence span; second (duplicate) must be ignored
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            explicit("slow_writes", ""),
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        // Only one unique term; its first span "slow writes" is valid
        assert_eq!(m.vocab_fields.symptoms.missing_evidence_span_count, 0);
        assert_eq!(m.vocab_fields.symptoms.grounded_strict_recall, 1.0);
    }

    // -----------------------------------------------------------------------
    // Section 5 — invalid vocab count
    // -----------------------------------------------------------------------

    #[test]
    fn invalid_vocab_terms_counted_by_invalid_vocab_count() {
        let golden = all_empty_golden();
        let sq = empty_query(vec![explicit("not_in_vocab", "some evidence")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "some evidence").unwrap();
        assert_eq!(m.vocab_fields.symptoms.invalid_vocab_count, 1);
    }

    #[test]
    fn invalid_vocab_terms_participate_in_predicted_set() {
        // An out-of-vocab term is a false positive (not in SoftGold, which is empty)
        let golden = all_empty_golden();
        let sq = empty_query(vec![explicit("not_in_vocab", "some evidence")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "some evidence").unwrap();
        assert_eq!(m.vocab_fields.symptoms.num_predicted_terms, 1);
        assert_eq!(m.vocab_fields.symptoms.num_false_positive, 1);
    }

    // -----------------------------------------------------------------------
    // Section 6 — set-based metrics
    // -----------------------------------------------------------------------

    #[test]
    fn set_metrics_with_full_strict_and_soft_coverage() {
        // StrictGold = {slow_writes}, SoftGold = {slow_writes, high_latency}
        // Predicted = {slow_writes, high_latency}
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &["high_latency"], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            explicit("high_latency", "high latency"),
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes high latency").unwrap();
        let s = &m.vocab_fields.symptoms;
        assert_eq!(s.num_predicted_terms, 2);
        assert_eq!(s.num_false_positive, 0);
        assert_eq!(s.num_false_negative_strict, 0);
        assert!(approx(s.precision_soft, 1.0));
        assert!(approx(s.recall_strict, 1.0));
        assert!(approx(s.recall_soft, 1.0));
    }

    #[test]
    fn precision_soft_is_zero_when_predicted_set_non_empty_and_soft_gold_empty() {
        let golden = all_empty_golden();
        let sq = empty_query(vec![explicit("slow_writes", "slow writes")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert!(approx(m.vocab_fields.symptoms.precision_soft, 0.0));
    }

    #[test]
    fn precision_soft_is_one_when_both_predicted_and_soft_gold_empty() {
        let golden = all_empty_golden();
        let sq = empty_query(vec![]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        assert!(approx(m.vocab_fields.symptoms.precision_soft, 1.0));
    }

    #[test]
    fn recall_strict_is_one_when_strict_gold_empty() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&[], &["high_latency"], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        assert!(approx(m.vocab_fields.symptoms.recall_strict, 1.0));
    }

    #[test]
    fn recall_soft_is_one_when_soft_gold_empty() {
        let golden = all_empty_golden();
        let sq = empty_query(vec![]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        assert!(approx(m.vocab_fields.symptoms.recall_soft, 1.0));
    }

    #[test]
    fn false_positive_and_false_negative_counts_are_correct() {
        // StrictGold = {slow_writes, timeout}, Predicted = {slow_writes, high_latency}
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes", "timeout"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            explicit("high_latency", "high latency"),
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes high latency").unwrap();
        let s = &m.vocab_fields.symptoms;
        // high_latency not in SoftGold → false positive
        assert_eq!(s.num_false_positive, 1);
        // timeout not predicted → false negative
        assert_eq!(s.num_false_negative_strict, 1);
        // precision_soft = 1/2 (only slow_writes is in SoftGold)
        assert!(approx(s.precision_soft, 0.5));
        // recall_strict = 1/2 (only slow_writes is predicted from {slow_writes, timeout})
        assert!(approx(s.recall_strict, 0.5));
    }

    // -----------------------------------------------------------------------
    // Section 7 — graded relevance metrics
    // -----------------------------------------------------------------------

    #[test]
    fn graded_coverage_is_correct() {
        // StrictGold = {slow_writes} (score 1.0), SoftOnly = {high_latency} (score 0.5)
        // positive_grade_sum = 1.5
        // Predicted = {slow_writes}: selected_positive = 1.0 → coverage = 1.0/1.5
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &["high_latency"], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![explicit("slow_writes", "slow writes")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert!(approx(m.vocab_fields.symptoms.graded_coverage, 1.0 / 1.5));
    }

    #[test]
    fn average_selected_score_is_correct() {
        // Predicted = {slow_writes(1.0), high_latency(0.5)} → avg = 1.5/2 = 0.75
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &["high_latency"], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            explicit("high_latency", "high latency"),
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes high latency").unwrap();
        assert!(approx(m.vocab_fields.symptoms.average_selected_score, 0.75));
    }

    #[test]
    fn zero_score_selection_count_counts_grade_zero_selections() {
        // timeout has grade 0.0 in graded_relevance
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &["timeout"]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            explicit("timeout", "timeout"),
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes timeout").unwrap();
        assert_eq!(m.vocab_fields.symptoms.zero_score_selection_count, 1);
    }

    #[test]
    fn graded_coverage_is_one_when_positive_grade_sum_zero_and_predicted_empty() {
        let golden = all_empty_golden();
        let sq = empty_query(vec![]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        assert!(approx(m.vocab_fields.symptoms.graded_coverage, 1.0));
    }

    #[test]
    fn graded_coverage_is_zero_when_positive_grade_sum_zero_and_predicted_non_empty() {
        // no positive-grade entries in graded_relevance; term has implicit grade 0.0
        let golden = all_empty_golden();
        let sq = empty_query(vec![explicit("slow_writes", "slow writes")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert!(approx(m.vocab_fields.symptoms.graded_coverage, 0.0));
    }

    #[test]
    fn average_selected_score_is_zero_when_predicted_empty() {
        let golden = all_empty_golden();
        let sq = empty_query(vec![]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        assert!(approx(m.vocab_fields.symptoms.average_selected_score, 0.0));
    }

    // -----------------------------------------------------------------------
    // Section 8 — grounding metrics
    // -----------------------------------------------------------------------

    #[test]
    fn grounded_strict_recall_counts_strict_terms_with_valid_grounding() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes", "timeout"], &[], &[]),
            ..all_empty_golden()
        };
        // slow_writes: valid span + Explicit → grounded
        // timeout: valid span + Explicit → grounded
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            explicit("timeout", "timeout"),
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes timeout").unwrap();
        assert!(approx(m.vocab_fields.symptoms.grounded_strict_recall, 1.0));
    }

    #[test]
    fn grounded_strict_recall_excludes_weak_inference_terms() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        // WeakInference support level disqualifies grounding even with valid span
        let sq = empty_query(vec![weak("slow_writes", "slow writes")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert!(approx(m.vocab_fields.symptoms.grounded_strict_recall, 0.0));
    }

    #[test]
    fn grounded_strict_recall_is_one_when_strict_gold_empty() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&[], &["high_latency"], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        assert!(approx(m.vocab_fields.symptoms.grounded_strict_recall, 1.0));
    }

    #[test]
    fn missing_evidence_span_count_counts_empty_normalized_spans() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![explicit("slow_writes", "")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert_eq!(m.vocab_fields.symptoms.missing_evidence_span_count, 1);
        assert_eq!(m.vocab_fields.symptoms.invalid_evidence_span_count, 0);
    }

    #[test]
    fn invalid_evidence_span_count_counts_non_empty_spans_not_in_query() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![explicit("slow_writes", "completely different text")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert_eq!(m.vocab_fields.symptoms.invalid_evidence_span_count, 1);
        assert_eq!(m.vocab_fields.symptoms.missing_evidence_span_count, 0);
    }

    #[test]
    fn evidence_span_near_substring_rate_reflects_matching_spans() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes", "timeout"], &[], &[]),
            ..all_empty_golden()
        };
        // slow_writes: span matches; timeout: span doesn't match
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            explicit("timeout", "completely different"),
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert!(approx(m.vocab_fields.symptoms.evidence_span_near_substring_rate, 0.5));
    }

    #[test]
    fn unsupported_selected_term_rate_covers_empty_span_non_substring_and_weak_inference() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes", "high_latency", "timeout"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),   // supported
            explicit("high_latency", ""),             // empty span → unsupported
            weak("timeout", "timeout"),               // WeakInference → unsupported
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes timeout").unwrap();
        // 2 of 3 unsupported
        assert!(approx(m.vocab_fields.symptoms.unsupported_selected_term_rate, 2.0 / 3.0));
    }

    // -----------------------------------------------------------------------
    // Normalization boundary examples from the spec
    // -----------------------------------------------------------------------

    #[test]
    fn normalization_strips_trailing_comma_from_token() {
        // "timeout" matches "timeout," after boundary stripping
        assert!(normalize_for_grounding("timeout,").contains("timeout"));
        assert_eq!(normalize_for_grounding("timeout,"), "timeout");
    }

    #[test]
    fn normalization_strips_trailing_colon_from_token() {
        assert_eq!(normalize_for_grounding("raft election:"), "raft election");
    }

    #[test]
    fn normalization_does_not_split_hyphenated_tokens() {
        assert_eq!(normalize_for_grounding("node-1"), "node-1");
        assert_eq!(normalize_for_grounding("read-only"), "read-only");
    }

    #[test]
    fn evidence_span_with_boundary_punctuation_matches_query() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        // evidence span "slow writes," should match against "slow writes" in query after normalization
        let sq = empty_query(vec![explicit("slow_writes", "slow writes,")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes and more").unwrap();
        assert_eq!(m.vocab_fields.symptoms.missing_evidence_span_count, 0);
        assert_eq!(m.vocab_fields.symptoms.invalid_evidence_span_count, 0);
        assert!(approx(m.vocab_fields.symptoms.evidence_span_near_substring_rate, 1.0));
    }

    // -----------------------------------------------------------------------
    // Section 9 — support-level metrics
    // -----------------------------------------------------------------------

    #[test]
    fn weak_inference_rate_counts_weak_selected_terms() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes", "timeout"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            weak("timeout", "timeout"),
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes timeout").unwrap();
        assert!(approx(m.vocab_fields.symptoms.weak_inference_rate, 0.5));
    }

    #[test]
    fn strict_terms_weak_inference_rate_is_zero_when_no_strict_terms_selected() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        assert!(approx(m.vocab_fields.symptoms.strict_terms_weak_inference_rate, 0.0));
    }

    #[test]
    fn strict_terms_weak_inference_rate_reflects_weak_strict_selections() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes", "timeout"], &[], &[]),
            ..all_empty_golden()
        };
        // Both strict; one is WeakInference
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            weak("timeout", "timeout"),
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes timeout").unwrap();
        // 1 weak out of 2 strict-selected
        assert!(approx(m.vocab_fields.symptoms.strict_terms_weak_inference_rate, 0.5));
    }

    #[test]
    fn weak_false_positive_rate_is_zero_when_no_false_positives() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![explicit("slow_writes", "slow writes")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert!(approx(m.vocab_fields.symptoms.weak_false_positive_rate, 0.0));
    }

    #[test]
    fn weak_false_positive_rate_reflects_weak_false_positive_terms() {
        // high_latency is not in SoftGold (false positive)
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            weak("high_latency", "high latency"), // FP + WeakInference
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes high latency").unwrap();
        // 1 weak-FP out of 1 total FP
        assert!(approx(m.vocab_fields.symptoms.weak_false_positive_rate, 1.0));
    }

    // -----------------------------------------------------------------------
    // Section 10 — field-level success metrics
    // -----------------------------------------------------------------------

    #[test]
    fn field_core_success_true_when_full_strict_recall_and_no_invalid_vocab() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![explicit("slow_writes", "slow writes")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert!(m.vocab_fields.symptoms.field_core_success);
    }

    #[test]
    fn field_core_success_false_when_invalid_vocab_present() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            explicit("not_in_vocab", "some text"),
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes some text").unwrap();
        assert!(!m.vocab_fields.symptoms.field_core_success);
    }

    #[test]
    fn field_grounded_success_false_when_unsupported_term_present() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        // WeakInference makes the term unsupported
        let sq = empty_query(vec![weak("slow_writes", "slow writes")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert!(!m.vocab_fields.symptoms.field_grounded_success);
    }

    #[test]
    fn field_grounded_success_false_when_soft_false_positive_is_unsupported() {
        // spec: field_grounded_success rejects unsupported soft/FP selections
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![
            explicit("slow_writes", "slow writes"),
            weak("high_latency", "high latency"), // unsupported false positive
        ]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes high latency").unwrap();
        assert!(!m.vocab_fields.symptoms.field_grounded_success);
    }

    #[test]
    fn empty_when_gold_exists_is_true_when_predicted_empty_and_strict_non_empty() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        assert!(m.vocab_fields.symptoms.empty_when_gold_exists);
    }

    #[test]
    fn empty_when_gold_exists_is_false_when_strict_gold_empty() {
        let golden = all_empty_golden();
        let sq = empty_query(vec![]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        assert!(!m.vocab_fields.symptoms.empty_when_gold_exists);
    }

    // -----------------------------------------------------------------------
    // Section 11 — non-vocabulary field metrics
    // -----------------------------------------------------------------------

    #[test]
    fn non_vocab_counts_reflect_array_lengths() {
        let golden = all_empty_golden();
        let sq = StructuredUserQuery {
            intent: "diagnose network issue".to_string(),
            scenario: "production cluster".to_string(),
            entities: vec!["node1".to_string(), "node2".to_string()],
            constraints: vec!["must be online".to_string()],
            triggers: vec!["deploy event".to_string(), "config change".to_string()],
            observability_signals: vec!["high cpu".to_string()],
            unresolved_terms: vec!["xyx_unknown".to_string()],
            ..empty_query(vec![])
        };
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "diagnose network").unwrap();
        let nv = &m.non_vocab_fields;
        assert_eq!(nv.entities_count, 2);
        assert_eq!(nv.constraints_count, 1);
        assert_eq!(nv.triggers_count, 2);
        assert_eq!(nv.observability_signals_count, 1);
        assert_eq!(nv.unresolved_terms_count, 1);
        assert!(nv.intent_present);
        assert!(nv.scenario_present);
    }

    #[test]
    fn intent_present_false_when_whitespace_only() {
        let golden = all_empty_golden();
        let sq = StructuredUserQuery {
            intent: "   ".to_string(),
            scenario: String::new(),
            ..empty_query(vec![])
        };
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        assert!(!m.non_vocab_fields.intent_present);
        assert!(!m.non_vocab_fields.scenario_present);
    }

    // -----------------------------------------------------------------------
    // Section 12 — cross-field aggregates
    // -----------------------------------------------------------------------

    #[test]
    fn macro_precision_soft_is_arithmetic_mean_of_four_fields() {
        // All four fields: Predicted empty, SoftGold non-empty → precision = 0.0
        // Except symptoms: both empty → precision = 1.0
        let golden = GoldenQueryStructuringTargets {
            symptoms: empty_field(),
            affected_subsystems: field_targets(&[], &["raft_leader"], &[]),
            failure_modes: field_targets(&[], &["lock_contention"], &[]),
            system_properties: field_targets(&[], &["linearizability"], &[]),
        };
        let sq = empty_query(vec![]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        // symptoms=1.0, others=1.0 (empty predicted, empty strict, soft non-empty → 0.0)
        // Wait: predicted empty + soft non-empty → precision = 0.0
        // symptoms: predicted empty + soft empty → precision = 1.0
        // others: predicted empty + soft non-empty → precision = 0.0
        // macro = (1.0 + 0.0 + 0.0 + 0.0) / 4 = 0.25
        assert!(approx(m.aggregates.macro_precision_soft, 0.25));
    }

    #[test]
    fn overall_grounded_strict_recall_is_one_when_all_strict_gold_empty() {
        let golden = all_empty_golden();
        let sq = empty_query(vec![]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        assert!(approx(m.aggregates.overall_grounded_strict_recall, 1.0));
    }

    #[test]
    fn overall_grounded_strict_recall_uses_global_numerator_and_denominator() {
        // symptoms: StrictGold={slow_writes,timeout}, grounded hits = 1
        // others: empty
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes", "timeout"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![explicit("slow_writes", "slow writes")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        // global: hits=1, total_gold=2 → 0.5
        assert!(approx(m.aggregates.overall_grounded_strict_recall, 0.5));
    }

    #[test]
    fn all_fields_core_success_rate_reflects_passed_fields_fraction() {
        // symptoms: all good → core success
        // others: empty gold + no predicted → core success (recall=1.0, invalid=0)
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![explicit("slow_writes", "slow writes")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert!(approx(m.aggregates.all_fields_core_success_rate, 1.0));
    }

    #[test]
    fn all_fields_core_success_rate_can_be_partial() {
        // symptoms: StrictGold={slow_writes}, Predicted={} → recall=0 → not core success
        // others: empty gold → core success
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &[], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "something").unwrap();
        // 3 of 4 pass (symptoms fails)
        assert!(approx(m.aggregates.all_fields_core_success_rate, 0.75));
    }

    #[test]
    fn top_level_equals_aggregates() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &["high_latency"], &[]),
            ..all_empty_golden()
        };
        let sq = empty_query(vec![explicit("slow_writes", "slow writes")]);
        let m = compute_query_structuring_metrics(&sq, &golden, &vocab(), "slow writes").unwrap();
        assert!(approx(m.top_level.macro_precision_soft, m.aggregates.macro_precision_soft));
        assert!(approx(m.top_level.macro_recall_strict, m.aggregates.macro_recall_strict));
        assert!(approx(m.top_level.macro_recall_soft, m.aggregates.macro_recall_soft));
        assert!(approx(m.top_level.overall_grounded_strict_recall, m.aggregates.overall_grounded_strict_recall));
        assert!(approx(m.top_level.all_fields_core_success_rate, m.aggregates.all_fields_core_success_rate));
    }

    // -----------------------------------------------------------------------
    // Invalid input rejection (Section 3 and Section 14)
    // -----------------------------------------------------------------------

    #[test]
    fn empty_raw_user_query_returns_invalid_raw_user_query_error() {
        let err = compute_query_structuring_metrics(
            &empty_query(vec![]),
            &all_empty_golden(),
            &vocab(),
            "",
        )
        .unwrap_err();
        assert!(
            matches!(err, QueryStructuringMetricsError::InvalidRawUserQuery { .. }),
            "expected InvalidRawUserQuery, got: {err}"
        );
    }

    #[test]
    fn whitespace_only_raw_user_query_returns_invalid_raw_user_query_error() {
        let err = compute_query_structuring_metrics(
            &empty_query(vec![]),
            &all_empty_golden(),
            &vocab(),
            "   ",
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringMetricsError::InvalidRawUserQuery { .. }));
    }

    #[test]
    fn duplicate_strict_term_returns_invalid_golden_targets_error() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: GoldenVocabularyFieldTargets {
                strict_vocabulary_terms: vec!["slow_writes".to_string(), "slow_writes".to_string()],
                soft_vocabulary_terms: vec!["slow_writes".to_string()],
                graded_relevance: vec![GoldenTermRelevance {
                    term: "slow_writes".to_string(),
                    score: 1.0,
                }],
            },
            ..all_empty_golden()
        };
        let err = compute_query_structuring_metrics(
            &empty_query(vec![]),
            &golden,
            &vocab(),
            "something",
        )
        .unwrap_err();
        assert!(
            matches!(err, QueryStructuringMetricsError::InvalidGoldenTargets { ref field, .. } if field == "symptoms"),
            "got: {err}"
        );
    }

    #[test]
    fn duplicate_soft_term_returns_invalid_golden_targets_error() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: GoldenVocabularyFieldTargets {
                strict_vocabulary_terms: vec![],
                soft_vocabulary_terms: vec!["high_latency".to_string(), "high_latency".to_string()],
                graded_relevance: vec![GoldenTermRelevance {
                    term: "high_latency".to_string(),
                    score: 0.5,
                }],
            },
            ..all_empty_golden()
        };
        let err = compute_query_structuring_metrics(
            &empty_query(vec![]),
            &golden,
            &vocab(),
            "something",
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringMetricsError::InvalidGoldenTargets { .. }));
    }

    #[test]
    fn duplicate_graded_relevance_term_returns_invalid_golden_targets_error() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: GoldenVocabularyFieldTargets {
                strict_vocabulary_terms: vec!["slow_writes".to_string()],
                soft_vocabulary_terms: vec!["slow_writes".to_string()],
                graded_relevance: vec![
                    GoldenTermRelevance { term: "slow_writes".to_string(), score: 1.0 },
                    GoldenTermRelevance { term: "slow_writes".to_string(), score: 1.0 },
                ],
            },
            ..all_empty_golden()
        };
        let err = compute_query_structuring_metrics(
            &empty_query(vec![]),
            &golden,
            &vocab(),
            "something",
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringMetricsError::InvalidGoldenTargets { .. }));
    }

    #[test]
    fn strict_not_subset_of_soft_returns_invalid_golden_targets_error() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: GoldenVocabularyFieldTargets {
                strict_vocabulary_terms: vec!["slow_writes".to_string()],
                soft_vocabulary_terms: vec![], // slow_writes missing from soft
                graded_relevance: vec![GoldenTermRelevance {
                    term: "slow_writes".to_string(),
                    score: 1.0,
                }],
            },
            ..all_empty_golden()
        };
        let err = compute_query_structuring_metrics(
            &empty_query(vec![]),
            &golden,
            &vocab(),
            "something",
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringMetricsError::InvalidGoldenTargets { .. }));
    }

    #[test]
    fn term_not_in_vocabulary_returns_inconsistent_vocabulary_mapping_error() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: GoldenVocabularyFieldTargets {
                strict_vocabulary_terms: vec!["not_a_symptom_term".to_string()],
                soft_vocabulary_terms: vec!["not_a_symptom_term".to_string()],
                graded_relevance: vec![GoldenTermRelevance {
                    term: "not_a_symptom_term".to_string(),
                    score: 1.0,
                }],
            },
            ..all_empty_golden()
        };
        let err = compute_query_structuring_metrics(
            &empty_query(vec![]),
            &golden,
            &vocab(),
            "something",
        )
        .unwrap_err();
        assert!(
            matches!(err, QueryStructuringMetricsError::InconsistentVocabularyMapping { .. }),
            "got: {err}"
        );
    }

    #[test]
    fn strict_term_with_wrong_grade_returns_inconsistent_graded_relevance_error() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: GoldenVocabularyFieldTargets {
                strict_vocabulary_terms: vec!["slow_writes".to_string()],
                soft_vocabulary_terms: vec!["slow_writes".to_string()],
                graded_relevance: vec![GoldenTermRelevance {
                    term: "slow_writes".to_string(),
                    score: 0.5, // must be 1.0
                }],
            },
            ..all_empty_golden()
        };
        let err = compute_query_structuring_metrics(
            &empty_query(vec![]),
            &golden,
            &vocab(),
            "something",
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringMetricsError::InconsistentGradedRelevance { .. }));
    }

    #[test]
    fn soft_only_term_with_wrong_grade_returns_inconsistent_graded_relevance_error() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: GoldenVocabularyFieldTargets {
                strict_vocabulary_terms: vec![],
                soft_vocabulary_terms: vec!["high_latency".to_string()],
                graded_relevance: vec![GoldenTermRelevance {
                    term: "high_latency".to_string(),
                    score: 1.0, // must be 0.5
                }],
            },
            ..all_empty_golden()
        };
        let err = compute_query_structuring_metrics(
            &empty_query(vec![]),
            &golden,
            &vocab(),
            "something",
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringMetricsError::InconsistentGradedRelevance { .. }));
    }

    #[test]
    fn grade_zero_term_in_soft_gold_returns_inconsistent_graded_relevance_error() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: GoldenVocabularyFieldTargets {
                strict_vocabulary_terms: vec![],
                soft_vocabulary_terms: vec!["high_latency".to_string()],
                graded_relevance: vec![GoldenTermRelevance {
                    term: "high_latency".to_string(),
                    score: 0.0, // must not be in SoftGold
                }],
            },
            ..all_empty_golden()
        };
        let err = compute_query_structuring_metrics(
            &empty_query(vec![]),
            &golden,
            &vocab(),
            "something",
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringMetricsError::InconsistentGradedRelevance { .. }));
    }

    #[test]
    fn invalid_graded_score_returns_invalid_golden_targets_error() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: GoldenVocabularyFieldTargets {
                strict_vocabulary_terms: vec![],
                soft_vocabulary_terms: vec![],
                graded_relevance: vec![GoldenTermRelevance {
                    term: "slow_writes".to_string(),
                    score: 0.3, // not 0.0, 0.5, or 1.0
                }],
            },
            ..all_empty_golden()
        };
        let err = compute_query_structuring_metrics(
            &empty_query(vec![]),
            &golden,
            &vocab(),
            "something",
        )
        .unwrap_err();
        assert!(matches!(err, QueryStructuringMetricsError::InvalidGoldenTargets { .. }));
    }

    #[test]
    fn errors_preserve_field_name_in_variant() {
        let golden = GoldenQueryStructuringTargets {
            affected_subsystems: GoldenVocabularyFieldTargets {
                strict_vocabulary_terms: vec!["raft_leader".to_string()],
                soft_vocabulary_terms: vec![], // strict not subset of soft
                graded_relevance: vec![GoldenTermRelevance {
                    term: "raft_leader".to_string(),
                    score: 1.0,
                }],
            },
            ..all_empty_golden()
        };
        let err = compute_query_structuring_metrics(
            &empty_query(vec![]),
            &golden,
            &vocab(),
            "something",
        )
        .unwrap_err();
        match err {
            QueryStructuringMetricsError::InvalidGoldenTargets { field, .. } => {
                assert_eq!(field, "affected_subsystems");
            }
            other => panic!("expected InvalidGoldenTargets, got: {other}"),
        }
    }

    #[test]
    fn helper_returns_exact_shared_query_structuring_metrics_shape() {
        let golden = GoldenQueryStructuringTargets {
            symptoms: field_targets(&["slow_writes"], &["high_latency"], &["timeout"]),
            affected_subsystems: field_targets(&["raft_leader"], &[], &[]),
            failure_modes: field_targets(&[], &["lock_contention"], &[]),
            system_properties: empty_field(),
        };
        let sq = StructuredUserQuery {
            symptoms: vec![explicit("slow_writes", "slow writes")],
            affected_subsystems: vec![explicit("raft_leader", "raft leader")],
            ..empty_query(vec![])
        };
        let m = compute_query_structuring_metrics(
            &sq,
            &golden,
            &vocab(),
            "slow writes raft leader",
        )
        .unwrap();
        // Verify all top-level substructures are present and populated
        let _ = &m.top_level;
        let _ = &m.vocab_fields.symptoms;
        let _ = &m.vocab_fields.affected_subsystems;
        let _ = &m.vocab_fields.failure_modes;
        let _ = &m.vocab_fields.system_properties;
        let _ = &m.non_vocab_fields;
        let _ = &m.aggregates;
    }
}
