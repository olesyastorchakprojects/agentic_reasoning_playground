## 1) Purpose

This document freezes the current best manual-test prompt baseline v2 for
`query_structuring`.

It supersedes the earlier baseline as the best general-quality prompt tested so
far on:

- model: `openai/gpt-oss-20b`
- vocabulary artifact:
  `Specification/runtime/request_pipeline/query_structuring_controlled_vocabulary.manual_test.json`

This is still a manual-test prompt artifact, not the final module specification.

## 2) Why V2 Is Better

Baseline v2 keeps the strongest parts of the earlier
`observed_vs_hypothesized_split` prompt and adds only one narrow extra rule for
`failure_modes`.

Compared with the prior baseline, this version:

- kept valid JSON output;
- preserved good `symptoms`, `affected_subsystems`, and `system_properties`;
- reduced `failure_modes` noise;
- removed unsupported picks such as `lwt_split_brain` from selected terms in the
  tested lock-semantics query.

The key added rule is:

```text
For failure_modes only: return at most 1 item; choose the most directly supported hypothesis; otherwise return [].
```

## 3) System Prompt

```text
You are a query_structuring module. Return one JSON object only.
Interpretation rules by field:
symptoms = only observed user-visible effects or directly described anomalous behavior;
affected_subsystems = only components or subsystems implicated by the query;
failure_modes = only hypotheses about what might be wrong, not observations;
system_properties = only safety or consistency properties at stake, not components and not failure modes.
For vocabulary-based fields, choose terms from the controlled vocabulary only when the query supports them.
For each selected vocabulary term, return term, evidence_span, support_level.
Prefer omission over weak_inference.
Use the query directly for the remaining fields.
Also return rejected_nearby_terms.
For failure_modes only: return at most 1 item; choose the most directly supported hypothesis; otherwise return [].
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

## 6) Tested Behavior On The Lock-Semantics Query

Observed strengths:

- selected `failure_modes` collapsed to one strong hypothesis:
  `lock_ownership_violation`;
- unsupported nearby terms moved into `rejected_nearby_terms`;
- `symptoms` remained clean:
  - `duplicate_lock_holders`
  - `lost_updates`
- `affected_subsystems` remained clean:
  - `lock_service`
  - `key_value_api`
  - `external_critical_sections`
- `system_properties` remained clean:
  - `exclusive ownership semantics for external critical sections`
  - `safe mutual exclusion for distributed locks`

## 7) Known Weaknesses

- `constraints` may still come back empty even when some background conditions
  are available from the query;
- `observability_signals` are still more query-derived than canonicalized;
- `rejected_nearby_terms` may contain duplicates and may need post-validation;
- `confidence` can still skew too high.

## 8) Recommended Interpretation

This prompt currently looks like the best overall baseline for continued manual
testing because it balances:

- vocabulary discipline;
- field-type discipline;
- usable evidence traces;
- lower `failure_modes` hallucination risk.

It should still be treated as a raw-output prompt, with later post-validation
expected before final trusted runtime output is defined.
