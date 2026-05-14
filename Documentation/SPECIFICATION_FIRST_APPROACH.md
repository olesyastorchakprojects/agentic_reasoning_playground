# Specification-First Approach

## What This Document Covers

This document explains the specification-first workflow used in this repository to design runtime behavior, generate implementation code, and generate tests.

The core separation is simple:

- the specification defines contracts, types, rules, and boundaries first
- code and tests are generated or implemented against those specifications
- the specification, not the generated code, is treated as the source of truth

This is one of the most important structural choices in the repository.

* * *
## Core Idea

The repository is designed so that the main design work happens before implementation generation.

Instead of starting from code and documenting it afterward, the project first makes system behavior explicit through:

- contracts
- types
- transition rules
- ownership boundaries
- persistence semantics
- observability requirements
- evaluation expectations

Only after that does the implementation get generated, written, or revised.

This keeps design intent outside the model and makes generation more constrained, more reproducible, and easier to audit.

* * *
## Why This Approach Is Used Here

This repository contains several tightly connected layers:

- a stateful runtime
- a step-by-step orchestrator
- a specification-owned transition policy
- request-pipeline modules
- persistent run state
- retrieval and evidence layers
- an eval engine
- observability surfaces

Without explicit specifications, generated code in a repository like this can easily:

- invent implicit behavior
- blur orchestration and execution boundaries
- drift across runtime and eval
- generate tests that merely mirror implementation
- weaken persistence and observability contracts

The specification-first approach exists here to reduce exactly that ambiguity.

* * *
## What Counts As A Specification In This Project

In this repository, a specification is not a vague feature request.

It defines the system in operational terms, including:

- input and output types
- required fields and JSON structure
- canonical step order
- branching and transition rules
- persistence behavior
- test expectations and test-owned behavior contracts
- terminal outcomes such as `WaitForUser`, `FinishWithResult`, and `FinishWithError`
- observability spans, events, and related telemetry expectations
- evaluation-facing artifact shapes

A useful spec therefore describes not only what a component is for, but also what it may accept, produce, assume, persist, and expose.

* * *
## Where Specifications Live

The main specification layer lives under [Specification/](../Specification).

Important areas include:

- [Specification/runtime/](../Specification/runtime) for runtime behavior, orchestrator rules, request-pipeline modules, API clients, and observability contracts
- [Specification/contracts/](../Specification/contracts) for runtime and storage contract definitions
- [Specification/evals/](../Specification/evals) for evaluation storage and related eval-layer contract material

This layout is deliberate.
The repository keeps specifications as a distinct engineering surface rather than embedding all system intent inside code comments or generated artifacts.

* * *
## What Is Specified Before Generation

Before code generation, the project tries to make four things explicit.

### Contracts

Contracts define boundaries between runtime stages, persistence layers, and evaluation consumers.

Examples include:

- step outputs stored in run state
- response validation outputs
- persisted eval artifacts
- observability-facing payload shapes

### Types

Types define the shapes passed between stages so that behavior does not depend on hidden shared state or informal conventions.

This matters especially because many persisted artifacts are stored as JSON blobs and later decoded by both runtime and eval components.

### Rules

Rules define behavior that should not be left to model interpretation or implementation guesswork.

Examples include:

- canonical step order
- continuation branching after observation-boundary resolution
- adequacy-gate behavior
- error and terminal transitions
- validation requirements
- required test-visible invariants
- required observability span and event behavior

### Interfaces

Interfaces define how one subsystem may interact with another so the implementation does not invent accidental couplings that were never intended.

* * *
## Why The Specification Is The Source Of Truth

Generated code is useful, but it is not treated as the design authority.

The specification remains the source of truth because:

- generated code can vary more easily than design intent
- different generations should still implement the same rules
- generated tests should validate the contract, not only the current implementation
- runtime, eval, and observability surfaces should stay aligned with the same definitions

Without that separation, the repository would drift back into implementation-first behavior.

* * *
## What The Model Is Expected To Do

In this workflow, the model is not asked to invent system behavior.

It is asked to implement already-declared behavior.

Its role is to:

- generate code from explicit contracts
- generate tests from explicit rules and expected artifacts
- preserve stage boundaries
- preserve persistence and observability contracts
- avoid introducing behavior that is not grounded in the specification

The model acts here as an implementation accelerator, not as the primary designer of the system.

* * *
## Why This Matters For Tests Too

The same approach applies to test generation.

Tests are more meaningful when they are generated from explicit contracts and rules, because they can check:

- schema conformance
- canonical transition behavior
- failure paths
- persistence expectations
- continuity rules across iterations

That is different from tests that only snapshot current implementation behavior without checking whether the behavior is actually correct.

The same principle applies to observability.

In this repository, important spans and events are not treated as incidental logging details. They are also specification-owned artifacts, so runtime instrumentation can be reviewed against explicit expectations instead of being left to ad hoc implementation choices.

* * *
## What This Improves

This approach improves several important things.

### 1. Less Ambiguity In Generation

The more explicit the rules are, the less room there is for model improvisation.

### 2. Easier Review

Review becomes simpler because the main question is:

**Does this implementation satisfy the specification?**

### 3. More Stable Contracts Across Runtime And Eval

The same specification mindset helps keep runtime artifacts, persisted JSON payloads, and eval-side decoding aligned.

### 4. Less Cross-Layer Drift

The same design authority is used for code, tests, persistence contracts, and observability spans, events, and higher-level observability surfaces.

* * *
## What This Does Not Mean

Specification-first does not mean the repository is fixed forever or that the first spec is perfect.

It does not remove iteration, and it does not eliminate the need for code review.

It changes where iteration happens:

- refine the specification
- regenerate or revise implementation against it
- review the result against the updated contract

That is different from repeatedly patching generated code while keeping the design intent implicit.

* * *
## How This Shapes The Repository

This approach is one reason the repository is split across several distinct engineering surfaces:

- execution
- specification
- measurement
- evidence
- documentation

That split reflects a deliberate separation between:

- what the system is supposed to do
- how it is implemented
- how it is measured
- what actually happened when it ran
- how it is explained to humans
