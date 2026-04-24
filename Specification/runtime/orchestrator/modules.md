## 1) Purpose

This document enumerates the planned orchestration-layer modules and their
responsibilities.

The current version anchors the module boundaries for the durable run
state-machine kernel. Detailed behavior, persistence semantics, resume policy,
and error handling are owned by follow-up module specifications.

## 2) Planned Modules

| Module | Responsibility |
| --- | --- |
| `orchestrator` | Own the public run-driving boundary and run lifecycle: create a new run, resume an existing run, resume with new user input, repeatedly ask policy for the next transition, dispatch step execution, apply state mutations through the run-state writer boundary, persist progress, and finish by surfacing either the final validated response or the recorded step error as a terminal `RunOutcome`. |
| `transition_policy` | Decide the next orchestration transition from the current `RunState`, including which step to execute next and when the run should finish with the final validated result or finish with the recorded step error. |
| `step_executor` | Bridge orchestration to request-processing leaf modules by executing a requested `StepKind` against the current `RunState` and returning a typed `StepResultEnvelope`. |
| `run_state` | Define the persisted canonical state subdomain for a single orchestration run, including `model`, `view`, and `apply` submodules. |
| `run_repository` | Provide the orchestration-facing persistence boundary for loading and persisting `RunState` hierarchy while delegating PostgreSQL storage details to a lower-level run-state store. |
| `errors` | Define orchestration-layer error types for infrastructure failures, invalid state application, execution dispatch errors, and repository failures. |

## 3) Notes

- The orchestration layer is a top-level runtime subtree, separate from
  `request_pipeline`.
- `request_pipeline` remains the parent boundary for leaf request-processing
  logic; orchestration modules must not embed retrieval, hydration,
  prompt-building, model-call, or response-validation internals.
- `orchestrator` itself is a concrete orchestrator-lifecycle module with its
  own dedicated spec in `Specification/runtime/orchestrator/orchestrator.md`;
  this module owns the public entrypoints and the canonical orchestration loop,
  not just the parent subtree name.
- `run_state` owns a small internal module set:
  - `model` defines the persisted `RunState` data model, including
    iteration and step execution facts, `StepKind`, `StepResultEnvelope`,
    and `StepError`;
  - `view` defines typed read projections used by policy, executor, and evals;
  - `apply` defines the only allowed mutation/update boundary.
- `transition_policy` and `step_executor` are orchestration-layer modules;
  neither one owns persistence or mutation of `RunState`.
- PostgreSQL persistence details for canonical run-state hierarchy should live
  in `api_clients/postgres/run_state_store`, while `run_repository` remains the
  orchestration-facing persistence boundary.
- Crash/resume behavior should be specified through `run_state` and
  `run_repository`, using the same persisted run history that evaluation
  workflows consume.
- The first implementation should keep the module set explicit and closed:
  no generic DAG engine, plugin-loaded step registry, or event-bus-first
  orchestration runtime is required for the MVP.
- Future diagnostic-loop behavior should extend policy and persisted run
  history without replacing the orchestration kernel.
