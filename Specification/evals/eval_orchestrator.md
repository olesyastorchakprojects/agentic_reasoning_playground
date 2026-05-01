## 1) Purpose

`eval_orchestrator` coordinates one complete diagnostics eval run.

It owns:

- eval-run bootstrap;
- manifest lifecycle;
- frozen runtime-run scope creation;
- eval processing-state bootstrap;
- stage invocation in pipeline order;
- stage promotion between eval-owned stages;
- run completion detection;
- final report materialization boundary.

It must not:

- execute suite prompts inline;
- normalize judge results inline;
- build logical iteration snapshots inline;
- mutate runtime-owned `RunState`.

## 2) Public Interface

The orchestrator must expose one top-level eval-run entrypoint.

The canonical current-version boundary should be equivalent in ownership to:

```python
run_eval_orchestrator(params: EvalOrchestratorParams) -> str
```

The returned string is the `eval_run_id` of the completed or resumed eval run.

## 3) Required Parameters

The orchestrator must receive explicit parameters including at least:

- `postgres_url`
- `run_type`
- `eval_config_path`
- `resume_eval_run_id`

The orchestrator must resolve judge runtime settings from the eval config and
must write the effective judge metadata into the run manifest.

## 4) Bootstrap Contract

At bootstrap for a new eval run, the orchestrator must:

1. discover eligible runtime runs for batch evaluation;
2. derive the target iteration subject for each runtime run;
3. freeze the resulting subject scope;
4. generate one `eval_run_id`;
5. create the initial manifest;
6. create the corresponding `eval_processing_state` rows.

The frozen scope must be persisted in the manifest as
`run_scope_runtime_run_ids` and `run_scope_subjects`.

## 5) Runtime-Run Eligibility

For the current MVP, the orchestrator must discover eligible runtime runs from
the completed golden-dataset batch outputs.

The exact SQL/storage source may evolve, but the semantic rule is:

- a runtime run is eligible for a new eval run only if it is a completed
  golden-dataset runtime run with the required final validated output for the
  target iteration;
- and it has not already been absorbed into another eval run's frozen scope.

The orchestrator must not create overlapping batch membership silently.

## 5.1) Target Iteration Rule

For the current MVP, the orchestrator must select exactly one target iteration
per runtime run:

- default selector: the last completed iteration in the runtime run;
- the selected `(runtime_run_id, iteration_id)` pairs must be frozen into the
  manifest as `run_scope_subjects`;
- resume must reuse this exact subject scope rather than recomputing it from
  mutated runtime state.

## 6) Resume Contract

When `resume_eval_run_id` is provided, the orchestrator must:

- load the existing manifest;
- require that its status is `running` or `failed`;
- reuse the exact frozen runtime-run scope and frozen subject scope from the
  manifest;
- reuse the same `eval_run_id`;
- reuse existing `eval_processing_state` rows for that eval run;
- re-enter the stage pipeline without redefining batch membership.

The orchestrator must not add newly created runtime runs to a resumed eval run.

## 7) Stage Order

The current MVP stage order is:

1. `judge_request_suites`
2. `build_eval_summary`

The orchestrator must invoke stages in that order and must not skip an
intermediate stage in a normal run.

## 8) Stage Drain Rule

The orchestrator may invoke a stage repeatedly until no eligible work remains
for that stage in the current eval run.

For the current MVP, no work remains for a stage iff there are no
`eval_processing_state` rows for the current `eval_run_id` and `current_stage`
with:

- `status = pending`
- or `status = running`
- or `status = failed`

that still require work after downstream idempotency checks.

## 9) Promotion Rule

After a subject reaches:

- `current_stage = judge_request_suites`
- `status = completed`

the orchestrator must promote that subject to:

- `current_stage = build_eval_summary`
- `status = pending`

unless that subject has already been promoted or already completed later-stage
work in a previous partial attempt.

Promotion must respect the same frozen subject identity.

## 10) Run Completion Rule

The orchestrator may mark the eval run completed only when:

- every subject in the frozen scope has reached
  `current_stage = build_eval_summary` and `status = completed`;
- final eval-run summaries have been materialized successfully;
- `run_report.md` has been written successfully from the final frozen-scope
  aggregate state;
- the manifest has been updated to terminal success.

## 11) Failure Boundary

The orchestrator must expose a clear run-level failure boundary.

When the orchestrator exits through terminal failure:

- the manifest must be updated to `status = failed`;
- `last_error` must be non-empty;
- partial downstream artifacts already written must remain preserved;
- the eval run must remain resumable if the underlying failure mode is
  operationally recoverable.

## 12) Required MVP Responsibilities

The current MVP orchestrator must also own:

- writing `run_manifest.json`;
- validating its structure before writing;
- writing `run_report.md` after summary completion;
- preserving token/cost visibility through the summary/report layer;
- keeping batch membership reproducible and inspectable.
