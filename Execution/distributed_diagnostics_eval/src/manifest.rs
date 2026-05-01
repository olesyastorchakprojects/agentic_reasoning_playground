use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::EvalSettings;
use crate::storage::FrozenEvalSubject;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalRunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestSubject {
    pub runtime_run_id: Uuid,
    pub iteration_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalRunManifest {
    pub eval_run_id: Uuid,
    pub run_type: String,
    pub status: EvalRunStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub stages: Vec<String>,
    pub judge_provider: String,
    pub judge_base_url: String,
    pub judge_model: String,
    pub suite_versions: BTreeMap<String, String>,
    pub runtime_run_count: usize,
    pub run_scope_runtime_run_ids: Vec<Uuid>,
    pub subject_count: usize,
    pub run_scope_subjects: Vec<ManifestSubject>,
    pub last_error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("artifact io failure: {0}")]
    Io(String),
    #[error("serialization failure: {0}")]
    Serialization(String),
    #[error("manifest not found for eval run: {0}")]
    ManifestNotFound(Uuid),
}

pub fn build_running_manifest(
    settings: &EvalSettings,
    eval_run_id: Uuid,
    started_at: DateTime<Utc>,
    subjects: &[FrozenEvalSubject],
) -> EvalRunManifest {
    let mut runtime_run_ids = Vec::with_capacity(subjects.len());
    let mut subject_rows = Vec::with_capacity(subjects.len());
    for subject in subjects {
        runtime_run_ids.push(subject.key.runtime_run_id);
        subject_rows.push(ManifestSubject {
            runtime_run_id: subject.key.runtime_run_id,
            iteration_id: subject.key.iteration_id,
        });
    }
    runtime_run_ids.sort();
    runtime_run_ids.dedup();

    let suite_versions = settings
        .suites
        .enabled
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|name| (name, "enabled_from_config".to_string()))
        .collect();

    EvalRunManifest {
        eval_run_id,
        run_type: settings.eval.run_type.clone(),
        status: EvalRunStatus::Running,
        started_at,
        completed_at: None,
        stages: vec![
            "judge_request_suites".to_string(),
            "build_eval_summary".to_string(),
        ],
        judge_provider: settings.judge.provider.clone(),
        judge_base_url: settings.judge.together.base_url.clone(),
        judge_model: settings.judge.model_name.clone(),
        suite_versions,
        runtime_run_count: runtime_run_ids.len(),
        run_scope_runtime_run_ids: runtime_run_ids,
        subject_count: subject_rows.len(),
        run_scope_subjects: subject_rows,
        last_error: None,
    }
}

pub fn create_eval_run_artifact_dir(
    root_dir: &Path,
    started_at: DateTime<Utc>,
    eval_run_id: Uuid,
) -> Result<PathBuf, ManifestError> {
    let safe_started_at = started_at
        .to_rfc3339()
        .replace(':', "-")
        .replace('+', "+");
    let dir = root_dir.join(format!("{safe_started_at}_{eval_run_id}"));
    fs::create_dir_all(&dir).map_err(|e| ManifestError::Io(e.to_string()))?;
    Ok(dir)
}

pub fn write_run_manifest(
    artifact_dir: &Path,
    manifest: &EvalRunManifest,
) -> Result<PathBuf, ManifestError> {
    let manifest_path = artifact_dir.join("run_manifest.json");
    let body = serde_json::to_vec_pretty(manifest)
        .map_err(|e| ManifestError::Serialization(e.to_string()))?;
    fs::write(&manifest_path, body).map_err(|e| ManifestError::Io(e.to_string()))?;
    Ok(manifest_path)
}

pub fn read_run_manifest(
    artifact_dir: &Path,
) -> Result<EvalRunManifest, ManifestError> {
    let manifest_path = artifact_dir.join("run_manifest.json");
    let body = fs::read(&manifest_path).map_err(|e| ManifestError::Io(e.to_string()))?;
    serde_json::from_slice(&body)
        .map_err(|e| ManifestError::Serialization(e.to_string()))
}

pub fn find_artifact_dir_for_eval_run(
    root_dir: &Path,
    eval_run_id: Uuid,
) -> Result<PathBuf, ManifestError> {
    let entries = fs::read_dir(root_dir).map_err(|e| ManifestError::Io(e.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|e| ManifestError::Io(e.to_string()))?;
        if !entry
            .file_type()
            .map_err(|e| ManifestError::Io(e.to_string()))?
            .is_dir()
        {
            continue;
        }
        let candidate = entry.path();
        let manifest_path = candidate.join("run_manifest.json");
        if !manifest_path.exists() {
            continue;
        }
        let body = fs::read(&manifest_path).map_err(|e| ManifestError::Io(e.to_string()))?;
        let manifest: EvalRunManifest = serde_json::from_slice(&body)
            .map_err(|e| ManifestError::Serialization(e.to_string()))?;
        if manifest.eval_run_id == eval_run_id {
            return Ok(candidate);
        }
    }
    Err(ManifestError::ManifestNotFound(eval_run_id))
}

#[cfg(test)]
mod tests {
    use super::{
        build_running_manifest, create_eval_run_artifact_dir, write_run_manifest,
        EvalRunStatus,
    };
    use crate::config::{
        ArtifactsSettings, EvalSettings, EvalSettingsEval, JudgeSettings,
        ObservabilitySettings, PostgresSettings, SuitesSettings,
        TogetherJudgeSettings,
    };
    use crate::storage::{EvalSubjectKey, FrozenEvalSubject};
    use chrono::Utc;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn settings(root_dir: PathBuf) -> EvalSettings {
        EvalSettings {
            eval: EvalSettingsEval {
                config_version: "v1".into(),
                run_type: "golden_dataset".into(),
                mode: "batch_golden".into(),
                resume_eval_run_id: None,
                batch_label: None,
                max_runtime_runs_per_new_eval_run: None,
            },
            judge: JudgeSettings {
                provider: "together".into(),
                model_name: "openai/gpt-oss-20b".into(),
                tokenizer_source: "Qwen/Qwen2.5-1.5B-Instruct".into(),
                input_cost_per_million_tokens: 0.05,
                output_cost_per_million_tokens: 0.20,
                together: TogetherJudgeSettings {
                    base_url: "https://api.together.xyz".into(),
                    api_key: "secret".into(),
                    timeout_sec: 120,
                    retry_max_attempts: 3,
                    retry_backoff: "exponential".into(),
                },
            },
            postgres: PostgresSettings {
                url: "postgres://localhost/db".into(),
            },
            artifacts: ArtifactsSettings {
                root_dir,
                write_manifest: true,
                write_markdown_report: true,
            },
            suites: SuitesSettings {
                catalog_path: PathBuf::from("Specification/evals/prompts.json"),
                required_for_mvp_only: true,
                enabled: Some(vec!["final_no_root_cause_claim".into()]),
            },
            observability: ObservabilitySettings {
                tracing_enabled: true,
                metrics_enabled: true,
                service_name: "distributed_diagnostics_eval".into(),
            },
        }
    }

    #[test]
    fn running_manifest_reflects_subject_scope() {
        let s = settings(PathBuf::from("Evidence/evals/runs"));
        let eval_run_id = Uuid::nil();
        let subject = FrozenEvalSubject {
            key: EvalSubjectKey {
                eval_run_id,
                runtime_run_id: Uuid::nil(),
                iteration_id: Uuid::from_u128(1),
            },
            subject_received_at: Utc::now(),
        };
        let manifest =
            build_running_manifest(&s, eval_run_id, Utc::now(), &[subject]);
        assert_eq!(manifest.status, EvalRunStatus::Running);
        assert_eq!(manifest.runtime_run_count, 1);
        assert_eq!(manifest.subject_count, 1);
    }

    #[test]
    fn artifact_dir_and_manifest_are_written() {
        let dir = tempdir().unwrap();
        let s = settings(dir.path().to_path_buf());
        let eval_run_id = Uuid::new_v4();
        let subject = FrozenEvalSubject {
            key: EvalSubjectKey {
                eval_run_id,
                runtime_run_id: Uuid::new_v4(),
                iteration_id: Uuid::new_v4(),
            },
            subject_received_at: Utc::now(),
        };
        let manifest =
            build_running_manifest(&s, eval_run_id, Utc::now(), &[subject]);
        let artifact_dir =
            create_eval_run_artifact_dir(&s.artifacts.root_dir, Utc::now(), eval_run_id)
                .unwrap();
        let manifest_path = write_run_manifest(&artifact_dir, &manifest).unwrap();
        assert!(artifact_dir.exists());
        assert!(manifest_path.exists());
    }
}
