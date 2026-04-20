# Query Structuring Prompt Evolution

## Why This Document Exists

The current `query_structuring` prompt did not appear in one step.
It was shaped through a series of manual experiments where we compared prompt
variants, looked at the returned JSON, checked token usage and latency, and
tried to understand which prompt instructions improved the structure and which
ones caused new distortions.

This document records the human-facing story of that process:

- what we were trying to get from the model;
- what kinds of prompt changes we tested;
- what consistently worked;
- what looked promising but had side effects;
- why `v2` became the current frozen baseline.

This is not a specification document.
It is a decision record for prompt quality work.

## What This Prompt Is For

This prompt belongs to the `query_structuring` step in the runtime request
pipeline.

That step takes a normalized user query and asks the model to convert it into a
structured representation that is easier for downstream runtime steps to use.

In simple terms, the step exists because the raw user query is too free-form.
Before retrieval and later reasoning can work well, we want a more explicit
object that says things such as:

- what symptoms are being described;
- which subsystems seem implicated;
- which failure modes are plausible hypotheses;
- which system properties may be at stake;
- which other entities, triggers, and observability signals are mentioned.

The prompt is therefore not a general answer-generation prompt.
It is a transformation prompt:

- input: normalized user query plus controlled vocabulary;
- output: strict JSON structure for the next pipeline steps.

An important design point is that the output is intentionally mixed-source.
We do not expect every field to come from the same place.

Some fields are vocabulary-backed:

- `symptoms`
- `affected_subsystems`
- `failure_modes`
- `system_properties`

For those fields, the model is expected to choose terms from the controlled
vocabulary when the query supports them.

Other fields are more directly query-derived:

- `intent`
- `scenario`
- `entities`
- `constraints`
- `triggers`
- `observability_signals`
- `unresolved_terms`
- `rejected_nearby_terms`
- overall `confidence`

Those fields are not simply copied from the vocabulary.
They are inferred or composed from the user query itself, while still being
constrained by the prompt instructions.

This split was one of the key ideas behind the prompt design:

- the vocabulary gives the model controlled terminology where normalization is
  useful;
- the query still provides the concrete situation, wording, and local context
  that the vocabulary alone does not contain.

## What Goes Into The Prompt

The prompt is assembled from two main inputs:

- the normalized user query;
- a controlled vocabulary built outside this runtime step.

The controlled vocabulary is important because we do not want the model to
invent arbitrary domain labels every time.
We want it to choose from a known terminology set when the query supports that
choice.

The real vocabulary used in testing was larger, but the shape looked like this:

```json
{
  "canonical_symptoms": [
    "duplicate_lock_holders",
    "lost_updates",
    "high_latency"
  ],
  "affected_components": [
    "lock_service",
    "key_value_api",
    "api_gateway"
  ],
  "failure_mode_candidates": [
    "lock_ownership_violation",
    "split_brain",
    "overload"
  ],
  "violated_properties": [
    "safe mutual exclusion for distributed locks",
    "availability",
    "linearizability"
  ]
}
```

This vocabulary was deliberately treated as guidance, not as a bag of labels the
model must blindly stuff into the output.

## What We Expected Back

The target was a strict JSON object with a stable shape.

Again, the real output in the experiments could be longer, but the expected
shape was roughly:

```json
{
  "intent": "string",
  "scenario": "string",
  "symptoms": [
    {
      "term": "string",
      "evidence_span": "string",
      "support_level": "explicit | strong_paraphrase | weak_inference"
    }
  ],
  "affected_subsystems": [
    {
      "term": "string",
      "evidence_span": "string",
      "support_level": "explicit | strong_paraphrase | weak_inference"
    }
  ],
  "failure_modes": [
    {
      "term": "string",
      "evidence_span": "string",
      "support_level": "explicit | strong_paraphrase | weak_inference"
    }
  ],
  "system_properties": [
    {
      "term": "string",
      "evidence_span": "string",
      "support_level": "explicit | strong_paraphrase | weak_inference"
    }
  ],
  "entities": ["string"],
  "constraints": ["string"],
  "triggers": ["string"],
  "observability_signals": ["string"],
  "unresolved_terms": ["string"],
  "rejected_nearby_terms": [
    {
      "term": "string",
      "reason": "string"
    }
  ],
  "confidence": "low | medium | high"
}
```

That target shape is why prompt wording mattered so much.
The model was not just filling slots.
It had to understand the field semantics well enough not to mix them up.

## What We Wanted From The Prompt

The goal of the prompt was not merely "return valid JSON".

We wanted the model to do a harder thing:

- extract a useful structured interpretation of the user query;
- use controlled terminology where the query supports it;
- keep observations separate from hypotheses;
- avoid inventing unsupported failure modes;
- leave enough evidence in the output to understand why a term was selected.

In practice, that meant we cared about several quality dimensions at once:

- JSON validity;
- field discipline;
- vocabulary discipline;
- low hallucination rate;
- useful evidence traces;
- reasonable completeness.

The difficult part was that improving one field often degraded another.

## Experimental Setup

The experiments were done manually against a live remote model.

The main working setup was:

- model: `openai/gpt-oss-20b`
- response format: JSON object
- temperature: `0.0`
- controlled vocabulary supplied together with the query

We also briefly compared against `openai/gpt-oss-120b`, but it was much slower
and did not justify itself for this step.
After that comparison, we stopped using it for prompt tuning here.

During the experiments we looked at:

- the raw JSON shape;
- the chosen terms by field;
- whether the model overreached on `failure_modes`;
- token usage;
- latency;
- behavior on both close-match queries and less directly matching queries.

## Compact Example: Prompt Inputs

One simplified example looked like this.

User query:

```text
During a network flap two workers behaved as if they both held the same
distributed lock and we saw conflicting writes.
```

Compact vocabulary snippet:

```json
{
  "canonical_symptoms": [
    "duplicate_lock_holders",
    "lost_updates"
  ],
  "affected_components": [
    "lock_service",
    "key_value_api"
  ],
  "failure_mode_candidates": [
    "lock_ownership_violation",
    "split_brain"
  ],
  "violated_properties": [
    "safe mutual exclusion for distributed locks"
  ]
}
```

The assembled user-facing prompt shape was:

```text
Query:
During a network flap two workers behaved as if they both held the same
distributed lock and we saw conflicting writes.

Controlled vocabulary:
{"canonical_symptoms":["duplicate_lock_holders","lost_updates"],"affected_components":["lock_service","key_value_api"],"failure_mode_candidates":["lock_ownership_violation","split_brain"],"violated_properties":["safe mutual exclusion for distributed locks"]}
```

And the returned JSON we wanted was the same kind of object as:

```json
{
  "intent": "diagnose incident cause around distributed locking behavior",
  "scenario": "Two workers appear to hold the same distributed lock during network instability.",
  "symptoms": [
    {
      "term": "duplicate_lock_holders",
      "evidence_span": "both held the same distributed lock",
      "support_level": "strong_paraphrase"
    },
    {
      "term": "lost_updates",
      "evidence_span": "conflicting writes",
      "support_level": "strong_paraphrase"
    }
  ],
  "affected_subsystems": [
    {
      "term": "lock_service",
      "evidence_span": "distributed lock",
      "support_level": "explicit"
    }
  ],
  "failure_modes": [
    {
      "term": "lock_ownership_violation",
      "evidence_span": "both held the same distributed lock",
      "support_level": "strong_paraphrase"
    }
  ],
  "system_properties": [
    {
      "term": "safe mutual exclusion for distributed locks",
      "evidence_span": "both held the same distributed lock",
      "support_level": "strong_paraphrase"
    }
  ],
  "entities": ["worker_a", "worker_b", "distributed_lock"],
  "constraints": [],
  "triggers": ["network flap"],
  "observability_signals": ["conflicting writes"],
  "unresolved_terms": [],
  "rejected_nearby_terms": [
    {
      "term": "split_brain",
      "reason": "not directly supported by the query"
    }
  ],
  "confidence": "medium"
}
```

This kind of example is important because it shows the real challenge:

- the vocabulary gives the model options;
- the query grounds the choice;
- the prompt must keep the model from selecting every plausible term.

## Compact Example: Why Prompt Quality Mattered

A weaker prompt could still produce valid JSON and still be wrong in practice.

For example, a weaker prompt might produce:

```json
{
  "symptoms": [
    {
      "term": "duplicate_lock_holders",
      "evidence_span": "both held the same distributed lock",
      "support_level": "strong_paraphrase"
    }
  ],
  "failure_modes": [
    {
      "term": "lock_ownership_violation",
      "evidence_span": "both held the same distributed lock",
      "support_level": "strong_paraphrase"
    },
    {
      "term": "split_brain",
      "evidence_span": "network flap",
      "support_level": "weak_inference"
    }
  ],
  "confidence": "high"
}
```

This looks superficially plausible, but it is worse:

- it over-selects `failure_modes`;
- it turns a nearby idea into a selected diagnosis;
- it becomes overconfident.

Much of the tuning work was about reducing exactly this kind of output.

## Main Prompt Directions We Tried

We did not just tweak wording randomly.
The experiments followed a few clear directions.

### 1. Make The Model Output Structured JSON

The first requirement was the most basic one:
the model needed to reliably return a JSON object with the required fields.

That part was necessary, but not sufficient.
A prompt can produce syntactically valid JSON and still be semantically poor if:

- symptoms and hypotheses get mixed together;
- unsupported vocabulary terms are selected;
- fields become noisy or redundant.

### 2. Split Observations From Hypotheses

One of the most useful prompt directions was making field semantics explicit.

In particular, it helped to state clearly that:

- `symptoms` are observed effects or directly described anomalous behavior;
- `affected_subsystems` are components implicated by the query;
- `failure_modes` are hypotheses about what may be wrong;
- `system_properties` are safety or consistency properties at stake.

This field-by-field interpretation guidance was one of the main reasons the
prompt started producing meaningfully better structures instead of just valid
JSON blobs.

### 3. Force Vocabulary Discipline

Another important direction was telling the model to select controlled-vocabulary
terms only when the query actually supports them.

This was paired with:

- explicit evidence spans;
- support-level labeling;
- a preference for omission over weak unsupported inference.

That combination helped reduce the tendency to "complete the ontology" just
because a term looked plausible in context.

### 4. Surface Rejected Nearby Terms

Adding `rejected_nearby_terms` was useful.

It created a place for the model to say:

- "this term is nearby,"
- "I considered it,"
- "but I am not selecting it because the query does not support it strongly enough."

This made the output easier to inspect and made hallucination behavior more
visible during manual evaluation.

## The Hardest Field: `failure_modes`

`failure_modes` was the most difficult part of the prompt.

This field is inherently tempting for the model:
once it sees symptoms and subsystem hints, it often wants to jump to a plausible
diagnosis even when the query itself does not fully justify that move.

We tested narrower instructions specifically for `failure_modes`.
The most successful one was a conservative rule:

- return at most one item;
- choose only the most directly supported hypothesis;
- otherwise return an empty list.

This improved `failure_modes` quality in several cases.

But we also saw an important tradeoff:

- stronger pressure on `failure_modes` could improve that one field;
- the same change could reduce quality or naturalness in other fields.

That was a key lesson from the tuning work:
prompt quality here is a balance problem, not a single-metric optimization.

## Why `v2` Became The Baseline

The current frozen baseline is `v2`.

It won not because it was perfect, but because it gave the best overall balance
across the fields we cared about.

What `v2` kept from earlier iterations:

- explicit interpretation rules by field;
- controlled-vocabulary selection discipline;
- evidence spans and support levels;
- preference for omission over weak unsupported inference;
- explicit JSON schema guidance.

What `v2` added in a targeted way:

- a narrow conservative rule specifically for `failure_modes`.

Why that mattered:

- it reduced noisy hypothesis selection;
- it helped move unsupported candidates out of selected output;
- it did this without destabilizing the whole prompt as much as stronger
  rewrites did.

In short, `v2` was the first version that felt robust enough across the full
structure, not just strong on one field.

## What We Learned About Prompt Tuning Here

Several practical lessons became clear.

### JSON Validity Is Not The Real Finish Line

Valid JSON was necessary, but it was never the true success criterion.

The useful prompt was the one that produced:

- disciplined fields;
- controlled vocabulary usage;
- low hallucination pressure;
- interpretable evidence.

### Small Local Rules Worked Better Than Big Rewrites

The most useful improvements were usually local and explicit.

For this task, broad rewrites were riskier.
They often moved the prompt away from the working balance we already had.

In contrast, narrow changes like the `failure_modes` cap were easier to reason
about and easier to evaluate.

### Over-Optimizing One Field Can Hurt The Whole Structure

This happened most clearly around `failure_modes`.

A change could look like an improvement if you inspected only that field.
But once you reviewed the whole object, it could be a regression.

That is why the final decision was made on overall output quality, not on a
single-field win.

### Prompt Quality Matters More Than Cheapness At This Stage

During this round we explicitly prioritized prompt quality over token cost.

We did record token usage and latency, and those numbers do matter.
But the immediate goal was to find a prompt that reliably produces the right
kind of structure.

Cost reduction is a separate optimization pass.

## Scope Decision We Made For MVP

The experiments also clarified an important product boundary.

The prompt works best when the user query is in the incident-investigation mode.

Once queries become mixed, such as:

- partly conceptual,
- partly explanatory,
- partly diagnostic,

the structure starts to feel forced.

That led to a clear MVP decision:

- for now, this prompt is only for incident-oriented queries;
- mixed-mode and conceptual-mode handling should come later;
- a future classifier will likely be needed before this step.

This was not only a product decision.
It was also a prompt-quality decision.
Restricting the prompt to the mode it handles well makes the current system more
honest and more reliable.

## The Current Working Position

The current position is:

- `v2` is the frozen baseline;
- it is good enough to build the first implementation around;
- it should be treated as the best current prompt, not as the final perfect one;
- later work can continue from `v2`, but comparisons should be made against it,
  not from scratch.

## What Is Still Weak

Even with `v2`, some weaknesses remained visible during the experiments:

- `constraints` can still come back thinner than we would like;
- `observability_signals` are still more query-derived than normalized;
- confidence can skew too high;
- `failure_modes` remains the most fragile field when the query is ambiguous.

These are known limitations, not hidden surprises.

## Final Decision From This Round

The outcome of this prompt-tuning round was:

- freeze `v2` as the current baseline;
- stop trying to perfect the prompt before implementation starts;
- build the module around this baseline;
- keep later prompt tuning incremental and evidence-driven.

That is the right tradeoff for this stage.
The experiments were strong enough to justify the direction, and the remaining
issues are now clear enough to improve later in a controlled way.
