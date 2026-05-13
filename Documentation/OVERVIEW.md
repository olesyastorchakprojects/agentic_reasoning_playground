# Distributed Diagnostics Overview

## What This System Does

This project is a precedent-guided diagnostic assistant for distributed systems incidents.

It does not try to guess a final root cause from a single user message.
Instead, it:

- finds the most relevant incident precedent from a curated corpus;
- uses a theory corpus to supply mechanism-level explanation alongside the incident precedent;
- retrieves primary and alternative incident context to widen the diagnostic search space;
- synthesizes that context into a small set of working hypotheses;
- proposes one discriminating check that helps test whether the live hypotheses remain viable;
- updates the diagnostic state when new observations arrive.

The core product idea is:

**a precedent-guided diagnostic state built around a leading explanation, competing explanations, and one check that helps separate them**

This is the main organizing principle for the retrieval pipeline, the prompts, the continuation loop, and the evaluation framework.

![Distributed Diagnostics overview screen](./images/Screenshot%202026-05-10%20163812.png)

The UI exposes the same reasoning primitives described in this documentation:

- signal quality;
- primary and alternative incident context;
- working hypotheses and a competing interpretation;
- one discriminating check;
- iteration-by-iteration updates as new observations arrive.

## Why This Is Not Just RAG

The system is not intended to behave like a generic question-answering assistant over incident documents.

A simple retrieval-and-summary system would be useful for recall, but it would miss the main diagnostic problem:

- multiple mechanisms can produce similar symptoms;
- the first useful response is often not a conclusion but the next best check;
- new observations should update the investigation state rather than restart reasoning from scratch.

This system is therefore built around **controlled investigation**, not just retrieval.

The goal of the first response is not “name the root cause”.
The goal is to:

- identify the strongest current explanation;
- keep a competing explanation in view rather than collapsing too early to one story;
- propose a single useful check that distinguishes them.

## Reasoning Model

The system reasons in a small diagnostic state rather than a free-form chat style.

### Retrieval Context

The assistant retrieves more than one kind of supporting context:

- a best-matching primary incident precedent;
- alternative incident context that shares part of the symptom pattern;
- theory evidence that helps name or explain the mechanism.

These are context sources, not final diagnoses.
They constrain and enrich the reasoning state, but they do not map one-to-one to the final user-visible hypotheses.

Alternative incident context is especially useful because it keeps the assistant from collapsing too early onto the first precedent match.
It does not need to become a separate displayed hypothesis on its own.
Often it acts as competing pressure that changes:

- which explanation leads;
- which explanation competes;
- what the next check needs to separate.

### Theory Layer

Theory evidence is retrieved alongside incident context as a separate mechanism-level layer.

It does not define the leading or competing explanation by itself.
Instead, it helps the system:

- name the underlying distributed-systems mechanism more clearly;
- connect a practical incident pattern to a more general explanation;
- add explanatory support without replacing precedent-guided diagnosis.

### Problem Understanding

`problem_understanding` is the neutral technical framing of the symptom.
It describes what is happening without yet committing to why it is happening.

### Hypotheses

Hypotheses are the tracked candidate explanations in the current diagnostic state.

They are expected to be:

- compact;
- mechanistically meaningful;
- revisable when new observations arrive.

In practice, the system usually works best with two live hypotheses:

- the leading explanation;
- the strongest competing explanation.

A third hypothesis is useful only when it materially changes ranking, interpretation, or the next check.

The source of a hypothesis may be `primary_incident`, `alternative_context`, or `theory_mechanism`.
What matters is not where the hypothesis came from, but whether it is a real live explanation that changes the next diagnostic move.

### Competing Interpretation

`competing_interpretation` is not a second full diagnosis.
It is a compact summary of the strongest rival reading of the current case.

It is often shaped by alternative incident context, but it is not limited to that source.
Its role is to keep the user oriented around the main competing story even when the visible hypotheses are mostly synthesized from the same primary incident family.

### Discriminating Check

The most important field in the response is usually not the explanation text.
It is the next check.

A good check:

- is exactly one next action;
- is operational rather than abstract;
- helps distinguish between live hypotheses by testing whether they remain viable;
- converts evidence into a better next move.

`result_interpretation.supports_primary_if` and `supports_competing_if` explain how to read the outcome of that check.
They are about the leading and competing explanations in the current diagnostic state, not about whether a hypothesis came from the primary or alternative retrieval bucket.

This is the system’s main user-facing value.

## Diagnostic Loop

The project is built around a continuation loop.

### First Response

Given an initial user problem, the system:

1. structures the query;
2. checks signal quality for a first diagnostic response using the structured query;
3. retrieves a primary precedent, explicit alternative context, and theory context;
4. gathers and role-packs supporting evidence;
5. produces a first diagnostic response with hypotheses and one check.

### Observation Update

When the user provides a new observation, the system should not simply rewrite the same answer.

Instead, it should:

1. resolve the new observation against the existing context so short follow-ups become standalone factual updates;
2. accept only input that behaves like an observation, rather than a new question or ambiguous clarification request;
3. extract a compact set of atomic observations and decide whether the input is sufficient to continue or whether the system should ask the user for clarification;
4. update the existing hypothesis state;
5. strengthen, weaken, or reject hypotheses where warranted;
6. choose the next best check for the updated state.

The continuation loop matters because real incident investigation is sequential.
The system is useful only if it becomes more specific after new evidence appears.

Continuity is important here.
The system should adapt hypotheses and checks not only to the newest observation in isolation, but to:

- the previous understanding of the problem;
- the existing live hypotheses;
- the newly resolved observation.

This preserves a continuous diagnostic line instead of restarting the investigation on every turn.

## Why The Corpora Matter

The incident corpus acts as operational memory, while the theory corpus supplies reusable mechanism-level explanation.

Together, the incident and theory corpora help the system:

- recognize familiar symptom patterns;
- propose realistic first checks;
- preserve plausible competing explanations without improvising from scratch;
- anchor explanations in real failure modes rather than generic distributed-systems language.

At the moment the incident corpus is still small.
That means the current system already demonstrates the reasoning pattern under constrained evidence.
As the incident corpus grows, the assistant should become more useful because more of its guidance can be grounded in direct precedent rather than weaker analogy.

## Proof Strategy

This repository should be read as a proof-backed project, not just a codebase.

Three kinds of project proof matter most:

### 1. Narrative Evidence

Case studies show how the system behaves end-to-end:

- the initial problem;
- the selected primary precedent;
- the competing interpretation;
- the first check;
- the hypothesis updates after new observations.

This is the best way to show the system is conducting an investigation rather than generating polished text.

Current example:

- [CASE_STUDY_AMAZON_RDS_READER_STALE_READS.md](./CASE_STUDY_AMAZON_RDS_READER_STALE_READS.md)

### 2. Quantitative Evidence

Golden-set and eval metrics show how often the system behaves well across runs.

Important dimensions include:

- first-response quality;
- continuation quality;
- hypothesis update discipline;
- next-check usefulness;
- truncation and step-failure rates;
- cost and latency.

The current evaluation framing and outputs are described in:

- [EVALUATION_STORY.md](./EVALUATION_STORY.md)

### 3. Comparative Evidence

Comparisons show what improves the system:

- stronger vs weaker models;
- prompt version changes;
- corpus changes;
- retrieval or context-packing changes.

This is critical because the project’s quality depends not only on prompt wording, but also on model capability and available precedent coverage.

The current observability and comparison surfaces are described in:

- [OBSERVABILITY_STORY.md](./OBSERVABILITY_STORY.md)

## Current Practical Lessons

A few lessons have already become clear from iteration and eval work:

- prompt design matters, but model quality sets a real ceiling;
- stronger models produce better diagnostic discipline, but can increase token burn and truncation risk;
- retrieval quality and precedent coverage are central to usefulness;
- hidden retries can be more dangerous than honest fail-fast behavior when completion budgets are expensive;
- the best system behavior emerges when the assistant maintains a leading explanation, a real competitor, and one check that can separate them.

## How To Read The Documentation

Recommended reading order:

1. [OVERVIEW.md](./OVERVIEW.md):
   product framing and the central diagnostic idea
2. [DIAGNOSTIC_MODEL.md](./DIAGNOSTIC_MODEL.md):
   the reasoning model in more detail
3. [CASE_STUDY_AMAZON_RDS_READER_STALE_READS.md](./CASE_STUDY_AMAZON_RDS_READER_STALE_READS.md):
   a concrete three-iteration investigation walk-through
4. [EVALUATION_STORY.md](./EVALUATION_STORY.md):
   how quality is measured and reported
5. [OBSERVABILITY_STORY.md](./OBSERVABILITY_STORY.md):
   how runtime and evaluation behavior are inspected in traces, reports, and dashboards
6. [ARCHITECTURE.md](./ARCHITECTURE.md):
   how the runtime pipeline implements the reasoning model

Existing detailed prompt and design notes remain useful, but they should be read as supporting documents rather than the main project narrative.

## Project Claim

The main claim of this project is not:

“an LLM can summarize incident reports”.

The main claim is:

**an incident-focused, precedent-guided assistant can conduct a more useful distributed-systems investigation by maintaining a leading explanation, a competing explanation, and one discriminating check that updates as the evidence changes.**
