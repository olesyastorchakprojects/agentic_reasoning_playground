use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tracing::{field, info_span};

use crate::config::{ChunkPackingSource, ChunkRolePackingSettings, DiagnosticUpdatePromptContextSettings};
use crate::shared_types::{
    Context, EvidenceTopology, HydratedCardBranchesInput, HypothesisStatus,
    IncidentCard, IncidentEvidenceChunk, IncidentEvidenceRetrievalOutput,
    ObservationExtractionOutput, ObservationPolarity, ProblemUnderstanding,
    PromptContextAssemblyOutput, PromptEvidenceRole, PromptIncidentEvidenceChunk,
    ResolvedObservation, SuggestedCheck, TheoryEvidenceRetrievalOutput, TrackedHypothesis,
};

use super::chunk_selection::{
    compute_eligible_incident_chunks, make_prompt_incident_chunk,
    select_alternative_context_chunks, select_theory_chunks,
};

// ─── Error boundary ───────────────────────────────────────────────────────────

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
    #[error("missing hydrated primary card")]
    MissingPrimaryCard,
    #[error("invalid problem understanding")]
    InvalidProblemUnderstanding,
    #[error("invalid resolved observation")]
    InvalidResolvedObservation,
    #[error("invalid hypothesis state: {0}")]
    InvalidHypothesisState(String),
    #[error("json serialization failed: {0}")]
    JsonSerializationFailed(String),
}

// ─── Module-private prompt asset type ────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct DiagnosticUpdateResponsePromptAsset {
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

// ─── JSON context DTOs ────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct JsonContext {
    problem_understanding: String,
    resolved_observation: ResolvedObservationDto,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    observations: Vec<ObservationDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostic_state: Option<DiagnosticStateDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_incident_card: Option<PrimaryIncidentCardDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    incident_evidence: Option<IncidentEvidenceDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    theory_evidence: Option<TheoryEvidenceDto>,
}

#[derive(serde::Serialize)]
struct ResolvedObservationDto {
    text: String,
}

#[derive(serde::Serialize)]
struct ObservationDto {
    statement: String,
    polarity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_relation: Option<String>,
}

#[derive(serde::Serialize)]
struct ActiveHypothesisDto {
    hypothesis_id: String,
    text: String,
}

#[derive(serde::Serialize)]
struct RejectedHypothesisDto {
    hypothesis_id: String,
    text: String,
    rejection_reason: String,
}

#[derive(serde::Serialize)]
struct LastCheckDto {
    text: String,
}

#[derive(serde::Serialize)]
struct DiagnosticStateDto {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    active_hypotheses: Vec<ActiveHypothesisDto>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    rejected_hypotheses: Vec<RejectedHypothesisDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_check: Option<LastCheckDto>,
}

#[derive(serde::Serialize)]
struct PrimaryIncidentCardDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<PrimaryCardContextDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hypotheses: Option<PrimaryCardHypothesesDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checks: Option<PrimaryCardChecksDto>,
}

#[derive(serde::Serialize)]
struct PrimaryCardContextDto {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    systems: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    affected_components: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    initial_symptoms: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    later_symptoms: Vec<String>,
}

#[derive(serde::Serialize)]
struct PrimaryCardHypothesesDto {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    failure_modes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hypothesis_signals: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hypothesis_updates: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    contributing_factors: Vec<String>,
}

#[derive(serde::Serialize)]
struct PrimaryCardChecksDto {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    investigation_questions: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    discriminating_checks: Vec<String>,
}

#[derive(serde::Serialize)]
struct IncidentEvidenceDto {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    evidence_for_match: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    next_check_hint: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    supporting_explanation: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    alternative_context: Vec<String>,
}

#[derive(serde::Serialize)]
struct TheoryEvidenceDto {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    mechanism_explanation: Vec<String>,
}

// ─── Public struct ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct DiagnosticUpdatePromptContextAssembly {
    settings: DiagnosticUpdatePromptContextSettings,
    prompt_asset: DiagnosticUpdateResponsePromptAsset,
}

impl DiagnosticUpdatePromptContextAssembly {
    pub fn new(
        settings: DiagnosticUpdatePromptContextSettings,
    ) -> Result<Self, DiagnosticUpdatePromptContextAssemblyError> {
        validate_settings(&settings)?;
        let prompt_asset = load_prompt_asset(&settings.prompt_asset_path)?;
        Ok(Self { settings, prompt_asset })
    }

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
    ) -> Result<PromptContextAssemblyOutput, DiagnosticUpdatePromptContextAssemblyError> {
        self.assemble_with_context(
            problem_understanding,
            resolved_observation,
            extracted_observations,
            cards,
            incident_evidence,
            theory_evidence,
            active_hypotheses,
            rejected_hypotheses,
            last_check,
            &Context::noop(),
        )
    }

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
    ) -> Result<PromptContextAssemblyOutput, DiagnosticUpdatePromptContextAssemblyError> {
        let oi_span = crate::observability::oi_chain_diagnostic_update_prompt_context_assembly_span(
            &context.open_inference.root_span,
        );

        let span = info_span!(
            "request_pipeline.diagnostic_update_prompt_context_assembly",
            module.name = "diagnostic_update_prompt_context_assembly",
            prompt.asset.name = %self.prompt_asset.name,
            prompt.asset.version = %self.prompt_asset.version,
            prompt.selected.total_chunks_count = field::Empty,
            prompt.rendered_chars = field::Empty,
            module.outcome = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );
        let _guard = span.enter();

        // Validate problem understanding
        let pu_text = problem_understanding.text.as_deref().unwrap_or("").trim();
        if pu_text.is_empty() {
            span.record("module.outcome", "failure");
            span.record("status", "error");
            span.record("error.type", "InvalidProblemUnderstanding");
            crate::observability::record_error(
                &oi_span,
                "DiagnosticUpdatePromptContextAssembly.InvalidProblemUnderstanding",
                "problem_understanding.text is None or empty",
            );
            return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidProblemUnderstanding);
        }

        // Validate resolved observation
        let resolved_text = resolved_observation.text.trim();
        if resolved_text.is_empty() {
            span.record("module.outcome", "failure");
            span.record("status", "error");
            span.record("error.type", "InvalidResolvedObservation");
            crate::observability::record_error(
                &oi_span,
                "DiagnosticUpdatePromptContextAssembly.InvalidResolvedObservation",
                "resolved_observation.text is empty",
            );
            return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidResolvedObservation);
        }

        // Validate active hypothesis states
        for hyp in active_hypotheses {
            match hyp.state_history.last() {
                Some(s) if matches!(s.status, HypothesisStatus::Active | HypothesisStatus::Weakened) => {}
                _ => {
                    let msg = format!(
                        "active hypothesis {} has unexpected latest state",
                        hyp.hypothesis_id.0
                    );
                    span.record("module.outcome", "failure");
                    span.record("status", "error");
                    span.record("error.type", "InvalidHypothesisState");
                    crate::observability::record_error(
                        &oi_span,
                        "DiagnosticUpdatePromptContextAssembly.InvalidHypothesisState",
                        &msg,
                    );
                    return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidHypothesisState(msg));
                }
            }
        }

        // Validate rejected hypothesis states
        for hyp in rejected_hypotheses {
            match hyp.state_history.last() {
                Some(s) if matches!(s.status, HypothesisStatus::Rejected(_)) => {}
                _ => {
                    let msg = format!(
                        "rejected hypothesis {} has non-rejected latest state",
                        hyp.hypothesis_id.0
                    );
                    span.record("module.outcome", "failure");
                    span.record("status", "error");
                    span.record("error.type", "InvalidHypothesisState");
                    crate::observability::record_error(
                        &oi_span,
                        "DiagnosticUpdatePromptContextAssembly.InvalidHypothesisState",
                        &msg,
                    );
                    return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidHypothesisState(msg));
                }
            }
        }

        // Select primary incident chunks (no cross-role deduplication in this module)
        let efm = select_primary_role(
            &incident_evidence.primary_chunks,
            &self.settings.chunk_packing.evidence_for_match,
            PromptEvidenceRole::EvidenceForMatch,
        );
        let nch = select_primary_role(
            &incident_evidence.primary_chunks,
            &self.settings.chunk_packing.next_check_hint,
            PromptEvidenceRole::FirstCheckHint,
        );
        let se = select_primary_role(
            &incident_evidence.primary_chunks,
            &self.settings.chunk_packing.supporting_explanation,
            PromptEvidenceRole::SupportingExplanation,
        );

        // Select alternative context chunks
        let already_selected_empty: HashSet<String> = HashSet::new();
        let ac = select_alternative_context_chunks(
            &incident_evidence.alternative_chunks,
            &self.settings.chunk_packing.alternative_context,
            &cards.alternatives,
            &already_selected_empty,
        );

        // Select theory chunks
        let theory_chunks = select_theory_chunks(
            &theory_evidence.chunks,
            self.settings.chunk_packing.mechanism_explanation.limit,
        );

        // Build evidence topology
        let mut primary_roles: Vec<&'static str> = Vec::new();
        if !efm.is_empty() { primary_roles.push("evidence_for_match"); }
        if !nch.is_empty() { primary_roles.push("next_check_hint"); }
        if !se.is_empty() { primary_roles.push("supporting_explanation"); }
        let mut seen_alt_cases: HashSet<String> = HashSet::new();
        let alt_case_ids: Vec<String> = ac.iter()
            .filter(|c| seen_alt_cases.insert(c.case_id.clone()))
            .map(|c| c.case_id.clone())
            .collect();
        let alternative_context_present = !ac.is_empty();
        let theory_evidence_present = !theory_chunks.is_empty();
        let evidence_topology = EvidenceTopology {
            primary_evidence_roles: primary_roles.iter().map(|s| s.to_string()).collect(),
            alternative_context_present,
            alternative_context_case_ids: alt_case_ids,
            theory_evidence_present,
        };

        // Collect incident chunks in role order
        let mut incident_chunks: Vec<PromptIncidentEvidenceChunk> = Vec::new();
        incident_chunks.extend(efm.iter().cloned());
        incident_chunks.extend(nch.iter().cloned());
        incident_chunks.extend(se.iter().cloned());
        incident_chunks.extend(ac.iter().cloned());

        // Build observations DTO
        let observations_dto: Vec<ObservationDto> = extracted_observations
            .observations
            .iter()
            .map(|obs| {
                let condition = obs.condition.as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let time_relation = obs.time_relation.as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                ObservationDto {
                    statement: obs.statement.trim().to_string(),
                    polarity: polarity_to_str(obs.polarity).to_string(),
                    condition,
                    time_relation,
                }
            })
            .collect();

        // Build active hypotheses DTO
        let active_hypotheses_dto: Vec<ActiveHypothesisDto> = active_hypotheses
            .iter()
            .map(|hyp| ActiveHypothesisDto {
                hypothesis_id: hyp.hypothesis_id.0.to_string(),
                text: hyp.text.trim().to_string(),
            })
            .collect();

        // Build rejected hypotheses DTO
        let mut rejected_hypotheses_dto: Vec<RejectedHypothesisDto> = Vec::new();
        for hyp in rejected_hypotheses {
            let rejection_reason = match hyp.state_history.last() {
                Some(s) => match &s.status {
                    HypothesisStatus::Rejected(reason) => reason.trim().to_string(),
                    _ => unreachable!("validated above"),
                },
                None => unreachable!("validated above"),
            };
            rejected_hypotheses_dto.push(RejectedHypothesisDto {
                hypothesis_id: hyp.hypothesis_id.0.to_string(),
                text: hyp.text.trim().to_string(),
                rejection_reason,
            });
        }

        // Build last_check DTO
        let last_check_dto = last_check.map(|check| LastCheckDto {
            text: check.text.trim().to_string(),
        });

        // Build diagnostic_state DTO (omit if all sub-fields are empty)
        let diagnostic_state_dto = if active_hypotheses_dto.is_empty()
            && rejected_hypotheses_dto.is_empty()
            && last_check_dto.is_none()
        {
            None
        } else {
            Some(DiagnosticStateDto {
                active_hypotheses: active_hypotheses_dto,
                rejected_hypotheses: rejected_hypotheses_dto,
                last_check: last_check_dto,
            })
        };

        // Build primary_incident_card DTO
        let primary_incident_card_dto = build_primary_incident_card(&cards.primary);

        // Build incident_evidence DTO (omit if all buckets empty)
        let efm_texts: Vec<String> = efm.iter().map(|c| c.text.trim().to_string()).collect();
        let nch_texts: Vec<String> = nch.iter().map(|c| c.text.trim().to_string()).collect();
        let se_texts: Vec<String> = se.iter().map(|c| c.text.trim().to_string()).collect();
        let ac_texts: Vec<String> = ac.iter().map(|c| c.text.trim().to_string()).collect();
        let incident_evidence_dto = if efm_texts.is_empty()
            && nch_texts.is_empty()
            && se_texts.is_empty()
            && ac_texts.is_empty()
        {
            None
        } else {
            Some(IncidentEvidenceDto {
                evidence_for_match: efm_texts,
                next_check_hint: nch_texts,
                supporting_explanation: se_texts,
                alternative_context: ac_texts,
            })
        };

        // Build theory_evidence DTO (omit if empty)
        let theory_texts: Vec<String> = theory_chunks.iter().map(|c| c.text.trim().to_string()).collect();
        let theory_evidence_dto = if theory_texts.is_empty() {
            None
        } else {
            Some(TheoryEvidenceDto { mechanism_explanation: theory_texts })
        };

        // Serialize JSON context (compact, not pretty-printed)
        let ctx = JsonContext {
            problem_understanding: pu_text.to_string(),
            resolved_observation: ResolvedObservationDto { text: resolved_text.to_string() },
            observations: observations_dto,
            diagnostic_state: diagnostic_state_dto,
            primary_incident_card: primary_incident_card_dto,
            incident_evidence: incident_evidence_dto,
            theory_evidence: theory_evidence_dto,
        };

        let json_str = serde_json::to_string(&ctx).map_err(|e| {
            let err = DiagnosticUpdatePromptContextAssemblyError::JsonSerializationFailed(
                e.to_string(),
            );
            span.record("module.outcome", "failure");
            span.record("status", "error");
            span.record("error.type", "JsonSerializationFailed");
            crate::observability::record_error(
                &oi_span,
                "DiagnosticUpdatePromptContextAssembly.JsonSerializationFailed",
                &e.to_string(),
            );
            err
        })?;

        let prompt = self.prompt_asset.template.replacen(
            &self.prompt_asset.context_placeholder,
            &json_str,
            1,
        );

        let total_chunks = incident_chunks.len() + theory_chunks.len();
        span.record("prompt.selected.total_chunks_count", total_chunks as i64);
        span.record("prompt.rendered_chars", prompt.len() as i64);
        span.record("module.outcome", "success");
        span.record("status", "ok");

        let oi_output = serde_json::json!({
            "selected_counts": {
                "evidence_for_match": efm.len(),
                "next_check_hint": nch.len(),
                "supporting_explanation": se.len(),
                "alternative_context": ac.len(),
                "mechanism_explanation": theory_chunks.len(),
                "total": total_chunks
            },
            "prompt_chars": prompt.len()
        })
        .to_string();
        oi_span.record("output.value", oi_output.as_str());
        oi_span.record("output.mime_type", "application/json");
        oi_span.record("status", "ok");

        Ok(PromptContextAssemblyOutput {
            prompt,
            response_schema: self.prompt_asset.response_schema.clone(),
            evidence_topology,
            incident_evidence_chunks: incident_chunks,
            theory_chunks,
        })
    }
}

// ─── Settings validation ──────────────────────────────────────────────────────

fn validate_settings(
    s: &DiagnosticUpdatePromptContextSettings,
) -> Result<(), DiagnosticUpdatePromptContextAssemblyError> {
    if s.prompt_asset_path.is_empty() {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "prompt_asset_path must not be empty".to_string(),
        ));
    }
    if !std::path::Path::new(&s.prompt_asset_path).is_absolute() {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "prompt_asset_path must be an absolute path".to_string(),
        ));
    }

    let cp = &s.chunk_packing;

    if cp.evidence_for_match.source != ChunkPackingSource::PrimaryIncident {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "evidence_for_match.source must be PrimaryIncident".to_string(),
        ));
    }
    if cp.next_check_hint.source != ChunkPackingSource::PrimaryIncident {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "next_check_hint.source must be PrimaryIncident".to_string(),
        ));
    }
    if cp.supporting_explanation.source != ChunkPackingSource::PrimaryIncident {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "supporting_explanation.source must be PrimaryIncident".to_string(),
        ));
    }
    if cp.alternative_context.source != ChunkPackingSource::AlternativeIncident {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "alternative_context.source must be AlternativeIncident".to_string(),
        ));
    }
    if cp.mechanism_explanation.source != ChunkPackingSource::Theory {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "mechanism_explanation.source must be Theory".to_string(),
        ));
    }

    if cp.evidence_for_match.limit < 1 {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "evidence_for_match.limit must be >= 1".to_string(),
        ));
    }
    if cp.next_check_hint.limit < 1 {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "next_check_hint.limit must be >= 1".to_string(),
        ));
    }

    if cp.alternative_context.limit > 0 {
        match cp.alternative_context.per_case_limit {
            None => {
                return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
                    "alternative_context.per_case_limit must be Some(n > 0) when limit > 0"
                        .to_string(),
                ))
            }
            Some(0) => {
                return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
                    "alternative_context.per_case_limit must be > 0 when limit > 0".to_string(),
                ))
            }
            _ => {}
        }
    }

    if !cp.mechanism_explanation.tag_priority.is_empty() {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "mechanism_explanation.tag_priority must be empty because theory chunks do not expose tags"
                .to_string(),
        ));
    }
    if cp.mechanism_explanation.limit > 1 {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "mechanism_explanation.limit must be <= 1".to_string(),
        ));
    }

    Ok(())
}

// ─── Prompt asset loading ─────────────────────────────────────────────────────

fn load_prompt_asset(
    path: &str,
) -> Result<DiagnosticUpdateResponsePromptAsset, DiagnosticUpdatePromptContextAssemblyError> {
    let schema_path = derive_schema_path(path)?;

    let asset_content = std::fs::read_to_string(path).map_err(|e| {
        DiagnosticUpdatePromptContextAssemblyError::PromptAssetReadFailed(format!(
            "failed to read prompt asset '{path}': {e}"
        ))
    })?;

    let asset_json: serde_json::Value =
        serde_json::from_str(&asset_content).map_err(|e| {
            DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetJson(e.to_string())
        })?;

    let schema_content = std::fs::read_to_string(&schema_path).map_err(|e| {
        DiagnosticUpdatePromptContextAssemblyError::PromptAssetSchemaReadFailed(format!(
            "failed to read prompt asset schema '{schema_path}': {e}"
        ))
    })?;

    let schema_json: serde_json::Value =
        serde_json::from_str(&schema_content).map_err(|e| {
            DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetSchemaJson(e.to_string())
        })?;

    let validator = jsonschema::options().build(&schema_json).map_err(|e| {
        DiagnosticUpdatePromptContextAssemblyError::PromptAssetSchemaValidationFailed(format!(
            "schema compile error: {e}"
        ))
    })?;

    if !validator.is_valid(&asset_json) {
        let errors: Vec<String> = validator
            .iter_errors(&asset_json)
            .map(|e| e.to_string())
            .collect();
        return Err(
            DiagnosticUpdatePromptContextAssemblyError::PromptAssetSchemaValidationFailed(
                errors.join("; "),
            ),
        );
    }

    let asset: DiagnosticUpdateResponsePromptAsset =
        serde_json::from_value(asset_json).map_err(|e| {
            DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetJson(format!(
                "deserialization failed: {e}"
            ))
        })?;

    // Asset contract validation
    let placeholder = "{{json_context}}";
    let count = asset.template.matches(placeholder).count();
    if count == 0 {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetContract(
            "template must contain exactly one {{json_context}} placeholder".to_string(),
        ));
    }
    if count > 1 {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetContract(
            "template must contain exactly one {{json_context}} placeholder, found more than one"
                .to_string(),
        ));
    }
    if asset.context_placeholder != placeholder {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetContract(
            format!(
                "context_placeholder must be '{{{{json_context}}}}', got '{}'",
                asset.context_placeholder
            ),
        ));
    }
    if asset.required_placeholders != vec!["json_context"] {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetContract(
            "required_placeholders must contain exactly [\"json_context\"]".to_string(),
        ));
    }
    if asset.policy_constraints.is_empty() {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetContract(
            "policy_constraints must be non-empty".to_string(),
        ));
    }
    if !asset.response_schema.is_object() {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetContract(
            "response_schema must be a JSON object".to_string(),
        ));
    }

    Ok(asset)
}

fn derive_schema_path(
    asset_path: &str,
) -> Result<String, DiagnosticUpdatePromptContextAssemblyError> {
    let path = std::path::Path::new(asset_path);
    let file_name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
        DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "invalid prompt asset path".to_string(),
        )
    })?;

    let schema_file_name = if file_name.ends_with(".manual_test.json") {
        file_name.replacen(".manual_test.json", ".schema.json", 1)
    } else if file_name.ends_with(".json") {
        file_name.replacen(".json", ".schema.json", 1)
    } else {
        return Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(
            "prompt asset path must end with .json".to_string(),
        ));
    };

    let parent = path.parent().unwrap_or(std::path::Path::new(""));
    Ok(parent.join(schema_file_name).to_string_lossy().into_owned())
}

// ─── Chunk selection helpers ──────────────────────────────────────────────────

fn select_primary_role(
    chunks: &[IncidentEvidenceChunk],
    role_settings: &ChunkRolePackingSettings,
    role: PromptEvidenceRole,
) -> Vec<PromptIncidentEvidenceChunk> {
    if role_settings.limit == 0 {
        return vec![];
    }
    let ranked = compute_eligible_incident_chunks(chunks, role_settings);
    ranked
        .into_iter()
        .take(role_settings.limit)
        .map(|rc| make_prompt_incident_chunk(rc.chunk, role))
        .collect()
}

// ─── Field mapping helpers ────────────────────────────────────────────────────

fn polarity_to_str(polarity: ObservationPolarity) -> &'static str {
    match polarity {
        ObservationPolarity::Present => "present",
        ObservationPolarity::Absent => "absent",
        ObservationPolarity::Corrected => "corrected",
    }
}

fn build_primary_incident_card(card: &IncidentCard) -> Option<PrimaryIncidentCardDto> {
    // systems: vendor_or_project + system_type, de-duplicated, empty strings omitted
    let mut seen_systems: HashSet<String> = HashSet::new();
    let systems: Vec<String> = [card.vendor_or_project.as_deref(), card.system_type.as_deref()]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .filter(|s| seen_systems.insert(s.to_string()))
        .map(str::to_string)
        .collect();

    let affected_components: Vec<String> = card
        .affected_components
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect();

    let initial_symptoms: Vec<String> = card
        .canonical_symptoms
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect();

    // later_symptoms: incident_phases[*].symptoms[*] then [*].user_visible_impact[*], de-duplicated
    let mut seen_later: HashSet<String> = HashSet::new();
    let later_symptoms: Vec<String> = card
        .incident_phases
        .iter()
        .flat_map(|phase| phase.symptoms.iter().chain(phase.user_visible_impact.iter()))
        .filter(|s| !s.is_empty())
        .filter(|s| seen_later.insert(s.to_string()))
        .cloned()
        .collect();

    let context_dto = if systems.is_empty()
        && affected_components.is_empty()
        && initial_symptoms.is_empty()
        && later_symptoms.is_empty()
    {
        None
    } else {
        Some(PrimaryCardContextDto {
            systems,
            affected_components,
            initial_symptoms,
            later_symptoms,
        })
    };

    let failure_modes: Vec<String> = card
        .failure_mode_candidates
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect();
    let hypothesis_signals: Vec<String> = card
        .candidate_explanations
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect();
    let hypothesis_updates: Vec<String> = card
        .diagnostic_patterns
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect();
    let contributing_factors: Vec<String> = card
        .confidence_notes
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect();

    let hypotheses_dto = if failure_modes.is_empty()
        && hypothesis_signals.is_empty()
        && hypothesis_updates.is_empty()
        && contributing_factors.is_empty()
    {
        None
    } else {
        Some(PrimaryCardHypothesesDto {
            failure_modes,
            hypothesis_signals,
            hypothesis_updates,
            contributing_factors,
        })
    };

    let investigation_questions: Vec<String> = card
        .discriminating_checks
        .iter()
        .map(|c| c.question.clone())
        .filter(|s| !s.is_empty())
        .collect();

    let discriminating_checks: Vec<String> = if !card.investigation_steps.is_empty() {
        card.investigation_steps
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect()
    } else {
        card.discriminating_checks
            .iter()
            .map(|c| c.question.clone())
            .filter(|s| !s.is_empty())
            .collect()
    };

    let checks_dto = if investigation_questions.is_empty() && discriminating_checks.is_empty() {
        None
    } else {
        Some(PrimaryCardChecksDto {
            investigation_questions,
            discriminating_checks,
        })
    };

    if context_dto.is_none() && hypotheses_dto.is_none() && checks_dto.is_none() {
        None
    } else {
        Some(PrimaryIncidentCardDto {
            context: context_dto,
            hypotheses: hypotheses_dto,
            checks: checks_dto,
        })
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        ChunkPackingSource, ChunkRolePackingSettings, DiagnosticUpdateChunkPackingSettings,
        DiagnosticUpdatePromptContextSettings,
    };
    use crate::shared_types::{
        Confidence, DiscriminatingCheck, ExtractedObservation, HypothesisEvidenceSource,
        HypothesisId, IncidentCard, IncidentChunkTag, IncidentEvidenceChunk,
        IncidentEvidenceRetrievalOutput, ModelTokenUsage, ObservationExtractionOutput,
        ObservationPolarity, ResolvedObservation, TheoryEvidenceChunk,
        TheoryEvidenceRetrievalOutput,
    };
    use crate::orchestrator::run_state::model::RunIterationId;
    use uuid::Uuid;

    // ─── Helpers ──────────────────────────────────────────────────────────────

    fn valid_asset_path() -> String {
        let manifest = env!("CARGO_MANIFEST_DIR");
        format!(
            "{manifest}/../../Specification/runtime/request_pipeline/diagnostic_update_prompt_context_assembly/diagnostic_update_response_prompt_baseline.manual_test.json"
        )
    }

    fn default_settings() -> DiagnosticUpdatePromptContextSettings {
        DiagnosticUpdatePromptContextSettings {
            prompt_asset_path: valid_asset_path(),
            chunk_packing: DiagnosticUpdateChunkPackingSettings {
                evidence_for_match: ChunkRolePackingSettings {
                    source: ChunkPackingSource::PrimaryIncident,
                    limit: 1,
                    per_case_limit: None,
                    fallback_to_any_chunk: true,
                    tag_priority: vec![IncidentChunkTag::Symptom, IncidentChunkTag::FailureMode],
                },
                next_check_hint: ChunkRolePackingSettings {
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

    fn new_iter_id() -> RunIterationId {
        RunIterationId(Uuid::new_v4())
    }

    fn make_problem_understanding(text: &str) -> ProblemUnderstanding {
        ProblemUnderstanding {
            iteration_id: new_iter_id(),
            text: Some(text.to_string()),
            source: crate::shared_types::ProblemUnderstandingSource::InitialRequest(
                text.to_string(),
            ),
        }
    }

    fn make_resolved_observation(text: &str) -> ResolvedObservation {
        ResolvedObservation { text: text.to_string() }
    }

    fn make_extraction_output(observations: Vec<ExtractedObservation>) -> ObservationExtractionOutput {
        ObservationExtractionOutput {
            normalized_user_input: "normalized".to_string(),
            resolved_observation: make_resolved_observation("resolved"),
            confidence: Confidence::Medium,
            observations,
            needs_more_context: false,
            missing_context_questions: vec![],
            token_usage: ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
        }
    }

    fn empty_extraction() -> ObservationExtractionOutput {
        make_extraction_output(vec![])
    }

    fn make_cards(primary: IncidentCard) -> HydratedCardBranchesInput {
        HydratedCardBranchesInput {
            primary,
            alternatives: vec![],
        }
    }

    fn minimal_card() -> IncidentCard {
        IncidentCard {
            case_id: "case-1".to_string(),
            title: "Test incident".to_string(),
            source_type: "test".to_string(),
            source_name: "test".to_string(),
            source_path: "test".to_string(),
            vendor_or_project: Some("vendor".to_string()),
            system_type: Some("sys".to_string()),
            version_tested: None,
            report_date: None,
            short_summary: "test summary".to_string(),
            canonical_symptoms: vec!["symptom-x".to_string()],
            affected_components: vec!["comp-a".to_string()],
            failure_mode_candidates: vec!["failure-A".to_string()],
            observed_phases: vec![],
            incident_phases: vec![],
            turning_points: vec![],
            candidate_explanations: vec!["explanation-1".to_string()],
            diagnostic_patterns: vec!["pattern-1".to_string()],
            discriminating_checks: vec![DiscriminatingCheck {
                question: "Is X present?".to_string(),
                why: "helps discriminate".to_string(),
            }],
            expected_observations: vec![],
            investigation_steps: vec!["Step 1".to_string()],
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

    fn empty_incident_evidence() -> IncidentEvidenceRetrievalOutput {
        IncidentEvidenceRetrievalOutput {
            primary_chunks: vec![],
            alternative_chunks: vec![],
            metrics: None,
        }
    }

    fn empty_theory_evidence() -> TheoryEvidenceRetrievalOutput {
        TheoryEvidenceRetrievalOutput { chunks: vec![], metrics: None }
    }

    fn make_incident_chunk(chunk_id: &str, case_id: &str, score: f32, tags: Vec<String>, text: &str) -> IncidentEvidenceChunk {
        IncidentEvidenceChunk {
            chunk_id: chunk_id.to_string(),
            case_id: case_id.to_string(),
            score,
            chunk_tags: tags,
            text: text.to_string(),
        }
    }

    fn make_tracked_hypothesis(text: &str, status: HypothesisStatus) -> TrackedHypothesis {
        TrackedHypothesis {
            hypothesis_id: HypothesisId(Uuid::new_v4()),
            text: text.to_string(),
            state_history: vec![crate::shared_types::HypothesisState {
                iteration_id: new_iter_id(),
                status,
                confidence: Confidence::Medium,
                source: HypothesisEvidenceSource::PrimaryIncident,
                problem_understanding: make_problem_understanding("pu"),
            }],
        }
    }

    fn assemble_minimal(module: &DiagnosticUpdatePromptContextAssembly) -> PromptContextAssemblyOutput {
        module.assemble(
            &make_problem_understanding("service latency is increasing"),
            &make_resolved_observation("latency spiked to 2s"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &empty_theory_evidence(),
            &[],
            &[],
            None,
        ).unwrap()
    }

    // ─── Constructor tests ────────────────────────────────────────────────────

    #[test]
    fn constructor_rejects_empty_prompt_asset_path() {
        let mut s = default_settings();
        s.prompt_asset_path = String::new();
        assert!(matches!(
            DiagnosticUpdatePromptContextAssembly::new(s),
            Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(_))
        ));
    }

    #[test]
    fn constructor_rejects_relative_prompt_asset_path() {
        let mut s = default_settings();
        s.prompt_asset_path = "relative/path.json".to_string();
        assert!(matches!(
            DiagnosticUpdatePromptContextAssembly::new(s),
            Err(DiagnosticUpdatePromptContextAssemblyError::InvalidSettings(_))
        ));
    }

    #[test]
    fn constructor_rejects_unreadable_prompt_asset_file() {
        let mut s = default_settings();
        s.prompt_asset_path = "/nonexistent/path/asset.json".to_string();
        assert!(matches!(
            DiagnosticUpdatePromptContextAssembly::new(s),
            Err(DiagnosticUpdatePromptContextAssemblyError::PromptAssetReadFailed(_))
        ));
    }

    #[test]
    fn constructor_rejects_invalid_prompt_asset_json() {
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("asset.json");
        let schema_path = dir.path().join("asset.schema.json");
        std::fs::write(&asset_path, "not json").unwrap();
        std::fs::write(&schema_path, "{}").unwrap();
        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        assert!(matches!(
            DiagnosticUpdatePromptContextAssembly::new(s),
            Err(DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetJson(_))
        ));
    }

    #[test]
    fn constructor_rejects_unreadable_schema_file() {
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("asset.json");
        std::fs::write(&asset_path, r#"{"version":"v1"}"#).unwrap();
        // No schema file written
        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        assert!(matches!(
            DiagnosticUpdatePromptContextAssembly::new(s),
            Err(DiagnosticUpdatePromptContextAssemblyError::PromptAssetSchemaReadFailed(_))
        ));
    }

    #[test]
    fn constructor_rejects_invalid_schema_json() {
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("asset.json");
        let schema_path = dir.path().join("asset.schema.json");
        std::fs::write(&asset_path, r#"{"version":"v1"}"#).unwrap();
        std::fs::write(&schema_path, "not json schema").unwrap();
        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        assert!(matches!(
            DiagnosticUpdatePromptContextAssembly::new(s),
            Err(DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetSchemaJson(_))
        ));
    }

    #[test]
    fn constructor_rejects_asset_not_matching_schema() {
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("asset.json");
        let schema_path = dir.path().join("asset.schema.json");
        let schema = r#"{"type":"object","required":["required_field"],"properties":{"required_field":{"type":"string"}}}"#;
        std::fs::write(&asset_path, r#"{"version":"v1"}"#).unwrap();
        std::fs::write(&schema_path, schema).unwrap();
        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        assert!(matches!(
            DiagnosticUpdatePromptContextAssembly::new(s),
            Err(DiagnosticUpdatePromptContextAssemblyError::PromptAssetSchemaValidationFailed(_))
        ));
    }

    #[test]
    fn constructor_rejects_asset_missing_json_context_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("asset.json");
        let schema_path = dir.path().join("asset.schema.json");
        let asset = r#"{
            "version":"v1","name":"test",
            "template":"no placeholder here",
            "context_placeholder":"{{json_context}}",
            "required_placeholders":["json_context"],
            "response_schema":{},"policy_constraints":["c1"]
        }"#;
        std::fs::write(&asset_path, asset).unwrap();
        std::fs::write(&schema_path, "{}").unwrap();
        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        assert!(matches!(
            DiagnosticUpdatePromptContextAssembly::new(s),
            Err(DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetContract(_))
        ));
    }

    #[test]
    fn constructor_rejects_asset_with_multiple_json_context_placeholders() {
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("asset.json");
        let schema_path = dir.path().join("asset.schema.json");
        let asset = r#"{
            "version":"v1","name":"test",
            "template":"{{json_context}} and {{json_context}}",
            "context_placeholder":"{{json_context}}",
            "required_placeholders":["json_context"],
            "response_schema":{},"policy_constraints":["c1"]
        }"#;
        std::fs::write(&asset_path, asset).unwrap();
        std::fs::write(&schema_path, "{}").unwrap();
        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        assert!(matches!(
            DiagnosticUpdatePromptContextAssembly::new(s),
            Err(DiagnosticUpdatePromptContextAssemblyError::InvalidPromptAssetContract(_))
        ));
    }

    #[test]
    fn constructor_succeeds_with_valid_asset() {
        let s = default_settings();
        assert!(DiagnosticUpdatePromptContextAssembly::new(s).is_ok());
    }

    #[test]
    fn constructor_derives_schema_path_from_asset_directory() {
        // Verify that schema is loaded from same directory as asset with .schema.json suffix
        let dir = tempfile::tempdir().unwrap();
        let asset_path = dir.path().join("my_asset.json");
        let schema_path = dir.path().join("my_asset.schema.json");
        let asset = r#"{
            "version":"v1","name":"test",
            "template":"Hello {{json_context}}",
            "context_placeholder":"{{json_context}}",
            "required_placeholders":["json_context"],
            "response_schema":{"type":"object"},"policy_constraints":["c1"]
        }"#;
        std::fs::write(&asset_path, asset).unwrap();
        std::fs::write(&schema_path, "{}").unwrap();
        let mut s = default_settings();
        s.prompt_asset_path = asset_path.to_string_lossy().into_owned();
        assert!(DiagnosticUpdatePromptContextAssembly::new(s).is_ok());
    }

    // ─── Assemble input validation tests ─────────────────────────────────────

    #[test]
    fn assemble_fails_when_problem_understanding_text_is_none() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let mut pu = make_problem_understanding("x");
        pu.text = None;
        let result = module.assemble(
            &pu,
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &empty_theory_evidence(),
            &[],
            &[],
            None,
        );
        assert!(matches!(
            result,
            Err(DiagnosticUpdatePromptContextAssemblyError::InvalidProblemUnderstanding)
        ));
    }

    #[test]
    fn assemble_fails_when_problem_understanding_text_is_empty() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let pu = make_problem_understanding("   ");
        let result = module.assemble(
            &pu,
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &empty_theory_evidence(),
            &[],
            &[],
            None,
        );
        assert!(matches!(
            result,
            Err(DiagnosticUpdatePromptContextAssemblyError::InvalidProblemUnderstanding)
        ));
    }

    #[test]
    fn assemble_fails_when_resolved_observation_text_is_empty() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let result = module.assemble(
            &make_problem_understanding("pu text"),
            &make_resolved_observation("   "),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &empty_theory_evidence(),
            &[],
            &[],
            None,
        );
        assert!(matches!(
            result,
            Err(DiagnosticUpdatePromptContextAssemblyError::InvalidResolvedObservation)
        ));
    }

    #[test]
    fn assemble_fails_when_active_hypothesis_has_rejected_latest_state() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let rejected_as_active = make_tracked_hypothesis(
            "H",
            HypothesisStatus::Rejected("contradicted".to_string()),
        );
        let result = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &empty_theory_evidence(),
            &[rejected_as_active],
            &[],
            None,
        );
        assert!(matches!(
            result,
            Err(DiagnosticUpdatePromptContextAssemblyError::InvalidHypothesisState(_))
        ));
    }

    #[test]
    fn assemble_fails_when_rejected_hypothesis_has_active_latest_state() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let active_as_rejected = make_tracked_hypothesis("H", HypothesisStatus::Active);
        let result = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &empty_theory_evidence(),
            &[],
            &[active_as_rejected],
            None,
        );
        assert!(matches!(
            result,
            Err(DiagnosticUpdatePromptContextAssemblyError::InvalidHypothesisState(_))
        ));
    }

    // ─── Output content tests ─────────────────────────────────────────────────

    #[test]
    fn output_includes_problem_understanding() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let output = assemble_minimal(&module);
        assert!(output.prompt.contains("service latency is increasing"));
    }

    #[test]
    fn output_does_not_include_user_problem_field() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let output = assemble_minimal(&module);
        assert!(!output.prompt.contains("\"user_problem\""));
    }

    #[test]
    fn output_includes_resolved_observation_text() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let output = assemble_minimal(&module);
        assert!(output.prompt.contains("latency spiked to 2s"));
    }

    #[test]
    fn output_includes_extracted_observations_in_order() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let obs1 = ExtractedObservation {
            statement: "first obs".to_string(),
            confidence: Confidence::Medium,
            condition: None,
            polarity: ObservationPolarity::Present,
            time_relation: None,
            source_span: "span".to_string(),
        };
        let obs2 = ExtractedObservation {
            statement: "second obs".to_string(),
            confidence: Confidence::Low,
            condition: None,
            polarity: ObservationPolarity::Absent,
            time_relation: None,
            source_span: "span".to_string(),
        };
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("resolved"),
            &make_extraction_output(vec![obs1, obs2]),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &empty_theory_evidence(),
            &[],
            &[],
            None,
        ).unwrap();
        let pos1 = output.prompt.find("first obs").unwrap();
        let pos2 = output.prompt.find("second obs").unwrap();
        assert!(pos1 < pos2);
    }

    #[test]
    fn output_includes_active_hypothesis_id_and_text() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let hyp = make_tracked_hypothesis("memory leak hypothesis", HypothesisStatus::Active);
        let hyp_id_str = hyp.hypothesis_id.0.to_string();
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &empty_theory_evidence(),
            &[hyp],
            &[],
            None,
        ).unwrap();
        assert!(output.prompt.contains(&hyp_id_str));
        assert!(output.prompt.contains("memory leak hypothesis"));
    }

    #[test]
    fn output_includes_rejected_hypothesis_with_rejection_reason() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let hyp = make_tracked_hypothesis(
            "disk io hypothesis",
            HypothesisStatus::Rejected("disk metrics normal".to_string()),
        );
        let hyp_id_str = hyp.hypothesis_id.0.to_string();
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &empty_theory_evidence(),
            &[],
            &[hyp],
            None,
        ).unwrap();
        assert!(output.prompt.contains(&hyp_id_str));
        assert!(output.prompt.contains("disk io hypothesis"));
        assert!(output.prompt.contains("disk metrics normal"));
    }

    #[test]
    fn output_includes_last_check_text_when_supplied() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let check = SuggestedCheck {
            iteration_id: new_iter_id(),
            text: "check the memory profiler output".to_string(),
        };
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &empty_theory_evidence(),
            &[],
            &[],
            Some(&check),
        ).unwrap();
        assert!(output.prompt.contains("check the memory profiler output"));
    }

    #[test]
    fn output_includes_primary_incident_card_summary() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let output = assemble_minimal(&module);
        assert!(output.prompt.contains("primary_incident_card"));
        assert!(output.prompt.contains("symptom-x"));
    }

    #[test]
    fn output_omits_empty_top_level_and_nested_fields() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        // No hypotheses, no last_check → diagnostic_state should be absent
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &empty_theory_evidence(),
            &[],
            &[],
            None,
        ).unwrap();
        assert!(!output.prompt.contains("\"diagnostic_state\""));
        // No evidence → incident_evidence absent
        assert!(!output.prompt.contains("\"incident_evidence\""));
        // No theory → theory_evidence absent
        assert!(!output.prompt.contains("\"theory_evidence\""));
        // No observations → observations absent
        assert!(!output.prompt.contains("\"observations\""));
    }

    #[test]
    fn rendered_prompt_contains_response_schema() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let output = assemble_minimal(&module);
        assert!(output.response_schema.is_object());
    }

    #[test]
    fn rendered_prompt_contains_json_context_after_marker() {
        let module = DiagnosticUpdatePromptContextAssembly::new(default_settings()).unwrap();
        let output = assemble_minimal(&module);
        let marker = "JSON context follows:";
        assert!(output.prompt.contains(marker));
        let after_marker = &output.prompt[output.prompt.find(marker).unwrap()..];
        // The JSON context should start with {
        assert!(after_marker.contains("{"));
    }

    #[test]
    fn rendered_prompt_serializes_evidence_for_match_role() {
        let mut s = default_settings();
        s.chunk_packing.evidence_for_match.limit = 1;
        let module = DiagnosticUpdatePromptContextAssembly::new(s).unwrap();
        let chunk = make_incident_chunk(
            "c1", "case-1", 0.9,
            vec!["chunk_role:symptom".to_string()],
            "symptom evidence text",
        );
        let evidence = IncidentEvidenceRetrievalOutput {
            primary_chunks: vec![chunk],
            alternative_chunks: vec![],
            metrics: None,
        };
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &evidence,
            &empty_theory_evidence(),
            &[],
            &[],
            None,
        ).unwrap();
        assert!(output.prompt.contains("evidence_for_match"));
        assert!(output.prompt.contains("symptom evidence text"));
    }

    #[test]
    fn rendered_prompt_serializes_next_check_hint_role() {
        let mut s = default_settings();
        s.chunk_packing.next_check_hint.limit = 1;
        let module = DiagnosticUpdatePromptContextAssembly::new(s).unwrap();
        let chunk = make_incident_chunk(
            "c2", "case-1", 0.8,
            vec!["chunk_role:diagnostic_step".to_string()],
            "check step evidence",
        );
        let evidence = IncidentEvidenceRetrievalOutput {
            primary_chunks: vec![chunk],
            alternative_chunks: vec![],
            metrics: None,
        };
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &evidence,
            &empty_theory_evidence(),
            &[],
            &[],
            None,
        ).unwrap();
        assert!(output.prompt.contains("next_check_hint"));
        assert!(!output.prompt.contains("first_check_hint"));
    }

    #[test]
    fn rendered_prompt_serializes_theory_mechanism_explanation_role() {
        let mut s = default_settings();
        s.chunk_packing.mechanism_explanation.limit = 1;
        let module = DiagnosticUpdatePromptContextAssembly::new(s).unwrap();
        let theory_chunk = TheoryEvidenceChunk {
            chunk_id: "t1".to_string(),
            score: 0.7,
            text: "theory explanation text".to_string(),
        };
        let theory = TheoryEvidenceRetrievalOutput { chunks: vec![theory_chunk], metrics: None };
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &theory,
            &[],
            &[],
            None,
        ).unwrap();
        assert!(output.prompt.contains("mechanism_explanation"));
        assert!(output.prompt.contains("theory explanation text"));
    }

    #[test]
    fn output_returns_selected_incident_chunks_separately() {
        let mut s = default_settings();
        s.chunk_packing.evidence_for_match.limit = 1;
        let module = DiagnosticUpdatePromptContextAssembly::new(s).unwrap();
        let chunk = make_incident_chunk(
            "c1", "case-1", 0.9,
            vec!["chunk_role:symptom".to_string()],
            "chunk text",
        );
        let evidence = IncidentEvidenceRetrievalOutput {
            primary_chunks: vec![chunk],
            alternative_chunks: vec![],
            metrics: None,
        };
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &evidence,
            &empty_theory_evidence(),
            &[],
            &[],
            None,
        ).unwrap();
        // Chunk may appear in multiple roles (no cross-role dedup in this module)
        let efm_chunks: Vec<_> = output.incident_evidence_chunks.iter()
            .filter(|c| c.role == PromptEvidenceRole::EvidenceForMatch)
            .collect();
        assert_eq!(efm_chunks.len(), 1);
        assert_eq!(efm_chunks[0].chunk_id, "c1");
    }

    #[test]
    fn output_returns_selected_theory_chunks_separately() {
        let mut s = default_settings();
        s.chunk_packing.mechanism_explanation.limit = 1;
        let module = DiagnosticUpdatePromptContextAssembly::new(s).unwrap();
        let theory_chunk = TheoryEvidenceChunk {
            chunk_id: "t1".to_string(),
            score: 0.7,
            text: "theory text".to_string(),
        };
        let theory = TheoryEvidenceRetrievalOutput { chunks: vec![theory_chunk], metrics: None };
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &theory,
            &[],
            &[],
            None,
        ).unwrap();
        assert_eq!(output.theory_chunks.len(), 1);
        assert_eq!(output.theory_chunks[0].chunk_id, "t1");
        assert_eq!(output.theory_chunks[0].role, PromptEvidenceRole::MechanismExplanation);
    }

    #[test]
    fn evidence_topology_primary_roles_in_documented_order() {
        let mut s = default_settings();
        s.chunk_packing.evidence_for_match.limit = 1;
        s.chunk_packing.next_check_hint.limit = 1;
        s.chunk_packing.supporting_explanation = ChunkRolePackingSettings {
            source: ChunkPackingSource::PrimaryIncident,
            limit: 1,
            per_case_limit: None,
            fallback_to_any_chunk: true,
            tag_priority: vec![IncidentChunkTag::ContributingFactor],
        };
        let module = DiagnosticUpdatePromptContextAssembly::new(s).unwrap();
        let chunks = vec![
            make_incident_chunk("c1", "case-1", 0.9, vec!["chunk_role:symptom".to_string()], "symptom text"),
            make_incident_chunk("c2", "case-1", 0.8, vec!["chunk_role:diagnostic_step".to_string()], "diag step text"),
            make_incident_chunk("c3", "case-1", 0.7, vec!["chunk_role:contributing_factor".to_string()], "contributing factor text"),
        ];
        let evidence = IncidentEvidenceRetrievalOutput {
            primary_chunks: chunks,
            alternative_chunks: vec![],
            metrics: None,
        };
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &evidence,
            &empty_theory_evidence(),
            &[],
            &[],
            None,
        ).unwrap();
        assert_eq!(
            output.evidence_topology.primary_evidence_roles,
            vec!["evidence_for_match", "next_check_hint", "supporting_explanation"]
        );
    }

    #[test]
    fn evidence_topology_alternative_context_present_when_chunks_selected() {
        let mut s = default_settings();
        s.chunk_packing.alternative_context = ChunkRolePackingSettings {
            source: ChunkPackingSource::AlternativeIncident,
            limit: 1,
            per_case_limit: Some(1),
            fallback_to_any_chunk: true,
            tag_priority: vec![IncidentChunkTag::FailureMode],
        };
        let module = DiagnosticUpdatePromptContextAssembly::new(s).unwrap();
        let alt_chunk = make_incident_chunk(
            "alt1", "case-alt", 0.8,
            vec!["chunk_role:failure_mode".to_string()],
            "alt context",
        );
        let evidence = IncidentEvidenceRetrievalOutput {
            primary_chunks: vec![],
            alternative_chunks: vec![alt_chunk],
            metrics: None,
        };
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &evidence,
            &empty_theory_evidence(),
            &[],
            &[],
            None,
        ).unwrap();
        assert!(output.evidence_topology.alternative_context_present);
    }

    #[test]
    fn evidence_topology_theory_evidence_present_when_chunks_selected() {
        let mut s = default_settings();
        s.chunk_packing.mechanism_explanation.limit = 1;
        let module = DiagnosticUpdatePromptContextAssembly::new(s).unwrap();
        let theory = TheoryEvidenceRetrievalOutput {
            chunks: vec![TheoryEvidenceChunk {
                chunk_id: "t1".to_string(),
                score: 0.6,
                text: "theory".to_string(),
            }],
            metrics: None,
        };
        let output = module.assemble(
            &make_problem_understanding("pu"),
            &make_resolved_observation("obs"),
            &empty_extraction(),
            &make_cards(minimal_card()),
            &empty_incident_evidence(),
            &theory,
            &[],
            &[],
            None,
        ).unwrap();
        assert!(output.evidence_topology.theory_evidence_present);
    }
}
