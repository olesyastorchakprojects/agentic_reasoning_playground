use async_trait::async_trait;

use crate::api_clients::embedding_client::{EmbeddingClient, EmbeddingClientError};
use crate::utils::tokenizer::HfTokenizer;
use crate::config::{CollectionRetrievalSettings, EmbeddingModelSettings};
use super::dense_search_client::{DenseSearchClientError, DenseSearchRequest, QdrantDenseSearchClient};
use super::hybrid_search_client::{
    HybridSearchClientError, HybridSearchRequest, QdrantHybridSearchClient,
};
use super::sparse_preparation;
use super::shared_types::{
    Bm25TermStatsArtifact, EmbeddingConfig, NormalizedUserQuery, QdrantDenseCollectionConfig,
    QdrantFilter, QdrantHybridCollectionConfig, QdrantMatchAnyFilter, QdrantPayloadValue,
    RetryPolicyConfig, SparseStrategyConfig, SparseVocabularyArtifact,
    dense_collection_config_from_settings, embedding_config_from_settings,
    hybrid_collection_config_from_settings,
};

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum PracticeChunksCollectionError {
    #[error("invalid request: {0}")]
    InvalidRequest(&'static str),
    #[error("query preparation failed: {0}")]
    QueryPreparation(&'static str),
    #[error("embedding shape does not match configured dimension")]
    IncorrectEmbeddingShape,
    #[error("qdrant dense error: {0}")]
    QdrantDense(#[from] DenseSearchClientError),
    #[error("qdrant hybrid error: {0}")]
    QdrantHybrid(#[from] HybridSearchClientError),
    #[error("embedding client error: {0}")]
    Embedding(#[from] EmbeddingClientError),
    #[error("payload mapping failed: {0}")]
    PayloadMapping(&'static str),
}

// ─── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PracticeChunkFilter {
    pub case_ids: Vec<String>,
    pub chunk_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PracticeChunkSearchRequest {
    pub user_query: NormalizedUserQuery,
    pub filter: PracticeChunkFilter,
    pub limit: usize,
    pub score_threshold: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PracticeChunkSearchHit {
    pub chunk_id: String,
    pub score: f32,
    pub case_id: String,
    pub chunk_tags: Vec<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PracticeChunkSearchResult {
    pub hits: Vec<PracticeChunkSearchHit>,
}

// ─── Trait ────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait PracticeChunksCollection: Send + Sync {
    async fn search(
        &self,
        request: &PracticeChunkSearchRequest,
    ) -> Result<PracticeChunkSearchResult, PracticeChunksCollectionError>;
}

// ─── Dense implementation ─────────────────────────────────────────────────────

pub struct QdrantPracticeChunksCollectionDense {
    embedding: EmbeddingConfig,
    qdrant: QdrantDenseCollectionConfig,
    embedding_client: EmbeddingClient,
    qdrant_client: QdrantDenseSearchClient,
}

impl QdrantPracticeChunksCollectionDense {
    pub fn from_settings(
        collection_settings: &CollectionRetrievalSettings,
        embedding_model: &EmbeddingModelSettings,
        qdrant_url: &str,
    ) -> Result<Self, PracticeChunksCollectionError> {
        let embedding = embedding_config_from_settings(embedding_model).map_err(|_| {
            PracticeChunksCollectionError::InvalidRequest("invalid embedding settings")
        })?;
        let qdrant = dense_collection_config_from_settings(collection_settings, qdrant_url)
            .map_err(|_| {
                PracticeChunksCollectionError::InvalidRequest("invalid dense collection settings")
            })?;

        Self::new(embedding, qdrant, collection_settings.embedding_retry.clone())
    }

    pub fn new(
        embedding: EmbeddingConfig,
        qdrant: QdrantDenseCollectionConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, PracticeChunksCollectionError> {
        let embedding_client = EmbeddingClient::new(embedding.clone(), retry_policy.clone())
            .map_err(PracticeChunksCollectionError::Embedding)?;
        let qdrant_client =
            QdrantDenseSearchClient::new(qdrant.qdrant_base_url.clone(), retry_policy)
                .map_err(PracticeChunksCollectionError::QdrantDense)?;
        Ok(Self { embedding, qdrant, embedding_client, qdrant_client })
    }
}

#[async_trait]
impl PracticeChunksCollection for QdrantPracticeChunksCollectionDense {
    async fn search(
        &self,
        request: &PracticeChunkSearchRequest,
    ) -> Result<PracticeChunkSearchResult, PracticeChunksCollectionError> {
        validate_request(request)?;

        let emb = self
            .embedding_client
            .embed(&request.user_query)
            .await
            .map_err(PracticeChunksCollectionError::Embedding)?;

        if emb.values.len() != self.embedding.embedding_dimension {
            return Err(PracticeChunksCollectionError::IncorrectEmbeddingShape);
        }

        let filter = map_filter(&request.filter);

        let dense_req = DenseSearchRequest {
            collection_name: self.qdrant.collection_name.clone(),
            embedding: emb,
            vector_name: self.qdrant.vector_name.clone(),
            filter: Some(filter),
            limit: request.limit,
            score_threshold: request.score_threshold,
        };

        let resp = self.qdrant_client.search(&dense_req).await?;

        let hits = resp
            .hits
            .into_iter()
            .map(|h| map_hit(h.score, &h.payload.fields))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PracticeChunkSearchResult { hits })
    }
}

// ─── Hybrid implementation ────────────────────────────────────────────────────

pub struct QdrantPracticeChunksCollectionHybrid {
    embedding: EmbeddingConfig,
    qdrant: QdrantHybridCollectionConfig,
    sparse: SparseStrategyConfig,
    embedding_client: EmbeddingClient,
    qdrant_client: QdrantHybridSearchClient,
    sparse_vocabulary: SparseVocabularyArtifact,
    bm25_term_stats: Option<Bm25TermStatsArtifact>,
    tokenizer: HfTokenizer,
}

impl QdrantPracticeChunksCollectionHybrid {
    pub async fn from_settings(
        collection_settings: &CollectionRetrievalSettings,
        embedding_model: &EmbeddingModelSettings,
        qdrant_url: &str,
    ) -> Result<Self, PracticeChunksCollectionError> {
        let embedding = embedding_config_from_settings(embedding_model).map_err(|_| {
            PracticeChunksCollectionError::InvalidRequest("invalid embedding settings")
        })?;
        let (qdrant, sparse) =
            hybrid_collection_config_from_settings(collection_settings, qdrant_url).map_err(
                |_| {
                    PracticeChunksCollectionError::InvalidRequest(
                        "invalid hybrid collection settings",
                    )
                },
            )?;

        Self::new(embedding, qdrant, sparse, collection_settings.embedding_retry.clone()).await
    }

    pub async fn new(
        embedding: EmbeddingConfig,
        qdrant: QdrantHybridCollectionConfig,
        sparse: SparseStrategyConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, PracticeChunksCollectionError> {
        let loaded = sparse_preparation::load_sparse_artifacts(&sparse, &qdrant.collection_name.0)
            .await
            .map_err(PracticeChunksCollectionError::InvalidRequest)?;

        let embedding_client = EmbeddingClient::new(embedding.clone(), retry_policy.clone())
            .map_err(PracticeChunksCollectionError::Embedding)?;
        let qdrant_client =
            QdrantHybridSearchClient::new(qdrant.qdrant_base_url.clone(), retry_policy)
                .map_err(PracticeChunksCollectionError::QdrantHybrid)?;

        Ok(Self {
            embedding,
            qdrant,
            sparse,
            embedding_client,
            qdrant_client,
            sparse_vocabulary: loaded.vocabulary,
            bm25_term_stats: loaded.bm25_term_stats,
            tokenizer: loaded.tokenizer,
        })
    }
}

#[async_trait]
impl PracticeChunksCollection for QdrantPracticeChunksCollectionHybrid {
    async fn search(
        &self,
        request: &PracticeChunkSearchRequest,
    ) -> Result<PracticeChunkSearchResult, PracticeChunksCollectionError> {
        validate_request(request)?;

        let emb = self
            .embedding_client
            .embed(&request.user_query)
            .await
            .map_err(PracticeChunksCollectionError::Embedding)?;

        if emb.values.len() != self.embedding.embedding_dimension {
            return Err(PracticeChunksCollectionError::IncorrectEmbeddingShape);
        }

        let sparse = sparse_preparation::build_sparse_vector(
            &request.user_query.0,
            &self.tokenizer,
            &self.sparse_vocabulary,
            self.bm25_term_stats.as_ref(),
            &self.sparse,
        )
        .map_err(PracticeChunksCollectionError::QueryPreparation)?;

        let filter = map_filter(&request.filter);

        let hybrid_req = HybridSearchRequest {
            collection_name: self.qdrant.collection_name.clone(),
            embedding: emb,
            sparse_vector: sparse,
            vector_name: self.qdrant.vector_name.clone(),
            sparse_vector_name: self.qdrant.sparse_vector_name.clone(),
            filter: Some(filter),
            limit: request.limit,
            score_threshold: request.score_threshold,
        };

        let resp = self.qdrant_client.search(&hybrid_req).await?;

        let hits = resp
            .hits
            .into_iter()
            .map(|h| map_hit(h.score, &h.payload.fields))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PracticeChunkSearchResult { hits })
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn validate_request(req: &PracticeChunkSearchRequest) -> Result<(), PracticeChunksCollectionError> {
    if req.user_query.0.trim().is_empty() {
        return Err(PracticeChunksCollectionError::InvalidRequest("user_query must not be empty"));
    }
    if req.filter.case_ids.is_empty() {
        return Err(PracticeChunksCollectionError::InvalidRequest("filter.case_ids must not be empty"));
    }
    if req.filter.chunk_tags.is_empty() {
        return Err(PracticeChunksCollectionError::InvalidRequest(
            "filter.chunk_tags must not be empty",
        ));
    }
    if req.limit == 0 {
        return Err(PracticeChunksCollectionError::InvalidRequest("limit must be > 0"));
    }
    if !req.score_threshold.is_finite() || req.score_threshold < 0.0 {
        return Err(PracticeChunksCollectionError::InvalidRequest(
            "score_threshold must be finite and non-negative",
        ));
    }
    Ok(())
}

fn map_filter(f: &PracticeChunkFilter) -> QdrantFilter {
    QdrantFilter {
        must_match_any: vec![
            QdrantMatchAnyFilter {
                field_name: "doc_id".to_string(),
                values: f.case_ids.clone(),
            },
            QdrantMatchAnyFilter {
                field_name: "chunk_tags".to_string(),
                values: f.chunk_tags.clone(),
            },
        ],
    }
}

fn map_hit(
    score: f32,
    fields: &std::collections::BTreeMap<String, QdrantPayloadValue>,
) -> Result<PracticeChunkSearchHit, PracticeChunksCollectionError> {
    let chunk_id = match fields.get("chunk_id") {
        Some(QdrantPayloadValue::String(s)) => s.clone(),
        _ => return Err(PracticeChunksCollectionError::PayloadMapping("missing or invalid chunk_id")),
    };

    let case_id = match fields.get("doc_id") {
        Some(QdrantPayloadValue::String(s)) if !s.is_empty() => s.clone(),
        Some(QdrantPayloadValue::String(_)) => {
            return Err(PracticeChunksCollectionError::PayloadMapping("empty doc_id"))
        }
        Some(_) => {
            return Err(PracticeChunksCollectionError::PayloadMapping(
                "invalid doc_id type",
            ))
        }
        None => {
            return Err(PracticeChunksCollectionError::PayloadMapping(
                "missing or invalid doc_id",
            ))
        }
    };

    let chunk_tags = match fields.get("chunk_tags") {
        Some(QdrantPayloadValue::StringList(v)) => v.clone(),
        _ => {
            return Err(PracticeChunksCollectionError::PayloadMapping(
                "missing or invalid chunk_tags",
            ))
        }
    };

    let text = match fields.get("text") {
        Some(QdrantPayloadValue::String(s)) => s.clone(),
        _ => return Err(PracticeChunksCollectionError::PayloadMapping("missing or invalid text")),
    };

    Ok(PracticeChunkSearchHit { chunk_id, score, case_id, chunk_tags, text })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_clients::qdrant::shared_types::{
        QdrantCollectionName, QdrantVectorName, RetryBackoffKind,
    };
    use crate::test_utils::{MockHttpServer, MockResponse, TempArtifactDir, populate_tokenizer_cache};

    fn policy() -> RetryPolicyConfig {
        RetryPolicyConfig { max_attempts: 1, backoff: RetryBackoffKind::Exponential }
    }

    fn emb_config(base_url: &str) -> EmbeddingConfig {
        EmbeddingConfig {
            base_url: url::Url::parse(base_url).unwrap(),
            model_name: "m".into(),
            embedding_dimension: 2,
        }
    }

    fn dense_col(qdrant_url: &str) -> QdrantDenseCollectionConfig {
        QdrantDenseCollectionConfig {
            qdrant_base_url: url::Url::parse(qdrant_url).unwrap(),
            collection_name: QdrantCollectionName("practice".into()),
            vector_name: None,
        }
    }

    fn valid_request() -> PracticeChunkSearchRequest {
        PracticeChunkSearchRequest {
            user_query: NormalizedUserQuery("query text".into()),
            filter: PracticeChunkFilter {
                case_ids: vec!["case1".into()],
                chunk_tags: vec!["tag:x".into()],
            },
            limit: 5,
            score_threshold: 0.2,
        }
    }

    fn emb_resp() -> String {
        serde_json::json!({"embeddings": [[0.1, 0.2]]}).to_string()
    }

    fn qdrant_resp_hit() -> String {
        serde_json::json!({
            "result": {"points": [{
                "score": 0.8,
                "payload": {
                    "chunk_id": "chunk1",
                    "doc_id": "case1",
                    "chunk_tags": ["tag:x"],
                    "text": "some text"
                }
            }]}
        })
        .to_string()
    }

    // ── Validation ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn dense_validates_before_embedding() {
        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let req = PracticeChunkSearchRequest {
            user_query: NormalizedUserQuery("".into()),
            filter: PracticeChunkFilter {
                case_ids: vec!["c".into()],
                chunk_tags: vec!["t".into()],
            },
            limit: 5,
            score_threshold: 0.2,
        };
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            PracticeChunksCollectionError::InvalidRequest(_)
        ));
        assert!(emb_server.take_bodies().await.is_empty());
    }

    #[tokio::test]
    async fn empty_case_ids_returns_invalid_request() {
        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let req = PracticeChunkSearchRequest {
            user_query: NormalizedUserQuery("q".into()),
            filter: PracticeChunkFilter { case_ids: vec![], chunk_tags: vec!["t".into()] },
            limit: 5,
            score_threshold: 0.2,
        };
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            PracticeChunksCollectionError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn empty_chunk_tags_returns_invalid_request() {
        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let req = PracticeChunkSearchRequest {
            user_query: NormalizedUserQuery("q".into()),
            filter: PracticeChunkFilter { case_ids: vec!["c".into()], chunk_tags: vec![] },
            limit: 5,
            score_threshold: 0.2,
        };
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            PracticeChunksCollectionError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn zero_limit_fails() {
        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let mut req = valid_request();
        req.limit = 0;
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            PracticeChunksCollectionError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn invalid_threshold_fails() {
        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let mut req = valid_request();
        req.score_threshold = f32::NEG_INFINITY;
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            PracticeChunksCollectionError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn embedding_shape_mismatch_fails_before_qdrant() {
        // Embedding returns 1 value but collection expects 2.
        let emb_resp = serde_json::json!({"embeddings": [[0.1]]}).to_string();
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp)]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;

        let client = QdrantPracticeChunksCollectionDense {
            embedding: EmbeddingConfig {
                base_url: url::Url::parse(&emb_server.base_url()).unwrap(),
                model_name: "m".into(),
                embedding_dimension: 2,
            },
            qdrant: dense_col(&qdrant_server.base_url()),
            embedding_client: EmbeddingClient::new(
                EmbeddingConfig {
                    base_url: url::Url::parse(&emb_server.base_url()).unwrap(),
                    model_name: "m".into(),
                    embedding_dimension: 1,
                },
                policy(),
            )
            .unwrap(),
            qdrant_client: QdrantDenseSearchClient::new(
                url::Url::parse(&qdrant_server.base_url()).unwrap(),
                policy(),
            )
            .unwrap(),
        };

        let err = client.search(&valid_request()).await.unwrap_err();
        assert!(matches!(err, PracticeChunksCollectionError::IncorrectEmbeddingShape));
        assert!(qdrant_server.take_bodies().await.is_empty());
    }

    // ── Filter mapping ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn dense_maps_filter_correctly() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server =
            MockHttpServer::new(vec![MockResponse::ok(
                serde_json::json!({"result": {"points": []}}).to_string(),
            )])
            .await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        client.search(&valid_request()).await.unwrap();
        let bodies = qdrant_server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        let must = body["filter"]["must"].as_array().unwrap();
        let card_clause = must.iter().find(|c| c["key"] == "doc_id").unwrap();
        let tag_clause = must.iter().find(|c| c["key"] == "chunk_tags").unwrap();
        assert_eq!(card_clause["match"]["any"], serde_json::json!(["case1"]));
        assert_eq!(tag_clause["match"]["any"], serde_json::json!(["tag:x"]));
    }

    // ── Error wrapping ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn dense_transport_error_wrapped_as_qdrant_dense() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::status(500, b"e".to_vec())]).await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(matches!(
            client.search(&valid_request()).await.unwrap_err(),
            PracticeChunksCollectionError::QdrantDense(_)
        ));
    }

    #[tokio::test]
    async fn embedding_error_wrapped_as_embedding() {
        let emb_server = MockHttpServer::new(vec![MockResponse::status(503, b"e".to_vec())]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(matches!(
            client.search(&valid_request()).await.unwrap_err(),
            PracticeChunksCollectionError::Embedding(_)
        ));
    }

    // ── Payload mapping ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn valid_payload_maps_to_hit() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(qdrant_resp_hit())]).await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let result = client.search(&valid_request()).await.unwrap();
        assert_eq!(result.hits[0].chunk_id, "chunk1");
        assert_eq!(result.hits[0].case_id, "case1");
        assert_eq!(result.hits[0].chunk_tags, vec!["tag:x"]);
        assert_eq!(result.hits[0].text, "some text");
    }

    #[tokio::test]
    async fn invalid_doc_id_type_fails_with_payload_mapping() {
        let resp = serde_json::json!({
            "result": {"points": [{
                "score": 0.8,
                "payload": {
                    "chunk_id": "c1",
                    "doc_id": 123,
                    "chunk_tags": ["t"],
                    "text": "txt"
                }
            }]}
        })
        .to_string();
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(matches!(
            client.search(&valid_request()).await.unwrap_err(),
            PracticeChunksCollectionError::PayloadMapping(_)
        ));
    }

    #[tokio::test]
    async fn empty_doc_id_fails_with_payload_mapping() {
        let resp = serde_json::json!({
            "result": {"points": [{
                "score": 0.8,
                "payload": {
                    "chunk_id": "c1",
                    "doc_id": "",
                    "chunk_tags": ["t"],
                    "text": "txt"
                }
            }]}
        })
        .to_string();
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(matches!(
            client.search(&valid_request()).await.unwrap_err(),
            PracticeChunksCollectionError::PayloadMapping(_)
        ));
    }

    #[tokio::test]
    async fn one_invalid_hit_fails_whole_mapping() {
        let resp = serde_json::json!({
            "result": {"points": [
                {"score": 0.9, "payload": {"chunk_id": "c1", "doc_id": "case1", "chunk_tags": ["t"], "text": "ok"}},
                {"score": 0.7, "payload": {"chunk_id": "c2", "doc_id": 99, "chunk_tags": ["t"], "text": "bad"}}
            ]}
        })
        .to_string();
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(matches!(
            client.search(&valid_request()).await.unwrap_err(),
            PracticeChunksCollectionError::PayloadMapping(_)
        ));
    }

    #[tokio::test]
    async fn empty_transport_result_returns_empty() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(
            serde_json::json!({"result": {"points": []}}).to_string(),
        )])
        .await;
        let client = QdrantPracticeChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let result = client.search(&valid_request()).await.unwrap();
        assert!(result.hits.is_empty());
    }

    // ── Hybrid artifact validation ─────────────────────────────────────────────

    #[tokio::test]
    async fn hybrid_constructor_fails_when_vocabulary_absent() {
        let result = QdrantPracticeChunksCollectionHybrid::new(
            EmbeddingConfig {
                base_url: url::Url::parse("http://localhost/").unwrap(),
                model_name: "m".into(),
                embedding_dimension: 2,
            },
            QdrantHybridCollectionConfig {
                qdrant_base_url: url::Url::parse("http://localhost/").unwrap(),
                collection_name: QdrantCollectionName("practice".into()),
                vector_name: QdrantVectorName("dense".into()),
                sparse_vector_name: QdrantVectorName("sparse".into()),
            },
            SparseStrategyConfig::BagOfWords {
                sparse_vocabulary_path: "/nonexistent/vocab.json".into(),
            },
            policy(),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn hybrid_constructor_validates_artifact_compatibility() {
        let dir = TempArtifactDir::new();
        populate_tokenizer_cache("test/model");
        let vocab_json = serde_json::json!({
            "vocabulary_name": "practice__sparse_vocabulary",
            "collection_name": "practice",
            "text_processing": {"lowercase": true, "min_token_length": 2},
            "tokenizer": {"library": "tokenizers", "source": "test/model"},
            "created_at": "2024-01-01T00:00:00Z",
            "tokens": []
        })
        .to_string();
        let stats_json = serde_json::json!({
            "collection_name": "practice",
            "vocabulary_name": "WRONG_vocab_name",
            "sparse_strategy": {"kind": "bm25_like", "version": "v1"},
            "document_count": 10,
            "average_document_length": 5.0,
            "document_frequency_by_token_id": {},
            "created_at": "2024-01-01T00:00:00Z"
        })
        .to_string();

        let vocab_path = dir.write_json("vocab.json", &vocab_json);
        let stats_path = dir.write_json("stats.json", &stats_json);

        let result = QdrantPracticeChunksCollectionHybrid::new(
            EmbeddingConfig {
                base_url: url::Url::parse("http://localhost/").unwrap(),
                model_name: "m".into(),
                embedding_dimension: 2,
            },
            QdrantHybridCollectionConfig {
                qdrant_base_url: url::Url::parse("http://localhost/").unwrap(),
                collection_name: QdrantCollectionName("practice".into()),
                vector_name: QdrantVectorName("dense".into()),
                sparse_vector_name: QdrantVectorName("sparse".into()),
            },
            SparseStrategyConfig::Bm25Like {
                sparse_vocabulary_path: vocab_path.to_str().unwrap().to_string(),
                bm25_term_stats_path: stats_path.to_str().unwrap().to_string(),
                k1: 1.5,
                b: 0.75,
                idf_smoothing: 0.5,
            },
            policy(),
        )
        .await;
        assert!(result.is_err());
    }
}
