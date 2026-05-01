use std::path::PathBuf;

use clap::Parser;
use uuid::Uuid;

#[derive(Debug, Parser, PartialEq, Eq)]
#[command(about = "Distributed diagnostics offline eval engine")]
pub struct Cli {
    /// Path to the eval TOML config file
    #[arg(long)]
    pub config: PathBuf,

    /// Resume an existing eval run by id
    #[arg(long)]
    pub resume_eval_run_id: Option<Uuid>,

    /// Override the eval run type from config
    #[arg(long)]
    pub run_type: Option<String>,

    /// Limit how many runtime runs are absorbed into a new eval run
    #[arg(long)]
    pub limit: Option<usize>,

    /// Override the artifact root directory
    #[arg(long)]
    pub artifact_root: Option<PathBuf>,

    /// Restrict execution to one or more explicitly named suites
    #[arg(long)]
    pub enabled_suite: Vec<String>,

    /// Validate config and print the launch plan without mutating state
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalCliOverrides {
    pub run_type: Option<String>,
    pub limit: Option<usize>,
    pub artifact_root: Option<PathBuf>,
    pub enabled_suites: Option<Vec<String>>,
}

impl From<&Cli> for EvalCliOverrides {
    fn from(value: &Cli) -> Self {
        let enabled_suites = (!value.enabled_suite.is_empty())
            .then(|| value.enabled_suite.clone());
        Self {
            run_type: value.run_type.clone(),
            limit: value.limit,
            artifact_root: value.artifact_root.clone(),
            enabled_suites,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, EvalCliOverrides};
    use clap::Parser;
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn cli_accepts_required_config_path() {
        let cli =
            Cli::try_parse_from(["bin", "--config", "eval.toml"]).unwrap();
        assert_eq!(cli.config, PathBuf::from("eval.toml"));
        assert_eq!(cli.resume_eval_run_id, None);
        assert_eq!(cli.run_type, None);
    }

    #[test]
    fn cli_accepts_resume_and_run_type() {
        let resume_id = Uuid::nil();
        let cli = Cli::try_parse_from([
            "bin",
            "--config",
            "eval.toml",
            "--resume-eval-run-id",
            &resume_id.to_string(),
            "--run-type",
            "golden_dataset",
        ])
        .unwrap();
        assert_eq!(cli.config, PathBuf::from("eval.toml"));
        assert_eq!(cli.resume_eval_run_id, Some(resume_id));
        assert_eq!(cli.run_type.as_deref(), Some("golden_dataset"));
    }

    #[test]
    fn cli_requires_config() {
        let result = Cli::try_parse_from(["bin"]);
        assert!(result.is_err(), "missing --config must fail");
    }

    #[test]
    fn cli_accepts_dev_overrides() {
        let cli = Cli::try_parse_from([
            "bin",
            "--config",
            "eval.toml",
            "--limit",
            "5",
            "--artifact-root",
            "tmp/evals",
            "--enabled-suite",
            "final_no_root_cause_claim",
            "--enabled-suite",
            "final_first_check_discriminates",
            "--dry-run",
        ])
        .unwrap();
        assert_eq!(cli.limit, Some(5));
        assert_eq!(cli.artifact_root, Some(PathBuf::from("tmp/evals")));
        assert_eq!(
            cli.enabled_suite,
            vec![
                "final_no_root_cause_claim".to_string(),
                "final_first_check_discriminates".to_string()
            ]
        );
        assert!(cli.dry_run);
    }

    #[test]
    fn overrides_drop_empty_enabled_suite_list() {
        let cli =
            Cli::try_parse_from(["bin", "--config", "eval.toml"]).unwrap();
        let overrides = EvalCliOverrides::from(&cli);
        assert_eq!(overrides.enabled_suites, None);
    }
}
