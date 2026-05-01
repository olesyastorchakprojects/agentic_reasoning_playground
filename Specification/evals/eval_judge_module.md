## 1) Purpose

This document defines the module contract for the eval crate's `judge`
module.

This module owns suite execution for one eval subject.

## 2) Responsibilities

The judge module owns:

- loading active suite definitions from the suite catalog;
- determining which suites are still missing for a subject;
- building suite payloads from the snapshot;
- invoking the judge transport;
- extracting factual token and cost usage;
- normalizing suite outputs into structured verdicts;
- persisting `judge_llm_calls` and `judge_results` via storage interfaces.

## 3) Non-Responsibilities

The judge module must not own:

- eval-run lifecycle and frozen-scope logic;
- `RunState` loading;
- final iteration/run aggregate formulas;
- final markdown report formatting.

## 4) Public Types

The module should expose types conceptually equivalent to:

- `JudgeRunner`
- `JudgeRunResult`
- `JudgeModuleError`
- `NormalizedSuiteVerdict`
- `JudgeUsageRecord`

It may also expose transport traits such as:

- `JudgeTransport`

and parser/normalizer helpers.

## 5) Public Interfaces

The main entrypoint should be one subject-level function conceptually
equivalent to:

```rust
async fn run_missing_suites(
    subject: &DiagnosticEvalIterationSnapshot,
    context: &JudgeRunContext,
) -> Result<JudgeRunResult, JudgeModuleError>
```

Where `JudgeRunContext` bundles:

- enabled suite set
- suite catalog
- transport implementation
- pricing settings
- storage repositories

## 6) Required Dependencies

The judge module may depend on:

- `snapshot`
- `suites`
- `storage`
- `config`

It should not depend on:

- `orchestrator`
- `report`

## 7) Persistence Ownership

The judge module owns persistence of:

- factual `judge_llm_calls` rows whenever a model response exists;
- semantic `judge_results` rows only after normalization succeeds.

This ownership is important because usage persistence must survive semantic
normalization failures.

## 8) Normalization Boundary

The judge module is the owner of the transition from:

- raw judge model response

to:

- normalized suite verdict

The normalization boundary must remain explicit and testable.

The module must not silently assign fallback scores when normalization fails.

## 9) Retry And Resume Interaction

The judge module must behave idempotently with resume-aware storage rules.

It should:

- skip already-satisfied suite rows;
- only issue calls for missing suites;
- preserve factual usage when a new call was actually made;
- surface failures clearly so the subject remains resumable.

