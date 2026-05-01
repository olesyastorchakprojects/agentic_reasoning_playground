## 1) Purpose

This document defines the module contract for the eval crate's `orchestrator`
module.

This module is the lifecycle owner for one eval run.

## 2) Responsibilities

The orchestrator module owns:

- creating new eval runs;
- resuming existing eval runs;
- discovering eligible runtime runs for new batch evals;
- freezing subject scope;
- writing and updating the run manifest;
- bootstrapping `eval_processing_state`;
- draining stages in pipeline order;
- detecting terminal success or terminal failure;
- materializing the final run report boundary.

## 3) Non-Responsibilities

The orchestrator module must not own:

- raw SQL row mapping details;
- `RunState` to snapshot projection internals;
- suite prompt text construction;
- per-suite normalization logic;
- markdown section formatting internals.

Those belong to `storage`, `snapshot`, `judge`, and `report`.

## 4) Public Interfaces

The module should expose a top-level orchestrator type, such as:

- `EvalOrchestrator`

and a startup entrypoint equivalent in meaning to:

```rust
async fn run(params: EvalOrchestratorParams) -> Result<EvalRunOutcome, EvalOrchestratorError>
```

Recommended supporting public types:

- `EvalOrchestratorParams`
- `EvalRunOutcome`
- `EvalOrchestratorError`

## 5) Required Inputs

The orchestrator must consume:

- resolved `EvalSettings`
- storage repositories or repository traits
- suite catalog access
- judge runner entrypoint
- summary builder entrypoint
- report writer entrypoint

It may also consume observability helpers and a clock abstraction if useful.

## 6) Required Outputs

The orchestrator must produce or ensure production of:

- `eval_processing_state` bootstrap rows
- final manifest state
- final `eval_run_summaries` state
- final `run_report.md`

It returns an `EvalRunOutcome` to the CLI boundary.

## 7) Internal Submodules

The orchestrator module may internally split into:

- `manifest`
- `bootstrap`
- `resume`
- `drain`
- `finalize`

This split is optional, but the responsibilities should remain visible even if
the implementation stays in fewer files initially.

## 8) Dependency Rules

The orchestrator may depend on:

- `config`
- `storage`
- `snapshot`
- `suites`
- `judge`
- `summary`
- `report`

The orchestrator should be the highest-level business module in the crate.

Lower-level modules must not depend back on the orchestrator module.

## 9) State-Machine Ownership

The orchestrator is the owner of:

- frozen eval-run scope;
- subject promotion between stages;
- run-level completion checks;
- run-level failure boundary.

Individual stage modules may update one subject's status, but only the
orchestrator may declare the eval run globally completed.

