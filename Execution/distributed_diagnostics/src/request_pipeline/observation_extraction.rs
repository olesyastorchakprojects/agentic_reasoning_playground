use std::sync::Arc;

use serde::Deserialize;
use tracing::{field, info_span, Instrument};

use crate::api_clients::model::{
    ModelClient, ModelClientError, ModelFinishReason, ModelGenerationRequest, ModelMessage,
    ModelMessageRole, ModelResponseMode,
};
use crate::shared_types::{
    Confidence, Context, ExtractedObservation, ModelTokenUsage, ObservationBoundaryResolution,
    ObservationBoundaryResolverOutput, ObservationExtractionOutput, ObservationPolarity,
    ResolvedObservation,
};

// ─── Placeholders ─────────────────────────────────────────────────────────────

const USER_MESSAGE_PLACEHOLDER: &str = "{{user_message}}";

// ─── Module-private prompt asset ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ObservationExtractionPromptAsset {
    version: String,
    system_prompt: String,
    user_template: String,
    response_schema: serde_json::Value,
}

// ─── Raw model output ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawObservationExtractionResponse {
    confidence: String,
    observations: Vec<RawExtractedObservation>,
    needs_more_context: bool,
    missing_context_questions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExtractedObservation {
    statement: String,
    confidence: String,
    condition: Option<String>,
    polarity: String,
    time_relation: Option<String>,
    source_span: String,
}

// ─── Settings ─────────────────────────────────────────────────────────────────

pub struct ObservationExtractionSettings {
    pub prompt_asset_path: String,
    pub max_output_tokens: u32,
}

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ObservationExtractionError {
    #[error("invalid settings: {0}")]
    InvalidSettings(String),

    #[error("invalid prompt asset: {0}")]
    InvalidPromptAsset(String),

    #[error("unsupported boundary input cannot be extracted")]
    UnsupportedBoundaryInput,

    #[error("model client error: {0}")]
    ModelClient(#[from] ModelClientError),

    #[error("invalid model output: {reason}")]
    InvalidModelOutput {
        reason: String,
        finish_reason: Option<ModelFinishReason>,
        token_usage: ModelTokenUsage,
    },
}

// ─── Public struct ────────────────────────────────────────────────────────────

pub struct ObservationExtraction {
    model_client: Arc<dyn ModelClient>,
    prompt_asset: ObservationExtractionPromptAsset,
    max_output_tokens: u32,
}

impl std::fmt::Debug for ObservationExtraction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObservationExtraction")
            .field("prompt_asset.version", &self.prompt_asset.version)
            .field("max_output_tokens", &self.max_output_tokens)
            .finish_non_exhaustive()
    }
}

impl ObservationExtraction {
    pub fn new(
        settings: ObservationExtractionSettings,
        model_client: Arc<dyn ModelClient>,
    ) -> Result<Self, ObservationExtractionError> {
        if settings.prompt_asset_path.trim().is_empty() {
            return Err(ObservationExtractionError::InvalidSettings(
                "prompt_asset_path must not be empty".to_string(),
            ));
        }
        if settings.max_output_tokens == 0 {
            return Err(ObservationExtractionError::InvalidSettings(
                "max_output_tokens must be greater than zero".to_string(),
            ));
        }

        let prompt_asset = load_prompt_asset(&settings.prompt_asset_path)?;

        Ok(Self {
            model_client,
            prompt_asset,
            max_output_tokens: settings.max_output_tokens,
        })
    }

    pub async fn extract(
        &self,
        input: &ObservationBoundaryResolverOutput,
    ) -> Result<ObservationExtractionOutput, ObservationExtractionError> {
        self.extract_with_context(input, &Context::noop()).await
    }

    pub async fn extract_with_context(
        &self,
        input: &ObservationBoundaryResolverOutput,
        context: &Context,
    ) -> Result<ObservationExtractionOutput, ObservationExtractionError> {
        let oi_span = crate::observability::oi_llm_observation_extraction_span(
            &context.open_inference.root_span,
        );
        oi_span.record("llm.model_name", "unknown");
        oi_span.record("llm.provider", "unknown");
        oi_span.record(
            "llm.invocation_parameters",
            format!(
                r#"{{"temperature":0.0,"response_format":"json_schema","max_output_tokens":{}}}"#,
                self.max_output_tokens
            )
            .as_str(),
        );

        let span = info_span!(
            "request_pipeline.observation_extraction",
            module.name = "observation_extraction",
            query.normalized = %input.normalized_user_input,
            asset.prompt.version = %self.prompt_asset.version,
            model.response_mode = field::Empty,
            model.temperature = field::Empty,
            model.max_output_tokens = field::Empty,
            model.finish_reason = field::Empty,
            model.prompt_tokens = field::Empty,
            model.completion_tokens = field::Empty,
            model.total_tokens = field::Empty,
            extraction.needs_more_context = field::Empty,
            extraction.observations_count = field::Empty,
            module.outcome = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        self.extract_instrumented(input, &oi_span)
            .instrument(span)
            .instrument(oi_span.clone())
            .await
    }

    async fn extract_instrumented(
        &self,
        input: &ObservationBoundaryResolverOutput,
        oi_span: &tracing::Span,
    ) -> Result<ObservationExtractionOutput, ObservationExtractionError> {
        // ── Check resolution boundary ─────────────────────────────────────────

        let resolved = match &input.resolution {
            ObservationBoundaryResolution::Supported(r) => r,
            ObservationBoundaryResolution::Unsupported => {
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                tracing::Span::current()
                    .record("error.type", "ObservationExtraction.UnsupportedBoundaryInput");
                tracing::Span::current()
                    .record("error.message", "resolution is Unsupported");
                crate::observability::record_error(
                    oi_span,
                    "ObservationExtraction.UnsupportedBoundaryInput",
                    "resolution is Unsupported",
                );
                return Err(ObservationExtractionError::UnsupportedBoundaryInput);
            }
        };

        let resolved_text = resolved.text.clone();

        // ── Substitute template ───────────────────────────────────────────────

        let user_message = self
            .prompt_asset
            .user_template
            .replacen(USER_MESSAGE_PLACEHOLDER, &resolved_text, 1);

        // ── Record full prompt in oi_span ─────────────────────────────────────

        let full_prompt = format!(
            "SYSTEM:\n{}\n\nUSER:\n{}",
            self.prompt_asset.system_prompt,
            user_message
        );
        oi_span.record("input.value", full_prompt.as_str());
        oi_span.record("input.mime_type", "text/plain");

        // ── Model call ────────────────────────────────────────────────────────

        let llm_span = info_span!(
            "llm.call.observation_extraction",
            llm.task = "observation_extraction",
            model.finish_reason = field::Empty,
            model.prompt_tokens = field::Empty,
            model.completion_tokens = field::Empty,
            model.total_tokens = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        let model_request = ModelGenerationRequest {
            messages: vec![
                ModelMessage {
                    role: ModelMessageRole::System,
                    content: self.prompt_asset.system_prompt.clone(),
                },
                ModelMessage {
                    role: ModelMessageRole::User,
                    content: user_message,
                },
            ],
            temperature: 0.0,
            max_output_tokens: Some(self.max_output_tokens),
            response_mode: ModelResponseMode::JsonSchema(
                self.prompt_asset.response_schema.clone(),
            ),
        };

        let response = {
            async {
                match self.model_client.generate(&model_request).await {
                    Ok(r) => {
                        oi_span.record("llm.raw_response", r.content.as_str());
                        oi_span.record("llm.model_name", "unknown");
                        oi_span.record("llm.provider", "unknown");
                        tracing::Span::current().record("model.response_mode", "JsonSchema");
                        tracing::Span::current().record("model.temperature", 0.0);
                        tracing::Span::current()
                            .record("model.max_output_tokens", self.max_output_tokens as i64);
                        if let Some(ref fr) = r.finish_reason {
                            tracing::Span::current()
                                .record("model.finish_reason", format!("{:?}", fr));
                        }
                        if let Some(pt) = r.prompt_tokens {
                            oi_span.record("llm.token_count.prompt", pt as i64);
                            tracing::Span::current().record("model.prompt_tokens", pt as i64);
                        }
                        if let Some(ct) = r.completion_tokens {
                            oi_span.record("llm.token_count.completion", ct as i64);
                            tracing::Span::current().record("model.completion_tokens", ct as i64);
                        }
                        if let Some(tt) = r.total_tokens {
                            oi_span.record("llm.token_count.total", tt as i64);
                            tracing::Span::current().record("model.total_tokens", tt as i64);
                        }
                        oi_span.record("status", "ok");
                        tracing::Span::current().record("status", "ok");
                        Ok(r)
                    }
                    Err(e) => {
                        let msg = format!("model client error: {e}");
                        crate::observability::record_error(
                            oi_span,
                            "ObservationExtraction.ModelClient",
                            &msg,
                        );
                        tracing::Span::current().record("status", "error");
                        tracing::Span::current()
                            .record("error.type", "ObservationExtraction.ModelClient");
                        tracing::Span::current().record("error.message", &msg);
                        Err(e)
                    }
                }
            }
            .instrument(llm_span)
            .await
        };

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                return Err(ObservationExtractionError::ModelClient(e));
            }
        };

        let token_usage = ModelTokenUsage {
            prompt_tokens: response.prompt_tokens,
            completion_tokens: response.completion_tokens,
            total_tokens: response.total_tokens,
        };
        let finish_reason = response.finish_reason.clone();

        // ── Finish-reason check ───────────────────────────────────────────────

        let is_acceptable_finish = matches!(&finish_reason, Some(ModelFinishReason::Stop) | None);
        if !is_acceptable_finish {
            let reason = if matches!(finish_reason, Some(ModelFinishReason::Length)) {
                "model output truncated: finish_reason was length"
            } else {
                "model returned unusable finish reason"
            };
            tracing::Span::current().record("module.outcome", "failure");
            tracing::Span::current().record("status", "error");
            tracing::Span::current()
                .record("error.type", "ObservationExtraction.InvalidModelOutput");
            tracing::Span::current().record("error.message", reason);
            crate::observability::record_error(
                oi_span,
                "ObservationExtraction.InvalidModelOutput",
                reason,
            );
            return Err(ObservationExtractionError::InvalidModelOutput {
                reason: reason.to_string(),
                finish_reason,
                token_usage,
            });
        }

        // ── Parse model JSON ──────────────────────────────────────────────────

        let raw: RawObservationExtractionResponse =
            serde_json::from_str(&response.content).map_err(|e| {
                let reason = format!("model output is not valid JSON: {e}");
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                crate::observability::record_error(
                    oi_span,
                    "ObservationExtraction.InvalidModelOutput",
                    &reason,
                );
                ObservationExtractionError::InvalidModelOutput {
                    reason,
                    finish_reason: finish_reason.clone(),
                    token_usage: token_usage.clone(),
                }
            })?;

        // ── Business rules ────────────────────────────────────────────────────

        let confidence = parse_confidence(&raw.confidence).ok_or_else(|| {
            let reason = format!("unknown top-level confidence value: '{}'", raw.confidence);
            tracing::Span::current().record("module.outcome", "failure");
            tracing::Span::current().record("status", "error");
            crate::observability::record_error(
                oi_span,
                "ObservationExtraction.InvalidModelOutput",
                &reason,
            );
            ObservationExtractionError::InvalidModelOutput {
                reason,
                finish_reason: finish_reason.clone(),
                token_usage: token_usage.clone(),
            }
        })?;

        // ── needs_more_context business rules ─────────────────────────────────

        if !raw.needs_more_context {
            if raw.observations.is_empty() {
                let reason =
                    "needs_more_context=false but observations is empty".to_string();
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                crate::observability::record_error(
                    oi_span,
                    "ObservationExtraction.InvalidModelOutput",
                    &reason,
                );
                return Err(ObservationExtractionError::InvalidModelOutput {
                    reason,
                    finish_reason,
                    token_usage,
                });
            }
            if !raw.missing_context_questions.is_empty() {
                let reason = "needs_more_context=false but missing_context_questions is non-empty"
                    .to_string();
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                crate::observability::record_error(
                    oi_span,
                    "ObservationExtraction.InvalidModelOutput",
                    &reason,
                );
                return Err(ObservationExtractionError::InvalidModelOutput {
                    reason,
                    finish_reason,
                    token_usage,
                });
            }
        } else {
            let q_count = raw.missing_context_questions.len();
            if q_count < 1 || q_count > 2 {
                let reason = format!(
                    "needs_more_context=true but missing_context_questions has {q_count} items (must be 1–2)"
                );
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                crate::observability::record_error(
                    oi_span,
                    "ObservationExtraction.InvalidModelOutput",
                    &reason,
                );
                return Err(ObservationExtractionError::InvalidModelOutput {
                    reason,
                    finish_reason,
                    token_usage,
                });
            }
        }

        // ── Parse observations ────────────────────────────────────────────────

        let mut observations = Vec::with_capacity(raw.observations.len());
        for raw_obs in &raw.observations {
            let obs_confidence = parse_confidence(&raw_obs.confidence).ok_or_else(|| {
                let reason = format!(
                    "unknown observation confidence value: '{}'",
                    raw_obs.confidence
                );
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                crate::observability::record_error(
                    oi_span,
                    "ObservationExtraction.InvalidModelOutput",
                    &reason,
                );
                ObservationExtractionError::InvalidModelOutput {
                    reason,
                    finish_reason: finish_reason.clone(),
                    token_usage: token_usage.clone(),
                }
            })?;

            let polarity = parse_polarity(&raw_obs.polarity).ok_or_else(|| {
                let reason = format!("unknown polarity value: '{}'", raw_obs.polarity);
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                crate::observability::record_error(
                    oi_span,
                    "ObservationExtraction.InvalidModelOutput",
                    &reason,
                );
                ObservationExtractionError::InvalidModelOutput {
                    reason,
                    finish_reason: finish_reason.clone(),
                    token_usage: token_usage.clone(),
                }
            })?;

            let statement = raw_obs.statement.trim().to_string();
            if statement.is_empty() {
                let reason = "observation statement is empty after trimming".to_string();
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                crate::observability::record_error(
                    oi_span,
                    "ObservationExtraction.InvalidModelOutput",
                    &reason,
                );
                return Err(ObservationExtractionError::InvalidModelOutput {
                    reason,
                    finish_reason,
                    token_usage,
                });
            }

            let condition = match &raw_obs.condition {
                Some(c) => {
                    let trimmed = c.trim().to_string();
                    if trimmed.is_empty() {
                        let reason =
                            "observation condition is present but empty after trimming".to_string();
                        tracing::Span::current().record("module.outcome", "failure");
                        tracing::Span::current().record("status", "error");
                        crate::observability::record_error(
                            oi_span,
                            "ObservationExtraction.InvalidModelOutput",
                            &reason,
                        );
                        return Err(ObservationExtractionError::InvalidModelOutput {
                            reason,
                            finish_reason,
                            token_usage,
                        });
                    }
                    Some(trimmed)
                }
                None => None,
            };

            let time_relation = match &raw_obs.time_relation {
                Some(t) => {
                    let trimmed = t.trim().to_string();
                    if trimmed.is_empty() {
                        let reason =
                            "observation time_relation is present but empty after trimming"
                                .to_string();
                        tracing::Span::current().record("module.outcome", "failure");
                        tracing::Span::current().record("status", "error");
                        crate::observability::record_error(
                            oi_span,
                            "ObservationExtraction.InvalidModelOutput",
                            &reason,
                        );
                        return Err(ObservationExtractionError::InvalidModelOutput {
                            reason,
                            finish_reason,
                            token_usage,
                        });
                    }
                    Some(trimmed)
                }
                None => None,
            };

            let raw_source_span = raw_obs.source_span.trim().to_string();
            if raw_source_span.is_empty() {
                let reason = "observation source_span is empty after trimming".to_string();
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                crate::observability::record_error(
                    oi_span,
                    "ObservationExtraction.InvalidModelOutput",
                    &reason,
                );
                return Err(ObservationExtractionError::InvalidModelOutput {
                    reason,
                    finish_reason,
                    token_usage,
                });
            }

            // Prefer exact provenance when the model returns one; otherwise soften to the
            // full resolved observation text so extraction can continue.
            let source_span = if resolved_text.contains(raw_source_span.as_str()) {
                raw_source_span
            } else {
                resolved_text.clone()
            };

            observations.push(ExtractedObservation {
                statement,
                confidence: obs_confidence,
                condition,
                polarity,
                time_relation,
                source_span,
            });
        }

        // ── Success ───────────────────────────────────────────────────────────

        tracing::Span::current().record("extraction.needs_more_context", raw.needs_more_context);
        tracing::Span::current().record("extraction.observations_count", observations.len() as i64);

        let resolved_observation = ResolvedObservation {
            text: resolved_text,
        };

        let output = ObservationExtractionOutput {
            normalized_user_input: input.normalized_user_input.clone(),
            resolved_observation,
            confidence,
            observations,
            needs_more_context: raw.needs_more_context,
            missing_context_questions: raw.missing_context_questions,
            token_usage,
        };

        if let Ok(output_json) = serde_json::to_string(&output) {
            oi_span.record("output.value", output_json.as_str());
            oi_span.record("output.mime_type", "application/json");
        }
        oi_span.record("status", "ok");
        tracing::Span::current().record("module.outcome", "success");
        tracing::Span::current().record("status", "ok");

        Ok(output)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn parse_confidence(s: &str) -> Option<Confidence> {
    match s {
        "low" => Some(Confidence::Low),
        "medium" => Some(Confidence::Medium),
        "high" => Some(Confidence::High),
        _ => None,
    }
}

fn parse_polarity(s: &str) -> Option<ObservationPolarity> {
    match s {
        "present" => Some(ObservationPolarity::Present),
        "absent" => Some(ObservationPolarity::Absent),
        "corrected" => Some(ObservationPolarity::Corrected),
        _ => None,
    }
}

fn derive_schema_path(asset_path: &str) -> Result<String, ObservationExtractionError> {
    let path = std::path::Path::new(asset_path);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ObservationExtractionError::InvalidPromptAsset(
            "prompt_asset_path has no file name component".to_string(),
        ))?;
    let schema_file_name = if file_name.ends_with(".manual_test.json") {
        file_name.replacen(".manual_test.json", ".schema.json", 1)
    } else {
        file_name.replacen(".json", ".schema.json", 1)
    };
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    Ok(parent.join(schema_file_name).to_string_lossy().into_owned())
}

fn load_prompt_asset(
    path: &str,
) -> Result<ObservationExtractionPromptAsset, ObservationExtractionError> {
    let schema_path = derive_schema_path(path)?;

    let asset_content = std::fs::read_to_string(path).map_err(|e| {
        ObservationExtractionError::InvalidPromptAsset(format!(
            "failed to read prompt asset '{path}': {e}"
        ))
    })?;

    let asset_json: serde_json::Value = serde_json::from_str(&asset_content).map_err(|e| {
        ObservationExtractionError::InvalidPromptAsset(format!(
            "invalid prompt asset JSON: {e}"
        ))
    })?;

    let schema_content = std::fs::read_to_string(&schema_path).map_err(|e| {
        ObservationExtractionError::InvalidPromptAsset(format!(
            "failed to read prompt asset schema '{schema_path}': {e}"
        ))
    })?;

    let schema_json: serde_json::Value =
        serde_json::from_str(&schema_content).map_err(|e| {
            ObservationExtractionError::InvalidPromptAsset(format!(
                "invalid prompt asset schema JSON: {e}"
            ))
        })?;

    let validator = jsonschema::options()
        .build(&schema_json)
        .map_err(|e| {
            ObservationExtractionError::InvalidPromptAsset(format!(
                "prompt asset schema compile error: {e}"
            ))
        })?;

    if !validator.is_valid(&asset_json) {
        let errors: Vec<String> = validator
            .iter_errors(&asset_json)
            .map(|e| e.to_string())
            .collect();
        return Err(ObservationExtractionError::InvalidPromptAsset(format!(
            "prompt asset schema validation failed: {}",
            errors.join("; ")
        )));
    }

    let asset: ObservationExtractionPromptAsset =
        serde_json::from_value(asset_json).map_err(|e| {
            ObservationExtractionError::InvalidPromptAsset(format!(
                "prompt asset deserialization failed: {e}"
            ))
        })?;

    validate_prompt_asset(&asset)?;

    Ok(asset)
}

fn validate_prompt_asset(
    asset: &ObservationExtractionPromptAsset,
) -> Result<(), ObservationExtractionError> {
    if asset.system_prompt.trim().is_empty() {
        return Err(ObservationExtractionError::InvalidPromptAsset(
            "system_prompt must be non-empty".to_string(),
        ));
    }
    if asset.user_template.trim().is_empty() {
        return Err(ObservationExtractionError::InvalidPromptAsset(
            "user_template must be non-empty".to_string(),
        ));
    }
    if !asset.response_schema.is_object() {
        return Err(ObservationExtractionError::InvalidPromptAsset(
            "response_schema must be a JSON object".to_string(),
        ));
    }

    let um_count = asset
        .user_template
        .matches(USER_MESSAGE_PLACEHOLDER)
        .count();

    if um_count != 1 {
        return Err(ObservationExtractionError::InvalidPromptAsset(format!(
            "user_template must contain exactly one {USER_MESSAGE_PLACEHOLDER} placeholder, found {um_count}"
        )));
    }

    let remaining = asset
        .user_template
        .replace(USER_MESSAGE_PLACEHOLDER, "");
    if remaining.contains("{{") || remaining.contains("}}") {
        return Err(ObservationExtractionError::InvalidPromptAsset(
            "user_template contains unsupported placeholder constructs".to_string(),
        ));
    }

    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::api_clients::model::{ModelClient, ModelClientError, ModelGenerationResponse};
    use crate::shared_types::{
        Confidence, ObservationBoundaryResolution, ObservationBoundaryResolverOutput,
        ResolvedObservation,
    };
    use crate::test_utils::TempArtifactDir;

    // ─── Asset constants ───────────────────────────────────────────────────────

    const VALID_ASSET_JSON: &str = r#"{
        "version": "v1",
        "system_prompt": "You are an observation extractor.",
        "user_template": "Extract observations from: {{user_message}}",
        "response_schema": {"type": "object"}
    }"#;

    const VALID_SCHEMA_JSON: &str = r#"{
        "type": "object",
        "properties": {
            "version": {"type": "string"},
            "system_prompt": {"type": "string"},
            "user_template": {"type": "string"},
            "response_schema": {"type": "object"}
        },
        "required": ["version", "system_prompt", "user_template", "response_schema"]
    }"#;

    // ─── Helpers ───────────────────────────────────────────────────────────────

    fn write_asset(dir: &TempArtifactDir) -> String {
        dir.write_json("oe_prompt.json", VALID_ASSET_JSON)
            .to_str()
            .unwrap()
            .to_string()
    }

    fn write_schema(dir: &TempArtifactDir) {
        dir.write_json("oe_prompt.schema.json", VALID_SCHEMA_JSON);
    }

    fn write_both(dir: &TempArtifactDir) -> String {
        write_schema(dir);
        write_asset(dir)
    }

    fn settings(path: &str) -> ObservationExtractionSettings {
        ObservationExtractionSettings {
            prompt_asset_path: path.to_string(),
            max_output_tokens: 256,
        }
    }

    fn noop_client() -> Arc<dyn ModelClient> {
        Arc::new(NoopModelClient)
    }

    struct NoopModelClient;

    #[async_trait::async_trait]
    impl ModelClient for NoopModelClient {
        async fn generate(
            &self,
            _req: &ModelGenerationRequest,
        ) -> Result<ModelGenerationResponse, ModelClientError> {
            Ok(ModelGenerationResponse {
                content: "{}".to_string(),
                finish_reason: Some(ModelFinishReason::Stop),
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            })
        }
    }

    struct FixedModelClient {
        content: String,
        finish_reason: Option<ModelFinishReason>,
    }

    impl FixedModelClient {
        fn with_content(content: impl Into<String>) -> Arc<dyn ModelClient> {
            Arc::new(Self {
                content: content.into(),
                finish_reason: Some(ModelFinishReason::Stop),
            })
        }
        fn with_finish_reason(
            content: impl Into<String>,
            finish_reason: Option<ModelFinishReason>,
        ) -> Arc<dyn ModelClient> {
            Arc::new(Self {
                content: content.into(),
                finish_reason,
            })
        }
    }

    #[async_trait::async_trait]
    impl ModelClient for FixedModelClient {
        async fn generate(
            &self,
            _req: &ModelGenerationRequest,
        ) -> Result<ModelGenerationResponse, ModelClientError> {
            Ok(ModelGenerationResponse {
                content: self.content.clone(),
                finish_reason: self.finish_reason.clone(),
                prompt_tokens: Some(10),
                completion_tokens: Some(5),
                total_tokens: Some(15),
            })
        }
    }

    struct FailingModelClient;

    #[async_trait::async_trait]
    impl ModelClient for FailingModelClient {
        async fn generate(
            &self,
            _req: &ModelGenerationRequest,
        ) -> Result<ModelGenerationResponse, ModelClientError> {
            Err(ModelClientError::Transport(
                "connection refused".to_string(),
            ))
        }
    }

    fn make_supported_input(resolved_text: &str) -> ObservationBoundaryResolverOutput {
        ObservationBoundaryResolverOutput {
            normalized_user_input: "memory spike on node-3".to_string(),
            confidence: Confidence::High,
            reason: "The user input describes a new observation.".to_string(),
            resolution: ObservationBoundaryResolution::Supported(ResolvedObservation {
                text: resolved_text.to_string(),
            }),
            token_usage: ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
        }
    }

    fn make_unsupported_input() -> ObservationBoundaryResolverOutput {
        ObservationBoundaryResolverOutput {
            normalized_user_input: "what does that mean?".to_string(),
            confidence: Confidence::Low,
            reason: "This is a question, not an observation.".to_string(),
            resolution: ObservationBoundaryResolution::Unsupported,
            token_usage: ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
        }
    }

    fn valid_extraction_json(resolved_text: &str) -> String {
        json!({
            "confidence": "high",
            "observations": [
                {
                    "statement": "Memory usage spiked to 95% on node-3",
                    "confidence": "high",
                    "condition": null,
                    "polarity": "present",
                    "time_relation": null,
                    "source_span": resolved_text
                }
            ],
            "needs_more_context": false,
            "missing_context_questions": []
        })
        .to_string()
    }

    fn needs_more_context_json() -> String {
        json!({
            "confidence": "low",
            "observations": [],
            "needs_more_context": true,
            "missing_context_questions": [
                "Which node did the memory spike occur on?",
                "At what time did the spike happen?"
            ]
        })
        .to_string()
    }

    // ── Constructor tests ─────────────────────────────────────────────────────

    #[test]
    fn new_rejects_empty_prompt_asset_path() {
        let err = ObservationExtraction::new(
            ObservationExtractionSettings {
                prompt_asset_path: "".to_string(),
                max_output_tokens: 256,
            },
            noop_client(),
        )
        .unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidSettings(_)));
    }

    #[test]
    fn new_rejects_zero_max_output_tokens() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let err = ObservationExtraction::new(
            ObservationExtractionSettings {
                prompt_asset_path: path,
                max_output_tokens: 0,
            },
            noop_client(),
        )
        .unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidSettings(_)));
    }

    #[test]
    fn new_rejects_unreadable_prompt_asset_file() {
        let err = ObservationExtraction::new(
            settings("/nonexistent/oe_prompt.json"),
            noop_client(),
        )
        .unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidPromptAsset(_)));
    }

    #[test]
    fn new_rejects_unreadable_schema_file() {
        let dir = TempArtifactDir::new();
        let path = write_asset(&dir);
        // schema file not written → derivation succeeds but read fails
        let err = ObservationExtraction::new(settings(&path), noop_client()).unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidPromptAsset(_)));
    }

    #[test]
    fn new_succeeds_with_valid_asset() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        assert!(ObservationExtraction::new(settings(&path), noop_client()).is_ok());
    }

    #[test]
    fn new_rejects_invalid_prompt_asset_json() {
        let dir = TempArtifactDir::new();
        write_schema(&dir);
        dir.write_json("oe_prompt.json", "not valid json {{");
        let path = dir.path().join("oe_prompt.json").to_str().unwrap().to_string();
        let err = ObservationExtraction::new(settings(&path), noop_client()).unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidPromptAsset(_)));
    }

    #[test]
    fn new_rejects_invalid_schema_json() {
        let dir = TempArtifactDir::new();
        dir.write_json("oe_prompt.schema.json", "not json");
        dir.write_json("oe_prompt.json", VALID_ASSET_JSON);
        let path = dir.path().join("oe_prompt.json").to_str().unwrap().to_string();
        let err = ObservationExtraction::new(settings(&path), noop_client()).unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidPromptAsset(_)));
    }

    #[test]
    fn new_rejects_user_template_with_extra_placeholder() {
        let dir = TempArtifactDir::new();
        write_schema(&dir);
        let asset = r#"{
            "version": "v1",
            "system_prompt": "You are an extractor.",
            "user_template": "{{user_message}} extra: {{forbidden}}",
            "response_schema": {"type": "object"}
        }"#;
        dir.write_json("oe_prompt.json", asset);
        let path = dir.path().join("oe_prompt.json").to_str().unwrap().to_string();
        let err = ObservationExtraction::new(settings(&path), noop_client()).unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidPromptAsset(_)));
    }

    #[test]
    fn provider_and_model_not_stored_in_settings() {
        let s = ObservationExtractionSettings {
            prompt_asset_path: "some/path.json".to_string(),
            max_output_tokens: 128,
        };
        assert_eq!(s.prompt_asset_path, "some/path.json");
        assert_eq!(s.max_output_tokens, 128);
    }

    // ── Capturing client for request shape assertions ──────────────────────────

    struct CapturingModelClient {
        captured: std::sync::Mutex<Option<ModelGenerationRequest>>,
        response_content: String,
    }

    impl CapturingModelClient {
        fn new(response_content: impl Into<String>) -> Arc<dyn ModelClient> {
            Arc::new(Self {
                captured: std::sync::Mutex::new(None),
                response_content: response_content.into(),
            })
        }
    }

    #[async_trait::async_trait]
    impl ModelClient for CapturingModelClient {
        async fn generate(
            &self,
            req: &ModelGenerationRequest,
        ) -> Result<ModelGenerationResponse, ModelClientError> {
            *self.captured.lock().unwrap() = Some(req.clone());
            Ok(ModelGenerationResponse {
                content: self.response_content.clone(),
                finish_reason: Some(ModelFinishReason::Stop),
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            })
        }
    }

    // ── Model call shape ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn sends_json_schema_response_mode() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let resolved = "memory spike on node-3";
        let capturing = Arc::new(CapturingModelClient {
            captured: std::sync::Mutex::new(None),
            response_content: valid_extraction_json(resolved),
        });
        let oe = ObservationExtraction::new(settings(&path), capturing.clone() as Arc<dyn ModelClient>).unwrap();
        oe.extract(&make_supported_input(resolved)).await.unwrap();
        let req = capturing.captured.lock().unwrap().clone().unwrap();
        assert!(matches!(req.response_mode, crate::api_clients::model::shared_types::ModelResponseMode::JsonSchema(_)));
    }

    #[tokio::test]
    async fn sends_configured_max_output_tokens() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let resolved = "memory spike on node-3";
        let capturing = Arc::new(CapturingModelClient {
            captured: std::sync::Mutex::new(None),
            response_content: valid_extraction_json(resolved),
        });
        let mut s = settings(&path);
        s.max_output_tokens = 512;
        let oe = ObservationExtraction::new(s, capturing.clone() as Arc<dyn ModelClient>).unwrap();
        oe.extract(&make_supported_input(resolved)).await.unwrap();
        let req = capturing.captured.lock().unwrap().clone().unwrap();
        assert_eq!(req.max_output_tokens, Some(512));
    }

    #[tokio::test]
    async fn prompt_substitutes_resolved_observation_into_user_message() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let resolved = "CPU throttling started after the deployment";
        let capturing = Arc::new(CapturingModelClient {
            captured: std::sync::Mutex::new(None),
            response_content: valid_extraction_json(resolved),
        });
        let oe = ObservationExtraction::new(settings(&path), capturing.clone() as Arc<dyn ModelClient>).unwrap();
        oe.extract(&make_supported_input(resolved)).await.unwrap();
        let req = capturing.captured.lock().unwrap().clone().unwrap();
        let user_msg = req.messages.iter().find(|m| matches!(m.role, crate::api_clients::model::shared_types::ModelMessageRole::User)).unwrap();
        assert!(user_msg.content.contains(resolved), "user message must contain resolved observation text");
    }

    // ── Unsupported input ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn extract_returns_unsupported_boundary_input_without_model_call() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(settings(&path), noop_client()).unwrap();
        let input = make_unsupported_input();
        let err = oe.extract(&input).await.unwrap_err();
        assert!(matches!(err, ObservationExtractionError::UnsupportedBoundaryInput));
    }

    // ── Successful extraction ─────────────────────────────────────────────────

    #[tokio::test]
    async fn extract_returns_correct_output_for_valid_response() {
        let resolved_text = "Memory usage spiked to 95% on node-3 at 14:32 UTC";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(valid_extraction_json(resolved_text)),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let out = oe.extract(&input).await.unwrap();
        assert_eq!(out.normalized_user_input, "memory spike on node-3");
        assert_eq!(out.resolved_observation.text, resolved_text);
        assert_eq!(out.confidence, Confidence::High);
        assert_eq!(out.observations.len(), 1);
        assert!(!out.needs_more_context);
        assert!(out.missing_context_questions.is_empty());
        assert_eq!(out.token_usage.prompt_tokens, Some(10));
        assert_eq!(out.token_usage.completion_tokens, Some(5));
        assert_eq!(out.token_usage.total_tokens, Some(15));
    }

    #[tokio::test]
    async fn extract_returns_needs_more_context_output() {
        let resolved_text = "something happened";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(needs_more_context_json()),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let out = oe.extract(&input).await.unwrap();
        assert!(out.needs_more_context);
        assert_eq!(out.missing_context_questions.len(), 2);
    }

    #[tokio::test]
    async fn extract_delegates_to_extract_with_context() {
        let resolved_text = "Memory usage spiked to 95% on node-3 at 14:32 UTC";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(valid_extraction_json(resolved_text)),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let r1 = oe.extract(&input).await.unwrap();
        let r2 = oe
            .extract_with_context(&input, &Context::noop())
            .await
            .unwrap();
        assert_eq!(r1.normalized_user_input, r2.normalized_user_input);
        assert_eq!(r1.observations.len(), r2.observations.len());
    }

    // ── Business rule violations ──────────────────────────────────────────────

    #[tokio::test]
    async fn needs_more_context_false_with_empty_observations_fails() {
        let resolved_text = "Memory usage spiked";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "confidence": "high",
                    "observations": [],
                    "needs_more_context": false,
                    "missing_context_questions": []
                })
                .to_string(),
            ),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let err = oe.extract(&input).await.unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidModelOutput { .. }));
    }

    #[tokio::test]
    async fn needs_more_context_false_with_nonempty_questions_fails() {
        let resolved_text = "Memory usage spiked";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "confidence": "high",
                    "observations": [
                        {
                            "statement": "Memory spiked",
                            "confidence": "high",
                            "condition": null,
                            "polarity": "present",
                            "time_relation": null,
                            "source_span": "Memory usage spiked"
                        }
                    ],
                    "needs_more_context": false,
                    "missing_context_questions": ["What happened?"]
                })
                .to_string(),
            ),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let err = oe.extract(&input).await.unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidModelOutput { .. }));
    }

    #[tokio::test]
    async fn needs_more_context_true_with_zero_questions_fails() {
        let resolved_text = "something vague";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "confidence": "low",
                    "observations": [],
                    "needs_more_context": true,
                    "missing_context_questions": []
                })
                .to_string(),
            ),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let err = oe.extract(&input).await.unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidModelOutput { .. }));
    }

    #[tokio::test]
    async fn needs_more_context_true_with_three_questions_fails() {
        let resolved_text = "something vague";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "confidence": "low",
                    "observations": [],
                    "needs_more_context": true,
                    "missing_context_questions": ["Q1", "Q2", "Q3"]
                })
                .to_string(),
            ),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let err = oe.extract(&input).await.unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidModelOutput { .. }));
    }

    #[tokio::test]
    async fn source_span_not_substring_of_resolved_text_is_auto_repaired() {
        let resolved_text = "Memory usage spiked to 95%";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "confidence": "high",
                    "observations": [
                        {
                            "statement": "CPU spiked",
                            "confidence": "high",
                            "condition": null,
                            "polarity": "present",
                            "time_relation": null,
                            "source_span": "CPU usage spiked to 100%"
                        }
                    ],
                    "needs_more_context": false,
                    "missing_context_questions": []
                })
                .to_string(),
            ),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let out = oe.extract(&input).await.unwrap();
        assert_eq!(out.observations[0].source_span, resolved_text);
    }

    #[tokio::test]
    async fn unknown_confidence_value_fails() {
        let resolved_text = "Memory usage spiked";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "confidence": "very_high",
                    "observations": [],
                    "needs_more_context": true,
                    "missing_context_questions": ["Question?"]
                })
                .to_string(),
            ),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let err = oe.extract(&input).await.unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidModelOutput { .. }));
    }

    #[tokio::test]
    async fn invalid_json_from_model_fails() {
        let resolved_text = "Memory usage spiked";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content("not json at all {{{"),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let err = oe.extract(&input).await.unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidModelOutput { .. }));
    }

    #[tokio::test]
    async fn model_client_error_propagates_as_typed_error() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe =
            ObservationExtraction::new(settings(&path), Arc::new(FailingModelClient)).unwrap();
        let input = make_supported_input("memory spike");
        let err = oe.extract(&input).await.unwrap_err();
        assert!(matches!(err, ObservationExtractionError::ModelClient(_)));
    }

    #[tokio::test]
    async fn length_finish_reason_fails_as_invalid_model_output() {
        let resolved_text = "Memory usage spiked";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_finish_reason(
                valid_extraction_json(resolved_text),
                Some(ModelFinishReason::Length),
            ),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let err = oe.extract(&input).await.unwrap_err();
        assert!(matches!(err, ObservationExtractionError::InvalidModelOutput { .. }));
    }

    #[tokio::test]
    async fn normalized_user_input_exactly_matches_input() {
        let resolved_text = "Memory usage spiked to 95% on node-3 at 14:32 UTC";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(valid_extraction_json(resolved_text)),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let out = oe.extract(&input).await.unwrap();
        assert_eq!(out.normalized_user_input, input.normalized_user_input);
    }

    #[tokio::test]
    async fn resolved_observation_exactly_matches_supported_resolution() {
        let resolved_text = "Memory usage spiked to 95% on node-3 at 14:32 UTC";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(valid_extraction_json(resolved_text)),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let out = oe.extract(&input).await.unwrap();
        assert_eq!(out.resolved_observation.text, resolved_text);
    }

    #[tokio::test]
    async fn observation_polarity_present_parses_correctly() {
        let resolved_text = "Memory usage spiked to 95%";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(valid_extraction_json(resolved_text)),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let out = oe.extract(&input).await.unwrap();
        assert_eq!(out.observations[0].polarity, ObservationPolarity::Present);
    }

    #[tokio::test]
    async fn observation_absent_polarity_parses_correctly() {
        let resolved_text = "No errors observed on node-3";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "confidence": "high",
                    "observations": [
                        {
                            "statement": "No errors observed on node-3",
                            "confidence": "high",
                            "condition": null,
                            "polarity": "absent",
                            "time_relation": null,
                            "source_span": "No errors observed on node-3"
                        }
                    ],
                    "needs_more_context": false,
                    "missing_context_questions": []
                })
                .to_string(),
            ),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let out = oe.extract(&input).await.unwrap();
        assert_eq!(out.observations[0].polarity, ObservationPolarity::Absent);
    }

    #[tokio::test]
    async fn observation_corrected_polarity_parses_correctly() {
        let resolved_text = "Actually the restart happened before the spike";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "confidence": "medium",
                    "observations": [
                        {
                            "statement": "Restart happened before the spike",
                            "confidence": "medium",
                            "condition": null,
                            "polarity": "corrected",
                            "time_relation": null,
                            "source_span": "restart happened before the spike"
                        }
                    ],
                    "needs_more_context": false,
                    "missing_context_questions": []
                })
                .to_string(),
            ),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let out = oe.extract(&input).await.unwrap();
        assert_eq!(out.observations[0].polarity, ObservationPolarity::Corrected);
    }

    #[tokio::test]
    async fn source_span_is_trimmed_before_substring_check() {
        // source_span with leading/trailing whitespace should still match after trim
        let resolved_text = "Memory usage spiked to 95%";
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let oe = ObservationExtraction::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "confidence": "high",
                    "observations": [
                        {
                            "statement": "Memory spiked",
                            "confidence": "high",
                            "condition": null,
                            "polarity": "present",
                            "time_relation": null,
                            "source_span": "  Memory usage spiked to 95%  "
                        }
                    ],
                    "needs_more_context": false,
                    "missing_context_questions": []
                })
                .to_string(),
            ),
        )
        .unwrap();
        let input = make_supported_input(resolved_text);
        let out = oe.extract(&input).await.unwrap();
        // source_span stored in trimmed form
        assert_eq!(out.observations[0].source_span, "Memory usage spiked to 95%");
    }
}
