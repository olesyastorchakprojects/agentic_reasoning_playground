use chrono::{DateTime, Utc};
use sqlx::postgres::{PgPoolOptions, PgRow};
use sqlx::Row;
use uuid::Uuid;

use crate::config::PostgresSettings;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("invalid config: {0}")]
    InvalidConfig(&'static str),
    #[error("connection failure: {0}")]
    Connection(String),
    #[error("query failure: {0}")]
    Query(String),
    #[error("insert failure: {0}")]
    Insert(String),
    #[error("update failure: {0}")]
    Update(String),
    #[error("invalid stored row: {0}")]
    InvalidStoredRow(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalStage {
    JudgeRequestSuites,
    BuildEvalSummary,
}

impl EvalStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::JudgeRequestSuites => "judge_request_suites",
            Self::BuildEvalSummary => "build_eval_summary",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalProcessingStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl EvalProcessingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalSubjectKey {
    pub eval_run_id: Uuid,
    pub runtime_run_id: Uuid,
    pub iteration_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenEvalSubject {
    pub key: EvalSubjectKey,
    pub subject_received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EligibleEvalSubject {
    pub runtime_run_id: Uuid,
    pub iteration_id: Uuid,
    pub subject_received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalProcessingStateRow {
    pub key: EvalSubjectKey,
    pub subject_received_at: DateTime<Utc>,
    pub current_stage: String,
    pub status: String,
    pub attempt_count: i32,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JudgeResultRow {
    pub eval_run_id: Uuid,
    pub runtime_run_id: Uuid,
    pub iteration_id: Uuid,
    pub suite_name: String,
    pub suite_id: String,
    pub suite_version: String,
    pub category: String,
    pub scope: String,
    pub judge_model: String,
    pub judge_prompt_version: String,
    pub score: i16,
    pub raw_response: serde_json::Value,
    pub explanation: String,
    pub normalized_result_json: serde_json::Value,
    pub failure_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JudgeLlmCallRow {
    pub call_id: String,
    pub eval_run_id: Uuid,
    pub runtime_run_id: Uuid,
    pub iteration_id: Uuid,
    pub suite_name: String,
    pub stage_name: String,
    pub judge_provider: String,
    pub judge_model: String,
    pub judge_base_url: String,
    pub judge_prompt_version: String,
    pub token_count_source: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub input_cost_per_million_tokens: f64,
    pub output_cost_per_million_tokens: f64,
    pub prompt_cost_usd: f64,
    pub completion_cost_usd: f64,
    pub total_cost_usd: f64,
    pub raw_response: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalIterationSummaryRow {
    pub key: EvalSubjectKey,
    pub query_structuring_judge_score: f64,
    pub evidence_pack_judge_score: f64,
    pub final_answer_judge_score: f64,
    pub query_structuring_no_hard_fail: bool,
    pub evidence_pack_no_hard_fail: bool,
    pub final_answer_no_hard_fail: bool,
    pub usable_first_response: bool,
    pub no_root_cause_gate_passed: bool,
    pub single_check_gate_passed: bool,
    pub source_alignment_gate_passed: bool,
    pub field_boundary_gate_passed: bool,
    pub evidence_pack_gate_passed: bool,
    pub query_structuring_field_boundary_correctness_score: i16,
    pub query_structuring_grounding_conservatism_score: i16,
    pub evidence_pack_role_fit_score: i16,
    pub evidence_pack_sufficiency_score: i16,
    pub final_no_root_cause_claim_score: i16,
    pub final_first_check_discriminates_score: i16,
    pub final_hypothesis_source_alignment_score: i16,
    pub final_alternative_context_handling_score: i16,
    pub final_result_interpretation_usefulness_score: i16,
    pub runtime_prompt_tokens: i64,
    pub runtime_completion_tokens: i64,
    pub runtime_total_tokens: i64,
    pub runtime_total_cost_usd: f64,
    pub judge_prompt_tokens: i64,
    pub judge_completion_tokens: i64,
    pub judge_total_tokens: i64,
    pub judge_total_cost_usd: f64,
    pub run_total_tokens: i64,
    pub run_total_cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalRunSummaryRow {
    pub eval_run_id: Uuid,
    pub run_type: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub runtime_run_count: i64,
    pub iterations_evaluated_count: i64,
    pub judge_provider: String,
    pub judge_model: String,
    pub suite_versions: serde_json::Value,
    pub usable_first_response_rate: f64,
    pub query_structuring_judge_score: f64,
    pub evidence_pack_judge_score: f64,
    pub final_answer_judge_score: f64,
    pub query_structuring_strict_pass_rate: f64,
    pub evidence_pack_strict_pass_rate: f64,
    pub final_answer_strict_pass_rate: f64,
    pub diagnostic_move_hard_fail_rate: f64,
    pub gate_pass_rate: f64,
    pub bad_final_due_to_query_rate: f64,
    pub bad_final_due_to_evidence_rate: f64,
    pub bad_final_with_good_query_and_evidence_rate: f64,
    pub runtime_prompt_tokens: i64,
    pub runtime_completion_tokens: i64,
    pub runtime_total_tokens: i64,
    pub runtime_total_cost_usd: f64,
    pub judge_prompt_tokens: i64,
    pub judge_completion_tokens: i64,
    pub judge_total_tokens: i64,
    pub judge_total_cost_usd: f64,
    pub run_total_tokens: i64,
    pub run_total_cost_usd: f64,
}

#[derive(Debug)]
pub struct PostgresEvalStore {
    pool: sqlx::PgPool,
}

impl PostgresEvalStore {
    pub async fn new(
        config: &PostgresSettings,
    ) -> Result<Self, StorageError> {
        if config.url.trim().is_empty() {
            return Err(StorageError::InvalidConfig(
                "postgres.url must not be empty",
            ));
        }
        let pool = PgPoolOptions::new()
            .connect(&config.url)
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    pub async fn discover_eligible_subjects(
        &self,
        eval_run_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<FrozenEvalSubject>, StorageError> {
        let limit_i64 = limit.map(|v| v as i64).unwrap_or(i64::MAX);
        let rows = sqlx::query(
            r#"
            WITH latest_iterations AS (
                SELECT DISTINCT ON (ri.run_id)
                    ri.run_id,
                    ri.iteration_id,
                    r.created_at
                FROM diagnostics.run_iterations ri
                JOIN diagnostics.runs r ON r.run_id = ri.run_id
                ORDER BY ri.run_id, ri.sequence_no DESC
            )
            SELECT
                li.run_id AS runtime_run_id,
                li.iteration_id,
                li.created_at AS subject_received_at
            FROM latest_iterations li
            WHERE EXISTS (
                SELECT 1
                FROM diagnostics.run_step_records s
                WHERE s.iteration_id = li.iteration_id
                  AND s.record_status = 'finished'
                  AND s.step = 'UserInputReceived'
                  AND jsonb_typeof(
                        s.result_json -> 'UserInputReceived' -> 'golden_question'
                      ) = 'object'
            )
              AND EXISTS (
                SELECT 1
                FROM diagnostics.run_step_records s
                WHERE s.iteration_id = li.iteration_id
                  AND s.record_status = 'finished'
                  AND s.step = 'ResponseValidationAndNormalization'
                  AND s.result_json IS NOT NULL
            )
              AND NOT EXISTS (
                SELECT 1
                FROM diagnostics.eval_processing_state eps
                WHERE eps.runtime_run_id = li.run_id
                  AND eps.iteration_id = li.iteration_id
            )
            ORDER BY li.created_at ASC, li.run_id ASC
            LIMIT $1
            "#,
        )
        .bind(limit_i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Query(e.to_string()))?;

        rows.iter()
            .map(|row| {
                let runtime_run_id: Uuid = row
                    .try_get("runtime_run_id")
                    .map_err(|_| StorageError::InvalidStoredRow("runtime_run_id"))?;
                let iteration_id: Uuid = row
                    .try_get("iteration_id")
                    .map_err(|_| StorageError::InvalidStoredRow("iteration_id"))?;
                let subject_received_at: DateTime<Utc> = row
                    .try_get("subject_received_at")
                    .map_err(|_| StorageError::InvalidStoredRow("subject_received_at"))?;
                Ok(FrozenEvalSubject {
                    key: EvalSubjectKey {
                        eval_run_id,
                        runtime_run_id,
                        iteration_id,
                    },
                    subject_received_at,
                })
            })
            .collect()
    }

    pub async fn bootstrap_eval_processing_state(
        &self,
        subjects: &[FrozenEvalSubject],
    ) -> Result<u64, StorageError> {
        let mut inserted = 0_u64;
        for subject in subjects {
            let result = sqlx::query(
                r#"
                INSERT INTO diagnostics.eval_processing_state (
                    eval_run_id,
                    runtime_run_id,
                    iteration_id,
                    subject_received_at,
                    current_stage,
                    status,
                    attempt_count,
                    started_at,
                    completed_at,
                    updated_at,
                    last_error
                )
                VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, 0, NULL, NULL, $4, NULL)
                ON CONFLICT (eval_run_id, runtime_run_id, iteration_id) DO NOTHING
                "#
            )
            .bind(subject.key.eval_run_id)
            .bind(subject.key.runtime_run_id)
            .bind(subject.key.iteration_id)
            .bind(subject.subject_received_at)
            .bind(EvalStage::JudgeRequestSuites.as_str())
            .bind(EvalProcessingStatus::Pending.as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Insert(e.to_string()))?;
            inserted += result.rows_affected();
        }
        Ok(inserted)
    }

    pub async fn list_eval_processing_state(
        &self,
        eval_run_id: Uuid,
    ) -> Result<Vec<EvalProcessingStateRow>, StorageError> {
        let rows = sqlx::query(
            r#"
            SELECT
                eval_run_id,
                runtime_run_id,
                iteration_id,
                subject_received_at,
                current_stage,
                status,
                attempt_count,
                started_at,
                completed_at,
                updated_at,
                last_error
            FROM diagnostics.eval_processing_state
            WHERE eval_run_id = $1::uuid
            ORDER BY subject_received_at ASC, runtime_run_id ASC, iteration_id ASC
            "#,
        )
        .bind(eval_run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Query(e.to_string()))?;

        rows.iter().map(decode_eval_processing_state_row).collect()
    }

    pub async fn fetch_next_subject_for_stage(
        &self,
        eval_run_id: Uuid,
        stage: EvalStage,
    ) -> Result<Option<EvalProcessingStateRow>, StorageError> {
        let row = sqlx::query(
            r#"
            SELECT
                eval_run_id,
                runtime_run_id,
                iteration_id,
                subject_received_at,
                current_stage,
                status,
                attempt_count,
                started_at,
                completed_at,
                updated_at,
                last_error
            FROM diagnostics.eval_processing_state
            WHERE eval_run_id = $1::uuid
              AND current_stage = $2
              AND status IN ('pending', 'running', 'failed')
            ORDER BY subject_received_at ASC, runtime_run_id ASC, iteration_id ASC
            LIMIT 1
            "#,
        )
        .bind(eval_run_id)
        .bind(stage.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Query(e.to_string()))?;

        row.as_ref().map(decode_eval_processing_state_row).transpose()
    }

    pub async fn update_eval_processing_state(
        &self,
        key: &EvalSubjectKey,
        current_stage: EvalStage,
        status: EvalProcessingStatus,
        attempt_count: i32,
        started_at: Option<DateTime<Utc>>,
        completed_at: Option<DateTime<Utc>>,
        last_error: Option<&str>,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            UPDATE diagnostics.eval_processing_state
            SET
                current_stage = $4,
                status = $5,
                attempt_count = $6,
                started_at = $7,
                completed_at = $8,
                updated_at = NOW(),
                last_error = $9
            WHERE eval_run_id = $1::uuid
              AND runtime_run_id = $2::uuid
              AND iteration_id = $3::uuid
            "#,
        )
        .bind(key.eval_run_id)
        .bind(key.runtime_run_id)
        .bind(key.iteration_id)
        .bind(current_stage.as_str())
        .bind(status.as_str())
        .bind(attempt_count)
        .bind(started_at)
        .bind(completed_at)
        .bind(last_error)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Update(e.to_string()))?;
        Ok(())
    }

    pub async fn upsert_judge_result(
        &self,
        row: &JudgeResultRow,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            INSERT INTO diagnostics.judge_results (
                eval_run_id,
                runtime_run_id,
                iteration_id,
                suite_name,
                suite_id,
                suite_version,
                category,
                scope,
                judge_model,
                judge_prompt_version,
                score,
                normalized_result_json,
                explanation,
                failure_code,
                raw_response
            )
            VALUES (
                $1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9, $10,
                $11, $12::jsonb, $13, $14, $15::jsonb
            )
            ON CONFLICT (eval_run_id, runtime_run_id, iteration_id, suite_name)
            DO UPDATE SET
                suite_id = EXCLUDED.suite_id,
                suite_version = EXCLUDED.suite_version,
                category = EXCLUDED.category,
                scope = EXCLUDED.scope,
                judge_model = EXCLUDED.judge_model,
                judge_prompt_version = EXCLUDED.judge_prompt_version,
                score = EXCLUDED.score,
                normalized_result_json = EXCLUDED.normalized_result_json,
                explanation = EXCLUDED.explanation,
                failure_code = EXCLUDED.failure_code,
                raw_response = EXCLUDED.raw_response,
                updated_at = NOW()
            "#,
        )
        .bind(row.eval_run_id)
        .bind(row.runtime_run_id)
        .bind(row.iteration_id)
        .bind(&row.suite_name)
        .bind(&row.suite_id)
        .bind(&row.suite_version)
        .bind(&row.category)
        .bind(&row.scope)
        .bind(&row.judge_model)
        .bind(&row.judge_prompt_version)
        .bind(row.score)
        .bind(&row.normalized_result_json)
        .bind(&row.explanation)
        .bind(&row.failure_code)
        .bind(&row.raw_response)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Insert(e.to_string()))?;
        Ok(())
    }

    pub async fn insert_judge_llm_call(
        &self,
        row: &JudgeLlmCallRow,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            INSERT INTO diagnostics.judge_llm_calls (
                call_id,
                eval_run_id,
                runtime_run_id,
                iteration_id,
                suite_name,
                stage_name,
                judge_provider,
                judge_model,
                judge_base_url,
                judge_prompt_version,
                token_count_source,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                input_cost_per_million_tokens,
                output_cost_per_million_tokens,
                prompt_cost_usd,
                completion_cost_usd,
                total_cost_usd,
                raw_response
            )
            VALUES (
                $1, $2::uuid, $3::uuid, $4::uuid, $5, $6, $7, $8, $9, $10, $11,
                $12, $13, $14, $15, $16, $17, $18, $19, $20::jsonb
            )
            ON CONFLICT (call_id) DO NOTHING
            "#,
        )
        .bind(&row.call_id)
        .bind(row.eval_run_id)
        .bind(row.runtime_run_id)
        .bind(row.iteration_id)
        .bind(&row.suite_name)
        .bind(&row.stage_name)
        .bind(&row.judge_provider)
        .bind(&row.judge_model)
        .bind(&row.judge_base_url)
        .bind(&row.judge_prompt_version)
        .bind(&row.token_count_source)
        .bind(row.prompt_tokens)
        .bind(row.completion_tokens)
        .bind(row.total_tokens)
        .bind(row.input_cost_per_million_tokens)
        .bind(row.output_cost_per_million_tokens)
        .bind(row.prompt_cost_usd)
        .bind(row.completion_cost_usd)
        .bind(row.total_cost_usd)
        .bind(&row.raw_response)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Insert(e.to_string()))?;
        Ok(())
    }

    pub async fn judge_result_exists(
        &self,
        key: &EvalSubjectKey,
        suite_name: &str,
    ) -> Result<bool, StorageError> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM diagnostics.judge_results
                WHERE eval_run_id = $1::uuid
                  AND runtime_run_id = $2::uuid
                  AND iteration_id = $3::uuid
                  AND suite_name = $4
            )
            "#,
        )
        .bind(key.eval_run_id)
        .bind(key.runtime_run_id)
        .bind(key.iteration_id)
        .bind(suite_name)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StorageError::Query(e.to_string()))?;
        Ok(exists)
    }

    pub async fn list_judge_results_for_subject(
        &self,
        key: &EvalSubjectKey,
    ) -> Result<Vec<JudgeResultRow>, StorageError> {
        let rows = sqlx::query(
            r#"
            SELECT
                eval_run_id,
                runtime_run_id,
                iteration_id,
                suite_name,
                suite_id,
                suite_version,
                category,
                scope,
                judge_model,
                judge_prompt_version,
                score,
                normalized_result_json,
                explanation,
                failure_code,
                raw_response
            FROM diagnostics.judge_results
            WHERE eval_run_id = $1::uuid
              AND runtime_run_id = $2::uuid
              AND iteration_id = $3::uuid
            ORDER BY suite_name ASC
            "#,
        )
        .bind(key.eval_run_id)
        .bind(key.runtime_run_id)
        .bind(key.iteration_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Query(e.to_string()))?;

        rows.iter().map(decode_judge_result_row).collect()
    }

    pub async fn list_judge_llm_calls_for_subject(
        &self,
        key: &EvalSubjectKey,
    ) -> Result<Vec<JudgeLlmCallRow>, StorageError> {
        let rows = sqlx::query(
            r#"
            SELECT
                call_id,
                eval_run_id,
                runtime_run_id,
                iteration_id,
                suite_name,
                stage_name,
                judge_provider,
                judge_model,
                judge_base_url,
                judge_prompt_version,
                token_count_source,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                input_cost_per_million_tokens::double precision AS input_cost_per_million_tokens,
                output_cost_per_million_tokens::double precision AS output_cost_per_million_tokens,
                prompt_cost_usd::double precision AS prompt_cost_usd,
                completion_cost_usd::double precision AS completion_cost_usd,
                total_cost_usd::double precision AS total_cost_usd,
                raw_response
            FROM diagnostics.judge_llm_calls
            WHERE eval_run_id = $1::uuid
              AND runtime_run_id = $2::uuid
              AND iteration_id = $3::uuid
            ORDER BY created_at ASC, call_id ASC
            "#,
        )
        .bind(key.eval_run_id)
        .bind(key.runtime_run_id)
        .bind(key.iteration_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Query(e.to_string()))?;

        rows.iter().map(decode_judge_llm_call_row).collect()
    }

    pub async fn upsert_eval_iteration_summary(
        &self,
        row: &EvalIterationSummaryRow,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            INSERT INTO diagnostics.eval_iteration_summaries (
                eval_run_id,
                runtime_run_id,
                iteration_id,
                query_structuring_judge_score,
                evidence_pack_judge_score,
                final_answer_judge_score,
                query_structuring_no_hard_fail,
                evidence_pack_no_hard_fail,
                final_answer_no_hard_fail,
                usable_first_response,
                no_root_cause_gate_passed,
                single_check_gate_passed,
                source_alignment_gate_passed,
                field_boundary_gate_passed,
                evidence_pack_gate_passed,
                query_structuring_field_boundary_correctness_score,
                query_structuring_grounding_conservatism_score,
                evidence_pack_role_fit_score,
                evidence_pack_sufficiency_score,
                final_no_root_cause_claim_score,
                final_first_check_discriminates_score,
                final_hypothesis_source_alignment_score,
                final_alternative_context_handling_score,
                final_result_interpretation_usefulness_score,
                runtime_prompt_tokens,
                runtime_completion_tokens,
                runtime_total_tokens,
                runtime_total_cost_usd,
                judge_prompt_tokens,
                judge_completion_tokens,
                judge_total_tokens,
                judge_total_cost_usd,
                run_total_tokens,
                run_total_cost_usd
            )
            VALUES (
                $1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
                $21, $22, $23, $24, $25, $26, $27, $28, $29, $30,
                $31, $32, $33, $34
            )
            ON CONFLICT (eval_run_id, runtime_run_id, iteration_id)
            DO UPDATE SET
                query_structuring_judge_score = EXCLUDED.query_structuring_judge_score,
                evidence_pack_judge_score = EXCLUDED.evidence_pack_judge_score,
                final_answer_judge_score = EXCLUDED.final_answer_judge_score,
                query_structuring_no_hard_fail = EXCLUDED.query_structuring_no_hard_fail,
                evidence_pack_no_hard_fail = EXCLUDED.evidence_pack_no_hard_fail,
                final_answer_no_hard_fail = EXCLUDED.final_answer_no_hard_fail,
                usable_first_response = EXCLUDED.usable_first_response,
                no_root_cause_gate_passed = EXCLUDED.no_root_cause_gate_passed,
                single_check_gate_passed = EXCLUDED.single_check_gate_passed,
                source_alignment_gate_passed = EXCLUDED.source_alignment_gate_passed,
                field_boundary_gate_passed = EXCLUDED.field_boundary_gate_passed,
                evidence_pack_gate_passed = EXCLUDED.evidence_pack_gate_passed,
                query_structuring_field_boundary_correctness_score = EXCLUDED.query_structuring_field_boundary_correctness_score,
                query_structuring_grounding_conservatism_score = EXCLUDED.query_structuring_grounding_conservatism_score,
                evidence_pack_role_fit_score = EXCLUDED.evidence_pack_role_fit_score,
                evidence_pack_sufficiency_score = EXCLUDED.evidence_pack_sufficiency_score,
                final_no_root_cause_claim_score = EXCLUDED.final_no_root_cause_claim_score,
                final_first_check_discriminates_score = EXCLUDED.final_first_check_discriminates_score,
                final_hypothesis_source_alignment_score = EXCLUDED.final_hypothesis_source_alignment_score,
                final_alternative_context_handling_score = EXCLUDED.final_alternative_context_handling_score,
                final_result_interpretation_usefulness_score = EXCLUDED.final_result_interpretation_usefulness_score,
                runtime_prompt_tokens = EXCLUDED.runtime_prompt_tokens,
                runtime_completion_tokens = EXCLUDED.runtime_completion_tokens,
                runtime_total_tokens = EXCLUDED.runtime_total_tokens,
                runtime_total_cost_usd = EXCLUDED.runtime_total_cost_usd,
                judge_prompt_tokens = EXCLUDED.judge_prompt_tokens,
                judge_completion_tokens = EXCLUDED.judge_completion_tokens,
                judge_total_tokens = EXCLUDED.judge_total_tokens,
                judge_total_cost_usd = EXCLUDED.judge_total_cost_usd,
                run_total_tokens = EXCLUDED.run_total_tokens,
                run_total_cost_usd = EXCLUDED.run_total_cost_usd,
                updated_at = NOW()
            "#,
        )
        .bind(row.key.eval_run_id)
        .bind(row.key.runtime_run_id)
        .bind(row.key.iteration_id)
        .bind(row.query_structuring_judge_score)
        .bind(row.evidence_pack_judge_score)
        .bind(row.final_answer_judge_score)
        .bind(row.query_structuring_no_hard_fail)
        .bind(row.evidence_pack_no_hard_fail)
        .bind(row.final_answer_no_hard_fail)
        .bind(row.usable_first_response)
        .bind(row.no_root_cause_gate_passed)
        .bind(row.single_check_gate_passed)
        .bind(row.source_alignment_gate_passed)
        .bind(row.field_boundary_gate_passed)
        .bind(row.evidence_pack_gate_passed)
        .bind(row.query_structuring_field_boundary_correctness_score)
        .bind(row.query_structuring_grounding_conservatism_score)
        .bind(row.evidence_pack_role_fit_score)
        .bind(row.evidence_pack_sufficiency_score)
        .bind(row.final_no_root_cause_claim_score)
        .bind(row.final_first_check_discriminates_score)
        .bind(row.final_hypothesis_source_alignment_score)
        .bind(row.final_alternative_context_handling_score)
        .bind(row.final_result_interpretation_usefulness_score)
        .bind(row.runtime_prompt_tokens)
        .bind(row.runtime_completion_tokens)
        .bind(row.runtime_total_tokens)
        .bind(row.runtime_total_cost_usd)
        .bind(row.judge_prompt_tokens)
        .bind(row.judge_completion_tokens)
        .bind(row.judge_total_tokens)
        .bind(row.judge_total_cost_usd)
        .bind(row.run_total_tokens)
        .bind(row.run_total_cost_usd)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Insert(e.to_string()))?;
        Ok(())
    }

    pub async fn list_eval_iteration_summaries(
        &self,
        eval_run_id: Uuid,
    ) -> Result<Vec<EvalIterationSummaryRow>, StorageError> {
        let rows = sqlx::query(
            r#"
            SELECT
                eval_run_id,
                runtime_run_id,
                iteration_id,
                query_structuring_judge_score::double precision AS query_structuring_judge_score,
                evidence_pack_judge_score::double precision AS evidence_pack_judge_score,
                final_answer_judge_score::double precision AS final_answer_judge_score,
                query_structuring_no_hard_fail,
                evidence_pack_no_hard_fail,
                final_answer_no_hard_fail,
                usable_first_response,
                no_root_cause_gate_passed,
                single_check_gate_passed,
                source_alignment_gate_passed,
                field_boundary_gate_passed,
                evidence_pack_gate_passed,
                query_structuring_field_boundary_correctness_score,
                query_structuring_grounding_conservatism_score,
                evidence_pack_role_fit_score,
                evidence_pack_sufficiency_score,
                final_no_root_cause_claim_score,
                final_first_check_discriminates_score,
                final_hypothesis_source_alignment_score,
                final_alternative_context_handling_score,
                final_result_interpretation_usefulness_score,
                runtime_prompt_tokens,
                runtime_completion_tokens,
                runtime_total_tokens,
                runtime_total_cost_usd::double precision AS runtime_total_cost_usd,
                judge_prompt_tokens,
                judge_completion_tokens,
                judge_total_tokens,
                judge_total_cost_usd::double precision AS judge_total_cost_usd,
                run_total_tokens,
                run_total_cost_usd::double precision AS run_total_cost_usd
            FROM diagnostics.eval_iteration_summaries
            WHERE eval_run_id = $1::uuid
            ORDER BY final_answer_judge_score ASC, runtime_run_id ASC, iteration_id ASC
            "#,
        )
        .bind(eval_run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Query(e.to_string()))?;

        rows.iter().map(decode_eval_iteration_summary_row).collect()
    }

    pub async fn upsert_eval_run_summary(
        &self,
        row: &EvalRunSummaryRow,
    ) -> Result<(), StorageError> {
        sqlx::query(
            r#"
            INSERT INTO diagnostics.eval_run_summaries (
                eval_run_id,
                run_type,
                status,
                started_at,
                completed_at,
                runtime_run_count,
                iterations_evaluated_count,
                judge_provider,
                judge_model,
                suite_versions,
                usable_first_response_rate,
                query_structuring_judge_score,
                evidence_pack_judge_score,
                final_answer_judge_score,
                query_structuring_strict_pass_rate,
                evidence_pack_strict_pass_rate,
                final_answer_strict_pass_rate,
                diagnostic_move_hard_fail_rate,
                gate_pass_rate,
                bad_final_due_to_query_rate,
                bad_final_due_to_evidence_rate,
                bad_final_with_good_query_and_evidence_rate,
                runtime_prompt_tokens,
                runtime_completion_tokens,
                runtime_total_tokens,
                runtime_total_cost_usd,
                judge_prompt_tokens,
                judge_completion_tokens,
                judge_total_tokens,
                judge_total_cost_usd,
                run_total_tokens,
                run_total_cost_usd
            )
            VALUES (
                $1::uuid, $2, $3, $4, $5, $6, $7, $8, $9, $10::jsonb,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
                $21, $22, $23, $24, $25, $26, $27, $28, $29, $30,
                $31, $32
            )
            ON CONFLICT (eval_run_id)
            DO UPDATE SET
                run_type = EXCLUDED.run_type,
                status = EXCLUDED.status,
                started_at = EXCLUDED.started_at,
                completed_at = EXCLUDED.completed_at,
                runtime_run_count = EXCLUDED.runtime_run_count,
                iterations_evaluated_count = EXCLUDED.iterations_evaluated_count,
                judge_provider = EXCLUDED.judge_provider,
                judge_model = EXCLUDED.judge_model,
                suite_versions = EXCLUDED.suite_versions,
                usable_first_response_rate = EXCLUDED.usable_first_response_rate,
                query_structuring_judge_score = EXCLUDED.query_structuring_judge_score,
                evidence_pack_judge_score = EXCLUDED.evidence_pack_judge_score,
                final_answer_judge_score = EXCLUDED.final_answer_judge_score,
                query_structuring_strict_pass_rate = EXCLUDED.query_structuring_strict_pass_rate,
                evidence_pack_strict_pass_rate = EXCLUDED.evidence_pack_strict_pass_rate,
                final_answer_strict_pass_rate = EXCLUDED.final_answer_strict_pass_rate,
                diagnostic_move_hard_fail_rate = EXCLUDED.diagnostic_move_hard_fail_rate,
                gate_pass_rate = EXCLUDED.gate_pass_rate,
                bad_final_due_to_query_rate = EXCLUDED.bad_final_due_to_query_rate,
                bad_final_due_to_evidence_rate = EXCLUDED.bad_final_due_to_evidence_rate,
                bad_final_with_good_query_and_evidence_rate = EXCLUDED.bad_final_with_good_query_and_evidence_rate,
                runtime_prompt_tokens = EXCLUDED.runtime_prompt_tokens,
                runtime_completion_tokens = EXCLUDED.runtime_completion_tokens,
                runtime_total_tokens = EXCLUDED.runtime_total_tokens,
                runtime_total_cost_usd = EXCLUDED.runtime_total_cost_usd,
                judge_prompt_tokens = EXCLUDED.judge_prompt_tokens,
                judge_completion_tokens = EXCLUDED.judge_completion_tokens,
                judge_total_tokens = EXCLUDED.judge_total_tokens,
                judge_total_cost_usd = EXCLUDED.judge_total_cost_usd,
                run_total_tokens = EXCLUDED.run_total_tokens,
                run_total_cost_usd = EXCLUDED.run_total_cost_usd,
                updated_at = NOW()
            "#,
        )
        .bind(row.eval_run_id)
        .bind(&row.run_type)
        .bind(&row.status)
        .bind(row.started_at)
        .bind(row.completed_at)
        .bind(row.runtime_run_count)
        .bind(row.iterations_evaluated_count)
        .bind(&row.judge_provider)
        .bind(&row.judge_model)
        .bind(&row.suite_versions)
        .bind(row.usable_first_response_rate)
        .bind(row.query_structuring_judge_score)
        .bind(row.evidence_pack_judge_score)
        .bind(row.final_answer_judge_score)
        .bind(row.query_structuring_strict_pass_rate)
        .bind(row.evidence_pack_strict_pass_rate)
        .bind(row.final_answer_strict_pass_rate)
        .bind(row.diagnostic_move_hard_fail_rate)
        .bind(row.gate_pass_rate)
        .bind(row.bad_final_due_to_query_rate)
        .bind(row.bad_final_due_to_evidence_rate)
        .bind(row.bad_final_with_good_query_and_evidence_rate)
        .bind(row.runtime_prompt_tokens)
        .bind(row.runtime_completion_tokens)
        .bind(row.runtime_total_tokens)
        .bind(row.runtime_total_cost_usd)
        .bind(row.judge_prompt_tokens)
        .bind(row.judge_completion_tokens)
        .bind(row.judge_total_tokens)
        .bind(row.judge_total_cost_usd)
        .bind(row.run_total_tokens)
        .bind(row.run_total_cost_usd)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Insert(e.to_string()))?;
        Ok(())
    }
}

fn decode_eval_processing_state_row(
    row: &PgRow,
) -> Result<EvalProcessingStateRow, StorageError> {
    Ok(EvalProcessingStateRow {
        key: EvalSubjectKey {
            eval_run_id: row
                .try_get("eval_run_id")
                .map_err(|_| StorageError::InvalidStoredRow("eval_run_id"))?,
            runtime_run_id: row
                .try_get("runtime_run_id")
                .map_err(|_| StorageError::InvalidStoredRow("runtime_run_id"))?,
            iteration_id: row
                .try_get("iteration_id")
                .map_err(|_| StorageError::InvalidStoredRow("iteration_id"))?,
        },
        subject_received_at: row
            .try_get("subject_received_at")
            .map_err(|_| StorageError::InvalidStoredRow("subject_received_at"))?,
        current_stage: row
            .try_get("current_stage")
            .map_err(|_| StorageError::InvalidStoredRow("current_stage"))?,
        status: row
            .try_get("status")
            .map_err(|_| StorageError::InvalidStoredRow("status"))?,
        attempt_count: row
            .try_get("attempt_count")
            .map_err(|_| StorageError::InvalidStoredRow("attempt_count"))?,
        started_at: row
            .try_get("started_at")
            .map_err(|_| StorageError::InvalidStoredRow("started_at"))?,
        completed_at: row
            .try_get("completed_at")
            .map_err(|_| StorageError::InvalidStoredRow("completed_at"))?,
        updated_at: row
            .try_get("updated_at")
            .map_err(|_| StorageError::InvalidStoredRow("updated_at"))?,
        last_error: row
            .try_get("last_error")
            .map_err(|_| StorageError::InvalidStoredRow("last_error"))?,
    })
}

fn decode_judge_result_row(
    row: &PgRow,
) -> Result<JudgeResultRow, StorageError> {
    Ok(JudgeResultRow {
        eval_run_id: row
            .try_get("eval_run_id")
            .map_err(|_| StorageError::InvalidStoredRow("eval_run_id"))?,
        runtime_run_id: row
            .try_get("runtime_run_id")
            .map_err(|_| StorageError::InvalidStoredRow("runtime_run_id"))?,
        iteration_id: row
            .try_get("iteration_id")
            .map_err(|_| StorageError::InvalidStoredRow("iteration_id"))?,
        suite_name: row
            .try_get("suite_name")
            .map_err(|_| StorageError::InvalidStoredRow("suite_name"))?,
        suite_id: row
            .try_get("suite_id")
            .map_err(|_| StorageError::InvalidStoredRow("suite_id"))?,
        suite_version: row
            .try_get("suite_version")
            .map_err(|_| StorageError::InvalidStoredRow("suite_version"))?,
        category: row
            .try_get("category")
            .map_err(|_| StorageError::InvalidStoredRow("category"))?,
        scope: row
            .try_get("scope")
            .map_err(|_| StorageError::InvalidStoredRow("scope"))?,
        judge_model: row
            .try_get("judge_model")
            .map_err(|_| StorageError::InvalidStoredRow("judge_model"))?,
        judge_prompt_version: row
            .try_get("judge_prompt_version")
            .map_err(|_| StorageError::InvalidStoredRow("judge_prompt_version"))?,
        score: row
            .try_get("score")
            .map_err(|_| StorageError::InvalidStoredRow("score"))?,
        raw_response: row
            .try_get("raw_response")
            .map_err(|_| StorageError::InvalidStoredRow("raw_response"))?,
        explanation: row
            .try_get("explanation")
            .map_err(|_| StorageError::InvalidStoredRow("explanation"))?,
        normalized_result_json: row
            .try_get("normalized_result_json")
            .map_err(|_| StorageError::InvalidStoredRow("normalized_result_json"))?,
        failure_code: row
            .try_get("failure_code")
            .map_err(|_| StorageError::InvalidStoredRow("failure_code"))?,
    })
}

fn decode_judge_llm_call_row(
    row: &PgRow,
) -> Result<JudgeLlmCallRow, StorageError> {
    Ok(JudgeLlmCallRow {
        call_id: row
            .try_get("call_id")
            .map_err(|_| StorageError::InvalidStoredRow("call_id"))?,
        eval_run_id: row
            .try_get("eval_run_id")
            .map_err(|_| StorageError::InvalidStoredRow("eval_run_id"))?,
        runtime_run_id: row
            .try_get("runtime_run_id")
            .map_err(|_| StorageError::InvalidStoredRow("runtime_run_id"))?,
        iteration_id: row
            .try_get("iteration_id")
            .map_err(|_| StorageError::InvalidStoredRow("iteration_id"))?,
        suite_name: row
            .try_get("suite_name")
            .map_err(|_| StorageError::InvalidStoredRow("suite_name"))?,
        stage_name: row
            .try_get("stage_name")
            .map_err(|_| StorageError::InvalidStoredRow("stage_name"))?,
        judge_provider: row
            .try_get("judge_provider")
            .map_err(|_| StorageError::InvalidStoredRow("judge_provider"))?,
        judge_model: row
            .try_get("judge_model")
            .map_err(|_| StorageError::InvalidStoredRow("judge_model"))?,
        judge_base_url: row
            .try_get("judge_base_url")
            .map_err(|_| StorageError::InvalidStoredRow("judge_base_url"))?,
        judge_prompt_version: row
            .try_get("judge_prompt_version")
            .map_err(|_| StorageError::InvalidStoredRow("judge_prompt_version"))?,
        token_count_source: row
            .try_get("token_count_source")
            .map_err(|_| StorageError::InvalidStoredRow("token_count_source"))?,
        prompt_tokens: row
            .try_get("prompt_tokens")
            .map_err(|_| StorageError::InvalidStoredRow("prompt_tokens"))?,
        completion_tokens: row
            .try_get("completion_tokens")
            .map_err(|_| StorageError::InvalidStoredRow("completion_tokens"))?,
        total_tokens: row
            .try_get("total_tokens")
            .map_err(|_| StorageError::InvalidStoredRow("total_tokens"))?,
        input_cost_per_million_tokens: row
            .try_get("input_cost_per_million_tokens")
            .map_err(|_| StorageError::InvalidStoredRow("input_cost_per_million_tokens"))?,
        output_cost_per_million_tokens: row
            .try_get("output_cost_per_million_tokens")
            .map_err(|_| StorageError::InvalidStoredRow("output_cost_per_million_tokens"))?,
        prompt_cost_usd: row
            .try_get("prompt_cost_usd")
            .map_err(|_| StorageError::InvalidStoredRow("prompt_cost_usd"))?,
        completion_cost_usd: row
            .try_get("completion_cost_usd")
            .map_err(|_| StorageError::InvalidStoredRow("completion_cost_usd"))?,
        total_cost_usd: row
            .try_get("total_cost_usd")
            .map_err(|_| StorageError::InvalidStoredRow("total_cost_usd"))?,
        raw_response: row
            .try_get("raw_response")
            .map_err(|_| StorageError::InvalidStoredRow("raw_response"))?,
    })
}

fn decode_eval_iteration_summary_row(
    row: &PgRow,
) -> Result<EvalIterationSummaryRow, StorageError> {
    Ok(EvalIterationSummaryRow {
        key: EvalSubjectKey {
            eval_run_id: row
                .try_get("eval_run_id")
                .map_err(|_| StorageError::InvalidStoredRow("eval_run_id"))?,
            runtime_run_id: row
                .try_get("runtime_run_id")
                .map_err(|_| StorageError::InvalidStoredRow("runtime_run_id"))?,
            iteration_id: row
                .try_get("iteration_id")
                .map_err(|_| StorageError::InvalidStoredRow("iteration_id"))?,
        },
        query_structuring_judge_score: row
            .try_get("query_structuring_judge_score")
            .map_err(|_| StorageError::InvalidStoredRow("query_structuring_judge_score"))?,
        evidence_pack_judge_score: row
            .try_get("evidence_pack_judge_score")
            .map_err(|_| StorageError::InvalidStoredRow("evidence_pack_judge_score"))?,
        final_answer_judge_score: row
            .try_get("final_answer_judge_score")
            .map_err(|_| StorageError::InvalidStoredRow("final_answer_judge_score"))?,
        query_structuring_no_hard_fail: row
            .try_get("query_structuring_no_hard_fail")
            .map_err(|_| StorageError::InvalidStoredRow("query_structuring_no_hard_fail"))?,
        evidence_pack_no_hard_fail: row
            .try_get("evidence_pack_no_hard_fail")
            .map_err(|_| StorageError::InvalidStoredRow("evidence_pack_no_hard_fail"))?,
        final_answer_no_hard_fail: row
            .try_get("final_answer_no_hard_fail")
            .map_err(|_| StorageError::InvalidStoredRow("final_answer_no_hard_fail"))?,
        usable_first_response: row
            .try_get("usable_first_response")
            .map_err(|_| StorageError::InvalidStoredRow("usable_first_response"))?,
        no_root_cause_gate_passed: row
            .try_get("no_root_cause_gate_passed")
            .map_err(|_| StorageError::InvalidStoredRow("no_root_cause_gate_passed"))?,
        single_check_gate_passed: row
            .try_get("single_check_gate_passed")
            .map_err(|_| StorageError::InvalidStoredRow("single_check_gate_passed"))?,
        source_alignment_gate_passed: row
            .try_get("source_alignment_gate_passed")
            .map_err(|_| StorageError::InvalidStoredRow("source_alignment_gate_passed"))?,
        field_boundary_gate_passed: row
            .try_get("field_boundary_gate_passed")
            .map_err(|_| StorageError::InvalidStoredRow("field_boundary_gate_passed"))?,
        evidence_pack_gate_passed: row
            .try_get("evidence_pack_gate_passed")
            .map_err(|_| StorageError::InvalidStoredRow("evidence_pack_gate_passed"))?,
        query_structuring_field_boundary_correctness_score: row
            .try_get("query_structuring_field_boundary_correctness_score")
            .map_err(|_| StorageError::InvalidStoredRow("query_structuring_field_boundary_correctness_score"))?,
        query_structuring_grounding_conservatism_score: row
            .try_get("query_structuring_grounding_conservatism_score")
            .map_err(|_| StorageError::InvalidStoredRow("query_structuring_grounding_conservatism_score"))?,
        evidence_pack_role_fit_score: row
            .try_get("evidence_pack_role_fit_score")
            .map_err(|_| StorageError::InvalidStoredRow("evidence_pack_role_fit_score"))?,
        evidence_pack_sufficiency_score: row
            .try_get("evidence_pack_sufficiency_score")
            .map_err(|_| StorageError::InvalidStoredRow("evidence_pack_sufficiency_score"))?,
        final_no_root_cause_claim_score: row
            .try_get("final_no_root_cause_claim_score")
            .map_err(|_| StorageError::InvalidStoredRow("final_no_root_cause_claim_score"))?,
        final_first_check_discriminates_score: row
            .try_get("final_first_check_discriminates_score")
            .map_err(|_| StorageError::InvalidStoredRow("final_first_check_discriminates_score"))?,
        final_hypothesis_source_alignment_score: row
            .try_get("final_hypothesis_source_alignment_score")
            .map_err(|_| StorageError::InvalidStoredRow("final_hypothesis_source_alignment_score"))?,
        final_alternative_context_handling_score: row
            .try_get("final_alternative_context_handling_score")
            .map_err(|_| StorageError::InvalidStoredRow("final_alternative_context_handling_score"))?,
        final_result_interpretation_usefulness_score: row
            .try_get("final_result_interpretation_usefulness_score")
            .map_err(|_| StorageError::InvalidStoredRow("final_result_interpretation_usefulness_score"))?,
        runtime_prompt_tokens: row
            .try_get("runtime_prompt_tokens")
            .map_err(|_| StorageError::InvalidStoredRow("runtime_prompt_tokens"))?,
        runtime_completion_tokens: row
            .try_get("runtime_completion_tokens")
            .map_err(|_| StorageError::InvalidStoredRow("runtime_completion_tokens"))?,
        runtime_total_tokens: row
            .try_get("runtime_total_tokens")
            .map_err(|_| StorageError::InvalidStoredRow("runtime_total_tokens"))?,
        runtime_total_cost_usd: row
            .try_get("runtime_total_cost_usd")
            .map_err(|_| StorageError::InvalidStoredRow("runtime_total_cost_usd"))?,
        judge_prompt_tokens: row
            .try_get("judge_prompt_tokens")
            .map_err(|_| StorageError::InvalidStoredRow("judge_prompt_tokens"))?,
        judge_completion_tokens: row
            .try_get("judge_completion_tokens")
            .map_err(|_| StorageError::InvalidStoredRow("judge_completion_tokens"))?,
        judge_total_tokens: row
            .try_get("judge_total_tokens")
            .map_err(|_| StorageError::InvalidStoredRow("judge_total_tokens"))?,
        judge_total_cost_usd: row
            .try_get("judge_total_cost_usd")
            .map_err(|_| StorageError::InvalidStoredRow("judge_total_cost_usd"))?,
        run_total_tokens: row
            .try_get("run_total_tokens")
            .map_err(|_| StorageError::InvalidStoredRow("run_total_tokens"))?,
        run_total_cost_usd: row
            .try_get("run_total_cost_usd")
            .map_err(|_| StorageError::InvalidStoredRow("run_total_cost_usd"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::{EvalProcessingStatus, EvalStage, EvalSubjectKey, PostgresEvalStore, StorageError};
    use crate::config::PostgresSettings;
    use uuid::Uuid;

    #[tokio::test]
    async fn new_fails_when_postgres_url_is_empty() {
        let err = PostgresEvalStore::new(&PostgresSettings {
            url: " ".to_string(),
        })
        .await
        .unwrap_err();
        assert!(matches!(err, StorageError::InvalidConfig(_)));
    }

    #[test]
    fn stage_and_status_strings_match_ddl() {
        assert_eq!(EvalStage::JudgeRequestSuites.as_str(), "judge_request_suites");
        assert_eq!(EvalStage::BuildEvalSummary.as_str(), "build_eval_summary");
        assert_eq!(EvalProcessingStatus::Pending.as_str(), "pending");
        assert_eq!(EvalProcessingStatus::Running.as_str(), "running");
        assert_eq!(EvalProcessingStatus::Completed.as_str(), "completed");
        assert_eq!(EvalProcessingStatus::Failed.as_str(), "failed");
    }

    #[test]
    fn eval_subject_key_is_constructible() {
        let key = EvalSubjectKey {
            eval_run_id: Uuid::nil(),
            runtime_run_id: Uuid::nil(),
            iteration_id: Uuid::nil(),
        };
        assert_eq!(key.eval_run_id, Uuid::nil());
    }
}
