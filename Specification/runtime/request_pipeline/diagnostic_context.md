## 1) Purpose / Scope

This document defines the `DiagnosticContext` shared type and its supporting types for the multi-iteration diagnostic pipeline.

`DiagnosticContext` is the diagnostic projection of a `RunState`. It is a structured, versioned view of what the system currently knows about the diagnostic case: the evolving problem understanding, the hypothesis set and its state history, the collected observations, and the suggested checks produced across iterations.

`DiagnosticContext` is not raw chat history. It is a domain-typed projection derived from the relevant step results stored in `RunState.iterations`.

The application lifecycle is driven by the hypothesis set:
- the first normal iteration seeds the hypothesis set and produces the first
  check;
- subsequent normal iterations update hypothesis states based on new
  observations and produce the next check.

This document is the source of truth for:
- the `DiagnosticContext` struct and all its supporting types;
- the `HypothesisEvidenceSource` typed enum replacing the former string source field;
- construction rules from a `RunState`, including precise field mappings to source step result types;
- view methods and their precise semantics;
- type invariants that must hold at all times.

This document does not define:
- how `ObservationBoundaryResolver` resolves observations — that belongs to its module specification;
- how the diagnostic update step produces hypothesis state transitions — that belongs to its module specification;
- orchestration policy or when `DiagnosticContext` is built during a run;
- persistence of `DiagnosticContext` as a storage artifact.

The `RunState`, `RunId`, and `RunIterationId` types used here are defined by:
- `Specification/runtime/orchestrator/run_state/model.md`
- `Specification/runtime/orchestrator/run_state/view.md`

The `ResponseValidationAndNormalizationOutput` and `DiagnosticResponse` types are defined by:
- `Specification/runtime/request_pipeline/response_validation_and_normalization.md`

The generated Rust module file for the current version is:
- `src/shared_types/diagnostic_context.rs`

---

## 2) Generated Rust Artifact

The generated Rust crate must include:

- `src/shared_types/diagnostic_context.rs`

Parent module exposure:

- `src/shared_types/mod.rs` must declare `mod diagnostic_context;` as a **private** submodule (not `pub mod`) and re-export all public items from this module via `pub use diagnostic_context::{...}`.
- External code must import `DiagnosticContext` and its supporting types only through `crate::shared_types`, never through `crate::shared_types::diagnostic_context` directly.

All types and methods defined by this spec must be public within the module.

---

## 3) Required Imports

The generated module resides in `src/shared_types/diagnostic_context.rs`. Sibling shared types are imported via `super::`. Orchestrator types are imported via `crate::`.

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::orchestrator::run_state::model::{
    RunId,
    RunIterationId,
    RunState,
    StepKind,
    StepResultEnvelope,
};
use crate::orchestrator::run_state::view::RunStateView;
use super::{
    Confidence,
    Hypothesis,
    HypothesisEvidenceSource,
    HypothesisId,
    HypothesisStatus,
    NormalizedUserRequest,
    ObservationBoundaryResolution,
    ObservationBoundaryResolverOutput,
    ResolvedObservation,
    ResponseValidationAndNormalizationOutput,
};
```

Exact import paths may be adjusted by the generator to match the generated crate layout.

---

## 4) Prerequisites

The following types have been added to `Specification/runtime/orchestrator/run_state/model.md` and are available for use:

- `StepKind::ObservationBoundaryResolver` in the `StepKind` enum;
- `StepResultEnvelope::ObservationBoundaryResolver(ObservationBoundaryResolverOutput)` in the `StepResultEnvelope` enum.

`ObservationBoundaryResolverOutput` is defined as a shared type in `Specification/runtime/runtime.md`.

---

## 5) Public Types

### 5.1 DiagnosticContext

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticContext {
    pub run_id: RunId,
    pub problem_understanding: Vec<ProblemUnderstanding>,
    pub hypotheses: Vec<TrackedHypothesis>,
    pub observations: Vec<Observation>,
    pub suggested_checks: Vec<SuggestedCheck>,
}
```

Fields:
- `run_id` — the run this context belongs to;
- `problem_understanding` — history of problem understanding entries, one per contributing closed iteration, in iteration order;
- `hypotheses` — the full set of tracked hypotheses for the run, each carrying its full state history across iterations;
- `observations` — all collected observations, in iteration order, one entry per contributing iteration;
- `suggested_checks` — all suggested checks produced across iterations, in iteration order.

### 5.2 ProblemUnderstanding

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProblemUnderstandingSource {
    InitialRequest(String),
    DiagnosticUpdate {
        problem_understanding: String,
        observation: Option<Observation>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemUnderstanding {
    pub iteration_id: RunIterationId,
    pub text: Option<String>,
    pub source: ProblemUnderstandingSource,
}
```

`ProblemUnderstanding.text` is the model's understanding output produced by this iteration's `DiagnosticResponse.problem_understanding`. It is `None` while the iteration is not yet closed (the `ResponseValidationAndNormalization` step has not completed). It is `Some(text)` once the iteration closes. This field exists on the struct for all variants — including `InitialRequest`.

`ProblemUnderstandingSource::InitialRequest(query)` is used only for the first
normal iteration. It carries the normalized user query from
`NormalizedUserRequest.query` in that iteration's `InputNormalization` step
result.

`ProblemUnderstandingSource::DiagnosticUpdate` is used for all normal
iterations after the first normal iteration:

- `problem_understanding: String` — the model's output from the previous
  closed normal iteration (N-1). Taken from `ProblemUnderstanding.text` of the
  prior entry once it was closed. This is the understanding that was used as
  input context when the model ran in the current iteration. Always non-empty
  because later normal iterations can only start after the prior normal
  iteration is closed.

- `observation: Option<Observation>` — the observation resolved in the current iteration by `ObservationBoundaryResolver`, embedded using the shared `Observation` type. `None` when `ObservationBoundaryResolver` has not run for this iteration, or when it ran and returned `ObservationBoundaryResolution::Unsupported`.

Reading pattern: to build the prompt for iteration N, read `problem_understanding[N-1].text` as the previous understanding and `problem_understanding[N].source.observation` as the new observation once it is available.

Note: the `Observation.status` embedded in `DiagnosticUpdate.observation` reflects the state at construction time. The authoritative `ObservationStatus` lives in `DiagnosticContext.observations`.

### 5.3 TrackedHypothesis

`HypothesisStatus`, `HypothesisEvidenceSource`, `Confidence`, and `HypothesisId` are shared types defined in `Specification/runtime/runtime.md` and imported via `super::` since this module is a private submodule of `shared_types`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisState {
    pub iteration_id: RunIterationId,
    pub status: HypothesisStatus,
    pub confidence: Confidence,
    pub source: HypothesisEvidenceSource,
    pub problem_understanding: ProblemUnderstanding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedHypothesis {
    pub hypothesis_id: HypothesisId,
    pub text: String,
    pub state_history: Vec<HypothesisState>,
}
```

`HypothesisState` fields:
- `iteration_id` — the iteration in which this hypothesis was introduced or
  its state was updated. In the first normal iteration it was added; in a
  later normal iteration it may have been weakened, rejected, or re-evaluated;
- `status` — the hypothesis status as returned by the model in that iteration;
- `confidence` — the model-assessed confidence in that iteration;
- `source` — the evidence origin for this hypothesis state in that iteration;
- `problem_understanding` — the `ProblemUnderstanding` from the same iteration where this state was recorded. Embedded directly so the state is self-sufficient; callers do not need to look up `DiagnosticContext.problem_understanding` separately.

`TrackedHypothesis` fields:
- `hypothesis_id` — stable identity across all iterations;
- `text` — the full hypothesis statement, set when the hypothesis is first introduced and not changed by subsequent state transitions;
- `state_history` — the ordered list of recorded states, one entry per contributing iteration. Must be non-empty.

### 5.4 Observation

`ResolvedObservation` is the shared type defined in `Specification/runtime/runtime.md` and imported via `super::` since this module is a private submodule of `shared_types`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationStatus {
    Pending,
    Processed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub iteration_id: RunIterationId,
    pub normalized_user_input: String,
    pub resolved: ResolvedObservation,
    pub status: ObservationStatus,
}
```

`Observation` fields:
- `iteration_id` — the iteration in which this observation was produced;
- `normalized_user_input` — the normalized user message, preserved from `ObservationBoundaryResolverOutput.normalized_user_input`;
- `resolved` — the context-enriched observation, taken from `ObservationBoundaryResolution::Supported(ResolvedObservation)` in the resolver output;
- `status` — `Pending` for the most recent observation; `Processed` for all earlier observations.

### 5.5 SuggestedCheck

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedCheck {
    pub iteration_id: RunIterationId,
    pub text: String,
}
```

`SuggestedCheck` fields:
- `iteration_id` — the iteration that produced this check suggestion;
- `text` — the suggested check text as returned by the model in the iteration's diagnostic response.

---

## 6) Construction from RunState

```rust
impl DiagnosticContext {
    pub fn from_run_state(run_state: &RunState) -> Result<Self, DiagnosticContextError>;
}
```

`from_run_state` constructs a `DiagnosticContext` by traversing
`RunStateView::new(run_state).normal_iterations()` in order and projecting the
relevant step results into diagnostic domain types.

An empty `RunState` (no iterations) must produce a valid empty `DiagnosticContext`:
- `run_id` set from `RunState.run_id`;
- all `Vec` fields empty.

This is not an error.

### 6.1 First Normal Iteration: Initial Diagnostic Response

For the first normal iteration in the run, `from_run_state` must project the
following step results.

**ProblemUnderstanding** — from `StepResultEnvelope::InputNormalization` and `StepResultEnvelope::ResponseValidationAndNormalization`:

```
ProblemUnderstanding {
    iteration_id: iteration_0.iteration_id,
    text: Some(response.problem_understanding.clone()),   // ← DiagnosticResponse.problem_understanding (iter 0)
    source: ProblemUnderstandingSource::InitialRequest(
        normalized_request.query.clone()                  // ← NormalizedUserRequest.query
    ),
}
```

`text` is `None` when only `InputNormalization` has completed and the model has not yet run; it becomes `Some(...)` once `ResponseValidationAndNormalization` completes.

Source field paths:
- `StepResultEnvelope::InputNormalization(NormalizedUserRequest).query` → `source = InitialRequest(...)`
- `StepResultEnvelope::ResponseValidationAndNormalization(...).response.problem_understanding` → `text`

**Hypotheses** — from `StepResultEnvelope::ResponseValidationAndNormalization`:

For each item in `response.hypotheses`:

```
TrackedHypothesis {
    hypothesis_id: hypothesis.id,             // ← Hypothesis.id from response
    text: hypothesis.text.clone(),            // ← Hypothesis.text
    state_history: vec![HypothesisState {
        iteration_id: iteration_0.iteration_id,
        status: hypothesis.status.clone(),    // ← Hypothesis.status
        confidence: hypothesis.confidence,    // ← Hypothesis.confidence
        source: hypothesis.source,            // ← Hypothesis.source
        problem_understanding: <the ProblemUnderstanding built above>,
    }],
}
```

Source field paths:
- `StepResultEnvelope::ResponseValidationAndNormalization(ResponseValidationAndNormalizationOutput).response.hypotheses[*].id` → `TrackedHypothesis.hypothesis_id`
- `StepResultEnvelope::ResponseValidationAndNormalization(...).response.hypotheses[*].text` → `TrackedHypothesis.text`
- `StepResultEnvelope::ResponseValidationAndNormalization(...).response.hypotheses[*].status` → `HypothesisState.status`
- `StepResultEnvelope::ResponseValidationAndNormalization(...).response.hypotheses[*].confidence` → `HypothesisState.confidence`
- `StepResultEnvelope::ResponseValidationAndNormalization(...).response.hypotheses[*].source` → `HypothesisState.source`

**SuggestedCheck** — from `StepResultEnvelope::ResponseValidationAndNormalization`:

```
SuggestedCheck {
    iteration_id: iteration_0.iteration_id,
    text: response.first_check.clone(),   // ← DiagnosticResponse.first_check
}
```

Source field path: `StepResultEnvelope::ResponseValidationAndNormalization(...).response.first_check`

No `Observation` is produced from the first normal iteration.

### 6.2 Subsequent Iterations: Observation and Diagnostic Update

For each normal iteration after the first normal iteration,
`from_run_state` must project two step result kinds.

**Entry creation rule:** a `ProblemUnderstanding` entry for iteration N is created if and only if at least one of `ObservationBoundaryResolver` or `ResponseValidationAndNormalization` produced a successful result in that iteration. If both are absent or failed, no entry is created for iteration N and the iteration contributes nothing to `DiagnosticContext.problem_understanding`. Individual field values (`text`, `source.observation`) remain `None` for the steps that did not complete.

**Observation** — from `StepResultEnvelope::ObservationBoundaryResolver` when `resolver_output.resolution = ObservationBoundaryResolution::Supported(resolved)`:

```
Observation {
    iteration_id: iteration_n.iteration_id,
    normalized_user_input: resolver_output.normalized_user_input.clone(), // ← ObservationBoundaryResolverOutput.normalized_user_input
    resolved: resolved.clone(),                               // ← ObservationBoundaryResolution::Supported(resolved)
    status: ObservationStatus::Pending,  // set to Processed by status rule — see section 6.3
}
```

Source field paths:
- `StepResultEnvelope::ObservationBoundaryResolver(ObservationBoundaryResolverOutput).normalized_user_input` → `Observation.normalized_user_input`
- `StepResultEnvelope::ObservationBoundaryResolver(...).resolution = Supported(resolved)` → `Observation.resolved`
- `StepResultEnvelope::ObservationBoundaryResolver(...).resolution = Unsupported` contributes no `Observation` to `DiagnosticContext.observations`

**ProblemUnderstanding** — built from step results of both the current and previous iterations:

```
ProblemUnderstanding {
    iteration_id: iteration_n.iteration_id,
    text: Some(response.problem_understanding.clone()),        // ← DiagnosticResponse from iter N (None until closed)
    source: ProblemUnderstandingSource::DiagnosticUpdate {
        problem_understanding: prev_entry.text.unwrap(),       // ← ProblemUnderstanding.text from iter N-1 (always Some for closed iter)
        observation: Some(Observation {                        // ← ObservationBoundaryResolverOutput from iter N (None until step completes)
            iteration_id: iteration_n.iteration_id,
            normalized_user_input: resolver_output.normalized_user_input.clone(),
            resolved: resolved.clone(),
            status: ObservationStatus::Pending,
        }),
    },
}
```

Source field paths:
- `StepResultEnvelope::ResponseValidationAndNormalization` of **iteration N** → `DiagnosticResponse.problem_understanding` → `ProblemUnderstanding.text`
- `ProblemUnderstanding.text` of **iteration N-1** (already closed) → `DiagnosticUpdate.problem_understanding`
- `StepResultEnvelope::ObservationBoundaryResolver` of **iteration N** with `resolution = Supported(...)` → `ObservationBoundaryResolverOutput` → `DiagnosticUpdate.observation`
- `StepResultEnvelope::ObservationBoundaryResolver` of **iteration N** with `resolution = Unsupported` leaves `DiagnosticUpdate.observation = None`

**Handling `prev_entry.text = None`:** if the previous iteration's `ProblemUnderstanding.text` is `None` (its model step failed or was skipped), `from_run_state` must return `Err(DiagnosticContextError::InvalidStepPayload)` for the current iteration rather than propagating the `None` into `DiagnosticUpdate.problem_understanding`. A `DiagnosticUpdate` entry must never be constructed with an empty prior understanding string.

**Hypothesis state updates** — from `StepResultEnvelope::ResponseValidationAndNormalization`:

The model returns the full current hypothesis set in `DiagnosticResponse.hypotheses`. For each `hypothesis` in `response.hypotheses`:

- find the existing `TrackedHypothesis` in `self.hypotheses` by `hypothesis.id`;
- if found: append one new `HypothesisState` to its `state_history`:

```
HypothesisState {
    iteration_id: iteration_n.iteration_id,
    status: hypothesis.status.clone(),    // ← Hypothesis.status
    confidence: hypothesis.confidence,    // ← Hypothesis.confidence
    source: hypothesis.source,            // ← Hypothesis.source
    problem_understanding: <the ProblemUnderstanding built above for this iteration>,
}
```

- if not found: create a new `TrackedHypothesis` with `hypothesis.id` and an initial `HypothesisState` (a new hypothesis was introduced in this iteration).

Source field paths for each `hypothesis` in `response.hypotheses`:
- `hypothesis.id` → match key for existing `TrackedHypothesis` or new `TrackedHypothesis.hypothesis_id`
- `hypothesis.text` → new `TrackedHypothesis.text` (when creating)
- `hypothesis.status` → `HypothesisState.status`
- `hypothesis.confidence` → `HypothesisState.confidence`
- `hypothesis.source` → `HypothesisState.source`

**SuggestedCheck** — from `StepResultEnvelope::ResponseValidationAndNormalization`:

```
SuggestedCheck {
    iteration_id: iteration_n.iteration_id,
    text: response.first_check.clone(),   // ← DiagnosticResponse.first_check
}
```

### 6.3 Observation Status Rule

Observation status is determined by position in the iteration sequence:

- the observation from the most recently processed iteration is `Pending`;
- all observations from earlier iterations are `Processed`.

When `from_run_state` finishes traversing all normal iterations, it must set
the last element of `self.observations` to `Pending` and all earlier elements
to `Processed`.

### 6.4 Missing or Failed Step Results

`from_run_state` must silently skip any step contribution whose step record is absent from an iteration or whose `FinishedStepRecord.result` is `Err(_)`. Failed iterations do not contribute to the diagnostic context. Construction must not fail because of failed or incomplete iterations.

`from_run_state` must also silently skip iterations whose
`IterationView::status()` is `RunIterationStatus::FinishedWithError` or
`RunIterationStatus::FinishedWithWaitInput`. Clarification-only short
iterations and error-finished iterations do not contribute problem
understanding, hypotheses, observations, or suggested checks to
`DiagnosticContext`.

`from_run_state` must return `Err(DiagnosticContextError::InvalidStepPayload { ... })` only when a step result is present, successful, and deserialized, but its payload cannot be projected into the expected diagnostic domain types — for example, a required field is unexpectedly absent in the structured payload.

---

## 7) View Methods

```rust
impl DiagnosticContext {
    pub fn current_problem_understanding(&self) -> Option<&ProblemUnderstanding>;

    pub fn active_hypotheses(&self) -> Vec<&TrackedHypothesis>;

    pub fn rejected_hypotheses(&self) -> Vec<&TrackedHypothesis>;

    pub fn last_check(&self) -> Option<&SuggestedCheck>;

    pub fn current_observation(&self) -> Option<&Observation>;
}
```

### `current_problem_understanding()`

Returns the last element of `self.problem_understanding`, or `None` if the vec is empty.

This is the most recent problem understanding, corresponding to the latest closed iteration that produced a problem understanding entry.

### `active_hypotheses()`

Returns all `TrackedHypothesis` entries in `self.hypotheses` whose `state_history` is non-empty and whose last `HypothesisState.status` is `Active` or `Weakened`.

Preserves the order of hypotheses in `self.hypotheses`.

### `rejected_hypotheses()`

Returns all `TrackedHypothesis` entries in `self.hypotheses` whose `state_history` is non-empty and whose last `HypothesisState.status` is `Rejected(_)`.

Preserves the order of hypotheses in `self.hypotheses`.

### `last_check()`

Returns the last element of `self.suggested_checks`, or `None` if the vec is empty.

This is the most recent suggested diagnostic check produced by the latest iteration that emitted one.

### `current_observation()`

Returns the last element of `self.observations`, or `None` if the vec is empty.

This is the most recently collected observation, corresponding to the latest iteration that produced a successful `ObservationBoundaryResolver` result. Its `status` is always `Pending` by the observation status rule.

---

## 8) Error Boundary

```rust
#[derive(Debug, Error)]
pub enum DiagnosticContextError {
    #[error("invalid step payload in iteration {iteration_id:?}: {reason}")]
    InvalidStepPayload {
        iteration_id: RunIterationId,
        reason: String,
    },
}
```

`InvalidStepPayload` — returned when a present and successful step result payload cannot be projected into the expected diagnostic domain types. Must carry the `iteration_id` of the offending iteration and a human-readable `reason`.

---

## 9) Type Invariants

**Ordering invariants:**
- `problem_understanding` entries are in iteration sequence order — earlier iterations appear first;
- `observations` entries are in iteration sequence order — earlier iterations appear first;
- `suggested_checks` entries are in iteration sequence order — earlier iterations appear first;
- `state_history` entries within each `TrackedHypothesis` are in iteration sequence order — earlier iterations appear first.
- only normal iterations may contribute entries to any `DiagnosticContext`
  collection;
- short iterations must contribute no entries to `problem_understanding`,
  `observations`, `suggested_checks`, or any hypothesis `state_history`.

**Uniqueness invariants:**
- `problem_understanding` has at most one entry per `iteration_id`;
- `observations` has at most one entry per `iteration_id`;
- `suggested_checks` has at most one entry per `iteration_id`;
- `hypotheses` has unique `hypothesis_id` values across all `TrackedHypothesis` entries.

**TrackedHypothesis invariants:**
- every `TrackedHypothesis.state_history` is non-empty — a hypothesis with no recorded state must not appear in `DiagnosticContext.hypotheses`;
- `HypothesisState.problem_understanding` must have an `iteration_id` that appears in `DiagnosticContext.problem_understanding`.

**Observation status invariant:**
- at most one `Observation` in `self.observations` has `status = Pending`;
- that entry, if present, is the last element of `self.observations`;
- all earlier entries have `status = Processed`.

**ProblemUnderstanding ordering invariant:**
- the first entry in `problem_understanding`, when present, has `source = InitialRequest(_)`;
- all subsequent entries have `source = DiagnosticUpdate { ... }`;
- `ProblemUnderstanding.text` is `None` only for the last entry when the current iteration has not yet completed `ResponseValidationAndNormalization`;
- for any entry before the last, `text` must be `Some`;
- `DiagnosticUpdate.problem_understanding` is always non-empty — it is populated from the prior entry's `text`, which must be `Some` for any closed iteration;
- `DiagnosticUpdate.observation` is `None` when `ObservationBoundaryResolver` did not run or returned `Unsupported` for that iteration; there is no ordering constraint requiring prior entries to have `Some`.

---

## 10) Testing Ownership

Unit-test ownership for runtime modules is defined by:
- `Specification/runtime/unit_tests.md`

Required unit-test cases for this module must be defined in:
- `Specification/runtime/unit_tests.md` section `4.16) diagnostic_context`

---

## 11) Non-Goals

For the current version, this type must not:
- persist itself directly to PostgreSQL — persistence remains a repository concern;
- contain raw prompt text, model wire payloads, or token usage metadata;
- define orchestration transitions or decide when to invoke pipeline steps;
- define observation extraction into multiple discrete facts — that belongs to the `ObservationExtraction` module;
- define how `ObservationBoundaryResolver` resolves raw input to `ResolvedObservation.text`.
