use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

use crate::orchestrator::run_state::model::{
    FinishedStepRecord, PendingStepRecord, RunIteration, RunIterationId, RunState, RunStatus,
    StepError, StepKind, StepRecord, StepRecordId, StepResultEnvelope,
};
use crate::shared_types::UserRequest;

pub struct RunStateWriter<'a> {
    state: &'a mut RunState,
}

pub struct CurrentIterationWriter<'a> {
    state: &'a mut RunState,
    iteration_index: usize,
}

pub struct PendingStepWriter<'a> {
    state: &'a mut RunState,
    iteration_index: usize,
    step_index: usize,
}

impl std::fmt::Debug for RunStateWriter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunStateWriter").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for CurrentIterationWriter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CurrentIterationWriter")
            .field("iteration_index", &self.iteration_index)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for PendingStepWriter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingStepWriter")
            .field("iteration_index", &self.iteration_index)
            .field("step_index", &self.step_index)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum StateApplyError {
    #[error("run is archived")]
    RunArchived,

    #[error("no current iteration")]
    NoCurrentIteration,

    #[error("pending step already exists")]
    PendingStepAlreadyExists,

    #[error("pending step handle is stale")]
    StalePendingStep,

    #[error("step result does not match step kind: {step:?}")]
    StepResultKindMismatch { step: StepKind },

    #[error("step error does not match step kind: {step:?}")]
    StepErrorKindMismatch { step: StepKind },
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn has_pending_step(state: &RunState) -> bool {
    state.iterations.iter().any(|iter| {
        iter.step_records
            .iter()
            .any(|r| matches!(r, StepRecord::Pending(_)))
    })
}

fn result_matches_step(result: &StepResultEnvelope, step: StepKind) -> bool {
    matches!(
        (result, step),
        (
            StepResultEnvelope::UserInputReceived(_),
            StepKind::UserInputReceived
        ) | (
            StepResultEnvelope::InputNormalization(_),
            StepKind::InputNormalization
        ) | (
            StepResultEnvelope::QueryStructuring(_),
            StepKind::QueryStructuring
        ) | (
            StepResultEnvelope::CandidateCardRetrieval(_),
            StepKind::CandidateCardRetrieval
        ) | (
            StepResultEnvelope::CardHydration(_),
            StepKind::CardHydration
        ) | (
            StepResultEnvelope::IncidentEvidenceRetrieval(_),
            StepKind::IncidentEvidenceRetrieval
        ) | (
            StepResultEnvelope::TheoryEvidenceRetrieval(_),
            StepKind::TheoryEvidenceRetrieval
        ) | (
            StepResultEnvelope::PromptContextAssembly(_),
            StepKind::PromptContextAssembly
        ) | (
            StepResultEnvelope::LlmStructuredGeneration(_),
            StepKind::LlmStructuredGeneration
        ) | (
            StepResultEnvelope::ResponseValidationAndNormalization(_),
            StepKind::ResponseValidationAndNormalization,
        )
    )
}

fn step_specific_error_mismatch(error: &StepError, step: StepKind) -> Option<StepKind> {
    let is_step_specific = matches!(
        error,
        StepError::InputNormalization(_)
            | StepError::QueryStructuring(_)
            | StepError::CandidateCardRetrieval(_)
            | StepError::CardHydration(_)
            | StepError::IncidentEvidenceRetrieval(_)
            | StepError::TheoryEvidenceRetrieval(_)
            | StepError::PromptContextAssembly(_)
            | StepError::LlmStructuredGeneration(_)
            | StepError::ResponseValidationAndNormalization(_)
    );
    if !is_step_specific {
        return None;
    }
    let matches_step = matches!(
        (error, step),
        (
            StepError::InputNormalization(_),
            StepKind::InputNormalization
        ) | (StepError::QueryStructuring(_), StepKind::QueryStructuring)
            | (
                StepError::CandidateCardRetrieval(_),
                StepKind::CandidateCardRetrieval
            )
            | (StepError::CardHydration(_), StepKind::CardHydration)
            | (
                StepError::IncidentEvidenceRetrieval(_),
                StepKind::IncidentEvidenceRetrieval
            )
            | (
                StepError::TheoryEvidenceRetrieval(_),
                StepKind::TheoryEvidenceRetrieval
            )
            | (
                StepError::PromptContextAssembly(_),
                StepKind::PromptContextAssembly
            )
            | (
                StepError::LlmStructuredGeneration(_),
                StepKind::LlmStructuredGeneration
            )
            | (
                StepError::ResponseValidationAndNormalization(_),
                StepKind::ResponseValidationAndNormalization,
            )
    );
    if matches_step {
        None
    } else {
        Some(step)
    }
}

fn apply_bookkeeping(state: &mut RunState) {
    state.updated_at = Utc::now();
    state.revision += 1;
}

// ── RunStateWriter ────────────────────────────────────────────────────────────

impl<'a> RunStateWriter<'a> {
    pub fn new(state: &'a mut RunState) -> Self {
        Self { state }
    }

    pub fn begin_iteration(
        &mut self,
        user_input: UserRequest,
    ) -> Result<CurrentIterationWriter<'_>, StateApplyError> {
        if self.state.status == RunStatus::Archived {
            return Err(StateApplyError::RunArchived);
        }
        if has_pending_step(self.state) {
            return Err(StateApplyError::PendingStepAlreadyExists);
        }
        let iteration_id = RunIterationId(Uuid::new_v4());
        let record_id = StepRecordId(Uuid::new_v4());
        let now = Utc::now();
        let step_record = StepRecord::Finished(FinishedStepRecord {
            record_id,
            step: StepKind::UserInputReceived,
            started_at: now,
            finished_at: now,
            result: Ok(StepResultEnvelope::UserInputReceived(user_input)),
        });
        self.state.iterations.push(RunIteration {
            iteration_id,
            config_snapshot: None,
            step_records: vec![step_record],
        });
        self.state.status = RunStatus::Active;
        apply_bookkeeping(self.state);
        let iteration_index = self.state.iterations.len() - 1;
        Ok(CurrentIterationWriter {
            state: self.state,
            iteration_index,
        })
    }

    pub fn current_iteration(&mut self) -> Result<CurrentIterationWriter<'_>, StateApplyError> {
        if self.state.status == RunStatus::Archived {
            return Err(StateApplyError::RunArchived);
        }
        if self.state.iterations.is_empty() {
            return Err(StateApplyError::NoCurrentIteration);
        }
        let iteration_index = self.state.iterations.len() - 1;
        Ok(CurrentIterationWriter {
            state: self.state,
            iteration_index,
        })
    }

    pub fn wait_for_user(&mut self) -> Result<(), StateApplyError> {
        if self.state.status == RunStatus::Archived {
            return Err(StateApplyError::RunArchived);
        }
        if has_pending_step(self.state) {
            return Err(StateApplyError::PendingStepAlreadyExists);
        }
        self.state.status = RunStatus::WaitingForUser;
        apply_bookkeeping(self.state);
        Ok(())
    }

    pub fn archive_run(&mut self) -> Result<(), StateApplyError> {
        if self.state.status == RunStatus::Archived {
            return Ok(());
        }
        self.state.status = RunStatus::Archived;
        apply_bookkeeping(self.state);
        Ok(())
    }
}

// ── CurrentIterationWriter ────────────────────────────────────────────────────

impl<'a> CurrentIterationWriter<'a> {
    pub fn iteration_id(&self) -> RunIterationId {
        self.state.iterations[self.iteration_index].iteration_id
    }

    pub fn begin_step(&mut self, step: StepKind) -> Result<PendingStepWriter<'_>, StateApplyError> {
        if self.state.status == RunStatus::Archived {
            return Err(StateApplyError::RunArchived);
        }
        if has_pending_step(self.state) {
            return Err(StateApplyError::PendingStepAlreadyExists);
        }
        let record_id = StepRecordId(Uuid::new_v4());
        let pending = PendingStepRecord {
            record_id,
            step,
            started_at: Utc::now(),
        };
        let iteration = &mut self.state.iterations[self.iteration_index];
        iteration.step_records.push(StepRecord::Pending(pending));
        let step_index = iteration.step_records.len() - 1;
        self.state.status = RunStatus::Active;
        apply_bookkeeping(self.state);
        Ok(PendingStepWriter {
            state: self.state,
            iteration_index: self.iteration_index,
            step_index,
        })
    }

    pub fn pending_step(&mut self) -> Option<PendingStepWriter<'_>> {
        let iteration = &self.state.iterations[self.iteration_index];
        let step_index = iteration
            .step_records
            .iter()
            .position(|r| matches!(r, StepRecord::Pending(_)))?;
        Some(PendingStepWriter {
            state: self.state,
            iteration_index: self.iteration_index,
            step_index,
        })
    }
}

// ── PendingStepWriter ─────────────────────────────────────────────────────────

impl<'a> PendingStepWriter<'a> {
    pub fn record_id(&self) -> StepRecordId {
        match &self.state.iterations[self.iteration_index].step_records[self.step_index] {
            StepRecord::Pending(r) => r.record_id,
            StepRecord::Finished(_) => {
                panic!("PendingStepWriter indices point to a finished step")
            }
        }
    }

    pub fn kind(&self) -> StepKind {
        match &self.state.iterations[self.iteration_index].step_records[self.step_index] {
            StepRecord::Pending(r) => r.step,
            StepRecord::Finished(_) => {
                panic!("PendingStepWriter indices point to a finished step")
            }
        }
    }

    pub fn record_success(self, result: StepResultEnvelope) -> Result<(), StateApplyError> {
        let record =
            match &self.state.iterations[self.iteration_index].step_records[self.step_index] {
                StepRecord::Pending(r) => r.clone(),
                StepRecord::Finished(_) => return Err(StateApplyError::StalePendingStep),
            };
        if !result_matches_step(&result, record.step) {
            return Err(StateApplyError::StepResultKindMismatch { step: record.step });
        }
        let finished = FinishedStepRecord {
            record_id: record.record_id,
            step: record.step,
            started_at: record.started_at,
            finished_at: finished_at_not_before_started(record.started_at),
            result: Ok(result),
        };
        self.state.iterations[self.iteration_index].step_records[self.step_index] =
            StepRecord::Finished(finished);
        self.state.status = RunStatus::Active;
        apply_bookkeeping(self.state);
        Ok(())
    }

    pub fn record_failure(self, error: StepError) -> Result<(), StateApplyError> {
        let record =
            match &self.state.iterations[self.iteration_index].step_records[self.step_index] {
                StepRecord::Pending(r) => r.clone(),
                StepRecord::Finished(_) => return Err(StateApplyError::StalePendingStep),
            };
        if let Some(mismatch_step) = step_specific_error_mismatch(&error, record.step) {
            return Err(StateApplyError::StepErrorKindMismatch {
                step: mismatch_step,
            });
        }
        let finished = FinishedStepRecord {
            record_id: record.record_id,
            step: record.step,
            started_at: record.started_at,
            finished_at: finished_at_not_before_started(record.started_at),
            result: Err(error),
        };
        self.state.iterations[self.iteration_index].step_records[self.step_index] =
            StepRecord::Finished(finished);
        self.state.status = RunStatus::Error;
        apply_bookkeeping(self.state);
        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use crate::orchestrator::run_state::apply::{RunStateWriter, StateApplyError};
    use crate::orchestrator::run_state::model::{
        FinishedStepRecord, PendingStepRecord, RunId, RunIteration, RunIterationId, RunState,
        RunStatus, StepError, StepKind, StepRecord, StepRecordId, StepResultEnvelope,
    };
    use crate::request_pipeline::input_normalization::InputNormalizationError;
    use crate::request_pipeline::query_structuring::QueryStructuringError;
    use crate::shared_types::UserRequest;

    // ─── Helpers ──────────────────────────────────────────────────────────────

    fn user_req() -> UserRequest {
        UserRequest {
            query: "hello".to_string(),
            golden_question: None,
        }
    }

    fn fresh_state() -> RunState {
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

    fn pending_record(kind: StepKind) -> StepRecord {
        StepRecord::Pending(PendingStepRecord {
            record_id: StepRecordId(Uuid::new_v4()),
            step: kind,
            started_at: Utc::now(),
        })
    }

    fn finished_ok_record(kind: StepKind, result: StepResultEnvelope) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: StepRecordId(Uuid::new_v4()),
            step: kind,
            started_at: now,
            finished_at: now,
            result: Ok(result),
        })
    }

    fn user_input_result() -> StepResultEnvelope {
        StepResultEnvelope::UserInputReceived(user_req())
    }

    fn state_with_pending_step() -> RunState {
        let mut state = fresh_state();
        let iid = RunIterationId(Uuid::new_v4());
        state.iterations.push(RunIteration {
            iteration_id: iid,
            config_snapshot: None,
            step_records: vec![
                finished_ok_record(StepKind::UserInputReceived, user_input_result()),
                pending_record(StepKind::InputNormalization),
            ],
        });
        state
    }

    fn archived_state() -> RunState {
        let mut state = fresh_state();
        state.status = RunStatus::Archived;
        state
    }

    // ─── RunStateWriter::new ──────────────────────────────────────────────────

    #[test]
    fn run_state_writer_new_wraps_mutable_state() {
        let mut state = fresh_state();
        let rid = state.run_id;
        let writer = RunStateWriter::new(&mut state);
        drop(writer);
        assert_eq!(state.run_id, rid);
    }

    // ─── begin_iteration ─────────────────────────────────────────────────────

    #[test]
    fn begin_iteration_appends_a_new_iteration() {
        let mut state = fresh_state();
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.begin_iteration(user_req()).unwrap();
        }
        assert_eq!(state.iterations.len(), 1);
    }

    #[test]
    fn begin_iteration_returns_pending_step_already_exists_when_pending_step_present() {
        let mut state = state_with_pending_step();
        let mut writer = RunStateWriter::new(&mut state);
        let err = writer.begin_iteration(user_req()).unwrap_err();
        assert!(matches!(err, StateApplyError::PendingStepAlreadyExists));
    }

    #[test]
    fn new_iteration_contains_exactly_one_finished_user_input_received_record() {
        let mut state = fresh_state();
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.begin_iteration(user_req()).unwrap();
        }
        let records = &state.iterations[0].step_records;
        assert_eq!(records.len(), 1);
        match &records[0] {
            StepRecord::Finished(f) => {
                assert_eq!(f.step, StepKind::UserInputReceived);
                assert!(matches!(
                    f.result,
                    Ok(StepResultEnvelope::UserInputReceived(_))
                ));
            }
            _ => panic!("expected a finished step"),
        }
    }

    #[test]
    fn begin_iteration_returns_current_iteration_writer_for_new_iteration() {
        let mut state = fresh_state();
        let mut writer = RunStateWriter::new(&mut state);
        let cw = writer.begin_iteration(user_req()).unwrap();
        // Verify the writer refers to the new iteration by checking its id matches state
        let iid = cw.iteration_id();
        drop(cw);
        drop(writer);
        assert_eq!(state.iterations[0].iteration_id, iid);
    }

    // ─── Bookkeeping: updated_at and revision ─────────────────────────────────

    #[test]
    fn begin_iteration_increments_revision_and_updates_updated_at() {
        let mut state = fresh_state();
        let before_revision = state.revision;
        let before_updated = state.updated_at;
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.begin_iteration(user_req()).unwrap();
        }
        assert_eq!(state.revision, before_revision + 1);
        assert!(state.updated_at >= before_updated);
    }

    // ─── current_iteration ────────────────────────────────────────────────────

    #[test]
    fn current_iteration_returns_no_current_iteration_when_empty() {
        let mut state = fresh_state();
        let mut writer = RunStateWriter::new(&mut state);
        let err = writer.current_iteration().unwrap_err();
        assert!(matches!(err, StateApplyError::NoCurrentIteration));
    }

    #[test]
    fn current_iteration_does_not_update_bookkeeping() {
        let mut state = fresh_state();
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.begin_iteration(user_req()).unwrap();
        }
        let revision_before = state.revision;
        let updated_before = state.updated_at;
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.current_iteration().unwrap();
        }
        assert_eq!(state.revision, revision_before);
        assert_eq!(state.updated_at, updated_before);
    }

    // ─── begin_step ───────────────────────────────────────────────────────────

    #[test]
    fn begin_step_appends_one_pending_step_to_current_iteration() {
        let mut state = fresh_state();
        {
            let mut writer = RunStateWriter::new(&mut state);
            let mut cw = writer.begin_iteration(user_req()).unwrap();
            cw.begin_step(StepKind::InputNormalization).unwrap();
        }
        let records = &state.iterations[0].step_records;
        assert_eq!(records.len(), 2);
        assert!(matches!(records[1], StepRecord::Pending(_)));
    }

    #[test]
    fn begin_step_returns_pending_step_already_exists_when_pending_present() {
        let mut state = state_with_pending_step();
        let mut writer = RunStateWriter::new(&mut state);
        let mut cw = writer.current_iteration().unwrap();
        let err = cw.begin_step(StepKind::QueryStructuring).unwrap_err();
        assert!(matches!(err, StateApplyError::PendingStepAlreadyExists));
    }

    #[test]
    fn begin_step_increments_revision() {
        let mut state = fresh_state();
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.begin_iteration(user_req()).unwrap();
        }
        let revision_before = state.revision;
        {
            let mut writer = RunStateWriter::new(&mut state);
            let mut cw = writer.current_iteration().unwrap();
            cw.begin_step(StepKind::InputNormalization).unwrap();
        }
        assert_eq!(state.revision, revision_before + 1);
    }

    // ─── pending_step ─────────────────────────────────────────────────────────

    #[test]
    fn pending_step_returns_the_pending_step_when_present() {
        let mut state = state_with_pending_step();
        let mut writer = RunStateWriter::new(&mut state);
        let mut cw = writer.current_iteration().unwrap();
        let pw = cw.pending_step();
        assert!(pw.is_some());
        assert_eq!(pw.unwrap().kind(), StepKind::InputNormalization);
    }

    #[test]
    fn pending_step_returns_none_when_no_pending_step() {
        let mut state = fresh_state();
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.begin_iteration(user_req()).unwrap();
        }
        let mut writer = RunStateWriter::new(&mut state);
        let mut cw = writer.current_iteration().unwrap();
        assert!(cw.pending_step().is_none());
    }

    // ─── record_success ───────────────────────────────────────────────────────

    #[test]
    fn record_success_replaces_pending_with_finished_ok() {
        let mut state = state_with_pending_step();
        {
            let mut writer = RunStateWriter::new(&mut state);
            let mut cw = writer.current_iteration().unwrap();
            let pw = cw.pending_step().unwrap();
            pw.record_success(StepResultEnvelope::InputNormalization(
                crate::shared_types::NormalizedUserRequest {
                    query: "normalized".to_string(),
                    input_token_count: 5,
                },
            ))
            .unwrap();
        }
        let records = &state.iterations[0].step_records;
        let last = records.last().unwrap();
        assert!(matches!(last, StepRecord::Finished(_)));
        if let StepRecord::Finished(f) = last {
            assert!(matches!(
                f.result,
                Ok(StepResultEnvelope::InputNormalization(_))
            ));
        }
    }

    #[test]
    fn record_success_rejects_mismatched_result_variant() {
        let mut state = state_with_pending_step();
        let mut writer = RunStateWriter::new(&mut state);
        let mut cw = writer.current_iteration().unwrap();
        let pw = cw.pending_step().unwrap();
        // pending step is InputNormalization; pass UserInputReceived — wrong kind
        let err = pw.record_success(user_input_result()).unwrap_err();
        assert!(matches!(
            err,
            StateApplyError::StepResultKindMismatch { .. }
        ));
    }

    #[test]
    fn record_success_sets_status_active() {
        let mut state = state_with_pending_step();
        state.status = RunStatus::Error;
        {
            let mut writer = RunStateWriter::new(&mut state);
            let mut cw = writer.current_iteration().unwrap();
            let pw = cw.pending_step().unwrap();
            pw.record_success(StepResultEnvelope::InputNormalization(
                crate::shared_types::NormalizedUserRequest {
                    query: "q".to_string(),
                    input_token_count: 1,
                },
            ))
            .unwrap();
        }
        assert_eq!(state.status, RunStatus::Active);
    }

    // ─── record_failure ───────────────────────────────────────────────────────

    #[test]
    fn record_failure_replaces_pending_with_finished_err() {
        let mut state = state_with_pending_step();
        let error = StepError::InputNormalization(InputNormalizationError::EmptyQuery);
        {
            let mut writer = RunStateWriter::new(&mut state);
            let mut cw = writer.current_iteration().unwrap();
            let pw = cw.pending_step().unwrap();
            pw.record_failure(error).unwrap();
        }
        let records = &state.iterations[0].step_records;
        let last = records.last().unwrap();
        assert!(matches!(
            last,
            StepRecord::Finished(f)
            if matches!(f.result, Err(StepError::InputNormalization(_)))
        ));
    }

    #[test]
    fn record_failure_rejects_step_specific_error_mismatch() {
        let mut state = state_with_pending_step();
        let mut writer = RunStateWriter::new(&mut state);
        let mut cw = writer.current_iteration().unwrap();
        let pw = cw.pending_step().unwrap();
        // pending step is InputNormalization; pass QueryStructuring error — mismatch
        let mismatched_error =
            StepError::QueryStructuring(QueryStructuringError::InvalidConfig("stub".to_string()));
        let err = pw.record_failure(mismatched_error).unwrap_err();
        assert!(matches!(err, StateApplyError::StepErrorKindMismatch { .. }));
    }

    #[test]
    fn record_failure_accepts_generic_step_error_regardless_of_step_kind() {
        let mut state = state_with_pending_step();
        let error = StepError::MissingRequiredInput {
            message: "missing".to_string(),
        };
        {
            let mut writer = RunStateWriter::new(&mut state);
            let mut cw = writer.current_iteration().unwrap();
            let pw = cw.pending_step().unwrap();
            pw.record_failure(error).unwrap();
        }
        let records = &state.iterations[0].step_records;
        assert!(matches!(
            records.last().unwrap(),
            StepRecord::Finished(f)
            if matches!(f.result, Err(StepError::MissingRequiredInput { .. }))
        ));
    }

    #[test]
    fn record_failure_sets_status_error() {
        let mut state = state_with_pending_step();
        {
            let mut writer = RunStateWriter::new(&mut state);
            let mut cw = writer.current_iteration().unwrap();
            let pw = cw.pending_step().unwrap();
            pw.record_failure(StepError::InputNormalization(
                InputNormalizationError::EmptyQuery,
            ))
            .unwrap();
        }
        assert_eq!(state.status, RunStatus::Error);
    }

    // ─── wait_for_user ────────────────────────────────────────────────────────

    #[test]
    fn wait_for_user_sets_status_waiting_for_user() {
        let mut state = fresh_state();
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.begin_iteration(user_req()).unwrap();
        }
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.wait_for_user().unwrap();
        }
        assert_eq!(state.status, RunStatus::WaitingForUser);
    }

    #[test]
    fn wait_for_user_returns_pending_step_already_exists_when_pending_present() {
        let mut state = state_with_pending_step();
        let mut writer = RunStateWriter::new(&mut state);
        let err = writer.wait_for_user().unwrap_err();
        assert!(matches!(err, StateApplyError::PendingStepAlreadyExists));
    }

    #[test]
    fn wait_for_user_increments_revision() {
        let mut state = fresh_state();
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.begin_iteration(user_req()).unwrap();
        }
        let rev_before = state.revision;
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.wait_for_user().unwrap();
        }
        assert_eq!(state.revision, rev_before + 1);
    }

    // ─── archive_run ──────────────────────────────────────────────────────────

    #[test]
    fn archive_run_sets_status_archived() {
        let mut state = fresh_state();
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.archive_run().unwrap();
        }
        assert_eq!(state.status, RunStatus::Archived);
    }

    #[test]
    fn archive_run_succeeds_even_when_pending_step_exists() {
        let mut state = state_with_pending_step();
        let mut writer = RunStateWriter::new(&mut state);
        assert!(writer.archive_run().is_ok());
        assert_eq!(state.status, RunStatus::Archived);
    }

    #[test]
    fn archive_run_is_no_op_when_already_archived() {
        let mut state = archived_state();
        let rev_before = state.revision;
        let updated_before = state.updated_at;
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.archive_run().unwrap();
        }
        assert_eq!(state.revision, rev_before);
        assert_eq!(state.updated_at, updated_before);
    }

    // ─── RunArchived guard on mutating methods ─────────────────────────────────

    #[test]
    fn begin_iteration_returns_run_archived_when_archived() {
        let mut state = archived_state();
        let mut writer = RunStateWriter::new(&mut state);
        let err = writer.begin_iteration(user_req()).unwrap_err();
        assert!(matches!(err, StateApplyError::RunArchived));
    }

    #[test]
    fn current_iteration_returns_run_archived_when_archived() {
        let mut state = archived_state();
        let mut writer = RunStateWriter::new(&mut state);
        let err = writer.current_iteration().unwrap_err();
        assert!(matches!(err, StateApplyError::RunArchived));
    }

    #[test]
    fn wait_for_user_returns_run_archived_when_archived() {
        let mut state = archived_state();
        let mut writer = RunStateWriter::new(&mut state);
        let err = writer.wait_for_user().unwrap_err();
        assert!(matches!(err, StateApplyError::RunArchived));
    }

    #[test]
    fn begin_step_returns_run_archived_when_archived() {
        // begin_step is only reachable via CurrentIterationWriter, which itself
        // requires current_iteration(). current_iteration() guards against archived
        // state, so the RunArchived path in begin_step is verified here via
        // current_iteration returning RunArchived for an archived run.
        let mut state = archived_state();
        state.iterations.push(RunIteration {
            iteration_id: RunIterationId(Uuid::new_v4()),
            config_snapshot: None,
            step_records: vec![finished_ok_record(
                StepKind::UserInputReceived,
                user_input_result(),
            )],
        });
        let mut writer = RunStateWriter::new(&mut state);
        let err = writer.current_iteration().unwrap_err();
        assert!(matches!(err, StateApplyError::RunArchived));
    }

    // ─── StepKind / StepResultEnvelope compatibility via record_success ────────

    #[test]
    fn record_success_accepts_matching_result_for_each_step_kind() {
        use crate::shared_types::{
            CandidateCardRetrievalOutput, CardHydrationOutput, IncidentEvidenceRetrievalOutput,
            NormalizedUserRequest, TheoryEvidenceRetrievalOutput,
        };

        fn make_state_with_pending(kind: StepKind) -> RunState {
            let mut s = fresh_state();
            let iid = RunIterationId(Uuid::new_v4());
            s.iterations.push(RunIteration {
                iteration_id: iid,
                config_snapshot: None,
                step_records: vec![
                    finished_ok_record(StepKind::UserInputReceived, user_input_result()),
                    pending_record_of_kind(kind),
                ],
            });
            s
        }

        fn pending_record_of_kind(kind: StepKind) -> StepRecord {
            StepRecord::Pending(PendingStepRecord {
                record_id: StepRecordId(Uuid::new_v4()),
                step: kind,
                started_at: Utc::now(),
            })
        }

        let normalized = NormalizedUserRequest {
            query: "q".to_string(),
            input_token_count: 1,
        };

        let cases: Vec<(StepKind, StepResultEnvelope)> = vec![
            (
                StepKind::InputNormalization,
                StepResultEnvelope::InputNormalization(normalized.clone()),
            ),
            (
                StepKind::CandidateCardRetrieval,
                StepResultEnvelope::CandidateCardRetrieval(CandidateCardRetrievalOutput {
                    primary: None,
                    alternatives: vec![],
            metrics: None,
                }),
            ),
            (
                StepKind::CardHydration,
                StepResultEnvelope::CardHydration(CardHydrationOutput {
                    primary: None,
                    alternatives: vec![],
                }),
            ),
            (
                StepKind::IncidentEvidenceRetrieval,
                StepResultEnvelope::IncidentEvidenceRetrieval(IncidentEvidenceRetrievalOutput {
                    primary_chunks: vec![],
                    alternative_chunks: vec![],
            metrics: None,
                }),
            ),
            (
                StepKind::TheoryEvidenceRetrieval,
                StepResultEnvelope::TheoryEvidenceRetrieval(TheoryEvidenceRetrievalOutput {
                    chunks: vec![],
            metrics: None,
                }),
            ),
        ];

        for (kind, result) in cases {
            let mut state = make_state_with_pending(kind);
            let mut writer = RunStateWriter::new(&mut state);
            let mut cw = writer.current_iteration().unwrap();
            let pw = cw.pending_step().unwrap();
            let res = pw.record_success(result);
            assert!(res.is_ok(), "record_success failed for {kind:?}");
        }
    }

    // ─── StepKind / StepError compatibility via record_failure ────────────────

    #[test]
    fn record_failure_accepts_matching_step_specific_errors() {
        use crate::api_clients::qdrant::theory_chunks_collection::TheoryChunksCollectionError;
        use crate::request_pipeline::{
            candidate_card_retrieval::CandidateCardRetrievalError,
            theory_evidence_retrieval::TheoryEvidenceRetrievalError,
        };

        fn make_state_with_pending_kind(kind: StepKind) -> RunState {
            let mut s = fresh_state();
            let iid = RunIterationId(Uuid::new_v4());
            s.iterations.push(RunIteration {
                iteration_id: iid,
                config_snapshot: None,
                step_records: vec![
                    finished_ok_record(StepKind::UserInputReceived, user_input_result()),
                    StepRecord::Pending(PendingStepRecord {
                        record_id: StepRecordId(Uuid::new_v4()),
                        step: kind,
                        started_at: Utc::now(),
                    }),
                ],
            });
            s
        }

        let cases: Vec<(StepKind, StepError)> = vec![
            (
                StepKind::InputNormalization,
                StepError::InputNormalization(InputNormalizationError::EmptyQuery),
            ),
            (
                StepKind::CandidateCardRetrieval,
                StepError::CandidateCardRetrieval(
                    CandidateCardRetrievalError::InvalidConfiguration(
                        "top_k must be greater than 0".to_string(),
                    ),
                ),
            ),
            (
                StepKind::TheoryEvidenceRetrieval,
                StepError::TheoryEvidenceRetrieval(TheoryEvidenceRetrievalError::Collection(
                    TheoryChunksCollectionError::InvalidRequest("invalid request".to_string()),
                )),
            ),
        ];

        for (kind, error) in cases {
            let mut state = make_state_with_pending_kind(kind);
            let mut writer = RunStateWriter::new(&mut state);
            let mut cw = writer.current_iteration().unwrap();
            let pw = cw.pending_step().unwrap();
            let res = pw.record_failure(error);
            assert!(res.is_ok(), "record_failure failed for {kind:?}");
        }
    }
}
fn finished_at_not_before_started(started_at: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    std::cmp::max(Utc::now(), started_at)
}
