use chrono::{DateTime, Utc};

use crate::orchestrator::run_state::model::{
    FinishedStepRecord, PendingStepRecord, RunId, RunIteration, RunIterationId, RunState,
    RunStatus, StepError, StepKind, StepRecord, StepRecordId, StepResultEnvelope,
};

#[derive(Debug, Clone, Copy)]
pub struct RunStateView<'a> {
    state: &'a RunState,
}

#[derive(Debug, Clone, Copy)]
pub struct IterationView<'a> {
    iteration: &'a RunIteration,
}

#[derive(Debug, Clone, Copy)]
pub enum StepView<'a> {
    Pending(PendingStepView<'a>),
    Finished(FinishedStepView<'a>),
}

#[derive(Debug, Clone, Copy)]
pub struct PendingStepView<'a> {
    record: &'a PendingStepRecord,
}

#[derive(Debug, Clone, Copy)]
pub struct FinishedStepView<'a> {
    record: &'a FinishedStepRecord,
}

impl<'a> RunStateView<'a> {
    pub fn new(state: &'a RunState) -> Self {
        Self { state }
    }

    pub fn run_id(&self) -> RunId {
        self.state.run_id
    }

    pub fn status(&self) -> RunStatus {
        self.state.status
    }

    pub fn iterations(&self) -> impl DoubleEndedIterator<Item = IterationView<'a>> {
        self.state
            .iterations
            .iter()
            .map(|iter| IterationView { iteration: iter })
    }

    pub fn last_iteration(&self) -> Option<IterationView<'a>> {
        self.state
            .iterations
            .last()
            .map(|iter| IterationView { iteration: iter })
    }
}

impl<'a> IterationView<'a> {
    pub fn iteration_id(&self) -> RunIterationId {
        self.iteration.iteration_id
    }

    pub fn steps(&self) -> impl DoubleEndedIterator<Item = StepView<'a>> {
        self.iteration.step_records.iter().map(|r| match r {
            StepRecord::Pending(p) => StepView::Pending(PendingStepView { record: p }),
            StepRecord::Finished(f) => StepView::Finished(FinishedStepView { record: f }),
        })
    }

    pub fn finished_steps(&self) -> impl DoubleEndedIterator<Item = FinishedStepView<'a>> {
        self.iteration.step_records.iter().filter_map(|r| match r {
            StepRecord::Finished(f) => Some(FinishedStepView { record: f }),
            StepRecord::Pending(_) => None,
        })
    }

    pub fn pending_step(&self) -> Option<PendingStepView<'a>> {
        self.iteration.step_records.iter().find_map(|r| match r {
            StepRecord::Pending(p) => Some(PendingStepView { record: p }),
            StepRecord::Finished(_) => None,
        })
    }

    pub fn finished_step(&self, kind: StepKind) -> Option<FinishedStepView<'a>> {
        self.iteration
            .step_records
            .iter()
            .rev()
            .find_map(|r| match r {
                StepRecord::Finished(f) if f.step == kind => Some(FinishedStepView { record: f }),
                _ => None,
            })
    }
}

impl<'a> PendingStepView<'a> {
    pub fn record_id(&self) -> StepRecordId {
        self.record.record_id
    }

    pub fn kind(&self) -> StepKind {
        self.record.step
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.record.started_at
    }
}

impl<'a> FinishedStepView<'a> {
    pub fn record_id(&self) -> StepRecordId {
        self.record.record_id
    }

    pub fn kind(&self) -> StepKind {
        self.record.step
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.record.started_at
    }

    pub fn finished_at(&self) -> DateTime<Utc> {
        self.record.finished_at
    }

    pub fn result(&self) -> &'a Result<StepResultEnvelope, StepError> {
        &self.record.result
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use crate::orchestrator::run_state::model::{
        FinishedStepRecord, PendingStepRecord, RunId, RunIteration, RunIterationId, RunState,
        RunStatus, StepError, StepKind, StepRecord, StepRecordId, StepResultEnvelope,
    };
    use crate::orchestrator::run_state::view::{
        FinishedStepView, PendingStepView, RunStateView, StepView,
    };
    use crate::shared_types::UserRequest;

    // ─── Helpers ──────────────────────────────────────────────────────────────

    fn user_req() -> UserRequest {
        UserRequest {
            query: "q".to_string(),
        }
    }

    fn new_run_id() -> RunId {
        RunId(Uuid::new_v4())
    }

    fn new_step_record_id() -> StepRecordId {
        StepRecordId(Uuid::new_v4())
    }

    fn new_iteration_id() -> RunIterationId {
        RunIterationId(Uuid::new_v4())
    }

    fn pending_record(kind: StepKind) -> StepRecord {
        StepRecord::Pending(PendingStepRecord {
            record_id: new_step_record_id(),
            step: kind,
            started_at: Utc::now(),
        })
    }

    fn finished_ok_record(kind: StepKind, result: StepResultEnvelope) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: new_step_record_id(),
            step: kind,
            started_at: now,
            finished_at: now,
            result: Ok(result),
        })
    }

    fn finished_err_record(kind: StepKind, error: StepError) -> StepRecord {
        let now = Utc::now();
        StepRecord::Finished(FinishedStepRecord {
            record_id: new_step_record_id(),
            step: kind,
            started_at: now,
            finished_at: now,
            result: Err(error),
        })
    }

    fn user_input_result() -> StepResultEnvelope {
        StepResultEnvelope::UserInputReceived(user_req())
    }

    fn empty_state(run_id: RunId) -> RunState {
        let now = Utc::now();
        RunState {
            run_id,
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations: vec![],
        }
    }

    fn state_with_iterations(iterations: Vec<RunIteration>) -> RunState {
        let rid = new_run_id();
        let now = Utc::now();
        RunState {
            run_id: rid,
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            revision: 0,
            iterations,
        }
    }

    fn iteration(step_records: Vec<StepRecord>) -> RunIteration {
        RunIteration {
            iteration_id: new_iteration_id(),
            step_records,
        }
    }

    // ─── Module visibility ────────────────────────────────────────────────────

    #[test]
    fn view_types_importable_from_view_path() {
        let mut state = empty_state(new_run_id());
        let _view = RunStateView::new(&state);
        // FinishedStepView and PendingStepView are imported but only constructed
        // via the view API — verify the import compiles.
        let _: Option<PendingStepView<'_>> = None;
        let _: Option<FinishedStepView<'_>> = None;
        drop(_view);
        drop(&mut state);
    }

    // ─── RunStateView::new ────────────────────────────────────────────────────

    #[test]
    fn run_state_view_new_wraps_borrowed_state() {
        let rid = new_run_id();
        let state = empty_state(rid);
        let view = RunStateView::new(&state);
        assert_eq!(view.run_id(), rid);
    }

    // ─── RunStateView::run_id ─────────────────────────────────────────────────

    #[test]
    fn run_state_view_run_id_returns_underlying_id() {
        let rid = new_run_id();
        let state = empty_state(rid);
        let view = RunStateView::new(&state);
        assert_eq!(view.run_id(), rid);
    }

    // ─── RunStateView::status ─────────────────────────────────────────────────

    #[test]
    fn run_state_view_status_returns_underlying_status() {
        let mut state = empty_state(new_run_id());
        state.status = RunStatus::WaitingForUser;
        let view = RunStateView::new(&state);
        assert_eq!(view.status(), RunStatus::WaitingForUser);
    }

    // ─── RunStateView::iterations ─────────────────────────────────────────────

    #[test]
    fn iterations_preserves_order() {
        let id1 = new_iteration_id();
        let id2 = new_iteration_id();
        let state = state_with_iterations(vec![
            RunIteration {
                iteration_id: id1,
                step_records: vec![],
            },
            RunIteration {
                iteration_id: id2,
                step_records: vec![],
            },
        ]);
        let view = RunStateView::new(&state);
        let ids: Vec<RunIterationId> = view.iterations().map(|iv| iv.iteration_id()).collect();
        assert_eq!(ids, vec![id1, id2]);
    }

    #[test]
    fn iterations_empty_when_no_iterations() {
        let state = empty_state(new_run_id());
        let view = RunStateView::new(&state);
        assert_eq!(view.iterations().count(), 0);
    }

    // ─── RunStateView::last_iteration ─────────────────────────────────────────

    #[test]
    fn last_iteration_returns_the_last_one() {
        let id1 = new_iteration_id();
        let id2 = new_iteration_id();
        let state = state_with_iterations(vec![
            RunIteration {
                iteration_id: id1,
                step_records: vec![],
            },
            RunIteration {
                iteration_id: id2,
                step_records: vec![],
            },
        ]);
        let view = RunStateView::new(&state);
        assert_eq!(view.last_iteration().unwrap().iteration_id(), id2);
    }

    #[test]
    fn last_iteration_returns_none_when_no_iterations() {
        let state = empty_state(new_run_id());
        let view = RunStateView::new(&state);
        assert!(view.last_iteration().is_none());
    }

    // ─── IterationView::iteration_id ──────────────────────────────────────────

    #[test]
    fn iteration_view_iteration_id_returns_underlying_id() {
        let iid = new_iteration_id();
        let state = state_with_iterations(vec![RunIteration {
            iteration_id: iid,
            step_records: vec![],
        }]);
        let view = RunStateView::new(&state);
        assert_eq!(view.last_iteration().unwrap().iteration_id(), iid);
    }

    // ─── IterationView::steps ─────────────────────────────────────────────────

    #[test]
    fn steps_preserves_order() {
        let r1 = finished_ok_record(StepKind::UserInputReceived, user_input_result());
        let r2 = pending_record(StepKind::InputNormalization);
        let state = state_with_iterations(vec![iteration(vec![r1, r2])]);
        let view = RunStateView::new(&state);
        let steps: Vec<_> = view.last_iteration().unwrap().steps().collect();
        assert_eq!(steps.len(), 2);
        assert!(matches!(steps[0], StepView::Finished(_)));
        assert!(matches!(steps[1], StepView::Pending(_)));
    }

    // ─── IterationView::finished_steps ────────────────────────────────────────

    #[test]
    fn finished_steps_returns_only_finished_in_order() {
        let r1 = finished_ok_record(StepKind::UserInputReceived, user_input_result());
        let r2 = pending_record(StepKind::InputNormalization);
        let r3 = finished_err_record(
            StepKind::QueryStructuring,
            StepError::MissingRequiredInput {
                message: "x".to_string(),
            },
        );
        let state = state_with_iterations(vec![iteration(vec![r1, r2, r3])]);
        let view = RunStateView::new(&state);
        let finished: Vec<_> = view.last_iteration().unwrap().finished_steps().collect();
        assert_eq!(finished.len(), 2);
        assert_eq!(finished[0].kind(), StepKind::UserInputReceived);
        assert_eq!(finished[1].kind(), StepKind::QueryStructuring);
    }

    // ─── IterationView::pending_step ──────────────────────────────────────────

    #[test]
    fn pending_step_returns_some_when_pending_exists() {
        let r1 = finished_ok_record(StepKind::UserInputReceived, user_input_result());
        let r2 = pending_record(StepKind::InputNormalization);
        let state = state_with_iterations(vec![iteration(vec![r1, r2])]);
        let view = RunStateView::new(&state);
        let pending = view.last_iteration().unwrap().pending_step();
        assert!(pending.is_some());
        assert_eq!(pending.unwrap().kind(), StepKind::InputNormalization);
    }

    #[test]
    fn pending_step_returns_none_when_no_pending() {
        let r1 = finished_ok_record(StepKind::UserInputReceived, user_input_result());
        let state = state_with_iterations(vec![iteration(vec![r1])]);
        let view = RunStateView::new(&state);
        assert!(view.last_iteration().unwrap().pending_step().is_none());
    }

    // ─── IterationView::finished_step(kind) ───────────────────────────────────

    #[test]
    fn finished_step_returns_last_with_requested_kind() {
        let r1 = finished_ok_record(StepKind::UserInputReceived, user_input_result());
        let r2 = finished_ok_record(StepKind::UserInputReceived, user_input_result());
        let state = state_with_iterations(vec![iteration(vec![r1, r2])]);
        let view = RunStateView::new(&state);
        let fv = view
            .last_iteration()
            .unwrap()
            .finished_step(StepKind::UserInputReceived);
        assert!(fv.is_some());
    }

    #[test]
    fn finished_step_returns_none_when_kind_absent() {
        let r1 = finished_ok_record(StepKind::UserInputReceived, user_input_result());
        let state = state_with_iterations(vec![iteration(vec![r1])]);
        let view = RunStateView::new(&state);
        let fv = view
            .last_iteration()
            .unwrap()
            .finished_step(StepKind::InputNormalization);
        assert!(fv.is_none());
    }

    // ─── StepView mapping ─────────────────────────────────────────────────────

    #[test]
    fn step_view_maps_pending_record_to_pending_variant() {
        let state = state_with_iterations(vec![iteration(vec![pending_record(
            StepKind::InputNormalization,
        )])]);
        let view = RunStateView::new(&state);
        let step = view.last_iteration().unwrap().steps().next().unwrap();
        assert!(matches!(step, StepView::Pending(_)));
    }

    #[test]
    fn step_view_maps_finished_record_to_finished_variant() {
        let state = state_with_iterations(vec![iteration(vec![finished_ok_record(
            StepKind::UserInputReceived,
            user_input_result(),
        )])]);
        let view = RunStateView::new(&state);
        let step = view.last_iteration().unwrap().steps().next().unwrap();
        assert!(matches!(step, StepView::Finished(_)));
    }

    // ─── PendingStepView accessors ────────────────────────────────────────────

    #[test]
    fn pending_step_view_returns_record_id_kind_started_at() {
        let rid = new_step_record_id();
        let started = Utc::now();
        let record = StepRecord::Pending(PendingStepRecord {
            record_id: rid,
            step: StepKind::InputNormalization,
            started_at: started,
        });
        let state = state_with_iterations(vec![iteration(vec![record])]);
        let view = RunStateView::new(&state);
        let pv = view.last_iteration().unwrap().pending_step().unwrap();
        assert_eq!(pv.record_id(), rid);
        assert_eq!(pv.kind(), StepKind::InputNormalization);
        assert_eq!(pv.started_at(), started);
    }

    // ─── FinishedStepView accessors ───────────────────────────────────────────

    #[test]
    fn finished_step_view_returns_all_fields() {
        let rid = new_step_record_id();
        let started = Utc::now();
        let finished = started;
        let result = Ok(user_input_result());
        let record = StepRecord::Finished(FinishedStepRecord {
            record_id: rid,
            step: StepKind::UserInputReceived,
            started_at: started,
            finished_at: finished,
            result: result.clone(),
        });
        let state = state_with_iterations(vec![iteration(vec![record])]);
        let view = RunStateView::new(&state);
        let fv = view
            .last_iteration()
            .unwrap()
            .finished_step(StepKind::UserInputReceived)
            .unwrap();
        assert_eq!(fv.record_id(), rid);
        assert_eq!(fv.kind(), StepKind::UserInputReceived);
        assert_eq!(fv.started_at(), started);
        assert_eq!(fv.finished_at(), finished);
        assert_eq!(fv.result(), &result);
    }
}
