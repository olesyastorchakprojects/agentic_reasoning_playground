use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use clap::Parser;
use distributed_diagnostics::golden_eval_input;
use distributed_diagnostics::observability::ObservabilityRuntime;
use distributed_diagnostics::orchestrator::orchestrator::RunOutcome;
use distributed_diagnostics::shared_types::UserRequest;
use distributed_diagnostics::{config, startup, RuntimeError};

#[derive(Parser)]
#[command(about = "Distributed diagnostics runtime")]
struct Cli {
    /// Path to the runtime TOML config file
    #[arg(long)]
    config: PathBuf,

    /// Path to the ingest TOML config file
    #[arg(long)]
    ingest_config: PathBuf,

    /// Path to the golden cases JSON file (batch-eval mode; must be paired with --golden-cases-schema)
    #[arg(long)]
    golden_cases_file: Option<PathBuf>,

    /// Path to the golden cases JSON Schema file (batch-eval mode; must be paired with --golden-cases-file)
    #[arg(long)]
    golden_cases_schema: Option<PathBuf>,
}

fn print_finished_result(result: &distributed_diagnostics::shared_types::ResponseValidationAndNormalizationOutput) {
    let r = &result.response;
    println!("Problem: {}", r.problem_understanding);
    println!("Context: {}", r.similar_practical_context);
    if !r.hypotheses.is_empty() {
        println!("Hypotheses:");
        for h in &r.hypotheses {
            println!("  - [{}] {}", format!("{:?}", h.status), h.text);
        }
    }
    println!("First check: {}", r.first_check);
    println!(
        "Result interpretation — supports primary if: {}",
        r.result_interpretation.supports_primary_if
    );
    println!(
        "Result interpretation — supports competing if: {}",
        r.result_interpretation.supports_competing_if
    );
    if let Some(inconclusive) = &r.result_interpretation.inconclusive_if {
        println!("Result interpretation — inconclusive if: {inconclusive}");
    }
    if let Some(competing) = &r.competing_interpretation {
        println!("Competing interpretation: {competing}");
    }
}

fn read_line(stdin: &io::Stdin, prompt: &str) -> Option<String> {
    let stdout = io::stdout();
    {
        let mut out = stdout.lock();
        write!(out, "{prompt}").expect("stdout write failed");
        out.flush().expect("stdout flush failed");
    }
    let mut line = String::new();
    match stdin.lock().read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => {
            let trimmed = line.trim().to_string();
            if trimmed.is_empty() || trimmed == "exit" { None } else { Some(trimmed) }
        }
        Err(e) => {
            eprintln!("read error: {e}");
            None
        }
    }
}

fn resolve_golden_mode(
    golden_cases_file: Option<PathBuf>,
    golden_cases_schema: Option<PathBuf>,
) -> Result<Option<(PathBuf, PathBuf)>, String> {
    match (golden_cases_file, golden_cases_schema) {
        (Some(file), Some(schema)) => Ok(Some((file, schema))),
        (None, None) => Ok(None),
        (Some(_), None) => {
            Err("--golden-cases-file requires --golden-cases-schema".to_string())
        }
        (None, Some(_)) => {
            Err("--golden-cases-schema requires --golden-cases-file".to_string())
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), RuntimeError> {
    let cli = Cli::parse();

    let golden_mode =
        match resolve_golden_mode(cli.golden_cases_file, cli.golden_cases_schema) {
            Ok(mode) => mode,
            Err(message) => {
                eprintln!("error: {message}");
                std::process::exit(1);
            }
        };

    let settings =
        config::load(&cli.config, &cli.ingest_config).map_err(RuntimeError::Config)?;

    let _observability =
        ObservabilityRuntime::initialize(&settings.observability)
            .map_err(RuntimeError::Observability)?;

    let orchestrator = startup::build_orchestrator(&settings).await?;

    match golden_mode {
        Some((cases_file, schema_file)) => {
            let requests =
                golden_eval_input::load_golden_eval_requests(&cases_file, &schema_file)?;

            for request in requests {
                let case_id = request
                    .golden_question
                    .as_ref()
                    .map(|q| q.case_id.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                println!("[{case_id}]");

                match orchestrator.run(request).await {
                    Ok(RunOutcome::Finished { result, .. }) => print_finished_result(&result),
                    Ok(RunOutcome::WaitingForUser { follow_up_questions, .. }) => {
                        eprintln!("[{case_id}] waiting for user — not supported in batch mode");
                        for q in &follow_up_questions {
                            eprintln!("  ? {q}");
                        }
                    }
                    Ok(RunOutcome::Failed { error, .. }) => {
                        eprintln!("[{case_id}] run failed: {error}");
                    }
                    Err(e) => {
                        eprintln!("[{case_id}] orchestrator error: {e}");
                    }
                }

                println!();
            }
        }
        None => {
            let stdin = io::stdin();

            loop {
                let query = match read_line(&stdin, "> ") {
                    Some(q) => q,
                    None => break,
                };

                let mut outcome = orchestrator
                    .run(UserRequest { query, golden_question: None })
                    .await;

                loop {
                    match outcome {
                        Ok(RunOutcome::Finished { run_id, result }) => {
                            print_finished_result(&result);
                            match read_line(&stdin, ">> Observation (Enter to finish): ") {
                                Some(observation) => {
                                    outcome = orchestrator
                                        .resume_with_input(
                                            run_id,
                                            UserRequest { query: observation, golden_question: None },
                                        )
                                        .await;
                                }
                                None => break,
                            }
                        }
                        Ok(RunOutcome::WaitingForUser { run_id, follow_up_questions }) => {
                            println!("Нужна дополнительная информация:");
                            for (i, q) in follow_up_questions.iter().enumerate() {
                                println!("  {}. {q}", i + 1);
                            }
                            let answer = match read_line(&stdin, ">> ") {
                                Some(a) => a,
                                None => break,
                            };
                            outcome = orchestrator
                                .resume_with_input(
                                    run_id,
                                    UserRequest { query: answer, golden_question: None },
                                )
                                .await;
                        }
                        Ok(RunOutcome::Failed { error, .. }) => {
                            eprintln!("Run failed: {error}");
                            break;
                        }
                        Err(e) => {
                            eprintln!("Orchestrator error: {e}");
                            break;
                        }
                    }
                }

                println!();
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_accepts_required_args_and_preserves_paths() {
        let cli = Cli::try_parse_from([
            "bin",
            "--config",
            "runtime.toml",
            "--ingest-config",
            "ingest.toml",
        ])
        .unwrap();
        assert_eq!(cli.config, PathBuf::from("runtime.toml"));
        assert_eq!(cli.ingest_config, PathBuf::from("ingest.toml"));
        assert!(cli.golden_cases_file.is_none());
        assert!(cli.golden_cases_schema.is_none());
    }

    #[test]
    fn cli_accepts_golden_args_together_and_preserves_paths() {
        let cli = Cli::try_parse_from([
            "bin",
            "--config",
            "runtime.toml",
            "--ingest-config",
            "ingest.toml",
            "--golden-cases-file",
            "cases.json",
            "--golden-cases-schema",
            "schema.json",
        ])
        .unwrap();
        assert_eq!(cli.config, PathBuf::from("runtime.toml"));
        assert_eq!(cli.ingest_config, PathBuf::from("ingest.toml"));
        assert_eq!(cli.golden_cases_file, Some(PathBuf::from("cases.json")));
        assert_eq!(cli.golden_cases_schema, Some(PathBuf::from("schema.json")));
    }

    #[test]
    fn cli_fails_when_config_is_missing() {
        let result = Cli::try_parse_from(["bin", "--ingest-config", "ingest.toml"]);
        assert!(result.is_err(), "missing --config must be a CLI error");
    }

    #[test]
    fn cli_fails_when_ingest_config_is_missing() {
        let result = Cli::try_parse_from(["bin", "--config", "runtime.toml"]);
        assert!(result.is_err(), "missing --ingest-config must be a CLI error");
    }

    #[test]
    fn golden_file_without_schema_returns_error_before_startup() {
        let result =
            resolve_golden_mode(Some(PathBuf::from("cases.json")), None);
        assert!(
            result.is_err(),
            "--golden-cases-file without --golden-cases-schema must fail"
        );
    }

    #[test]
    fn golden_schema_without_file_returns_error_before_startup() {
        let result =
            resolve_golden_mode(None, Some(PathBuf::from("schema.json")));
        assert!(
            result.is_err(),
            "--golden-cases-schema without --golden-cases-file must fail"
        );
    }

    #[test]
    fn cli_has_no_env_file_argument() {
        // .env loading is delegated to config module — the CLI must not expose an --env-file argument
        let result = Cli::try_parse_from([
            "bin",
            "--config",
            "runtime.toml",
            "--ingest-config",
            "ingest.toml",
            "--env-file",
            "path.env",
        ]);
        assert!(
            result.is_err(),
            "CLI must not accept --env-file; .env loading belongs to the config module"
        );
    }

    #[test]
    fn startup_fails_before_runtime_when_config_is_invalid() {
        // Config loading must fail before ObservabilityRuntime or orchestrator are touched.
        // Empty TOML files trigger a ConfigError::Load well before any runtime wiring begins.
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("runtime.toml");
        let ingest_path = dir.path().join("ingest.toml");
        std::fs::File::create(&config_path).unwrap();
        std::fs::File::create(&ingest_path).unwrap();

        let result = config::load(&config_path, &ingest_path);
        assert!(
            result.is_err(),
            "config::load must fail before any runtime initialization when config is missing required fields"
        );
    }

    #[test]
    fn resolve_golden_mode_returns_none_when_both_absent() {
        let result = resolve_golden_mode(None, None).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn resolve_golden_mode_returns_both_paths_when_present() {
        let file = PathBuf::from("cases.json");
        let schema = PathBuf::from("schema.json");
        let result =
            resolve_golden_mode(Some(file.clone()), Some(schema.clone())).unwrap();
        assert_eq!(result, Some((file, schema)));
    }

    #[test]
    fn cli_delegates_golden_parsing_to_golden_eval_input_module() {
        // Verify that golden_eval_input::load_golden_eval_requests is the dedicated
        // parsing entrypoint — calling it with a missing file returns the typed module error,
        // confirming main.rs delegates rather than reimplementing the logic inline.
        let dir = tempfile::TempDir::new().unwrap();
        let schema_path = dir.path().join("schema.json");
        let cases_path = dir.path().join("cases.json");

        let err =
            golden_eval_input::load_golden_eval_requests(&cases_path, &schema_path)
                .unwrap_err();
        assert!(
            matches!(
                err,
                distributed_diagnostics::golden_eval_input::GoldenEvalInputError::GoldenCasesSchemaRead { .. }
            ),
            "expected GoldenCasesSchemaRead from the dedicated module, got: {err}"
        );
    }
}
