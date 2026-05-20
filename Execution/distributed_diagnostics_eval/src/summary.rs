use chrono::{DateTime, Utc};

use crate::config::EvalSettings;
use crate::manifest::EvalRunManifest;
use crate::snapshot::DiagnosticEvalIterationSnapshot;
use crate::storage::{
    EvalIterationSummaryRow, EvalRunSummaryRow, EvalSubjectKey, JudgeLlmCallRow, JudgeResultRow,
};
use distributed_diagnostics::shared_types::IterationProfile;

pub const FINAL_ANSWER_SUITES: &[&str] = &[
    "final_no_root_cause_claim",
    "final_first_check_discriminates",
    "final_alternative_context_handling",
    "final_result_interpretation_usefulness",
];

#[derive(Debug, thiserror::Error)]
pub enum SummaryError {
    #[error("required judge result missing for subject: {0}")]
    MissingJudgeResult(String),
}

pub fn validate_supported_suite_subset(
    _enabled_suite_names: &[String],
) -> Result<(), SummaryError> {
    Ok(())
}

fn suite_score(results: &[JudgeResultRow], suite_name: &str) -> Option<i16> {
    results.iter().find(|r| r.suite_name == suite_name).map(|r| r.score)
}

fn serialize_retrieval_branch(
    eval_metrics: &distributed_diagnostics::shared_types::RetrievalEvaluationMetrics,
    call_stats: &distributed_diagnostics::shared_types::RetrievalCallStats,
) -> serde_json::Value {
    serde_json::json!({
        "evaluated_k": eval_metrics.evaluated_k,
        "recall_strict": eval_metrics.recall_strict,
        "recall_soft": eval_metrics.recall_soft,
        "rr_strict": eval_metrics.rr_strict,
        "rr_soft": eval_metrics.rr_soft,
        "ndcg": eval_metrics.ndcg,
        "first_relevant_rank_strict": eval_metrics.first_relevant_rank_strict,
        "first_relevant_rank_soft": eval_metrics.first_relevant_rank_soft,
        "num_relevant_strict": eval_metrics.num_relevant_strict,
        "num_relevant_soft": eval_metrics.num_relevant_soft,
        "hits_count": call_stats.hits_count,
        "selected_count": call_stats.selected_count,
        "top_score": call_stats.top_score,
        "min_score": call_stats.min_score,
    })
}

fn avg_runtime_field(
    rows: &[EvalIterationSummaryRow],
    get_json: impl Fn(&EvalIterationSummaryRow) -> Option<&serde_json::Value>,
    field: &str,
) -> f64 {
    let values: Vec<f64> = rows
        .iter()
        .filter_map(|r| get_json(r))
        .filter_map(|v| v.get(field)?.as_f64())
        .collect();
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn category_avg(scores: &[Option<i16>]) -> f64 {
    let present: Vec<f64> = scores.iter().flatten().map(|&s| s as f64).collect();
    if present.is_empty() {
        return 0.0;
    }
    present.iter().sum::<f64>() / present.len() as f64
}

pub fn build_iteration_summary_row(
    key: EvalSubjectKey,
    snapshot: &DiagnosticEvalIterationSnapshot,
    judge_results: &[JudgeResultRow],
    judge_calls: &[JudgeLlmCallRow],
) -> Result<EvalIterationSummaryRow, SummaryError> {
    if judge_results.is_empty() {
        return Err(SummaryError::MissingJudgeResult("no judge results found".to_string()));
    }

    // Suite scores — 0 when suite was not enabled in this run
    let no_root_cause_score = suite_score(judge_results, "final_no_root_cause_claim").unwrap_or(0);
    let first_check_score = suite_score(judge_results, "final_first_check_discriminates").unwrap_or(0);
    let alt_ctx_score = suite_score(judge_results, "final_alternative_context_handling").unwrap_or(0);
    let interp_score = suite_score(judge_results, "final_result_interpretation_usefulness").unwrap_or(0);

    // --- Optional scores (will be 0 until implemented) ---
    let field_boundary_score = suite_score(judge_results, "query_structuring_field_boundary_correctness").unwrap_or(0);
    let grounding_score = suite_score(judge_results, "query_structuring_grounding_conservatism").unwrap_or(0);
    let role_fit_score = suite_score(judge_results, "evidence_pack_role_fit").unwrap_or(0);
    let sufficiency_score = suite_score(judge_results, "evidence_pack_sufficiency").unwrap_or(0);
    let source_align_score = suite_score(judge_results, "final_hypothesis_source_alignment").unwrap_or(0);

    // --- Category averages ---
    let query_structuring_judge_score = category_avg(&[
        if field_boundary_score > 0 || suite_score(judge_results, "query_structuring_field_boundary_correctness").is_some() { Some(field_boundary_score) } else { None },
        if grounding_score > 0 || suite_score(judge_results, "query_structuring_grounding_conservatism").is_some() { Some(grounding_score) } else { None },
    ]);
    let evidence_pack_judge_score = category_avg(&[
        if suite_score(judge_results, "evidence_pack_role_fit").is_some() { Some(role_fit_score) } else { None },
        if suite_score(judge_results, "evidence_pack_sufficiency").is_some() { Some(sufficiency_score) } else { None },
    ]);
    let final_answer_judge_score = category_avg(&[
        Some(no_root_cause_score),
        Some(first_check_score),
        Some(alt_ctx_score),
        Some(interp_score),
        if suite_score(judge_results, "final_hypothesis_source_alignment").is_some() { Some(source_align_score) } else { None },
    ]);

    // --- Gate booleans ---
    let no_root_cause_gate_passed = no_root_cause_score >= 1;
    let single_check_gate_passed = first_check_score >= 1;
    let source_alignment_gate_passed = source_align_score >= 1;
    let field_boundary_gate_passed =
        suite_score(judge_results, "query_structuring_field_boundary_correctness")
            .map(|s| s >= 1)
            .unwrap_or(true);
    let evidence_pack_gate_passed =
        suite_score(judge_results, "evidence_pack_sufficiency")
            .map(|s| s >= 1)
            .unwrap_or(true);

    // --- Category hard-fail flags ---
    let query_structuring_no_hard_fail =
        suite_score(judge_results, "query_structuring_field_boundary_correctness")
            .map(|s| s > 0)
            .unwrap_or(true)
            && suite_score(judge_results, "query_structuring_grounding_conservatism")
            .map(|s| s > 0)
            .unwrap_or(true);
    let evidence_pack_no_hard_fail =
        suite_score(judge_results, "evidence_pack_role_fit")
            .map(|s| s > 0)
            .unwrap_or(true)
            && suite_score(judge_results, "evidence_pack_sufficiency")
            .map(|s| s > 0)
            .unwrap_or(true);
    let final_answer_no_hard_fail =
        no_root_cause_score > 0
            && first_check_score > 0
            && alt_ctx_score > 0
            && interp_score > 0;

    // --- Usable first response (spec formula) ---
    let usable_first_response =
        no_root_cause_score >= 1 && first_check_score >= 1 && interp_score >= 1;

    // --- Continuation scores (None = n/a for initial iterations) ---
    let is_continuation = snapshot.iteration_kind == IterationProfile::Continuation;
    let iteration_kind = if is_continuation { "continuation".to_string() } else { "initial".to_string() };

    let cu1 = suite_score(judge_results, "continuation_hypothesis_update_discipline");
    let cu2 = suite_score(judge_results, "continuation_problem_understanding_update");
    let cu3 = suite_score(judge_results, "continuation_next_check_progression");
    let cu4 = suite_score(judge_results, "continuation_observation_resolution_context_recovery");

    let (
        continuation_hypothesis_update_discipline_score,
        continuation_problem_understanding_update_score,
        continuation_next_check_progression_score,
        continuation_observation_resolution_context_recovery_score,
        usable_continuation_response,
        continuation_update_no_hard_fail,
        continuation_input_no_hard_fail,
    ) = if is_continuation {
        let cu1_s = cu1;
        let cu2_s = cu2;
        let cu3_s = cu3;
        let cu4_s = cu4;
        let usable = cu1_s.map(|c1| c1 >= 1)
            .zip(cu2_s.map(|c2| c2 >= 1))
            .zip(cu3_s.map(|c3| c3 >= 1))
            .map(|((c1, c2), c3)| {
                c1 && c2 && c3
                    && no_root_cause_score >= 1
                    && first_check_score >= 1
                    && interp_score >= 1
            });
        let no_hard_fail = cu1_s.zip(cu2_s).zip(cu3_s)
            .map(|((c1, c2), c3)| c1 > 0 && c2 > 0 && c3 > 0);
        let input_no_hard_fail = cu4_s.map(|c4| c4 > 0);
        (cu1_s, cu2_s, cu3_s, cu4_s, usable, no_hard_fail, input_no_hard_fail)
    } else {
        (None, None, None, None, None, None, None)
    };

    // --- Runtime gold metrics ---
    let runtime_qs_metrics = snapshot.query_structuring_output.metrics.as_ref()
        .and_then(|m| serde_json::to_value(m).ok());

    let runtime_candidate_cards_metrics = snapshot
        .candidate_card_retrieval_output
        .metrics
        .as_ref()
        .map(|m| serialize_retrieval_branch(&m.retrieval_relevant_cards, &m.call_stats));

    let runtime_incident_primary_metrics = snapshot
        .incident_evidence_retrieval_output
        .metrics
        .as_ref()
        .map(|m| {
            serialize_retrieval_branch(
                &m.primary_card_evidence_query.relevance_judgments,
                &m.primary_card_evidence_query.call_stats,
            )
        });

    let runtime_incident_alternatives_metrics = snapshot
        .incident_evidence_retrieval_output
        .metrics
        .as_ref()
        .map(|m| {
            serialize_retrieval_branch(
                &m.alternative_cards_evidence_query.relevance_judgments,
                &m.alternative_cards_evidence_query.call_stats,
            )
        });

    let runtime_theory_evidence_metrics = snapshot
        .theory_evidence_retrieval_output
        .metrics
        .as_ref()
        .map(|m| serialize_retrieval_branch(&m.mechanism_explanation, &m.call_stats));

    // --- Token and cost ---
    let judge_prompt_tokens: i64 = judge_calls.iter().map(|row| row.prompt_tokens).sum();
    let judge_completion_tokens: i64 =
        judge_calls.iter().map(|row| row.completion_tokens).sum();
    let judge_total_tokens: i64 = judge_calls.iter().map(|row| row.total_tokens).sum();
    let judge_total_cost_usd: f64 = judge_calls.iter().map(|row| row.total_cost_usd).sum();

    let runtime_prompt_tokens =
        snapshot.runtime_token_usage.total.token_usage.prompt_tokens.unwrap_or(0) as i64;
    let runtime_completion_tokens =
        snapshot.runtime_token_usage.total.token_usage.completion_tokens.unwrap_or(0) as i64;
    let runtime_total_tokens =
        snapshot.runtime_token_usage.total.token_usage.total_tokens.unwrap_or(0) as i64;
    let runtime_total_cost_usd = snapshot.runtime_token_usage.total.total_cost_usd;
    let runtime_query_structuring_tokens =
        snapshot.runtime_token_usage.query_structuring.token_usage.total_tokens.unwrap_or(0) as i64;
    let runtime_query_structuring_prompt_tokens =
        snapshot.runtime_token_usage.query_structuring.token_usage.prompt_tokens.unwrap_or(0) as i64;
    let runtime_query_structuring_completion_tokens =
        snapshot.runtime_token_usage.query_structuring.token_usage.completion_tokens.unwrap_or(0) as i64;
    let runtime_query_structuring_cost_usd =
        snapshot.runtime_token_usage.query_structuring.total_cost_usd;
    let runtime_query_structuring_input_cost_per_million_tokens =
        snapshot.runtime_token_usage.query_structuring.input_cost_per_million_tokens;
    let runtime_query_structuring_output_cost_per_million_tokens =
        snapshot.runtime_token_usage.query_structuring.output_cost_per_million_tokens;
    let runtime_observation_boundary_resolver_tokens = snapshot
        .runtime_token_usage
        .observation_boundary_resolver
        .token_usage
        .total_tokens
        .unwrap_or(0) as i64;
    let runtime_observation_boundary_resolver_prompt_tokens = snapshot
        .runtime_token_usage
        .observation_boundary_resolver
        .token_usage
        .prompt_tokens
        .unwrap_or(0) as i64;
    let runtime_observation_boundary_resolver_completion_tokens = snapshot
        .runtime_token_usage
        .observation_boundary_resolver
        .token_usage
        .completion_tokens
        .unwrap_or(0) as i64;
    let runtime_observation_boundary_resolver_cost_usd = snapshot
        .runtime_token_usage
        .observation_boundary_resolver
        .total_cost_usd;
    let runtime_observation_boundary_resolver_input_cost_per_million_tokens = snapshot
        .runtime_token_usage
        .observation_boundary_resolver
        .input_cost_per_million_tokens;
    let runtime_observation_boundary_resolver_output_cost_per_million_tokens = snapshot
        .runtime_token_usage
        .observation_boundary_resolver
        .output_cost_per_million_tokens;
    let runtime_observation_extraction_tokens = snapshot
        .runtime_token_usage
        .observation_extraction
        .token_usage
        .total_tokens
        .unwrap_or(0) as i64;
    let runtime_observation_extraction_prompt_tokens = snapshot
        .runtime_token_usage
        .observation_extraction
        .token_usage
        .prompt_tokens
        .unwrap_or(0) as i64;
    let runtime_observation_extraction_completion_tokens = snapshot
        .runtime_token_usage
        .observation_extraction
        .token_usage
        .completion_tokens
        .unwrap_or(0) as i64;
    let runtime_observation_extraction_cost_usd = snapshot
        .runtime_token_usage
        .observation_extraction
        .total_cost_usd;
    let runtime_observation_extraction_input_cost_per_million_tokens = snapshot
        .runtime_token_usage
        .observation_extraction
        .input_cost_per_million_tokens;
    let runtime_observation_extraction_output_cost_per_million_tokens = snapshot
        .runtime_token_usage
        .observation_extraction
        .output_cost_per_million_tokens;
    let runtime_llm_structured_generation_tokens = snapshot
        .runtime_token_usage
        .llm_structured_generation
        .token_usage
        .total_tokens
        .unwrap_or(0) as i64;
    let runtime_llm_structured_generation_prompt_tokens = snapshot
        .runtime_token_usage
        .llm_structured_generation
        .token_usage
        .prompt_tokens
        .unwrap_or(0) as i64;
    let runtime_llm_structured_generation_completion_tokens = snapshot
        .runtime_token_usage
        .llm_structured_generation
        .token_usage
        .completion_tokens
        .unwrap_or(0) as i64;
    let runtime_llm_structured_generation_cost_usd = snapshot
        .runtime_token_usage
        .llm_structured_generation
        .total_cost_usd;
    let runtime_llm_structured_generation_input_cost_per_million_tokens = snapshot
        .runtime_token_usage
        .llm_structured_generation
        .input_cost_per_million_tokens;
    let runtime_llm_structured_generation_output_cost_per_million_tokens = snapshot
        .runtime_token_usage
        .llm_structured_generation
        .output_cost_per_million_tokens;
    let config_snapshot = snapshot
        .config_snapshot
        .as_ref()
        .expect("eval snapshot must carry runtime config snapshot");

    Ok(EvalIterationSummaryRow {
        key,
        iteration_kind,
        query_structuring_judge_score,
        evidence_pack_judge_score,
        final_answer_judge_score,
        query_structuring_no_hard_fail,
        evidence_pack_no_hard_fail,
        final_answer_no_hard_fail,
        usable_first_response,
        no_root_cause_gate_passed,
        single_check_gate_passed,
        source_alignment_gate_passed,
        field_boundary_gate_passed,
        evidence_pack_gate_passed,
        query_structuring_field_boundary_correctness_score: field_boundary_score,
        query_structuring_grounding_conservatism_score: grounding_score,
        evidence_pack_role_fit_score: role_fit_score,
        evidence_pack_sufficiency_score: sufficiency_score,
        final_no_root_cause_claim_score: no_root_cause_score,
        final_first_check_discriminates_score: first_check_score,
        final_hypothesis_source_alignment_score: source_align_score,
        final_alternative_context_handling_score: alt_ctx_score,
        final_result_interpretation_usefulness_score: interp_score,
        continuation_hypothesis_update_discipline_score,
        continuation_problem_understanding_update_score,
        continuation_next_check_progression_score,
        continuation_observation_resolution_context_recovery_score,
        usable_continuation_response,
        continuation_update_no_hard_fail,
        continuation_input_no_hard_fail,
        runtime_qs_metrics,
        runtime_candidate_cards_metrics,
        runtime_incident_primary_metrics,
        runtime_incident_alternatives_metrics,
        runtime_theory_evidence_metrics,
        runtime_query_structuring_model: snapshot.runtime_token_usage.query_structuring.model_name.clone(),
        runtime_observation_boundary_resolver_model: snapshot
            .runtime_token_usage
            .observation_boundary_resolver
            .model_name
            .clone(),
        runtime_observation_extraction_model: snapshot
            .runtime_token_usage
            .observation_extraction
            .model_name
            .clone(),
        runtime_llm_structured_generation_model: snapshot
            .runtime_token_usage
            .llm_structured_generation
            .model_name
            .clone(),
        runtime_query_structuring_prompt_tokens,
        runtime_query_structuring_completion_tokens,
        runtime_query_structuring_input_cost_per_million_tokens,
        runtime_query_structuring_output_cost_per_million_tokens,
        runtime_observation_boundary_resolver_prompt_tokens,
        runtime_observation_boundary_resolver_completion_tokens,
        runtime_observation_boundary_resolver_input_cost_per_million_tokens,
        runtime_observation_boundary_resolver_output_cost_per_million_tokens,
        runtime_observation_extraction_prompt_tokens,
        runtime_observation_extraction_completion_tokens,
        runtime_observation_extraction_input_cost_per_million_tokens,
        runtime_observation_extraction_output_cost_per_million_tokens,
        runtime_llm_structured_generation_prompt_tokens,
        runtime_llm_structured_generation_completion_tokens,
        runtime_llm_structured_generation_input_cost_per_million_tokens,
        runtime_llm_structured_generation_output_cost_per_million_tokens,
        runtime_query_structuring_prompt_version: config_snapshot
            .query_structuring_prompt_version
            .clone(),
        runtime_observation_boundary_resolver_prompt_version: config_snapshot
            .observation_boundary_resolver_prompt_version
            .clone(),
        runtime_observation_extraction_prompt_version: config_snapshot
            .observation_extraction_prompt_version
            .clone(),
        runtime_prompt_context_prompt_version: config_snapshot
            .prompt_context_prompt_version
            .clone(),
        runtime_diagnostic_update_prompt_context_prompt_version: config_snapshot
            .diagnostic_update_prompt_context_prompt_version
            .clone(),
        runtime_query_structuring_tokens,
        runtime_query_structuring_cost_usd,
        runtime_observation_boundary_resolver_tokens,
        runtime_observation_boundary_resolver_cost_usd,
        runtime_observation_extraction_tokens,
        runtime_observation_extraction_cost_usd,
        runtime_llm_structured_generation_tokens,
        runtime_llm_structured_generation_cost_usd,
        runtime_prompt_tokens,
        runtime_completion_tokens,
        runtime_total_tokens,
        runtime_total_cost_usd,
        judge_prompt_tokens,
        judge_completion_tokens,
        judge_total_tokens,
        judge_total_cost_usd,
        run_total_tokens: runtime_total_tokens + judge_total_tokens,
        run_total_cost_usd: runtime_total_cost_usd + judge_total_cost_usd,
    })
}

pub fn build_run_summary_row(
    settings: &EvalSettings,
    manifest: &EvalRunManifest,
    iteration_rows: &[EvalIterationSummaryRow],
    status: &str,
    completed_at: Option<DateTime<Utc>>,
) -> EvalRunSummaryRow {
    let count = iteration_rows.len() as f64;
    let denom = if count > 0.0 { count } else { 1.0 };
    let initial_rows: Vec<&EvalIterationSummaryRow> = iteration_rows
        .iter()
        .filter(|row| row.iteration_kind == "initial")
        .collect();
    let initial_count = initial_rows.len() as f64;
    let initial_denom = if initial_count > 0.0 { initial_count } else { 1.0 };

    let usable_first_response_rate = initial_rows
        .iter()
        .filter(|row| row.usable_first_response)
        .count() as f64
        / initial_denom;
    let query_structuring_judge_score = initial_rows
        .iter()
        .map(|row| row.query_structuring_judge_score)
        .sum::<f64>()
        / initial_denom;
    let evidence_pack_judge_score = initial_rows
        .iter()
        .map(|row| row.evidence_pack_judge_score)
        .sum::<f64>()
        / initial_denom;
    let final_answer_judge_score = iteration_rows
        .iter()
        .map(|row| row.final_answer_judge_score)
        .sum::<f64>()
        / denom;
    let query_structuring_no_hard_fail_rate = initial_rows
        .iter()
        .filter(|row| row.query_structuring_no_hard_fail)
        .count() as f64
        / initial_denom;
    let evidence_pack_no_hard_fail_rate = initial_rows
        .iter()
        .filter(|row| row.evidence_pack_no_hard_fail)
        .count() as f64
        / initial_denom;
    let final_answer_no_hard_fail_rate = iteration_rows
        .iter()
        .filter(|row| row.final_answer_no_hard_fail)
        .count() as f64
        / denom;
    let query_structuring_strict_pass_rate = initial_rows
        .iter()
        .filter(|row|
            row.query_structuring_field_boundary_correctness_score == 2
            && row.query_structuring_grounding_conservatism_score == 2
        )
        .count() as f64 / initial_denom;
    let evidence_pack_strict_pass_rate = initial_rows
        .iter()
        .filter(|row|
            row.evidence_pack_role_fit_score == 2
            && row.evidence_pack_sufficiency_score == 2
        )
        .count() as f64 / initial_denom;
    let final_answer_strict_pass_rate = iteration_rows
        .iter()
        .filter(|row|
            row.final_no_root_cause_claim_score == 2
            && row.final_first_check_discriminates_score == 2
            && row.final_hypothesis_source_alignment_score == 2
            && row.final_alternative_context_handling_score == 2
            && row.final_result_interpretation_usefulness_score == 2
        )
        .count() as f64 / denom;
    let diagnostic_move_hard_fail_rate = iteration_rows
        .iter()
        .filter(|row| !row.final_answer_no_hard_fail)
        .count() as f64
        / denom;

    let runtime_qs_core_success_rate = avg_runtime_field(
        iteration_rows,
        |r| r.runtime_qs_metrics.as_ref().and_then(|v| v.get("top_level")),
        "all_fields_core_success_rate",
    );
    let runtime_qs_macro_precision_soft = avg_runtime_field(
        iteration_rows,
        |r| r.runtime_qs_metrics.as_ref().and_then(|v| v.get("top_level")),
        "macro_precision_soft",
    );
    let runtime_qs_macro_recall_strict = avg_runtime_field(
        iteration_rows,
        |r| r.runtime_qs_metrics.as_ref().and_then(|v| v.get("top_level")),
        "macro_recall_strict",
    );
    let runtime_qs_macro_recall_soft = avg_runtime_field(
        iteration_rows,
        |r| r.runtime_qs_metrics.as_ref().and_then(|v| v.get("top_level")),
        "macro_recall_soft",
    );
    let runtime_qs_grounded_strict_recall = avg_runtime_field(
        iteration_rows,
        |r| r.runtime_qs_metrics.as_ref().and_then(|v| v.get("top_level")),
        "overall_grounded_strict_recall",
    );

    let runtime_retrieval_candidate_cards_recall_strict = avg_runtime_field(
        iteration_rows,
        |r| r.runtime_candidate_cards_metrics.as_ref(),
        "recall_strict",
    );
    let runtime_retrieval_incident_primary_recall_strict = avg_runtime_field(
        iteration_rows,
        |r| r.runtime_incident_primary_metrics.as_ref(),
        "recall_strict",
    );
    let runtime_retrieval_incident_alternatives_recall_strict = avg_runtime_field(
        iteration_rows,
        |r| r.runtime_incident_alternatives_metrics.as_ref(),
        "recall_strict",
    );
    let runtime_retrieval_theory_evidence_recall_strict = avg_runtime_field(
        iteration_rows,
        |r| r.runtime_theory_evidence_metrics.as_ref(),
        "recall_strict",
    );

    let runtime_retrieval_mean_ndcg = {
        let per_run: Vec<f64> = iteration_rows
            .iter()
            .filter_map(|r| {
                let targets = [
                    r.runtime_candidate_cards_metrics.as_ref().and_then(|v| v.get("ndcg")?.as_f64()),
                    r.runtime_incident_primary_metrics.as_ref().and_then(|v| v.get("ndcg")?.as_f64()),
                    r.runtime_incident_alternatives_metrics.as_ref().and_then(|v| v.get("ndcg")?.as_f64()),
                    r.runtime_theory_evidence_metrics.as_ref().and_then(|v| v.get("ndcg")?.as_f64()),
                ];
                let present: Vec<f64> = targets.into_iter().flatten().collect();
                if present.is_empty() {
                    None
                } else {
                    Some(present.iter().sum::<f64>() / present.len() as f64)
                }
            })
            .collect();
        if per_run.is_empty() {
            0.0
        } else {
            per_run.iter().sum::<f64>() / per_run.len() as f64
        }
    };

    let runtime_retrieval_all_strict_recall_success_rate = {
        let per_run: Vec<f64> = iteration_rows
            .iter()
            .filter_map(|r| {
                let targets = [
                    r.runtime_candidate_cards_metrics.as_ref().and_then(|v| v.get("recall_strict")?.as_f64()),
                    r.runtime_incident_primary_metrics.as_ref().and_then(|v| v.get("recall_strict")?.as_f64()),
                    r.runtime_incident_alternatives_metrics.as_ref().and_then(|v| v.get("recall_strict")?.as_f64()),
                    r.runtime_theory_evidence_metrics.as_ref().and_then(|v| v.get("recall_strict")?.as_f64()),
                ];
                let present: Vec<f64> = targets.into_iter().flatten().collect();
                if present.is_empty() {
                    None
                } else {
                    let success_count = present.iter().filter(|&&v| v >= 1.0).count() as f64;
                    Some(success_count / present.len() as f64)
                }
            })
            .collect();
        if per_run.is_empty() {
            0.0
        } else {
            per_run.iter().sum::<f64>() / per_run.len() as f64
        }
    };

    let runtime_retrieval_all_soft_recall_success_rate = {
        let per_run: Vec<f64> = iteration_rows
            .iter()
            .filter_map(|r| {
                let targets = [
                    r.runtime_candidate_cards_metrics.as_ref().and_then(|v| v.get("recall_soft")?.as_f64()),
                    r.runtime_incident_primary_metrics.as_ref().and_then(|v| v.get("recall_soft")?.as_f64()),
                    r.runtime_incident_alternatives_metrics.as_ref().and_then(|v| v.get("recall_soft")?.as_f64()),
                    r.runtime_theory_evidence_metrics.as_ref().and_then(|v| v.get("recall_soft")?.as_f64()),
                ];
                let present: Vec<f64> = targets.into_iter().flatten().collect();
                if present.is_empty() {
                    None
                } else {
                    let success_count = present.iter().filter(|&&v| v > 0.0).count() as f64;
                    Some(success_count / present.len() as f64)
                }
            })
            .collect();
        if per_run.is_empty() {
            0.0
        } else {
            per_run.iter().sum::<f64>() / per_run.len() as f64
        }
    };

    let runtime_retrieval_zero_hit_rate = {
        let per_run: Vec<f64> = iteration_rows
            .iter()
            .filter_map(|r| {
                let targets = [
                    r.runtime_candidate_cards_metrics.as_ref().and_then(|v| v.get("hits_count")?.as_f64()),
                    r.runtime_incident_primary_metrics.as_ref().and_then(|v| v.get("hits_count")?.as_f64()),
                    r.runtime_incident_alternatives_metrics.as_ref().and_then(|v| v.get("hits_count")?.as_f64()),
                    r.runtime_theory_evidence_metrics.as_ref().and_then(|v| v.get("hits_count")?.as_f64()),
                ];
                let present: Vec<f64> = targets.into_iter().flatten().collect();
                if present.is_empty() {
                    None
                } else {
                    let zero_count = present.iter().filter(|&&v| v == 0.0).count() as f64;
                    Some(zero_count / present.len() as f64)
                }
            })
            .collect();
        if per_run.is_empty() {
            0.0
        } else {
            per_run.iter().sum::<f64>() / per_run.len() as f64
        }
    };

    let gate_pass_rate = iteration_rows
        .iter()
        .filter(|row| {
            row.no_root_cause_gate_passed
                && row.single_check_gate_passed
                && row.source_alignment_gate_passed
                && row.field_boundary_gate_passed
                && row.evidence_pack_gate_passed
        })
        .count() as f64
        / denom;

    let bad_final_due_to_query_rate = iteration_rows
        .iter()
        .filter(|row| !row.usable_first_response && !row.query_structuring_no_hard_fail)
        .count() as f64
        / denom;
    let bad_final_due_to_evidence_rate = iteration_rows
        .iter()
        .filter(|row| !row.usable_first_response && !row.evidence_pack_no_hard_fail)
        .count() as f64
        / denom;
    let bad_final_with_good_query_and_evidence_rate = iteration_rows
        .iter()
        .filter(|row| {
            !row.usable_first_response
                && row.query_structuring_no_hard_fail
                && row.evidence_pack_no_hard_fail
        })
        .count() as f64
        / denom;

    // --- Continuation aggregates (only over continuation iterations) ---
    let cont_rows: Vec<&EvalIterationSummaryRow> = iteration_rows
        .iter()
        .filter(|r| r.iteration_kind == "continuation")
        .collect();
    let cont_count = cont_rows.len();

    let opt_mean_score = |get: &dyn Fn(&EvalIterationSummaryRow) -> Option<i16>| -> Option<f64> {
        let vals: Vec<f64> = cont_rows.iter().filter_map(|r| get(r).map(|s| s as f64)).collect();
        if vals.is_empty() { None } else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
    };
    let opt_frac_bool = |get: &dyn Fn(&EvalIterationSummaryRow) -> Option<bool>| -> Option<f64> {
        let applicable: Vec<bool> = cont_rows.iter().filter_map(|r| get(r)).collect();
        if applicable.is_empty() { None } else {
            Some(applicable.iter().filter(|&&v| v).count() as f64 / applicable.len() as f64)
        }
    };

    let continuation_hypothesis_update_discipline_score_avg =
        opt_mean_score(&|r| r.continuation_hypothesis_update_discipline_score);
    let continuation_problem_understanding_update_score_avg =
        opt_mean_score(&|r| r.continuation_problem_understanding_update_score);
    let continuation_next_check_progression_score_avg =
        opt_mean_score(&|r| r.continuation_next_check_progression_score);
    let continuation_observation_resolution_context_recovery_score_avg =
        opt_mean_score(&|r| r.continuation_observation_resolution_context_recovery_score);

    let usable_continuation_response_rate =
        opt_frac_bool(&|r| r.usable_continuation_response);
    let continuation_update_no_hard_fail_rate =
        opt_frac_bool(&|r| r.continuation_update_no_hard_fail);
    let continuation_input_no_hard_fail_rate =
        opt_frac_bool(&|r| r.continuation_input_no_hard_fail);

    let continuation_update_judge_score = if cont_count == 0 {
        None
    } else {
        let cu1_vals: Vec<f64> = cont_rows.iter().filter_map(|r| r.continuation_hypothesis_update_discipline_score.map(|s| s as f64)).collect();
        let cu2_vals: Vec<f64> = cont_rows.iter().filter_map(|r| r.continuation_problem_understanding_update_score.map(|s| s as f64)).collect();
        let cu3_vals: Vec<f64> = cont_rows.iter().filter_map(|r| r.continuation_next_check_progression_score.map(|s| s as f64)).collect();
        let iter_avgs: Vec<f64> = cu1_vals.iter().zip(cu2_vals.iter()).zip(cu3_vals.iter())
            .map(|((c1, c2), c3)| (c1 + c2 + c3) / 3.0)
            .collect();
        if iter_avgs.is_empty() { None }
        else { Some(iter_avgs.iter().sum::<f64>() / iter_avgs.len() as f64) }
    };

    let continuation_input_judge_score =
        opt_mean_score(&|r| r.continuation_observation_resolution_context_recovery_score);

    let continuation_update_strict_pass_rate = if cont_count == 0 { None } else {
        let applicable: Vec<bool> = cont_rows.iter().filter_map(|r| {
            r.continuation_hypothesis_update_discipline_score.zip(
            r.continuation_problem_understanding_update_score).zip(
            r.continuation_next_check_progression_score)
            .map(|((c1, c2), c3)| c1 == 2 && c2 == 2 && c3 == 2)
        }).collect();
        if applicable.is_empty() { None }
        else { Some(applicable.iter().filter(|&&v| v).count() as f64 / applicable.len() as f64) }
    };

    let continuation_input_strict_pass_rate = if cont_count == 0 { None } else {
        let applicable: Vec<bool> = cont_rows.iter()
            .filter_map(|r| r.continuation_observation_resolution_context_recovery_score.map(|s| s == 2))
            .collect();
        if applicable.is_empty() { None }
        else { Some(applicable.iter().filter(|&&v| v).count() as f64 / applicable.len() as f64) }
    };

    let runtime_prompt_tokens: i64 =
        iteration_rows.iter().map(|row| row.runtime_prompt_tokens).sum();
    let runtime_completion_tokens: i64 =
        iteration_rows.iter().map(|row| row.runtime_completion_tokens).sum();
    let runtime_total_tokens: i64 =
        iteration_rows.iter().map(|row| row.runtime_total_tokens).sum();
    let runtime_total_cost_usd: f64 = iteration_rows
        .iter()
        .map(|row| row.runtime_total_cost_usd)
        .sum();
    let runtime_query_structuring_tokens: i64 = iteration_rows
        .iter()
        .map(|row| row.runtime_query_structuring_tokens)
        .sum();
    let runtime_query_structuring_cost_usd: f64 = iteration_rows
        .iter()
        .map(|row| row.runtime_query_structuring_cost_usd)
        .sum();
    let runtime_observation_boundary_resolver_tokens: i64 = iteration_rows
        .iter()
        .map(|row| row.runtime_observation_boundary_resolver_tokens)
        .sum();
    let runtime_observation_boundary_resolver_cost_usd: f64 = iteration_rows
        .iter()
        .map(|row| row.runtime_observation_boundary_resolver_cost_usd)
        .sum();
    let runtime_observation_extraction_tokens: i64 = iteration_rows
        .iter()
        .map(|row| row.runtime_observation_extraction_tokens)
        .sum();
    let runtime_observation_extraction_cost_usd: f64 = iteration_rows
        .iter()
        .map(|row| row.runtime_observation_extraction_cost_usd)
        .sum();
    let runtime_llm_structured_generation_tokens: i64 = iteration_rows
        .iter()
        .map(|row| row.runtime_llm_structured_generation_tokens)
        .sum();
    let runtime_llm_structured_generation_cost_usd: f64 = iteration_rows
        .iter()
        .map(|row| row.runtime_llm_structured_generation_cost_usd)
        .sum();
    let first_row = iteration_rows.first();
    let judge_prompt_tokens: i64 =
        iteration_rows.iter().map(|row| row.judge_prompt_tokens).sum();
    let judge_completion_tokens: i64 = iteration_rows
        .iter()
        .map(|row| row.judge_completion_tokens)
        .sum();
    let judge_total_tokens: i64 =
        iteration_rows.iter().map(|row| row.judge_total_tokens).sum();
    let judge_total_cost_usd: f64 =
        iteration_rows.iter().map(|row| row.judge_total_cost_usd).sum();

    EvalRunSummaryRow {
        eval_run_id: manifest.eval_run_id,
        run_type: manifest.run_type.clone(),
        status: status.to_string(),
        started_at: manifest.started_at,
        completed_at,
        runtime_run_count: manifest.runtime_run_count as i64,
        iterations_evaluated_count: iteration_rows.len() as i64,
        judge_provider: settings.judge.provider.clone(),
        judge_model: settings.judge.model_name.clone(),
        runtime_query_structuring_model: first_row
            .map(|row| row.runtime_query_structuring_model.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        runtime_observation_boundary_resolver_model: first_row
            .map(|row| row.runtime_observation_boundary_resolver_model.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        runtime_observation_extraction_model: first_row
            .map(|row| row.runtime_observation_extraction_model.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        runtime_llm_structured_generation_model: first_row
            .map(|row| row.runtime_llm_structured_generation_model.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        runtime_query_structuring_prompt_version: first_row
            .map(|row| row.runtime_query_structuring_prompt_version.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        runtime_observation_boundary_resolver_prompt_version: first_row
            .map(|row| row.runtime_observation_boundary_resolver_prompt_version.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        runtime_observation_extraction_prompt_version: first_row
            .map(|row| row.runtime_observation_extraction_prompt_version.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        runtime_prompt_context_prompt_version: first_row
            .map(|row| row.runtime_prompt_context_prompt_version.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        runtime_diagnostic_update_prompt_context_prompt_version: first_row
            .map(|row| row.runtime_diagnostic_update_prompt_context_prompt_version.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        suite_versions: serde_json::to_value(&manifest.suite_versions)
            .expect("suite_versions must serialize"),
        usable_first_response_rate,
        query_structuring_judge_score,
        evidence_pack_judge_score,
        final_answer_judge_score,
        query_structuring_no_hard_fail_rate,
        evidence_pack_no_hard_fail_rate,
        final_answer_no_hard_fail_rate,
        query_structuring_strict_pass_rate,
        evidence_pack_strict_pass_rate,
        final_answer_strict_pass_rate,
        diagnostic_move_hard_fail_rate,
        runtime_qs_core_success_rate,
        runtime_qs_macro_precision_soft,
        runtime_qs_macro_recall_strict,
        runtime_qs_macro_recall_soft,
        runtime_qs_grounded_strict_recall,
        runtime_retrieval_mean_ndcg,
        runtime_retrieval_all_strict_recall_success_rate,
        runtime_retrieval_all_soft_recall_success_rate,
        runtime_retrieval_zero_hit_rate,
        runtime_retrieval_candidate_cards_recall_strict,
        runtime_retrieval_incident_primary_recall_strict,
        runtime_retrieval_incident_alternatives_recall_strict,
        runtime_retrieval_theory_evidence_recall_strict,
        gate_pass_rate,
        bad_final_due_to_query_rate,
        bad_final_due_to_evidence_rate,
        bad_final_with_good_query_and_evidence_rate: bad_final_with_good_query_and_evidence_rate,
        runtime_query_structuring_tokens,
        runtime_query_structuring_cost_usd,
        runtime_observation_boundary_resolver_tokens,
        runtime_observation_boundary_resolver_cost_usd,
        runtime_observation_extraction_tokens,
        runtime_observation_extraction_cost_usd,
        runtime_llm_structured_generation_tokens,
        runtime_llm_structured_generation_cost_usd,
        usable_continuation_response_rate,
        continuation_update_judge_score,
        continuation_input_judge_score,
        continuation_update_no_hard_fail_rate,
        continuation_update_strict_pass_rate,
        continuation_input_no_hard_fail_rate,
        continuation_input_strict_pass_rate,
        continuation_hypothesis_update_discipline_score_avg,
        continuation_problem_understanding_update_score_avg,
        continuation_next_check_progression_score_avg,
        continuation_observation_resolution_context_recovery_score_avg,
        runtime_prompt_tokens,
        runtime_completion_tokens,
        runtime_total_tokens,
        runtime_total_cost_usd,
        judge_prompt_tokens,
        judge_completion_tokens,
        judge_total_tokens,
        judge_total_cost_usd,
        run_total_tokens: runtime_total_tokens + judge_total_tokens,
        run_total_cost_usd: runtime_total_cost_usd + judge_total_cost_usd,
    }
}
