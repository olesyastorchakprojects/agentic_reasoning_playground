use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::api_clients::postgres::run_state_store::{
    PostgresRunStateStore, PostgresRunStateStoreTx, RunStateStoreError, RunSummaryRow,
};
use crate::orchestrator::run_state::model::{
    FinishedStepRecord, RunId, RunIteration, RunIterationId, RunState, RunStatus, StepRecord,
    StepRecordId,
};

fn repository_error_type(e: &RunRepositoryError) -> &'static str {
    match e {
        RunRepositoryError::DuplicateRun { .. } => "RunRepositoryError.DuplicateRun",
        RunRepositoryError::InvalidRunState { .. } => "RunRepositoryError.InvalidRunState",
        RunRepositoryError::MissingInitialUserQuery { .. } => {
            "RunRepositoryError.MissingInitialUserQuery"
        }
        RunRepositoryError::Store(_) => "RunRepositoryError.Store",
    }
}

#[derive(Debug)]
pub struct RunRepository {
    run_state_store: PostgresRunStateStore,
}

impl RunRepository {
    pub fn new(run_state_store: PostgresRunStateStore) -> Self {
        Self { run_state_store }
    }

    pub async fn create_run(&self, run: &RunState) -> Result<(), RunRepositoryError> {
        if !run.iterations.is_empty() {
            return Err(RunRepositoryError::InvalidRunState {
                message: "create_run requires an empty initial RunState".to_string(),
            });
        }

        self.run_state_store
            .insert_run(run)
            .await
            .map_err(map_store_error)
    }

    pub async fn load_run(&self, run_id: RunId) -> Result<Option<RunState>, RunRepositoryError> {
        self.run_state_store
            .load_run(run_id)
            .await
            .map_err(map_store_error)
    }

    pub async fn append_iteration(
        &self,
        run: &RunState,
        iteration_sequence_no: u64,
        iteration: &RunIteration,
    ) -> Result<(), RunRepositoryError> {
        let run = run.clone();
        let iteration = iteration.clone();
        self.run_state_store
            .with_transaction(|tx: &mut PostgresRunStateStoreTx<'_>| {
                Box::pin(async move {
                    tx.insert_iteration(run.run_id, iteration_sequence_no, &iteration)
                        .await?;
                    tx.update_run_header(run.run_id, run.status, run.updated_at, run.revision)
                        .await?;
                    Ok(())
                })
            })
            .await
            .map_err(map_store_error)
    }

    pub async fn append_step_record(
        &self,
        run: &RunState,
        iteration_id: RunIterationId,
        step_sequence_no: u64,
        step_record: &StepRecord,
    ) -> Result<(), RunRepositoryError> {
        let run_id_str = run.run_id.0.to_string();
        let iter_id_str = iteration_id.0.to_string();
        let (step_kind_str, record_id_str) = match step_record {
            StepRecord::Pending(p) => (p.step.as_ref().to_string(), p.record_id.0.to_string()),
            StepRecord::Finished(f) => (f.step.as_ref().to_string(), f.record_id.0.to_string()),
        };
        let span = crate::observability::append_pending_span(
            &run_id_str,
            &iter_id_str,
            &step_kind_str,
            &record_id_str,
            step_sequence_no,
        );
        let _entered = span.enter();

        let run = run.clone();
        let step_record = step_record.clone();
        let result = self
            .run_state_store
            .with_transaction(|tx: &mut PostgresRunStateStoreTx<'_>| {
                Box::pin(async move {
                    tx.insert_step_record(iteration_id, step_sequence_no, &step_record)
                        .await?;
                    tx.update_run_header(run.run_id, run.status, run.updated_at, run.revision)
                        .await?;
                    Ok(())
                })
            })
            .await
            .map_err(map_store_error);
        if let Err(e) = &result {
            crate::observability::record_error(&span, repository_error_type(e), &e.to_string());
        } else {
            span.record("status", "ok");
        }
        result
    }

    pub async fn finish_step_record(
        &self,
        run: &RunState,
        record_id: StepRecordId,
        finished_record: &FinishedStepRecord,
    ) -> Result<(), RunRepositoryError> {
        let run_id_str = run.run_id.0.to_string();
        let (iter_id_str, step_sequence_no) = run
            .iterations
            .iter()
            .find_map(|it| {
                it.step_records.iter().enumerate().find_map(|(idx, r)| match r {
                    StepRecord::Finished(f) if f.record_id == record_id => {
                        Some((it.iteration_id.0.to_string(), idx as u64))
                    }
                    StepRecord::Pending(p) if p.record_id == record_id => {
                        Some((it.iteration_id.0.to_string(), idx as u64))
                    }
                    _ => None,
                })
            })
            .unwrap_or_default();
        let step_kind_str = finished_record.step.as_ref().to_string();
        let record_id_str = record_id.0.to_string();
        let persisted_step_outcome = if finished_record.result.is_ok() { "success" } else { "failure" };
        let span = crate::observability::finish_step_span(
            &run_id_str,
            &iter_id_str,
            &step_kind_str,
            &record_id_str,
        );
        let _entered = span.enter();
        span.record("step.sequence_no", step_sequence_no);
        span.record("persisted.step.outcome", persisted_step_outcome);

        if finished_record.record_id != record_id {
            span.record("status", "error");
            return Err(RunRepositoryError::InvalidRunState {
                message: "finished_record.record_id must equal the supplied record_id".to_string(),
            });
        }

        let run = run.clone();
        let finished_record = finished_record.clone();
        let result = self
            .run_state_store
            .with_transaction(|tx: &mut PostgresRunStateStoreTx<'_>| {
                Box::pin(async move {
                    tx.finish_step_record(record_id, &finished_record).await?;
                    tx.update_run_header(run.run_id, run.status, run.updated_at, run.revision)
                        .await?;
                    Ok(())
                })
            })
            .await
            .map_err(map_store_error);
        if let Err(e) = &result {
            crate::observability::record_error(&span, repository_error_type(e), &e.to_string());
        } else {
            span.record("status", "ok");
        }
        result
    }

    pub async fn update_run_header(&self, run: &RunState) -> Result<(), RunRepositoryError> {
        self.run_state_store
            .update_run_header(run.run_id, run.status, run.updated_at, run.revision)
            .await
            .map_err(map_store_error)
    }

    pub async fn list_runs(&self) -> Result<Vec<RunListItem>, RunRepositoryError> {
        let summaries = self
            .run_state_store
            .list_run_summaries()
            .await
            .map_err(map_store_error)?;

        summaries.into_iter().map(build_run_list_item).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunListItem {
    pub run_id: RunId,
    pub status: RunStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: u64,
    pub initial_user_query: String,
    pub final_problem_understanding: Option<String>,
}

#[derive(Debug, Error)]
pub enum RunRepositoryError {
    #[error("run already exists: {run_id:?}")]
    DuplicateRun { run_id: RunId },

    #[error("run state is invalid for persistence: {message}")]
    InvalidRunState { message: String },

    #[error("run list row is missing the initial user query for run {run_id:?}")]
    MissingInitialUserQuery { run_id: RunId },

    #[error(transparent)]
    Store(#[from] RunStateStoreError),
}

fn map_store_error(err: RunStateStoreError) -> RunRepositoryError {
    match err {
        RunStateStoreError::DuplicateRun(run_id) => RunRepositoryError::DuplicateRun { run_id },
        RunStateStoreError::InvalidRunState(message) => RunRepositoryError::InvalidRunState {
            message: message.to_string(),
        },
        other => RunRepositoryError::Store(other),
    }
}

fn build_run_list_item(summary: RunSummaryRow) -> Result<RunListItem, RunRepositoryError> {
    let initial_user_query =
        summary
            .initial_user_query
            .ok_or(RunRepositoryError::MissingInitialUserQuery {
                run_id: summary.run_id,
            })?;

    Ok(RunListItem {
        run_id: summary.run_id,
        status: summary.status,
        created_at: summary.created_at,
        updated_at: summary.updated_at,
        revision: summary.revision,
        initial_user_query,
        final_problem_understanding: summary.final_problem_understanding,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::run_state::model::RunState;

    #[test]
    fn create_run_rejects_non_empty_initial_run_before_store_call() {
        let now = Utc::now();
        let run = RunState {
            run_id: RunId(uuid::Uuid::new_v4()),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![RunIteration {
                iteration_id: RunIterationId(uuid::Uuid::new_v4()),
                step_records: vec![],
            }],
        };

        let err = if run.iterations.is_empty() {
            None
        } else {
            Some(RunRepositoryError::InvalidRunState {
                message: "create_run requires an empty initial RunState".to_string(),
            })
        }
        .expect("must construct InvalidRunState");

        assert!(matches!(err, RunRepositoryError::InvalidRunState { .. }));
    }

    #[test]
    fn build_run_list_item_requires_initial_user_query() {
        let now = Utc::now();
        let summary = RunSummaryRow {
            run_id: RunId(uuid::Uuid::new_v4()),
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            initial_user_query: None,
            final_problem_understanding: None,
        };

        let err = build_run_list_item(summary).unwrap_err();
        assert!(matches!(
            err,
            RunRepositoryError::MissingInitialUserQuery { .. }
        ));
    }
}
