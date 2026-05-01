use std::fs;
use std::path::{Path, PathBuf};

use crate::manifest::EvalRunManifest;
use crate::storage::{EvalIterationSummaryRow, EvalRunSummaryRow};

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("report io failure: {0}")]
    Io(String),
}

pub fn write_run_report(
    artifact_dir: &Path,
    manifest: &EvalRunManifest,
    run_summary: &EvalRunSummaryRow,
    iteration_rows: &[EvalIterationSummaryRow],
) -> Result<PathBuf, ReportError> {
    let path = artifact_dir.join("run_report.md");
    let body = render_run_report(manifest, run_summary, iteration_rows);
    fs::write(&path, body).map_err(|e| ReportError::Io(e.to_string()))?;
    Ok(path)
}

fn render_run_report(
    manifest: &EvalRunManifest,
    run_summary: &EvalRunSummaryRow,
    iteration_rows: &[EvalIterationSummaryRow],
) -> String {
    let worst_cases = iteration_rows.iter().take(5).collect::<Vec<_>>();

    let mut out = String::new();
    out.push_str("# Eval Run Report\n\n");
    out.push_str("## Run Metadata\n\n");
    out.push_str(&format!("- eval_run_id: `{}`\n", manifest.eval_run_id));
    out.push_str(&format!("- run_type: `{}`\n", manifest.run_type));
    out.push_str(&format!("- status: `{}`\n", run_summary.status));
    out.push_str(&format!("- started_at: `{}`\n", manifest.started_at));
    out.push_str(&format!(
        "- completed_at: `{}`\n",
        run_summary
            .completed_at
            .map(|v| v.to_string())
            .unwrap_or_else(|| "<none>".to_string())
    ));
    out.push_str(&format!("- runtime_run_count: `{}`\n", run_summary.runtime_run_count));
    out.push_str(&format!(
        "- iterations_evaluated_count: `{}`\n",
        run_summary.iterations_evaluated_count
    ));
    out.push_str(&format!("- judge_model: `{}`\n", run_summary.judge_model));
    out.push_str(&format!("- suite_versions: `{}`\n\n", manifest.suite_versions.len()));

    out.push_str("## Aggregated Metrics\n\n");
    out.push_str("| metric | value |\n|---|---:|\n");
    out.push_str(&format!(
        "| usable_first_response_rate | {:.4} |\n",
        run_summary.usable_first_response_rate
    ));
    out.push_str(&format!(
        "| final_answer_judge_score | {:.4} |\n",
        run_summary.final_answer_judge_score
    ));
    out.push_str(&format!(
        "| final_answer_strict_pass_rate | {:.4} |\n",
        run_summary.final_answer_strict_pass_rate
    ));
    out.push_str(&format!(
        "| diagnostic_move_hard_fail_rate | {:.4} |\n\n",
        run_summary.diagnostic_move_hard_fail_rate
    ));

    out.push_str("## Suite Distributions\n\n");
    let hard_fail = iteration_rows
        .iter()
        .filter(|row| row.final_no_root_cause_claim_score == 0)
        .count();
    let borderline = iteration_rows
        .iter()
        .filter(|row| row.final_no_root_cause_claim_score == 1)
        .count();
    let success = iteration_rows
        .iter()
        .filter(|row| row.final_no_root_cause_claim_score == 2)
        .count();
    out.push_str("| suite | score_0 | score_1 | score_2 |\n|---|---:|---:|---:|\n");
    out.push_str(&format!(
        "| final_no_root_cause_claim | {} | {} | {} |\n\n",
        hard_fail, borderline, success
    ));

    out.push_str("## Gate Breakdown\n\n");
    let gate_fail_count = iteration_rows
        .iter()
        .filter(|row| !row.no_root_cause_gate_passed)
        .count();
    let gate_fail_rate = if iteration_rows.is_empty() {
        0.0
    } else {
        gate_fail_count as f64 / iteration_rows.len() as f64
    };
    out.push_str("| gate | fail_count | fail_rate |\n|---|---:|---:|\n");
    out.push_str(&format!(
        "| no_root_cause_gate_passed | {} | {:.4} |\n\n",
        gate_fail_count, gate_fail_rate
    ));

    out.push_str("## Failure Attribution\n\n");
    out.push_str("| metric | value |\n|---|---:|\n");
    out.push_str(&format!(
        "| bad_final_due_to_query_rate | {:.4} |\n",
        run_summary.bad_final_due_to_query_rate
    ));
    out.push_str(&format!(
        "| bad_final_due_to_evidence_rate | {:.4} |\n",
        run_summary.bad_final_due_to_evidence_rate
    ));
    out.push_str(&format!(
        "| bad_final_with_good_query_and_evidence_rate | {:.4} |\n\n",
        run_summary.bad_final_with_good_query_and_evidence_rate
    ));

    out.push_str("## Worst-Case Preview\n\n");
    out.push_str("| runtime_run_id | iteration_id | final_score | usable_first_response |\n|---|---|---:|---:|\n");
    for row in worst_cases {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} |\n",
            row.key.runtime_run_id,
            row.key.iteration_id,
            row.final_no_root_cause_claim_score,
            row.usable_first_response
        ));
    }
    out.push('\n');

    out.push_str("## Token Usage\n\n");
    out.push_str("| scope | total_tokens | total_cost_usd |\n|---|---:|---:|\n");
    out.push_str(&format!(
        "| runtime | {} | {:.6} |\n",
        run_summary.runtime_total_tokens, run_summary.runtime_total_cost_usd
    ));
    out.push_str(&format!(
        "| judge_total | {} | {:.6} |\n",
        run_summary.judge_total_tokens, run_summary.judge_total_cost_usd
    ));
    out.push_str(&format!(
        "| run_total | {} | {:.6} |\n\n",
        run_summary.run_total_tokens, run_summary.run_total_cost_usd
    ));
    out.push_str(&format!(
        "Run total cost usd = runtime total cost usd + judge total cost usd = {:.6} + {:.6} = {:.6}\n",
        run_summary.runtime_total_cost_usd,
        run_summary.judge_total_cost_usd,
        run_summary.run_total_cost_usd
    ));
    out
}
