use std::sync::Arc;

use crate::api_clients::model::{
    ModelClient, ModelClientError, ModelFinishReason, ModelGenerationRequest, ModelMessage,
    ModelMessageRole, ModelResponseMode,
};
use crate::config::LlmStructuredGenerationSettings;
use crate::shared_types::{
    Context, LlmStructuredGenerationOutput, ModelTokenUsage, PromptContextAssemblyOutput,
};
use tracing::{info_span, field, Instrument};

const REQUIRED_RESPONSE_TOP_LEVEL_KEYS: &[&str] = &[
    "problem_understanding",
    "similar_practical_context",
    "first_check",
    "result_interpretation",
    "alternative_context_assessment",
    "active_hypotheses",
];

const VALID_HYPOTHESIS_SOURCES: &[&str] =
    &["primary_incident", "alternative_context", "theory_mechanism"];
const VALID_HYPOTHESIS_CONFIDENCES: &[&str] = &["low", "medium", "high"];

const REQUIRED_RESULT_INTERPRETATION_KEYS: &[&str] = &[
    "supports_primary_if",
    "supports_competing_if",
    "inconclusive_if",
];
const REPAIR_MAX_OUTPUT_TOKENS_CAP: u32 = 2000;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum LlmStructuredGenerationError {
    #[error("invalid config: {0}")]
    InvalidConfig(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error(transparent)]
    Model(#[from] ModelClientError),
    #[error("invalid model output: {reason}")]
    InvalidModelOutput {
        reason: String,
        token_usage: ModelTokenUsage,
        finish_reason: Option<ModelFinishReason>,
    },
}

pub struct LlmStructuredGeneration {
    model_client: Arc<dyn ModelClient>,
    max_output_tokens: u32,
}

impl std::fmt::Debug for LlmStructuredGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmStructuredGeneration")
            .field("max_output_tokens", &self.max_output_tokens)
            .finish_non_exhaustive()
    }
}

impl LlmStructuredGeneration {
    pub fn new(
        settings: LlmStructuredGenerationSettings,
        model_client: Arc<dyn ModelClient>,
    ) -> Result<Self, LlmStructuredGenerationError> {
        if settings.max_output_tokens == 0 {
            return Err(LlmStructuredGenerationError::InvalidConfig(
                "max_output_tokens must be greater than zero".to_string(),
            ));
        }
        Ok(Self {
            model_client,
            max_output_tokens: settings.max_output_tokens,
        })
    }

    pub async fn generate(
        &self,
        prompt_context: &PromptContextAssemblyOutput,
    ) -> Result<LlmStructuredGenerationOutput, LlmStructuredGenerationError> {
        self.generate_with_context(prompt_context, &Context::noop())
            .await
    }

    pub async fn generate_with_context(
        &self,
        prompt_context: &PromptContextAssemblyOutput,
        context: &Context,
    ) -> Result<LlmStructuredGenerationOutput, LlmStructuredGenerationError> {
        let prompt = &prompt_context.prompt;
        let prompt_chars = prompt.len();
        let prompt_empty = prompt.trim().is_empty();
        let oi_span = crate::observability::oi_llm_diagnostic_response_span(
            &context.open_inference.root_span,
        );
        let oi_input_json = serde_json::json!({
            "prompt_chars": prompt_chars,
            "prompt_empty": prompt_empty
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
            "request_pipeline.llm_structured_generation",
            module.name = "llm_structured_generation",
            llm.task = "diagnostic_response",
            llm.prompt_chars = prompt_chars as i64,
            llm.prompt_empty = prompt_empty,
            llm.response_mode = field::Empty,
            llm.temperature = field::Empty,
            llm.max_output_tokens = field::Empty,
            llm.finish_reason = field::Empty,
            llm.prompt_tokens = field::Empty,
            llm.completion_tokens = field::Empty,
            llm.total_tokens = field::Empty,
            llm.output.parse_success = field::Empty,
            llm.output.top_level_type = field::Empty,
            llm.output.object_field_count = field::Empty,
            llm.output.has_markdown_fence = field::Empty,
            llm.output.content_chars = field::Empty,
            llm.output.shape_valid_initial = field::Empty,
            llm.output.strip_attempted = field::Empty,
            llm.output.strip_succeeded = field::Empty,
            llm.output.unwrap_attempted = field::Empty,
            llm.output.unwrap_succeeded = field::Empty,
            llm.output.rename_attempted = field::Empty,
            llm.output.rename_succeeded = field::Empty,
            llm.output.repair_attempted = field::Empty,
            llm.output.repair_succeeded = field::Empty,
            module.outcome = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        self.generate_instrumented(prompt_context, &oi_span)
            .instrument(span)
            .await
    }

    async fn generate_instrumented(
        &self,
        prompt_context: &PromptContextAssemblyOutput,
        oi_span: &tracing::Span,
    ) -> Result<LlmStructuredGenerationOutput, LlmStructuredGenerationError> {
        if prompt_context.prompt.trim().is_empty() {
            crate::observability::record_error(
                oi_span,
                "LlmStructuredGeneration.InvalidInput",
                "prompt must be non-empty after trimming",
            );
            tracing::Span::current().record("module.outcome", "failure");
            tracing::Span::current().record("status", "error");
            tracing::Span::current().record("error.type", "LlmStructuredGeneration.InvalidInput");
            tracing::Span::current()
                .record("error.message", "prompt must be non-empty after trimming");
            return Err(LlmStructuredGenerationError::InvalidInput(
                "prompt must be non-empty after trimming".to_string(),
            ));
        }

        let request = ModelGenerationRequest {
            messages: vec![ModelMessage {
                role: ModelMessageRole::User,
                content: prompt_context.prompt.clone(),
            }],
            response_mode: ModelResponseMode::JsonSchema(prompt_context.response_schema.clone()),
            temperature: 0.0,
            max_output_tokens: Some(self.max_output_tokens),
        };

        let llm_span = info_span!(
            "llm.call.diagnostic_response",
            model.response_mode = field::Empty,
            model.temperature = field::Empty,
            model.max_output_tokens = field::Empty,
            model.finish_reason = field::Empty,
            model.prompt_tokens = field::Empty,
            model.completion_tokens = field::Empty,
            model.total_tokens = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        let response = self.run_model_request(&request, oi_span, llm_span).await;

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                crate::observability::record_error(
                    oi_span,
                    "LlmStructuredGeneration.Model",
                    &format!("Model client error: {}", e),
                );
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                tracing::Span::current().record("error.type", "LlmStructuredGeneration.Model");
                tracing::Span::current()
                    .record("error.message", format!("Model client error: {}", e));
                return Err(LlmStructuredGenerationError::Model(e));
            }
        };

        let token_usage = ModelTokenUsage {
            prompt_tokens: response.prompt_tokens,
            completion_tokens: response.completion_tokens,
            total_tokens: response.total_tokens,
        };
        let finish_reason = response.finish_reason.clone();
        let content = response.content.clone();
        oi_span.record("llm.raw_response", content.as_str());

        tracing::Span::current().record("llm.response_mode", "JsonSchema");
        tracing::Span::current().record("llm.temperature", 0.0);
        tracing::Span::current().record("llm.max_output_tokens", self.max_output_tokens as i64);
        if let Some(ref fr) = finish_reason {
            tracing::Span::current().record("llm.finish_reason", format!("{:?}", fr));
        }
        if let Some(pt) = response.prompt_tokens {
            tracing::Span::current().record("llm.prompt_tokens", pt as i64);
        }
        if let Some(ct) = response.completion_tokens {
            tracing::Span::current().record("llm.completion_tokens", ct as i64);
        }
        if let Some(tt) = response.total_tokens {
            tracing::Span::current().record("llm.total_tokens", tt as i64);
        }

        // Step 2: inspect finish_reason — only Stop or None is acceptable
        match &finish_reason {
            Some(ModelFinishReason::Stop) | None => {}
            Some(_) => {
                crate::observability::record_error(
                    oi_span,
                    "LlmStructuredGeneration.InvalidModelOutput",
                    "model stopped with a non-Stop finish reason",
                );
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                tracing::Span::current().record("error.type", "LlmStructuredGeneration.InvalidModelOutput");
                tracing::Span::current()
                    .record("error.message", "model stopped with a non-Stop finish reason");
                return Err(LlmStructuredGenerationError::InvalidModelOutput {
                    reason: "model stopped with a non-Stop finish reason".to_string(),
                    token_usage,
                    finish_reason,
                });
            }
        }

        let content_chars = content.len();
        let has_markdown_fence = content.contains("```");

        // Step 3: parse content as JSON
        let parsed = parse_json_object_output(
            &content,
            content_chars,
            has_markdown_fence,
            &token_usage,
            &finish_reason,
            oi_span,
        )?;

        let parse_success = true;
        let top_level_type = "object".to_string();
        let object_field_count = parsed.as_object().map(|obj| obj.len()).unwrap_or(0);
        let shape_valid_initial = json_matches_expected_shape(&parsed);
        let mut final_parsed = parsed;
        let mut final_token_usage = token_usage;

        if !shape_valid_initial {
            tracing::event!(
                tracing::Level::INFO,
                event.name = "llm_output_initial_invalid_payload",
                llm.raw_response = %content,
                llm.output.parsed_json = %serde_json::to_string(&final_parsed)
                    .unwrap_or_else(|_| "{}".to_string())
            );
        }

        let (strip_attempted, strip_succeeded) = if !shape_valid_initial {
            match try_strip_extra_fields(&final_parsed) {
                Some((stripped, stripped_field_names)) => {
                    oi_span.in_scope(|| {
                        tracing::event!(
                            tracing::Level::INFO,
                            event.name = "llm_output_strip_succeeded",
                            llm.output.stripped_field_names = %stripped_field_names,
                        );
                    });
                    final_parsed = stripped;
                    (true, true)
                }
                None => (true, false),
            }
        } else {
            (false, false)
        };

        let (unwrap_attempted, unwrap_succeeded) =
            if !shape_valid_initial && !strip_succeeded {
                match try_unwrap_single_key(&final_parsed) {
                    Some((unwrapped, wrapper_key)) => {
                        oi_span.in_scope(|| {
                            tracing::event!(
                                tracing::Level::INFO,
                                event.name = "llm_output_unwrap_succeeded",
                                llm.output.unwrapped_from_key = %wrapper_key,
                            );
                        });
                        final_parsed = unwrapped;
                        (true, true)
                    }
                    None => (true, false),
                }
            } else {
                (false, false)
            };

        let (rename_attempted, rename_succeeded) =
            if !shape_valid_initial && !strip_succeeded && !unwrap_succeeded {
                match try_rename_misnamed_field(&final_parsed) {
                    Some((renamed, from_key, to_key)) => {
                        oi_span.in_scope(|| {
                            tracing::event!(
                                tracing::Level::INFO,
                                event.name = "llm_output_rename_succeeded",
                                llm.output.renamed_from = %from_key,
                                llm.output.renamed_to = %to_key,
                            );
                        });
                        final_parsed = renamed;
                        (true, true)
                    }
                    None => (true, false),
                }
            } else {
                (false, false)
            };

        let should_attempt_repair = !json_matches_expected_shape(&final_parsed)
            && should_attempt_shape_repair(&final_parsed);

        tracing::Span::current().record("llm.output.parse_success", parse_success);
        tracing::Span::current().record("llm.output.top_level_type", &top_level_type);
        tracing::Span::current().record("llm.output.object_field_count", object_field_count as i64);
        tracing::Span::current().record("llm.output.has_markdown_fence", has_markdown_fence);
        tracing::Span::current().record("llm.output.content_chars", content_chars as i64);
        tracing::Span::current().record("llm.output.shape_valid_initial", shape_valid_initial);
        tracing::Span::current().record("llm.output.strip_attempted", strip_attempted);
        tracing::Span::current().record("llm.output.strip_succeeded", strip_succeeded);
        tracing::Span::current().record("llm.output.unwrap_attempted", unwrap_attempted);
        tracing::Span::current().record("llm.output.unwrap_succeeded", unwrap_succeeded);
        tracing::Span::current().record("llm.output.rename_attempted", rename_attempted);
        tracing::Span::current().record("llm.output.rename_succeeded", rename_succeeded);
        tracing::Span::current().record("llm.output.repair_attempted", should_attempt_repair);
        tracing::Span::current().record("llm.output.repair_succeeded", false);

        if should_attempt_repair {
            let repair_max_output_tokens = self.max_output_tokens.min(REPAIR_MAX_OUTPUT_TOKENS_CAP);
            let repair_request = ModelGenerationRequest {
                messages: vec![ModelMessage {
                    role: ModelMessageRole::User,
                    content: build_shape_repair_prompt(&prompt_context.prompt, &content),
                }],
                response_mode: ModelResponseMode::JsonObject,
                temperature: 0.0,
                max_output_tokens: Some(repair_max_output_tokens),
            };

            let repair_span = info_span!(
                "llm.call.diagnostic_response_repair",
                model.response_mode = field::Empty,
                model.temperature = field::Empty,
                model.max_output_tokens = field::Empty,
                model.finish_reason = field::Empty,
                model.prompt_tokens = field::Empty,
                model.completion_tokens = field::Empty,
                model.total_tokens = field::Empty,
                status = field::Empty,
                error.type = field::Empty,
                error.message = field::Empty,
            );

            let repair_response =
                self.run_model_request(&repair_request, oi_span, repair_span).await?;
            let repair_token_usage = ModelTokenUsage {
                prompt_tokens: repair_response.prompt_tokens,
                completion_tokens: repair_response.completion_tokens,
                total_tokens: repair_response.total_tokens,
            };
            let repair_finish_reason = repair_response.finish_reason.clone();

            match &repair_finish_reason {
                Some(ModelFinishReason::Stop) | None => {}
                Some(ModelFinishReason::Length) => {
                    return Err(LlmStructuredGenerationError::InvalidModelOutput {
                        reason: "repair model hit output token limit".to_string(),
                        token_usage: combine_token_usage(&final_token_usage, &repair_token_usage),
                        finish_reason: repair_finish_reason,
                    });
                }
                Some(ref fr) => {
                    return Err(LlmStructuredGenerationError::InvalidModelOutput {
                        reason: format!("repair model stopped with unexpected finish reason: {:?}", fr),
                        token_usage: combine_token_usage(&final_token_usage, &repair_token_usage),
                        finish_reason: repair_finish_reason,
                    });
                }
            }

            let repaired = parse_json_object_output(
                &repair_response.content,
                repair_response.content.len(),
                repair_response.content.contains("```"),
                &repair_token_usage,
                &repair_finish_reason,
                oi_span,
            )?;

            if !json_matches_expected_shape(&repaired) {
                tracing::event!(
                    tracing::Level::INFO,
                    event.name = "llm_output_repair_invalid_payload",
                    llm.raw_response = %repair_response.content,
                    llm.output.parsed_json = %serde_json::to_string(&repaired)
                        .unwrap_or_else(|_| "{}".to_string())
                );
                return Err(LlmStructuredGenerationError::InvalidModelOutput {
                    reason: "repair response JSON does not match expected shape".to_string(),
                    token_usage: combine_token_usage(&final_token_usage, &repair_token_usage),
                    finish_reason: repair_finish_reason,
                });
            }

            final_token_usage = combine_token_usage(&final_token_usage, &repair_token_usage);
            final_parsed = repaired;
            tracing::Span::current().record("llm.output.repair_succeeded", true);
        }

        let parsed_json =
            serde_json::to_string(&final_parsed).unwrap_or_else(|_| "{}".to_string());
        tracing::event!(
            tracing::Level::INFO,
            event.name = "llm_output_payload",
            llm.output.parsed_json = %parsed_json
        );
        oi_span.record("output.value", parsed_json.as_str());
        oi_span.record("output.mime_type", "application/json");
        oi_span.record("status", "ok");
        tracing::Span::current().record("module.outcome", "success");
        tracing::Span::current().record("status", "ok");

        Ok(LlmStructuredGenerationOutput {
            response_json: final_parsed,
            token_usage: final_token_usage,
        })
    }

    async fn run_model_request(
        &self,
        request: &ModelGenerationRequest,
        oi_span: &tracing::Span,
        llm_span: tracing::Span,
    ) -> Result<crate::api_clients::model::shared_types::ModelGenerationResponse, ModelClientError>
    {
        async {
            match self.model_client.generate(request).await {
                Ok(r) => {
                    oi_span.record("llm.model_name", "unknown");
                    oi_span.record("llm.provider", "unknown");
                    tracing::Span::current().record("model.response_mode", "JsonObject");
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
                    crate::observability::record_error(
                        oi_span,
                        "LlmStructuredGeneration.Model",
                        &format!("Model client error: {}", e),
                    );
                    tracing::Span::current().record("status", "error");
                    tracing::Span::current().record("error.type", "LlmStructuredGeneration.Model");
                    tracing::Span::current()
                        .record("error.message", format!("Model client error: {}", e));
                    Err(e)
                }
            }
        }
        .instrument(llm_span)
        .await
    }
}

fn parse_json_object_output(
    content: &str,
    content_chars: usize,
    has_markdown_fence: bool,
    token_usage: &ModelTokenUsage,
    finish_reason: &Option<ModelFinishReason>,
    oi_span: &tracing::Span,
) -> Result<serde_json::Value, LlmStructuredGenerationError> {
    let parsed: serde_json::Value = serde_json::from_str(content).map_err(|e| {
        crate::observability::record_error(
            oi_span,
            "LlmStructuredGeneration.InvalidModelOutput",
            &format!("Failed to parse JSON: {}", e),
        );
        tracing::Span::current().record("llm.output.parse_success", false);
        tracing::Span::current().record("llm.output.content_chars", content_chars as i64);
        tracing::Span::current().record("llm.output.has_markdown_fence", has_markdown_fence);
        tracing::Span::current().record("module.outcome", "failure");
        tracing::Span::current().record("status", "error");
        tracing::Span::current().record("error.type", "LlmStructuredGeneration.InvalidModelOutput");
        tracing::Span::current()
            .record("error.message", format!("Failed to parse JSON: {}", e));
        LlmStructuredGenerationError::InvalidModelOutput {
            reason: "model response is not valid JSON".to_string(),
            token_usage: token_usage.clone(),
            finish_reason: finish_reason.clone(),
        }
    })?;

    if !parsed.is_object() {
        crate::observability::record_error(
            oi_span,
            "LlmStructuredGeneration.InvalidModelOutput",
            "model response is not a JSON object",
        );
        tracing::Span::current().record("llm.output.parse_success", false);
        tracing::Span::current().record("llm.output.content_chars", content_chars as i64);
        tracing::Span::current().record("llm.output.has_markdown_fence", has_markdown_fence);
        tracing::Span::current().record("module.outcome", "failure");
        tracing::Span::current().record("status", "error");
        tracing::Span::current().record("error.type", "LlmStructuredGeneration.InvalidModelOutput");
        tracing::Span::current()
            .record("error.message", "model response is not a JSON object");
        return Err(LlmStructuredGenerationError::InvalidModelOutput {
            reason: "model response is not a JSON object".to_string(),
            token_usage: token_usage.clone(),
            finish_reason: finish_reason.clone(),
        });
    }

    Ok(parsed)
}

fn json_matches_expected_shape(parsed: &serde_json::Value) -> bool {
    let Some(obj) = parsed.as_object() else {
        return false;
    };

    if obj.len() != REQUIRED_RESPONSE_TOP_LEVEL_KEYS.len() {
        return false;
    }

    if !REQUIRED_RESPONSE_TOP_LEVEL_KEYS
        .iter()
        .all(|key| obj.contains_key(*key))
    {
        return false;
    }

    let Some(problem_understanding) = obj.get("problem_understanding") else {
        return false;
    };
    if !problem_understanding.is_string() {
        return false;
    }

    let Some(similar_practical_context) = obj.get("similar_practical_context") else {
        return false;
    };
    if !similar_practical_context.is_string() {
        return false;
    }

    let Some(active_hypotheses) = obj.get("active_hypotheses").and_then(|v| v.as_array()) else {
        return false;
    };
    if !(2..=3).contains(&active_hypotheses.len()) {
        return false;
    }
    for item in active_hypotheses {
        let Some(item_obj) = item.as_object() else {
            return false;
        };
        if item_obj.len() != 3 {
            return false;
        }
        if !item_obj.get("hypothesis").map_or(false, |v| v.is_string()) {
            return false;
        }
        let source_ok = item_obj
            .get("source")
            .and_then(|v| v.as_str())
            .map_or(false, |s| VALID_HYPOTHESIS_SOURCES.contains(&s));
        if !source_ok {
            return false;
        }
        let confidence_ok = item_obj
            .get("confidence")
            .and_then(|v| v.as_str())
            .map_or(false, |s| VALID_HYPOTHESIS_CONFIDENCES.contains(&s));
        if !confidence_ok {
            return false;
        }
    }

    let Some(first_check) = obj.get("first_check") else {
        return false;
    };
    if !first_check.is_string() {
        return false;
    }

    let Some(result_interpretation) = obj.get("result_interpretation").and_then(|v| v.as_object())
    else {
        return false;
    };

    if result_interpretation.len() != REQUIRED_RESULT_INTERPRETATION_KEYS.len() {
        return false;
    }

    if !REQUIRED_RESULT_INTERPRETATION_KEYS
        .iter()
        .all(|key| result_interpretation.contains_key(*key))
    {
        return false;
    }

    let Some(supports_primary_if) = result_interpretation.get("supports_primary_if") else {
        return false;
    };
    if !supports_primary_if.is_string() {
        return false;
    }

    let Some(supports_competing_if) = result_interpretation.get("supports_competing_if") else {
        return false;
    };
    if !supports_competing_if.is_string() {
        return false;
    }

    let Some(inconclusive_if) = result_interpretation.get("inconclusive_if") else {
        return false;
    };
    if !(inconclusive_if.is_null() || inconclusive_if.is_string()) {
        return false;
    }

    let Some(aca) = obj
        .get("alternative_context_assessment")
        .and_then(|v| v.as_object())
    else {
        return false;
    };
    if aca.len() != 2 {
        return false;
    }
    if !aca.get("used_as_hypothesis").map_or(false, |v| v.is_boolean()) {
        return false;
    }
    if !aca.get("reason").map_or(false, |v| v.is_string()) {
        return false;
    }

    true
}

fn try_strip_extra_fields(parsed: &serde_json::Value) -> Option<(serde_json::Value, String)> {
    let obj = parsed.as_object()?;

    if !REQUIRED_RESPONSE_TOP_LEVEL_KEYS
        .iter()
        .all(|k| obj.contains_key(*k))
    {
        return None;
    }

    let required_set: std::collections::HashSet<&str> =
        REQUIRED_RESPONSE_TOP_LEVEL_KEYS.iter().copied().collect();
    let mut extra_keys: Vec<String> = obj
        .keys()
        .filter(|k| !required_set.contains(k.as_str()))
        .cloned()
        .collect();
    extra_keys.sort();

    let mut stripped = serde_json::Map::new();
    for key in REQUIRED_RESPONSE_TOP_LEVEL_KEYS {
        stripped.insert(key.to_string(), obj[*key].clone());
    }

    if let Some(ri) = stripped.get_mut("result_interpretation") {
        if let Some(ri_obj) = ri.as_object().cloned() {
            if REQUIRED_RESULT_INTERPRETATION_KEYS
                .iter()
                .all(|k| ri_obj.contains_key(*k))
            {
                let ri_required_set: std::collections::HashSet<&str> =
                    REQUIRED_RESULT_INTERPRETATION_KEYS.iter().copied().collect();
                let mut ri_extra: Vec<String> = ri_obj
                    .keys()
                    .filter(|k| !ri_required_set.contains(k.as_str()))
                    .cloned()
                    .collect();
                ri_extra.sort();
                extra_keys.extend(ri_extra);

                let mut ri_stripped = serde_json::Map::new();
                for key in REQUIRED_RESULT_INTERPRETATION_KEYS {
                    ri_stripped.insert(key.to_string(), ri_obj[*key].clone());
                }
                *ri = serde_json::Value::Object(ri_stripped);
            }
        }
    }

    let result = serde_json::Value::Object(stripped);
    if json_matches_expected_shape(&result) {
        Some((result, extra_keys.join(", ")))
    } else {
        None
    }
}

fn try_rename_misnamed_field(parsed: &serde_json::Value) -> Option<(serde_json::Value, String, String)> {
    let obj = parsed.as_object()?;
    if obj.len() != REQUIRED_RESPONSE_TOP_LEVEL_KEYS.len() {
        return None;
    }

    let required_set: std::collections::HashSet<&str> =
        REQUIRED_RESPONSE_TOP_LEVEL_KEYS.iter().copied().collect();

    let extra_keys: Vec<&str> = obj
        .keys()
        .filter(|k| !required_set.contains(k.as_str()))
        .map(|k| k.as_str())
        .collect();

    let missing_keys: Vec<&str> = REQUIRED_RESPONSE_TOP_LEVEL_KEYS
        .iter()
        .filter(|k| !obj.contains_key(**k))
        .copied()
        .collect();

    if extra_keys.len() != 1 || missing_keys.len() != 1 {
        return None;
    }

    let extra_key = extra_keys[0];
    let missing_key = missing_keys[0];
    let value = obj[extra_key].clone();

    let mut renamed = obj.clone();
    renamed.remove(extra_key);
    renamed.insert(missing_key.to_string(), value);

    let result = serde_json::Value::Object(renamed);
    if json_matches_expected_shape(&result) {
        Some((result, extra_key.to_string(), missing_key.to_string()))
    } else {
        None
    }
}

fn try_unwrap_single_key(parsed: &serde_json::Value) -> Option<(serde_json::Value, String)> {
    let obj = parsed.as_object()?;
    if obj.len() != 1 {
        return None;
    }
    let (key, inner) = obj.iter().next()?;
    if json_matches_expected_shape(inner) {
        Some((inner.clone(), key.clone()))
    } else {
        None
    }
}

fn should_attempt_shape_repair(parsed: &serde_json::Value) -> bool {
    let Some(obj) = parsed.as_object() else {
        return false;
    };

    obj.keys()
        .any(|key| REQUIRED_RESPONSE_TOP_LEVEL_KEYS.contains(&key.as_str()))
}

fn build_shape_repair_prompt(original_prompt: &str, invalid_json: &str) -> String {
    format!(
        concat!(
            "Repair the JSON shape only.\n",
            "Do not write a new answer from scratch.\n",
            "Preserve the original meaning and reuse existing text whenever possible.\n",
            "Return exactly one valid JSON object and nothing else.\n",
            "Required top-level keys exactly: ",
            "[\"problem_understanding\",\"similar_practical_context\",\"first_check\",\"result_interpretation\",\"alternative_context_assessment\",\"active_hypotheses\"].\n",
            "Required nested keys inside result_interpretation exactly: ",
            "[\"supports_primary_if\",\"supports_competing_if\",\"inconclusive_if\"].\n",
            "Constraints:\n",
            "- active_hypotheses must be an array of 2 or 3 objects, each with: hypothesis (string), source (one of: primary_incident, alternative_context, theory_mechanism), confidence (one of: low, medium, high)\n",
            "- alternative_context_assessment must be an object with: used_as_hypothesis (boolean), reason (string)\n",
            "- inconclusive_if may be null\n",
            "- no markdown fences and no text outside JSON\n",
            "- preserve uncertainty and avoid definitive-root-cause language\n",
            "- keep every field concise\n",
            "- do not add extra top-level keys\n",
            "- if a required field is missing, fill it from the closest compatible content in the previous JSON\n",
            "- if no compatible content exists, use null only for nullable fields; otherwise use a short string\n\n",
            "Return JSON in this exact shape:\n",
            "{{\n",
            "  \"problem_understanding\": \"string\",\n",
            "  \"similar_practical_context\": \"string\",\n",
            "  \"first_check\": \"string\",\n",
            "  \"result_interpretation\": {{\n",
            "    \"supports_primary_if\": \"string\",\n",
            "    \"supports_competing_if\": \"string\",\n",
            "    \"inconclusive_if\": \"string|null\"\n",
            "  }},\n",
            "  \"alternative_context_assessment\": {{\n",
            "    \"used_as_hypothesis\": true,\n",
            "    \"reason\": \"string\"\n",
            "  }},\n",
            "  \"active_hypotheses\": [\n",
            "    {{\"hypothesis\": \"string\", \"source\": \"primary_incident|alternative_context|theory_mechanism\", \"confidence\": \"low|medium|high\"}}\n",
            "  ]\n",
            "}}\n\n",
            "Original task prompt:\n{original_prompt}\n\n",
            "Previous invalid JSON:\n{invalid_json}\n"
        ),
        original_prompt = original_prompt,
        invalid_json = invalid_json,
    )
}

fn combine_optional_counts(lhs: Option<usize>, rhs: Option<usize>) -> Option<usize> {
    match (lhs, rhs) {
        (Some(a), Some(b)) => Some(a + b),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn combine_token_usage(lhs: &ModelTokenUsage, rhs: &ModelTokenUsage) -> ModelTokenUsage {
    ModelTokenUsage {
        prompt_tokens: combine_optional_counts(lhs.prompt_tokens, rhs.prompt_tokens),
        completion_tokens: combine_optional_counts(lhs.completion_tokens, rhs.completion_tokens),
        total_tokens: combine_optional_counts(lhs.total_tokens, rhs.total_tokens),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_clients::model::shared_types::{
        ModelGenerationRequest, ModelGenerationResponse,
    };
    use std::sync::Mutex;

    // ---------------------------------------------------------------------------
    // Stubs
    // ---------------------------------------------------------------------------

    struct StubModelClient {
        response: ModelGenerationResponse,
        captured: Mutex<Option<ModelGenerationRequest>>,
    }

    impl StubModelClient {
        fn ok(response: ModelGenerationResponse) -> Arc<Self> {
            Arc::new(Self {
                response,
                captured: Mutex::new(None),
            })
        }

        fn last_request(&self) -> Option<ModelGenerationRequest> {
            self.captured.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl ModelClient for StubModelClient {
        async fn generate(
            &self,
            request: &ModelGenerationRequest,
        ) -> Result<ModelGenerationResponse, ModelClientError> {
            *self.captured.lock().unwrap() = Some(request.clone());
            Ok(self.response.clone())
        }
    }

    struct ErrStubModelClient {
        error: Mutex<Option<ModelClientError>>,
    }

    impl ErrStubModelClient {
        fn once(error: ModelClientError) -> Arc<Self> {
            Arc::new(Self {
                error: Mutex::new(Some(error)),
            })
        }
    }

    #[async_trait::async_trait]
    impl ModelClient for ErrStubModelClient {
        async fn generate(
            &self,
            _request: &ModelGenerationRequest,
        ) -> Result<ModelGenerationResponse, ModelClientError> {
            Err(self.error.lock().unwrap().take().unwrap())
        }
    }

    struct SeqStubModelClient {
        responses: Mutex<Vec<ModelGenerationResponse>>,
        captured: Mutex<Vec<ModelGenerationRequest>>,
    }

    impl SeqStubModelClient {
        fn ok(responses: Vec<ModelGenerationResponse>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses.into_iter().rev().collect()),
                captured: Mutex::new(Vec::new()),
            })
        }

        fn captured_requests(&self) -> Vec<ModelGenerationRequest> {
            self.captured.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl ModelClient for SeqStubModelClient {
        async fn generate(
            &self,
            request: &ModelGenerationRequest,
        ) -> Result<ModelGenerationResponse, ModelClientError> {
            self.captured.lock().unwrap().push(request.clone());
            self.responses
                .lock()
                .unwrap()
                .pop()
                .ok_or_else(|| ModelClientError::Transport("no stub response left".into()))
        }
    }

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    fn settings(max_output_tokens: u32) -> LlmStructuredGenerationSettings {
        LlmStructuredGenerationSettings { max_output_tokens }
    }

    fn ok_response(content: &str) -> ModelGenerationResponse {
        ModelGenerationResponse {
            content: content.to_string(),
            finish_reason: Some(ModelFinishReason::Stop),
            prompt_tokens: Some(10),
            completion_tokens: Some(5),
            total_tokens: Some(15),
        }
    }

    fn prompt_ctx(prompt: &str) -> PromptContextAssemblyOutput {
        PromptContextAssemblyOutput {
            prompt: prompt.to_string(),
            response_schema: serde_json::Value::Object(serde_json::Map::new()),
            evidence_topology: Default::default(),
            incident_evidence_chunks: vec![],
            theory_chunks: vec![],
        }
    }

    fn make_instance(client: Arc<dyn ModelClient>) -> LlmStructuredGeneration {
        LlmStructuredGeneration::new(settings(512), client).unwrap()
    }

    // ---------------------------------------------------------------------------
    // Constructor
    // ---------------------------------------------------------------------------

    #[test]
    fn constructor_rejects_zero_max_output_tokens() {
        let client = StubModelClient::ok(ok_response("{}"));
        let err = LlmStructuredGeneration::new(settings(0), client).unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidConfig(_)),
            "expected InvalidConfig, got: {err}"
        );
    }

    #[test]
    fn constructor_accepts_nonzero_max_output_tokens() {
        let client = StubModelClient::ok(ok_response("{}"));
        assert!(LlmStructuredGeneration::new(settings(1), client).is_ok());
    }

    // ---------------------------------------------------------------------------
    // Input validation
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn generate_rejects_empty_prompt() {
        let inst = make_instance(StubModelClient::ok(ok_response("{}")));
        let err = inst.generate(&prompt_ctx("")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidInput(_)),
            "expected InvalidInput, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_rejects_whitespace_only_prompt() {
        let inst = make_instance(StubModelClient::ok(ok_response("{}")));
        let err = inst.generate(&prompt_ctx("   \t\n  ")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidInput(_)),
            "expected InvalidInput, got: {err}"
        );
    }

    // ---------------------------------------------------------------------------
    // Model client error propagation
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn generate_propagates_model_client_error() {
        let client = ErrStubModelClient::once(ModelClientError::Transport("timeout".into()));
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::Model(_)),
            "expected Model, got: {err}"
        );
    }

    // ---------------------------------------------------------------------------
    // Finish-reason rules
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn generate_accepts_stop_finish_reason() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: r#"{"ok":true}"#.into(),
            finish_reason: Some(ModelFinishReason::Stop),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        });
        let inst = make_instance(client);
        assert!(inst.generate(&prompt_ctx("hello")).await.is_ok());
    }

    #[tokio::test]
    async fn generate_accepts_none_finish_reason_with_valid_json() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: r#"{"ok":true}"#.into(),
            finish_reason: None,
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        });
        let inst = make_instance(client);
        assert!(inst.generate(&prompt_ctx("hello")).await.is_ok());
    }

    #[tokio::test]
    async fn generate_rejects_length_finish_reason() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: r#"{"partial":true}"#.into(),
            finish_reason: Some(ModelFinishReason::Length),
            prompt_tokens: Some(10),
            completion_tokens: Some(512),
            total_tokens: Some(522),
        });
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidModelOutput { .. }),
            "expected InvalidModelOutput, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_does_not_parse_json_when_length_finish_reason() {
        // content is truncated/invalid JSON — still must not parse it
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: r#"{"truncated":"#.into(),
            finish_reason: Some(ModelFinishReason::Length),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        });
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(
                err,
                LlmStructuredGenerationError::InvalidModelOutput {
                    ref reason,
                    ..
                } if reason.contains("non-Stop")
            ),
            "expected non-Stop reason, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_rejects_content_filter_finish_reason() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: "{}".into(),
            finish_reason: Some(ModelFinishReason::ContentFilter),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        });
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidModelOutput { .. }),
            "expected InvalidModelOutput, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_rejects_tool_calls_finish_reason() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: "{}".into(),
            finish_reason: Some(ModelFinishReason::ToolCalls),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        });
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidModelOutput { .. }),
            "expected InvalidModelOutput, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_rejects_unknown_finish_reason() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: "{}".into(),
            finish_reason: Some(ModelFinishReason::Unknown("custom_stop".into())),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        });
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidModelOutput { .. }),
            "expected InvalidModelOutput, got: {err}"
        );
    }

    // ---------------------------------------------------------------------------
    // JSON parsing
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn generate_rejects_invalid_json() {
        let client = StubModelClient::ok(ok_response("not json at all"));
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidModelOutput { .. }),
            "expected InvalidModelOutput, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_rejects_json_array() {
        let client = StubModelClient::ok(ok_response(r#"[{"a":1}]"#));
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidModelOutput { .. }),
            "expected InvalidModelOutput for JSON array, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_rejects_json_string() {
        let client = StubModelClient::ok(ok_response(r#""hello""#));
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidModelOutput { .. }),
            "expected InvalidModelOutput for JSON string, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_rejects_json_number() {
        let client = StubModelClient::ok(ok_response("42"));
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidModelOutput { .. }),
            "expected InvalidModelOutput for JSON number, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_rejects_json_null() {
        let client = StubModelClient::ok(ok_response("null"));
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidModelOutput { .. }),
            "expected InvalidModelOutput for JSON null, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_rejects_json_boolean() {
        let client = StubModelClient::ok(ok_response("true"));
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidModelOutput { .. }),
            "expected InvalidModelOutput for JSON boolean, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_rejects_markdown_fenced_json() {
        let client = StubModelClient::ok(ok_response("```json\n{\"a\":1}\n```"));
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidModelOutput { .. }),
            "expected InvalidModelOutput for markdown fenced JSON, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_rejects_none_finish_reason_with_invalid_json() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: "not json".into(),
            finish_reason: None,
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        });
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        assert!(
            matches!(err, LlmStructuredGenerationError::InvalidModelOutput { .. }),
            "expected InvalidModelOutput, got: {err}"
        );
    }

    // ---------------------------------------------------------------------------
    // Output and token usage
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn generate_preserves_all_json_fields() {
        let json = r#"{"diagnosis":"oom","score":0.95,"nested":{"key":"val"}}"#;
        let client = StubModelClient::ok(ok_response(json));
        let inst = make_instance(client);
        let out = inst.generate(&prompt_ctx("hello")).await.unwrap();
        let expected: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(out.response_json, expected);
    }

    #[tokio::test]
    async fn generate_does_not_drop_unknown_json_fields() {
        let json = r#"{"known_field":"x","unknown_future_field":123,"nested_unknown":{"a":1}}"#;
        let client = StubModelClient::ok(ok_response(json));
        let inst = make_instance(client);
        let out = inst.generate(&prompt_ctx("hello")).await.unwrap();
        assert!(out.response_json.get("unknown_future_field").is_some());
        assert!(out.response_json.get("nested_unknown").is_some());
    }

    #[tokio::test]
    async fn generate_preserves_token_usage_in_output() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: "{}".into(),
            finish_reason: Some(ModelFinishReason::Stop),
            prompt_tokens: Some(100),
            completion_tokens: Some(50),
            total_tokens: Some(150),
        });
        let inst = make_instance(client);
        let out = inst.generate(&prompt_ctx("hello")).await.unwrap();
        assert_eq!(out.token_usage.prompt_tokens, Some(100));
        assert_eq!(out.token_usage.completion_tokens, Some(50));
        assert_eq!(out.token_usage.total_tokens, Some(150));
    }

    #[tokio::test]
    async fn generate_preserves_none_token_usage_when_model_omits_it() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: "{}".into(),
            finish_reason: Some(ModelFinishReason::Stop),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        });
        let inst = make_instance(client);
        let out = inst.generate(&prompt_ctx("hello")).await.unwrap();
        assert_eq!(out.token_usage.prompt_tokens, None);
        assert_eq!(out.token_usage.completion_tokens, None);
        assert_eq!(out.token_usage.total_tokens, None);
    }

    #[tokio::test]
    async fn generate_repairs_json_object_with_missing_required_fields() {
        let invalid_but_parseable = r#"{
            "problem_understanding":"Two concurrent transactions lose one append.",
            "similar_practical_context":"Looks like a repeatable-read lost update.",
            "active_hypotheses":["H1","H2"]
        }"#;
        let repaired = r#"{
            "problem_understanding":"Two concurrent transactions lose one append.",
            "similar_practical_context":"Looks like a repeatable-read lost update.",
            "active_hypotheses":[
                {"hypothesis":"Repeatable Read permits this anomaly.","source":"primary_incident","confidence":"medium"},
                {"hypothesis":"The app may assume stronger protection than MySQL provides.","source":"theory_mechanism","confidence":"low"}
            ],
            "first_check":"Inspect whether both transactions read the missing key before either insert became visible.",
            "result_interpretation":{
                "supports_primary_if":"Both transactions read the key as absent before writing.",
                "supports_competing_if":"The reads were serialized yet one append still disappeared.",
                "inconclusive_if":null
            },
            "alternative_context_assessment":{"used_as_hypothesis":false,"reason":"Alternative context not available."}
        }"#;

        let client = SeqStubModelClient::ok(vec![ok_response(invalid_but_parseable), ok_response(repaired)]);
        let inst = make_instance(client.clone());
        let out = inst.generate(&prompt_ctx("hello")).await.unwrap();

        assert_eq!(
            out.response_json.get("first_check").and_then(|v| v.as_str()),
            Some("Inspect whether both transactions read the missing key before either insert became visible.")
        );
        assert_eq!(out.token_usage.prompt_tokens, Some(20));
        assert_eq!(out.token_usage.completion_tokens, Some(10));
        assert_eq!(out.token_usage.total_tokens, Some(30));

        let requests = client.captured_requests();
        assert_eq!(requests.len(), 2, "expected initial request plus repair request");
        assert!(
            requests[1].messages[0]
                .content
                .contains("Previous invalid JSON"),
            "repair prompt must include the invalid JSON payload"
        );
        assert!(
            requests[1].messages[0]
                .content
                .contains("Repair the JSON shape only."),
            "repair prompt must explicitly instruct shape repair"
        );
        assert_eq!(
            requests[1].max_output_tokens,
            Some(512),
            "repair request should use the capped output-token budget"
        );
    }

    #[tokio::test]
    async fn generate_strips_unknown_top_level_field_without_repair() {
        let with_extra_field = r#"{
            "problem_understanding":"Reader misses writes visible on writer.",
            "similar_practical_context":"Primary and reader disagree on visibility.",
            "active_hypotheses":[
                {"hypothesis":"Reader endpoint has weaker visibility semantics.","source":"primary_incident","confidence":"medium"},
                {"hypothesis":"Replica ordering differs from writer ordering.","source":"alternative_context","confidence":"low"}
            ],
            "first_check":"Compare the same read on writer and reader.",
            "result_interpretation":{
                "supports_primary_if":"Reader still misses the write while writer sees it.",
                "supports_competing_if":"Reader sees the write, suggesting a narrower divergence.",
                "inconclusive_if":null
            },
            "alternative_context_assessment":{"used_as_hypothesis":true,"reason":"Alternative shows a different ordering anomaly."},
            "extra_field":"should be stripped"
        }"#;

        let client = StubModelClient::ok(ok_response(with_extra_field));
        let inst = make_instance(Arc::clone(&client) as Arc<dyn ModelClient>);
        let out = inst.generate(&prompt_ctx("hello")).await.unwrap();

        assert_eq!(
            out.response_json
                .get("problem_understanding")
                .and_then(|v| v.as_str()),
            Some("Reader misses writes visible on writer.")
        );
        assert!(
            out.response_json.get("extra_field").is_none(),
            "extra field must be stripped"
        );
        assert_eq!(
            client.last_request().unwrap().messages.len(),
            1,
            "strip must resolve shape without a repair call"
        );
    }

    #[tokio::test]
    async fn generate_strips_unknown_result_interpretation_field_without_repair() {
        let with_extra_nested = r#"{
            "problem_understanding":"Two clients hold the same lock.",
            "similar_practical_context":"etcd lease expiry allows two holders.",
            "active_hypotheses":[
                {"hypothesis":"Lease expiry overlap.","source":"primary_incident","confidence":"medium"},
                {"hypothesis":"Keepalive missed.","source":"primary_incident","confidence":"low"}
            ],
            "first_check":"Correlate conflicts with lease renewal timing.",
            "result_interpretation":{
                "supports_primary_if":"Conflicts cluster around lease expiry.",
                "supports_competing_if":"Conflicts occur independently of lease timing.",
                "inconclusive_if":null,
                "extra_nested_key":"should be stripped"
            },
            "alternative_context_assessment":{"used_as_hypothesis":false,"reason":"No credible alternative mechanism from alternative context."}
        }"#;

        let client = StubModelClient::ok(ok_response(with_extra_nested));
        let inst = make_instance(Arc::clone(&client) as Arc<dyn ModelClient>);
        let out = inst.generate(&prompt_ctx("hello")).await.unwrap();

        let ri = out.response_json
            .get("result_interpretation")
            .and_then(|v| v.as_object())
            .expect("result_interpretation must be present");
        assert!(
            ri.get("extra_nested_key").is_none(),
            "extra nested field must be stripped"
        );
        assert_eq!(
            client.last_request().unwrap().messages.len(),
            1,
            "strip must resolve shape without a repair call"
        );
    }

    #[tokio::test]
    async fn generate_unwraps_single_key_wrapper_without_repair() {
        let wrapped = r#"{
            "response": {
                "problem_understanding":"Two sessions lose one append.",
                "similar_practical_context":"Lost update under default RavenDB sessions.",
                "first_check":"Open two sessions, both load the same document, append distinct values, commit both, read and verify.",
                "result_interpretation":{
                    "supports_primary_if":"Only one appended value present after both commits.",
                    "supports_competing_if":"Both values present but in wrong order.",
                    "inconclusive_if":null
                },
                "alternative_context_assessment":{"used_as_hypothesis":false,"reason":"Alternative context does not suggest a different mechanism."},
                "active_hypotheses":[
                    {"hypothesis":"Default sessions allow last-writer-wins.","source":"primary_incident","confidence":"medium"},
                    {"hypothesis":"No optimistic concurrency means no conflict detection.","source":"theory_mechanism","confidence":"medium"}
                ]
            }
        }"#;

        let client = StubModelClient::ok(ok_response(wrapped));
        let inst = make_instance(Arc::clone(&client) as Arc<dyn ModelClient>);
        let out = inst.generate(&prompt_ctx("hello")).await.unwrap();

        assert_eq!(
            out.response_json.get("problem_understanding").and_then(|v| v.as_str()),
            Some("Two sessions lose one append.")
        );
        assert!(
            out.response_json.get("response").is_none(),
            "wrapper key must be removed"
        );
        assert_eq!(
            client.last_request().unwrap().messages.len(),
            1,
            "unwrap must resolve shape without a repair call"
        );
    }

    #[tokio::test]
    async fn generate_renames_misnamed_field_without_repair() {
        let with_wrong_key = r#"{
            "final{": "Two sessions both commit appends but one value disappears.",
            "similar_practical_context":"Default RavenDB sessions allow lost updates without optimistic concurrency.",
            "first_check":"Open two sessions, load the same document, each append a distinct value, save both, then read and verify both values are present.",
            "result_interpretation":{
                "supports_primary_if":"Only one appended value present after both commits.",
                "supports_competing_if":"Both values present but in unexpected order.",
                "inconclusive_if":null
            },
            "alternative_context_assessment":{"used_as_hypothesis":false,"reason":"Alternative context describes a similar symptom but via a different mechanism."},
            "active_hypotheses":[
                {"hypothesis":"Default sessions use last-writer-wins semantics.","source":"primary_incident","confidence":"medium"},
                {"hypothesis":"No conflict detection without optimistic concurrency.","source":"theory_mechanism","confidence":"medium"}
            ]
        }"#;

        let client = StubModelClient::ok(ok_response(with_wrong_key));
        let inst = make_instance(Arc::clone(&client) as Arc<dyn ModelClient>);
        let out = inst.generate(&prompt_ctx("hello")).await.unwrap();

        assert_eq!(
            out.response_json.get("problem_understanding").and_then(|v| v.as_str()),
            Some("Two sessions both commit appends but one value disappears.")
        );
        assert!(
            out.response_json.get("final{").is_none(),
            "misnamed key must be removed"
        );
        assert_eq!(
            client.last_request().unwrap().messages.len(),
            1,
            "rename must resolve shape without a repair call"
        );
    }

    #[tokio::test]
    async fn generate_preserves_token_usage_in_invalid_finish_reason_error() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: "{}".into(),
            finish_reason: Some(ModelFinishReason::Length),
            prompt_tokens: Some(200),
            completion_tokens: Some(512),
            total_tokens: Some(712),
        });
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        match err {
            LlmStructuredGenerationError::InvalidModelOutput { token_usage, .. } => {
                assert_eq!(token_usage.prompt_tokens, Some(200));
                assert_eq!(token_usage.completion_tokens, Some(512));
                assert_eq!(token_usage.total_tokens, Some(712));
            }
            other => panic!("expected InvalidModelOutput, got: {other}"),
        }
    }

    #[tokio::test]
    async fn generate_preserves_token_usage_in_json_parse_error() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: "not json".into(),
            finish_reason: Some(ModelFinishReason::Stop),
            prompt_tokens: Some(30),
            completion_tokens: Some(10),
            total_tokens: Some(40),
        });
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        match err {
            LlmStructuredGenerationError::InvalidModelOutput { token_usage, .. } => {
                assert_eq!(token_usage.prompt_tokens, Some(30));
                assert_eq!(token_usage.completion_tokens, Some(10));
                assert_eq!(token_usage.total_tokens, Some(40));
            }
            other => panic!("expected InvalidModelOutput, got: {other}"),
        }
    }

    #[tokio::test]
    async fn generate_preserves_finish_reason_in_error() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: "{}".into(),
            finish_reason: Some(ModelFinishReason::ContentFilter),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        });
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        match err {
            LlmStructuredGenerationError::InvalidModelOutput { finish_reason, .. } => {
                assert_eq!(finish_reason, Some(ModelFinishReason::ContentFilter));
            }
            other => panic!("expected InvalidModelOutput, got: {other}"),
        }
    }

    #[tokio::test]
    async fn generate_finish_reason_in_json_shape_error_reflects_stop() {
        let client = StubModelClient::ok(ModelGenerationResponse {
            content: "[1,2,3]".into(),
            finish_reason: Some(ModelFinishReason::Stop),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        });
        let inst = make_instance(client);
        let err = inst.generate(&prompt_ctx("hello")).await.unwrap_err();
        match err {
            LlmStructuredGenerationError::InvalidModelOutput { finish_reason, .. } => {
                assert_eq!(finish_reason, Some(ModelFinishReason::Stop));
            }
            other => panic!("expected InvalidModelOutput, got: {other}"),
        }
    }

    // ---------------------------------------------------------------------------
    // Model call shape
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn generate_sends_exactly_one_user_message() {
        let client = StubModelClient::ok(ok_response("{}"));
        let inst = make_instance(Arc::clone(&client) as Arc<dyn ModelClient>);
        inst.generate(&prompt_ctx("my prompt text")).await.unwrap();
        let req = client.last_request().unwrap();
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, ModelMessageRole::User);
    }

    #[tokio::test]
    async fn generate_uses_prompt_as_message_content() {
        let client = StubModelClient::ok(ok_response("{}"));
        let inst = make_instance(Arc::clone(&client) as Arc<dyn ModelClient>);
        inst.generate(&prompt_ctx("the assembled prompt"))
            .await
            .unwrap();
        let req = client.last_request().unwrap();
        assert_eq!(req.messages[0].content, "the assembled prompt");
    }

    #[tokio::test]
    async fn generate_uses_json_schema_response_mode() {
        let client = StubModelClient::ok(ok_response("{}"));
        let inst = make_instance(Arc::clone(&client) as Arc<dyn ModelClient>);
        inst.generate(&prompt_ctx("hello")).await.unwrap();
        let req = client.last_request().unwrap();
        assert!(
            matches!(req.response_mode, ModelResponseMode::JsonSchema(_)),
            "expected JsonSchema mode, got: {:?}",
            req.response_mode
        );
    }

    #[tokio::test]
    async fn generate_uses_zero_temperature() {
        let client = StubModelClient::ok(ok_response("{}"));
        let inst = make_instance(Arc::clone(&client) as Arc<dyn ModelClient>);
        inst.generate(&prompt_ctx("hello")).await.unwrap();
        let req = client.last_request().unwrap();
        assert_eq!(req.temperature, 0.0);
    }

    #[tokio::test]
    async fn generate_uses_configured_max_output_tokens() {
        let client = StubModelClient::ok(ok_response("{}"));
        let custom_inst = LlmStructuredGeneration::new(
            settings(2048),
            Arc::clone(&client) as Arc<dyn ModelClient>,
        )
        .unwrap();
        custom_inst.generate(&prompt_ctx("hello")).await.unwrap();
        let req = client.last_request().unwrap();
        assert_eq!(req.max_output_tokens, Some(2048));
    }
}
