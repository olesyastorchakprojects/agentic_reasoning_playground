## 1) Purpose

This document defines the continuation-iteration eval design for the offline
diagnostic eval engine.

It extends the existing eval model so that later iterations in one runtime run
can be evaluated as diagnostic updates rather than as standalone first
responses.

The design goal is to let the eval system answer both:

- whether the first iteration produced a useful first diagnostic move;
- whether later iterations correctly incorporated new observations and improved
  or degraded the diagnosis.

## 2) Core Distinction

The eval engine must distinguish two iteration kinds:

- `initial`
- `continuation`

An `initial` iteration is judged as a standalone first diagnostic response.

A `continuation` iteration is judged as a transition:

- prior diagnostic state;
- new user observation or check result;
- updated diagnostic state.

This distinction is mandatory because the same final-answer rubric is not
enough for continuation quality.

The suite catalog must therefore distinguish suite applicability explicitly
using:

- `shared`
- `initial_only`
- `continuation_only`

This applicability classification belongs to the suite definition itself, not
only to runtime config.

## 3) Continuation Eval Subject

The canonical continuation eval subject is:

- one `runtime_run_id`;
- one completed `iteration_id`;
- the immediately prior completed iteration in the same runtime run;
- the continuation-specific runtime step outputs for the current iteration;
- the updated validated response for the current iteration.

Conceptually, the subject is:

`prior diagnostic state -> new observation -> updated diagnostic state`

The judge must be able to inspect all three parts.

## 4) Required Continuation Snapshot Additions

The continuation snapshot must expose at least:

- `iteration_kind`
- `previous_iteration_id`
- `previous_response`
- `previous_active_hypotheses`
- `previous_first_check`
- `observation_boundary_resolver_output`
- `observation_extraction_output`
- `card_branch_reranking_output`
- `diagnostic_update_prompt_context_output`
- `current_response`

The snapshot must preserve runtime-owned typed outputs rather than judge-owned
reinterpretations.

## 5) MVP Continuation Judge Suites

The first continuation eval slice should introduce suites for:

- `continuation_observation_extraction_fidelity`
- `continuation_problem_understanding_update`
- `continuation_hypothesis_update_discipline`
- `continuation_next_check_progression`
- `continuation_result_interpretation_alignment`

The recommended first vertical slice is:

- `continuation_hypothesis_update_discipline`

because it directly evaluates whether the system actually incorporates new
observations into the evolving diagnosis rather than merely rephrasing the
previous answer.

The practical trimmed continuation MVP suite set is:

- `continuation_hypothesis_update_discipline`
- `continuation_problem_understanding_update`
- `continuation_next_check_progression`

These continuation-only suites are intended to run alongside the existing
`shared` final-answer suites.

These suites together should answer:

- was the new observation understood correctly;
- was it integrated into the updated problem framing;
- did hypothesis ranking and status change appropriately;
- did the next suggested check progress the diagnosis;
- did the result-interpretation logic stay aligned with the updated
  hypotheses.

## 6) Optional Second-Wave Continuation Suites

After the MVP continuation slice, the eval engine may add:

- `continuation_observation_resolution_context_recovery`
- `continuation_card_reranking_stability`
- `continuation_update_context_role_fit`
- `continuation_update_context_sufficiency`
- `continuation_competing_interpretation_quality`
- `continuation_no_regression_from_prior_state`
- `continuation_no_premature_convergence`

These are valuable, but they should not block the first usable continuation
eval layer.

## 7) Iteration Summary Expectations

Each evaluated iteration must still produce exactly one iteration-summary row.

That row must additionally preserve:

- `iteration_kind`
- an iteration-kind-appropriate usability signal
- continuation-specific suite scores when `iteration_kind = continuation`

For continuation iterations, the summary should answer:

- was the updated response usable;
- where quality was lost: observation understanding, update discipline, next
  check, or interpretation logic.

## 8) Run Summary Expectations

Run-level aggregates must separate:

- initial-iteration quality;
- continuation-iteration quality.

The run-level summary must not collapse all iterations into one
`usable_first_response_rate`-style metric only.

At minimum, the aggregate layer should expose:

- initial usable-response rate;
- continuation usable-response rate;
- continuation observation-quality aggregate;
- continuation update-quality aggregate.

## 9) Report Expectations

`run_report.md` should evolve to show:

- one per-iteration section keyed by `iteration_id`;
- iteration kind for each section;
- iteration-appropriate suite results;
- one full-picture section summarizing the whole trajectory of the runtime run
  or eval batch.

For continuation iterations, the report should explicitly show:

- the new observation;
- the prior leading hypotheses;
- the updated leading hypotheses;
- the next check selected after the update.

## 10) Storage Compatibility

The current eval storage identity model is already compatible with
continuation-iteration evaluation because it keys rows by:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`

Therefore, the primary required work is not a relational identity redesign but
rather:

- snapshot expansion;
- suite-catalog expansion;
- summary semantic expansion;
- report semantic expansion.
