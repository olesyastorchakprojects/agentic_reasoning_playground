use chrono::{DateTime, Utc};

use crate::config::EvalSettings;
use crate::manifest::EvalRunManifest;
use crate::snapshot::DiagnosticEvalIterationSnapshot;
use crate::storage::{
    EvalIterationSummaryRow, EvalRunSummaryRow, EvalSubjectKey, JudgeLlmCallRow, JudgeResultRow,
};

pub const CURRENT_IMPLEMENTATION_SUPPORTED_SUITES: &[&str] = &["final_no_root_cause_claim"];

#[derive(Debug, thiserror::Error)]
pub enum SummaryError {
    #[error("unsupported enabled suite in current implementation slice: {0}")]
    UnsupportedSuite(String),
    #[error("required judge result missing for subject: {0}")]
    MissingJudgeResult(String),
}

pub fn validate_supported_suite_subset(
    enabled_suite_names: &[String],
) -> Result<(), SummaryError> {
    for suite_name in enabled_suite_names {
        if !CURRENT_IMPLEMENTATION_SUPPORTED_SUITES
            .iter()
            .any(|supported| supported == suite_name)
        {
            return Err(SummaryError::UnsupportedSuite(suite_name.clone()));
        }
    }
    Ok(())
}

pub fn build_iteration_summary_row(
    key: EvalSubjectKey,
    snapshot: &DiagnosticEvalIterationSnapshot,
    judge_results: &[JudgeResultRow],
    judge_calls: &[JudgeLlmCallRow],
) -> Result<EvalIterationSummaryRow, SummaryError> {
    let no_root_cause_score = judge_results
        .iter()
        .find(|row| row.suite_name == "final_no_root_cause_claim")
        .map(|row| row.score)
        .ok_or_else(|| {
            SummaryError::MissingJudgeResult("final_no_root_cause_claim".to_string())
        })?;

    let judge_prompt_tokens: i64 = judge_calls.iter().map(|row| row.prompt_tokens).sum();
    let judge_completion_tokens: i64 =
        judge_calls.iter().map(|row| row.completion_tokens).sum();
    let judge_total_tokens: i64 = judge_calls.iter().map(|row| row.total_tokens).sum();
    let judge_total_cost_usd: f64 = judge_calls.iter().map(|row| row.total_cost_usd).sum();

    let runtime_prompt_tokens =
        snapshot.runtime_token_usage.total.prompt_tokens.unwrap_or(0) as i64;
    let runtime_completion_tokens =
        snapshot.runtime_token_usage.total.completion_tokens.unwrap_or(0) as i64;
    let runtime_total_tokens =
        snapshot.runtime_token_usage.total.total_tokens.unwrap_or(0) as i64;
    let runtime_total_cost_usd = 0.0_f64;

    let no_root_cause_gate_passed = no_root_cause_score == 2;
    let final_answer_no_hard_fail = no_root_cause_score > 0;

    Ok(EvalIterationSummaryRow {
        key,
        query_structuring_judge_score: 0.0,
        evidence_pack_judge_score: 0.0,
        final_answer_judge_score: no_root_cause_score as f64,
        query_structuring_no_hard_fail: true,
        evidence_pack_no_hard_fail: true,
        final_answer_no_hard_fail,
        usable_first_response: no_root_cause_gate_passed,
        no_root_cause_gate_passed,
        single_check_gate_passed: true,
        source_alignment_gate_passed: true,
        field_boundary_gate_passed: true,
        evidence_pack_gate_passed: true,
        query_structuring_field_boundary_correctness_score: 0,
        query_structuring_grounding_conservatism_score: 0,
        evidence_pack_role_fit_score: 0,
        evidence_pack_sufficiency_score: 0,
        final_no_root_cause_claim_score: no_root_cause_score,
        final_first_check_discriminates_score: 0,
        final_hypothesis_source_alignment_score: 0,
        final_alternative_context_handling_score: 0,
        final_result_interpretation_usefulness_score: 0,
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

    let usable_first_response_rate = iteration_rows
        .iter()
        .filter(|row| row.usable_first_response)
        .count() as f64
        / denom;
    let query_structuring_judge_score = iteration_rows
        .iter()
        .map(|row| row.query_structuring_judge_score)
        .sum::<f64>()
        / denom;
    let evidence_pack_judge_score = iteration_rows
        .iter()
        .map(|row| row.evidence_pack_judge_score)
        .sum::<f64>()
        / denom;
    let final_answer_judge_score = iteration_rows
        .iter()
        .map(|row| row.final_answer_judge_score)
        .sum::<f64>()
        / denom;
    let query_structuring_strict_pass_rate = iteration_rows
        .iter()
        .filter(|row| row.query_structuring_no_hard_fail)
        .count() as f64
        / denom;
    let evidence_pack_strict_pass_rate = iteration_rows
        .iter()
        .filter(|row| row.evidence_pack_no_hard_fail)
        .count() as f64
        / denom;
    let final_answer_strict_pass_rate = iteration_rows
        .iter()
        .filter(|row| row.no_root_cause_gate_passed)
        .count() as f64
        / denom;
    let diagnostic_move_hard_fail_rate = iteration_rows
        .iter()
        .filter(|row| !row.final_answer_no_hard_fail)
        .count() as f64
        / denom;
    let gate_pass_rate = iteration_rows
        .iter()
        .filter(|row| row.no_root_cause_gate_passed)
        .count() as f64
        / denom;

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
        suite_versions: serde_json::to_value(&manifest.suite_versions)
            .expect("suite_versions must serialize"),
        usable_first_response_rate,
        query_structuring_judge_score,
        evidence_pack_judge_score,
        final_answer_judge_score,
        query_structuring_strict_pass_rate,
        evidence_pack_strict_pass_rate,
        final_answer_strict_pass_rate,
        diagnostic_move_hard_fail_rate,
        gate_pass_rate,
        bad_final_due_to_query_rate: 0.0,
        bad_final_due_to_evidence_rate: 0.0,
        bad_final_with_good_query_and_evidence_rate: 0.0,
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
