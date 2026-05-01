## 1) Purpose

This document defines the physical SQL/storage contract layer for the new
diagnostics eval engine.

The documents in this folder translate the logical eval specs into concrete
PostgreSQL expectations for:

- table boundaries;
- column families;
- unique keys;
- indexes;
- upsert semantics;
- resume-safe write behavior.

These storage contracts are intentionally PostgreSQL-oriented because the eval
engine already relies on PostgreSQL for durable batch orchestration.

## 2) Storage Scope

The current MVP storage layer covers exactly these eval-owned tables:

- `eval_processing_state`
- `judge_results`
- `judge_llm_calls`
- `eval_iteration_summaries`
- `eval_run_summaries`

The current MVP does not require a persisted snapshot table for
`DiagnosticEvalIterationSnapshot`.

Snapshot construction remains a logical projection from runtime-owned
`RunState`.

## 3) Design Rules

All eval-owned tables must follow these rules:

- use explicit `eval_run_id` in every per-subject table;
- preserve `runtime_run_id` and `iteration_id` in every subject-level table;
- support idempotent resume through stable unique keys;
- avoid destructive overwrite of raw factual rows;
- separate semantic verdict rows from factual LLM-usage rows;
- keep aggregate/materialized tables distinct from raw result tables.

## 4) Timestamp Rules

All tables must use explicit timestamps.

The minimum conventions are:

- `created_at` means the row was first inserted into eval storage;
- `updated_at` means the row was last semantically changed by eval code;
- `started_at` and `completed_at` are stage-lifecycle timestamps and should be
  used only where stage progression semantics require them.

All timestamps should be stored in PostgreSQL `timestamptz`.

## 5) Identifier Types

The storage layer should prefer:

- `text` for externally generated opaque ids when UUID-only assumptions are not
  guaranteed by all producers;
- `uuid` only if the implementation already guarantees UUID values end to end.

The storage contract does not force one global id type, but the chosen type
must remain consistent across all eval-owned tables.

## 6) Numeric Type Rules

The storage layer should prefer:

- integer types for token counts and score values;
- `numeric` for USD pricing and cost values;
- `jsonb` for structured payloads and version maps.

Suggested numeric precision for USD values is sufficient to avoid cumulative
rollup drift across batch aggregates.

## 7) Upsert Philosophy

The eval engine must use two write patterns:

- append-only or insert-mostly for factual call ledgers;
- idempotent upsert for semantic and aggregate tables.

Concretely:

- `judge_llm_calls` should preserve each factual call row and must not collapse
  multiple distinct calls into one row;
- `eval_processing_state`, `judge_results`, `eval_iteration_summaries`, and
  `eval_run_summaries` should support deterministic `INSERT ... ON CONFLICT`
  behavior.

## 8) Foreign-Key Philosophy

The MVP may omit hard foreign keys from eval tables to runtime-owned storage if
that simplifies integration with existing runtime persistence.

However, eval-owned tables should maintain logical referential consistency
through:

- shared ids;
- orchestrator-owned frozen scope;
- deterministic write order.

If hard foreign keys are added later, they must not weaken resume behavior.

## 9) Recommended Physical Layer Files

This folder contains one file per core table:

- [eval_processing_state.sql.md](/home/olesia/code/dist_sys_assistant/Specification/evals/storage/eval_processing_state.sql.md:1)
- [judge_results.sql.md](/home/olesia/code/dist_sys_assistant/Specification/evals/storage/judge_results.sql.md:1)
- [judge_llm_calls.sql.md](/home/olesia/code/dist_sys_assistant/Specification/evals/storage/judge_llm_calls.sql.md:1)
- [eval_iteration_summaries.sql.md](/home/olesia/code/dist_sys_assistant/Specification/evals/storage/eval_iteration_summaries.sql.md:1)
- [eval_run_summaries.sql.md](/home/olesia/code/dist_sys_assistant/Specification/evals/storage/eval_run_summaries.sql.md:1)
