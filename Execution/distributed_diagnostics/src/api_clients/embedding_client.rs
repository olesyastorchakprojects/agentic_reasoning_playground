use backon::Retryable;
use serde::{Deserialize, Serialize};

use crate::utils::retry::build_backoff;

use super::qdrant::shared_types::{
    Embedding, EmbeddingConfig, NormalizedUserQuery, RetryPolicyConfig,
};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum EmbeddingClientError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("transport failure: {0}")]
    Transport(String),
    #[error("unexpected HTTP status: {0}")]
    UnexpectedStatus(u16),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

pub struct EmbeddingClient {
    http_client: reqwest::Client,
    config: EmbeddingConfig,
    retry_policy: RetryPolicyConfig,
}

impl EmbeddingClient {
    pub fn new(
        config: EmbeddingConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, EmbeddingClientError> {
        validate_config(&config, &retry_policy)?;

        let http_client = reqwest::Client::builder()
            .build()
            .map_err(|e| EmbeddingClientError::Transport(e.to_string()))?;

        Ok(Self {
            http_client,
            config,
            retry_policy,
        })
    }

    pub async fn embed(
        &self,
        query: &NormalizedUserQuery,
    ) -> Result<Embedding, EmbeddingClientError> {
        let url = self
            .config
            .base_url
            .join("api/embed")
            .map_err(|e| EmbeddingClientError::Transport(e.to_string()))?;

        let wire_req = WireRequest {
            model: &self.config.model_name,
            input: vec![query.0.as_str()],
        };

        let http_client = self.http_client.clone();
        let url_clone = url.clone();
        let expected_dim = self.config.embedding_dimension;

        let raw = {
            let body = serde_json::to_value(&wire_req)
                .map_err(|e| EmbeddingClientError::Transport(e.to_string()))?;

            (|| {
                let client = http_client.clone();
                let u = url_clone.clone();
                let b = body.clone();
                async move {
                    let resp = client
                        .post(u)
                        .json(&b)
                        .send()
                        .await
                        .map_err(|e| EmbeddingClientError::Transport(e.to_string()))?;

                    let status = resp.status().as_u16();
                    if !resp.status().is_success() {
                        return Err(EmbeddingClientError::UnexpectedStatus(status));
                    }

                    resp.bytes()
                        .await
                        .map_err(|e| EmbeddingClientError::Transport(e.to_string()))
                }
            })
            .retry(build_backoff(&self.retry_policy))
            .when(is_retryable)
            .await?
        };

        let wire_resp: WireResponse = serde_json::from_slice(&raw).map_err(|_| {
            EmbeddingClientError::InvalidResponse("failed to parse embedding response".to_string())
        })?;

        validate_and_extract(wire_resp, expected_dim)
    }
}

fn validate_config(
    config: &EmbeddingConfig,
    policy: &RetryPolicyConfig,
) -> Result<(), EmbeddingClientError> {
    let scheme = config.base_url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(EmbeddingClientError::InvalidRequest(
            "base_url must use http or https".to_string(),
        ));
    }
    if config.base_url.host().is_none() {
        return Err(EmbeddingClientError::InvalidRequest(
            "base_url must contain a host".to_string(),
        ));
    }
    if config.base_url.query().is_some() {
        return Err(EmbeddingClientError::InvalidRequest(
            "base_url must not contain query parameters".to_string(),
        ));
    }
    if config.base_url.fragment().is_some() {
        return Err(EmbeddingClientError::InvalidRequest(
            "base_url must not contain a fragment".to_string(),
        ));
    }
    if config.embedding_dimension == 0 {
        return Err(EmbeddingClientError::InvalidRequest(
            "embedding_dimension must be > 0".to_string(),
        ));
    }
    if policy.max_attempts == 0 {
        return Err(EmbeddingClientError::InvalidRequest(
            "retry_policy.max_attempts must be > 0".to_string(),
        ));
    }
    Ok(())
}

fn validate_and_extract(
    resp: WireResponse,
    expected_dim: usize,
) -> Result<Embedding, EmbeddingClientError> {
    if resp.embeddings.len() == 0 {
        return Err(EmbeddingClientError::InvalidResponse(
            "embeddings array is empty".to_string(),
        ));
    }
    if resp.embeddings.len() > 1 {
        return Err(EmbeddingClientError::InvalidResponse(
            "expected exactly one embedding".to_string(),
        ));
    }

    let values = resp.embeddings.into_iter().next().unwrap();

    if values.len() != expected_dim {
        return Err(EmbeddingClientError::InvalidResponse(
            "embedding dimension does not match configured dimension".to_string(),
        ));
    }

    for v in &values {
        if !v.is_finite() {
            return Err(EmbeddingClientError::InvalidResponse(
                "embedding contains invalid float values".to_string(),
            ));
        }
    }

    Ok(Embedding { values })
}

fn is_retryable(err: &EmbeddingClientError) -> bool {
    matches!(
        err,
        EmbeddingClientError::Transport(_) | EmbeddingClientError::UnexpectedStatus(500..=599)
    )
}

// ─── Wire types ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct WireRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Deserialize)]
struct WireResponse {
    embeddings: Vec<Vec<f32>>,
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{MockHttpServer, MockResponse};
    use crate::utils::retry::RetryBackoffKind;

    fn config(base_url: &str) -> EmbeddingConfig {
        EmbeddingConfig {
            base_url: url::Url::parse(base_url).unwrap(),
            model_name: "embed-model".into(),
            embedding_dimension: 3,
        }
    }

    fn policy() -> RetryPolicyConfig {
        RetryPolicyConfig {
            max_attempts: 1,
            backoff: RetryBackoffKind::Exponential,
        }
    }

    fn query(s: &str) -> NormalizedUserQuery {
        NormalizedUserQuery(s.to_string())
    }

    #[test]
    fn constructor_rejects_zero_dimension() {
        let mut cfg = config("http://localhost/");
        cfg.embedding_dimension = 0;
        assert!(EmbeddingClient::new(cfg, policy()).is_err());
    }

    #[test]
    fn constructor_rejects_zero_max_attempts() {
        let p = RetryPolicyConfig {
            max_attempts: 0,
            backoff: RetryBackoffKind::Exponential,
        };
        assert!(EmbeddingClient::new(config("http://localhost/"), p).is_err());
    }

    #[test]
    fn constructor_rejects_invalid_base_url_shape() {
        assert!(EmbeddingClient::new(config("ftp://localhost/"), policy()).is_err());
    }

    #[tokio::test]
    async fn valid_response_returns_embedding() {
        let resp_body = serde_json::json!({"embeddings": [[0.1, 0.2, 0.3]]}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = EmbeddingClient::new(config(&server.base_url()), policy()).unwrap();
        let emb = client.embed(&query("test")).await.unwrap();
        assert_eq!(emb.values.len(), 3);
    }

    #[tokio::test]
    async fn request_body_contains_model_and_unchanged_query() {
        let resp_body = serde_json::json!({"embeddings": [[0.1, 0.2, 0.3]]}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = EmbeddingClient::new(config(&server.base_url()), policy()).unwrap();
        client.embed(&query("my query text")).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["model"], "embed-model");
        assert_eq!(body["input"].as_array().unwrap().len(), 1);
        assert_eq!(body["input"][0], "my query text");
    }

    #[tokio::test]
    async fn non_2xx_returns_unexpected_status() {
        let server = MockHttpServer::new(vec![MockResponse::status(503, b"err".to_vec())]).await;
        let client = EmbeddingClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.embed(&query("x")).await.unwrap_err(),
            EmbeddingClientError::UnexpectedStatus(503)
        ));
    }

    #[tokio::test]
    async fn missing_embeddings_field_returns_invalid_response() {
        let resp_body = serde_json::json!({"other": "field"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = EmbeddingClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.embed(&query("x")).await.unwrap_err(),
            EmbeddingClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn zero_embeddings_returns_invalid_response() {
        let resp_body = serde_json::json!({"embeddings": []}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = EmbeddingClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.embed(&query("x")).await.unwrap_err(),
            EmbeddingClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn more_than_one_embedding_returns_invalid_response() {
        let resp_body =
            serde_json::json!({"embeddings": [[0.1, 0.2, 0.3], [0.4, 0.5, 0.6]]}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = EmbeddingClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.embed(&query("x")).await.unwrap_err(),
            EmbeddingClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn wrong_dimension_returns_invalid_response() {
        let resp_body = serde_json::json!({"embeddings": [[0.1, 0.2]]}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = EmbeddingClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.embed(&query("x")).await.unwrap_err(),
            EmbeddingClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn invalid_float_in_embedding_returns_invalid_response() {
        let resp_body = serde_json::json!({"embeddings": [[f32::INFINITY, 0.2, 0.3]]}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp_body)]).await;
        let client = EmbeddingClient::new(config(&server.base_url()), policy()).unwrap();
        assert!(matches!(
            client.embed(&query("x")).await.unwrap_err(),
            EmbeddingClientError::InvalidResponse(_)
        ));
    }
}
