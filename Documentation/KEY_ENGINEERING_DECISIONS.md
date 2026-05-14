# Key Engineering Decisions

## Why This Document Exists

The repository already contains runtime code, specifications, and narrative architecture documents.

This document captures the rationale behind the most important design choices so that a reader can answer a different question from "how does it work?":

**why is it built this way instead of in a simpler or more conventional way?**

The goal is not to restate implementation details.
The goal is to preserve the main decisions, their intended benefits, and the tradeoffs they introduce.

## Runtime Shape

## 1. Stateful Diagnostic Loop Instead Of One-Shot Question Answering

The system is modeled as `run -> iteration -> step` instead of a single request-response call.

Why this was chosen:

- the product needs to pause, ask for clarification, and resume later without losing diagnostic history
- continuation input must update an existing investigation rather than restart from scratch
- execution state, errors, and intermediate artifacts need to remain inspectable

What this gives us:

- durable continuation behavior
- explicit orchestration boundaries
- better debugging and evaluation surfaces

Tradeoff:

- the runtime is more complex than a one-pass RAG pipeline

## 2. Structured Diagnostic State Instead Of Free-Form Answers

The runtime returns a bounded diagnostic state rather than treating the final output as unconstrained prose.

Why this was chosen:

- the most important user-facing output is usually the next discriminating check, not a polished explanation
- continuation requires stable state that can be updated across iterations
- evaluation is much easier when hypotheses, competing interpretation, and check semantics are explicit

What this gives us:

- clearer continuation behavior
- more disciplined responses
- stronger eval contracts

Tradeoff:

- the prompt and validation layers must enforce a more rigid output shape

## 3. Transition Policy Separated From Step Execution

The runtime keeps transition selection separate from step execution.

Why this was chosen:

- orchestration rules should be inspectable and testable without mixing them with retrieval or model logic
- initial and continuation flows have different canonical step orders and branching rules
- terminal transitions such as `WaitForUser`, `FinishWithResult`, and `FinishWithError` are part of the policy boundary, not side effects of random modules

What this gives us:

- explicit policy reasoning
- cleaner executor responsibilities
- a path for adding or changing policies later without rewriting the executor

Tradeoff:

- the system has more boundary types and more files than an inline orchestration loop

## 4. Structured Interpretation As Constrained Transformation Steps

The runtime uses dedicated constrained-transformation steps for both the initial user report and continuation observations rather than folding that work into retrieval or final generation.

Why this was chosen:

- both the initial report and later observations are too free-form for downstream adequacy checks, retrieval refresh, and evaluation
- some output fields benefit from controlled terminology while others should remain close to the user-provided wording
- the system needs conservative normalization, not aggressive diagnosis too early
- continuation updates also need explicit observation shaping before they can safely affect the diagnostic state

What this gives us:

- typed intermediate artifacts that can be judged directly
- clearer separation between observed evidence and inferred interpretations
- a controlled place to prefer omission over weak unsupported inference
- a shared design pattern across initial structuring and continuation observation handling

Tradeoff:

- extra transformation steps are introduced before retrieval refresh and final generation

## 5. Adequacy Gates And `WaitForUser` As First-Class Runtime Transitions

The system treats insufficient signal as a valid runtime outcome rather than forcing the pipeline to answer anyway.

Why this was chosen:

- a weak or ambiguous initial report can make retrieval and generation look confident while being poorly grounded
- continuation input may be non-observational, under-specified, or unsupported
- the product is more honest when it explicitly asks for the missing information

What this gives us:

- safer early stopping behavior
- clearer user-facing follow-up questions
- better separation between "insufficient evidence" and "bad final answer"

Tradeoff:

- some runs intentionally end without a diagnostic answer on that iteration

## 6. Separate Retrieval, Reranking, And Generation Boundaries

The runtime deliberately separates retrieval-like steps, branch reranking, prompt assembly, and generation into distinct modules and step kinds.

Why this was chosen:

- candidate selection, branch ranking, and final response generation fail in different ways
- continuation flow needs explicit branch reranking after new observations
- evaluation is more useful when stage boundaries remain attributable

What this gives us:

- clearer failure attribution
- easier experimentation on retrieval and ranking behavior
- less pressure to hide multiple concerns inside one opaque reasoning step

Tradeoff:

- the pipeline has more moving parts and more intermediate artifacts

## Evidence And Prompting

## 7. Primary Precedent, Alternative Context, And Theory As Separate Evidence Roles

The project does not flatten all evidence into one undifferentiated retrieval list.

Why this was chosen:

- the best matching precedent and the strongest competing context play different roles in diagnosis
- theory evidence helps explain mechanisms but should not automatically outrank incident-grounded evidence
- the model needs both grounding and disciplined ambiguity

What this gives us:

- stronger support for competing explanations
- better next-check generation
- clearer prompt packing semantics

Tradeoff:

- retrieval and prompt assembly logic are more opinionated than generic top-k retrieval

## 8. Card Retrieval Followed By Card Hydration

The runtime first retrieves compact card-level search documents, then hydrates the selected canonical cards from PostgreSQL.

Why this was chosen:

- semantic search and canonical structured storage serve different purposes
- retrieval indexes should stay optimized for search rather than full canonical representation
- downstream steps need the full structured card, not only the retrieval snippet

What this gives us:

- clean separation between searchable representations and source-of-truth records
- more stable structured inputs for prompt assembly
- easier evolution of retrieval documents without redefining canonical storage

Tradeoff:

- the runtime has a two-step precedent loading path instead of a single datastore lookup

## 9. Role-Based Prompt Packing Instead Of Passing Top-K Chunks Directly

Prompt assembly is a bounded selection layer, not a dump of the highest-scoring retrieved chunks.

Why this was chosen:

- the final prompt needs different kinds of evidence for different jobs
- retrieval score alone does not tell us which chunk is best for match grounding, competing pressure, or next-check guidance
- prompt usefulness depends on relevance density, not raw volume

What this gives us:

- more predictable prompt composition
- better support for discriminating checks
- clearer evaluation targets for evidence quality

Tradeoff:

- prompt assembly becomes a substantive design surface, not a trivial serialization step

## 10. Response Validation And Normalization As A Separate Trust Boundary

The runtime does not treat raw model output as final application output.

Why this was chosen:

- the generation step can be structurally invalid, semantically malformed, or out of contract
- downstream runtime and eval behavior need a trusted structured boundary
- a validated artifact is easier to persist, compare, and inspect than unconstrained model text

What this gives us:

- stronger runtime contracts
- safer persistence into run state
- cleaner evaluation inputs

Tradeoff:

- some model outputs are rejected or normalized instead of being passed through directly

## Contracts, Persistence, And Shared Types

## 11. Eval Engine As A Rust Binary Reusing Runtime Types

The evaluation engine is implemented as a Rust binary that uses the runtime library instead of being written as a separate tool with redefined data models.

Why this was chosen:

- many persisted artifacts are stored as JSON blobs in the database
- those JSON payloads already correspond to runtime-owned types
- reusing the same Rust types gives a more stable contract than redefining parallel eval-side models
- eval logic can use the same serialization and domain semantics as the runtime

What this gives us:

- less contract drift between runtime and eval
- more stable decoding of persisted artifacts
- lower maintenance overhead when runtime payloads evolve

Tradeoff:

- the eval engine is more tightly coupled to the runtime crate than a fully separate external tool would be

## 12. Persisted JSON Blobs Are An Explicit Contract Boundary

The project stores many step outputs and related artifacts as typed JSON payloads rather than flattening every intermediate structure into separate relational tables.

Why this was chosen:

- step outputs vary by step kind and evolve over time
- preserving the original typed payload is useful for replay, inspection, and evaluation
- the contract is clearer when serialization follows shared domain types rather than many loosely synchronized table definitions

What this gives us:

- durable storage of rich runtime artifacts
- simpler persistence for heterogeneous step outputs
- better compatibility with the Rust eval engine and shared runtime types

Tradeoff:

- some downstream querying is less convenient than fully normalized relational storage

## 13. Specification Is The Source Of Truth For Code And Test Generation

Generated code is not treated as the design authority in this repository.

The source of truth is the specification.

Why this was chosen:

- contracts, types, rules, interfaces, and boundaries are easier to reason about when they are defined explicitly before generation
- model-assisted code and test generation becomes easier to direct when the intended behavior is already fixed in a human-reviewable form
- generated code is easier to audit when it can be reviewed against a stable specification rather than against inferred intent

What this gives us:

- lower ambiguity in design intent
- better reproducibility for generation work
- a clearer review discipline for both code and tests

Tradeoff:

- the repository must maintain specifications as first-class artifacts rather than treating them as optional supporting notes

## Evaluation And Observability

## 14. Eval Runs Use Iterations As The Main Unit Of Judgment

The evaluation layer judges one iteration inside one runtime run rather than collapsing everything to whole-run or final-text-only scoring.

Why this was chosen:

- continuation is one of the central product behaviors
- first-response and continuation quality are not the same task
- iteration-level artifacts are already explicit in `RunState`

What this gives us:

- continuation-specific evaluation
- cleaner attribution of failures to one stage of the diagnostic loop
- a better match between runtime structure and eval structure

Tradeoff:

- eval modeling is more detailed than a simple answer-grading setup

## 15. Frozen Eval Scope And Resume Semantics

An eval run freezes its subject scope and keeps that scope stable across resume.

Why this was chosen:

- a resumed run should not silently absorb newly eligible runtime runs
- comparisons between eval runs should remain meaningful
- evaluation artifacts should describe one stable experiment boundary

What this gives us:

- reproducible eval runs
- trustworthy comparisons
- safer resume behavior after failure

Tradeoff:

- operators must create a new eval run when they want a new population, rather than mutating the old one

## 16. Observability Is A Required Contract, Not Optional Instrumentation

The runtime and eval engine are designed to be inspectable through traces and related observability surfaces.

Why this was chosen:

- the system has many failure surfaces: structuring, retrieval, prompt packing, generation, validation, persistence, and evaluation
- stage-level visibility is necessary for engineering decisions
- token usage, latency, and failure attribution are part of product quality, not incidental metadata

What this gives us:

- better debugging
- stronger performance and cost visibility
- more trustworthy comparisons between variants

Tradeoff:

- instrumentation is part of core engineering work, not an optional later polish pass

## 17. Phoenix Gets A Semantic OpenInference Slice Inside The Same Trace

The project supports a Phoenix-facing OpenInference view, but it does not emit a second parallel trace.

Instead, a fixed semantic subset of spans inside the same OpenTelemetry trace is annotated for Phoenix-oriented inspection.

Why this was chosen:

- the full engineering trace should remain available for detailed runtime diagnosis
- Phoenix still needs a compact semantic surface for chain, retriever, and model-oriented inspection
- duplicating trace pipelines would make instrumentation and trace interpretation harder to maintain

What this gives us:

- one trace with two useful readings
- detailed operational visibility and compact AI-facing visibility at the same time
- less duplication in observability plumbing

Tradeoff:

- instrumentation must be disciplined enough to preserve both the operational and semantic views inside one trace structure

## 18. Telemetry Flows Through An OpenTelemetry Collector Layer

The application does not export telemetry directly to every observability backend.

Instead, telemetry is sent through an OpenTelemetry Collector layer that sits between the app and downstream backends.

Why this was chosen:

- the project needs multiple observability surfaces without turning each backend into a direct runtime integration concern
- routing, shaping, and backend-specific export logic fit better in an intermediate telemetry layer than in the application runtime
- runtime observability stays less tightly coupled to any single backend

What this gives us:

- cleaner telemetry routing
- easier multi-backend observability setup
- a more backend-agnostic runtime instrumentation model

Tradeoff:

- the operating stack gains another infrastructure component that must be configured and understood

## 19. Evidence Is Kept Separate From Implementation

Produced run artifacts, eval outputs, and supporting traces are treated as evidence, not as part of the code path itself.

Why this was chosen:

- the project needs an inspectable engineering record of what actually happened
- runtime evidence, reports, and case-study materials should remain reviewable without being mixed into the source tree structure of the implementation itself
- code, specifications, documentation, and produced evidence serve different purposes

What this gives us:

- cleaner repository boundaries
- easier review of experiment outputs and case-study artifacts
- a stronger distinction between implementation and proof

Tradeoff:

- readers need to understand more than one top-level repository area

## 20. Local Infrastructure Is Part Of The Operating Model

The local stack for storage, observability, and runtime inspection is treated as part of the system rather than as a disposable developer convenience.

Why this was chosen:

- the project depends on reproducible storage and tracing surfaces
- architecture, evaluation, and observability docs are only useful if the environment can be reproduced locally
- demos, debugging, and development all benefit from the same operating setup

What this gives us:

- more reliable end-to-end validation
- easier onboarding for contributors
- a closer match between development and real project operation

Tradeoff:

- local setup is heavier than a pure library-style repository
