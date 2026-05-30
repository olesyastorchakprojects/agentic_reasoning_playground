## 1) Purpose

`judge_request_suites` is the generic suite-driven judge worker for the
diagnostic eval engine.

This stage:

- selects the next eligible frozen subject for the current eval run;
- loads the corresponding `RunState`;
- derives `DiagnosticEvalIterationSnapshot`;
- executes all required missing applicable judge suites for that subject;
- writes factual judge-call usage rows;
- writes normalized judge verdict rows;
- promotes the subject to `build_eval_summary` after applicable suites are
  complete.

## 2) Stage Shape

The stage remains one generic iteration-level judge stage covering:

- initial-only suites;
- continuation-only suites;
- shared suites.

The current code path does not split these into separate worker types.

## 3) Subject Identity

The canonical subject identity is:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`

All judge result rows are keyed by that identity plus `suite_name`.

## 4) Applicability Rules

Applicability is driven by the suite catalog field `applies_to`.

Current semantics:

- suites marked applicable to initial iterations run only for `initial`;
- suites marked applicable to continuation iterations run only for
  `continuation`;
- shared suites run for both kinds.

The stage must skip suites whose `applies_to` classification does not match the
current snapshot iteration kind.

## 5) Missing-Suite Detection

Before issuing a judge call, the stage checks whether a normalized
`judge_results` row already exists for:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `suite_name`

Only missing suites are executed.

## 6) Current Supported Suite Set

The current implementation supports suite request construction for at least:

- `query_structuring_field_boundary_correctness`
- `query_structuring_grounding_conservatism`
- `evidence_pack_role_fit`
- `evidence_pack_sufficiency`
- `final_no_root_cause_claim`
- `final_first_check_discriminates`
- `final_hypothesis_source_alignment`
- `final_alternative_context_handling`
- `final_result_interpretation_usefulness`
- `continuation_hypothesis_update_discipline`
- `continuation_problem_understanding_update`
- `continuation_next_check_progression`
- `continuation_observation_resolution_context_recovery`

Unknown suite names are a hard error.

## 7) Retry Behavior

Current judge-call retry behavior is intentionally narrow:

- the stage performs one extra retry when the judge client returns an error
  indicating empty content;
- broader transport retry policy remains provider-owned through the judge client
  configuration.

## 8) Usage And Result Persistence

For every factual judge call that returns a response, the stage must write one
`judge_llm_calls` row.

For every successfully normalized suite result, the stage must upsert one
`judge_results` row.

Current `call_id` format:

- `<eval_run_id>:<iteration_id>:<suite_name>`

## 9) Stage-Local Failure Rule

If subject preparation or suite execution fails:

- the subject remains in the current stage;
- processing-state status becomes `failed`;
- `last_error` is updated;
- the subject remains resumable while it still has remaining attempts.
