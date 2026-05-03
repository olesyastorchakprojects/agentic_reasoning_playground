pub mod cli;
pub mod config;
pub mod observability;
pub mod judge;
pub mod manifest;
pub mod orchestrator;
pub mod report;
pub mod runtime_runs;
pub mod snapshot;
pub mod storage;
pub mod subject_preparation;
pub mod summary;
pub mod suites;

pub const CRATE_NAME: &str = "distributed_diagnostics_eval";
