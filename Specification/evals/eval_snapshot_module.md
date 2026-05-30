## 1) Purpose

This document defines the current module contract for the eval crate's
`snapshot` module.

This module is the adapter boundary between runtime-owned `RunState` and the
eval-owned `DiagnosticEvalIterationSnapshot`.

## 2) Responsibilities

The snapshot module owns:

- selecting the target iteration from a `RunState`;
- reading required finished step outputs from that iteration;
- classifying the iteration as `initial` or `continuation`;
- building `DiagnosticEvalIterationSnapshot`;
- recovering prior-iteration context for continuation subjects;
- projecting runtime token/cost usage into eval-owned summary structures.

## 3) Public Types

The current public boundary includes at least:

- `DiagnosticEvalIterationSnapshot`
- `SnapshotBuildError`
- `SnapshotIterationSelector`
- `RuntimeTokenUsageSummary`
- `RuntimeLlmStageUsageSummary`

## 4) Target Selection

The current selector set is:

- `LastCompletedIteration`
- `ExactIteration(iteration_id)`

The primary build boundary is equivalent to:

```rust
fn build_snapshot(
    run_state: &RunState,
    selector: SnapshotIterationSelector,
) -> Result<DiagnosticEvalIterationSnapshot, SnapshotBuildError>
```

## 5) Iteration-Kind Detection

The current implementation classifies an iteration as `continuation` when the
iteration contains a finished `ObservationBoundaryResolver` step record.

Otherwise the iteration is classified as `initial`.

This detection rule is part of the current snapshot contract.

## 6) Snapshot Shape

The current snapshot contains at least:

- run identity and timestamps;
- `user_request`;
- optional `golden_question`;
- `normalized_user_request`;
- retrieval and prompt-context outputs;
- final validated response output;
- optional continuation-only outputs:
  - `observation_boundary_resolver_output`
  - `observation_extraction_output`
- optional `previous_snapshot`;
- optional runtime `config_snapshot`;
- aggregated runtime token/cost usage.

## 7) Continuation Semantics

For continuation iterations, snapshot construction must additionally:

- find the directly preceding completed iteration in the same run;
- recursively build the previous snapshot for that iteration;
- require continuation-specific step outputs;
- use `DiagnosticUpdatePromptContextAssembly` output instead of the initial
  prompt-context assembly path.

Current implementation detail:

- for continuation iterations, `query_structuring_output` is taken from the
  previous completed snapshot rather than recomputed from the current
  continuation iteration.

## 8) Runtime Usage Projection

The snapshot module is currently the owner of projected runtime usage fields
used later by summary materialization.

The projection includes stage-level usage for:

- `query_structuring`
- `observation_boundary_resolver`
- `observation_extraction`
- `llm_structured_generation`
- `total`

Current continuation rule:

- continuation snapshots record zero token usage for `query_structuring` in the
  current iteration, because the current iteration reuses the prior structured
  query rather than executing a fresh query-structuring step.

## 9) Failure Semantics

Snapshot construction must fail explicitly when:

- no completed iteration exists for `LastCompletedIteration`;
- the exact requested iteration does not exist;
- a continuation iteration has no previous completed iteration;
- a required step is missing;
- a required step failed;
- a step produced an unexpected payload type.

The module must not fabricate partial successful snapshots for required suites.
