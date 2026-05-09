-- Continuation iteration metrics for eval_iteration_summaries
ALTER TABLE diagnostics.eval_iteration_summaries
    ADD COLUMN IF NOT EXISTS iteration_kind text,
    ADD COLUMN IF NOT EXISTS continuation_hypothesis_update_discipline_score smallint,
    ADD COLUMN IF NOT EXISTS continuation_problem_understanding_update_score smallint,
    ADD COLUMN IF NOT EXISTS continuation_next_check_progression_score smallint,
    ADD COLUMN IF NOT EXISTS continuation_observation_resolution_context_recovery_score smallint,
    ADD COLUMN IF NOT EXISTS usable_continuation_response boolean,
    ADD COLUMN IF NOT EXISTS continuation_update_no_hard_fail boolean,
    ADD COLUMN IF NOT EXISTS continuation_input_no_hard_fail boolean;

ALTER TABLE diagnostics.eval_iteration_summaries
    ADD CONSTRAINT eval_iteration_summaries_iteration_kind_allowed
        CHECK (iteration_kind IS NULL OR iteration_kind IN ('initial', 'continuation')),
    ADD CONSTRAINT eval_iteration_summaries_cu1_score_allowed
        CHECK (continuation_hypothesis_update_discipline_score IS NULL
            OR continuation_hypothesis_update_discipline_score IN (0, 1, 2)),
    ADD CONSTRAINT eval_iteration_summaries_cu2_score_allowed
        CHECK (continuation_problem_understanding_update_score IS NULL
            OR continuation_problem_understanding_update_score IN (0, 1, 2)),
    ADD CONSTRAINT eval_iteration_summaries_cu3_score_allowed
        CHECK (continuation_next_check_progression_score IS NULL
            OR continuation_next_check_progression_score IN (0, 1, 2)),
    ADD CONSTRAINT eval_iteration_summaries_cu4_score_allowed
        CHECK (continuation_observation_resolution_context_recovery_score IS NULL
            OR continuation_observation_resolution_context_recovery_score IN (0, 1, 2));

-- Continuation aggregate metrics for eval_run_summaries
ALTER TABLE diagnostics.eval_run_summaries
    ADD COLUMN IF NOT EXISTS usable_continuation_response_rate numeric(10,6),
    ADD COLUMN IF NOT EXISTS continuation_update_judge_score numeric(10,4),
    ADD COLUMN IF NOT EXISTS continuation_input_judge_score numeric(10,4),
    ADD COLUMN IF NOT EXISTS continuation_update_no_hard_fail_rate numeric(10,6),
    ADD COLUMN IF NOT EXISTS continuation_update_strict_pass_rate numeric(10,6),
    ADD COLUMN IF NOT EXISTS continuation_input_no_hard_fail_rate numeric(10,6),
    ADD COLUMN IF NOT EXISTS continuation_input_strict_pass_rate numeric(10,6),
    ADD COLUMN IF NOT EXISTS continuation_hypothesis_update_discipline_score_avg numeric(10,4),
    ADD COLUMN IF NOT EXISTS continuation_problem_understanding_update_score_avg numeric(10,4),
    ADD COLUMN IF NOT EXISTS continuation_next_check_progression_score_avg numeric(10,4),
    ADD COLUMN IF NOT EXISTS continuation_observation_resolution_context_recovery_score_avg numeric(10,4);

comment on column diagnostics.eval_iteration_summaries.iteration_kind is
    'initial or continuation; null for rows written before this migration';
comment on column diagnostics.eval_run_summaries.usable_continuation_response_rate is
    'null when no continuation iterations were evaluated';
