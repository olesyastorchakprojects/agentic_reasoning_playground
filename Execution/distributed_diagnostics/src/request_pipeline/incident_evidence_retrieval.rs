use std::sync::Arc;

use crate::api_clients::qdrant::practice_chunks_collection::{
    PracticeChunkFilter, PracticeChunkSearchRequest, PracticeChunksCollection,
    PracticeChunksCollectionError,
};
use crate::api_clients::qdrant::shared_types::NormalizedUserQuery;
use crate::config::CollectionRetrievalSettings;
use crate::request_pipeline::context::Context;
use crate::shared_types::{
    CandidateCardRetrievalOutput, IncidentEvidenceChunk, IncidentEvidenceRetrievalOutput,
    NormalizedUserRequest,
};
use tracing::{info_span, field, Instrument};

// ─── Tag sets ─────────────────────────────────────────────────────────────────

const PRIMARY_TAGS: &[&str] = &[
    "chunk_role:symptom",
    "chunk_role:impact",
    "chunk_role:timeline",
    "chunk_role:symptom_change",
    "chunk_role:investigation",
    "chunk_role:diagnostic_step",
    "chunk_role:hypothesis_update",
    "chunk_role:recovery",
];

const ALTERNATIVE_TAGS: &[&str] = &[
    "chunk_role:failure_mode",
    "chunk_role:root_cause",
    "chunk_role:contributing_factor",
    "chunk_role:uncertainty",
    "chunk_role:lesson",
];

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum IncidentEvidenceRetrievalError {
    #[error("invalid settings: {0}")]
    InvalidSettings(String),
    #[error("practice chunks collection error: {0}")]
    Collection(#[from] PracticeChunksCollectionError),
}

// ─── Public struct ────────────────────────────────────────────────────────────

pub struct IncidentEvidenceRetrieval {
    collection: Arc<dyn PracticeChunksCollection>,
    settings: CollectionRetrievalSettings,
}

impl std::fmt::Debug for IncidentEvidenceRetrieval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IncidentEvidenceRetrieval")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl IncidentEvidenceRetrieval {
    pub fn new(
        collection: Arc<dyn PracticeChunksCollection>,
        settings: CollectionRetrievalSettings,
    ) -> Result<Self, IncidentEvidenceRetrievalError> {
        if settings.top_k == 0 {
            return Err(IncidentEvidenceRetrievalError::InvalidSettings(
                "top_k must be greater than 0".to_string(),
            ));
        }
        Ok(Self {
            collection,
            settings,
        })
    }

    pub async fn retrieve(
        &self,
        request: &NormalizedUserRequest,
        candidates: &CandidateCardRetrievalOutput,
    ) -> Result<IncidentEvidenceRetrievalOutput, IncidentEvidenceRetrievalError> {
        self.retrieve_with_context(request, candidates, &Context::noop())
            .await
    }

    pub async fn retrieve_with_context(
        &self,
        request: &NormalizedUserRequest,
        candidates: &CandidateCardRetrievalOutput,
        context: &Context,
    ) -> Result<IncidentEvidenceRetrievalOutput, IncidentEvidenceRetrievalError> {
        let query = request.query.clone();
        let primary_case_id = candidates.primary.as_ref().map(|p| p.case_id.as_str()).unwrap_or("");
        let primary_tag_set = format!("{:?}", PRIMARY_TAGS);
        let alternative_tag_set = format!("{:?}", ALTERNATIVE_TAGS);
        let oi_primary_span = crate::observability::oi_retriever_incident_primary_span(
            &context.open_inference.root_span,
        );
        let oi_alternatives_span = crate::observability::oi_retriever_incident_alternatives_span(
            &context.open_inference.root_span,
        );
        let oi_primary_input_json = serde_json::json!({
            "normalized_query": request.query,
            "case_id": primary_case_id,
            "top_k": self.settings.top_k,
            "score_threshold": self.settings.score_threshold,
            "tags": PRIMARY_TAGS
        })
        .to_string();
        oi_primary_span.record("input.value", oi_primary_input_json.as_str());
        oi_primary_span.record("input.mime_type", "application/json");
        let oi_alternative_input_json = serde_json::json!({
            "normalized_query": request.query,
            "case_ids": candidates.alternatives.iter().map(|c| c.case_id.as_str()).collect::<Vec<_>>(),
            "top_k": self.settings.top_k,
            "score_threshold": self.settings.score_threshold,
            "tags": ALTERNATIVE_TAGS
        })
        .to_string();
        oi_alternatives_span.record("input.value", oi_alternative_input_json.as_str());
        oi_alternatives_span.record("input.mime_type", "application/json");

        let span = info_span!(
            "request_pipeline.incident_evidence_retrieval",
            module.name = "incident_evidence_retrieval",
            query.normalized = %query,
            incident_evidence.primary.case_id = primary_case_id,
            incident_evidence.top_k = self.settings.top_k,
            incident_evidence.score_threshold = self.settings.score_threshold,
            incident_evidence.primary_tag_set = %primary_tag_set,
            incident_evidence.alternative_tag_set = %alternative_tag_set,
            incident_evidence.primary_search.executed = field::Empty,
            incident_evidence.alternative_search.executed = field::Empty,
            incident_evidence.primary_chunks.count = field::Empty,
            incident_evidence.alternative_chunks.count = field::Empty,
            incident_evidence.total_chunks.count = field::Empty,
            module.outcome = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        self.retrieve_instrumented(request, candidates, &oi_primary_span, &oi_alternatives_span)
            .instrument(span)
            .await
    }

    async fn retrieve_instrumented(
        &self,
        request: &NormalizedUserRequest,
        candidates: &CandidateCardRetrievalOutput,
        oi_primary_span: &tracing::Span,
        oi_alternatives_span: &tracing::Span,
    ) -> Result<IncidentEvidenceRetrievalOutput, IncidentEvidenceRetrievalError> {
        let alternative_case_ids: Vec<&str> = candidates
            .alternatives
            .iter()
            .map(|c| c.case_id.as_str())
            .collect();

        // Primary search
        let primary_chunks = if let Some(primary) = &candidates.primary {
            tracing::Span::current().record("incident_evidence.primary_search.executed", true);

            let primary_span = info_span!(
                "qdrant.practice_chunks.search.primary",
                retrieval.branch = "primary",
                retrieval.collection = "practice_chunks",
                retrieval.case_ids_count = 1usize,
                retrieval.chunk_tags_filter.count = PRIMARY_TAGS.len(),
                retrieval.chunk_tags_filter = format!("{:?}", PRIMARY_TAGS),
                retrieval.limit = self.settings.top_k,
                retrieval.score_threshold = self.settings.score_threshold,
                retrieval.hits_count = field::Empty,
                retrieval.hit_scores = field::Empty,
                retrieval.top_score = field::Empty,
                retrieval.min_score = field::Empty,
                status = field::Empty,
                error.type = field::Empty,
                error.message = field::Empty,
            );

            let req = build_request(
                &request.query,
                vec![primary.case_id.clone()],
                PRIMARY_TAGS,
                &self.settings,
            );

            let result = {
                async {
                    match self.collection.search(&req).await {
                        Ok(r) => {
                            let hit_count = r.hits.len();
                            let chunk_ids: Vec<&str> = r.hits.iter().map(|h| h.chunk_id.as_str()).collect();
                        let scores: Vec<f32> = r.hits.iter().map(|h| h.score).collect();

                        tracing::Span::current().record("retrieval.hits_count", hit_count);
                        tracing::Span::current().record("retrieval.hit_scores", format!("{:?}", scores));
                        tracing::event!(
                            tracing::Level::INFO,
                            event.name = "incident_primary_hit_ids",
                            retrieval.hit_chunk_ids = %serde_json::to_string(&chunk_ids)
                                .unwrap_or_else(|_| "[]".to_string())
                        );

                        if !scores.is_empty() {
                                let top_score = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                                let min_score = scores.iter().copied().fold(f32::INFINITY, f32::min);
                                tracing::Span::current().record("retrieval.top_score", top_score);
                                tracing::Span::current().record("retrieval.min_score", min_score);
                            }

                            tracing::Span::current().record("status", "ok");
                            Ok(r)
                        }
                        Err(e) => {
                            crate::observability::record_error(
                                oi_primary_span,
                                "IncidentEvidenceRetrieval.Collection",
                                &format!("Primary search failed: {}", e),
                            );
                            tracing::Span::current().record("status", "error");
                            tracing::Span::current().record("error.type", "IncidentEvidenceRetrieval.Collection");
                            tracing::Span::current()
                                .record("error.message", format!("Primary search failed: {}", e));
                            Err(e)
                        }
                    }
                }
                .instrument(primary_span)
                .await
            };

            let result = match result {
                Ok(r) => r,
                Err(e) => {
                    crate::observability::record_error(
                        oi_primary_span,
                        "IncidentEvidenceRetrieval.Collection",
                        &format!("Primary search failed: {}", e),
                    );
                    tracing::Span::current().record("module.outcome", "failure");
                    tracing::Span::current().record("status", "error");
                    tracing::Span::current().record("error.type", "IncidentEvidenceRetrieval.Collection");
                    tracing::Span::current()
                        .record("error.message", format!("Primary search failed: {}", e));
                    return Err(IncidentEvidenceRetrievalError::Collection(e));
                }
            };

            let chunks = map_hits(result.hits);
            let chunk_ids: Vec<&str> = chunks.iter().map(|c| c.chunk_id.as_str()).collect();
            tracing::Span::current().record("incident_evidence.primary_chunks.count", chunks.len());
            tracing::event!(
                tracing::Level::INFO,
                event.name = "incident_primary_chunk_ids",
                incident_evidence.primary_chunks.ids = %serde_json::to_string(&chunk_ids)
                    .unwrap_or_else(|_| "[]".to_string())
            );
            let oi_primary_output_json = serde_json::json!({
                "documents": chunks.iter().map(|c| {
                    serde_json::json!({
                        "document.id": c.chunk_id,
                        "document.score": c.score,
                        "document.metadata": { "case_id": c.case_id }
                    })
                }).collect::<Vec<_>>()
            })
            .to_string();
            oi_primary_span.record("output.value", oi_primary_output_json.as_str());
            oi_primary_span.record("output.mime_type", "application/json");
            oi_primary_span.record("status", "ok");
            chunks
        } else {
            tracing::Span::current().record("incident_evidence.primary_search.executed", false);
            tracing::Span::current().record("incident_evidence.primary_chunks.count", 0);
            tracing::event!(
                tracing::Level::INFO,
                event.name = "incident_primary_chunk_ids",
                incident_evidence.primary_chunks.ids = "[]"
            );
            oi_primary_span.record("output.value", r#"{"documents":[]}"#);
            oi_primary_span.record("output.mime_type", "application/json");
            oi_primary_span.record("status", "ok");
            vec![]
        };

        // Alternative search
        let alternative_chunks = if !candidates.alternatives.is_empty() {
            tracing::Span::current().record("incident_evidence.alternative_search.executed", true);

            let case_ids: Vec<String> = candidates
                .alternatives
                .iter()
                .map(|c| c.case_id.clone())
                .collect();

            let alternative_span = info_span!(
                "qdrant.practice_chunks.search.alternatives",
                retrieval.branch = "alternatives",
                retrieval.collection = "practice_chunks",
                retrieval.case_ids_count = case_ids.len(),
                retrieval.chunk_tags_filter.count = ALTERNATIVE_TAGS.len(),
                retrieval.chunk_tags_filter = format!("{:?}", ALTERNATIVE_TAGS),
                retrieval.limit = self.settings.top_k,
                retrieval.score_threshold = self.settings.score_threshold,
                retrieval.hits_count = field::Empty,
                retrieval.hit_scores = field::Empty,
                retrieval.top_score = field::Empty,
                retrieval.min_score = field::Empty,
                status = field::Empty,
                error.type = field::Empty,
                error.message = field::Empty,
            );

            let req = build_request(&request.query, case_ids, ALTERNATIVE_TAGS, &self.settings);

            let result = {
                async {
                    match self.collection.search(&req).await {
                        Ok(r) => {
                            let hit_count = r.hits.len();
                            let chunk_ids: Vec<&str> = r.hits.iter().map(|h| h.chunk_id.as_str()).collect();
                        let scores: Vec<f32> = r.hits.iter().map(|h| h.score).collect();

                        tracing::Span::current().record("retrieval.hits_count", hit_count);
                        tracing::Span::current().record("retrieval.hit_scores", format!("{:?}", scores));
                        tracing::event!(
                            tracing::Level::INFO,
                            event.name = "incident_alternative_hit_ids",
                            retrieval.hit_chunk_ids = %serde_json::to_string(&chunk_ids)
                                .unwrap_or_else(|_| "[]".to_string())
                        );

                        if !scores.is_empty() {
                                let top_score = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                                let min_score = scores.iter().copied().fold(f32::INFINITY, f32::min);
                                tracing::Span::current().record("retrieval.top_score", top_score);
                                tracing::Span::current().record("retrieval.min_score", min_score);
                            }

                            tracing::Span::current().record("status", "ok");
                            Ok(r)
                        }
                        Err(e) => {
                            crate::observability::record_error(
                                oi_alternatives_span,
                                "IncidentEvidenceRetrieval.Collection",
                                &format!("Alternative search failed: {}", e),
                            );
                            tracing::Span::current().record("status", "error");
                            tracing::Span::current().record("error.type", "IncidentEvidenceRetrieval.Collection");
                            tracing::Span::current()
                                .record("error.message", format!("Alternative search failed: {}", e));
                            Err(e)
                        }
                    }
                }
                .instrument(alternative_span)
                .await
            };

            let result = match result {
                Ok(r) => r,
                Err(e) => {
                    crate::observability::record_error(
                        oi_alternatives_span,
                        "IncidentEvidenceRetrieval.Collection",
                        &format!("Alternative search failed: {}", e),
                    );
                    tracing::Span::current().record("module.outcome", "failure");
                    tracing::Span::current().record("status", "error");
                    tracing::Span::current().record("error.type", "IncidentEvidenceRetrieval.Collection");
                    tracing::Span::current()
                        .record("error.message", format!("Alternative search failed: {}", e));
                    return Err(IncidentEvidenceRetrievalError::Collection(e));
                }
            };

            let chunks = map_hits(result.hits);
            let chunk_ids: Vec<&str> = chunks.iter().map(|c| c.chunk_id.as_str()).collect();
            tracing::Span::current().record("incident_evidence.alternative_chunks.count", chunks.len());
            tracing::event!(
                tracing::Level::INFO,
                event.name = "incident_alternative_chunk_ids",
                incident_evidence.alternative_chunks.ids = %serde_json::to_string(&chunk_ids)
                    .unwrap_or_else(|_| "[]".to_string())
            );
            let oi_alternative_output_json = serde_json::json!({
                "documents": chunks.iter().map(|c| {
                    serde_json::json!({
                        "document.id": c.chunk_id,
                        "document.score": c.score,
                        "document.metadata": { "case_id": c.case_id }
                    })
                }).collect::<Vec<_>>()
            })
            .to_string();
            oi_alternatives_span.record("output.value", oi_alternative_output_json.as_str());
            oi_alternatives_span.record("output.mime_type", "application/json");
            oi_alternatives_span.record("status", "ok");
            chunks
        } else {
            tracing::Span::current().record("incident_evidence.alternative_search.executed", false);
            tracing::Span::current().record("incident_evidence.alternative_chunks.count", 0);
            tracing::event!(
                tracing::Level::INFO,
                event.name = "incident_alternative_chunk_ids",
                incident_evidence.alternative_chunks.ids = "[]"
            );
            oi_alternatives_span.record("output.value", r#"{"documents":[]}"#);
            oi_alternatives_span.record("output.mime_type", "application/json");
            oi_alternatives_span.record("status", "ok");
            vec![]
        };

        tracing::event!(
            tracing::Level::INFO,
            event.name = "incident_alternative_case_ids",
            incident_evidence.alternative.case_ids = %serde_json::to_string(&alternative_case_ids)
                .unwrap_or_else(|_| "[]".to_string())
        );

        let total_count = primary_chunks.len() + alternative_chunks.len();
        tracing::Span::current().record("incident_evidence.total_chunks.count", total_count);
        tracing::Span::current().record("module.outcome", "success");
        tracing::Span::current().record("status", "ok");

        Ok(IncidentEvidenceRetrievalOutput {
            primary_chunks,
            alternative_chunks,
        })
    }
}

// ─── Private helpers ──────────────────────────────────────────────────────────

fn build_request(
    query: &str,
    case_ids: Vec<String>,
    tags: &[&str],
    settings: &CollectionRetrievalSettings,
) -> PracticeChunkSearchRequest {
    PracticeChunkSearchRequest {
        user_query: NormalizedUserQuery(query.to_owned()),
        filter: PracticeChunkFilter {
            case_ids,
            chunk_tags: tags.iter().map(|s| s.to_string()).collect(),
        },
        limit: settings.top_k,
        score_threshold: settings.score_threshold,
    }
}

fn map_hits(
    hits: Vec<crate::api_clients::qdrant::practice_chunks_collection::PracticeChunkSearchHit>,
) -> Vec<IncidentEvidenceChunk> {
    hits.into_iter()
        .map(|h| IncidentEvidenceChunk {
            chunk_id: h.chunk_id,
            case_id: h.case_id,
            score: h.score,
            chunk_tags: h.chunk_tags,
            text: h.text,
        })
        .collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_clients::qdrant::practice_chunks_collection::{
        PracticeChunkSearchHit, PracticeChunkSearchResult,
    };
    use crate::config::{CollectionSettings, DenseCollectionSettings};
    use crate::shared_types::CandidateCard;
    use crate::utils::retry::{RetryBackoffKind, RetryPolicyConfig};
    use async_trait::async_trait;
    use std::sync::Mutex;

    // ─── Helpers ──────────────────────────────────────────────────────────────

    fn settings(top_k: usize) -> CollectionRetrievalSettings {
        CollectionRetrievalSettings {
            top_k,
            score_threshold: 0.5,
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
                name: "test".to_string(),
                vector_name: "v".to_string(),
                corpus_version: "1".to_string(),
            }),
        }
    }

    fn request(query: &str) -> NormalizedUserRequest {
        NormalizedUserRequest {
            query: query.to_string(),
            input_token_count: 10,
        }
    }

    fn hit(chunk_id: &str, case_id: &str, score: f32) -> PracticeChunkSearchHit {
        PracticeChunkSearchHit {
            chunk_id: chunk_id.to_string(),
            score,
            case_id: case_id.to_string(),
            chunk_tags: vec!["chunk_role:symptom".to_string()],
            text: "text".to_string(),
        }
    }

    fn candidate(case_id: &str) -> CandidateCard {
        CandidateCard {
            case_id: case_id.to_string(),
            score: 0.9,
        }
    }

    // ─── Mock ─────────────────────────────────────────────────────────────────

    struct MockCollection {
        responses: Mutex<Vec<Result<PracticeChunkSearchResult, PracticeChunksCollectionError>>>,
        captured: Mutex<Vec<PracticeChunkSearchRequest>>,
    }

    impl MockCollection {
        fn new(
            responses: Vec<Result<PracticeChunkSearchResult, PracticeChunksCollectionError>>,
        ) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses),
                captured: Mutex::new(vec![]),
            })
        }

        fn captured_requests(&self) -> Vec<PracticeChunkSearchRequest> {
            self.captured.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl PracticeChunksCollection for MockCollection {
        async fn search(
            &self,
            request: &PracticeChunkSearchRequest,
        ) -> Result<PracticeChunkSearchResult, PracticeChunksCollectionError> {
            self.captured.lock().unwrap().push(request.clone());
            self.responses.lock().unwrap().remove(0)
        }
    }

    fn ok_result(
        hits: Vec<PracticeChunkSearchHit>,
    ) -> Result<PracticeChunkSearchResult, PracticeChunksCollectionError> {
        Ok(PracticeChunkSearchResult { hits })
    }

    // ─── Constructor tests ────────────────────────────────────────────────────

    #[test]
    fn new_fails_when_top_k_is_zero() {
        let mock = MockCollection::new(vec![]);
        let err = IncidentEvidenceRetrieval::new(mock, settings(0)).unwrap_err();
        assert!(matches!(
            err,
            IncidentEvidenceRetrievalError::InvalidSettings(_)
        ));
    }

    #[test]
    fn new_succeeds_when_top_k_is_nonzero() {
        let mock = MockCollection::new(vec![]);
        assert!(IncidentEvidenceRetrieval::new(mock, settings(5)).is_ok());
    }

    // ─── Empty input ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn empty_candidates_returns_empty_output_without_collection_call() {
        let mock = MockCollection::new(vec![]);
        let sut = IncidentEvidenceRetrieval::new(mock.clone(), settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: None,
            alternatives: vec![],
        };

        let out = sut.retrieve(&request("q"), &candidates).await.unwrap();

        assert!(out.primary_chunks.is_empty());
        assert!(out.alternative_chunks.is_empty());
        assert!(mock.captured_requests().is_empty());
    }

    // ─── Primary only ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn primary_only_issues_one_call_alternative_chunks_empty() {
        let mock = MockCollection::new(vec![ok_result(vec![hit("c1", "card-1", 0.8)])]);
        let sut = IncidentEvidenceRetrieval::new(mock.clone(), settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![],
        };

        let out = sut.retrieve(&request("q"), &candidates).await.unwrap();

        assert_eq!(out.primary_chunks.len(), 1);
        assert!(out.alternative_chunks.is_empty());
        assert_eq!(mock.captured_requests().len(), 1);
    }

    #[tokio::test]
    async fn primary_search_uses_primary_tag_set() {
        let mock = MockCollection::new(vec![ok_result(vec![])]);
        let sut = IncidentEvidenceRetrieval::new(mock.clone(), settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![],
        };

        sut.retrieve(&request("q"), &candidates).await.unwrap();

        let reqs = mock.captured_requests();
        let tags = &reqs[0].filter.chunk_tags;
        let expected: Vec<String> = PRIMARY_TAGS.iter().map(|s| s.to_string()).collect();
        assert_eq!(tags, &expected);
    }

    #[tokio::test]
    async fn primary_search_uses_primary_case_id() {
        let mock = MockCollection::new(vec![ok_result(vec![])]);
        let sut = IncidentEvidenceRetrieval::new(mock.clone(), settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-42")),
            alternatives: vec![],
        };

        sut.retrieve(&request("q"), &candidates).await.unwrap();

        assert_eq!(mock.captured_requests()[0].filter.case_ids, vec!["card-42"]);
    }

    // ─── Alternatives only ────────────────────────────────────────────────────

    #[tokio::test]
    async fn alternatives_only_issues_one_call_primary_chunks_empty() {
        let mock = MockCollection::new(vec![ok_result(vec![hit("c1", "card-2", 0.7)])]);
        let sut = IncidentEvidenceRetrieval::new(mock.clone(), settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: None,
            alternatives: vec![candidate("card-2")],
        };

        let out = sut.retrieve(&request("q"), &candidates).await.unwrap();

        assert!(out.primary_chunks.is_empty());
        assert_eq!(out.alternative_chunks.len(), 1);
        assert_eq!(mock.captured_requests().len(), 1);
    }

    #[tokio::test]
    async fn alternative_search_uses_alternative_tag_set() {
        let mock = MockCollection::new(vec![ok_result(vec![])]);
        let sut = IncidentEvidenceRetrieval::new(mock.clone(), settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: None,
            alternatives: vec![candidate("card-2")],
        };

        sut.retrieve(&request("q"), &candidates).await.unwrap();

        let reqs = mock.captured_requests();
        let tags = &reqs[0].filter.chunk_tags;
        let expected: Vec<String> = ALTERNATIVE_TAGS.iter().map(|s| s.to_string()).collect();
        assert_eq!(tags, &expected);
    }

    #[tokio::test]
    async fn alternative_search_passes_all_case_ids_in_order() {
        let mock = MockCollection::new(vec![ok_result(vec![])]);
        let sut = IncidentEvidenceRetrieval::new(mock.clone(), settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: None,
            alternatives: vec![
                candidate("card-A"),
                candidate("card-B"),
                candidate("card-C"),
            ],
        };

        sut.retrieve(&request("q"), &candidates).await.unwrap();

        assert_eq!(
            mock.captured_requests()[0].filter.case_ids,
            vec!["card-A", "card-B", "card-C"]
        );
    }

    // ─── Both present ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn both_present_issues_two_calls_hits_separated() {
        let mock = MockCollection::new(vec![
            ok_result(vec![hit("p1", "card-1", 0.9)]),
            ok_result(vec![hit("a1", "card-2", 0.7), hit("a2", "card-3", 0.6)]),
        ]);
        let sut = IncidentEvidenceRetrieval::new(mock.clone(), settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![candidate("card-2"), candidate("card-3")],
        };

        let out = sut.retrieve(&request("q"), &candidates).await.unwrap();

        assert_eq!(mock.captured_requests().len(), 2);
        assert_eq!(out.primary_chunks.len(), 1);
        assert_eq!(out.alternative_chunks.len(), 2);
    }

    #[tokio::test]
    async fn both_present_primary_call_comes_first() {
        let mock = MockCollection::new(vec![
            ok_result(vec![hit("p1", "card-1", 0.9)]),
            ok_result(vec![hit("a1", "card-2", 0.7)]),
        ]);
        let sut = IncidentEvidenceRetrieval::new(mock.clone(), settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![candidate("card-2")],
        };

        sut.retrieve(&request("q"), &candidates).await.unwrap();

        let reqs = mock.captured_requests();
        assert_eq!(reqs[0].filter.case_ids, vec!["card-1"]);
        assert_eq!(reqs[1].filter.case_ids, vec!["card-2"]);
    }

    // ─── Settings passthrough ─────────────────────────────────────────────────

    #[tokio::test]
    async fn settings_top_k_and_score_threshold_passed_through() {
        let mock = MockCollection::new(vec![ok_result(vec![])]);
        let mut s = settings(7);
        s.score_threshold = 0.42;
        let sut = IncidentEvidenceRetrieval::new(mock.clone(), s).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![],
        };

        sut.retrieve(&request("q"), &candidates).await.unwrap();

        let req = &mock.captured_requests()[0];
        assert_eq!(req.limit, 7);
        assert_eq!(req.score_threshold, 0.42);
    }

    // ─── Query passthrough ────────────────────────────────────────────────────

    #[tokio::test]
    async fn query_string_passed_unchanged_to_collection() {
        let mock = MockCollection::new(vec![ok_result(vec![])]);
        let sut = IncidentEvidenceRetrieval::new(mock.clone(), settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![],
        };

        sut.retrieve(&request("exact query text"), &candidates)
            .await
            .unwrap();

        assert_eq!(mock.captured_requests()[0].user_query.0, "exact query text");
    }

    // ─── Hit ordering ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn primary_chunk_order_preserved() {
        let hits = vec![
            hit("c3", "x", 0.9),
            hit("c1", "x", 0.7),
            hit("c2", "x", 0.5),
        ];
        let mock = MockCollection::new(vec![ok_result(hits)]);
        let sut = IncidentEvidenceRetrieval::new(mock, settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("x")),
            alternatives: vec![],
        };

        let out = sut.retrieve(&request("q"), &candidates).await.unwrap();

        let ids: Vec<&str> = out
            .primary_chunks
            .iter()
            .map(|c| c.chunk_id.as_str())
            .collect();
        assert_eq!(ids, vec!["c3", "c1", "c2"]);
    }

    #[tokio::test]
    async fn alternative_chunk_order_preserved() {
        let hits = vec![hit("c2", "x", 0.8), hit("c1", "x", 0.6)];
        let mock = MockCollection::new(vec![ok_result(hits)]);
        let sut = IncidentEvidenceRetrieval::new(mock, settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: None,
            alternatives: vec![candidate("x")],
        };

        let out = sut.retrieve(&request("q"), &candidates).await.unwrap();

        let ids: Vec<&str> = out
            .alternative_chunks
            .iter()
            .map(|c| c.chunk_id.as_str())
            .collect();
        assert_eq!(ids, vec!["c2", "c1"]);
    }

    // ─── case_id passthrough ──────────────────────────────────────────────────

    #[tokio::test]
    async fn case_id_from_hit_not_rewritten() {
        let mock = MockCollection::new(vec![ok_result(vec![hit("c1", "returned-case-id", 0.8)])]);
        let sut = IncidentEvidenceRetrieval::new(mock, settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("different-case-id")),
            alternatives: vec![],
        };

        let out = sut.retrieve(&request("q"), &candidates).await.unwrap();

        assert_eq!(out.primary_chunks[0].case_id, "returned-case-id");
    }

    // ─── Error propagation ────────────────────────────────────────────────────

    #[tokio::test]
    async fn primary_collection_error_propagated() {
        let mock = MockCollection::new(vec![Err(PracticeChunksCollectionError::InvalidRequest(
            "boom".to_string(),
        ))]);
        let sut = IncidentEvidenceRetrieval::new(mock, settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![],
        };

        let err = sut.retrieve(&request("q"), &candidates).await.unwrap_err();
        assert!(matches!(err, IncidentEvidenceRetrievalError::Collection(_)));
    }

    #[tokio::test]
    async fn alternative_collection_error_propagated() {
        let mock = MockCollection::new(vec![Err(PracticeChunksCollectionError::InvalidRequest(
            "boom".to_string(),
        ))]);
        let sut = IncidentEvidenceRetrieval::new(mock, settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: None,
            alternatives: vec![candidate("card-2")],
        };

        let err = sut.retrieve(&request("q"), &candidates).await.unwrap_err();
        assert!(matches!(err, IncidentEvidenceRetrievalError::Collection(_)));
    }

    #[tokio::test]
    async fn alternative_error_after_primary_success_fails_whole_call() {
        let mock = MockCollection::new(vec![
            ok_result(vec![hit("p1", "card-1", 0.9)]),
            Err(PracticeChunksCollectionError::InvalidRequest(
                "boom".to_string(),
            )),
        ]);
        let sut = IncidentEvidenceRetrieval::new(mock, settings(5)).unwrap();
        let candidates = CandidateCardRetrievalOutput {
            primary: Some(candidate("card-1")),
            alternatives: vec![candidate("card-2")],
        };

        let err = sut.retrieve(&request("q"), &candidates).await.unwrap_err();
        assert!(matches!(err, IncidentEvidenceRetrievalError::Collection(_)));
    }
}
