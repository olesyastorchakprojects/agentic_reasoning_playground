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
    QdrantHybridCollectionConfig, QdrantPayloadValue, RetryPolicyConfig, SparseStrategyConfig,
    SparseVocabularyArtifact, dense_collection_config_from_settings,
    embedding_config_from_settings, hybrid_collection_config_from_settings,
};

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum TheoryChunksCollectionError {
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

#[derive(Debug, Clone, PartialEq)]
pub struct TheoryChunkSearchRequest {
    pub user_query: NormalizedUserQuery,
    pub limit: usize,
    pub score_threshold: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TheoryChunkSearchHit {
    pub chunk_id: String,
    pub score: f32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TheoryChunkSearchResult {
    pub hits: Vec<TheoryChunkSearchHit>,
}

// ─── Trait ────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait TheoryChunksCollection: Send + Sync {
    async fn search(
        &self,
        request: &TheoryChunkSearchRequest,
    ) -> Result<TheoryChunkSearchResult, TheoryChunksCollectionError>;
}

// ─── Dense implementation ─────────────────────────────────────────────────────

pub struct QdrantTheoryChunksCollectionDense {
    embedding: EmbeddingConfig,
    qdrant: QdrantDenseCollectionConfig,
    embedding_client: EmbeddingClient,
    qdrant_client: QdrantDenseSearchClient,
}

impl QdrantTheoryChunksCollectionDense {
    pub fn from_settings(
        collection_settings: &CollectionRetrievalSettings,
        embedding_model: &EmbeddingModelSettings,
        qdrant_url: &str,
    ) -> Result<Self, TheoryChunksCollectionError> {
        let embedding = embedding_config_from_settings(embedding_model)
            .map_err(|_| TheoryChunksCollectionError::InvalidRequest("invalid embedding settings"))?;
        let qdrant = dense_collection_config_from_settings(collection_settings, qdrant_url)
            .map_err(|_| TheoryChunksCollectionError::InvalidRequest("invalid dense collection settings"))?;

        Self::new(embedding, qdrant, collection_settings.embedding_retry.clone())
    }

    pub fn new(
        embedding: EmbeddingConfig,
        qdrant: QdrantDenseCollectionConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, TheoryChunksCollectionError> {
        let embedding_client = EmbeddingClient::new(embedding.clone(), retry_policy.clone())
            .map_err(TheoryChunksCollectionError::Embedding)?;
        let qdrant_client =
            QdrantDenseSearchClient::new(qdrant.qdrant_base_url.clone(), retry_policy)
                .map_err(TheoryChunksCollectionError::QdrantDense)?;
        Ok(Self { embedding, qdrant, embedding_client, qdrant_client })
    }
}

#[async_trait]
impl TheoryChunksCollection for QdrantTheoryChunksCollectionDense {
    async fn search(
        &self,
        request: &TheoryChunkSearchRequest,
    ) -> Result<TheoryChunkSearchResult, TheoryChunksCollectionError> {
        validate_request(request)?;

        let emb = self
            .embedding_client
            .embed(&request.user_query)
            .await
            .map_err(TheoryChunksCollectionError::Embedding)?;

        if emb.values.len() != self.embedding.embedding_dimension {
            return Err(TheoryChunksCollectionError::IncorrectEmbeddingShape);
        }

        let dense_req = DenseSearchRequest {
            collection_name: self.qdrant.collection_name.clone(),
            embedding: emb,
            vector_name: self.qdrant.vector_name.clone(),
            filter: None,
            limit: request.limit,
            score_threshold: request.score_threshold,
        };

        let resp = self.qdrant_client.search(&dense_req).await?;

        let hits = resp
            .hits
            .into_iter()
            .map(|h| map_hit(h.score, &h.payload.fields))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TheoryChunkSearchResult { hits })
    }
}

// ─── Hybrid implementation ────────────────────────────────────────────────────

pub struct QdrantTheoryChunksCollectionHybrid {
    embedding: EmbeddingConfig,
    qdrant: QdrantHybridCollectionConfig,
    sparse: SparseStrategyConfig,
    embedding_client: EmbeddingClient,
    qdrant_client: QdrantHybridSearchClient,
    sparse_vocabulary: SparseVocabularyArtifact,
    bm25_term_stats: Option<Bm25TermStatsArtifact>,
    tokenizer: HfTokenizer,
}

impl QdrantTheoryChunksCollectionHybrid {
    pub async fn from_settings(
        collection_settings: &CollectionRetrievalSettings,
        embedding_model: &EmbeddingModelSettings,
        qdrant_url: &str,
    ) -> Result<Self, TheoryChunksCollectionError> {
        let embedding = embedding_config_from_settings(embedding_model)
            .map_err(|_| TheoryChunksCollectionError::InvalidRequest("invalid embedding settings"))?;
        let (qdrant, sparse) =
            hybrid_collection_config_from_settings(collection_settings, qdrant_url).map_err(
                |_| TheoryChunksCollectionError::InvalidRequest("invalid hybrid collection settings"),
            )?;

        Self::new(embedding, qdrant, sparse, collection_settings.embedding_retry.clone()).await
    }

    pub async fn new(
        embedding: EmbeddingConfig,
        qdrant: QdrantHybridCollectionConfig,
        sparse: SparseStrategyConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, TheoryChunksCollectionError> {
        let loaded = sparse_preparation::load_sparse_artifacts(&sparse, &qdrant.collection_name.0)
            .await
            .map_err(TheoryChunksCollectionError::InvalidRequest)?;

        let embedding_client = EmbeddingClient::new(embedding.clone(), retry_policy.clone())
            .map_err(TheoryChunksCollectionError::Embedding)?;
        let qdrant_client =
            QdrantHybridSearchClient::new(qdrant.qdrant_base_url.clone(), retry_policy)
                .map_err(TheoryChunksCollectionError::QdrantHybrid)?;

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
impl TheoryChunksCollection for QdrantTheoryChunksCollectionHybrid {
    async fn search(
        &self,
        request: &TheoryChunkSearchRequest,
    ) -> Result<TheoryChunkSearchResult, TheoryChunksCollectionError> {
        validate_request(request)?;

        let emb = self
            .embedding_client
            .embed(&request.user_query)
            .await
            .map_err(TheoryChunksCollectionError::Embedding)?;

        if emb.values.len() != self.embedding.embedding_dimension {
            return Err(TheoryChunksCollectionError::IncorrectEmbeddingShape);
        }

        let sparse = sparse_preparation::build_sparse_vector(
            &request.user_query.0,
            &self.tokenizer,
            &self.sparse_vocabulary,
            self.bm25_term_stats.as_ref(),
            &self.sparse,
        )
        .map_err(TheoryChunksCollectionError::QueryPreparation)?;

        let hybrid_req = HybridSearchRequest {
            collection_name: self.qdrant.collection_name.clone(),
            embedding: emb,
            sparse_vector: sparse,
            vector_name: self.qdrant.vector_name.clone(),
            sparse_vector_name: self.qdrant.sparse_vector_name.clone(),
            filter: None,
            limit: request.limit,
            score_threshold: request.score_threshold,
        };

        let resp = self.qdrant_client.search(&hybrid_req).await?;

        let hits = resp
            .hits
            .into_iter()
            .map(|h| map_hit(h.score, &h.payload.fields))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(TheoryChunkSearchResult { hits })
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn validate_request(req: &TheoryChunkSearchRequest) -> Result<(), TheoryChunksCollectionError> {
    if req.user_query.0.trim().is_empty() {
        return Err(TheoryChunksCollectionError::InvalidRequest("user_query must not be empty"));
    }
    if req.limit == 0 {
        return Err(TheoryChunksCollectionError::InvalidRequest("limit must be > 0"));
    }
    if !req.score_threshold.is_finite() || req.score_threshold < 0.0 {
        return Err(TheoryChunksCollectionError::InvalidRequest(
            "score_threshold must be finite and non-negative",
        ));
    }
    Ok(())
}

fn map_hit(
    score: f32,
    fields: &std::collections::BTreeMap<String, QdrantPayloadValue>,
) -> Result<TheoryChunkSearchHit, TheoryChunksCollectionError> {
    let chunk_id = match fields.get("chunk_id") {
        Some(QdrantPayloadValue::String(s)) if !s.is_empty() => s.clone(),
        Some(QdrantPayloadValue::String(_)) => {
            return Err(TheoryChunksCollectionError::PayloadMapping("chunk_id is empty"))
        }
        _ => return Err(TheoryChunksCollectionError::PayloadMapping("missing or invalid chunk_id")),
    };

    let text = match fields.get("text") {
        Some(QdrantPayloadValue::String(s)) if !s.is_empty() => s.clone(),
        Some(QdrantPayloadValue::String(_)) => {
            return Err(TheoryChunksCollectionError::PayloadMapping("text is empty"))
        }
        _ => return Err(TheoryChunksCollectionError::PayloadMapping("missing or invalid text")),
    };

    Ok(TheoryChunkSearchHit { chunk_id, score, text })
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
            collection_name: QdrantCollectionName("theory".into()),
            vector_name: None,
        }
    }

    fn valid_request() -> TheoryChunkSearchRequest {
        TheoryChunkSearchRequest {
            user_query: NormalizedUserQuery("consensus fault".into()),
            limit: 5,
            score_threshold: 0.2,
        }
    }

    fn emb_resp() -> String {
        serde_json::json!({"embeddings": [[0.1, 0.2]]}).to_string()
    }

    fn qdrant_resp_hit() -> String {
        serde_json::json!({
            "result": {"points": [{"score": 0.8, "payload": {"chunk_id": "tc1", "text": "theory text"}}]}
        })
        .to_string()
    }

    // ── Validation ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn dense_validates_before_embedding() {
        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantTheoryChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let req = TheoryChunkSearchRequest {
            user_query: NormalizedUserQuery("".into()),
            limit: 5,
            score_threshold: 0.2,
        };
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            TheoryChunksCollectionError::InvalidRequest(_)
        ));
        assert!(emb_server.take_bodies().await.is_empty());
    }

    #[tokio::test]
    async fn zero_limit_fails() {
        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantTheoryChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let mut req = valid_request();
        req.limit = 0;
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            TheoryChunksCollectionError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn invalid_threshold_fails() {
        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantTheoryChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let mut req = valid_request();
        req.score_threshold = f32::INFINITY;
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            TheoryChunksCollectionError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn embedding_shape_mismatch_fails_before_qdrant() {
        let emb_resp = serde_json::json!({"embeddings": [[0.1]]}).to_string();
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp)]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;

        let client = QdrantTheoryChunksCollectionDense {
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
        assert!(matches!(err, TheoryChunksCollectionError::IncorrectEmbeddingShape));
        assert!(qdrant_server.take_bodies().await.is_empty());
    }

    // ── No filter ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn dense_builds_request_with_no_filter() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(qdrant_resp_hit())]).await;
        let client = QdrantTheoryChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        client.search(&valid_request()).await.unwrap();
        let bodies = qdrant_server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert!(body.get("filter").is_none());
    }

    // ── Error wrapping ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn dense_transport_error_wrapped_as_qdrant_dense() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::status(500, b"e".to_vec())]).await;
        let client = QdrantTheoryChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(matches!(
            client.search(&valid_request()).await.unwrap_err(),
            TheoryChunksCollectionError::QdrantDense(_)
        ));
    }

    #[tokio::test]
    async fn embedding_error_wrapped_as_embedding() {
        let emb_server = MockHttpServer::new(vec![MockResponse::status(503, b"e".to_vec())]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantTheoryChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(matches!(
            client.search(&valid_request()).await.unwrap_err(),
            TheoryChunksCollectionError::Embedding(_)
        ));
    }

    // ── Payload mapping ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn valid_payload_maps_to_hit() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(qdrant_resp_hit())]).await;
        let client = QdrantTheoryChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let result = client.search(&valid_request()).await.unwrap();
        assert_eq!(result.hits[0].chunk_id, "tc1");
        assert_eq!(result.hits[0].text, "theory text");
    }

    #[tokio::test]
    async fn empty_chunk_id_fails_with_payload_mapping() {
        let resp = serde_json::json!({
            "result": {"points": [{"score": 0.8, "payload": {"chunk_id": "", "text": "txt"}}]}
        })
        .to_string();
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let client = QdrantTheoryChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(matches!(
            client.search(&valid_request()).await.unwrap_err(),
            TheoryChunksCollectionError::PayloadMapping(_)
        ));
    }

    #[tokio::test]
    async fn empty_text_fails_with_payload_mapping() {
        let resp = serde_json::json!({
            "result": {"points": [{"score": 0.8, "payload": {"chunk_id": "c1", "text": ""}}]}
        })
        .to_string();
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let client = QdrantTheoryChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(matches!(
            client.search(&valid_request()).await.unwrap_err(),
            TheoryChunksCollectionError::PayloadMapping(_)
        ));
    }

    #[tokio::test]
    async fn one_invalid_hit_fails_whole_mapping() {
        let resp = serde_json::json!({
            "result": {"points": [
                {"score": 0.9, "payload": {"chunk_id": "c1", "text": "ok"}},
                {"score": 0.7, "payload": {"chunk_id": "", "text": "bad"}}
            ]}
        })
        .to_string();
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let client = QdrantTheoryChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(matches!(
            client.search(&valid_request()).await.unwrap_err(),
            TheoryChunksCollectionError::PayloadMapping(_)
        ));
    }

    #[tokio::test]
    async fn empty_transport_result_returns_empty() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(
            serde_json::json!({"result": {"points": []}}).to_string(),
        )])
        .await;
        let client = QdrantTheoryChunksCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_col(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(client.search(&valid_request()).await.unwrap().hits.is_empty());
    }

    // ── Hybrid artifact validation ─────────────────────────────────────────────

    #[tokio::test]
    async fn hybrid_constructor_fails_when_vocabulary_absent() {
        let result = QdrantTheoryChunksCollectionHybrid::new(
            EmbeddingConfig {
                base_url: url::Url::parse("http://localhost/").unwrap(),
                model_name: "m".into(),
                embedding_dimension: 2,
            },
            QdrantHybridCollectionConfig {
                qdrant_base_url: url::Url::parse("http://localhost/").unwrap(),
                collection_name: QdrantCollectionName("theory".into()),
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
            "vocabulary_name": "theory__sparse_vocabulary",
            "collection_name": "theory",
            "text_processing": {"lowercase": true, "min_token_length": 2},
            "tokenizer": {"library": "tokenizers", "source": "test/model"},
            "created_at": "2024-01-01T00:00:00Z",
            "tokens": []
        })
        .to_string();
        let stats_json = serde_json::json!({
            "collection_name": "theory",
            "vocabulary_name": "WRONG",
            "sparse_strategy": {"kind": "bm25_like", "version": "v1"},
            "document_count": 10,
            "average_document_length": 5.0,
            "document_frequency_by_token_id": {},
            "created_at": "2024-01-01T00:00:00Z"
        })
        .to_string();

        let vocab_path = dir.write_json("vocab.json", &vocab_json);
        let stats_path = dir.write_json("stats.json", &stats_json);

        let result = QdrantTheoryChunksCollectionHybrid::new(
            EmbeddingConfig {
                base_url: url::Url::parse("http://localhost/").unwrap(),
                model_name: "m".into(),
                embedding_dimension: 2,
            },
            QdrantHybridCollectionConfig {
                qdrant_base_url: url::Url::parse("http://localhost/").unwrap(),
                collection_name: QdrantCollectionName("theory".into()),
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
