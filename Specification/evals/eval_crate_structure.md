## 1) Purpose

This document defines the code-structure contract for the new diagnostics eval
engine crate.

It exists to answer:

- where the crate lives;
- what its top-level modules are;
- which responsibilities stay inside the eval crate;
- which runtime types are reused from `distributed_diagnostics`;
- what the first implementation slice should be.

This is a module-boundary spec, not a behavioral pipeline spec.

## 2) Crate Identity

The eval engine should live in a dedicated sibling crate:

- `Execution/distributed_diagnostics_eval/`

The crate should expose:

- one library target for reusable eval logic;
- one binary target as the CLI entrypoint.

Recommended files:

- `src/lib.rs`
- `src/main.rs`

## 3) High-Level Shape

The crate should keep business logic out of `main.rs`.

Recommended split:

- `main.rs`
  - parse CLI arguments
  - load config
  - initialize observability
  - invoke orchestrator
- `lib.rs`
  - expose the internal modules needed by tests and by the binary

## 4) Top-Level Modules

The crate should start with these top-level modules:

- `config`
- `orchestrator`
- `storage`
- `snapshot`
- `suites`
- `judge`
- `summary`
- `report`
- `observability`
- `errors`

Not every module needs to be fully implemented on day one, but the namespace
should reflect these stable architectural boundaries.

## 5) Module Responsibilities

### 5.1) `config`

Owns:

- eval config structs
- config loading
- env resolution
- config validation

Must not own:

- orchestration
- report rendering
- SQL writes

### 5.2) `orchestrator`

Owns:

- eval-run bootstrap
- frozen scope creation
- resume
- stage draining
- manifest lifecycle
- finalization

Must not own:

- direct judge prompt logic
- direct markdown formatting logic

### 5.3) `storage`

Owns:

- PostgreSQL repositories
- DDL-facing row models
- idempotent writes
- selection queries for processing state

Must not own:

- prompt building
- aggregate formulas

### 5.4) `snapshot`

Owns:

- projection from `RunState` to `DiagnosticEvalIterationSnapshot`
- target iteration selection helpers
- snapshot validation

Must not own:

- SQL persistence
- judge transport

### 5.5) `suites`

Owns:

- suite catalog loading
- suite metadata
- enabled-suite filtering
- suite-specific input-variable declarations

Must not own:

- transport execution
- summary aggregation

### 5.6) `judge`

Owns:

- suite execution loop for one subject
- judge transport abstraction
- normalized output parsing
- factual usage extraction

Must not own:

- eval-run lifecycle
- final report rendering

### 5.7) `summary`

Owns:

- iteration summary construction
- eval-run summary construction
- aggregate formulas
- materialized rollup building

Must not own:

- suite transport
- CLI parsing

### 5.8) `report`

Owns:

- markdown report rendering
- report section formatting
- worst-case preview formatting

Must not own:

- raw SQL selection
- prompt execution

### 5.9) `observability`

Owns:

- eval-specific tracing setup helpers
- span naming helpers
- metrics labeling helpers

### 5.10) `errors`

Owns:

- crate-local error enums
- cross-module error composition where useful

## 6) Reused Runtime Types

The eval crate should reuse runtime-domain types from
`distributed_diagnostics` rather than redefining them locally.

The reused set should include at least:

- `RunState`
- `RunIteration`
- `RunId`
- `RunIterationId`
- `StepKind`
- `StepRecord`
- `StepResultEnvelope`
- `UserRequest`
- `GoldenQuestion`
- `QueryStructuringOutput`
- `ModelTokenUsage`

This reuse means the eval crate uses the same semantic contracts as the
runtime crate when reading runtime-produced artifacts.

## 7) Eval-Only Types

The eval crate should define its own types for eval-owned concerns, including
at least:

- `EvalSettings`
- `EvalRunManifest`
- `EvalProcessingStateRow`
- `DiagnosticEvalIterationSnapshot`
- `JudgeResultRow`
- `JudgeLlmCallRow`
- `EvalIterationSummaryRow`
- `EvalRunSummaryRow`

These types must remain eval-owned and must not be pushed back into the
runtime crate unless they become genuinely shared infrastructure.

## 8) Dependency Boundary

The eval crate may depend on `distributed_diagnostics`.

The recommended dependency direction is:

- `distributed_diagnostics_eval` depends on `distributed_diagnostics`
- `distributed_diagnostics` must not depend on the eval crate

This keeps runtime execution independent from offline eval concerns.

## 9) First Implementation Slice

The first implementation slice should be vertical rather than broad.

Recommended order:

1. `config`
2. `main.rs` CLI
3. `storage.eval_processing_state`
4. `orchestrator` bootstrap/resume
5. `snapshot`
6. one suite end to end
7. `judge_llm_calls`
8. `judge_results`
9. minimal report

The early goal is not all 9 suites. The early goal is one working end-to-end
path across all major layers.

