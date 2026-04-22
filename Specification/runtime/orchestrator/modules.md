## 1) Purpose

This document enumerates the planned orchestration-layer modules and their
responsibilities.

The current version anchors the module boundaries for the durable run
state-machine kernel. Detailed behavior, persistence semantics, resume policy,
and error handling are owned by follow-up module specifications.

## 2) Planned Modules

| Module | Responsibility |
| --- | --- |
| `orchestrator` | Own the run lifecycle: create or resume a run, repeatedly ask policy for the next transition, dispatch step execution, apply step results, persist progress, and return when no immediate transition should run. |
| `transition_policy` | Decide the next orchestration transition from the current `RunState`, including which step to execute next and when the run should wait for user input, surface an execution error, or pause with the current active investigation state. |
| `step_executor` | Bridge orchestration to request-processing leaf modules by executing a requested `StepKind` against the current `RunState` and returning a typed `StepResultEnvelope`. |
| `run_state` | Define the persisted canonical state subdomain for a single orchestration run, including `model`, `view`, and `apply` submodules. |
| `run_repository` | Provide the persistence boundary for inserting, saving, loading, and inspecting orchestration runs and step-level history. |
| `errors` | Define orchestration-layer error types for infrastructure failures, invalid state application, execution dispatch errors, and repository failures. |

## 3) Notes

- The orchestration layer is a top-level runtime subtree, separate from
  `request_pipeline`.
- `request_pipeline` remains the parent boundary for leaf request-processing
  logic; orchestration modules must not embed retrieval, hydration,
  prompt-building, model-call, or response-validation internals.
- `run_state` owns a small internal module set:
  - `model` defines the persisted `RunState` data model, including
    iteration and step execution facts, `StepKind`, `StepResultEnvelope`,
    and `StepError`;
  - `view` defines typed read projections used by policy, executor, and evals;
  - `apply` defines the only allowed mutation/update boundary.
- Crash/resume behavior should be specified through `run_state` and
  `run_repository`, using the same persisted run history that evaluation
  workflows consume.
- The first implementation should keep the module set explicit and closed:
  no generic DAG engine, plugin-loaded step registry, or event-bus-first
  orchestration runtime is required for the MVP.
- Future diagnostic-loop behavior should extend policy and persisted run
  history without replacing the orchestration kernel.
