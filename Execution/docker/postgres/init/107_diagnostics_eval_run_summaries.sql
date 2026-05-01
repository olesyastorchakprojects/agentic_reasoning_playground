create schema if not exists diagnostics;

create table if not exists diagnostics.eval_run_summaries (
    eval_run_id uuid primary key,
    run_type text not null,
    status text not null,
    started_at timestamptz not null,
    completed_at timestamptz,
    runtime_run_count bigint not null,
    iterations_evaluated_count bigint not null,
    judge_provider text not null,
    judge_model text not null,
    suite_versions jsonb not null,
    usable_first_response_rate numeric(10,6) not null,
    query_structuring_judge_score numeric(10,4) not null,
    evidence_pack_judge_score numeric(10,4) not null,
    final_answer_judge_score numeric(10,4) not null,
    query_structuring_strict_pass_rate numeric(10,6) not null,
    evidence_pack_strict_pass_rate numeric(10,6) not null,
    final_answer_strict_pass_rate numeric(10,6) not null,
    diagnostic_move_hard_fail_rate numeric(10,6) not null,
    gate_pass_rate numeric(10,6) not null,
    bad_final_due_to_query_rate numeric(10,6) not null,
    bad_final_due_to_evidence_rate numeric(10,6) not null,
    bad_final_with_good_query_and_evidence_rate numeric(10,6) not null,
    runtime_prompt_tokens bigint not null,
    runtime_completion_tokens bigint not null,
    runtime_total_tokens bigint not null,
    runtime_total_cost_usd numeric(20,10) not null,
    judge_prompt_tokens bigint not null,
    judge_completion_tokens bigint not null,
    judge_total_tokens bigint not null,
    judge_total_cost_usd numeric(20,10) not null,
    run_total_tokens bigint not null,
    run_total_cost_usd numeric(20,10) not null,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint eval_run_summaries_run_type_not_blank check (length(btrim(run_type)) > 0),
    constraint eval_run_summaries_status_allowed check (
        status in ('running', 'completed', 'failed')
    ),
    constraint eval_run_summaries_judge_provider_not_blank check (
        length(btrim(judge_provider)) > 0
    ),
    constraint eval_run_summaries_judge_model_not_blank check (
        length(btrim(judge_model)) > 0
    ),
    constraint eval_run_summaries_suite_versions_is_object check (
        jsonb_typeof(suite_versions) = 'object'
    ),
    constraint eval_run_summaries_runtime_run_count_non_negative check (
        runtime_run_count >= 0
    ),
    constraint eval_run_summaries_iterations_count_non_negative check (
        iterations_evaluated_count >= 0
    ),
    constraint eval_run_summaries_runtime_prompt_tokens_non_negative check (
        runtime_prompt_tokens >= 0
    ),
    constraint eval_run_summaries_runtime_completion_tokens_non_negative check (
        runtime_completion_tokens >= 0
    ),
    constraint eval_run_summaries_runtime_total_tokens_non_negative check (
        runtime_total_tokens >= 0
    ),
    constraint eval_run_summaries_judge_prompt_tokens_non_negative check (
        judge_prompt_tokens >= 0
    ),
    constraint eval_run_summaries_judge_completion_tokens_non_negative check (
        judge_completion_tokens >= 0
    ),
    constraint eval_run_summaries_judge_total_tokens_non_negative check (
        judge_total_tokens >= 0
    ),
    constraint eval_run_summaries_run_total_tokens_non_negative check (
        run_total_tokens >= 0
    ),
    constraint eval_run_summaries_runtime_cost_non_negative check (
        runtime_total_cost_usd >= 0
    ),
    constraint eval_run_summaries_judge_cost_non_negative check (
        judge_total_cost_usd >= 0
    ),
    constraint eval_run_summaries_run_cost_non_negative check (
        run_total_cost_usd >= 0
    ),
    constraint eval_run_summaries_runtime_total_tokens_matches_parts check (
        runtime_total_tokens = runtime_prompt_tokens + runtime_completion_tokens
    ),
    constraint eval_run_summaries_judge_total_tokens_matches_parts check (
        judge_total_tokens = judge_prompt_tokens + judge_completion_tokens
    ),
    constraint eval_run_summaries_run_total_tokens_matches_parts check (
        run_total_tokens = runtime_total_tokens + judge_total_tokens
    ),
    constraint eval_run_summaries_completed_not_before_started check (
        completed_at is null or completed_at >= started_at
    ),
    constraint eval_run_summaries_updated_not_before_created check (
        updated_at >= created_at
    )
);

create index if not exists eval_run_summaries_started_at_idx
    on diagnostics.eval_run_summaries (started_at desc);

create index if not exists eval_run_summaries_status_idx
    on diagnostics.eval_run_summaries (status, started_at desc);

create index if not exists eval_run_summaries_quality_idx
    on diagnostics.eval_run_summaries (usable_first_response_rate, run_total_cost_usd);

comment on table diagnostics.eval_run_summaries is
    'Materialized eval-run aggregate rows for reports and dashboard queries.';
