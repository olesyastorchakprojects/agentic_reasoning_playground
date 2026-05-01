# Eval Engine Implementation Progress

## Purpose

This file is a handoff note for continuing work on the Rust eval engine in
`Execution/distributed_diagnostics_eval`.

It summarizes:

- what is already implemented;
- what was validated end-to-end;
- what bugs were discovered and fixed;
- what is still incomplete;
- what the next recommended implementation steps are.

## Current Status

The eval engine is now able to run **one real judge suite end-to-end** against
fresh runtime runs stored in Postgres.

Practical interpretation:

- the **engine skeleton is already working**;
- the project is **past the “build the engine from scratch” stage**;
- the main remaining work is **turning the current one-suite vertical slice into
  a real multi-suite eval engine**.

Implemented and validated path:

1. bootstrap an `eval_run`;
2. freeze eligible runtime subjects;
3. load `RunState` from runtime storage;
4. build an eval snapshot for the frozen iteration;
5. execute the first real judge suite using Together;
6. persist `judge_llm_calls`;
7. persist `judge_results`;
8. build `eval_iteration_summaries`;
9. build `eval_run_summaries`;
10. write `run_report.md`.

## What Was Successfully Run

A real eval run was completed for 5 fresh golden runtime runs.

Completed eval run:

- `eval_run_id = 1bfc954f-e1d1-4e85-9070-bea54bcf242e`

Artifacts:

- [run_manifest.json](/home/olesia/code/dist_sys_assistant/Evidence/evals/runs/2026-05-01T21-24-33.637011839+00-00_1bfc954f-e1d1-4e85-9070-bea54bcf242e/run_manifest.json)
- [run_report.md](/home/olesia/code/dist_sys_assistant/Evidence/evals/runs/2026-05-01T21-24-33.637011839+00-00_1bfc954f-e1d1-4e85-9070-bea54bcf242e/run_report.md)

Currently enabled suite:

- `final_no_root_cause_claim`

## Current Product Meaning

The current engine is **not yet a full eval suite runner**.

It is already valuable as an infrastructure milestone because it proves:

- runtime runs can be discovered from Postgres;
- frozen eval subjects can be resumed safely;
- Together-based judge calls work;
- token and cost accounting works;
- summary tables and report generation work;
- the first judge suite can score a real batch.

But the report is still **one-suite-specific** in practical meaning, because
only `final_no_root_cause_claim` is implemented and enabled.

## What “Done” Means Right Now

It is accurate to say that the **core engine/framework is already in place**.

That includes:

- eval-run bootstrap and resume;
- subject discovery and frozen scope;
- runtime-state loading;
- snapshot projection;
- real judge execution through Together;
- judge result persistence;
- token and cost accounting;
- subject summaries;
- run summaries;
- report generation.

It is **not** accurate to say that only prompt files remain.

What still remains is not “just add prompts”, but:

- add more suite-specific request builders and execution paths;
- generalize summary logic from the current one-suite slice to multi-suite
  behavior;
- ensure report sections remain semantically correct when several suites are
  active;
- then wire the resulting outputs into dashboard consumption.

Short version:

- **engine skeleton:** ready
- **single-suite end-to-end path:** ready
- **full multi-suite eval product behavior:** not ready yet

## Important Fixes Already Made

### 1. Runtime persistence bug fixed

Problem:

- `UserInputReceived` existed in `RunState` but was not persisted to
  `diagnostics.run_step_records`.

Fix:

- [run_repository.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics/src/orchestrator/run_repository.rs:53)
  now persists all existing iteration step records when `append_iteration(...)`
  is called.

Why it mattered:

- eval discovery depends on persisted `UserInputReceived`;
- without it, eligible runtime runs were incorrectly detected as `0`.

Validation:

- `cargo test --manifest-path Execution/distributed_diagnostics/Cargo.toml`
  passed after the fix.

### 2. Eval discovery query fixed

Problem:

- eval discovery expected `golden_question` at top level in `result_json`;
- actual persisted shape is nested under `UserInputReceived`.

Fix:

- [storage.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/storage.rs:231)
  now checks:
  `s.result_json -> 'UserInputReceived' -> 'golden_question'`

Result:

- eligibility became `5` for the 5 fresh runs.

### 3. Judge response parsing made more robust

Problem:

- real judge output came back in a slightly wrapped JSON shape;
- parser expected `score` at the top level only.

Fix:

- [judge.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/judge.rs:252)
  now canonicalizes a single top-level wrapper if the inner object contains
  `score`.

### 4. Postgres numeric read-path fixes

Problem:

- summary/report generation failed when reading `numeric` columns back into Rust
  `f64`.

Fixes:

- [storage.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/storage.rs:647)
  read-path for `judge_llm_calls` casts `numeric` fields to `double precision`;
- [storage.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/storage.rs:817)
  read-path for `eval_iteration_summaries` casts `numeric` fields to
  `double precision`.

### 5. Artifact root corrected

Problem:

- artifacts were initially written under
  `Execution/distributed_diagnostics_eval/Evidence/...`
  because `artifact_root` was resolved relative to `eval.toml`.

Fix:

- [eval.toml](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/eval.toml:17)
  now uses:
  `root_dir = "../../Evidence/evals/runs"`

Result:

- eval artifacts now live in the shared project-level `Evidence/evals/runs`.

## What Is Implemented in the Crate

### Crate bootstrap

- `Cargo.toml`
- `src/lib.rs`
- `src/main.rs`

### Config and CLI

- [config.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/config.rs)
- [cli.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/cli.rs)

Current behavior:

- loads eval TOML config;
- resolves env-backed secrets and Postgres URL;
- supports Together only;
- supports fresh run and resume modes.

### Storage layer

- [storage.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/storage.rs)

Implemented:

- `eval_processing_state`
- `judge_results`
- `judge_llm_calls`
- `eval_iteration_summaries`
- `eval_run_summaries`

### Manifest and orchestrator

- [manifest.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/manifest.rs)
- [orchestrator.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/orchestrator.rs)

Implemented:

- new eval run bootstrap;
- resume by `eval_run_id`;
- subject draining for `judge_request_suites`;
- subject draining for `build_eval_summary`;
- final report generation.

### Runtime loading and snapshot projection

- [runtime_runs.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/runtime_runs.rs)
- [snapshot.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/snapshot.rs)
- [subject_preparation.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/subject_preparation.rs)

Implemented:

- load `RunState` from runtime Postgres storage;
- build snapshot for frozen `iteration_id`;
- reject missing required steps;
- reject failed required steps.

### Suites and judge execution

- [suites.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/suites.rs)
- [judge.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/judge.rs)

Implemented:

- suite catalog loading from `Specification/evals/prompts.json`;
- current implementation slice validation;
- Together judge client;
- first suite request building;
- judge result persistence;
- judge token/cost persistence.

### Summary and report

- [summary.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/summary.rs)
- [report.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/report.rs)

Implemented:

- subject-level summary materialization;
- eval-run summary materialization;
- markdown report generation.

## Current Config

Config file:

- [eval.toml](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/eval.toml)

Important current settings:

- `provider = "together"`
- `enabled = ["final_no_root_cause_claim"]`
- artifact root points to project-level `Evidence/evals/runs`

## Current Constraints / Limitations

### Only one suite is implemented

The current engine only supports:

- `final_no_root_cause_claim`

The suite catalog contains more suites, but the code intentionally restricts the
active implementation slice.

### Reports are structurally general but semantically one-suite-heavy

The report and summary schemas already contain broad fields, but only one suite
currently contributes real signal.

This means some metrics exist mainly for schema compatibility and future use,
not because all upstream suite families are already implemented.

### Runtime cost is still zero in current reports

Runtime token usage is available via runtime outputs, but runtime USD costing is
not yet fully populated. Current reports therefore show runtime token totals but
runtime cost may still be `0.0`.

### No dashboard integration yet

The DB tables are populated, but Grafana/dashboard wiring has not been built for
this new engine yet.

## Tests Passing

Validated commands:

- `cargo test --manifest-path Execution/distributed_diagnostics/Cargo.toml`
- `cargo test --manifest-path Execution/distributed_diagnostics_eval/Cargo.toml`

Latest eval crate test status:

- `26 passed`

## Recommended Next Steps

### Highest-value next step

Implement the **second suite**.

Recommended candidate:

- `final_first_check_discriminates`

Why this one:

- it adds immediate user-facing value;
- it complements `final_no_root_cause_claim`;
- it makes the report meaningfully more diagnostic instead of only checking
  “no overclaim”.

### After that

1. Add `final_result_interpretation_usefulness`
2. Generalize summary logic from one-suite assumptions to multi-suite behavior
3. Expand report sections to reflect real final-answer multi-suite scoring
4. Add query/evidence suites
5. Add Grafana/dashboard consumption

## Suggested Implementation Order From Here

1. Add request builder for `final_first_check_discriminates`
2. Extend `judge.rs` suite dispatch beyond first suite only
3. Update summary scoring logic to aggregate at least two final-answer suites
4. Re-run a fresh 5-case golden batch
5. Inspect the new `run_report.md`

## If Another Model Picks This Up

Start from these files first:

- [judge.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/judge.rs)
- [orchestrator.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/orchestrator.rs)
- [summary.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/summary.rs)
- [storage.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/storage.rs)
- [suites.rs](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/src/suites.rs)
- [eval.toml](/home/olesia/code/dist_sys_assistant/Execution/distributed_diagnostics_eval/eval.toml)

Useful artifact for understanding current output shape:

- [run_report.md](/home/olesia/code/dist_sys_assistant/Evidence/evals/runs/2026-05-01T21-24-33.637011839+00-00_1bfc954f-e1d1-4e85-9070-bea54bcf242e/run_report.md)

Useful specs:

- [Specification/evals](/home/olesia/code/dist_sys_assistant/Specification/evals)

## One-Line Summary

The Rust eval engine is now **operational for one real suite over real runtime
runs**, with persistence, resume, token/cost accounting, summaries, and report
generation all working; the next major task is to turn it from a one-suite
vertical slice into a genuinely multi-suite eval engine.
