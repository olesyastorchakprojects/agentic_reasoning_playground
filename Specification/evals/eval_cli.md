## 1) Purpose

This document defines the CLI contract for the new diagnostics eval engine
crate.

The CLI is the top-level operational entrypoint for launching or resuming
offline eval runs.

## 2) CLI Role

The CLI must be intentionally narrow.

Its responsibilities are:

- accept launch-time parameters;
- load and validate eval config;
- resolve optional overrides;
- invoke the eval orchestrator;
- exit with a clear success or failure code.

The CLI must not contain embedded eval business logic beyond argument
validation and startup wiring.

## 3) Required MVP Arguments

The MVP CLI should accept at least:

- `--config`
- `--resume-eval-run-id`
- `--run-type`

It may also accept:

- `--limit`
- `--dry-run`
- `--artifact-root`
- `--enabled-suite`

## 4) Required Argument Semantics

### 4.1) `--config`

- required for normal execution
- path to the eval-engine TOML config file

The CLI must fail fast if the path does not exist or cannot be parsed.

### 4.2) `--resume-eval-run-id`

- optional
- when present, the engine must resume the specified eval run rather than
  bootstrap a new one

The CLI must not allow conflicting launch semantics such as resuming one
eval run while also pretending to bootstrap a different frozen scope.

### 4.3) `--run-type`

- optional override
- when supplied for a new run, it overrides the config file's `[eval].run_type`
- when supplied during resume, it must either:
  - match the manifest's existing run type;
  - or be rejected as an invalid conflicting override

## 5) Optional MVP-Friendly Arguments

### 5.1) `--limit`

- optional
- integer
- limits how many eligible runtime runs are absorbed into a newly created eval
  run

This is useful for incremental local development and report debugging.

It must not change the frozen scope of an already created eval run during
resume.

### 5.2) `--dry-run`

- optional boolean flag
- validates config and prints the would-be launch plan without writing eval
  storage or artifacts

This is strongly useful during development and should be supported if the
implementation cost is small.

### 5.3) `--artifact-root`

- optional path override
- overrides `[artifacts].root_dir`

This is useful for local experimentation without rewriting the checked-in
config.

### 5.4) `--enabled-suite`

- optional repeatable argument
- narrows the active suite set to one or more explicitly named suites

This is especially useful for the early implementation phase when only one
suite may be wired end to end.

The CLI should reject unknown suite names after loading the catalog.

## 6) Precedence Rules

The precedence order must be:

1. manifest-frozen values for resumed eval runs
2. explicit CLI overrides
3. config-file values
4. documented defaults

This matters because:

- resume must preserve frozen identity and scope;
- CLI must be convenient for local iteration;
- config must remain the stable base behavior.

## 7) Resume Rules

When `--resume-eval-run-id` is provided:

- the CLI must load the existing manifest through orchestrator startup;
- frozen scope must come from the manifest, not from new discovery;
- launch-time overrides that would mutate frozen scope must be rejected;
- launch-time overrides that are operationally harmless may be allowed only if
  explicitly documented.

For MVP, the safest rule is:

- allow `--config` and observability-related operational settings;
- reject conflicting semantic overrides such as changed suite set or changed
  run type unless later explicitly supported.

## 8) Exit Semantics

The CLI must exit with:

- `0` on successful completion of a new or resumed eval run;
- non-zero on config errors, startup errors, orchestrator failures, or resume
  conflicts.

It should print concise human-readable error messages to stderr.

## 9) Recommended MVP Shape

The conceptual CLI shape should be equivalent to:

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

## 10) Minimal MVP Clap-Style Contract

The CLI design should map cleanly onto a Rust `clap` parser with:

- one top-level command
- strongly typed paths
- typed optional `Uuid` for `resume_eval_run_id`
- repeatable string arguments for enabled suites

The implementation should mirror the style already used in
`Execution/distributed_diagnostics/src/main.rs`.

