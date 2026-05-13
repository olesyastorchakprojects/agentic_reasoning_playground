# Documentation Plan

## Goal

Capture the next documentation gaps so they do not get lost while the system is still changing quickly.

The current docs already explain:

- the product idea and diagnostic framing;
- the reasoning model;
- prompt context assembly;
- one prompt-evolution story for query structuring.

The biggest remaining gap is that the documentation set still says more about **how the assistant thinks** than about **how the system is built, evaluated, and debugged**.

## Recommended Next Docs

### 1. `ARCHITECTURE.md`

Why add it:

- It gives one shared map of the whole system.
- It explains how request-time components and offline data-preparation components fit together.
- It reduces onboarding cost for future contributors.
- It prevents important implementation assumptions from being scattered across code and implicit team memory.

What it should cover:

- end-to-end request flow;
- main runtime stages;
- storage systems and their responsibilities;
- core domain objects passed between stages;
- where prompt assets, retrieval outputs, and diagnostic state are produced and consumed.

### 2. `EVALUATION_PROTOCOL.md`

Why add it:

- It defines how we decide whether a change is actually better.
- It prevents prompt and retrieval iteration from becoming taste-driven.
- It creates a stable regression check when prompts, ranking logic, or schemas change.
- It makes past experiment results easier to interpret and compare.

What it should cover:

- evaluation goals;
- test case categories;
- qualitative and quantitative criteria;
- what counts as a regression;
- how to compare variants;
- how evaluation artifacts should be recorded.

### 3. `OBSERVABILITY_AND_DEBUGGING.md`

Why add it:

- It explains how to inspect real system behavior instead of reasoning from outputs alone.
- It shortens the loop when answers look wrong but the failure source is unclear.
- It makes runtime failures easier to localize across retrieval, context packing, prompting, and state updates.

What it should cover:

- what telemetry exists;
- what artifacts should be logged for each request;
- how to inspect a bad response;
- common failure signatures;
- how to separate retrieval problems from prompt problems from state-update problems.

## Good Follow-Ups After The Top 3

### 4. `DATA_MODEL.md`

Why it matters:

- It documents the structure of `IncidentCard`, chunk roles, theory records, and request-time data objects.
- It helps keep storage, retrieval, and prompting aligned when schemas evolve.

### 5. `REQUEST_LIFECYCLE.md`

Why it matters:

- It gives a single narrative walkthrough from user input to first response to continuation update.
- It complements architecture with a request-centric view that is easier for product and eval work.

### 6. `ADR_INDEX.md`

Why it matters:

- It creates a home for key decisions instead of hiding them inside long narrative docs.
- It makes it easier to track which choices are current, superseded, or still experimental.

### 7. `KNOWN_LIMITATIONS.md`

Why it matters:

- It forces explicit acknowledgment of current failure modes.
- It helps evaluation and roadmap planning stay grounded in real system weaknesses.

### 8. `CASE_STUDY.md`

Why it matters:

- It is useful for explanation, demos, and external communication.
- It is most valuable after architecture, evaluation, and observability are already documented, so the story can point to the underlying system design.

## Priority Order

Recommended sequence:

1. `ARCHITECTURE.md`
2. `EVALUATION_PROTOCOL.md`
3. `OBSERVABILITY_AND_DEBUGGING.md`
4. `DATA_MODEL.md`
5. `REQUEST_LIFECYCLE.md`
6. `ADR_INDEX.md`
7. `KNOWN_LIMITATIONS.md`
8. `CASE_STUDY.md`

## Why This Order

- `ARCHITECTURE.md` is the main missing anchor for the whole doc set.
- `EVALUATION_PROTOCOL.md` is needed before more prompt and retrieval iteration accumulates.
- `OBSERVABILITY_AND_DEBUGGING.md` is critical for making failures explainable during development.
- The remaining docs become easier and more consistent once those three foundations exist.
