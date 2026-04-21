use std::sync::Arc;

use crate::api_clients::qdrant::shared_types::NormalizedUserQuery;
use crate::api_clients::qdrant::theory_chunks_collection::{
    TheoryChunkSearchRequest, TheoryChunksCollection, TheoryChunksCollectionError,
};
use crate::config::CollectionRetrievalSettings;
use crate::shared_types::{NormalizedUserRequest, TheoryEvidenceChunk, TheoryEvidenceRetrievalOutput};

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum TheoryEvidenceRetrievalError {
    #[error("invalid settings: {0}")]
    InvalidSettings(&'static str),
    #[error("theory chunks collection error: {0}")]
    Collection(TheoryChunksCollectionError),
}

// ─── Public struct ────────────────────────────────────────────────────────────

pub struct TheoryEvidenceRetrieval {
    collection: Arc<dyn TheoryChunksCollection + Send + Sync>,
    settings: CollectionRetrievalSettings,
}

impl std::fmt::Debug for TheoryEvidenceRetrieval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TheoryEvidenceRetrieval")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl TheoryEvidenceRetrieval {
    pub fn new(
        collection: Arc<dyn TheoryChunksCollection + Send + Sync>,
        settings: CollectionRetrievalSettings,
    ) -> Result<Self, TheoryEvidenceRetrievalError> {
        if settings.top_k == 0 {
            return Err(TheoryEvidenceRetrievalError::InvalidSettings(
                "top_k must be greater than 0",
            ));
        }
        if settings.score_threshold < 0.0 {
            return Err(TheoryEvidenceRetrievalError::InvalidSettings(
                "score_threshold must be non-negative",
            ));
        }
        if settings.score_threshold.is_nan() {
            return Err(TheoryEvidenceRetrievalError::InvalidSettings(
                "score_threshold must not be NaN",
            ));
        }
        if settings.score_threshold.is_infinite() {
            return Err(TheoryEvidenceRetrievalError::InvalidSettings(
                "score_threshold must not be infinite",
            ));
        }
        Ok(Self { collection, settings })
    }

    pub async fn retrieve(
        &self,
        request: &NormalizedUserRequest,
    ) -> Result<TheoryEvidenceRetrievalOutput, TheoryEvidenceRetrievalError> {
        let search_request = TheoryChunkSearchRequest {
            user_query: NormalizedUserQuery(request.query.clone()),
            limit: self.settings.top_k,
            score_threshold: self.settings.score_threshold,
        };

        let result = self
            .collection
            .search(&search_request)
            .await
            .map_err(TheoryEvidenceRetrievalError::Collection)?;

        let chunks = result
            .hits
            .into_iter()
            .map(|h| TheoryEvidenceChunk {
                chunk_id: h.chunk_id,
                score: h.score,
                text: h.text,
            })
            .collect();

        Ok(TheoryEvidenceRetrievalOutput { chunks })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_clients::qdrant::theory_chunks_collection::{
        TheoryChunkSearchHit, TheoryChunkSearchResult,
    };
    use crate::config::{CollectionSettings, DenseCollectionSettings};
    use crate::utils::retry::{RetryBackoffKind, RetryPolicyConfig};
    use async_trait::async_trait;
    use std::sync::Mutex;

    // ─── Helpers ──────────────────────────────────────────────────────────────

    fn settings(top_k: usize, score_threshold: f32) -> CollectionRetrievalSettings {
        CollectionRetrievalSettings {
            top_k,
            score_threshold,
            max_alternatives: 3,
            embedding_retry: RetryPolicyConfig {
                max_attempts: 1,
                backoff: RetryBackoffKind::Exponential,
            },
            qdrant_retry: RetryPolicyConfig {
                max_attempts: 1,
                backoff: RetryBackoffKind::Exponential,
            },
            collection: CollectionSettings::Dense(DenseCollectionSettings {
                name: "theory".to_string(),
                vector_name: "v".to_string(),
                corpus_version: "1".to_string(),
            }),
        }
    }

    fn request(query: &str) -> NormalizedUserRequest {
        NormalizedUserRequest { query: query.to_string(), input_token_count: 10 }
    }

    fn hit(chunk_id: &str, score: f32, text: &str) -> TheoryChunkSearchHit {
        TheoryChunkSearchHit {
            chunk_id: chunk_id.to_string(),
            score,
            text: text.to_string(),
        }
    }

    // ─── Mock ─────────────────────────────────────────────────────────────────

    struct MockCollection {
        responses: Mutex<Vec<Result<TheoryChunkSearchResult, TheoryChunksCollectionError>>>,
        captured: Mutex<Vec<TheoryChunkSearchRequest>>,
    }

    impl MockCollection {
        fn new(
            responses: Vec<Result<TheoryChunkSearchResult, TheoryChunksCollectionError>>,
        ) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses),
                captured: Mutex::new(vec![]),
            })
        }

        fn captured_requests(&self) -> Vec<TheoryChunkSearchRequest> {
            self.captured.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl TheoryChunksCollection for MockCollection {
        async fn search(
            &self,
            request: &TheoryChunkSearchRequest,
        ) -> Result<TheoryChunkSearchResult, TheoryChunksCollectionError> {
            self.captured.lock().unwrap().push(request.clone());
            self.responses.lock().unwrap().remove(0)
        }
    }

    fn ok(hits: Vec<TheoryChunkSearchHit>) -> Result<TheoryChunkSearchResult, TheoryChunksCollectionError> {
        Ok(TheoryChunkSearchResult { hits })
    }

    // ─── Constructor: top_k validation ───────────────────────────────────────

    #[test]
    fn new_rejects_top_k_zero() {
        let mock = MockCollection::new(vec![]);
        let err = TheoryEvidenceRetrieval::new(mock, settings(0, 0.5)).unwrap_err();
        assert!(matches!(err, TheoryEvidenceRetrievalError::InvalidSettings(_)));
    }

    #[test]
    fn new_succeeds_when_top_k_nonzero() {
        let mock = MockCollection::new(vec![]);
        assert!(TheoryEvidenceRetrieval::new(mock, settings(1, 0.5)).is_ok());
    }

    // ─── Constructor: score_threshold validation ──────────────────────────────

    #[test]
    fn new_rejects_negative_score_threshold() {
        let mock = MockCollection::new(vec![]);
        let err = TheoryEvidenceRetrieval::new(mock, settings(5, -0.1)).unwrap_err();
        assert!(matches!(err, TheoryEvidenceRetrievalError::InvalidSettings(_)));
    }

    #[test]
    fn new_rejects_nan_score_threshold() {
        let mock = MockCollection::new(vec![]);
        let err = TheoryEvidenceRetrieval::new(mock, settings(5, f32::NAN)).unwrap_err();
        assert!(matches!(err, TheoryEvidenceRetrievalError::InvalidSettings(_)));
    }

    #[test]
    fn new_rejects_positive_infinity_score_threshold() {
        let mock = MockCollection::new(vec![]);
        let err = TheoryEvidenceRetrieval::new(mock, settings(5, f32::INFINITY)).unwrap_err();
        assert!(matches!(err, TheoryEvidenceRetrievalError::InvalidSettings(_)));
    }

    #[test]
    fn new_rejects_negative_infinity_score_threshold() {
        let mock = MockCollection::new(vec![]);
        let err = TheoryEvidenceRetrieval::new(mock, settings(5, f32::NEG_INFINITY)).unwrap_err();
        assert!(matches!(err, TheoryEvidenceRetrievalError::InvalidSettings(_)));
    }

    // ─── retrieve: single collection call ────────────────────────────────────

    #[tokio::test]
    async fn retrieve_issues_exactly_one_collection_call() {
        let mock = MockCollection::new(vec![ok(vec![])]);
        let sut = TheoryEvidenceRetrieval::new(mock.clone(), settings(5, 0.5)).unwrap();
        sut.retrieve(&request("q")).await.unwrap();
        assert_eq!(mock.captured_requests().len(), 1);
    }

    // ─── retrieve: request construction ──────────────────────────────────────

    #[tokio::test]
    async fn request_construction_uses_unchanged_query_text() {
        let mock = MockCollection::new(vec![ok(vec![])]);
        let sut = TheoryEvidenceRetrieval::new(mock.clone(), settings(5, 0.5)).unwrap();
        sut.retrieve(&request("raft leader election")).await.unwrap();
        assert_eq!(mock.captured_requests()[0].user_query.0, "raft leader election");
    }

    #[tokio::test]
    async fn request_construction_uses_top_k_as_limit() {
        let mock = MockCollection::new(vec![ok(vec![])]);
        let sut = TheoryEvidenceRetrieval::new(mock.clone(), settings(7, 0.5)).unwrap();
        sut.retrieve(&request("q")).await.unwrap();
        assert_eq!(mock.captured_requests()[0].limit, 7);
    }

    #[tokio::test]
    async fn request_construction_passes_score_threshold() {
        let mock = MockCollection::new(vec![ok(vec![])]);
        let sut = TheoryEvidenceRetrieval::new(mock.clone(), settings(5, 0.42)).unwrap();
        sut.retrieve(&request("q")).await.unwrap();
        assert_eq!(mock.captured_requests()[0].score_threshold, 0.42);
    }

    // ─── retrieve: empty result ───────────────────────────────────────────────

    #[tokio::test]
    async fn empty_collection_result_returns_empty_output() {
        let mock = MockCollection::new(vec![ok(vec![])]);
        let sut = TheoryEvidenceRetrieval::new(mock, settings(5, 0.5)).unwrap();
        let out = sut.retrieve(&request("q")).await.unwrap();
        assert!(out.chunks.is_empty());
    }

    // ─── retrieve: hit mapping ────────────────────────────────────────────────

    #[tokio::test]
    async fn hit_fields_mapped_correctly_to_theory_evidence_chunk() {
        let mock = MockCollection::new(vec![ok(vec![hit("tc-42", 0.77, "paxos quorum")])]);
        let sut = TheoryEvidenceRetrieval::new(mock, settings(5, 0.5)).unwrap();
        let out = sut.retrieve(&request("q")).await.unwrap();
        assert_eq!(out.chunks.len(), 1);
        assert_eq!(out.chunks[0].chunk_id, "tc-42");
        assert_eq!(out.chunks[0].score, 0.77);
        assert_eq!(out.chunks[0].text, "paxos quorum");
    }

    // ─── retrieve: hit ordering ───────────────────────────────────────────────

    #[tokio::test]
    async fn collection_hit_order_preserved_in_output() {
        let hits = vec![
            hit("c3", 0.9, "t3"),
            hit("c1", 0.7, "t1"),
            hit("c2", 0.5, "t2"),
        ];
        let mock = MockCollection::new(vec![ok(hits)]);
        let sut = TheoryEvidenceRetrieval::new(mock, settings(5, 0.5)).unwrap();
        let out = sut.retrieve(&request("q")).await.unwrap();
        let ids: Vec<&str> = out.chunks.iter().map(|c| c.chunk_id.as_str()).collect();
        assert_eq!(ids, vec!["c3", "c1", "c2"]);
    }

    // ─── retrieve: score and text preserved raw ───────────────────────────────

    #[tokio::test]
    async fn raw_score_preserved_without_rounding() {
        let raw_score = 0.123_456_78_f32;
        let mock = MockCollection::new(vec![ok(vec![hit("c1", raw_score, "text")])]);
        let sut = TheoryEvidenceRetrieval::new(mock, settings(5, 0.0)).unwrap();
        let out = sut.retrieve(&request("q")).await.unwrap();
        assert_eq!(out.chunks[0].score, raw_score);
    }

    #[tokio::test]
    async fn raw_text_preserved_unchanged() {
        let raw_text = "  verbatim  text\nwith whitespace  ";
        let mock = MockCollection::new(vec![ok(vec![hit("c1", 0.8, raw_text)])]);
        let sut = TheoryEvidenceRetrieval::new(mock, settings(5, 0.0)).unwrap();
        let out = sut.retrieve(&request("q")).await.unwrap();
        assert_eq!(out.chunks[0].text, raw_text);
    }

    // ─── retrieve: no truncation ──────────────────────────────────────────────

    #[tokio::test]
    async fn all_returned_hits_passed_through_without_truncation() {
        let hits: Vec<_> = (0..5).map(|i| hit(&format!("c{i}"), 0.9 - i as f32 * 0.1, "t")).collect();
        let mock = MockCollection::new(vec![ok(hits)]);
        let sut = TheoryEvidenceRetrieval::new(mock, settings(5, 0.0)).unwrap();
        let out = sut.retrieve(&request("q")).await.unwrap();
        assert_eq!(out.chunks.len(), 5);
    }

    // ─── retrieve: no deduplication ──────────────────────────────────────────

    #[tokio::test]
    async fn duplicate_chunk_ids_not_deduplicated() {
        let hits = vec![hit("same-id", 0.9, "t1"), hit("same-id", 0.7, "t2")];
        let mock = MockCollection::new(vec![ok(hits)]);
        let sut = TheoryEvidenceRetrieval::new(mock, settings(5, 0.5)).unwrap();
        let out = sut.retrieve(&request("q")).await.unwrap();
        assert_eq!(out.chunks.len(), 2);
        assert_eq!(out.chunks[0].chunk_id, "same-id");
        assert_eq!(out.chunks[1].chunk_id, "same-id");
    }

    // ─── retrieve: error propagation ─────────────────────────────────────────

    #[tokio::test]
    async fn collection_error_wrapped_as_collection_variant() {
        let mock = MockCollection::new(vec![Err(
            TheoryChunksCollectionError::InvalidRequest("boom"),
        )]);
        let sut = TheoryEvidenceRetrieval::new(mock, settings(5, 0.5)).unwrap();
        let err = sut.retrieve(&request("q")).await.unwrap_err();
        assert!(matches!(err, TheoryEvidenceRetrievalError::Collection(_)));
    }

    // ─── retrieve: independence from other pipeline outputs ───────────────────

    #[tokio::test]
    async fn retrieve_accepts_only_normalized_request_no_other_pipeline_outputs() {
        let mock = MockCollection::new(vec![ok(vec![hit("c1", 0.8, "text")])]);
        let sut = TheoryEvidenceRetrieval::new(mock, settings(5, 0.5)).unwrap();
        let out = sut.retrieve(&request("standalone query")).await.unwrap();
        assert_eq!(out.chunks.len(), 1);
    }
}
