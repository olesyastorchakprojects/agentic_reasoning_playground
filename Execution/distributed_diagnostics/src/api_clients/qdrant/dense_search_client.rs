use backon::Retryable;

use crate::utils::retry::build_backoff;

use super::shared_types::{
    Embedding, QdrantCollectionName, QdrantFilter, QdrantMatchAnyFilter, QdrantPayloadValue,
    QdrantVectorName, RawQdrantHit, RawQdrantPayload, RetryPolicyConfig,
};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum DenseSearchClientError {
    #[error("invalid request: {0}")]
    InvalidRequest(&'static str),
    #[error("transport failure: {0}")]
    Transport(String),
    #[error("unexpected HTTP status: {0}")]
    UnexpectedStatus(u16),
    #[error("invalid response: {0}")]
    InvalidResponse(&'static str),
}

pub struct DenseSearchRequest {
    pub collection_name: QdrantCollectionName,
    pub embedding: Embedding,
    pub vector_name: Option<QdrantVectorName>,
    pub filter: Option<QdrantFilter>,
    pub limit: usize,
    pub score_threshold: f32,
}

#[derive(Debug, Clone)]
pub struct DenseSearchResponse {
    pub hits: Vec<RawQdrantHit>,
}

pub struct QdrantDenseSearchClient {
    http_client: reqwest::Client,
    qdrant_base_url: url::Url,
    retry_policy: RetryPolicyConfig,
}

impl QdrantDenseSearchClient {
    pub fn new(
        qdrant_base_url: url::Url,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, DenseSearchClientError> {
        validate_config(&qdrant_base_url, &retry_policy)?;

        let http_client = reqwest::Client::new();
        Ok(Self { http_client, qdrant_base_url, retry_policy })
    }

    pub async fn search(
        &self,
        request: &DenseSearchRequest,
    ) -> Result<DenseSearchResponse, DenseSearchClientError> {
        validate_request(request)?;

        let collection = percent_encode_path(&request.collection_name.0);
        let url = self
            .qdrant_base_url
            .join(&format!("collections/{}/points/query", collection))
            .map_err(|e| DenseSearchClientError::Transport(e.to_string()))?;

        let wire_query: serde_json::Value =
            serde_json::to_value(&request.embedding.values).unwrap();

        let mut body = serde_json::json!({
            "query": wire_query,
            "limit": request.limit,
            "score_threshold": request.score_threshold,
            "with_payload": true,
            "with_vector": false,
        });

        if let Some(vn) = &request.vector_name {
            body["using"] = serde_json::Value::String(vn.0.clone());
        }

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
                    .map_err(|e| DenseSearchClientError::Transport(e.to_string()))?;

                let status = resp.status().as_u16();
                if !resp.status().is_success() {
                    return Err(DenseSearchClientError::UnexpectedStatus(status));
                }

                resp.bytes()
                    .await
                    .map_err(|e| DenseSearchClientError::Transport(e.to_string()))
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
) -> Result<(), DenseSearchClientError> {
    let s = base_url.scheme();
    if s != "http" && s != "https" {
        return Err(DenseSearchClientError::InvalidRequest("qdrant_base_url must use http or https"));
    }
    if base_url.host().is_none() {
        return Err(DenseSearchClientError::InvalidRequest("qdrant_base_url must contain a host"));
    }
    if base_url.query().is_some() {
        return Err(DenseSearchClientError::InvalidRequest(
            "qdrant_base_url must not contain query parameters",
        ));
    }
    if base_url.fragment().is_some() {
        return Err(DenseSearchClientError::InvalidRequest(
            "qdrant_base_url must not contain a fragment",
        ));
    }
    if policy.max_attempts == 0 {
        return Err(DenseSearchClientError::InvalidRequest(
            "retry_policy.max_attempts must be > 0",
        ));
    }
    Ok(())
}

fn validate_request(req: &DenseSearchRequest) -> Result<(), DenseSearchClientError> {
    if req.embedding.values.is_empty() {
        return Err(DenseSearchClientError::InvalidRequest("embedding must not be empty"));
    }
    if req.limit == 0 {
        return Err(DenseSearchClientError::InvalidRequest("limit must be > 0"));
    }
    if !req.score_threshold.is_finite() || req.score_threshold < 0.0 {
        return Err(DenseSearchClientError::InvalidRequest(
            "score_threshold must be finite and non-negative",
        ));
    }
    Ok(())
}

pub fn encode_filter(f: &QdrantFilter) -> serde_json::Value {
    let must: Vec<serde_json::Value> = f
        .must_match_any
        .iter()
        .map(|m| encode_match_any(m))
        .collect();
    serde_json::json!({ "must": must })
}

fn encode_match_any(m: &QdrantMatchAnyFilter) -> serde_json::Value {
    serde_json::json!({
        "key": m.field_name,
        "match": { "any": m.values }
    })
}

fn parse_response(raw: &[u8]) -> Result<DenseSearchResponse, DenseSearchClientError> {
    let v: serde_json::Value = serde_json::from_slice(raw)
        .map_err(|_| DenseSearchClientError::InvalidResponse("failed to parse response JSON"))?;

    let points = v
        .get("result")
        .and_then(|r| r.get("points"))
        .and_then(|p| p.as_array())
        .ok_or(DenseSearchClientError::InvalidResponse("missing result.points"))?;

    let mut hits = Vec::with_capacity(points.len());
    for point in points {
        let score = point
            .get("score")
            .and_then(|s| s.as_f64())
            .ok_or(DenseSearchClientError::InvalidResponse("missing score in hit"))? as f32;

        let payload_obj = point
            .get("payload")
            .and_then(|p| p.as_object())
            .ok_or(DenseSearchClientError::InvalidResponse("missing payload in hit"))?;

        let fields = parse_payload(payload_obj)?;
        hits.push(RawQdrantHit { score, payload: RawQdrantPayload { fields } });
    }

    Ok(DenseSearchResponse { hits })
}

pub fn parse_payload(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Result<BTreeMap<String, QdrantPayloadValue>, DenseSearchClientError> {
    let mut fields = BTreeMap::new();
    for (k, v) in obj {
        let pv = json_to_payload_value(v)?;
        fields.insert(k.clone(), pv);
    }
    Ok(fields)
}

pub fn json_to_payload_value(
    v: &serde_json::Value,
) -> Result<QdrantPayloadValue, DenseSearchClientError> {
    match v {
        serde_json::Value::String(s) => Ok(QdrantPayloadValue::String(s.clone())),
        serde_json::Value::Number(n) => n
            .as_f64()
            .map(QdrantPayloadValue::Number)
            .ok_or(DenseSearchClientError::InvalidResponse("unsupported numeric payload value")),
        serde_json::Value::Bool(b) => Ok(QdrantPayloadValue::Bool(*b)),
        serde_json::Value::Null => Ok(QdrantPayloadValue::Null),
        serde_json::Value::Array(arr) => {
            let mut strings = Vec::new();
            for item in arr {
                match item {
                    serde_json::Value::String(s) => strings.push(s.clone()),
                    _ => {
                        return Err(DenseSearchClientError::InvalidResponse(
                            "unsupported payload array element type",
                        ))
                    }
                }
            }
            Ok(QdrantPayloadValue::StringList(strings))
        }
        serde_json::Value::Object(_) => Err(DenseSearchClientError::InvalidResponse(
            "unsupported nested object in payload",
        )),
    }
}

fn is_retryable(err: &DenseSearchClientError) -> bool {
    matches!(
        err,
        DenseSearchClientError::Transport(_)
            | DenseSearchClientError::UnexpectedStatus(500..=599)
            | DenseSearchClientError::UnexpectedStatus(429)
    )
}

fn percent_encode_path(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{MockHttpServer, MockResponse};
    use crate::utils::retry::RetryBackoffKind;

    fn policy() -> RetryPolicyConfig {
        RetryPolicyConfig { max_attempts: 1, backoff: RetryBackoffKind::Exponential }
    }

    fn client(base_url: &str) -> QdrantDenseSearchClient {
        QdrantDenseSearchClient::new(url::Url::parse(base_url).unwrap(), policy()).unwrap()
    }

    fn simple_request(_base: &str) -> DenseSearchRequest {
        DenseSearchRequest {
            collection_name: QdrantCollectionName("test_col".into()),
            embedding: Embedding { values: vec![0.1, 0.2, 0.3] },
            vector_name: None,
            filter: None,
            limit: 5,
            score_threshold: 0.2,
        }
    }

    #[test]
    fn constructor_rejects_zero_max_attempts() {
        let p = RetryPolicyConfig { max_attempts: 0, backoff: RetryBackoffKind::Exponential };
        assert!(QdrantDenseSearchClient::new(url::Url::parse("http://localhost/").unwrap(), p).is_err());
    }

    #[tokio::test]
    async fn successful_response_preserves_hit_order() {
        let resp = serde_json::json!({
            "result": {"points": [
                {"score": 0.9, "payload": {"chunk_id": "a"}},
                {"score": 0.7, "payload": {"chunk_id": "b"}}
            ]}
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        let result = c.search(&simple_request(&server.base_url())).await.unwrap();
        assert_eq!(result.hits.len(), 2);
        assert!((result.hits[0].score - 0.9).abs() < 1e-5);
        assert!((result.hits[1].score - 0.7).abs() < 1e-5);
    }

    #[tokio::test]
    async fn request_body_shape() {
        let resp = serde_json::json!({"result": {"points": []}}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        c.search(&simple_request(&server.base_url())).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        let query = body["query"].as_array().unwrap();
        let expected = [0.1_f64, 0.2, 0.3];
        assert_eq!(query.len(), expected.len());
        for (actual, expected) in query.iter().zip(expected.iter()) {
            assert!((actual.as_f64().unwrap() - expected).abs() < 1e-5);
        }
        assert_eq!(body["limit"], 5);
        assert_eq!(body["with_payload"], true);
        assert_eq!(body["with_vector"], false);
    }

    #[tokio::test]
    async fn no_using_when_vector_name_is_none() {
        let resp = serde_json::json!({"result": {"points": []}}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        c.search(&simple_request(&server.base_url())).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert!(body.get("using").is_none());
    }

    #[tokio::test]
    async fn sets_using_when_vector_name_provided() {
        let resp = serde_json::json!({"result": {"points": []}}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        let req = DenseSearchRequest {
            collection_name: QdrantCollectionName("col".into()),
            embedding: Embedding { values: vec![0.1] },
            vector_name: Some(QdrantVectorName("dense".into())),
            filter: None,
            limit: 5,
            score_threshold: 0.0,
        };
        c.search(&req).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["using"], "dense");
    }

    #[tokio::test]
    async fn no_filter_when_none() {
        let resp = serde_json::json!({"result": {"points": []}}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        c.search(&simple_request(&server.base_url())).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert!(body.get("filter").is_none());
    }

    #[tokio::test]
    async fn filter_encoded_as_must_array() {
        let resp = serde_json::json!({"result": {"points": []}}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        let req = DenseSearchRequest {
            collection_name: QdrantCollectionName("col".into()),
            embedding: Embedding { values: vec![0.1] },
            vector_name: None,
            filter: Some(QdrantFilter {
                must_match_any: vec![QdrantMatchAnyFilter {
                    field_name: "card_id".into(),
                    values: vec!["id1".into(), "id2".into()],
                }],
            }),
            limit: 5,
            score_threshold: 0.0,
        };
        c.search(&req).await.unwrap();
        let bodies = server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert_eq!(body["filter"]["must"][0]["key"], "card_id");
        assert_eq!(body["filter"]["must"][0]["match"]["any"], serde_json::json!(["id1", "id2"]));
    }

    #[tokio::test]
    async fn empty_points_returns_empty_response() {
        let resp = serde_json::json!({"result": {"points": []}}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        let result = c.search(&simple_request(&server.base_url())).await.unwrap();
        assert!(result.hits.is_empty());
    }

    #[tokio::test]
    async fn non_2xx_returns_unexpected_status() {
        let server = MockHttpServer::new(vec![MockResponse::status(500, b"err".to_vec())]).await;
        let c = client(&server.base_url());
        assert!(matches!(
            c.search(&simple_request(&server.base_url())).await.unwrap_err(),
            DenseSearchClientError::UnexpectedStatus(500)
        ));
    }

    #[tokio::test]
    async fn missing_result_points_returns_invalid_response() {
        let resp = serde_json::json!({"status": "ok"}).to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        assert!(matches!(
            c.search(&simple_request(&server.base_url())).await.unwrap_err(),
            DenseSearchClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn hit_without_score_returns_invalid_response() {
        let resp = serde_json::json!({
            "result": {"points": [{"payload": {"x": "y"}}]}
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        assert!(matches!(
            c.search(&simple_request(&server.base_url())).await.unwrap_err(),
            DenseSearchClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn hit_without_payload_returns_invalid_response() {
        let resp = serde_json::json!({
            "result": {"points": [{"score": 0.5}]}
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        assert!(matches!(
            c.search(&simple_request(&server.base_url())).await.unwrap_err(),
            DenseSearchClientError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn unsupported_payload_shape_returns_invalid_response() {
        let resp = serde_json::json!({
            "result": {"points": [{"score": 0.5, "payload": {"nested": {"a": "b"}}}]}
        })
        .to_string();
        let server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let c = client(&server.base_url());
        assert!(matches!(
            c.search(&simple_request(&server.base_url())).await.unwrap_err(),
            DenseSearchClientError::InvalidResponse(_)
        ));
    }
}
