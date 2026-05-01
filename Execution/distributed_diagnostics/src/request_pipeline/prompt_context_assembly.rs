use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use crate::config::{ChunkPackingSource, ChunkRolePackingSettings, PromptContextSettings};
use crate::shared_types::{
    CardHydrationOutput, Context, IncidentCard, IncidentChunkTag,
    IncidentEvidenceChunk, IncidentEvidenceRetrievalOutput, NormalizedUserRequest,
    PromptContextAssemblyOutput, PromptEvidenceRole, PromptIncidentEvidenceChunk,
    PromptTheoryEvidenceChunk, QueryStructuringOutput, StructuredUserQuery,
    TheoryEvidenceRetrievalOutput,
};
use tracing::{field, info_span};

// ---------------------------------------------------------------------------
// Error boundary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum PromptContextAssemblyError {
    #[error("invalid settings: {0}")]
    InvalidSettings(String),
    #[error("prompt asset error: {0}")]
    PromptAsset(String),
    #[error("missing hydrated primary card")]
    MissingPrimaryCard,
    #[error("missing required evidence for role: {role:?}")]
    MissingRequiredEvidence { role: PromptEvidenceRole },
    #[error("inconsistent evidence data: {0}")]
    InconsistentEvidence(String),
}

// ---------------------------------------------------------------------------
// Module-private prompt asset type
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct DiagnosticResponsePromptAsset {
    #[allow(dead_code)]
    version: String,
    #[allow(dead_code)]
    name: String,
    template: String,
    context_placeholder: String,
    required_placeholders: Vec<String>,
    response_schema: serde_json::Value,
    policy_constraints: Vec<String>,
}

// ---------------------------------------------------------------------------
// Module-private serialization DTOs
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct JsonContext<'a> {
    task: String,
    user_problem: &'a str,
    input_token_count: usize,
    normalized_incident_query: NormalizedIncidentQueryDto,
    matched_incident_card: MatchedIncidentCardDto,
    incident_evidence_chunks: Vec<IncidentChunkDto>,
    theory_chunks: Vec<TheoryChunkDto>,
    policy_constraints: &'a Vec<String>,
}

#[derive(serde::Serialize)]
struct NormalizedIncidentQueryDto {
    recognized_canonical_symptoms: Vec<String>,
    unmapped_user_symptoms: Vec<String>,
    affected_components: Vec<String>,
    failure_mode_candidates: Vec<String>,
    observed_phase: Vec<String>,
    signals_present: Vec<String>,
    missing_signals: Vec<String>,
}

#[derive(serde::Serialize)]
struct IncidentChunkDto {
    role: String,
    source_document_id: String,
    chunk_tags: Vec<String>,
    text: String,
}

#[derive(serde::Serialize)]
struct MatchedIncidentCardDto {
    context: MatchedIncidentContextDto,
    hypotheses: MatchedIncidentHypothesesDto,
    checks: MatchedIncidentChecksDto,
}

#[derive(serde::Serialize)]
struct MatchedIncidentContextDto {
    systems: Vec<String>,
    affected_components: Vec<String>,
    initial_symptoms: Vec<String>,
    later_symptoms: Vec<String>,
}

#[derive(serde::Serialize)]
struct MatchedIncidentHypothesesDto {
    failure_modes: Vec<String>,
    hypothesis_signals: Vec<String>,
    hypothesis_updates: Vec<String>,
    contributing_factors: Vec<String>,
}

#[derive(serde::Serialize)]
struct MatchedIncidentChecksDto {
    investigation_questions: Vec<String>,
    discriminating_checks: Vec<String>,
}

#[derive(serde::Serialize)]
struct TheoryChunkDto {
    role: String,
    source_document_id: String,
    text: String,
}

// ---------------------------------------------------------------------------
// Role serialization helper
// ---------------------------------------------------------------------------

fn role_to_str(role: PromptEvidenceRole) -> &'static str {
    match role {
        PromptEvidenceRole::EvidenceForMatch => "evidence_for_match",
        PromptEvidenceRole::FirstCheckHint => "first_check_hint",
        PromptEvidenceRole::SupportingExplanation => "supporting_explanation",
        PromptEvidenceRole::AlternativeContext => "alternative_context",
        PromptEvidenceRole::MechanismExplanation => "mechanism_explanation",
    }
}

// ---------------------------------------------------------------------------
// Public struct
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct PromptContextAssembly {
    settings: PromptContextSettings,
    prompt_asset: DiagnosticResponsePromptAsset,
}

impl PromptContextAssembly {
    pub fn new(settings: PromptContextSettings) -> Result<Self, PromptContextAssemblyError> {
        validate_settings(&settings)?;
        let prompt_asset = load_prompt_asset(&settings.prompt_asset_path)?;
        Ok(Self {
            settings,
            prompt_asset,
        })
    }

    pub fn assemble(
        &self,
        request: &NormalizedUserRequest,
        query: &QueryStructuringOutput,
        cards: &CardHydrationOutput,
        incident_evidence: &IncidentEvidenceRetrievalOutput,
        theory_evidence: &TheoryEvidenceRetrievalOutput,
    ) -> Result<PromptContextAssemblyOutput, PromptContextAssemblyError> {
        self.assemble_with_context(
            request,
            query,
            cards,
            incident_evidence,
            theory_evidence,
            &Context::noop(),
        )
    }

    pub fn assemble_with_context(
        &self,
        request: &NormalizedUserRequest,
        query: &QueryStructuringOutput,
        cards: &CardHydrationOutput,
        incident_evidence: &IncidentEvidenceRetrievalOutput,
        theory_evidence: &TheoryEvidenceRetrievalOutput,
        context: &Context,
    ) -> Result<PromptContextAssemblyOutput, PromptContextAssemblyError> {
        let primary_card_present = cards.primary.is_some();
        let primary_card_case_id = cards
            .primary
            .as_ref()
            .map(|card| card.case_id.as_str())
            .unwrap_or("");
        let alternative_card_case_ids: Vec<&str> =
            cards.alternatives.iter().map(|card| card.case_id.as_str()).collect();
        let primary_incident_chunk_ids: Vec<&str> = incident_evidence
            .primary_chunks
            .iter()
            .map(|chunk| chunk.chunk_id.as_str())
            .collect();
        let alternative_incident_chunk_ids: Vec<&str> = incident_evidence
            .alternative_chunks
            .iter()
            .map(|chunk| chunk.chunk_id.as_str())
            .collect();
        let theory_chunk_ids: Vec<&str> = theory_evidence
            .chunks
            .iter()
            .map(|chunk| chunk.chunk_id.as_str())
            .collect();
        let structured_query_json = serde_json::to_string(&query.structured_query)
            .unwrap_or_else(|_| "{}".to_string());
        let oi_span =
            crate::observability::oi_chain_prompt_context_assembly_span(&context.open_inference.root_span);
        let oi_input_json = serde_json::json!({
            "normalized_query": request.query,
            "structured_query": query.structured_query,
            "primary_card_present": primary_card_present,
            "alternative_cards_count": cards.alternatives.len(),
            "primary_incident_chunks_count": incident_evidence.primary_chunks.len(),
            "alternative_incident_chunks_count": incident_evidence.alternative_chunks.len(),
            "theory_chunks_count": theory_evidence.chunks.len()
        })
        .to_string();
        oi_span.record("input.value", oi_input_json.as_str());
        oi_span.record("input.mime_type", "application/json");

        let span = info_span!(
            "request_pipeline.prompt_context_assembly",
            module.name = "prompt_context_assembly",
            query.normalized = %request.query,
            prompt.asset.name = %self.prompt_asset.name,
            prompt.asset.version = %self.prompt_asset.version,
            prompt.asset.policy_constraints_count = self.prompt_asset.policy_constraints.len() as i64,
            prompt.input.primary_card_present = primary_card_present,
            prompt.input.primary_card.case_id = primary_card_case_id,
            prompt.input.alternative_cards_count = cards.alternatives.len() as i64,
            prompt.input.primary_incident_chunks_count = incident_evidence.primary_chunks.len() as i64,
            prompt.input.alternative_incident_chunks_count = incident_evidence.alternative_chunks.len() as i64,
            prompt.input.theory_chunks_count = theory_evidence.chunks.len() as i64,
            prompt.selected.total_chunks_count = field::Empty,
            prompt.selected.evidence_for_match.count = field::Empty,
            prompt.selected.first_check_hint.count = field::Empty,
            prompt.selected.supporting_explanation.count = field::Empty,
            prompt.selected.alternative_context.count = field::Empty,
            prompt.selected.mechanism_explanation.count = field::Empty,
            prompt.rendered_chars = field::Empty,
            prompt.context_json_chars = field::Empty,
            module.outcome = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        let _guard = span.enter();
        tracing::event!(
            tracing::Level::INFO,
            event.name = "prompt_structured_query_payload",
            structured_query.json = %structured_query_json
        );
        tracing::event!(
            tracing::Level::INFO,
            event.name = "prompt_input_alternative_card_case_ids",
            prompt.input.alternative_card.case_ids = %serde_json::to_string(&alternative_card_case_ids)
                .unwrap_or_else(|_| "[]".to_string())
        );
        tracing::event!(
            tracing::Level::INFO,
            event.name = "prompt_input_primary_incident_chunk_ids",
            prompt.input.primary_incident_chunk_ids = %serde_json::to_string(&primary_incident_chunk_ids)
                .unwrap_or_else(|_| "[]".to_string())
        );
        tracing::event!(
            tracing::Level::INFO,
            event.name = "prompt_input_alternative_incident_chunk_ids",
            prompt.input.alternative_incident_chunk_ids = %serde_json::to_string(&alternative_incident_chunk_ids)
                .unwrap_or_else(|_| "[]".to_string())
        );
        tracing::event!(
            tracing::Level::INFO,
            event.name = "prompt_input_theory_chunk_ids",
            prompt.input.theory_chunk_ids = %serde_json::to_string(&theory_chunk_ids)
                .unwrap_or_else(|_| "[]".to_string())
        );

        let primary_card = cards
            .primary
            .as_ref()
            .ok_or_else(|| {
                record_prompt_context_error(
                    &span,
                    "PromptContextAssembly.MissingPrimaryCard",
                    "missing hydrated primary card",
                );
                crate::observability::record_error(
                    &oi_span,
                    "PromptContextAssembly.MissingPrimaryCard",
                    "missing hydrated primary card",
                );
                PromptContextAssemblyError::MissingPrimaryCard
            })?;

        let mut already_selected: HashSet<String> = HashSet::new();

        let efm = select_primary_role(
            &incident_evidence.primary_chunks,
            &self.settings.chunk_packing.evidence_for_match,
            &mut already_selected,
            PromptEvidenceRole::EvidenceForMatch,
            true,
        )
        .map_err(|e| {
            record_prompt_context_error_for_err(&span, &e);
            crate::observability::record_error(
                &oi_span,
                "PromptContextAssembly.MissingRequiredEvidence",
                &e.to_string(),
            );
            e
        })?;

        let fch = select_primary_role(
            &incident_evidence.primary_chunks,
            &self.settings.chunk_packing.first_check_hint,
            &mut already_selected,
            PromptEvidenceRole::FirstCheckHint,
            true,
        )
        .map_err(|e| {
            record_prompt_context_error_for_err(&span, &e);
            crate::observability::record_error(
                &oi_span,
                "PromptContextAssembly.MissingRequiredEvidence",
                &e.to_string(),
            );
            e
        })?;

        let se = select_primary_role(
            &incident_evidence.primary_chunks,
            &self.settings.chunk_packing.supporting_explanation,
            &mut already_selected,
            PromptEvidenceRole::SupportingExplanation,
            false,
        )
        .map_err(|e| {
            record_prompt_context_error_for_err(&span, &e);
            crate::observability::record_error(
                &oi_span,
                "PromptContextAssembly.InvalidSettings",
                &e.to_string(),
            );
            e
        })?;

        let ac = select_alternative_context(
            &incident_evidence.alternative_chunks,
            &self.settings.chunk_packing.alternative_context,
            &cards.alternatives,
            &already_selected,
        );

        let theory_limit = self.settings.chunk_packing.mechanism_explanation.limit;
        let theory_chunks: Vec<PromptTheoryEvidenceChunk> = theory_evidence
            .chunks
            .iter()
            .take(theory_limit)
            .map(|c| PromptTheoryEvidenceChunk {
                role: PromptEvidenceRole::MechanismExplanation,
                chunk_id: c.chunk_id.clone(),
                score: c.score,
                text: c.text.clone(),
            })
            .collect();

        // Consistency checks
        let primary_case_id = &primary_card.case_id;
        for chunk in efm.iter().chain(fch.iter()).chain(se.iter()) {
            if chunk.case_id != *primary_case_id {
                let err = PromptContextAssemblyError::InconsistentEvidence(format!(
                    "primary chunk '{}' case_id '{}' does not match primary card '{}'",
                    chunk.chunk_id, chunk.case_id, primary_case_id
                ));
                record_prompt_context_error_for_err(&span, &err);
                crate::observability::record_error(
                    &oi_span,
                    "PromptContextAssembly.InconsistentEvidence",
                    &err.to_string(),
                );
                return Err(err);
            }
        }

        for chunk in &ac {
            let found = cards
                .alternatives
                .iter()
                .any(|c| c.case_id == chunk.case_id);
            if !found {
                let err = PromptContextAssemblyError::InconsistentEvidence(format!(
                    "alternative chunk '{}' case_id '{}' has no hydrated alternative card",
                    chunk.chunk_id, chunk.case_id
                ));
                record_prompt_context_error_for_err(&span, &err);
                crate::observability::record_error(
                    &oi_span,
                    "PromptContextAssembly.InconsistentEvidence",
                    &err.to_string(),
                );
                return Err(err);
            }
        }

        // Assemble incident chunks in role order
        let efm_count = efm.len();
        let fch_count = fch.len();
        let se_count = se.len();
        let ac_count = ac.len();
        let theory_count = theory_chunks.len();
        let mut incident_chunks = Vec::new();
        incident_chunks.extend(efm);
        incident_chunks.extend(fch);
        incident_chunks.extend(se);
        incident_chunks.extend(ac);

        // Build normalized incident query
        let normalized_incident_query = build_normalized_incident_query(&query.structured_query);
        let matched_incident_card = build_matched_incident_card(primary_card);

        // Serialize context to JSON
        let ctx = JsonContext {
            task: "diagnostic_response".to_string(),
            user_problem: &request.query,
            input_token_count: request.input_token_count,
            normalized_incident_query,
            matched_incident_card,
            incident_evidence_chunks: incident_chunks.iter().map(incident_chunk_to_dto).collect(),
            theory_chunks: theory_chunks.iter().map(theory_chunk_to_dto).collect(),
            policy_constraints: &self.prompt_asset.policy_constraints,
        };

        let json_str = serde_json::to_string_pretty(&ctx).map_err(|e| {
            let err = PromptContextAssemblyError::PromptAsset(format!(
                "JSON serialization failed: {e}"
            ));
            record_prompt_context_error_for_err(&span, &err);
            crate::observability::record_error(
                &oi_span,
                "PromptContextAssembly.PromptAsset",
                &err.to_string(),
            );
            err
        })?;

        let prompt = self.prompt_asset.template.replacen(
            &self.prompt_asset.context_placeholder,
            &json_str,
            1,
        );

        span.record("prompt.selected.evidence_for_match.count", efm_count as i64);
        span.record("prompt.selected.first_check_hint.count", fch_count as i64);
        span.record("prompt.selected.supporting_explanation.count", se_count as i64);
        span.record("prompt.selected.alternative_context.count", ac_count as i64);
        span.record("prompt.selected.mechanism_explanation.count", theory_count as i64);
        span.record(
            "prompt.selected.total_chunks_count",
            (incident_chunks.len() + theory_chunks.len()) as i64,
        );
        span.record("prompt.rendered_chars", prompt.len() as i64);
        span.record("prompt.context_json_chars", json_str.len() as i64);
        let oi_output_json = serde_json::json!({
            "selected_counts": {
                "evidence_for_match": efm_count,
                "first_check_hint": fch_count,
                "supporting_explanation": se_count,
                "alternative_context": ac_count,
                "mechanism_explanation": theory_count,
                "total": incident_chunks.len() + theory_chunks.len()
            },
            "context_json_chars": json_str.len(),
            "prompt_chars": prompt.len()
        })
        .to_string();
        oi_span.record("output.value", oi_output_json.as_str());
        oi_span.record("output.mime_type", "application/json");
        oi_span.record("status", "ok");
        span.record("module.outcome", "success");
        span.record("status", "ok");

        Ok(PromptContextAssemblyOutput {
            prompt,
            response_schema: self.prompt_asset.response_schema.clone(),
            incident_evidence_chunks: incident_chunks,
            theory_chunks,
        })
    }
}

fn record_prompt_context_error(
    span: &tracing::Span,
    error_type: &'static str,
    error_message: &str,
) {
    span.record("module.outcome", "failure");
    span.record("status", "error");
    span.record("error.type", error_type);
    span.record("error.message", error_message);
}

fn record_prompt_context_error_for_err(
    span: &tracing::Span,
    err: &PromptContextAssemblyError,
) {
    match err {
        PromptContextAssemblyError::InvalidSettings(message) => record_prompt_context_error(
            span,
            "PromptContextAssembly.InvalidSettings",
            message,
        ),
        PromptContextAssemblyError::PromptAsset(message) => record_prompt_context_error(
            span,
            "PromptContextAssembly.PromptAsset",
            message,
        ),
        PromptContextAssemblyError::MissingPrimaryCard => record_prompt_context_error(
            span,
            "PromptContextAssembly.MissingPrimaryCard",
            "missing hydrated primary card",
        ),
        PromptContextAssemblyError::MissingRequiredEvidence { role } => {
            let message = format!("missing required evidence for role: {:?}", role);
            record_prompt_context_error(
                span,
                "PromptContextAssembly.MissingRequiredEvidence",
                &message,
            );
        }
        PromptContextAssemblyError::InconsistentEvidence(message) => record_prompt_context_error(
            span,
            "PromptContextAssembly.InconsistentEvidence",
            message,
        ),
    }
}

// ---------------------------------------------------------------------------
// Settings validation
// ---------------------------------------------------------------------------

fn validate_settings(s: &PromptContextSettings) -> Result<(), PromptContextAssemblyError> {
    if s.prompt_asset_path.is_empty() {
        return Err(PromptContextAssemblyError::InvalidSettings(
            "prompt_asset_path must not be empty".to_string(),
        ));
    }

    let cp = &s.chunk_packing;

    if cp.evidence_for_match.source != ChunkPackingSource::PrimaryIncident {
        return Err(PromptContextAssemblyError::InvalidSettings(
            "evidence_for_match.source must be PrimaryIncident".to_string(),
        ));
    }
    if cp.first_check_hint.source != ChunkPackingSource::PrimaryIncident {
        return Err(PromptContextAssemblyError::InvalidSettings(
            "first_check_hint.source must be PrimaryIncident".to_string(),
        ));
    }
    if cp.supporting_explanation.source != ChunkPackingSource::PrimaryIncident {
        return Err(PromptContextAssemblyError::InvalidSettings(
            "supporting_explanation.source must be PrimaryIncident".to_string(),
        ));
    }
    if cp.alternative_context.source != ChunkPackingSource::AlternativeIncident {
        return Err(PromptContextAssemblyError::InvalidSettings(
            "alternative_context.source must be AlternativeIncident".to_string(),
        ));
    }
    if cp.mechanism_explanation.source != ChunkPackingSource::Theory {
        return Err(PromptContextAssemblyError::InvalidSettings(
            "mechanism_explanation.source must be Theory".to_string(),
        ));
    }

    if cp.evidence_for_match.limit < 1 {
        return Err(PromptContextAssemblyError::InvalidSettings(
            "evidence_for_match.limit must be >= 1".to_string(),
        ));
    }
    if cp.first_check_hint.limit < 1 {
        return Err(PromptContextAssemblyError::InvalidSettings(
            "first_check_hint.limit must be >= 1".to_string(),
        ));
    }

    if cp.alternative_context.limit > 0 {
        match cp.alternative_context.per_case_limit {
            None => {
                return Err(PromptContextAssemblyError::InvalidSettings(
                    "alternative_context.per_case_limit must be Some(n > 0) when limit > 0"
                        .to_string(),
                ))
            }
            Some(0) => {
                return Err(PromptContextAssemblyError::InvalidSettings(
                    "alternative_context.per_case_limit must be > 0 when limit > 0".to_string(),
                ))
            }
            _ => {}
        }
    }

    if !cp.mechanism_explanation.tag_priority.is_empty() {
        return Err(PromptContextAssemblyError::InvalidSettings(
            "mechanism_explanation.tag_priority must be empty because theory chunks do not expose tags".to_string(),
        ));
    }
    if cp.mechanism_explanation.limit > 1 {
        return Err(PromptContextAssemblyError::InvalidSettings(
            "mechanism_explanation.limit must be <= 1".to_string(),
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Prompt asset loading
// ---------------------------------------------------------------------------

fn load_prompt_asset(
    path: &str,
) -> Result<DiagnosticResponsePromptAsset, PromptContextAssemblyError> {
    let schema_path = derive_schema_path(path)?;

    let asset_content = std::fs::read_to_string(path).map_err(|e| {
        PromptContextAssemblyError::PromptAsset(format!(
            "failed to read prompt asset '{path}': {e}"
        ))
    })?;

    let asset_json: serde_json::Value = serde_json::from_str(&asset_content).map_err(|e| {
        PromptContextAssemblyError::PromptAsset(format!("invalid prompt asset JSON: {e}"))
    })?;

    let schema_content = std::fs::read_to_string(&schema_path).map_err(|e| {
        PromptContextAssemblyError::PromptAsset(format!(
            "failed to read prompt asset schema '{schema_path}': {e}"
        ))
    })?;

    let schema_json: serde_json::Value = serde_json::from_str(&schema_content).map_err(|e| {
        PromptContextAssemblyError::PromptAsset(format!("invalid prompt asset schema JSON: {e}"))
    })?;

    let validator = jsonschema::options().build(&schema_json).map_err(|e| {
        PromptContextAssemblyError::PromptAsset(format!("prompt asset schema compile error: {e}"))
    })?;

    if !validator.is_valid(&asset_json) {
        let errors: Vec<String> = validator
            .iter_errors(&asset_json)
            .map(|e| e.to_string())
            .collect();
        return Err(PromptContextAssemblyError::PromptAsset(format!(
            "prompt asset schema validation failed: {}",
            errors.join("; ")
        )));
    }

    let asset: DiagnosticResponsePromptAsset = serde_json::from_value(asset_json).map_err(|e| {
        PromptContextAssemblyError::PromptAsset(format!("prompt asset deserialization failed: {e}"))
    })?;

    // Asset contract validation
    let placeholder = "{{json_context}}";
    let count = asset.template.matches(placeholder).count();
    if count == 0 {
        return Err(PromptContextAssemblyError::PromptAsset(
            "template must contain exactly one {{json_context}} placeholder".to_string(),
        ));
    }
    if count > 1 {
        return Err(PromptContextAssemblyError::PromptAsset(
            "template must contain exactly one {{json_context}} placeholder, found more than one"
                .to_string(),
        ));
    }
    if asset.context_placeholder != placeholder {
        return Err(PromptContextAssemblyError::PromptAsset(format!(
            "context_placeholder must be '{{{{json_context}}}}', got '{}'",
            asset.context_placeholder
        )));
    }
    if asset.required_placeholders != vec!["json_context"] {
        return Err(PromptContextAssemblyError::PromptAsset(
            "required_placeholders must contain exactly [\"json_context\"]".to_string(),
        ));
    }
    if asset.policy_constraints.is_empty() {
        return Err(PromptContextAssemblyError::PromptAsset(
            "policy_constraints must be non-empty".to_string(),
        ));
    }
    if !asset.response_schema.is_object() {
        return Err(PromptContextAssemblyError::PromptAsset(
            "response_schema must be a JSON object".to_string(),
        ));
    }

    Ok(asset)
}

fn derive_schema_path(asset_path: &str) -> Result<String, PromptContextAssemblyError> {
    let path = std::path::Path::new(asset_path);
    let file_name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
        PromptContextAssemblyError::PromptAsset("invalid prompt asset path".to_string())
    })?;

    let schema_file_name = if file_name.ends_with(".manual_test.json") {
        file_name.replacen(".manual_test.json", ".schema.json", 1)
    } else if file_name.ends_with(".json") {
        file_name.replacen(".json", ".schema.json", 1)
    } else {
        return Err(PromptContextAssemblyError::PromptAsset(
            "prompt asset path must end with .json".to_string(),
        ));
    };

    let parent = path.parent().unwrap_or(std::path::Path::new(""));
    Ok(parent.join(schema_file_name).to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Chunk selection helpers
// ---------------------------------------------------------------------------

struct RankedChunk<'a> {
    tag_bucket_index: usize,
    score: f32,
    source_index: usize,
    chunk: &'a IncidentEvidenceChunk,
}

fn compute_eligible_chunks<'a>(
    chunks: &'a [IncidentEvidenceChunk],
    role_settings: &ChunkRolePackingSettings,
) -> Vec<RankedChunk<'a>> {
    let fallback_bucket = role_settings.tag_priority.len();
    let mut ranked: Vec<RankedChunk<'a>> = Vec::new();

    for (source_index, chunk) in chunks.iter().enumerate() {
        let mut best_tag_index: Option<usize> = None;
        for raw_tag in &chunk.chunk_tags {
            if let Ok(tag) = IncidentChunkTag::from_str(raw_tag) {
                if let Some(idx) = role_settings.tag_priority.iter().position(|t| *t == tag) {
                    best_tag_index = Some(match best_tag_index {
                        Some(prev) => prev.min(idx),
                        None => idx,
                    });
                }
            }
        }

        let tag_bucket_index = match best_tag_index {
            Some(idx) => idx,
            None if role_settings.fallback_to_any_chunk => fallback_bucket,
            None => continue,
        };

        ranked.push(RankedChunk {
            tag_bucket_index,
            score: chunk.score,
            source_index,
            chunk,
        });
    }

    ranked.sort_by(|a, b| {
        a.tag_bucket_index
            .cmp(&b.tag_bucket_index)
            .then_with(|| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.source_index.cmp(&b.source_index))
    });

    ranked
}

fn make_prompt_incident_chunk(
    source: &IncidentEvidenceChunk,
    role: PromptEvidenceRole,
) -> PromptIncidentEvidenceChunk {
    let chunk_tags = source
        .chunk_tags
        .iter()
        .filter_map(|t| IncidentChunkTag::from_str(t).ok())
        .collect();
    PromptIncidentEvidenceChunk {
        role,
        chunk_id: source.chunk_id.clone(),
        case_id: source.case_id.clone(),
        score: source.score,
        chunk_tags,
        text: source.text.clone(),
    }
}

fn select_primary_role(
    chunks: &[IncidentEvidenceChunk],
    role_settings: &ChunkRolePackingSettings,
    already_selected: &mut HashSet<String>,
    role: PromptEvidenceRole,
    is_required: bool,
) -> Result<Vec<PromptIncidentEvidenceChunk>, PromptContextAssemblyError> {
    if role_settings.limit == 0 {
        return Ok(vec![]);
    }

    let ranked = compute_eligible_chunks(chunks, role_settings);

    // Split into distinct (not already selected) and duplicate candidates
    let mut distinct: Vec<&RankedChunk<'_>> = Vec::new();
    let mut duplicates: Vec<&RankedChunk<'_>> = Vec::new();

    for rc in &ranked {
        if already_selected.contains(&rc.chunk.chunk_id) {
            duplicates.push(rc);
        } else {
            distinct.push(rc);
        }
    }

    // Determine the pool to select from
    let pool: Vec<&RankedChunk<'_>> = if !distinct.is_empty() {
        distinct
    } else if is_required && role_settings.fallback_to_any_chunk {
        duplicates
    } else {
        vec![]
    };

    if pool.is_empty() && is_required {
        return Err(PromptContextAssemblyError::MissingRequiredEvidence { role });
    }

    let mut selected = Vec::new();
    for rc in pool.into_iter().take(role_settings.limit) {
        if !already_selected.contains(&rc.chunk.chunk_id) {
            already_selected.insert(rc.chunk.chunk_id.clone());
        }
        selected.push(make_prompt_incident_chunk(rc.chunk, role));
    }

    Ok(selected)
}

fn select_alternative_context(
    alt_chunks: &[IncidentEvidenceChunk],
    role_settings: &ChunkRolePackingSettings,
    alt_cards: &[IncidentCard],
    already_selected: &HashSet<String>,
) -> Vec<PromptIncidentEvidenceChunk> {
    if role_settings.limit == 0 {
        return vec![];
    }

    let per_case_limit = role_settings.per_case_limit.unwrap_or(usize::MAX);

    // Determine case iteration order
    let case_order: Vec<String> = if alt_cards.is_empty() {
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        for chunk in alt_chunks {
            if seen.insert(chunk.case_id.clone()) {
                order.push(chunk.case_id.clone());
            }
        }
        order
    } else {
        alt_cards.iter().map(|c| c.case_id.clone()).collect()
    };

    // Build ranked eligible chunks per case (skip already selected — optional role cannot reuse)
    let fallback_bucket = role_settings.tag_priority.len();
    let mut case_pools: HashMap<String, Vec<(usize, f32, usize, &IncidentEvidenceChunk)>> =
        HashMap::new();

    for (src_idx, chunk) in alt_chunks.iter().enumerate() {
        if already_selected.contains(&chunk.chunk_id) {
            continue;
        }

        let mut best_tag_index: Option<usize> = None;
        for raw_tag in &chunk.chunk_tags {
            if let Ok(tag) = IncidentChunkTag::from_str(raw_tag) {
                if let Some(idx) = role_settings.tag_priority.iter().position(|t| *t == tag) {
                    best_tag_index = Some(match best_tag_index {
                        Some(prev) => prev.min(idx),
                        None => idx,
                    });
                }
            }
        }

        let tag_bucket = match best_tag_index {
            Some(idx) => idx,
            None if role_settings.fallback_to_any_chunk => fallback_bucket,
            None => continue,
        };

        case_pools.entry(chunk.case_id.clone()).or_default().push((
            tag_bucket,
            chunk.score,
            src_idx,
            chunk,
        ));
    }

    // Sort within each case pool
    for pool in case_pools.values_mut() {
        pool.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| a.2.cmp(&b.2))
        });
    }

    // Round-robin selection
    let mut selected: Vec<PromptIncidentEvidenceChunk> = Vec::new();
    let mut cursors: HashMap<String, usize> = HashMap::new();
    let mut counts: HashMap<String, usize> = HashMap::new();

    loop {
        let prev_len = selected.len();

        for case_id in &case_order {
            if selected.len() >= role_settings.limit {
                break;
            }

            let pool = match case_pools.get(case_id) {
                Some(p) => p,
                None => continue,
            };

            let count = *counts.get(case_id).unwrap_or(&0);
            if count >= per_case_limit {
                continue;
            }

            let cursor = cursors.entry(case_id.clone()).or_insert(0);
            if *cursor >= pool.len() {
                continue;
            }

            let (_, _, _, chunk) = pool[*cursor];
            *cursor += 1;
            *counts.entry(case_id.clone()).or_insert(0) += 1;
            selected.push(make_prompt_incident_chunk(
                chunk,
                PromptEvidenceRole::AlternativeContext,
            ));
        }

        if selected.len() == prev_len || selected.len() >= role_settings.limit {
            break;
        }
    }

    selected
}

// ---------------------------------------------------------------------------
// Normalized incident query builder
// ---------------------------------------------------------------------------

fn build_normalized_incident_query(query: &StructuredUserQuery) -> NormalizedIncidentQueryDto {
    let mut seen: HashSet<String> = HashSet::new();
    let mut signals_present: Vec<String> = Vec::new();

    for term in query
        .symptoms
        .iter()
        .map(|s| &s.term)
        .chain(query.triggers.iter())
        .chain(query.observability_signals.iter())
    {
        if !term.is_empty() && seen.insert(term.clone()) {
            signals_present.push(term.clone());
        }
    }

    NormalizedIncidentQueryDto {
        recognized_canonical_symptoms: query
            .symptoms
            .iter()
            .map(|s| s.term.clone())
            .filter(|t| !t.is_empty())
            .collect(),
        unmapped_user_symptoms: vec![],
        affected_components: query
            .affected_subsystems
            .iter()
            .map(|s| s.term.clone())
            .filter(|t| !t.is_empty())
            .collect(),
        failure_mode_candidates: query
            .failure_modes
            .iter()
            .map(|s| s.term.clone())
            .filter(|t| !t.is_empty())
            .collect(),
        observed_phase: vec![],
        signals_present,
        missing_signals: vec![],
    }
}

fn build_matched_incident_card(card: &IncidentCard) -> MatchedIncidentCardDto {
    let systems = [card.vendor_or_project.as_deref(), card.system_type.as_deref()]
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect();

    let later_symptoms = card
        .incident_phases
        .iter()
        .flat_map(|phase| phase.symptoms.iter().chain(phase.user_visible_impact.iter()))
        .filter(|value| !value.is_empty())
        .cloned()
        .collect();

    let investigation_questions = card
        .discriminating_checks
        .iter()
        .map(|check| check.question.clone())
        .filter(|value| !value.is_empty())
        .collect();

    let discriminating_checks = if !card.investigation_steps.is_empty() {
        card.investigation_steps
            .iter()
            .filter(|value| !value.is_empty())
            .cloned()
            .collect()
    } else {
        card.discriminating_checks
            .iter()
            .map(|check| check.question.clone())
            .filter(|value| !value.is_empty())
            .collect()
    };

    MatchedIncidentCardDto {
        context: MatchedIncidentContextDto {
            systems,
            affected_components: card.affected_components.clone(),
            initial_symptoms: card.canonical_symptoms.clone(),
            later_symptoms,
        },
        hypotheses: MatchedIncidentHypothesesDto {
            failure_modes: card.failure_mode_candidates.clone(),
            hypothesis_signals: card.candidate_explanations.clone(),
            hypothesis_updates: card.diagnostic_patterns.clone(),
            contributing_factors: card.confidence_notes.clone(),
        },
        checks: MatchedIncidentChecksDto {
            investigation_questions,
            discriminating_checks,
        },
    }
}

// ---------------------------------------------------------------------------
// DTO conversion helpers
// ---------------------------------------------------------------------------

fn incident_chunk_to_dto(chunk: &PromptIncidentEvidenceChunk) -> IncidentChunkDto {
    IncidentChunkDto {
        role: role_to_str(chunk.role).to_string(),
        source_document_id: chunk.case_id.clone(),
        chunk_tags: chunk
            .chunk_tags
            .iter()
            .map(|t| t.as_ref().to_owned())
            .collect(),
        text: chunk.text.clone(),
    }
}

fn theory_chunk_to_dto(chunk: &PromptTheoryEvidenceChunk) -> TheoryChunkDto {
    TheoryChunkDto {
        role: role_to_str(chunk.role).to_string(),
        source_document_id: chunk.chunk_id.clone(),
        text: chunk.text.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ChunkPackingSettings, ChunkPackingSource, ChunkRolePackingSettings, PromptContextSettings,
    };
    use crate::shared_types::{
        IncidentCard, IncidentEvidenceChunk, IncidentEvidenceRetrievalOutput, ModelTokenUsage,
        NormalizedUserRequest, QueryStructuringOutput, StructuredUserQuery,
        StructuredUserQueryConfidence, StructuredUserQuerySupportLevel, StructuredUserQueryTerm,
        TheoryEvidenceChunk, TheoryEvidenceRetrievalOutput,
    };

    // ---------------------------------------------------------------------------
    // Test helpers
    // ---------------------------------------------------------------------------

    fn valid_asset_path() -> String {
        let manifest = env!("CARGO_MANIFEST_DIR");
        format!(
            "{manifest}/../../Specification/runtime/request_pipeline/prompt_context_assembly/diagnostic_response_prompt_baseline.manual_test.json"
        )
    }

    fn default_settings() -> PromptContextSettings {
        PromptContextSettings {
            prompt_asset_path: valid_asset_path(),
            chunk_packing: ChunkPackingSettings {
                evidence_for_match: ChunkRolePackingSettings {
                    source: ChunkPackingSource::PrimaryIncident,
                    limit: 1,
                    per_case_limit: None,
                    fallback_to_any_chunk: true,
                    tag_priority: vec![IncidentChunkTag::Symptom, IncidentChunkTag::FailureMode],
                },
                first_check_hint: ChunkRolePackingSettings {
                    source: ChunkPackingSource::PrimaryIncident,
                    limit: 1,
                    per_case_limit: None,
                    fallback_to_any_chunk: true,
                    tag_priority: vec![
                        IncidentChunkTag::DiagnosticStep,
                        IncidentChunkTag::Investigation,
                    ],
                },
                supporting_explanation: ChunkRolePackingSettings {
                    source: ChunkPackingSource::PrimaryIncident,
                    limit: 1,
                    per_case_limit: None,
                    fallback_to_any_chunk: false,
                    tag_priority: vec![IncidentChunkTag::ContributingFactor],
                },
                alternative_context: ChunkRolePackingSettings {
                    source: ChunkPackingSource::AlternativeIncident,
                    limit: 2,
                    per_case_limit: Some(1),
                    fallback_to_any_chunk: false,
                    tag_priority: vec![IncidentChunkTag::FailureMode],
                },
                mechanism_explanation: ChunkRolePackingSettings {
                    source: ChunkPackingSource::Theory,
                    limit: 1,
                    per_case_limit: None,
                    fallback_to_any_chunk: false,
                    tag_priority: vec![],
                },
            },
        }
    }

    fn minimal_card(case_id: &str) -> IncidentCard {
        IncidentCard {
            case_id: case_id.to_string(),
            title: format!("Title {case_id}"),
            source_type: "report".to_string(),
            source_name: format!("Source {case_id}"),
            source_path: "path".to_string(),
            vendor_or_project: None,
            system_type: None,
            version_tested: None,
            report_date: None,
            short_summary: "summary".to_string(),
            canonical_symptoms: vec![],
            affected_components: vec![],
            failure_mode_candidates: vec![],
            observed_phases: vec![],
            incident_phases: vec![],
            turning_points: vec![],
            candidate_explanations: vec![],
            diagnostic_patterns: vec![],
            discriminating_checks: vec![],
            expected_observations: vec![],
            investigation_steps: vec![],
            root_cause_summary: None,
            reasoning_summary: None,
            mitigations_or_workarounds: vec![],
            prevention_or_design_followups: vec![],
            claimed_guarantees: vec![],
            violated_properties: vec![],
            resolution_status: None,
            fix_versions: vec![],
            confidence_notes: vec![],
            source_refs: vec![],
        }
    }

    fn chunk(
        id: &str,
        case_id: &str,
        score: f32,
        tags: &[&str],
        text: &str,
    ) -> IncidentEvidenceChunk {
        IncidentEvidenceChunk {
            chunk_id: id.to_string(),
            case_id: case_id.to_string(),
            score,
            chunk_tags: tags.iter().map(|t| t.to_string()).collect(),
            text: text.to_string(),
        }
    }

    fn theory_chunk(id: &str, score: f32, text: &str) -> TheoryEvidenceChunk {
        TheoryEvidenceChunk {
            chunk_id: id.to_string(),
            score,
            text: text.to_string(),
        }
    }

    fn minimal_query() -> QueryStructuringOutput {
        QueryStructuringOutput {
            structured_query: StructuredUserQuery {
                intent: "diagnose".to_string(),
                scenario: "test".to_string(),
                symptoms: vec![],
                affected_subsystems: vec![],
                failure_modes: vec![],
                system_properties: vec![],
                entities: vec![],
                constraints: vec![],
                triggers: vec![],
                observability_signals: vec![],
                unresolved_terms: vec![],
                rejected_nearby_terms: vec![],
                confidence: StructuredUserQueryConfidence::Medium,
            },
            token_usage: ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
            metrics: Some(crate::shared_types::QueryStructuringMetrics::default()),
        }
    }

    fn minimal_request() -> NormalizedUserRequest {
        NormalizedUserRequest {
            query: "test query".to_string(),
            input_token_count: 5,
        }
    }

    fn make_assembly() -> PromptContextAssembly {
        PromptContextAssembly::new(default_settings()).expect("default settings must succeed")
    }

    // Creates a minimal primary-only evidence output with one chunk using the given tags.
    fn primary_evidence(chunks: Vec<IncidentEvidenceChunk>) -> IncidentEvidenceRetrievalOutput {
        IncidentEvidenceRetrievalOutput {
            primary_chunks: chunks,
            alternative_chunks: vec![],
            metrics: None,
        }
    }

    // Runs assemble with minimal valid inputs and a provided primary card / chunks.
    fn assemble_with(
        primary_card: IncidentCard,
        primary_chunks: Vec<IncidentEvidenceChunk>,
    ) -> Result<PromptContextAssemblyOutput, PromptContextAssemblyError> {
        let asm = make_assembly();
        let request = minimal_request();
        let query = minimal_query();
        let cards = CardHydrationOutput {
            primary: Some(primary_card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(primary_chunks);
        let theory = TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None };
        asm.assemble(&request, &query, &cards, &evidence, &theory)
    }

    // Provides two primary chunks — symptom (high score) and diagnostic_step (lower score) —
    // enough to fill both evidence_for_match and first_check_hint without duplication.
    fn two_primary_chunks(case_id: &str) -> Vec<IncidentEvidenceChunk> {
        vec![
            chunk("c1", case_id, 0.9, &["chunk_role:symptom"], "symptom text"),
            chunk(
                "c2",
                case_id,
                0.7,
                &["chunk_role:diagnostic_step"],
                "step text",
            ),
        ]
    }

    // ---------------------------------------------------------------------------
    // Constructor tests
    // ---------------------------------------------------------------------------

    #[test]
    fn constructor_rejects_empty_prompt_asset_path() {
        let mut s = default_settings();
        s.prompt_asset_path = String::new();
        let err = PromptContextAssembly::new(s).expect_err("should fail — empty path");
        assert!(
            matches!(err, PromptContextAssemblyError::InvalidSettings(_)),
            "expected InvalidSettings, got: {err:?}"
        );
    }

    #[test]
    fn constructor_rejects_unreadable_prompt_asset_file() {
        let mut s = default_settings();
        s.prompt_asset_path = "does/not/exist/asset.json".to_string();
        assert!(PromptContextAssembly::new(s).is_err());
    }

    #[test]
    fn constructor_rejects_unreadable_schema_file() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        // Write valid JSON that would normally be a prompt asset, but schema path won't exist.
        write!(f, r#"{{"version":"v1","name":"x","template":"{{{{json_context}}}}","context_placeholder":"{{{{json_context}}}}","required_placeholders":["json_context"],"response_schema":{{}},"policy_constraints":["c"]}}"#).unwrap();
        let mut s = default_settings();
        s.prompt_asset_path = f.path().to_string_lossy().into_owned();
        let err = PromptContextAssembly::new(s).expect_err("should fail — schema unreachable");
        assert!(matches!(err, PromptContextAssemblyError::PromptAsset(_)));
    }

    #[test]
    fn constructor_rejects_invalid_prompt_asset_json() {
        // Asset file has .json suffix so derive_schema_path succeeds, but the JSON is malformed —
        // the error must come from the serde_json parse step, not from path rejection.
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("asset.json");
        std::fs::write(&asset_path, "not json at all {{ garbage").unwrap();
        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        let err = PromptContextAssembly::new(s).expect_err("should fail — invalid JSON");
        assert!(
            matches!(err, PromptContextAssemblyError::PromptAsset(_)),
            "expected PromptAsset, got: {err:?}"
        );
    }

    #[test]
    fn constructor_rejects_invalid_schema_json() {
        // Asset file is valid JSON; schema file exists but contains invalid JSON.
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("asset.json");
        let schema_path = dir.path().join("asset.schema.json");
        std::fs::write(&asset_path, r#"{"version":"v1","name":"x","template":"{{json_context}}","context_placeholder":"{{json_context}}","required_placeholders":["json_context"],"response_schema":{},"policy_constraints":["c"]}"#).unwrap();
        std::fs::write(&schema_path, "not valid json {{").unwrap();
        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        let err = PromptContextAssembly::new(s).expect_err("should fail — invalid schema JSON");
        assert!(
            matches!(err, PromptContextAssemblyError::PromptAsset(_)),
            "expected PromptAsset, got: {err:?}"
        );
    }

    #[test]
    fn constructor_rejects_asset_that_fails_schema_validation() {
        // Use a valid JSON object missing required fields.
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("asset.json");
        let schema_path = dir.path().join("asset.schema.json");

        std::fs::write(
            &schema_path,
            r#"{"type":"object","required":["must_have"],"properties":{"must_have":{"type":"string"}}}"#,
        )
        .unwrap();
        std::fs::write(&asset_path, r#"{"no_required_field": true}"#).unwrap();

        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        assert!(PromptContextAssembly::new(s).is_err());
    }

    #[test]
    fn constructor_rejects_asset_missing_json_context_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("asset.json");
        let schema_path = dir.path().join("asset.schema.json");

        // Use a permissive schema that accepts anything
        std::fs::write(&schema_path, r#"{"type":"object"}"#).unwrap();
        // Template has no {{json_context}}
        let asset = serde_json::json!({
            "version": "v1",
            "name": "test",
            "template": "no placeholder here",
            "context_placeholder": "{{json_context}}",
            "required_placeholders": ["json_context"],
            "response_schema": {},
            "policy_constraints": ["c"]
        });
        std::fs::write(&asset_path, serde_json::to_string(&asset).unwrap()).unwrap();

        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        let err = PromptContextAssembly::new(s).expect_err("should fail — missing placeholder");
        assert!(matches!(err, PromptContextAssemblyError::PromptAsset(_)));
    }

    #[test]
    fn constructor_rejects_asset_with_more_than_one_json_context_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("asset.json");
        let schema_path = dir.path().join("asset.schema.json");
        std::fs::write(&schema_path, r#"{"type":"object"}"#).unwrap();
        let asset = serde_json::json!({
            "version": "v1",
            "name": "test",
            "template": "{{json_context}} and {{json_context}}",
            "context_placeholder": "{{json_context}}",
            "required_placeholders": ["json_context"],
            "response_schema": {},
            "policy_constraints": ["c"]
        });
        std::fs::write(&asset_path, serde_json::to_string(&asset).unwrap()).unwrap();

        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        let err = PromptContextAssembly::new(s).expect_err("should fail — duplicate placeholder");
        assert!(matches!(err, PromptContextAssemblyError::PromptAsset(_)));
    }

    #[test]
    fn constructor_rejects_asset_with_empty_policy_constraints() {
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("asset.json");
        let schema_path = dir.path().join("asset.schema.json");
        std::fs::write(&schema_path, r#"{"type":"object"}"#).unwrap();
        let asset = serde_json::json!({
            "version": "v1",
            "name": "test",
            "template": "{{json_context}}",
            "context_placeholder": "{{json_context}}",
            "required_placeholders": ["json_context"],
            "response_schema": {},
            "policy_constraints": []
        });
        std::fs::write(&asset_path, serde_json::to_string(&asset).unwrap()).unwrap();

        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        let err = PromptContextAssembly::new(s).expect_err("should fail — empty constraints");
        assert!(matches!(err, PromptContextAssemblyError::PromptAsset(_)));
    }

    #[test]
    fn constructor_succeeds_with_valid_settings_and_asset() {
        assert!(PromptContextAssembly::new(default_settings()).is_ok());
    }

    #[test]
    fn constructor_derives_schema_path_by_replacing_suffix_with_schema_json() {
        // Valid asset at .manual_test.json must find its .schema.json automatically.
        assert!(PromptContextAssembly::new(default_settings()).is_ok());
    }

    #[test]
    fn constructor_rejects_evidence_for_match_limit_zero() {
        let mut s = default_settings();
        s.chunk_packing.evidence_for_match.limit = 0;
        let err = PromptContextAssembly::new(s).expect_err("should fail");
        assert!(matches!(
            err,
            PromptContextAssemblyError::InvalidSettings(_)
        ));
    }

    #[test]
    fn constructor_rejects_first_check_hint_limit_zero() {
        let mut s = default_settings();
        s.chunk_packing.first_check_hint.limit = 0;
        let err = PromptContextAssembly::new(s).expect_err("should fail");
        assert!(matches!(
            err,
            PromptContextAssemblyError::InvalidSettings(_)
        ));
    }

    #[test]
    fn constructor_allows_supporting_explanation_limit_zero() {
        let mut s = default_settings();
        s.chunk_packing.supporting_explanation.limit = 0;
        assert!(PromptContextAssembly::new(s).is_ok());
    }

    #[test]
    fn constructor_succeeds_supporting_explanation_limit_one() {
        let mut s = default_settings();
        s.chunk_packing.supporting_explanation.limit = 1;
        assert!(PromptContextAssembly::new(s).is_ok());
    }

    #[test]
    fn constructor_allows_alternative_context_limit_zero() {
        let mut s = default_settings();
        s.chunk_packing.alternative_context.limit = 0;
        s.chunk_packing.alternative_context.per_case_limit = None;
        assert!(PromptContextAssembly::new(s).is_ok());
    }

    #[test]
    fn constructor_allows_mechanism_explanation_limit_zero() {
        let mut s = default_settings();
        s.chunk_packing.mechanism_explanation.limit = 0;
        assert!(PromptContextAssembly::new(s).is_ok());
    }

    #[test]
    fn constructor_rejects_evidence_for_match_wrong_source() {
        let mut s = default_settings();
        s.chunk_packing.evidence_for_match.source = ChunkPackingSource::Theory;
        let err = PromptContextAssembly::new(s).expect_err("should fail");
        assert!(matches!(
            err,
            PromptContextAssemblyError::InvalidSettings(_)
        ));
    }

    #[test]
    fn constructor_rejects_first_check_hint_wrong_source() {
        let mut s = default_settings();
        s.chunk_packing.first_check_hint.source = ChunkPackingSource::AlternativeIncident;
        let err = PromptContextAssembly::new(s).expect_err("should fail");
        assert!(matches!(
            err,
            PromptContextAssemblyError::InvalidSettings(_)
        ));
    }

    #[test]
    fn constructor_rejects_supporting_explanation_wrong_source() {
        let mut s = default_settings();
        s.chunk_packing.supporting_explanation.source = ChunkPackingSource::AlternativeIncident;
        let err = PromptContextAssembly::new(s).expect_err("should fail");
        assert!(matches!(
            err,
            PromptContextAssemblyError::InvalidSettings(_)
        ));
    }

    #[test]
    fn constructor_rejects_alternative_context_wrong_source() {
        let mut s = default_settings();
        s.chunk_packing.alternative_context.source = ChunkPackingSource::PrimaryIncident;
        let err = PromptContextAssembly::new(s).expect_err("should fail");
        assert!(matches!(
            err,
            PromptContextAssemblyError::InvalidSettings(_)
        ));
    }

    #[test]
    fn constructor_rejects_mechanism_explanation_wrong_source() {
        let mut s = default_settings();
        s.chunk_packing.mechanism_explanation.source = ChunkPackingSource::PrimaryIncident;
        let err = PromptContextAssembly::new(s).expect_err("should fail");
        assert!(matches!(
            err,
            PromptContextAssemblyError::InvalidSettings(_)
        ));
    }

    #[test]
    fn constructor_rejects_alt_context_limit_gt0_per_case_limit_none() {
        let mut s = default_settings();
        s.chunk_packing.alternative_context.limit = 2;
        s.chunk_packing.alternative_context.per_case_limit = None;
        let err = PromptContextAssembly::new(s).expect_err("should fail");
        assert!(matches!(
            err,
            PromptContextAssemblyError::InvalidSettings(_)
        ));
    }

    #[test]
    fn constructor_rejects_alt_context_limit_gt0_per_case_limit_zero() {
        let mut s = default_settings();
        s.chunk_packing.alternative_context.limit = 2;
        s.chunk_packing.alternative_context.per_case_limit = Some(0);
        let err = PromptContextAssembly::new(s).expect_err("should fail");
        assert!(matches!(
            err,
            PromptContextAssemblyError::InvalidSettings(_)
        ));
    }

    #[test]
    fn constructor_accepts_alt_context_per_case_limit_none_when_limit_is_zero() {
        let mut s = default_settings();
        s.chunk_packing.alternative_context.limit = 0;
        s.chunk_packing.alternative_context.per_case_limit = None;
        assert!(PromptContextAssembly::new(s).is_ok());
    }

    #[test]
    fn constructor_accepts_any_per_case_limit_when_alt_context_limit_is_zero() {
        let mut s = default_settings();
        s.chunk_packing.alternative_context.limit = 0;
        s.chunk_packing.alternative_context.per_case_limit = Some(999);
        assert!(PromptContextAssembly::new(s).is_ok());
    }

    #[test]
    fn constructor_rejects_mechanism_explanation_nonempty_tag_priority() {
        let mut s = default_settings();
        s.chunk_packing.mechanism_explanation.tag_priority = vec![IncidentChunkTag::Symptom];
        let err = PromptContextAssembly::new(s).expect_err("should fail");
        assert!(matches!(
            err,
            PromptContextAssemblyError::InvalidSettings(_)
        ));
    }

    // ---------------------------------------------------------------------------
    // assemble() — primary card and missing card
    // ---------------------------------------------------------------------------

    #[test]
    fn assemble_fails_when_primary_card_is_none() {
        let asm = make_assembly();
        let request = minimal_request();
        let query = minimal_query();
        let cards = CardHydrationOutput {
            primary: None,
            alternatives: vec![],
        };
        let evidence = primary_evidence(vec![]);
        let theory = TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None };
        let err = asm
            .assemble(&request, &query, &cards, &evidence, &theory)
            .expect_err("should fail — no primary card");
        assert!(matches!(
            err,
            PromptContextAssemblyError::MissingPrimaryCard
        ));
    }

    #[test]
    fn assemble_includes_primary_card_as_matched_incident_card() {
        let mut card = minimal_card("case_a");
        card.vendor_or_project = Some("etcd".to_string());
        card.system_type = Some("coordination store".to_string());
        card.affected_components = vec!["lock_service".to_string()];
        card.canonical_symptoms = vec!["duplicate_lock_holders".to_string()];
        card.failure_mode_candidates = vec!["lock_ownership_violation".to_string()];
        card.candidate_explanations = vec!["lock ownership may not be exclusive".to_string()];
        card.diagnostic_patterns = vec!["healthy kv behavior can coexist with unsafe locks".to_string()];
        card.discriminating_checks = vec![crate::shared_types::DiscriminatingCheck {
            question: "Do conflicts cluster around lease expiry?".to_string(),
            why: "Tests a lease-related trigger.".to_string(),
        }];
        card.investigation_steps =
            vec!["Correlate failures with lease timing and lock wait paths.".to_string()];
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert!(ctx["matched_incident_card"].get("context").is_some());
        assert!(ctx["matched_incident_card"].get("hypotheses").is_some());
        assert!(ctx["matched_incident_card"].get("checks").is_some());
        assert!(ctx["matched_incident_card"].get("case_id").is_none());
        assert!(ctx["matched_incident_card"].get("title").is_none());
        assert!(ctx["matched_incident_card"].get("source_name").is_none());
    }

    #[test]
    fn assemble_excludes_full_alternative_cards() {
        let asm = make_assembly();
        let request = minimal_request();
        let query = minimal_query();
        let primary = minimal_card("primary_case");
        let alt = minimal_card("alt_case");
        let cards = CardHydrationOutput {
            primary: Some(primary),
            alternatives: vec![alt],
        };

        let chunks = two_primary_chunks("primary_case");
        let evidence = primary_evidence(chunks);
        let theory = TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None };
        let out = asm
            .assemble(&request, &query, &cards, &evidence, &theory)
            .unwrap();

        // The alt card's fields should not appear as a top-level JSON key "alternatives"
        // (the full card object should not be in the prompt)
        let json_start = out.prompt.find("JSON context follows:").unwrap();
        let json_part = &out.prompt[json_start..];
        // matched_incident_card should only contain primary, not an "alternatives" key at top level
        assert!(!json_part.contains("\"alternatives\""));
    }

    // ---------------------------------------------------------------------------
    // assemble() — normalized_incident_query mapping
    // ---------------------------------------------------------------------------

    fn query_with(
        symptoms: &[&str],
        subsystems: &[&str],
        failure_modes: &[&str],
        triggers: &[&str],
        observability_signals: &[&str],
    ) -> QueryStructuringOutput {
        let make_terms = |terms: &[&str]| {
            terms
                .iter()
                .map(|t| StructuredUserQueryTerm {
                    term: t.to_string(),
                    evidence_span: t.to_string(),
                    support_level: StructuredUserQuerySupportLevel::Explicit,
                })
                .collect()
        };
        QueryStructuringOutput {
            structured_query: StructuredUserQuery {
                intent: "diagnose".to_string(),
                scenario: "test".to_string(),
                symptoms: make_terms(symptoms),
                affected_subsystems: make_terms(subsystems),
                failure_modes: make_terms(failure_modes),
                system_properties: vec![],
                entities: vec![],
                constraints: vec![],
                triggers: triggers.iter().map(|t| t.to_string()).collect(),
                observability_signals: observability_signals
                    .iter()
                    .map(|t| t.to_string())
                    .collect(),
                unresolved_terms: vec![],
                rejected_nearby_terms: vec![],
                confidence: StructuredUserQueryConfidence::High,
            },
            token_usage: ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
            metrics: Some(crate::shared_types::QueryStructuringMetrics::default()),
        }
    }

    fn extract_json_context(prompt: &str) -> serde_json::Value {
        let marker = "JSON context follows:\n";
        let start = prompt.find(marker).expect("JSON context marker not found") + marker.len();
        serde_json::from_str(&prompt[start..]).expect("failed to parse JSON context")
    }

    #[test]
    fn assemble_copies_user_problem_from_request_query() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let asm = make_assembly();
        let request = NormalizedUserRequest {
            query: "specific query text".to_string(),
            input_token_count: 3,
        };
        let query = minimal_query();
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let theory = TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None };
        let out = asm
            .assemble(&request, &query, &cards, &evidence, &theory)
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert_eq!(ctx["user_problem"], "specific query text");
    }

    #[test]
    fn assemble_builds_recognized_canonical_symptoms_from_symptoms_terms() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let asm = make_assembly();
        let request = minimal_request();
        let query = query_with(&["lost_writes", "read_skew"], &[], &[], &[], &[]);
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let theory = TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None };
        let out = asm
            .assemble(&request, &query, &cards, &evidence, &theory)
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert_eq!(
            ctx["normalized_incident_query"]["recognized_canonical_symptoms"],
            serde_json::json!(["lost_writes", "read_skew"])
        );
    }

    #[test]
    fn assemble_builds_affected_components_from_subsystems_terms() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let asm = make_assembly();
        let query = query_with(&[], &["transaction_api", "retry_mechanism"], &[], &[], &[]);
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let theory = TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None };
        let out = asm
            .assemble(&minimal_request(), &query, &cards, &evidence, &theory)
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert_eq!(
            ctx["normalized_incident_query"]["affected_components"],
            serde_json::json!(["transaction_api", "retry_mechanism"])
        );
    }

    #[test]
    fn assemble_builds_failure_mode_candidates_from_failure_modes_terms() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let asm = make_assembly();
        let query = query_with(&[], &[], &["transaction_retry_bug"], &[], &[]);
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let theory = TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None };
        let out = asm
            .assemble(&minimal_request(), &query, &cards, &evidence, &theory)
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert_eq!(
            ctx["normalized_incident_query"]["failure_mode_candidates"],
            serde_json::json!(["transaction_retry_bug"])
        );
    }

    #[test]
    fn assemble_builds_signals_present_with_deduplication() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let asm = make_assembly();
        // "lost_writes" appears in both symptoms and observability_signals — only once in output
        let query = query_with(
            &["lost_writes", "read_skew"],
            &[],
            &[],
            &["network issues"],
            &["inconsistent reads", "lost_writes"],
        );
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let theory = TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None };
        let out = asm
            .assemble(&minimal_request(), &query, &cards, &evidence, &theory)
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        let signals = ctx["normalized_incident_query"]["signals_present"]
            .as_array()
            .unwrap();
        let signal_strs: Vec<&str> = signals.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(
            signal_strs.iter().filter(|&&s| s == "lost_writes").count(),
            1,
            "dedup should eliminate duplicate"
        );
        // Order: symptoms first, then triggers, then observability_signals (minus dupes)
        assert_eq!(signal_strs[0], "lost_writes");
        assert_eq!(signal_strs[1], "read_skew");
        assert_eq!(signal_strs[2], "network issues");
        assert_eq!(signal_strs[3], "inconsistent reads");
    }

    #[test]
    fn assemble_emits_unmapped_user_symptoms_as_empty_array() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert_eq!(
            ctx["normalized_incident_query"]["unmapped_user_symptoms"],
            serde_json::json!([])
        );
    }

    #[test]
    fn assemble_emits_observed_phase_as_empty_array() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert_eq!(
            ctx["normalized_incident_query"]["observed_phase"],
            serde_json::json!([])
        );
    }

    #[test]
    fn assemble_emits_missing_signals_as_empty_array() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert_eq!(
            ctx["normalized_incident_query"]["missing_signals"],
            serde_json::json!([])
        );
    }

    #[test]
    fn assemble_does_not_include_internal_fields_in_prompt_context() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        let niq = &ctx["normalized_incident_query"];
        assert!(niq.get("evidence_span").is_none());
        assert!(niq.get("support_level").is_none());
        assert!(niq.get("rejected_nearby_terms").is_none());
        assert!(niq.get("token_usage").is_none());
    }

    #[test]
    fn assemble_does_not_fail_when_source_arrays_are_empty() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let query = query_with(&[], &[], &[], &[], &[]);
        let asm = make_assembly();
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let theory = TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None };
        let out = asm
            .assemble(&minimal_request(), &query, &cards, &evidence, &theory)
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert_eq!(
            ctx["normalized_incident_query"]["recognized_canonical_symptoms"],
            serde_json::json!([])
        );
    }

    #[test]
    fn assemble_includes_task_diagnostic_response() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert_eq!(ctx["task"], "diagnostic_response");
    }

    #[test]
    fn assemble_includes_policy_constraints_from_asset() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert!(ctx["policy_constraints"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false));
    }

    #[test]
    fn assemble_returns_nonempty_prompt_string() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        assert!(!out.prompt.is_empty());
    }

    #[test]
    fn rendered_prompt_contains_json_context_follows_marker() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        assert!(out.prompt.contains("JSON context follows:"));
    }

    #[test]
    fn rendered_prompt_contains_valid_json_context_after_marker() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let _ctx = extract_json_context(&out.prompt); // panics if invalid JSON
    }

    // ---------------------------------------------------------------------------
    // assemble() — chunk selection and role serialization
    // ---------------------------------------------------------------------------

    #[test]
    fn rendered_prompt_embeds_incident_chunks_with_manual_prompt_fields() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        let chunks_json = ctx["incident_evidence_chunks"].as_array().unwrap();
        assert!(!chunks_json.is_empty());
        // Roles are snake_case strings
        for c in chunks_json {
            let role = c["role"].as_str().unwrap();
            assert!(
                [
                    "evidence_for_match",
                    "first_check_hint",
                    "supporting_explanation",
                    "alternative_context"
                ]
                .contains(&role),
                "unexpected role: {role}"
            );
            assert!(c.get("chunk_id").is_none());
            assert!(c.get("case_id").is_none());
            assert!(c.get("score").is_none());
            assert_eq!(c["source_document_id"], "case_a");
            for tag in c["chunk_tags"].as_array().unwrap() {
                let t = tag.as_str().unwrap();
                assert!(t.starts_with("chunk_role:"), "tag should be canonical: {t}");
            }
        }
    }

    #[test]
    fn rendered_prompt_serializes_all_roles_through_snake_case_mapping() {
        let card = minimal_card("case_a");

        // Set up settings with supporting_explanation and alternative_context enabled
        let mut s = default_settings();
        s.chunk_packing.supporting_explanation.limit = 1;
        s.chunk_packing.supporting_explanation.tag_priority =
            vec![IncidentChunkTag::ContributingFactor];
        s.chunk_packing.alternative_context.limit = 1;
        s.chunk_packing.alternative_context.per_case_limit = Some(1);

        let primary_chunks = vec![
            chunk("c1", "case_a", 0.9, &["chunk_role:symptom"], "symptom text"),
            chunk(
                "c2",
                "case_a",
                0.8,
                &["chunk_role:diagnostic_step"],
                "step text",
            ),
            chunk(
                "c3",
                "case_a",
                0.7,
                &["chunk_role:contributing_factor"],
                "contributing text",
            ),
        ];

        let alt_card = minimal_card("alt_a");
        let alt_chunk = chunk(
            "ca1",
            "alt_a",
            0.6,
            &["chunk_role:failure_mode"],
            "alt text",
        );

        let mut s2 = s.clone();
        s2.chunk_packing.mechanism_explanation.limit = 1;
        let asm = PromptContextAssembly::new(s2).unwrap();

        let request = minimal_request();
        let query = minimal_query();
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![alt_card],
        };
        let evidence = IncidentEvidenceRetrievalOutput {
            primary_chunks,
            alternative_chunks: vec![alt_chunk],
            metrics: None,
        };
        let theory = TheoryEvidenceRetrievalOutput {
            chunks: vec![theory_chunk("t1", 0.5, "theory text")],
            metrics: None,
        };

        let out = asm
            .assemble(&request, &query, &cards, &evidence, &theory)
            .unwrap();
        let ctx = extract_json_context(&out.prompt);

        let incident_roles: Vec<&str> = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["role"].as_str().unwrap())
            .collect();

        assert!(incident_roles.contains(&"evidence_for_match"));
        assert!(incident_roles.contains(&"first_check_hint"));
        assert!(incident_roles.contains(&"supporting_explanation"));
        assert!(incident_roles.contains(&"alternative_context"));

        let theory_roles: Vec<&str> = ctx["theory_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["role"].as_str().unwrap())
            .collect();
        assert!(theory_roles.contains(&"mechanism_explanation"));
    }

    #[test]
    fn evidence_for_match_uses_configured_tag_priority_before_score() {
        let card = minimal_card("case_a");
        // c1 has symptom (priority 0) but lower score; c2 has no matching tag but higher score
        let chunks = vec![
            chunk("c1", "case_a", 0.5, &["chunk_role:symptom"], "symptom"),
            chunk("c2", "case_a", 0.95, &["chunk_role:lesson"], "lesson"),
        ];
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        let efm: Vec<_> = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "evidence_for_match")
            .collect();
        assert_eq!(efm[0]["text"], "symptom");
    }

    #[test]
    fn first_check_hint_uses_configured_tag_priority_before_score() {
        let card = minimal_card("case_a");
        // c1: symptom tag (efm priority) but will be consumed by efm
        // c2: diagnostic_step (fch priority 0), lower score
        // c3: investigation (fch priority 1), higher score
        let chunks = vec![
            chunk("c1", "case_a", 0.9, &["chunk_role:symptom"], "symptom"),
            chunk(
                "c2",
                "case_a",
                0.6,
                &["chunk_role:diagnostic_step"],
                "diag step",
            ),
            chunk(
                "c3",
                "case_a",
                0.85,
                &["chunk_role:investigation"],
                "investigation",
            ),
        ];
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        let fch: Vec<_> = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "first_check_hint")
            .collect();
        assert_eq!(fch[0]["text"], "diag step");
    }

    #[test]
    fn supporting_explanation_uses_configured_tag_priority_before_score() {
        let card = minimal_card("case_a");
        // c1: symptom (efm), c2: diagnostic_step (fch), c3: contributing_factor (se prio 0) low score, c4: lesson (no se tag) high score
        let chunks = vec![
            chunk("c1", "case_a", 0.9, &["chunk_role:symptom"], "symptom"),
            chunk("c2", "case_a", 0.8, &["chunk_role:diagnostic_step"], "diag"),
            chunk(
                "c3",
                "case_a",
                0.3,
                &["chunk_role:contributing_factor"],
                "contrib",
            ),
            chunk("c4", "case_a", 0.99, &["chunk_role:lesson"], "lesson"),
        ];
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        let se: Vec<_> = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "supporting_explanation")
            .collect();
        assert_eq!(se[0]["text"], "contrib");
    }

    #[test]
    fn when_two_chunks_match_same_priority_higher_score_wins() {
        let card = minimal_card("case_a");
        // Both c1 and c2 have symptom (same bucket), c1 lower score, c2 higher score
        // c3 has diagnostic_step for fch
        let chunks = vec![
            chunk(
                "c1",
                "case_a",
                0.5,
                &["chunk_role:symptom"],
                "low-score symptom",
            ),
            chunk(
                "c2",
                "case_a",
                0.9,
                &["chunk_role:symptom"],
                "high-score symptom",
            ),
            chunk(
                "c3",
                "case_a",
                0.7,
                &["chunk_role:diagnostic_step"],
                "diag step",
            ),
        ];
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        let efm: Vec<_> = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "evidence_for_match")
            .collect();
        assert_eq!(efm[0]["text"], "high-score symptom");
    }

    #[test]
    fn when_priority_and_score_tie_original_order_wins() {
        let card = minimal_card("case_a");
        // c1 and c2: same tag (symptom), same score; c1 appears first
        let chunks = vec![
            chunk("c1", "case_a", 0.8, &["chunk_role:symptom"], "first"),
            chunk("c2", "case_a", 0.8, &["chunk_role:symptom"], "second"),
            chunk("c3", "case_a", 0.7, &["chunk_role:diagnostic_step"], "diag"),
        ];
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        let efm: Vec<_> = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "evidence_for_match")
            .collect();
        assert_eq!(efm[0]["text"], "first");
    }

    #[test]
    fn chunks_with_multiple_tags_use_best_matching_tag_for_role() {
        let card = minimal_card("case_a");
        // c1 has both failure_mode (efm priority 1) and symptom (efm priority 0) — best is symptom
        // c2 has diagnostic_step for fch
        let chunks = vec![
            chunk(
                "c1",
                "case_a",
                0.7,
                &["chunk_role:failure_mode", "chunk_role:symptom"],
                "multi-tag",
            ),
            chunk("c2", "case_a", 0.6, &["chunk_role:diagnostic_step"], "diag"),
        ];
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        let efm: Vec<_> = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "evidence_for_match")
            .collect();
        assert_eq!(efm[0]["text"], "multi-tag");
    }

    #[test]
    fn unknown_collection_returned_tags_are_ignored_for_role_matching() {
        let card = minimal_card("case_a");
        // c1 has an unknown tag — must still be eligible via fallback
        // c2 has a recognized diagnostic_step for fch
        let mut s = default_settings();
        s.chunk_packing.evidence_for_match.fallback_to_any_chunk = true;
        s.chunk_packing.evidence_for_match.tag_priority = vec![IncidentChunkTag::Symptom];
        let asm = PromptContextAssembly::new(s).unwrap();
        let chunks = vec![
            chunk(
                "c1",
                "case_a",
                0.5,
                &["chunk_role:unknown_xyz"],
                "unknown tag chunk",
            ),
            chunk("c2", "case_a", 0.7, &["chunk_role:diagnostic_step"], "diag"),
        ];
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let out = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None },
            )
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        let efm: Vec<_> = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "evidence_for_match")
            .collect();
        assert!(!efm.is_empty());
    }

    #[test]
    fn fallback_chunks_considered_only_when_fallback_to_any_chunk_is_true() {
        let card = minimal_card("case_a");
        // Only a chunk with an unmatched tag; fallback_to_any_chunk = false → required role must fail
        let mut s = default_settings();
        s.chunk_packing.evidence_for_match.fallback_to_any_chunk = false;
        s.chunk_packing.evidence_for_match.tag_priority = vec![IncidentChunkTag::Symptom];
        let asm = PromptContextAssembly::new(s).unwrap();
        let chunks = vec![chunk(
            "c1",
            "case_a",
            0.9,
            &["chunk_role:lesson"],
            "no match",
        )];
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let err = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None },
            )
            .expect_err("should fail — no eligible chunk");
        assert!(matches!(
            err,
            PromptContextAssemblyError::MissingRequiredEvidence { .. }
        ));
    }

    #[test]
    fn required_role_fails_with_missing_required_evidence_when_no_eligible_chunk_and_fallback_disabled(
    ) {
        let card = minimal_card("case_a");
        let mut s = default_settings();
        s.chunk_packing.evidence_for_match.fallback_to_any_chunk = false;
        s.chunk_packing.evidence_for_match.tag_priority = vec![IncidentChunkTag::Symptom];
        let asm = PromptContextAssembly::new(s).unwrap();
        let chunks = vec![];
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let err = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None },
            )
            .expect_err("should fail");
        assert!(matches!(
            err,
            PromptContextAssemblyError::MissingRequiredEvidence { .. }
        ));
    }

    #[test]
    fn selected_chunks_are_not_duplicated_across_required_roles_when_distinct_exists() {
        let card = minimal_card("case_a");
        // Two distinct chunks — one for each required role
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        let texts: Vec<&str> = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "evidence_for_match" || c["role"] == "first_check_hint")
            .map(|c| c["text"].as_str().unwrap())
            .collect();
        let unique: std::collections::HashSet<_> = texts.iter().collect();
        assert_eq!(texts.len(), unique.len());
    }

    #[test]
    fn duplicate_chunk_reuse_allowed_for_required_role_when_no_distinct_chunk_available() {
        let card = minimal_card("case_a");
        // Only one chunk that matches both required roles; fallback must allow reuse
        let mut s = default_settings();
        s.chunk_packing.evidence_for_match.fallback_to_any_chunk = true;
        s.chunk_packing.evidence_for_match.tag_priority = vec![IncidentChunkTag::Symptom];
        s.chunk_packing.first_check_hint.fallback_to_any_chunk = true;
        s.chunk_packing.first_check_hint.tag_priority = vec![IncidentChunkTag::DiagnosticStep];
        s.chunk_packing.supporting_explanation.limit = 0;
        let asm = PromptContextAssembly::new(s).unwrap();

        // Only one chunk — matches symptom for efm; fch has diagnostic_step priority but only fallback available
        let chunks = vec![chunk(
            "c1",
            "case_a",
            0.9,
            &["chunk_role:symptom"],
            "symptom only",
        )];
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let out = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None },
            )
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        // Both roles should be filled (even if same chunk is reused)
        let efm_count = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "evidence_for_match")
            .count();
        let fch_count = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "first_check_hint")
            .count();
        assert_eq!(efm_count, 1);
        assert_eq!(fch_count, 1);
    }

    #[test]
    fn optional_supporting_explanation_does_not_reuse_duplicate_when_no_distinct_chunk() {
        let card = minimal_card("case_a");
        // efm gets c1 (symptom), fch gets c2 (diagnostic_step), no remaining distinct for se
        let mut s = default_settings();
        s.chunk_packing.supporting_explanation.fallback_to_any_chunk = false;
        s.chunk_packing.supporting_explanation.tag_priority = vec![IncidentChunkTag::Symptom]; // all symptom already taken
        let asm = PromptContextAssembly::new(s).unwrap();
        let chunks = vec![
            chunk("c1", "case_a", 0.9, &["chunk_role:symptom"], "symptom"),
            chunk("c2", "case_a", 0.7, &["chunk_role:diagnostic_step"], "diag"),
        ];
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let out = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None },
            )
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        let se_count = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "supporting_explanation")
            .count();
        assert_eq!(se_count, 0, "optional role must not reuse a duplicate");
    }

    // ---------------------------------------------------------------------------
    // assemble() — alternative context
    // ---------------------------------------------------------------------------

    #[test]
    fn alternative_context_limit_zero_selects_no_alternative_chunks() {
        let mut s = default_settings();
        s.chunk_packing.alternative_context.limit = 0;
        let asm = PromptContextAssembly::new(s).unwrap();
        let card = minimal_card("case_a");
        let alt_card = minimal_card("alt_a");
        let alt_chunk = chunk("ca1", "alt_a", 0.7, &["chunk_role:failure_mode"], "alt");
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![alt_card],
        };
        let evidence = IncidentEvidenceRetrievalOutput {
            primary_chunks: two_primary_chunks("case_a"),
            alternative_chunks: vec![alt_chunk],
            metrics: None,
        };
        let out = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None },
            )
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        let ac_count = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "alternative_context")
            .count();
        assert_eq!(ac_count, 0);
    }

    #[test]
    fn alternative_context_round_robin_follows_alt_cards_order() {
        let mut s = default_settings();
        s.chunk_packing.alternative_context.limit = 2;
        s.chunk_packing.alternative_context.per_case_limit = Some(1);
        s.chunk_packing.alternative_context.tag_priority = vec![IncidentChunkTag::FailureMode];
        let asm = PromptContextAssembly::new(s).unwrap();

        let card = minimal_card("primary");
        let alt_a = minimal_card("alt_a");
        let alt_b = minimal_card("alt_b");

        let chunks = two_primary_chunks("primary");
        let alt_chunks = vec![
            chunk(
                "cb1",
                "alt_b",
                0.9,
                &["chunk_role:failure_mode"],
                "alt_b first",
            ),
            chunk(
                "ca1",
                "alt_a",
                0.7,
                &["chunk_role:failure_mode"],
                "alt_a first",
            ),
        ];

        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![alt_a, alt_b], // alt_a comes first in card order
        };
        let evidence = IncidentEvidenceRetrievalOutput {
            primary_chunks: chunks,
            alternative_chunks: alt_chunks,
            metrics: None,
        };

        let out = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None },
            )
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        let ac: Vec<_> = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "alternative_context")
            .collect();
        // alt_a should appear first (card order), then alt_b
        assert_eq!(ac[0]["source_document_id"], "alt_a");
        assert_eq!(ac[1]["source_document_id"], "alt_b");
    }

    #[test]
    fn alternative_context_respects_per_case_limit() {
        let mut s = default_settings();
        s.chunk_packing.alternative_context.limit = 4;
        s.chunk_packing.alternative_context.per_case_limit = Some(1);
        s.chunk_packing.alternative_context.tag_priority = vec![IncidentChunkTag::FailureMode];
        s.chunk_packing.alternative_context.fallback_to_any_chunk = true;
        let asm = PromptContextAssembly::new(s).unwrap();

        let card = minimal_card("primary");
        let alt_a = minimal_card("alt_a");
        let chunks = two_primary_chunks("primary");
        let alt_chunks = vec![
            chunk("ca1", "alt_a", 0.9, &["chunk_role:failure_mode"], "first"),
            chunk("ca2", "alt_a", 0.8, &["chunk_role:failure_mode"], "second"),
            chunk("ca3", "alt_a", 0.7, &["chunk_role:failure_mode"], "third"),
        ];

        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![alt_a],
        };
        let evidence = IncidentEvidenceRetrievalOutput {
            primary_chunks: chunks,
            alternative_chunks: alt_chunks,
            metrics: None,
        };

        let out = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None },
            )
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        let ac_count = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "alternative_context")
            .count();
        assert_eq!(ac_count, 1, "per_case_limit=1 should cap alt_a at 1");
    }

    #[test]
    fn alternative_context_is_optional_when_no_alt_chunks_exist() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        let ac_count = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["role"] == "alternative_context")
            .count();
        assert_eq!(ac_count, 0);
    }

    #[test]
    fn competing_precedent_context_is_not_rendered() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert!(ctx.get("competing_precedent_context").is_none());
    }

    // ---------------------------------------------------------------------------
    // assemble() — theory chunks
    // ---------------------------------------------------------------------------

    #[test]
    fn theory_chunk_selection_is_capped_at_one() {
        let mut s = default_settings();
        s.chunk_packing.mechanism_explanation.limit = 1;
        let asm = PromptContextAssembly::new(s).unwrap();
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let theory = TheoryEvidenceRetrievalOutput {
            chunks: vec![
                theory_chunk("t1", 0.9, "first theory"),
                theory_chunk("t2", 0.8, "second theory"),
                theory_chunk("t3", 0.7, "third theory"),
            ],
            metrics: None,
        };
        let out = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &theory,
            )
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert_eq!(ctx["theory_chunks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn constructor_rejects_mechanism_explanation_limit_above_one() {
        let mut s = default_settings();
        s.chunk_packing.mechanism_explanation.limit = 2;
        let err = PromptContextAssembly::new(s).unwrap_err();
        assert!(matches!(
            err,
            PromptContextAssemblyError::InvalidSettings(message)
            if message == "mechanism_explanation.limit must be <= 1"
        ));
    }

    #[test]
    fn mechanism_explanation_limit_zero_selects_no_theory_chunks() {
        let mut s = default_settings();
        s.chunk_packing.mechanism_explanation.limit = 0;
        let asm = PromptContextAssembly::new(s).unwrap();
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let theory = TheoryEvidenceRetrievalOutput {
            chunks: vec![theory_chunk("t1", 0.9, "theory text")],
            metrics: None,
        };
        let out = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &theory,
            )
            .unwrap();
        let ctx = extract_json_context(&out.prompt);
        assert_eq!(ctx["theory_chunks"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn empty_theory_evidence_is_not_an_error() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks);
        assert!(out.is_ok());
    }

    // ---------------------------------------------------------------------------
    // assemble() — consistency checks
    // ---------------------------------------------------------------------------

    #[test]
    fn primary_chunk_with_wrong_case_id_fails_with_inconsistent_evidence() {
        let card = minimal_card("case_a");
        // Chunk has case_id "case_b" — not the primary card's case
        let chunks = vec![
            chunk("c1", "case_b", 0.9, &["chunk_role:symptom"], "wrong case"),
            chunk(
                "c2",
                "case_b",
                0.7,
                &["chunk_role:diagnostic_step"],
                "wrong case step",
            ),
        ];
        let err = assemble_with(card, chunks).expect_err("should fail — inconsistent case_id");
        assert!(matches!(
            err,
            PromptContextAssemblyError::InconsistentEvidence(_)
        ));
    }

    #[test]
    fn alt_chunk_with_no_hydrated_alt_card_fails_with_inconsistent_evidence() {
        let mut s = default_settings();
        s.chunk_packing.alternative_context.limit = 1;
        s.chunk_packing.alternative_context.per_case_limit = Some(1);
        s.chunk_packing.alternative_context.tag_priority = vec![IncidentChunkTag::FailureMode];
        let asm = PromptContextAssembly::new(s).unwrap();

        let card = minimal_card("primary");
        // No hydrated alternative card for "alt_a"
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let primary_chunks = two_primary_chunks("primary");
        let alt_chunk = chunk(
            "ca1",
            "alt_a",
            0.7,
            &["chunk_role:failure_mode"],
            "alt text",
        );
        let evidence = IncidentEvidenceRetrievalOutput {
            primary_chunks,
            alternative_chunks: vec![alt_chunk],
            metrics: None,
        };
        let err = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None },
            )
            .expect_err("should fail — no alt card");
        assert!(matches!(
            err,
            PromptContextAssemblyError::InconsistentEvidence(_)
        ));
    }

    // ---------------------------------------------------------------------------
    // assemble() — output separation and consistency
    // ---------------------------------------------------------------------------

    #[test]
    fn output_returns_selected_incident_chunks_separately_from_prompt() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        assert!(!out.incident_evidence_chunks.is_empty());
    }

    #[test]
    fn output_returns_selected_theory_chunks_separately_from_prompt() {
        let mut s = default_settings();
        s.chunk_packing.mechanism_explanation.limit = 1;
        let asm = PromptContextAssembly::new(s).unwrap();
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let theory = TheoryEvidenceRetrievalOutput {
            chunks: vec![theory_chunk("t1", 0.8, "theory")],
            metrics: None,
        };
        let out = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &theory,
            )
            .unwrap();
        assert_eq!(out.theory_chunks.len(), 1);
    }

    #[test]
    fn chunks_returned_separately_match_chunks_embedded_in_prompt() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let ctx = extract_json_context(&out.prompt);
        let embedded_texts: Vec<&str> = ctx["incident_evidence_chunks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["text"].as_str().unwrap())
            .collect();
        let returned_texts: Vec<&str> = out
            .incident_evidence_chunks
            .iter()
            .map(|c| c.text.as_str())
            .collect();
        assert_eq!(embedded_texts, returned_texts);
    }

    #[test]
    fn selected_incident_chunks_preserve_raw_runtime_fields_while_prompt_is_compact() {
        let card = minimal_card("case_a");
        let chunks = two_primary_chunks("case_a");
        let out = assemble_with(card, chunks).unwrap();
        let efm = out
            .incident_evidence_chunks
            .iter()
            .find(|c| c.role == PromptEvidenceRole::EvidenceForMatch)
            .unwrap();
        assert_eq!(efm.case_id, "case_a");
        assert!((efm.score - 0.9).abs() < 1e-5);
        assert_eq!(efm.text, "symptom text");
    }

    #[test]
    fn selected_incident_chunks_return_recognized_tags_as_typed_incident_chunk_tag() {
        let card = minimal_card("case_a");
        let chunks = vec![
            chunk(
                "c1",
                "case_a",
                0.9,
                &["chunk_role:symptom", "chunk_role:unknown_xyz"],
                "text",
            ),
            chunk("c2", "case_a", 0.7, &["chunk_role:diagnostic_step"], "diag"),
        ];
        let out = assemble_with(card, chunks).unwrap();
        let efm = out
            .incident_evidence_chunks
            .iter()
            .find(|c| c.role == PromptEvidenceRole::EvidenceForMatch)
            .unwrap();
        // Only recognized tag is returned; unknown_xyz is dropped
        assert_eq!(efm.chunk_tags, vec![IncidentChunkTag::Symptom]);
    }

    #[test]
    fn selected_theory_chunks_preserve_raw_runtime_fields_while_prompt_is_compact() {
        let mut s = default_settings();
        s.chunk_packing.mechanism_explanation.limit = 1;
        let asm = PromptContextAssembly::new(s).unwrap();
        let card = minimal_card("case_a");
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(two_primary_chunks("case_a"));
        let theory = TheoryEvidenceRetrievalOutput {
            chunks: vec![theory_chunk("t42", 0.77, "theory content")],
            metrics: None,
        };
        let out = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &theory,
            )
            .unwrap();
        let t = &out.theory_chunks[0];
        assert_eq!(t.chunk_id, "t42");
        assert!((t.score - 0.77).abs() < 1e-5);
        assert_eq!(t.text, "theory content");
    }

    #[test]
    fn selected_incident_chunks_are_emitted_in_role_order() {
        let mut s = default_settings();
        s.chunk_packing.supporting_explanation.limit = 1;
        s.chunk_packing.supporting_explanation.tag_priority =
            vec![IncidentChunkTag::ContributingFactor];
        s.chunk_packing.alternative_context.limit = 1;
        s.chunk_packing.alternative_context.per_case_limit = Some(1);
        s.chunk_packing.alternative_context.tag_priority = vec![IncidentChunkTag::FailureMode];
        let asm = PromptContextAssembly::new(s).unwrap();

        let card = minimal_card("primary");
        let alt_card = minimal_card("alt_a");
        let primary_chunks = vec![
            chunk("c1", "primary", 0.9, &["chunk_role:symptom"], "symptom"),
            chunk(
                "c2",
                "primary",
                0.8,
                &["chunk_role:diagnostic_step"],
                "step",
            ),
            chunk(
                "c3",
                "primary",
                0.5,
                &["chunk_role:contributing_factor"],
                "contrib",
            ),
        ];
        let alt_chunk = chunk("ca1", "alt_a", 0.7, &["chunk_role:failure_mode"], "alt");

        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![alt_card],
        };
        let evidence = IncidentEvidenceRetrievalOutput {
            primary_chunks,
            alternative_chunks: vec![alt_chunk],
            metrics: None,
        };

        let out = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None },
            )
            .unwrap();
        let roles: Vec<PromptEvidenceRole> = out
            .incident_evidence_chunks
            .iter()
            .map(|c| c.role)
            .collect();
        assert_eq!(
            roles,
            vec![
                PromptEvidenceRole::EvidenceForMatch,
                PromptEvidenceRole::FirstCheckHint,
                PromptEvidenceRole::SupportingExplanation,
                PromptEvidenceRole::AlternativeContext,
            ]
        );
    }

    #[test]
    fn no_unselected_chunks_in_output() {
        let card = minimal_card("case_a");
        // 4 chunks but settings limit each role to 1, supporting_explanation has tag not present
        let mut s = default_settings();
        s.chunk_packing.supporting_explanation.limit = 1;
        s.chunk_packing.supporting_explanation.tag_priority =
            vec![IncidentChunkTag::ContributingFactor];
        let asm = PromptContextAssembly::new(s).unwrap();
        let chunks = vec![
            chunk("c1", "case_a", 0.9, &["chunk_role:symptom"], "a"),
            chunk("c2", "case_a", 0.8, &["chunk_role:diagnostic_step"], "b"),
            chunk("c3", "case_a", 0.7, &["chunk_role:lesson"], "c"),
            chunk("c4", "case_a", 0.6, &["chunk_role:lesson"], "d"),
        ];
        let cards = CardHydrationOutput {
            primary: Some(card),
            alternatives: vec![],
        };
        let evidence = primary_evidence(chunks);
        let out = asm
            .assemble(
                &minimal_request(),
                &minimal_query(),
                &cards,
                &evidence,
                &TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None },
            )
            .unwrap();
        // At most 2 chunks (efm=1, fch=1, se=0 because no contributing_factor)
        assert!(out.incident_evidence_chunks.len() <= 3);
        // All returned chunks must have a chunk_id from our input
        for c in &out.incident_evidence_chunks {
            assert!(["c1", "c2", "c3", "c4"].contains(&c.chunk_id.as_str()));
        }
    }
}
