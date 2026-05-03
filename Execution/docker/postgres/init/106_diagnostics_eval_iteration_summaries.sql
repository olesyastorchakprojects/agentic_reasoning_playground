create schema if not exists diagnostics;

create table if not exists diagnostics.eval_iteration_summaries (
    eval_run_id uuid not null,
    runtime_run_id uuid not null references diagnostics.runs(run_id) on delete cascade,
    iteration_id uuid not null references diagnostics.run_iterations(iteration_id) on delete cascade,
    query_structuring_judge_score numeric(10,4) not null,
    evidence_pack_judge_score numeric(10,4) not null,
    final_answer_judge_score numeric(10,4) not null,
    query_structuring_no_hard_fail boolean not null,
    evidence_pack_no_hard_fail boolean not null,
    final_answer_no_hard_fail boolean not null,
    usable_first_response boolean not null,
    no_root_cause_gate_passed boolean not null,
    single_check_gate_passed boolean not null,
    source_alignment_gate_passed boolean not null,
    field_boundary_gate_passed boolean not null,
    evidence_pack_gate_passed boolean not null,
    query_structuring_field_boundary_correctness_score smallint not null,
    query_structuring_grounding_conservatism_score smallint not null,
    evidence_pack_role_fit_score smallint not null,
    evidence_pack_sufficiency_score smallint not null,
    final_no_root_cause_claim_score smallint not null,
    final_first_check_discriminates_score smallint not null,
    final_hypothesis_source_alignment_score smallint not null,
    final_alternative_context_handling_score smallint not null,
    final_result_interpretation_usefulness_score smallint not null,
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
    runtime_qs_metrics jsonb,
    runtime_candidate_cards_metrics jsonb,
    runtime_incident_primary_metrics jsonb,
    runtime_incident_alternatives_metrics jsonb,
    runtime_theory_evidence_metrics jsonb,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    constraint eval_iteration_summaries_subject_pk primary key (
        eval_run_id,
        runtime_run_id,
        iteration_id
    ),
    constraint eval_iteration_summaries_suite_scores_allowed check (
        query_structuring_field_boundary_correctness_score in (0, 1, 2)
        and query_structuring_grounding_conservatism_score in (0, 1, 2)
        and evidence_pack_role_fit_score in (0, 1, 2)
        and evidence_pack_sufficiency_score in (0, 1, 2)
        and final_no_root_cause_claim_score in (0, 1, 2)
        and final_first_check_discriminates_score in (0, 1, 2)
        and final_hypothesis_source_alignment_score in (0, 1, 2)
        and final_alternative_context_handling_score in (0, 1, 2)
        and final_result_interpretation_usefulness_score in (0, 1, 2)
    ),
    constraint eval_iteration_summaries_runtime_prompt_tokens_non_negative check (
        runtime_prompt_tokens >= 0
    ),
    constraint eval_iteration_summaries_runtime_completion_tokens_non_negative check (
        runtime_completion_tokens >= 0
    ),
    constraint eval_iteration_summaries_runtime_total_tokens_non_negative check (
        runtime_total_tokens >= 0
    ),
    constraint eval_iteration_summaries_judge_prompt_tokens_non_negative check (
        judge_prompt_tokens >= 0
    ),
    constraint eval_iteration_summaries_judge_completion_tokens_non_negative check (
        judge_completion_tokens >= 0
    ),
    constraint eval_iteration_summaries_judge_total_tokens_non_negative check (
        judge_total_tokens >= 0
    ),
    constraint eval_iteration_summaries_run_total_tokens_non_negative check (
        run_total_tokens >= 0
    ),
    constraint eval_iteration_summaries_runtime_cost_non_negative check (
        runtime_total_cost_usd >= 0
    ),
    constraint eval_iteration_summaries_judge_cost_non_negative check (
        judge_total_cost_usd >= 0
    ),
    constraint eval_iteration_summaries_run_cost_non_negative check (
        run_total_cost_usd >= 0
    ),
    constraint eval_iteration_summaries_runtime_total_tokens_matches_parts check (
        runtime_total_tokens = runtime_prompt_tokens + runtime_completion_tokens
    ),
    constraint eval_iteration_summaries_judge_total_tokens_matches_parts check (
        judge_total_tokens = judge_prompt_tokens + judge_completion_tokens
    ),
    constraint eval_iteration_summaries_run_total_tokens_matches_parts check (
        run_total_tokens = runtime_total_tokens + judge_total_tokens
    ),
    constraint eval_iteration_summaries_updated_not_before_created check (
        updated_at >= created_at
    )
);

create index if not exists eval_iteration_summaries_run_idx
    on diagnostics.eval_iteration_summaries (eval_run_id);

create index if not exists eval_iteration_summaries_final_score_idx
    on diagnostics.eval_iteration_summaries (eval_run_id, final_answer_judge_score);

create index if not exists eval_iteration_summaries_usable_idx
    on diagnostics.eval_iteration_summaries (eval_run_id, usable_first_response);

comment on table diagnostics.eval_iteration_summaries is
    'Materialized iteration-level eval summaries with denormalized suite scores and usage totals.';
