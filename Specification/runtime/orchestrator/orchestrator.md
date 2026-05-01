## 1) Purpose

This document defines the orchestration lifecycle boundary for
`orchestrator::orchestrator`.

`orchestrator` owns the public run-driving entrypoints and the canonical
orchestration loop for the MVP runtime.

It must:

- create a new run and begin its first iteration from `UserRequest`;
- resume an existing run without creating a new iteration;
- resume an existing run by appending a new iteration from fresh
  `UserRequest`;
- repeatedly ask `TransitionPolicy` for the next transition;
- open pending steps through `RunStateWriter` and `CurrentIterationWriter`;
- execute steps through `StepExecutor`;
- persist run progress through `RunRepository`;
- stop only when policy surfaces the final validated result or the recorded
  step error.

It must not:

- implement request-pipeline leaf logic inline;
- decide the next step outside `TransitionPolicy`;
- mutate `RunState` inline without going through `run_state::apply`;
- embed PostgreSQL storage details directly.

This document is the source of truth not only for the public orchestrator API,
but also for the canonical control-flow shape of the orchestration loop.

## 2) Generated Rust Artifact

The generated Rust crate must include:

- `src/orchestrator/orchestrator.rs`

Parent module exposure:

- `src/orchestrator/mod.rs` must expose `orchestrator`.

All public structs, enums, and methods defined by this spec must be public.

## 3) Imports

The generated module requires:

```rust
use thiserror::Error;

use crate::orchestrator::run_repository::{RunRepository, RunRepositoryError};
use crate::orchestrator::run_state::apply::{RunStateWriter, StateApplyError};
use crate::orchestrator::run_state::model::{
    FinishedStepRecord,
    RunId,
    RunState,
    StepError,
    StepKind,
    StepRecord,
};
use crate::orchestrator::run_state::view::RunStateView;
use crate::orchestrator::step_executor::StepExecutor;
use crate::orchestrator::transition_policy::{
    PolicyError,
    PolicyTransition,
    TransitionPolicy,
};
use crate::shared_types::{
    Context,
    OpenInferenceContext,
    ResponseValidationAndNormalizationOutput,
    UserRequest,
};
```

Exact import paths may be adjusted by the generator to match the generated
crate layout.

## 4) Public Types

The generated module must define:

```rust
#[derive(Debug)]
pub struct Orchestrator<P> {
    policy: P,
    executor: StepExecutor,
    run_repository: RunRepository,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunOutcome {
    Finished {
        run_id: RunId,
        result: ResponseValidationAndNormalizationOutput,
    },
    Failed {
        run_id: RunId,
        error: StepError,
    },
}

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("run not found: {run_id:?}")]
    RunNotFound { run_id: RunId },

    #[error("policy error: {0}")]
    Policy(#[from] PolicyError),

    #[error("state-application error: {0}")]
    StateApply(#[from] StateApplyError),

    #[error("run repository error: {0}")]
    Repository(#[from] RunRepositoryError),

    #[error("pending step for {step:?} was expected but not found")]
    MissingPendingStep { step: StepKind },
}
```

Design rules:

- `RunOutcome` is the public terminal result of one orchestration invocation;
- `RunOutcome::Finished` means that the current orchestration invocation
  produced a user-facing validated response and stopped at that boundary; it
  does not mean that the persisted run is closed, exhausted, or archived;
- the current MVP does not define a public wait outcome because the active MVP
  linear policy finishes with either a validated response or a recorded
  `StepError`;
- `OrchestratorError` is reserved for infrastructure and orchestration-contract
  failures, not request-pipeline step failures already captured as
  `RunOutcome::Failed`.

## 5) Constructor

The generated module must define:

```rust
impl<P> Orchestrator<P>
where
    P: TransitionPolicy,
{
    pub fn new(
        policy: P,
        executor: StepExecutor,
        run_repository: RunRepository,
    ) -> Self;
}
```

`new` must only wrap the supplied dependencies.

## 6) Public Entry Points

The generated module must define:

```rust
impl<P> Orchestrator<P>
where
    P: TransitionPolicy,
{
    pub async fn run(
        &self,
        user_input: UserRequest,
    ) -> Result<RunOutcome, OrchestratorError>;

    pub async fn resume(
        &self,
        run_id: RunId,
    ) -> Result<RunOutcome, OrchestratorError>;

    pub async fn resume_with_input(
        &self,
        run_id: RunId,
        user_input: UserRequest,
    ) -> Result<RunOutcome, OrchestratorError>;
}
```

The generated method names and signatures must match exactly.

Required semantics:

- `run(user_input)` creates a new `RunState`, persists the empty run header,
  begins the first iteration from `user_input`, persists the appended
  iteration, then enters the canonical orchestration loop;
- `resume(run_id)` loads an existing run, does not create a new iteration, and
  re-enters the canonical orchestration loop using the current last iteration;
- `resume_with_input(run_id, user_input)` loads an existing run, appends a new
  iteration from `user_input`, persists that appended iteration, then enters
  the canonical orchestration loop.

The generated implementation must not infer that a successful
`RunOutcome::Finished` closes the persisted run. The run remains resumable
until an explicit later lifecycle action such as archiving.

The generated module must also define these private helpers with the exact
names and signatures shown below:

```rust
impl<P> Orchestrator<P>
where
    P: TransitionPolicy,
{
    async fn load_existing_run(
        &self,
        run_id: RunId,
    ) -> Result<RunState, OrchestratorError>;

    async fn drive_to_outcome(
        &self,
        state: &mut RunState,
    ) -> Result<RunOutcome, OrchestratorError>;
}
```

These helper names are part of the canonical generated shape for this module
and must not be renamed.

## 7) Hidden Invariants That Must Be Explicitly Enforced

The generated orchestrator implementation must treat the following constraints
as mandatory:

- every new iteration starts with one successful `StepKind::UserInputReceived`
  finished record created by `RunStateWriter::begin_iteration(user_input)`;
- the orchestration loop must assemble one execution-time `Context` for the
  current invocation before context-aware step execution begins;
- the orchestrator must derive `Context.golden_question` only from the current
  iteration's recorded `UserRequest`;
- the orchestrator must never attempt to execute
  `StepKind::UserInputReceived` through `StepExecutor`;
- `resume(run_id)` must not mutate or replace the stored user input for the
  current iteration;
- `resume_with_input(run_id, user_input)` must always create a new iteration
  rather than modifying the previous iteration;
- the orchestration loop must read and drive only the last iteration of the run
  via `RunStateView`, because `TransitionPolicy` and `StepExecutor` are both
  specified to operate on the current iteration;
- when the orchestrator needs an owned copy of the just-opened pending step or
  the just-finished step for persistence handoff, it must obtain that copy
  through documented `run_state::view` or `run_state::apply` helper methods
  rather than through direct field access on `RunState`, `RunIteration`, or
  `StepRecord`.

## 8) Execution-Time Context Assembly

The generated orchestrator implementation must assemble one execution-time
`Context` for each invocation that operates on a current iteration.

The current MVP context contract is:

```rust
pub struct OpenInferenceContext {
    pub root_span: tracing::Span,
}

pub struct Context {
    pub open_inference: OpenInferenceContext,
    pub golden_question: Option<GoldenQuestion>,
}
```

Context-assembly rules:

- the orchestrator owns creation of `OpenInferenceContext` for the current
  invocation and current iteration;
- the orchestrator must create `OpenInferenceContext.root_span` as the
  iteration-scoped OpenInference chain span `oi.chain.diagnostic_iteration`
  defined in `Specification/runtime/observability/open_inference_spans.md`;
- that OpenInference root span must be a child of the active
  `diagnostics.iteration` span for the same invocation;
- the orchestrator must read the current iteration's successful
  `StepKind::UserInputReceived` record through `RunStateView` /
  `IterationView`;
- when that recorded `UserRequest` contains `golden_question = Some(...)`, the
  assembled `Context.golden_question` must preserve the same value unchanged;
- when that recorded `UserRequest` contains `golden_question = None`, the
  assembled `Context.golden_question` must be `None`;
- the orchestrator must not synthesize fallback golden-question values and must
  not mutate the recorded user input after `begin_iteration(user_input)`;
- the assembled `Context` must then be passed unchanged into context-aware step
  execution.

Outcome rules:

- on successful terminal iteration completion, the orchestrator must record
  `status = "ok"` and `run.outcome = "success"` on
  `OpenInferenceContext.root_span`;
- when the final result is serializable, the orchestrator must also record the
  serialized result as `output.value` with `output.mime_type = "application/json"`;
- on terminal failure, the orchestrator must record `run.outcome = "failure"`
  and an OpenInference error status on `OpenInferenceContext.root_span`.

This context-assembly boundary is orchestration-owned.
Golden-file validation, golden-file parsing, and initial `UserRequest`
construction are runtime-entry concerns and are not owned by
`orchestrator::orchestrator`.

## 9) Canonical Orchestration Loop

The generated implementation must follow this control-flow shape.

The exact local helper names may differ, but the orchestration behavior must
remain equivalent.

```rust
impl<P> Orchestrator<P>
where
    P: TransitionPolicy,
{
    async fn drive_to_outcome(
        &self,
        state: &mut RunState,
    ) -> Result<RunOutcome, OrchestratorError> {
        let context = self.build_context(RunStateView::new(state))?;

        loop {
            let decision = self
                .policy
                .next_transition(RunStateView::new(state))?;

            match decision {
                PolicyTransition::ExecuteStep { step } => {
                    let (record_id, iteration_id) = {
                        let mut writer = RunStateWriter::new(state);
                        let mut iteration = writer.current_iteration()?;
                        let pending = iteration.begin_step(step)?;
                        (pending.record_id(), iteration.iteration_id())
                    };
                    let iteration_view = RunStateView::new(state)
                        .last_iteration()
                        .ok_or(StateApplyError::NoCurrentIteration)?;
                    let step_sequence_no =
                        iteration_view.step_count() as u64 - 1;
                    let step_record = match iteration_view.pending_step() {
                        Some(pending) => StepRecord::Pending(pending.to_owned()),
                        None => {
                            return Err(OrchestratorError::MissingPendingStep { step });
                        }
                    };

                    self.run_repository
                        .append_step_record(
                            state,
                            iteration_id,
                            step_sequence_no,
                            &step_record,
                        )
                        .await?;

                    let execution_result = self
                        .executor
                        .execute_with_context(step, RunStateView::new(state), &context)
                        .await;

                    {
                        let mut writer = RunStateWriter::new(state);
                        let mut iteration = writer.current_iteration()?;
                        let pending = iteration
                            .pending_step()
                            .ok_or(OrchestratorError::MissingPendingStep { step })?;

                        match execution_result {
                            Ok(result) => pending.record_success(result)?,
                            Err(error) => pending.record_failure(error)?,
                        }
                    }

                    let finished_record: FinishedStepRecord = match RunStateView::new(state)
                        .last_iteration()
                        .and_then(|iteration| iteration.finished_step(step))
                        .map(|record| record.to_owned())
                    {
                        Some(record) => record,
                        _ => {
                            return Err(OrchestratorError::MissingPendingStep { step });
                        }
                    };

                    self.run_repository
                        .finish_step_record(state, record_id, &finished_record)
                        .await?;
                }
                PolicyTransition::FinishWithResult { result } => {
                    return Ok(RunOutcome::Finished {
                        run_id: state.run_id,
                        result,
                    });
                }
                PolicyTransition::FinishWithError { error } => {
                    return Ok(RunOutcome::Failed {
                        run_id: state.run_id,
                        error,
                    });
                }
            }
        }
    }
}
```

Normative rules for this loop:

- the loop must ask policy for the next transition on every iteration;
- `ExecuteStep` must first create and persist a pending step record before step
  execution begins;
- the generated implementation must avoid holding mutable writer borrows across
  later direct reads from `state`; the compileable implementation should use
  short writer scopes like the canonical example above;
- the just-opened pending step must be persisted as the current iteration's
  last `StepRecord::Pending(...)` record;
- its `step_sequence_no` must equal
  `RunStateView::last_iteration().unwrap().step_count() - 1` immediately after
  `begin_step(step)` succeeds;
- the pending record passed to `RunRepository::append_step_record(...)` must
  carry the exact `record_id` returned by `PendingStepWriter::record_id()`;
- step execution must be invoked only through
  `StepExecutor::execute_with_context(...)` or through
  `StepExecutor::execute(...)` when orchestration intentionally relies on the
  documented `Context::noop()` delegation path;
- after execution returns, the pending step must be finished through
  `record_success(...)` or `record_failure(...)`;
- the finished step transition must then be persisted through
  `RunRepository::finish_step_record(...)`;
- the finished record passed to `RunRepository::finish_step_record(...)` must
  be the finished replacement of the same logical step record opened earlier in
  the loop, identified by the same `record_id`;
- `PolicyTransition::FinishWithResult` must return `RunOutcome::Finished`
  without inferring that the persisted run has reached a terminal archived or
  closed state;
- the loop must terminate only when policy returns `FinishWithResult` or
  `FinishWithError`.

## 10) Canonical Entry-Point Algorithms

The generated implementation must follow these entry-point shapes.

The exact formatting may differ, but the call sequence and method usage must
remain equivalent.

### 9.1 `run(user_input)`

```rust
pub async fn run(
    &self,
    user_input: UserRequest,
) -> Result<RunOutcome, OrchestratorError> {
    let mut state = RunState::new();
    self.run_repository.create_run(&state).await?;

    {
        let mut writer = RunStateWriter::new(&mut state);
        writer.begin_iteration(user_input)?;
    }

    let iteration_sequence_no =
        RunStateView::new(&state).iteration_count() as u64 - 1;
    let iteration_id = RunStateView::new(&state)
        .last_iteration()
        .expect("begin_iteration must create a current iteration")
        .iteration_id();
    let iteration = RunStateView::new(&state)
        .iteration(iteration_id)
        .expect("last_iteration view must reference a stored iteration");

    self.run_repository
        .append_iteration(&state, iteration_sequence_no, iteration)
        .await?;

    self.drive_to_outcome(&mut state).await
}
```

### 9.2 `resume(run_id)`

```rust
pub async fn resume(
    &self,
    run_id: RunId,
) -> Result<RunOutcome, OrchestratorError> {
    let mut state = self.load_existing_run(run_id).await?;
    self.drive_to_outcome(&mut state).await
}
```

### 9.3 `resume_with_input(run_id, user_input)`

```rust
pub async fn resume_with_input(
    &self,
    run_id: RunId,
    user_input: UserRequest,
) -> Result<RunOutcome, OrchestratorError> {
    let mut state = self.load_existing_run(run_id).await?;

    {
        let mut writer = RunStateWriter::new(&mut state);
        writer.begin_iteration(user_input)?;
    }

    let iteration_sequence_no =
        RunStateView::new(&state).iteration_count() as u64 - 1;
    let iteration_id = RunStateView::new(&state)
        .last_iteration()
        .expect("begin_iteration must create a current iteration")
        .iteration_id();
    let iteration = RunStateView::new(&state)
        .iteration(iteration_id)
        .expect("last_iteration view must reference a stored iteration");

    self.run_repository
        .append_iteration(&state, iteration_sequence_no, iteration)
        .await?;

    self.drive_to_outcome(&mut state).await
}
```

### 9.4 `load_existing_run(run_id)`

```rust
async fn load_existing_run(
    &self,
    run_id: RunId,
) -> Result<RunState, OrchestratorError> {
    let state = self
        .run_repository
        .load_run(run_id)
        .await?
        .ok_or(OrchestratorError::RunNotFound { run_id })?;

    Ok(state)
}
```

`load_existing_run(run_id)` must:

- call `RunRepository::load_run(run_id)`;
- map repository absence `Ok(None)` into
  `OrchestratorError::RunNotFound { run_id }`;
- return the loaded `RunState` unchanged when it exists;
- perform no in-memory mutation and no persistence.

## 11) Persistence Order

The generated implementation must persist run progress in the same mutation
granularity already defined by `run_repository`.

### 10.1 `run(user_input)`

Required persistence order:

1. create an empty run header via `RunRepository::create_run(&state)`;
2. mutate in memory via `RunStateWriter::begin_iteration(user_input)`;
3. persist the appended iteration via `RunRepository::append_iteration(...)`;
4. enter `drive_to_outcome(...)`.

### 10.2 `resume(run_id)`

Required persistence order:

1. load the run via `RunRepository::load_run(run_id)`;
2. enter `drive_to_outcome(...)`.

`resume` must not create or append a new iteration.

### 10.3 `resume_with_input(run_id, user_input)`

Required persistence order:

1. load the run via `RunRepository::load_run(run_id)`;
2. mutate in memory via `RunStateWriter::begin_iteration(user_input)`;
3. persist the appended iteration via `RunRepository::append_iteration(...)`;
4. enter `drive_to_outcome(...)`.

## 12) Resume Semantics

`resume(run_id)` is a retry/continue entrypoint for the current iteration.

The generated implementation must treat it as:

- re-entering the orchestration loop with the same persisted current-iteration
  user input;
- preserving already successful finished steps in the current iteration;
- allowing policy to select the next executable step from the persisted current
  state;
- not clearing iteration history, not replacing `UserInputReceived`, and not
  forcing a full rerun from the first step unless policy naturally selects that
  outcome from the stored state.

The generated implementation must treat a run that previously returned
`RunOutcome::Finished` as still resumable unless it has later been explicitly
archived.

`resume_with_input(run_id, user_input)` is the entrypoint for the future
multi-turn interaction shape.

The generated implementation must treat it as:

- starting a brand-new iteration inside the same run;
- preserving prior iterations unchanged for history and evaluation;
- using the new iteration as the only execution context inspected by policy and
  executor during the new invocation.

## 13) Non-Responsibilities

`orchestrator` must not:

- inspect older iterations when the current last iteration exists;
- directly decode step payload variants from `FinishedStepRecord` outside the
  `TransitionPolicy` and `StepExecutor` contracts;
- read `RunState.iterations` or `RunIteration.step_records` directly inside the
  canonical orchestration loop when equivalent documented
  `run_state::view`/`run_state::apply` helper methods exist;
- persist the entire run through a hypothetical generic `save_run(...)`
  shortcut;
- own golden-file validation, schema checking, or golden JSON parsing;
- bypass `RunStateWriter` and mutate `RunState.iterations` or
  `RunIteration.step_records` inline;
- collapse step failure into `OrchestratorError` when policy can surface the
  recorded `StepError` as `RunOutcome::Failed`.

## 14) Testing Seams

The generated production implementation must keep the exact public
`Orchestrator<P>` API defined by this document.

At the same time, the generator is explicitly allowed to introduce private or
test-only seams that make orchestrator unit testing practical without changing
the production public API.

Allowed test-only support includes:

- private helper traits implemented by `StepExecutor` and `RunRepository`;
- `#[cfg(test)]` adapters, spies, fakes, or harness constructors that wrap the
  concrete executor and repository dependencies;
- test-only helper constructors that preserve the documented production
  constructor and entrypoint signatures.

Disallowed changes:

- changing the public fields or public method signatures of `Orchestrator<P>`;
- replacing the documented production constructor with a different public
  test-only constructor;
- introducing a second public orchestrator type solely for testing.

## 15) Unit-Test Ownership

Required unit-test coverage for the orchestrator runtime subtree is owned by:

- `Specification/runtime/unit_tests.md`
- `Specification/runtime/unit_tests_common.md`
