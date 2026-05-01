## 1) Purpose

This document defines the module contract for the eval crate's `summary`
module.

This module is the materialization boundary for iteration-level and eval-run-
level aggregate state.

## 2) Responsibilities

The summary module owns:

- computing iteration-level summary rows from `judge_results` and usage data;
- computing eval-run-level aggregate rows from iteration summaries;
- applying aggregate formulas from the eval aggregate spec;
- enforcing required-suite completeness before materialization;
- refreshing materialized summary tables in an idempotent way.

## 3) Non-Responsibilities

The summary module must not own:

- eval-run bootstrap or resume;
- raw judge transport calls;
- `RunState` loading;
- markdown report formatting internals.

## 4) Public Types

The module should expose types conceptually equivalent to:

- `IterationSummaryBuilder`
- `RunSummaryBuilder`
- `SummaryBuildResult`
- `SummaryModuleError`

It may also expose internal aggregate helper types for:

- gate results
- category rollups
- usage rollups
- failure-attribution rollups

## 5) Public Interfaces

The module should expose one subject-level summary entrypoint conceptually
equivalent to:

```rust
async fn build_iteration_summary(
    subject_key: &EvalSubjectKey,
    context: &SummaryBuildContext,
) -> Result<SummaryBuildResult, SummaryModuleError>
```

and one eval-run-level refresh entrypoint conceptually equivalent to:

```rust
async fn refresh_run_summary(
    eval_run_id: EvalRunId,
    context: &SummaryBuildContext,
) -> Result<(), SummaryModuleError>
```

## 6) Inputs

The summary module consumes:

- `judge_results`
- `judge_llm_calls`
- runtime usage values projected for the subject
- suite catalog metadata where needed
- aggregate rules defined by the current eval design

The summary module should not need raw prompt text or raw runtime step
internals beyond already-projected usage inputs.

## 7) Outputs

The summary module persists:

- `eval_iteration_summaries`
- `eval_run_summaries`

It may also return structured rollup results to the orchestrator or report
module if that simplifies finalization.

## 8) Dependency Rules

The summary module may depend on:

- `storage`
- `suites`
- `config`

It should not depend on:

- `orchestrator`
- `judge`
- `report`

The report module may consume summary outputs, not the other way around.

## 9) Completeness Rules

The summary module must refuse to materialize a successful iteration summary
when required suites are missing for that subject.

It must also avoid producing a misleading final eval-run summary if the frozen
scope has not yet reached the required subject-level completion boundary.

## 10) Formula Ownership

The summary module is the code-level owner of:

- gate formulas
- category average formulas
- usable-first-response formulas
- failure-attribution formulas
- judge/runtime/run-total usage formulas

These formulas should remain centralized here rather than being duplicated in
orchestrator or report code.

