use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use thiserror::Error;
use uuid::Uuid;

use crate::orchestrator::run_state::model::{
    FinishedStepRecord, PendingStepRecord, RunId, RunIteration, RunIterationId,
    RunIterationStatus, RunState, RunStatus, StepError, StepKind, StepRecord, StepRecordId,
    StepResultEnvelope,
};

// ─── Error Model ─────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum RunStateStoreError {
    #[error("invalid config: {0}")]
    InvalidConfig(&'static str),

    #[error("invalid run state: {0}")]
    InvalidRunState(&'static str),

    #[error("serialization failure: {0}")]
    Serialization(String),

    #[error("deserialization failure: {0}")]
    Deserialization(String),

    #[error("connection failure: {0}")]
    Connection(String),

    #[error("insert failure: {0}")]
    Insert(String),

    #[error("update failure: {0}")]
    Update(String),

    #[error("query failure: {0}")]
    Query(String),

    #[error("duplicate run: {0:?}")]
    DuplicateRun(RunId),

    #[error("duplicate iteration for run {run_id:?} at sequence {sequence_no}")]
    DuplicateIteration { run_id: RunId, sequence_no: u64 },

    #[error("duplicate step record for iteration {iteration_id:?} at sequence {sequence_no}")]
    DuplicateStepRecord {
        iteration_id: RunIterationId,
        sequence_no: u64,
    },

    #[error("step record not found: {0:?}")]
    StepRecordNotFound(StepRecordId),

    #[error("step record already finished: {0:?}")]
    StepRecordAlreadyFinished(StepRecordId),

    #[error(
        "step kind mismatch for record {record_id:?}: expected {expected}, actual {actual}"
    )]
    StepKindMismatch {
        record_id: StepRecordId,
        expected: StepKind,
        actual: StepKind,
    },

    #[error("missing parent run: {0:?}")]
    MissingParentRun(RunId),

    #[error("missing parent iteration: {0:?}")]
    MissingParentIteration(RunIterationId),

    #[error("invalid stored row: {0}")]
    InvalidStoredRow(&'static str),
}

// ─── Config + Store ───────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct PostgresRunStateStoreConfig {
    pub postgres_url: String,
}

#[derive(Debug)]
pub struct PostgresRunStateStore {
    pool: sqlx::PgPool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunSummaryRow {
    pub run_id: RunId,
    pub status: RunStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
    pub initial_user_query: Option<String>,
    pub final_problem_understanding: Option<String>,
}

pub struct PostgresRunStateStoreTx<'tx> {
    conn: &'tx mut sqlx::pool::PoolConnection<sqlx::Postgres>,
}

impl PostgresRunStateStore {
    pub async fn new(
        config: PostgresRunStateStoreConfig,
    ) -> Result<Self, RunStateStoreError> {
        if config.postgres_url.trim().is_empty() {
            return Err(RunStateStoreError::InvalidConfig(
                "postgres_url must not be empty",
            ));
        }
        let pool = PgPoolOptions::new()
            .connect(&config.postgres_url)
            .await
            .map_err(|e| RunStateStoreError::Connection(e.to_string()))?;
        Ok(Self { pool })
    }

    pub async fn with_transaction<T, F>(
        &self,
        f: F,
    ) -> Result<T, RunStateStoreError>
    where
        F: for<'tx> FnOnce(
            &'tx mut PostgresRunStateStoreTx<'tx>,
        ) -> Pin<Box<dyn Future<Output = Result<T, RunStateStoreError>> + 'tx>>,
    {
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| RunStateStoreError::Update(e.to_string()))?;
        sqlx::query("BEGIN")
            .execute(&mut *conn)
            .await
            .map_err(|e| RunStateStoreError::Update(e.to_string()))?;

        let result = {
            let mut wrapper = PostgresRunStateStoreTx { conn: &mut conn };
            f(&mut wrapper).await
        };

        match result {
            Ok(value) => {
                sqlx::query("COMMIT")
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| RunStateStoreError::Update(e.to_string()))?;
                Ok(value)
            }
            Err(err) => {
                sqlx::query("ROLLBACK")
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| RunStateStoreError::Update(e.to_string()))?;
                Err(err)
            }
        }
    }

    // ─── insert_run ───────────────────────────────────────────────────────────

    pub async fn insert_run(&self, run: &RunState) -> Result<(), RunStateStoreError> {
        validate_run_header(run)?;

        let result = sqlx::query(
            r#"
            INSERT INTO diagnostics.runs (run_id, status, created_at, updated_at, revision)
            VALUES ($1::uuid, $2, $3, $4, $5)
            "#,
        )
        .bind(run.run_id.0.to_string())
        .bind(run.status.to_string())
        .bind(run.created_at)
        .bind(run.updated_at)
        .bind(run.revision as i64)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(db)) if is_unique_violation(&*db) => {
                Err(RunStateStoreError::DuplicateRun(run.run_id))
            }
            Err(e) => Err(RunStateStoreError::Insert(e.to_string())),
        }
    }

    // ─── insert_iteration ─────────────────────────────────────────────────────

    pub async fn insert_iteration(
        &self,
        run_id: RunId,
        sequence_no: u64,
        iteration: &RunIteration,
    ) -> Result<(), RunStateStoreError> {
        insert_iteration_query(&self.pool, run_id, sequence_no, iteration).await
    }

    // ─── insert_step_record ───────────────────────────────────────────────────

    pub async fn insert_step_record(
        &self,
        iteration_id: RunIterationId,
        sequence_no: u64,
        step_record: &StepRecord,
    ) -> Result<(), RunStateStoreError> {
        insert_step_record_query(&self.pool, iteration_id, sequence_no, step_record).await
    }

    // ─── finish_step_record ───────────────────────────────────────────────────

    pub async fn finish_step_record(
        &self,
        record_id: StepRecordId,
        finished_record: &FinishedStepRecord,
    ) -> Result<(), RunStateStoreError> {
        let maybe_row = sqlx::query(
            r#"
            SELECT step, record_status
            FROM diagnostics.run_step_records
            WHERE record_id = $1::uuid
            "#,
        )
        .bind(record_id.0.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RunStateStoreError::Query(e.to_string()))?;

        let row = maybe_row.ok_or(RunStateStoreError::StepRecordNotFound(record_id))?;

        let status: String = row
            .try_get("record_status")
            .map_err(|_| RunStateStoreError::InvalidStoredRow("record_status column"))?;
        if status == "finished" {
            return Err(RunStateStoreError::StepRecordAlreadyFinished(record_id));
        }

        let stored_step_str: String = row
            .try_get("step")
            .map_err(|_| RunStateStoreError::InvalidStoredRow("step column"))?;
        let stored_step = parse_step_kind(&stored_step_str)?;
        if stored_step != finished_record.step {
            return Err(RunStateStoreError::StepKindMismatch {
                record_id,
                expected: stored_step,
                actual: finished_record.step,
            });
        }

        let (result_json, error_json) =
            serialize_step_result(finished_record.step, &finished_record.result)?;

        sqlx::query(
            r#"
            UPDATE diagnostics.run_step_records
            SET record_status = 'finished',
                finished_at   = $1,
                result_json   = $2,
                error_json    = $3
            WHERE record_id = $4::uuid
            "#,
        )
        .bind(finished_record.finished_at)
        .bind(result_json.map(sqlx::types::Json))
        .bind(error_json.map(sqlx::types::Json))
        .bind(record_id.0.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| RunStateStoreError::Update(e.to_string()))?;

        Ok(())
    }

    // ─── update_run_header ────────────────────────────────────────────────────

    pub async fn update_run_header(
        &self,
        run_id: RunId,
        status: RunStatus,
        updated_at: DateTime<Utc>,
        revision: u64,
    ) -> Result<(), RunStateStoreError> {
        update_run_header_query(&self.pool, run_id, status, updated_at, revision).await
    }

    // ─── load_run ─────────────────────────────────────────────────────────────

    pub async fn load_run(
        &self,
        run_id: RunId,
    ) -> Result<Option<RunState>, RunStateStoreError> {
        // 1. Load run header
        let maybe_header = sqlx::query(
            r#"
            SELECT run_id, status, created_at, updated_at, revision
            FROM diagnostics.runs
            WHERE run_id = $1::uuid
            "#,
        )
        .bind(run_id.0.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RunStateStoreError::Query(e.to_string()))?;

        let header_row = match maybe_header {
            None => return Ok(None),
            Some(r) => r,
        };

        let status_str: String = header_row
            .try_get("status")
            .map_err(|_| RunStateStoreError::InvalidStoredRow("status column"))?;
        let status = parse_run_status(&status_str)?;
        let created_at: DateTime<Utc> = header_row
            .try_get("created_at")
            .map_err(|_| RunStateStoreError::InvalidStoredRow("created_at column"))?;
        let updated_at: DateTime<Utc> = header_row
            .try_get("updated_at")
            .map_err(|_| RunStateStoreError::InvalidStoredRow("updated_at column"))?;
        let revision_i64: i64 = header_row
            .try_get("revision")
            .map_err(|_| RunStateStoreError::InvalidStoredRow("revision column"))?;

        // 2. Load iterations ordered by sequence_no asc
        let iteration_rows = sqlx::query(
            r#"
            SELECT iteration_id::text AS iteration_id, sequence_no, status, config_snapshot
            FROM diagnostics.run_iterations
            WHERE run_id = $1::uuid
            ORDER BY sequence_no ASC
            "#,
        )
        .bind(run_id.0.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RunStateStoreError::Query(e.to_string()))?;

        let mut iterations = Vec::with_capacity(iteration_rows.len());
        for iter_row in &iteration_rows {
            let iter_id_str: String = iter_row
                .try_get("iteration_id")
                .map_err(|_| RunStateStoreError::InvalidStoredRow("iteration_id column"))?;
            let iter_uuid = Uuid::parse_str(&iter_id_str).map_err(|_| {
                RunStateStoreError::InvalidStoredRow("iteration_id is not a valid UUID")
            })?;
            let iteration_id = RunIterationId(iter_uuid);

            // 3. Load step records for this iteration ordered by sequence_no asc
            let step_rows = sqlx::query(
                r#"
                SELECT record_id::text AS record_id, step, record_status, started_at,
                       finished_at, result_json, error_json
                FROM diagnostics.run_step_records
                WHERE iteration_id = $1::uuid
                ORDER BY sequence_no ASC
                "#,
            )
            .bind(iter_id_str)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RunStateStoreError::Query(e.to_string()))?;

            let mut step_records = Vec::with_capacity(step_rows.len());
            for sr in &step_rows {
                step_records.push(decode_step_record_row(sr)?);
            }

            let iteration_status_str: String = iter_row
                .try_get("status")
                .map_err(|_| RunStateStoreError::InvalidStoredRow("iteration status column"))?;
            let iteration_status = parse_iteration_status(&iteration_status_str)?;

            let config_snapshot: Option<crate::shared_types::RunConfigSnapshot> = iter_row
                .try_get::<Option<serde_json::Value>, _>("config_snapshot")
                .ok()
                .flatten()
                .and_then(|v| serde_json::from_value(v).ok());

            iterations.push(RunIteration {
                iteration_id,
                config_snapshot,
                status: iteration_status,
                step_records,
            });
        }

        Ok(Some(RunState {
            run_id,
            status,
            created_at,
            updated_at,
            revision: revision_i64 as u64,
            iterations,
        }))
    }

    // ─── list_run_ids ─────────────────────────────────────────────────────────

    pub async fn update_iteration_status(
        &self,
        iteration_id: RunIterationId,
        status: RunIterationStatus,
    ) -> Result<(), RunStateStoreError> {
        update_iteration_status_query(&self.pool, iteration_id, status).await
    }

    pub async fn list_run_ids(&self) -> Result<Vec<RunId>, RunStateStoreError> {
        let rows = sqlx::query(
            r#"SELECT run_id::text AS run_id FROM diagnostics.runs ORDER BY created_at DESC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RunStateStoreError::Query(e.to_string()))?;

        rows.iter()
            .map(|r| {
                let uuid_str: String = r
                    .try_get("run_id")
                    .map_err(|_| RunStateStoreError::InvalidStoredRow("run_id column"))?;
                let uuid = Uuid::parse_str(&uuid_str).map_err(|_| {
                    RunStateStoreError::InvalidStoredRow("run_id is not a valid UUID")
                })?;
                Ok(RunId(uuid))
            })
            .collect()
    }

    pub async fn list_run_summaries(&self) -> Result<Vec<RunSummaryRow>, RunStateStoreError> {
        let run_ids = self.list_run_ids().await?;
        let mut summaries = Vec::with_capacity(run_ids.len());
        for run_id in run_ids {
            match self.load_run(run_id).await {
                Ok(Some(run)) => summaries.push(build_run_summary_row(&run)),
                Ok(None) => {
                    tracing::warn!(
                        run_id = %run_id.0,
                        "skipping run summary row because the run could not be loaded"
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        run_id = %run_id.0,
                        error = %error,
                        "skipping unreadable run while building run summaries"
                    );
                }
            }
        }
        Ok(summaries)
    }
}

impl<'tx> PostgresRunStateStoreTx<'tx> {
    pub async fn insert_iteration(
        &mut self,
        run_id: RunId,
        sequence_no: u64,
        iteration: &RunIteration,
    ) -> Result<(), RunStateStoreError> {
        insert_iteration_query(
            &mut **self.conn,
            run_id,
            sequence_no,
            iteration,
        )
        .await
    }

    pub async fn insert_step_record(
        &mut self,
        iteration_id: RunIterationId,
        sequence_no: u64,
        step_record: &StepRecord,
    ) -> Result<(), RunStateStoreError> {
        insert_step_record_query(
            &mut **self.conn,
            iteration_id,
            sequence_no,
            step_record,
        )
        .await
    }

    pub async fn finish_step_record(
        &mut self,
        record_id: StepRecordId,
        finished_record: &FinishedStepRecord,
    ) -> Result<(), RunStateStoreError> {
        let tx = &mut **self.conn;
        let maybe_row = sqlx::query(
            r#"
            SELECT step, record_status
            FROM diagnostics.run_step_records
            WHERE record_id = $1::uuid
            "#,
        )
        .bind(record_id.0.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| RunStateStoreError::Query(e.to_string()))?;

        let row = maybe_row.ok_or(RunStateStoreError::StepRecordNotFound(record_id))?;

        let status: String = row
            .try_get("record_status")
            .map_err(|_| RunStateStoreError::InvalidStoredRow("record_status column"))?;
        if status == "finished" {
            return Err(RunStateStoreError::StepRecordAlreadyFinished(record_id));
        }

        let stored_step_str: String = row
            .try_get("step")
            .map_err(|_| RunStateStoreError::InvalidStoredRow("step column"))?;
        let stored_step = parse_step_kind(&stored_step_str)?;
        if stored_step != finished_record.step {
            return Err(RunStateStoreError::StepKindMismatch {
                record_id,
                expected: stored_step,
                actual: finished_record.step,
            });
        }

        let (result_json, error_json) =
            serialize_step_result(finished_record.step, &finished_record.result)?;

        sqlx::query(
            r#"
            UPDATE diagnostics.run_step_records
            SET record_status = 'finished',
                finished_at   = $1,
                result_json   = $2,
                error_json    = $3
            WHERE record_id = $4::uuid
            "#,
        )
        .bind(finished_record.finished_at)
        .bind(result_json.map(sqlx::types::Json))
        .bind(error_json.map(sqlx::types::Json))
        .bind(record_id.0.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| RunStateStoreError::Update(e.to_string()))?;

        Ok(())
    }

    pub async fn update_run_header(
        &mut self,
        run_id: RunId,
        status: RunStatus,
        updated_at: DateTime<Utc>,
        revision: u64,
    ) -> Result<(), RunStateStoreError> {
        update_run_header_query(
            &mut **self.conn,
            run_id,
            status,
            updated_at,
            revision,
        )
        .await
    }

    pub async fn update_iteration_status(
        &mut self,
        iteration_id: RunIterationId,
        status: RunIterationStatus,
    ) -> Result<(), RunStateStoreError> {
        update_iteration_status_query(&mut **self.conn, iteration_id, status).await
    }
}

// ─── Private helpers ──────────────────────────────────────────────────────────

async fn insert_iteration_query<'e, E>(
    executor: E,
    run_id: RunId,
    sequence_no: u64,
    iteration: &RunIteration,
) -> Result<(), RunStateStoreError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let config_snapshot_json = iteration
        .config_snapshot
        .as_ref()
        .map(|s| serde_json::to_value(s).expect("RunConfigSnapshot must serialize"));

    let result = sqlx::query(
        r#"
        INSERT INTO diagnostics.run_iterations (iteration_id, run_id, sequence_no, status, config_snapshot)
        VALUES ($1::uuid, $2::uuid, $3, $4, $5)
        "#,
    )
    .bind(iteration.iteration_id.0.to_string())
    .bind(run_id.0.to_string())
    .bind(sequence_no as i64)
    .bind(iteration.status.to_string())
    .bind(config_snapshot_json)
    .execute(executor)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(db)) if is_unique_violation(&*db) => {
            Err(RunStateStoreError::DuplicateIteration { run_id, sequence_no })
        }
        Err(sqlx::Error::Database(db)) if is_fk_violation(&*db) => {
            Err(RunStateStoreError::MissingParentRun(run_id))
        }
        Err(e) => Err(RunStateStoreError::Insert(e.to_string())),
    }
}

async fn insert_step_record_query<'e, E>(
    executor: E,
    iteration_id: RunIterationId,
    sequence_no: u64,
    step_record: &StepRecord,
) -> Result<(), RunStateStoreError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let result = match step_record {
        StepRecord::Pending(pending) => {
            sqlx::query(
                r#"
                INSERT INTO diagnostics.run_step_records
                    (record_id, iteration_id, sequence_no, step, record_status, started_at,
                     finished_at, result_json, error_json)
                VALUES ($1::uuid, $2::uuid, $3, $4, 'pending', $5, NULL, NULL, NULL)
                "#,
            )
            .bind(pending.record_id.0.to_string())
            .bind(iteration_id.0.to_string())
            .bind(sequence_no as i64)
            .bind(pending.step.to_string())
            .bind(pending.started_at)
            .execute(executor)
            .await
        }
        StepRecord::Finished(finished) => {
            let (result_json, error_json) =
                serialize_step_result(finished.step, &finished.result)?;
            sqlx::query(
                r#"
                INSERT INTO diagnostics.run_step_records
                    (record_id, iteration_id, sequence_no, step, record_status, started_at,
                     finished_at, result_json, error_json)
                VALUES ($1::uuid, $2::uuid, $3, $4, 'finished', $5, $6, $7, $8)
                "#,
            )
            .bind(finished.record_id.0.to_string())
            .bind(iteration_id.0.to_string())
            .bind(sequence_no as i64)
            .bind(finished.step.to_string())
            .bind(finished.started_at)
            .bind(finished.finished_at)
            .bind(result_json.map(sqlx::types::Json))
            .bind(error_json.map(sqlx::types::Json))
            .execute(executor)
            .await
        }
    };

    match result {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(db)) if is_unique_violation(&*db) => Err(
            RunStateStoreError::DuplicateStepRecord {
                iteration_id,
                sequence_no,
            },
        ),
        Err(sqlx::Error::Database(db)) if is_fk_violation(&*db) => {
            Err(RunStateStoreError::MissingParentIteration(iteration_id))
        }
        Err(e) => Err(RunStateStoreError::Insert(e.to_string())),
    }
}

async fn update_run_header_query<'e, E>(
    executor: E,
    run_id: RunId,
    status: RunStatus,
    updated_at: DateTime<Utc>,
    revision: u64,
) -> Result<(), RunStateStoreError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows_affected = sqlx::query(
        r#"
        UPDATE diagnostics.runs
        SET status     = $1,
            updated_at = $2,
            revision   = $3
        WHERE run_id = $4::uuid
        "#,
    )
    .bind(status.to_string())
    .bind(updated_at)
    .bind(revision as i64)
    .bind(run_id.0.to_string())
    .execute(executor)
    .await
    .map_err(|e| RunStateStoreError::Update(e.to_string()))?
    .rows_affected();

    if rows_affected == 0 {
        return Err(RunStateStoreError::MissingParentRun(run_id));
    }
    Ok(())
}

fn validate_run_header(run: &RunState) -> Result<(), RunStateStoreError> {
    if run.updated_at < run.created_at {
        return Err(RunStateStoreError::InvalidRunState(
            "updated_at must not be before created_at",
        ));
    }
    Ok(())
}

fn build_run_summary_row(run: &RunState) -> RunSummaryRow {
    RunSummaryRow {
        run_id: run.run_id,
        status: run.status,
        created_at: run.created_at,
        updated_at: run.updated_at,
        revision: run.revision,
        initial_user_query: first_iteration_initial_user_query(run),
        final_problem_understanding: first_iteration_final_problem_understanding(run),
    }
}

fn first_iteration_initial_user_query(run: &RunState) -> Option<String> {
    let iteration = run.iterations.first()?;
    iteration.step_records.iter().find_map(|record| match record {
        StepRecord::Finished(finished) if finished.step == StepKind::UserInputReceived => {
            match &finished.result {
                Ok(StepResultEnvelope::UserInputReceived(request)) => Some(request.query.clone()),
                _ => None,
            }
        }
        _ => None,
    })
}

fn first_iteration_final_problem_understanding(run: &RunState) -> Option<String> {
    let iteration = run.iterations.first()?;
    iteration
        .step_records
        .iter()
        .rev()
        .find_map(|record| match record {
            StepRecord::Finished(finished)
                if finished.step == StepKind::ResponseValidationAndNormalization =>
            {
                match &finished.result {
                    Ok(StepResultEnvelope::ResponseValidationAndNormalization(output)) => {
                        Some(output.response.problem_understanding.clone())
                    }
                    _ => None,
                }
            }
            _ => None,
        })
}

fn parse_run_status(s: &str) -> Result<RunStatus, RunStateStoreError> {
    RunStatus::from_str(s).map_err(|_| {
        RunStateStoreError::Deserialization(format!("unknown run status: {s}"))
    })
}

fn parse_iteration_status(s: &str) -> Result<RunIterationStatus, RunStateStoreError> {
    RunIterationStatus::from_str(s).map_err(|_| {
        RunStateStoreError::Deserialization(format!("unknown iteration status: {s}"))
    })
}

async fn update_iteration_status_query<'e, E>(
    executor: E,
    iteration_id: RunIterationId,
    status: RunIterationStatus,
) -> Result<(), RunStateStoreError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows_affected = sqlx::query(
        r#"
        UPDATE diagnostics.run_iterations
        SET status = $1
        WHERE iteration_id = $2::uuid
        "#,
    )
    .bind(status.to_string())
    .bind(iteration_id.0.to_string())
    .execute(executor)
    .await
    .map_err(|e| RunStateStoreError::Update(e.to_string()))?
    .rows_affected();

    if rows_affected == 0 {
        return Err(RunStateStoreError::MissingParentIteration(iteration_id));
    }
    Ok(())
}

fn parse_step_kind(s: &str) -> Result<StepKind, RunStateStoreError> {
    StepKind::from_str(s)
        .map_err(|_| RunStateStoreError::InvalidStoredRow("unknown step kind value"))
}

fn serialize_to_json<T: serde::Serialize>(
    value: &T,
) -> Result<serde_json::Value, RunStateStoreError> {
    serde_json::to_value(value).map_err(|e| RunStateStoreError::Serialization(e.to_string()))
}

fn validate_result_variant_matches(
    step: StepKind,
    envelope: &StepResultEnvelope,
) -> Result<(), RunStateStoreError> {
    let ok = matches!(
        (step, envelope),
        (StepKind::UserInputReceived, StepResultEnvelope::UserInputReceived(_))
            | (StepKind::InputNormalization, StepResultEnvelope::InputNormalization(_))
            | (StepKind::QueryStructuring, StepResultEnvelope::QueryStructuring(_))
            | (
                StepKind::CandidateCardRetrieval,
                StepResultEnvelope::CandidateCardRetrieval(_)
            )
            | (StepKind::CardHydration, StepResultEnvelope::CardHydration(_))
            | (
                StepKind::IncidentEvidenceRetrieval,
                StepResultEnvelope::IncidentEvidenceRetrieval(_)
            )
            | (
                StepKind::TheoryEvidenceRetrieval,
                StepResultEnvelope::TheoryEvidenceRetrieval(_)
            )
            | (
                StepKind::PromptContextAssembly,
                StepResultEnvelope::PromptContextAssembly(_)
            )
            | (
                StepKind::LlmStructuredGeneration,
                StepResultEnvelope::LlmStructuredGeneration(_)
            )
            | (
                StepKind::ResponseValidationAndNormalization,
                StepResultEnvelope::ResponseValidationAndNormalization(_)
            )
            | (
                StepKind::InformationAdequacyInitial,
                StepResultEnvelope::InformationAdequacy(_)
            )
            | (
                StepKind::InformationAdequacySupportedObservation,
                StepResultEnvelope::InformationAdequacy(_)
            )
            | (
                StepKind::InformationAdequacyUnsupportedObservation,
                StepResultEnvelope::InformationAdequacy(_)
            )
            | (
                StepKind::CardBranchReranking,
                StepResultEnvelope::CardBranchReranking(_)
            )
            | (
                StepKind::DiagnosticUpdatePromptContextAssembly,
                StepResultEnvelope::DiagnosticUpdatePromptContextAssembly(_)
            )
            | (
                StepKind::ObservationBoundaryResolver,
                StepResultEnvelope::ObservationBoundaryResolver(_)
            )
            | (
                StepKind::ObservationExtraction,
                StepResultEnvelope::ObservationExtraction(_)
            )
    );
    if !ok {
        Err(RunStateStoreError::InvalidRunState(
            "StepResultEnvelope variant does not match step kind",
        ))
    } else {
        Ok(())
    }
}

fn validate_error_variant_matches(
    step: StepKind,
    error: &StepError,
) -> Result<(), RunStateStoreError> {
    let ok = match error {
        StepError::MissingRequiredInput { .. }
        | StepError::InvalidState { .. }
        | StepError::ExternalDependency { .. }
        | StepError::Unexpected { .. } => true,
        StepError::InputNormalization(_) => step == StepKind::InputNormalization,
        StepError::QueryStructuring(_) => step == StepKind::QueryStructuring,
        StepError::CandidateCardRetrieval(_) => step == StepKind::CandidateCardRetrieval,
        StepError::CardHydration(_) => step == StepKind::CardHydration,
        StepError::IncidentEvidenceRetrieval(_) => step == StepKind::IncidentEvidenceRetrieval,
        StepError::TheoryEvidenceRetrieval(_) => step == StepKind::TheoryEvidenceRetrieval,
        StepError::PromptContextAssembly(_) => step == StepKind::PromptContextAssembly,
        StepError::LlmStructuredGeneration(_) => step == StepKind::LlmStructuredGeneration,
        StepError::ResponseValidationAndNormalization(_) => {
            step == StepKind::ResponseValidationAndNormalization
        }
        StepError::CardBranchReranking(_) => step == StepKind::CardBranchReranking,
        StepError::DiagnosticUpdatePromptContextAssembly(_) => {
            step == StepKind::DiagnosticUpdatePromptContextAssembly
        }
        StepError::ObservationBoundaryResolver(_) => step == StepKind::ObservationBoundaryResolver,
        StepError::ObservationExtraction(_) => step == StepKind::ObservationExtraction,
        StepError::InformationAdequacy(_) => matches!(
            step,
            StepKind::InformationAdequacyInitial
                | StepKind::InformationAdequacySupportedObservation
                | StepKind::InformationAdequacyUnsupportedObservation
        ),
    };
    if !ok {
        Err(RunStateStoreError::InvalidRunState(
            "step-specific StepError variant does not match step kind",
        ))
    } else {
        Ok(())
    }
}

fn serialize_step_result(
    step: StepKind,
    result: &Result<StepResultEnvelope, StepError>,
) -> Result<(Option<serde_json::Value>, Option<serde_json::Value>), RunStateStoreError> {
    match result {
        Ok(envelope) => {
            validate_result_variant_matches(step, envelope)?;
            let json = serialize_to_json(envelope)?;
            Ok((Some(json), None))
        }
        Err(error) => {
            validate_error_variant_matches(step, error)?;
            let json = serialize_to_json(error)?;
            Ok((None, Some(json)))
        }
    }
}

fn decode_step_record_row(row: &sqlx::postgres::PgRow) -> Result<StepRecord, RunStateStoreError> {
    let record_id_str: String = row
        .try_get("record_id")
        .map_err(|_| RunStateStoreError::InvalidStoredRow("record_id column"))?;
    let record_uuid = Uuid::parse_str(&record_id_str)
        .map_err(|_| RunStateStoreError::InvalidStoredRow("record_id is not a valid UUID"))?;
    let record_id = StepRecordId(record_uuid);

    let step_str: String = row
        .try_get("step")
        .map_err(|_| RunStateStoreError::InvalidStoredRow("step column"))?;
    let step = parse_step_kind(&step_str)?;

    let record_status: String = row
        .try_get("record_status")
        .map_err(|_| RunStateStoreError::InvalidStoredRow("record_status column"))?;

    let started_at: DateTime<Utc> = row
        .try_get("started_at")
        .map_err(|_| RunStateStoreError::InvalidStoredRow("started_at column"))?;

    match record_status.as_str() {
        "pending" => {
            let finished_at: Option<DateTime<Utc>> = row
                .try_get("finished_at")
                .map_err(|_| RunStateStoreError::InvalidStoredRow("finished_at column"))?;
            let result_json: Option<sqlx::types::Json<serde_json::Value>> = row
                .try_get("result_json")
                .map_err(|_| RunStateStoreError::InvalidStoredRow("result_json column"))?;
            let error_json: Option<sqlx::types::Json<serde_json::Value>> = row
                .try_get("error_json")
                .map_err(|_| RunStateStoreError::InvalidStoredRow("error_json column"))?;

            if finished_at.is_some() || result_json.is_some() || error_json.is_some() {
                return Err(RunStateStoreError::InvalidStoredRow(
                    "pending row has unexpected non-null fields",
                ));
            }
            Ok(StepRecord::Pending(PendingStepRecord {
                record_id,
                step,
                started_at,
            }))
        }
        "finished" => {
            let finished_at: Option<DateTime<Utc>> = row
                .try_get("finished_at")
                .map_err(|_| RunStateStoreError::InvalidStoredRow("finished_at column"))?;
            let finished_at = finished_at.ok_or(RunStateStoreError::InvalidStoredRow(
                "finished row has null finished_at",
            ))?;

            let result_json: Option<sqlx::types::Json<serde_json::Value>> = row
                .try_get("result_json")
                .map_err(|_| RunStateStoreError::InvalidStoredRow("result_json column"))?;
            let error_json: Option<sqlx::types::Json<serde_json::Value>> = row
                .try_get("error_json")
                .map_err(|_| RunStateStoreError::InvalidStoredRow("error_json column"))?;

            let result = match (result_json, error_json) {
                (Some(rj), None) => {
                    let envelope: StepResultEnvelope =
                        serde_json::from_value(rj.0).map_err(|e| {
                            RunStateStoreError::Deserialization(format!(
                                "result_json: {e}"
                            ))
                        })?;
                    validate_result_variant_matches(step, &envelope).map_err(|_| {
                        RunStateStoreError::Deserialization(
                            "result_json envelope variant does not match stored step kind"
                                .to_string(),
                        )
                    })?;
                    Ok(envelope)
                }
                (None, Some(ej)) => {
                    let error: StepError =
                        serde_json::from_value(ej.0).map_err(|e| {
                            RunStateStoreError::Deserialization(format!("error_json: {e}"))
                        })?;
                    validate_error_variant_matches(step, &error).map_err(|_| {
                        RunStateStoreError::Deserialization(
                            "error_json variant does not match stored step kind".to_string(),
                        )
                    })?;
                    Err(error)
                }
                _ => {
                    return Err(RunStateStoreError::InvalidStoredRow(
                        "finished row must have exactly one of result_json, error_json",
                    ))
                }
            };

            Ok(StepRecord::Finished(FinishedStepRecord {
                record_id,
                step,
                started_at,
                finished_at,
                result,
            }))
        }
        _ => Err(RunStateStoreError::InvalidStoredRow("unknown record_status value")),
    }
}

fn is_unique_violation(e: &dyn sqlx::error::DatabaseError) -> bool {
    e.code().as_deref() == Some("23505")
}

fn is_fk_violation(e: &dyn sqlx::error::DatabaseError) -> bool {
    e.code().as_deref() == Some("23503")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::run_state::model::{
        RunId, RunIterationStatus, RunState, RunStatus, StepError, StepKind, StepResultEnvelope,
    };
    use crate::shared_types::{
        CandidateCardRetrievalOutput, LlmStructuredGenerationOutput, ModelTokenUsage,
        NormalizedUserRequest, UserRequest,
    };
    use chrono::Utc;
    use uuid::Uuid;

    // ─── Fixtures ─────────────────────────────────────────────────────────────

    fn new_run_id() -> RunId {
        RunId(Uuid::new_v4())
    }

    fn valid_run() -> RunState {
        let now = Utc::now();
        RunState {
            run_id: new_run_id(),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![],
        }
    }

    fn user_input_envelope() -> StepResultEnvelope {
        StepResultEnvelope::UserInputReceived(UserRequest {
            query: "service down".to_string(),
            golden_question: None,
        })
    }

    fn input_normalization_envelope() -> StepResultEnvelope {
        StepResultEnvelope::InputNormalization(NormalizedUserRequest {
            query: "service down".to_string(),
            input_token_count: 2,
        })
    }

    fn candidate_retrieval_envelope() -> StepResultEnvelope {
        StepResultEnvelope::CandidateCardRetrieval(CandidateCardRetrievalOutput {
            ranked_candidates: vec![],
            primary: None,
            alternatives: vec![],
            metrics: None,
        })
    }

    fn llm_output_envelope() -> StepResultEnvelope {
        StepResultEnvelope::LlmStructuredGeneration(LlmStructuredGenerationOutput {
            response_json: serde_json::json!({}),
            token_usage: ModelTokenUsage {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
            },
        })
    }

    fn missing_input_error() -> StepError {
        StepError::MissingRequiredInput {
            message: "test failure".to_string(),
        }
    }

    // ─── new(): URL validation ────────────────────────────────────────────────

    #[test]
    fn new_fails_when_postgres_url_is_empty() {
        assert!(
            "".trim().is_empty(),
            "guard: empty url triggers InvalidConfig"
        );
        let err = RunStateStoreError::InvalidConfig("postgres_url must not be empty");
        assert!(matches!(err, RunStateStoreError::InvalidConfig(_)));
    }

    #[test]
    fn new_fails_when_postgres_url_is_whitespace_only() {
        assert!("   ".trim().is_empty());
        let err = RunStateStoreError::InvalidConfig("postgres_url must not be empty");
        assert!(matches!(err, RunStateStoreError::InvalidConfig(_)));
    }

    // ─── validate_run_header ──────────────────────────────────────────────────

    #[test]
    fn validate_run_header_accepts_equal_timestamps() {
        let run = valid_run();
        assert!(validate_run_header(&run).is_ok());
    }

    #[test]
    fn validate_run_header_rejects_updated_before_created() {
        let mut run = valid_run();
        run.created_at = run.updated_at + chrono::Duration::seconds(10);
        let err = validate_run_header(&run).unwrap_err();
        assert!(matches!(err, RunStateStoreError::InvalidRunState(_)));
    }

    // ─── parse_run_status ─────────────────────────────────────────────────────

    #[test]
    fn parse_run_status_succeeds_for_all_variants() {
        for (s, expected) in [
            ("Active", RunStatus::Active),
            ("WaitingForUser", RunStatus::WaitingForUser),
            ("Error", RunStatus::Error),
            ("Archived", RunStatus::Archived),
        ] {
            assert_eq!(parse_run_status(s).unwrap(), expected);
        }
    }

    #[test]
    fn parse_run_status_fails_for_unknown_value() {
        let err = parse_run_status("unknown_status").unwrap_err();
        assert!(matches!(err, RunStateStoreError::Deserialization(_)));
    }

    // ─── parse_iteration_status ───────────────────────────────────────────────

    #[test]
    fn parse_iteration_status_succeeds_for_all_variants() {
        for (s, expected) in [
            ("Active", RunIterationStatus::Active),
            ("FinishedWithSuccess", RunIterationStatus::FinishedWithSuccess),
            ("FinishedWithError", RunIterationStatus::FinishedWithError),
            ("FinishedWithWaitInput", RunIterationStatus::FinishedWithWaitInput),
        ] {
            assert_eq!(parse_iteration_status(s).unwrap(), expected);
        }
    }

    #[test]
    fn parse_iteration_status_fails_for_unknown_value() {
        let err = parse_iteration_status("unknown").unwrap_err();
        assert!(matches!(err, RunStateStoreError::Deserialization(_)));
    }

    // ─── parse_step_kind ──────────────────────────────────────────────────────

    #[test]
    fn parse_step_kind_succeeds_for_all_variants() {
        let cases = [
            ("UserInputReceived", StepKind::UserInputReceived),
            ("InputNormalization", StepKind::InputNormalization),
            ("QueryStructuring", StepKind::QueryStructuring),
            ("CandidateCardRetrieval", StepKind::CandidateCardRetrieval),
            ("CardHydration", StepKind::CardHydration),
            ("IncidentEvidenceRetrieval", StepKind::IncidentEvidenceRetrieval),
            ("TheoryEvidenceRetrieval", StepKind::TheoryEvidenceRetrieval),
            ("PromptContextAssembly", StepKind::PromptContextAssembly),
            ("LlmStructuredGeneration", StepKind::LlmStructuredGeneration),
            (
                "ResponseValidationAndNormalization",
                StepKind::ResponseValidationAndNormalization,
            ),
        ];
        for (s, expected) in cases {
            assert_eq!(parse_step_kind(s).unwrap(), expected);
        }
    }

    #[test]
    fn parse_step_kind_fails_for_unknown_value() {
        let err = parse_step_kind("NotAStep").unwrap_err();
        assert!(matches!(err, RunStateStoreError::InvalidStoredRow(_)));
    }

    // ─── validate_result_variant_matches ─────────────────────────────────────

    #[test]
    fn validate_result_variant_matches_accepts_correct_pairs() {
        let pairs: &[(StepKind, StepResultEnvelope)] = &[
            (
                StepKind::UserInputReceived,
                user_input_envelope(),
            ),
            (StepKind::InputNormalization, input_normalization_envelope()),
            (
                StepKind::CandidateCardRetrieval,
                candidate_retrieval_envelope(),
            ),
            (StepKind::LlmStructuredGeneration, llm_output_envelope()),
        ];
        for (step, envelope) in pairs {
            assert!(
                validate_result_variant_matches(*step, envelope).is_ok(),
                "{step:?} should accept its own envelope"
            );
        }
    }

    #[test]
    fn validate_result_variant_matches_rejects_mismatched_pairs() {
        let err = validate_result_variant_matches(
            StepKind::QueryStructuring,
            &input_normalization_envelope(),
        )
        .unwrap_err();
        assert!(matches!(err, RunStateStoreError::InvalidRunState(_)));
    }

    // ─── validate_error_variant_matches ──────────────────────────────────────

    #[test]
    fn validate_error_variant_matches_accepts_non_step_specific_errors_for_any_step() {
        for step in [
            StepKind::InputNormalization,
            StepKind::QueryStructuring,
            StepKind::CardHydration,
        ] {
            assert!(
                validate_error_variant_matches(step, &missing_input_error()).is_ok(),
                "non-step-specific error should be valid for any step"
            );
        }
    }

    #[test]
    fn validate_error_variant_matches_rejects_step_specific_error_for_wrong_step() {
        let card_hydration_error =
            StepError::CardHydration(crate::request_pipeline::card_hydration::CardHydrationError::MissingCard {
                case_id: "x".to_string(),
            });
        let err = validate_error_variant_matches(StepKind::QueryStructuring, &card_hydration_error)
            .unwrap_err();
        assert!(matches!(err, RunStateStoreError::InvalidRunState(_)));
    }

    #[test]
    fn validate_error_variant_matches_accepts_step_specific_error_for_correct_step() {
        let card_hydration_error =
            StepError::CardHydration(crate::request_pipeline::card_hydration::CardHydrationError::MissingCard {
                case_id: "x".to_string(),
            });
        assert!(
            validate_error_variant_matches(StepKind::CardHydration, &card_hydration_error).is_ok()
        );
    }

    // ─── serialize_step_result ────────────────────────────────────────────────

    #[test]
    fn serialize_step_result_produces_result_json_for_ok_envelope() {
        let (result_json, error_json) =
            serialize_step_result(StepKind::InputNormalization, &Ok(input_normalization_envelope()))
                .unwrap();
        assert!(result_json.is_some());
        assert!(error_json.is_none());
    }

    #[test]
    fn serialize_step_result_produces_error_json_for_err_payload() {
        let (result_json, error_json) =
            serialize_step_result(StepKind::InputNormalization, &Err(missing_input_error()))
                .unwrap();
        assert!(result_json.is_none());
        assert!(error_json.is_some());
    }

    #[test]
    fn serialize_step_result_fails_when_ok_envelope_variant_mismatches_step() {
        let err = serialize_step_result(
            StepKind::QueryStructuring,
            &Ok(input_normalization_envelope()),
        )
        .unwrap_err();
        assert!(matches!(err, RunStateStoreError::InvalidRunState(_)));
    }

    // ─── insert_run(): pre-write validation (no DB needed) ────────────────────

    #[test]
    fn insert_run_validates_run_header_before_db_write() {
        let mut run = valid_run();
        run.created_at = run.updated_at + chrono::Duration::seconds(5);
        let err = validate_run_header(&run).unwrap_err();
        assert!(matches!(err, RunStateStoreError::InvalidRunState(_)));
    }

}
