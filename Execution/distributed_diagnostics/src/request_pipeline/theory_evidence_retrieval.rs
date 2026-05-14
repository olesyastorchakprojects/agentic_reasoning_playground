use std::sync::Arc;

use crate::api_clients::qdrant::shared_types::NormalizedUserQuery;
use crate::api_clients::qdrant::theory_chunks_collection::{
    TheoryChunkSearchRequest, TheoryChunksCollection, TheoryChunksCollectionError,
};
use crate::config::CollectionRetrievalSettings;
use crate::request_pipeline::retrieval_metrics::{
    compute_retrieval_metrics, GoldenRetrievalRelevanceById, GoldenRetrievalTargetsById,
};
use crate::shared_types::{
    Context, NormalizedUserRequest, RetrievalCallStats, TheoryEvidenceChunk,
    TheoryEvidenceRetrievalMetrics, TheoryEvidenceRetrievalOutput,
};
use tracing::{info_span, field, Instrument};

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum TheoryEvidenceRetrievalError {
    #[error("invalid settings: {0}")]
    InvalidSettings(String),
    #[error("theory chunks collection error: {0}")]
    Collection(TheoryChunksCollectionError),
    #[error("retrieval metrics computation failed: {0}")]
    MetricsComputation(String),
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
                "top_k must be greater than 0".to_string(),
            ));
        }
        if settings.score_threshold < 0.0 {
            return Err(TheoryEvidenceRetrievalError::InvalidSettings(
                "score_threshold must be non-negative".to_string(),
            ));
        }
        if settings.score_threshold.is_nan() {
            return Err(TheoryEvidenceRetrievalError::InvalidSettings(
                "score_threshold must not be NaN".to_string(),
            ));
        }
        if settings.score_threshold.is_infinite() {
            return Err(TheoryEvidenceRetrievalError::InvalidSettings(
                "score_threshold must not be infinite".to_string(),
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
    ) -> Result<TheoryEvidenceRetrievalOutput, TheoryEvidenceRetrievalError> {
        self.retrieve_with_context(request, &Context::noop()).await
    }

    pub async fn retrieve_with_context(
        &self,
        request: &NormalizedUserRequest,
        context: &Context,
    ) -> Result<TheoryEvidenceRetrievalOutput, TheoryEvidenceRetrievalError> {
        let query = request.query.clone();
        let collection_name = match &self.settings.collection {
            crate::config::CollectionSettings::Dense(dense) => dense.name.clone(),
            crate::config::CollectionSettings::Hybrid(_) => "theory_chunks".to_string(),
        };
        let oi_span =
            crate::observability::oi_retriever_theory_span(&context.open_inference.root_span);
        let oi_input_json = serde_json::json!({
            "normalized_query": request.query,
            "collection": collection_name,
            "top_k": self.settings.top_k,
            "score_threshold": self.settings.score_threshold
        })
        .to_string();
        oi_span.record("input.value", oi_input_json.as_str());
        oi_span.record("input.mime_type", "application/json");

        let span = info_span!(
            "request_pipeline.theory_evidence_retrieval",
            module.name = "theory_evidence_retrieval",
            query.normalized = %query,
            theory_retrieval.collection = %collection_name,
            theory_retrieval.top_k = self.settings.top_k,
            theory_retrieval.score_threshold = self.settings.score_threshold,
            theory_retrieval.search_executed = field::Empty,
            theory_retrieval.hits_count = field::Empty,
            theory_retrieval.scores = field::Empty,
            theory_retrieval.empty_result = field::Empty,
            module.outcome = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        self.retrieve_instrumented(request, &oi_span, context)
            .instrument(span)
            .instrument(oi_span.clone())
            .await
    }

    async fn retrieve_instrumented(
        &self,
        request: &NormalizedUserRequest,
        oi_span: &tracing::Span,
        context: &Context,
    ) -> Result<TheoryEvidenceRetrievalOutput, TheoryEvidenceRetrievalError> {
        let collection_name = match &self.settings.collection {
            crate::config::CollectionSettings::Dense(dense) => dense.name.clone(),
            crate::config::CollectionSettings::Hybrid(_) => "theory_chunks".to_string(),
        };

        tracing::Span::current().record("theory_retrieval.search_executed", true);

        let search_request = TheoryChunkSearchRequest {
            user_query: NormalizedUserQuery(request.query.clone()),
            limit: self.settings.top_k,
            score_threshold: self.settings.score_threshold,
        };

        let qdrant_span = info_span!(
            "qdrant.theory_chunks.search",
            retrieval.collection = %collection_name,
            retrieval.limit = self.settings.top_k,
            retrieval.score_threshold = self.settings.score_threshold,
            retrieval.hits_count = field::Empty,
            retrieval.hit_scores = field::Empty,
            status = field::Empty,
            error.type = field::Empty,
            error.message = field::Empty,
        );

        let result = {
            async {
                match self.collection.search(&search_request).await {
                    Ok(r) => {
                        let hit_count = r.hits.len();
                        let chunk_ids: Vec<&str> = r.hits.iter().map(|h| h.chunk_id.as_str()).collect();
                        let scores: Vec<f32> = r.hits.iter().map(|h| h.score).collect();

                        tracing::Span::current().record("retrieval.hits_count", hit_count);
                        tracing::Span::current().record("retrieval.hit_scores", format!("{:?}", scores));
                        tracing::event!(
                            tracing::Level::INFO,
                            event.name = "theory_retrieval_hit_ids",
                            retrieval.hit_chunk_ids = %serde_json::to_string(&chunk_ids)
                                .unwrap_or_else(|_| "[]".to_string())
                        );
                        tracing::Span::current().record("status", "ok");
                        Ok(r)
                    }
                    Err(e) => {
                        crate::observability::record_error(
                            oi_span,
                            "TheoryEvidenceRetrieval.Collection",
                            &format!("Theory chunks search failed: {}", e),
                        );
                        tracing::Span::current().record("status", "error");
                        tracing::Span::current().record("error.type", "TheoryEvidenceRetrieval.Collection");
                        tracing::Span::current()
                            .record("error.message", format!("Theory chunks search failed: {}", e));
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
                    "TheoryEvidenceRetrieval.Collection",
                    &format!("Theory chunks search failed: {}", e),
                );
                tracing::Span::current().record("module.outcome", "failure");
                tracing::Span::current().record("status", "error");
                tracing::Span::current().record("error.type", "TheoryEvidenceRetrieval.Collection");
                tracing::Span::current()
                    .record("error.message", format!("Theory chunks search failed: {}", e));
                return Err(TheoryEvidenceRetrievalError::Collection(e));
            }
        };

        let empty_result = result.hits.is_empty();
        let chunks: Vec<TheoryEvidenceChunk> = result
            .hits
            .into_iter()
            .map(|h| TheoryEvidenceChunk {
                chunk_id: h.chunk_id,
                score: h.score,
                text: h.text,
            })
            .collect();

        let chunk_ids: Vec<&str> = chunks.iter().map(|c| c.chunk_id.as_str()).collect();
        let scores: Vec<f32> = chunks.iter().map(|c| c.score).collect();

        tracing::Span::current().record("theory_retrieval.hits_count", chunks.len());
        tracing::Span::current().record("theory_retrieval.scores", format!("{:?}", scores));
        tracing::event!(
            tracing::Level::INFO,
            event.name = "theory_retrieval_output_ids",
            theory_retrieval.chunk_ids = %serde_json::to_string(&chunk_ids)
                .unwrap_or_else(|_| "[]".to_string())
        );
        let oi_output_json = serde_json::json!({
            "documents": chunks.iter().map(|c| {
                serde_json::json!({
                    "document.id": c.chunk_id,
                    "document.score": c.score
                })
            }).collect::<Vec<_>>()
        })
        .to_string();
        oi_span.record("output.value", oi_output_json.as_str());
        oi_span.record("output.mime_type", "application/json");
        oi_span.record("status", "ok");
        tracing::Span::current().record("theory_retrieval.empty_result", empty_result);
        tracing::Span::current().record("module.outcome", "success");
        tracing::Span::current().record("status", "ok");

        let metrics = if let Some(golden_question) = &context.golden_question {
            let golden = &golden_question.expected_theory_evidence.mechanism_explanation;
            if golden.strict_chunk_ids.is_empty() || golden.soft_chunk_ids.is_empty() {
                None
            } else {
                let golden_targets = GoldenRetrievalTargetsById {
                    strict_positive_ids: golden.strict_chunk_ids.clone(),
                    soft_positive_ids: golden.soft_chunk_ids.clone(),
                    graded_relevance: golden
                        .graded_relevance
                        .iter()
                        .map(|rel| GoldenRetrievalRelevanceById {
                            id: rel.chunk_id.clone(),
                            score: rel.score,
                        })
                        .collect(),
                };
                let actual_ranked_ids: Vec<String> =
                    chunks.iter().map(|chunk| chunk.chunk_id.clone()).collect();
                let computed = compute_retrieval_metrics(
                    &golden_targets,
                    &actual_ranked_ids,
                    self.settings.top_k,
                )
                .map_err(|e| TheoryEvidenceRetrievalError::MetricsComputation(e.to_string()))?;
                let metrics = TheoryEvidenceRetrievalMetrics {
                    mechanism_explanation: computed,
                    call_stats: RetrievalCallStats {
                        hits_count: chunks.len() as u32,
                        selected_count: chunks.len() as u32,
                        top_score: chunks.iter().map(|c| c.score).reduce(f32::max),
                        min_score: chunks.iter().map(|c| c.score).reduce(f32::min),
                    },
                };
                oi_span.in_scope(|| emit_theory_evidence_metrics_oi_span(&oi_span, &metrics));
                Some(metrics)
            }
        } else {
            None
        };

        Ok(TheoryEvidenceRetrievalOutput {
            chunks,
            metrics,
        })
    }
}

fn emit_theory_evidence_metrics_oi_span(
    oi_parent: &tracing::Span,
    metrics: &TheoryEvidenceRetrievalMetrics,
) {
    use opentelemetry::{Key, Value};
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let span = crate::observability::oi_chain_theory_evidence_retrieval_metrics_span(oi_parent);
    span.record(
        "input.value",
        r#"{"golden_backed":true,"source":"theory_evidence"}"#,
    );
    span.record("input.mime_type", "application/json");

    let m = &metrics.mechanism_explanation;
    span.set_attribute(
        Key::from("rt.theory_evidence.recall_soft"),
        Value::F64(m.recall_soft as f64),
    );
    span.set_attribute(
        Key::from("rt.theory_evidence.recall_strict"),
        Value::F64(m.recall_strict as f64),
    );
    span.set_attribute(
        Key::from("rt.theory_evidence.rr_soft"),
        Value::F64(m.rr_soft as f64),
    );
    span.set_attribute(
        Key::from("rt.theory_evidence.rr_strict"),
        Value::F64(m.rr_strict as f64),
    );
    span.set_attribute(Key::from("rt.theory_evidence.ndcg"), Value::F64(m.ndcg as f64));
    span.set_attribute(
        Key::from("rt.theory_evidence.evaluated_k"),
        Value::I64(m.evaluated_k as i64),
    );
    span.set_attribute(
        Key::from("rt.theory_evidence.first_relevant_rank_soft"),
        opt_u32_attr(m.first_relevant_rank_soft),
    );
    span.set_attribute(
        Key::from("rt.theory_evidence.first_relevant_rank_strict"),
        opt_u32_attr(m.first_relevant_rank_strict),
    );
    span.set_attribute(
        Key::from("rt.theory_evidence.num_relevant_soft"),
        Value::I64(m.num_relevant_soft as i64),
    );
    span.set_attribute(
        Key::from("rt.theory_evidence.num_relevant_strict"),
        Value::I64(m.num_relevant_strict as i64),
    );

    let payload = serde_json::json!({
        "recall_soft": m.recall_soft,
        "recall_strict": m.recall_strict,
        "rr_soft": m.rr_soft,
        "rr_strict": m.rr_strict,
        "ndcg": m.ndcg,
        "evaluated_k": m.evaluated_k,
        "first_relevant_rank_soft": m.first_relevant_rank_soft,
        "first_relevant_rank_strict": m.first_relevant_rank_strict,
        "num_relevant_soft": m.num_relevant_soft,
        "num_relevant_strict": m.num_relevant_strict,
    })
    .to_string();

    let _guard = span.enter();
    tracing::event!(
        tracing::Level::INFO,
        event.name = "retrieval_metrics.theory_evidence",
        payload = %payload
    );
    drop(_guard);

    let output_json = serde_json::to_string(metrics).unwrap_or_default();
    span.record("output.value", output_json.as_str());
    span.record("output.mime_type", "application/json");
    span.record("status", "ok");
}

fn opt_u32_attr(value: Option<u32>) -> opentelemetry::Value {
    match value {
        Some(v) => opentelemetry::Value::I64(v as i64),
        None => opentelemetry::Value::String("null".into()),
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
        NormalizedUserRequest {
            query: query.to_string(),
            input_token_count: 10,
        }
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

    fn ok(
        hits: Vec<TheoryChunkSearchHit>,
    ) -> Result<TheoryChunkSearchResult, TheoryChunksCollectionError> {
        Ok(TheoryChunkSearchResult { hits })
    }

    // ─── Constructor: top_k validation ───────────────────────────────────────

    #[test]
    fn new_rejects_top_k_zero() {
        let mock = MockCollection::new(vec![]);
        let err = TheoryEvidenceRetrieval::new(mock, settings(0, 0.5)).unwrap_err();
        assert!(matches!(
            err,
            TheoryEvidenceRetrievalError::InvalidSettings(_)
        ));
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
        assert!(matches!(
            err,
            TheoryEvidenceRetrievalError::InvalidSettings(_)
        ));
    }

    #[test]
    fn new_rejects_nan_score_threshold() {
        let mock = MockCollection::new(vec![]);
        let err = TheoryEvidenceRetrieval::new(mock, settings(5, f32::NAN)).unwrap_err();
        assert!(matches!(
            err,
            TheoryEvidenceRetrievalError::InvalidSettings(_)
        ));
    }

    #[test]
    fn new_rejects_positive_infinity_score_threshold() {
        let mock = MockCollection::new(vec![]);
        let err = TheoryEvidenceRetrieval::new(mock, settings(5, f32::INFINITY)).unwrap_err();
        assert!(matches!(
            err,
            TheoryEvidenceRetrievalError::InvalidSettings(_)
        ));
    }

    #[test]
    fn new_rejects_negative_infinity_score_threshold() {
        let mock = MockCollection::new(vec![]);
        let err = TheoryEvidenceRetrieval::new(mock, settings(5, f32::NEG_INFINITY)).unwrap_err();
        assert!(matches!(
            err,
            TheoryEvidenceRetrievalError::InvalidSettings(_)
        ));
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
        sut.retrieve(&request("raft leader election"))
            .await
            .unwrap();
        assert_eq!(
            mock.captured_requests()[0].user_query.0,
            "raft leader election"
        );
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
        let hits: Vec<_> = (0..5)
            .map(|i| hit(&format!("c{i}"), 0.9 - i as f32 * 0.1, "t"))
            .collect();
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
        let mock = MockCollection::new(vec![Err(TheoryChunksCollectionError::InvalidRequest(
            "boom".to_string(),
        ))]);
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
