## 1) Purpose

This document freezes the current best manual-test prompt baseline for
`query_structuring` so later experiments can compare against a stable reference.

This is not yet the final module specification.
It is a working prompt artifact for ongoing prompt tuning on:

- model: `openai/gpt-oss-20b`
- vocabulary artifact:
  `Specification/runtime/request_pipeline/query_structuring_controlled_vocabulary.manual_test.json`

## 2) Current Best Baseline

The current best-performing prompt shape is the evidence-backed selection format:

- selected controlled-vocabulary terms must include supporting query evidence;
- unsupported nearby terms may be explicitly rejected;
- non-vocabulary fields are extracted directly from the query.

This baseline is currently preferred because it reduced the most obvious false
positive vocabulary picks better than:

- plain anti-hallucination wording;
- hard-cap-only prompts;
- two-phase wording without evidence fields;
- few-shot anti-example alone.

## 3) System Prompt

```text
You are a query_structuring module. Return one JSON object only.
For symptoms, affected_subsystems, failure_modes, and system_properties, choose terms from the full controlled vocabulary only when supported by the query.
For each selected controlled-vocabulary term, return: term, evidence_span, support_level.
evidence_span must be a short near-verbatim fragment from the query, not a free-form explanation.
support_level meanings: explicit = directly named or unmistakably stated; strong_paraphrase = clearly described in different words; weak_inference = plausible but not directly grounded.
Prefer omission over weak_inference.
Also return rejected_nearby_terms for up to 4 tempting but unsupported vocabulary terms, with a short reason for rejection.
Extract entities, constraints, triggers, observability_signals, and unresolved_terms directly from the query.
Keep arrays short.
Schema: {"intent":string,"scenario":string,"symptoms":[{"term":string,"evidence_span":string,"support_level":"explicit|strong_paraphrase|weak_inference"}],"affected_subsystems":[{"term":string,"evidence_span":string,"support_level":"explicit|strong_paraphrase|weak_inference"}],"failure_modes":[{"term":string,"evidence_span":string,"support_level":"explicit|strong_paraphrase|weak_inference"}],"system_properties":[{"term":string,"evidence_span":string,"support_level":"explicit|strong_paraphrase|weak_inference"}],"entities":string[],"constraints":string[],"triggers":string[],"observability_signals":string[],"unresolved_terms":string[],"rejected_nearby_terms":[{"term":string,"reason":string}],"confidence":"low|medium|high"}
```

## 4) User Prompt Shape

```text
Query:
<normalized user query>

Controlled vocabulary:
<compact JSON dictionary>
```

The compact JSON dictionary should currently contain:

- `canonical_symptoms`
- `affected_components`
- `failure_mode_candidates`
- `violated_properties`

## 5) Recommended Runtime Parameters For Manual Tests

- `temperature = 0.0`
- `response_format = {"type": "json_object"}`
- `max_tokens = 2200`

## 6) What Worked

- obvious unsupported terms such as `lwt_split_brain` moved out of selected terms;
- `rejected_nearby_terms` gave useful debugging signal;
- `evidence_span` made selected terms easier to inspect and post-validate.

## 7) Known Weaknesses

- token usage is high compared with simpler prompts;
- some terms can still be selected on weak support if not filtered later;
- `constraints` and `triggers` can still overlap;
- this shape is better as raw model output than as final trusted module output.

## 8) Intended Next Step

Future experiments should compare against this baseline rather than replacing it
implicitly.

Candidate directions:

- stricter post-validation on `support_level`;
- prompt variants that reduce token cost while preserving evidence-backed term selection;
- prompt variants that better separate `constraints` from `triggers`.
