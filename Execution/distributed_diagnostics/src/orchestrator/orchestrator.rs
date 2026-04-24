use async_trait::async_trait;
use thiserror::Error;

use crate::orchestrator::run_repository::{RunRepository, RunRepositoryError};
use crate::orchestrator::run_state::apply::{RunStateWriter, StateApplyError};
use crate::orchestrator::run_state::model::{
    FinishedStepRecord, RunId, RunIteration, RunState, StepError, StepKind, StepRecord,
    StepResultEnvelope,
};
use crate::orchestrator::run_state::view::RunStateView;
use crate::orchestrator::step_executor::StepExecutor;
use crate::orchestrator::transition_policy::{
    PolicyError, PolicyTransition, TransitionPolicy,
};
use crate::shared_types::{ResponseValidationAndNormalizationOutput, UserRequest};

#[derive(Debug)]
pub struct Orchestrator<P> {
    policy: P,
    executor: StepExecutor,
    run_repository: RunRepository,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunOutcome {
    Finished {
        run_id: RunId,
        result: ResponseValidationAndNormalizationOutput,
    },
    Failed {
        run_id: RunId,
        error: StepError,
    },
}

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("run not found: {run_id:?}")]
    RunNotFound { run_id: RunId },

    #[error("policy error: {0}")]
    Policy(#[from] PolicyError),

    #[error("state-application error: {0}")]
    StateApply(#[from] StateApplyError),

    #[error("run repository error: {0}")]
    Repository(#[from] RunRepositoryError),

    #[error("pending step for {step:?} was expected but not found")]
    MissingPendingStep { step: StepKind },
}

impl<P> Orchestrator<P>
where
    P: TransitionPolicy,
{
    pub fn new(policy: P, executor: StepExecutor, run_repository: RunRepository) -> Self {
        Self {
            policy,
            executor,
            run_repository,
        }
    }

    pub async fn run(&self, user_input: UserRequest) -> Result<RunOutcome, OrchestratorError> {
        let mut state = RunState::new();
        self.run_repository.create_run(&state).await?;

        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.begin_iteration(user_input)?;
        }

        let iteration_sequence_no = RunStateView::new(&state).iteration_count() as u64 - 1;
        let iteration_id = RunStateView::new(&state)
            .last_iteration()
            .expect("begin_iteration must create a current iteration")
            .iteration_id();
        let iteration = RunStateView::new(&state)
            .iteration(iteration_id)
            .expect("last_iteration view must reference a stored iteration");

        self.run_repository
            .append_iteration(&state, iteration_sequence_no, iteration)
            .await?;

        self.drive_to_outcome(&mut state).await
    }

    pub async fn resume(&self, run_id: RunId) -> Result<RunOutcome, OrchestratorError> {
        let mut state = self.load_existing_run(run_id).await?;
        self.drive_to_outcome(&mut state).await
    }

    pub async fn resume_with_input(
        &self,
        run_id: RunId,
        user_input: UserRequest,
    ) -> Result<RunOutcome, OrchestratorError> {
        let mut state = self.load_existing_run(run_id).await?;

        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.begin_iteration(user_input)?;
        }

        let iteration_sequence_no = RunStateView::new(&state).iteration_count() as u64 - 1;
        let iteration_id = RunStateView::new(&state)
            .last_iteration()
            .expect("begin_iteration must create a current iteration")
            .iteration_id();
        let iteration = RunStateView::new(&state)
            .iteration(iteration_id)
            .expect("last_iteration view must reference a stored iteration");

        self.run_repository
            .append_iteration(&state, iteration_sequence_no, iteration)
            .await?;

        self.drive_to_outcome(&mut state).await
    }

    async fn load_existing_run(&self, run_id: RunId) -> Result<RunState, OrchestratorError> {
        load_existing_run_impl(&self.run_repository, run_id).await
    }

    async fn drive_to_outcome(
        &self,
        state: &mut RunState,
    ) -> Result<RunOutcome, OrchestratorError> {
        drive_to_outcome_impl(&self.policy, &self.executor, &self.run_repository, state).await
    }
}

#[async_trait(?Send)]
trait ExecutorLike {
    async fn execute_step(
        &self,
        step: StepKind,
        state: RunStateView<'_>,
    ) -> Result<StepResultEnvelope, StepError>;
}

#[async_trait(?Send)]
trait RepositoryLike {
    #[cfg_attr(not(test), allow(dead_code))]
    async fn create_run(&self, run: &RunState) -> Result<(), RunRepositoryError>;
    async fn load_run(&self, run_id: RunId) -> Result<Option<RunState>, RunRepositoryError>;
    #[cfg_attr(not(test), allow(dead_code))]
    async fn append_iteration(
        &self,
        run: &RunState,
        iteration_sequence_no: u64,
        iteration: &RunIteration,
    ) -> Result<(), RunRepositoryError>;
    async fn append_step_record(
        &self,
        run: &RunState,
        iteration_id: crate::orchestrator::run_state::model::RunIterationId,
        step_sequence_no: u64,
        step_record: &StepRecord,
    ) -> Result<(), RunRepositoryError>;
    async fn finish_step_record(
        &self,
        run: &RunState,
        record_id: crate::orchestrator::run_state::model::StepRecordId,
        finished_record: &FinishedStepRecord,
    ) -> Result<(), RunRepositoryError>;
}

#[async_trait(?Send)]
impl ExecutorLike for StepExecutor {
    async fn execute_step(
        &self,
        step: StepKind,
        state: RunStateView<'_>,
    ) -> Result<StepResultEnvelope, StepError> {
        self.execute(step, state).await
    }
}

#[async_trait(?Send)]
impl RepositoryLike for RunRepository {
    async fn create_run(&self, run: &RunState) -> Result<(), RunRepositoryError> {
        self.create_run(run).await
    }

    async fn load_run(&self, run_id: RunId) -> Result<Option<RunState>, RunRepositoryError> {
        self.load_run(run_id).await
    }

    async fn append_iteration(
        &self,
        run: &RunState,
        iteration_sequence_no: u64,
        iteration: &RunIteration,
    ) -> Result<(), RunRepositoryError> {
        self.append_iteration(run, iteration_sequence_no, iteration).await
    }

    async fn append_step_record(
        &self,
        run: &RunState,
        iteration_id: crate::orchestrator::run_state::model::RunIterationId,
        step_sequence_no: u64,
        step_record: &StepRecord,
    ) -> Result<(), RunRepositoryError> {
        self.append_step_record(run, iteration_id, step_sequence_no, step_record)
            .await
    }

    async fn finish_step_record(
        &self,
        run: &RunState,
        record_id: crate::orchestrator::run_state::model::StepRecordId,
        finished_record: &FinishedStepRecord,
    ) -> Result<(), RunRepositoryError> {
        self.finish_step_record(run, record_id, finished_record).await
    }
}

#[cfg(test)]
async fn run_impl<P, E, R>(
    policy: &P,
    executor: &E,
    run_repository: &R,
    user_input: UserRequest,
) -> Result<RunOutcome, OrchestratorError>
where
    P: TransitionPolicy,
    E: ExecutorLike + Sync,
    R: RepositoryLike + Sync,
{
    let mut state = RunState::new();
    run_repository.create_run(&state).await?;

    {
        let mut writer = RunStateWriter::new(&mut state);
        writer.begin_iteration(user_input)?;
    }

    let iteration_sequence_no = RunStateView::new(&state).iteration_count() as u64 - 1;
    let iteration_id = RunStateView::new(&state)
        .last_iteration()
        .expect("begin_iteration must create a current iteration")
        .iteration_id();
    let iteration = RunStateView::new(&state)
        .iteration(iteration_id)
        .expect("last_iteration view must reference a stored iteration");

    run_repository
        .append_iteration(&state, iteration_sequence_no, iteration)
        .await?;

    drive_to_outcome_impl(policy, executor, run_repository, &mut state).await
}

#[cfg(test)]
async fn resume_impl<P, E, R>(
    policy: &P,
    executor: &E,
    run_repository: &R,
    run_id: RunId,
) -> Result<RunOutcome, OrchestratorError>
where
    P: TransitionPolicy,
    E: ExecutorLike + Sync,
    R: RepositoryLike + Sync,
{
    let mut state = load_existing_run_impl(run_repository, run_id).await?;
    drive_to_outcome_impl(policy, executor, run_repository, &mut state).await
}

#[cfg(test)]
async fn resume_with_input_impl<P, E, R>(
    policy: &P,
    executor: &E,
    run_repository: &R,
    run_id: RunId,
    user_input: UserRequest,
) -> Result<RunOutcome, OrchestratorError>
where
    P: TransitionPolicy,
    E: ExecutorLike + Sync,
    R: RepositoryLike + Sync,
{
    let mut state = load_existing_run_impl(run_repository, run_id).await?;

    {
        let mut writer = RunStateWriter::new(&mut state);
        writer.begin_iteration(user_input)?;
    }

    let iteration_sequence_no = RunStateView::new(&state).iteration_count() as u64 - 1;
    let iteration_id = RunStateView::new(&state)
        .last_iteration()
        .expect("begin_iteration must create a current iteration")
        .iteration_id();
    let iteration = RunStateView::new(&state)
        .iteration(iteration_id)
        .expect("last_iteration view must reference a stored iteration");

    run_repository
        .append_iteration(&state, iteration_sequence_no, iteration)
        .await?;

    drive_to_outcome_impl(policy, executor, run_repository, &mut state).await
}

async fn load_existing_run_impl<R>(
    run_repository: &R,
    run_id: RunId,
) -> Result<RunState, OrchestratorError>
where
    R: RepositoryLike + Sync,
{
    run_repository
        .load_run(run_id)
        .await?
        .ok_or(OrchestratorError::RunNotFound { run_id })
}

async fn drive_to_outcome_impl<P, E, R>(
    policy: &P,
    executor: &E,
    run_repository: &R,
    state: &mut RunState,
) -> Result<RunOutcome, OrchestratorError>
where
    P: TransitionPolicy,
    E: ExecutorLike + Sync,
    R: RepositoryLike + Sync,
{
    loop {
        let decision = policy.next_transition(RunStateView::new(state))?;

        match decision {
            PolicyTransition::ExecuteStep { step } => {
                let (record_id, iteration_id) = {
                    let mut writer = RunStateWriter::new(state);
                    let mut iteration = writer.current_iteration()?;
                    let pending = iteration.begin_step(step)?;
                    (pending.record_id(), iteration.iteration_id())
                };

                let iteration_view = RunStateView::new(state)
                    .last_iteration()
                    .ok_or(StateApplyError::NoCurrentIteration)?;
                let step_sequence_no = iteration_view.step_count() as u64 - 1;
                let step_record = match iteration_view.pending_step() {
                    Some(pending) => StepRecord::Pending(pending.to_owned()),
                    None => return Err(OrchestratorError::MissingPendingStep { step }),
                };

                run_repository
                    .append_step_record(state, iteration_id, step_sequence_no, &step_record)
                    .await?;

                let execution_result = executor.execute_step(step, RunStateView::new(state)).await;

                {
                    let mut writer = RunStateWriter::new(state);
                    let mut iteration = writer.current_iteration()?;
                    let pending = iteration
                        .pending_step()
                        .ok_or(OrchestratorError::MissingPendingStep { step })?;

                    match execution_result {
                        Ok(result) => pending.record_success(result)?,
                        Err(error) => pending.record_failure(error)?,
                    }
                }

                let finished_record = match RunStateView::new(state)
                    .last_iteration()
                    .and_then(|iteration| iteration.finished_step(step))
                    .map(|record| record.to_owned())
                {
                    Some(record) if record.record_id == record_id => record,
                    _ => return Err(OrchestratorError::MissingPendingStep { step }),
                };

                run_repository
                    .finish_step_record(state, record_id, &finished_record)
                    .await?;
            }
            PolicyTransition::FinishWithResult { result } => {
                return Ok(RunOutcome::Finished {
                    run_id: state.run_id,
                    result,
                });
            }
            PolicyTransition::FinishWithError { error } => {
                return Ok(RunOutcome::Failed {
                    run_id: state.run_id,
                    error,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use uuid::Uuid;

    use super::*;
    use crate::orchestrator::run_repository::RunRepositoryError;
    use crate::orchestrator::run_state::model::{RunIterationId, RunStatus, StepRecordId};
    use crate::shared_types::{DiagnosticResponse, DiagnosticResultInterpretation, NormalizedUserRequest};

    #[derive(Debug, Clone, PartialEq)]
    enum Event {
        CreateRun,
        LoadRun(RunId),
        AppendIteration { sequence_no: u64, iteration_id: RunIterationId },
        AppendStepRecord { iteration_id: RunIterationId, sequence_no: u64, record_id: StepRecordId },
        ExecuteStep { step: StepKind },
        FinishStepRecord { record_id: StepRecordId, is_ok: bool },
    }

    #[derive(Debug, Default)]
    struct FakePolicy {
        decisions: Mutex<VecDeque<Result<PolicyTransition, PolicyError>>>,
    }

    impl FakePolicy {
        fn from_decisions(decisions: Vec<Result<PolicyTransition, PolicyError>>) -> Self {
            Self {
                decisions: Mutex::new(VecDeque::from(decisions)),
            }
        }
    }

    impl TransitionPolicy for FakePolicy {
        fn next_transition(
            &self,
            _state: RunStateView<'_>,
        ) -> Result<PolicyTransition, PolicyError> {
            self.decisions
                .lock()
                .expect("policy mutex poisoned")
                .pop_front()
                .expect("policy decision queue exhausted")
        }
    }

    #[derive(Debug, Default)]
    struct FakeExecutor {
        results: Mutex<VecDeque<Result<StepResultEnvelope, StepError>>>,
        events: Arc<Mutex<Vec<Event>>>,
    }

    impl FakeExecutor {
        fn new(
            results: Vec<Result<StepResultEnvelope, StepError>>,
            events: Arc<Mutex<Vec<Event>>>,
        ) -> Self {
            Self {
                results: Mutex::new(VecDeque::from(results)),
                events,
            }
        }
    }

    #[async_trait(?Send)]
    impl ExecutorLike for FakeExecutor {
        async fn execute_step(
            &self,
            step: StepKind,
            _state: RunStateView<'_>,
        ) -> Result<StepResultEnvelope, StepError> {
            self.events
                .lock()
                .expect("events mutex poisoned")
                .push(Event::ExecuteStep { step });
            self.results
                .lock()
                .expect("executor mutex poisoned")
                .pop_front()
                .expect("executor result queue exhausted")
        }
    }

    #[derive(Debug)]
    struct FakeRepository {
        loaded_run: Mutex<Option<RunState>>,
        events: Arc<Mutex<Vec<Event>>>,
        create_run_result: Mutex<VecDeque<Result<(), RunRepositoryError>>>,
        load_run_result: Mutex<VecDeque<Result<Option<RunState>, RunRepositoryError>>>,
        append_iteration_result: Mutex<VecDeque<Result<(), RunRepositoryError>>>,
        append_step_record_result: Mutex<VecDeque<Result<(), RunRepositoryError>>>,
        finish_step_record_result: Mutex<VecDeque<Result<(), RunRepositoryError>>>,
    }

    impl FakeRepository {
        fn new(loaded_run: Option<RunState>, events: Arc<Mutex<Vec<Event>>>) -> Self {
            Self {
                loaded_run: Mutex::new(loaded_run),
                events,
                create_run_result: Mutex::new(VecDeque::new()),
                load_run_result: Mutex::new(VecDeque::new()),
                append_iteration_result: Mutex::new(VecDeque::new()),
                append_step_record_result: Mutex::new(VecDeque::new()),
                finish_step_record_result: Mutex::new(VecDeque::new()),
            }
        }

        fn with_create_run_results(self, results: Vec<Result<(), RunRepositoryError>>) -> Self {
            *self
                .create_run_result
                .lock()
                .expect("create_run_result mutex poisoned") = VecDeque::from(results);
            self
        }

        fn with_load_run_results(
            self,
            results: Vec<Result<Option<RunState>, RunRepositoryError>>,
        ) -> Self {
            *self
                .load_run_result
                .lock()
                .expect("load_run_result mutex poisoned") = VecDeque::from(results);
            self
        }

        fn with_append_iteration_results(
            self,
            results: Vec<Result<(), RunRepositoryError>>,
        ) -> Self {
            *self
                .append_iteration_result
                .lock()
                .expect("append_iteration_result mutex poisoned") = VecDeque::from(results);
            self
        }

        fn with_append_step_record_results(
            self,
            results: Vec<Result<(), RunRepositoryError>>,
        ) -> Self {
            *self
                .append_step_record_result
                .lock()
                .expect("append_step_record_result mutex poisoned") = VecDeque::from(results);
            self
        }

        fn with_finish_step_record_results(
            self,
            results: Vec<Result<(), RunRepositoryError>>,
        ) -> Self {
            *self
                .finish_step_record_result
                .lock()
                .expect("finish_step_record_result mutex poisoned") = VecDeque::from(results);
            self
        }
    }

    #[async_trait(?Send)]
    impl RepositoryLike for FakeRepository {
        async fn create_run(&self, _run: &RunState) -> Result<(), RunRepositoryError> {
            self.events
                .lock()
                .expect("events mutex poisoned")
                .push(Event::CreateRun);
            if let Some(result) = self
                .create_run_result
                .lock()
                .expect("create_run_result mutex poisoned")
                .pop_front()
            {
                return result;
            }
            Ok(())
        }

        async fn load_run(&self, run_id: RunId) -> Result<Option<RunState>, RunRepositoryError> {
            self.events
                .lock()
                .expect("events mutex poisoned")
                .push(Event::LoadRun(run_id));
            if let Some(result) = self
                .load_run_result
                .lock()
                .expect("load_run_result mutex poisoned")
                .pop_front()
            {
                return result;
            }
            Ok(self
                .loaded_run
                .lock()
                .expect("loaded_run mutex poisoned")
                .clone())
        }

        async fn append_iteration(
            &self,
            _run: &RunState,
            iteration_sequence_no: u64,
            iteration: &RunIteration,
        ) -> Result<(), RunRepositoryError> {
            self.events
                .lock()
                .expect("events mutex poisoned")
                .push(Event::AppendIteration {
                    sequence_no: iteration_sequence_no,
                    iteration_id: iteration.iteration_id,
                });
            if let Some(result) = self
                .append_iteration_result
                .lock()
                .expect("append_iteration_result mutex poisoned")
                .pop_front()
            {
                return result;
            }
            Ok(())
        }

        async fn append_step_record(
            &self,
            _run: &RunState,
            iteration_id: RunIterationId,
            step_sequence_no: u64,
            step_record: &StepRecord,
        ) -> Result<(), RunRepositoryError> {
            let record_id = match step_record {
                StepRecord::Pending(record) => record.record_id,
                StepRecord::Finished(_) => panic!("append_step_record must persist a pending record"),
            };
            self.events
                .lock()
                .expect("events mutex poisoned")
                .push(Event::AppendStepRecord {
                    iteration_id,
                    sequence_no: step_sequence_no,
                    record_id,
                });
            if let Some(result) = self
                .append_step_record_result
                .lock()
                .expect("append_step_record_result mutex poisoned")
                .pop_front()
            {
                return result;
            }
            Ok(())
        }

        async fn finish_step_record(
            &self,
            _run: &RunState,
            record_id: StepRecordId,
            finished_record: &FinishedStepRecord,
        ) -> Result<(), RunRepositoryError> {
            self.events
                .lock()
                .expect("events mutex poisoned")
                .push(Event::FinishStepRecord {
                    record_id,
                    is_ok: finished_record.result.is_ok(),
                });
            if let Some(result) = self
                .finish_step_record_result
                .lock()
                .expect("finish_step_record_result mutex poisoned")
                .pop_front()
            {
                return result;
            }
            Ok(())
        }
    }

    fn user_request(query: &str) -> UserRequest {
        UserRequest {
            query: query.to_string(),
        }
    }

    fn final_output(problem_understanding: &str) -> ResponseValidationAndNormalizationOutput {
        ResponseValidationAndNormalizationOutput {
            response: DiagnosticResponse {
                problem_understanding: problem_understanding.to_string(),
                similar_practical_context: "ctx".to_string(),
                active_hypotheses: vec!["h1".to_string()],
                first_check: "check".to_string(),
                result_interpretation: DiagnosticResultInterpretation {
                    supports_primary_if: "supports".to_string(),
                    supports_competing_if: "competes".to_string(),
                    inconclusive_if: Some("maybe".to_string()),
                },
                competing_interpretation: None,
            },
        }
    }

    fn normalized_request(query: &str) -> NormalizedUserRequest {
        NormalizedUserRequest {
            query: query.to_string(),
            input_token_count: 3,
        }
    }

    fn invalid_repo_state(message: &str) -> RunRepositoryError {
        RunRepositoryError::InvalidRunState {
            message: message.to_string(),
        }
    }

    fn run_with_single_iteration(query: &str) -> RunState {
        let mut state = RunState::new();
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer
                .begin_iteration(user_request(query))
                .expect("begin_iteration must succeed");
        }
        state
    }

    #[tokio::test]
    async fn run_creates_run_and_persists_first_iteration_before_finishing() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let policy = FakePolicy::from_decisions(vec![Ok(PolicyTransition::FinishWithResult {
            result: final_output("done"),
        })]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(None, Arc::clone(&events));

        let outcome = run_impl(&policy, &executor, &repo, user_request("first"))
            .await
            .expect("run must succeed");

        assert!(matches!(outcome, RunOutcome::Finished { .. }));
        assert!(matches!(
            &events.lock().expect("events mutex poisoned")[..],
            [Event::CreateRun, Event::AppendIteration { sequence_no: 0, .. }]
        ));
    }

    #[tokio::test]
    async fn resume_loads_existing_run_without_appending_iteration() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let state = run_with_single_iteration("persisted");
        let run_id = state.run_id;
        let policy = FakePolicy::from_decisions(vec![Ok(PolicyTransition::FinishWithResult {
            result: final_output("done"),
        })]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(Some(state), Arc::clone(&events));

        let outcome = resume_impl(&policy, &executor, &repo, run_id)
            .await
            .expect("resume must succeed");

        assert!(matches!(outcome, RunOutcome::Finished { .. }));
        assert!(matches!(
            &events.lock().expect("events mutex poisoned")[..],
            [Event::LoadRun(id)] if *id == run_id
        ));
    }

    #[tokio::test]
    async fn resume_with_input_loads_run_and_appends_exactly_one_new_iteration() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let state = run_with_single_iteration("persisted");
        let run_id = state.run_id;
        let old_iteration_count = state.iterations.len();
        let policy = FakePolicy::from_decisions(vec![Ok(PolicyTransition::FinishWithResult {
            result: final_output("done"),
        })]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(Some(state), Arc::clone(&events));

        let outcome = resume_with_input_impl(
            &policy,
            &executor,
            &repo,
            run_id,
            user_request("fresh"),
        )
        .await
        .expect("resume_with_input must succeed");

        assert!(matches!(outcome, RunOutcome::Finished { .. }));
        assert!(matches!(
            &events.lock().expect("events mutex poisoned")[..],
            [Event::LoadRun(id), Event::AppendIteration { sequence_no, .. }]
            if *id == run_id && *sequence_no == old_iteration_count as u64
        ));
    }

    #[tokio::test]
    async fn load_existing_run_maps_absence_to_run_not_found() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let run_id = RunId(Uuid::new_v4());
        let repo = FakeRepository::new(None, Arc::clone(&events));

        let err = load_existing_run_impl(&repo, run_id)
            .await
            .expect_err("missing run must fail");

        assert!(matches!(err, OrchestratorError::RunNotFound { run_id: id } if id == run_id));
    }

    #[tokio::test]
    async fn drive_to_outcome_persists_pending_executes_and_persists_finished_success() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = run_with_single_iteration("persisted");
        let policy = FakePolicy::from_decisions(vec![
            Ok(PolicyTransition::ExecuteStep {
                step: StepKind::InputNormalization,
            }),
            Ok(PolicyTransition::FinishWithResult {
                result: final_output("done"),
            }),
        ]);
        let executor = FakeExecutor::new(
            vec![Ok(StepResultEnvelope::InputNormalization(normalized_request(
                "persisted",
            )))],
            Arc::clone(&events),
        );
        let repo = FakeRepository::new(None, Arc::clone(&events));

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state)
            .await
            .expect("drive_to_outcome must succeed");

        assert!(matches!(outcome, RunOutcome::Finished { .. }));

        let events = events.lock().expect("events mutex poisoned");
        assert!(matches!(events[0], Event::AppendStepRecord { sequence_no: 1, .. }));
        assert_eq!(events[1], Event::ExecuteStep { step: StepKind::InputNormalization });
        assert!(matches!(events[2], Event::FinishStepRecord { is_ok: true, .. }));

        let pending_record_id = match events[0] {
            Event::AppendStepRecord { record_id, .. } => record_id,
            _ => unreachable!(),
        };
        let finished_record_id = match events[2] {
            Event::FinishStepRecord { record_id, .. } => record_id,
            _ => unreachable!(),
        };
        assert_eq!(pending_record_id, finished_record_id);
    }

    #[tokio::test]
    async fn drive_to_outcome_records_failure_and_returns_failed_outcome() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = run_with_single_iteration("persisted");
        let step_error = StepError::Unexpected {
            message: "boom".to_string(),
        };
        let policy = FakePolicy::from_decisions(vec![
            Ok(PolicyTransition::ExecuteStep {
                step: StepKind::InputNormalization,
            }),
            Ok(PolicyTransition::FinishWithError {
                error: step_error.clone(),
            }),
        ]);
        let executor = FakeExecutor::new(vec![Err(step_error.clone())], Arc::clone(&events));
        let repo = FakeRepository::new(None, Arc::clone(&events));

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state)
            .await
            .expect("drive_to_outcome must succeed");

        assert_eq!(
            outcome,
            RunOutcome::Failed {
                run_id: state.run_id,
                error: step_error,
            }
        );
        assert!(matches!(
            events.lock().expect("events mutex poisoned")[2],
            Event::FinishStepRecord { is_ok: false, .. }
        ));
        assert_eq!(state.status, RunStatus::Error);
    }

    #[tokio::test]
    async fn resume_preserves_successful_finished_steps_in_current_iteration() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = run_with_single_iteration("persisted");
        {
            let mut writer = RunStateWriter::new(&mut state);
            let mut iteration = writer
                .current_iteration()
                .expect("current_iteration must exist");
            let pending = iteration
                .begin_step(StepKind::InputNormalization)
                .expect("begin_step must succeed");
            pending
                .record_success(StepResultEnvelope::InputNormalization(normalized_request(
                    "persisted",
                )))
                .expect("record_success must succeed");
        }

        let run_id = state.run_id;
        let policy = FakePolicy::from_decisions(vec![Ok(PolicyTransition::FinishWithResult {
            result: final_output("done"),
        })]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(Some(state.clone()), Arc::clone(&events));

        let outcome = resume_impl(&policy, &executor, &repo, run_id)
            .await
            .expect("resume must succeed");

        assert!(matches!(outcome, RunOutcome::Finished { .. }));
        assert_eq!(
            state.iterations.last().expect("iteration").step_records.len(),
            2
        );
        assert!(events
            .lock()
            .expect("events mutex poisoned")
            .iter()
            .all(|event| !matches!(event, Event::AppendIteration { .. })));
    }

    #[tokio::test]
    async fn run_maps_create_run_error() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let policy = FakePolicy::from_decisions(vec![]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(None, Arc::clone(&events)).with_create_run_results(vec![
            Err(invalid_repo_state("create failed")),
        ]);

        let err = run_impl(&policy, &executor, &repo, user_request("first"))
            .await
            .expect_err("run must fail");

        assert!(matches!(
            err,
            OrchestratorError::Repository(RunRepositoryError::InvalidRunState { .. })
        ));
    }

    #[tokio::test]
    async fn run_maps_append_iteration_error() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let policy = FakePolicy::from_decisions(vec![]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(None, Arc::clone(&events))
            .with_append_iteration_results(vec![Err(invalid_repo_state("append iteration failed"))]);

        let err = run_impl(&policy, &executor, &repo, user_request("first"))
            .await
            .expect_err("run must fail");

        assert!(matches!(
            err,
            OrchestratorError::Repository(RunRepositoryError::InvalidRunState { .. })
        ));
    }

    #[tokio::test]
    async fn resume_maps_load_run_error() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let run_id = RunId(Uuid::new_v4());
        let policy = FakePolicy::from_decisions(vec![]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(None, Arc::clone(&events)).with_load_run_results(vec![
            Err(invalid_repo_state("load failed")),
        ]);

        let err = resume_impl(&policy, &executor, &repo, run_id)
            .await
            .expect_err("resume must fail");

        assert!(matches!(
            err,
            OrchestratorError::Repository(RunRepositoryError::InvalidRunState { .. })
        ));
    }

    #[tokio::test]
    async fn resume_with_input_maps_append_iteration_error() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let state = run_with_single_iteration("persisted");
        let run_id = state.run_id;
        let policy = FakePolicy::from_decisions(vec![]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(Some(state), Arc::clone(&events))
            .with_append_iteration_results(vec![Err(invalid_repo_state("append iteration failed"))]);

        let err = resume_with_input_impl(
            &policy,
            &executor,
            &repo,
            run_id,
            user_request("fresh"),
        )
        .await
        .expect_err("resume_with_input must fail");

        assert!(matches!(
            err,
            OrchestratorError::Repository(RunRepositoryError::InvalidRunState { .. })
        ));
    }

    #[tokio::test]
    async fn drive_to_outcome_maps_policy_error() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = run_with_single_iteration("persisted");
        let policy = FakePolicy::from_decisions(vec![Err(PolicyError::MissingUserInput)]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(None, Arc::clone(&events));

        let err = drive_to_outcome_impl(&policy, &executor, &repo, &mut state)
            .await
            .expect_err("policy failure must bubble up");

        assert!(matches!(err, OrchestratorError::Policy(PolicyError::MissingUserInput)));
        assert!(events.lock().expect("events mutex poisoned").is_empty());
    }

    #[tokio::test]
    async fn drive_to_outcome_maps_append_step_record_error() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = run_with_single_iteration("persisted");
        let policy = FakePolicy::from_decisions(vec![Ok(PolicyTransition::ExecuteStep {
            step: StepKind::InputNormalization,
        })]);
        let executor = FakeExecutor::new(
            vec![Ok(StepResultEnvelope::InputNormalization(normalized_request(
                "persisted",
            )))],
            Arc::clone(&events),
        );
        let repo = FakeRepository::new(None, Arc::clone(&events))
            .with_append_step_record_results(vec![Err(invalid_repo_state("append step failed"))]);

        let err = drive_to_outcome_impl(&policy, &executor, &repo, &mut state)
            .await
            .expect_err("append_step_record failure must bubble up");

        assert!(matches!(
            err,
            OrchestratorError::Repository(RunRepositoryError::InvalidRunState { .. })
        ));
    }

    #[tokio::test]
    async fn drive_to_outcome_maps_finish_step_record_error() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = run_with_single_iteration("persisted");
        let policy = FakePolicy::from_decisions(vec![Ok(PolicyTransition::ExecuteStep {
            step: StepKind::InputNormalization,
        })]);
        let executor = FakeExecutor::new(
            vec![Ok(StepResultEnvelope::InputNormalization(normalized_request(
                "persisted",
            )))],
            Arc::clone(&events),
        );
        let repo = FakeRepository::new(None, Arc::clone(&events))
            .with_finish_step_record_results(vec![Err(invalid_repo_state("finish step failed"))]);

        let err = drive_to_outcome_impl(&policy, &executor, &repo, &mut state)
            .await
            .expect_err("finish_step_record failure must bubble up");

        assert!(matches!(
            err,
            OrchestratorError::Repository(RunRepositoryError::InvalidRunState { .. })
        ));
    }

    #[tokio::test]
    async fn finish_with_result_does_not_call_executor_or_step_writes() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = run_with_single_iteration("persisted");
        let policy = FakePolicy::from_decisions(vec![Ok(PolicyTransition::FinishWithResult {
            result: final_output("done"),
        })]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(None, Arc::clone(&events));

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state)
            .await
            .expect("must succeed");

        assert!(matches!(outcome, RunOutcome::Finished { .. }));
        assert!(events.lock().expect("events mutex poisoned").is_empty());
    }

    #[tokio::test]
    async fn finish_with_error_does_not_call_executor() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = run_with_single_iteration("persisted");
        let policy = FakePolicy::from_decisions(vec![Ok(PolicyTransition::FinishWithError {
            error: StepError::Unexpected {
                message: "recorded".to_string(),
            },
        })]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(None, Arc::clone(&events));

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state)
            .await
            .expect("must succeed");

        assert!(matches!(outcome, RunOutcome::Failed { .. }));
        assert!(events.lock().expect("events mutex poisoned").is_empty());
    }

    #[tokio::test]
    async fn drive_to_outcome_can_execute_multiple_steps_before_finishing() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = run_with_single_iteration("persisted");
        let policy = FakePolicy::from_decisions(vec![
            Ok(PolicyTransition::ExecuteStep {
                step: StepKind::InputNormalization,
            }),
            Ok(PolicyTransition::ExecuteStep {
                step: StepKind::QueryStructuring,
            }),
            Ok(PolicyTransition::FinishWithResult {
                result: final_output("done"),
            }),
        ]);
        let executor = FakeExecutor::new(
            vec![
                Ok(StepResultEnvelope::InputNormalization(normalized_request(
                    "persisted",
                ))),
                Ok(StepResultEnvelope::QueryStructuring(
                    crate::shared_types::QueryStructuringOutput {
                        structured_query: crate::shared_types::StructuredUserQuery {
                            intent: "debug".to_string(),
                            scenario: "persisted".to_string(),
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
                            confidence: crate::shared_types::StructuredUserQueryConfidence::High,
                        },
                        token_usage: crate::shared_types::ModelTokenUsage {
                            prompt_tokens: Some(1),
                            completion_tokens: Some(1),
                            total_tokens: Some(2),
                        },
                    },
                )),
            ],
            Arc::clone(&events),
        );
        let repo = FakeRepository::new(None, Arc::clone(&events));

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state)
            .await
            .expect("must succeed");

        assert!(matches!(outcome, RunOutcome::Finished { .. }));
        let events = events.lock().expect("events mutex poisoned");
        let executed_steps: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::ExecuteStep { step } => Some(*step),
                _ => None,
            })
            .collect();
        assert_eq!(
            executed_steps,
            vec![StepKind::InputNormalization, StepKind::QueryStructuring]
        );
    }
}
