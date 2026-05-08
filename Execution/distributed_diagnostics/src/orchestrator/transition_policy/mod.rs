use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::orchestrator::run_state::model::{StepError, StepKind};
use crate::orchestrator::run_state::view::RunStateView;
use crate::shared_types::ResponseValidationAndNormalizationOutput;

pub use diagnostic_loop::DiagnosticLoopTransitionPolicy;
pub use linear_pipeline::LinearPipelineTransitionPolicy;

mod diagnostic_loop;
mod linear_pipeline;

pub trait TransitionPolicy {
    fn next_transition(
        &self,
        state: RunStateView<'_>,
    ) -> Result<PolicyTransition, PolicyError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PolicyTransition {
    ExecuteStep { step: StepKind },
    WaitForUser {
        follow_up_questions: Vec<String>,
    },
    FinishWithResult {
        result: ResponseValidationAndNormalizationOutput,
    },
    FinishWithError {
        error: StepError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum PolicyError {
    #[error("run is archived")]
    RunArchived,

    #[error("run has no current iteration")]
    NoCurrentIteration,

    #[error("current iteration has a pending step")]
    PendingStepPresent,

    #[error("current iteration contains duplicate successful step records for {step}")]
    DuplicateSuccessfulStep { step: StepKind },

    #[error("current iteration contains a successful step out of canonical order: {step}")]
    StepOutOfOrder { step: StepKind },

    #[error("current iteration is missing required user input")]
    MissingUserInput,

    #[error("current iteration stores an unexpected successful result variant for {step}")]
    UnexpectedStepResult { step: StepKind },
}
