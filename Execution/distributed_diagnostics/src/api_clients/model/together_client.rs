use std::time::Duration;

use async_trait::async_trait;
use backon::Retryable;
use serde::{Deserialize, Serialize};

use crate::config::TogetherModelSettings;
use crate::utils::retry::build_backoff;

use super::model_client::{validate_request, ModelClient, ModelClientError};
use super::shared_types::{
    ModelFinishReason, ModelGenerationRequest, ModelGenerationResponse, ModelMessageRole,
    ModelResponseMode, RetryPolicyConfig, TogetherModelClientConfig,
};

pub struct TogetherModelClient {
    http_client: reqwest::Client,
    config: TogetherModelClientConfig,
    retry_policy: RetryPolicyConfig,
}

impl TogetherModelClient {
    pub fn new(
        config: TogetherModelClientConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, ModelClientError> {
        validate_client_config(&config, &retry_policy)?;

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_sec))
            .build()
            .map_err(|e| ModelClientError::Transport(e.to_string()))?;

        Ok(Self {
            http_client,
            config,
            retry_policy,
        })
    }

    pub fn from_settings(settings: &TogetherModelSettings) -> Result<Self, ModelClientError> {
        let config = TogetherModelClientConfig {
            base_url: url::Url::parse(&settings.url).map_err(|_| {
                ModelClientError::InvalidRequest("base_url must be a valid URL".to_string())
            })?,
            api_key: settings.api_key.clone(),
            model_name: settings.model_name.clone(),
            timeout_sec: settings.timeout_sec,
        };

        Self::new(config, settings.retry.clone())
    }
}

fn validate_client_config(
    config: &TogetherModelClientConfig,
    policy: &RetryPolicyConfig,
) -> Result<(), ModelClientError> {
    let scheme = config.base_url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(ModelClientError::InvalidRequest(
            "base_url must use http or https".to_string(),
        ));
    }
    if config.base_url.host().is_none() {
        return Err(ModelClientError::InvalidRequest(
            "base_url must contain a host".to_string(),
        ));
    }
    if config.base_url.query().is_some() {
        return Err(ModelClientError::InvalidRequest(
            "base_url must not contain query parameters".to_string(),
        ));
    }
    if config.base_url.fragment().is_some() {
        return Err(ModelClientError::InvalidRequest(
            "base_url must not contain a fragment".to_string(),
        ));
    }
    if config.api_key.trim().is_empty() {
        return Err(ModelClientError::InvalidRequest(
            "api_key must not be empty".to_string(),
        ));
    }
    if config.model_name.trim().is_empty() {
        return Err(ModelClientError::InvalidRequest(
            "model_name must not be empty".to_string(),
        ));
    }
    if config.timeout_sec == 0 {
        return Err(ModelClientError::InvalidRequest(
            "timeout_sec must be > 0".to_string(),
        ));
    }
    if policy.max_attempts == 0 {
        return Err(ModelClientError::InvalidRequest(
            "retry_policy.max_attempts must be > 0".to_string(),
        ));
    }
    Ok(())
}

#[derive(Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<WireResponseFormat>,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct WireResponseFormat {
    r#type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    json_schema: Option<WireJsonSchema>,
}

#[derive(Serialize)]
struct WireJsonSchema {
    name: &'static str,
    schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct WireResponse {
    choices: Vec<WireChoice>,
    usage: Option<WireUsage>,
}

#[derive(Debug, Deserialize)]
struct WireChoice {
    message: WireAssistantMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireAssistantMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireUsage {
    prompt_tokens: Option<usize>,
    completion_tokens: Option<usize>,
    total_tokens: Option<usize>,
}

#[async_trait]
impl ModelClient for TogetherModelClient {
    async fn generate(
        &self,
        request: &ModelGenerationRequest,
    ) -> Result<ModelGenerationResponse, ModelClientError> {
        validate_request(request)?;

        let url = self
            .config
            .base_url
            .join("v1/chat/completions")
            .map_err(|e| ModelClientError::Transport(e.to_string()))?;

        let wire_messages: Vec<WireMessage> = request
            .messages
            .iter()
            .map(|m| WireMessage {
                role: map_role(&m.role),
                content: &m.content,
            })
            .collect();

        let response_format = match &request.response_mode {
            ModelResponseMode::Text => None,
            ModelResponseMode::JsonObject => Some(WireResponseFormat {
                r#type: "json_object",
                json_schema: None,
            }),
            ModelResponseMode::JsonSchema(schema) => Some(WireResponseFormat {
                r#type: "json_schema",
                json_schema: Some(WireJsonSchema {
                    name: "judge_response",
                    schema: schema.clone(),
                }),
            }),
        };

        let wire_req = WireRequest {
            model: &self.config.model_name,
            messages: wire_messages,
            temperature: request.temperature,
            max_tokens: request.max_output_tokens,
            response_format,
        };

        let api_key = self.config.api_key.clone();
        let http_client = self.http_client.clone();
        let url_clone = url.clone();

        let raw_response = {
            let body = serde_json::to_value(&wire_req)
                .map_err(|e| ModelClientError::Transport(e.to_string()))?;

            (|| {
                let client = http_client.clone();
                let u = url_clone.clone();
                let b = body.clone();
                let key = api_key.clone();
                async move {
                    let resp = client
                        .post(u)
                        .header("Authorization", format!("Bearer {}", key))
                        .header("Content-Type", "application/json")
                        .json(&b)
                        .send()
                        .await
                        .map_err(|e| ModelClientError::Transport(e.to_string()))?;

                    let status = resp.status().as_u16();
                    if !resp.status().is_success() {
                        return Err(ModelClientError::UnexpectedStatus(status));
                    }

                    let bytes = resp
                        .bytes()
                        .await
                        .map_err(|e| ModelClientError::Transport(e.to_string()))?;

                    Ok::<_, ModelClientError>(bytes)
                }
            })
            .retry(build_backoff(&self.retry_policy))
            .when(is_retryable)
            .await?
        };

        let wire_resp: WireResponse = serde_json::from_slice(&raw_response).map_err(|_| {
            let raw_str = String::from_utf8_lossy(&raw_response).to_string();
            tracing::info!("failed to parse response JSON; raw_response: {}", raw_str);
            ModelClientError::InvalidResponse("failed to parse response JSON".to_string())
        })?;

        map_response(wire_resp, &raw_response, request.max_output_tokens)
    }
}

fn map_role(role: &ModelMessageRole) -> &'static str {
    match role {
        ModelMessageRole::System => "system",
        ModelMessageRole::User => "user",
        ModelMessageRole::Assistant => "assistant",
    }
}

fn map_finish_reason(s: &str) -> ModelFinishReason {
    match s {
        "stop" => ModelFinishReason::Stop,
        "length" => ModelFinishReason::Length,
        "content_filter" => ModelFinishReason::ContentFilter,
        "tool_calls" => ModelFinishReason::ToolCalls,
        other => ModelFinishReason::Unknown(other.to_string()),
    }
}

fn map_response(wire: WireResponse, raw_response: &[u8], max_output_tokens: Option<u32>) -> Result<ModelGenerationResponse, ModelClientError> {
    let raw_str = String::from_utf8_lossy(raw_response).to_string();

    if wire.choices.is_empty() {
        tracing::info!("choices array is empty; raw_response: {}", raw_str);
        return Err(ModelClientError::InvalidResponse(
            "choices array is empty".to_string(),
        ));
    }

    let choice = &wire.choices[0];

    // Check for length truncation first - this takes priority
    if let Some(ref finish) = choice.finish_reason {
        if finish == "length" {
            if let Some(max_tokens) = max_output_tokens {
                tracing::info!("model output was truncated due to length limit; finish_reason='length'; max_output_tokens={}; raw_response: {}", max_tokens, raw_str);
                return Err(ModelClientError::InvalidResponseWithLimit {
                    max_output_tokens: max_tokens,
                });
            }
        }
    }

    let content = choice
        .message
        .content
        .as_deref()
        .ok_or_else(|| {
            tracing::info!("missing message.content; raw_response: {}", raw_str);
            ModelClientError::InvalidResponse("missing message.content".to_string())
        })?;

    if content.trim().is_empty() {
        tracing::info!("model returned empty content; raw_response: {}", raw_str);
        return Err(ModelClientError::InvalidResponse(
            "assistant content is empty (choices[0].message.content is empty or whitespace-only)".to_string(),
        ));
    }

    let finish_reason = choice.finish_reason.as_deref().map(map_finish_reason);

    let (prompt_tokens, completion_tokens, total_tokens) = if let Some(u) = &wire.usage {
        (u.prompt_tokens, u.completion_tokens, u.total_tokens)
    } else {
        (None, None, None)
    };

    Ok(ModelGenerationResponse {
        content: content.to_string(),
        finish_reason,
        prompt_tokens,
        completion_tokens,
        total_tokens,
    })
}

fn is_retryable(err: &ModelClientError) -> bool {
    matches!(
        err,
        ModelClientError::Transport(_)
            | ModelClientError::UnexpectedStatus(500..=599)
            | ModelClientError::InvalidResponse(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TogetherModelSettings;
    use crate::test_utils::{MockHttpServer, MockResponse};
    use crate::utils::retry::RetryBackoffKind;

    fn config(base_url: &str) -> TogetherModelClientConfig {
        TogetherModelClientConfig {
            base_url: url::Url::parse(base_url).unwrap(),
            api_key: "test-key".into(),
            model_name: "gpt-test".into(),
            timeout_sec: 5,
        }
    }

    fn policy() -> RetryPolicyConfig {
        RetryPolicyConfig {
            max_attempts: 1,
            backoff: RetryBackoffKind::Exponential,
        }
    }

    fn settings(url: &str) -> TogetherModelSettings {
        TogetherModelSettings {
            url: url.to_string(),
            api_key: "test-key".into(),
            model_name: "gpt-test".into(),
            timeout_sec: 5,
            retry: policy(),
            input_cost_per_million_tokens: 0.0,
            output_cost_per_million_tokens: 0.0,
        }
    }

    fn simple_request() -> ModelGenerationRequest {
        ModelGenerationRequest {
            messages: vec![super::super::shared_types::ModelMessage {
                role: ModelMessageRole::User,
                content: "hello".into(),
            }],
            temperature: 0.2,
            max_output_tokens: None,
            response_mode: ModelResponseMode::Text,
        }
    }

    #[test]
    fn constructor_rejects_empty_api_key() {
        let mut cfg = config("https://api.openai.com/");
        cfg.api_key = "  ".into();
        assert!(TogetherModelClient::new(cfg, policy()).is_err());
    }

    #[test]
    fn constructor_rejects_empty_model_name() {
        let mut cfg = config("https://api.openai.com/");
        cfg.model_name = "".into();
        assert!(TogetherModelClient::new(cfg, policy()).is_err());
    }

    #[test]
    fn constructor_rejects_zero_timeout() {
        let mut cfg = config("https://api.openai.com/");
        cfg.timeout_sec = 0;
        assert!(TogetherModelClient::new(cfg, policy()).is_err());
    }

    #[test]
    fn constructor_rejects_zero_max_attempts() {
        let cfg = config("https://api.openai.com/");
        let p = RetryPolicyConfig {
            max_attempts: 0,
            backoff: RetryBackoffKind::Exponential,
        };
        assert!(TogetherModelClient::new(cfg, p).is_err());
    }

    #[test]
    fn constructor_rejects_invalid_scheme() {
        assert!(TogetherModelClient::new(config("ftp://example.com/"), policy()).is_err());
    }

    #[test]
    fn from_settings_maps_valid_together_settings() {
        let client =
            TogetherModelClient::from_settings(&settings("https://api.together.xyz/")).unwrap();
        assert_eq!(client.config.api_key, "test-key");
        assert_eq!(client.config.model_name, "gpt-test");
        assert_eq!(client.config.timeout_sec, 5);
        assert_eq!(client.config.base_url.as_str(), "https://api.together.xyz/");
        assert_eq!(client.retry_policy.max_attempts, 1);
    }

    #[test]
    fn from_settings_rejects_invalid_url() {
        let err = TogetherModelClient::from_settings(&settings("not a url"))
            .err()
            .expect("should fail");
        assert!(matches!(err, ModelClientError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn empty_messages_fails_before_http() {
        let server = MockHttpServer::new(vec![]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.messages.clear();
        let err = client.generate(&req).await.unwrap_err();
        assert!(matches!(err, ModelClientError::InvalidRequest(_)));
        assert!(server.take_bodies().await.is_empty());
    }

    #[tokio::test]
    async fn empty_content_fails_before_http() {
        let server = MockHttpServer::new(vec![]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.messages[0].content = "   ".into();
        let err = client.generate(&req).await.unwrap_err();
        assert!(matches!(err, ModelClientError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn negative_temperature_fails_before_http() {
        let server = MockHttpServer::new(vec![]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.temperature = -0.1;
        assert!(matches!(
            client.generate(&req).await.unwrap_err(),
            ModelClientError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn nan_temperature_fails_before_http() {
        let server = MockHttpServer::new(vec![]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.temperature = f32::NAN;
        assert!(matches!(
            client.generate(&req).await.unwrap_err(),
            ModelClientError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn zero_max_output_tokens_fails_before_http() {
        let server = MockHttpServer::new(vec![]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.max_output_tokens = Some(0);
        assert!(matches!(
            client.generate(&req).await.unwrap_err(),
            ModelClientError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn json_schema_non_object_fails_before_http() {
        let server = MockHttpServer::new(vec![]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.response_mode = ModelResponseMode::JsonSchema(serde_json::json!([1, 2, 3]));
        assert!(matches!(
            client.generate(&req).await.unwrap_err(),
            ModelClientError::InvalidRequest(_)
        ));
        assert!(server.take_bodies().await.is_empty());
    }

    #[tokio::test]
    async fn json_schema_mode_sends_correct_response_format() {
        let resp_body = serde_json::json!({
            "choices": [{"message": {"content": "{}"}, "finish_reason": "stop"}]
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        let schema = serde_json::json!({"type": "object", "properties": {"x": {"type": "string"}}});
        req.response_mode = ModelResponseMode::JsonSchema(schema.clone());
        client.generate(&req).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["name"], "judge_response");
        assert_eq!(body["response_format"]["json_schema"]["schema"], schema);
    }

    #[tokio::test]
    async fn outbound_body_contains_model_name() {
        let resp_body = serde_json::json!({
            "choices": [{"message": {"content": "hi"}, "finish_reason": "stop"}]
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        client.generate(&simple_request()).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["model"], "gpt-test");
    }

    #[tokio::test]
    async fn outbound_body_preserves_message_order() {
        let resp_body = serde_json::json!({
            "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}]
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let req = ModelGenerationRequest {
            messages: vec![
                super::super::shared_types::ModelMessage {
                    role: ModelMessageRole::System,
                    content: "sys".into(),
                },
                super::super::shared_types::ModelMessage {
                    role: ModelMessageRole::User,
                    content: "usr".into(),
                },
            ],
            temperature: 0.0,
            max_output_tokens: None,
            response_mode: ModelResponseMode::Text,
        };
        client.generate(&req).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
    }

    #[tokio::test]
    async fn no_max_tokens_when_none() {
        let resp_body = serde_json::json!({
            "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}]
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        client.generate(&simple_request()).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert!(body.get("max_tokens").is_none());
    }

    #[tokio::test]
    async fn text_mode_omits_response_format() {
        let resp_body = serde_json::json!({
            "choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}]
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        client.generate(&simple_request()).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert!(body.get("response_format").is_none());
    }

    #[tokio::test]
    async fn json_mode_sends_response_format() {
        let resp_body = serde_json::json!({
            "choices": [{"message": {"content": "{}"}, "finish_reason": "stop"}]
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.response_mode = ModelResponseMode::JsonObject;
        client.generate(&req).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["response_format"]["type"], "json_object");
    }

    #[tokio::test]
    async fn uses_first_choice_when_multiple() {
        let resp_body = serde_json::json!({
            "choices": [
                {"message": {"content": "first"}, "finish_reason": "stop"},
                {"message": {"content": "second"}, "finish_reason": "stop"}
            ]
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let result = client.generate(&simple_request()).await.unwrap();
        assert_eq!(result.content, "first");
    }

    #[tokio::test]
    async fn maps_token_usage() {
        let resp_body = serde_json::json!({
            "choices": [{"message": {"content": "hi"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let result = client.generate(&simple_request()).await.unwrap();
        assert_eq!(result.prompt_tokens, Some(10));
        assert_eq!(result.completion_tokens, Some(5));
        assert_eq!(result.total_tokens, Some(15));
    }

    #[tokio::test]
    async fn maps_known_finish_reasons() {
        for (raw, expected) in [
            ("stop", ModelFinishReason::Stop),
            ("length", ModelFinishReason::Length),
            ("content_filter", ModelFinishReason::ContentFilter),
            ("tool_calls", ModelFinishReason::ToolCalls),
        ] {
            let resp_body = serde_json::json!({
                "choices": [{"message": {"content": "hi"}, "finish_reason": raw}]
            })
            .to_string();
            let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
            let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
            let result = client.generate(&simple_request()).await.unwrap();
            assert_eq!(result.finish_reason, Some(expected));
        }
    }

    #[tokio::test]
    async fn unknown_finish_reason_maps_to_unknown_variant() {
        let resp_body = serde_json::json!({
            "choices": [{"message": {"content": "hi"}, "finish_reason": "whatever"}]
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let result = client.generate(&simple_request()).await.unwrap();
        assert!(matches!(
            result.finish_reason,
            Some(ModelFinishReason::Unknown(_))
        ));
    }

    #[tokio::test]
    async fn message_role_is_ignored_in_mapping() {
        let resp_body = serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}]
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        let result = client.generate(&simple_request()).await.unwrap();
        assert_eq!(result.content, "hi");
    }

    #[tokio::test]
    async fn non_2xx_returns_unexpected_status() {
        let server = MockHttpServer::new(vec![MockResponse::status(500, b"err".to_vec())]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.generate(&simple_request()).await.unwrap_err(),
            ModelClientError::UnexpectedStatus(500)
        ));
    }

    #[tokio::test]
    async fn missing_choices_returns_invalid_response() {
        let resp_body = serde_json::json!({"choices": []}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.generate(&simple_request()).await.unwrap_err(),
            ModelClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn missing_content_returns_invalid_response() {
        let resp_body = serde_json::json!({
            "choices": [{"message": {}, "finish_reason": "stop"}]
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.generate(&simple_request()).await.unwrap_err(),
            ModelClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn empty_content_returns_invalid_response() {
        let resp_body = serde_json::json!({
            "choices": [{"message": {"content": "   "}, "finish_reason": "stop"}]
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = TogetherModelClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.generate(&simple_request()).await.unwrap_err(),
            ModelClientError::InvalidResponse(_)
        ));
    }
}
