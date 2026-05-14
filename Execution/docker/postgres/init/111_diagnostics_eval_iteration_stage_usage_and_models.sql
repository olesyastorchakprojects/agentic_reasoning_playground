ALTER TABLE diagnostics.eval_iteration_summaries
    ADD COLUMN IF NOT EXISTS runtime_query_structuring_model text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS runtime_observation_boundary_resolver_model text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS runtime_observation_extraction_model text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS runtime_llm_structured_generation_model text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS runtime_query_structuring_tokens bigint NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_query_structuring_cost_usd numeric(20,10) NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_observation_boundary_resolver_tokens bigint NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_observation_boundary_resolver_cost_usd numeric(20,10) NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_observation_extraction_tokens bigint NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_observation_extraction_cost_usd numeric(20,10) NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_llm_structured_generation_tokens bigint NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS runtime_llm_structured_generation_cost_usd numeric(20,10) NOT NULL DEFAULT 0;

COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_query_structuring_model IS
    'Resolved raw model id used by query_structuring for this evaluated runtime iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_boundary_resolver_model IS
    'Resolved raw model id used by observation_boundary_resolver for this evaluated runtime iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_extraction_model IS
    'Resolved raw model id used by observation_extraction for this evaluated runtime iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_llm_structured_generation_model IS
    'Resolved raw model id used by llm_structured_generation for this evaluated runtime iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_query_structuring_tokens IS
    'Total tokens attributed to query_structuring for this evaluated runtime iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_query_structuring_cost_usd IS
    'Total USD cost attributed to query_structuring for this evaluated runtime iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_boundary_resolver_tokens IS
    'Total tokens attributed to observation_boundary_resolver for this evaluated runtime iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_boundary_resolver_cost_usd IS
    'Total USD cost attributed to observation_boundary_resolver for this evaluated runtime iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_extraction_tokens IS
    'Total tokens attributed to observation_extraction for this evaluated runtime iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_observation_extraction_cost_usd IS
    'Total USD cost attributed to observation_extraction for this evaluated runtime iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_llm_structured_generation_tokens IS
    'Total tokens attributed to llm_structured_generation for this evaluated runtime iteration.';
COMMENT ON COLUMN diagnostics.eval_iteration_summaries.runtime_llm_structured_generation_cost_usd IS
    'Total USD cost attributed to llm_structured_generation for this evaluated runtime iteration.';
