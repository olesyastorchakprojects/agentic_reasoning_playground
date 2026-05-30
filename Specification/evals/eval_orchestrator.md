## 1) Purpose

`eval_orchestrator` coordinates the eval-owned execution stages for one
diagnostics eval run.

It currently owns:

- new-run bootstrap;
- frozen subject discovery handoff through storage;
- eval processing-state bootstrap;
- one-subject stage execution;
- stage draining for the current eval run;
- subject promotion from judge stage to summary stage.

Final CLI-level report writing and run-summary refresh currently happen outside
the orchestrator in `main.rs`.

## 2) Current Public Interface

The current orchestrator exposes stage-oriented methods rather than one single
`run_eval_orchestrator(...)` entrypoint.

The important current boundaries are equivalent in ownership to:

- `bootstrap_new_eval_run() -> BootstrapResult`
- `drain_judge_request_suites_for_eval_run(eval_run_id, ...) -> JudgeDrainResult`
- `drain_build_eval_summary_for_eval_run(eval_run_id, ...) -> SummaryDrainResult`

## 3) Required Parameters And Dependencies

The orchestrator is constructed from:

- fully resolved `EvalSettings`;
- an eval-owned storage implementation.

Its stage methods additionally receive:

- `eval_run_id`
- runtime-run loader
- suite catalog
- judge client

depending on the stage.

## 4) Bootstrap Contract

At bootstrap for a new eval run, the orchestrator must:

1. generate one `eval_run_id`;
2. ask storage to discover eligible frozen subjects;
3. fail if no eligible subjects exist;
4. build the initial manifest view;
5. create the artifact directory;
6. write the initial `run_manifest.json`;
7. bootstrap the corresponding `eval_processing_state` rows.

The returned `BootstrapResult` must include at least:

- `eval_run_id`
- `started_at`
- `artifact_dir`
- `manifest_path`
- `runtime_run_count`
- `subject_count`

## 5) Discovery Semantics

The current frozen-scope discovery behavior is storage-defined and must be
reflected accurately here.

Current implementation semantics:

- discovery starts from runtime runs that contain at least one iteration with a
  finished `ResponseValidationAndNormalization` result;
- runtime runs are ordered by runtime-run creation time descending, then
  `runtime_run_id` descending;
- the optional limit applies at runtime-run selection time;
- after selecting eligible runtime runs, all completed iterations inside those
  runs that satisfy the same final-output condition become frozen eval subjects.

Important note:

- this differs from the older one-target-iteration-per-run design;
- the current implementation freezes one or more iteration subjects per
  selected runtime run.

## 6) Resume Contract

The current implementation re-enters stage draining through the CLI, using the
existing `eval_run_id`.

Required semantics:

- resume must reuse the same `eval_run_id`;
- resume must reuse existing `eval_processing_state` rows;
- resume must not bootstrap a new frozen scope;
- stage drains must operate only on subjects already belonging to that eval run.

The current orchestrator does not itself load or rewrite the manifest before
resume; that work is currently coordinated by the CLI.

## 7) Stage Order

The current stage order remains:

1. `judge_request_suites`
2. `build_eval_summary`

Promotion from the first stage to the second is orchestrator-owned.

## 8) Stage Drain Rule

The orchestrator may invoke a stage repeatedly until storage returns no next
eligible subject for that stage.

For the current implementation, a subject is considered stage-eligible when:

- `current_stage` matches the requested stage;
- `status` is one of `pending`, `running`, or `failed`;
- `attempt_count` is below the stage retry ceiling.

Current retry ceiling:

- `MAX_ATTEMPTS_PER_STAGE = 2`

## 9) Judge Stage Promotion Rule

After all required applicable suites for a subject are complete, the
orchestrator promotes that subject to:

- `current_stage = build_eval_summary`
- `status = pending`
- `attempt_count = 0`

This promotion is keyed by the same frozen subject identity:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`

## 10) Subject Failure Handling

During stage execution:

- subject-preparation failures mark the subject `failed` in the current stage;
- judge execution failures mark the subject `failed` in the current stage;
- stage drains continue to the next subject unless a non-stage-local storage or
  manifest failure aborts the run.

This means a drain result may contain both completed and failed subject counts
for the same pass.

## 11) Current Completion Boundary

The orchestrator currently owns subject-level completion for both stages, but
run-level completion is still finalized outside the orchestrator.

Today the end-to-end success path is:

1. orchestrator bootstraps a new eval run;
2. orchestrator drains `judge_request_suites`;
3. orchestrator drains `build_eval_summary`;
4. CLI rebuilds the run-level summary row;
5. CLI writes `run_report.md`.

This is the current source of truth and should be reflected in the spec even if
future refactoring consolidates finalization back into the orchestrator.
