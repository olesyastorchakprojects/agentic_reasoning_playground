## 1) Purpose

This document defines the current architecture of the offline eval engine for
the distributed diagnostics project.

The eval engine exists to evaluate completed runtime executions against golden
dataset expectations and judge-model rubrics, then produce:

- persisted judge verdict rows;
- persisted iteration-level and eval-run-level summary rows;
- a human-readable eval run report;
- dashboard-friendly aggregates;
- factual token and cost accounting for judge work.

The eval engine treats the runtime system and the eval system as distinct
layers:

- runtime execution produces `RunState` records;
- eval execution reads completed runtime runs and produces eval-owned artifacts
  about them.

## 2) Core Concepts

### 2.1) Runtime Run

A runtime run is one persisted diagnostic execution owned by the runtime
orchestrator and identified by `RunState.run_id`.

One runtime run stores:

- one or more iterations;
- step history for each iteration;
- runtime-produced typed outputs;
- runtime token usage produced by model-facing steps.

### 2.2) Iteration

An iteration is one item in `RunState.iterations`.

The eval engine treats the iteration, not the whole run, as the fundamental
evaluation subject.

Iterations are classified as:

- `initial`
- `continuation`

### 2.3) Eval Run

An eval run is one offline batch evaluation over a frozen set of runtime runs
and frozen iteration subjects.

One eval run:

- has its own `eval_run_id`;
- owns one frozen set of runtime-run ids;
- owns one frozen set of `(runtime_run_id, iteration_id)` subjects;
- may be resumed after interruption;
- aggregates many subjects into one report and one set of summary rows.

### 2.4) Eval Subject

The primary semantic subject of judge evaluation is:

- one runtime run;
- one concrete iteration inside that run;
- one suite-specific view over that iteration.

This subject is materialized as `DiagnosticEvalIterationSnapshot`.

## 3) Current MVP Operating Mode

The current operating mode is offline batch evaluation over runtime runs that
contain completed final validated outputs.

Current workflow:

1. runtime runs are persisted through the runtime system;
2. the eval engine discovers eligible runtime runs;
3. for those selected runtime runs, the eval engine freezes every eligible
   completed iteration subject currently returned by storage discovery;
4. the eval engine evaluates every frozen subject;
5. the eval engine writes judge rows, summary rows, and a run report for the
   eval run as a whole.

Important note:

- the current implementation no longer matches the earlier “exactly one target
  iteration per runtime run” description;
- one runtime run may contribute more than one frozen eval subject when more
  than one iteration is eligible.

## 4) Source Of Truth

The runtime source of truth for eval inputs is `RunState`.

The eval engine must:

- load persisted `RunState`;
- select or reuse a frozen iteration subject;
- project that iteration into an eval snapshot;
- build suite-specific judge payloads from that snapshot.

Eval-owned persisted artifacts include:

- judge result rows;
- judge usage rows;
- iteration summary rows;
- eval-run summary rows;
- manifest and report artifacts.

## 5) Engine Shape

The current engine uses one generic suite-driven judge stage and one summary
stage:

1. `judge_request_suites`
2. `build_eval_summary`

The CLI currently performs the final run-summary rebuild and report write after
these stage drains complete.

## 6) Frozen Scope Rule

Each eval run owns a frozen scope consisting of:

- runtime-run membership;
- evaluated subject membership.

That frozen scope must remain stable for the life of the eval run so that:

- reports remain reproducible;
- resume does not redefine membership;
- comparisons between eval runs remain interpretable.

## 7) Continuation Semantics

Continuation evaluation remains a first-class part of the engine.

For continuation subjects, snapshot and summary logic must preserve:

- the current iteration outputs;
- the immediately prior completed iteration context;
- continuation-specific judge suites;
- continuation-specific summary signals.

## 8) Required Outputs

For each completed or partially completed eval run, the engine may produce:

- normalized suite verdict rows in eval storage;
- factual judge usage rows with token and cost accounting;
- iteration-level summaries;
- eval-run-level summaries;
- `run_manifest.json`;
- `run_report.md`.

These outputs remain iteration-granular even when one runtime run contributes
multiple evaluated iterations.
