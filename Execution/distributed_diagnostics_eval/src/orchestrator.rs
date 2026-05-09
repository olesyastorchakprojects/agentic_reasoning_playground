use std::path::PathBuf;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use tracing::Instrument;

use crate::config::EvalSettings;
use crate::judge::{execute_one_suite_for_subject, JudgeClient, JudgeExecutionError};
use crate::observability::{eval_subject_span, record_error};
use crate::manifest::{
    build_running_manifest, create_eval_run_artifact_dir, write_run_manifest,
    ManifestError,
};
use crate::runtime_runs::{PostgresRuntimeRunLoader, RuntimeRunLoaderError};
use crate::summary::{build_iteration_summary_row, SummaryError};
use crate::storage::{PostgresEvalStore, StorageError};
use crate::storage::{EvalProcessingStatus, EvalStage};
use crate::subject_preparation::{
    prepare_subject_from_processing_state, SubjectPreparationError,
};
use crate::suites::{JudgeSuiteCatalog, SuiteCatalogError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapResult {
    pub eval_run_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub artifact_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub runtime_run_count: usize,
    pub subject_count: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    RuntimeRunLoader(#[from] RuntimeRunLoaderError),
    #[error(transparent)]
    SubjectPreparation(#[from] SubjectPreparationError),
    #[error(transparent)]
    SuiteCatalog(#[from] SuiteCatalogError),
    #[error(transparent)]
    JudgeExecution(#[from] JudgeExecutionError),
    #[error(transparent)]
    Summary(#[from] SummaryError),
    #[error("no eligible runtime runs found for bootstrap")]
    NoEligibleRuntimeRuns,
}

pub struct EvalOrchestrator {
    settings: EvalSettings,
    store: PostgresEvalStore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgeDrainResult {
    pub attempted_subjects: usize,
    pub completed_subjects: usize,
    pub failed_subjects: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryDrainResult {
    pub attempted_subjects: usize,
    pub completed_subjects: usize,
    pub failed_subjects: usize,
}

impl EvalOrchestrator {
    pub fn new(settings: EvalSettings, store: PostgresEvalStore) -> Self {
        Self { settings, store }
    }

    pub async fn bootstrap_new_eval_run(
        &self,
    ) -> Result<BootstrapResult, OrchestratorError> {
        let started_at = Utc::now();
        let eval_run_id = Uuid::new_v4();
        let discovered = self
            .store
            .discover_eligible_subjects(
                eval_run_id,
                self.settings.eval.max_runtime_runs_per_new_eval_run,
            )
            .await?;
        if discovered.is_empty() {
            return Err(OrchestratorError::NoEligibleRuntimeRuns);
        }

        let manifest = build_running_manifest(
            &self.settings,
            eval_run_id,
            started_at,
            &discovered,
        );
        let artifact_dir = create_eval_run_artifact_dir(
            &self.settings.artifacts.root_dir,
            started_at,
            eval_run_id,
        )?;
        let manifest_path = write_run_manifest(&artifact_dir, &manifest)?;
        self.store
            .bootstrap_eval_processing_state(&discovered)
            .await?;

        Ok(BootstrapResult {
            eval_run_id,
            started_at,
            artifact_dir,
            manifest_path,
            runtime_run_count: manifest.runtime_run_count,
            subject_count: manifest.subject_count,
        })
    }

    pub fn settings(&self) -> &EvalSettings {
        &self.settings
    }

    pub fn store(&self) -> &PostgresEvalStore {
        &self.store
    }

    pub async fn run_one_judge_request_suites_for_eval_run(
        &self,
        eval_run_id: Uuid,
        runtime_loader: &PostgresRuntimeRunLoader,
        catalog: &JudgeSuiteCatalog,
        client: &dyn JudgeClient,
    ) -> Result<bool, OrchestratorError> {
        let Some(processing_state) = self
            .store
            .fetch_next_subject_for_stage(eval_run_id, EvalStage::JudgeRequestSuites)
            .await?
        else {
            return Ok(false);
        };

        let attempt_started_at = Utc::now();
        let next_attempt_count = processing_state.attempt_count + 1;
        self.store
            .update_eval_processing_state(
                &processing_state.key,
                EvalStage::JudgeRequestSuites,
                EvalProcessingStatus::Running,
                next_attempt_count,
                Some(attempt_started_at),
                None,
                None,
            )
            .await?;

        let preparation_result =
            prepare_subject_from_processing_state(processing_state.clone(), runtime_loader).await;
        let prepared_subject = match preparation_result {
            Ok(subject) => subject,
            Err(error) => {
                self.store
                    .update_eval_processing_state(
                        &processing_state.key,
                        EvalStage::JudgeRequestSuites,
                        EvalProcessingStatus::Failed,
                        next_attempt_count,
                        Some(attempt_started_at),
                        None,
                        Some(&error.to_string()),
                    )
                    .await?;
                return Err(error.into());
            }
        };

        let enabled_suites = catalog
            .resolve_enabled_suite_names(&self.settings.suites)
            .map_err(OrchestratorError::SuiteCatalog)?;

        let subject_span = eval_subject_span(
            &prepared_subject.processing_state.key.eval_run_id.to_string(),
            &prepared_subject.processing_state.key.runtime_run_id.to_string(),
            &prepared_subject.processing_state.key.iteration_id.to_string(),
        );

        let iteration_kind = prepared_subject.snapshot.iteration_kind;

        let mut first_error: Option<JudgeExecutionError> = None;
        for suite_name in &enabled_suites {
            let suite_def = catalog
                .get(suite_name)
                .ok_or_else(|| OrchestratorError::JudgeExecution(
                    JudgeExecutionError::MissingSuiteDefinition(suite_name.clone()),
                ))?;

            let applicable = match iteration_kind {
                distributed_diagnostics::shared_types::IterationProfile::Initial => {
                    suite_def.applies_to.applies_to_initial()
                }
                distributed_diagnostics::shared_types::IterationProfile::Continuation => {
                    suite_def.applies_to.applies_to_continuation()
                }
            };
            if !applicable {
                tracing::debug!(
                    suite_name,
                    ?iteration_kind,
                    "skipping suite: applies_to does not match iteration kind"
                );
                continue;
            }

            let already_done = self
                .store
                .judge_result_exists(&prepared_subject.processing_state.key, suite_name)
                .await?;
            if already_done {
                continue;
            }
            match execute_one_suite_for_subject(
                &self.store,
                &self.settings.judge,
                suite_name,
                catalog,
                &prepared_subject,
                client,
            )
            .instrument(subject_span.clone())
            .await
            {
                Ok(()) => {}
                Err(error) => {
                    first_error = Some(error);
                    break;
                }
            }
        }

        if let Some(error) = first_error {
            subject_span.record("eval.subject_status", "failed");
            record_error(&subject_span, "JudgeExecutionError", &error.to_string());
            self.store
                .update_eval_processing_state(
                    &prepared_subject.processing_state.key,
                    EvalStage::JudgeRequestSuites,
                    EvalProcessingStatus::Failed,
                    next_attempt_count,
                    Some(attempt_started_at),
                    None,
                    Some(&error.to_string()),
                )
                .await?;
            return Err(OrchestratorError::JudgeExecution(error));
        }

        subject_span.record("eval.subject_status", "completed");
        self.store
            .update_eval_processing_state(
                &prepared_subject.processing_state.key,
                EvalStage::BuildEvalSummary,
                EvalProcessingStatus::Pending,
                0,
                None,
                None,
                None,
            )
            .await?;
        Ok(true)
    }

    pub async fn drain_judge_request_suites_for_eval_run(
        &self,
        eval_run_id: Uuid,
        runtime_loader: &PostgresRuntimeRunLoader,
        catalog: &JudgeSuiteCatalog,
        client: &dyn JudgeClient,
    ) -> Result<JudgeDrainResult, OrchestratorError> {
        let mut attempted_subjects = 0_usize;
        let mut completed_subjects = 0_usize;
        let mut failed_subjects = 0_usize;

        loop {
            match self
                .run_one_judge_request_suites_for_eval_run(
                    eval_run_id,
                    runtime_loader,
                    catalog,
                    client,
                )
                .await
            {
                Ok(true) => {
                    attempted_subjects += 1;
                    completed_subjects += 1;
                }
                Ok(false) => {
                    break;
                }
                Err(OrchestratorError::JudgeExecution(_) | OrchestratorError::SubjectPreparation(_)) => {
                    attempted_subjects += 1;
                    failed_subjects += 1;
                }
                Err(error) => return Err(error),
            }
        }

        Ok(JudgeDrainResult {
            attempted_subjects,
            completed_subjects,
            failed_subjects,
        })
    }

    pub async fn run_one_build_eval_summary_for_eval_run(
        &self,
        eval_run_id: Uuid,
        runtime_loader: &PostgresRuntimeRunLoader,
    ) -> Result<bool, OrchestratorError> {
        let Some(processing_state) = self
            .store
            .fetch_next_subject_for_stage(eval_run_id, EvalStage::BuildEvalSummary)
            .await?
        else {
            return Ok(false);
        };

        let attempt_started_at = Utc::now();
        let next_attempt_count = processing_state.attempt_count + 1;
        self.store
            .update_eval_processing_state(
                &processing_state.key,
                EvalStage::BuildEvalSummary,
                EvalProcessingStatus::Running,
                next_attempt_count,
                Some(attempt_started_at),
                None,
                None,
            )
            .await?;

        let preparation_result =
            prepare_subject_from_processing_state(processing_state.clone(), runtime_loader).await;
        let prepared_subject = match preparation_result {
            Ok(subject) => subject,
            Err(error) => {
                self.store
                    .update_eval_processing_state(
                        &processing_state.key,
                        EvalStage::BuildEvalSummary,
                        EvalProcessingStatus::Failed,
                        next_attempt_count,
                        Some(attempt_started_at),
                        None,
                        Some(&error.to_string()),
                    )
                    .await?;
                return Err(error.into());
            }
        };

        let judge_results = self
            .store
            .list_judge_results_for_subject(&prepared_subject.processing_state.key)
            .await?;
        let judge_calls = self
            .store
            .list_judge_llm_calls_for_subject(&prepared_subject.processing_state.key)
            .await?;

        let iteration_summary = build_iteration_summary_row(
            prepared_subject.processing_state.key.clone(),
            &prepared_subject.snapshot,
            &judge_results,
            &judge_calls,
        )?;
        self.store
            .upsert_eval_iteration_summary(&iteration_summary)
            .await?;
        self.store
            .update_eval_processing_state(
                &prepared_subject.processing_state.key,
                EvalStage::BuildEvalSummary,
                EvalProcessingStatus::Completed,
                next_attempt_count,
                Some(attempt_started_at),
                Some(Utc::now()),
                None,
            )
            .await?;
        Ok(true)
    }

    pub async fn drain_build_eval_summary_for_eval_run(
        &self,
        eval_run_id: Uuid,
        runtime_loader: &PostgresRuntimeRunLoader,
    ) -> Result<SummaryDrainResult, OrchestratorError> {
        let mut attempted_subjects = 0_usize;
        let mut completed_subjects = 0_usize;
        let failed_subjects = 0_usize;

        loop {
            match self
                .run_one_build_eval_summary_for_eval_run(eval_run_id, runtime_loader)
                .await
            {
                Ok(true) => {
                    attempted_subjects += 1;
                    completed_subjects += 1;
                }
                Ok(false) => break,
                Err(error) => return Err(error),
            }
        }

        Ok(SummaryDrainResult {
            attempted_subjects,
            completed_subjects,
            failed_subjects,
        })
    }
}
