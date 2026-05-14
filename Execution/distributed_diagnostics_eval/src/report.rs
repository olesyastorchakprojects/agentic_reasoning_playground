use std::fs;
use std::path::{Path, PathBuf};

use std::collections::BTreeMap;

use crate::manifest::EvalRunManifest;
use crate::storage::{EvalIterationSummaryRow, EvalRunSummaryRow, JudgeLlmCallRow};
use crate::suites::{JudgeSuiteCatalog, SuiteApplicability};

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
    judge_calls: &[JudgeLlmCallRow],
    catalog: &JudgeSuiteCatalog,
    enabled_suites: &[String],
) -> Result<PathBuf, ReportError> {
    let path = artifact_dir.join("run_report.md");
    let body = render_run_report(manifest, run_summary, iteration_rows, judge_calls, catalog, enabled_suites);
    fs::write(&path, body).map_err(|e| ReportError::Io(e.to_string()))?;
    Ok(path)
}

const SUITE_CODES: &[(&str, &str)] = &[
    ("QS1", "query_structuring_field_boundary_correctness"),
    ("QS2", "query_structuring_grounding_conservatism"),
    ("EP1", "evidence_pack_role_fit"),
    ("EP2", "evidence_pack_sufficiency"),
    ("FA1", "final_no_root_cause_claim"),
    ("FA2", "final_first_check_discriminates"),
    ("FA3", "final_hypothesis_source_alignment"),
    ("FA4", "final_alternative_context_handling"),
    ("FA5", "final_result_interpretation_usefulness"),
    ("CU1", "continuation_hypothesis_update_discipline"),
    ("CU2", "continuation_problem_understanding_update"),
    ("CU3", "continuation_next_check_progression"),
    ("CU4", "continuation_observation_resolution_context_recovery"),
];

fn suite_code(name: &str) -> &'static str {
    SUITE_CODES.iter().find(|(_, s)| *s == name).map(|(c, _)| *c).unwrap_or("?")
}

fn code_legend() -> String {
    "> QS1 = query_structuring_field_boundary_correctness ; QS2 = query_structuring_grounding_conservatism\n\
     > EP1 = evidence_pack_role_fit ; EP2 = evidence_pack_sufficiency\n\
     > FA1 = final_no_root_cause_claim ; FA2 = final_first_check_discriminates ; FA3 = final_hypothesis_source_alignment ; FA4 = final_alternative_context_handling ; FA5 = final_result_interpretation_usefulness\n\
     > CU1 = continuation_hypothesis_update_discipline ; CU2 = continuation_problem_understanding_update ; CU3 = continuation_next_check_progression ; CU4 = continuation_observation_resolution_context_recovery\n".to_string()
}

fn suite_overview_link_note() -> &'static str {
    "\n\n> See [Appendix D: Suite Overview](#appendix-d-suite-overview) for the detailed suite description.\n"
}

fn suite_score_for_name(r: &EvalIterationSummaryRow, name: &str) -> i16 {
    match name {
        "final_no_root_cause_claim" => r.final_no_root_cause_claim_score,
        "final_first_check_discriminates" => r.final_first_check_discriminates_score,
        "final_alternative_context_handling" => r.final_alternative_context_handling_score,
        "final_result_interpretation_usefulness" => r.final_result_interpretation_usefulness_score,
        "final_hypothesis_source_alignment" => r.final_hypothesis_source_alignment_score,
        "query_structuring_field_boundary_correctness" => r.query_structuring_field_boundary_correctness_score,
        "query_structuring_grounding_conservatism" => r.query_structuring_grounding_conservatism_score,
        "evidence_pack_role_fit" => r.evidence_pack_role_fit_score,
        "evidence_pack_sufficiency" => r.evidence_pack_sufficiency_score,
        _ => 0,
    }
}

fn quality_status(score: f64) -> &'static str {
    if score >= 0.75 { "strong" } else if score >= 0.50 { "mixed" } else { "weak" }
}

fn render_quality_loss_section(
    run_summary: &EvalRunSummaryRow,
    iteration_rows: &[EvalIterationSummaryRow],
) -> String {
    let n = iteration_rows.len();
    let denom = if n > 0 { n as f64 } else { 1.0 };
    let unusable = iteration_rows.iter().filter(|r| !r.usable_first_response).count();

    let cont_rows: Vec<&EvalIterationSummaryRow> = iteration_rows.iter()
        .filter(|r| r.iteration_kind == "continuation")
        .collect();
    let has_cont = !cont_rows.is_empty();

    // Continuation aggregate signals
    let cont_usable_rate = iter_opt_frac(&cont_rows, |r| r.usable_continuation_response);
    let cont_update_judge = {
        let vals: Vec<f64> = cont_rows.iter().filter_map(|r| {
            r.continuation_hypothesis_update_discipline_score
                .zip(r.continuation_problem_understanding_update_score)
                .zip(r.continuation_next_check_progression_score)
                .map(|((c1, c2), c3)| (c1 as f64 + c2 as f64 + c3 as f64) / 3.0)
        }).collect();
        if vals.is_empty() { None } else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
    };
    let cont_input_judge = iter_opt_score(&cont_rows, |r| r.continuation_observation_resolution_context_recovery_score);
    let cont_no_hard_fail = iter_opt_frac(&cont_rows, |r| r.continuation_update_no_hard_fail);
    let cont_update_strict = {
        let applicable: Vec<bool> = cont_rows.iter().filter_map(|r| {
            r.continuation_hypothesis_update_discipline_score
                .zip(r.continuation_problem_understanding_update_score)
                .zip(r.continuation_next_check_progression_score)
                .map(|((c1, c2), c3)| c1 == 2 && c2 == 2 && c3 == 2)
        }).collect();
        if applicable.is_empty() { None }
        else { Some(applicable.iter().filter(|&&v| v).count() as f64 / applicable.len() as f64) }
    };

    let cont_status = match (cont_usable_rate, cont_update_judge) {
        (Some(ucr), Some(cujs)) if ucr >= 1.0 && cujs >= 1.5 => "strong",
        (Some(ucr), _) if ucr >= 0.5 && cont_no_hard_fail.unwrap_or(0.0) >= 0.5 => "mixed",
        _ => "weak",
    };

    // Composite scores (0-1)
    let qs_score = (run_summary.query_structuring_judge_score / 2.0
        + run_summary.query_structuring_no_hard_fail_rate
        + run_summary.runtime_qs_core_success_rate) / 3.0;
    let rt_score = (run_summary.runtime_retrieval_all_strict_recall_success_rate
        + run_summary.runtime_retrieval_mean_ndcg) / 2.0;
    let ep_score = (run_summary.evidence_pack_judge_score / 2.0
        + run_summary.evidence_pack_no_hard_fail_rate) / 2.0;
    let fa_score = (run_summary.final_answer_judge_score / 2.0
        + run_summary.final_answer_no_hard_fail_rate
        + run_summary.usable_first_response_rate) / 3.0;

    let mut out = String::new();
    out.push_str("## Where Quality Was Lost\n\n");

    // ── Pipeline Stage Summary ────────────────────────────────────────────────
    out.push_str("### Pipeline Stage Summary\n\n");
    out.push_str("| stage | signals | status | interpretation |\n|---|---|---|---|\n");

    // Query structuring
    let qs_interp = {
        let gate_fails = iteration_rows.iter().filter(|r| !r.field_boundary_gate_passed).count();
        let mut parts = vec![];
        if run_summary.query_structuring_strict_pass_rate == 0.0 {
            parts.push("no strict pass on any run".to_string());
        }
        if gate_fails > 0 {
            parts.push(format!("{} field boundary gate fail(s)", gate_fails));
        }
        if run_summary.runtime_qs_core_success_rate < 0.8 {
            parts.push(format!("runtime core success {:.0}%", run_summary.runtime_qs_core_success_rate * 100.0));
        }
        if parts.is_empty() {
            "structured queries are well-formed and grounded".to_string()
        } else if gate_fails > 0 && run_summary.runtime_qs_core_success_rate == 0.0 {
            format!("query structuring was the earliest semantic failure point: field-boundary gate failed in {} run(s) and runtime core success was 0%", gate_fails)
        } else {
            parts.join("; ")
        }
    };
    out.push_str(&format!(
        "| query structuring | judge {:.2}, no-hard-fail {:.0}%, runtime core {:.0}% | {} | {} |\n",
        run_summary.query_structuring_judge_score,
        run_summary.query_structuring_no_hard_fail_rate * 100.0,
        run_summary.runtime_qs_core_success_rate * 100.0,
        quality_status(qs_score), qs_interp
    ));

    // Retrieval
    let rt_interp = if rt_score >= 0.9 {
        "expected evidence found in all runs across all targets".to_string()
    } else if run_summary.runtime_retrieval_all_strict_recall_success_rate == 0.0
           && run_summary.runtime_retrieval_all_soft_recall_success_rate == 0.0 {
        format!("both recall and ranking were weak (nDCG {:.2})",
            run_summary.runtime_retrieval_mean_ndcg)
    } else if run_summary.runtime_retrieval_zero_hit_rate > 0.0 {
        format!("zero-hit rate {:.0}% — some retrieval calls returned nothing",
            run_summary.runtime_retrieval_zero_hit_rate * 100.0)
    } else if run_summary.runtime_retrieval_all_strict_recall_success_rate > 0.0 {
        format!("recall was present, but ranking quality remained weak (nDCG {:.2})",
            run_summary.runtime_retrieval_mean_ndcg)
    } else {
        format!("both recall and ranking were weak (nDCG {:.2})",
            run_summary.runtime_retrieval_mean_ndcg)
    };
    out.push_str(&format!(
        "| retrieval | strict recall {:.0}%, nDCG {:.2} | {} | {} |\n",
        run_summary.runtime_retrieval_all_strict_recall_success_rate * 100.0,
        run_summary.runtime_retrieval_mean_ndcg,
        quality_status(rt_score), rt_interp
    ));

    // Evidence packing
    let ep_interp = {
        let ep2_fails = iteration_rows.iter().filter(|r| r.evidence_pack_sufficiency_score == 0).count();
        let ep1_fails = iteration_rows.iter().filter(|r| r.evidence_pack_role_fit_score == 0).count();
        if ep_score >= 0.75 {
            "selected evidence pack was sufficient and mostly role-appropriate".to_string()
        } else if ep2_fails > 0 {
            format!("{} run(s) with insufficient evidence pack (EP2=0)", ep2_fails)
        } else {
            format!("{} run(s) with role-fit issues (EP1=0)", ep1_fails)
        }
    };
    out.push_str(&format!(
        "| evidence packing | judge {:.2}, no-hard-fail {:.0}% | {} | {} |\n",
        run_summary.evidence_pack_judge_score,
        run_summary.evidence_pack_no_hard_fail_rate * 100.0,
        quality_status(ep_score), ep_interp
    ));

    // Final answer (must come before continuation stage in table but we build interp first)
    let fa_interp = {
        let fa1_fails = iteration_rows.iter().filter(|r| r.final_no_root_cause_claim_score == 0).count();
        let fa2_fails = iteration_rows.iter().filter(|r| r.final_first_check_discriminates_score == 0).count();
        let fa3_weak = iteration_rows.iter().filter(|r| r.final_hypothesis_source_alignment_score < 2).count();
        let fa5_fails = iteration_rows.iter().filter(|r| r.final_result_interpretation_usefulness_score == 0).count();
        let mut parts = vec![];
        if fa1_fails > 0 { parts.push(format!("{} premature certainty (FA1=0)", fa1_fails)); }
        if fa2_fails > 0 { parts.push(format!("{} vague first check (FA2=0)", fa2_fails)); }
        if fa5_fails > 0 { parts.push(format!("{} weak interpretation (FA5=0)", fa5_fails)); }
        if fa3_weak > 0 && fa3_weak < n { parts.push(format!("{} partial source alignment (FA3<2)", fa3_weak)); }
        if parts.is_empty() {
            format!("{}% usable, uncertainty preserved, source labels mostly correct",
                (run_summary.usable_first_response_rate * 100.0).round())
        } else { parts.join("; ") }
    };
    out.push_str(&format!(
        "| final answer | usable {:.0}%, judge {:.2}, no-hard-fail {:.0}% | {} | {} |\n",
        run_summary.usable_first_response_rate * 100.0,
        run_summary.final_answer_judge_score,
        run_summary.final_answer_no_hard_fail_rate * 100.0,
        quality_status(fa_score), fa_interp
    ));

    if has_cont {
        let cont_interp = match cont_status {
            "strong" => "continuation preserved diagnostic quality after the new observation".to_string(),
            "mixed" => {
                let mut parts = vec![];
                if cont_usable_rate.unwrap_or(0.0) < 1.0 {
                    parts.push(format!("usable rate {:.0}%", cont_usable_rate.unwrap_or(0.0) * 100.0));
                }
                if cont_no_hard_fail.unwrap_or(0.0) < 1.0 {
                    parts.push(format!("no-hard-fail {:.0}%", cont_no_hard_fail.unwrap_or(0.0) * 100.0));
                }
                if parts.is_empty() { "some continuation iterations had quality issues".to_string() }
                else { format!("partial degradation: {}", parts.join(", ")) }
            }
            _ => {
                let cu4_fails = cont_rows.iter().filter(|r| r.continuation_observation_resolution_context_recovery_score == Some(0)).count();
                let cu1_fails = cont_rows.iter().filter(|r| r.continuation_hypothesis_update_discipline_score == Some(0)).count();
                if cu4_fails > 0 { format!("{} hard fail(s) on input reconstruction (CU4=0)", cu4_fails) }
                else if cu1_fails > 0 { format!("{} hard fail(s) on hypothesis update discipline (CU1=0)", cu1_fails) }
                else { "continuation quality was weak".to_string() }
            }
        };
        let cont_signals = format!(
            "usable cont {:.0}%, update score {}, input score {}, no-hard-fail {:.0}%",
            cont_usable_rate.unwrap_or(0.0) * 100.0,
            fmt_opt(cont_update_judge, 2),
            fmt_opt(cont_input_judge, 2),
            cont_no_hard_fail.unwrap_or(0.0) * 100.0,
        );
        out.push_str(&format!(
            "| continuation | {} | {} | {} |\n",
            cont_signals, cont_status, cont_interp
        ));
    }
    out.push('\n');

    // ── Failure Path ──────────────────────────────────────────────────────────
    out.push_str("### Failure Path\n\n");

    if unusable == 0 {
        out.push_str(&format!("All {} responses were usable. No hard failures at the final answer stage.\n\n", n));
        out.push_str("Soft weaknesses observed:\n\n");
        if run_summary.query_structuring_strict_pass_rate == 0.0 || qs_score < 0.75 {
            out.push_str(&format!(
                "- **Query structuring**: strict pass rate {:.0}%, no-hard-fail {:.0}%, runtime core {:.0}%\n",
                run_summary.query_structuring_strict_pass_rate * 100.0,
                run_summary.query_structuring_no_hard_fail_rate * 100.0,
                run_summary.runtime_qs_core_success_rate * 100.0,
            ));
        }
        if ep_score < 0.75 {
            out.push_str(&format!(
                "- **Evidence packing**: judge score {:.2}/2, no-hard-fail {:.0}%\n",
                run_summary.evidence_pack_judge_score,
                run_summary.evidence_pack_no_hard_fail_rate * 100.0,
            ));
        }
        if fa_score < 0.75 {
            out.push_str(&format!(
                "- **Final answer**: strict pass rate {:.0}%\n",
                run_summary.final_answer_strict_pass_rate * 100.0,
            ));
        }
    } else {
        let bad_query = (run_summary.bad_final_due_to_query_rate * denom).round() as usize;
        let bad_evidence = (run_summary.bad_final_due_to_evidence_rate * denom).round() as usize;
        let bad_both_ok = (run_summary.bad_final_with_good_query_and_evidence_rate * denom).round() as usize;

        out.push_str(&format!("{} of {} responses were unusable.\n\n", unusable, n));
        if bad_query > 0 {
            out.push_str(&format!("- {} unusable → **query structuring hard fail** (QS1=0 or QS2=0)\n", bad_query));
        }
        if bad_evidence > 0 {
            out.push_str(&format!("- {} unusable → **evidence packing hard fail** (EP1=0 or EP2=0)\n", bad_evidence));
        }
        if bad_both_ok > 0 {
            out.push_str(&format!("- {} unusable despite good query + evidence → **final answer stage failure**\n", bad_both_ok));
            let fa1_f = iteration_rows.iter().filter(|r| !r.usable_first_response && r.final_no_root_cause_claim_score == 0).count();
            let fa2_f = iteration_rows.iter().filter(|r| !r.usable_first_response && r.final_first_check_discriminates_score == 0).count();
            let fa5_f = iteration_rows.iter().filter(|r| !r.usable_first_response && r.final_result_interpretation_usefulness_score == 0).count();
            if fa1_f > 0 { out.push_str(&format!("  - {} × FA1=0: premature certainty or root cause claim\n", fa1_f)); }
            if fa2_f > 0 { out.push_str(&format!("  - {} × FA2=0: vague or non-discriminating first check\n", fa2_f)); }
            if fa5_f > 0 { out.push_str(&format!("  - {} × FA5=0: result interpretation missing or unusable\n", fa5_f)); }
        }
    }
    if has_cont {
        out.push('\n');
        let initial_usable = run_summary.usable_first_response_rate;
        let cont_usable = cont_usable_rate.unwrap_or(0.0);
        let trajectory = if cont_usable < initial_usable - 0.001 { "degraded" } else { "remained stable" };

        match cont_status {
            "strong" => {
                out.push_str("Continuation behavior was stable:\n\n");
                out.push_str(&format!("- usable continuation rate: {:.0}%\n", cont_usable * 100.0));
                out.push_str(&format!("- continuation update strict pass rate: {:.0}%\n", cont_update_strict.unwrap_or(0.0) * 100.0));
                out.push_str(&format!("- continuation input strict pass rate: {:.0}%\n", {
                    let s: Vec<bool> = cont_rows.iter().filter_map(|r| r.continuation_observation_resolution_context_recovery_score.map(|s| s == 2)).collect();
                    if s.is_empty() { 0.0 } else { s.iter().filter(|&&v| v).count() as f64 / s.len() as f64 * 100.0 }
                }));
                out.push_str(&format!("\nNo degradation was observed between initial and continuation iterations.\n"));
            }
            _ => {
                out.push_str("Continuation was the main observed degradation point:\n\n");
                let cu4_fails = cont_rows.iter().filter(|r| r.continuation_observation_resolution_context_recovery_score == Some(0)).count();
                let cu1_fails = cont_rows.iter().filter(|r| r.continuation_hypothesis_update_discipline_score == Some(0)).count();
                let cu2_fails = cont_rows.iter().filter(|r| r.continuation_problem_understanding_update_score == Some(0)).count();
                let cu3_fails = cont_rows.iter().filter(|r| r.continuation_next_check_progression_score == Some(0)).count();
                if cu4_fails > 0 { out.push_str(&format!("- {} continuation input reconstruction hard fail(s) (CU4=0)\n", cu4_fails)); }
                if cu1_fails > 0 { out.push_str(&format!("- {} hypothesis update discipline hard fail(s) (CU1=0)\n", cu1_fails)); }
                if cu2_fails > 0 { out.push_str(&format!("- {} problem understanding update hard fail(s) (CU2=0)\n", cu2_fails)); }
                if cu3_fails > 0 { out.push_str(&format!("- {} next check progression hard fail(s) (CU3=0)\n", cu3_fails)); }
                let closing = if trajectory == "degraded" {
                    "Quality degraded between initial and continuation iterations."
                } else {
                    "Quality remained stable between initial and continuation iterations."
                };
                out.push_str(&format!("\n{}\n", closing));
            }
        }
    }
    out.push('\n');

    // ── Conclusion ────────────────────────────────────────────────────────────
    let stages = [("query structuring", qs_score), ("retrieval", rt_score),
                  ("evidence packing", ep_score), ("final answer", fa_score)];
    let weakest = stages.iter().min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let all_strong = stages.iter().all(|(_, s)| *s >= 0.75);

    let conclusion = if all_strong {
        "All pipeline stages performed well in this run.".to_string()
    } else if let Some((name, score)) = weakest {
        let other_strong: Vec<&str> = stages.iter()
            .filter(|(n, s)| *n != *name && *s >= 0.75)
            .map(|(n, _)| *n)
            .collect();
        let others = if other_strong.is_empty() {
            String::new()
        } else if other_strong.len() == 1 {
            format!(" {} quality was strong.", other_strong[0])
        } else {
            format!(" {} quality was strong.", other_strong.join(" and "))
        };
        format!("Main observed weakness: **{}** (composite {:.2}).{}",
            name, score, others)
    } else {
        String::new()
    };
    out.push_str(&format!("{}\n\n", conclusion));

    out
}

fn score_dist(rows: &[EvalIterationSummaryRow], get: impl Fn(&EvalIterationSummaryRow) -> i16) -> (usize, usize, usize) {
    let s0 = rows.iter().filter(|r| get(r) == 0).count();
    let s1 = rows.iter().filter(|r| get(r) == 1).count();
    let s2 = rows.iter().filter(|r| get(r) == 2).count();
    (s0, s1, s2)
}

fn score_dist_opt(rows: &[EvalIterationSummaryRow], get: impl Fn(&EvalIterationSummaryRow) -> Option<i16>) -> (usize, usize, usize) {
    let s0 = rows.iter().filter(|r| get(r) == Some(0)).count();
    let s1 = rows.iter().filter(|r| get(r) == Some(1)).count();
    let s2 = rows.iter().filter(|r| get(r) == Some(2)).count();
    (s0, s1, s2)
}

fn gate_fail_opt(rows: &[EvalIterationSummaryRow], get: impl Fn(&EvalIterationSummaryRow) -> Option<i16>) -> (usize, f64) {
    let applicable: Vec<i16> = rows.iter().filter_map(|r| get(r)).collect();
    let fail = applicable.iter().filter(|&&s| s == 0).count();
    let rate = if applicable.is_empty() { 0.0 } else { fail as f64 / applicable.len() as f64 };
    (fail, rate)
}

fn gate_fail(rows: &[EvalIterationSummaryRow], get: impl Fn(&EvalIterationSummaryRow) -> bool) -> (usize, f64) {
    let fail = rows.iter().filter(|r| !get(r)).count();
    let rate = if rows.is_empty() { 0.0 } else { fail as f64 / rows.len() as f64 };
    (fail, rate)
}

fn avg_rt_field<F>(rows: &[EvalIterationSummaryRow], get_json: F, field: &str) -> Option<f64>
where
    F: Fn(&EvalIterationSummaryRow) -> Option<&serde_json::Value>,
{
    let vals: Vec<f64> = rows.iter()
        .filter_map(|r| get_json(r))
        .filter_map(|v| v.get(field)?.as_f64())
        .collect();
    if vals.is_empty() { None } else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
}

fn fmt_opt(v: Option<f64>, decimals: usize) -> String {
    match v {
        Some(x) => format!("{:.prec$}", x, prec = decimals),
        None => "—".to_string(),
    }
}

fn json_as_f64(v: &serde_json::Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_bool().map(|b| if b { 1.0 } else { 0.0 }))
}

fn avg_qs_vocab(rows: &[EvalIterationSummaryRow], vocab_field: &str, metric: &str) -> Option<f64> {
    let vals: Vec<f64> = rows.iter().filter_map(|r| {
        json_as_f64(r.runtime_qs_metrics.as_ref()?.get("vocab_fields")?.get(vocab_field)?.get(metric)?)
    }).collect();
    if vals.is_empty() { None } else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
}

fn avg_qs_non_vocab(rows: &[EvalIterationSummaryRow], metric: &str) -> Option<f64> {
    let vals: Vec<f64> = rows.iter().filter_map(|r| {
        json_as_f64(r.runtime_qs_metrics.as_ref()?.get("non_vocab_fields")?.get(metric)?)
    }).collect();
    if vals.is_empty() { None } else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
}

fn iter_frac<F: Fn(&EvalIterationSummaryRow) -> bool>(
    rows: &[&EvalIterationSummaryRow], f: F,
) -> Option<f64> {
    if rows.is_empty() { return None; }
    Some(rows.iter().filter(|r| f(r)).count() as f64 / rows.len() as f64)
}

fn iter_mean<F: Fn(&EvalIterationSummaryRow) -> f64>(
    rows: &[&EvalIterationSummaryRow], f: F,
) -> Option<f64> {
    if rows.is_empty() { return None; }
    Some(rows.iter().map(|r| f(r)).sum::<f64>() / rows.len() as f64)
}

fn iter_opt_score<F: Fn(&EvalIterationSummaryRow) -> Option<i16>>(
    rows: &[&EvalIterationSummaryRow], f: F,
) -> Option<f64> {
    let vals: Vec<f64> = rows.iter().filter_map(|r| f(r).map(|s| s as f64)).collect();
    if vals.is_empty() { None } else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
}

fn iter_opt_frac<F: Fn(&EvalIterationSummaryRow) -> Option<bool>>(
    rows: &[&EvalIterationSummaryRow], f: F,
) -> Option<f64> {
    let applicable: Vec<bool> = rows.iter().filter_map(|r| f(r)).collect();
    if applicable.is_empty() { None }
    else { Some(applicable.iter().filter(|&&v| v).count() as f64 / applicable.len() as f64) }
}

fn judge_metrics_table(initial: &[&EvalIterationSummaryRow], cont: &[&EvalIterationSummaryRow]) -> String {
    let all: Vec<&EvalIterationSummaryRow> = initial.iter().chain(cont.iter()).copied().collect();
    let mut out = String::new();

    out.push_str("| metric | initial iter-s | continuation iter-s | total | formula |\n|---|---:|---:|---:|---|\n");

    // Initial-only: usable_first_response is a first-response metric, not meaningful for continuations
    out.push_str(&format!("| usable_first_response_rate | {} | n/a | {} | frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |\n",
        fmt_opt(iter_frac(initial, |r| r.usable_first_response), 4),
        fmt_opt(iter_frac(initial, |r| r.usable_first_response), 4)));

    // Initial-only
    out.push_str(&format!("| query_structuring_judge_score | {} | n/a | {} | mean of avg(QS1, QS2) over initial iter-s |\n",
        fmt_opt(iter_mean(initial, |r| r.query_structuring_judge_score), 4),
        fmt_opt(iter_mean(initial, |r| r.query_structuring_judge_score), 4)));

    // Initial-only
    out.push_str(&format!("| evidence_pack_judge_score | {} | n/a | {} | mean of avg(EP1, EP2) over initial iter-s |\n",
        fmt_opt(iter_mean(initial, |r| r.evidence_pack_judge_score), 4),
        fmt_opt(iter_mean(initial, |r| r.evidence_pack_judge_score), 4)));

    // Shared
    out.push_str(&format!("| final_answer_judge_score | {} | {} | {} | mean of avg(FA1, FA2, FA3, FA4, FA5) |\n",
        fmt_opt(iter_mean(initial, |r| r.final_answer_judge_score), 4),
        fmt_opt(iter_mean(cont, |r| r.final_answer_judge_score), 4),
        fmt_opt(iter_mean(&all, |r| r.final_answer_judge_score), 4)));

    // Initial-only
    out.push_str(&format!("| query_structuring_no_hard_fail_rate | {} | n/a | {} | frac(QS1>0 ∧ QS2>0) |\n",
        fmt_opt(iter_frac(initial, |r| r.query_structuring_no_hard_fail), 4),
        fmt_opt(iter_frac(initial, |r| r.query_structuring_no_hard_fail), 4)));

    // Initial-only
    out.push_str(&format!("| evidence_pack_no_hard_fail_rate | {} | n/a | {} | frac(EP1>0 ∧ EP2>0) |\n",
        fmt_opt(iter_frac(initial, |r| r.evidence_pack_no_hard_fail), 4),
        fmt_opt(iter_frac(initial, |r| r.evidence_pack_no_hard_fail), 4)));

    // Shared
    out.push_str(&format!("| final_answer_no_hard_fail_rate | {} | {} | {} | frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0) |\n",
        fmt_opt(iter_frac(initial, |r| r.final_answer_no_hard_fail), 4),
        fmt_opt(iter_frac(cont, |r| r.final_answer_no_hard_fail), 4),
        fmt_opt(iter_frac(&all, |r| r.final_answer_no_hard_fail), 4)));

    // Shared
    out.push_str(&format!("| diagnostic_move_hard_fail_rate | {} | {} | {} | 1 − final_answer_no_hard_fail_rate |\n",
        fmt_opt(iter_frac(initial, |r| !r.final_answer_no_hard_fail), 4),
        fmt_opt(iter_frac(cont, |r| !r.final_answer_no_hard_fail), 4),
        fmt_opt(iter_frac(&all, |r| !r.final_answer_no_hard_fail), 4)));

    // Initial-only
    out.push_str(&format!("| query_structuring_strict_pass_rate | {} | n/a | {} | frac(QS1=2 ∧ QS2=2) |\n",
        fmt_opt(iter_frac(initial, |r| {
            r.query_structuring_field_boundary_correctness_score == 2
            && r.query_structuring_grounding_conservatism_score == 2
        }), 4),
        fmt_opt(iter_frac(initial, |r| {
            r.query_structuring_field_boundary_correctness_score == 2
            && r.query_structuring_grounding_conservatism_score == 2
        }), 4)));

    // Initial-only
    out.push_str(&format!("| evidence_pack_strict_pass_rate | {} | n/a | {} | frac(EP1=2 ∧ EP2=2) |\n",
        fmt_opt(iter_frac(initial, |r| {
            r.evidence_pack_role_fit_score == 2 && r.evidence_pack_sufficiency_score == 2
        }), 4),
        fmt_opt(iter_frac(initial, |r| {
            r.evidence_pack_role_fit_score == 2 && r.evidence_pack_sufficiency_score == 2
        }), 4)));

    let fa_strict = |r: &EvalIterationSummaryRow| {
        r.final_no_root_cause_claim_score == 2
        && r.final_first_check_discriminates_score == 2
        && r.final_hypothesis_source_alignment_score == 2
        && r.final_alternative_context_handling_score == 2
        && r.final_result_interpretation_usefulness_score == 2
    };
    // Shared
    out.push_str(&format!("| final_answer_strict_pass_rate | {} | {} | {} | frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2) |\n",
        fmt_opt(iter_frac(initial, fa_strict), 4),
        fmt_opt(iter_frac(cont, fa_strict), 4),
        fmt_opt(iter_frac(&all, fa_strict), 4)));

    // Continuation-only
    out.push_str(&format!("| continuation_hypothesis_update_discipline_score | n/a | {} | {} | mean(CU1) over continuation iter-s |\n",
        fmt_opt(iter_opt_score(cont, |r| r.continuation_hypothesis_update_discipline_score), 4),
        fmt_opt(iter_opt_score(cont, |r| r.continuation_hypothesis_update_discipline_score), 4)));

    out.push_str(&format!("| continuation_problem_understanding_update_score | n/a | {} | {} | mean(CU2) over continuation iter-s |\n",
        fmt_opt(iter_opt_score(cont, |r| r.continuation_problem_understanding_update_score), 4),
        fmt_opt(iter_opt_score(cont, |r| r.continuation_problem_understanding_update_score), 4)));

    out.push_str(&format!("| continuation_next_check_progression_score | n/a | {} | {} | mean(CU3) over continuation iter-s |\n",
        fmt_opt(iter_opt_score(cont, |r| r.continuation_next_check_progression_score), 4),
        fmt_opt(iter_opt_score(cont, |r| r.continuation_next_check_progression_score), 4)));

    out.push_str(&format!("| continuation_observation_resolution_context_recovery_score | n/a | {} | {} | mean(CU4) over continuation iter-s |\n",
        fmt_opt(iter_opt_score(cont, |r| r.continuation_observation_resolution_context_recovery_score), 4),
        fmt_opt(iter_opt_score(cont, |r| r.continuation_observation_resolution_context_recovery_score), 4)));

    out.push_str(&format!("| usable_continuation_response_rate | n/a | {} | {} | frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1) |\n",
        fmt_opt(iter_opt_frac(cont, |r| r.usable_continuation_response), 4),
        fmt_opt(iter_opt_frac(cont, |r| r.usable_continuation_response), 4)));

    let cu_update_judge = |rows: &[&EvalIterationSummaryRow]| -> Option<f64> {
        let vals: Vec<f64> = rows.iter().filter_map(|r| {
            r.continuation_hypothesis_update_discipline_score
                .zip(r.continuation_problem_understanding_update_score)
                .zip(r.continuation_next_check_progression_score)
                .map(|((c1, c2), c3)| (c1 as f64 + c2 as f64 + c3 as f64) / 3.0)
        }).collect();
        if vals.is_empty() { None } else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
    };
    out.push_str(&format!("| continuation_update_judge_score | n/a | {} | {} | mean of avg(CU1, CU2, CU3) over continuation iter-s |\n",
        fmt_opt(cu_update_judge(cont), 4),
        fmt_opt(cu_update_judge(cont), 4)));

    out.push_str(&format!("| continuation_update_no_hard_fail_rate | n/a | {} | {} | frac(CU1>0 ∧ CU2>0 ∧ CU3>0) |\n",
        fmt_opt(iter_opt_frac(cont, |r| r.continuation_update_no_hard_fail), 4),
        fmt_opt(iter_opt_frac(cont, |r| r.continuation_update_no_hard_fail), 4)));

    let cu_update_strict_pass = |rows: &[&EvalIterationSummaryRow]| -> Option<f64> {
        let applicable: Vec<bool> = rows.iter().filter_map(|r| {
            r.continuation_hypothesis_update_discipline_score
                .zip(r.continuation_problem_understanding_update_score)
                .zip(r.continuation_next_check_progression_score)
                .map(|((c1, c2), c3)| c1 == 2 && c2 == 2 && c3 == 2)
        }).collect();
        if applicable.is_empty() { None }
        else { Some(applicable.iter().filter(|&&v| v).count() as f64 / applicable.len() as f64) }
    };
    out.push_str(&format!("| continuation_update_strict_pass_rate | n/a | {} | {} | frac(CU1=2 ∧ CU2=2 ∧ CU3=2) |\n",
        fmt_opt(cu_update_strict_pass(cont), 4),
        fmt_opt(cu_update_strict_pass(cont), 4)));

    out.push_str(&format!("| continuation_input_judge_score | n/a | {} | {} | mean(CU4) over continuation iter-s |\n",
        fmt_opt(iter_opt_score(cont, |r| r.continuation_observation_resolution_context_recovery_score), 4),
        fmt_opt(iter_opt_score(cont, |r| r.continuation_observation_resolution_context_recovery_score), 4)));

    out.push_str(&format!("| continuation_input_no_hard_fail_rate | n/a | {} | {} | frac(CU4>0) |\n",
        fmt_opt(iter_opt_frac(cont, |r| r.continuation_input_no_hard_fail), 4),
        fmt_opt(iter_opt_frac(cont, |r| r.continuation_input_no_hard_fail), 4)));

    let cu_input_strict_pass = |rows: &[&EvalIterationSummaryRow]| -> Option<f64> {
        let applicable: Vec<bool> = rows.iter()
            .filter_map(|r| r.continuation_observation_resolution_context_recovery_score.map(|s| s == 2))
            .collect();
        if applicable.is_empty() { None }
        else { Some(applicable.iter().filter(|&&v| v).count() as f64 / applicable.len() as f64) }
    };
    out.push_str(&format!("| continuation_input_strict_pass_rate | n/a | {} | {} | frac(CU4=2) |\n",
        fmt_opt(cu_input_strict_pass(cont), 4),
        fmt_opt(cu_input_strict_pass(cont), 4)));

    out
}

fn render_judge_aggregated_metrics(
    iteration_rows: &[EvalIterationSummaryRow],
) -> String {
    let initial: Vec<&EvalIterationSummaryRow> = iteration_rows.iter()
        .filter(|r| r.iteration_kind == "initial")
        .collect();
    let cont: Vec<&EvalIterationSummaryRow> = iteration_rows.iter()
        .filter(|r| r.iteration_kind == "continuation")
        .collect();

    let mut out = String::new();
    out.push_str("## Judge-Based Aggregated Metrics\n\n");
    out.push_str("> initial iter-s = all initial iterations across runs; continuation iter-s = all continuation iterations across runs; total ignores n/a\n\n");
    out.push_str(&judge_metrics_table(&initial, &cont));
    out.push('\n');
    out.push_str(&code_legend());
    out.push_str(suite_overview_link_note());
    out.push('\n');
    out
}

fn render_judge_per_run_appendix(
    iteration_rows: &[EvalIterationSummaryRow],
    appendix_label: &str,
) -> String {
    let mut by_run: BTreeMap<String, Vec<&EvalIterationSummaryRow>> = BTreeMap::new();
    for row in iteration_rows {
        by_run.entry(row.key.runtime_run_id.to_string()).or_default().push(row);
    }

    let mut out = String::new();
    out.push_str(&format!("## {}: Judge Metrics Per Run\n\n", appendix_label));

    for (run_id, rows) in &by_run {
        // Sort: initial first, then continuations by iteration_id for stable order
        let mut iters: Vec<&EvalIterationSummaryRow> = rows.to_vec();
        iters.sort_by(|a, b| {
            let ak = if a.iteration_kind == "initial" { 0i32 } else { 1 };
            let bk = if b.iteration_kind == "initial" { 0i32 } else { 1 };
            ak.cmp(&bk).then(a.key.iteration_id.cmp(&b.key.iteration_id))
        });

        let initial: Vec<&EvalIterationSummaryRow> = iters.iter().filter(|r| r.iteration_kind == "initial").copied().collect();
        let cont: Vec<&EvalIterationSummaryRow> = iters.iter().filter(|r| r.iteration_kind == "continuation").copied().collect();
        let all: Vec<&EvalIterationSummaryRow> = iters.iter().copied().collect();

        out.push_str(&format!("### Run `{}`\n\n", run_id));

        // Header: one column per iteration (1-based) + total + formula
        let mut header = "| metric".to_string();
        let mut sep = "|---".to_string();
        for i in 1..=iters.len() {
            header.push_str(&format!(" | iter_{}", i));
            sep.push_str(" |---:");
        }
        header.push_str(" | total | formula |");
        sep.push_str(" |---:|---|");
        out.push_str(&header);
        out.push_str("\n");
        out.push_str(&sep);
        out.push_str("\n");

        let ic = |r: &&EvalIterationSummaryRow| r.iteration_kind == "continuation";
        let b = |v: bool| if v { "1".to_string() } else { "0".to_string() };
        let oi16 = |v: Option<i16>| v.map(|s| s.to_string()).unwrap_or_else(|| "n/a".to_string());
        let obool = |v: Option<bool>| match v { Some(true) => "1".to_string(), Some(false) => "0".to_string(), None => "n/a".to_string() };

        macro_rules! mrow {
            ($name:expr, $cells:expr, $total:expr, $formula:expr) => {{
                let mut line = format!("| {}", $name);
                for c in $cells { line.push_str(&format!(" | {}", c)); }
                line.push_str(&format!(" | {} | {} |\n", $total, $formula));
                out.push_str(&line);
            }};
        }

        // Shared
        mrow!("usable_first_response_rate",
            iters.iter().map(|r| b(r.usable_first_response)).collect::<Vec<_>>(),
            fmt_opt(iter_frac(&all, |r| r.usable_first_response), 4),
            "frac(FA1≥1 ∧ FA2≥1 ∧ FA5≥1)");
        // Initial-only
        mrow!("query_structuring_judge_score",
            iters.iter().map(|r| if ic(&r) { "n/a".to_string() } else { format!("{:.4}", r.query_structuring_judge_score) }).collect::<Vec<_>>(),
            fmt_opt(iter_mean(&initial, |r| r.query_structuring_judge_score), 4),
            "mean of avg(QS1, QS2) over initial iter-s");
        mrow!("evidence_pack_judge_score",
            iters.iter().map(|r| if ic(&r) { "n/a".to_string() } else { format!("{:.4}", r.evidence_pack_judge_score) }).collect::<Vec<_>>(),
            fmt_opt(iter_mean(&initial, |r| r.evidence_pack_judge_score), 4),
            "mean of avg(EP1, EP2) over initial iter-s");
        // Shared
        mrow!("final_answer_judge_score",
            iters.iter().map(|r| format!("{:.4}", r.final_answer_judge_score)).collect::<Vec<_>>(),
            fmt_opt(iter_mean(&all, |r| r.final_answer_judge_score), 4),
            "mean of avg(FA1, FA2, FA3, FA4, FA5)");
        // Initial-only
        mrow!("query_structuring_no_hard_fail_rate",
            iters.iter().map(|r| if ic(&r) { "n/a".to_string() } else { b(r.query_structuring_no_hard_fail) }).collect::<Vec<_>>(),
            fmt_opt(iter_frac(&initial, |r| r.query_structuring_no_hard_fail), 4),
            "frac(QS1>0 ∧ QS2>0)");
        mrow!("evidence_pack_no_hard_fail_rate",
            iters.iter().map(|r| if ic(&r) { "n/a".to_string() } else { b(r.evidence_pack_no_hard_fail) }).collect::<Vec<_>>(),
            fmt_opt(iter_frac(&initial, |r| r.evidence_pack_no_hard_fail), 4),
            "frac(EP1>0 ∧ EP2>0)");
        // Shared
        mrow!("final_answer_no_hard_fail_rate",
            iters.iter().map(|r| b(r.final_answer_no_hard_fail)).collect::<Vec<_>>(),
            fmt_opt(iter_frac(&all, |r| r.final_answer_no_hard_fail), 4),
            "frac(FA1>0 ∧ FA2>0 ∧ FA4>0 ∧ FA5>0)");
        mrow!("diagnostic_move_hard_fail_rate",
            iters.iter().map(|r| b(!r.final_answer_no_hard_fail)).collect::<Vec<_>>(),
            fmt_opt(iter_frac(&all, |r| !r.final_answer_no_hard_fail), 4),
            "1 − final_answer_no_hard_fail_rate");
        // Initial-only
        mrow!("query_structuring_strict_pass_rate",
            iters.iter().map(|r| if ic(&r) { "n/a".to_string() } else {
                b(r.query_structuring_field_boundary_correctness_score == 2
                  && r.query_structuring_grounding_conservatism_score == 2)
            }).collect::<Vec<_>>(),
            fmt_opt(iter_frac(&initial, |r| {
                r.query_structuring_field_boundary_correctness_score == 2
                && r.query_structuring_grounding_conservatism_score == 2
            }), 4),
            "frac(QS1=2 ∧ QS2=2)");
        mrow!("evidence_pack_strict_pass_rate",
            iters.iter().map(|r| if ic(&r) { "n/a".to_string() } else {
                b(r.evidence_pack_role_fit_score == 2 && r.evidence_pack_sufficiency_score == 2)
            }).collect::<Vec<_>>(),
            fmt_opt(iter_frac(&initial, |r| {
                r.evidence_pack_role_fit_score == 2 && r.evidence_pack_sufficiency_score == 2
            }), 4),
            "frac(EP1=2 ∧ EP2=2)");
        // Shared
        let fa_sp = |r: &EvalIterationSummaryRow| {
            r.final_no_root_cause_claim_score == 2
            && r.final_first_check_discriminates_score == 2
            && r.final_hypothesis_source_alignment_score == 2
            && r.final_alternative_context_handling_score == 2
            && r.final_result_interpretation_usefulness_score == 2
        };
        mrow!("final_answer_strict_pass_rate",
            iters.iter().map(|r| b(fa_sp(r))).collect::<Vec<_>>(),
            fmt_opt(iter_frac(&all, fa_sp), 4),
            "frac(FA1=2 ∧ FA2=2 ∧ FA3=2 ∧ FA4=2 ∧ FA5=2)");
        // Continuation-only
        mrow!("continuation_hypothesis_update_discipline_score",
            iters.iter().map(|r| if !ic(&r) { "n/a".to_string() } else { oi16(r.continuation_hypothesis_update_discipline_score) }).collect::<Vec<_>>(),
            fmt_opt(iter_opt_score(&cont, |r| r.continuation_hypothesis_update_discipline_score), 4),
            "mean(CU1) over continuation iter-s");
        mrow!("continuation_problem_understanding_update_score",
            iters.iter().map(|r| if !ic(&r) { "n/a".to_string() } else { oi16(r.continuation_problem_understanding_update_score) }).collect::<Vec<_>>(),
            fmt_opt(iter_opt_score(&cont, |r| r.continuation_problem_understanding_update_score), 4),
            "mean(CU2) over continuation iter-s");
        mrow!("continuation_next_check_progression_score",
            iters.iter().map(|r| if !ic(&r) { "n/a".to_string() } else { oi16(r.continuation_next_check_progression_score) }).collect::<Vec<_>>(),
            fmt_opt(iter_opt_score(&cont, |r| r.continuation_next_check_progression_score), 4),
            "mean(CU3) over continuation iter-s");
        mrow!("continuation_observation_resolution_context_recovery_score",
            iters.iter().map(|r| if !ic(&r) { "n/a".to_string() } else { oi16(r.continuation_observation_resolution_context_recovery_score) }).collect::<Vec<_>>(),
            fmt_opt(iter_opt_score(&cont, |r| r.continuation_observation_resolution_context_recovery_score), 4),
            "mean(CU4) over continuation iter-s");
        mrow!("usable_continuation_response_rate",
            iters.iter().map(|r| if !ic(&r) { "n/a".to_string() } else { obool(r.usable_continuation_response) }).collect::<Vec<_>>(),
            fmt_opt(iter_opt_frac(&cont, |r| r.usable_continuation_response), 4),
            "frac(CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1)");
        let cu_upd_judge = |rows: &[&EvalIterationSummaryRow]| -> Option<f64> {
            let vals: Vec<f64> = rows.iter().filter_map(|r| {
                r.continuation_hypothesis_update_discipline_score
                    .zip(r.continuation_problem_understanding_update_score)
                    .zip(r.continuation_next_check_progression_score)
                    .map(|((c1, c2), c3)| (c1 as f64 + c2 as f64 + c3 as f64) / 3.0)
            }).collect();
            if vals.is_empty() { None } else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
        };
        mrow!("continuation_update_judge_score",
            iters.iter().map(|r| if !ic(&r) { "n/a".to_string() } else {
                match r.continuation_hypothesis_update_discipline_score
                    .zip(r.continuation_problem_understanding_update_score)
                    .zip(r.continuation_next_check_progression_score)
                {
                    Some(((c1, c2), c3)) => format!("{:.4}", (c1 as f64 + c2 as f64 + c3 as f64) / 3.0),
                    None => "n/a".to_string(),
                }
            }).collect::<Vec<_>>(),
            fmt_opt(cu_upd_judge(&cont), 4),
            "mean of avg(CU1, CU2, CU3) over continuation iter-s");
        mrow!("continuation_update_no_hard_fail_rate",
            iters.iter().map(|r| if !ic(&r) { "n/a".to_string() } else { obool(r.continuation_update_no_hard_fail) }).collect::<Vec<_>>(),
            fmt_opt(iter_opt_frac(&cont, |r| r.continuation_update_no_hard_fail), 4),
            "frac(CU1>0 ∧ CU2>0 ∧ CU3>0)");
        let cu_upd_sp = |rows: &[&EvalIterationSummaryRow]| -> Option<f64> {
            let applicable: Vec<bool> = rows.iter().filter_map(|r| {
                r.continuation_hypothesis_update_discipline_score
                    .zip(r.continuation_problem_understanding_update_score)
                    .zip(r.continuation_next_check_progression_score)
                    .map(|((c1, c2), c3)| c1 == 2 && c2 == 2 && c3 == 2)
            }).collect();
            if applicable.is_empty() { None }
            else { Some(applicable.iter().filter(|&&v| v).count() as f64 / applicable.len() as f64) }
        };
        mrow!("continuation_update_strict_pass_rate",
            iters.iter().map(|r| if !ic(&r) { "n/a".to_string() } else {
                match r.continuation_hypothesis_update_discipline_score
                    .zip(r.continuation_problem_understanding_update_score)
                    .zip(r.continuation_next_check_progression_score)
                {
                    Some(((c1, c2), c3)) => b(c1 == 2 && c2 == 2 && c3 == 2),
                    None => "n/a".to_string(),
                }
            }).collect::<Vec<_>>(),
            fmt_opt(cu_upd_sp(&cont), 4),
            "frac(CU1=2 ∧ CU2=2 ∧ CU3=2)");
        mrow!("continuation_input_judge_score",
            iters.iter().map(|r| if !ic(&r) { "n/a".to_string() } else { oi16(r.continuation_observation_resolution_context_recovery_score) }).collect::<Vec<_>>(),
            fmt_opt(iter_opt_score(&cont, |r| r.continuation_observation_resolution_context_recovery_score), 4),
            "mean(CU4) over continuation iter-s");
        mrow!("continuation_input_no_hard_fail_rate",
            iters.iter().map(|r| if !ic(&r) { "n/a".to_string() } else { obool(r.continuation_input_no_hard_fail) }).collect::<Vec<_>>(),
            fmt_opt(iter_opt_frac(&cont, |r| r.continuation_input_no_hard_fail), 4),
            "frac(CU4>0)");
        let cu_inp_sp = |rows: &[&EvalIterationSummaryRow]| -> Option<f64> {
            let applicable: Vec<bool> = rows.iter()
                .filter_map(|r| r.continuation_observation_resolution_context_recovery_score.map(|s| s == 2))
                .collect();
            if applicable.is_empty() { None }
            else { Some(applicable.iter().filter(|&&v| v).count() as f64 / applicable.len() as f64) }
        };
        mrow!("continuation_input_strict_pass_rate",
            iters.iter().map(|r| if !ic(&r) { "n/a".to_string() } else {
                match r.continuation_observation_resolution_context_recovery_score {
                    Some(s) => b(s == 2),
                    None => "n/a".to_string(),
                }
            }).collect::<Vec<_>>(),
            fmt_opt(cu_inp_sp(&cont), 4),
            "frac(CU4=2)");

        out.push('\n');
    }

    out.push_str(&code_legend());
    out.push_str(suite_overview_link_note());
    out.push('\n');
    out
}

fn avg_rt_target(rows: &[EvalIterationSummaryRow], get_metrics: fn(&EvalIterationSummaryRow) -> Option<&serde_json::Value>, metric: &str) -> Option<f64> {
    let vals: Vec<f64> = rows.iter()
        .filter_map(|r| get_metrics(r)?.get(metric)?.as_f64())
        .collect();
    if vals.is_empty() { None } else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
}

#[derive(Debug, Clone)]
struct RuntimeStageReportRow {
    scope: &'static str,
    model: String,
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
    input_cost_per_million_tokens: f64,
    output_cost_per_million_tokens: f64,
    total_cost_usd: f64,
}

#[derive(Debug, Clone)]
struct JudgeSuiteReportRow {
    suite: String,
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
    input_cost_per_million_tokens: f64,
    output_cost_per_million_tokens: f64,
    prompt_cost_usd: f64,
    completion_cost_usd: f64,
    total_cost_usd: f64,
}

#[derive(Debug, Clone)]
struct ModelCostReportRow {
    model: String,
    prompt_tokens: i64,
    completion_tokens: i64,
    input_cost_per_million_tokens: f64,
    output_cost_per_million_tokens: f64,
    prompt_cost_usd: f64,
    completion_cost_usd: f64,
    total_cost_usd: f64,
}

fn format_cost_cell(tokens: i64, cost_per_million: f64, total_cost_usd: f64) -> String {
    format!(
        "{} * ${}/1M = {:.6}",
        tokens, format_price_per_million(cost_per_million), total_cost_usd
    )
}

fn format_price_per_million(cost_per_million: f64) -> String {
    let mut s = format!("{:.2}", cost_per_million);
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.push('0');
    }
    s
}

fn render_run_report(
    manifest: &EvalRunManifest,
    run_summary: &EvalRunSummaryRow,
    iteration_rows: &[EvalIterationSummaryRow],
    judge_calls: &[JudgeLlmCallRow],
    catalog: &JudgeSuiteCatalog,
    enabled_suites: &[String],
) -> String {
    let has_cont = iteration_rows.iter().any(|r| r.iteration_kind == "continuation");
    let cont_rows: Vec<&EvalIterationSummaryRow> = iteration_rows.iter()
        .filter(|r| r.iteration_kind == "continuation")
        .collect();

    let mut worst_cases: Vec<&EvalIterationSummaryRow> = iteration_rows.iter().collect();
    worst_cases.sort_by(|a, b| {
        a.final_answer_judge_score
            .partial_cmp(&b.final_answer_judge_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let worst_cases: Vec<_> = worst_cases.into_iter().take(5).collect();
    let first_iteration = iteration_rows.first();
    let first_judge_call = judge_calls.first();

    let runtime_stage_rows = vec![
        RuntimeStageReportRow {
            scope: "query_structuring",
            model: run_summary.runtime_query_structuring_model.clone(),
            prompt_tokens: iteration_rows.iter().map(|row| row.runtime_query_structuring_prompt_tokens).sum(),
            completion_tokens: iteration_rows.iter().map(|row| row.runtime_query_structuring_completion_tokens).sum(),
            total_tokens: iteration_rows.iter().map(|row| row.runtime_query_structuring_tokens).sum(),
            input_cost_per_million_tokens: first_iteration
                .map(|row| row.runtime_query_structuring_input_cost_per_million_tokens)
                .unwrap_or(0.0),
            output_cost_per_million_tokens: first_iteration
                .map(|row| row.runtime_query_structuring_output_cost_per_million_tokens)
                .unwrap_or(0.0),
            total_cost_usd: iteration_rows.iter().map(|row| row.runtime_query_structuring_cost_usd).sum(),
        },
        RuntimeStageReportRow {
            scope: "observation_boundary_resolver",
            model: run_summary.runtime_observation_boundary_resolver_model.clone(),
            prompt_tokens: iteration_rows
                .iter()
                .map(|row| row.runtime_observation_boundary_resolver_prompt_tokens)
                .sum(),
            completion_tokens: iteration_rows
                .iter()
                .map(|row| row.runtime_observation_boundary_resolver_completion_tokens)
                .sum(),
            total_tokens: iteration_rows
                .iter()
                .map(|row| row.runtime_observation_boundary_resolver_tokens)
                .sum(),
            input_cost_per_million_tokens: first_iteration
                .map(|row| row.runtime_observation_boundary_resolver_input_cost_per_million_tokens)
                .unwrap_or(0.0),
            output_cost_per_million_tokens: first_iteration
                .map(|row| row.runtime_observation_boundary_resolver_output_cost_per_million_tokens)
                .unwrap_or(0.0),
            total_cost_usd: iteration_rows
                .iter()
                .map(|row| row.runtime_observation_boundary_resolver_cost_usd)
                .sum(),
        },
        RuntimeStageReportRow {
            scope: "observation_extraction",
            model: run_summary.runtime_observation_extraction_model.clone(),
            prompt_tokens: iteration_rows
                .iter()
                .map(|row| row.runtime_observation_extraction_prompt_tokens)
                .sum(),
            completion_tokens: iteration_rows
                .iter()
                .map(|row| row.runtime_observation_extraction_completion_tokens)
                .sum(),
            total_tokens: iteration_rows
                .iter()
                .map(|row| row.runtime_observation_extraction_tokens)
                .sum(),
            input_cost_per_million_tokens: first_iteration
                .map(|row| row.runtime_observation_extraction_input_cost_per_million_tokens)
                .unwrap_or(0.0),
            output_cost_per_million_tokens: first_iteration
                .map(|row| row.runtime_observation_extraction_output_cost_per_million_tokens)
                .unwrap_or(0.0),
            total_cost_usd: iteration_rows
                .iter()
                .map(|row| row.runtime_observation_extraction_cost_usd)
                .sum(),
        },
        RuntimeStageReportRow {
            scope: "llm_structured_generation",
            model: run_summary.runtime_llm_structured_generation_model.clone(),
            prompt_tokens: iteration_rows
                .iter()
                .map(|row| row.runtime_llm_structured_generation_prompt_tokens)
                .sum(),
            completion_tokens: iteration_rows
                .iter()
                .map(|row| row.runtime_llm_structured_generation_completion_tokens)
                .sum(),
            total_tokens: iteration_rows
                .iter()
                .map(|row| row.runtime_llm_structured_generation_tokens)
                .sum(),
            input_cost_per_million_tokens: first_iteration
                .map(|row| row.runtime_llm_structured_generation_input_cost_per_million_tokens)
                .unwrap_or(0.0),
            output_cost_per_million_tokens: first_iteration
                .map(|row| row.runtime_llm_structured_generation_output_cost_per_million_tokens)
                .unwrap_or(0.0),
            total_cost_usd: iteration_rows
                .iter()
                .map(|row| row.runtime_llm_structured_generation_cost_usd)
                .sum(),
        },
    ];
    let runtime_total_prompt_cost_usd: f64 = runtime_stage_rows
        .iter()
        .map(|row| row.prompt_tokens as f64 * row.input_cost_per_million_tokens / 1_000_000.0)
        .sum();
    let runtime_total_completion_cost_usd: f64 = runtime_stage_rows
        .iter()
        .map(|row| row.completion_tokens as f64 * row.output_cost_per_million_tokens / 1_000_000.0)
        .sum();
    let runtime_total_stage_row = RuntimeStageReportRow {
        scope: "runtime_total",
        model: "—".to_string(),
        prompt_tokens: runtime_stage_rows.iter().map(|row| row.prompt_tokens).sum(),
        completion_tokens: runtime_stage_rows.iter().map(|row| row.completion_tokens).sum(),
        total_tokens: runtime_stage_rows.iter().map(|row| row.total_tokens).sum(),
        input_cost_per_million_tokens: 0.0,
        output_cost_per_million_tokens: 0.0,
        total_cost_usd: runtime_stage_rows.iter().map(|row| row.total_cost_usd).sum(),
    };
    let mut model_pricing: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    if let Some(call) = first_judge_call {
        model_pricing.insert(
            run_summary.judge_model.clone(),
            (
                call.input_cost_per_million_tokens,
                call.output_cost_per_million_tokens,
            ),
        );
    }
    for row in &runtime_stage_rows {
        model_pricing
            .entry(row.model.clone())
            .or_insert((row.input_cost_per_million_tokens, row.output_cost_per_million_tokens));
    }
    let mut runtime_model_costs: BTreeMap<String, ModelCostReportRow> = BTreeMap::new();
    for row in &runtime_stage_rows {
        let prompt_cost_usd =
            row.prompt_tokens as f64 * row.input_cost_per_million_tokens / 1_000_000.0;
        let completion_cost_usd =
            row.completion_tokens as f64 * row.output_cost_per_million_tokens / 1_000_000.0;
        let entry = runtime_model_costs
            .entry(row.model.clone())
            .or_insert_with(|| ModelCostReportRow {
                model: row.model.clone(),
                prompt_tokens: 0,
                completion_tokens: 0,
                input_cost_per_million_tokens: row.input_cost_per_million_tokens,
                output_cost_per_million_tokens: row.output_cost_per_million_tokens,
                prompt_cost_usd: 0.0,
                completion_cost_usd: 0.0,
                total_cost_usd: 0.0,
            });
        entry.prompt_tokens += row.prompt_tokens;
        entry.completion_tokens += row.completion_tokens;
        entry.prompt_cost_usd += prompt_cost_usd;
        entry.completion_cost_usd += completion_cost_usd;
        entry.total_cost_usd += row.total_cost_usd;
    }

    let mut out = String::new();
    out.push_str("# Eval Run Report\n\n");
    out.push_str("## Contents\n\n");
    out.push_str("- [Run Metadata](#run-metadata)\n");
    out.push_str("- [Metric Layers](#metric-layers)\n");
    out.push_str("- [Executive Summary](#executive-summary)\n");
    out.push_str("- [Runtime Gold Metrics](#runtime-gold-metrics)\n");
    out.push_str("- [Where Quality Was Lost](#where-quality-was-lost)\n");
    out.push_str("- [Judge-Based Aggregated Metrics](#judge-based-aggregated-metrics)\n");
    out.push_str("- [Suite Distributions](#suite-distributions)\n");
    out.push_str("- [Gate Breakdown](#gate-breakdown)\n");
    out.push_str("- [Failure Attribution](#failure-attribution)\n");
    out.push_str("- [Runtime vs Judge Interpretation](#runtime-vs-judge-interpretation)\n");
    out.push_str("- [Worst-Case Preview](#worst-case-preview)\n");
    out.push_str("- [Token Usage](#token-usage)\n");
    out.push_str("- [Appendix A: Full Query Structuring Diagnostics](#appendix-a-full-query-structuring-diagnostics)\n");
    out.push_str("- [Appendix B: Full Retrieval Diagnostics](#appendix-b-full-retrieval-diagnostics)\n");
    out.push_str("- [Appendix C: Judge Metrics Per Run](#appendix-c-judge-metrics-per-run)\n\n");
    out.push_str("- [Appendix D: Suite Overview](#appendix-d-suite-overview)\n\n");

    out.push_str("## Run Metadata\n\n");
    out.push_str(&format!("- eval_run_id: `{}`\n", manifest.eval_run_id));
    out.push_str(&format!("- run_type: `{}`\n", manifest.run_type));
    out.push_str(&format!("- status: `{}`\n", run_summary.status));
    out.push_str(&format!("- started_at: `{}`\n", manifest.started_at));
    out.push_str(&format!(
        "- completed_at: `{}`\n",
        run_summary.completed_at.map(|v| v.to_string()).unwrap_or_else(|| "<none>".to_string())
    ));
    out.push_str(&format!("- runtime_run_count: `{}`\n", run_summary.runtime_run_count));
    out.push_str(&format!("- iterations_evaluated_count: `{}`\n", run_summary.iterations_evaluated_count));
    out.push_str(&format!("- judge_model: `{}`\n", run_summary.judge_model));
    out.push_str(&format!(
        "- query_structuring_model: `{}`\n",
        run_summary.runtime_query_structuring_model
    ));
    out.push_str(&format!(
        "- observation_boundary_resolver_model: `{}`\n",
        run_summary.runtime_observation_boundary_resolver_model
    ));
    out.push_str(&format!(
        "- observation_extraction_model: `{}`\n",
        run_summary.runtime_observation_extraction_model
    ));
    out.push_str(&format!(
        "- llm_structured_generation_model: `{}`\n",
        run_summary.runtime_llm_structured_generation_model
    ));
    out.push_str(&format!(
        "- query_structuring_prompt_version: `{}`\n",
        run_summary.runtime_query_structuring_prompt_version
    ));
    out.push_str(&format!(
        "- observation_boundary_resolver_prompt_version: `{}`\n",
        run_summary.runtime_observation_boundary_resolver_prompt_version
    ));
    out.push_str(&format!(
        "- observation_extraction_prompt_version: `{}`\n",
        run_summary.runtime_observation_extraction_prompt_version
    ));
    out.push_str(&format!(
        "- prompt_context_prompt_version: `{}`\n",
        run_summary.runtime_prompt_context_prompt_version
    ));
    out.push_str(&format!(
        "- diagnostic_update_prompt_context_prompt_version: `{}`\n",
        run_summary.runtime_diagnostic_update_prompt_context_prompt_version
    ));
    out.push_str(&format!("- suite_count: `{}`\n\n", manifest.suite_versions.len()));
    out.push_str("### Token Pricing\n\n");
    if !model_pricing.is_empty() {
        out.push_str("| model | input_price_per_1m | output_price_per_1m |\n|---|---:|---:|\n");
        for (model, (input_price, output_price)) in &model_pricing {
            out.push_str(&format!(
                "| {} | ${}/1M | ${}/1M |\n",
                model,
                format_price_per_million(*input_price),
                format_price_per_million(*output_price),
            ));
        }
    }
    out.push('\n');

    // ── Metric Layers ────────────────────────────────────────────────────────
    out.push_str("## Metric Layers\n\n");
    out.push_str("| layer | source | evaluates | interpretation |\n|---|---|---|---|\n");
    out.push_str("| Judge-based quality metrics | judge model outputs | semantic quality of structuring, evidence pack, and final answer | answers whether the diagnostic behavior is good |\n");
    out.push_str("| Runtime gold metrics | runtime trace spans with golden labels | query structuring and retrieval against expected labels / evidence | answers whether upstream modules selected the expected terms and evidence |\n");
    out.push_str("| Runtime diagnostics | runtime trace attributes and events | low-level counters, hit counts, configuration, support-level issues | helps debug why a metric failed |\n\n");

    // ── Executive Summary ─────────────────────────────────────────────────────
    out.push_str("## Executive Summary\n\n");
    out.push_str("| metric | value | meaning |\n|---|---:|---|\n");
    out.push_str(&format!("| usable_first_response_rate | {:.4} | Share of runs where the final answer can be shown as a first diagnostic response |\n", run_summary.usable_first_response_rate));
    out.push_str(&format!("| gate_pass_rate | {:.4} | Share of runs without critical gate failures |\n", run_summary.gate_pass_rate));
    out.push_str(&format!("| query_structuring_judge_score | {:.4} | Judge-based semantic quality of query structuring |\n", run_summary.query_structuring_judge_score));
    out.push_str(&format!("| runtime_query_structuring_core_success_rate | {:.4} | Gold-backed runtime success of structured query fields |\n", run_summary.runtime_qs_core_success_rate));
    out.push_str(&format!("| runtime_retrieval_mean_ndcg | {:.4} | Average ranking quality across retrieval targets and runs |\n", run_summary.runtime_retrieval_mean_ndcg));
    out.push_str(&format!("| runtime_retrieval_all_strict_recall_success_rate | {:.4} | Average per-run share of retrieval targets where strict expected evidence was found |\n", run_summary.runtime_retrieval_all_strict_recall_success_rate));
    out.push_str(&format!("| evidence_pack_judge_score | {:.4} | Judge-based quality of selected evidence pack |\n", run_summary.evidence_pack_judge_score));
    out.push_str(&format!("| final_answer_judge_score | {:.4} | Judge-based quality of final diagnostic response |\n", run_summary.final_answer_judge_score));
    if has_cont {
        out.push_str(&format!("| usable_continuation_response_rate | {} | Share of continuation iterations with usable update behavior |\n", fmt_opt(run_summary.usable_continuation_response_rate, 4)));
        out.push_str(&format!("| continuation_update_judge_score | {} | Judge-based quality of updating the diagnostic frame |\n", fmt_opt(run_summary.continuation_update_judge_score, 4)));
        out.push_str(&format!("| continuation_input_judge_score | {} | Judge-based quality of reconstructing the new observation from context |\n", fmt_opt(run_summary.continuation_input_judge_score, 4)));
        out.push_str(&format!("| continuation_update_strict_pass_rate | {} | Share of continuation iterations where CU1, CU2, CU3 all scored 2 |\n", fmt_opt(run_summary.continuation_update_strict_pass_rate, 4)));
    }
    out.push('\n');
    out.push_str(&code_legend());
    out.push_str(suite_overview_link_note());
    out.push('\n');

    // ── Judge-Based Aggregated Metrics ────────────────────────────────────────
    out.push_str(&render_judge_aggregated_metrics(iteration_rows));

    // ── Runtime Gold Metrics ──────────────────────────────────────────────────
    out.push_str("## Runtime Gold Metrics\n\n");
    out.push_str("These metrics are computed from runtime trace spans and compare structured query / retrieval outputs against golden labels.\n\n");

    out.push_str("### Query Structuring Core Metrics\n\n");
    out.push_str("| metric | value | meaning |\n|---|---:|---|\n");
    out.push_str(&format!("| runtime_query_structuring_macro_precision_soft | {:.4} | How many selected vocabulary terms are acceptable under soft relevance |\n", run_summary.runtime_qs_macro_precision_soft));
    out.push_str(&format!("| runtime_query_structuring_macro_recall_strict | {:.4} | Whether strictly expected terms were recovered |\n", run_summary.runtime_qs_macro_recall_strict));
    out.push_str(&format!("| runtime_query_structuring_macro_recall_soft | {:.4} | Coverage of broader acceptable terms |\n", run_summary.runtime_qs_macro_recall_soft));
    out.push_str(&format!("| runtime_query_structuring_grounded_strict_recall | {:.4} | Whether strict terms are selected with valid grounding |\n", run_summary.runtime_qs_grounded_strict_recall));
    out.push_str(&format!("| runtime_query_structuring_core_success_rate | {:.4} | Whether all vocab fields passed their core gold-backed checks |\n\n", run_summary.runtime_qs_core_success_rate));

    // Field-level core table
    out.push_str("#### Query Structuring Field Core Metrics\n\n");
    out.push_str("| field | precision_soft | recall_strict | recall_soft | grounded_strict_recall | field_core_success | field_grounded_success |\n|---|---:|---:|---:|---:|---:|---:|\n");
    for f in ["symptoms", "affected_subsystems", "failure_modes", "system_properties"] {
        out.push_str(&format!("| {} | {} | {} | {} | {} | {} | {} |\n", f,
            fmt_opt(avg_qs_vocab(iteration_rows, f, "precision_soft"), 4),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "recall_strict"), 4),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "recall_soft"), 4),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "grounded_strict_recall"), 4),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "field_core_success"), 4),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "field_grounded_success"), 4)));
    }
    out.push('\n');

    out.push_str("### Retrieval Core Metrics\n\n");
    out.push_str("> Each value is averaged over runs where the target was evaluated.\n\n");
    out.push_str("| retrieval_target | evaluated_k | recall_strict | recall_soft | rr_strict | rr_soft | nDCG | frr_strict | frr_soft | n_strict | n_soft |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    let rt_targets: &[(&str, fn(&EvalIterationSummaryRow) -> Option<&serde_json::Value>)] = &[
        ("candidate_cards",        |r| r.runtime_candidate_cards_metrics.as_ref()),
        ("incident_primary",       |r| r.runtime_incident_primary_metrics.as_ref()),
        ("incident_alternatives",  |r| r.runtime_incident_alternatives_metrics.as_ref()),
        ("theory_evidence",        |r| r.runtime_theory_evidence_metrics.as_ref()),
    ];
    for (target_name, get_fn) in rt_targets {
        let ek  = avg_rt_field(iteration_rows, get_fn, "evaluated_k");
        let rcs = avg_rt_field(iteration_rows, get_fn, "recall_strict");
        let rso = avg_rt_field(iteration_rows, get_fn, "recall_soft");
        let rrs = avg_rt_field(iteration_rows, get_fn, "rr_strict");
        let rro = avg_rt_field(iteration_rows, get_fn, "rr_soft");
        let ndcg = avg_rt_field(iteration_rows, get_fn, "ndcg");
        let frrs = avg_rt_field(iteration_rows, get_fn, "first_relevant_rank_strict");
        let frro = avg_rt_field(iteration_rows, get_fn, "first_relevant_rank_soft");
        let ns  = avg_rt_field(iteration_rows, get_fn, "num_relevant_strict");
        let no  = avg_rt_field(iteration_rows, get_fn, "num_relevant_soft");
        out.push_str(&format!("| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            target_name,
            fmt_opt(ek, 1), fmt_opt(rcs, 4), fmt_opt(rso, 4),
            fmt_opt(rrs, 4), fmt_opt(rro, 4), fmt_opt(ndcg, 4),
            fmt_opt(frrs, 2), fmt_opt(frro, 2),
            fmt_opt(ns, 2), fmt_opt(no, 2)));
    }
    out.push('\n');

    out.push_str("### Retrieval Summary\n\n");
    out.push_str("| metric | value | formula | meaning |\n|---|---:|---|---|\n");
    out.push_str(&format!("| runtime_retrieval_mean_ndcg | {:.4} | avg_run(avg_target(ndcg)) | Average ranking quality across retrieval targets and runs |\n", run_summary.runtime_retrieval_mean_ndcg));
    out.push_str(&format!("| runtime_retrieval_all_strict_recall_success_rate | {:.4} | avg_run(frac_target(recall_strict=1)) | Average per-run share of retrieval targets with strict recall success |\n", run_summary.runtime_retrieval_all_strict_recall_success_rate));
    out.push_str(&format!("| runtime_retrieval_all_soft_recall_success_rate | {:.4} | avg_run(frac_target(recall_soft>0)) | Average per-run share of retrieval targets with any soft recall |\n", run_summary.runtime_retrieval_all_soft_recall_success_rate));
    // Penalized first relevant rank: None → evaluated_k + 1
    let penalized_frr = {
        let per_run: Vec<f64> = iteration_rows.iter().filter_map(|r| {
            let targets: Vec<Option<(f64, f64)>> = vec![
                r.runtime_candidate_cards_metrics.as_ref().map(|v| (
                    v.get("first_relevant_rank_strict").and_then(|x| x.as_f64()).unwrap_or_else(|| v.get("evaluated_k").and_then(|k| k.as_f64()).unwrap_or(8.0) + 1.0),
                    1.0,
                )),
                r.runtime_incident_primary_metrics.as_ref().map(|v| (
                    v.get("first_relevant_rank_strict").and_then(|x| x.as_f64()).unwrap_or_else(|| v.get("evaluated_k").and_then(|k| k.as_f64()).unwrap_or(12.0) + 1.0),
                    1.0,
                )),
                r.runtime_incident_alternatives_metrics.as_ref().map(|v| (
                    v.get("first_relevant_rank_strict").and_then(|x| x.as_f64()).unwrap_or_else(|| v.get("evaluated_k").and_then(|k| k.as_f64()).unwrap_or(12.0) + 1.0),
                    1.0,
                )),
                r.runtime_theory_evidence_metrics.as_ref().map(|v| (
                    v.get("first_relevant_rank_strict").and_then(|x| x.as_f64()).unwrap_or_else(|| v.get("evaluated_k").and_then(|k| k.as_f64()).unwrap_or(12.0) + 1.0),
                    1.0,
                )),
            ];
            let present: Vec<f64> = targets.into_iter().flatten().map(|(v, _)| v).collect();
            if present.is_empty() { None } else { Some(present.iter().sum::<f64>() / present.len() as f64) }
        }).collect();
        if per_run.is_empty() { None } else { Some(per_run.iter().sum::<f64>() / per_run.len() as f64) }
    };
    out.push_str(&format!("| runtime_retrieval_penalized_first_relevant_rank_strict | {} | avg_run(avg_target(frr_strict or k+1)) | Penalized rank; missing strict hit treated as k+1 |\n", fmt_opt(penalized_frr, 2)));
    out.push_str(&format!("| runtime_retrieval_zero_hit_rate | {:.4} | avg_run(frac_target(hits_count=0)) | Average per-run share of retrieval calls with no hits |\n\n", run_summary.runtime_retrieval_zero_hit_rate));

    out.push_str("## Suite Distributions\n\n");
    out.push_str("| suite | score_0 | score_1 | score_2 |\n|---|---:|---:|---:|\n");
    for (name, getter) in &[
        ("final_no_root_cause_claim", &(|r: &EvalIterationSummaryRow| r.final_no_root_cause_claim_score) as &dyn Fn(&EvalIterationSummaryRow) -> i16),
        ("final_first_check_discriminates", &(|r: &EvalIterationSummaryRow| r.final_first_check_discriminates_score) as &dyn Fn(&EvalIterationSummaryRow) -> i16),
        ("final_alternative_context_handling", &(|r: &EvalIterationSummaryRow| r.final_alternative_context_handling_score) as &dyn Fn(&EvalIterationSummaryRow) -> i16),
        ("final_result_interpretation_usefulness", &(|r: &EvalIterationSummaryRow| r.final_result_interpretation_usefulness_score) as &dyn Fn(&EvalIterationSummaryRow) -> i16),
        ("final_hypothesis_source_alignment", &(|r: &EvalIterationSummaryRow| r.final_hypothesis_source_alignment_score) as &dyn Fn(&EvalIterationSummaryRow) -> i16),
        ("query_structuring_field_boundary_correctness", &(|r: &EvalIterationSummaryRow| r.query_structuring_field_boundary_correctness_score) as &dyn Fn(&EvalIterationSummaryRow) -> i16),
        ("query_structuring_grounding_conservatism", &(|r: &EvalIterationSummaryRow| r.query_structuring_grounding_conservatism_score) as &dyn Fn(&EvalIterationSummaryRow) -> i16),
        ("evidence_pack_role_fit", &(|r: &EvalIterationSummaryRow| r.evidence_pack_role_fit_score) as &dyn Fn(&EvalIterationSummaryRow) -> i16),
        ("evidence_pack_sufficiency", &(|r: &EvalIterationSummaryRow| r.evidence_pack_sufficiency_score) as &dyn Fn(&EvalIterationSummaryRow) -> i16),
    ] {
        let (s0, s1, s2) = score_dist(iteration_rows, getter);
        if s0 + s1 + s2 > 0 || *name == "final_no_root_cause_claim" {
            out.push_str(&format!("| {} | {} | {} | {} |\n", name, s0, s1, s2));
        }
    }
    if has_cont {
        for (name, getter) in &[
            ("continuation_hypothesis_update_discipline",                 &(|r: &EvalIterationSummaryRow| r.continuation_hypothesis_update_discipline_score)                as &dyn Fn(&EvalIterationSummaryRow) -> Option<i16>),
            ("continuation_problem_understanding_update",                 &(|r: &EvalIterationSummaryRow| r.continuation_problem_understanding_update_score)                as &dyn Fn(&EvalIterationSummaryRow) -> Option<i16>),
            ("continuation_next_check_progression",                      &(|r: &EvalIterationSummaryRow| r.continuation_next_check_progression_score)                     as &dyn Fn(&EvalIterationSummaryRow) -> Option<i16>),
            ("continuation_observation_resolution_context_recovery",     &(|r: &EvalIterationSummaryRow| r.continuation_observation_resolution_context_recovery_score)    as &dyn Fn(&EvalIterationSummaryRow) -> Option<i16>),
        ] {
            let (s0, s1, s2) = score_dist_opt(iteration_rows, getter);
            if s0 + s1 + s2 > 0 {
                out.push_str(&format!("| {} | {} | {} | {} |\n", name, s0, s1, s2));
            }
        }
    }
    out.push('\n');

    out.push_str("## Gate Breakdown\n\n");
    out.push_str("| gate | fail_count | fail_rate |\n|---|---:|---:|\n");
    for (suite_name, getter) in &[
        ("final_no_root_cause_claim", &(|r: &EvalIterationSummaryRow| r.no_root_cause_gate_passed) as &dyn Fn(&EvalIterationSummaryRow) -> bool),
        ("final_first_check_discriminates", &(|r: &EvalIterationSummaryRow| r.single_check_gate_passed) as &dyn Fn(&EvalIterationSummaryRow) -> bool),
        ("final_hypothesis_source_alignment", &(|r: &EvalIterationSummaryRow| r.source_alignment_gate_passed) as &dyn Fn(&EvalIterationSummaryRow) -> bool),
        ("query_structuring_field_boundary_correctness", &(|r: &EvalIterationSummaryRow| r.field_boundary_gate_passed) as &dyn Fn(&EvalIterationSummaryRow) -> bool),
        ("evidence_pack_sufficiency", &(|r: &EvalIterationSummaryRow| r.evidence_pack_gate_passed) as &dyn Fn(&EvalIterationSummaryRow) -> bool),
    ] {
        let (fail, rate) = gate_fail(iteration_rows, getter);
        out.push_str(&format!("| {} | {} | {:.4} |\n", suite_name, fail, rate));
    }
    if has_cont {
        for (suite_name, getter) in &[
            ("continuation_hypothesis_update_discipline",              &(|r: &EvalIterationSummaryRow| r.continuation_hypothesis_update_discipline_score)             as &dyn Fn(&EvalIterationSummaryRow) -> Option<i16>),
            ("continuation_problem_understanding_update",              &(|r: &EvalIterationSummaryRow| r.continuation_problem_understanding_update_score)             as &dyn Fn(&EvalIterationSummaryRow) -> Option<i16>),
            ("continuation_next_check_progression",                   &(|r: &EvalIterationSummaryRow| r.continuation_next_check_progression_score)                  as &dyn Fn(&EvalIterationSummaryRow) -> Option<i16>),
            ("continuation_observation_resolution_context_recovery",  &(|r: &EvalIterationSummaryRow| r.continuation_observation_resolution_context_recovery_score) as &dyn Fn(&EvalIterationSummaryRow) -> Option<i16>),
        ] {
            let (fail, rate) = gate_fail_opt(iteration_rows, getter);
            out.push_str(&format!("| {} | {} | {:.4} |\n", suite_name, fail, rate));
        }
    }
    out.push_str("\n> Gate fails when suite score = 0. Pass threshold: score ≥ 1.\n");
    out.push_str("> Note: `Gate Breakdown` reflects critical standalone gates and may differ from composite no-hard-fail formulas (e.g., `final_hypothesis_source_alignment` is gated individually but excluded from `final_answer_no_hard_fail_rate`).\n\n");

    out.push_str("## Failure Attribution\n\n");

    out.push_str("### Initial / First-Response Attribution\n\n");
    out.push_str("| metric | value | formula |\n|---|---:|---|\n");
    out.push_str(&format!("| bad_final_due_to_query_rate | {:.4} | frac(!usable ∧ (QS1=0 ∨ QS2=0)) |\n", run_summary.bad_final_due_to_query_rate));
    out.push_str(&format!("| bad_final_due_to_evidence_rate | {:.4} | frac(!usable ∧ (EP1=0 ∨ EP2=0)) |\n", run_summary.bad_final_due_to_evidence_rate));
    out.push_str(&format!("| bad_final_with_good_query_and_evidence_rate | {:.4} | frac(!usable ∧ QS1>0 ∧ QS2>0 ∧ EP1>0 ∧ EP2>0) |\n\n", run_summary.bad_final_with_good_query_and_evidence_rate));
    out.push_str("> usable = FA1≥1 ∧ FA2≥1 ∧ FA5≥1\n\n");

    if has_cont {
        out.push_str("### Continuation Attribution\n\n");
        out.push_str("> `frac(condition)` uses only continuation iterations where all fields referenced by that formula are present; missing values are excluded from both numerator and denominator.\n\n");
        out.push_str("| metric | value | formula |\n|---|---:|---|\n");

        // Eligibility-filtered fractions over continuation rows
        let cont_attr_frac = |cond: &dyn Fn(&EvalIterationSummaryRow) -> Option<bool>| -> Option<f64> {
            let eligible: Vec<bool> = cont_rows.iter().filter_map(|r| cond(r)).collect();
            if eligible.is_empty() { None }
            else { Some(eligible.iter().filter(|&&v| v).count() as f64 / eligible.len() as f64) }
        };

        // bad_continuation_due_to_input_resolution_rate
        // eligible: usable_continuation_response is Some AND CU4 is Some
        // condition: !usable AND CU4 = 0
        let bad_input = cont_attr_frac(&|r| {
            let usable = r.usable_continuation_response?;
            let cu4 = r.continuation_observation_resolution_context_recovery_score?;
            Some(!usable && cu4 == 0)
        });

        // bad_continuation_due_to_update_logic_rate
        // eligible: usable, CU4, CU1, CU2, CU3 all Some
        // condition: !usable AND CU4 > 0 AND (CU1=0 OR CU2=0 OR CU3=0)
        let bad_update = cont_attr_frac(&|r| {
            let usable = r.usable_continuation_response?;
            let cu4 = r.continuation_observation_resolution_context_recovery_score?;
            let cu1 = r.continuation_hypothesis_update_discipline_score?;
            let cu2 = r.continuation_problem_understanding_update_score?;
            let cu3 = r.continuation_next_check_progression_score?;
            Some(!usable && cu4 > 0 && (cu1 == 0 || cu2 == 0 || cu3 == 0))
        });

        // bad_continuation_despite_good_input_rate
        // eligible: usable and CU4 both Some
        // condition: !usable AND CU4 = 2
        let bad_despite_good = cont_attr_frac(&|r| {
            let usable = r.usable_continuation_response?;
            let cu4 = r.continuation_observation_resolution_context_recovery_score?;
            Some(!usable && cu4 == 2)
        });

        // good_continuation_despite_input_issue_rate
        // eligible: usable and CU4 both Some
        // condition: usable AND CU4 < 2
        let good_despite_issue = cont_attr_frac(&|r| {
            let usable = r.usable_continuation_response?;
            let cu4 = r.continuation_observation_resolution_context_recovery_score?;
            Some(usable && cu4 < 2)
        });

        out.push_str(&format!("| bad_continuation_due_to_input_resolution_rate | {} | frac(!usable_continuation ∧ CU4=0) |\n", fmt_opt(bad_input, 4)));
        out.push_str(&format!("| bad_continuation_due_to_update_logic_rate | {} | frac(!usable_continuation ∧ CU4>0 ∧ (CU1=0 ∨ CU2=0 ∨ CU3=0)) |\n", fmt_opt(bad_update, 4)));
        out.push_str(&format!("| bad_continuation_despite_good_input_rate | {} | frac(!usable_continuation ∧ CU4=2) |\n", fmt_opt(bad_despite_good, 4)));
        out.push_str(&format!("| good_continuation_despite_input_issue_rate | {} | frac(usable_continuation ∧ CU4<2) |\n\n", fmt_opt(good_despite_issue, 4)));
        out.push_str("> usable_continuation = CU1≥1 ∧ CU2≥1 ∧ CU3≥1 ∧ FA1≥1 ∧ FA2≥1 ∧ FA5≥1\n\n");
    }

    out.push_str(&code_legend());
    out.push_str(suite_overview_link_note());
    out.push('\n');

    // ── Where Quality Was Lost ────────────────────────────────────────────────
    out.push_str(&render_quality_loss_section(run_summary, iteration_rows));

    // ── Runtime vs Judge Interpretation ───────────────────────────────────────
    out.push_str("## Runtime vs Judge Interpretation\n\n");
    out.push_str("| signal | interpretation |\n|---|---|\n");
    out.push_str("| Good query structuring runtime metrics + bad query structuring judge score | Terms may match gold labels, but field semantics or conservatism are wrong |\n");
    out.push_str("| Bad query structuring runtime metrics + good final answer | Final model compensated for upstream errors; system may be unstable |\n");
    out.push_str("| Good retrieval metrics + bad evidence_pack judge score | Retrieved relevant chunks, but selected/packed chunks do not serve diagnostic roles |\n");
    out.push_str("| Good evidence_pack judge score + bad final answer | Final prompt/model likely needs work |\n");
    out.push_str("| Bad alternative retrieval + bad alternative handling | Alternative context is not giving the model useful competing evidence |\n\n");

    out.push_str("## Worst-Case Preview\n\n");
    out.push_str("| runtime_run_id | iteration_id | final_answer_score | usable_first_response |\n|---|---|---:|---:|\n");
    for row in &worst_cases {
        out.push_str(&format!(
            "| `{}` | `{}` | {:.4} | {} |\n",
            row.key.runtime_run_id,
            row.key.iteration_id,
            row.final_answer_judge_score,
            row.usable_first_response,
        ));
    }
    out.push('\n');

    out.push_str("## Token Usage\n\n");

    // Per-suite breakdown
    let mut suite_totals: BTreeMap<String, JudgeSuiteReportRow> = BTreeMap::new();
    for call in judge_calls {
        let entry = suite_totals
            .entry(call.suite_name.clone())
            .or_insert_with(|| JudgeSuiteReportRow {
                suite: call.suite_name.clone(),
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                input_cost_per_million_tokens: call.input_cost_per_million_tokens,
                output_cost_per_million_tokens: call.output_cost_per_million_tokens,
                prompt_cost_usd: 0.0,
                completion_cost_usd: 0.0,
                total_cost_usd: 0.0,
            });
        entry.prompt_tokens += call.prompt_tokens;
        entry.completion_tokens += call.completion_tokens;
        entry.total_tokens += call.total_tokens;
        entry.prompt_cost_usd += call.prompt_cost_usd;
        entry.completion_cost_usd += call.completion_cost_usd;
        entry.total_cost_usd += call.total_cost_usd;
    }
    if !suite_totals.is_empty() {
        out.push_str("### Judge Calls by Suite\n\n");
        out.push_str("| suite | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |\n|---|---:|---:|---:|---:|\n");
        for row in suite_totals.values() {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {:.6} |\n",
                row.suite,
                row.prompt_tokens,
                row.completion_tokens,
                row.total_tokens,
                row.total_cost_usd,
            ));
        }
        let judge_total_prompt_cost_usd: f64 = suite_totals.values().map(|row| row.prompt_cost_usd).sum();
        let judge_total_completion_cost_usd: f64 = suite_totals
            .values()
            .map(|row| row.completion_cost_usd)
            .sum();
        let judge_total = JudgeSuiteReportRow {
            suite: "judge_total".to_string(),
            prompt_tokens: suite_totals.values().map(|row| row.prompt_tokens).sum(),
            completion_tokens: suite_totals.values().map(|row| row.completion_tokens).sum(),
            total_tokens: suite_totals.values().map(|row| row.total_tokens).sum(),
            input_cost_per_million_tokens: first_judge_call
                .map(|call| call.input_cost_per_million_tokens)
                .unwrap_or(0.0),
            output_cost_per_million_tokens: first_judge_call
                .map(|call| call.output_cost_per_million_tokens)
                .unwrap_or(0.0),
            prompt_cost_usd: judge_total_prompt_cost_usd,
            completion_cost_usd: judge_total_completion_cost_usd,
            total_cost_usd: suite_totals.values().map(|row| row.total_cost_usd).sum(),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {:.6} |\n\n",
            judge_total.suite,
            judge_total.prompt_tokens,
            judge_total.completion_tokens,
            judge_total.total_tokens,
            judge_total.total_cost_usd,
        ));
        out.push_str("| model | prompt_tokens_cost | completion_tokens_cost | total_cost_usd |\n|---|---|---|---:|\n");
        out.push_str(&format!(
            "| {} | {} | {} | {:.6} |\n\n",
            run_summary.judge_model,
            format_cost_cell(
                judge_total.prompt_tokens,
                judge_total.input_cost_per_million_tokens,
                judge_total.prompt_cost_usd,
            ),
            format_cost_cell(
                judge_total.completion_tokens,
                judge_total.output_cost_per_million_tokens,
                judge_total.completion_cost_usd,
            ),
            judge_total.total_cost_usd,
        ));
    }

    out.push_str("### Runtime by Stage\n\n");
    out.push_str("| scope | model | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |\n|---|---|---:|---:|---:|---:|\n");
    for row in &runtime_stage_rows {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {:.6} |\n",
            row.scope,
            row.model,
            row.prompt_tokens,
            row.completion_tokens,
            row.total_tokens,
            row.total_cost_usd,
        ));
    }
    out.push_str(&format!(
        "| {} | {} | {} | {} | {} | {:.6} |\n\n",
        runtime_total_stage_row.scope,
        runtime_total_stage_row.model,
        runtime_total_stage_row.prompt_tokens,
        runtime_total_stage_row.completion_tokens,
        runtime_total_stage_row.total_tokens,
        runtime_total_stage_row.total_cost_usd,
    ));
    out.push_str("| model | prompt_tokens_cost | completion_tokens_cost | total_cost_usd |\n|---|---|---|---:|\n");
    for row in runtime_model_costs.values() {
        out.push_str(&format!(
            "| {} | {} | {} | {:.6} |\n",
            row.model,
            format_cost_cell(
                row.prompt_tokens,
                row.input_cost_per_million_tokens,
                row.prompt_cost_usd,
            ),
            format_cost_cell(
                row.completion_tokens,
                row.output_cost_per_million_tokens,
                row.completion_cost_usd,
            ),
            row.total_cost_usd,
        ));
    }
    out.push_str(&format!(
        "| {} | {} | {} | {:.6} |\n\n",
        "runtime_total",
        format!("sum(stage prompt costs) = {:.6}", runtime_total_prompt_cost_usd),
        format!("sum(stage completion costs) = {:.6}", runtime_total_completion_cost_usd),
        runtime_total_stage_row.total_cost_usd,
    ));

    out.push_str("### Totals\n\n");
    out.push_str("| scope | prompt_tokens | completion_tokens | total_tokens | total_cost_usd |\n|---|---:|---:|---:|---:|\n");
    out.push_str(&format!(
        "| runtime | {} | {} | {} | {:.6} |\n",
        run_summary.runtime_prompt_tokens,
        run_summary.runtime_completion_tokens,
        run_summary.runtime_total_tokens,
        run_summary.runtime_total_cost_usd,
    ));
    out.push_str(&format!(
        "| judge_total | {} | {} | {} | {:.6} |\n",
        run_summary.judge_prompt_tokens,
        run_summary.judge_completion_tokens,
        run_summary.judge_total_tokens,
        run_summary.judge_total_cost_usd,
    ));
    out.push_str(&format!(
        "| run_total | {} | {} | {} | {:.6} |\n\n",
        run_summary.runtime_prompt_tokens + run_summary.judge_prompt_tokens,
        run_summary.runtime_completion_tokens + run_summary.judge_completion_tokens,
        run_summary.run_total_tokens,
        run_summary.run_total_cost_usd,
    ));
    out.push_str(&format!(
        "Run total cost usd = runtime total cost usd + judge total cost usd = {:.6} + {:.6} = {:.6}\n",
        run_summary.runtime_total_cost_usd,
        run_summary.judge_total_cost_usd,
        run_summary.run_total_cost_usd,
    ));

    // ── Appendix A: Full Query Structuring Diagnostics ────────────────────────
    out.push_str("\n## Appendix A: Full Query Structuring Diagnostics\n\n");

    out.push_str("### A.1 Contract Diagnostics\n\n");
    out.push_str("| field | invalid_vocab_count | duplicate_term_count |\n|---|---:|---:|\n");
    for f in ["symptoms", "affected_subsystems", "failure_modes", "system_properties"] {
        out.push_str(&format!("| {} | {} | {} |\n", f,
            fmt_opt(avg_qs_vocab(iteration_rows, f, "invalid_vocab_count"), 2),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "duplicate_term_count"), 2)));
    }
    out.push('\n');

    out.push_str("### A.2 Selection Diagnostics\n\n");
    out.push_str("| field | num_predicted_terms | num_false_positive | num_false_negative_strict | zero_score_selection_count |\n|---|---:|---:|---:|---:|\n");
    for f in ["symptoms", "affected_subsystems", "failure_modes", "system_properties"] {
        out.push_str(&format!("| {} | {} | {} | {} | {} |\n", f,
            fmt_opt(avg_qs_vocab(iteration_rows, f, "num_predicted_terms"), 2),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "num_false_positive"), 2),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "num_false_negative_strict"), 2),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "zero_score_selection_count"), 2)));
    }
    out.push('\n');

    out.push_str("### A.3 Graded Relevance Diagnostics\n\n");
    out.push_str("| field | graded_coverage | average_selected_score |\n|---|---:|---:|\n");
    for f in ["symptoms", "affected_subsystems", "failure_modes", "system_properties"] {
        out.push_str(&format!("| {} | {} | {} |\n", f,
            fmt_opt(avg_qs_vocab(iteration_rows, f, "graded_coverage"), 4),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "average_selected_score"), 4)));
    }
    out.push('\n');

    out.push_str("### A.4 Grounding Diagnostics\n\n");
    out.push_str("| field | unsupported_selected_term_rate | missing_evidence_span_count | invalid_evidence_span_count | evidence_span_near_substring_rate |\n|---|---:|---:|---:|---:|\n");
    for f in ["symptoms", "affected_subsystems", "failure_modes", "system_properties"] {
        out.push_str(&format!("| {} | {} | {} | {} | {} |\n", f,
            fmt_opt(avg_qs_vocab(iteration_rows, f, "unsupported_selected_term_rate"), 4),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "missing_evidence_span_count"), 2),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "invalid_evidence_span_count"), 2),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "evidence_span_near_substring_rate"), 4)));
    }
    out.push('\n');

    out.push_str("### A.5 Support-Level Diagnostics\n\n");
    out.push_str("| field | weak_inference_rate | strict_terms_weak_inference_rate | weak_false_positive_rate |\n|---|---:|---:|---:|\n");
    for f in ["symptoms", "affected_subsystems", "failure_modes", "system_properties"] {
        out.push_str(&format!("| {} | {} | {} | {} |\n", f,
            fmt_opt(avg_qs_vocab(iteration_rows, f, "weak_inference_rate"), 4),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "strict_terms_weak_inference_rate"), 4),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "weak_false_positive_rate"), 4)));
    }
    out.push('\n');

    out.push_str("### A.6 Field Success Diagnostics\n\n");
    out.push_str("| field | field_core_success | field_grounded_success | empty_when_gold_exists |\n|---|---:|---:|---:|\n");
    for f in ["symptoms", "affected_subsystems", "failure_modes", "system_properties"] {
        out.push_str(&format!("| {} | {} | {} | {} |\n", f,
            fmt_opt(avg_qs_vocab(iteration_rows, f, "field_core_success"), 4),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "field_grounded_success"), 4),
            fmt_opt(avg_qs_vocab(iteration_rows, f, "empty_when_gold_exists"), 4)));
    }
    out.push('\n');

    out.push_str("### A.7 Query-Level Non-Vocabulary Diagnostics\n\n");
    out.push_str("| entities_count_avg | constraints_count_avg | triggers_count_avg | observability_signals_count_avg | unresolved_terms_count_avg | intent_present_rate | scenario_present_rate |\n|---:|---:|---:|---:|---:|---:|---:|\n");
    out.push_str(&format!("| {} | {} | {} | {} | {} | {} | {} |\n\n",
        fmt_opt(avg_qs_non_vocab(iteration_rows, "entities_count"), 2),
        fmt_opt(avg_qs_non_vocab(iteration_rows, "constraints_count"), 2),
        fmt_opt(avg_qs_non_vocab(iteration_rows, "triggers_count"), 2),
        fmt_opt(avg_qs_non_vocab(iteration_rows, "observability_signals_count"), 2),
        fmt_opt(avg_qs_non_vocab(iteration_rows, "unresolved_terms_count"), 2),
        fmt_opt(avg_qs_non_vocab(iteration_rows, "intent_present"), 4),
        fmt_opt(avg_qs_non_vocab(iteration_rows, "scenario_present"), 4)));

    // ── Appendix B: Full Retrieval Diagnostics ────────────────────────────────
    out.push_str("## Appendix B: Full Retrieval Diagnostics\n\n");

    out.push_str("### B.1 Retrieval Configuration\n\n");
    out.push_str("| retrieval_target | collection | top_k |\n|---|---|---:|\n");
    for (target, get_fn) in rt_targets {
        let ek = avg_rt_target(iteration_rows, *get_fn, "evaluated_k");
        let collection = match *target {
            "candidate_cards"       => "cards",
            "incident_primary"      => "practice_chunks",
            "incident_alternatives" => "practice_chunks",
            _                       => "theory_chunks",
        };
        out.push_str(&format!("| {} | {} | {} |\n", target, collection, fmt_opt(ek, 0)));
    }
    out.push('\n');

    out.push_str("### B.2 Retrieval Hit Counts\n\n");
    out.push_str("| retrieval_target | hits_count_avg | selected_count_avg | top_score_avg | min_score_avg |\n|---|---:|---:|---:|---:|\n");
    for (target, get_fn) in rt_targets {
        out.push_str(&format!("| {} | {} | {} | {} | {} |\n", target,
            fmt_opt(avg_rt_target(iteration_rows, *get_fn, "hits_count"), 1),
            fmt_opt(avg_rt_target(iteration_rows, *get_fn, "selected_count"), 1),
            fmt_opt(avg_rt_target(iteration_rows, *get_fn, "top_score"), 4),
            fmt_opt(avg_rt_target(iteration_rows, *get_fn, "min_score"), 4)));
    }
    out.push('\n');

    // ── Appendix C: Judge Metrics Per Run ─────────────────────────────────────
    out.push_str(&render_judge_per_run_appendix(iteration_rows, "Appendix C"));

    // ── Appendix D: Suite Overview ────────────────────────────────────────────
    out.push_str("## Appendix D: Suite Overview\n\n");
    for suite_name in enabled_suites {
        if let Some(def) = catalog.get(suite_name) {
            let code = suite_code(suite_name);
            let applies_to = match def.applies_to {
                SuiteApplicability::Shared => "shared",
                SuiteApplicability::InitialOnly => "initial only",
                SuiteApplicability::ContinuationOnly => "continuation only",
            };
            out.push_str(&format!("### {}\n\n", suite_name));
            out.push_str("| code | applies to | checks | why | inputs | score |\n|---|---|---|---|---|---:|\n");
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | 0/1/2 |\n\n",
                code,
                applies_to,
                def.what_it_checks,
                def.why_it_matters,
                def.inputs_to_judge.join(", "),
            ));
        }
    }

    out
}
