## 1) Purpose

This document freezes the current `baseline v3 candidate` for manual prompt
testing of `query_structuring`.

It does not replace baseline v2 yet.
It records the next most promising prompt variant discovered after additional
testing from the v2 baseline.

Target setup:

- model: `openai/gpt-oss-20b`
- vocabulary artifact:
  `Specification/runtime/request_pipeline/query_structuring_controlled_vocabulary.manual_test.json`

## 2) Why This Is Only A Candidate

Baseline v3 candidate is intentionally conservative.
It keeps baseline v2 almost unchanged and adds only one extra local rule:

```text
observability_signals should preserve user wording as much as possible and should be short near-verbatim observations from the query.
```

This variant looked promising because it improved `observability_signals`
without clearly degrading the rest of the tested structure.

It is still marked as a candidate because:

- the full prompt has not yet been tested on multiple query styles;
- baseline v2 remains the safer frozen reference;
- the benefit so far is local rather than broad across many scenarios.

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
observability_signals should preserve user wording as much as possible and should be short near-verbatim observations from the query.
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

## 6) What Looked Better In The Tested Query

Compared with baseline v2, this candidate improved the shape of
`observability_signals` in the tested lock-semantics query:

- `two workers sometimes both continue as if they still hold the same lock`
- `conflicting updates`
- `lost writes in the external system`

Other tested strengths were preserved:

- `failure_modes` remained focused on `lock_ownership_violation`;
- `lwt_split_brain` did not return as a selected term;
- `system_properties` remained reasonable;
- JSON output stayed valid.

## 7) Known Limits

- not yet proven on multiple query styles;
- `constraints` are not improved in this candidate;
- `rejected_nearby_terms` may become empty, which can reduce debug signal;
- this should still be treated as raw model output rather than final trusted
  runtime output.

## 8) Recommended Use

Use this artifact as the next comparison point for continued prompt tuning,
while keeping baseline v2 as the currently frozen safest reference.
