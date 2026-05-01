use async_trait::async_trait;
use distributed_diagnostics::api_clients::model::{
    ModelClient, ModelGenerationRequest, ModelMessage, ModelMessageRole, ModelResponseMode,
    RetryBackoffKind, RetryPolicyConfig, TogetherModelClient, TogetherModelClientConfig,
};
use serde_json::{json, Value};

use crate::config::JudgeSettings;
use crate::snapshot::DiagnosticEvalIterationSnapshot;
use crate::storage::{
    EvalStage, JudgeLlmCallRow, JudgeResultRow, PostgresEvalStore, StorageError,
};
use crate::subject_preparation::PreparedJudgeSubject;
use crate::suites::{JudgeSuiteCatalog, JudgeSuiteDefinition, SuiteCatalogError};

const FINAL_NO_ROOT_CAUSE_CLAIM: &str = "final_no_root_cause_claim";

#[derive(Debug, Clone, PartialEq)]
pub struct JudgeCallRequest {
    pub suite_name: String,
    pub suite_id: String,
    pub suite_version: String,
    pub prompt_version: String,
    pub prompt_text: String,
    pub input_payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JudgeCallResponse {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub token_count_source: String,
    pub raw_response: Value,
    pub normalized_result: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum JudgeExecutionError {
    #[error(transparent)]
    SuiteCatalog(#[from] SuiteCatalogError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("suite is not supported yet in the first implementation slice: {0}")]
    UnsupportedSuite(String),
    #[error("suite catalog is missing definition for {0}")]
    MissingSuiteDefinition(String),
    #[error("normalized judge response is missing required field: {0}")]
    MissingNormalizedField(&'static str),
    #[error("normalized judge response has invalid score")]
    InvalidScore,
    #[error("judge client failure: {0}")]
    Client(String),
}

#[async_trait]
pub trait JudgeClient: Send + Sync {
    async fn execute(
        &self,
        request: JudgeCallRequest,
    ) -> Result<JudgeCallResponse, JudgeExecutionError>;
}

pub struct TogetherJudgeClient {
    inner: TogetherModelClient,
}

impl TogetherJudgeClient {
    pub fn from_settings(
        judge_settings: &JudgeSettings,
    ) -> Result<Self, JudgeExecutionError> {
        let base_url = url::Url::parse(&judge_settings.together.base_url)
            .map_err(|e| JudgeExecutionError::Client(format!("invalid together base_url: {e}")))?;
        let retry_policy = RetryPolicyConfig {
            max_attempts: judge_settings.together.retry_max_attempts,
            backoff: parse_retry_backoff(&judge_settings.together.retry_backoff)?,
        };
        let inner = TogetherModelClient::new(
            TogetherModelClientConfig {
                base_url,
                api_key: judge_settings.together.api_key.clone(),
                model_name: judge_settings.model_name.clone(),
                timeout_sec: judge_settings.together.timeout_sec,
            },
            retry_policy,
        )
        .map_err(|e| JudgeExecutionError::Client(e.to_string()))?;
        Ok(Self { inner })
    }
}

#[derive(Debug, Clone)]
pub struct FinalNoRootCauseClaimSuiteInput {
    pub eval_context: Value,
    pub final_answer: Value,
}

pub async fn execute_first_suite_for_subject(
    store: &PostgresEvalStore,
    judge_settings: &JudgeSettings,
    catalog: &JudgeSuiteCatalog,
    subject: &PreparedJudgeSubject,
    client: &dyn JudgeClient,
) -> Result<String, JudgeExecutionError> {
    let suite_name = FINAL_NO_ROOT_CAUSE_CLAIM.to_string();
    let suite_def = catalog
        .get(&suite_name)
        .ok_or_else(|| JudgeExecutionError::MissingSuiteDefinition(suite_name.clone()))?;
    let request = build_first_suite_request(suite_name.clone(), suite_def, &subject.snapshot)?;
    let response = client.execute(request.clone()).await?;

    let call_id = format!(
        "{}:{}:{}",
        subject.processing_state.key.eval_run_id, subject.processing_state.key.iteration_id, suite_name
    );

    let llm_row = build_judge_llm_call_row(
        &call_id,
        judge_settings,
        subject,
        &request,
        &response,
    );
    store.insert_judge_llm_call(&llm_row).await?;

    let result_row = build_judge_result_row(
        judge_settings,
        subject,
        &suite_name,
        suite_def,
        &response,
    )?;
    store.upsert_judge_result(&result_row).await?;

    Ok(suite_name)
}

fn build_first_suite_request(
    suite_name: String,
    suite_def: &JudgeSuiteDefinition,
    snapshot: &DiagnosticEvalIterationSnapshot,
) -> Result<JudgeCallRequest, JudgeExecutionError> {
    if suite_name != FINAL_NO_ROOT_CAUSE_CLAIM {
        return Err(JudgeExecutionError::UnsupportedSuite(suite_name));
    }

    let input = build_final_no_root_cause_claim_input(snapshot);
    Ok(JudgeCallRequest {
        suite_name,
        suite_id: suite_def.id.clone(),
        suite_version: suite_def.version.clone(),
        prompt_version: suite_def.version.clone(),
        prompt_text: render_prompt(suite_def, &input),
        input_payload: json!({
            "eval_context": input.eval_context,
            "final_answer": input.final_answer,
        }),
    })
}

fn build_final_no_root_cause_claim_input(
    snapshot: &DiagnosticEvalIterationSnapshot,
) -> FinalNoRootCauseClaimSuiteInput {
    FinalNoRootCauseClaimSuiteInput {
        eval_context: json!({
            "raw_user_query": snapshot.user_request.query,
            "structured_query": snapshot.query_structuring_output.structured_query,
            "matched_incident_card": snapshot.card_hydration_output.primary,
            "incident_evidence_chunks": snapshot.prompt_context_assembly_output.incident_evidence_chunks,
            "theory_chunks": snapshot.prompt_context_assembly_output.theory_chunks,
            "active_hypotheses": snapshot
                .response_validation_and_normalization_output
                .response
                .active_hypotheses,
            "first_check": snapshot
                .response_validation_and_normalization_output
                .response
                .first_check,
            "result_interpretation": snapshot
                .response_validation_and_normalization_output
                .response
                .result_interpretation,
        }),
        final_answer: serde_json::to_value(
            &snapshot.response_validation_and_normalization_output.response,
        )
        .expect("diagnostic response must serialize"),
    }
}

fn render_prompt(
    suite_def: &JudgeSuiteDefinition,
    input: &FinalNoRootCauseClaimSuiteInput,
) -> String {
    format!(
        "{template}\n\nINPUT:\n{payload}",
        template = suite_def.prompt_template,
        payload = serde_json::to_string_pretty(&json!({
            "eval_context": input.eval_context,
            "final_answer": input.final_answer,
        }))
        .expect("suite payload must serialize")
    )
}

fn build_judge_llm_call_row(
    call_id: &str,
    judge_settings: &JudgeSettings,
    subject: &PreparedJudgeSubject,
    request: &JudgeCallRequest,
    response: &JudgeCallResponse,
) -> JudgeLlmCallRow {
    let prompt_cost_usd = token_cost_usd(
        response.prompt_tokens,
        judge_settings.input_cost_per_million_tokens,
    );
    let completion_cost_usd = token_cost_usd(
        response.completion_tokens,
        judge_settings.output_cost_per_million_tokens,
    );

    JudgeLlmCallRow {
        call_id: call_id.to_string(),
        eval_run_id: subject.processing_state.key.eval_run_id,
        runtime_run_id: subject.processing_state.key.runtime_run_id,
        iteration_id: subject.processing_state.key.iteration_id,
        suite_name: request.suite_name.clone(),
        stage_name: EvalStage::JudgeRequestSuites.as_str().to_string(),
        judge_provider: judge_settings.provider.clone(),
        judge_model: judge_settings.model_name.clone(),
        judge_base_url: judge_settings.together.base_url.clone(),
        judge_prompt_version: request.prompt_version.clone(),
        token_count_source: response.token_count_source.clone(),
        prompt_tokens: response.prompt_tokens as i64,
        completion_tokens: response.completion_tokens as i64,
        total_tokens: (response.prompt_tokens + response.completion_tokens) as i64,
        input_cost_per_million_tokens: judge_settings.input_cost_per_million_tokens,
        output_cost_per_million_tokens: judge_settings.output_cost_per_million_tokens,
        prompt_cost_usd,
        completion_cost_usd,
        total_cost_usd: prompt_cost_usd + completion_cost_usd,
        raw_response: response.raw_response.clone(),
    }
}

fn build_judge_result_row(
    judge_settings: &JudgeSettings,
    subject: &PreparedJudgeSubject,
    suite_name: &str,
    suite_def: &JudgeSuiteDefinition,
    response: &JudgeCallResponse,
) -> Result<JudgeResultRow, JudgeExecutionError> {
    let normalized_result = canonical_normalized_result(&response.normalized_result);
    let score = normalized_result
        .get("score")
        .and_then(Value::as_i64)
        .ok_or(JudgeExecutionError::MissingNormalizedField("score"))?;
    if !(0..=2).contains(&score) {
        return Err(JudgeExecutionError::InvalidScore);
    }

    let explanation = normalized_result
        .get("explanation")
        .and_then(Value::as_str)
        .ok_or(JudgeExecutionError::MissingNormalizedField("explanation"))?
        .to_string();
    let failure_code = normalized_result
        .get("failure_code")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    Ok(JudgeResultRow {
        eval_run_id: subject.processing_state.key.eval_run_id,
        runtime_run_id: subject.processing_state.key.runtime_run_id,
        iteration_id: subject.processing_state.key.iteration_id,
        suite_name: suite_name.to_string(),
        suite_id: suite_def.id.clone(),
        suite_version: suite_def.version.clone(),
        category: suite_def.category.clone(),
        scope: suite_def.scope.clone(),
        judge_model: judge_settings.model_name.clone(),
        judge_prompt_version: suite_def.version.clone(),
        score: score as i16,
        normalized_result_json: normalized_result.clone(),
        explanation,
        failure_code,
        raw_response: response.raw_response.clone(),
    })
}

fn canonical_normalized_result(normalized_result: &Value) -> Value {
    if normalized_result.get("score").is_some() {
        return normalized_result.clone();
    }

    let Some(object) = normalized_result.as_object() else {
        return normalized_result.clone();
    };

    if object.len() != 1 {
        return normalized_result.clone();
    }

    let Some(inner) = object.values().next() else {
        return normalized_result.clone();
    };

    if inner.get("score").is_some() {
        inner.clone()
    } else {
        normalized_result.clone()
    }
}

fn token_cost_usd(tokens: u64, per_million: f64) -> f64 {
    tokens as f64 * per_million / 1_000_000.0
}

fn parse_retry_backoff(
    raw: &str,
) -> Result<RetryBackoffKind, JudgeExecutionError> {
    match raw.trim() {
        "exponential" => Ok(RetryBackoffKind::Exponential),
        other => Err(JudgeExecutionError::Client(format!(
            "unsupported together retry_backoff: {other}"
        ))),
    }
}

#[async_trait]
impl JudgeClient for TogetherJudgeClient {
    async fn execute(
        &self,
        request: JudgeCallRequest,
    ) -> Result<JudgeCallResponse, JudgeExecutionError> {
        let response = self
            .inner
            .generate(&ModelGenerationRequest {
                messages: vec![ModelMessage {
                    role: ModelMessageRole::User,
                    content: request.prompt_text,
                }],
                temperature: 0.0,
                max_output_tokens: None,
                response_mode: ModelResponseMode::JsonObject,
            })
            .await
            .map_err(|e| JudgeExecutionError::Client(e.to_string()))?;

        let normalized_result: Value = serde_json::from_str(&response.content).map_err(|e| {
            JudgeExecutionError::Client(format!("judge returned invalid JSON content: {e}"))
        })?;

        let prompt_tokens = response.prompt_tokens.unwrap_or(0) as u64;
        let completion_tokens = response.completion_tokens.unwrap_or(0) as u64;
        let token_count_source = if response.prompt_tokens.is_some()
            || response.completion_tokens.is_some()
            || response.total_tokens.is_some()
        {
            "provider_usage".to_string()
        } else {
            "provider_usage_missing_zeroed".to_string()
        };

        Ok(JudgeCallResponse {
            prompt_tokens,
            completion_tokens,
            token_count_source,
            raw_response: json!({
                "content": response.content,
                "finish_reason": response.finish_reason,
                "prompt_tokens": response.prompt_tokens,
                "completion_tokens": response.completion_tokens,
                "total_tokens": response.total_tokens,
            }),
            normalized_result,
        })
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use chrono::Utc;
    use distributed_diagnostics::api_clients::model::RetryBackoffKind;
    use distributed_diagnostics::orchestrator::run_state::model::{
        FinishedStepRecord, RunId, RunIteration, RunIterationId, RunState, RunStatus, StepKind,
        StepRecord, StepRecordId, StepResultEnvelope,
    };
    use distributed_diagnostics::shared_types::{
        AlternativeContextAssessment, DiagnosticResponse, DiagnosticResultInterpretation,
        HypothesisConfidence, HypothesisSource, LlmStructuredGenerationOutput, ModelTokenUsage,
        NormalizedUserRequest, PromptContextAssemblyOutput, QueryStructuringOutput,
        ResponseValidationAndNormalizationOutput, StructuredUserQuery,
        StructuredUserQueryConfidence, UserRequest,
    };
    use serde_json::json;
    use uuid::Uuid;

    use crate::judge::{
        build_final_no_root_cause_claim_input, build_first_suite_request,
        canonical_normalized_result, parse_retry_backoff, JudgeCallRequest, JudgeCallResponse,
        JudgeClient, JudgeExecutionError, FINAL_NO_ROOT_CAUSE_CLAIM,
    };
    use crate::snapshot::{build_snapshot, SnapshotIterationSelector};
    use crate::suites::JudgeSuiteCatalog;

    struct StubJudgeClient;

    #[async_trait]
    impl JudgeClient for StubJudgeClient {
        async fn execute(
            &self,
            _request: JudgeCallRequest,
        ) -> Result<JudgeCallResponse, JudgeExecutionError> {
            Ok(JudgeCallResponse {
                prompt_tokens: 100,
                completion_tokens: 20,
                token_count_source: "stub".to_string(),
                raw_response: json!({"model_output":"ok"}),
                normalized_result: json!({
                    "score": 2,
                    "violations": [],
                    "explanation": "No root cause claim detected."
                }),
            })
        }
    }

    fn minimal_catalog() -> JudgeSuiteCatalog {
        serde_json::from_value(json!({
            "judge_suites": {
                "final_no_root_cause_claim": {
                    "id": "evals.diagnostics.final_no_root_cause_claim",
                    "version": "v1",
                    "category": "final_answer",
                    "scope": "iteration",
                    "required_for_mvp": true,
                    "input_variables": ["eval_context", "final_answer"],
                    "prompt_template": "Evaluate root cause overclaiming.",
                    "normalized_output_schema_hint": {"required":["score","violations","explanation"]}
                }
            }
        }))
        .unwrap()
    }

    fn step_record(step: StepKind, result: StepResultEnvelope) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: StepRecordId(Uuid::new_v4()),
            step,
            started_at: now,
            finished_at: now,
            result: Ok(result),
        })
    }

    fn minimal_iteration(iteration_id: RunIterationId) -> RunIteration {
        RunIteration {
            iteration_id,
            step_records: vec![
                step_record(
                    StepKind::UserInputReceived,
                    StepResultEnvelope::UserInputReceived(UserRequest {
                        query: "why did the cluster stall?".to_string(),
                        golden_question: None,
                    }),
                ),
                step_record(
                    StepKind::InputNormalization,
                    StepResultEnvelope::InputNormalization(NormalizedUserRequest {
                        query: "why did the cluster stall?".to_string(),
                        input_token_count: 6,
                    }),
                ),
                step_record(
                    StepKind::QueryStructuring,
                    StepResultEnvelope::QueryStructuring(QueryStructuringOutput {
                        structured_query: StructuredUserQuery {
                            intent: "diagnose".to_string(),
                            scenario: "distributed cluster".to_string(),
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
                        token_usage: ModelTokenUsage {
                            prompt_tokens: Some(100),
                            completion_tokens: Some(50),
                            total_tokens: Some(150),
                        },
                        metrics: None,
                    }),
                ),
                step_record(
                    StepKind::CandidateCardRetrieval,
                    StepResultEnvelope::CandidateCardRetrieval(
                        distributed_diagnostics::shared_types::CandidateCardRetrievalOutput {
                            primary: None,
                            alternatives: vec![],
                            metrics: None,
                        },
                    ),
                ),
                step_record(
                    StepKind::CardHydration,
                    StepResultEnvelope::CardHydration(
                        distributed_diagnostics::shared_types::CardHydrationOutput {
                            primary: None,
                            alternatives: vec![],
                        },
                    ),
                ),
                step_record(
                    StepKind::IncidentEvidenceRetrieval,
                    StepResultEnvelope::IncidentEvidenceRetrieval(
                        distributed_diagnostics::shared_types::IncidentEvidenceRetrievalOutput {
                            primary_chunks: vec![],
                            alternative_chunks: vec![],
                            metrics: None,
                        },
                    ),
                ),
                step_record(
                    StepKind::TheoryEvidenceRetrieval,
                    StepResultEnvelope::TheoryEvidenceRetrieval(
                        distributed_diagnostics::shared_types::TheoryEvidenceRetrievalOutput {
                            chunks: vec![],
                            metrics: None,
                        },
                    ),
                ),
                step_record(
                    StepKind::PromptContextAssembly,
                    StepResultEnvelope::PromptContextAssembly(PromptContextAssemblyOutput {
                        prompt: "prompt".to_string(),
                        response_schema: json!({"type": "object"}),
                        incident_evidence_chunks: vec![],
                        theory_chunks: vec![],
                    }),
                ),
                step_record(
                    StepKind::LlmStructuredGeneration,
                    StepResultEnvelope::LlmStructuredGeneration(LlmStructuredGenerationOutput {
                        response_json: json!({"problem_understanding": "foo"}),
                        token_usage: ModelTokenUsage {
                            prompt_tokens: Some(200),
                            completion_tokens: Some(100),
                            total_tokens: Some(300),
                        },
                    }),
                ),
                step_record(
                    StepKind::ResponseValidationAndNormalization,
                    StepResultEnvelope::ResponseValidationAndNormalization(
                        ResponseValidationAndNormalizationOutput {
                            response: DiagnosticResponse {
                                problem_understanding: "foo".to_string(),
                                similar_practical_context: "bar".to_string(),
                                active_hypotheses: vec![
                                    distributed_diagnostics::shared_types::ActiveHypothesis {
                                        hypothesis: "leader election instability".to_string(),
                                        source: HypothesisSource::PrimaryIncident,
                                        confidence: HypothesisConfidence::Medium,
                                    },
                                ],
                                first_check: "check raft logs".to_string(),
                                result_interpretation: DiagnosticResultInterpretation {
                                    supports_primary_if: "if leader churn spikes".to_string(),
                                    supports_competing_if: "if logs are clean".to_string(),
                                    inconclusive_if: Some("if logs are missing".to_string()),
                                },
                                alternative_context_assessment: AlternativeContextAssessment {
                                    used_as_hypothesis: false,
                                    reason: "primary precedent dominates".to_string(),
                                },
                            },
                        },
                    ),
                ),
            ],
        }
    }

    fn minimal_run_state(iterations: Vec<RunIteration>) -> RunState {
        let now = Utc::now();
        RunState {
            run_id: RunId(Uuid::new_v4()),
            status: RunStatus::Archived,
            created_at: now,
            updated_at: now,
            revision: 1,
            iterations,
        }
    }

    #[test]
    fn final_no_root_cause_claim_input_contains_final_answer() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let input = build_final_no_root_cause_claim_input(&snapshot);
        assert!(input.final_answer.get("problem_understanding").is_some());
        assert!(input.eval_context.get("active_hypotheses").is_some());
    }

    #[tokio::test]
    async fn builds_request_for_first_supported_suite() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let catalog = minimal_catalog();
        let suite_def = catalog.get(FINAL_NO_ROOT_CAUSE_CLAIM).unwrap();

        let request = build_first_suite_request(
            FINAL_NO_ROOT_CAUSE_CLAIM.to_string(),
            suite_def,
            &snapshot,
        )
        .expect("request");

        assert_eq!(request.suite_name, FINAL_NO_ROOT_CAUSE_CLAIM);
        assert!(request.prompt_text.contains("Evaluate root cause overclaiming."));

        let client = StubJudgeClient;
        let response = client.execute(request).await.expect("stub response");
        assert_eq!(response.normalized_result["score"], 2);
    }

    #[test]
    fn retry_backoff_maps_exponential() {
        assert_eq!(
            parse_retry_backoff("exponential").unwrap(),
            RetryBackoffKind::Exponential
        );
    }

    #[test]
    fn canonical_normalized_result_unwraps_single_top_level_wrapper() {
        let wrapped = json!({
            "evaluation": {
                "score": 2,
                "violations": [],
                "explanation": "ok"
            }
        });

        let canonical = canonical_normalized_result(&wrapped);
        assert_eq!(canonical["score"], 2);
        assert_eq!(canonical["explanation"], "ok");
    }
}
