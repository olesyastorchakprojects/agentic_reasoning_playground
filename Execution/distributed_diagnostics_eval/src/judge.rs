use async_trait::async_trait;
use distributed_diagnostics::api_clients::model::{
    ModelClient, ModelGenerationRequest, ModelMessage, ModelMessageRole, ModelResponseMode,
    RetryBackoffKind, RetryPolicyConfig, TogetherModelClient, TogetherModelClientConfig,
};
use serde_json::{json, Value};

use tracing::Instrument;

use crate::config::JudgeSettings;
use crate::observability::{eval_suite_span, record_error};
use crate::snapshot::DiagnosticEvalIterationSnapshot;
use crate::storage::{
    EvalStage, JudgeLlmCallRow, JudgeResultRow, PostgresEvalStore, StorageError,
};
use crate::subject_preparation::PreparedJudgeSubject;
use crate::suites::{JudgeSuiteCatalog, JudgeSuiteDefinition, SuiteCatalogError};

const FINAL_NO_ROOT_CAUSE_CLAIM: &str = "final_no_root_cause_claim";
const FINAL_FIRST_CHECK_DISCRIMINATES: &str = "final_first_check_discriminates";
const FINAL_ALTERNATIVE_CONTEXT_HANDLING: &str = "final_alternative_context_handling";
const FINAL_RESULT_INTERPRETATION_USEFULNESS: &str = "final_result_interpretation_usefulness";
const QUERY_STRUCTURING_FIELD_BOUNDARY_CORRECTNESS: &str =
    "query_structuring_field_boundary_correctness";
const QUERY_STRUCTURING_GROUNDING_CONSERVATISM: &str = "query_structuring_grounding_conservatism";
const EVIDENCE_PACK_ROLE_FIT: &str = "evidence_pack_role_fit";
const EVIDENCE_PACK_SUFFICIENCY: &str = "evidence_pack_sufficiency";
const FINAL_HYPOTHESIS_SOURCE_ALIGNMENT: &str = "final_hypothesis_source_alignment";

#[derive(Debug, Clone, PartialEq)]
pub struct JudgeCallRequest {
    pub suite_name: String,
    pub suite_id: String,
    pub suite_version: String,
    pub prompt_version: String,
    pub prompt_text: String,
    pub input_payload: Value,
    pub response_schema: Value,
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

// Input types for suite request builders
#[derive(Debug, Clone)]
pub struct FinalNoRootCauseClaimSuiteInput {
    pub eval_context: Value,
    pub final_answer: Value,
}

#[derive(Debug, Clone)]
pub struct FinalResultInterpretationInput {
    pub final_answer: Value,
    pub active_hypotheses: Value,
    pub first_check: Value,
}

pub async fn execute_one_suite_for_subject(
    store: &PostgresEvalStore,
    judge_settings: &JudgeSettings,
    suite_name: &str,
    catalog: &JudgeSuiteCatalog,
    subject: &PreparedJudgeSubject,
    client: &dyn JudgeClient,
) -> Result<(), JudgeExecutionError> {
    let suite_def = catalog
        .get(suite_name)
        .ok_or_else(|| JudgeExecutionError::MissingSuiteDefinition(suite_name.to_string()))?;

    let span = eval_suite_span(
        &subject.processing_state.key.eval_run_id.to_string(),
        &subject.processing_state.key.runtime_run_id.to_string(),
        &subject.processing_state.key.iteration_id.to_string(),
        suite_name,
        &suite_def.category,
        &suite_def.scope,
        &judge_settings.model_name,
    );

    let result = execute_one_suite_inner(
        store,
        judge_settings,
        suite_name,
        suite_def,
        subject,
        client,
    )
    .instrument(span.clone())
    .await;

    if let Err(ref e) = result {
        record_error(&span, "JudgeExecutionError", &e.to_string());
    }
    result
}

async fn execute_one_suite_inner(
    store: &PostgresEvalStore,
    judge_settings: &JudgeSettings,
    suite_name: &str,
    suite_def: &JudgeSuiteDefinition,
    subject: &PreparedJudgeSubject,
    client: &dyn JudgeClient,
) -> Result<(), JudgeExecutionError> {
    let request = build_suite_request(suite_name.to_string(), suite_def, &subject.snapshot)?;

    // One retry for transient empty-content responses from the judge model.
    let response = match client.execute(request.clone()).await {
        Ok(r) => r,
        Err(JudgeExecutionError::Client(msg)) if msg.contains("content is empty") => {
            tracing::warn!(suite_name, "judge returned empty content, retrying once");
            client.execute(request.clone()).await?
        }
        Err(e) => return Err(e),
    };

    let span = tracing::Span::current();
    span.record("llm.token_count.prompt", response.prompt_tokens as i64);
    span.record("llm.token_count.completion", response.completion_tokens as i64);
    span.record(
        "llm.token_count.total",
        (response.prompt_tokens + response.completion_tokens) as i64,
    );

    let call_id = format!(
        "{}:{}:{}",
        subject.processing_state.key.eval_run_id,
        subject.processing_state.key.iteration_id,
        suite_name,
    );

    let llm_row = build_judge_llm_call_row(&call_id, judge_settings, subject, &request, &response);
    let total_cost = llm_row.total_cost_usd;
    store.insert_judge_llm_call(&llm_row).await?;

    let result_row =
        build_judge_result_row(judge_settings, subject, suite_name, suite_def, &response)?;
    span.record("eval.score", result_row.score as i64);
    span.record("eval.total_cost_usd", total_cost);
    store.upsert_judge_result(&result_row).await?;

    Ok(())
}

pub fn build_suite_request(
    suite_name: String,
    suite_def: &JudgeSuiteDefinition,
    snapshot: &DiagnosticEvalIterationSnapshot,
) -> Result<JudgeCallRequest, JudgeExecutionError> {
    let payload = match suite_name.as_str() {
        FINAL_NO_ROOT_CAUSE_CLAIM
        | FINAL_FIRST_CHECK_DISCRIMINATES
        | FINAL_ALTERNATIVE_CONTEXT_HANDLING => {
            let input = build_eval_context_and_final_answer_input(snapshot);
            json!({
                "eval_context": input.eval_context,
                "final_answer": input.final_answer,
            })
        }
        FINAL_RESULT_INTERPRETATION_USEFULNESS => {
            let input = build_final_result_interpretation_input(snapshot);
            json!({
                "final_answer": input.final_answer,
                "active_hypotheses": input.active_hypotheses,
                "first_check": input.first_check,
            })
        }
        QUERY_STRUCTURING_FIELD_BOUNDARY_CORRECTNESS
        | QUERY_STRUCTURING_GROUNDING_CONSERVATISM => {
            build_query_structuring_payload(snapshot)
        }
        EVIDENCE_PACK_ROLE_FIT => build_evidence_pack_role_fit_payload(snapshot),
        EVIDENCE_PACK_SUFFICIENCY => build_evidence_pack_sufficiency_payload(snapshot),
        FINAL_HYPOTHESIS_SOURCE_ALIGNMENT => {
            build_final_hypothesis_source_alignment_payload(snapshot)
        }
        other => return Err(JudgeExecutionError::UnsupportedSuite(other.to_string())),
    };

    Ok(JudgeCallRequest {
        suite_name,
        suite_id: suite_def.id.clone(),
        suite_version: suite_def.version.clone(),
        prompt_version: suite_def.version.clone(),
        prompt_text: render_prompt_with_payload(suite_def, &payload),
        input_payload: payload,
        response_schema: suite_def.response_schema.clone(),
    })
}

pub fn build_final_no_root_cause_claim_input(
    snapshot: &DiagnosticEvalIterationSnapshot,
) -> FinalNoRootCauseClaimSuiteInput {
    let inner = build_eval_context_and_final_answer_input(snapshot);
    FinalNoRootCauseClaimSuiteInput {
        eval_context: inner.eval_context,
        final_answer: inner.final_answer,
    }
}

fn build_eval_context_and_final_answer_input(
    snapshot: &DiagnosticEvalIterationSnapshot,
) -> FinalNoRootCauseClaimSuiteInput {
    let resp = &snapshot.response_validation_and_normalization_output.response;
    FinalNoRootCauseClaimSuiteInput {
        eval_context: json!({
            "raw_user_query": snapshot.user_request.query,
            "structured_query": snapshot.query_structuring_output.structured_query,
            "matched_incident_card": snapshot.card_hydration_output.primary,
            "incident_evidence_chunks": snapshot.prompt_context_assembly_output.incident_evidence_chunks,
            "theory_chunks": snapshot.prompt_context_assembly_output.theory_chunks,
            "active_hypotheses": resp.active_hypotheses,
            "first_check": resp.first_check,
            "result_interpretation": resp.result_interpretation,
        }),
        final_answer: serde_json::to_value(resp).expect("diagnostic response must serialize"),
    }
}

fn build_final_result_interpretation_input(
    snapshot: &DiagnosticEvalIterationSnapshot,
) -> FinalResultInterpretationInput {
    let resp = &snapshot.response_validation_and_normalization_output.response;
    FinalResultInterpretationInput {
        final_answer: serde_json::to_value(resp).expect("diagnostic response must serialize"),
        active_hypotheses: serde_json::to_value(&resp.active_hypotheses)
            .expect("active_hypotheses must serialize"),
        first_check: Value::String(resp.first_check.clone()),
    }
}

fn build_query_structuring_payload(snapshot: &DiagnosticEvalIterationSnapshot) -> Value {
    json!({
        "raw_user_query": snapshot.user_request.query,
        "structured_query": snapshot.query_structuring_output.structured_query,
    })
}

fn build_evidence_pack_role_fit_payload(snapshot: &DiagnosticEvalIterationSnapshot) -> Value {
    json!({
        "raw_user_query": snapshot.user_request.query,
        "structured_query": snapshot.query_structuring_output.structured_query,
        "evidence_topology": snapshot.prompt_context_assembly_output.evidence_topology,
        "incident_evidence_chunks": snapshot.prompt_context_assembly_output.incident_evidence_chunks,
        "theory_chunks": snapshot.prompt_context_assembly_output.theory_chunks,
    })
}

fn build_evidence_pack_sufficiency_payload(snapshot: &DiagnosticEvalIterationSnapshot) -> Value {
    json!({
        "raw_user_query": snapshot.user_request.query,
        "structured_query": snapshot.query_structuring_output.structured_query,
        "matched_incident_card": snapshot.card_hydration_output.primary,
        "incident_evidence_chunks": snapshot.prompt_context_assembly_output.incident_evidence_chunks,
        "theory_chunks": snapshot.prompt_context_assembly_output.theory_chunks,
    })
}

fn build_final_hypothesis_source_alignment_payload(
    snapshot: &DiagnosticEvalIterationSnapshot,
) -> Value {
    let resp = &snapshot.response_validation_and_normalization_output.response;
    json!({
        "evidence_topology": snapshot.prompt_context_assembly_output.evidence_topology,
        "matched_incident_card": snapshot.card_hydration_output.primary,
        "incident_evidence_chunks": snapshot.prompt_context_assembly_output.incident_evidence_chunks,
        "theory_chunks": snapshot.prompt_context_assembly_output.theory_chunks,
        "final_answer": serde_json::to_value(resp).expect("diagnostic response must serialize"),
    })
}

fn render_prompt_with_payload(suite_def: &JudgeSuiteDefinition, payload: &Value) -> String {
    format!(
        "{template}\n\nINPUT:\n{payload_str}",
        template = suite_def.prompt_template,
        payload_str =
            serde_json::to_string_pretty(payload).expect("suite payload must serialize")
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
        .or_else(|| normalized_result.get("reason"))
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

    // Single-key wrapper: {"evaluation": {"score": N, ...}} -> unwrap inner
    if object.len() == 1 {
        let inner = object.values().next().unwrap();
        if inner.get("score").is_some() {
            return inner.clone();
        }
    }

    // Fallback: model used a non-standard key for the score value (e.g. "commentary": 2).
    // Find the first integer value in [0,2] at the top level and rename it to "score".
    let score_entry = object.iter().find(|(_, v)| {
        v.as_i64().map(|n| (0..=2).contains(&n)).unwrap_or(false)
    });
    if let Some((score_key, score_val)) = score_entry {
        let mut canonical = serde_json::Map::new();
        canonical.insert("score".to_string(), score_val.clone());
        for (k, v) in object {
            if k != score_key {
                canonical.insert(k.clone(), v.clone());
            }
        }
        return Value::Object(canonical);
    }

    normalized_result.clone()
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
                response_mode: ModelResponseMode::JsonSchema(request.response_schema.clone()),
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
        build_final_no_root_cause_claim_input, build_suite_request, canonical_normalized_result,
        parse_retry_backoff, JudgeCallRequest, JudgeCallResponse, JudgeClient, JudgeExecutionError,
        EVIDENCE_PACK_ROLE_FIT, EVIDENCE_PACK_SUFFICIENCY, FINAL_ALTERNATIVE_CONTEXT_HANDLING,
        FINAL_FIRST_CHECK_DISCRIMINATES, FINAL_HYPOTHESIS_SOURCE_ALIGNMENT,
        FINAL_NO_ROOT_CAUSE_CLAIM, FINAL_RESULT_INTERPRETATION_USEFULNESS,
        QUERY_STRUCTURING_FIELD_BOUNDARY_CORRECTNESS, QUERY_STRUCTURING_GROUNDING_CONSERVATISM,
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

    fn stub_schema() -> serde_json::Value {
        json!({"type":"object","properties":{"score":{"type":"integer","minimum":0,"maximum":2}},"required":["score"]})
    }

    fn minimal_catalog() -> JudgeSuiteCatalog {
        let schema = stub_schema();
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
                    "normalized_output_schema_hint": {"required":["score","explanation"]},
                    "response_schema": schema
                },
                "final_first_check_discriminates": {
                    "id": "evals.diagnostics.final_first_check_discriminates",
                    "version": "v1",
                    "category": "final_answer",
                    "scope": "iteration",
                    "required_for_mvp": true,
                    "input_variables": ["eval_context", "final_answer"],
                    "prompt_template": "Evaluate first check discriminating power.",
                    "normalized_output_schema_hint": {"required":["score","reason"]},
                    "response_schema": stub_schema()
                },
                "final_alternative_context_handling": {
                    "id": "evals.diagnostics.final_alternative_context_handling",
                    "version": "v1",
                    "category": "final_answer",
                    "scope": "iteration",
                    "required_for_mvp": true,
                    "input_variables": ["eval_context", "final_answer"],
                    "prompt_template": "Evaluate alternative context handling.",
                    "normalized_output_schema_hint": {"required":["score","reason"]},
                    "response_schema": stub_schema()
                },
                "final_result_interpretation_usefulness": {
                    "id": "evals.diagnostics.final_result_interpretation_usefulness",
                    "version": "v1",
                    "category": "final_answer",
                    "scope": "iteration",
                    "required_for_mvp": true,
                    "input_variables": ["final_answer", "active_hypotheses", "first_check"],
                    "prompt_template": "Evaluate result interpretation usefulness.",
                    "normalized_output_schema_hint": {"required":["score","reason"]},
                    "response_schema": stub_schema()
                },
                "query_structuring_field_boundary_correctness": {
                    "id": "evals.diagnostics.query_structuring_field_boundary_correctness",
                    "version": "v1",
                    "category": "query_structuring",
                    "scope": "iteration",
                    "required_for_mvp": true,
                    "input_variables": ["raw_user_query", "structured_query"],
                    "prompt_template": "Evaluate query structuring field boundaries.",
                    "normalized_output_schema_hint": {"required":["score","explanation"]},
                    "response_schema": stub_schema()
                },
                "query_structuring_grounding_conservatism": {
                    "id": "evals.diagnostics.query_structuring_grounding_conservatism",
                    "version": "v1",
                    "category": "query_structuring",
                    "scope": "iteration",
                    "required_for_mvp": true,
                    "input_variables": ["raw_user_query", "structured_query"],
                    "prompt_template": "Evaluate query structuring grounding conservatism.",
                    "normalized_output_schema_hint": {"required":["score","explanation"]},
                    "response_schema": stub_schema()
                },
                "evidence_pack_role_fit": {
                    "id": "evals.diagnostics.evidence_pack_role_fit",
                    "version": "v1",
                    "category": "evidence_pack",
                    "scope": "iteration",
                    "required_for_mvp": true,
                    "input_variables": ["raw_user_query", "structured_query", "evidence_topology", "incident_evidence_chunks", "theory_chunks"],
                    "prompt_template": "Evaluate evidence pack role fit.",
                    "normalized_output_schema_hint": {"required":["score","explanation"]},
                    "response_schema": stub_schema()
                },
                "evidence_pack_sufficiency": {
                    "id": "evals.diagnostics.evidence_pack_sufficiency",
                    "version": "v1",
                    "category": "evidence_pack",
                    "scope": "iteration",
                    "required_for_mvp": true,
                    "input_variables": ["raw_user_query", "structured_query", "matched_incident_card", "incident_evidence_chunks", "theory_chunks"],
                    "prompt_template": "Evaluate evidence pack sufficiency.",
                    "normalized_output_schema_hint": {"required":["score","explanation"]},
                    "response_schema": stub_schema()
                },
                "final_hypothesis_source_alignment": {
                    "id": "evals.diagnostics.final_hypothesis_source_alignment",
                    "version": "v1",
                    "category": "final_answer",
                    "scope": "iteration",
                    "required_for_mvp": true,
                    "input_variables": ["evidence_topology", "matched_incident_card", "incident_evidence_chunks", "theory_chunks", "final_answer"],
                    "prompt_template": "Evaluate hypothesis source alignment.",
                    "normalized_output_schema_hint": {"required":["score","explanation"]},
                    "response_schema": stub_schema()
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
            config_snapshot: None,
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
                        evidence_topology: Default::default(),
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

    #[test]
    fn build_suite_request_final_no_root_cause_claim_has_eval_context_and_final_answer() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let catalog = minimal_catalog();
        let suite_def = catalog.get(FINAL_NO_ROOT_CAUSE_CLAIM).unwrap();

        let request =
            build_suite_request(FINAL_NO_ROOT_CAUSE_CLAIM.to_string(), suite_def, &snapshot)
                .expect("request");

        assert_eq!(request.suite_name, FINAL_NO_ROOT_CAUSE_CLAIM);
        assert!(request.prompt_text.contains("Evaluate root cause overclaiming."));
        assert!(request.input_payload.get("eval_context").is_some());
        assert!(request.input_payload.get("final_answer").is_some());
    }

    #[test]
    fn build_suite_request_final_first_check_discriminates_has_eval_context_and_final_answer() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let catalog = minimal_catalog();
        let suite_def = catalog.get(FINAL_FIRST_CHECK_DISCRIMINATES).unwrap();

        let request =
            build_suite_request(FINAL_FIRST_CHECK_DISCRIMINATES.to_string(), suite_def, &snapshot)
                .expect("request");

        assert_eq!(request.suite_name, FINAL_FIRST_CHECK_DISCRIMINATES);
        assert!(request.prompt_text.contains("Evaluate first check discriminating power."));
        assert!(request.input_payload.get("eval_context").is_some());
        assert!(request.input_payload.get("final_answer").is_some());
    }

    #[test]
    fn build_suite_request_final_alternative_context_handling_has_eval_context_and_final_answer() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let catalog = minimal_catalog();
        let suite_def = catalog.get(FINAL_ALTERNATIVE_CONTEXT_HANDLING).unwrap();

        let request = build_suite_request(
            FINAL_ALTERNATIVE_CONTEXT_HANDLING.to_string(),
            suite_def,
            &snapshot,
        )
        .expect("request");

        assert_eq!(request.suite_name, FINAL_ALTERNATIVE_CONTEXT_HANDLING);
        assert!(request.prompt_text.contains("Evaluate alternative context handling."));
        assert!(request.input_payload.get("eval_context").is_some());
        assert!(request.input_payload.get("final_answer").is_some());
    }

    #[test]
    fn build_suite_request_final_result_interpretation_usefulness_has_correct_fields() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let catalog = minimal_catalog();
        let suite_def = catalog.get(FINAL_RESULT_INTERPRETATION_USEFULNESS).unwrap();

        let request = build_suite_request(
            FINAL_RESULT_INTERPRETATION_USEFULNESS.to_string(),
            suite_def,
            &snapshot,
        )
        .expect("request");

        assert_eq!(request.suite_name, FINAL_RESULT_INTERPRETATION_USEFULNESS);
        assert!(request.prompt_text.contains("Evaluate result interpretation usefulness."));
        assert!(request.input_payload.get("final_answer").is_some());
        assert!(request.input_payload.get("active_hypotheses").is_some());
        assert!(request.input_payload.get("first_check").is_some());
        assert!(request.input_payload.get("eval_context").is_none());
    }

    #[test]
    fn build_suite_request_returns_error_for_unknown_suite() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let fake_def = serde_json::from_value(serde_json::json!({
            "id": "x",
            "version": "v1",
            "category": "final_answer",
            "scope": "iteration",
            "required_for_mvp": false,
            "input_variables": [],
            "prompt_template": "x",
            "normalized_output_schema_hint": {},
            "response_schema": {"type": "object"}
        }))
        .unwrap();

        let err = build_suite_request("unknown_suite".to_string(), &fake_def, &snapshot)
            .unwrap_err();
        assert!(matches!(err, JudgeExecutionError::UnsupportedSuite(_)));
    }

    #[tokio::test]
    async fn stub_client_returns_valid_response_for_final_no_root_cause_claim() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let catalog = minimal_catalog();
        let suite_def = catalog.get(FINAL_NO_ROOT_CAUSE_CLAIM).unwrap();

        let request =
            build_suite_request(FINAL_NO_ROOT_CAUSE_CLAIM.to_string(), suite_def, &snapshot)
                .expect("request");

        let client = StubJudgeClient;
        let response = client.execute(request).await.expect("stub response");
        assert_eq!(response.normalized_result["score"], 2);
    }

    #[test]
    fn build_suite_request_query_structuring_field_boundary_has_raw_query_and_structured_query() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let catalog = minimal_catalog();
        let suite_def = catalog.get(QUERY_STRUCTURING_FIELD_BOUNDARY_CORRECTNESS).unwrap();

        let request = build_suite_request(
            QUERY_STRUCTURING_FIELD_BOUNDARY_CORRECTNESS.to_string(),
            suite_def,
            &snapshot,
        )
        .expect("request");

        assert_eq!(request.suite_name, QUERY_STRUCTURING_FIELD_BOUNDARY_CORRECTNESS);
        assert!(request.input_payload.get("raw_user_query").is_some());
        assert!(request.input_payload.get("structured_query").is_some());
        assert!(request.input_payload.get("eval_context").is_none());
    }

    #[test]
    fn build_suite_request_query_structuring_grounding_conservatism_has_correct_fields() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let catalog = minimal_catalog();
        let suite_def = catalog.get(QUERY_STRUCTURING_GROUNDING_CONSERVATISM).unwrap();

        let request = build_suite_request(
            QUERY_STRUCTURING_GROUNDING_CONSERVATISM.to_string(),
            suite_def,
            &snapshot,
        )
        .expect("request");

        assert_eq!(request.suite_name, QUERY_STRUCTURING_GROUNDING_CONSERVATISM);
        assert!(request.input_payload.get("raw_user_query").is_some());
        assert!(request.input_payload.get("structured_query").is_some());
    }

    #[test]
    fn build_suite_request_evidence_pack_role_fit_has_topology_and_chunks() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let catalog = minimal_catalog();
        let suite_def = catalog.get(EVIDENCE_PACK_ROLE_FIT).unwrap();

        let request =
            build_suite_request(EVIDENCE_PACK_ROLE_FIT.to_string(), suite_def, &snapshot)
                .expect("request");

        assert_eq!(request.suite_name, EVIDENCE_PACK_ROLE_FIT);
        assert!(request.input_payload.get("raw_user_query").is_some());
        assert!(request.input_payload.get("evidence_topology").is_some());
        assert!(request.input_payload.get("incident_evidence_chunks").is_some());
        assert!(request.input_payload.get("theory_chunks").is_some());
        assert!(request.input_payload.get("matched_incident_card").is_none());
    }

    #[test]
    fn build_suite_request_evidence_pack_sufficiency_has_matched_card_and_chunks() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let catalog = minimal_catalog();
        let suite_def = catalog.get(EVIDENCE_PACK_SUFFICIENCY).unwrap();

        let request =
            build_suite_request(EVIDENCE_PACK_SUFFICIENCY.to_string(), suite_def, &snapshot)
                .expect("request");

        assert_eq!(request.suite_name, EVIDENCE_PACK_SUFFICIENCY);
        assert!(request.input_payload.get("matched_incident_card").is_some());
        assert!(request.input_payload.get("incident_evidence_chunks").is_some());
        assert!(request.input_payload.get("evidence_topology").is_none());
    }

    #[test]
    fn build_suite_request_final_hypothesis_source_alignment_has_topology_and_final_answer() {
        let run = minimal_run_state(vec![minimal_iteration(RunIterationId(Uuid::new_v4()))]);
        let snapshot = build_snapshot(&run, SnapshotIterationSelector::LastCompletedIteration)
            .expect("snapshot");
        let catalog = minimal_catalog();
        let suite_def = catalog.get(FINAL_HYPOTHESIS_SOURCE_ALIGNMENT).unwrap();

        let request = build_suite_request(
            FINAL_HYPOTHESIS_SOURCE_ALIGNMENT.to_string(),
            suite_def,
            &snapshot,
        )
        .expect("request");

        assert_eq!(request.suite_name, FINAL_HYPOTHESIS_SOURCE_ALIGNMENT);
        assert!(request.input_payload.get("evidence_topology").is_some());
        assert!(request.input_payload.get("matched_incident_card").is_some());
        assert!(request.input_payload.get("final_answer").is_some());
        assert!(request.input_payload.get("eval_context").is_none());
    }

    #[test]
    fn canonical_normalized_result_recovers_score_from_nonstandard_key() {
        let malformed = json!({
            "commentary to=assistant{": 2,
            "violations": [],
            "explanation": "ok"
        });
        let canonical = canonical_normalized_result(&malformed);
        assert_eq!(canonical["score"], 2);
        assert_eq!(canonical["explanation"], "ok");
    }

    #[test]
    fn canonical_normalized_result_accepts_reason_field_unchanged() {
        // Suites that return "reason" instead of "explanation" pass through as-is;
        // build_judge_result_row then falls back to the "reason" key.
        let with_reason = json!({"score": 1, "reason": "marginal check"});
        let canonical = canonical_normalized_result(&with_reason);
        assert_eq!(canonical["score"], 1);
        assert_eq!(canonical["reason"], "marginal check");
        assert!(canonical.get("explanation").is_none());
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
