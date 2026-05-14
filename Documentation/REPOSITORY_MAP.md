# Repository Map

## Purpose

This document explains the repository layout in clear project terms.

It is especially useful for onboarding, architecture review, and project explanation because this repository intentionally separates implementation, specifications, measurement, produced evidence, and narrative documentation.

That separation is one of the clearest expressions of the project's specification-first approach.

* * *
## Top-Level Areas

The repository is organized around a small number of top-level directories with distinct roles.

### `Execution/`

`Execution/` contains runnable code and the files needed to operate the system.

This includes:

- the Rust runtime crate under `Execution/distributed_diagnostics/`
- the Rust eval binary under `Execution/distributed_diagnostics_eval/`
- ingest and conversion tooling
- schemas and runtime-facing artifacts
- docker-related local operating assets
- tests, scripts, and supporting execution assets

If something is executed, compiled, tested, or launched, it usually belongs here.

### `Specification/`

`Specification/` contains the written source of truth for how the system is supposed to behave.

This includes:

- runtime behavior specs
- orchestrator and transition-policy specs
- request-pipeline module specs
- API client specs
- observability-related specs
- runtime and storage contracts
- eval-related contract material

This is the repository's contract layer.

### `Measurement/`

`Measurement/` contains assets used to measure and visualize system behavior.

In the current repository, that especially includes eval-oriented Grafana surfaces and related measurement material.

This layer is about how behavior is observed and compared.

### `Evidence/`

`Evidence/` contains produced or source evidence artifacts used by the system and its evaluation story.

This includes:

- incident cards
- incident-report chunks
- eval datasets
- eval run outputs

This layer answers two different but related questions:

- what knowledge artifacts does the system operate over?
- what happened when the system actually ran or was evaluated?

### `Documentation/`

`Documentation/` contains narrative project documentation.

This is where presentation-oriented and explanatory documents live:

- architecture writeups
- reasoning-model docs
- prompt-context docs
- case studies
- repository-level explanatory docs such as this one

It is not the same as `Specification/`.
`Documentation/` explains the project; `Specification/` constrains it.

* * *
## Important Subtrees

Some subtrees are especially important when navigating the repository.

### `Execution/distributed_diagnostics/`

The main runtime crate.

This is where the orchestrator, request-pipeline implementation, persistence-facing runtime code, and observability instrumentation come together.

### `Execution/distributed_diagnostics_eval/`

The eval engine binary.

This subtree is important because the evaluation layer is implemented in Rust and reuses runtime-owned types instead of redefining separate parallel models.

### `Execution/artifacts/`

Repository-owned execution inputs and helper artifacts such as vocabularies and tokenizer-related data used by the runtime or supporting tooling.

### `Execution/docker/`

Local operating environment assets for services such as storage and observability dependencies.

This is part of the reproducible operating model, not just disposable developer convenience.

### `Specification/runtime/`

The main runtime specification area.

This subtree describes orchestrator behavior, request-pipeline modules, observability expectations, and other runtime-owned contracts.

### `Specification/contracts/`

Contract-oriented specifications for runtime and storage boundaries.

### `Evidence/evals/`

Produced evaluation artifacts, including datasets and run outputs.

### `Evidence/incident_cards/`

Canonical structured incident cards used as practical precedent knowledge.

### `Evidence/incedent_reports/chunks/`

Chunked report material used by retrieval layers.

The directory name is currently spelled `incedent_reports` in the repository; this map reflects the actual on-disk structure.

* * *
## Why This Structure Matters

This layout is not cosmetic.

It prevents several common repository problems:

- executable code mixed with produced artifacts
- contracts hidden inside implementation details
- evaluation logic drifting from runtime types
- architecture explanation being confused with source-of-truth specifications

The structure makes the project easier to:

- navigate
- explain
- review
- validate
- extend

* * *
## A Simple Mental Model

One useful way to remember the repository is:

- `Execution/` is how the system runs
- `Specification/` is what the system is supposed to do
- `Measurement/` is how the system is observed and compared
- `Evidence/` is what the system runs on and what it produces
- `Documentation/` is how the system is explained

That model is intentionally stronger than a generic "src plus docs" layout.

* * *
## How To Read The Repository

A practical navigation order is:

1. start with [Documentation/README.md](./README.md) for the main documentation reading path
2. read [ARCHITECTURE.md](./ARCHITECTURE.md) and [KEY_ENGINEERING_DECISIONS.md](./KEY_ENGINEERING_DECISIONS.md)
3. open [Specification/](../Specification) when you need the authoritative runtime and contract definitions
4. inspect `Execution/distributed_diagnostics/` and `Execution/distributed_diagnostics_eval/` when you want to see the concrete implementation
5. use `Evidence/` and `Measurement/` when you want to inspect what the system ran on, produced, or measured
