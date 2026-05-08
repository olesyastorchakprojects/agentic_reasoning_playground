## 1) Purpose / Scope

This document defines the runtime leaf-module contract for
`diagnostic_update_prompt_context_assembly`.

Its task:
- assemble continuation-oriented prompt context.

This document is the source of truth for:
- the `diagnostic_update_prompt_context_assembly` leaf-module boundary;
- the module public interface;
- the module-owned deterministic chunk-selection mechanics;
- the prompt asset contract consumed by this module;
- the prompt-facing JSON context rendered by this module;
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
- model response validation or normalization.

Shared request and response types are defined by:
- `Specification/runtime/runtime.md`

OpenInference span behavior for the context-aware execution path is defined by:
- `Specification/runtime/observability/open_inference_spans.md`

The generated Rust module file for the current version is:
- `src/request_pipeline/diagnostic_update_prompt_context_assembly.rs`

## 2) Required Shared Types

This module must use the shared runtime input types:
- `Context`
- `ProblemUnderstanding`
- `ResolvedObservation`
- `ObservationExtractionOutput`
- `ExtractedObservation`
- `ObservationPolarity`
- `HydratedCardBranchesInput`
- `IncidentCard`
- `IncidentEvidenceChunk`
- `IncidentEvidenceRetrievalOutput`
- `TheoryEvidenceChunk`
- `TheoryEvidenceRetrievalOutput`
- `TrackedHypothesis`
- `SuggestedCheck`
- `HypothesisId`
- `HypothesisStatus`
- `EvidenceTopology`
- `PromptContextAssemblyOutput`

This module must use the shared runtime enum:
- `IncidentChunkTag`

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
- `DiagnosticUpdatePromptContextSettings`, `ChunkRolePackingSettings`, and `ChunkPackingSource` must be imported from `crate::config`.

Shared-type placement rule:
- before code generation for this module, all shared prompt-context types listed in this section must exist in `src/shared_types/mod.rs`.

## 3) Settings Dependency

This module must receive the typed settings slice:
- `DiagnosticUpdatePromptContextSettings`

`DiagnosticUpdatePromptContextSettings` is defined at the crate-level runtime boundary in:
- `Specification/runtime/runtime.md`

Settings rules:
- this module must receive `DiagnosticUpdatePromptContextSettings` through its constructor;
- this module must not read raw TOML or raw environment variables directly;
- `prompt_asset_path` is the runtime-owned absolute JSON prompt asset path selected by config;
- chunk role limits must be read from `DiagnosticUpdatePromptContextSettings`;
- chunk role tag priorities must be read from `DiagnosticUpdatePromptContextSettings`;
- chunk role fallback behavior must be read from `DiagnosticUpdatePromptContextSettings`;
- the module must not hardcode incident tag priority lists or chunk limits;
- the module owns the deterministic selection algorithm.

Constructor settings validation rules:
- `settings.prompt_asset_path` must be an absolute path;
- `settings.chunk_packing.evidence_for_match.source` must be `ChunkPackingSource::PrimaryIncident`;
- `settings.chunk_packing.next_check_hint.source` must be `ChunkPackingSource::PrimaryIncident`;
- `settings.chunk_packing.supporting_explanation.source` must be `ChunkPackingSource::PrimaryIncident`;
- `settings.chunk_packing.alternative_context.source` must be `ChunkPackingSource::AlternativeIncident`;
- `settings.chunk_packing.mechanism_explanation.source` must be `ChunkPackingSource::Theory`;
- `settings.chunk_packing.evidence_for_match.limit` must be greater than or equal to `1`;
- `settings.chunk_packing.next_check_hint.limit` must be greater than or equal to `1`;
- `settings.chunk_packing.supporting_explanation.limit` may be `0`;
- `settings.chunk_packing.alternative_context.limit` may be `0`;
- `settings.chunk_packing.mechanism_explanation.limit` may be `0`;
- `settings.chunk_packing.mechanism_explanation.limit` must be less than or equal to `1`;
- `settings.chunk_packing.alternative_context.per_case_limit` must be `Some(n)` with `n > 0` when `alternative_context.limit > 0`;
- when `settings.chunk_packing.alternative_context.limit = 0`, `per_case_limit` may be `None` or any `Some(n)` value;
- `settings.chunk_packing.mechanism_explanation.tag_priority` must be empty because theory chunks do not expose tags.

## 4) Prompt Asset

This module must load a prompt asset from:
- `DiagnosticUpdatePromptContextSettings.prompt_asset_path`

The schema for prompt assets must live in the same directory as the configured
prompt asset. The schema path is derived from `prompt_asset_path` by using the
same parent directory and replacing only the file name:
- if the prompt asset file name ends with `.manual_test.json`, replace that suffix with `.schema.json`;
- otherwise, if the prompt asset file name ends with `.json`, replace that suffix with `.schema.json`.

The generated runtime prompt asset type must be equivalent in ownership to:

```rust
#[derive(Debug, serde::Deserialize)]
struct DiagnosticUpdateResponsePromptAsset {
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
- `DiagnosticUpdateResponsePromptAsset` is module-private to `diagnostic_update_prompt_context_assembly`;
- `DiagnosticUpdateResponsePromptAsset` must not be re-exported from `diagnostic_update_prompt_context_assembly`;
- `DiagnosticUpdateResponsePromptAsset` must not be defined or re-exported from `crate::shared_types`;
- `new(...)` must read the prompt asset from `settings.prompt_asset_path`;
- `new(...)` must derive the prompt asset schema path from `settings.prompt_asset_path`;
- `new(...)` must fail when `prompt_asset_path` is empty;
- `new(...)` must fail when `prompt_asset_path` is not absolute;
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
- the module must not hardcode the diagnostic-update prompt template;
- the module must render the prompt by replacing `{{json_context}}` with the serialized JSON context object.

## 5) Public Interface

The generated Rust module must define a public module boundary equivalent in
ownership to:

```rust
pub struct DiagnosticUpdatePromptContextAssembly {
    // implementation-owned fields
}

impl DiagnosticUpdatePromptContextAssembly {
    pub fn new(
        settings: DiagnosticUpdatePromptContextSettings,
    ) -> Result<Self, DiagnosticUpdatePromptContextAssemblyError>;

    pub fn assemble(
        &self,
        problem_understanding: &ProblemUnderstanding,
        resolved_observation: &ResolvedObservation,
        extracted_observations: &ObservationExtractionOutput,
        cards: &HydratedCardBranchesInput,
        incident_evidence: &IncidentEvidenceRetrievalOutput,
        theory_evidence: &TheoryEvidenceRetrievalOutput,
        active_hypotheses: &[TrackedHypothesis],
        rejected_hypotheses: &[TrackedHypothesis],
        last_check: Option<&SuggestedCheck>,
    ) -> Result<PromptContextAssemblyOutput, DiagnosticUpdatePromptContextAssemblyError>;

    pub fn assemble_with_context(
        &self,
        problem_understanding: &ProblemUnderstanding,
        resolved_observation: &ResolvedObservation,
        extracted_observations: &ObservationExtractionOutput,
        cards: &HydratedCardBranchesInput,
        incident_evidence: &IncidentEvidenceRetrievalOutput,
        theory_evidence: &TheoryEvidenceRetrievalOutput,
        active_hypotheses: &[TrackedHypothesis],
        rejected_hypotheses: &[TrackedHypothesis],
        last_check: Option<&SuggestedCheck>,
        context: &Context,
    ) -> Result<PromptContextAssemblyOutput, DiagnosticUpdatePromptContextAssemblyError>;
}
```

For the current version, the implementation-owned fields must contain exactly:
- `settings: DiagnosticUpdatePromptContextSettings`
- `prompt_asset: DiagnosticUpdateResponsePromptAsset`

Rules:
- `new(...)` must validate constructor-owned settings and retain settings for reuse;
- `new(...)` must load and validate the configured prompt asset;
- `new(...)` constructor validation failures must be returned through `DiagnosticUpdatePromptContextAssemblyError`;
- `assemble(...)` must delegate to `assemble_with_context(...)` with `&Context::noop()`;
- `assemble_with_context(...)` is the context-aware request-time entrypoint used by the orchestrator;
- `assemble_with_context(...)` must treat `context.open_inference.root_span` as the parent span for the module-owned OpenInference chain span `oi.chain.diagnostic_update_prompt_context_assembly`;
- `assemble(...)` must not mutate any input;
- `assemble(...)` must not call external services;
- `assemble(...)` must render the filled diagnostic-update prompt string;
- `assemble(...)` must not invoke the model or construct provider-specific message objects.

## 6) Chunk Roles

This module must use exactly these prompt evidence roles:
- `evidence_for_match`
- `next_check_hint`
- `supporting_explanation`
- `alternative_context`
- `mechanism_explanation`

Role-to-source rules:
- `evidence_for_match` selects from primary incident evidence only;
- `next_check_hint` selects from primary incident evidence only;
- `supporting_explanation` selects from primary incident evidence only;
- `alternative_context` selects from alternative incident evidence only;
- `mechanism_explanation` selects from theory evidence only.

Shared enum mapping rules:
- the shared `PromptEvidenceRole` enum is reused from `crate::shared_types`;
- the role names above map to shared enum variants as follows:
  - `evidence_for_match` → `PromptEvidenceRole::EvidenceForMatch`
  - `next_check_hint` → `PromptEvidenceRole::FirstCheckHint`
  - `supporting_explanation` → `PromptEvidenceRole::SupportingExplanation`
  - `alternative_context` → `PromptEvidenceRole::AlternativeContext`
  - `mechanism_explanation` → `PromptEvidenceRole::MechanismExplanation`
- `PromptEvidenceRole::FirstCheckHint` must serialize as `"next_check_hint"` in this module's prompt-facing JSON context (not `"first_check_hint"`);
- this module-private mapping must be implemented via a private DTO or match expression and must not modify the shared enum.

## 7) Prompt Context JSON Shape

The prompt JSON context rendered into `{{json_context}}` must contain only the
following top-level fields:

```json
{
  "problem_understanding": "string",
  "resolved_observation": {
    "text": "string"
  },
  "observations": [
    {
      "statement": "string",
      "polarity": "present|absent|corrected",
      "condition": "string",
      "time_relation": "string"
    }
  ],
  "diagnostic_state": {
    "active_hypotheses": [
      {
        "hypothesis_id": "uuid-string",
        "text": "string"
      }
    ],
    "rejected_hypotheses": [
      {
        "hypothesis_id": "uuid-string",
        "text": "string",
        "rejection_reason": "string"
      }
    ],
    "last_check": {
      "text": "string"
    }
  },
  "primary_incident_card": {
    "context": {
      "systems": ["string"],
      "affected_components": ["string"],
      "initial_symptoms": ["string"],
      "later_symptoms": ["string"]
    },
    "hypotheses": {
      "failure_modes": ["string"],
      "hypothesis_signals": ["string"],
      "hypothesis_updates": ["string"],
      "contributing_factors": ["string"]
    },
    "checks": {
      "investigation_questions": ["string"],
      "discriminating_checks": ["string"]
    }
  },
  "incident_evidence": {
    "evidence_for_match": ["string"],
    "next_check_hint": ["string"],
    "supporting_explanation": ["string"],
    "alternative_context": ["string"]
  },
  "theory_evidence": {
    "mechanism_explanation": ["string"]
  }
}
```

Shape rules:
- `problem_understanding` must be serialized as a string;
- `resolved_observation.text` must be serialized as a string;
- `observations[*].statement` and `observations[*].polarity` must be serialized as strings;
- `observations[*].condition` and `observations[*].time_relation` must be serialized as strings only when non-empty;
- `diagnostic_state.active_hypotheses[*].hypothesis_id` must be serialized as a UUID string;
- `diagnostic_state.rejected_hypotheses[*].hypothesis_id` must be serialized as a UUID string;
- `incident_evidence` role buckets must contain only chunk text strings;
- `theory_evidence.mechanism_explanation` must contain only chunk text strings;
- `primary_incident_card` must follow the existing prompt-facing compact incident-card shape defined for prompt assembly;
- prompt-facing JSON must not include score fields, chunk ids, card ids, tag strings, token counts, confidence values for active hypotheses, raw `RunState`, or internal step metadata.

Presence rules:
- empty top-level fields must not be included in the rendered JSON;
- empty nested objects must not be included in the rendered JSON;
- empty arrays must not be included in the rendered JSON.

## 8) Field Mapping Rules

The rendered `json_context` must be constructed using these mapping rules.

`problem_understanding`:
- must be copied from `problem_understanding.text`;
- `problem_understanding.text` must be `Some(non-empty string)` after trimming;
- if `problem_understanding.text` is `None` or empty after trimming, assembly must fail.

`resolved_observation`:
- `resolved_observation.text` <- trimmed standalone observation text from `ResolvedObservation`.

`observations`:
- must be built from `ObservationExtractionOutput.observations` in original order;
- `statement` <- trimmed `ExtractedObservation.statement`;
- `polarity` <- required snake-case string literal mapped from `ObservationPolarity`;
- `condition` <- trimmed `ExtractedObservation.condition` when present and non-empty;
- `time_relation` <- trimmed `ExtractedObservation.time_relation` when present and non-empty.

`diagnostic_state.active_hypotheses`:
- must be built from `active_hypotheses` in original input order;
- `hypothesis_id` <- UUID string from `TrackedHypothesis.hypothesis_id`;
- `text` <- trimmed `TrackedHypothesis.text`;
- the latest `HypothesisState` of each `TrackedHypothesis` must have status `Active` or `Weakened`.

`diagnostic_state.rejected_hypotheses`:
- must be built from `rejected_hypotheses` in original input order;
- `hypothesis_id` <- UUID string from `TrackedHypothesis.hypothesis_id`;
- `text` <- trimmed `TrackedHypothesis.text`;
- `rejection_reason` <- trimmed payload string from the latest `HypothesisState.status = Rejected(reason)`.

`diagnostic_state.last_check`:
- when `last_check` is `Some(check)`, `text` <- trimmed `SuggestedCheck.text`;
- when `last_check` is `None`, the `last_check` object must not be included.

`primary_incident_card`:
- must be derived from `cards.primary`;
- the module must derive a compact prompt-facing `primary_incident_card` object from that card rather than embedding the full shared `IncidentCard` payload.
- `primary_incident_card` must contain exactly these nested objects:
  - `context`
  - `hypotheses`
  - `checks`
- `primary_incident_card.context` must contain:
  - `systems`
  - `affected_components`
  - `initial_symptoms`
  - `later_symptoms`
- `primary_incident_card.context.systems` <- `IncidentCard.vendor_or_project` and `IncidentCard.system_type`; preserve first occurrence order after de-duplicating exact string matches; omit empty strings if present.
- `primary_incident_card.context.affected_components` <- `IncidentCard.affected_components`; preserve source order; omit empty strings if present.
- `primary_incident_card.context.initial_symptoms` <- `IncidentCard.canonical_symptoms`; preserve source order; omit empty strings if present.
- `primary_incident_card.context.later_symptoms` <- concatenate in order `IncidentCard.incident_phases[*].symptoms[*]`, then `IncidentCard.incident_phases[*].user_visible_impact[*]`; preserve first occurrence order after de-duplicating exact string matches; omit empty strings if present.
- `primary_incident_card.hypotheses` must contain:
  - `failure_modes`
  - `hypothesis_signals`
  - `hypothesis_updates`
  - `contributing_factors`
- `primary_incident_card.hypotheses.failure_modes` <- `IncidentCard.failure_mode_candidates`; preserve source order; omit empty strings if present.
- `primary_incident_card.hypotheses.hypothesis_signals` <- `IncidentCard.candidate_explanations`; preserve source order; omit empty strings if present.
- `primary_incident_card.hypotheses.hypothesis_updates` <- `IncidentCard.diagnostic_patterns`; preserve source order; omit empty strings if present.
- `primary_incident_card.hypotheses.contributing_factors` <- `IncidentCard.confidence_notes`; preserve source order; omit empty strings if present.
- `primary_incident_card.checks` must contain:
  - `investigation_questions`
  - `discriminating_checks`
- `primary_incident_card.checks.investigation_questions` <- `IncidentCard.discriminating_checks[*].question`; preserve source order; omit empty strings if present.
- `primary_incident_card.checks.discriminating_checks` <- `IncidentCard.investigation_steps` when non-empty; otherwise <- `IncidentCard.discriminating_checks[*].question`; preserve source order; omit empty strings if present.

`incident_evidence`:
- `evidence_for_match` <- selected primary incident chunks assigned `PromptEvidenceRole::EvidenceForMatch`, serialized as trimmed chunk text only;
- `next_check_hint` <- selected primary incident chunks assigned `PromptEvidenceRole::FirstCheckHint`, serialized as trimmed chunk text only;
- `supporting_explanation` <- selected primary incident chunks assigned `PromptEvidenceRole::SupportingExplanation`, serialized as trimmed chunk text only;
- `alternative_context` <- selected alternative incident chunks assigned `PromptEvidenceRole::AlternativeContext`, serialized as trimmed chunk text only.

`theory_evidence`:
- `mechanism_explanation` <- selected theory chunks assigned `PromptEvidenceRole::MechanismExplanation`, serialized as trimmed chunk text only.

## 9) Chunk Selection Rules

The module must select evidence deterministically.

Primary incident selection rules:
- candidate source for `evidence_for_match` is `incident_evidence.primary_chunks` only;
- candidate source for `next_check_hint` is `incident_evidence.primary_chunks` only;
- candidate source for `supporting_explanation` is `incident_evidence.primary_chunks` only.

Alternative incident selection rules:
- candidate source for `alternative_context` is `incident_evidence.alternative_chunks` only.

Theory selection rules:
- candidate source for `mechanism_explanation` is `theory_evidence.chunks` only.

Per-role selection rules:
- for each role, the module must filter candidate chunks by the configured tag-priority list for that role;
- within the same role, chunks with higher-priority tags must be considered before chunks with lower-priority tags;
- when multiple chunks have the same highest matching tag priority, preserve original retrieval order;
- each role must stop selecting chunks once its configured limit is reached;
- for `alternative_context`, `per_case_limit` must be enforced after tag-priority ordering and before final truncation to the role limit;
- a chunk already selected for one role may also be selected for another role in the current version if it independently satisfies that role's rules.

Serialization rules for selected chunks:
- prompt-facing JSON must serialize only the selected chunk text;
- `PromptContextAssemblyOutput.incident_evidence_chunks` and `PromptContextAssemblyOutput.theory_chunks` must preserve full traceability metadata for the selected chunks.

Evidence-topology rules:
- `PromptContextAssemblyOutput.evidence_topology.primary_evidence_roles` must contain the populated primary role bucket names in this order when present:
  - `evidence_for_match`
  - `next_check_hint`
  - `supporting_explanation`
- `PromptContextAssemblyOutput.evidence_topology.alternative_context_present` must be `true` when at least one `alternative_context` chunk was selected;
- `PromptContextAssemblyOutput.evidence_topology.alternative_context_case_ids` must contain the first-seen unique `case_id` values of selected alternative-context chunks in preserved order;
- `PromptContextAssemblyOutput.evidence_topology.theory_evidence_present` must be `true` when at least one theory chunk was selected.

## 10) Rendering Rules

The module must render the final prompt by:
1. building the prompt-facing JSON context object;
2. serializing the JSON context object as compact valid JSON;
3. replacing the configured `{{json_context}}` placeholder exactly once in the loaded template;
4. attaching `prompt_asset.response_schema.clone()` to `PromptContextAssemblyOutput.response_schema`;
5. attaching the computed `EvidenceTopology` summary to `PromptContextAssemblyOutput.evidence_topology`.

Rendering rules:
- the rendered prompt must be produced by replacing the loaded template's `{{json_context}}` placeholder exactly once;
- the module must not perform any other placeholder substitution in the current version;
- the module must not pretty-print the JSON context in the current version;
- the module must preserve all other template text exactly as loaded from the prompt asset.

## 11) Error Boundary

This module must define a module-owned direct error type equivalent in
ownership to:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
pub enum DiagnosticUpdatePromptContextAssemblyError {
    #[error("invalid settings: {0}")]
    InvalidSettings(String),
    #[error("prompt asset read failed: {0}")]
    PromptAssetReadFailed(String),
    #[error("prompt asset schema read failed: {0}")]
    PromptAssetSchemaReadFailed(String),
    #[error("invalid prompt asset json: {0}")]
    InvalidPromptAssetJson(String),
    #[error("invalid prompt asset schema json: {0}")]
    InvalidPromptAssetSchemaJson(String),
    #[error("prompt asset schema validation failed: {0}")]
    PromptAssetSchemaValidationFailed(String),
    #[error("invalid prompt asset contract: {0}")]
    InvalidPromptAssetContract(String),
    #[error("invalid problem understanding")]
    InvalidProblemUnderstanding,
    #[error("invalid resolved observation")]
    InvalidResolvedObservation,
    #[error("invalid hypothesis state: {0}")]
    InvalidHypothesisState(String),
    #[error("json serialization failed: {0}")]
    JsonSerializationFailed(String),
}
```

Error rules:
- invalid constructor settings must return `InvalidSettings`;
- missing or unreadable prompt files must return prompt-asset read errors;
- invalid prompt-asset JSON or schema JSON must return typed parse errors;
- asset-schema validation failures must return `PromptAssetSchemaValidationFailed`;
- `problem_understanding.text = None` or empty after trimming must return `InvalidProblemUnderstanding`;
- empty resolved standalone observation text must return `InvalidResolvedObservation`;
- an active hypothesis input whose latest state is `Rejected(_)` must return `InvalidHypothesisState`;
- a rejected hypothesis input whose latest state is not `Rejected(_)` must return `InvalidHypothesisState`;
- serialization failure while rendering `json_context` must return `JsonSerializationFailed`;
- the module must not panic on invalid input or invalid prompt assets.

## 12) Unit Tests

Unit-test requirements are defined by:
- `Specification/runtime/unit_tests.md`
