# Implementation Foundation: First Response and Evidence Packing

This file captures the main implementation-level findings from the first round of manual experiments for the distributed-systems diagnostic assistant.

It is intended to serve as a practical foundation for future implementation work.

---

## 1. Main conclusion

The core product idea looks viable:

- a distributed-systems assistant can use incident cards plus retrieved evidence chunks to produce a useful first diagnostic response;
- the first response does not need a full multi-turn agentic loop to be useful;
- the same scheme also appears capable of supporting early multi-step diagnosis;
- a compact structured context is enough to produce a strong initial investigative frame.

What already looks workable:

- one matched incident card as structured prior;
- a small evidence pack of practical chunks;
- an optional theory chunk;
- a user-facing JSON response from the model.
- a continued diagnostic update step after user observation;
- one primary card plus competing-card chunks in ambiguous cases.

This is not yet proof that the entire future system will work end-to-end, but it is strong evidence that the first-response slice is practical and worth implementing.
This is not yet proof that the entire future system will work end-to-end, but it is strong evidence that the first-response slice is practical and that the broader diagnostic-loop direction is worth implementing.

---

## 2. What we tested

We manually tested the workflow described in `first_iteration_incident_card_chunk_model_design.md`:

```text
user problem
  -> one matched incident card
  -> extract context / hypotheses / checks
  -> add compact chunk evidence pack
  -> optional theory chunk
  -> send prepared context to the model
  -> inspect first diagnostic response
```

We tested:

- a strong single-card match:
  - `etcd_3_4_3` / distributed lock unsafety
- a weaker, more ambiguous transactional case:
  - `mongodb_4_2_6` with alternatives from `mysql_8_0_34`, `ravendb_6_0_2`, and related cases
- a multi-card evidence-packing scenario:
  - one primary card plus competing chunks from multiple plausible cards
- a manual multi-step loop:
  - first response
  - user observation
  - hypothesis update
  - next check
  - another user observation
  - another hypothesis update

Remote model used in manual runs:

- `openai/gpt-oss-20b`

Additional architectural conclusions were also refined during the same work:

- cards should likely be stored canonically in `Postgres`;
- card candidate ranking should likely be done in `Qdrant`;
- chunk tags are more important for chunk packing than for initial candidate retrieval.

---

## 3. What we learned about the first response

The first response should not be a free-form blob of text.

The best current direction is:

- structured JSON;
- but still written as a user-facing answer in English.

This means:

- the response is easy for the system to parse;
- the same response is almost ready to show directly to the user;
- we avoid a second rendering layer that would have to translate internal labels into readable language.

Current best response shape:

```json
{
  "problem_understanding": "string",
  "similar_practical_context": "string",
  "active_hypotheses": ["string", "string"],
  "first_check": "string",
  "result_interpretation": {
    "supports_primary_if": "string",
    "supports_competing_if": "string",
    "inconclusive_if": "string | null"
  },
  "competing_interpretation": "string | null"
}
```

Important response properties:

- `problem_understanding` should sound like a direct restatement of the user's problem.
- `similar_practical_context` should name the incident family, not jump to a final diagnosis.
- `active_hypotheses` should be short English hypotheses, not internal enum-like labels.
- `first_check` must be exactly one uncertainty-reducing next check.
- `result_interpretation` should explain how the outcome of the first check changes the hypothesis landscape.
- `competing_interpretation` should preserve one compact alternative explanation when the evidence is not unique.

Tone rule:

- preserve uncertainty;
- do not say `root cause confirmed`;
- prefer language like:
  - `strengthens the primary explanation`
  - `keeps the competing interpretation alive`
  - `remains inconclusive`

---

## 4. Why incident cards are useful

The card is the structured prior.

It does not prove the diagnosis.

It gives the model:

- a compact incident frame;
- known symptom pattern;
- plausible failure modes;
- likely discriminating checks;
- incident dynamics over time.

The most important finding here:

- one primary card is enough to anchor the first response.

This was true even in ambiguous cases, as long as we added competing evidence through chunks.

---

## 5. Why chunk tags matter

Chunk tags turned out to be important.

They are not just metadata decoration.

They help us reduce the evidence pack without turning it into a random top-N list.

Without tags, the fallback would be something like:

- take top-N chunks by retrieval score;
- or do an extra model call to classify chunk roles.

Both options are weaker:

- top-N often over-selects repeated chunks with the same function;
- extra model calls add cost and complexity.

Tags make chunk selection role-aware.

Current practical use of tags:

- `chunk_role:symptom`
  - good for `evidence_for_match`
- `chunk_role:diagnostic_step`
  - good for `first_check_hint`
- `chunk_role:investigation`
  - helps choose discriminating questions or investigative framing
- `chunk_role:failure_mode`
  - useful for competing interpretations and mechanism framing
- `chunk_role:lesson`
  - useful for concise practical explanation
- `chunk_role:uncertainty`
  - useful when we want to preserve non-final reasoning
- `chunk_role:symptom_change`
  - useful when incident dynamics matter
- `chunk_role:timeline`
  - useful when sequencing of events matters

The key idea:

- tags help answer not only `is this chunk relevant?`
- they help answer `what role should this chunk play in the prompt?`

That is what made a small chunk budget realistic.

---

## 6. What we learned about theory chunks

Theory chunks are useful, but not always necessary.

Their best role is:

- `mechanism_explanation`

They help most when:

- the practical precedent is too vendor-specific;
- several practical cards point to the same deeper mechanism;
- we want the model to generalize beyond one incident report;
- we want the competing interpretation to sound principled rather than arbitrary.

They help less when:

- the practical precedent is already very clean;
- the theory chunk is too generic;
- the theory chunk does not add a mechanism, only general background.

Current conclusion:

- theory chunks are optional in the first response;
- they are leverage, not the core;
- the first response can work without them;
- they are especially helpful when there are multiple plausible practical precedents.

---

## 7. Hard chunk budget: does it work?

This was one of the main questions of the experiments.

Current answer:

- yes, a hard small chunk budget looks viable for the first response.

But the important nuance is:

- it works not because small top-N retrieval is magically enough;
- it works because the context is packed deliberately.

What seems to work:

- one primary card;
- a tiny role-balanced evidence pack;
- explicit preservation of competing interpretation in the prompt and output schema.

This means:

- chunk count does not need to grow linearly with the number of plausible cards in the first response;
- several plausible cards can still be represented in a small pack.

This is a strong result for implementation, because it suggests the first-response system can stay compact even as the corpus grows.

---

## 8. Best current packing rule

For the first response:

```text
1 primary card
+ 2 primary-card chunks
+ 1 chunk from each of up to 2 competing cards
+ 0-1 theory chunk
```

Recommended meaning of those chunks:

- primary chunk 1:
  - `evidence_for_match`
- primary chunk 2:
  - `first_check_hint`
- competing chunks:
  - `alternative_context`
- theory chunk:
  - `mechanism_explanation`

Why this works:

- one stable main frame;
- small context;
- competing interpretations remain visible;
- the answer does not collapse into a list of report summaries.

Important negative conclusion:

- do not send 2 or 3 full cards to the model in the first response.

That would likely:

- inflate context size;
- blur the main frame;
- increase the chance of generic answers.

Better pattern:

- one full primary card;
- competing cards represented through carefully chosen chunks.

---

## 9. Competing interpretation was essential

Prompt-only nudging helped preserve alternative context somewhat.

But the stronger and cleaner solution was:

- add an explicit `competing_interpretation` field to the response.

This did two useful things:

- forced the model to keep one explicit alternative alive;
- made that alternative machine-readable for downstream orchestration.

This was better than trying to infer alternative context from the hypothesis list alone.

Related improvement:

- structured `result_interpretation`

Instead of one interpretation paragraph, the best current variant is:

```json
"result_interpretation": {
  "supports_primary_if": "...",
  "supports_competing_if": "...",
  "inconclusive_if": "..."
}
```

This makes the next diagnostic step easier because the system can map user observations directly to:

- evidence strengthening the primary explanation;
- evidence keeping the competing interpretation alive;
- unresolved ambiguity.

---

## 10. User-facing JSON is the right target

One especially important discovery:

- the first response JSON should be both machine-readable and user-readable.

This means we should avoid outputs like:

- `weak_default_read_write_concerns`
- `transaction_retry_bug`

inside the final user-facing response fields.

Instead, the model should say things like:

- `The application is relying on defaults that do not apply inside transactions.`
- `Retry logic may be misinterpreting ambiguous transaction errors.`
- `The isolation semantics may be weaker than the application assumes.`

This keeps the response:

- easier to display;
- easier to read;
- still structured enough for orchestration.

The same principle extends beyond the first response:

- later update-step JSON should also remain user-facing;
- only the structure should be machine-oriented;
- the values should still read like a natural diagnostic response.

---

## 11. Card retrieval architecture

The current preferred card-retrieval split is:

- `Qdrant` for card candidate retrieval and ranking
- `Postgres` for canonical incident-card storage

Why this split looks right:

- Qdrant naturally returns ordered candidates with scores.
- Those scores are useful for choosing:
  - one `primary` card
  - one or two `competing` cards
- Postgres is a better home for the full structured card because cards are reasoning objects, not only retrieval documents.

This creates a useful role split:

- `Qdrant` finds the candidates
- `Postgres` holds the truth

Practical implication:

- do not rely on Postgres alone to rank primary vs secondary card candidates;
- use Qdrant for ranked candidate selection;
- fetch full structured cards from Postgres by `case_id`.

Related operational advantage:

- retrieval representation and embeddings in Qdrant become disposable and reindexable;
- canonical card content remains stable in Postgres.

This makes reindexing much easier:

- change embedding model
- change retrieval serialization
- rebuild card collection

without moving the card body itself.

---

## 12. Multi-step loop viability

We manually tested a continuation of the transactional case beyond the first response.

The tested branch looked like this:

```text
first response
  -> first check about transaction-level concerns
  -> user observation confirms concerns were not set explicitly
  -> hypothesis update
  -> next check
  -> user observation shows strong concerns remove most missing writes
  -> hypothesis update
  -> next check shifts toward retry-path behavior
```

Main result:

- the scheme does not only work for first response;
- it also appears capable of supporting a real diagnostic progression.

What worked:

- the model updated hypotheses instead of repeating the first frame;
- it kept giving one next check;
- it did not jump immediately to a final diagnosis;
- after the primary explanation was partially supported, it pivoted toward the next live hypothesis.

What was weaker:

- one update step was somewhat conservative and did not advance as aggressively as ideal;
- prompt design for update steps still needs refinement;
- plain JSON constraints need to be enforced more tightly in update prompts.

Current takeaway:

- multi-step diagnosis now looks `promising`, not just the first response;
- the architectural direction appears viable beyond a single turn.

---

## 13. What currently looks implementation-ready

These parts now look mature enough to guide implementation:

- store structured incident cards;
- store canonical cards in `Postgres`;
- store card retrieval representation in `Qdrant`;
- use `Qdrant` to pick ranked card candidates;
- retrieve a small role-balanced practical chunk pack;
- optionally add one theory chunk;
- use one primary card in the prompt;
- represent competing cards through `alternative_context` chunks;
- request a strict user-facing JSON answer;
- include:
  - `competing_interpretation`
  - structured `result_interpretation`
- for later steps, use:
  - structured hypothesis updates
  - one next check
  - structured interpretation of that next check

This is enough to build a strong first-response slice and begin experimenting with early multi-step diagnosis.

---

## 14. What is not yet proven

Important caution:

these experiments were strong enough to validate the direction, but not enough to prove all future assumptions.

Still open:

- whether the same chunk budget will remain strong across a much larger test set;
- how robust the system is when retrieval quality is weaker;
- how often competing cards will be harder to separate than in the current runs;
- how often multi-step updates will need more card context or more chunks;
- whether later diagnostic-loop stages will require richer memory than the current compact prompt;
- how stable the update-step prompts will remain across many different incident families.

So the current status is:

- viable for first-response implementation;
- promising for early multi-step diagnosis;
- not yet fully proven for the whole future assistant.

---

## 15. Current implementation takeaway

The first implementation should not try to solve everything.

It should implement the strongest currently supported slice:

```text
user problem
  -> normalize
  -> retrieve ranked card candidates
  -> choose 1 primary card
  -> choose 1-2 competing cards if needed
  -> fetch canonical cards from Postgres
  -> retrieve tiny role-balanced evidence pack
  -> optionally add 1 theory chunk
  -> ask model for user-facing JSON
  -> preserve 1 competing interpretation
  -> provide 1 first discriminating check
```

Then extend that into:

```text
user observation
  -> hypothesis update
  -> one next check
  -> structured interpretation
```

This combined slice now looks:

- useful;
- realistic;
- operationally coherent;
- worth building as the first serious implementation target.
