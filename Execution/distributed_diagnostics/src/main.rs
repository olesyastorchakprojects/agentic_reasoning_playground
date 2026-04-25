use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use clap::Parser;
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
}

#[tokio::main]
async fn main() -> Result<(), RuntimeError> {
    let cli = Cli::parse();
    let settings = config::load(&cli.config, &cli.ingest_config).map_err(RuntimeError::Config)?;

    let _observability =
        ObservabilityRuntime::initialize(&settings.observability).map_err(RuntimeError::Observability)?;

    let orchestrator = startup::build_orchestrator(&settings).await?;

    let stdin = io::stdin();
    let stdout = io::stdout();

    loop {
        {
            let mut out = stdout.lock();
            write!(out, "> ").expect("stdout write failed");
            out.flush().expect("stdout flush failed");
        }

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("read error: {e}");
                break;
            }
        }

        let query = line.trim().to_string();

        if query.is_empty() || query == "exit" {
            break;
        }

        let request = UserRequest { query };

        match orchestrator.run(request).await {
            Ok(RunOutcome::Finished { result, .. }) => {
                let r = &result.response;
                println!("Problem: {}", r.problem_understanding);
                println!("Context: {}", r.similar_practical_context);
                if !r.active_hypotheses.is_empty() {
                    println!("Hypotheses:");
                    for h in &r.active_hypotheses {
                        println!("  - {h}");
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
            Ok(RunOutcome::Failed { error, .. }) => {
                eprintln!("Run failed: {error}");
            }
            Err(e) => {
                eprintln!("Orchestrator error: {e}");
            }
        }

        println!();
    }

    Ok(())
}
