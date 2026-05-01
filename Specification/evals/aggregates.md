## 1) Purpose

This document defines the aggregate formulas for the diagnostics eval engine.

Its goal is to turn raw suite verdicts into actionable engineering signals,
not one opaque quality average.

The aggregate model must answer:

- how often the system produces a usable first response;
- which stage of the diagnostic chain breaks first;
- whether failures come from query structuring, evidence packing, or final
  response construction;
- how much usage and cost the eval run consumed.

## 2) Score Domain

All MVP judge suites normalize to:

- `0`
- `1`
- `2`

The aggregate formulas in this document assume that score domain.

## 3) Base Per-Suite Aggregates

For every suite, the summary layer must support at least:

- `avg_score = avg(score)`
- `pass_rate_strict = % score = 2`
- `pass_rate_soft = % score >= 1`
- `fail_rate = % score = 0`
- `minor_issue_rate = % score = 1`

These per-suite aggregates are required at eval-run level.

## 4) Category Scores

The current MVP categories are:

1. `query_structuring`
2. `evidence_pack`
3. `final_answer`

Category-level scores are simple averages of the corresponding suite scores.

Required category aggregates:

- `query_structuring_judge_score`
- `evidence_pack_judge_score`
- `final_answer_judge_score`

## 5) Category Pass Metrics

Required category pass metrics:

- `query_structuring_strict_pass_rate`
- `evidence_pack_strict_pass_rate`
- `final_answer_strict_pass_rate`
- `query_structuring_no_hard_fail_rate`
- `evidence_pack_no_hard_fail_rate`
- `final_answer_no_hard_fail_rate`

Definitions:

- `strict_pass` means all required suites in that category equal `2`
- `no_hard_fail` means all required suites in that category are `>= 1`

## 6) Usable First Response

The primary MVP KPI is:

- `usable_first_response_rate`

Per iteration:

- `usable_first_response = true` iff
  - `final_no_root_cause_claim_score >= 1`
  - `final_first_check_discriminates_score >= 1`
  - `final_result_interpretation_usefulness_score >= 1`

At eval-run level:

- `usable_first_response_rate = % iterations where usable_first_response = true`

## 7) Critical Gates

The MVP gate conditions are:

- `no_root_cause_gate_passed = final_no_root_cause_claim_score >= 1`
- `single_check_gate_passed = final_first_check_discriminates_score >= 1`
- `source_alignment_gate_passed = final_hypothesis_source_alignment_score >= 1`
- `field_boundary_gate_passed = query_structuring_field_boundary_correctness_score >= 1`
- `evidence_pack_gate_passed = evidence_pack_sufficiency_score >= 1`

Required eval-run aggregates:

- `gate_pass_rate = % iterations where all gates passed`
- `gate_fail_breakdown = fail count/rate by gate`

## 8) Failure Attribution

The eval engine must support the following derived booleans per iteration:

- `query_good = all required query_structuring suites >= 1`
- `evidence_good = all required evidence_pack suites >= 1`
- `final_good = usable_first_response`
- `query_hard_fail = any required query_structuring suite = 0`
- `evidence_hard_fail = any required evidence_pack suite = 0`
- `final_bad = not usable_first_response`

Required eval-run aggregates:

- `bad_final_due_to_query_rate = % final_bad AND query_hard_fail`
- `bad_final_due_to_evidence_rate = % final_bad AND evidence_hard_fail`
- `bad_final_with_good_query_and_evidence_rate = % final_bad AND query_good AND evidence_good`

These are required because they directly support engineering triage.

## 9) Usage And Cost Aggregates

At iteration level:

- `judge_total_tokens = sum judge prompt/completion tokens across suite calls`
- `judge_total_cost_usd = sum judge total cost across suite calls`
- `run_total_tokens = runtime_total_tokens + judge_total_tokens`
- `run_total_cost_usd = runtime_total_cost_usd + judge_total_cost_usd`

At eval-run level:

- every runtime usage field must be summed across evaluated iteration subjects;
- every judge usage field must be summed across `judge_llm_calls` for the
  current `eval_run_id`;
- run totals must be computed from those two domain totals.

## 10) MVP Aggregate Set

The current MVP summary/report layer must expose at least:

- per-suite `avg_score`
- per-suite `pass_rate_strict`
- per-suite `fail_rate`
- `query_structuring_judge_score`
- `evidence_pack_judge_score`
- `final_answer_judge_score`
- `usable_first_response_rate`
- `gate_pass_rate`
- `gate_fail_breakdown`
- `bad_final_due_to_query_rate`
- `bad_final_due_to_evidence_rate`
- `bad_final_with_good_query_and_evidence_rate`
- aggregated runtime usage totals
- aggregated judge usage totals
- aggregated run totals

## 11) Deferred Aggregates

The following remain valuable but are not required in the first version:

- alternative-context-specific secondary aggregates beyond the core suite score;
- cross-iteration loop-progress metrics;
- rich percentile/tail views beyond what the first report needs;
- deep error taxonomy breakdowns from `failure_code`.
