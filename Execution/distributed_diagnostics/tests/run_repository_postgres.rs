#![cfg(feature = "postgres-integration")]

use chrono::Utc;
use distributed_diagnostics::api_clients::postgres::{
    PostgresRunStateStore, PostgresRunStateStoreConfig,
};
use distributed_diagnostics::orchestrator::run_repository::{
    RunListItem, RunRepository, RunRepositoryError,
};
use distributed_diagnostics::orchestrator::run_state::model::{
    FinishedStepRecord, PendingStepRecord, RunId, RunIteration, RunIterationId, RunState,
    RunStatus, StepKind, StepRecord, StepRecordId, StepResultEnvelope,
};
use distributed_diagnostics::shared_types::{
    DiagnosticResponse, DiagnosticResultInterpretation, NormalizedUserRequest,
    ResponseValidationAndNormalizationOutput, UserRequest,
};
use sqlx::postgres::PgPoolOptions;
use std::sync::OnceLock;
use tokio::sync::{Mutex, MutexGuard};
use url::Url;
use uuid::Uuid;

fn new_run_id() -> RunId {
    RunId(Uuid::new_v4())
}

fn new_iteration_id() -> RunIterationId {
    RunIterationId(Uuid::new_v4())
}

fn new_record_id() -> StepRecordId {
    StepRecordId(Uuid::new_v4())
}

fn valid_run() -> RunState {
    let now = Utc::now();
    RunState {
        run_id: new_run_id(),
        status: RunStatus::Active,
        created_at: now,
        updated_at: now,
        revision: 0,
        iterations: vec![],
    }
}

fn user_input_envelope(query: &str) -> StepResultEnvelope {
    StepResultEnvelope::UserInputReceived(UserRequest {
        query: query.to_string(),
    })
}

fn normalization_envelope() -> StepResultEnvelope {
    StepResultEnvelope::InputNormalization(NormalizedUserRequest {
        query: "service down".to_string(),
        input_token_count: 2,
    })
}

fn final_response_envelope(problem_understanding: &str) -> StepResultEnvelope {
    StepResultEnvelope::ResponseValidationAndNormalization(
        ResponseValidationAndNormalizationOutput {
            response: DiagnosticResponse {
                problem_understanding: problem_understanding.to_string(),
                similar_practical_context: "similar context".to_string(),
                active_hypotheses: vec!["hypothesis".to_string()],
                first_check: "check replicas".to_string(),
                result_interpretation: DiagnosticResultInterpretation {
                    supports_primary_if: "supports primary".to_string(),
                    supports_competing_if: "supports competing".to_string(),
                    inconclusive_if: None,
                },
                competing_interpretation: None,
            },
        },
    )
}

fn test_database_url() -> String {
    static ENV_INIT: OnceLock<()> = OnceLock::new();
    ENV_INIT.get_or_init(|| {
        let _ = dotenvy::from_filename("Execution/distributed_diagnostics/.env.test");
        let _ = dotenvy::from_filename(".env.test");
    });

    std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must point to postgres://.../distributed_diagnostics_test")
}

async fn connect_repository() -> RunRepository {
    let url = test_database_url();
    let parsed = Url::parse(&url).expect("TEST_DATABASE_URL must be a valid URL");
    assert_eq!(
        parsed.path().trim_start_matches('/'),
        "distributed_diagnostics_test",
        "TEST_DATABASE_URL must point to the dedicated distributed_diagnostics_test database",
    );

    let store = PostgresRunStateStore::new(PostgresRunStateStoreConfig { postgres_url: url })
        .await
        .expect("connect to test database");
    RunRepository::new(store)
}

async fn cleanup_run(database_url: &str, run_id: RunId) {
    let pool = PgPoolOptions::new()
        .connect(database_url)
        .await
        .expect("connect cleanup pool");
    let _ = sqlx::query("DELETE FROM diagnostics.runs WHERE run_id = $1::uuid")
        .bind(run_id.0.to_string())
        .execute(&pool)
        .await;
}

async fn cleanup_all_runs(database_url: &str) {
    let pool = PgPoolOptions::new()
        .connect(database_url)
        .await
        .expect("connect cleanup pool");
    let _ = sqlx::query("DELETE FROM diagnostics.runs")
        .execute(&pool)
        .await;
}

async fn test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().await
}

#[tokio::test]
async fn create_run_maps_duplicate_run_store_failure() {
    let _guard = test_lock().await;
    let database_url = test_database_url();
    cleanup_all_runs(&database_url).await;
    let repository = connect_repository().await;
    let run = valid_run();

    repository.create_run(&run).await.expect("first create_run");
    let err = repository.create_run(&run).await.unwrap_err();
    assert!(matches!(err, RunRepositoryError::DuplicateRun { .. }));

    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn load_run_returns_none_when_run_does_not_exist() {
    let _guard = test_lock().await;
    let database_url = test_database_url();
    cleanup_all_runs(&database_url).await;
    let repository = connect_repository().await;
    let result = repository.load_run(new_run_id()).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn append_iteration_persists_exactly_one_iteration_and_updates_header() {
    let _guard = test_lock().await;
    let database_url = test_database_url();
    cleanup_all_runs(&database_url).await;
    let repository = connect_repository().await;
    let initial = valid_run();
    repository.create_run(&initial).await.unwrap();

    let iteration = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    let run = RunState {
        run_id: initial.run_id,
        status: RunStatus::WaitingForUser,
        created_at: initial.created_at,
        updated_at: Utc::now(),
        revision: 1,
        iterations: vec![iteration.clone()],
    };
    repository.append_iteration(&run, 0, &iteration).await.unwrap();

    let loaded = repository.load_run(initial.run_id).await.unwrap().unwrap();
    assert_eq!(loaded.status, RunStatus::WaitingForUser);
    assert_eq!(loaded.revision, 1);
    assert_eq!(loaded.iterations.len(), 1);
    assert_eq!(loaded.iterations[0].iteration_id, iteration.iteration_id);

    cleanup_run(&database_url, initial.run_id).await;
}

#[tokio::test]
async fn append_step_record_persists_exactly_one_step_record_and_updates_header() {
    let _guard = test_lock().await;
    let database_url = test_database_url();
    cleanup_all_runs(&database_url).await;
    let repository = connect_repository().await;
    let initial = valid_run();
    repository.create_run(&initial).await.unwrap();

    let iteration = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    let after_iteration = RunState {
        run_id: initial.run_id,
        status: RunStatus::Active,
        created_at: initial.created_at,
        updated_at: Utc::now(),
        revision: 1,
        iterations: vec![iteration.clone()],
    };
    repository
        .append_iteration(&after_iteration, 0, &iteration)
        .await
        .unwrap();

    let step_record = StepRecord::Pending(PendingStepRecord {
        record_id: new_record_id(),
        step: StepKind::InputNormalization,
        started_at: Utc::now(),
    });
    let after_step = RunState {
        run_id: initial.run_id,
        status: RunStatus::Error,
        created_at: initial.created_at,
        updated_at: Utc::now(),
        revision: 2,
        iterations: vec![iteration.clone()],
    };
    repository
        .append_step_record(&after_step, iteration.iteration_id, 0, &step_record)
        .await
        .unwrap();

    let loaded = repository.load_run(initial.run_id).await.unwrap().unwrap();
    assert_eq!(loaded.status, RunStatus::Error);
    assert_eq!(loaded.revision, 2);
    assert_eq!(loaded.iterations[0].step_records.len(), 1);

    cleanup_run(&database_url, initial.run_id).await;
}

#[tokio::test]
async fn finish_step_record_finishes_existing_pending_record_and_updates_header() {
    let _guard = test_lock().await;
    let database_url = test_database_url();
    cleanup_all_runs(&database_url).await;
    let repository = connect_repository().await;
    let initial = valid_run();
    repository.create_run(&initial).await.unwrap();

    let iteration = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    let after_iteration = RunState {
        run_id: initial.run_id,
        status: RunStatus::Active,
        created_at: initial.created_at,
        updated_at: Utc::now(),
        revision: 1,
        iterations: vec![iteration.clone()],
    };
    repository
        .append_iteration(&after_iteration, 0, &iteration)
        .await
        .unwrap();

    let record_id = new_record_id();
    let pending = StepRecord::Pending(PendingStepRecord {
        record_id,
        step: StepKind::InputNormalization,
        started_at: Utc::now(),
    });
    let after_pending = RunState {
        run_id: initial.run_id,
        status: RunStatus::Active,
        created_at: initial.created_at,
        updated_at: Utc::now(),
        revision: 2,
        iterations: vec![iteration.clone()],
    };
    repository
        .append_step_record(&after_pending, iteration.iteration_id, 0, &pending)
        .await
        .unwrap();

    let finished = FinishedStepRecord {
        record_id,
        step: StepKind::InputNormalization,
        started_at: Utc::now(),
        finished_at: Utc::now(),
        result: Ok(normalization_envelope()),
    };
    let after_finish = RunState {
        run_id: initial.run_id,
        status: RunStatus::WaitingForUser,
        created_at: initial.created_at,
        updated_at: Utc::now(),
        revision: 3,
        iterations: vec![iteration.clone()],
    };
    repository
        .finish_step_record(&after_finish, record_id, &finished)
        .await
        .unwrap();

    let loaded = repository.load_run(initial.run_id).await.unwrap().unwrap();
    assert_eq!(loaded.status, RunStatus::WaitingForUser);
    assert_eq!(loaded.revision, 3);
    assert!(matches!(
        &loaded.iterations[0].step_records[0],
        StepRecord::Finished(_)
    ));

    cleanup_run(&database_url, initial.run_id).await;
}

#[tokio::test]
async fn update_run_header_updates_only_the_header() {
    let _guard = test_lock().await;
    let database_url = test_database_url();
    cleanup_all_runs(&database_url).await;
    let repository = connect_repository().await;
    let initial = valid_run();
    repository.create_run(&initial).await.unwrap();

    let updated = RunState {
        run_id: initial.run_id,
        status: RunStatus::Archived,
        created_at: initial.created_at,
        updated_at: Utc::now(),
        revision: 7,
        iterations: vec![],
    };
    repository.update_run_header(&updated).await.unwrap();

    let loaded = repository.load_run(initial.run_id).await.unwrap().unwrap();
    assert_eq!(loaded.status, RunStatus::Archived);
    assert_eq!(loaded.revision, 7);
    assert!(loaded.iterations.is_empty());

    cleanup_run(&database_url, initial.run_id).await;
}

#[tokio::test]
async fn list_runs_returns_rows_ordered_by_created_at_desc_and_derives_projection_fields() {
    let _guard = test_lock().await;
    let database_url = test_database_url();
    cleanup_all_runs(&database_url).await;
    let repository = connect_repository().await;

    let older = RunState {
        created_at: Utc::now() - chrono::Duration::seconds(10),
        updated_at: Utc::now() - chrono::Duration::seconds(10),
        ..valid_run()
    };
    let newer = RunState {
        created_at: Utc::now(),
        updated_at: Utc::now(),
        ..valid_run()
    };

    repository.create_run(&older).await.unwrap();
    repository.create_run(&newer).await.unwrap();

    let older_iteration = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    repository
        .append_iteration(
            &RunState {
                run_id: older.run_id,
                status: RunStatus::Active,
                created_at: older.created_at,
                updated_at: Utc::now(),
                revision: 1,
                iterations: vec![older_iteration.clone()],
            },
            0,
            &older_iteration,
        )
        .await
        .unwrap();
    repository
        .append_step_record(
            &RunState {
                run_id: older.run_id,
                status: RunStatus::Active,
                created_at: older.created_at,
                updated_at: Utc::now(),
                revision: 2,
                iterations: vec![older_iteration.clone()],
            },
            older_iteration.iteration_id,
            0,
            &StepRecord::Finished(FinishedStepRecord {
                record_id: new_record_id(),
                step: StepKind::UserInputReceived,
                started_at: Utc::now(),
                finished_at: Utc::now(),
                result: Ok(user_input_envelope("older query")),
            }),
        )
        .await
        .unwrap();

    let newer_iteration = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    repository
        .append_iteration(
            &RunState {
                run_id: newer.run_id,
                status: RunStatus::Active,
                created_at: newer.created_at,
                updated_at: Utc::now(),
                revision: 1,
                iterations: vec![newer_iteration.clone()],
            },
            0,
            &newer_iteration,
        )
        .await
        .unwrap();
    repository
        .append_step_record(
            &RunState {
                run_id: newer.run_id,
                status: RunStatus::Active,
                created_at: newer.created_at,
                updated_at: Utc::now(),
                revision: 2,
                iterations: vec![newer_iteration.clone()],
            },
            newer_iteration.iteration_id,
            0,
            &StepRecord::Finished(FinishedStepRecord {
                record_id: new_record_id(),
                step: StepKind::UserInputReceived,
                started_at: Utc::now(),
                finished_at: Utc::now(),
                result: Ok(user_input_envelope("newer query")),
            }),
        )
        .await
        .unwrap();
    repository
        .append_step_record(
            &RunState {
                run_id: newer.run_id,
                status: RunStatus::WaitingForUser,
                created_at: newer.created_at,
                updated_at: Utc::now(),
                revision: 3,
                iterations: vec![newer_iteration.clone()],
            },
            newer_iteration.iteration_id,
            1,
            &StepRecord::Finished(FinishedStepRecord {
                record_id: new_record_id(),
                step: StepKind::ResponseValidationAndNormalization,
                started_at: Utc::now(),
                finished_at: Utc::now(),
                result: Ok(final_response_envelope("newer understanding")),
            }),
        )
        .await
        .unwrap();

    let runs = repository.list_runs().await.unwrap();
    assert!(!runs.is_empty());

    let newer_pos = runs.iter().position(|item| item.run_id == newer.run_id).unwrap();
    let older_pos = runs.iter().position(|item| item.run_id == older.run_id).unwrap();
    assert!(newer_pos < older_pos);

    let newer_item: &RunListItem = runs.iter().find(|item| item.run_id == newer.run_id).unwrap();
    assert_eq!(newer_item.initial_user_query, "newer query");
    assert_eq!(
        newer_item.final_problem_understanding.as_deref(),
        Some("newer understanding")
    );

    cleanup_run(&database_url, older.run_id).await;
    cleanup_run(&database_url, newer.run_id).await;
}

#[tokio::test]
async fn list_runs_returns_missing_initial_user_query_when_summary_lacks_it() {
    let _guard = test_lock().await;
    let database_url = test_database_url();
    cleanup_all_runs(&database_url).await;
    let repository = connect_repository().await;
    let run = valid_run();
    repository.create_run(&run).await.unwrap();

    let err = repository.list_runs().await.unwrap_err();
    assert!(matches!(
        err,
        RunRepositoryError::MissingInitialUserQuery { run_id } if run_id == run.run_id
    ));

    cleanup_run(&database_url, run.run_id).await;
}
