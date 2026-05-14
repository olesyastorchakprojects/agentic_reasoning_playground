ALTER TABLE diagnostics.eval_iteration_summaries
    ADD COLUMN IF NOT EXISTS runtime_query_structuring_prompt_version text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS runtime_observation_boundary_resolver_prompt_version text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS runtime_observation_extraction_prompt_version text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS runtime_prompt_context_prompt_version text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS runtime_diagnostic_update_prompt_context_prompt_version text NOT NULL DEFAULT 'unknown';

ALTER TABLE diagnostics.eval_run_summaries
    ADD COLUMN IF NOT EXISTS runtime_query_structuring_prompt_version text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS runtime_observation_boundary_resolver_prompt_version text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS runtime_observation_extraction_prompt_version text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS runtime_prompt_context_prompt_version text NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS runtime_diagnostic_update_prompt_context_prompt_version text NOT NULL DEFAULT 'unknown';
