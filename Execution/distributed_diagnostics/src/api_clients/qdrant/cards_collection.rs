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
    Bm25TermStatsArtifact, EmbeddingConfig, NormalizedUserQuery,
    QdrantDenseCollectionConfig, QdrantHybridCollectionConfig, QdrantPayloadValue,
    RetryPolicyConfig, SparseStrategyConfig, SparseVocabularyArtifact,
    dense_collection_config_from_settings, embedding_config_from_settings,
    hybrid_collection_config_from_settings,
};

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CardsCollectionError {
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
pub struct CardSearchRequest {
    pub user_query: NormalizedUserQuery,
    pub limit: usize,
    pub score_threshold: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardSearchHit {
    pub case_id: String,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardSearchResult {
    pub hits: Vec<CardSearchHit>,
}

// ─── Trait ────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait CardsCollection: Send + Sync {
    async fn search(
        &self,
        request: &CardSearchRequest,
    ) -> Result<CardSearchResult, CardsCollectionError>;
}

// ─── Dense implementation ─────────────────────────────────────────────────────

pub struct QdrantCardsCollectionDense {
    embedding: EmbeddingConfig,
    qdrant: QdrantDenseCollectionConfig,
    embedding_client: EmbeddingClient,
    qdrant_client: QdrantDenseSearchClient,
}

impl QdrantCardsCollectionDense {
    pub fn from_settings(
        collection_settings: &CollectionRetrievalSettings,
        embedding_model: &EmbeddingModelSettings,
        qdrant_url: &str,
    ) -> Result<Self, CardsCollectionError> {
        let embedding = embedding_config_from_settings(embedding_model)
            .map_err(|_| CardsCollectionError::InvalidRequest("invalid embedding settings"))?;
        let qdrant = dense_collection_config_from_settings(collection_settings, qdrant_url)
            .map_err(|_| CardsCollectionError::InvalidRequest("invalid dense collection settings"))?;

        Self::new(embedding, qdrant, collection_settings.embedding_retry.clone())
    }

    pub fn new(
        embedding: EmbeddingConfig,
        qdrant: QdrantDenseCollectionConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, CardsCollectionError> {
        let embedding_client = EmbeddingClient::new(embedding.clone(), retry_policy.clone())
            .map_err(CardsCollectionError::Embedding)?;
        let qdrant_client =
            QdrantDenseSearchClient::new(qdrant.qdrant_base_url.clone(), retry_policy)
                .map_err(CardsCollectionError::QdrantDense)?;

        Ok(Self { embedding, qdrant, embedding_client, qdrant_client })
    }
}

#[async_trait]
impl CardsCollection for QdrantCardsCollectionDense {
    async fn search(
        &self,
        request: &CardSearchRequest,
    ) -> Result<CardSearchResult, CardsCollectionError> {
        validate_request(request)?;

        let emb = self
            .embedding_client
            .embed(&request.user_query)
            .await
            .map_err(CardsCollectionError::Embedding)?;

        if emb.values.len() != self.embedding.embedding_dimension {
            return Err(CardsCollectionError::IncorrectEmbeddingShape);
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
            .map(|h| map_hit_to_card_hit(h.score, &h.payload.fields))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CardSearchResult { hits })
    }
}

// ─── Hybrid implementation ────────────────────────────────────────────────────

pub struct QdrantCardsCollectionHybrid {
    embedding: EmbeddingConfig,
    qdrant: QdrantHybridCollectionConfig,
    sparse: SparseStrategyConfig,
    embedding_client: EmbeddingClient,
    qdrant_client: QdrantHybridSearchClient,
    sparse_vocabulary: SparseVocabularyArtifact,
    bm25_term_stats: Option<Bm25TermStatsArtifact>,
    tokenizer: HfTokenizer,
}

impl QdrantCardsCollectionHybrid {
    pub async fn from_settings(
        collection_settings: &CollectionRetrievalSettings,
        embedding_model: &EmbeddingModelSettings,
        qdrant_url: &str,
    ) -> Result<Self, CardsCollectionError> {
        let embedding = embedding_config_from_settings(embedding_model)
            .map_err(|_| CardsCollectionError::InvalidRequest("invalid embedding settings"))?;
        let (qdrant, sparse) =
            hybrid_collection_config_from_settings(collection_settings, qdrant_url).map_err(
                |_| CardsCollectionError::InvalidRequest("invalid hybrid collection settings"),
            )?;

        Self::new(embedding, qdrant, sparse, collection_settings.embedding_retry.clone()).await
    }

    pub async fn new(
        embedding: EmbeddingConfig,
        qdrant: QdrantHybridCollectionConfig,
        sparse: SparseStrategyConfig,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, CardsCollectionError> {
        let loaded = sparse_preparation::load_sparse_artifacts(&sparse, &qdrant.collection_name.0)
            .await
            .map_err(CardsCollectionError::InvalidRequest)?;

        let embedding_client = EmbeddingClient::new(embedding.clone(), retry_policy.clone())
            .map_err(CardsCollectionError::Embedding)?;
        let qdrant_client =
            QdrantHybridSearchClient::new(qdrant.qdrant_base_url.clone(), retry_policy)
                .map_err(CardsCollectionError::QdrantHybrid)?;

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
impl CardsCollection for QdrantCardsCollectionHybrid {
    async fn search(
        &self,
        request: &CardSearchRequest,
    ) -> Result<CardSearchResult, CardsCollectionError> {
        validate_request(request)?;

        let emb = self
            .embedding_client
            .embed(&request.user_query)
            .await
            .map_err(CardsCollectionError::Embedding)?;

        if emb.values.len() != self.embedding.embedding_dimension {
            return Err(CardsCollectionError::IncorrectEmbeddingShape);
        }

        let sparse = sparse_preparation::build_sparse_vector(
            &request.user_query.0,
            &self.tokenizer,
            &self.sparse_vocabulary,
            self.bm25_term_stats.as_ref(),
            &self.sparse,
        )
        .map_err(CardsCollectionError::QueryPreparation)?;

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
            .map(|h| map_hit_to_card_hit(h.score, &h.payload.fields))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CardSearchResult { hits })
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn validate_request(req: &CardSearchRequest) -> Result<(), CardsCollectionError> {
    if req.user_query.0.trim().is_empty() {
        return Err(CardsCollectionError::InvalidRequest("user_query must not be empty"));
    }
    if req.limit == 0 {
        return Err(CardsCollectionError::InvalidRequest("limit must be > 0"));
    }
    if !req.score_threshold.is_finite() || req.score_threshold < 0.0 {
        return Err(CardsCollectionError::InvalidRequest(
            "score_threshold must be finite and non-negative",
        ));
    }
    Ok(())
}

fn map_hit_to_card_hit(
    score: f32,
    fields: &std::collections::BTreeMap<String, QdrantPayloadValue>,
) -> Result<CardSearchHit, CardsCollectionError> {
    let case_id = match fields.get("doc_id") {
        Some(QdrantPayloadValue::String(s)) if !s.is_empty() => s.clone(),
        Some(QdrantPayloadValue::String(_)) => {
            return Err(CardsCollectionError::PayloadMapping("doc_id is empty"))
        }
        _ => return Err(CardsCollectionError::PayloadMapping("missing or invalid doc_id")),
    };
    Ok(CardSearchHit { case_id, score })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BagOfWordsSettings, CollectionRetrievalSettings, CollectionSettings, DenseCollectionSettings,
        EmbeddingModelSettings, HybridCollectionSettings, SparsePreprocessingSettings,
        SparseSettings, SparseStrategySettings, TokenizerSettings,
    };
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
            model_name: "model".into(),
            embedding_dimension: 2,
        }
    }

    fn dense_collection_config(qdrant_url: &str) -> QdrantDenseCollectionConfig {
        QdrantDenseCollectionConfig {
            qdrant_base_url: url::Url::parse(qdrant_url).unwrap(),
            collection_name: QdrantCollectionName("cards".into()),
            vector_name: None,
        }
    }

    fn hybrid_collection_config(qdrant_url: &str) -> QdrantHybridCollectionConfig {
        QdrantHybridCollectionConfig {
            qdrant_base_url: url::Url::parse(qdrant_url).unwrap(),
            collection_name: QdrantCollectionName("cards".into()),
            vector_name: QdrantVectorName("dense".into()),
            sparse_vector_name: QdrantVectorName("sparse".into()),
        }
    }

    fn embedding_model_settings(url: &str) -> EmbeddingModelSettings {
        EmbeddingModelSettings {
            url: url.to_string(),
            name: "embed-model".into(),
            dimension: 2,
        }
    }

    fn dense_collection_settings() -> CollectionRetrievalSettings {
        CollectionRetrievalSettings {
            top_k: 5,
            score_threshold: 0.2,
            max_alternatives: 3,
            embedding_retry: policy(),
            qdrant_retry: policy(),
            collection: CollectionSettings::Dense(DenseCollectionSettings {
                name: "cards_dense".into(),
                vector_name: "dense".into(),
                corpus_version: "v1".into(),
            }),
        }
    }

    fn hybrid_collection_settings(vocab_path: &str) -> CollectionRetrievalSettings {
        CollectionRetrievalSettings {
            top_k: 5,
            score_threshold: 0.2,
            max_alternatives: 3,
            embedding_retry: policy(),
            qdrant_retry: policy(),
            collection: CollectionSettings::Hybrid(HybridCollectionSettings {
                dense_vector_name: "dense".into(),
                sparse_vector_name: "sparse".into(),
                corpus_version: "v1".into(),
                sparse: SparseSettings {
                    tokenizer: TokenizerSettings {
                        library: "tokenizers".into(),
                        source: "Qwen/Qwen3-Embedding-0.6B".into(),
                    },
                    preprocessing: SparsePreprocessingSettings {
                        kind: "basic_word_v1".into(),
                        lowercase: true,
                        min_token_length: 2,
                    },
                    strategy: SparseStrategySettings::BagOfWords(BagOfWordsSettings {
                        name: "cards_bow".into(),
                        query: "binary_presence".into(),
                        sparse_vocabulary_path: vocab_path.into(),
                    }),
                },
            }),
        }
    }

    fn valid_request() -> CardSearchRequest {
        CardSearchRequest {
            user_query: NormalizedUserQuery("service down".into()),
            limit: 5,
            score_threshold: 0.2,
        }
    }

    fn vocab_json(vocab_name: &str, col: &str, tokenizer_source: &str) -> String {
        serde_json::json!({
            "vocabulary_name": vocab_name,
            "collection_name": col,
            "text_processing": {"lowercase": true, "min_token_length": 2},
            "tokenizer": {"library": "tokenizers", "source": tokenizer_source},
            "created_at": "2024-01-01T00:00:00Z",
            "tokens": [
                {"token": "service", "token_id": 0},
                {"token": "down", "token_id": 1}
            ]
        })
        .to_string()
    }

    fn emb_resp() -> String {
        serde_json::json!({"embeddings": [[0.1, 0.2]]}).to_string()
    }

    fn qdrant_resp(case_id: &str) -> String {
        serde_json::json!({
            "result": {"points": [{"score": 0.8, "payload": {"doc_id": case_id}}]}
        })
        .to_string()
    }

    // ── Request validation (dense) ────────────────────────────────────────────

    #[tokio::test]
    async fn dense_validates_before_embedding() {
        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantCardsCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_collection_config(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let req = CardSearchRequest {
            user_query: NormalizedUserQuery("   ".into()),
            limit: 5,
            score_threshold: 0.2,
        };
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            CardsCollectionError::InvalidRequest(_)
        ));
        assert!(emb_server.take_bodies().await.is_empty());
    }

    #[test]
    fn dense_from_settings_constructs_client() {
        let client = QdrantCardsCollectionDense::from_settings(
            &dense_collection_settings(),
            &embedding_model_settings("http://localhost:11434/"),
            "http://localhost:6333/",
        )
        .unwrap();

        assert_eq!(client.qdrant.collection_name.0, "cards_dense");
        assert_eq!(client.embedding.model_name, "embed-model");
    }

    #[test]
    fn dense_from_settings_rejects_hybrid_variant() {
        let dir = TempArtifactDir::new();
        let vocab_path = dir.write_json(
            "vocab.json",
            &vocab_json("cards__sparse_vocabulary", "cards", "test/model"),
        );
        let err = QdrantCardsCollectionDense::from_settings(
            &hybrid_collection_settings(vocab_path.to_str().unwrap()),
            &embedding_model_settings("http://localhost:11434/"),
            "http://localhost:6333/",
        )
        .err()
        .expect("should fail");

        assert!(matches!(err, CardsCollectionError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn empty_user_query_returns_invalid_request() {
        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantCardsCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_collection_config(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let req = CardSearchRequest {
            user_query: NormalizedUserQuery("".into()),
            limit: 5,
            score_threshold: 0.2,
        };
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            CardsCollectionError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn zero_limit_returns_invalid_request() {
        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantCardsCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_collection_config(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let req = CardSearchRequest {
            user_query: NormalizedUserQuery("q".into()),
            limit: 0,
            score_threshold: 0.2,
        };
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            CardsCollectionError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn invalid_threshold_returns_invalid_request() {
        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantCardsCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_collection_config(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let req = CardSearchRequest {
            user_query: NormalizedUserQuery("q".into()),
            limit: 5,
            score_threshold: f32::NAN,
        };
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            CardsCollectionError::InvalidRequest(_)
        ));
    }

    #[tokio::test]
    async fn embedding_shape_mismatch_fails_before_qdrant() {
        // Embedding server returns 1 value but dimension is 2.
        let emb_resp = serde_json::json!({"embeddings": [[0.1]]}).to_string();
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp)]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;

        let mut emb_cfg = emb_config(&emb_server.base_url());
        emb_cfg.embedding_dimension = 2;
        // Override dimension so embedding client accepts the response, but collection rejects it
        emb_cfg.embedding_dimension = 1; // make client accept dim=1
        // Now set collection config expecting dim=2
        let emb_resp2 = serde_json::json!({"embeddings": [[0.1]]}).to_string();
        let emb_server2 = MockHttpServer::new(vec![MockResponse::ok(emb_resp2)]).await;

        // Use a client whose internal embedding client accepts dim=1 but collection expects dim=2
        // Simulate: embedding client returns 1 value, collection configured for 2
        let col_cfg = dense_collection_config(&qdrant_server.base_url());
        let client = QdrantCardsCollectionDense {
            embedding: EmbeddingConfig {
                base_url: url::Url::parse(&emb_server2.base_url()).unwrap(),
                model_name: "m".into(),
                embedding_dimension: 2,
            },
            qdrant: col_cfg,
            embedding_client: EmbeddingClient::new(
                EmbeddingConfig {
                    base_url: url::Url::parse(&emb_server2.base_url()).unwrap(),
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
        assert!(matches!(err, CardsCollectionError::IncorrectEmbeddingShape));
        assert!(qdrant_server.take_bodies().await.is_empty());
    }

    #[tokio::test]
    async fn dense_builds_request_with_no_filter() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(qdrant_resp("card1"))]).await;
        let client = QdrantCardsCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_collection_config(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        client.search(&valid_request()).await.unwrap();
        let bodies = qdrant_server.take_bodies().await;
        let body: serde_json::Value = serde_json::from_slice(&bodies[0]).unwrap();
        assert!(body.get("filter").is_none());
    }

    #[tokio::test]
    async fn dense_transport_error_wrapped_as_qdrant_dense() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server =
            MockHttpServer::new(vec![MockResponse::status(500, b"err".to_vec())]).await;
        let client = QdrantCardsCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_collection_config(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let err = client.search(&valid_request()).await.unwrap_err();
        assert!(matches!(err, CardsCollectionError::QdrantDense(_)));
    }

    #[tokio::test]
    async fn embedding_error_wrapped_correctly() {
        let emb_server =
            MockHttpServer::new(vec![MockResponse::status(503, b"err".to_vec())]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;
        let client = QdrantCardsCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_collection_config(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let err = client.search(&valid_request()).await.unwrap_err();
        assert!(matches!(err, CardsCollectionError::Embedding(_)));
    }

    #[tokio::test]
    async fn valid_payload_maps_to_card_hit() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(qdrant_resp("case_abc"))]).await;
        let client = QdrantCardsCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_collection_config(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let result = client.search(&valid_request()).await.unwrap();
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].case_id, "case_abc");
    }

    #[tokio::test]
    async fn empty_doc_id_returns_payload_mapping_error() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(qdrant_resp(""))]).await;
        let client = QdrantCardsCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_collection_config(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(matches!(
            client.search(&valid_request()).await.unwrap_err(),
            CardsCollectionError::PayloadMapping(_)
        ));
    }

    #[tokio::test]
    async fn extra_payload_fields_ignored() {
        let resp = serde_json::json!({
            "result": {"points": [{"score": 0.7, "payload": {"doc_id": "case1", "extra": "ignored"}}]}
        })
        .to_string();
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let client = QdrantCardsCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_collection_config(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let result = client.search(&valid_request()).await.unwrap();
        assert_eq!(result.hits[0].case_id, "case1");
    }

    #[tokio::test]
    async fn one_invalid_hit_fails_whole_mapping() {
        let resp = serde_json::json!({
            "result": {"points": [
                {"score": 0.9, "payload": {"doc_id": "ok"}},
                {"score": 0.7, "payload": {"doc_id": ""}}
            ]}
        })
        .to_string();
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::ok(resp)]).await;
        let client = QdrantCardsCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_collection_config(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        assert!(matches!(
            client.search(&valid_request()).await.unwrap_err(),
            CardsCollectionError::PayloadMapping(_)
        ));
    }

    #[tokio::test]
    async fn empty_transport_result_returns_empty_search_result() {
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server =
            MockHttpServer::new(vec![MockResponse::ok(
                serde_json::json!({"result": {"points": []}}).to_string(),
            )])
            .await;
        let client = QdrantCardsCollectionDense::new(
            emb_config(&emb_server.base_url()),
            dense_collection_config(&qdrant_server.base_url()),
            policy(),
        )
        .unwrap();
        let result = client.search(&valid_request()).await.unwrap();
        assert!(result.hits.is_empty());
    }

    // ── Hybrid constructor artifact validation ────────────────────────────────

    #[tokio::test]
    async fn hybrid_constructor_fails_when_vocabulary_absent() {
        let result = QdrantCardsCollectionHybrid::new(
            EmbeddingConfig {
                base_url: url::Url::parse("http://localhost:11434/").unwrap(),
                model_name: "m".into(),
                embedding_dimension: 2,
            },
            hybrid_collection_config("http://localhost:6333/"),
            SparseStrategyConfig::BagOfWords {
                sparse_vocabulary_path: "/nonexistent/vocab.json".into(),
            },
            policy(),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn hybrid_from_settings_constructs_client() {
        let dir = TempArtifactDir::new();
        populate_tokenizer_cache("test/model");
        let vocab_path = dir.write_json(
            "vocab.json",
            &vocab_json("cards__sparse_vocabulary", "cards", "test/model"),
        );
        let client = QdrantCardsCollectionHybrid::from_settings(
            &hybrid_collection_settings(vocab_path.to_str().unwrap()),
            &embedding_model_settings("http://localhost:11434/"),
            "http://localhost:6333/",
        )
        .await
        .unwrap();

        assert_eq!(client.qdrant.collection_name.0, "cards_bow");
    }

    #[tokio::test]
    async fn hybrid_validates_before_embedding() {
        let dir = TempArtifactDir::new();
        populate_tokenizer_cache("test/model");
        let vocab_path = dir.write_json(
            "vocab.json",
            &vocab_json("cards__sparse_vocabulary", "cards", "test/model"),
        );

        let emb_server = MockHttpServer::new(vec![]).await;
        let qdrant_server = MockHttpServer::new(vec![]).await;

        let client = QdrantCardsCollectionHybrid::new(
            emb_config(&emb_server.base_url()),
            hybrid_collection_config(&qdrant_server.base_url()),
            SparseStrategyConfig::BagOfWords {
                sparse_vocabulary_path: vocab_path.to_str().unwrap().to_string(),
            },
            policy(),
        )
        .await
        .unwrap();

        let req = CardSearchRequest {
            user_query: NormalizedUserQuery("".into()),
            limit: 5,
            score_threshold: 0.2,
        };
        assert!(matches!(
            client.search(&req).await.unwrap_err(),
            CardsCollectionError::InvalidRequest(_)
        ));
        assert!(emb_server.take_bodies().await.is_empty());
    }

    #[tokio::test]
    async fn hybrid_transport_error_wrapped_as_qdrant_hybrid() {
        let dir = TempArtifactDir::new();
        populate_tokenizer_cache("test/model");
        let vocab_path = dir.write_json(
            "vocab.json",
            &vocab_json("cards__sparse_vocabulary", "cards", "test/model"),
        );
        let emb_server = MockHttpServer::new(vec![MockResponse::ok(emb_resp())]).await;
        let qdrant_server = MockHttpServer::new(vec![MockResponse::status(500, b"err".to_vec())]).await;

        let client = QdrantCardsCollectionHybrid::new(
            emb_config(&emb_server.base_url()),
            hybrid_collection_config(&qdrant_server.base_url()),
            SparseStrategyConfig::BagOfWords {
                sparse_vocabulary_path: vocab_path.to_str().unwrap().to_string(),
            },
            policy(),
        )
        .await
        .unwrap();

        let err = client.search(&valid_request()).await.unwrap_err();
        assert!(matches!(err, CardsCollectionError::QdrantHybrid(_)));
    }
}
