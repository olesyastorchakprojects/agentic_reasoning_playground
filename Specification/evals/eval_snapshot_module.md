## 1) Purpose

This document defines the module contract for the eval crate's `snapshot`
module.

This module is the adapter boundary between runtime-owned `RunState` and the
eval-owned `DiagnosticEvalIterationSnapshot`.

## 2) Responsibilities

The snapshot module owns:

- selecting the target iteration from a `RunState`;
- reading finished step outputs from that iteration;
- validating required fields for MVP snapshot construction;
- building `DiagnosticEvalIterationSnapshot`;
- exposing suite-payload helper views derived from the snapshot.

## 3) Non-Responsibilities

The snapshot module must not own:

- SQL persistence;
- judge transport;
- orchestration state transitions;
- markdown rendering.

## 4) Public Types

The module should expose at least:

- `DiagnosticEvalIterationSnapshot`
- `SnapshotBuildError`
- `TargetIterationSelector`

It may also expose narrower payload-builder helper types if that keeps the
judge module simpler.

## 5) Public Interfaces

The module should expose one primary build entrypoint conceptually equivalent
to:

```rust
fn build_snapshot(
    run_state: &RunState,
    selector: &TargetIterationSelector,
) -> Result<DiagnosticEvalIterationSnapshot, SnapshotBuildError>
```

The implementation may also expose:

- `select_target_iteration(...)`
- `build_eval_context(...)`

## 6) Input Boundary

The snapshot module consumes runtime-domain types from
`distributed_diagnostics`, especially:

- `RunState`
- `RunIteration`
- `StepKind`
- `StepResultEnvelope`

It should not require eval-owned SQL rows to build the snapshot.

## 7) Output Boundary

The snapshot output must be stable enough that:

- judge suites consume snapshot-derived values rather than raw `RunState`;
- future additions to runtime internals do not force widespread judge code
  churn.

The snapshot is therefore the eval crate's main in-memory boundary type.

## 8) Dependency Rules

The snapshot module may depend on:

- runtime-domain types from `distributed_diagnostics`

It should not depend on:

- `storage`
- `orchestrator`
- `report`

The `judge` module may depend on `snapshot`, not the other way around.

## 9) Failure Semantics

If required runtime step outputs are missing or malformed for the targeted
subject, the snapshot module must fail explicitly with structured error
information.

It must not fabricate partial successful snapshots for required MVP suites.

