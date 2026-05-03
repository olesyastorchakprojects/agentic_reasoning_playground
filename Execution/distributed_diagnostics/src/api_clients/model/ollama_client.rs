use std::time::Duration;

use async_trait::async_trait;
use backon::Retryable;
use serde::{Deserialize, Serialize};

use crate::config::OllamaModelSettings;
use crate::utils::retry::build_backoff;

use super::model_client::{validate_request, ModelClient, ModelClientError};
use super::shared_types::{
    ModelFinishReason, ModelGenerationRequest, ModelGenerationResponse, ModelMessageRole,
    ModelResponseMode, OllamaModelClientConfig, RetryPolicyConfig,
};

pub struct OllamaModelClient {
    http_client: reqwest::Client,
    config: OllamaModelClientConfig,
    retry_policy: RetryPolicyConfig,
}

impl OllamaModelClient {
    pub fn new(
        config: OllamaModelClientConfig,
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

    pub fn from_settings(settings: &OllamaModelSettings) -> Result<Self, ModelClientError> {
        let config = OllamaModelClientConfig {
            base_url: url::Url::parse(&settings.url).map_err(|_| {
                ModelClientError::InvalidRequest("base_url must be a valid URL".to_string())
            })?,
            model_name: settings.model_name.clone(),
            timeout_sec: settings.timeout_sec,
        };

        Self::new(config, settings.retry.clone())
    }
}

fn validate_client_config(
    config: &OllamaModelClientConfig,
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

// ─── Wire types (private) ────────────────────────────────────────────────────

#[derive(Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<WireOptions>,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Serialize)]
struct WireOptions {
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Deserialize)]
struct WireResponse {
    message: Option<WireAssistantMessage>,
    done_reason: Option<String>,
    prompt_eval_count: Option<usize>,
    eval_count: Option<usize>,
}

#[derive(Deserialize)]
struct WireAssistantMessage {
    content: Option<String>,
}

// ─── ModelClient impl ────────────────────────────────────────────────────────

#[async_trait]
impl ModelClient for OllamaModelClient {
    async fn generate(
        &self,
        request: &ModelGenerationRequest,
    ) -> Result<ModelGenerationResponse, ModelClientError> {
        validate_request(request)?;

        let url = self
            .config
            .base_url
            .join("api/chat")
            .map_err(|e| ModelClientError::Transport(e.to_string()))?;

        let wire_messages: Vec<WireMessage> = request
            .messages
            .iter()
            .map(|m| WireMessage {
                role: map_role(&m.role),
                content: &m.content,
            })
            .collect();

        let format = match &request.response_mode {
            ModelResponseMode::Text => None,
            ModelResponseMode::JsonObject | ModelResponseMode::JsonSchema(_) => Some("json"),
        };

        let wire_req = WireRequest {
            model: &self.config.model_name,
            messages: wire_messages,
            stream: false,
            format,
            options: Some(WireOptions {
                temperature: request.temperature,
                num_predict: request.max_output_tokens,
            }),
        };

        let http_client = self.http_client.clone();
        let url_clone = url.clone();

        let raw_response = {
            let body = serde_json::to_value(&wire_req)
                .map_err(|e| ModelClientError::Transport(e.to_string()))?;

            (|| {
                let client = http_client.clone();
                let u = url_clone.clone();
                let b = body.clone();
                async move {
                    let resp = client
                        .post(u)
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
            ModelClientError::InvalidResponse("failed to parse response JSON".to_string())
        })?;

        map_response(wire_resp)
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
        other => ModelFinishReason::Unknown(other.to_string()),
    }
}

fn map_response(wire: WireResponse) -> Result<ModelGenerationResponse, ModelClientError> {
    let msg = wire.message.ok_or(ModelClientError::InvalidResponse(
        "missing message field".to_string(),
    ))?;

    let content = msg
        .content
        .as_deref()
        .ok_or(ModelClientError::InvalidResponse(
            "missing message.content".to_string(),
        ))?;

    if content.trim().is_empty() {
        return Err(ModelClientError::InvalidResponse(
            "assistant content is empty".to_string(),
        ));
    }

    let finish_reason = wire.done_reason.as_deref().map(map_finish_reason);

    let (prompt_tokens, completion_tokens, total_tokens) =
        match (wire.prompt_eval_count, wire.eval_count) {
            (Some(p), Some(c)) => (Some(p), Some(c), Some(p + c)),
            (Some(p), None) => (Some(p), None, None),
            (None, Some(c)) => (None, Some(c), None),
            (None, None) => (None, None, None),
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
        ModelClientError::Transport(_) | ModelClientError::UnexpectedStatus(500..=599)
    )
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OllamaModelSettings;
    use crate::test_utils::{MockHttpServer, MockResponse};
    use crate::utils::retry::RetryBackoffKind;

    fn config(base_url: &str) -> OllamaModelClientConfig {
        OllamaModelClientConfig {
            base_url: url::Url::parse(base_url).unwrap(),
            model_name: "llama-test".into(),
            timeout_sec: 5,
        }
    }

    fn policy() -> RetryPolicyConfig {
        RetryPolicyConfig {
            max_attempts: 1,
            backoff: RetryBackoffKind::Exponential,
        }
    }

    fn settings(url: &str) -> OllamaModelSettings {
        OllamaModelSettings {
            url: url.to_string(),
            model_name: "llama-test".into(),
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

    // ── Constructor validation ────────────────────────────────────────────────

    #[test]
    fn constructor_rejects_empty_model_name() {
        let mut cfg = config("http://localhost:11434/");
        cfg.model_name = "".into();
        assert!(OllamaModelClient::new(cfg, policy()).is_err());
    }

    #[test]
    fn constructor_rejects_zero_timeout() {
        let mut cfg = config("http://localhost:11434/");
        cfg.timeout_sec = 0;
        assert!(OllamaModelClient::new(cfg, policy()).is_err());
    }

    #[test]
    fn constructor_rejects_zero_max_attempts() {
        let cfg = config("http://localhost:11434/");
        let p = RetryPolicyConfig {
            max_attempts: 0,
            backoff: RetryBackoffKind::Exponential,
        };
        assert!(OllamaModelClient::new(cfg, p).is_err());
    }

    #[test]
    fn constructor_rejects_invalid_scheme() {
        assert!(OllamaModelClient::new(config("ftp://localhost/"), policy()).is_err());
    }

    #[test]
    fn from_settings_maps_valid_ollama_settings() {
        let client =
            OllamaModelClient::from_settings(&settings("http://localhost:11434/")).unwrap();
        assert_eq!(client.config.model_name, "llama-test");
        assert_eq!(client.config.timeout_sec, 5);
        assert_eq!(client.config.base_url.as_str(), "http://localhost:11434/");
        assert_eq!(client.retry_policy.max_attempts, 1);
    }

    #[test]
    fn from_settings_rejects_invalid_url() {
        let err = OllamaModelClient::from_settings(&settings("not a url"))
            .err()
            .expect("should fail");
        assert!(matches!(err, ModelClientError::InvalidRequest(_)));
    }

    // ── Request validation ────────────────────────────────────────────────────

    #[tokio::test]
    async fn empty_messages_fails_before_http() {
        let server = MockHttpServer::new(vec![]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.messages.clear();
        let err = client.generate(&req).await.unwrap_err();
        assert!(matches!(err, ModelClientError::InvalidRequest(_)));
        assert!(server.take_bodies().await.is_empty());
    }

    #[tokio::test]
    async fn empty_content_fails_before_http() {
        let server = MockHttpServer::new(vec![]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.messages[0].content = "".into();
        assert!(matches!(
            client.generate(&req).await.unwrap_err(),
            ModelClientError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn invalid_temperature_fails_before_http() {
        let server = MockHttpServer::new(vec![]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.temperature = f32::INFINITY;
        assert!(matches!(
            client.generate(&req).await.unwrap_err(),
            ModelClientError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn zero_max_output_tokens_fails_before_http() {
        let server = MockHttpServer::new(vec![]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.max_output_tokens = Some(0);
        assert!(matches!(
            client.generate(&req).await.unwrap_err(),
            ModelClientError::InvalidRequest(_)
        ));
    }

    // ── Request body shape ────────────────────────────────────────────────────

    #[tokio::test]
    async fn outbound_body_contains_model_name() {
        let resp_body = serde_json::json!({
            "message": {"content": "hi"},
            "done_reason": "stop"
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        client.generate(&simple_request()).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["model"], "llama-test");
    }

    #[tokio::test]
    async fn always_sends_stream_false() {
        let resp_body =
            serde_json::json!({"message": {"content": "hi"}, "done_reason": "stop"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        client.generate(&simple_request()).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["stream"], false);
    }

    #[tokio::test]
    async fn temperature_encoded_under_options() {
        let resp_body =
            serde_json::json!({"message": {"content": "hi"}, "done_reason": "stop"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.temperature = 0.7;
        client.generate(&req).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert!((body["options"]["temperature"].as_f64().unwrap() - 0.7).abs() < 1e-5);
    }

    #[tokio::test]
    async fn max_output_tokens_encoded_as_num_predict() {
        let resp_body =
            serde_json::json!({"message": {"content": "hi"}, "done_reason": "stop"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.max_output_tokens = Some(300);
        client.generate(&req).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["options"]["num_predict"], 300);
    }

    #[tokio::test]
    async fn text_mode_omits_format() {
        let resp_body =
            serde_json::json!({"message": {"content": "hi"}, "done_reason": "stop"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        client.generate(&simple_request()).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert!(body.get("format").is_none());
    }

    #[tokio::test]
    async fn json_mode_sends_format_json() {
        let resp_body =
            serde_json::json!({"message": {"content": "{}"}, "done_reason": "stop"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        let mut req = simple_request();
        req.response_mode = ModelResponseMode::JsonObject;
        client.generate(&req).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["format"], "json");
    }

    #[tokio::test]
    async fn preserves_message_order() {
        let resp_body =
            serde_json::json!({"message": {"content": "ok"}, "done_reason": "stop"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
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

    // ── Response mapping ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn maps_token_counts() {
        let resp_body = serde_json::json!({
            "message": {"content": "hi"},
            "done_reason": "stop",
            "prompt_eval_count": 10,
            "eval_count": 5
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
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
        ] {
            let resp_body = serde_json::json!({
                "message": {"content": "hi"},
                "done_reason": raw
            })
            .to_string();
            let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
            let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
            let result = client.generate(&simple_request()).await.unwrap();
            assert_eq!(result.finish_reason, Some(expected));
        }
    }

    #[tokio::test]
    async fn unknown_finish_reason_maps_to_unknown_variant() {
        let resp_body =
            serde_json::json!({"message": {"content": "hi"}, "done_reason": "weird"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        let result = client.generate(&simple_request()).await.unwrap();
        assert!(matches!(
            result.finish_reason,
            Some(ModelFinishReason::Unknown(_))
        ));
    }

    #[tokio::test]
    async fn message_role_is_ignored() {
        let resp_body = serde_json::json!({
            "message": {"role": "assistant", "content": "hi"},
            "done_reason": "stop"
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        let result = client.generate(&simple_request()).await.unwrap();
        assert_eq!(result.content, "hi");
    }

    // ── Error variants ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn non_2xx_returns_unexpected_status() {
        let server = MockHttpServer::new(vec![MockResponse::status(500, b"err".to_vec())]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.generate(&simple_request()).await.unwrap_err(),
            ModelClientError::UnexpectedStatus(500)
        ));
    }

    #[tokio::test]
    async fn missing_message_returns_invalid_response() {
        let resp_body = serde_json::json!({"done_reason": "stop"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.generate(&simple_request()).await.unwrap_err(),
            ModelClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn missing_message_content_returns_invalid_response() {
        let resp_body = serde_json::json!({"message": {}, "done_reason": "stop"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.generate(&simple_request()).await.unwrap_err(),
            ModelClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn empty_content_returns_invalid_response() {
        let resp_body =
            serde_json::json!({"message": {"content": " "}, "done_reason": "stop"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = OllamaModelClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.generate(&simple_request()).await.unwrap_err(),
            ModelClientError::InvalidResponse(_)
        ));
    }
}
