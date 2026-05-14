# Distributed Diagnostics Assistant

This repository is a specification-first diagnostic assistant for distributed systems incidents.

It is built around a simple idea: instead of producing a one-shot free-form answer over incident documents, the system builds a bounded diagnostic state. It retrieves a leading precedent, keeps competing context in view, adds theory-level explanation, proposes one discriminating check, and updates that state when new observations arrive.

Key entry points:

- [Amazon RDS case study](./Documentation/CASE_STUDY_AMAZON_RDS_READER_STALE_READS.md)
- [Runtime architecture](./Documentation/ARCHITECTURE.md)
- [Documentation index](./Documentation/README.md)

## Start Here

The best way to understand this project is to read the [Amazon RDS case study](./Documentation/CASE_STUDY_AMAZON_RDS_READER_STALE_READS.md).

It shows one diagnostic run across three iterations: the system starts with competing explanations, accepts new observations, refreshes retrieval, rebuilds prompt context, updates hypothesis confidence, and proposes a more targeted discriminating check.

For a quick architectural picture, start with [Architecture](./Documentation/ARCHITECTURE.md).

## Why This Repository Matters

This is not a generic RAG repository.

The project is interesting because it combines:

- a stateful diagnostic loop modeled as `run -> iteration -> step`
- a bounded diagnostic response shape instead of unconstrained answer text
- precedent-guided reasoning with primary precedent, alternative context, and theory evidence
- continuation as an update to prior diagnostic state rather than a restart
- a specification-first workflow where contracts define behavior before code and tests
- a Rust runtime and a Rust eval engine that share runtime-owned types and persisted JSON-blob contracts
- strong observability with OpenTelemetry, Phoenix-facing semantic spans, and explicit evaluation surfaces

## What The System Does

At a high level, the runtime:

1. normalizes and structures an initial user-reported incident symptom
2. decides whether the input is diagnostically sufficient or whether it should ask follow-up questions
3. retrieves a leading precedent, competing incident context, and theory evidence
4. assembles a bounded prompt context for generation
5. returns a structured diagnostic state with hypotheses and one next check
6. accepts later observations and updates that state across continuation iterations

The goal of the first response is usually not to claim a final root cause.
The goal is to produce the best current diagnostic frame and the next most useful check.

## Core Design Ideas

- `Diagnostic state instead of free-form answers`: the system returns structured hypotheses, competing interpretation, and a discriminating check.
- `Continuation as state update`: later observations refine the current case instead of restarting the investigation.
- `Specification-first development`: specs are the source of truth for behavior, tests, and important observability expectations.
- `Shared runtime/eval contracts`: the Rust eval engine reuses runtime-owned types rather than redefining parallel models.
- `Evidence-backed repository structure`: implementation, specifications, measurement, evidence, and documentation are intentionally separated.
- `Observability as a first-class concern`: traces, semantic OpenInference spans, and evaluation outputs are part of the engineering model.

## Repository Structure

The top-level layout is intentional:

- `Execution/` contains runnable code, including the runtime crate and the eval binary.
- `Specification/` contains the authoritative behavior and contract definitions.
- `Measurement/` contains measurement and visualization assets.
- `Evidence/` contains incident knowledge artifacts and produced evaluation outputs.
- `Documentation/` contains architecture, design, case-study, and project explanation documents.

For a fuller walkthrough, see [Documentation/REPOSITORY_MAP.md](./Documentation/REPOSITORY_MAP.md).

## Documentation Entry Points

The main documentation hub is [Documentation/README.md](./Documentation/README.md).

Good reading order:

1. [Documentation/CASE_STUDY_AMAZON_RDS_READER_STALE_READS.md](./Documentation/CASE_STUDY_AMAZON_RDS_READER_STALE_READS.md)
2. [Documentation/OVERVIEW.md](./Documentation/OVERVIEW.md)
3. [Documentation/ARCHITECTURE.md](./Documentation/ARCHITECTURE.md)
4. [Documentation/KEY_ENGINEERING_DECISIONS.md](./Documentation/KEY_ENGINEERING_DECISIONS.md)
5. [Documentation/SPECIFICATION_FIRST_APPROACH.md](./Documentation/SPECIFICATION_FIRST_APPROACH.md)
6. [Documentation/EVALUATION_STORY.md](./Documentation/EVALUATION_STORY.md)
7. [Documentation/OBSERVABILITY_STORY.md](./Documentation/OBSERVABILITY_STORY.md)

## Specification-First Workflow

One of the strongest repository-level decisions is that the specification is the source of truth.

In practice:

- contracts, types, rules, and boundaries are defined in `Specification/`
- code and tests are generated or implemented against those specs
- generated or current code is reviewed against the specification, not treated as the design authority

This is especially important in a repository where runtime behavior, persisted artifacts, eval logic, and observability expectations all need to stay aligned.

## Evaluation And Observability

The repository includes more than a runtime implementation.

It also includes:

- a Rust eval engine under `Execution/distributed_diagnostics_eval/`
- iteration-based evaluation logic and artifacts
- observability surfaces described in [Documentation/OBSERVABILITY_STORY.md](./Documentation/OBSERVABILITY_STORY.md)
- a Phoenix-facing OpenInference semantic slice inside the same OTEL trace, rather than a separate parallel trace

The project is designed to make runtime behavior inspectable, comparable, and reviewable rather than opaque.

## Current Status

The repository already contains:

- a documented runtime architecture
- a documented reasoning model
- a concrete multi-iteration case study
- a specification-first contract layer
- an eval story and observability story

Detailed bring-up docs are still being consolidated.

## Getting Started

If you are new to the repository:

1. start with the [Amazon RDS case study](./Documentation/CASE_STUDY_AMAZON_RDS_READER_STALE_READS.md)
2. then read [Documentation/README.md](./Documentation/README.md)
3. use [AGENTS.md](./AGENTS.md) for repository-local working conventions
4. open `Specification/` when you need the authoritative contracts and runtime rules

## Additional Guides

- [Documentation/README.md](./Documentation/README.md): documentation index
- [AGENTS.md](./AGENTS.md): repository-local working guide for coding agents
- [Documentation/REPOSITORY_MAP.md](./Documentation/REPOSITORY_MAP.md): repository layout and navigation
