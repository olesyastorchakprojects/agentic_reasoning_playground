use backon::Retryable;

use crate::utils::retry::build_backoff;

use super::dense_search_client::{encode_filter, parse_payload, DenseSearchClientError};
use super::shared_types::{
    Embedding, QdrantCollectionName, QdrantFilter, QdrantVectorName, RawQdrantHit,
    RawQdrantPayload, RetryPolicyConfig, SparseVector,
};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum HybridSearchClientError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("transport failure: {0}")]
    Transport(String),
    #[error("unexpected HTTP status: {0}")]
    UnexpectedStatus(u16),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HybridSearchRequest {
    pub collection_name: QdrantCollectionName,
    pub embedding: Embedding,
    pub sparse_vector: SparseVector,
    pub vector_name: QdrantVectorName,
    pub sparse_vector_name: QdrantVectorName,
    pub filter: Option<QdrantFilter>,
    pub limit: usize,
    pub score_threshold: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HybridSearchResponse {
    pub hits: Vec<RawQdrantHit>,
}

pub struct QdrantHybridSearchClient {
    http_client: reqwest::Client,
    qdrant_base_url: url::Url,
    retry_policy: RetryPolicyConfig,
}

impl QdrantHybridSearchClient {
    pub fn new(
        qdrant_base_url: url::Url,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, HybridSearchClientError> {
        validate_config(&qdrant_base_url, &retry_policy)?;
        let http_client = reqwest::Client::new();
        Ok(Self {
            http_client,
            qdrant_base_url,
            retry_policy,
        })
    }

    pub async fn search(
        &self,
        request: &HybridSearchRequest,
    ) -> Result<HybridSearchResponse, HybridSearchClientError> {
        validate_request(request)?;

        let collection = percent_encode_path(&request.collection_name.0);
        let url = self
            .qdrant_base_url
            .join(&format!("collections/{}/points/query", collection))
            .map_err(|e| HybridSearchClientError::Transport(e.to_string()))?;

        let dense_prefetch = serde_json::json!({
            "query": request.embedding.values,
            "using": request.vector_name.0,
            "limit": request.limit,
        });

        let sparse_prefetch = serde_json::json!({
            "query": {
                "indices": request.sparse_vector.indices,
                "values": request.sparse_vector.values,
            },
            "using": request.sparse_vector_name.0,
            "limit": request.limit,
        });

        let mut body = serde_json::json!({
            "prefetch": [dense_prefetch, sparse_prefetch],
            "query": { "fusion": "rrf" },
            "limit": request.limit,
            "score_threshold": request.score_threshold,
            "with_payload": true,
            "with_vector": false,
        });

        if let Some(f) = &request.filter {
            body["filter"] = encode_filter(f);
        }

        let http_client = self.http_client.clone();
        let url_clone = url.clone();

        let raw = (|| {
            let client = http_client.clone();
            let u = url_clone.clone();
            let b = body.clone();
            async move {
                let resp = client
                    .post(u)
                    .json(&b)
                    .send()
                    .await
                    .map_err(|e| HybridSearchClientError::Transport(e.to_string()))?;

                let status = resp.status().as_u16();
                if !resp.status().is_success() {
                    return Err(HybridSearchClientError::UnexpectedStatus(status));
                }

                resp.bytes()
                    .await
                    .map_err(|e| HybridSearchClientError::Transport(e.to_string()))
            }
        })
        .retry(build_backoff(&self.retry_policy))
        .when(is_retryable)
        .await?;

        parse_response(&raw)
    }
}

fn validate_config(
    base_url: &url::Url,
    policy: &RetryPolicyConfig,
) -> Result<(), HybridSearchClientError> {
    let s = base_url.scheme();
    if s != "http" && s != "https" {
        return Err(HybridSearchClientError::InvalidRequest(
            "qdrant_base_url must use http or https".to_string(),
        ));
    }
    if base_url.host().is_none() {
        return Err(HybridSearchClientError::InvalidRequest(
            "qdrant_base_url must contain a host".to_string(),
        ));
    }
    if base_url.query().is_some() {
        return Err(HybridSearchClientError::InvalidRequest(
            "qdrant_base_url must not contain query parameters".to_string(),
        ));
    }
    if base_url.fragment().is_some() {
        return Err(HybridSearchClientError::InvalidRequest(
            "qdrant_base_url must not contain a fragment".to_string(),
        ));
    }
    if policy.max_attempts == 0 {
        return Err(HybridSearchClientError::InvalidRequest(
            "retry_policy.max_attempts must be > 0".to_string(),
        ));
    }
    Ok(())
}

fn validate_request(req: &HybridSearchRequest) -> Result<(), HybridSearchClientError> {
    if req.embedding.values.is_empty() {
        return Err(HybridSearchClientError::InvalidRequest(
            "embedding must not be empty".to_string(),
        ));
    }
    if req.sparse_vector.indices.len() != req.sparse_vector.values.len() {
        return Err(HybridSearchClientError::InvalidRequest(
            "sparse_vector indices and values must be aligned".to_string(),
        ));
    }
    if req.sparse_vector.indices.is_empty() {
        return Err(HybridSearchClientError::InvalidRequest(
            "sparse_vector must not be empty".to_string(),
        ));
    }
    if req.limit == 0 {
        return Err(HybridSearchClientError::InvalidRequest(
            "limit must be > 0".to_string(),
        ));
    }
    if !req.score_threshold.is_finite() || req.score_threshold < 0.0 {
        return Err(HybridSearchClientError::InvalidRequest(
            "score_threshold must be finite and non-negative".to_string(),
        ));
    }
    Ok(())
}

fn parse_response(raw: &[u8]) -> Result<HybridSearchResponse, HybridSearchClientError> {
    let v: serde_json::Value = serde_json::from_slice(raw).map_err(|_| {
        HybridSearchClientError::InvalidResponse("failed to parse response JSON".to_string())
    })?;

    let points = v
        .get("result")
        .and_then(|r| r.get("points"))
        .and_then(|p| p.as_array())
        .ok_or(HybridSearchClientError::InvalidResponse(
            "missing result.points".to_string(),
        ))?;

    let mut hits = Vec::with_capacity(points.len());
    for point in points {
        let score = point.get("score").and_then(|s| s.as_f64()).ok_or(
            HybridSearchClientError::InvalidResponse("missing score in hit".to_string()),
        )? as f32;

        let payload_obj = point.get("payload").and_then(|p| p.as_object()).ok_or(
            HybridSearchClientError::InvalidResponse("missing payload in hit".to_string()),
        )?;

        let fields = parse_payload(payload_obj).map_err(|e| match e {
            DenseSearchClientError::InvalidResponse(msg) => {
                HybridSearchClientError::InvalidResponse(msg)
            }
            _ => HybridSearchClientError::InvalidResponse("payload parse error".to_string()),
        })?;

        hits.push(RawQdrantHit {
            score,
            payload: RawQdrantPayload { fields },
        });
    }

    Ok(HybridSearchResponse { hits })
}

fn is_retryable(err: &HybridSearchClientError) -> bool {
    matches!(
        err,
        HybridSearchClientError::Transport(_)
            | HybridSearchClientError::UnexpectedStatus(500..=599)
            | HybridSearchClientError::UnexpectedStatus(429)
    )
}

fn percent_encode_path(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_clients::qdrant::QdrantPayloadValue;
    use crate::test_utils::{MockHttpServer, MockResponse};
    use crate::utils::retry::RetryBackoffKind;

    fn policy() -> RetryPolicyConfig {
        RetryPolicyConfig {
            max_attempts: 1,
            backoff: RetryBackoffKind::Exponential,
        }
    }

    fn client(base_url: &str) -> QdrantHybridSearchClient {
        QdrantHybridSearchClient::new(url::Url::parse(base_url).unwrap(), policy()).unwrap()
    }

    fn simple_request() -> HybridSearchRequest {
        HybridSearchRequest {
            collection_name: QdrantCollectionName("col".into()),
            embedding: Embedding {
                values: vec![0.1, 0.2],
            },
            sparse_vector: SparseVector {
                indices: vec![1, 7],
                values: vec![1.0, 1.0],
            },
            vector_name: QdrantVectorName("dense".into()),
            sparse_vector_name: QdrantVectorName("sparse".into()),
            filter: None,
            limit: 6,
            score_threshold: 0.2,
        }
    }

    #[test]
    fn constructor_rejects_zero_max_attempts() {
        let p = RetryPolicyConfig {
            max_attempts: 0,
            backoff: RetryBackoffKind::Exponential,
        };
        assert!(
            QdrantHybridSearchClient::new(url::Url::parse("http://localhost/").unwrap(), p)
                .is_err()
        );
    }

    #[tokio::test]
    async fn successful_response_preserves_hit_order() {
        let resp = serde_json::json!({
            "result": {"points": [
                {"score": 0.8, "payload": {"x": "a"}},
                {"score": 0.5, "payload": {"x": "b"}}
            ]}
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        let result = c.search(&simple_request()).await.unwrap();
        assert_eq!(result.hits.len(), 2);
        assert!((result.hits[0].score - 0.8).abs() < 1e-5);
    }

    #[tokio::test]
    async fn request_body_rrf_fusion_shape() {
        let resp = serde_json::json!({"result": {"points": []}}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        c.search(&simple_request()).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();

        assert_eq!(body["query"]["fusion"], "rrf");
        assert_eq!(body["with_payload"], true);
        assert_eq!(body["with_vector"], false);

        let prefetch = body["prefetch"].as_array().unwrap();
        assert_eq!(prefetch.len(), 2);

        // Dense branch first
        assert_eq!(prefetch[0]["using"], "dense");
        assert_eq!(prefetch[0]["limit"], 6);

        // Sparse branch second
        assert_eq!(prefetch[1]["using"], "sparse");
        assert_eq!(prefetch[1]["limit"], 6);

        // Both limits equal top-level limit
        assert_eq!(body["limit"], 6);
    }

    #[tokio::test]
    async fn no_filter_when_none() {
        let resp = serde_json::json!({"result": {"points": []}}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        c.search(&simple_request()).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert!(body.get("filter").is_none());
    }

    #[tokio::test]
    async fn filter_encoded_as_must_array() {
        use crate::api_clients::qdrant::shared_types::QdrantMatchAnyFilter;
        let resp = serde_json::json!({"result": {"points": []}}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        let mut req = simple_request();
        req.filter = Some(QdrantFilter {
            must_match_any: vec![QdrantMatchAnyFilter {
                field_name: "chunk_tags".into(),
                values: vec!["tag:a".into()],
            }],
        });
        c.search(&req).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["filter"]["must"][0]["key"], "chunk_tags");
    }

    #[tokio::test]
    async fn empty_points_returns_empty_response() {
        let resp = serde_json::json!({"result": {"points": []}}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        assert!(c.search(&simple_request()).await.unwrap().hits.is_empty());
    }

    #[tokio::test]
    async fn non_2xx_returns_unexpected_status() {
        let server = MockHttpServer::new(vec![MockResponse::status(503, b"err".to_vec())]).await;
        let c = client(&server.base_url());
        assert!(matches!(
            c.search(&simple_request()).await.unwrap_err(),
            HybridSearchClientError::UnexpectedStatus(503)
        ));
    }

    #[tokio::test]
    async fn missing_result_points_returns_invalid_response() {
        let resp = serde_json::json!({"status": "ok"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        assert!(matches!(
            c.search(&simple_request()).await.unwrap_err(),
            HybridSearchClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn hit_without_score_returns_invalid_response() {
        let resp = serde_json::json!({"result": {"points": [{"payload": {"x": "y"}}]}}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        assert!(matches!(
            c.search(&simple_request()).await.unwrap_err(),
            HybridSearchClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn hit_without_payload_returns_invalid_response() {
        let resp = serde_json::json!({"result": {"points": [{"score": 0.5}]}}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        assert!(matches!(
            c.search(&simple_request()).await.unwrap_err(),
            HybridSearchClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn nested_payload_objects_are_ignored() {
        let resp = serde_json::json!({
            "result": {"points": [{"score": 0.5, "payload": {"x": "y", "nested": {"a": "b"}}}]}
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        let response = c.search(&simple_request()).await.unwrap();
        assert_eq!(response.hits.len(), 1);
        assert_eq!(
            response.hits[0].payload.fields.get("x"),
            Some(&QdrantPayloadValue::String("y".to_string()))
        );
        assert!(!response.hits[0].payload.fields.contains_key("nested"));
    }
}
