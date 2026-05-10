# Diagnostic Model

## Purpose

This document defines the reasoning model used by the assistant during first-response diagnosis and continuation updates.

The system does not treat diagnosis as a one-shot answer-generation task.
It maintains a compact diagnostic state and updates that state as new observations arrive.

![Iteration state in the UI](./images/Screenshot%202026-05-10%20163941.png)

This screen shows how the reasoning model appears in the application:

- the current top hypothesis;
- per-iteration observations;
- signal quality;
- hypothesis state changes;
- a compact competing interpretation;
- the next discriminating check.

## Core Entities

### Problem Understanding

`problem_understanding` is the current technical framing of the user's problem.

It should:

- describe the observed symptom clearly;
- remain faithful to what the user reported;
- incorporate diagnostically important conditions or constraints;
- become more specific when later observations materially refine the picture.

It should not:

- jump to a final diagnosis;
- silently invert the meaning of the user's report;
- discard important previously established context without reason.

### Retrieval Context

The reasoning state is built on top of retrieved context, not directly on raw free-form generation.

That context usually contains:

- the best matching incident precedent;
- explicit alternative incident context;
- theory evidence.

These are context sources, not final displayed explanations.
They create the evidence pressure from which the current diagnostic state is synthesized.
Alternative incident context is especially useful because it keeps a competing reading alive even when the visible hypotheses are still mostly rooted in the same primary incident family.

### Context Budget And Evidence Packing

The assistant does not pass every retrieved chunk into the final prompt.
It builds a deliberately limited prompt context from a larger retrieval result.

For the first response, that compact context typically contains:

- the primary incident card as a structured precedent;
- a small role-packed set of primary incident chunks;
- selected alternative chunks plus hydrated alternative-card metadata;
- a small mechanism-oriented theory slice.

For continuation updates, the compact context also carries the existing diagnostic state and the structured form of the new observation.

This packing step matters because prompt usefulness depends on relevance density, not on raw chunk volume.
The system tries to keep only the context that can explain the match, shape the next check, preserve ambiguity, or supply mechanism-level framing.

More detailed packing rules live in [PROMPT_CONTEXT_ASSEMBLY.md](/home/olesia/code/dist_sys_assistant/Documentation/PROMPT_CONTEXT_ASSEMBLY.md).

### Chunk Roles And Tag-Guided Selection

Retrieved chunks are not inserted into the prompt as an undifferentiated list.
They are assigned prompt-facing roles.

For incident evidence, the main roles are:

- `evidence_for_match`
- `first_check_hint` or `next_check_hint`
- `supporting_explanation`
- `alternative_context`

For theory evidence, the main prompt-facing role is:

- `mechanism_explanation`

Role selection is tag-guided before it is score-guided.
In practice this means the system first prefers chunks whose tags match the intent of the role, and only then uses retrieval score and retrieval order to break ties.

This is why a chunk tagged like `diagnostic_step` can beat a higher-scoring symptom chunk for the check-hint role: the prompt needs the right kind of evidence, not just the globally highest-scoring text.

Alternative-context packing is also constrained on purpose.
The system spreads that role across alternative cases instead of filling the whole budget from one case, which keeps the competing context broader and less repetitive.

### Why `fixed_in_structural` Chunks Matter

Earlier versions relied on the structural chunker output directly.
In practice those chunks often made prompt requests too large and too uneven in size for reliable prompt packing.

The current runtime therefore uses `fixed_in_structural` corpora for incident and theory retrieval.
The goal is not to remove structure, but to preserve structural locality while keeping chunk size smaller and more predictable.

That change made it easier to:

- fit richer evidence mixes into the prompt budget;
- keep role-based packing stable across runs;
- reduce the chance that one oversized chunk crowds out the rest of the diagnostic context.

### Leading Explanation

The leading explanation is the strongest current explanation of the problem.
It is usually grounded in the best matching incident precedent and the supporting incident evidence associated with that precedent.

The leading explanation is not “the truth”.
It is the current front-runner in the investigation.

### Competing Explanation

The competing explanation is the strongest live rival to the leading explanation.

It may be grounded in:

- explicit alternative incident context;
- a narrower competing reading of the primary incident evidence;
- a theory-backed mechanism that stays live under the current evidence.

Its role is to preserve uncertainty in a disciplined way.
It exists so the system can propose checks that separate plausible explanations instead of collapsing too early to a single story.

A competing explanation is useful when it changes:

- the ranking of explanations;
- the interpretation of the next check;
- the next diagnostic move.

A competing explanation is not useful when it is merely:

- a wording variant of the leading explanation;
- a distant failure mechanism introduced for contrast only;
- generic distributed-systems background with no effect on the next step.

### Theory Layer

Theory evidence is a separate mechanism-level layer retrieved alongside incident evidence.

It helps the system:

- name the underlying mechanism more clearly;
- connect a practical incident pattern to a more general explanation;
- add explanatory depth without replacing precedent-guided diagnosis.

Theory evidence can strengthen explanation quality, but it should not displace stronger incident-grounded lines or define the diagnostic line by itself without good reason.

### Hypothesis

A hypothesis is the user-facing representation of a live diagnostic line.

Each hypothesis should be:

- compact;
- mechanistically meaningful;
- revisable;
- useful for deciding the next check.

The system usually works best with two live hypotheses:

- the leading hypothesis;
- the strongest competing hypothesis.

A third hypothesis is justified only when it materially changes:

- ranking;
- interpretation;
- or the next check.

The source of a hypothesis may be `primary_incident`, `alternative_context`, or `theory_mechanism`.
That provenance matters for transparency, but not every competing hypothesis needs to come from `alternative_context`.

### Competing Interpretation

`competing_interpretation` is the compact summary of the strongest live alternative to the leading explanation.

Its purpose is not to add prose variety.
Its purpose is to keep the user oriented around the most important rival explanation still worth testing.

It is often shaped by alternative incident context, but it is not restricted to that source.
It may summarize a rival reading that is still largely assembled from the same primary incident family as the leading hypothesis.

### Discriminating Check

The discriminating check is the most important operational output of the assistant.

It should:

- be exactly one next check;
- be concrete and operational;
- help test whether the live hypotheses remain viable;
- reduce uncertainty rather than merely restate the symptom.

A good check produces information that changes what the assistant should believe next.
The paired interpretations such as `supports_primary_if` and `supports_competing_if` should therefore be read as outcomes for the current leading and competing explanations, not as proof that each hypothesis came from a different retrieved source.

## State Shape

The diagnostic state is small on purpose.

At a minimum it contains:

- the current `problem_understanding`;
- the current live hypotheses;
- the strongest competing interpretation;
- the latest suggested check.

These entities play different roles:

- `problem_understanding` frames the symptom;
- `hypotheses` track candidate mechanisms;
- `competing_interpretation` summarizes the main rival story for the user;
- `result_interpretation` explains how to read the next check.

This state must persist across iterations.
The continuation loop should refine it rather than recreate it from scratch.

## First Response Behavior

In the first response, the system should:

1. structure the user query;
2. assess signal quality for a first diagnostic response using the structured query;
3. retrieve:
   - a primary incident precedent;
   - explicit alternative incident context;
   - theory evidence;
4. assemble the evidence into diagnostically meaningful roles;
5. produce:
   - `problem_understanding`
   - 2 or 3 hypotheses
   - one discriminating `first_check`
   - `result_interpretation`
   - `competing_interpretation`

The first response should establish a useful investigation path, not a final answer.

## Continuation Behavior

When a new user message arrives after the first response, the system should not immediately assume it is usable evidence.

It should first:

1. resolve the new message against the current context so short follow-ups become standalone factual updates;
2. determine whether the message is actually an observation rather than:
   - a new question;
   - an ambiguous clarification;
   - unsupported or non-observational follow-up;
3. extract a compact set of atomic observations;
4. decide whether signal quality is sufficient to continue or whether clarification is needed.

Only then should it update the diagnostic state.

## Continuity Rules

The continuation loop must preserve diagnostic continuity.

New observations should be interpreted against:

- the previous understanding of the problem;
- the existing live hypotheses;
- the previous check;
- the newly resolved observation.

The assistant should not behave as if every turn starts a new investigation.

Continuity means:

- preserving hypothesis identity when the underlying mechanism is still the same;
- preserving the existing diagnostic line unless the new observation clearly requires a re-ranking or replacement;
- refining wording without losing the state transition;
- keeping previously established useful context unless the new evidence contradicts it;
- selecting a next check that responds to the updated state, not just to the newest sentence.

## Hypothesis Update Discipline

When a new observation arrives, the system should update hypotheses with discipline.

### Strengthen

Strengthen a hypothesis when the new observation:

- directly supports its mechanism;
- removes an important rival explanation;
- confirms a condition that the hypothesis depends on.

### Weaken

Weaken a hypothesis when the new observation:

- contradicts one of its expected conditions;
- favors a stronger competing explanation;
- shows that the proposed mechanism does not explain the observed behavior well.

### Reject

Reject a hypothesis when the new observation clearly contradicts it and leaves little room for it to remain live.

Rejection should be explicit, not hidden behind a cosmetic rewrite.

### Preserve

Preserve a hypothesis when the new observation does not materially change its plausibility.

The system should not force change for the sake of visible motion.
But it also should not keep all hypotheses at equal apparent strength when the new evidence clearly favors one of them.

### Introduce a New Hypothesis

Introduce a genuinely new hypothesis only when the new observation creates a real new line of reasoning.

Do not add a new hypothesis when:

- an existing hypothesis can be refined instead;
- the new line would not change the next check;
- the new line is only decorative context.

## Check Discipline

The next check should be chosen from the current state, not from generic troubleshooting instincts.

### A Good Check

A good check:

- directly targets the live decision boundary between hypotheses;
- uses an observable runtime signal, state, config, metric, trace, or DB view;
- changes confidence in at least one live hypothesis;
- moves the investigation forward.

### A Weak Check

A weak check:

- only repeats the symptom;
- is too broad to distinguish the live lines;
- requires many sub-checks;
- does not change what the system would believe next.

### A Non-Discriminating Check

A check is non-discriminating when the top live hypotheses would predict the same result.

When that happens, the assistant should choose a different check if possible.

## Signal Quality

The system should not treat every input as diagnostically usable.

Signal quality matters in two places:

### Initial Request

The assistant should determine whether the user input contains enough structured signal, after query structuring, to support a first diagnostic response.

### Continuation Input

The assistant should determine whether the new user turn contains enough observational content, after observation resolution and extraction, to justify a state update.

If signal quality is too low, the system should prefer asking for clarification over inventing a confident update.

## Common Failure Modes

The diagnostic model is most at risk when it drifts into one of these patterns:

- collapsing too early to one explanation;
- keeping decorative alternatives alive even when they do not affect the next move;
- adding a third hypothesis that changes nothing;
- replacing precedent-guided reasoning with generic mechanism talk;
- proposing a check that does not actually separate live hypotheses;
- rewriting the answer cosmetically instead of updating the state;
- treating a question or vague follow-up as if it were a concrete observation;
- losing continuity between iterations.

## What Good Behavior Looks Like

A strong diagnostic response should show:

- a faithful problem framing;
- a leading line and a meaningful competitor;
- a compact hypothesis set;
- one useful discriminating check;
- interpretation that tells the user what the result would mean.

A strong continuation response should show:

- preserved continuity from the previous state;
- disciplined strengthening, weakening, or rejection of hypotheses;
- a next check that reflects the updated state;
- less uncertainty than before, or a clear explanation of why uncertainty remains.
