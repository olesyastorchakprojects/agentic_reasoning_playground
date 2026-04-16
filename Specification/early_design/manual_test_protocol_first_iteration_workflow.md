# Manual Test Protocol: First Iteration Workflow

This document defines the manual test for the workflow described in
`first_iteration_incident_card_chunk_model_design.md`.

Workflow under test:

```text
user problem
  -> one matched incident card
  -> extract context / hypotheses / checks
  -> add compact chunk evidence pack
  -> optional theory chunk
  -> send prepared context with first-response prompt
  -> inspect first diagnostic response
```

---

## Goal

Verify that the first-iteration workflow can produce a useful first diagnostic response.

We are not testing:
- full multi-turn diagnosis;
- hypothesis update after user observation;
- final root cause determination;
- full mitigation planning.

We are testing whether the workflow can produce a good first investigative frame.

---

## Steps

For one test query:

1. Select one user problem.
2. Select one primary incident card.
3. Extract from the card:
   - `context`
   - `hypotheses`
   - `checks`
4. Select a compact incident chunk evidence pack:
   - `evidence_for_match`
   - `first_check_hint`
   - optional `supporting_explanation`
   - optional `alternative_context`
5. Optionally add one theory chunk from the theory collection.
6. Build the first-iteration prompt context.
7. Send that context to the model.
8. Inspect the response.

---

## Success Criteria

The workflow is considered successful if:

1. The selected card is a plausible primary precedent.
2. The card-derived `context`, `hypotheses`, and `checks` are actually useful scaffolding.
3. The selected chunks add evidence or nuance rather than just repeating the card.
4. The model returns the expected response shape:
   - `Problem understanding`
   - `Similar practical context`
   - `Active hypotheses`
   - `First check`
   - `How to interpret the result`
5. The answer gives one useful discriminating first check.
6. The answer preserves uncertainty and does not prematurely claim a final root cause.

---

## Failure Modes

The workflow is considered weak or failed if:

- the card match is implausible;
- the chunks do not support the card in a meaningful way;
- the model ignores the card checks and gives generic advice;
- the model overfits to the card and states a final diagnosis too early;
- the compact evidence pack is too small and the response becomes vague;
- the answer does not help the user start the investigation.

---

## Output Format For Each Manual Run

```text
Query:

Primary matched card:

Card-derived context:

Card-derived hypotheses:

Card-derived checks:

Selected incident chunks:
1.
2.
3.

Optional theory chunk:

Prompt context assembled:
yes / no

Model output shape:
good / weak

Verdict:
good / weak / overfit / too_generic / missing_alternative
```
