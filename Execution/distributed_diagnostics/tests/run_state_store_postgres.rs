#![cfg(feature = "postgres-integration")]

use chrono::Utc;
use distributed_diagnostics::api_clients::postgres::{
    PostgresRunStateStore, PostgresRunStateStoreConfig, RunStateStoreError,
};
use distributed_diagnostics::orchestrator::run_state::model::{
    FinishedStepRecord, PendingStepRecord, RunId, RunIteration, RunIterationId, RunState,
    RunStatus, StepError, StepKind, StepRecord, StepRecordId, StepResultEnvelope,
};
use distributed_diagnostics::shared_types::{
    DiagnosticResponse, DiagnosticResultInterpretation, NormalizedUserRequest,
    ResponseValidationAndNormalizationOutput, UserRequest,
};
use sqlx::postgres::PgPoolOptions;
use std::sync::OnceLock;
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

fn user_input_envelope() -> StepResultEnvelope {
    StepResultEnvelope::UserInputReceived(UserRequest {
        query: "service down".to_string(),
    })
}

fn input_normalization_envelope() -> StepResultEnvelope {
    StepResultEnvelope::InputNormalization(NormalizedUserRequest {
        query: "service down".to_string(),
        input_token_count: 2,
    })
}

fn missing_input_error() -> StepError {
    StepError::MissingRequiredInput {
        message: "test failure".to_string(),
    }
}

fn final_response_envelope(problem_understanding: &str) -> StepResultEnvelope {
    StepResultEnvelope::ResponseValidationAndNormalization(
        ResponseValidationAndNormalizationOutput {
            response: DiagnosticResponse {
                problem_understanding: problem_understanding.to_string(),
                similar_practical_context: "similar context".to_string(),
                active_hypotheses: vec!["hypothesis".to_string()],
                first_check: "check replica lag".to_string(),
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

async fn connect_test_store() -> PostgresRunStateStore {
    let url = test_database_url();
    let parsed = Url::parse(&url).expect("TEST_DATABASE_URL must be a valid URL");
    assert_eq!(
        parsed.path().trim_start_matches('/'),
        "distributed_diagnostics_test",
        "TEST_DATABASE_URL must point to the dedicated distributed_diagnostics_test database",
    );

    PostgresRunStateStore::new(PostgresRunStateStoreConfig { postgres_url: url })
        .await
        .expect("connect to test database")
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

#[tokio::test]
async fn insert_run_writes_only_canonical_header_fields() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.expect("insert_run must succeed");

    let loaded = store
        .load_run(run.run_id)
        .await
        .expect("load_run")
        .expect("run must be found");
    assert_eq!(loaded.run_id, run.run_id);
    assert_eq!(loaded.status, run.status);
    assert_eq!(loaded.revision, run.revision);
    assert!(loaded.iterations.is_empty(), "no iterations must be written");

    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn duplicate_run_id_insert_fails_with_duplicate_run_error() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.expect("first insert");
    let err = store.insert_run(&run).await.unwrap_err();
    assert!(
        matches!(err, RunStateStoreError::DuplicateRun(_)),
        "expected DuplicateRun, got: {err:?}"
    );
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn insert_iteration_preserves_sequence_no() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();

    let iter = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store
        .insert_iteration(run.run_id, 7, &iter)
        .await
        .expect("insert_iteration");

    let loaded = store.load_run(run.run_id).await.unwrap().unwrap();
    assert_eq!(loaded.iterations.len(), 1);
    assert_eq!(loaded.iterations[0].iteration_id, iter.iteration_id);
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn insert_iteration_fails_when_parent_run_does_not_exist() {
    let store = connect_test_store().await;
    let ghost_run_id = new_run_id();
    let iter = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    let err = store
        .insert_iteration(ghost_run_id, 0, &iter)
        .await
        .unwrap_err();
    assert!(matches!(err, RunStateStoreError::MissingParentRun(_)));
}

#[tokio::test]
async fn duplicate_iteration_sequence_fails_with_duplicate_iteration_error() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();
    let iter = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store.insert_iteration(run.run_id, 0, &iter).await.unwrap();
    let iter2 = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    let err = store
        .insert_iteration(run.run_id, 0, &iter2)
        .await
        .unwrap_err();
    assert!(matches!(err, RunStateStoreError::DuplicateIteration { .. }));
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn insert_step_record_serializes_ok_payload_into_result_json() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();
    let iter = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store.insert_iteration(run.run_id, 0, &iter).await.unwrap();

    let now = Utc::now();
    let record = StepRecord::Finished(FinishedStepRecord {
        record_id: new_record_id(),
        step: StepKind::UserInputReceived,
        started_at: now,
        finished_at: now,
        result: Ok(user_input_envelope()),
    });
    store
        .insert_step_record(iter.iteration_id, 0, &record)
        .await
        .expect("insert_step_record");

    let loaded = store.load_run(run.run_id).await.unwrap().unwrap();
    assert_eq!(loaded.iterations[0].step_records.len(), 1);
    if let StepRecord::Finished(f) = &loaded.iterations[0].step_records[0] {
        assert!(f.result.is_ok());
        assert!(matches!(
            f.result.as_ref().unwrap(),
            StepResultEnvelope::UserInputReceived(_)
        ));
    } else {
        panic!("expected finished record");
    }
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn insert_step_record_serializes_err_payload_into_error_json() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();
    let iter = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store.insert_iteration(run.run_id, 0, &iter).await.unwrap();

    let now = Utc::now();
    let record = StepRecord::Finished(FinishedStepRecord {
        record_id: new_record_id(),
        step: StepKind::InputNormalization,
        started_at: now,
        finished_at: now,
        result: Err(missing_input_error()),
    });
    store
        .insert_step_record(iter.iteration_id, 0, &record)
        .await
        .unwrap();

    let loaded = store.load_run(run.run_id).await.unwrap().unwrap();
    if let StepRecord::Finished(f) = &loaded.iterations[0].step_records[0] {
        assert!(f.result.is_err());
    } else {
        panic!("expected finished record");
    }
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn insert_step_record_fails_when_parent_iteration_does_not_exist() {
    let store = connect_test_store().await;
    let ghost_iter_id = new_iteration_id();
    let now = Utc::now();
    let record = StepRecord::Pending(PendingStepRecord {
        record_id: new_record_id(),
        step: StepKind::InputNormalization,
        started_at: now,
    });
    let err = store
        .insert_step_record(ghost_iter_id, 0, &record)
        .await
        .unwrap_err();
    assert!(matches!(err, RunStateStoreError::MissingParentIteration(_)));
}

#[tokio::test]
async fn duplicate_step_record_sequence_fails_with_duplicate_step_record_error() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();
    let iter = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store.insert_iteration(run.run_id, 0, &iter).await.unwrap();
    let now = Utc::now();
    let make_pending = || {
        StepRecord::Pending(PendingStepRecord {
            record_id: new_record_id(),
            step: StepKind::InputNormalization,
            started_at: now,
        })
    };
    store
        .insert_step_record(iter.iteration_id, 0, &make_pending())
        .await
        .unwrap();
    let err = store
        .insert_step_record(iter.iteration_id, 0, &make_pending())
        .await
        .unwrap_err();
    assert!(matches!(err, RunStateStoreError::DuplicateStepRecord { .. }));
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn finish_step_record_updates_pending_to_finished_and_preserves_identity() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();
    let iter = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store.insert_iteration(run.run_id, 0, &iter).await.unwrap();

    let now = Utc::now();
    let record_id = new_record_id();
    let pending = StepRecord::Pending(PendingStepRecord {
        record_id,
        step: StepKind::UserInputReceived,
        started_at: now,
    });
    store
        .insert_step_record(iter.iteration_id, 0, &pending)
        .await
        .unwrap();

    let finished_record = FinishedStepRecord {
        record_id,
        step: StepKind::UserInputReceived,
        started_at: now,
        finished_at: now,
        result: Ok(user_input_envelope()),
    };
    store
        .finish_step_record(record_id, &finished_record)
        .await
        .expect("finish_step_record");

    let loaded = store.load_run(run.run_id).await.unwrap().unwrap();
    if let StepRecord::Finished(f) = &loaded.iterations[0].step_records[0] {
        assert_eq!(f.record_id, record_id);
        assert_eq!(f.step, StepKind::UserInputReceived);
    } else {
        panic!("expected finished record");
    }
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn finish_step_record_fails_with_not_found_when_record_id_absent() {
    let store = connect_test_store().await;
    let ghost_id = new_record_id();
    let now = Utc::now();
    let finished_record = FinishedStepRecord {
        record_id: ghost_id,
        step: StepKind::InputNormalization,
        started_at: now,
        finished_at: now,
        result: Ok(input_normalization_envelope()),
    };
    let err = store
        .finish_step_record(ghost_id, &finished_record)
        .await
        .unwrap_err();
    assert!(matches!(err, RunStateStoreError::StepRecordNotFound(_)));
}

#[tokio::test]
async fn finish_step_record_fails_with_already_finished_when_row_is_finished() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();
    let iter = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store.insert_iteration(run.run_id, 0, &iter).await.unwrap();

    let now = Utc::now();
    let record_id = new_record_id();
    let pending = StepRecord::Pending(PendingStepRecord {
        record_id,
        step: StepKind::UserInputReceived,
        started_at: now,
    });
    store
        .insert_step_record(iter.iteration_id, 0, &pending)
        .await
        .unwrap();

    let finished_record = FinishedStepRecord {
        record_id,
        step: StepKind::UserInputReceived,
        started_at: now,
        finished_at: now,
        result: Ok(user_input_envelope()),
    };
    store
        .finish_step_record(record_id, &finished_record)
        .await
        .unwrap();
    let err = store
        .finish_step_record(record_id, &finished_record)
        .await
        .unwrap_err();
    assert!(matches!(err, RunStateStoreError::StepRecordAlreadyFinished(_)));
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn finish_step_record_fails_when_step_kind_mismatches_stored_row() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();
    let iter = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store.insert_iteration(run.run_id, 0, &iter).await.unwrap();

    let now = Utc::now();
    let record_id = new_record_id();
    let pending = StepRecord::Pending(PendingStepRecord {
        record_id,
        step: StepKind::UserInputReceived,
        started_at: now,
    });
    store
        .insert_step_record(iter.iteration_id, 0, &pending)
        .await
        .unwrap();

    let wrong_finished = FinishedStepRecord {
        record_id,
        step: StepKind::InputNormalization,
        started_at: now,
        finished_at: now,
        result: Ok(input_normalization_envelope()),
    };
    let err = store
        .finish_step_record(record_id, &wrong_finished)
        .await
        .unwrap_err();
    assert!(
        matches!(err, RunStateStoreError::StepKindMismatch { .. }),
        "expected StepKindMismatch, got: {err:?}"
    );
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn update_run_header_updates_only_status_updated_at_revision() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();

    let new_updated_at = Utc::now();
    store
        .update_run_header(run.run_id, RunStatus::WaitingForUser, new_updated_at, 1)
        .await
        .expect("update_run_header");

    let loaded = store.load_run(run.run_id).await.unwrap().unwrap();
    assert_eq!(loaded.status, RunStatus::WaitingForUser);
    assert_eq!(loaded.revision, 1);
    assert!(loaded.iterations.is_empty());
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn update_run_header_fails_with_missing_parent_run_when_run_not_found() {
    let store = connect_test_store().await;
    let err = store
        .update_run_header(new_run_id(), RunStatus::Active, Utc::now(), 0)
        .await
        .unwrap_err();
    assert!(matches!(err, RunStateStoreError::MissingParentRun(_)));
}

#[tokio::test]
async fn load_run_returns_none_when_run_does_not_exist() {
    let store = connect_test_store().await;
    let result = store.load_run(new_run_id()).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn load_run_reconstructs_full_run_state_hierarchy() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();

    let iter = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store.insert_iteration(run.run_id, 0, &iter).await.unwrap();

    let now = Utc::now();
    let record = StepRecord::Finished(FinishedStepRecord {
        record_id: new_record_id(),
        step: StepKind::UserInputReceived,
        started_at: now,
        finished_at: now,
        result: Ok(user_input_envelope()),
    });
    store
        .insert_step_record(iter.iteration_id, 0, &record)
        .await
        .unwrap();

    let loaded = store.load_run(run.run_id).await.unwrap().unwrap();
    assert_eq!(loaded.iterations.len(), 1);
    assert_eq!(loaded.iterations[0].step_records.len(), 1);
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn load_run_preserves_iteration_order_by_sequence_no() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();

    let id0 = new_iteration_id();
    let id1 = new_iteration_id();
    let id2 = new_iteration_id();
    for (seq, id) in [(2u64, id2), (0u64, id0), (1u64, id1)] {
        store
            .insert_iteration(
                run.run_id,
                seq,
                &RunIteration {
                    iteration_id: id,
                    step_records: vec![],
                },
            )
            .await
            .unwrap();
    }

    let loaded = store.load_run(run.run_id).await.unwrap().unwrap();
    assert_eq!(loaded.iterations[0].iteration_id, id0);
    assert_eq!(loaded.iterations[1].iteration_id, id1);
    assert_eq!(loaded.iterations[2].iteration_id, id2);
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn load_run_preserves_step_record_order_by_sequence_no() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();
    let iter = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store.insert_iteration(run.run_id, 0, &iter).await.unwrap();

    let now = Utc::now();
    let r0 = new_record_id();
    let r1 = new_record_id();
    for (seq, record_id, step, envelope) in [
        (1u64, r1, StepKind::InputNormalization, input_normalization_envelope()),
        (0u64, r0, StepKind::UserInputReceived, user_input_envelope()),
    ] {
        store
            .insert_step_record(
                iter.iteration_id,
                seq,
                &StepRecord::Finished(FinishedStepRecord {
                    record_id,
                    step,
                    started_at: now,
                    finished_at: now,
                    result: Ok(envelope),
                }),
            )
            .await
            .unwrap();
    }

    let loaded = store.load_run(run.run_id).await.unwrap().unwrap();
    let records = &loaded.iterations[0].step_records;
    if let StepRecord::Finished(f) = &records[0] {
        assert_eq!(f.record_id, r0);
    }
    if let StepRecord::Finished(f) = &records[1] {
        assert_eq!(f.record_id, r1);
    }
    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn list_run_ids_returns_ids_ordered_by_created_at_desc() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let now = Utc::now();

    let run_a = RunState {
        run_id: new_run_id(),
        status: RunStatus::Active,
        created_at: now - chrono::Duration::seconds(10),
        updated_at: now - chrono::Duration::seconds(10),
        revision: 0,
        iterations: vec![],
    };
    let run_b = RunState {
        run_id: new_run_id(),
        status: RunStatus::Active,
        created_at: now,
        updated_at: now,
        revision: 0,
        iterations: vec![],
    };
    store.insert_run(&run_a).await.unwrap();
    store.insert_run(&run_b).await.unwrap();

    let ids = store.list_run_ids().await.unwrap();
    let pos_b = ids.iter().position(|id| *id == run_b.run_id).unwrap();
    let pos_a = ids.iter().position(|id| *id == run_a.run_id).unwrap();
    assert!(pos_b < pos_a, "newer run must appear first in DESC order");

    cleanup_run(&database_url, run_a.run_id).await;
    cleanup_run(&database_url, run_b.run_id).await;
}

#[tokio::test]
async fn list_run_summaries_derives_fields_from_first_iteration_only() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();

    let first_iteration = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store
        .insert_iteration(run.run_id, 0, &first_iteration)
        .await
        .unwrap();

    let now = Utc::now();
    store
        .insert_step_record(
            first_iteration.iteration_id,
            0,
            &StepRecord::Finished(FinishedStepRecord {
                record_id: new_record_id(),
                step: StepKind::UserInputReceived,
                started_at: now,
                finished_at: now,
                result: Ok(user_input_envelope()),
            }),
        )
        .await
        .unwrap();
    store
        .insert_step_record(
            first_iteration.iteration_id,
            1,
            &StepRecord::Finished(FinishedStepRecord {
                record_id: new_record_id(),
                step: StepKind::ResponseValidationAndNormalization,
                started_at: now,
                finished_at: now,
                result: Ok(final_response_envelope("first-iteration understanding")),
            }),
        )
        .await
        .unwrap();

    let second_iteration = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store
        .insert_iteration(run.run_id, 1, &second_iteration)
        .await
        .unwrap();
    store
        .insert_step_record(
            second_iteration.iteration_id,
            0,
            &StepRecord::Finished(FinishedStepRecord {
                record_id: new_record_id(),
                step: StepKind::UserInputReceived,
                started_at: now,
                finished_at: now,
                result: Ok(StepResultEnvelope::UserInputReceived(UserRequest {
                    query: "newer query must be ignored".to_string(),
                })),
            }),
        )
        .await
        .unwrap();
    store
        .insert_step_record(
            second_iteration.iteration_id,
            1,
            &StepRecord::Finished(FinishedStepRecord {
                record_id: new_record_id(),
                step: StepKind::ResponseValidationAndNormalization,
                started_at: now,
                finished_at: now,
                result: Ok(final_response_envelope("second iteration must be ignored")),
            }),
        )
        .await
        .unwrap();

    let summaries = store.list_run_summaries().await.unwrap();
    let summary = summaries
        .into_iter()
        .find(|summary| summary.run_id == run.run_id)
        .expect("summary must exist");
    assert_eq!(summary.initial_user_query.as_deref(), Some("service down"));
    assert_eq!(
        summary.final_problem_understanding.as_deref(),
        Some("first-iteration understanding")
    );

    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn list_run_summaries_returns_none_when_first_iteration_has_no_successful_final_response() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();

    let iteration = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store.insert_iteration(run.run_id, 0, &iteration).await.unwrap();

    let now = Utc::now();
    store
        .insert_step_record(
            iteration.iteration_id,
            0,
            &StepRecord::Finished(FinishedStepRecord {
                record_id: new_record_id(),
                step: StepKind::UserInputReceived,
                started_at: now,
                finished_at: now,
                result: Ok(user_input_envelope()),
            }),
        )
        .await
        .unwrap();

    let summaries = store.list_run_summaries().await.unwrap();
    let summary = summaries
        .into_iter()
        .find(|summary| summary.run_id == run.run_id)
        .expect("summary must exist");
    assert_eq!(summary.initial_user_query.as_deref(), Some("service down"));
    assert_eq!(summary.final_problem_understanding, None);

    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn with_transaction_commits_child_writes_when_callback_returns_ok() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();

    let iteration = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    store
        .with_transaction(|tx| {
            Box::pin(async move {
                tx.insert_iteration(run.run_id, 0, &iteration).await?;
                tx.update_run_header(run.run_id, RunStatus::WaitingForUser, Utc::now(), 1)
                    .await?;
                Ok(())
            })
        })
        .await
        .expect("transaction must commit");

    let loaded = store.load_run(run.run_id).await.unwrap().unwrap();
    assert_eq!(loaded.status, RunStatus::WaitingForUser);
    assert_eq!(loaded.revision, 1);
    assert_eq!(loaded.iterations.len(), 1);

    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn with_transaction_rolls_back_writes_when_callback_returns_err() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();

    let iteration = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    let err = store
        .with_transaction(|tx| {
            Box::pin(async move {
                tx.insert_iteration(run.run_id, 0, &iteration).await?;
                Result::<(), RunStateStoreError>::Err(RunStateStoreError::InvalidRunState(
                    "force transaction rollback",
                ))
            })
        })
        .await
        .unwrap_err();
    assert!(matches!(err, RunStateStoreError::InvalidRunState(_)));

    let loaded = store.load_run(run.run_id).await.unwrap().unwrap();
    assert!(loaded.iterations.is_empty(), "iteration write must roll back");

    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn with_transaction_rolls_back_earlier_writes_when_later_tx_write_fails() {
    let database_url = test_database_url();
    let store = connect_test_store().await;
    let run = valid_run();
    store.insert_run(&run).await.unwrap();

    let iteration = RunIteration {
        iteration_id: new_iteration_id(),
        step_records: vec![],
    };
    let err = store
        .with_transaction(|tx| {
            Box::pin(async move {
                tx.insert_iteration(run.run_id, 0, &iteration).await?;
                tx.update_run_header(new_run_id(), RunStatus::WaitingForUser, Utc::now(), 1)
                    .await?;
                Ok(())
            })
        })
        .await
        .unwrap_err();
    assert!(matches!(err, RunStateStoreError::MissingParentRun(_)));

    let loaded = store.load_run(run.run_id).await.unwrap().unwrap();
    assert!(loaded.iterations.is_empty(), "earlier child write must roll back");
    assert_eq!(loaded.status, RunStatus::Active);
    assert_eq!(loaded.revision, 0);

    cleanup_run(&database_url, run.run_id).await;
}

#[tokio::test]
async fn integration_helper_requires_dedicated_test_database() {
    let url = test_database_url();
    let parsed = Url::parse(&url).unwrap();
    assert_eq!(parsed.path().trim_start_matches('/'), "distributed_diagnostics_test");
}
