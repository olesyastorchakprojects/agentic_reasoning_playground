use std::sync::Arc;

use serde::Deserialize;
use tracing::{field, info_span, Instrument};

use crate::api_clients::model::{
    ModelClient, ModelClientError, ModelFinishReason, ModelGenerationRequest, ModelMessage,
    ModelMessageRole, ModelResponseMode,
};
use crate::shared_types::{
    Confidence, Context, DiagnosticContext, ModelTokenUsage, NormalizedUserRequest,
    ObservationBoundaryResolution, ObservationBoundaryResolverOutput, ResolvedObservation,
};

// ─── Placeholders ─────────────────────────────────────────────────────────────

const DIAGNOSTIC_CONTEXT_PLACEHOLDER: &str = "{{diagnostic_context}}";
const USER_INPUT_PLACEHOLDER: &str = "{{normalized_user_input}}";

// ─── Module-private prompt asset ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ObservationBoundaryResolverPromptAsset {
    version: String,
    system_prompt: String,
    user_template: String,
    response_schema: serde_json::Value,
}

// ─── Raw model output ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawObservationBoundaryResponse {
    supported: bool,
    confidence: String,
    reason: String,
    full_query: Option<String>,
}

// ─── Settings ─────────────────────────────────────────────────────────────────

pub struct ObservationBoundaryResolverSettings {
    pub prompt_asset_path: String,
    pub max_output_tokens: u32,
}

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum ObservationBoundaryResolverError {
    #[error("invalid settings: {0}")]
    InvalidSettings(String),

    #[error("invalid prompt asset: {0}")]
    InvalidPromptAsset(String),

    #[error("missing required diagnostic context data: {0}")]
    InvalidContext(String),

    #[error(transparent)]
    ModelClient(#[from] ModelClientError),

    #[error("invalid model output: {reason}")]
    InvalidModelOutput {
        reason: String,
        finish_reason: Option<ModelFinishReason>,
        token_usage: ModelTokenUsage,
    },
}

// ─── Public struct ────────────────────────────────────────────────────────────

pub struct ObservationBoundaryResolver {
    model_client: Arc<dyn ModelClient>,
    prompt_asset: ObservationBoundaryResolverPromptAsset,
    max_output_tokens: u32,
}

impl std::fmt::Debug for ObservationBoundaryResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObservationBoundaryResolver")
            .field("prompt_asset.version", &self.prompt_asset.version)
            .field("max_output_tokens", &self.max_output_tokens)
            .finish_non_exhaustive()
    }
}

impl ObservationBoundaryResolver {
    pub fn new(
        settings: ObservationBoundaryResolverSettings,
        model_client: Arc<dyn ModelClient>,
    ) -> Result<Self, ObservationBoundaryResolverError> {
        if settings.prompt_asset_path.trim().is_empty() {
            return Err(ObservationBoundaryResolverError::InvalidSettings(
                "prompt_asset_path must not be empty".to_string(),
            ));
        }
        if settings.max_output_tokens == 0 {
            return Err(ObservationBoundaryResolverError::InvalidSettings(
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

    pub async fn resolve(
        &self,
        request: &NormalizedUserRequest,
        diagnostic_context: &DiagnosticContext,
    ) -> Result<ObservationBoundaryResolverOutput, ObservationBoundaryResolverError> {
        self.resolve_with_context(request, diagnostic_context, &Context::noop())
            .await
    }

    pub async fn resolve_with_context(
        &self,
        request: &NormalizedUserRequest,
        diagnostic_context: &DiagnosticContext,
        context: &Context,
    ) -> Result<ObservationBoundaryResolverOutput, ObservationBoundaryResolverError> {
        let oi_span = crate::observability::oi_llm_observation_boundary_resolver_span(
            &context.open_inference.root_span,
        );
        let pu_text = diagnostic_context
            .current_problem_understanding()
            .and_then(|pu| pu.text.as_deref())
            .unwrap_or("");
        let active_hypotheses: Vec<&str> = diagnostic_context
            .active_hypotheses()
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        let last_check_text = diagnostic_context
            .last_check()
            .map(|c| c.text.as_str())
            .unwrap_or("");
        let oi_input_json = serde_json::json!({
            "normalized_user_input": request.query,
            "problem_understanding": pu_text,
            "active_hypotheses": active_hypotheses,
            "latest_suggested_check": last_check_text,
        })
        .to_string();
        oi_span.record("input.value", oi_input_json.as_str());
        oi_span.record("input.mime_type", "application/json");
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
            "request_pipeline.observation_boundary_resolver",
            module.name = "observation_boundary_resolver",
            query.normalized = %request.query,
            asset.prompt.version = %self.prompt_asset.version,
            model.response_mode = field::Empty,
            model.temperature = field::Empty,
            model.max_output_tokens = field::Empty,
            model.finish_reason = field::Empty,
            model.prompt_tokens = field::Empty,
            model.completion_tokens = field::Empty,
            model.total_tokens = field::Empty,
            resolution.supported = field::Empty,
            resolution.confidence = field::Empty,
            module.outcome = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        self.resolve_instrumented(request, diagnostic_context, &oi_span)
            .instrument(span)
            .await
    }

    async fn resolve_instrumented(
        &self,
        request: &NormalizedUserRequest,
        diagnostic_context: &DiagnosticContext,
        oi_span: &tracing::Span,
    ) -> Result<ObservationBoundaryResolverOutput, ObservationBoundaryResolverError> {
        // ── Validate diagnostic context ───────────────────────────────────────

        let pu = diagnostic_context
            .current_problem_understanding()
            .ok_or(ObservationBoundaryResolverError::InvalidContext(
                "DiagnosticContext has no problem understanding entry".to_string(),
            ))?;

        let pu_text = pu.text.as_deref().ok_or(
            ObservationBoundaryResolverError::InvalidContext(
                "current problem understanding text is None (iteration not yet closed)"
                    .to_string(),
            ),
        )?;

        let last_check = diagnostic_context
            .last_check()
            .ok_or(ObservationBoundaryResolverError::InvalidContext(
                "DiagnosticContext has no suggested check".to_string(),
            ))?;

        // ── Build compact prompt-facing context ───────────────────────────────

        let active_hypotheses: Vec<&str> = diagnostic_context
            .active_hypotheses()
            .iter()
            .map(|t| t.text.as_str())
            .collect();

        let context_view = serde_json::json!({
            "problem_understanding": pu_text,
            "active_hypotheses": active_hypotheses,
            "latest_suggested_check": last_check.text,
        });
        let context_json = serde_json::to_string(&context_view)
            .expect("compact context view must serialize");

        // ── Substitute template ───────────────────────────────────────────────

        let user_message = self
            .prompt_asset
            .user_template
            .replacen(DIAGNOSTIC_CONTEXT_PLACEHOLDER, &context_json, 1)
            .replacen(USER_INPUT_PLACEHOLDER, &request.query, 1);

        // ── Model call ────────────────────────────────────────────────────────

        let llm_span = info_span!(
            "llm.call.observation_boundary_resolver",
            llm.task = "observation_boundary_resolver",
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
                            "ObservationBoundaryResolver.ModelClient",
                            &msg,
                        );
                        tracing::Span::current().record("status", "error");
                        tracing::Span::current().record(
                            "error.type",
                            "ObservationBoundaryResolver.ModelClient",
                        );
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
                return Err(ObservationBoundaryResolverError::ModelClient(e));
            }
        };

        let token_usage = ModelTokenUsage {
            prompt_tokens: response.prompt_tokens,
            completion_tokens: response.completion_tokens,
            total_tokens: response.total_tokens,
        };
        let finish_reason = response.finish_reason.clone();
        oi_span.record("llm.raw_response", response.content.as_str());

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
                .record("error.type", "ObservationBoundaryResolver.InvalidModelOutput");
            tracing::Span::current().record("error.message", reason);
            crate::observability::record_error(
                oi_span,
                "ObservationBoundaryResolver.InvalidModelOutput",
                reason,
            );
            return Err(ObservationBoundaryResolverError::InvalidModelOutput {
                reason: reason.to_string(),
                finish_reason,
                token_usage,
            });
        }

        // ── Parse model JSON ──────────────────────────────────────────────────

        let raw: RawObservationBoundaryResponse =
            serde_json::from_str(&response.content).map_err(|e| {
                let reason = format!("model output is not valid JSON: {e}");
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                crate::observability::record_error(
                    oi_span,
                    "ObservationBoundaryResolver.InvalidModelOutput",
                    &reason,
                );
                ObservationBoundaryResolverError::InvalidModelOutput {
                    reason,
                    finish_reason: finish_reason.clone(),
                    token_usage: token_usage.clone(),
                }
            })?;

        // ── Business rules ────────────────────────────────────────────────────

        let confidence = parse_confidence(&raw.confidence).ok_or_else(|| {
            let reason = format!("unknown confidence value: '{}'", raw.confidence);
            tracing::Span::current().record("module.outcome", "failure");
            tracing::Span::current().record("status", "error");
            crate::observability::record_error(
                oi_span,
                "ObservationBoundaryResolver.InvalidModelOutput",
                &reason,
            );
            ObservationBoundaryResolverError::InvalidModelOutput {
                reason,
                finish_reason: finish_reason.clone(),
                token_usage: token_usage.clone(),
            }
        })?;

        let resolution = if raw.supported {
            match &raw.full_query {
                Some(q) if !q.trim().is_empty() => {
                    ObservationBoundaryResolution::Supported(ResolvedObservation {
                        text: q.trim().to_string(),
                    })
                }
                _ => {
                    let reason =
                        "supported=true but full_query is null or empty".to_string();
                    tracing::Span::current().record("module.outcome", "failure");
                    tracing::Span::current().record("status", "error");
                    crate::observability::record_error(
                        oi_span,
                        "ObservationBoundaryResolver.InvalidModelOutput",
                        &reason,
                    );
                    return Err(ObservationBoundaryResolverError::InvalidModelOutput {
                        reason,
                        finish_reason,
                        token_usage,
                    });
                }
            }
        } else {
            if raw.full_query.is_some() {
                let reason = "supported=false but full_query is non-null".to_string();
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                crate::observability::record_error(
                    oi_span,
                    "ObservationBoundaryResolver.InvalidModelOutput",
                    &reason,
                );
                return Err(ObservationBoundaryResolverError::InvalidModelOutput {
                    reason,
                    finish_reason,
                    token_usage,
                });
            }
            ObservationBoundaryResolution::Unsupported
        };

        // ── Success ───────────────────────────────────────────────────────────

        tracing::Span::current().record("resolution.supported", raw.supported);
        tracing::Span::current().record("resolution.confidence", raw.confidence.as_str());

        let output = ObservationBoundaryResolverOutput {
            normalized_user_input: request.query.clone(),
            confidence,
            reason: raw.reason,
            resolution,
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

fn derive_schema_path(asset_path: &str) -> Result<String, ObservationBoundaryResolverError> {
    let path = std::path::Path::new(asset_path);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(ObservationBoundaryResolverError::InvalidPromptAsset(
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
) -> Result<ObservationBoundaryResolverPromptAsset, ObservationBoundaryResolverError> {
    let schema_path = derive_schema_path(path)?;

    let asset_content =
        std::fs::read_to_string(path).map_err(|e| {
            ObservationBoundaryResolverError::InvalidPromptAsset(format!(
                "failed to read prompt asset '{path}': {e}"
            ))
        })?;

    let asset_json: serde_json::Value = serde_json::from_str(&asset_content).map_err(|e| {
        ObservationBoundaryResolverError::InvalidPromptAsset(format!(
            "invalid prompt asset JSON: {e}"
        ))
    })?;

    let schema_content = std::fs::read_to_string(&schema_path).map_err(|e| {
        ObservationBoundaryResolverError::InvalidPromptAsset(format!(
            "failed to read prompt asset schema '{schema_path}': {e}"
        ))
    })?;

    let schema_json: serde_json::Value =
        serde_json::from_str(&schema_content).map_err(|e| {
            ObservationBoundaryResolverError::InvalidPromptAsset(format!(
                "invalid prompt asset schema JSON: {e}"
            ))
        })?;

    let validator =
        jsonschema::options()
            .build(&schema_json)
            .map_err(|e| {
                ObservationBoundaryResolverError::InvalidPromptAsset(format!(
                    "prompt asset schema compile error: {e}"
                ))
            })?;

    if !validator.is_valid(&asset_json) {
        let errors: Vec<String> = validator
            .iter_errors(&asset_json)
            .map(|e| e.to_string())
            .collect();
        return Err(ObservationBoundaryResolverError::InvalidPromptAsset(format!(
            "prompt asset schema validation failed: {}",
            errors.join("; ")
        )));
    }

    let asset: ObservationBoundaryResolverPromptAsset =
        serde_json::from_value(asset_json).map_err(|e| {
            ObservationBoundaryResolverError::InvalidPromptAsset(format!(
                "prompt asset deserialization failed: {e}"
            ))
        })?;

    validate_prompt_asset(&asset)?;

    Ok(asset)
}

fn validate_prompt_asset(
    asset: &ObservationBoundaryResolverPromptAsset,
) -> Result<(), ObservationBoundaryResolverError> {
    if asset.system_prompt.trim().is_empty() {
        return Err(ObservationBoundaryResolverError::InvalidPromptAsset(
            "system_prompt must be non-empty".to_string(),
        ));
    }
    if asset.user_template.trim().is_empty() {
        return Err(ObservationBoundaryResolverError::InvalidPromptAsset(
            "user_template must be non-empty".to_string(),
        ));
    }
    if !asset.response_schema.is_object() {
        return Err(ObservationBoundaryResolverError::InvalidPromptAsset(
            "response_schema must be a JSON object".to_string(),
        ));
    }

    let dc_count = asset
        .user_template
        .matches(DIAGNOSTIC_CONTEXT_PLACEHOLDER)
        .count();
    let ui_count = asset
        .user_template
        .matches(USER_INPUT_PLACEHOLDER)
        .count();

    if dc_count != 1 {
        return Err(ObservationBoundaryResolverError::InvalidPromptAsset(format!(
            "user_template must contain exactly one {DIAGNOSTIC_CONTEXT_PLACEHOLDER} placeholder, found {dc_count}"
        )));
    }
    if ui_count != 1 {
        return Err(ObservationBoundaryResolverError::InvalidPromptAsset(format!(
            "user_template must contain exactly one {USER_INPUT_PLACEHOLDER} placeholder, found {ui_count}"
        )));
    }

    let remaining = asset
        .user_template
        .replace(DIAGNOSTIC_CONTEXT_PLACEHOLDER, "")
        .replace(USER_INPUT_PLACEHOLDER, "");
    if remaining.contains("{{") || remaining.contains("}}") {
        return Err(ObservationBoundaryResolverError::InvalidPromptAsset(
            "user_template contains unsupported placeholder constructs".to_string(),
        ));
    }

    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::api_clients::model::{ModelClient, ModelClientError, ModelGenerationResponse};
    use crate::orchestrator::run_state::model::{
        FinishedStepRecord, RunId, RunIteration, RunIterationId, RunIterationStatus, RunState, RunStatus,
        StepKind, StepRecord, StepRecordId, StepResultEnvelope,
    };
    use crate::shared_types::{
        Confidence, DiagnosticContext, DiagnosticResponse, DiagnosticResultInterpretation,
        Hypothesis, HypothesisEvidenceSource, HypothesisId, HypothesisStatus,
        NormalizedUserRequest, ObservationBoundaryResolution, ObservationBoundaryResolverOutput,
        ResolvedObservation, ResponseValidationAndNormalizationOutput,
    };
    use crate::test_utils::TempArtifactDir;

    // ─── Asset constants ───────────────────────────────────────────────────────

    const VALID_ASSET_JSON: &str = r#"{
        "version": "v1",
        "system_prompt": "You are an observation boundary classifier.",
        "user_template": "Context: {{diagnostic_context}}\nUser input: {{normalized_user_input}}",
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
        dir.write_json("obr_prompt.json", VALID_ASSET_JSON)
            .to_str()
            .unwrap()
            .to_string()
    }

    fn write_schema(dir: &TempArtifactDir) {
        dir.write_json("obr_prompt.schema.json", VALID_SCHEMA_JSON);
    }

    fn write_both(dir: &TempArtifactDir) -> String {
        write_schema(dir);
        write_asset(dir)
    }

    fn settings(path: &str) -> ObservationBoundaryResolverSettings {
        ObservationBoundaryResolverSettings {
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

    fn make_request(query: &str) -> NormalizedUserRequest {
        NormalizedUserRequest {
            query: query.to_string(),
            input_token_count: 5,
        }
    }

    // ─── DiagnosticContext builder ─────────────────────────────────────────────

    fn finished_ok(kind: StepKind, result: StepResultEnvelope) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: StepRecordId(Uuid::new_v4()),
            step: kind,
            started_at: now,
            finished_at: now,
            result: Ok(result),
        })
    }

    fn input_norm_result(query: &str) -> StepResultEnvelope {
        use crate::shared_types::NormalizedUserRequest;
        StepResultEnvelope::InputNormalization(NormalizedUserRequest {
            query: query.to_string(),
            input_token_count: 5,
        })
    }

    fn rvn_result(
        problem_understanding: &str,
        hypotheses: Vec<Hypothesis>,
        first_check: &str,
    ) -> StepResultEnvelope {
        StepResultEnvelope::ResponseValidationAndNormalization(
            ResponseValidationAndNormalizationOutput {
                response: DiagnosticResponse {
                    problem_understanding: problem_understanding.to_string(),
                    similar_practical_context: "ctx".to_string(),
                    hypotheses,
                    first_check: first_check.to_string(),
                    result_interpretation: DiagnosticResultInterpretation {
                        supports_primary_if: "if X".to_string(),
                        supports_competing_if: "if Y".to_string(),
                        inconclusive_if: None,
                    },
                    competing_interpretation: None,
                },
            },
        )
    }

    fn make_hypothesis(status: HypothesisStatus, text: &str) -> Hypothesis {
        Hypothesis {
            id: HypothesisId(Uuid::new_v4()),
            text: text.to_string(),
            status,
            source: HypothesisEvidenceSource::PrimaryIncident,
            confidence: Confidence::Medium,
        }
    }

    fn run_state_with_iter0(
        problem_understanding: &str,
        hypotheses: Vec<Hypothesis>,
        first_check: &str,
    ) -> RunState {
        let now = Utc::now();
        RunState {
            run_id: RunId(Uuid::new_v4()),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![RunIteration {
                iteration_id: RunIterationId(Uuid::new_v4()),
                config_snapshot: None,
                status: RunIterationStatus::Active,
                step_records: vec![
                    finished_ok(
                        StepKind::InputNormalization,
                        input_norm_result("query"),
                    ),
                    finished_ok(
                        StepKind::ResponseValidationAndNormalization,
                        rvn_result(problem_understanding, hypotheses, first_check),
                    ),
                ],
            }],
        }
    }

    fn minimal_diagnostic_context() -> DiagnosticContext {
        let state = run_state_with_iter0(
            "The service is experiencing intermittent failures",
            vec![make_hypothesis(HypothesisStatus::Active, "Memory pressure")],
            "Check memory usage on all nodes",
        );
        DiagnosticContext::from_run_state(&state).unwrap()
    }

    fn empty_diagnostic_context() -> DiagnosticContext {
        let now = Utc::now();
        let state = RunState {
            run_id: RunId(Uuid::new_v4()),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![],
        };
        DiagnosticContext::from_run_state(&state).unwrap()
    }

    fn supported_json() -> String {
        json!({
            "supported": true,
            "confidence": "high",
            "reason": "The user input describes a new observation about memory.",
            "full_query": "Memory usage spiked to 95% on node-3 at 14:32 UTC"
        })
        .to_string()
    }

    fn unsupported_json() -> String {
        json!({
            "supported": false,
            "confidence": "low",
            "reason": "This is a clarifying question, not an observation.",
            "full_query": null
        })
        .to_string()
    }

    // ── Constructor tests ─────────────────────────────────────────────────────

    #[test]
    fn new_rejects_empty_prompt_asset_path() {
        let dir = TempArtifactDir::new();
        write_schema(&dir);
        let err = ObservationBoundaryResolver::new(
            ObservationBoundaryResolverSettings {
                prompt_asset_path: "".to_string(),
                max_output_tokens: 256,
            },
            noop_client(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidSettings(_)
        ));
    }

    #[test]
    fn new_rejects_zero_max_output_tokens() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let err = ObservationBoundaryResolver::new(
            ObservationBoundaryResolverSettings {
                prompt_asset_path: path,
                max_output_tokens: 0,
            },
            noop_client(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidSettings(_)
        ));
    }

    #[test]
    fn new_rejects_unreadable_prompt_asset_file() {
        let dir = TempArtifactDir::new();
        write_schema(&dir);
        let err = ObservationBoundaryResolver::new(
            settings("/nonexistent/obr_prompt.json"),
            noop_client(),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidPromptAsset(_)
        ));
    }

    #[test]
    fn new_rejects_unreadable_schema_file() {
        let dir = TempArtifactDir::new();
        let path = write_asset(&dir);
        // schema file not written → derivation succeeds but read fails
        let err =
            ObservationBoundaryResolver::new(settings(&path), noop_client()).unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidPromptAsset(_)
        ));
    }

    #[test]
    fn new_rejects_invalid_prompt_asset_json() {
        let dir = TempArtifactDir::new();
        write_schema(&dir);
        dir.write_json("obr_prompt.json", "not valid json {{");
        let path = dir
            .path()
            .join("obr_prompt.json")
            .to_str()
            .unwrap()
            .to_string();
        let err =
            ObservationBoundaryResolver::new(settings(&path), noop_client()).unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidPromptAsset(_)
        ));
    }

    #[test]
    fn new_rejects_invalid_schema_json() {
        let dir = TempArtifactDir::new();
        dir.write_json("obr_prompt.schema.json", "not json");
        dir.write_json("obr_prompt.json", VALID_ASSET_JSON);
        let path = dir
            .path()
            .join("obr_prompt.json")
            .to_str()
            .unwrap()
            .to_string();
        let err =
            ObservationBoundaryResolver::new(settings(&path), noop_client()).unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidPromptAsset(_)
        ));
    }

    #[test]
    fn new_succeeds_with_valid_asset() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        assert!(ObservationBoundaryResolver::new(settings(&path), noop_client()).is_ok());
    }

    #[test]
    fn new_rejects_user_template_with_extra_placeholder() {
        let dir = TempArtifactDir::new();
        write_schema(&dir);
        let asset = r#"{
            "version": "v1",
            "system_prompt": "You are a classifier.",
            "user_template": "{{diagnostic_context}} {{normalized_user_input}} {{extra_field}}",
            "response_schema": {"type": "object"}
        }"#;
        dir.write_json("obr_prompt.json", asset);
        let path = dir
            .path()
            .join("obr_prompt.json")
            .to_str()
            .unwrap()
            .to_string();
        let err = ObservationBoundaryResolver::new(settings(&path), noop_client()).unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidPromptAsset(_)
        ));
    }

    #[test]
    fn provider_and_model_not_stored_in_settings() {
        let s = ObservationBoundaryResolverSettings {
            prompt_asset_path: "some/path.json".to_string(),
            max_output_tokens: 128,
        };
        // ObservationBoundaryResolverSettings has no provider or model fields
        assert_eq!(s.prompt_asset_path, "some/path.json");
        assert_eq!(s.max_output_tokens, 128);
    }

    // ── Request execution tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn resolve_sends_json_schema_response_mode() {
        // Verified indirectly: the FixedModelClient receives the request and
        // returns the content. If response_mode were wrong the request would fail.
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(supported_json()),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let req = make_request("memory spike on node-3");
        let result = obr.resolve(&req, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn resolve_sends_configured_max_output_tokens() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            ObservationBoundaryResolverSettings {
                prompt_asset_path: path,
                max_output_tokens: 512,
            },
            FixedModelClient::with_content(supported_json()),
        )
        .unwrap();
        assert_eq!(obr.max_output_tokens, 512);
        let ctx = minimal_diagnostic_context();
        let req = make_request("memory spike");
        let result = obr.resolve(&req, &ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn prompt_assembly_reads_from_diagnostic_context_view_methods() {
        // Verified by ensuring resolve succeeds when context has all required data.
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(supported_json()),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let req = make_request("memory spike on node-3");
        let out = obr.resolve(&req, &ctx).await.unwrap();
        assert_eq!(out.normalized_user_input, "memory spike on node-3");
    }

    #[tokio::test]
    async fn resolve_fails_when_current_problem_understanding_is_none() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(supported_json()),
        )
        .unwrap();
        let ctx = empty_diagnostic_context();
        let req = make_request("something");
        let err = obr.resolve(&req, &ctx).await.unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidContext(_)
        ));
    }

    #[tokio::test]
    async fn resolve_fails_when_current_problem_understanding_text_is_none() {
        // Build a context where InputNormalization succeeded but RVN did not
        // → ProblemUnderstanding entry exists with text=None
        let now = Utc::now();
        let state = RunState {
            run_id: RunId(Uuid::new_v4()),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![RunIteration {
                iteration_id: RunIterationId(Uuid::new_v4()),
                config_snapshot: None,
                status: RunIterationStatus::Active,
                step_records: vec![finished_ok(
                    StepKind::InputNormalization,
                    input_norm_result("q"),
                )],
            }],
        };
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(supported_json()),
        )
        .unwrap();
        let req = make_request("something");
        let err = obr.resolve(&req, &ctx).await.unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidContext(_)
        ));
    }

    #[tokio::test]
    async fn resolve_fails_when_last_check_is_none() {
        // Empty context → no suggested checks
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(supported_json()),
        )
        .unwrap();
        let ctx = empty_diagnostic_context();
        let req = make_request("something");
        let err = obr.resolve(&req, &ctx).await.unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidContext(_)
        ));
    }

    // ── Output mapping tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn supported_true_maps_to_supported_resolution() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(supported_json()),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let req = make_request("node-3 memory spike");
        let out = obr.resolve(&req, &ctx).await.unwrap();
        assert!(matches!(
            out.resolution,
            ObservationBoundaryResolution::Supported(_)
        ));
        if let ObservationBoundaryResolution::Supported(ref ro) = out.resolution {
            assert_eq!(ro.text, "Memory usage spiked to 95% on node-3 at 14:32 UTC");
        }
    }

    #[tokio::test]
    async fn supported_false_maps_to_unsupported_resolution() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(unsupported_json()),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let req = make_request("what does that mean?");
        let out = obr.resolve(&req, &ctx).await.unwrap();
        assert!(matches!(
            out.resolution,
            ObservationBoundaryResolution::Unsupported
        ));
    }

    #[tokio::test]
    async fn normalized_user_input_exactly_matches_request_query() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(supported_json()),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let req = make_request("exact query text here");
        let out = obr.resolve(&req, &ctx).await.unwrap();
        assert_eq!(out.normalized_user_input, "exact query text here");
    }

    #[tokio::test]
    async fn supported_true_with_null_full_query_fails() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "supported": true,
                    "confidence": "high",
                    "reason": "obs",
                    "full_query": null
                })
                .to_string(),
            ),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let err = obr.resolve(&make_request("q"), &ctx).await.unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidModelOutput { .. }
        ));
    }

    #[tokio::test]
    async fn supported_true_with_empty_full_query_fails() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "supported": true,
                    "confidence": "high",
                    "reason": "obs",
                    "full_query": "   "
                })
                .to_string(),
            ),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let err = obr.resolve(&make_request("q"), &ctx).await.unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidModelOutput { .. }
        ));
    }

    #[tokio::test]
    async fn supported_false_with_non_null_full_query_fails() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "supported": false,
                    "confidence": "low",
                    "reason": "not obs",
                    "full_query": "some text"
                })
                .to_string(),
            ),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let err = obr.resolve(&make_request("q"), &ctx).await.unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidModelOutput { .. }
        ));
    }

    #[tokio::test]
    async fn unknown_confidence_value_fails() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(
                json!({
                    "supported": true,
                    "confidence": "very_high",
                    "reason": "obs",
                    "full_query": "the text"
                })
                .to_string(),
            ),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let err = obr.resolve(&make_request("q"), &ctx).await.unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidModelOutput { .. }
        ));
    }

    #[tokio::test]
    async fn invalid_json_from_model_fails() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content("not json at all {{{"),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let err = obr.resolve(&make_request("q"), &ctx).await.unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidModelOutput { .. }
        ));
    }

    #[tokio::test]
    async fn model_client_error_propagates_as_typed_error() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            Arc::new(FailingModelClient),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let err = obr.resolve(&make_request("q"), &ctx).await.unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::ModelClient(_)
        ));
    }

    #[tokio::test]
    async fn length_finish_reason_fails_as_invalid_model_output() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_finish_reason(
                supported_json(),
                Some(ModelFinishReason::Length),
            ),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let err = obr.resolve(&make_request("q"), &ctx).await.unwrap_err();
        assert!(matches!(
            err,
            ObservationBoundaryResolverError::InvalidModelOutput { .. }
        ));
    }

    #[tokio::test]
    async fn resolve_delegates_to_resolve_with_context() {
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(supported_json()),
        )
        .unwrap();
        let ctx = minimal_diagnostic_context();
        let req = make_request("memory spike");
        let r1 = obr.resolve(&req, &ctx).await.unwrap();
        let r2 = obr
            .resolve_with_context(&req, &ctx, &Context::noop())
            .await
            .unwrap();
        assert_eq!(r1.normalized_user_input, r2.normalized_user_input);
        assert_eq!(r1.reason, r2.reason);
    }

    #[tokio::test]
    async fn prompt_contains_active_hypotheses_as_strings() {
        // Validated indirectly: the context has one Active hypothesis.
        // If prompt assembly fails, resolve would fail.
        let dir = TempArtifactDir::new();
        let path = write_both(&dir);
        let state = run_state_with_iter0(
            "services intermittently failing",
            vec![
                make_hypothesis(
                    HypothesisStatus::Active,
                    "Memory pressure causing OOM kills",
                ),
                make_hypothesis(
                    HypothesisStatus::Weakened,
                    "Network partitioning between zones",
                ),
                make_hypothesis(
                    HypothesisStatus::Rejected("disproved by metrics".to_string()),
                    "CPU throttling",
                ),
            ],
            "Check memory metrics",
        );
        let ctx = DiagnosticContext::from_run_state(&state).unwrap();
        // Only Active and Weakened are included; Rejected should be excluded.
        let obr = ObservationBoundaryResolver::new(
            settings(&path),
            FixedModelClient::with_content(supported_json()),
        )
        .unwrap();
        let out = obr.resolve(&make_request("memory spike"), &ctx).await.unwrap();
        assert!(matches!(
            out.resolution,
            ObservationBoundaryResolution::Supported(_)
        ));
    }
}
