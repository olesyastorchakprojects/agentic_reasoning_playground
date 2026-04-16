# Multi-Card Chunk Pack Test

Goal:

- test how to build a compact evidence pack when multiple cards are plausible;
- keep one primary card for structure;
- inject competing evidence through chunks from other plausible cards.

Current packing rule under test:

1. Choose one primary card.
2. Take 2 chunks from the primary card:
   - `evidence_for_match`
   - `first_check_hint`
3. Take 1 chunk each from up to 2 competing cards:
   - both as `alternative_context`
4. Optionally add 1 theory chunk.

Budget under test:

- primary-card chunks: 2
- competing-card chunks: 2
- theory chunks: 1
- total practical chunks: 4

Why this shape:

- primary card still provides the main investigation frame;
- competing cards widen the hypothesis space without replacing the main frame;
- the chunk budget stays small enough for first-response generation.
