ALTER TABLE diagnostics.eval_iteration_summaries
    ADD COLUMN IF NOT EXISTS runtime_query_structuring_prompt_tokens BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_query_structuring_completion_tokens BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_query_structuring_input_cost_per_million_tokens NUMERIC(20,10) NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_query_structuring_output_cost_per_million_tokens NUMERIC(20,10) NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_observation_boundary_resolver_prompt_tokens BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_observation_boundary_resolver_completion_tokens BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_observation_boundary_resolver_input_cost_per_million_tokens NUMERIC(20,10) NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_observation_boundary_resolver_output_cost_per_million_tokens NUMERIC(20,10) NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_observation_extraction_prompt_tokens BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_observation_extraction_completion_tokens BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_observation_extraction_input_cost_per_million_tokens NUMERIC(20,10) NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_observation_extraction_output_cost_per_million_tokens NUMERIC(20,10) NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_llm_structured_generation_prompt_tokens BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_llm_structured_generation_completion_tokens BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_llm_structured_generation_input_cost_per_million_tokens NUMERIC(20,10) NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_llm_structured_generation_output_cost_per_million_tokens NUMERIC(20,10) NOT NULL DEFAULT 0;

COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_query_structuring_prompt_tokens IS
    'Prompt tokens consumed by the query_structuring runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_query_structuring_completion_tokens IS
    'Completion tokens consumed by the query_structuring runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_query_structuring_input_cost_per_million_tokens IS
    'Configured input token price for the query_structuring runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_query_structuring_output_cost_per_million_tokens IS
    'Configured output token price for the query_structuring runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_boundary_resolver_prompt_tokens IS
    'Prompt tokens consumed by the observation_boundary_resolver runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_boundary_resolver_completion_tokens IS
    'Completion tokens consumed by the observation_boundary_resolver runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_boundary_resolver_input_cost_per_million_tokens IS
    'Configured input token price for the observation_boundary_resolver runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_boundary_resolver_output_cost_per_million_tokens IS
    'Configured output token price for the observation_boundary_resolver runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_extraction_prompt_tokens IS
    'Prompt tokens consumed by the observation_extraction runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_extraction_completion_tokens IS
    'Completion tokens consumed by the observation_extraction runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_extraction_input_cost_per_million_tokens IS
    'Configured input token price for the observation_extraction runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_extraction_output_cost_per_million_tokens IS
    'Configured output token price for the observation_extraction runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_llm_structured_generation_prompt_tokens IS
    'Prompt tokens consumed by the llm_structured_generation runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_llm_structured_generation_completion_tokens IS
    'Completion tokens consumed by the llm_structured_generation runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_llm_structured_generation_input_cost_per_million_tokens IS
    'Configured input token price for the llm_structured_generation runtime stage within this iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_llm_structured_generation_output_cost_per_million_tokens IS
    'Configured output token price for the llm_structured_generation runtime stage within this iteration.';
