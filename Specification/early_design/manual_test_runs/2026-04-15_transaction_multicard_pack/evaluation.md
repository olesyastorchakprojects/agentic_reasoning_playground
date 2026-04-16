# Multi-Card Chunk Pack Evaluation

Query:

`We thought we were using strong enough transactional guarantees, but under load and especially during network issues we started seeing strange behavior: some writes seem to disappear, sometimes reads are inconsistent, and retries after errors seem to make things even more confusing.`

Primary card:

- `mongodb_4_2_6`

Competing cards represented through chunks:

- `ravendb_6_0_2`
- `mysql_8_0_34`

Chunk pack under test:

1. primary `evidence_for_match` from MongoDB
2. primary `first_check_hint` from MongoDB
3. `alternative_context` from RavenDB
4. `alternative_context` from MySQL
5. one optional theory chunk

Verdict:

- `good`

What worked:

- The response stayed compact and did not turn into a list of report summaries.
- One primary precedent still anchored the answer.
- Competing evidence widened the hypothesis space in a useful way.
- The model preserved a non-primary hypothesis about weaker isolation semantics.

What we learned:

- In a multi-card case, it is better to keep exactly one primary card for structure.
- Competing cards should usually contribute chunks, not full card payloads, in the first response.
- A small mixed pack can work:
  - 2 primary chunks
  - 1 chunk from each of up to 2 competing cards
  - optional theory chunk

Current packing rule:

- Do not send 2 or 3 full cards to the model in the first response.
- Send:
  - one primary card,
  - two primary chunks,
  - one competing chunk per plausible competing card,
  - at most one theory chunk.

Why:

- this preserves a stable main frame;
- avoids context explosion;
- still gives the model enough material to keep alternative explanations alive.
