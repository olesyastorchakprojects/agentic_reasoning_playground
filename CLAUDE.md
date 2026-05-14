# CLAUDE.md

## Read First

Read [AGENTS.md](./AGENTS.md) before starting any task. It covers repository structure, source authority order, editing rules, terminology, observability conventions, and what to synchronize when behavior changes.

## Database Safety

**Never truncate or drop without explicit user permission:**
- `diagnostics.runs`
- `diagnostics.run_iterations`
- `diagnostics.run_step_records`

These tables contain the golden dataset — expensive and time-consuming to regenerate.

**Only these eval tables may be cleared (and still only with explicit permission):**
- `diagnostics.eval_processing_state`
- `diagnostics.judge_llm_calls`
- `diagnostics.judge_results`
- `diagnostics.eval_iteration_summaries`
- `diagnostics.eval_run_summaries`

## Eval Runs

Never run `cargo run -- --config eval.toml` (a new eval run) without explicit user permission.

Before suggesting a new eval run, always ask: can this be done with `--resume-eval-run-id` instead? Regenerating a report from existing DB data avoids new judge calls entirely.

Cost reference: one 5-case batch ≈ $0.017 and ~200k tokens.

## Git Workflow

Do not create commits automatically. Wait for an explicit request before staging or committing anything.

## Known Intermittent Bug

The check constraint `runs_updated_not_before_created` can be violated under tight timing conditions. This is a known race condition — do not treat it as a new bug or attempt to fix it without context.
