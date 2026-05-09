use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use thiserror::Error;

use crate::orchestrator::run_repository::{RunRepository, RunRepositoryError};
use crate::orchestrator::run_state::apply::{RunStateWriter, StateApplyError};
use crate::orchestrator::run_state::model::{
    FinishedStepRecord, RunId, RunIteration, RunIterationStatus, RunState, RunStatus, StepError,
    StepKind, StepRecord, StepResultEnvelope,
};
use crate::orchestrator::run_state::view::RunStateView;
use crate::orchestrator::step_executor::StepExecutor;
use crate::orchestrator::transition_policy::{
    PolicyError, PolicyTransition, TransitionPolicy,
};
use crate::shared_types::{
    Context, OpenInferenceContext, ResponseValidationAndNormalizationOutput, RunConfigSnapshot,
    UserRequest,
};

pub struct Orchestrator<P> {
    policy: P,
    executor: StepExecutor,
    run_repository: RunRepository,
    config_snapshot: Option<RunConfigSnapshot>,
    run_otel_contexts: Mutex<HashMap<RunId, opentelemetry::Context>>,
}

impl<P: std::fmt::Debug> std::fmt::Debug for Orchestrator<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Orchestrator")
            .field("policy", &self.policy)
            .field("executor", &self.executor)
            .field("run_repository", &self.run_repository)
            .field("config_snapshot", &self.config_snapshot)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunOutcome {
    Finished {
        run_id: RunId,
        result: ResponseValidationAndNormalizationOutput,
    },
    WaitingForUser {
        run_id: RunId,
        follow_up_questions: Vec<String>,
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

    #[error("run {run_id:?} is waiting for user input; use resume_with_input")]
    WaitingForUserRequiresNewInput { run_id: RunId },
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
            config_snapshot: None,
            run_otel_contexts: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_config_snapshot(mut self, snapshot: RunConfigSnapshot) -> Self {
        self.config_snapshot = Some(snapshot);
        self
    }

    pub async fn run(&self, user_input: UserRequest) -> Result<RunOutcome, OrchestratorError> {
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        let mut state = RunState::new();
        let run_id = state.run_id;
        let run_id_str = run_id.0.to_string();
        let root_span = crate::observability::run_span(&run_id_str, "run");
        let _root_entered = root_span.enter();
        root_span.record("input.value", user_input.query.as_str());
        root_span.record("input.mime_type", "text/plain");
        self.run_otel_contexts.lock().unwrap().insert(run_id, root_span.context());
        let outcome = self.run_body(user_input, &mut state).await;
        record_run_outcome(&root_span, &outcome, Some(&state));
        outcome
    }

    async fn run_body(
        &self,
        user_input: UserRequest,
        state: &mut RunState,
    ) -> Result<RunOutcome, OrchestratorError> {
        self.run_repository.create_run(state).await?;
        {
            let mut writer = RunStateWriter::new(state);
            writer.begin_iteration(user_input)?;
        }
        if let Some(snapshot) = &self.config_snapshot {
            if let Some(iter) = state.iterations.last_mut() {
                iter.config_snapshot = Some(snapshot.clone());
            }
        }
        let iteration_sequence_no = RunStateView::new(state).iteration_count() as u64 - 1;
        let iteration_id = RunStateView::new(state)
            .last_iteration()
            .expect("begin_iteration must create a current iteration")
            .iteration_id();
        let iteration = RunStateView::new(state)
            .iteration(iteration_id)
            .expect("last_iteration view must reference a stored iteration");
        self.run_repository
            .append_iteration(state, iteration_sequence_no, iteration)
            .await?;
        self.drive_to_outcome(state, None).await
    }

    pub async fn resume(&self, run_id: RunId) -> Result<RunOutcome, OrchestratorError> {
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        let run_id_str = run_id.0.to_string();
        let parent_ctx = self.run_otel_contexts.lock().unwrap().get(&run_id).cloned();
        let root_span = crate::observability::run_span(&run_id_str, "resume");
        if let Some(ctx) = parent_ctx {
            let _ = root_span.set_parent(ctx);
        }
        let _root_entered = root_span.enter();
        let mut state = match self.load_existing_run(run_id).await {
            Err(e) => {
                crate::observability::record_error(
                    &root_span,
                    orchestrator_error_type(&e),
                    &e.to_string(),
                );
                root_span.record("status", "error");
                root_span.record("run.outcome", "failure");
                return Err(e);
            }
            Ok(s) => s,
        };
        if state.status == RunStatus::WaitingForUser {
            return Err(OrchestratorError::WaitingForUserRequiresNewInput { run_id });
        }
        let outcome = self.drive_to_outcome(&mut state, None).await;
        record_run_outcome(&root_span, &outcome, Some(&state));
        outcome
    }

    pub async fn resume_with_input(
        &self,
        run_id: RunId,
        user_input: UserRequest,
    ) -> Result<RunOutcome, OrchestratorError> {
        let parent_ctx = self.run_otel_contexts.lock().unwrap().get(&run_id).cloned();
        let (outcome, _state) = self.resume_with_input_body(run_id, user_input, parent_ctx).await;
        outcome
    }

    async fn resume_with_input_body(
        &self,
        run_id: RunId,
        user_input: UserRequest,
        parent_ctx: Option<opentelemetry::Context>,
    ) -> (Result<RunOutcome, OrchestratorError>, Option<RunState>) {
        let mut state = match self.load_existing_run(run_id).await {
            Err(e) => return (Err(e), None),
            Ok(s) => s,
        };
        {
            let mut writer = RunStateWriter::new(&mut state);
            if let Err(e) = writer.begin_iteration(user_input) {
                return (Err(OrchestratorError::from(e)), Some(state));
            }
        }
        let iteration_sequence_no = RunStateView::new(&state).iteration_count() as u64 - 1;
        let iteration_id = RunStateView::new(&state)
            .last_iteration()
            .expect("begin_iteration must create a current iteration")
            .iteration_id();
        let iteration = RunStateView::new(&state)
            .iteration(iteration_id)
            .expect("last_iteration view must reference a stored iteration");
        if let Err(e) = self
            .run_repository
            .append_iteration(&state, iteration_sequence_no, iteration)
            .await
        {
            return (Err(OrchestratorError::from(e)), Some(state));
        }
        let outcome = self.drive_to_outcome(&mut state, parent_ctx).await;
        (outcome, Some(state))
    }

    async fn load_existing_run(&self, run_id: RunId) -> Result<RunState, OrchestratorError> {
        load_existing_run_impl(&self.run_repository, run_id).await
    }

    async fn drive_to_outcome(
        &self,
        state: &mut RunState,
        parent_ctx: Option<opentelemetry::Context>,
    ) -> Result<RunOutcome, OrchestratorError> {
        drive_to_outcome_impl(
            &self.policy,
            &self.executor,
            &self.run_repository,
            state,
            parent_ctx.as_ref(),
        )
        .await
    }
}

#[async_trait(?Send)]
trait ExecutorLike {
    async fn execute_step(
        &self,
        step: StepKind,
        state: RunStateView<'_>,
        context: &Context,
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

    async fn update_iteration_status(
        &self,
        run: &RunState,
        iteration_id: crate::orchestrator::run_state::model::RunIterationId,
        status: RunIterationStatus,
    ) -> Result<(), RunRepositoryError>;
}

#[async_trait(?Send)]
impl ExecutorLike for StepExecutor {
    async fn execute_step(
        &self,
        step: StepKind,
        state: RunStateView<'_>,
        context: &Context,
    ) -> Result<StepResultEnvelope, StepError> {
        self.execute_with_context(step, state, context).await
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

    async fn update_iteration_status(
        &self,
        run: &RunState,
        iteration_id: crate::orchestrator::run_state::model::RunIterationId,
        status: RunIterationStatus,
    ) -> Result<(), RunRepositoryError> {
        self.update_iteration_status(run, iteration_id, status).await
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

    drive_to_outcome_impl(policy, executor, run_repository, &mut state, None).await
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
    if state.status == RunStatus::WaitingForUser {
        return Err(OrchestratorError::WaitingForUserRequiresNewInput { run_id });
    }
    drive_to_outcome_impl(policy, executor, run_repository, &mut state, None).await
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

    drive_to_outcome_impl(policy, executor, run_repository, &mut state, None).await
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
    parent_ctx: Option<&opentelemetry::Context>,
) -> Result<RunOutcome, OrchestratorError>
where
    P: TransitionPolicy,
    E: ExecutorLike + Sync,
    R: RepositoryLike + Sync,
{
    let run_id_str = state.run_id.0.to_string();
    let (iter_span, iter_id_str) = match state.iterations.last() {
        Some(it) => {
            let iter_id = it.iteration_id.0.to_string();
            let seq = (state.iterations.len() - 1) as u64;
            let span =
                crate::observability::iteration_span(&run_id_str, &iter_id, seq, parent_ctx);
            (span, iter_id)
        }
        None => (tracing::Span::none(), String::new()),
    };
    let _iter_entered = iter_span.enter();
    let iteration_sequence_no = state.iterations.len().saturating_sub(1) as u64;
    let context = Context::new(
        OpenInferenceContext {
            root_span: crate::observability::oi_iteration_chain_span(
                &iter_span,
                &run_id_str,
                &iter_id_str,
                iteration_sequence_no,
            ),
        },
        current_iteration_golden_question(state),
    );

    let outcome = drive_to_outcome_loop(
        policy,
        executor,
        run_repository,
        state,
        &run_id_str,
        &iter_id_str,
        &context,
    )
    .await;

    match &outcome {
        Ok(RunOutcome::Finished { result, .. }) => {
            iter_span.record("status", "ok");
            context.open_inference.root_span.record("status", "ok");
            context.open_inference.root_span.record("run.outcome", "success");
            if let Ok(output_json) = serde_json::to_string(result) {
                context
                    .open_inference
                    .root_span
                    .record("output.value", output_json.as_str());
                context
                    .open_inference
                    .root_span
                    .record("output.mime_type", "application/json");
            }
        }
        Ok(RunOutcome::WaitingForUser { .. }) => {
            iter_span.record("status", "ok");
            context.open_inference.root_span.record("run.outcome", "waiting_for_user");
        }
        Ok(RunOutcome::Failed { error, .. }) => {
            iter_span.record("status", "error");
            context.open_inference.root_span.record("run.outcome", "failure");
            crate::observability::record_error(
                &iter_span,
                step_error_type(error),
                &error.to_string(),
            );
            crate::observability::record_error(
                &context.open_inference.root_span,
                step_error_type(error),
                &error.to_string(),
            );
        }
        Err(error) => {
            iter_span.record("status", "error");
            context.open_inference.root_span.record("run.outcome", "failure");
            crate::observability::record_error(
                &iter_span,
                orchestrator_error_type(error),
                &error.to_string(),
            );
            crate::observability::record_error(
                &context.open_inference.root_span,
                orchestrator_error_type(error),
                &error.to_string(),
            );
        }
    }

    outcome
}

fn current_iteration_golden_question(state: &RunState) -> Option<crate::shared_types::GoldenQuestion> {
    let iteration = RunStateView::new(state).last_iteration()?;
    let user_input = iteration.finished_step(StepKind::UserInputReceived)?;
    match user_input.result() {
        Ok(StepResultEnvelope::UserInputReceived(request)) => request.golden_question.clone(),
        _ => None,
    }
}

async fn drive_to_outcome_loop<P, E, R>(
    policy: &P,
    executor: &E,
    run_repository: &R,
    state: &mut RunState,
    run_id_str: &str,
    iter_id_str: &str,
    context: &Context,
) -> Result<RunOutcome, OrchestratorError>
where
    P: TransitionPolicy,
    E: ExecutorLike + Sync,
    R: RepositoryLike + Sync,
{
    loop {
        let (finished_steps_count, pending_step_present, last_finished_step_kind) = {
            let iter = RunStateView::new(state).last_iteration();
            let fsc = iter.map_or(0, |it| it.finished_steps().count() as u64);
            let psp = iter.map_or(false, |it| it.pending_step().is_some());
            let lfs = iter
                .and_then(|it| it.finished_steps().next_back())
                .map(|s| s.kind().as_ref().to_string());
            (fsc, psp, lfs)
        };

        let policy_span = crate::observability::policy_transition_span(
            run_id_str,
            iter_id_str,
            finished_steps_count,
            pending_step_present,
            last_finished_step_kind.as_deref(),
        );

        let decision = {
            let _policy_entered = policy_span.enter();
            let decision = policy.next_transition(RunStateView::new(state));
            match &decision {
                Ok(PolicyTransition::ExecuteStep { step }) => {
                    let next_seq = state
                        .iterations
                        .last()
                        .map(|it| it.step_records.len() as u64)
                        .unwrap_or(0);
                    policy_span.record("status", "ok");
                    policy_span.record("transition.kind", "ExecuteStep");
                    policy_span.record("step.kind", step.as_ref());
                    policy_span.record("step.sequence_no", next_seq);
                }
                Ok(PolicyTransition::FinishWithResult { .. }) => {
                    policy_span.record("status", "ok");
                    policy_span.record("transition.kind", "FinishWithResult");
                }
                Ok(PolicyTransition::FinishWithError { .. }) => {
                    policy_span.record("status", "ok");
                    policy_span.record("transition.kind", "FinishWithError");
                }
                Ok(PolicyTransition::WaitForUser { .. }) => {
                    policy_span.record("status", "ok");
                    policy_span.record("transition.kind", "WaitForUser");
                }
                Err(e) => {
                    crate::observability::record_error(
                        &policy_span,
                        "PolicyError",
                        &e.to_string(),
                    );
                }
            }
            decision
        }?;

        match decision {
            PolicyTransition::ExecuteStep { step } => {
                let step_kind_str = step.as_ref().to_string();
                let step_span =
                    crate::observability::step_span(run_id_str, iter_id_str, &step_kind_str);
                let _step_entered = step_span.enter();

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

                step_span.record("step.sequence_no", step_sequence_no);
                let record_id_str = record_id.0.to_string();
                step_span.record("record.id", &record_id_str as &str);

                run_repository
                    .append_step_record(state, iteration_id, step_sequence_no, &step_record)
                    .await
                    .map_err(|e| {
                        let msg = e.to_string();
                        crate::observability::record_error(
                            &step_span,
                            "OrchestratorError.Repository",
                            &msg,
                        );
                        step_span.record("status", "error");
                        OrchestratorError::from(e)
                    })?;

                let execution_result = executor
                    .execute_step(step, RunStateView::new(state), context)
                    .await;
                let step_error_info = execution_result
                    .as_ref()
                    .err()
                    .map(|e| (step_error_type(e), e.to_string()));
                let step_execution_failed = execution_result.is_err();

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
                    .await
                    .map_err(|e| {
                        let msg = e.to_string();
                        crate::observability::record_error(
                            &step_span,
                            "OrchestratorError.Repository",
                            &msg,
                        );
                        step_span.record("status", "error");
                        if step_execution_failed {
                            step_span.record("step.outcome", "failure");
                        }
                        OrchestratorError::from(e)
                    })?;

                if step_execution_failed {
                    if let Some((et, em)) = step_error_info {
                        crate::observability::record_error(&step_span, et, &em);
                    }
                    step_span.record("step.outcome", "failure");
                    // record_failure() set iteration.status = FinishedWithError in memory.
                    // Persist it now so the iteration is durable before policy re-reads state.
                    run_repository
                        .update_iteration_status(
                            state,
                            iteration_id,
                            RunIterationStatus::FinishedWithError,
                        )
                        .await?;
                } else {
                    step_span.record("step.outcome", "success");
                    step_span.record("status", "ok");
                }
            }
            PolicyTransition::FinishWithResult { result } => {
                let iteration_id = {
                    let mut writer = RunStateWriter::new(state);
                    let current = writer.current_iteration()?;
                    let iteration_id = current.iteration_id();
                    writer.finish_current_iteration_success()?;
                    iteration_id
                };
                run_repository
                    .update_iteration_status(
                        state,
                        iteration_id,
                        RunIterationStatus::FinishedWithSuccess,
                    )
                    .await?;
                return Ok(RunOutcome::Finished {
                    run_id: state.run_id,
                    result,
                });
            }
            PolicyTransition::FinishWithError { error } => {
                // The iteration is already FinishedWithError in memory (set by record_failure).
                // Get iteration_id and persist the status + run header.
                let iteration_id = RunStateView::new(state)
                    .last_iteration()
                    .expect("FinishWithError requires current iteration")
                    .iteration_id();
                run_repository
                    .update_iteration_status(
                        state,
                        iteration_id,
                        RunIterationStatus::FinishedWithError,
                    )
                    .await?;
                return Ok(RunOutcome::Failed {
                    run_id: state.run_id,
                    error,
                });
            }
            PolicyTransition::WaitForUser { follow_up_questions } => {
                let iteration_id = {
                    let mut writer = RunStateWriter::new(state);
                    writer.wait_for_user()?;
                    RunStateView::new(state)
                        .last_iteration()
                        .expect("wait_for_user requires current iteration")
                        .iteration_id()
                };
                run_repository
                    .update_iteration_status(
                        state,
                        iteration_id,
                        RunIterationStatus::FinishedWithWaitInput,
                    )
                    .await?;
                return Ok(RunOutcome::WaitingForUser {
                    run_id: state.run_id,
                    follow_up_questions,
                });
            }
        }
    }
}

fn record_run_outcome(
    span: &tracing::Span,
    outcome: &Result<RunOutcome, OrchestratorError>,
    state: Option<&RunState>,
) {
    match outcome {
        Ok(RunOutcome::Finished { result, .. }) => {
            span.record("status", "ok");
            span.record("run.outcome", "success");
            span.record("terminal.transition", "FinishWithResult");
            if let Ok(output_json) = serde_json::to_string(result) {
                span.record("output.value", output_json.as_str());
                span.record("output.mime_type", "application/json");
            }
        }
        Ok(RunOutcome::WaitingForUser { follow_up_questions, .. }) => {
            span.record("status", "ok");
            span.record("run.outcome", "waiting_for_user");
            span.record("terminal.transition", "WaitForUser");
            if let Ok(output_json) = serde_json::to_string(follow_up_questions) {
                span.record("output.value", output_json.as_str());
                span.record("output.mime_type", "application/json");
            }
        }
        Ok(RunOutcome::Failed { error, .. }) => {
            span.record("status", "error");
            span.record("run.outcome", "failure");
            span.record("terminal.transition", "FinishWithError");
            if let Some(s) = state {
                record_failed_step_kind(span, s);
            }
            crate::observability::record_error(span, step_error_type(error), &error.to_string());
        }
        Err(e) => {
            span.record("status", "error");
            span.record("run.outcome", "failure");
            if let Some(s) = state {
                record_failed_step_kind(span, s);
            }
            crate::observability::record_error(span, orchestrator_error_type(e), &e.to_string());
        }
    }
}

fn record_failed_step_kind(span: &tracing::Span, state: &RunState) {
    let kind = state.iterations.last().and_then(|it| {
        it.step_records
            .iter()
            .rev()
            .find_map(|r| match r {
                StepRecord::Finished(f) if f.result.is_err() => {
                    Some(f.step.as_ref().to_string())
                }
                _ => None,
            })
            .or_else(|| {
                it.step_records.iter().find_map(|r| match r {
                    StepRecord::Pending(p) => Some(p.step.as_ref().to_string()),
                    _ => None,
                })
            })
    });
    if let Some(k) = kind {
        span.record("failed_step.kind", &k as &str);
    }
}

fn step_error_type(e: &StepError) -> &'static str {
    match e {
        StepError::MissingRequiredInput { .. } => "StepError.MissingRequiredInput",
        StepError::InvalidState { .. } => "StepError.InvalidState",
        StepError::InputNormalization(_) => "StepError.InputNormalization",
        StepError::QueryStructuring(_) => "StepError.QueryStructuring",
        StepError::CandidateCardRetrieval(_) => "StepError.CandidateCardRetrieval",
        StepError::CardHydration(_) => "StepError.CardHydration",
        StepError::IncidentEvidenceRetrieval(_) => "StepError.IncidentEvidenceRetrieval",
        StepError::TheoryEvidenceRetrieval(_) => "StepError.TheoryEvidenceRetrieval",
        StepError::PromptContextAssembly(_) => "StepError.PromptContextAssembly",
        StepError::LlmStructuredGeneration(_) => "StepError.LlmStructuredGeneration",
        StepError::ResponseValidationAndNormalization(_) => {
            "StepError.ResponseValidationAndNormalization"
        }
        StepError::CardBranchReranking(_) => "StepError.CardBranchReranking",
        StepError::DiagnosticUpdatePromptContextAssembly(_) => {
            "StepError.DiagnosticUpdatePromptContextAssembly"
        }
        StepError::ObservationBoundaryResolver(_) => "StepError.ObservationBoundaryResolver",
        StepError::ObservationExtraction(_) => "StepError.ObservationExtraction",
        StepError::InformationAdequacy(_) => "StepError.InformationAdequacy",
        StepError::ExternalDependency { .. } => "StepError.ExternalDependency",
        StepError::Unexpected { .. } => "StepError.Unexpected",
    }
}

fn orchestrator_error_type(e: &OrchestratorError) -> &'static str {
    match e {
        OrchestratorError::RunNotFound { .. } => "OrchestratorError.RunNotFound",
        OrchestratorError::Policy(_) => "OrchestratorError.Policy",
        OrchestratorError::StateApply(_) => "OrchestratorError.StateApply",
        OrchestratorError::Repository(_) => "OrchestratorError.Repository",
        OrchestratorError::MissingPendingStep { .. } => "OrchestratorError.MissingPendingStep",
        OrchestratorError::WaitingForUserRequiresNewInput { .. } => {
            "OrchestratorError.WaitingForUserRequiresNewInput"
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
        UpdateIterationStatus { iteration_id: RunIterationId, status: RunIterationStatus },
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
            _context: &Context,
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
        update_iteration_status_result: Mutex<VecDeque<Result<(), RunRepositoryError>>>,
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
                update_iteration_status_result: Mutex::new(VecDeque::new()),
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

        async fn update_iteration_status(
            &self,
            _run: &RunState,
            iteration_id: RunIterationId,
            status: RunIterationStatus,
        ) -> Result<(), RunRepositoryError> {
            self.events
                .lock()
                .expect("events mutex poisoned")
                .push(Event::UpdateIterationStatus { iteration_id, status });
            if let Some(result) = self
                .update_iteration_status_result
                .lock()
                .expect("update_iteration_status_result mutex poisoned")
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
            golden_question: None,
        }
    }

    fn final_output(problem_understanding: &str) -> ResponseValidationAndNormalizationOutput {
        use crate::shared_types::{Confidence, Hypothesis, HypothesisEvidenceSource, HypothesisId, HypothesisStatus};
        use uuid::Uuid;
        ResponseValidationAndNormalizationOutput {
            response: DiagnosticResponse {
                problem_understanding: problem_understanding.to_string(),
                similar_practical_context: "ctx".to_string(),
                hypotheses: vec![
                    Hypothesis {
                        id: HypothesisId(Uuid::new_v4()),
                        text: "h1".to_string(),
                        status: HypothesisStatus::Active,
                        source: HypothesisEvidenceSource::PrimaryIncident,
                        confidence: Confidence::Medium,
                    },
                    Hypothesis {
                        id: HypothesisId(Uuid::new_v4()),
                        text: "h2".to_string(),
                        status: HypothesisStatus::Active,
                        source: HypothesisEvidenceSource::PrimaryIncident,
                        confidence: Confidence::Low,
                    },
                ],
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
        // CreateRun → AppendIteration → UpdateIterationStatus(FinishedWithSuccess)
        let evts = events.lock().expect("events mutex poisoned");
        assert!(matches!(evts[0], Event::CreateRun));
        assert!(matches!(evts[1], Event::AppendIteration { sequence_no: 0, .. }));
        assert!(matches!(evts[2], Event::UpdateIterationStatus { status: RunIterationStatus::FinishedWithSuccess, .. }));
        assert_eq!(evts.len(), 3);
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
        // LoadRun → UpdateIterationStatus(FinishedWithSuccess)
        let evts = events.lock().expect("events mutex poisoned");
        assert!(matches!(evts[0], Event::LoadRun(id) if id == run_id));
        assert!(matches!(evts[1], Event::UpdateIterationStatus { status: RunIterationStatus::FinishedWithSuccess, .. }));
        assert_eq!(evts.len(), 2);
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
        // LoadRun → AppendIteration → UpdateIterationStatus(FinishedWithSuccess)
        let evts = events.lock().expect("events mutex poisoned");
        assert!(matches!(evts[0], Event::LoadRun(id) if id == run_id));
        assert!(matches!(evts[1], Event::AppendIteration { sequence_no, .. } if sequence_no == old_iteration_count as u64));
        assert!(matches!(evts[2], Event::UpdateIterationStatus { status: RunIterationStatus::FinishedWithSuccess, .. }));
        assert_eq!(evts.len(), 3);
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

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state, None)
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

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state, None)
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

        let err = drive_to_outcome_impl(&policy, &executor, &repo, &mut state, None)
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

        let err = drive_to_outcome_impl(&policy, &executor, &repo, &mut state, None)
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

        let err = drive_to_outcome_impl(&policy, &executor, &repo, &mut state, None)
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

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state, None)
            .await
            .expect("must succeed");

        assert!(matches!(outcome, RunOutcome::Finished { .. }));
        // Only UpdateIterationStatus is expected — no executor or step-record events
        let evts = events.lock().expect("events mutex poisoned");
        assert_eq!(evts.len(), 1);
        assert!(matches!(evts[0], Event::UpdateIterationStatus { status: RunIterationStatus::FinishedWithSuccess, .. }));
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

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state, None)
            .await
            .expect("must succeed");

        assert!(matches!(outcome, RunOutcome::Failed { .. }));
        // Only UpdateIterationStatus is expected — no executor events
        let evts = events.lock().expect("events mutex poisoned");
        assert_eq!(evts.len(), 1);
        assert!(matches!(evts[0], Event::UpdateIterationStatus { status: RunIterationStatus::FinishedWithError, .. }));
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
                        metrics: Some(crate::shared_types::QueryStructuringMetrics::default()),
                    },
                )),
            ],
            Arc::clone(&events),
        );
        let repo = FakeRepository::new(None, Arc::clone(&events));

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state, None)
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

    // ─── Error classification ─────────────────────────────────────────────────

    #[test]
    fn orchestrator_error_type_run_not_found() {
        use uuid::Uuid;
        let e = OrchestratorError::RunNotFound { run_id: RunId(Uuid::new_v4()) };
        assert_eq!(orchestrator_error_type(&e), "OrchestratorError.RunNotFound");
    }

    #[test]
    fn orchestrator_error_type_policy() {
        use crate::orchestrator::transition_policy::PolicyError;
        let e = OrchestratorError::Policy(PolicyError::MissingUserInput);
        assert_eq!(orchestrator_error_type(&e), "OrchestratorError.Policy");
    }

    #[test]
    fn orchestrator_error_type_repository() {
        let e = OrchestratorError::Repository(RunRepositoryError::InvalidRunState {
            message: "test".to_string(),
        });
        assert_eq!(orchestrator_error_type(&e), "OrchestratorError.Repository");
    }

    #[test]
    fn step_error_type_missing_required_input() {
        let e = StepError::MissingRequiredInput { message: "no input".to_string() };
        assert_eq!(step_error_type(&e), "StepError.MissingRequiredInput");
    }

    #[test]
    fn step_error_type_unexpected() {
        let e = StepError::Unexpected { message: "boom".to_string() };
        assert_eq!(step_error_type(&e), "StepError.Unexpected");
    }

    #[test]
    fn step_error_type_response_validation() {
        use crate::request_pipeline::response_validation_and_normalization::ResponseValidationAndNormalizationError;
        let e = StepError::ResponseValidationAndNormalization(
            ResponseValidationAndNormalizationError::InvalidResponseShape("bad".to_string()),
        );
        assert_eq!(step_error_type(&e), "StepError.ResponseValidationAndNormalization");
    }

    // ─── record_failed_step_kind ──────────────────────────────────────────────

    #[test]
    fn record_failed_step_kind_finds_last_failed_finished_step() {
        let mut state = run_with_single_iteration("q");
        {
            let mut writer = RunStateWriter::new(&mut state);
            let mut it = writer.current_iteration().expect("iteration");
            let pending = it.begin_step(StepKind::InputNormalization).expect("begin_step");
            pending
                .record_failure(StepError::Unexpected { message: "fail".to_string() })
                .expect("record_failure");
        }
        let span = tracing::Span::none();
        record_failed_step_kind(&span, &state);
    }

    #[test]
    fn record_failed_step_kind_is_noop_when_no_failed_step() {
        let state = run_with_single_iteration("q");
        let span = tracing::Span::none();
        record_failed_step_kind(&span, &state);
    }

    // ─── WaitForUser ──────────────────────────────────────────────────────────

    fn run_waiting_for_user(query: &str) -> RunState {
        let mut state = run_with_single_iteration(query);
        {
            let mut writer = RunStateWriter::new(&mut state);
            writer.wait_for_user().expect("wait_for_user must succeed");
        }
        state
    }

    #[tokio::test]
    async fn wait_for_user_transition_returns_waiting_for_user_outcome() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = run_with_single_iteration("need more info");
        let questions = vec!["What exact error did you see?".to_string(), "Which service?".to_string()];
        let policy = FakePolicy::from_decisions(vec![Ok(PolicyTransition::WaitForUser {
            follow_up_questions: questions.clone(),
        })]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(None, Arc::clone(&events));

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state, None)
            .await
            .expect("must succeed");

        assert_eq!(
            outcome,
            RunOutcome::WaitingForUser {
                run_id: state.run_id,
                follow_up_questions: questions,
            }
        );
        // Only UpdateIterationStatus(FinishedWithWaitInput) — no executor or step events
        let evts = events.lock().expect("events mutex poisoned");
        assert_eq!(evts.len(), 1);
        assert!(matches!(
            evts[0],
            Event::UpdateIterationStatus { status: RunIterationStatus::FinishedWithWaitInput, .. }
        ));
        assert_eq!(state.status, RunStatus::WaitingForUser);
    }

    #[tokio::test]
    async fn resume_returns_waiting_for_user_requires_new_input_when_run_is_waiting() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let state = run_waiting_for_user("stalled");
        let run_id = state.run_id;
        let policy = FakePolicy::from_decisions(vec![]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(Some(state), Arc::clone(&events));

        let err = resume_impl(&policy, &executor, &repo, run_id)
            .await
            .expect_err("resume on WaitingForUser run must fail");

        assert!(matches!(
            err,
            OrchestratorError::WaitingForUserRequiresNewInput { run_id: id } if id == run_id
        ));
        // Only LoadRun — no policy, executor, or iteration events
        let evts = events.lock().expect("events mutex poisoned");
        assert_eq!(evts.len(), 1);
        assert!(matches!(evts[0], Event::LoadRun(id) if id == run_id));
    }

    #[tokio::test]
    async fn resume_with_input_succeeds_when_run_is_waiting_for_user() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let state = run_waiting_for_user("stalled");
        let run_id = state.run_id;
        let old_iteration_count = state.iterations.len();
        let policy = FakePolicy::from_decisions(vec![Ok(PolicyTransition::FinishWithResult {
            result: final_output("resolved"),
        })]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(Some(state), Arc::clone(&events));

        let outcome = resume_with_input_impl(
            &policy,
            &executor,
            &repo,
            run_id,
            user_request("here is the info"),
        )
        .await
        .expect("resume_with_input must succeed for WaitingForUser run");

        assert!(matches!(outcome, RunOutcome::Finished { .. }));
        // LoadRun → AppendIteration(sequence_no=old+1) → UpdateIterationStatus(FinishedWithSuccess)
        let evts = events.lock().expect("events mutex poisoned");
        assert!(matches!(evts[0], Event::LoadRun(id) if id == run_id));
        assert!(matches!(
            evts[1],
            Event::AppendIteration { sequence_no, .. } if sequence_no == old_iteration_count as u64
        ));
        assert!(matches!(
            evts[2],
            Event::UpdateIterationStatus { status: RunIterationStatus::FinishedWithSuccess, .. }
        ));
        assert_eq!(evts.len(), 3);
    }

    // ─── RunOutcome mapping ───────────────────────────────────────────────────

    #[tokio::test]
    async fn drive_to_outcome_failed_step_returns_run_outcome_failed() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = run_with_single_iteration("persisted");
        let step_error = StepError::Unexpected { message: "executor failed".to_string() };
        let policy = FakePolicy::from_decisions(vec![
            Ok(PolicyTransition::ExecuteStep { step: StepKind::InputNormalization }),
            Ok(PolicyTransition::FinishWithError { error: step_error.clone() }),
        ]);
        let executor = FakeExecutor::new(vec![Err(step_error)], Arc::clone(&events));
        let repo = FakeRepository::new(None, Arc::clone(&events));

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state, None)
            .await
            .expect("must not return OrchestratorError");

        assert!(
            matches!(outcome, RunOutcome::Failed { .. }),
            "failed step must produce RunOutcome::Failed, not Finished"
        );
    }

    #[tokio::test]
    async fn drive_to_outcome_finish_with_result_returns_run_outcome_finished() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut state = run_with_single_iteration("persisted");
        let policy = FakePolicy::from_decisions(vec![Ok(PolicyTransition::FinishWithResult {
            result: final_output("done"),
        })]);
        let executor = FakeExecutor::new(vec![], Arc::clone(&events));
        let repo = FakeRepository::new(None, Arc::clone(&events));

        let outcome = drive_to_outcome_impl(&policy, &executor, &repo, &mut state, None)
            .await
            .expect("must succeed");

        assert!(matches!(outcome, RunOutcome::Finished { .. }));
    }
}
