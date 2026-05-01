# Golden Dataset Authoring

This document defines how to create and update golden cases so that they evaluate the actual functionality of the distributed diagnostics application, rather than an idealized retrieval or reasoning system.

## Goal

A golden case must be compatible with the current runtime world:

- the current controlled vocabulary;
- the current incident and theory collections;
- the current chunk IDs;
- the current candidate-card retrieval behavior;
- the current incident-evidence retrieval filters;
- the current prompt packing rules.

If a target is not reachable under the current pipeline contract, the golden case is invalid for evaluation purposes even if it looks semantically reasonable to a human reviewer.

## Core Principle

Golden cases must describe a reachable and diagnostically useful contract for the current application.

Do not author cases against an ideal answer that bypasses:

- retrieval-stage constraints;
- current candidate-card outputs;
- current chunk filters;
- current collection contents;
- current schema and vocabulary boundaries.

## Authoring Workflow

Use the following workflow for every new or updated case.

### 1. Start from a real primary precedent

Choose a primary incident card that the application should realistically retrieve as the main practical match.

The question should be written to support that match naturally. Avoid relying on hidden expert inference to make the primary precedent look correct after the fact.

### 2. Write the question for the runtime, not for a human benchmark

The question should:

- sound like a plausible user request;
- include a clear symptom;
- include at least one explicit subsystem, constraint, trigger, or observability clue;
- avoid unnecessary ambiguity for easy and medium cases;
- avoid requiring abstract semantic jumps for strict expectations.

### 3. Fill `expected_query_structuring`

Only use vocabulary terms that are present in the controlled vocabulary.

When assigning terms:

- `strict_vocabulary_terms` must be directly supported by the question text;
- `soft_vocabulary_terms` may include nearby concepts that are plausible but not required;
- abstract or inferential concepts should usually be `soft`, not `strict`.

Do not place a term in `strict` only because it is a good expert summary of the case.

### 4. Run the case through the current runtime

Before finalizing retrieval targets, run the case through the application and inspect the trace.

At minimum, verify:

- the actual primary card;
- the actual alternative cards;
- the actual primary incident chunk pool;
- the actual alternative incident chunk pool;
- the actual theory chunk pool.

Golden retrieval targets should be aligned with what the current pipeline is allowed to produce.

### 5. Fill `expected_candidate_cards`

Use one clear expected primary card for easy cases.

When choosing soft card targets:

- include realistic nearby cases;
- keep the neighborhood small and meaningful;
- avoid using a broad semantic cloud that makes failures hard to interpret.

If the primary card is unstable across normal reruns, the case should not be classified as easy.

### 6. Fill `expected_incident_evidence`

For primary evidence:

- every strict chunk must belong to the primary card;
- every strict chunk must pass the current primary chunk-role filter;
- strict chunks should be practically useful for the final answer, not just interesting excerpts.

For alternative evidence:

- every strict chunk must belong to a realistically reachable alternative card;
- every strict chunk must pass the current alternative chunk-role filter;
- do not target chunks from cards that the current candidate-card stage does not actually surface for this case.

If the alternative-card set is unstable, prefer softer expectations or simplify the case.

### 7. Fill `expected_theory_evidence`

Theory targets must be:

- present in the current theory collection;
- reachable under current retrieval behavior;
- useful for mechanism explanation in the final answer.

Do not choose a theory chunk only because it is the best textbook explanation in the abstract.

### 8. Check reachability

Before finalizing a case, verify that:

- every referenced chunk ID exists in the current collections;
- every referenced chunk ID belongs to the current chunking scheme;
- no strict target is filtered out before ranking by runtime retrieval logic;
- no alternative target depends on an unreachable alternative card set.

If a target is unreachable because of runtime filters or candidate selection, the golden case must be updated.

### 9. Evaluate diagnostic usefulness

A good case should help localize failures.

When a metric drops, it should be possible to say whether the issue is in:

- query structuring;
- candidate-card retrieval;
- incident evidence retrieval;
- theory retrieval;
- or downstream generation.

If a case mixes too many hidden assumptions, it becomes hard to diagnose why it failed and should be simplified.

## Difficulty Guidelines

### Easy

An easy case should have:

- one explicit symptom;
- one clear primary precedent;
- minimal ambiguity;
- minimal contrastive reasoning;
- strict targets that are obvious from the question text.

### Medium

A medium case may include:

- one plausible competing interpretation;
- some semantic generalization;
- a small amount of ambiguity;
- still a clear expected primary path.

### Hard

A hard case may include:

- contrastive diagnosis;
- abstract property-level interpretation;
- multiple plausible precedents;
- inference-heavy theory alignment.

Hard cases are appropriate only after easy and medium coverage already exists.

## Review Checklist

Use this checklist before committing any golden case.

### Question

- Is the question natural and concise?
- Does it contain a clear symptom?
- Does it contain explicit evidence for the strict query-structuring targets?

### Query Structuring

- Are all strict terms in the controlled vocabulary?
- Are all strict terms directly supported by the question?
- Are more inferential concepts moved to soft relevance?

### Candidate Cards

- Is the primary card realistic for the current runtime?
- Is the soft neighborhood small and meaningful?

### Incident Evidence

- Do strict primary chunks belong to the primary card?
- Do they pass the primary chunk-role filter?
- Do strict alternative chunks belong to realistically reachable alternative cards?
- Do they pass the alternative chunk-role filter?

### Theory Evidence

- Does the strict theory chunk exist in the current theory corpus?
- Is it realistically retrievable for this question?

### Reachability

- Do all chunk IDs exist?
- Are all chunk IDs aligned with the current chunking version?
- Are any strict targets impossible under current filters or candidate sets?

### Diagnostic Quality

- If this case fails, will the failure be interpretable?
- Does this case test the application contract rather than an idealized expert answer?

## Practical Rule

Before finalizing a case, ask:

“Does this golden case describe what the current application should be able to return, or what I personally think the ideal answer would be?”

Only the first is valid for runtime evaluation.
