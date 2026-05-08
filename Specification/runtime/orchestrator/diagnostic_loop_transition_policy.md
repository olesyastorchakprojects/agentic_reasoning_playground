## 1) Purpose

This document defines the transition-selection boundary for
`orchestrator::transition_policy::diagnostic_loop`.

This policy reads the current persisted run-state iteration and decides what
orchestration should do next for the diagnostic loop pipeline:

- execute the next step;
- finish the current iteration with the final validated result;
- finish the current iteration with the recorded step error.

This module does not:

- mutate `RunState`;
- execute request-pipeline steps;
- persist runs or step history;
- print results or errors;
- create new iterations.

Successful transition selection in this policy is iteration-scoped:

- `PolicyTransition::FinishWithResult` means the current iteration produced a
  final validated diagnostic response;
- `PolicyTransition::FinishWithError` means the current iteration produced a
  terminal recorded step error;
- whether orchestration later opens another iteration for the same run is
  owned outside this policy boundary.

## 2) Generated Rust Artifacts

The generated Rust crate must include:

- `src/orchestrator/transition_policy/diagnostic_loop.rs`

Parent module exposure:

- `src/orchestrator/mod.rs` must expose `transition_policy`;
- `src/orchestrator/transition_policy/mod.rs` must publicly re-export
  `DiagnosticLoopTransitionPolicy`.

All public traits, enums, and methods defined by this spec must be public.

## 3) Module Layout

The generated layout must separate the public policy boundary from the
diagnostic-loop implementation so that additional policies can be added later
without changing the public module path.

Required ownership by file:

- `src/orchestrator/transition_policy/diagnostic_loop.rs` owns:
  - `DiagnosticLoopTransitionPolicy`;
  - `impl TransitionPolicy for DiagnosticLoopTransitionPolicy`;
  - private diagnostic-loop helpers.

## 4) Imports

The generated module requires:

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::orchestrator::run_state::model::{
    RunStatus,
    StepError,
    StepKind,
    StepResultEnvelope,
};
use crate::orchestrator::run_state::view::{FinishedStepView, IterationView, RunStateView};
use crate::shared_types::ResponseValidationAndNormalizationOutput;
```

Exact import paths may be adjusted by the generator to match the generated
crate layout.

## 5) Required Parent-Module Types

The parent module path `crate::orchestrator::transition_policy` must expose the
shared policy boundary used by this diagnostic-loop implementation:

```rust
pub trait TransitionPolicy {
    fn next_transition(
        &self,
        state: RunStateView<'_>,
    ) -> Result<PolicyTransition, PolicyError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PolicyTransition {
    ExecuteStep { step: StepKind },
    WaitForUser {
        follow_up_questions: Vec<String>,
    },
    FinishWithResult {
        result: ResponseValidationAndNormalizationOutput,
    },
    FinishWithError {
        error: StepError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum PolicyError {
    #[error("run is archived")]
    RunArchived,

    #[error("run has no current iteration")]
    NoCurrentIteration,

    #[error("current iteration has a pending step")]
    PendingStepPresent,

    #[error("current iteration contains duplicate successful step records for {step}")]
    DuplicateSuccessfulStep { step: StepKind },

    #[error("current iteration contains a successful step out of canonical order: {step}")]
    StepOutOfOrder { step: StepKind },

    #[error("current iteration is missing required user input")]
    MissingUserInput,

    #[error("current iteration stores an unexpected successful result variant for {step}")]
    UnexpectedStepResult { step: StepKind },
}
```

This spec does not own those shared parent-module definitions. It only depends
on them.

`src/orchestrator/transition_policy/diagnostic_loop.rs` must define:

```rust
#[derive(Debug, Default)]
pub struct DiagnosticLoopTransitionPolicy;
```

## 6) Constructor

The generated module must define:

```rust
impl DiagnosticLoopTransitionPolicy {
    pub fn new() -> Self;
}
```

`new()` must construct a stateless diagnostic-loop policy instance.

## 7) Iteration Index Classification

This policy must choose the executable canonical step order by the current
iteration index.

The current iteration index is defined as:

```rust
let iteration_index = state.iteration_count() - 1;
```

Rules:

- when `state.iteration_count() == 0`, `next_transition` must return
  `Err(PolicyError::NoCurrentIteration)`;
- when `iteration_index == 0`, apply the initial-iteration canonical step order
  defined by section `8)`;
- when `iteration_index > 0`, apply the continuation-iteration canonical step
  order defined by section `9)`.

The policy must not inspect older iterations to choose the order. Only the
index of the current iteration matters.

## 8) Initial-Iteration Canonical Step Order

When `iteration_index == 0`, the policy must use this executable step order:

1. `StepKind::InputNormalization`
2. `StepKind::QueryStructuring`
3. `StepKind::InformationAdequacyInitial`
4. `StepKind::CandidateCardRetrieval`
5. `StepKind::CardHydration`
6. `StepKind::IncidentEvidenceRetrieval`
7. `StepKind::TheoryEvidenceRetrieval`
8. `StepKind::PromptContextAssembly`
9. `StepKind::LlmStructuredGeneration`
10. `StepKind::ResponseValidationAndNormalization`

Initial-iteration adequacy rule:

- when `StepKind::InformationAdequacyInitial` completed successfully with
  `AdequacyAssessment.status = Blocking` or `WeakButRunnable`, this policy
  must return:

```rust
PolicyTransition::WaitForUser {
    follow_up_questions: adequacy.follow_up_questions.clone(),
}
```

- when `StepKind::InformationAdequacyInitial` completed successfully with
  `AdequacyAssessment.status = Sufficient`, this policy must continue to
  `StepKind::CandidateCardRetrieval`.

## 9) Continuation-Iteration Branching Order

When `iteration_index > 0`, the policy must use this continuation-only
branching order:

1. `StepKind::InputNormalization`
2. `StepKind::ObservationBoundaryResolver`
3. branch on `ObservationBoundaryResolverOutput.resolution`
4. either `StepKind::ObservationExtraction` then
   `StepKind::InformationAdequacySupportedObservation`, or directly
   `StepKind::InformationAdequacyUnsupportedObservation`
5. `StepKind::CandidateCardRetrieval`
6. `StepKind::CardBranchReranking`
7. `StepKind::CardHydration`
8. `StepKind::IncidentEvidenceRetrieval`
9. `StepKind::TheoryEvidenceRetrieval`
10. `StepKind::DiagnosticUpdatePromptContextAssembly`
11. `StepKind::LlmStructuredGeneration`
12. `StepKind::ResponseValidationAndNormalization`

Continuation branching rules:

- after a successful `StepKind::ObservationBoundaryResolver` result:
  - when `resolution = Supported(...)`, the next executable step must be
    `StepKind::ObservationExtraction`;
  - when `resolution = Unsupported`, the next executable step must be
    `StepKind::InformationAdequacyUnsupportedObservation`;
- after a successful `StepKind::ObservationExtraction` result, the next
  executable step must be
  `StepKind::InformationAdequacySupportedObservation`;
- after a successful
  `StepKind::InformationAdequacySupportedObservation` result:
  - when `AdequacyAssessment.status = Blocking` or `WeakButRunnable`, this
    policy must return
    `PolicyTransition::WaitForUser { follow_up_questions }` using the exact
    stored `AdequacyAssessment.follow_up_questions`;
  - when `AdequacyAssessment.status = Sufficient`, the next executable step
    must be `StepKind::CandidateCardRetrieval`;
- after a successful
  `StepKind::InformationAdequacyUnsupportedObservation` result, this policy
  must return
  `PolicyTransition::WaitForUser { follow_up_questions }` using the exact
  stored `AdequacyAssessment.follow_up_questions`;

Current-version scope rule:

- `InformationAdequacyUnsupportedObservation` is the only adequacy step valid
  after `ObservationBoundaryResolverOutput.resolution = Unsupported`;
- `InformationAdequacySupportedObservation` is valid only after a successful
  `ObservationExtraction` result.

`StepKind::UserInputReceived` is a required current-iteration anchor record but
is not part of the executable canonical order and must never be returned inside
`PolicyTransition::ExecuteStep`.

## 10) Public Decision API

The generated module must define:

```rust
impl TransitionPolicy for DiagnosticLoopTransitionPolicy {
    fn next_transition(
        &self,
        state: RunStateView<'_>,
    ) -> Result<PolicyTransition, PolicyError>;
}
```

Decision rules:

- `next_transition` must read only from `RunStateView`;
- `next_transition` must inspect only the current iteration;
- `next_transition` must reject an archived run before selecting a transition;
- `next_transition` must validate the current iteration before selecting the
  next transition;
- `next_transition` must not inspect older iterations when the current
  iteration exists;
- `next_transition` must not mutate the supplied run state.

## 11) Required Run-State Reads

`next_transition` must use these read entrypoints from `run_state::view`:

- `state.status()` to inspect the run status;
- `state.iteration_count()` to determine the current iteration index;
- `state.last_iteration()` to load the current iteration;
- `iteration.pending_step()` to detect whether the current iteration has a
  pending step;
- `iteration.finished_step(step_kind)` to load the latest finished record for a
  specific step kind;
- `iteration.finished_steps()` to inspect all finished step records in the
  current iteration when validating canonical order or locating a recorded
  error.

This spec defines the required view methods available to the implementation,
but it does not require one exact iteration algorithm such as reverse
traversal.

## 12) Current Iteration Selection

`next_transition` must:

- call `state.status()`;
- return `Err(PolicyError::RunArchived)` when
  `state.status() == RunStatus::Archived`;
- call `state.last_iteration()`;
- return `Err(PolicyError::NoCurrentIteration)` when it returns `None`;
- otherwise operate only on that returned `IterationView`.

## 13) Pending-Step Rule

`next_transition` must:

- call `iteration.pending_step()`;
- return `Err(PolicyError::PendingStepPresent)` when it returns `Some(_)`.

The policy must not attempt to select a new transition while the current
iteration still contains a pending step.

## 14) Required User Input Rule

The current iteration must contain a successful
`StepKind::UserInputReceived` finished record before any executable-step
decision is made.

`next_transition` must execute this algorithm:

1. call `iteration.finished_step(StepKind::UserInputReceived)`;
2. when it returns `None`, return:

```rust
Err(PolicyError::MissingUserInput)
```

3. otherwise inspect `FinishedStepView::result()`;
4. when it is `Err(...)`, treat that record as missing required user input and
   return:

```rust
Err(PolicyError::MissingUserInput)
```

5. when it is
   `Ok(StepResultEnvelope::UserInputReceived(_))`, continue;
6. when it is `Ok(...)` with any other successful envelope variant, return:

```rust
Err(PolicyError::UnexpectedStepResult {
    step: StepKind::UserInputReceived,
})
```

## 15) Successful-Step Validation

Before selecting the next transition, `next_transition` must validate
successful executable finished steps in the current iteration.

Validation scope:

- only executable canonical-order steps selected by sections `8)` or `9)` are
  validated here;
- `UserInputReceived` is not part of the executable order validation;
- failed finished steps do not participate in this validation.

Validation rules:

- if the current iteration contains more than one successful finished record for
  the same executable `StepKind`, return:

```rust
Err(PolicyError::DuplicateSuccessfulStep { step })
```

- if a successful executable step at canonical position `i` exists in the
  current iteration, but at least one executable canonical step at position
  `j < i` does not have a successful finished record in the current iteration,
  then that successful step violates canonical order;
- when such a violation exists, return:

```rust
Err(PolicyError::StepOutOfOrder { step })
```

where `step` is the violating successful executable step with the smallest
canonical position `i`.

Equivalently, successful executable finished steps in the current iteration
must form a prefix of the active executable canonical order.

- if a successful finished record used by this validation stores an unexpected
  `StepResultEnvelope` variant for its `StepKind`, return:

```rust
Err(PolicyError::UnexpectedStepResult { step })
```

## 16) Terminal Error Transition

If the current iteration contains any finished executable step record whose
`FinishedStepView::result()` is `Err(step_error)`, then:

- `next_transition` must return:

```rust
Ok(PolicyTransition::FinishWithError {
    error: step_error.clone(),
})
```

This transition must return the recorded `StepError` payload directly so that
the orchestrator does not need to re-read and decode the failed step record.

`UserInputReceived` is not part of this terminal error scan. A failed
`UserInputReceived` record is handled earlier by section `14)` as
`PolicyError::MissingUserInput`.

## 17) Terminal Result Transition

If the current iteration contains a finished
`StepKind::ResponseValidationAndNormalization` record whose result is:

```rust
Ok(StepResultEnvelope::ResponseValidationAndNormalization(result))
```

then `next_transition` must return:

```rust
Ok(PolicyTransition::FinishWithResult {
    result: result.clone(),
})
```

This transition must return the final validated output payload directly so that
the orchestrator does not need to re-read and decode the finished step record.

If the current iteration contains a finished
`StepKind::ResponseValidationAndNormalization` record with a successful result
whose envelope variant does not match, `next_transition` must return:

```rust
Err(PolicyError::UnexpectedStepResult {
    step: StepKind::ResponseValidationAndNormalization,
})
```

## 18) Next-Step Selection

If no terminal error transition applies and no terminal result transition
applies, `next_transition` must:

- inspect executable canonical-order steps in order selected by the current
  iteration index;
- determine whether each step already has a successful finished record in the
  current iteration;
- select the first canonical step that does not yet have a successful finished
  record;
- return:

```rust
Ok(PolicyTransition::ExecuteStep { step })
```

`ExecuteStep` must never return:

- `StepKind::UserInputReceived`.

If all canonical-order steps already have successful finished records, the
recorded successful `StepKind::ResponseValidationAndNormalization` result must
already be present, and section `17)` applies.

## 19) Terminal Precedence

Transition-selection precedence is:

1. archived run -> `PolicyError::RunArchived`
2. missing current iteration -> `PolicyError::NoCurrentIteration`
3. pending step present -> `PolicyError::PendingStepPresent`
4. missing required user input or invalid user-input record ->
   `PolicyError::MissingUserInput` or `PolicyError::UnexpectedStepResult`
5. invalid successful-step history ->
   `PolicyError::DuplicateSuccessfulStep`,
   `PolicyError::StepOutOfOrder`, or
   `PolicyError::UnexpectedStepResult`
6. any recorded finished-step error -> `PolicyTransition::FinishWithError`
7. recorded successful final response ->
   `PolicyTransition::FinishWithResult`
8. otherwise -> `PolicyTransition::ExecuteStep { step }`

Structural anomalies in current-iteration history have priority over a recorded
terminal `StepError`, because they indicate persisted-state corruption or an
invalid execution shape rather than an ordinary business failure of one step.

## 20) Borrowing And Cloning Rules

`next_transition` must prefer borrowed reads from `RunStateView`.

Cloning rules:

- cloning the terminal `StepError` is allowed when returning
  `PolicyTransition::FinishWithError`;
- cloning the terminal
  `ResponseValidationAndNormalizationOutput` is allowed when returning
  `PolicyTransition::FinishWithResult`;
- `next_transition` must not clone the whole `RunState`;
- `next_transition` must not materialize an owned iteration snapshot solely for
  convenience.

## 21) Private Helper Allowances

The generated implementation may define private helpers for:

- computing the current iteration index;
- selecting the canonical step order from the current iteration index;
- loading the current `IterationView`;
- checking whether a step has a successful finished record;
- decoding successful step payload variants;
- locating any recorded finished-step error;
- validating canonical-order success history;
- selecting the next missing successful canonical-order step.

Private helpers must not expose new public runtime APIs.

## 22) Unit-Test Ownership

Required unit tests for this policy are owned by:

- `Specification/runtime/unit_tests.md`
- `Specification/runtime/unit_tests_common.md`

This document defines runtime behavior and API contracts only. It must not be
treated as the source of truth for the crate-level required unit-test list.

## 23) Ownership Boundaries

- this document owns the transition-selection contract for the diagnostic-loop
  orchestration policy;
- it must not define step execution behavior;
- it must not define persistence behavior;
- it must not redefine `run_state` model or view contracts;
- it must not define CLI-loop behavior or terminal output printing behavior.
