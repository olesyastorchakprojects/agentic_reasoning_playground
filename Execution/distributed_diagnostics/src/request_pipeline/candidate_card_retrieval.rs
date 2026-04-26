use std::sync::Arc;

use crate::api_clients::qdrant::cards_collection::{
    CardSearchRequest, CardsCollection, CardsCollectionError,
};
use crate::api_clients::qdrant::shared_types::NormalizedUserQuery;
use crate::request_pipeline::context::Context;
use crate::shared_types::{CandidateCard, CandidateCardRetrievalOutput, NormalizedUserRequest};
use tracing::{info_span, field, Instrument};

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum CandidateCardRetrievalError {
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),
    #[error("cards collection error: {0}")]
    Collection(#[from] CardsCollectionError),
}

// ─── Public struct ────────────────────────────────────────────────────────────

pub struct CandidateCardRetrieval {
    cards_collection: Arc<dyn CardsCollection>,
    top_k: usize,
    max_alternatives: usize,
    score_threshold: f32,
}

impl std::fmt::Debug for CandidateCardRetrieval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CandidateCardRetrieval")
            .field("top_k", &self.top_k)
            .field("max_alternatives", &self.max_alternatives)
            .field("score_threshold", &self.score_threshold)
            .finish_non_exhaustive()
    }
}

impl CandidateCardRetrieval {
    pub fn new(
        settings: crate::config::CollectionRetrievalSettings,
        cards_collection: Arc<dyn CardsCollection>,
    ) -> Result<Self, CandidateCardRetrievalError> {
        let top_k = settings.top_k;
        let max_alternatives = settings.max_alternatives;
        let score_threshold = settings.score_threshold;

        if top_k == 0 {
            return Err(CandidateCardRetrievalError::InvalidConfiguration(
                "top_k must be greater than 0".to_string(),
            ));
        }
        if top_k < 1 + max_alternatives {
            return Err(CandidateCardRetrievalError::InvalidConfiguration(
                "top_k must be at least 1 + max_alternatives".to_string(),
            ));
        }
        if score_threshold < 0.0 {
            return Err(CandidateCardRetrievalError::InvalidConfiguration(
                "score_threshold must not be negative".to_string(),
            ));
        }
        if score_threshold.is_nan() {
            return Err(CandidateCardRetrievalError::InvalidConfiguration(
                "score_threshold must not be NaN".to_string(),
            ));
        }
        if score_threshold.is_infinite() {
            return Err(CandidateCardRetrievalError::InvalidConfiguration(
                "score_threshold must not be infinite".to_string(),
            ));
        }
        if max_alternatives > 2 {
            return Err(CandidateCardRetrievalError::InvalidConfiguration(
                "max_alternatives must not exceed 2".to_string(),
            ));
        }

        Ok(Self {
            cards_collection,
            top_k,
            max_alternatives,
            score_threshold,
        })
    }

    pub async fn retrieve(
        &self,
        request: &NormalizedUserRequest,
    ) -> Result<CandidateCardRetrievalOutput, CandidateCardRetrievalError> {
        self.retrieve_with_context(request, &Context::noop()).await
    }

    pub async fn retrieve_with_context(
        &self,
        request: &NormalizedUserRequest,
        context: &Context,
    ) -> Result<CandidateCardRetrievalOutput, CandidateCardRetrievalError> {
        let query = request.query.clone();
        let oi_span =
            crate::observability::oi_retriever_candidate_cards_span(&context.open_inference.root_span);
        let oi_input_json = serde_json::json!({
            "normalized_query": request.query,
            "top_k": self.top_k,
            "score_threshold": self.score_threshold,
            "max_alternatives": self.max_alternatives
        })
        .to_string();
        oi_span.record("input.value", oi_input_json.as_str());
        oi_span.record("input.mime_type", "application/json");

        let span = info_span!(
            "request_pipeline.candidate_card_retrieval",
            module.name = "candidate_card_retrieval",
            query.normalized = %query,
            retrieval.collection = "cards",
            retrieval.top_k = self.top_k,
            retrieval.score_threshold = self.score_threshold,
            retrieval.max_alternatives = self.max_alternatives,
            retrieval.request_limit = self.top_k,
            retrieval.hits_count = field::Empty,
            retrieval.selected_total_count = field::Empty,
            retrieval.scores = field::Empty,
            candidate.primary.present = field::Empty,
            candidate.primary.case_id = field::Empty,
            candidate.primary.score = field::Empty,
            candidate.alternatives.count = field::Empty,
            candidate.output.total_count = field::Empty,
            module.outcome = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        self.retrieve_instrumented(request, &oi_span)
            .instrument(span)
            .await
    }

    async fn retrieve_instrumented(
        &self,
        request: &NormalizedUserRequest,
        oi_span: &tracing::Span,
    ) -> Result<CandidateCardRetrievalOutput, CandidateCardRetrievalError> {
        let search_request = CardSearchRequest {
            user_query: NormalizedUserQuery(request.query.clone()),
            limit: self.top_k,
            score_threshold: self.score_threshold,
        };

        let qdrant_span = info_span!(
            "qdrant.cards.search",
            qdrant.collection = "cards",
            qdrant.operation = "search",
            retrieval.limit = self.top_k,
            retrieval.score_threshold = self.score_threshold,
            retrieval.hits_count = field::Empty,
            retrieval.scores = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        let result = {
            async {
                match self.cards_collection.search(&search_request).await {
                    Ok(r) => {
                        let scores: Vec<f32> = r.hits.iter().map(|h| h.score).collect();
                        let hits_count = r.hits.len();
                        tracing::Span::current().record("retrieval.hits_count", hits_count);
                        tracing::Span::current().record("retrieval.scores", format!("{:?}", scores));
                        tracing::Span::current().record("status", "ok");
                        Ok(r)
                    }
                    Err(e) => {
                        crate::observability::record_error(
                            oi_span,
                            "CandidateCardRetrieval.Collection",
                            &format!("Qdrant search failed: {}", e),
                        );
                        tracing::Span::current().record("status", "error");
                        tracing::Span::current().record("error.type", "CandidateCardRetrieval.Collection");
                        tracing::Span::current()
                            .record("error.message", format!("Qdrant search failed: {}", e));
                        Err(e)
                    }
                }
            }
            .instrument(qdrant_span)
            .await
        };

        let result = match result {
            Ok(r) => r,
            Err(e) => {
                crate::observability::record_error(
                    oi_span,
                    "CandidateCardRetrieval.Collection",
                    &format!("Qdrant search failed: {}", e),
                );
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                tracing::Span::current().record("error.type", "CandidateCardRetrieval.Collection");
                tracing::Span::current()
                    .record("error.message", format!("Qdrant search failed: {}", e));
                return Err(CandidateCardRetrievalError::Collection(e));
            }
        };

        if result.hits.is_empty() {
            tracing::Span::current().record("retrieval.hits_count", 0);
            tracing::Span::current().record("retrieval.scores", "[]");
            tracing::Span::current().record("candidate.primary.present", false);
            tracing::Span::current().record("candidate.alternatives.count", 0);
            tracing::Span::current().record("candidate.output.total_count", 0);
            tracing::Span::current().record("retrieval.selected_total_count", 0);
            tracing::event!(
                tracing::Level::INFO,
                event.name = "candidate_alternative_case_ids",
                candidate.alternatives.case_ids = "[]"
            );
            oi_span.record("output.value", r#"{"primary":null,"alternatives":[]}"#);
            oi_span.record("output.mime_type", "application/json");
            oi_span.record("status", "ok");
            tracing::Span::current().record("module.outcome", "success");
            tracing::Span::current().record("status", "ok");

            return Ok(CandidateCardRetrievalOutput {
                primary: None,
                alternatives: vec![],
            });
        }

        let scores: Vec<f32> = result.hits.iter().map(|h| h.score).collect();
        let hits_count = result.hits.len();
        tracing::Span::current().record("retrieval.hits_count", hits_count);
        tracing::Span::current().record("retrieval.scores", format!("{:?}", scores));

        let mut hits = result.hits.into_iter();

        let primary = hits.next().map(|h| CandidateCard {
            case_id: h.case_id,
            score: h.score,
        });

        let alternatives: Vec<CandidateCard> = hits
            .take(self.max_alternatives)
            .map(|h| CandidateCard {
                case_id: h.case_id,
                score: h.score,
            })
            .collect();

        let primary_present = primary.is_some();
        let primary_case_id = primary.as_ref().map(|p| p.case_id.as_str()).unwrap_or("");
        let primary_score = primary.as_ref().map(|p| p.score).unwrap_or(0.0);
        let alt_case_ids: Vec<&str> = alternatives.iter().map(|a| a.case_id.as_str()).collect();
        let alt_count = alternatives.len();
        let total_count = (if primary_present { 1 } else { 0 }) + alt_count;

        tracing::Span::current().record("candidate.primary.present", primary_present);
        if primary_present {
            tracing::Span::current().record("candidate.primary.case_id", primary_case_id);
            tracing::Span::current().record("candidate.primary.score", primary_score);
        }
        tracing::Span::current().record("candidate.alternatives.count", alt_count);
        tracing::event!(
            tracing::Level::INFO,
            event.name = "candidate_alternative_case_ids",
            candidate.alternatives.case_ids = %serde_json::to_string(&alt_case_ids)
                .unwrap_or_else(|_| "[]".to_string())
        );
        tracing::Span::current().record("candidate.output.total_count", total_count);
        tracing::Span::current().record("retrieval.selected_total_count", total_count);
        let oi_output_json = serde_json::json!({
            "primary": primary.as_ref().map(|p| {
                serde_json::json!({
                    "document.id": p.case_id,
                    "document.score": p.score,
                    "role": "primary"
                })
            }),
            "alternatives": alternatives.iter().map(|a| {
                serde_json::json!({
                    "document.id": a.case_id,
                    "document.score": a.score,
                    "role": "alternative"
                })
            }).collect::<Vec<_>>()
        })
        .to_string();
        oi_span.record("output.value", oi_output_json.as_str());
        oi_span.record("output.mime_type", "application/json");
        oi_span.record("status", "ok");
        tracing::Span::current().record("module.outcome", "success");
        tracing::Span::current().record("status", "ok");

        Ok(CandidateCardRetrievalOutput {
            primary,
            alternatives,
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_clients::qdrant::cards_collection::{
        CardSearchHit, CardSearchResult, CardsCollectionError,
    };
    use crate::config::{CollectionRetrievalSettings, CollectionSettings, DenseCollectionSettings};
    use crate::utils::retry::{RetryBackoffKind, RetryPolicyConfig};
    use async_trait::async_trait;
    use std::sync::Mutex;

    // ─── Helpers ──────────────────────────────────────────────────────────────

    fn settings(
        top_k: usize,
        max_alternatives: usize,
        score_threshold: f32,
    ) -> CollectionRetrievalSettings {
        CollectionRetrievalSettings {
            top_k,
            score_threshold,
            max_alternatives,
            embedding_retry: RetryPolicyConfig {
                max_attempts: 1,
                backoff: RetryBackoffKind::Exponential,
            },
            qdrant_retry: RetryPolicyConfig {
                max_attempts: 1,
                backoff: RetryBackoffKind::Exponential,
            },
            collection: CollectionSettings::Dense(DenseCollectionSettings {
                name: "cards".to_string(),
                vector_name: "v".to_string(),
                corpus_version: "1".to_string(),
            }),
        }
    }

    fn request(query: &str) -> NormalizedUserRequest {
        NormalizedUserRequest {
            query: query.to_string(),
            input_token_count: 5,
        }
    }

    fn hit(case_id: &str, score: f32) -> CardSearchHit {
        CardSearchHit {
            case_id: case_id.to_string(),
            score,
        }
    }

    // ─── Mock ─────────────────────────────────────────────────────────────────

    struct MockCardsCollection {
        response: Result<CardSearchResult, CardsCollectionError>,
        captured: Mutex<Option<CardSearchRequest>>,
    }

    impl MockCardsCollection {
        fn returning(hits: Vec<CardSearchHit>) -> Arc<Self> {
            Arc::new(Self {
                response: Ok(CardSearchResult { hits }),
                captured: Mutex::new(None),
            })
        }

        fn failing(err: CardsCollectionError) -> Arc<Self> {
            Arc::new(Self {
                response: Err(err),
                captured: Mutex::new(None),
            })
        }

        fn captured(&self) -> Option<CardSearchRequest> {
            self.captured.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl CardsCollection for MockCardsCollection {
        async fn search(
            &self,
            request: &CardSearchRequest,
        ) -> Result<CardSearchResult, CardsCollectionError> {
            *self.captured.lock().unwrap() = Some(request.clone());
            match &self.response {
                Ok(r) => Ok(r.clone()),
                Err(_e) => Err(CardsCollectionError::InvalidRequest(
                    "mock error".to_string(),
                )),
            }
        }
    }

    // ─── Constructor validation ───────────────────────────────────────────────

    #[test]
    fn new_fails_when_top_k_is_zero() {
        let mock = MockCardsCollection::returning(vec![]);
        assert!(matches!(
            CandidateCardRetrieval::new(settings(0, 0, 0.5), mock),
            Err(CandidateCardRetrievalError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn new_fails_when_top_k_less_than_one_plus_max_alternatives() {
        let mock = MockCardsCollection::returning(vec![]);
        assert!(matches!(
            CandidateCardRetrieval::new(settings(2, 2, 0.5), mock),
            Err(CandidateCardRetrievalError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn new_fails_when_score_threshold_negative() {
        let mock = MockCardsCollection::returning(vec![]);
        assert!(matches!(
            CandidateCardRetrieval::new(settings(3, 2, -0.1), mock),
            Err(CandidateCardRetrievalError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn new_fails_when_score_threshold_nan() {
        let mock = MockCardsCollection::returning(vec![]);
        assert!(matches!(
            CandidateCardRetrieval::new(settings(3, 2, f32::NAN), mock),
            Err(CandidateCardRetrievalError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn new_fails_when_score_threshold_positive_infinity() {
        let mock = MockCardsCollection::returning(vec![]);
        assert!(matches!(
            CandidateCardRetrieval::new(settings(3, 2, f32::INFINITY), mock),
            Err(CandidateCardRetrievalError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn new_fails_when_score_threshold_negative_infinity() {
        let mock = MockCardsCollection::returning(vec![]);
        assert!(matches!(
            CandidateCardRetrieval::new(settings(3, 2, f32::NEG_INFINITY), mock),
            Err(CandidateCardRetrievalError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn new_fails_when_max_alternatives_exceeds_two() {
        let mock = MockCardsCollection::returning(vec![]);
        assert!(matches!(
            CandidateCardRetrieval::new(settings(4, 3, 0.5), mock),
            Err(CandidateCardRetrievalError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn new_succeeds_with_valid_settings() {
        let mock = MockCardsCollection::returning(vec![]);
        assert!(CandidateCardRetrieval::new(settings(3, 2, 0.5), mock).is_ok());
    }

    #[test]
    fn new_succeeds_when_max_alternatives_is_zero() {
        let mock = MockCardsCollection::returning(vec![]);
        assert!(CandidateCardRetrieval::new(settings(1, 0, 0.5), mock).is_ok());
    }

    // ─── Empty hits ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn empty_hits_returns_none_primary_and_empty_alternatives() {
        let mock = MockCardsCollection::returning(vec![]);
        let sut = CandidateCardRetrieval::new(settings(3, 2, 0.5), mock).unwrap();

        let out = sut.retrieve(&request("q")).await.unwrap();

        assert!(out.primary.is_none());
        assert!(out.alternatives.is_empty());
    }

    // ─── Partitioning ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn single_hit_becomes_primary_alternatives_empty() {
        let mock = MockCardsCollection::returning(vec![hit("card-1", 0.9)]);
        let sut = CandidateCardRetrieval::new(settings(3, 2, 0.5), mock).unwrap();

        let out = sut.retrieve(&request("q")).await.unwrap();

        assert_eq!(out.primary.unwrap().case_id, "card-1");
        assert!(out.alternatives.is_empty());
    }

    #[tokio::test]
    async fn first_hit_is_primary_rest_are_alternatives_in_order() {
        let mock = MockCardsCollection::returning(vec![
            hit("card-1", 0.9),
            hit("card-2", 0.8),
            hit("card-3", 0.7),
        ]);
        let sut = CandidateCardRetrieval::new(settings(3, 2, 0.5), mock).unwrap();

        let out = sut.retrieve(&request("q")).await.unwrap();

        assert_eq!(out.primary.unwrap().case_id, "card-1");
        let alt_ids: Vec<&str> = out
            .alternatives
            .iter()
            .map(|c| c.case_id.as_str())
            .collect();
        assert_eq!(alt_ids, vec!["card-2", "card-3"]);
    }

    #[tokio::test]
    async fn alternatives_capped_at_max_alternatives() {
        let mock = MockCardsCollection::returning(vec![
            hit("card-1", 0.9),
            hit("card-2", 0.8),
            hit("card-3", 0.7),
            hit("card-4", 0.6),
        ]);
        let sut = CandidateCardRetrieval::new(settings(4, 1, 0.5), mock).unwrap();

        let out = sut.retrieve(&request("q")).await.unwrap();

        assert_eq!(out.primary.unwrap().case_id, "card-1");
        assert_eq!(out.alternatives.len(), 1);
        assert_eq!(out.alternatives[0].case_id, "card-2");
    }

    #[tokio::test]
    async fn max_alternatives_zero_returns_only_primary() {
        let mock = MockCardsCollection::returning(vec![hit("card-1", 0.9), hit("card-2", 0.8)]);
        let sut = CandidateCardRetrieval::new(settings(2, 0, 0.5), mock).unwrap();

        let out = sut.retrieve(&request("q")).await.unwrap();

        assert_eq!(out.primary.unwrap().case_id, "card-1");
        assert!(out.alternatives.is_empty());
    }

    // ─── Request construction ─────────────────────────────────────────────────

    #[tokio::test]
    async fn request_passes_query_unchanged() {
        let mock = MockCardsCollection::returning(vec![]);
        let sut = CandidateCardRetrieval::new(settings(3, 2, 0.5), mock.clone()).unwrap();

        sut.retrieve(&request("exact query text")).await.unwrap();

        assert_eq!(mock.captured().unwrap().user_query.0, "exact query text");
    }

    #[tokio::test]
    async fn request_uses_top_k_as_limit() {
        let mock = MockCardsCollection::returning(vec![]);
        let sut = CandidateCardRetrieval::new(settings(7, 2, 0.5), mock.clone()).unwrap();

        sut.retrieve(&request("q")).await.unwrap();

        assert_eq!(mock.captured().unwrap().limit, 7);
    }

    #[tokio::test]
    async fn request_uses_score_threshold() {
        let mock = MockCardsCollection::returning(vec![]);
        let sut = CandidateCardRetrieval::new(settings(3, 2, 0.42), mock.clone()).unwrap();

        sut.retrieve(&request("q")).await.unwrap();

        assert_eq!(mock.captured().unwrap().score_threshold, 0.42);
    }

    // ─── Score passthrough ────────────────────────────────────────────────────

    #[tokio::test]
    async fn score_preserved_without_rounding() {
        let mock = MockCardsCollection::returning(vec![hit("card-1", 0.123456789)]);
        let sut = CandidateCardRetrieval::new(settings(3, 2, 0.0), mock).unwrap();

        let out = sut.retrieve(&request("q")).await.unwrap();

        assert_eq!(out.primary.unwrap().score, 0.123456789_f32);
    }

    // ─── Collection error ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn collection_error_wrapped_as_collection_variant() {
        let mock =
            MockCardsCollection::failing(CardsCollectionError::InvalidRequest("boom".to_string()));
        let sut = CandidateCardRetrieval::new(settings(3, 2, 0.5), mock).unwrap();

        let err = sut.retrieve(&request("q")).await.unwrap_err();
        assert!(matches!(err, CandidateCardRetrievalError::Collection(_)));
    }
}
