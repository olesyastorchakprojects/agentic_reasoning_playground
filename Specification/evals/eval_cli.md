## 1) Purpose

This document defines the current CLI contract for the diagnostics eval engine
crate.

The CLI is the top-level operational entrypoint for launching or resuming
offline eval runs.

## 2) CLI Role

The CLI is intentionally small, but it is not a zero-logic wrapper.

Its current responsibilities are:

- accept launch-time parameters;
- load and validate eval config;
- resolve optional CLI overrides;
- initialize observability;
- load the suite catalog and runtime dependencies;
- bootstrap a new eval run or resume an existing one;
- drain the required eval stages in order;
- materialize the final run-level summary row;
- write `run_report.md`;
- exit with a clear success or failure code.

The CLI must not own:

- judge prompt normalization logic;
- snapshot construction logic;
- SQL query details;
- markdown section formatting internals.

## 3) Required MVP Arguments

The current CLI accepts at least:

- `--config`
- `--resume-eval-run-id`
- `--run-type`

It also accepts:

- `--limit`
- `--dry-run`
- `--artifact-root`
- `--enabled-suite`

## 4) Required Argument Semantics

### 4.1) `--config`

- required for normal execution;
- path to the eval-engine TOML config file.

The CLI must fail fast if the path does not exist or cannot be parsed.

### 4.2) `--resume-eval-run-id`

- optional;
- when present, the CLI resumes the specified eval run rather than bootstrapping
  a new frozen scope.

Current implementation shape:

- the CLI locates the artifact directory for that `eval_run_id`;
- reads the existing `run_manifest.json`;
- drains `judge_request_suites`;
- drains `build_eval_summary`;
- rebuilds the run-level summary row from persisted iteration summaries;
- rewrites `run_report.md`.

### 4.3) `--run-type`

- optional override;
- when supplied for a new run, it overrides `[eval].run_type` from config.

Current implementation note:

- the CLI/config loader applies this override before orchestrator bootstrap;
- resume-specific validation against the stored manifest run type is not yet
  enforced in code and remains a future tightening step.

## 5) Optional Arguments

### 5.1) `--limit`

- optional integer;
- limits how many runtime runs are absorbed into a newly created eval run.

This override affects only new bootstrap discovery. It must not change the
frozen scope of an existing eval run during resume.

### 5.2) `--dry-run`

- optional boolean flag;
- validates config and prints the resolved launch plan without writing eval
  storage or artifacts.

The current dry-run output must include at least:

- config path;
- resolved `run_type`;
- resolved `mode`;
- resolved artifact root;
- resolved suite-catalog path;
- resolved enabled-suite set;
- whether tracing is enabled.

### 5.3) `--artifact-root`

- optional path override;
- overrides `[artifacts].root_dir`.

### 5.4) `--enabled-suite`

- optional repeatable argument;
- narrows the active suite set to one or more explicitly named suites.

The CLI passes these names through config loading and suite-catalog resolution.
Unknown suite names must be rejected during catalog resolution.

## 6) Precedence Rules

The current precedence order is:

1. manifest-frozen identity and subject scope for resumed eval runs;
2. explicit CLI overrides;
3. config-file values;
4. documented defaults.

Notes:

- the frozen scope of a resumed run comes from persisted artifacts and storage,
  not from fresh discovery;
- operational settings such as tracing configuration still come from the active
  config load.

## 7) Resume Rules

When `--resume-eval-run-id` is provided:

- the CLI must not call new-run bootstrap;
- frozen scope must come from the existing manifest and persisted
  `eval_processing_state` rows;
- stage draining must reuse the same `eval_run_id`;
- the CLI must not silently add newly eligible runtime runs to that eval run.

Current implementation detail:

- resume is coordinated at the CLI level by wiring the existing `eval_run_id`
  into orchestrator stage-drain methods rather than through a single
  `run_eval_orchestrator(...)` method.

## 8) Exit Semantics

The CLI must exit with:

- `0` on successful completion of a new or resumed eval run;
- non-zero on config errors, startup errors, orchestrator failures, or report
  finalization failures.

Errors should remain concise and human-readable.

## 9) Recommended Current Shape

```text
distributed_diagnostics_eval \
  --config Execution/distributed_diagnostics_eval/eval.toml \
  [--run-type golden_dataset] \
  [--resume-eval-run-id <uuid>] \
  [--limit 10] \
  [--artifact-root Evidence/evals/runs] \
  [--enabled-suite final_no_root_cause_claim] \
  [--dry-run]
```

## 10) Architectural Note

The current implementation intentionally keeps the CLI as the place where
process-wide wiring happens:

- observability initialization;
- dependency construction;
- bootstrap-or-resume branching;
- final report write after stage drains.

This is the current source of truth for runtime behavior even though a future
version may move more of this flow behind a single top-level orchestrator
entrypoint.
