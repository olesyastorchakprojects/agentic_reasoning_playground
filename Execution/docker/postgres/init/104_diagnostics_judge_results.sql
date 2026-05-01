create schema if not exists diagnostics;

create table if not exists diagnostics.judge_results (
    eval_run_id uuid not null,
    runtime_run_id uuid not null references diagnostics.runs(run_id) on delete cascade,
    iteration_id uuid not null references diagnostics.run_iterations(iteration_id) on delete cascade,
    suite_name text not null,
    suite_id text not null,
    suite_version text not null,
    category text not null,
    scope text not null,
    judge_model text not null,
    judge_prompt_version text not null,
    score smallint not null,
    normalized_result_json jsonb not null,
    explanation text not null,
    failure_code text,
    raw_response jsonb not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint judge_results_subject_suite_pk primary key (
        eval_run_id,
        runtime_run_id,
        iteration_id,
        suite_name
    ),
    constraint judge_results_suite_name_not_blank check (length(btrim(suite_name)) > 0),
    constraint judge_results_suite_id_not_blank check (length(btrim(suite_id)) > 0),
    constraint judge_results_suite_version_not_blank check (length(btrim(suite_version)) > 0),
    constraint judge_results_category_not_blank check (length(btrim(category)) > 0),
    constraint judge_results_scope_not_blank check (length(btrim(scope)) > 0),
    constraint judge_results_judge_model_not_blank check (length(btrim(judge_model)) > 0),
    constraint judge_results_judge_prompt_version_not_blank check (
        length(btrim(judge_prompt_version)) > 0
    ),
    constraint judge_results_score_allowed check (score in (0, 1, 2)),
    constraint judge_results_normalized_result_json_is_object check (
        jsonb_typeof(normalized_result_json) = 'object'
    ),
    constraint judge_results_raw_response_json_allowed check (
        jsonb_typeof(raw_response) in ('object', 'array', 'string', 'number', 'boolean')
    ),
    constraint judge_results_updated_not_before_created check (updated_at >= created_at)
);

create index if not exists judge_results_subject_idx
    on diagnostics.judge_results (eval_run_id, runtime_run_id, iteration_id);

create index if not exists judge_results_suite_idx
    on diagnostics.judge_results (eval_run_id, suite_name, score);

create index if not exists judge_results_category_idx
    on diagnostics.judge_results (eval_run_id, category);

create index if not exists judge_results_normalized_gin_idx
    on diagnostics.judge_results
    using gin (normalized_result_json);

comment on table diagnostics.judge_results is
    'Normalized semantic judge verdicts, one suite result per eval subject.';

