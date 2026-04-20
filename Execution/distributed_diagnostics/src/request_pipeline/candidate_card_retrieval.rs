use std::sync::Arc;

use crate::api_clients::qdrant::cards_collection::{CardsCollection, CardsCollectionError, CardSearchRequest};
use crate::api_clients::qdrant::shared_types::NormalizedUserQuery;
use crate::shared_types::{CandidateCard, CandidateCardRetrievalOutput, NormalizedUserRequest};

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CandidateCardRetrievalError {
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(&'static str),
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
            return Err(CandidateCardRetrievalError::InvalidConfiguration("top_k must be greater than 0"));
        }
        if top_k < 1 + max_alternatives {
            return Err(CandidateCardRetrievalError::InvalidConfiguration(
                "top_k must be at least 1 + max_alternatives",
            ));
        }
        if score_threshold < 0.0 {
            return Err(CandidateCardRetrievalError::InvalidConfiguration(
                "score_threshold must not be negative",
            ));
        }
        if score_threshold.is_nan() {
            return Err(CandidateCardRetrievalError::InvalidConfiguration(
                "score_threshold must not be NaN",
            ));
        }
        if score_threshold.is_infinite() {
            return Err(CandidateCardRetrievalError::InvalidConfiguration(
                "score_threshold must not be infinite",
            ));
        }
        if max_alternatives > 2 {
            return Err(CandidateCardRetrievalError::InvalidConfiguration(
                "max_alternatives must not exceed 2",
            ));
        }

        Ok(Self { cards_collection, top_k, max_alternatives, score_threshold })
    }

    pub async fn retrieve(
        &self,
        request: &NormalizedUserRequest,
    ) -> Result<CandidateCardRetrievalOutput, CandidateCardRetrievalError> {
        let search_request = CardSearchRequest {
            user_query: NormalizedUserQuery(request.query.clone()),
            limit: self.top_k,
            score_threshold: self.score_threshold,
        };

        let result = self.cards_collection.search(&search_request).await?;

        if result.hits.is_empty() {
            return Ok(CandidateCardRetrievalOutput { primary: None, alternatives: vec![] });
        }

        let mut hits = result.hits.into_iter();

        let primary = hits.next().map(|h| CandidateCard {
            case_id: h.case_id,
            score: h.score,
        });

        let alternatives = hits
            .take(self.max_alternatives)
            .map(|h| CandidateCard { case_id: h.case_id, score: h.score })
            .collect();

        Ok(CandidateCardRetrievalOutput { primary, alternatives })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_clients::qdrant::cards_collection::{
        CardSearchHit, CardSearchResult, CardsCollectionError,
    };
    use crate::config::{
        CollectionRetrievalSettings, CollectionSettings, DenseCollectionSettings,
    };
    use crate::utils::retry::{RetryBackoffKind, RetryPolicyConfig};
    use async_trait::async_trait;
    use std::sync::Mutex;

    // ─── Helpers ──────────────────────────────────────────────────────────────

    fn settings(top_k: usize, max_alternatives: usize, score_threshold: f32) -> CollectionRetrievalSettings {
        CollectionRetrievalSettings {
            top_k,
            score_threshold,
            max_alternatives,
            embedding_retry: RetryPolicyConfig { max_attempts: 1, backoff: RetryBackoffKind::Exponential },
            qdrant_retry: RetryPolicyConfig { max_attempts: 1, backoff: RetryBackoffKind::Exponential },
            collection: CollectionSettings::Dense(DenseCollectionSettings {
                name: "cards".to_string(),
                vector_name: "v".to_string(),
                corpus_version: "1".to_string(),
            }),
        }
    }

    fn request(query: &str) -> NormalizedUserRequest {
        NormalizedUserRequest { query: query.to_string(), input_token_count: 5 }
    }

    fn hit(case_id: &str, score: f32) -> CardSearchHit {
        CardSearchHit { case_id: case_id.to_string(), score }
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
            Arc::new(Self { response: Err(err), captured: Mutex::new(None) })
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
                Err(_e) => Err(CardsCollectionError::InvalidRequest("mock error")),
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
        let alt_ids: Vec<&str> = out.alternatives.iter().map(|c| c.case_id.as_str()).collect();
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
        let mock = MockCardsCollection::returning(vec![
            hit("card-1", 0.9),
            hit("card-2", 0.8),
        ]);
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
        let mock = MockCardsCollection::failing(CardsCollectionError::InvalidRequest("boom"));
        let sut = CandidateCardRetrieval::new(settings(3, 2, 0.5), mock).unwrap();

        let err = sut.retrieve(&request("q")).await.unwrap_err();
        assert!(matches!(err, CandidateCardRetrievalError::Collection(_)));
    }
}
