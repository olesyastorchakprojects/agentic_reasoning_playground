## 1) Purpose

This document defines the module contract for the eval crate's `storage`
module.

This module is the database access boundary for eval-owned relational state.

## 2) Responsibilities

The storage module owns:

- PostgreSQL connection setup used by the eval crate;
- repository interfaces and implementations;
- SQL row models for eval-owned tables;
- query helpers for selection and upsert;
- transaction boundaries where needed for eval persistence.

It is the code-level home for the tables described in the storage and DDL
specifications.

## 3) Non-Responsibilities

The storage module must not own:

- `RunState` business interpretation;
- judge prompt logic;
- aggregate formulas;
- markdown rendering;
- CLI parsing.

## 4) Required Repository Areas

The module should expose repositories or repository traits for at least:

- `eval_processing_state`
- `judge_results`
- `judge_llm_calls`
- `eval_iteration_summaries`
- `eval_run_summaries`

It may also expose manifest artifact helpers if the implementation chooses to
persist manifest-related metadata through storage-oriented code paths.

## 5) Public Types

Recommended public row/domain types include:

- `EvalProcessingStateRow`
- `JudgeResultRow`
- `JudgeLlmCallRow`
- `EvalIterationSummaryRow`
- `EvalRunSummaryRow`

Recommended public repository interfaces include types or traits conceptually
equivalent to:

- `EvalProcessingStateRepository`
- `JudgeResultsRepository`
- `JudgeLlmCallsRepository`
- `EvalIterationSummariesRepository`
- `EvalRunSummariesRepository`

## 6) Query Ownership

The storage module owns:

- FIFO subject selection queries;
- existence checks for already-satisfied suites;
- idempotent upserts;
- aggregate-row persistence;
- dashboard-facing summary row persistence.

The orchestrator may decide when these queries are used, but it should not own
their SQL details.

## 7) Dependency Rules

The storage module may depend on:

- `config` for postgres settings
- shared runtime id types if needed

It must not depend on:

- `orchestrator`
- `report`

This keeps storage reusable from tests and stage workers.

## 8) Testing Boundary

The storage module should be testable in two layers:

- unit tests around row mapping and query-shape helpers where practical;
- postgres integration tests for real DDL-backed behavior.

Because DDL already exists, storage integration tests should eventually become
the source of truth for repo behavior.

