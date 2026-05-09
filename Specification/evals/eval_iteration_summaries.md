## 1) Purpose

This document defines the iteration-level summary storage contract for the
diagnostic eval engine.

`eval_iteration_summaries` is the canonical summary table for one evaluated
iteration subject after all required judge suites have completed.

Its role is to provide:

- one compact row per evaluated iteration;
- suite score rollups;
- category score rollups;
- gate outcomes;
- user-facing usability signals;
- iteration-level token and cost rollups.

It is the natural bridge between raw `judge_results` and higher-level eval-run
aggregates.

## 2) Granularity

One `eval_iteration_summaries` row represents exactly one:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`

combination.

The table must not aggregate multiple iterations into one row.

## 3) Required Identity Fields

Each row must contain at least:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `iteration_kind`
- `created_at`
- `updated_at`

## 4) Required Summary Fields

Each row must contain at least:

- `query_structuring_judge_score`
- `evidence_pack_judge_score`
- `final_answer_judge_score`
- `query_structuring_no_hard_fail`
- `evidence_pack_no_hard_fail`
- `final_answer_no_hard_fail`
- `usable_first_response`
- `usable_continuation_response`
- `no_root_cause_gate_passed`
- `single_check_gate_passed`
- `source_alignment_gate_passed`
- `field_boundary_gate_passed`
- `evidence_pack_gate_passed`
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

The table may also include denormalized per-suite score columns for the current
MVP suite set.

For continuation iterations, the table should additionally expose explicit
per-suite score columns for the enabled continuation suite set.

## 5) Per-Suite Score Fields

For the current MVP, the iteration summary should expose explicit per-suite
score columns for at least:

- `query_structuring_field_boundary_correctness_score`
- `query_structuring_grounding_conservatism_score`
- `evidence_pack_role_fit_score`
- `evidence_pack_sufficiency_score`
- `final_no_root_cause_claim_score`
- `final_first_check_discriminates_score`
- `final_hypothesis_source_alignment_score`
- `final_alternative_context_handling_score`
- `final_result_interpretation_usefulness_score`

This denormalization is justified because:

- the MVP suite set is intentionally small and stable;
- reports and dashboards benefit from easy direct access to these scores;
- the raw canonical suite details still live in `judge_results`.

## 6) Category Score Semantics

Category score fields are averages over the corresponding suite scores for that
iteration.

For the current MVP:

- `query_structuring_judge_score` averages the query-structuring suites;
- `evidence_pack_judge_score` averages the evidence-pack suites;
- `final_answer_judge_score` averages the final-answer suites.

The current aggregate spec is the source of truth for the exact formulas.

## 7) Gate Fields

The iteration summary must persist the boolean outcomes of critical gates.

For the current MVP the required gate booleans are:

- `no_root_cause_gate_passed`
- `single_check_gate_passed`
- `source_alignment_gate_passed`
- `field_boundary_gate_passed`
- `evidence_pack_gate_passed`

These gate booleans must be derived directly from suite scores according to the
aggregate rules.

## 8) Usability Signal

`usable_first_response` is a first-class iteration-level boolean summary signal
for `iteration_kind = initial`.

Its purpose is to answer:

- can this iteration's final response plausibly be shown to a user as a useful
  first diagnostic move?

The exact formula belongs to the aggregate specification, but the resulting
boolean must be stored here for efficient downstream use.

`usable_continuation_response` is the corresponding first-class iteration-level
boolean summary signal for `iteration_kind = continuation`.

Its purpose is to answer:

- can this iteration's updated response plausibly be shown to a user as a
  useful diagnostic update after the new observation?

Exactly one of these usability booleans may be meaningful for a given
iteration-kind contract; both may be stored for schema stability.

## 9) Usage And Cost Fields

Iteration-level usage fields must respect the token/cost accounting contract:

- runtime usage comes from runtime-owned projected values;
- judge usage comes from `judge_llm_calls`;
- run totals are the sum of those two domains.

Required formulas:

- `run_total_tokens = runtime_total_tokens + judge_total_tokens`
- `run_total_cost_usd = runtime_total_cost_usd + judge_total_cost_usd`

## 10) Write Rule

The summary builder must upsert one row only after:

- all required suites are present for the subject;
- iteration-level rollups have been computed successfully.

The table must be resumable and idempotent for the same
`(eval_run_id, runtime_run_id, iteration_id)` key.

The summary semantics must remain iteration-kind-aware even though the storage
key remains the same for initial and continuation iterations.
