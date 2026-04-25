use std::sync::Arc;

use crate::api_clients::model::{
    ModelClient, ModelClientError, ModelFinishReason, ModelGenerationRequest, ModelMessage,
    ModelMessageRole, ModelResponseMode,
};
use crate::config::LlmStructuredGenerationSettings;
use crate::shared_types::{
    LlmStructuredGenerationOutput, ModelTokenUsage, PromptContextAssemblyOutput,
};
use tracing::{info_span, field, Instrument};

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
        let prompt = &prompt_context.prompt;
        let prompt_chars = prompt.len();
        let prompt_empty = prompt.trim().is_empty();

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
            llm.output.parsed_json = field::Empty,
            module.outcome = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        self.generate_instrumented(prompt_context).instrument(span).await
    }

    async fn generate_instrumented(
        &self,
        prompt_context: &PromptContextAssemblyOutput,
    ) -> Result<LlmStructuredGenerationOutput, LlmStructuredGenerationError> {
        if prompt_context.prompt.trim().is_empty() {
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
            response_mode: ModelResponseMode::JsonObject,
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

        let response = {
            async {
                match self.model_client.generate(&request).await {
                    Ok(r) => {
                        tracing::Span::current().record("model.response_mode", "JsonObject");
                        tracing::Span::current().record("model.temperature", 0.0);
                        tracing::Span::current().record("model.max_output_tokens", self.max_output_tokens as i64);
                        if let Some(ref fr) = r.finish_reason {
                            tracing::Span::current().record("model.finish_reason", format!("{:?}", fr));
                        }
                        if let Some(pt) = r.prompt_tokens {
                            tracing::Span::current().record("model.prompt_tokens", pt as i64);
                        }
                        if let Some(ct) = r.completion_tokens {
                            tracing::Span::current().record("model.completion_tokens", ct as i64);
                        }
                        if let Some(tt) = r.total_tokens {
                            tracing::Span::current().record("model.total_tokens", tt as i64);
                        }
                        tracing::Span::current().record("status", "ok");
                        Ok(r)
                    }
                    Err(e) => {
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
        };

        let response = match response {
            Ok(r) => r,
            Err(e) => {
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

        tracing::Span::current().record("llm.response_mode", "JsonObject");
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
        let parsed: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
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
                token_usage,
                finish_reason,
            });
        }

        let parse_success = true;
        let top_level_type = "object".to_string();
        let object_field_count = parsed.as_object().map(|obj| obj.len()).unwrap_or(0);
        let parsed_json = serde_json::to_string(&parsed).unwrap_or_else(|_| "{}".to_string());

        tracing::Span::current().record("llm.output.parse_success", parse_success);
        tracing::Span::current().record("llm.output.top_level_type", &top_level_type);
        tracing::Span::current().record("llm.output.object_field_count", object_field_count as i64);
        tracing::Span::current().record("llm.output.has_markdown_fence", has_markdown_fence);
        tracing::Span::current().record("llm.output.content_chars", content_chars as i64);
        tracing::Span::current().record("llm.output.parsed_json", &parsed_json);
        tracing::Span::current().record("module.outcome", "success");
        tracing::Span::current().record("status", "ok");

        Ok(LlmStructuredGenerationOutput {
            response_json: parsed,
            token_usage,
        })
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
    async fn generate_uses_json_object_response_mode() {
        let client = StubModelClient::ok(ok_response("{}"));
        let inst = make_instance(Arc::clone(&client) as Arc<dyn ModelClient>);
        inst.generate(&prompt_ctx("hello")).await.unwrap();
        let req = client.last_request().unwrap();
        assert_eq!(req.response_mode, ModelResponseMode::JsonObject);
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
