use distributed_diagnostics::orchestrator::run_state::model::RunState;
use uuid::Uuid;

use crate::runtime_runs::{PostgresRuntimeRunLoader, RuntimeRunLoaderError};
use crate::snapshot::{
    build_snapshot, DiagnosticEvalIterationSnapshot, SnapshotBuildError,
    SnapshotIterationSelector,
};
use crate::storage::{EvalProcessingStateRow, EvalStage, PostgresEvalStore, StorageError};

#[derive(Debug)]
pub struct PreparedJudgeSubject {
    pub processing_state: EvalProcessingStateRow,
    pub run_state: RunState,
    pub snapshot: DiagnosticEvalIterationSnapshot,
}

#[derive(Debug, thiserror::Error)]
pub enum SubjectPreparationError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    RuntimeRunLoader(#[from] RuntimeRunLoaderError),
    #[error(transparent)]
    Snapshot(#[from] SnapshotBuildError),
    #[error(
        "prepared snapshot iteration {snapshot_iteration_id:?} does not match frozen subject iteration {frozen_iteration_id:?}"
    )]
    FrozenIterationMismatch {
        frozen_iteration_id: distributed_diagnostics::orchestrator::run_state::model::RunIterationId,
        snapshot_iteration_id:
            distributed_diagnostics::orchestrator::run_state::model::RunIterationId,
    },
}

pub async fn prepare_next_subject_for_judge(
    eval_run_id: Uuid,
    eval_store: &PostgresEvalStore,
    runtime_loader: &PostgresRuntimeRunLoader,
) -> Result<Option<PreparedJudgeSubject>, SubjectPreparationError> {
    let Some(processing_state) = eval_store
        .fetch_next_subject_for_stage(eval_run_id, EvalStage::JudgeRequestSuites)
        .await?
    else {
        return Ok(None);
    };

    prepare_subject_from_processing_state(processing_state, runtime_loader)
        .await
        .map(Some)
}

pub async fn prepare_subject_from_processing_state(
    processing_state: EvalProcessingStateRow,
    runtime_loader: &PostgresRuntimeRunLoader,
) -> Result<PreparedJudgeSubject, SubjectPreparationError> {
    let run_state = runtime_loader
        .load_run_state(processing_state.key.runtime_run_id)
        .await?;
    let snapshot = build_snapshot(
        &run_state,
        SnapshotIterationSelector::ExactIteration(
            distributed_diagnostics::orchestrator::run_state::model::RunIterationId(
                processing_state.key.iteration_id,
            ),
        ),
    )?;

    if snapshot.iteration_id.0 != processing_state.key.iteration_id {
        return Err(SubjectPreparationError::FrozenIterationMismatch {
            frozen_iteration_id:
                distributed_diagnostics::orchestrator::run_state::model::RunIterationId(
                    processing_state.key.iteration_id,
                ),
            snapshot_iteration_id: snapshot.iteration_id,
        });
    }

    Ok(PreparedJudgeSubject {
        processing_state,
        run_state,
        snapshot,
    })
}
