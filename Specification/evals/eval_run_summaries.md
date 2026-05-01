## 1) Purpose

This document defines the eval-run-level summary storage contract for the
diagnostic eval engine.

`eval_run_summaries` is the canonical aggregate table for one completed or
in-progress eval run.

Its role is to support:

- dashboard listing of eval runs;
- comparison between eval runs;
- aggregate quality metrics for a whole batch;
- aggregate token and cost totals;
- report metadata materialization.

## 2) Granularity

One `eval_run_summaries` row represents exactly one `eval_run_id`.

The table must not mix multiple eval runs into one row.

## 3) Required Metadata Fields

Each row must contain at least:

- `eval_run_id`
- `run_type`
- `status`
- `started_at`
- `completed_at`
- `runtime_run_count`
- `iterations_evaluated_count`
- `judge_provider`
- `judge_model`
- `created_at`
- `updated_at`

## 4) Required Quality Aggregate Fields

Each row must contain at least:

- `usable_first_response_rate`
- `query_structuring_judge_score`
- `evidence_pack_judge_score`
- `final_answer_judge_score`
- `query_structuring_strict_pass_rate`
- `evidence_pack_strict_pass_rate`
- `final_answer_strict_pass_rate`
- `diagnostic_move_hard_fail_rate`
- `gate_pass_rate`
- `bad_final_due_to_query_rate`
- `bad_final_due_to_evidence_rate`
- `bad_final_with_good_query_and_evidence_rate`

The exact formulas are defined by the aggregate spec.

## 5) Required Usage And Cost Fields

Each row must contain at least:

- `runtime_prompt_tokens`
- `runtime_completion_tokens`
- `runtime_total_tokens`
- `runtime_total_cost_usd`
- `judge_prompt_tokens`
- `judge_completion_tokens`
- `judge_total_tokens`
- `judge_total_cost_usd`
- `run_total_tokens`
- `run_total_cost_usd`

These fields are required because eval-run comparison dashboards explicitly
depend on aggregated token and cost totals.

## 6) Report-Facing Metadata

The row should also preserve enough report-facing metadata to support concise
run comparisons, including where available:

- suite version map;
- runtime model metadata rolled up from the run scope;
- optional golden dataset identifier or batch label.

The MVP does not require component-comparison-heavy metadata beyond what is
useful for the current project.

## 7) Write Rule

The summary builder must upsert one row for the eval run whenever it computes a
fresh consistent run-level aggregate state.

At terminal success, the row must represent the final aggregate state for the
completed eval run.

The row must remain resumable and idempotent for the same `eval_run_id`.

## 8) Dashboard Role

`eval_run_summaries` is the primary relational source for:

- listing eval runs by time range;
- comparing eval runs on quality and usage;
- showing total runtime usage, judge usage, and run totals.

It is not a replacement for:

- raw `judge_results`;
- raw `judge_llm_calls`;
- markdown `run_report.md`.
