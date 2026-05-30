## 1) Purpose

This document defines the current eval-run-level summary storage contract for
the diagnostics eval engine.

`eval_run_summaries` is the canonical aggregate row for one eval run.

## 2) Granularity

One row represents exactly one `eval_run_id`.

## 3) Required Metadata Fields

Each row contains at least:

- `eval_run_id`
- `run_type`
- `status`
- `started_at`
- `completed_at`
- `runtime_run_count`
- `iterations_evaluated_count`
- `judge_provider`
- `judge_model`

## 4) Runtime Configuration Rollups

The current row also preserves runtime-facing rollups including:

- `runtime_query_structuring_model`
- `runtime_observation_boundary_resolver_model`
- `runtime_observation_extraction_model`
- `runtime_llm_structured_generation_model`
- prompt-version fields for the same runtime stages

These fields are part of the current contract because the report and comparison
surfaces use them directly.

## 5) Quality Aggregate Fields

The current row contains at least:

- `usable_first_response_rate`
- `query_structuring_judge_score`
- `evidence_pack_judge_score`
- `final_answer_judge_score`
- `query_structuring_no_hard_fail_rate`
- `evidence_pack_no_hard_fail_rate`
- `final_answer_no_hard_fail_rate`
- `query_structuring_strict_pass_rate`
- `evidence_pack_strict_pass_rate`
- `final_answer_strict_pass_rate`
- `diagnostic_move_hard_fail_rate`
- `gate_pass_rate`
- `bad_final_due_to_query_rate`
- `bad_final_due_to_evidence_rate`
- `bad_final_with_good_query_and_evidence_rate`

## 6) Continuation Aggregate Fields

When continuation iterations are present, the row also stores optional
continuation aggregates including:

- `usable_continuation_response_rate`
- `continuation_update_judge_score`
- `continuation_input_judge_score`
- `continuation_update_no_hard_fail_rate`
- `continuation_update_strict_pass_rate`
- `continuation_input_no_hard_fail_rate`
- `continuation_input_strict_pass_rate`
- per-suite continuation score averages

These remain `NULL` when no continuation iterations were evaluated.

## 7) Runtime Gold And Retrieval Aggregates

The current row also stores runtime aggregate diagnostics including:

- query-structuring core success rate;
- query-structuring macro precision/recall style metrics;
- grounded strict recall;
- retrieval mean nDCG;
- strict and soft retrieval recall success rates;
- retrieval zero-hit rate;
- branch-level strict recall aggregates for candidate cards, primary incident,
  alternative incident, and theory evidence.

These are part of the current run-summary contract, not merely dashboard-only
derived values.

## 8) Usage And Cost Fields

The row contains:

- runtime token/cost totals;
- judge token/cost totals;
- run-total token/cost totals;
- stage-level runtime token/cost totals for major runtime model stages.

## 9) Report-Facing Metadata

The current row also stores:

- `suite_versions` as JSON;
- runtime metadata needed by `run_report.md`.

Current implementation note:

- `suite_versions` currently reflects enabled-suite metadata and should not yet
  be interpreted as a perfectly audited prompt-version record.
