create schema if not exists diagnostics;

create table if not exists diagnostics.runs (
    run_id uuid primary key,
    status text not null,
    created_at timestamptz not null,
    updated_at timestamptz not null,
    revision bigint not null,
    constraint runs_status_not_blank check (length(btrim(status)) > 0),
    constraint runs_updated_not_before_created check (updated_at >= created_at),
    constraint runs_revision_non_negative check (revision >= 0)
);

create table if not exists diagnostics.run_iterations (
    iteration_id uuid primary key,
    run_id uuid not null references diagnostics.runs(run_id) on delete cascade,
    sequence_no bigint not null,
    config_snapshot jsonb,
    constraint run_iterations_sequence_non_negative check (sequence_no >= 0),
    constraint run_iterations_run_id_sequence_unique unique (run_id, sequence_no)
);

create table if not exists diagnostics.run_step_records (
    record_id uuid primary key,
    iteration_id uuid not null references diagnostics.run_iterations(iteration_id) on delete cascade,
    sequence_no bigint not null,
    step text not null,
    record_status text not null,
    started_at timestamptz not null,
    finished_at timestamptz,
    result_json jsonb,
    error_json jsonb,
    constraint run_step_records_sequence_non_negative check (sequence_no >= 0),
    constraint run_step_records_step_not_blank check (length(btrim(step)) > 0),
    constraint run_step_records_status_allowed check (record_status in ('pending', 'finished')),
    constraint run_step_records_finished_not_before_started check (
        finished_at is null or finished_at >= started_at
    ),
    constraint run_step_records_pending_shape check (
        record_status <> 'pending'
        or (
            finished_at is null
            and result_json is null
            and error_json is null
        )
    ),
    constraint run_step_records_finished_shape check (
        record_status <> 'finished'
        or (
            finished_at is not null
            and (
                (result_json is not null and error_json is null)
                or (result_json is null and error_json is not null)
            )
        )
    ),
    constraint run_step_records_result_json_is_object_or_array check (
        result_json is null or jsonb_typeof(result_json) in ('object', 'array', 'string', 'number', 'boolean')
    ),
    constraint run_step_records_error_json_is_object_or_array check (
        error_json is null or jsonb_typeof(error_json) in ('object', 'array', 'string', 'number', 'boolean')
    ),
    constraint run_step_records_iteration_sequence_unique unique (iteration_id, sequence_no)
);

create index if not exists runs_status_idx
    on diagnostics.runs (status);

create index if not exists run_iterations_run_id_idx
    on diagnostics.run_iterations (run_id, sequence_no);

create index if not exists run_step_records_iteration_id_idx
    on diagnostics.run_step_records (iteration_id, sequence_no);

create index if not exists run_step_records_step_idx
    on diagnostics.run_step_records (step);

create index if not exists run_step_records_result_json_gin_idx
    on diagnostics.run_step_records
    using gin (result_json);

create index if not exists run_step_records_error_json_gin_idx
    on diagnostics.run_step_records
    using gin (error_json);

comment on table diagnostics.runs is
    'Canonical run-state header rows.';

comment on table diagnostics.run_iterations is
    'Canonical ordered run iterations. sequence_no is the zero-based ordinal within one run.';

comment on table diagnostics.run_step_records is
    'Canonical ordered step records. sequence_no is the zero-based ordinal within one iteration. Step payloads are stored in result_json or error_json.';
