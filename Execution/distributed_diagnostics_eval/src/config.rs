use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::cli::EvalCliOverrides;

#[derive(Debug, Clone, PartialEq)]
pub struct EvalSettings {
    pub eval: EvalSettingsEval,
    pub judge: JudgeSettings,
    pub postgres: PostgresSettings,
    pub artifacts: ArtifactsSettings,
    pub suites: SuitesSettings,
    pub observability: ObservabilitySettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalSettingsEval {
    pub config_version: String,
    pub run_type: String,
    pub mode: String,
    pub resume_eval_run_id: Option<String>,
    pub batch_label: Option<String>,
    pub max_runtime_runs_per_new_eval_run: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JudgeSettings {
    pub provider: String,
    pub model_name: String,
    pub tokenizer_source: String,
    pub input_cost_per_million_tokens: f64,
    pub output_cost_per_million_tokens: f64,
    pub together: TogetherJudgeSettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TogetherJudgeSettings {
    pub base_url: String,
    pub api_key: String,
    pub timeout_sec: u64,
    pub retry_max_attempts: u32,
    pub retry_backoff: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresSettings {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactsSettings {
    pub root_dir: PathBuf,
    pub write_manifest: bool,
    pub write_markdown_report: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuitesSettings {
    pub catalog_path: PathBuf,
    pub required_for_mvp_only: bool,
    pub enabled: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilitySettings {
    pub tracing_enabled: bool,
    pub service_name: String,
    pub tracing_endpoint: String,
}

#[derive(Debug, thiserror::Error)]
pub enum EvalConfigError {
    #[error("failed to load eval config: {0}")]
    Load(String),
    #[error("invalid eval config: {0}")]
    InvalidConfig(String),
    #[error("missing environment variable: {0}")]
    MissingEnv(String),
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    eval: RawEval,
    judge: RawJudge,
    postgres: RawPostgres,
    artifacts: RawArtifacts,
    suites: RawSuites,
    observability: RawObservability,
}

#[derive(Debug, Deserialize)]
struct RawEval {
    config_version: String,
    run_type: String,
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default)]
    resume_eval_run_id: Option<String>,
    #[serde(default)]
    batch_label: Option<String>,
    #[serde(default)]
    max_runtime_runs_per_new_eval_run: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct RawJudge {
    provider: String,
    model_name: String,
    tokenizer_source: String,
    input_cost_per_million_tokens: f64,
    output_cost_per_million_tokens: f64,
    #[serde(default)]
    together: Option<RawJudgeProviderSettings>,
}

#[derive(Debug, Deserialize)]
struct RawJudgeProviderSettings {
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    api_key_env: Option<String>,
    timeout_sec: u64,
    #[serde(default = "default_retry_max_attempts")]
    retry_max_attempts: u32,
    #[serde(default = "default_retry_backoff")]
    retry_backoff: String,
}

#[derive(Debug, Deserialize)]
struct RawPostgres {
    url_env: String,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawArtifacts {
    root_dir: PathBuf,
    #[serde(default = "default_true")]
    write_manifest: bool,
    #[serde(default = "default_true")]
    write_markdown_report: bool,
}

#[derive(Debug, Deserialize)]
struct RawSuites {
    catalog_path: PathBuf,
    #[serde(default)]
    enabled: Option<Vec<String>>,
    #[serde(default)]
    required_for_mvp_only: bool,
}

#[derive(Debug, Deserialize)]
struct RawObservability {
    tracing_enabled: bool,
    #[serde(default = "default_service_name")]
    service_name: String,
    #[serde(default = "default_tracing_endpoint_env")]
    tracing_endpoint_env: String,
    #[serde(default)]
    tracing_endpoint: Option<String>,
}

fn default_tracing_endpoint_env() -> String {
    "TRACING_ENDPOINT".to_string()
}

fn default_mode() -> String {
    "batch_golden".to_string()
}

fn default_true() -> bool {
    true
}

fn default_retry_max_attempts() -> u32 {
    3
}

fn default_retry_backoff() -> String {
    "exponential".to_string()
}

fn default_service_name() -> String {
    "distributed_diagnostics_eval".to_string()
}

pub fn load_eval_settings(
    config_path: &Path,
    cli_overrides: &EvalCliOverrides,
) -> Result<EvalSettings, EvalConfigError> {
    let _ = dotenvy::dotenv();
    load_eval_settings_inner(config_path, cli_overrides, &|key| {
        std::env::var(key).ok()
    })
}

fn load_eval_settings_inner(
    config_path: &Path,
    cli_overrides: &EvalCliOverrides,
    env_fn: &impl Fn(&str) -> Option<String>,
) -> Result<EvalSettings, EvalConfigError> {
    let raw: RawConfig = config::Config::builder()
        .add_source(config::File::from(config_path).format(config::FileFormat::Toml))
        .build()
        .map_err(|e| EvalConfigError::Load(e.to_string()))?
        .try_deserialize()
        .map_err(|e| EvalConfigError::Load(e.to_string()))?;

    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let run_type = cli_overrides
        .run_type
        .clone()
        .unwrap_or_else(|| raw.eval.run_type.clone());
    if run_type.trim().is_empty() {
        return Err(EvalConfigError::InvalidConfig(
            "eval.run_type must not be empty".to_string(),
        ));
    }

    let postgres_url = resolve_postgres_url(&raw.postgres, env_fn)?;
    let judge = resolve_judge_settings(&raw.judge, env_fn)?;
    let artifacts_root = cli_overrides
        .artifact_root
        .clone()
        .unwrap_or_else(|| raw.artifacts.root_dir.clone());
    let artifacts_root = resolve_path(config_dir, &artifacts_root);
    let catalog_path = resolve_path(config_dir, &raw.suites.catalog_path);
    if !catalog_path.exists() {
        return Err(EvalConfigError::InvalidConfig(format!(
            "suite catalog path does not exist: {}",
            catalog_path.display()
        )));
    }

    let enabled = match &cli_overrides.enabled_suites {
        Some(suites) => Some(non_empty_suites(suites.clone())?),
        None => raw
            .suites
            .enabled
            .clone()
            .map(non_empty_suites)
            .transpose()?,
    };

    Ok(EvalSettings {
        eval: EvalSettingsEval {
            config_version: require_non_empty(
                &raw.eval.config_version,
                "eval.config_version",
            )?,
            run_type,
            mode: require_non_empty(&raw.eval.mode, "eval.mode")?,
            resume_eval_run_id: raw.eval.resume_eval_run_id,
            batch_label: raw.eval.batch_label,
            max_runtime_runs_per_new_eval_run: cli_overrides
                .limit
                .or(raw.eval.max_runtime_runs_per_new_eval_run),
        },
        judge,
        postgres: PostgresSettings { url: postgres_url },
        artifacts: ArtifactsSettings {
            root_dir: artifacts_root,
            write_manifest: raw.artifacts.write_manifest,
            write_markdown_report: raw.artifacts.write_markdown_report,
        },
        suites: SuitesSettings {
            catalog_path,
            required_for_mvp_only: raw.suites.required_for_mvp_only,
            enabled,
        },
        observability: ObservabilitySettings {
            tracing_enabled: raw.observability.tracing_enabled,
            service_name: require_non_empty(
                &raw.observability.service_name,
                "observability.service_name",
            )?,
            tracing_endpoint: if let Some(explicit) = raw.observability.tracing_endpoint.as_deref() {
                explicit.to_string()
            } else {
                std::env::var(&raw.observability.tracing_endpoint_env)
                    .unwrap_or_else(|_| "http://localhost:4317".to_string())
            },
        },
    })
}

fn resolve_env_backed_string(
    explicit: Option<&str>,
    env_name: &str,
    field: &str,
) -> Result<String, EvalConfigError> {
    if let Some(val) = explicit {
        return require_non_empty(val, field);
    }
    std::env::var(env_name).map_err(|_| EvalConfigError::MissingEnv(env_name.to_string()))
}

fn resolve_postgres_url(
    raw: &RawPostgres,
    env_fn: &impl Fn(&str) -> Option<String>,
) -> Result<String, EvalConfigError> {
    if let Some(url) = &raw.url {
        return require_non_empty(url, "postgres.url");
    }
    let env_name = require_non_empty(&raw.url_env, "postgres.url_env")?;
    env_fn(&env_name).ok_or(EvalConfigError::MissingEnv(env_name))
}

fn resolve_judge_settings(
    raw: &RawJudge,
    env_fn: &impl Fn(&str) -> Option<String>,
) -> Result<JudgeSettings, EvalConfigError> {
    let provider_name = require_non_empty(&raw.provider, "judge.provider")?;
    if provider_name != "together" {
        return Err(EvalConfigError::InvalidConfig(format!(
            "unsupported judge provider for eval engine: {provider_name}"
        )));
    }
    let p = raw.together.as_ref().ok_or_else(|| {
        EvalConfigError::InvalidConfig(
            "judge.together section is required for provider=together"
                .to_string(),
        )
    })?;
    let api_key_env = p
        .api_key_env
        .clone()
        .unwrap_or_else(|| "TOGETHER_API_KEY".to_string());
    let api_key =
        env_fn(&api_key_env).ok_or(EvalConfigError::MissingEnv(api_key_env))?;
    let together = TogetherJudgeSettings {
        base_url: p
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.together.xyz".to_string()),
        api_key,
        timeout_sec: p.timeout_sec,
        retry_max_attempts: p.retry_max_attempts,
        retry_backoff: p.retry_backoff.clone(),
    };

    if raw.input_cost_per_million_tokens < 0.0
        || raw.output_cost_per_million_tokens < 0.0
    {
        return Err(EvalConfigError::InvalidConfig(
            "judge per-million token costs must be non-negative".to_string(),
        ));
    }

    Ok(JudgeSettings {
        provider: provider_name,
        model_name: require_non_empty(&raw.model_name, "judge.model_name")?,
        tokenizer_source: require_non_empty(
            &raw.tokenizer_source,
            "judge.tokenizer_source",
        )?,
        input_cost_per_million_tokens: raw.input_cost_per_million_tokens,
        output_cost_per_million_tokens: raw.output_cost_per_million_tokens,
        together,
    })
}

fn require_non_empty(
    value: &str,
    field_name: &str,
) -> Result<String, EvalConfigError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EvalConfigError::InvalidConfig(format!(
            "{field_name} must not be empty"
        )));
    }
    Ok(trimmed.to_string())
}

fn non_empty_suites(
    suites: Vec<String>,
) -> Result<Vec<String>, EvalConfigError> {
    let cleaned: Vec<String> = suites
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if cleaned.is_empty() {
        return Err(EvalConfigError::InvalidConfig(
            "enabled suite list must not be empty".to_string(),
        ));
    }
    Ok(cleaned)
}

fn resolve_path(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::{load_eval_settings_inner, EvalConfigError, EvalSettings};
    use crate::cli::EvalCliOverrides;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::tempdir;

    fn write_config(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempdir().unwrap();
        let cfg_path = dir.path().join("eval.toml");
        fs::write(&cfg_path, contents).unwrap();
        let catalog_path = dir.path().join("prompts.json");
        fs::write(&catalog_path, "{}").unwrap();
        (dir, cfg_path)
    }

    fn no_overrides() -> EvalCliOverrides {
        EvalCliOverrides {
            run_type: None,
            limit: None,
            artifact_root: None,
            enabled_suites: None,
        }
    }

    fn load_with_env(
        config_path: &std::path::Path,
        overrides: &EvalCliOverrides,
        env: HashMap<String, String>,
    ) -> Result<EvalSettings, EvalConfigError> {
        load_eval_settings_inner(config_path, overrides, &|key| env.get(key).cloned())
    }

    #[test]
    fn loads_together_config_and_resolves_env() {
        let (_dir, path) = write_config(
            r#"
[eval]
config_version = "v1"
run_type = "golden_dataset"

[judge]
provider = "together"
model_name = "openai/gpt-oss-20b"
tokenizer_source = "Qwen/Qwen2.5-1.5B-Instruct"
input_cost_per_million_tokens = 0.05
output_cost_per_million_tokens = 0.20

[judge.together]
timeout_sec = 120

[postgres]
url_env = "POSTGRES_URL"

[artifacts]
root_dir = "Evidence/evals/runs"

[suites]
catalog_path = "prompts.json"
required_for_mvp_only = true

[observability]
tracing_enabled = true
metrics_enabled = true
"#,
        );
        let env = HashMap::from([
            (
                "POSTGRES_URL".to_string(),
                "postgres://postgres:postgres@localhost:5432/agentic_reasoning"
                    .to_string(),
            ),
            ("TOGETHER_API_KEY".to_string(), "secret".to_string()),
        ]);
        let settings = load_with_env(&path, &no_overrides(), env).unwrap();
        assert_eq!(settings.eval.run_type, "golden_dataset");
        assert_eq!(
            settings.postgres.url,
            "postgres://postgres:postgres@localhost:5432/agentic_reasoning"
        );
        assert_eq!(settings.judge.provider, "together");
        assert_eq!(settings.judge.together.api_key, "secret");
        assert_eq!(settings.judge.together.base_url, "https://api.together.xyz");
        assert!(settings.suites.catalog_path.ends_with("prompts.json"));
    }

    #[test]
    fn run_type_and_suites_are_overridden_by_cli() {
        let (_dir, path) = write_config(
            r#"
[eval]
config_version = "v1"
run_type = "golden_dataset"
max_runtime_runs_per_new_eval_run = 10

[judge]
provider = "together"
model_name = "openai/gpt-oss-20b"
tokenizer_source = "Qwen/Qwen2.5-1.5B-Instruct"
input_cost_per_million_tokens = 0.05
output_cost_per_million_tokens = 0.20

[judge.together]
timeout_sec = 120

[postgres]
url_env = "POSTGRES_URL"

[artifacts]
root_dir = "Evidence/evals/runs"

[suites]
catalog_path = "prompts.json"
enabled = ["a", "b"]

[observability]
tracing_enabled = true
metrics_enabled = false
"#,
        );
        let env = HashMap::from([
            (
                "POSTGRES_URL".to_string(),
                "postgres://postgres:postgres@localhost:5432/agentic_reasoning"
                    .to_string(),
            ),
            ("TOGETHER_API_KEY".to_string(), "secret".to_string()),
        ]);
        let overrides = EvalCliOverrides {
            run_type: Some("local_dev_eval".to_string()),
            limit: Some(3),
            artifact_root: None,
            enabled_suites: Some(vec!["x".to_string(), "y".to_string()]),
        };
        let settings = load_with_env(&path, &overrides, env).unwrap();
        assert_eq!(settings.eval.run_type, "local_dev_eval");
        assert_eq!(settings.eval.max_runtime_runs_per_new_eval_run, Some(3));
        assert_eq!(settings.suites.enabled, Some(vec!["x".into(), "y".into()]));
    }

    #[test]
    fn missing_postgres_env_fails() {
        let (_dir, path) = write_config(
            r#"
[eval]
config_version = "v1"
run_type = "golden_dataset"

[judge]
provider = "together"
model_name = "openai/gpt-oss-20b"
tokenizer_source = "Qwen/Qwen2.5-1.5B-Instruct"
input_cost_per_million_tokens = 0.05
output_cost_per_million_tokens = 0.20

[judge.together]
timeout_sec = 120

[postgres]
url_env = "POSTGRES_URL"

[artifacts]
root_dir = "Evidence/evals/runs"

[suites]
catalog_path = "prompts.json"

[observability]
tracing_enabled = true
metrics_enabled = true
"#,
        );
        let err =
            load_with_env(&path, &no_overrides(), HashMap::new()).unwrap_err();
        match err {
            EvalConfigError::MissingEnv(name) => assert_eq!(name, "POSTGRES_URL"),
            _ => panic!("expected missing env error"),
        }
    }

    #[test]
    fn ollama_provider_is_rejected_for_eval_engine() {
        let (_dir, path) = write_config(
            r#"
[eval]
config_version = "v1"
run_type = "golden_dataset"

[judge]
provider = "ollama"
model_name = "qwen2.5"
tokenizer_source = "Qwen/Qwen2.5-1.5B-Instruct"
input_cost_per_million_tokens = 0.0
output_cost_per_million_tokens = 0.0

[postgres]
url_env = "POSTGRES_URL"

[artifacts]
root_dir = "Evidence/evals/runs"

[suites]
catalog_path = "prompts.json"

[observability]
tracing_enabled = true
metrics_enabled = true
"#,
        );
        let env = HashMap::from([(
            "POSTGRES_URL".to_string(),
            "postgres://postgres:postgres@localhost:5432/agentic_reasoning"
                .to_string(),
        )]);
        let err = load_with_env(&path, &no_overrides(), env).unwrap_err();
        match err {
            EvalConfigError::InvalidConfig(message) => {
                assert!(message.contains("unsupported judge provider"))
            }
            _ => panic!("expected invalid config error"),
        }
    }
}
