create schema if not exists diagnostics;

create table if not exists diagnostics.eval_processing_state (
    eval_run_id uuid not null,
    runtime_run_id uuid not null references diagnostics.runs(run_id) on delete cascade,
    iteration_id uuid not null references diagnostics.run_iterations(iteration_id) on delete cascade,
    subject_received_at timestamptz not null,
    current_stage text not null,
    status text not null,
    attempt_count integer not null default 0,
    started_at timestamptz,
    completed_at timestamptz,
    updated_at timestamptz not null default now(),
    last_error text,
    constraint eval_processing_state_subject_pk primary key (eval_run_id, runtime_run_id, iteration_id),
    constraint eval_processing_state_stage_not_blank check (length(btrim(current_stage)) > 0),
    constraint eval_processing_state_status_allowed check (
        status in ('pending', 'running', 'completed', 'failed')
    ),
    constraint eval_processing_state_stage_allowed check (
        current_stage in ('judge_request_suites', 'build_eval_summary')
    ),
    constraint eval_processing_state_attempt_count_non_negative check (attempt_count >= 0),
    constraint eval_processing_state_completed_not_before_received check (
        completed_at is null or completed_at >= subject_received_at
    ),
    constraint eval_processing_state_started_not_before_received check (
        started_at is null or started_at >= subject_received_at
    ),
    constraint eval_processing_state_updated_not_before_received check (
        updated_at >= subject_received_at
    )
);

create index if not exists eval_processing_state_stage_queue_idx
    on diagnostics.eval_processing_state (
        eval_run_id,
        current_stage,
        status,
        subject_received_at,
        runtime_run_id,
        iteration_id
    );

create index if not exists eval_processing_state_run_completion_idx
    on diagnostics.eval_processing_state (eval_run_id, current_stage, status);

create index if not exists eval_processing_state_runtime_run_idx
    on diagnostics.eval_processing_state (runtime_run_id, iteration_id);

comment on table diagnostics.eval_processing_state is
    'Eval-owned resumable scheduling ledger for one eval subject per runtime run and iteration.';

