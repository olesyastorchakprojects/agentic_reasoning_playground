use clap::Parser;
use distributed_diagnostics_eval::cli::{Cli, EvalCliOverrides};
use distributed_diagnostics_eval::config::load_eval_settings;
use distributed_diagnostics_eval::judge::TogetherJudgeClient;
use distributed_diagnostics_eval::manifest::{
    find_artifact_dir_for_eval_run, read_run_manifest,
};
use distributed_diagnostics_eval::observability::{eval_run_span, eval_summary_span, ObservabilityRuntime};
use distributed_diagnostics_eval::orchestrator::EvalOrchestrator;
use distributed_diagnostics_eval::report::write_run_report;
use distributed_diagnostics_eval::runtime_runs::PostgresRuntimeRunLoader;
use distributed_diagnostics_eval::storage::PostgresEvalStore;
use distributed_diagnostics_eval::summary::build_run_summary_row;
use distributed_diagnostics_eval::suites::JudgeSuiteCatalog;
use chrono::Utc;
use tracing::Instrument;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let overrides = EvalCliOverrides::from(&cli);
    let settings = load_eval_settings(&cli.config, &overrides)?;

    if cli.dry_run {
        println!("distributed_diagnostics_eval dry run");
        println!("config={}", cli.config.display());
        println!("run_type={}", settings.eval.run_type);
        println!("mode={}", settings.eval.mode);
        println!("artifact_root={}", settings.artifacts.root_dir.display());
        println!(
            "catalog_path={}",
            settings.suites.catalog_path.display()
        );
        println!(
            "enabled_suites={}",
            settings
                .suites
                .enabled
                .as_ref()
                .map(|v| v.join(","))
                .unwrap_or_else(|| "<catalog default>".to_string())
        );
        println!("tracing_enabled={}", settings.observability.tracing_enabled);
        return Ok(());
    }

    let _observability = ObservabilityRuntime::initialize(&settings.observability)
        .map_err(|e| format!("observability init failed: {e}"))?;

    let catalog = JudgeSuiteCatalog::load_from_path(&settings.suites.catalog_path)?;
    let enabled_suites = catalog.resolve_enabled_suite_names(&settings.suites)?;
    let runtime_loader = PostgresRuntimeRunLoader::new(&settings.postgres).await?;
    let judge_client = TogetherJudgeClient::from_settings(&settings.judge)?;
    let artifact_root = settings.artifacts.root_dir.clone();
    let store = PostgresEvalStore::new(&settings.postgres).await?;
    let orchestrator = EvalOrchestrator::new(settings, store);

    if let Some(eval_run_id) = &cli.resume_eval_run_id {
        println!("resume_eval_run_id={eval_run_id}");
        let artifact_dir =
            find_artifact_dir_for_eval_run(&artifact_root, *eval_run_id)?;
        let manifest = read_run_manifest(&artifact_dir)?;
        let drain = orchestrator
            .drain_judge_request_suites_for_eval_run(
                *eval_run_id,
                &runtime_loader,
                &catalog,
                &judge_client,
            )
            .await?;
        println!("attempted_subjects={}", drain.attempted_subjects);
        println!("completed_subjects={}", drain.completed_subjects);
        println!("failed_subjects={}", drain.failed_subjects);
        let summary_drain = orchestrator
            .drain_build_eval_summary_for_eval_run(*eval_run_id, &runtime_loader)
            .await?;
        println!(
            "summary_attempted_subjects={}",
            summary_drain.attempted_subjects
        );
        println!(
            "summary_completed_subjects={}",
            summary_drain.completed_subjects
        );
        let iteration_rows = orchestrator
            .store()
            .list_eval_iteration_summaries(*eval_run_id)
            .await?;
        let judge_calls = orchestrator
            .store()
            .list_judge_llm_calls_for_eval_run(*eval_run_id)
            .await?;
        let run_summary = build_run_summary_row(
            orchestrator.settings(),
            &manifest,
            &iteration_rows,
            "completed",
            Some(Utc::now()),
        );
        orchestrator
            .store()
            .upsert_eval_run_summary(&run_summary)
            .await?;
        let report_path =
            write_run_report(&artifact_dir, &manifest, &run_summary, &iteration_rows, &judge_calls, &catalog, &enabled_suites)?;
        println!("report_path={}", report_path.display());
        return Ok(());
    }

    let result = orchestrator.bootstrap_new_eval_run().await?;
    println!("bootstrapped eval_run_id={}", result.eval_run_id);
    println!("artifact_dir={}", result.artifact_dir.display());
    println!("manifest_path={}", result.manifest_path.display());
    println!("runtime_run_count={}", result.runtime_run_count);
    println!("subject_count={}", result.subject_count);

    let run_span = eval_run_span(
        &result.eval_run_id.to_string(),
        orchestrator.settings().eval.run_type.as_str(),
        orchestrator.settings().judge.model_name.as_str(),
    );
    run_span.record("eval.runtime_run_count", result.runtime_run_count as i64);

    let run_result = async {
        let drain = orchestrator
            .drain_judge_request_suites_for_eval_run(
                result.eval_run_id,
                &runtime_loader,
                &catalog,
                &judge_client,
            )
            .await?;
        println!("attempted_subjects={}", drain.attempted_subjects);
        println!("completed_subjects={}", drain.completed_subjects);
        println!("failed_subjects={}", drain.failed_subjects);

        let summary_span = eval_summary_span(
            &result.eval_run_id.to_string(),
            result.runtime_run_count,
        );
        let summary_drain = orchestrator
            .drain_build_eval_summary_for_eval_run(result.eval_run_id, &runtime_loader)
            .instrument(summary_span)
            .await?;
        println!("summary_attempted_subjects={}", summary_drain.attempted_subjects);
        println!("summary_completed_subjects={}", summary_drain.completed_subjects);

        let manifest = read_run_manifest(&result.artifact_dir)?;
        let iteration_rows = orchestrator
            .store()
            .list_eval_iteration_summaries(result.eval_run_id)
            .await?;
        let judge_calls = orchestrator
            .store()
            .list_judge_llm_calls_for_eval_run(result.eval_run_id)
            .await?;
        let run_summary = build_run_summary_row(
            orchestrator.settings(),
            &manifest,
            &iteration_rows,
            "completed",
            Some(Utc::now()),
        );
        orchestrator.store().upsert_eval_run_summary(&run_summary).await?;
        let report_path =
            write_run_report(&result.artifact_dir, &manifest, &run_summary, &iteration_rows, &judge_calls, &catalog, &enabled_suites)?;
        println!("report_path={}", report_path.display());
        Ok::<_, Box<dyn std::error::Error>>(iteration_rows.len())
    }
    .instrument(run_span.clone())
    .await;

    match &run_result {
        Ok(count) => { run_span.record("eval.iterations_evaluated_count", *count as i64); }
        Err(e) => {
            use distributed_diagnostics_eval::observability::record_error;
            record_error(&run_span, "EvalRunError", &e.to_string());
        }
    }
    _observability.flush();
    run_result.map(|_| ())
}
