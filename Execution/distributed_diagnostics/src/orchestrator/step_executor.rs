use crate::orchestrator::run_state::model::{RunStatus, StepError, StepKind, StepResultEnvelope};
use crate::orchestrator::run_state::view::{FinishedStepView, IterationView, RunStateView};
use crate::request_pipeline::candidate_card_retrieval::CandidateCardRetrieval;
use crate::request_pipeline::card_hydration::CardHydration;
use crate::request_pipeline::incident_evidence_retrieval::IncidentEvidenceRetrieval;
use crate::request_pipeline::input_normalization::InputNormalization;
use crate::request_pipeline::llm_structured_generation::LlmStructuredGeneration;
use crate::request_pipeline::prompt_context_assembly::PromptContextAssembly;
use crate::request_pipeline::query_structuring::QueryStructuring;
use crate::request_pipeline::response_validation_and_normalization::ResponseValidationAndNormalization;
use crate::request_pipeline::theory_evidence_retrieval::TheoryEvidenceRetrieval;
use crate::shared_types::{
    CandidateCardRetrievalOutput, CardHydrationOutput, IncidentEvidenceRetrievalOutput,
    LlmStructuredGenerationOutput, NormalizedUserRequest, PromptContextAssemblyOutput,
    QueryStructuringOutput, TheoryEvidenceRetrievalOutput, UserRequest,
};

#[derive(Debug)]
pub struct StepExecutor {
    input_normalization: InputNormalization,
    query_structuring: QueryStructuring,
    candidate_card_retrieval: CandidateCardRetrieval,
    card_hydration: CardHydration,
    incident_evidence_retrieval: IncidentEvidenceRetrieval,
    theory_evidence_retrieval: TheoryEvidenceRetrieval,
    prompt_context_assembly: PromptContextAssembly,
    llm_structured_generation: LlmStructuredGeneration,
    response_validation_and_normalization: ResponseValidationAndNormalization,
}

#[derive(Debug)]
pub struct StepExecutorModules {
    pub input_normalization: InputNormalization,
    pub query_structuring: QueryStructuring,
    pub candidate_card_retrieval: CandidateCardRetrieval,
    pub card_hydration: CardHydration,
    pub incident_evidence_retrieval: IncidentEvidenceRetrieval,
    pub theory_evidence_retrieval: TheoryEvidenceRetrieval,
    pub prompt_context_assembly: PromptContextAssembly,
    pub llm_structured_generation: LlmStructuredGeneration,
    pub response_validation_and_normalization: ResponseValidationAndNormalization,
}

impl StepExecutor {
    pub fn new(modules: StepExecutorModules) -> Self {
        Self {
            input_normalization: modules.input_normalization,
            query_structuring: modules.query_structuring,
            candidate_card_retrieval: modules.candidate_card_retrieval,
            card_hydration: modules.card_hydration,
            incident_evidence_retrieval: modules.incident_evidence_retrieval,
            theory_evidence_retrieval: modules.theory_evidence_retrieval,
            prompt_context_assembly: modules.prompt_context_assembly,
            llm_structured_generation: modules.llm_structured_generation,
            response_validation_and_normalization: modules.response_validation_and_normalization,
        }
    }

    pub async fn execute(
        &self,
        step: StepKind,
        state: RunStateView<'_>,
    ) -> Result<StepResultEnvelope, StepError> {
        let run_id_str = state.run_id().0.to_string();
        let iter_id_str = state
            .last_iteration()
            .map(|it| it.iteration_id().0.to_string())
            .unwrap_or_default();
        let span =
            crate::observability::dispatch_span(&run_id_str, &iter_id_str, step.as_ref());
        let _entered = span.enter();
        let result = self.execute_inner(step, state).await;
        span.record("status", if result.is_ok() { "ok" } else { "error" });
        result
    }

    async fn execute_inner(
        &self,
        step: StepKind,
        state: RunStateView<'_>,
    ) -> Result<StepResultEnvelope, StepError> {
        if state.status() == RunStatus::Archived {
            return Err(StepError::InvalidState {
                message: "cannot execute a step on an archived run".to_string(),
            });
        }

        let iteration = Self::require_iteration(state)?;

        if let Some(pending) = iteration.pending_step() {
            if pending.kind() != step {
                return Err(StepError::InvalidState {
                    message: "current iteration already has an in-flight pending step".to_string(),
                });
            }
        }

        match step {
            StepKind::UserInputReceived => Err(StepError::InvalidState {
                message: "UserInputReceived is recorded by begin_iteration and is not executable"
                    .to_string(),
            }),
            StepKind::InputNormalization => {
                let user_request = Self::read_user_input(iteration)?.clone();
                let result = self.input_normalization.normalize(user_request)?;
                Ok(StepResultEnvelope::InputNormalization(result))
            }
            StepKind::QueryStructuring => {
                let normalized_request = Self::read_normalized_request(iteration)?;
                let result = self.query_structuring.structure(normalized_request).await?;
                Ok(StepResultEnvelope::QueryStructuring(result))
            }
            StepKind::CandidateCardRetrieval => {
                let normalized_request = Self::read_normalized_request(iteration)?;
                let result = self
                    .candidate_card_retrieval
                    .retrieve(normalized_request)
                    .await?;
                Ok(StepResultEnvelope::CandidateCardRetrieval(result))
            }
            StepKind::CardHydration => {
                let candidates = Self::read_candidates(iteration)?;
                let result = self.card_hydration.hydrate(candidates).await?;
                Ok(StepResultEnvelope::CardHydration(result))
            }
            StepKind::IncidentEvidenceRetrieval => {
                let normalized_request = Self::read_normalized_request(iteration)?;
                let candidates = Self::read_candidates(iteration)?;
                let result = self
                    .incident_evidence_retrieval
                    .retrieve(normalized_request, candidates)
                    .await?;
                Ok(StepResultEnvelope::IncidentEvidenceRetrieval(result))
            }
            StepKind::TheoryEvidenceRetrieval => {
                let normalized_request = Self::read_normalized_request(iteration)?;
                let result = self
                    .theory_evidence_retrieval
                    .retrieve(normalized_request)
                    .await?;
                Ok(StepResultEnvelope::TheoryEvidenceRetrieval(result))
            }
            StepKind::PromptContextAssembly => {
                let normalized_request = Self::read_normalized_request(iteration)?;
                let structured_query = Self::read_structured_query(iteration)?;
                let hydrated_cards = Self::read_hydrated_cards(iteration)?;
                let incident_evidence = Self::read_incident_evidence(iteration)?;
                let theory_evidence = Self::read_theory_evidence(iteration)?;
                let result = self.prompt_context_assembly.assemble(
                    normalized_request,
                    structured_query,
                    hydrated_cards,
                    incident_evidence,
                    theory_evidence,
                )?;
                Ok(StepResultEnvelope::PromptContextAssembly(result))
            }
            StepKind::LlmStructuredGeneration => {
                let prompt_context = Self::read_prompt_context(iteration)?;
                let result = self.llm_structured_generation.generate(prompt_context).await?;
                Ok(StepResultEnvelope::LlmStructuredGeneration(result))
            }
            StepKind::ResponseValidationAndNormalization => {
                let llm_output = Self::read_llm_output(iteration)?;
                let result = self
                    .response_validation_and_normalization
                    .validate_and_normalize(llm_output)?;
                Ok(StepResultEnvelope::ResponseValidationAndNormalization(result))
            }
        }
    }

    // ─── Iteration helpers ────────────────────────────────────────────────────

    fn require_iteration<'a>(state: RunStateView<'a>) -> Result<IterationView<'a>, StepError> {
        state
            .last_iteration()
            .ok_or_else(|| StepError::MissingRequiredInput {
                message: "step execution requires a current iteration".to_string(),
            })
    }

    fn require_finished_step<'a>(
        iteration: IterationView<'a>,
        kind: StepKind,
    ) -> Result<FinishedStepView<'a>, StepError> {
        iteration
            .finished_step(kind)
            .ok_or_else(|| StepError::MissingRequiredInput {
                message: format!(
                    "required input step {:?} is not present in the current iteration",
                    kind
                ),
            })
    }

    fn extract_ok<'a, T, F>(view: FinishedStepView<'a>, extract: F) -> Result<&'a T, StepError>
    where
        F: Fn(&'a StepResultEnvelope) -> Option<&'a T>,
    {
        let kind = view.kind();
        match view.result() {
            Ok(envelope) => extract(envelope).ok_or_else(|| StepError::InvalidState {
                message: format!(
                    "required input step {:?} stored an unexpected result variant",
                    kind
                ),
            }),
            Err(error) => Err(StepError::MissingRequiredInput {
                message: format!(
                    "required input step {:?} did not complete successfully: {}",
                    kind, error
                ),
            }),
        }
    }

    // ─── Per-step payload readers ─────────────────────────────────────────────

    fn read_user_input<'a>(iteration: IterationView<'a>) -> Result<&'a UserRequest, StepError> {
        let view = Self::require_finished_step(iteration, StepKind::UserInputReceived)?;
        Self::extract_ok(view, |e| match e {
            StepResultEnvelope::UserInputReceived(r) => Some(r),
            _ => None,
        })
    }

    fn read_normalized_request<'a>(
        iteration: IterationView<'a>,
    ) -> Result<&'a NormalizedUserRequest, StepError> {
        let view = Self::require_finished_step(iteration, StepKind::InputNormalization)?;
        Self::extract_ok(view, |e| match e {
            StepResultEnvelope::InputNormalization(r) => Some(r),
            _ => None,
        })
    }

    fn read_candidates<'a>(
        iteration: IterationView<'a>,
    ) -> Result<&'a CandidateCardRetrievalOutput, StepError> {
        let view = Self::require_finished_step(iteration, StepKind::CandidateCardRetrieval)?;
        Self::extract_ok(view, |e| match e {
            StepResultEnvelope::CandidateCardRetrieval(r) => Some(r),
            _ => None,
        })
    }

    fn read_structured_query<'a>(
        iteration: IterationView<'a>,
    ) -> Result<&'a QueryStructuringOutput, StepError> {
        let view = Self::require_finished_step(iteration, StepKind::QueryStructuring)?;
        Self::extract_ok(view, |e| match e {
            StepResultEnvelope::QueryStructuring(r) => Some(r),
            _ => None,
        })
    }

    fn read_hydrated_cards<'a>(
        iteration: IterationView<'a>,
    ) -> Result<&'a CardHydrationOutput, StepError> {
        let view = Self::require_finished_step(iteration, StepKind::CardHydration)?;
        Self::extract_ok(view, |e| match e {
            StepResultEnvelope::CardHydration(r) => Some(r),
            _ => None,
        })
    }

    fn read_incident_evidence<'a>(
        iteration: IterationView<'a>,
    ) -> Result<&'a IncidentEvidenceRetrievalOutput, StepError> {
        let view = Self::require_finished_step(iteration, StepKind::IncidentEvidenceRetrieval)?;
        Self::extract_ok(view, |e| match e {
            StepResultEnvelope::IncidentEvidenceRetrieval(r) => Some(r),
            _ => None,
        })
    }

    fn read_theory_evidence<'a>(
        iteration: IterationView<'a>,
    ) -> Result<&'a TheoryEvidenceRetrievalOutput, StepError> {
        let view = Self::require_finished_step(iteration, StepKind::TheoryEvidenceRetrieval)?;
        Self::extract_ok(view, |e| match e {
            StepResultEnvelope::TheoryEvidenceRetrieval(r) => Some(r),
            _ => None,
        })
    }

    fn read_prompt_context<'a>(
        iteration: IterationView<'a>,
    ) -> Result<&'a PromptContextAssemblyOutput, StepError> {
        let view = Self::require_finished_step(iteration, StepKind::PromptContextAssembly)?;
        Self::extract_ok(view, |e| match e {
            StepResultEnvelope::PromptContextAssembly(r) => Some(r),
            _ => None,
        })
    }

    fn read_llm_output<'a>(
        iteration: IterationView<'a>,
    ) -> Result<&'a LlmStructuredGenerationOutput, StepError> {
        let view = Self::require_finished_step(iteration, StepKind::LlmStructuredGeneration)?;
        Self::extract_ok(view, |e| match e {
            StepResultEnvelope::LlmStructuredGeneration(r) => Some(r),
            _ => None,
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_clients::model::model_client::ModelClientError;
    use crate::api_clients::model::ModelClient;
    use crate::api_clients::model::shared_types::{
        ModelFinishReason, ModelGenerationRequest, ModelGenerationResponse,
    };
    use crate::api_clients::qdrant::cards_collection::{
        CardSearchRequest, CardSearchResult, CardsCollection, CardsCollectionError,
    };
    use crate::api_clients::qdrant::practice_chunks_collection::{
        PracticeChunkSearchRequest, PracticeChunkSearchResult, PracticeChunksCollection,
        PracticeChunksCollectionError,
    };
    use crate::api_clients::qdrant::theory_chunks_collection::{
        TheoryChunkSearchRequest, TheoryChunkSearchResult, TheoryChunksCollection,
        TheoryChunksCollectionError,
    };
    use crate::config::{
        ChunkPackingSettings, ChunkPackingSource, ChunkRolePackingSettings,
        CollectionRetrievalSettings, CollectionSettings, DenseCollectionSettings,
        InputNormalizationSettings, LlmStructuredGenerationSettings, PromptContextSettings,
        QueryStructuringSettings,
    };
    use crate::orchestrator::run_state::model::{
        FinishedStepRecord, RunId, RunIteration, RunIterationId, RunState, RunStatus, StepError,
        StepKind, StepRecord, StepRecordId, StepResultEnvelope,
    };
    use crate::orchestrator::run_state::view::RunStateView;
    use crate::request_pipeline::candidate_card_retrieval::CandidateCardRetrieval;
    use crate::request_pipeline::card_hydration::CardHydration;
    use crate::request_pipeline::incident_evidence_retrieval::IncidentEvidenceRetrieval;
    use crate::request_pipeline::input_normalization::InputNormalization;
    use crate::request_pipeline::llm_structured_generation::LlmStructuredGeneration;
    use crate::request_pipeline::prompt_context_assembly::PromptContextAssembly;
    use crate::request_pipeline::query_structuring::QueryStructuring;
    use crate::request_pipeline::response_validation_and_normalization::ResponseValidationAndNormalization;
    use crate::request_pipeline::theory_evidence_retrieval::TheoryEvidenceRetrieval;
    use crate::shared_types::{
        CandidateCardRetrievalOutput, CardHydrationOutput, IncidentCard, IncidentEvidenceChunk,
        IncidentEvidenceRetrievalOutput, LlmStructuredGenerationOutput, ModelTokenUsage,
        NormalizedUserRequest, PromptContextAssemblyOutput, QueryStructuringOutput,
        StructuredUserQuery, StructuredUserQueryConfidence, TheoryEvidenceRetrievalOutput,
        UserRequest,
    };
    use crate::test_utils::{
        postgres_store::MockPostgresIncidentCardStore, populate_tokenizer_cache, TempArtifactDir,
    };
    use crate::utils::retry::{RetryBackoffKind, RetryPolicyConfig};
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::Arc;
    use uuid::Uuid;

    // ─── Mock: ModelClient ────────────────────────────────────────────────────

    struct MockModelClient {
        response: ModelGenerationResponse,
    }

    impl MockModelClient {
        fn returning(response: ModelGenerationResponse) -> Arc<Self> {
            Arc::new(Self { response })
        }
    }

    #[async_trait]
    impl ModelClient for MockModelClient {
        async fn generate(
            &self,
            _request: &ModelGenerationRequest,
        ) -> Result<ModelGenerationResponse, ModelClientError> {
            Ok(self.response.clone())
        }
    }

    // ─── Mock: CardsCollection ────────────────────────────────────────────────

    struct MockCardsCollection;

    #[async_trait]
    impl CardsCollection for MockCardsCollection {
        async fn search(
            &self,
            _request: &CardSearchRequest,
        ) -> Result<CardSearchResult, CardsCollectionError> {
            Ok(CardSearchResult { hits: vec![] })
        }
    }

    // ─── Mock: PracticeChunksCollection ──────────────────────────────────────

    struct MockPracticeChunksCollection;

    #[async_trait]
    impl PracticeChunksCollection for MockPracticeChunksCollection {
        async fn search(
            &self,
            _request: &PracticeChunkSearchRequest,
        ) -> Result<PracticeChunkSearchResult, PracticeChunksCollectionError> {
            Ok(PracticeChunkSearchResult { hits: vec![] })
        }
    }

    // ─── Mock: TheoryChunksCollection ────────────────────────────────────────

    struct MockTheoryChunksCollection;

    #[async_trait]
    impl TheoryChunksCollection for MockTheoryChunksCollection {
        async fn search(
            &self,
            _request: &TheoryChunkSearchRequest,
        ) -> Result<TheoryChunkSearchResult, TheoryChunksCollectionError> {
            Ok(TheoryChunkSearchResult { hits: vec![] })
        }
    }

    // ─── Fixtures ─────────────────────────────────────────────────────────────

    const TOKENIZER_SOURCE: &str = "step-executor-tests/tok";

    const VOCAB_JSON: &str = r#"{
        "canonical_symptoms": ["high_latency"],
        "affected_components": ["api_gateway"],
        "failure_mode_candidates": ["overload"],
        "violated_properties": ["availability"]
    }"#;

    const QS_PROMPT_JSON: &str = r#"{
        "version": "v1",
        "system_prompt": "You are a helpful assistant.",
        "user_template": "Query: {{normalized_query}}\nVocabulary: {{controlled_vocabulary_json}}"
    }"#;

    fn new_record_id() -> StepRecordId {
        StepRecordId(Uuid::new_v4())
    }

    fn new_iteration_id() -> RunIterationId {
        RunIterationId(Uuid::new_v4())
    }

    fn finished_ok(kind: StepKind, result: StepResultEnvelope) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: new_record_id(),
            step: kind,
            started_at: now,
            finished_at: now,
            result: Ok(result),
        })
    }

    fn finished_err(kind: StepKind, error: StepError) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: new_record_id(),
            step: kind,
            started_at: now,
            finished_at: now,
            result: Err(error),
        })
    }

    fn run_with_records(records: Vec<StepRecord>) -> RunState {
        let now = Utc::now();
        RunState {
            run_id: RunId(Uuid::new_v4()),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![RunIteration {
                iteration_id: new_iteration_id(),
                step_records: records,
            }],
        }
    }

    fn empty_run() -> RunState {
        let now = Utc::now();
        RunState {
            run_id: RunId(Uuid::new_v4()),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![],
        }
    }

    fn user_request() -> UserRequest {
        UserRequest {
            query: "service down".to_string(),
        }
    }

    fn normalized_request() -> NormalizedUserRequest {
        NormalizedUserRequest {
            query: "service down".to_string(),
            input_token_count: 2,
        }
    }

    fn empty_token_usage() -> ModelTokenUsage {
        ModelTokenUsage {
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        }
    }

    fn minimal_structured_query() -> QueryStructuringOutput {
        QueryStructuringOutput {
            structured_query: StructuredUserQuery {
                intent: "diagnose failure".to_string(),
                scenario: "Service is down.".to_string(),
                symptoms: vec![],
                affected_subsystems: vec![],
                failure_modes: vec![],
                system_properties: vec![],
                entities: vec![],
                constraints: vec![],
                triggers: vec![],
                observability_signals: vec![],
                unresolved_terms: vec![],
                rejected_nearby_terms: vec![],
                confidence: StructuredUserQueryConfidence::Medium,
            },
            token_usage: empty_token_usage(),
        }
    }

    fn empty_candidates() -> CandidateCardRetrievalOutput {
        CandidateCardRetrievalOutput {
            primary: None,
            alternatives: vec![],
        }
    }

    fn empty_card_hydration() -> CardHydrationOutput {
        CardHydrationOutput {
            primary: None,
            alternatives: vec![],
        }
    }

    fn card_hydration_with_primary() -> CardHydrationOutput {
        CardHydrationOutput {
            primary: Some(minimal_incident_card("case-1")),
            alternatives: vec![],
        }
    }

    fn minimal_incident_card(case_id: &str) -> IncidentCard {
        IncidentCard {
            case_id: case_id.to_string(),
            title: format!("Incident {case_id}"),
            source_type: "report".to_string(),
            source_name: "source".to_string(),
            source_path: "path".to_string(),
            vendor_or_project: None,
            system_type: None,
            version_tested: None,
            report_date: None,
            short_summary: "summary".to_string(),
            canonical_symptoms: vec![],
            affected_components: vec![],
            failure_mode_candidates: vec![],
            observed_phases: vec![],
            incident_phases: vec![],
            turning_points: vec![],
            candidate_explanations: vec![],
            diagnostic_patterns: vec![],
            discriminating_checks: vec![],
            expected_observations: vec![],
            investigation_steps: vec![],
            root_cause_summary: None,
            reasoning_summary: None,
            mitigations_or_workarounds: vec![],
            prevention_or_design_followups: vec![],
            claimed_guarantees: vec![],
            violated_properties: vec![],
            resolution_status: None,
            fix_versions: vec![],
            confidence_notes: vec![],
            source_refs: vec![],
        }
    }

    fn empty_incident_evidence() -> IncidentEvidenceRetrievalOutput {
        IncidentEvidenceRetrievalOutput {
            primary_chunks: vec![],
            alternative_chunks: vec![],
        }
    }

    fn incident_evidence_with_two_primary_chunks() -> IncidentEvidenceRetrievalOutput {
        IncidentEvidenceRetrievalOutput {
            primary_chunks: vec![
                IncidentEvidenceChunk {
                    chunk_id: "chunk-1".to_string(),
                    case_id: "case-1".to_string(),
                    score: 0.9,
                    chunk_tags: vec![],
                    text: "First evidence chunk".to_string(),
                },
                IncidentEvidenceChunk {
                    chunk_id: "chunk-2".to_string(),
                    case_id: "case-1".to_string(),
                    score: 0.8,
                    chunk_tags: vec![],
                    text: "Second evidence chunk".to_string(),
                },
            ],
            alternative_chunks: vec![],
        }
    }

    fn empty_theory_evidence() -> TheoryEvidenceRetrievalOutput {
        TheoryEvidenceRetrievalOutput { chunks: vec![] }
    }

    fn valid_llm_output() -> LlmStructuredGenerationOutput {
        LlmStructuredGenerationOutput {
            response_json: serde_json::json!({
                "problem_understanding": "Service is down",
                "similar_practical_context": "Similar past incidents",
                "active_hypotheses": ["overload", "network partition"],
                "first_check": "Check logs",
                "result_interpretation": {
                    "supports_primary_if": "Logs show errors",
                    "supports_competing_if": "No errors in logs",
                    "inconclusive_if": null
                },
                "competing_interpretation": null
            }),
            token_usage: empty_token_usage(),
        }
    }

    fn prompt_context_output_with_nonempty_prompt() -> PromptContextAssemblyOutput {
        PromptContextAssemblyOutput {
            prompt: "Diagnose the incident based on the evidence provided.".to_string(),
            incident_evidence_chunks: vec![],
            theory_chunks: vec![],
        }
    }

    // ─── Config helpers ───────────────────────────────────────────────────────

    fn collection_retrieval_settings(top_k: usize) -> CollectionRetrievalSettings {
        CollectionRetrievalSettings {
            top_k,
            score_threshold: 0.5,
            max_alternatives: 0,
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
                vector_name: "vec".to_string(),
                corpus_version: "1".to_string(),
            }),
        }
    }

    fn prompt_context_settings() -> PromptContextSettings {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let asset_path = format!(
            "{manifest}/../../Specification/runtime/request_pipeline/prompt_context_assembly/diagnostic_response_prompt_baseline.manual_test.json"
        );
        PromptContextSettings {
            prompt_asset_path: asset_path,
            chunk_packing: ChunkPackingSettings {
                evidence_for_match: ChunkRolePackingSettings {
                    source: ChunkPackingSource::PrimaryIncident,
                    limit: 1,
                    per_case_limit: None,
                    fallback_to_any_chunk: true,
                    tag_priority: vec![],
                },
                first_check_hint: ChunkRolePackingSettings {
                    source: ChunkPackingSource::PrimaryIncident,
                    limit: 1,
                    per_case_limit: None,
                    fallback_to_any_chunk: true,
                    tag_priority: vec![],
                },
                supporting_explanation: ChunkRolePackingSettings {
                    source: ChunkPackingSource::PrimaryIncident,
                    limit: 0,
                    per_case_limit: None,
                    fallback_to_any_chunk: false,
                    tag_priority: vec![],
                },
                alternative_context: ChunkRolePackingSettings {
                    source: ChunkPackingSource::AlternativeIncident,
                    limit: 0,
                    per_case_limit: None,
                    fallback_to_any_chunk: false,
                    tag_priority: vec![],
                },
                mechanism_explanation: ChunkRolePackingSettings {
                    source: ChunkPackingSource::Theory,
                    limit: 0,
                    per_case_limit: None,
                    fallback_to_any_chunk: false,
                    tag_priority: vec![],
                },
            },
        }
    }

    fn valid_qs_model_response() -> ModelGenerationResponse {
        ModelGenerationResponse {
            content: serde_json::json!({
                "intent": "diagnose failure",
                "scenario": "Service is down.",
                "symptoms": [],
                "affected_subsystems": [],
                "failure_modes": [],
                "system_properties": [],
                "entities": [],
                "constraints": [],
                "triggers": [],
                "observability_signals": [],
                "unresolved_terms": [],
                "rejected_nearby_terms": [],
                "confidence": "medium"
            })
            .to_string(),
            finish_reason: Some(ModelFinishReason::Stop),
            prompt_tokens: Some(100),
            completion_tokens: Some(50),
            total_tokens: Some(150),
        }
    }

    fn valid_llm_model_response() -> ModelGenerationResponse {
        ModelGenerationResponse {
            content: serde_json::json!({
                "problem_understanding": "Service is down",
                "similar_practical_context": "Similar incidents in the past",
                "active_hypotheses": ["overload"],
                "first_check": "Check service logs",
                "result_interpretation": {
                    "supports_primary_if": "Logs show error spikes",
                    "supports_competing_if": "No errors in logs",
                    "inconclusive_if": null
                },
                "competing_interpretation": null
            })
            .to_string(),
            finish_reason: Some(ModelFinishReason::Stop),
            prompt_tokens: Some(200),
            completion_tokens: Some(100),
            total_tokens: Some(300),
        }
    }

    // ─── Executor factory ─────────────────────────────────────────────────────

    async fn make_executor(dir: &TempArtifactDir) -> StepExecutor {
        make_executor_with_model_clients(
            dir,
            MockModelClient::returning(valid_qs_model_response()),
            MockModelClient::returning(valid_llm_model_response()),
        )
        .await
    }

    async fn make_executor_with_model_clients(
        dir: &TempArtifactDir,
        qs_client: Arc<dyn ModelClient>,
        llm_client: Arc<dyn ModelClient>,
    ) -> StepExecutor {
        populate_tokenizer_cache(TOKENIZER_SOURCE);

        let vocab_path = dir
            .write_json("vocab.json", VOCAB_JSON)
            .to_str()
            .unwrap()
            .to_string();
        let qs_prompt_path = dir
            .write_json("qs_prompt.json", QS_PROMPT_JSON)
            .to_str()
            .unwrap()
            .to_string();

        let input_normalization = InputNormalization::new(InputNormalizationSettings {
            tokenizer_source: TOKENIZER_SOURCE.to_string(),
            max_input_tokens: 100,
        })
        .await
        .expect("InputNormalization::new");

        let query_structuring = QueryStructuring::new(
            QueryStructuringSettings {
                controlled_vocabulary_path: vocab_path,
                prompt_asset_path: qs_prompt_path,
                max_output_tokens: 256,
            },
            qs_client,
        )
        .expect("QueryStructuring::new");

        let candidate_card_retrieval = CandidateCardRetrieval::new(
            collection_retrieval_settings(3),
            Arc::new(MockCardsCollection),
        )
        .expect("CandidateCardRetrieval::new");

        let card_hydration =
            CardHydration::new(Arc::new(MockPostgresIncidentCardStore::new(vec![])));

        let incident_evidence_retrieval = IncidentEvidenceRetrieval::new(
            Arc::new(MockPracticeChunksCollection),
            collection_retrieval_settings(5),
        )
        .expect("IncidentEvidenceRetrieval::new");

        let theory_evidence_retrieval = TheoryEvidenceRetrieval::new(
            Arc::new(MockTheoryChunksCollection),
            collection_retrieval_settings(5),
        )
        .expect("TheoryEvidenceRetrieval::new");

        let prompt_context_assembly =
            PromptContextAssembly::new(prompt_context_settings()).expect("PromptContextAssembly::new");

        let llm_structured_generation = LlmStructuredGeneration::new(
            LlmStructuredGenerationSettings {
                max_output_tokens: 512,
            },
            llm_client,
        )
        .expect("LlmStructuredGeneration::new");

        let response_validation_and_normalization = ResponseValidationAndNormalization::new();

        StepExecutor::new(StepExecutorModules {
            input_normalization,
            query_structuring,
            candidate_card_retrieval,
            card_hydration,
            incident_evidence_retrieval,
            theory_evidence_retrieval,
            prompt_context_assembly,
            llm_structured_generation,
            response_validation_and_normalization,
        })
    }

    // ─── Constructor ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn new_stores_all_modules_without_extra_validation() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let debug = format!("{executor:?}");
        assert!(debug.contains("StepExecutor"));
    }

    // ─── Missing iteration ────────────────────────────────────────────────────

    #[tokio::test]
    async fn execute_returns_missing_required_input_when_no_iteration() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let state = empty_run();
        let view = RunStateView::new(&state);
        let err = executor
            .execute(StepKind::InputNormalization, view)
            .await
            .unwrap_err();
        assert!(matches!(err, StepError::MissingRequiredInput { .. }));
    }

    // ─── UserInputReceived rejected ───────────────────────────────────────────

    #[tokio::test]
    async fn execute_returns_invalid_state_for_user_input_received() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let state = run_with_records(vec![finished_ok(
            StepKind::UserInputReceived,
            StepResultEnvelope::UserInputReceived(user_request()),
        )]);
        let view = RunStateView::new(&state);
        let err = executor
            .execute(StepKind::UserInputReceived, view)
            .await
            .unwrap_err();
        assert!(matches!(err, StepError::InvalidState { .. }));
    }

    // ─── Archived run guard ───────────────────────────────────────────────────

    #[tokio::test]
    async fn execute_returns_invalid_state_when_run_is_archived() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let now = Utc::now();
        let state = RunState {
            run_id: RunId(Uuid::new_v4()),
            status: RunStatus::Archived,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![RunIteration {
                iteration_id: new_iteration_id(),
                step_records: vec![finished_ok(
                    StepKind::UserInputReceived,
                    StepResultEnvelope::UserInputReceived(user_request()),
                )],
            }],
        };
        let view = RunStateView::new(&state);
        let err = executor
            .execute(StepKind::InputNormalization, view)
            .await
            .unwrap_err();
        assert!(matches!(err, StepError::InvalidState { .. }));
    }

    // ─── Pending step guard ───────────────────────────────────────────────────

    #[tokio::test]
    async fn execute_allows_pending_step_for_the_same_step() {
        use crate::orchestrator::run_state::model::PendingStepRecord;
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let now = Utc::now();
        let state = RunState {
            run_id: RunId(Uuid::new_v4()),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![RunIteration {
                iteration_id: new_iteration_id(),
                step_records: vec![
                    finished_ok(
                        StepKind::UserInputReceived,
                        StepResultEnvelope::UserInputReceived(user_request()),
                    ),
                    StepRecord::Pending(PendingStepRecord {
                        record_id: new_record_id(),
                        step: StepKind::InputNormalization,
                        started_at: now,
                    }),
                ],
            }],
        };
        let view = RunStateView::new(&state);
        let result = executor
            .execute(StepKind::InputNormalization, view)
            .await
            .unwrap();
        assert!(matches!(result, StepResultEnvelope::InputNormalization(_)));
    }

    #[tokio::test]
    async fn execute_returns_invalid_state_when_current_iteration_has_pending_step_for_other_step() {
        use crate::orchestrator::run_state::model::PendingStepRecord;
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let now = Utc::now();
        let state = RunState {
            run_id: RunId(Uuid::new_v4()),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![RunIteration {
                iteration_id: new_iteration_id(),
                step_records: vec![
                    finished_ok(
                        StepKind::UserInputReceived,
                        StepResultEnvelope::UserInputReceived(user_request()),
                    ),
                    StepRecord::Pending(PendingStepRecord {
                        record_id: new_record_id(),
                        step: StepKind::QueryStructuring,
                        started_at: now,
                    }),
                ],
            }],
        };
        let view = RunStateView::new(&state);
        let err = executor
            .execute(StepKind::InputNormalization, view)
            .await
            .unwrap_err();
        assert!(matches!(err, StepError::InvalidState { .. }));
    }

    // ─── Missing required input ───────────────────────────────────────────────

    #[tokio::test]
    async fn execute_returns_missing_required_input_when_required_input_absent() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        // InputNormalization requires UserInputReceived — not present in iteration.
        let state = run_with_records(vec![]);
        let view = RunStateView::new(&state);
        let err = executor
            .execute(StepKind::InputNormalization, view)
            .await
            .unwrap_err();
        assert!(matches!(err, StepError::MissingRequiredInput { .. }));
    }

    // ─── Errored prerequisite ─────────────────────────────────────────────────

    #[tokio::test]
    async fn execute_returns_missing_required_input_when_prerequisite_errored() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        // QueryStructuring requires InputNormalization — provide it as an error.
        let state = run_with_records(vec![finished_err(
            StepKind::InputNormalization,
            StepError::MissingRequiredInput {
                message: "upstream failure".to_string(),
            },
        )]);
        let view = RunStateView::new(&state);
        let err = executor
            .execute(StepKind::QueryStructuring, view)
            .await
            .unwrap_err();
        assert!(matches!(err, StepError::MissingRequiredInput { .. }));
    }

    // ─── Mismatched variant ───────────────────────────────────────────────────

    #[tokio::test]
    async fn execute_returns_invalid_state_when_prerequisite_variant_mismatched() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        // InputNormalization reads UserInputReceived, but the stored envelope is
        // InputNormalization — wrong variant for that step kind.
        let state = run_with_records(vec![finished_ok(
            StepKind::UserInputReceived,
            StepResultEnvelope::InputNormalization(normalized_request()),
        )]);
        let view = RunStateView::new(&state);
        let err = executor
            .execute(StepKind::InputNormalization, view)
            .await
            .unwrap_err();
        assert!(matches!(err, StepError::InvalidState { .. }));
    }

    // ─── Multi-input steps ────────────────────────────────────────────────────

    #[tokio::test]
    async fn execute_incident_evidence_retrieval_fails_when_candidate_card_retrieval_absent() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        // Provide InputNormalization but not CandidateCardRetrieval.
        let state = run_with_records(vec![finished_ok(
            StepKind::InputNormalization,
            StepResultEnvelope::InputNormalization(normalized_request()),
        )]);
        let view = RunStateView::new(&state);
        let err = executor
            .execute(StepKind::IncidentEvidenceRetrieval, view)
            .await
            .unwrap_err();
        assert!(matches!(err, StepError::MissingRequiredInput { .. }));
    }

    #[tokio::test]
    async fn execute_prompt_context_assembly_fails_when_any_of_five_inputs_absent() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        // Provide 4 of 5 required inputs — omit TheoryEvidenceRetrieval.
        let state = run_with_records(vec![
            finished_ok(
                StepKind::InputNormalization,
                StepResultEnvelope::InputNormalization(normalized_request()),
            ),
            finished_ok(
                StepKind::QueryStructuring,
                StepResultEnvelope::QueryStructuring(minimal_structured_query()),
            ),
            finished_ok(
                StepKind::CardHydration,
                StepResultEnvelope::CardHydration(empty_card_hydration()),
            ),
            finished_ok(
                StepKind::IncidentEvidenceRetrieval,
                StepResultEnvelope::IncidentEvidenceRetrieval(empty_incident_evidence()),
            ),
        ]);
        let view = RunStateView::new(&state);
        let err = executor
            .execute(StepKind::PromptContextAssembly, view)
            .await
            .unwrap_err();
        assert!(matches!(err, StepError::MissingRequiredInput { .. }));
    }

    // ─── Current iteration only ───────────────────────────────────────────────

    #[tokio::test]
    async fn execute_reads_only_current_iteration_not_older_ones() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let now = Utc::now();
        // Older iteration has UserInputReceived; current (last) iteration is empty.
        let state = RunState {
            run_id: RunId(Uuid::new_v4()),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 1,
            iterations: vec![
                RunIteration {
                    iteration_id: new_iteration_id(),
                    step_records: vec![finished_ok(
                        StepKind::UserInputReceived,
                        StepResultEnvelope::UserInputReceived(user_request()),
                    )],
                },
                RunIteration {
                    iteration_id: new_iteration_id(),
                    step_records: vec![],
                },
            ],
        };
        let view = RunStateView::new(&state);
        let err = executor
            .execute(StepKind::InputNormalization, view)
            .await
            .unwrap_err();
        assert!(
            matches!(err, StepError::MissingRequiredInput { .. }),
            "must fail because current iteration has no UserInputReceived"
        );
    }

    // ─── Happy path: each step dispatches and returns correct variant ─────────

    #[tokio::test]
    async fn execute_dispatches_input_normalization_and_returns_correct_variant() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let state = run_with_records(vec![finished_ok(
            StepKind::UserInputReceived,
            StepResultEnvelope::UserInputReceived(user_request()),
        )]);
        let view = RunStateView::new(&state);
        let result = executor
            .execute(StepKind::InputNormalization, view)
            .await
            .expect("InputNormalization must succeed");
        assert!(matches!(result, StepResultEnvelope::InputNormalization(_)));
    }

    #[tokio::test]
    async fn execute_dispatches_query_structuring_and_returns_correct_variant() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let state = run_with_records(vec![finished_ok(
            StepKind::InputNormalization,
            StepResultEnvelope::InputNormalization(normalized_request()),
        )]);
        let view = RunStateView::new(&state);
        let result = executor
            .execute(StepKind::QueryStructuring, view)
            .await
            .expect("QueryStructuring must succeed");
        assert!(matches!(result, StepResultEnvelope::QueryStructuring(_)));
    }

    #[tokio::test]
    async fn execute_dispatches_candidate_card_retrieval_and_returns_correct_variant() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let state = run_with_records(vec![finished_ok(
            StepKind::InputNormalization,
            StepResultEnvelope::InputNormalization(normalized_request()),
        )]);
        let view = RunStateView::new(&state);
        let result = executor
            .execute(StepKind::CandidateCardRetrieval, view)
            .await
            .expect("CandidateCardRetrieval must succeed");
        assert!(matches!(result, StepResultEnvelope::CandidateCardRetrieval(_)));
    }

    #[tokio::test]
    async fn execute_dispatches_card_hydration_and_returns_correct_variant() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        // Empty candidates cause CardHydration to return early without any store call.
        let state = run_with_records(vec![finished_ok(
            StepKind::CandidateCardRetrieval,
            StepResultEnvelope::CandidateCardRetrieval(empty_candidates()),
        )]);
        let view = RunStateView::new(&state);
        let result = executor
            .execute(StepKind::CardHydration, view)
            .await
            .expect("CardHydration must succeed");
        assert!(matches!(result, StepResultEnvelope::CardHydration(_)));
    }

    #[tokio::test]
    async fn execute_dispatches_incident_evidence_retrieval_and_returns_correct_variant() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        // Empty candidates cause IncidentEvidenceRetrieval to return early.
        let state = run_with_records(vec![
            finished_ok(
                StepKind::InputNormalization,
                StepResultEnvelope::InputNormalization(normalized_request()),
            ),
            finished_ok(
                StepKind::CandidateCardRetrieval,
                StepResultEnvelope::CandidateCardRetrieval(empty_candidates()),
            ),
        ]);
        let view = RunStateView::new(&state);
        let result = executor
            .execute(StepKind::IncidentEvidenceRetrieval, view)
            .await
            .expect("IncidentEvidenceRetrieval must succeed");
        assert!(matches!(result, StepResultEnvelope::IncidentEvidenceRetrieval(_)));
    }

    #[tokio::test]
    async fn execute_dispatches_theory_evidence_retrieval_and_returns_correct_variant() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let state = run_with_records(vec![finished_ok(
            StepKind::InputNormalization,
            StepResultEnvelope::InputNormalization(normalized_request()),
        )]);
        let view = RunStateView::new(&state);
        let result = executor
            .execute(StepKind::TheoryEvidenceRetrieval, view)
            .await
            .expect("TheoryEvidenceRetrieval must succeed");
        assert!(matches!(result, StepResultEnvelope::TheoryEvidenceRetrieval(_)));
    }

    #[tokio::test]
    async fn execute_dispatches_prompt_context_assembly_and_returns_correct_variant() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        // Provide all 5 required inputs. CardHydration must have a primary card so
        // PromptContextAssembly can find the matched incident card.
        // Two incident chunks ensure evidence_for_match and first_check_hint each
        // get a distinct chunk (both roles have tag_priority=[] + fallback=true).
        let state = run_with_records(vec![
            finished_ok(
                StepKind::InputNormalization,
                StepResultEnvelope::InputNormalization(normalized_request()),
            ),
            finished_ok(
                StepKind::QueryStructuring,
                StepResultEnvelope::QueryStructuring(minimal_structured_query()),
            ),
            finished_ok(
                StepKind::CardHydration,
                StepResultEnvelope::CardHydration(card_hydration_with_primary()),
            ),
            finished_ok(
                StepKind::IncidentEvidenceRetrieval,
                StepResultEnvelope::IncidentEvidenceRetrieval(
                    incident_evidence_with_two_primary_chunks(),
                ),
            ),
            finished_ok(
                StepKind::TheoryEvidenceRetrieval,
                StepResultEnvelope::TheoryEvidenceRetrieval(empty_theory_evidence()),
            ),
        ]);
        let view = RunStateView::new(&state);
        let result = executor
            .execute(StepKind::PromptContextAssembly, view)
            .await
            .expect("PromptContextAssembly must succeed");
        assert!(matches!(result, StepResultEnvelope::PromptContextAssembly(_)));
    }

    #[tokio::test]
    async fn execute_dispatches_llm_structured_generation_and_returns_correct_variant() {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let state = run_with_records(vec![finished_ok(
            StepKind::PromptContextAssembly,
            StepResultEnvelope::PromptContextAssembly(prompt_context_output_with_nonempty_prompt()),
        )]);
        let view = RunStateView::new(&state);
        let result = executor
            .execute(StepKind::LlmStructuredGeneration, view)
            .await
            .expect("LlmStructuredGeneration must succeed");
        assert!(matches!(result, StepResultEnvelope::LlmStructuredGeneration(_)));
    }

    #[tokio::test]
    async fn execute_dispatches_response_validation_and_normalization_and_returns_correct_variant()
    {
        let dir = TempArtifactDir::new();
        let executor = make_executor(&dir).await;
        let state = run_with_records(vec![finished_ok(
            StepKind::LlmStructuredGeneration,
            StepResultEnvelope::LlmStructuredGeneration(valid_llm_output()),
        )]);
        let view = RunStateView::new(&state);
        let result = executor
            .execute(StepKind::ResponseValidationAndNormalization, view)
            .await
            .expect("ResponseValidationAndNormalization must succeed");
        assert!(matches!(
            result,
            StepResultEnvelope::ResponseValidationAndNormalization(_)
        ));
    }
}
