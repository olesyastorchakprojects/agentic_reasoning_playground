use std::path::PathBuf;

use clap::Parser;
use distributed_diagnostics::{config, RuntimeError};

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
    let _settings =
        config::load(&cli.config, &cli.ingest_config).map_err(RuntimeError::Config)?;
    Ok(())
}
