create schema if not exists diagnostics;

create table if not exists diagnostics.judge_llm_calls (
    call_id text primary key,
    eval_run_id uuid not null,
    runtime_run_id uuid not null references diagnostics.runs(run_id) on delete cascade,
    iteration_id uuid not null references diagnostics.run_iterations(iteration_id) on delete cascade,
    suite_name text not null,
    stage_name text not null,
    judge_provider text not null,
    judge_model text not null,
    judge_base_url text not null,
    judge_prompt_version text not null,
    token_count_source text not null,
    prompt_tokens bigint not null,
    completion_tokens bigint not null,
    total_tokens bigint not null,
    input_cost_per_million_tokens numeric(20,10) not null,
    output_cost_per_million_tokens numeric(20,10) not null,
    prompt_cost_usd numeric(20,10) not null,
    completion_cost_usd numeric(20,10) not null,
    total_cost_usd numeric(20,10) not null,
    raw_response jsonb not null,
    created_at timestamptz not null default now(),
    constraint judge_llm_calls_call_id_not_blank check (length(btrim(call_id)) > 0),
    constraint judge_llm_calls_suite_name_not_blank check (length(btrim(suite_name)) > 0),
    constraint judge_llm_calls_stage_name_not_blank check (length(btrim(stage_name)) > 0),
    constraint judge_llm_calls_judge_provider_not_blank check (length(btrim(judge_provider)) > 0),
    constraint judge_llm_calls_judge_model_not_blank check (length(btrim(judge_model)) > 0),
    constraint judge_llm_calls_judge_base_url_not_blank check (length(btrim(judge_base_url)) > 0),
    constraint judge_llm_calls_judge_prompt_version_not_blank check (
        length(btrim(judge_prompt_version)) > 0
    ),
    constraint judge_llm_calls_token_count_source_not_blank check (
        length(btrim(token_count_source)) > 0
    ),
    constraint judge_llm_calls_stage_name_allowed check (
        stage_name in ('judge_request_suites')
    ),
    constraint judge_llm_calls_prompt_tokens_non_negative check (prompt_tokens >= 0),
    constraint judge_llm_calls_completion_tokens_non_negative check (completion_tokens >= 0),
    constraint judge_llm_calls_total_tokens_non_negative check (total_tokens >= 0),
    constraint judge_llm_calls_total_tokens_matches_parts check (
        total_tokens = prompt_tokens + completion_tokens
    ),
    constraint judge_llm_calls_input_cost_per_million_non_negative check (
        input_cost_per_million_tokens >= 0
    ),
    constraint judge_llm_calls_output_cost_per_million_non_negative check (
        output_cost_per_million_tokens >= 0
    ),
    constraint judge_llm_calls_prompt_cost_non_negative check (prompt_cost_usd >= 0),
    constraint judge_llm_calls_completion_cost_non_negative check (completion_cost_usd >= 0),
    constraint judge_llm_calls_total_cost_non_negative check (total_cost_usd >= 0),
    constraint judge_llm_calls_raw_response_json_allowed check (
        jsonb_typeof(raw_response) in ('object', 'array', 'string', 'number', 'boolean')
    )
);

create index if not exists judge_llm_calls_subject_idx
    on diagnostics.judge_llm_calls (eval_run_id, runtime_run_id, iteration_id);

create index if not exists judge_llm_calls_suite_idx
    on diagnostics.judge_llm_calls (eval_run_id, suite_name, created_at);

create index if not exists judge_llm_calls_stage_idx
    on diagnostics.judge_llm_calls (eval_run_id, stage_name);

comment on table diagnostics.judge_llm_calls is
    'Append-oriented factual judge call ledger with token usage and USD cost accounting.';
