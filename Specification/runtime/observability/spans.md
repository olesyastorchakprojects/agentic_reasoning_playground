# 1) Purpose / Scope

This document defines the span contract for the runtime orchestrator.

It defines:
- root span contract;
- orchestration span hierarchy;
- span ownership;
- span scope boundaries;
- required span attributes;
- root span implementation requirements.

# 2) Root Span Contract

Each run trace contains exactly one root span.

The root span name is:
- `diagnostics.run`

`diagnostics.run` rules:
- it is created at the beginning of orchestrator entrypoint handling;
- it is the ancestor of every other span in the run trace;
- it closes only after final success or terminal failure is known for that run invocation;
- it remains present in partial and failed traces;
- it must not be replaced by per-step spans, repository spans, or pipeline spans.

Entrypoint rules:
- one invocation of `Orchestrator::run(...)` creates one `diagnostics.run` span;
- one invocation of `Orchestrator::resume(...)` creates one `diagnostics.run` span;
- one invocation of `Orchestrator::resume_with_input(...)` creates one `diagnostics.run` span;
- the root span required attribute `run.entrypoint` records which of those three entrypoints created the span.

# 3) Attribute Naming Rules

Attribute names in this document are local to the span that owns them.

Rules:
- attribute names in this document do not repeat the span name prefix;
- shared attributes keep their full names:
  - `span.module`
  - `span.stage`
  - `status`
  - `error.type`
  - `error.message`
- all other attributes are interpreted in the scope of the span section that defines them.

Enum string rules:
- `step.kind` uses the canonical Rust enum variant name from `StepKind`;
- `transition.kind` uses the canonical Rust enum variant name from `PolicyTransition`;
- the required format is Rust variant casing, not snake_case and not kebab-case.

# 4) Mandatory Span Hierarchy

Each traced orchestrator invocation contains the following mandatory hierarchy:

- `diagnostics.run`
- `diagnostics.iteration`
- `orchestrator.policy.next_transition`
- `orchestrator.step`
- `repository.step.append_pending`
- `step_executor.dispatch`
- `repository.step.finish`

Hierarchy rules:
- every listed span is a descendant of `diagnostics.run`;
- `diagnostics.iteration` is created only when the current invocation has a current iteration;
- in `resume(...)`, `diagnostics.iteration` is created for the loaded current iteration when one exists, even if that iteration was not created by the current invocation;
- `diagnostics.iteration` is not created when the current invocation has no current iteration to work with;
- `orchestrator.policy.next_transition` is a child of the active iteration span when an iteration exists;
- `orchestrator.step` is created only for `PolicyTransition::ExecuteStep`;
- `repository.step.append_pending`, `step_executor.dispatch`, and `repository.step.finish` are children of the owning `orchestrator.step` span;
- request-pipeline spans are nested under `step_executor.dispatch`;
- dependency spans are nested inside the stage or module that owns the dependency call;
- sibling spans are not used to represent parent-child execution;
- duplicate spans for the same action are forbidden.

# 5) Span Ownership

Span ownership is fixed by module:

- `diagnostics.run` and `diagnostics.iteration` belong to `orchestrator`;
- `orchestrator.policy.next_transition` belongs to `transition_policy`;
- `orchestrator.step` belongs to `orchestrator`;
- `repository.step.append_pending` and `repository.step.finish` belong to `run_repository`;
- `step_executor.dispatch` belongs to `step_executor`;
- `request_pipeline.*` spans belong to the owning request-pipeline module;
- dependency spans such as `llm.call` and `qdrant.search` belong to the module that performs that dependency call.

Ownership rules:
- a module creates only its own spans;
- a module does not create spans for another module;
- business modules do not create or close the root run span;
- repository code does not create orchestrator spans;
- cross-module telemetry side effects are forbidden.

# 6) Required Attributes For Mandatory Spans

Every mandatory span listed in sections `2)` and `4)` contains:
- `span.module`;
- `span.stage`;
- `status`.

If a mandatory span ends with an error, it also contains:
- `error.type`;
- `error.message`.

Allowed `status` values are fixed:
- `ok`;
- `error`.

# 7) Detailed Span Contracts

`diagnostics.run`

- begins:
  - before orchestrator request handling begins for one invocation of `run`, `resume`, or `resume_with_input`
- ends:
  - after the invocation returns final success or terminal failure
- includes:
  - full orchestration work performed during that invocation
- required attributes:
  - `run.id`
    - type: string
    - source: orchestrator state
    - value: run UUID string
  - `run.entrypoint`
    - type: string
    - source: orchestration-derived
    - value: `run | resume | resume_with_input`
  - `span.module`
    - type: string
    - source: constant
    - value: `orchestrator`
  - `span.stage`
    - type: string
    - source: constant
    - value: `run`
  - `status`
    - type: string
    - source: result-derived
    - value: `ok | error`

`diagnostics.run` rules:
- `run.entrypoint` is required only on `diagnostics.run`;
- child spans do not duplicate `run.entrypoint`;
- `diagnostics.run` must still be emitted when the invocation ends in `PolicyTransition::FinishWithError`;
- `diagnostics.run` must still be emitted when step execution or persistence returns an error;
- the root span must remain the active parent while the orchestrator awaits downstream async work.

`diagnostics.iteration`

- begins:
  - before work for the current iteration begins inside the current invocation
- ends:
  - after iteration-scoped orchestration work in that invocation completes
- includes:
  - work against the current iteration for that invocation, including a current iteration loaded by `resume(...)`
- required attributes:
  - `run.id`
    - type: string
    - source: orchestrator state
  - `iteration.id`
    - type: string
    - source: orchestrator state
  - `iteration.sequence_no`
    - type: integer
    - source: orchestrator state
  - `span.module`
    - type: string
    - source: constant
    - value: `orchestrator`
  - `span.stage`
    - type: string
    - source: constant
    - value: `iteration`
  - `status`
    - type: string
    - source: result-derived
    - value: `ok | error`

`orchestrator.policy.next_transition`

- begins:
  - immediately before one call to `TransitionPolicy::next_transition(...)`
- ends:
  - immediately after that call returns
- required attributes:
  - `run.id`
  - `iteration.id`
  - `span.module`
    - value: `transition_policy`
  - `span.stage`
    - value: `next_transition`
  - `status`
- conditional attributes:
  - `transition.kind`
    - type: string
    - source: policy-derived
    - emitted when the policy call succeeds
    - value format: canonical `PolicyTransition` variant name such as `ExecuteStep`, `FinishWithResult`, or `FinishWithError`
  - `step.kind`
    - type: string
    - source: policy-derived
    - emitted only when `transition.kind = "ExecuteStep"`
    - value format: canonical `StepKind` variant name such as `InputNormalization`

`orchestrator.step`

- begins:
  - immediately before orchestrator bookkeeping for one `PolicyTransition::ExecuteStep`
- ends:
  - after pending-step persistence, step execution, state mutation, and finished-step persistence complete
- required attributes:
  - `run.id`
  - `iteration.id`
  - `step.kind`
    - type: string
    - source: orchestrator-derived
    - value format: canonical `StepKind` variant name such as `InputNormalization`
  - `span.module`
    - value: `orchestrator`
  - `span.stage`
    - value: `step`
  - `status`

`repository.step.append_pending`

- begins:
  - immediately before persisting the pending step record
- ends:
  - immediately after the persistence call returns
- required attributes:
  - `run.id`
  - `iteration.id`
  - `step.kind`
  - `record.id`
  - `span.module`
    - value: `run_repository`
  - `span.stage`
    - value: `append_pending`
  - `status`

`step_executor.dispatch`

- begins:
  - immediately before dispatching one step to `StepExecutor`
- ends:
  - immediately after `StepExecutor` returns
- required attributes:
  - `run.id`
  - `iteration.id`
  - `step.kind`
  - `span.module`
    - value: `step_executor`
  - `span.stage`
    - value: `dispatch`
  - `status`

`repository.step.finish`

- begins:
  - immediately before persisting the finished step record
- ends:
  - immediately after the persistence call returns
- required attributes:
  - `run.id`
  - `iteration.id`
  - `step.kind`
  - `record.id`
  - `span.module`
    - value: `run_repository`
  - `span.stage`
    - value: `finish`
  - `status`

# 8) Root Span Implementation Requirements

The root span lifecycle is fixed.

Required implementation pattern:
- create `diagnostics.run` exactly once inside each orchestrator entrypoint method;
- create it before any downstream async orchestration work starts;
- enter that span before awaiting the downstream future that performs orchestration work;
- keep that entered root span active for the full lifetime of the awaited orchestration future;
- record final success or error status on the root span before it closes;
- drop the entered root span only after the awaited orchestration future has resolved.

The implementation must not:
- create the root span inside `drive_to_outcome(...)`;
- create a fresh root span per iteration;
- create a fresh root span per step;
- enter the root span and then start downstream work outside that entered scope;
- rely on child functions to recreate parentage when the entrypoint failed to keep the root span active.

Async safety rules:
- the root span must be entered in the same orchestrator entrypoint scope that awaits downstream orchestration;
- the entered root span must remain active across `await`;
- child spans must be created while the root span is active, not retroactively attached afterward;
- ad hoc recreation of a root-like span in lower layers is forbidden.

Tracing primitive rules:
- spans are created with `tracing::span!`;
- the runtime uses `tracing_opentelemetry` only as the bridge from `tracing` spans to OpenTelemetry export;
- business code and the internal observability API must not create orchestration spans through OpenTelemetry SDK span constructors directly.

Parentage propagation rules:
- parent-child span relationships are established through the active `tracing` span scope;
- `run_repository`, `step_executor`, and request-pipeline modules create their own spans locally with `tracing::span!`;
- those child spans become descendants of the active parent span because the caller keeps the parent span entered while awaiting downstream work;
- the internal observability API must not require explicit OpenTelemetry span-context passing between orchestrator, repository, executor, and pipeline layers.

Rationale:
- root span parentage is fragile in async Rust when the root span is created in one place and the awaited work runs outside the entered scope;
- the contract in this section exists to prevent orphan child spans, split traces, and accidental per-step root traces.

# 9) Safety Constraints

The following values must not be written to spans:
- secrets;
- API keys;
- authorization headers;
- environment variable values;
- raw prompt text;
- raw retrieved document text;
- raw model output text.
