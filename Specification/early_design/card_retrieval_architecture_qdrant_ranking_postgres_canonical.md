# Card Retrieval Architecture: Qdrant Ranking, Postgres Canonical Cards

This note captures the current preferred architecture for incident-card retrieval in the distributed-systems diagnostic assistant.

---

## Core decision

Use:

- `Qdrant` for card candidate retrieval and ranking
- `Postgres` for canonical incident-card storage

Short version:

```text
Qdrant chooses the candidates.
Postgres holds the canonical cards.
```

---

## Why this split makes sense

### Why Qdrant should rank cards

For first-response generation, we do not only need:

- one matching card

We usually need:

- one `primary` card
- one or two `competing` cards

This naturally depends on ranking and score.

Qdrant is well-suited for this because it returns:

- an ordered candidate list
- similarity scores
- a natural way to distinguish:
  - strongest match
  - secondary match
  - weak fallback

This is useful for:

- choosing the primary card
- deciding whether a competing interpretation is necessary
- deciding how many competing cards to keep

### Why Postgres should remain the source of truth

Incident cards are structured reasoning objects, not just retrieval documents.

They contain fields like:

- symptoms
- phases
- affected components
- diagnostic patterns
- discriminating checks
- expected observations
- reasoning summaries

This makes Postgres a better home for:

- canonical storage
- structured updates
- reliable reads by `case_id`
- future joins, filtering, and operational workflows

Postgres should hold the full card.
Qdrant should hold a retrieval-oriented representation of the card.

---

## Recommended card retrieval pipeline

```text
1. User submits problem description
2. Optional normalization layer produces structured hints
3. Qdrant retrieves top card candidates with scores
4. System selects:
   - 1 primary card
   - 1-2 competing cards above threshold
5. System fetches full cards from Postgres by case_id
6. System builds chunk pack:
   - primary-card chunks
   - competing-card chunks
   - optional theory chunk
7. System generates first response
```

---

## What should live in Qdrant

Qdrant should not try to replace the full structured card.

Instead, it should store a retrieval index representation, for example:

- `case_id`
- title
- short summary
- canonical symptoms
- candidate explanations
- diagnostic patterns
- compact phase summary
- embedding
- optional lightweight metadata for filtering

This representation should be optimized for candidate retrieval, not for full downstream reasoning.

---

## What should live in Postgres

Postgres should store the complete canonical incident card.

This includes all structured fields needed for:

- prompt construction
- explanation generation
- later diagnostic loop state transitions
- future analytics and maintenance

Postgres should be treated as the source of truth for card content.

---

## How to use Qdrant scores

The score gap between top card candidates is useful.

Examples:

- if `top1` is much stronger than `top2`:
  - the first response can stay more focused
  - `competing_interpretation` may be optional or weak

- if `top1` and `top2` are close:
  - the first response should preserve alternative context explicitly
  - at least one competing card should contribute an `alternative_context` chunk

- if several scores are clustered:
  - the system should avoid premature narrowing
  - one primary card still anchors the response, but competing evidence should be included

This makes Qdrant useful not only for retrieval, but also for uncertainty management.

---

## Why not rely on Postgres alone for card ranking

Postgres can store the cards well.

But it is not naturally good at expressing:

- `primary candidate`
- `secondary candidate`
- `plausible but weaker fallback`

This can be approximated with:

- weighted field overlap
- SQL scoring logic
- lexical ranking

But that is usually:

- less natural
- harder to tune
- more brittle on messy user input
- less useful for ambiguity-aware ranking

For candidate ordering, Qdrant is a better fit.

---

## Relationship to normalization

This architecture does not remove the value of normalization.

Normalization can still help by producing:

- canonical symptom hints
- component hints
- phase hints
- likely failure-mode hints

These can be used to:

- enrich the Qdrant card query
- add metadata filters
- later explain why a card was selected

So the intended model is not:

```text
normalization OR semantic retrieval
```

It is:

```text
normalization + semantic card retrieval
```

---

## Relationship to chunk selection

Once card candidates are chosen, chunk selection should not be fully open-ended across the whole practical corpus.

Instead:

- use the chosen cards to limit the candidate chunk set
- use chunk tags to assemble the final evidence pack

Recommended first-response pattern:

- one primary card
- two chunks from the primary card
- one chunk from each of up to two competing cards
- optional one theory chunk

This keeps retrieval and reasoning aligned:

- Qdrant ranks cards
- Postgres supplies structure
- chunk tags shape the evidence pack

---

## Current implementation takeaway

Best current architecture:

```text
Qdrant = card candidate ranking layer
Postgres = canonical card storage layer
Tags = chunk packing layer
Theory retrieval = optional semantic mechanism layer
```

This split is coherent with the current experiments and looks like the strongest basis for implementation.
