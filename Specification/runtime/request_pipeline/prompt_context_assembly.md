## 1) Purpose / Scope

This document defines the runtime leaf-module contract for
`prompt_context_assembly`.

This module exists to:
- accept the shared normalized user input;
- accept the shared structured query output;
- accept hydrated incident cards;
- accept retrieved incident evidence chunks;
- accept retrieved theory evidence chunks;
- receive prompt-context packing policy through typed runtime settings;
- load a prompt asset selected by typed runtime settings;
- select a compact, role-balanced evidence pack for model generation;
- render a filled diagnostic-response prompt for the next model-generation module;
- return the selected evidence chunks separately for history and traceability.

This document is the source of truth for:
- the `prompt_context_assembly` leaf-module boundary;
- the module public interface;
- the module-owned deterministic chunk-selection mechanics;
- the prompt asset contract consumed by this module;
- the filled prompt output produced by this module;
- the selected evidence chunk output produced by this module;
- the module-owned error boundary.

This document does not define:
- raw runtime TOML shape or config-file parsing;
- semantic card retrieval from Qdrant;
- PostgreSQL card hydration;
- incident evidence retrieval;
- theory evidence retrieval;
- model invocation;
- model response validation or normalization;
- multi-turn diagnostic update behavior.

Shared request and response types are defined by:
- `Specification/runtime/runtime.md`

OpenInference span behavior for the context-aware execution path is defined by:
- `Specification/runtime/observability/open_inference_spans.md`

The generated Rust module file for the current version is:
- `src/request_pipeline/prompt_context_assembly.rs`

## 2) Required Shared Types

This module must use the shared runtime input types:
- `NormalizedUserRequest`
- `QueryStructuringOutput`
- `StructuredUserQuery`
- `StructuredUserQueryTerm`
- `RejectedNearbyTerm`
- `CardHydrationOutput`
- `IncidentCard`
- `IncidentEvidenceChunk`
- `IncidentEvidenceRetrievalOutput`
- `TheoryEvidenceChunk`
- `TheoryEvidenceRetrievalOutput`

This module must use the shared runtime enum:
- `IncidentChunkTag`

This module must produce the shared runtime output type:
- `PromptContextAssemblyOutput`

The current generated Rust runtime must define prompt-assembly shared types
equivalent in ownership to:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptEvidenceRole {
    EvidenceForMatch,
    FirstCheckHint,
    SupportingExplanation,
    AlternativeContext,
    MechanismExplanation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromptIncidentEvidenceChunk {
    pub role: PromptEvidenceRole,
    pub chunk_id: String,
    pub case_id: String,
    pub score: f32,
    pub chunk_tags: Vec<IncidentChunkTag>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromptTheoryEvidenceChunk {
    pub role: PromptEvidenceRole,
    pub chunk_id: String,
    pub score: f32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EvidenceTopology {
    pub primary_evidence_roles: Vec<String>,
    pub alternative_context_present: bool,
    pub alternative_context_case_ids: Vec<String>,
    pub theory_evidence_present: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromptContextAssemblyOutput {
    pub prompt: String,
    pub response_schema: serde_json::Value,
    pub evidence_topology: EvidenceTopology,
    pub incident_evidence_chunks: Vec<PromptIncidentEvidenceChunk>,
    pub theory_chunks: Vec<PromptTheoryEvidenceChunk>,
}
```

Shared-type rules:
- `IncidentChunkTag` is the shared typed representation of canonical incident chunk tags and is defined in `Specification/runtime/runtime.md`;
- `PromptEvidenceRole` is the prompt-facing role assigned by this module to selected chunks;
- `PromptEvidenceRole` must not derive `serde::Serialize` in the current version;
- serialization of `PromptEvidenceRole` into prompt JSON must use a module-private DTO mapping each variant to its required snake-case string literal;
- `PromptIncidentEvidenceChunk.chunk_tags` must contain typed `IncidentChunkTag` values, not raw strings;
- `PromptIncidentEvidenceChunk.chunk_tags` must contain only recognized tags parsed from the source `IncidentEvidenceChunk.chunk_tags`;
- unknown collection-returned tag strings must not be included in `PromptIncidentEvidenceChunk.chunk_tags`;
- `PromptContextAssemblyOutput.prompt` is the fully rendered prompt string passed to the next model-generation module;
- `PromptContextAssemblyOutput.response_schema` is the validated prompt-owned response schema passed unchanged to the next model-generation module;
- `PromptContextAssemblyOutput.evidence_topology` is the compact summary of which evidence branches and role buckets were actually embedded in the rendered prompt context;
- `PromptContextAssemblyOutput.incident_evidence_chunks` contains the selected incident chunks separately from the prompt for history and traceability;
- `PromptContextAssemblyOutput.theory_chunks` contains the selected theory chunks separately from the prompt for history and traceability;
- the selected chunks returned separately in `PromptContextAssemblyOutput` must be exactly the selected chunks embedded inside the rendered prompt context.

Import rule for the generated Rust module:
- shared input and output types used by this module, including `IncidentChunkTag`, must be imported from `crate::shared_types`;
- `PromptContextSettings`, `ChunkRolePackingSettings`, and `ChunkPackingSource` must be imported from `crate::config`.

Shared-type placement rule:
- before code generation for this module, all shared prompt-context types listed in this section must exist in `src/shared_types.rs`.

`EvidenceTopology` rules:
- `primary_evidence_roles` must contain only prompt-facing snake-case role names;
- `primary_evidence_roles` must preserve role order;
- `alternative_context_case_ids` must contain unique case ids in first-seen order;
- `theory_evidence_present` must reflect whether any theory chunk was selected.

## 3) Settings Dependency

This module must receive the typed settings slice:
- `PromptContextSettings`

`PromptContextSettings` is defined at the crate-level runtime boundary in:
- `Specification/runtime/runtime.md`

Settings rules:
- this module must receive `PromptContextSettings` through its constructor;
- this module must not read raw TOML or raw environment variables directly;
- `prompt_asset_path` is the runtime-owned JSON prompt asset path selected by config;
- chunk role limits must be read from `PromptContextSettings`;
- chunk role tag priorities must be read from `PromptContextSettings`;
- chunk role fallback behavior must be read from `PromptContextSettings`;
- the module must not hardcode incident tag priority lists or chunk limits;
- the module owns the deterministic selection algorithm.

Constructor settings validation rules:
- `settings.chunk_packing.evidence_for_match.source` must be `ChunkPackingSource::PrimaryIncident`;
- `settings.chunk_packing.first_check_hint.source` must be `ChunkPackingSource::PrimaryIncident`;
- `settings.chunk_packing.supporting_explanation.source` must be `ChunkPackingSource::PrimaryIncident`;
- `settings.chunk_packing.alternative_context.source` must be `ChunkPackingSource::AlternativeIncident`;
- `settings.chunk_packing.mechanism_explanation.source` must be `ChunkPackingSource::Theory`;
- `settings.chunk_packing.evidence_for_match.limit` must be greater than or equal to `1`;
- `settings.chunk_packing.first_check_hint.limit` must be greater than or equal to `1`;
- `settings.chunk_packing.supporting_explanation.limit` may be `0`;
- `settings.chunk_packing.alternative_context.limit` may be `0`;
- `settings.chunk_packing.mechanism_explanation.limit` may be `0`;
- `settings.chunk_packing.mechanism_explanation.limit` must be less than or equal to `1`;
- `settings.chunk_packing.alternative_context.per_case_limit` must be `Some(n)` with `n > 0` when `alternative_context.limit > 0`;
- when `settings.chunk_packing.alternative_context.limit = 0`, `per_case_limit` may be `None` or any `Some(n)` value; the constructor must not reject it because alternative context selection is disabled;
- `settings.chunk_packing.mechanism_explanation.tag_priority` must be empty because theory chunks do not expose tags.

## 4) Prompt Asset

This module must load a prompt asset from:
- `PromptContextSettings.prompt_asset_path`

The schema for prompt assets must live in the same directory as the configured
prompt asset. The schema path is derived from `prompt_asset_path` by using the
same parent directory and replacing only the file name:
- if the prompt asset file name ends with `.manual_test.json`, replace that suffix with `.schema.json`;
- otherwise, if the prompt asset file name ends with `.json`, replace that suffix with `.schema.json`.

The generated runtime prompt asset type must be equivalent in ownership to:

```rust
struct DiagnosticResponsePromptAsset {
    pub version: String,
    pub name: String,
    pub template: String,
    pub context_placeholder: String,
    pub required_placeholders: Vec<String>,
    pub response_schema: serde_json::Value,
    pub policy_constraints: Vec<String>,
}
```

Prompt-asset rules:
- `DiagnosticResponsePromptAsset` is module-private to `prompt_context_assembly`;
- `DiagnosticResponsePromptAsset` must not be re-exported from `prompt_context_assembly`;
- `DiagnosticResponsePromptAsset` must not be defined or re-exported from `crate::shared_types`;
- `new(...)` must read the prompt asset from `settings.prompt_asset_path`;
- `new(...)` must derive the prompt asset schema path from `settings.prompt_asset_path`;
- `new(...)` must fail when `prompt_asset_path` is empty;
- `new(...)` must fail when the prompt asset file cannot be read;
- `new(...)` must fail when the derived prompt asset schema file cannot be read;
- `new(...)` must fail when the prompt asset JSON is invalid;
- `new(...)` must fail when the prompt asset schema JSON is invalid;
- `new(...)` must validate the prompt asset against the derived prompt asset schema;
- `new(...)` must validate the prompt asset against the asset contract in this section after schema validation;
- `template` must contain exactly one `{{json_context}}` placeholder in the current version;
- `context_placeholder` must equal `{{json_context}}`;
- `required_placeholders` must contain exactly one value: `json_context`;
- `policy_constraints` must be non-empty;
- `response_schema` must be present and must be a JSON object;
- the module must not hardcode the diagnostic-response prompt template;
- the module must render the prompt by replacing `{{json_context}}` with the serialized JSON context object.

## 5) Public Interface

The generated Rust module must define a public module boundary equivalent in
ownership to:

```rust
pub struct PromptContextAssembly {
    // implementation-owned fields
}

impl PromptContextAssembly {
    pub fn new(
        settings: PromptContextSettings,
    ) -> Result<Self, PromptContextAssemblyError>;

    pub fn assemble(
        &self,
        request: &NormalizedUserRequest,
        query: &QueryStructuringOutput,
        cards: &CardHydrationOutput,
        incident_evidence: &IncidentEvidenceRetrievalOutput,
        theory_evidence: &TheoryEvidenceRetrievalOutput,
    ) -> Result<PromptContextAssemblyOutput, PromptContextAssemblyError>;

    pub fn assemble_with_context(
        &self,
        request: &NormalizedUserRequest,
        query: &QueryStructuringOutput,
        cards: &CardHydrationOutput,
        incident_evidence: &IncidentEvidenceRetrievalOutput,
        theory_evidence: &TheoryEvidenceRetrievalOutput,
        context: &Context,
    ) -> Result<PromptContextAssemblyOutput, PromptContextAssemblyError>;
}
```

For the current version, the implementation-owned fields must contain exactly:
- `settings: PromptContextSettings`
- `prompt_asset: DiagnosticResponsePromptAsset`

Rules:
- `new(...)` must validate constructor-owned settings and retain settings for reuse;
- `new(...)` must load and validate the configured prompt asset;
- `new(...)` constructor validation failures must be returned through `PromptContextAssemblyError`;
- `assemble(...)` must delegate to `assemble_with_context(...)` with
  `&Context::noop()`;
- `assemble_with_context(...)` is the context-aware request-time entrypoint used
  by the orchestrator;
- `assemble_with_context(...)` must treat `context.open_inference.root_span` as
  the parent span for the module-owned OpenInference chain span
  `oi.chain.prompt_context_assembly`;
- `assemble(...)` must not mutate any input;
- `assemble(...)` must not call external services;
- `assemble(...)` must render the filled diagnostic-response prompt string;
- `assemble(...)` must not invoke the model or construct provider-specific message objects.

## 6) Primary Card Rule

This module uses one hydrated primary incident card as the structured precedent.

Rules:
- if `cards.primary` is `Some(card)`, the module must derive a compact prompt-facing `matched_incident_card` object from that card rather than embedding the full shared `IncidentCard` payload;
- if `cards.primary` is `None`, assembly must fail with `PromptContextAssemblyError::MissingPrimaryCard`;
- full alternative cards from `cards.alternatives` must not be included in the rendered prompt JSON context in the current version;
- alternative cards may affect ordering of `alternative_context` chunks by their `case_id`;
- the module must treat the primary card as a structured precedent, not as proof of diagnosis.

Evidence-card consistency rules:
- every selected primary incident chunk must have the same `case_id` as the hydrated primary card;
- every selected alternative incident chunk must have a hydrated alternative card with the same `case_id`;
- this invariant is expected because incident chunks are derived from incident cards;
- if a selected incident chunk violates this invariant, assembly must fail with `PromptContextAssemblyError::InconsistentEvidence`;
- the module must not silently include alternative evidence for a card that was not hydrated.

## 7) Chunk Selection Algorithm

The module owns deterministic chunk-selection mechanics.

Role selection order:
- `EvidenceForMatch`
- `FirstCheckHint`
- `SupportingExplanation`
- `AlternativeContext`
- `MechanismExplanation`

Definitions:
- `source_index` is the zero-based index of a chunk inside its source vector before selection;
- `recognized_tags` are source `chunk_tags` parsed into `IncidentChunkTag`;
- `matching_tags` are recognized tags present in the role's configured `tag_priority`;
- `best_tag_index` is the lowest configured priority index among `matching_tags`;
- `fallback_bucket_index` is one greater than the last configured priority index.

For one incident role, a chunk is eligible when:
- it has at least one `matching_tag`; or
- it has no `matching_tag` and that role has `fallback_to_any_chunk = true`.

For one incident role, rank eligible chunks by this tuple, ascending:

```text
(
  tag_bucket_index,
  score_sort_key,
  source_index
)
```

Where:
- `tag_bucket_index = best_tag_index` when the chunk has a matching tag;
- `tag_bucket_index = fallback_bucket_index` when selected through fallback;
- `score_sort_key = -IncidentEvidenceChunk.score`, so higher scores sort first;
- `source_index` preserves retrieval order as the final tie-breaker.

Duplicate rules:
- selected `chunk_id` values should not repeat across incident prompt roles when another eligible chunk is available;
- required roles may reuse a duplicate chunk only when no distinct eligible chunk can fill the role and `fallback_to_any_chunk = true`;
- optional roles must not reuse a duplicate chunk in the current version.

Example:

```text
Role: first_check_hint
Configured tag priority:
  0: chunk_role:diagnostic_step
  1: chunk_role:investigation

Input chunks:
  A tags=[chunk_role:symptom]          score=0.95 source_index=0
  B tags=[chunk_role:diagnostic_step] score=0.70 source_index=1
  C tags=[chunk_role:investigation]   score=0.90 source_index=2

Selected order:
  B first, because diagnostic_step has priority index 0
  C second, because investigation has priority index 1
  A only through fallback, after all configured tag matches, despite higher score
```

Theory chunk ranking rules:
- theory chunks do not have tags in the current version;
- `mechanism_explanation` selection must use `TheoryEvidenceRetrievalOutput.chunks` in collection-returned order;
- selected theory chunks must preserve raw score and text;
- no theory chunks are selected when `mechanism_explanation.limit = 0`;
- when `mechanism_explanation.limit = 1`, at most one theory chunk may be selected;
- empty theory evidence is valid when `mechanism_explanation.limit = 0`;
- empty theory evidence is also valid when `mechanism_explanation.limit > 0`; the role is optional.

## 8) Role-Specific Selection Rules

### `evidence_for_match`

Source:
- `IncidentEvidenceRetrievalOutput.primary_chunks`

Tag policy:
- use `settings.chunk_packing.evidence_for_match.tag_priority`;
- apply the incident chunk ranking algorithm from section `7) Chunk Selection Algorithm`;
- output chunk tags must be the recognized `IncidentChunkTag` values parsed from selected source chunk tags.

Rules:
- select up to `settings.chunk_packing.evidence_for_match.limit` chunks;
- selected chunks must be emitted with `PromptEvidenceRole::EvidenceForMatch`;
- this role is required;
- if no chunk can be selected for this role, assembly must fail with `PromptContextAssemblyError::MissingRequiredEvidence`.

### `first_check_hint`

Source:
- `IncidentEvidenceRetrievalOutput.primary_chunks`

Tag policy:
- use `settings.chunk_packing.first_check_hint.tag_priority`;
- apply the incident chunk ranking algorithm from section `7) Chunk Selection Algorithm`;
- output chunk tags must be the recognized `IncidentChunkTag` values parsed from selected source chunk tags.

Rules:
- select up to `settings.chunk_packing.first_check_hint.limit` chunks;
- selected chunks must be emitted with `PromptEvidenceRole::FirstCheckHint`;
- this role is required;
- selection must prefer chunks not already selected for `evidence_for_match`;
- if no distinct chunk can be selected but a duplicate required-role fallback is available, the module may reuse a chunk only when `fallback_to_any_chunk = true`;
- if no chunk can be selected for this role, assembly must fail with `PromptContextAssemblyError::MissingRequiredEvidence`.

### `supporting_explanation`

Source:
- `IncidentEvidenceRetrievalOutput.primary_chunks`

Tag policy:
- use `settings.chunk_packing.supporting_explanation.tag_priority`;
- apply the incident chunk ranking algorithm from section `7) Chunk Selection Algorithm`;
- output chunk tags must be the recognized `IncidentChunkTag` values parsed from selected source chunk tags.

Rules:
- select up to `settings.chunk_packing.supporting_explanation.limit` chunks;
- selected chunks must be emitted with `PromptEvidenceRole::SupportingExplanation`;
- this role is optional at request time;
- the runtime config default must set `supporting_explanation.limit = 1`;
- when `limit = 0`, no supporting explanation chunks are selected;
- selection must prefer chunks not already selected for `evidence_for_match` or `first_check_hint`;
- if no distinct chunk can be selected, this optional role must not reuse a duplicate chunk in the current version;
- when selected, this role is intended to preserve primary-precedent nuance such as ambiguity, contributing factors, retry-path behavior, and hypothesis updates.

### `alternative_context`

Source:
- `IncidentEvidenceRetrievalOutput.alternative_chunks`

Tag policy:
- use `settings.chunk_packing.alternative_context.tag_priority`;
- apply the incident chunk ranking algorithm from section `7) Chunk Selection Algorithm` within each alternative case pool;
- output chunk tags must be the recognized `IncidentChunkTag` values parsed from selected source chunk tags.

Rules:
- select up to `settings.chunk_packing.alternative_context.limit` chunks;
- selected chunks must be emitted with `PromptEvidenceRole::AlternativeContext`;
- this role is optional;
- when `limit = 0`, no alternative context chunks are selected;
- when `cards.alternatives` is non-empty, alternative case ordering must follow `cards.alternatives[*].case_id`;
- when `cards.alternatives` is empty, alternative chunks must be considered in retrieval order;
- when `per_case_limit = Some(n)`, at most `n` chunks may be selected per `case_id`;
- alternative-context chunks must be selected using deterministic round-robin across cases.

Alternative-context round-robin algorithm:
- group eligible alternative chunks by `case_id`;
- rank chunks inside each `case_id` group using the incident chunk ranking algorithm from section `7) Chunk Selection Algorithm`;
- determine case iteration order as:
  - `cards.alternatives[*].case_id` order when hydrated alternatives are present;
  - first-seen retrieval order from `IncidentEvidenceRetrievalOutput.alternative_chunks` when `cards.alternatives` is empty;
- proceed in rounds;
- in each round, visit case groups in case iteration order;
- for each case, take the next ranked eligible chunk when:
  - the global `alternative_context.limit` has not been reached;
  - that case has not reached `per_case_limit`;
  - that case still has an unselected eligible chunk;
- skip cases that have reached `per_case_limit` or have no remaining eligible chunks;
- continue rounds until the global limit is reached or all case groups are exhausted.

### `mechanism_explanation`

Source:
- `TheoryEvidenceRetrievalOutput.chunks`

Rules:
- select at most one chunk, controlled by `settings.chunk_packing.mechanism_explanation.limit`;
- selected chunks must be emitted with `PromptEvidenceRole::MechanismExplanation`;
- this role is optional;
- when `limit = 0`, no theory chunks are selected;
- when `limit = 1`, the first retrieved theory chunk must be selected and retrieval order must be preserved.

## 9) Prompt Rendering Rules

The module must render a filled diagnostic-response prompt string from the
loaded prompt asset after selecting chunks.

The rendered prompt must contain:
- the diagnostic assistant instructions from the loaded prompt asset;
- the strict JSON response schema from the loaded prompt asset;
- the policy constraints from the loaded prompt asset;
- a `JSON context follows:` marker from the loaded prompt asset;
- a JSON context object containing the normalized request, prompt-facing normalized incident query, a compact primary-card summary, selected chunks, selected theory chunks, and policy constraints.

The prompt JSON context rendered into `{{json_context}}` must contain exactly
these top-level fields in the current version:

```json
{
  "task": "diagnostic_response",
  "user_problem": "...",
  "input_token_count": 0,
  "normalized_incident_query": {},
  "matched_incident_card": {},
  "incident_evidence_chunks": [],
  "theory_chunks": [],
  "policy_constraints": []
}
```

### JSON Context Field Mapping

`task`:
- must be the string literal `diagnostic_response`.

`user_problem`:
- must be copied from `NormalizedUserRequest.query`.

`input_token_count`:
- must be copied from `NormalizedUserRequest.input_token_count`.

`normalized_incident_query`:
- must be a prompt-facing derived object built from `QueryStructuringOutput.structured_query`;
- must not copy the internal `StructuredUserQuery` shape directly;
- must include only fields supported by the current `StructuredUserQuery` shape;
- must not include `evidence_span`, `support_level`, `rejected_nearby_terms`, or `token_usage` in the prompt context;
- must not synthesize values from `IncidentCard`, incident evidence chunks, theory chunks, or prompt instructions;
- when a prompt-facing field has no source field in the current `StructuredUserQuery` shape, it must be emitted as an empty array.

`normalized_incident_query.recognized_canonical_symptoms`:
- <- `StructuredUserQuery.symptoms[*].term`
- preserve source order;
- omit empty strings if present.

`normalized_incident_query.unmapped_user_symptoms`:
- <- empty array in the current version;
- `StructuredUserQuery.unresolved_terms` must not be treated as unmapped symptoms because unresolved terms are not symptom-specific.

`normalized_incident_query.affected_components`:
- <- `StructuredUserQuery.affected_subsystems[*].term`
- preserve source order;
- omit empty strings if present.

`normalized_incident_query.failure_mode_candidates`:
- <- `StructuredUserQuery.failure_modes[*].term`
- preserve source order;
- omit empty strings if present.

`normalized_incident_query.observed_phase`:
- <- empty array in the current version because `StructuredUserQuery` has no observed-phase field.

`normalized_incident_query.signals_present`:
- <- concatenate these existing structured-query sources in order:
  - `StructuredUserQuery.symptoms[*].term`
  - `StructuredUserQuery.triggers`
  - `StructuredUserQuery.observability_signals`
- preserve first occurrence order after de-duplicating exact string matches;
- omit empty strings if present.

`normalized_incident_query.missing_signals`:
- <- empty array in the current version because `StructuredUserQuery` has no missing-signal field;
- the module must not infer missing signals from absent fields.

Additional `StructuredUserQuery` fields:
- `intent`, `scenario`, `entities`, `constraints`, `system_properties`, `unresolved_terms`, `rejected_nearby_terms`, and `confidence` must not be included in `normalized_incident_query` in the current version unless the prompt-context asset schema is updated to require them;
- `system_properties[*].term` may be used in a future version only after the prompt-facing context schema defines a matching field.

Insufficient query-structure data rules:
- `QueryStructuringOutput.structured_query` is assumed to have passed upstream shape validation;
- this module must not fail solely because a source array used by `normalized_incident_query` is empty;
- this module must represent unavailable prompt-facing arrays as `[]`;
- this module must not fill missing values by re-reading raw prompt text, card text, chunk text, or evidence spans.

`matched_incident_card`:
- must be built from `CardHydrationOutput.primary`;
- must be a compact prompt-facing DTO, not the shared `IncidentCard` shape;
- must not include retrieval scores or candidate-card metadata.

`matched_incident_card` must contain exactly these nested objects:
- `context`
- `hypotheses`
- `checks`

`matched_incident_card.context`:
- must contain:
  - `systems`
  - `affected_components`
  - `initial_symptoms`
  - `later_symptoms`

`matched_incident_card.context.systems`:
- <- `IncidentCard.vendor_or_project` and `IncidentCard.system_type`;
- preserve first occurrence order after de-duplicating exact string matches;
- omit empty strings if present.

`matched_incident_card.context.affected_components`:
- <- `IncidentCard.affected_components`;
- preserve source order;
- omit empty strings if present.

`matched_incident_card.context.initial_symptoms`:
- <- `IncidentCard.canonical_symptoms`;
- preserve source order;
- omit empty strings if present.

`matched_incident_card.context.later_symptoms`:
- <- concatenate in order:
  - `IncidentCard.incident_phases[*].symptoms[*]`
  - `IncidentCard.incident_phases[*].user_visible_impact[*]`
- preserve first occurrence order after de-duplicating exact string matches;
- omit empty strings if present.

`matched_incident_card.hypotheses`:
- must contain:
  - `failure_modes`
  - `hypothesis_signals`
  - `hypothesis_updates`
  - `contributing_factors`

`matched_incident_card.hypotheses.failure_modes`:
- <- `IncidentCard.failure_mode_candidates`;
- preserve source order;
- omit empty strings if present.

`matched_incident_card.hypotheses.hypothesis_signals`:
- <- `IncidentCard.candidate_explanations`;
- preserve source order;
- omit empty strings if present.

`matched_incident_card.hypotheses.hypothesis_updates`:
- <- `IncidentCard.diagnostic_patterns`;
- preserve source order;
- omit empty strings if present.

`matched_incident_card.hypotheses.contributing_factors`:
- <- `IncidentCard.confidence_notes`;
- preserve source order;
- omit empty strings if present.

`matched_incident_card.checks`:
- must contain:
  - `investigation_questions`
  - `discriminating_checks`

`matched_incident_card.checks.investigation_questions`:
- <- `IncidentCard.discriminating_checks[*].question`;
- preserve source order;
- omit empty strings if present.

`matched_incident_card.checks.discriminating_checks`:
- <- `IncidentCard.investigation_steps` when non-empty;
- otherwise <- `IncidentCard.discriminating_checks[*].question`;
- preserve source order;
- omit empty strings if present.

Fields that must not appear in `matched_incident_card` in the current version:
- `case_id`
- `title`
- `source_name`
- full-card sections not explicitly mapped above

`incident_evidence_chunks`:
- must contain selected incident chunks only;
- each object must contain:
  - `role`
  - `source_document_id`
  - `chunk_tags`
  - `text`
- `role` must serialize through the prompt evidence role serialization table below;
- `source_document_id` <- selected incident chunk `case_id`;
- `chunk_tags` must serialize from `IncidentChunkTag` using canonical full tag strings such as `chunk_role:symptom`;
- raw unknown source tags must not appear in prompt context output.

Prompt evidence role serialization table:
- `PromptEvidenceRole::EvidenceForMatch` -> `evidence_for_match`
- `PromptEvidenceRole::FirstCheckHint` -> `first_check_hint`
- `PromptEvidenceRole::SupportingExplanation` -> `supporting_explanation`
- `PromptEvidenceRole::AlternativeContext` -> `alternative_context`
- `PromptEvidenceRole::MechanismExplanation` -> `mechanism_explanation`

Serialization rules:
- serialization of `PromptEvidenceRole` for prompt JSON must use the table above exactly;
- the generated implementation must use a module-private serializable DTO or helper mapping for prompt JSON rendering;
- `PromptEvidenceRole` itself must not be made serializable solely for prompt rendering.

`theory_chunks`:
- must contain selected theory chunks only;
- each object must contain:
  - `role`
  - `source_document_id`
  - `text`
- `role` must serialize through the prompt evidence role serialization table above.
- `source_document_id` <- selected theory chunk `chunk_id`

`policy_constraints`:
- must be copied from `DiagnosticResponsePromptAsset.policy_constraints`.

Rendering rules:
- prompt rendering must be deterministic for identical inputs and settings;
- the rendered prompt must be plain UTF-8 text;
- the rendered prompt must not be split into provider-specific system/user messages in the current version;
- the rendered JSON context must be built through structured serialization such as `serde_json`, not through `Debug` formatting or ad hoc string concatenation of JSON fragments;
- all module-private JSON-context DTOs must use explicit serde field renaming or explicit field names so rendered JSON field names are the snake-case names required by this section;
- all module-private enum DTOs used for prompt rendering must use explicit serde renaming or explicit string conversion so rendered enum values are the snake-case string literals required by this section;
- any generated implementation may use module-private serializable DTOs for prompt rendering when shared runtime types do not directly implement `serde::Serialize`;
- the rendered prompt must be produced by replacing the loaded template's `{{json_context}}` placeholder exactly once;
- the rendered prompt must preserve uncertainty instructions when `AlternativeContext` chunks are present.

## 10) Output Assembly Rules

The module must return `PromptContextAssemblyOutput` after rendering the prompt.

Output rules:
- `PromptContextAssemblyOutput.prompt` must contain the rendered prompt string;
- `PromptContextAssemblyOutput.incident_evidence_chunks` must contain selected incident chunks only;
- `PromptContextAssemblyOutput.theory_chunks` must contain selected theory chunks only;
- selected incident chunks must be emitted in role order: `EvidenceForMatch`, `FirstCheckHint`, `SupportingExplanation`, then `AlternativeContext`;
- returned selected chunks must be exactly the same chunks represented inside the rendered prompt context;
- returned selected chunks are intended for history, tracing, and later diagnostic state;
- the OpenInference input/output payload contract for
  `assemble_with_context(...)` is owned by
  `Specification/runtime/observability/open_inference_spans.md`;
- the prompt string is the input intended for the next model-generation module.

## 11) Error Boundary

The generated Rust module must define a public error enum equivalent in
ownership to:

```rust
#[derive(Debug, thiserror::Error)]
pub enum PromptContextAssemblyError {
    #[error("invalid settings: {0}")]
    InvalidSettings(&'static str),
    #[error("prompt asset error: {0}")]
    PromptAsset(String),
    #[error("missing hydrated primary card")]
    MissingPrimaryCard,
    #[error("missing required evidence for role: {role:?}")]
    MissingRequiredEvidence { role: PromptEvidenceRole },
    #[error("inconsistent evidence data: {0}")]
    InconsistentEvidence(String),
}
```

Error rules:
- constructor validation failures must return `PromptContextAssemblyError::InvalidSettings`;
- prompt asset read, parse, or validation failures must return `PromptContextAssemblyError::PromptAsset`;
- missing primary card must return `PromptContextAssemblyError::MissingPrimaryCard`;
- missing required evidence for `EvidenceForMatch` or `FirstCheckHint` must return `PromptContextAssemblyError::MissingRequiredEvidence`;
- selected evidence chunk case/card mismatches must return `PromptContextAssemblyError::InconsistentEvidence`;
- missing alternative context is not an error;
- missing theory evidence is not an error.

## 12) Tests

Detailed unit-test requirements for this module must be defined in:
- `Specification/runtime/unit_tests.md`
