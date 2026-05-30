## 1) Purpose

This document defines the current iteration-level summary storage contract for
the diagnostics eval engine.

`eval_iteration_summaries` is the canonical compact row for one evaluated
iteration subject after the required suites for that subject have completed.

## 2) Granularity

One row represents exactly one:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`

combination.

## 3) Required Identity Fields

Each row contains at least:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `iteration_kind`

## 4) Required Judge Rollup Fields

Each row contains at least:

- `query_structuring_judge_score`
- `evidence_pack_judge_score`
- `final_answer_judge_score`
- `query_structuring_no_hard_fail`
- `evidence_pack_no_hard_fail`
- `final_answer_no_hard_fail`
- `usable_first_response`
- `no_root_cause_gate_passed`
- `single_check_gate_passed`
- `source_alignment_gate_passed`
- `field_boundary_gate_passed`
- `evidence_pack_gate_passed`

## 5) Continuation-Specific Fields

For continuation-aware storage stability, the row also contains optional
continuation fields:

- `continuation_hypothesis_update_discipline_score`
- `continuation_problem_understanding_update_score`
- `continuation_next_check_progression_score`
- `continuation_observation_resolution_context_recovery_score`
- `usable_continuation_response`
- `continuation_update_no_hard_fail`
- `continuation_input_no_hard_fail`

For initial iterations these values are `NULL` / not applicable.

## 6) Per-Suite Score Fields

The current row shape includes explicit per-suite score columns for:

- query-structuring suites;
- evidence-pack suites;
- shared final-answer suites;
- continuation suites.

This denormalization is intentional because the current suite set is small and
reporting depends on direct access to these fields.

## 7) Runtime Gold And Retrieval Metrics

The current row shape also stores projected runtime metrics JSON for:

- query structuring;
- candidate card retrieval;
- primary incident evidence retrieval;
- alternative incident evidence retrieval;
- theory evidence retrieval.

These are eval-facing runtime diagnostics and are part of the current summary
contract, not just transient report helpers.

## 8) Runtime Model And Prompt Metadata

The current row also preserves runtime-facing metadata including:

- runtime model names for relevant stages;
- runtime prompt versions for relevant stages;
- per-stage runtime token counts;
- per-stage runtime cost fields.

This metadata exists so that run-level aggregation and later comparisons can
explain quality changes alongside runtime configuration shifts.

## 9) Usage And Cost Fields

The row contains:

- runtime prompt/completion/total tokens and total cost;
- judge prompt/completion/total tokens and total cost;
- run-total tokens and total cost.

Required formulas remain:

- `run_total_tokens = runtime_total_tokens + judge_total_tokens`
- `run_total_cost_usd = runtime_total_cost_usd + judge_total_cost_usd`

## 10) Current Formula Semantics

Current implementation semantics include:

- category scores average only suites actually present for that subject;
- suite scores default to `0` only for denormalized storage fields when the
  suite is absent;
- gate booleans default to pass when the corresponding suite is not applicable
  or not present;
- continuation usability requires positive continuation update scores and the
  shared final-answer usability prerequisites.

## 11) Write Rule

The summary builder currently requires at least one judge result for the
subject. The intended stronger future rule remains:

- all required applicable suites should be present before a successful summary
  row is materialized.

The row must remain idempotent for the same subject key.
