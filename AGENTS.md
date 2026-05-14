# AGENTS Guide

## Purpose

This file is the repository-local working guide for coding agents.

Use it to understand:

- where authoritative intent lives
- how to navigate the repository
- what to edit carefully
- what to synchronize when behavior changes

This file does not replace `Specification/` or `Documentation/`.
It explains how to work with them safely.

## Project Shape

This repository is a specification-first distributed diagnostics project with:

- a Rust runtime under `Execution/distributed_diagnostics/`
- a Rust eval binary under `Execution/distributed_diagnostics_eval/`
- authoritative behavior and contract definitions under `Specification/`
- produced knowledge and evaluation artifacts under `Evidence/`
- narrative documentation under `Documentation/`

The runtime is stateful.
Core concepts are `run -> iteration -> step`.

## Authority Order

When sources disagree, use this order by default:

1. `Specification/`
2. code and tests under `Execution/`
3. `Documentation/`
4. evidence artifacts and traces under `Evidence/`

Interpretation:

- `Specification/` is the source of truth for behavior, contracts, boundaries, tests, and important observability expectations.
- `Documentation/` explains the system, but should be updated to match the spec and implementation.
- `Evidence/` shows what happened in real runs; it is useful for validation and examples, but it is not the design authority.

## Read This First

For most tasks, read in this order:

1. [Documentation/README.md](./Documentation/README.md)
2. [Documentation/ARCHITECTURE.md](./Documentation/ARCHITECTURE.md)
3. [Documentation/KEY_ENGINEERING_DECISIONS.md](./Documentation/KEY_ENGINEERING_DECISIONS.md)
4. [Documentation/SPECIFICATION_FIRST_APPROACH.md](./Documentation/SPECIFICATION_FIRST_APPROACH.md)
5. the relevant files under `Specification/runtime/` or `Specification/contracts/`

If the task is runtime-policy-related, read:

- `Specification/runtime/orchestrator/`
- relevant case-study or architecture docs only after the spec

If the task is eval-related, read:

- `Specification/evals/`
- `Documentation/EVALUATION_STORY.md`
- code under `Execution/distributed_diagnostics_eval/`

## Important Repository Areas

- `Execution/distributed_diagnostics/`: main runtime crate
- `Execution/distributed_diagnostics_eval/`: eval engine binary
- `Specification/runtime/`: runtime behavior specs
- `Specification/contracts/`: runtime and storage contracts
- `Evidence/incident_cards/`: canonical structured incident cards
- `Evidence/evals/`: eval datasets and run outputs
- `Documentation/`: narrative docs and diagrams

## Working Model

The repository uses a specification-first workflow.

Default rule:

- if behavior, contracts, types, transition rules, test expectations, or important observability behavior change, update the relevant spec first or at the same time as the implementation

Do not treat generated or current code as the design authority when the repository already has a relevant spec.

## Editing Rules

Safe default:

- edit `Specification/`, `Execution/`, and `Documentation/` when the task requires it
- treat `Evidence/` as evidence, not as normal implementation surface

Be careful with:

- diagram sources in `Documentation/*.uml`: if you change them, also regenerate the corresponding `.svg`
- docs indexes such as `Documentation/README.md`: update them when adding, renaming, or deleting major docs
- persisted artifact assumptions: many runtime and eval payloads are JSON blobs backed by shared Rust types

Do not casually modify:

- `Evidence/evals/runs/` outputs
- trace exports used as case-study evidence
- exploratory files under `Z_examples/`

`Z_examples/` is not authoritative.
Do not treat it as current design truth unless the user explicitly asks to use it as reference material.

## Terminology

Do not blur these terms:

- `run`: the durable container for one diagnostic session
- `iteration`: one bounded pass inside a run
- `step`: one executable unit inside an iteration
- `initial`: the first iteration
- `continuation`: later iterations after new user input
- `primary precedent`: the leading matched incident card
- `alternative context`: competing incident context kept alongside the primary branch
- `theory evidence`: mechanism-level evidence retrieved separately from incident evidence

Also keep these boundary terms precise:

- `WaitForUser`
- `FinishWithResult`
- `FinishWithError`

These are policy-level outcomes, not casual prose labels.

## Observability Conventions

Important observability behavior is specification-owned in this repository.

Keep in mind:

- Phoenix gets a semantic OpenInference slice inside the same OTEL trace
- the project does not maintain a second parallel trace just for Phoenix
- telemetry flows through an OpenTelemetry Collector layer

If a task changes span names, event behavior, or telemetry shape, check whether the relevant observability spec also needs updating.

## What To Sync

When making changes, synchronize neighboring surfaces deliberately.

If you change orchestrator or transition behavior:

- update the relevant `Specification/runtime/orchestrator/` files
- update affected docs in `Documentation/`
- update diagrams if the pipeline shape changed

If you change request-pipeline behavior:

- update the relevant `Specification/runtime/request_pipeline/` files
- check whether prompt-context or architecture docs need updates

If you change runtime or eval payload types:

- check persistence implications
- check shared-type implications for the Rust eval binary
- update docs if the external shape is user-visible or evaluation-visible

If you add or remove major docs:

- update `Documentation/README.md`

## Documentation Style

Preferred style in this repository:

- clear project-facing language
- avoid `human-oriented`, `for humans`, and similar phrasing
- keep specs authoritative and docs explanatory

Do not introduce wording that implies a document is authoritative if it is only narrative.

## Verification

For documentation-only changes:

- verify links and neighboring indexes
- regenerate diagrams if `.uml` changed

For runtime or eval changes:

- run the narrowest relevant tests first
- prefer validating against the spec, not only against current code behavior

## When In Doubt

If you are unsure whether something is authoritative:

1. check `Specification/`
2. check `Documentation/KEY_ENGINEERING_DECISIONS.md`
3. check `Documentation/SPECIFICATION_FIRST_APPROACH.md`

If uncertainty remains, prefer preserving existing boundaries rather than inventing a new one.
