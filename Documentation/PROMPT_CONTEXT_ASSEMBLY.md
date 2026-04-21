# Prompt Context Assembly

## Why This Document Exists

This document explains how the `prompt_context_assembly` step builds the prompt
used for diagnostic response generation.

It is not the formal implementation contract. The formal contract lives in:

- `Specification/runtime/request_pipeline/prompt_context_assembly.md`
- `Specification/runtime/runtime.md`
- `Specification/contracts/runtime/runtime_config.md`

This document is the human-readable explanation of the design:

- what goes into prompt assembly;
- where each piece of data comes from;
- how retrieved chunks are filtered and assigned roles;
- why the prompt-facing context is not a raw dump of internal pipeline types;
- what the final prompt looks like.

## What This Module Is For

The module sits after retrieval and hydration.

At this point the runtime already has:

- a normalized user request;
- a structured interpretation of that request;
- one hydrated primary incident card;
- zero or more hydrated alternative incident cards;
- practice chunks retrieved from incident-card-derived chunk collections;
- theory chunks retrieved from the theory corpus;
- a prompt asset loaded from runtime config.

The job of `prompt_context_assembly` is to combine those inputs into one
diagnostic prompt for the next model-generation step.

It also returns the selected chunks separately from the prompt so later runtime
state can preserve what evidence was shown to the model.

## Inputs

The module receives these request-time inputs:

- `NormalizedUserRequest`
- `QueryStructuringOutput`
- `CardHydrationOutput`
- `IncidentEvidenceRetrievalOutput`
- `TheoryEvidenceRetrievalOutput`

It also receives constructor-time settings:

- `PromptContextSettings`

And it loads a prompt asset from:

- `PromptContextSettings.prompt_asset_path`

The prompt asset is a JSON wrapper around the diagnostic-response prompt text.
It contains the prompt template, the response schema, policy constraints, and the
`{{json_context}}` placeholder where assembled context is inserted.

## Why The Context Is Prompt-Facing

The module does not pass internal pipeline types directly to the model.

The most important example is `QueryStructuringOutput`.
That output contains fields such as:

- `term`
- `evidence_span`
- `support_level`
- `rejected_nearby_terms`
- `token_usage`

Those fields are useful for upstream quality control and debugging. They explain
why a structured query term was selected and how strongly it was supported.

They are not necessarily useful as model input for diagnostic answer generation.
Passing them directly would make the prompt noisy and could push the answer
model to reason about internal confidence labels instead of the user-facing
incident.

So `prompt_context_assembly` builds a derived prompt-facing object:

- `normalized_incident_query`

This object keeps only the compact signal needed by the diagnostic prompt.

## Query Mapping

`normalized_incident_query` is built only from fields that actually exist in
`QueryStructuringOutput.structured_query`.

Current mapping:

```text
normalized_incident_query.recognized_canonical_symptoms
  <- StructuredUserQuery.symptoms[*].term

normalized_incident_query.affected_components
  <- StructuredUserQuery.affected_subsystems[*].term

normalized_incident_query.failure_mode_candidates
  <- StructuredUserQuery.failure_modes[*].term

normalized_incident_query.signals_present
  <- StructuredUserQuery.symptoms[*].term
   + StructuredUserQuery.triggers
   + StructuredUserQuery.observability_signals

normalized_incident_query.unmapped_user_symptoms
  <- []

normalized_incident_query.observed_phase
  <- []

normalized_incident_query.missing_signals
  <- []
```

The empty arrays are intentional.
The current `StructuredUserQuery` shape does not have symptom-specific unmapped
terms, observed phases, or missing signals. The assembly step must not invent
them from cards, chunks, or raw prompt wording.

Fields not included in the prompt context today:

- `evidence_span`
- `support_level`
- `rejected_nearby_terms`
- `token_usage`
- `confidence`
- `system_properties`
- `entities`
- `constraints`
- `intent`
- `scenario`

Those may become useful later, but they should be added only when the
prompt-facing context schema and prompt text are updated deliberately.

## Card Inputs

The primary hydrated card becomes:

- `matched_incident_card`

The prompt uses it as a structured precedent, not as proof.
This distinction matters: the model should use the card to ground a plausible
diagnostic path, while still preserving uncertainty.

Full alternative cards are not inserted into the prompt in the current version.
Instead, alternative cards provide metadata for a smaller prompt-facing section:

- `competing_precedent_context`

This section is built from selected alternative chunks plus hydrated alternative
card metadata.

## Incident Chunk Selection

Incident chunks come from card-derived practice chunks.

The module assigns selected incident chunks to four prompt roles:

- `evidence_for_match`
- `first_check_hint`
- `supporting_explanation`
- `alternative_context`

The roles are selected in this order:

1. `evidence_for_match`
2. `first_check_hint`
3. `supporting_explanation`
4. `alternative_context`

Each role has settings in runtime config:

- source pool;
- limit;
- optional per-case limit;
- fallback behavior;
- tag priority list.

For a role, candidate chunks are ranked by:

1. best matching configured tag priority;
2. retrieval score, higher first;
3. original retrieval order.

This means tag intent wins before score.
For example, a lower-scoring `chunk_role:diagnostic_step` chunk can beat a
higher-scoring `chunk_role:symptom` chunk for `first_check_hint`, because that
role is looking for a discriminating next check.

Duplicate chunks are avoided across roles when another eligible chunk exists.
Required roles may reuse a duplicate only as a last fallback. Optional roles do
not reuse duplicates in the current version.

Alternative context uses deterministic round-robin across alternative cases.
The module groups eligible chunks by `case_id`, ranks each group by the same tag
and score rules, then walks the cases in hydrated alternative-card order. It
takes at most one next chunk per case per round until the global role limit is
reached or all case groups are exhausted.

## Role Meanings

`evidence_for_match` explains why the primary incident card is relevant.
It usually prefers symptom, failure-mode, or contributing-factor evidence.

`first_check_hint` provides material for the single next discriminating check.
It usually prefers diagnostic-step, investigation, or lesson evidence.

`supporting_explanation` preserves nuance from the primary precedent.
This role exists because manual evaluation showed that transactional and retry
cases can lose important ambiguity if only match evidence and first-check hints
are selected. It is enabled in the default runtime config with `limit = 1`.

`alternative_context` gives the model an explicit competing precedent when the
retrieval result is not unique.
It is optional, but when present it should make the model less likely to collapse
too early to the primary card.

## Competing Precedent Context

`alternative_context` chunks still appear in `incident_evidence_chunks`.

In addition, the module builds a prompt-facing summary handle:

```json
"competing_precedent_context": [
  {
    "case_id": "mysql_8_0_34",
    "title": "MySQL transaction isolation anomaly example",
    "source_name": "Example source",
    "competing_signal": "Some transaction anomalies can also come from weaker-than-expected isolation semantics even without faults."
  }
]
```

This field is not model-generated.

It is derived from:

- selected `AlternativeContext` chunk `case_id`;
- hydrated alternative card `title`;
- hydrated alternative card `source_name`;
- selected `AlternativeContext` chunk text.

The `competing_signal` is only normalized to one line by trimming and collapsing
whitespace. It is not summarized or rewritten.

`competing_precedent_context` has one entry per selected `alternative_context`
chunk. If the same alternative case contributes two selected chunks, the prompt
context intentionally contains two entries with the same `case_id`, preserving
selected chunk order.

The module expects every selected alternative chunk to have a hydrated
alternative card with the same `case_id`. Chunks are derived from cards, so a
chunk without its card would mean inconsistent pipeline data.

## Theory Chunk Selection

Theory chunks are selected separately from incident chunks.

They use the role:

- `mechanism_explanation`

Theory chunks do not expose incident chunk tags in the current version, so they
are selected in retrieval order up to the configured limit.

They are used to explain general mechanisms behind the incident, such as
transaction isolation anomalies, lost updates, or distributed consistency
properties.

## Prompt Asset

The prompt text is not hardcoded in the module.

Runtime config points to a JSON prompt asset:

```toml
[prompt_context]
prompt_asset_path = "Specification/runtime/request_pipeline/prompt_context_assembly/diagnostic_response_prompt_baseline.manual_test.json"
```

The asset contains the diagnostic-response instructions and one
`{{json_context}}` placeholder.

At runtime, the module serializes the prompt context as JSON and replaces that
placeholder exactly once.

Prompt-facing JSON uses module-private DTOs and explicit snake-case field and
role names. The shared `PromptEvidenceRole` enum is not serialized directly.

## Final Prompt Shape

The final prompt is plain UTF-8 text.
It is not split into provider-specific system/user messages in the current
version.

Conceptually it looks like this:

```text
You are a diagnostic assistant for distributed systems incidents.

Your task:
Produce the first diagnostic response to the user as strict JSON.

...

JSON context follows:
{
  "task": "diagnostic_response",
  "user_problem": "...",
  "input_token_count": 0,
  "normalized_incident_query": { ... },
  "matched_incident_card": { ... },
  "incident_evidence_chunks": [ ... ],
  "competing_precedent_context": [ ... ],
  "theory_chunks": [ ... ],
  "policy_constraints": [ ... ]
}
```

The output of the module is:

```text
PromptContextAssemblyOutput {
  prompt,
  incident_evidence_chunks,
  theory_chunks
}
```

The selected chunks returned in the output must match the chunks embedded in the
rendered prompt context.

## Compact Example

Suppose the user asks:

```text
During network issues our transactions sometimes lose writes, reads look
inconsistent, and retrying ambiguous errors makes the outcome confusing.
```

The query structuring step might produce:

```json
{
  "symptoms": [
    { "term": "lost_writes", "evidence_span": "lose writes", "support_level": "explicit" },
    { "term": "read_skew", "evidence_span": "reads look inconsistent", "support_level": "strong_paraphrase" }
  ],
  "affected_subsystems": [
    { "term": "transaction_api", "evidence_span": "transactions", "support_level": "explicit" },
    { "term": "retry_mechanism", "evidence_span": "retrying ambiguous errors", "support_level": "explicit" }
  ],
  "failure_modes": [
    { "term": "transaction_retry_bug", "evidence_span": "retrying ambiguous errors", "support_level": "strong_paraphrase" }
  ],
  "triggers": ["network issues"],
  "observability_signals": ["inconsistent reads", "lost writes"]
}
```

The prompt-facing query becomes:

```json
{
  "recognized_canonical_symptoms": ["lost_writes", "read_skew"],
  "unmapped_user_symptoms": [],
  "affected_components": ["transaction_api", "retry_mechanism"],
  "failure_mode_candidates": ["transaction_retry_bug"],
  "observed_phase": [],
  "signals_present": [
    "lost_writes",
    "read_skew",
    "network issues",
    "inconsistent reads",
    "lost writes"
  ],
  "missing_signals": []
}
```

Selected incident chunks might be:

```json
[
  {
    "role": "evidence_for_match",
    "case_id": "mongodb_4_2_6_jepsen_2020_05_15",
    "chunk_tags": ["chunk_role:symptom", "chunk_role:failure_mode"],
    "text": "Acknowledged writes could disappear when transactions and partitions interacted."
  },
  {
    "role": "first_check_hint",
    "case_id": "mongodb_4_2_6_jepsen_2020_05_15",
    "chunk_tags": ["chunk_role:diagnostic_step"],
    "text": "Inspect the effective read concern and write concern attached to each transaction."
  },
  {
    "role": "supporting_explanation",
    "case_id": "mongodb_4_2_6_jepsen_2020_05_15",
    "chunk_tags": ["chunk_role:contributing_factor"],
    "text": "Ambiguous transaction errors can make retry behavior dangerous because the actual outcome may not match the apparent error."
  },
  {
    "role": "alternative_context",
    "case_id": "mysql_8_0_34",
    "chunk_tags": ["chunk_role:failure_mode"],
    "text": "Some transaction anomalies can also come from weaker-than-expected isolation semantics even without network faults."
  }
]
```

Then the prompt also includes:

```json
"competing_precedent_context": [
  {
    "case_id": "mysql_8_0_34",
    "title": "MySQL transaction isolation anomaly example",
    "source_name": "Example source",
    "competing_signal": "Some transaction anomalies can also come from weaker-than-expected isolation semantics even without network faults."
  }
]
```

This gives the answer model two things at once:

- enough primary evidence to produce a useful first diagnostic response;
- an explicit competing precedent so it keeps uncertainty alive when the match
  is not unique.

## Main Design Choices

The current design makes several deliberate tradeoffs.

It keeps the prompt context compact instead of dumping every internal field.
This reduces noise and keeps the answer model focused on user-facing diagnosis.

It selects chunks by role, not only by score.
This makes the evidence pack more balanced: one chunk for why the match matters,
one for the next check, one optional supporting nuance, and optional competing
context.

It separates `alternative_context` from `competing_precedent_context`.
The former is evidence; the latter is a prompt-facing attention handle.
This separation exists because manual evaluation showed that alternative
evidence can be ignored when it is only mixed into a generic evidence list.

It does not generate summaries during prompt assembly.
All prompt context is deterministic and derived from already available pipeline
data.
