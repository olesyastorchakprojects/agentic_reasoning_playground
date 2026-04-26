## 1) Purpose / Scope

This document defines the crate-level generated unit-test contract for the runtime skeleton.

This document is the single source of truth for:
- required crate-level unit-test artifacts and ownership boundaries;
- crate-level completion rules for generated unit tests;
- how crate-level unit-test generation composes with child runtime test specifications.

This document must be read together with:
- `Specification/runtime/unit_tests_common.md`

Shared runtime-wide unit-test generation rules are owned by
`Specification/runtime/unit_tests_common.md`.
This document defines only the additional crate-level required unit-test cases
and crate-level ownership rules for the current runtime skeleton.

This document does not redefine:
- shared unit-test generation rules, placement rules, helper rules, or environment rules already owned by `Specification/runtime/unit_tests_common.md`;
- child API-client unit-test cases already owned by dedicated child specifications;
- future crate-level integration, smoke, or end-to-end test requirements;
- test requirements for runtime areas that are not yet part of the current skeleton.

## 2) Covered Crate-Level Modules

The current crate-level runtime unit-test scope covers:

- `errors`
- `config`
- `observability`
- `api_clients`
- `request_pipeline`
- `orchestrator`
- `utils`
- `main`

This scope is crate-level and compositional.
Detailed required test cases for child API-client subtrees remain defined in their dedicated unit-test specifications.

## 3) Crate-Level Unit-Test Ownership

Crate-level unit-test ownership rules:

- this document owns only crate-level and cross-cutting unit-test requirements;
- detailed required unit tests for child runtime slices must remain owned by the corresponding child specifications;
- crate-level generation must not duplicate child-owned test-case lists;
- when a child runtime slice has its own dedicated unit-test specification, that child specification is the source of truth for the required unit tests of that slice.

The current child-owned unit-test specifications include:

- `Specification/runtime/api_clients/model/unit_tests.md`
- `Specification/runtime/api_clients/qdrant/unit_tests.md`
- `Specification/runtime/api_clients/postgres/unit_tests.md`

Additional child-owned unit-test specifications may be added later without changing the meaning of this document.

## 4) Required Crate-Level Unit Tests

### 4.1) `errors`

Generated unit tests for the crate-level error boundary must include all of the following cases:

- conversion from `ConfigError` into `RuntimeError` produces the exact `RuntimeError::Config(...)` variant;
- conversion from `ApiClientError` into `RuntimeError` produces the exact `RuntimeError::ApiClients(...)` variant;
- the crate-level error boundary preserves typed child errors rather than flattening them into strings.

### 4.2) `api_clients`

Generated unit tests for the crate-level `api_clients` parent boundary must include all of the following cases:

- conversion from `EmbeddingClientError` into `ApiClientError` produces the exact `ApiClientError::Embedding(...)` variant;
- conversion from `ModelApiClientError` into `ApiClientError` produces the exact `ApiClientError::Model(...)` variant;
- conversion from `QdrantApiClientError` into `ApiClientError` produces the exact `ApiClientError::Qdrant(...)` variant;
- conversion from `PostgresApiClientError` into `ApiClientError` produces the exact `ApiClientError::Postgres(...)` variant.

### 4.3) `config`

Generated unit tests for the crate-level `config` boundary must include all of the following cases:

- runtime TOML, ingest TOML, and environment values merge into one executable typed `Settings` object;
- missing required environment variables fail with the exact config-owned error category;
- `OLLAMA_URL` maps into the exact `Settings.embedding_model.url` value;
- `TRACING_ENDPOINT` maps into the exact `Settings.observability.tracing_endpoint` value;
- `METRICS_ENDPOINT` maps into the exact `Settings.observability.metrics_endpoint` value;
- `model.transport_kind = "ollama"` resolves into the exact `ModelTransportSettings::Ollama(...)` variant;
- `model.transport_kind = "together"` resolves into the exact `ModelTransportSettings::Together(...)` variant;
- `qdrant.collections.<collection>.kind = "dense"` resolves into the exact `CollectionSettings::Dense(...)` variant;
- `qdrant.collections.<collection>.kind = "hybrid"` resolves into the exact `CollectionSettings::Hybrid(...)` variant;
- `hybrid.sparse.strategy.kind = "bag_of_words"` resolves into the exact `SparseStrategySettings::BagOfWords(...)` variant;
- `hybrid.sparse.strategy.kind = "bm25_like"` resolves into the exact `SparseStrategySettings::Bm25Like(...)` variant;
- sparse artifact paths from ingest config are preserved in the resolved sparse strategy settings variant;
- unsupported transport kinds fail before runtime request processing begins;
- unsupported collection kinds fail before runtime request processing begins;
- unsupported sparse strategy kinds fail before runtime request processing begins;
- resolved collection retrieval settings preserve `top_k`, `score_threshold`, `max_alternatives`, retry settings, and the typed collection variant in one `CollectionRetrievalSettings` value;
- resolved settings place `corpus_version` on the collection settings boundary rather than as one shared top-level retrieval field;
- resolved settings preserve `query_structuring.controlled_vocabulary_path`, `query_structuring.prompt_asset_path`, and `query_structuring.max_output_tokens`;
- resolved settings preserve `prompt_context.prompt_asset_path`;
- resolved prompt-context settings preserve chunk role sources, limits, per-case limits, fallback flags, and typed tag priorities;
- resolved default runtime config enables `supporting_explanation.limit = 1`;
- prompt-context config accepts full canonical chunk tags such as `chunk_role:symptom`;
- prompt-context config rejects short chunk tag aliases such as `symptom`;
- prompt-context config rejects unknown chunk tags;
- prompt-context config rejects duplicate tags inside one role priority list;
- prompt-context config rejects invalid source values;

### 4.4) `utils`

The current crate-level `utils` test scope is limited.

Generated unit tests for crate-level `utils` modules must include all of the following cases:

- retry helpers reject invalid retry-policy inputs when the runtime contract requires constructor validation;
- tokenizer helper test coverage must follow `Specification/runtime/utils/tokenizer.md`;
- sparse-text-space filtering and normalization beyond the tokenizer utility must be tested by the Qdrant runtime specs that depend on that contract.

These crate-level `utils` tests remain helper-focused.
Qdrant-specific sparse query preparation tests remain owned by the Qdrant unit-test specification.

### 4.5) `input_normalization`

Generated unit tests for the `input_normalization` runtime module must include all of the following cases:

- constructor succeeds when the configured tokenizer can be loaded successfully;
- leading and trailing whitespace are trimmed from the query;
- newlines are flattened during normalization;
- tabs and mixed whitespace runs are canonicalized into single ASCII spaces;
- a query that becomes empty after normalization fails with `InputNormalizationError::EmptyQuery`;
- a non-empty query whose normalized form produces zero tokens fails with `InputNormalizationError::EmptyQuery`;
- a query whose token count is exactly equal to `max_input_tokens` succeeds;
- a query whose token count is greater than `max_input_tokens` fails with `InputNormalizationError::InputTooLong`;
- successful normalization returns the expected canonical query string;
- successful normalization returns the correct `input_token_count` for the canonical query.

Tokenizer utility behavior such as download, cache reuse, and tokenizer-load failure handling that is already owned by `Specification/runtime/utils/tokenizer.md` must not be duplicated here.

### 4.6) `observability`

Generated unit tests for the crate-level `observability` boundary must include all of the following cases:

- initialization succeeds in disabled mode when both `tracing_enabled = false` and `metrics_enabled = false`;
- initialization fails as a startup error when tracing is enabled and `tracing_endpoint` is invalid;
- initialization fails as a startup error when metrics are enabled and `metrics_endpoint` is invalid;
- initialization uses `trace_batch_scheduled_delay_ms` when constructing the tracing pipeline;
- initialization uses `metrics_export_interval_ms` when constructing the metrics pipeline.

### 4.7) `query_structuring`

Generated unit tests for the `query_structuring` runtime module must include all of the following cases:

- constructor fails when either asset path is empty;
- constructor fails when `max_output_tokens = 0`;
- constructor fails when the vocabulary asset file cannot be read from disk;
- constructor fails when the prompt asset file cannot be read from disk;
- constructor fails when the vocabulary asset JSON is invalid;
- constructor fails when the prompt asset JSON is invalid;
- constructor fails when the user template is missing either required placeholder;
- constructor succeeds when both JSON asset files exist on disk and satisfy the asset contract;
- `structure(...)` builds exactly two messages in order: one `system`, one `user`;
- `structure(...)` serializes the loaded vocabulary into compact JSON inside the user message;
- `structure(...)` sends `JsonObject` response mode;
- `structure(...)` sends `temperature = 0.0`;
- `structure(...)` sends the configured `max_output_tokens`;
- successful model output preserves `prompt_tokens`, `completion_tokens`, and `total_tokens` in `QueryStructuringOutput.token_usage`;
- model output with malformed JSON fails with `InvalidModelOutput`;
- model output missing a required top-level field fails with `InvalidModelOutput`;
- model output with an unknown `support_level` fails with `InvalidModelOutput`;
- model output with an unknown `confidence` fails with `InvalidModelOutput`;
- model output with more than one `failure_mode` fails with `InvalidModelOutput`;
- model output with `finish_reason = stop` and otherwise valid content succeeds;
- model output with `finish_reason = length` and incomplete JSON fails with `InvalidModelOutput`;
- model output with any non-`stop` finish reason fails with `InvalidModelOutput`;
- `InvalidModelOutput` preserves available `prompt_tokens`, `completion_tokens`, and `total_tokens`;
- `InvalidModelOutput` preserves the parsed `finish_reason` when the provider returned one;
- successful model output maps into the exact shared `QueryStructuringOutput` shape.

### 4.8) `candidate_card_retrieval`

Generated unit tests for the `candidate_card_retrieval` runtime module must include all of the following cases:

- constructor rejection of `top_k = 0`;
- constructor rejection of `top_k < 1 + max_alternatives`;
- constructor rejection of negative `score_threshold`;
- constructor rejection of `score_threshold = f32::NAN`;
- constructor rejection of `score_threshold = f32::INFINITY`;
- constructor rejection of `score_threshold = f32::NEG_INFINITY`;
- constructor rejection of `max_alternatives > 2`;
- empty-result success returning `primary = None` and empty `alternatives`;
- one-hit success returning `primary = Some(...)` and empty `alternatives`;
- multi-hit success returning the first hit as `primary` and the next hits as `alternatives` in original order;
- truncation of `alternatives` to `max_alternatives`;
- returned selection containing at most three cards total even when retrieval returns more than three hits;
- pass-through of collection errors through `CandidateCardRetrievalError::Collection`;
- request construction using the unchanged normalized query text;
- request construction using `limit = top_k`;
- request construction using the configured `score_threshold`.

### 4.9) `main`

Generated unit tests for the crate-level `main` CLI boundary must include all of the following cases:

- CLI argument parsing accepts `--config` and `--ingest-config` and preserves the supplied paths exactly;
- missing required `--config` fails as a startup-time CLI argument error;
- missing required `--ingest-config` fails as a startup-time CLI argument error;
- startup loads `.env` through library-owned config-loading code rather than requiring a dedicated CLI path argument for the env contract;
- startup fails before later runtime initialization when a required environment variable is absent after config loading;
- the CLI delegates config loading to library-owned code rather than parsing TOML content inside `main.rs`.

### 4.10) `incident_evidence_retrieval`

Generated unit tests for the `incident_evidence_retrieval` runtime module must include all of the following cases:

- empty-input success returning empty `primary_chunks` and empty `alternative_chunks` without calling the collection;
- primary-only input issues exactly one collection call using one `case_id`, the hardcoded primary tag set, `limit = top_k`, and `score_threshold = settings.score_threshold`;
- alternatives-only input issues exactly one collection call using alternative `case_id`s in original order, the hardcoded alternative tag set, `limit = top_k`, and `score_threshold = settings.score_threshold`;
- combined input with both primary and alternatives issues exactly two collection calls;
- successful mapping from `PracticeChunkSearchHit` into `IncidentEvidenceChunk`;
- preservation of primary-search hit order in `primary_chunks`;
- preservation of alternative-search hit order in `alternative_chunks`;
- pass-through of collection errors through `IncidentEvidenceRetrievalError::Collection`;
- whole-module failure when one branch succeeds and the other branch returns a collection error;
- no deduplication of returned chunks.

### 4.11) `theory_evidence_retrieval`

Generated unit tests for the `theory_evidence_retrieval` runtime module must include all of the following cases:

- constructor rejection of `top_k = 0`;
- constructor rejection of negative `score_threshold`;
- constructor rejection of `score_threshold = f32::NAN`;
- constructor rejection of `score_threshold = f32::INFINITY`;
- constructor rejection of `score_threshold = f32::NEG_INFINITY`;
- constructor success when `top_k > 0`;
- `retrieve(...)` issues exactly one collection call for a valid request;
- request construction uses the unchanged normalized query text;
- request construction uses `limit = settings.top_k`;
- request construction uses `score_threshold = settings.score_threshold`;
- successful empty-result retrieval returns `TheoryEvidenceRetrievalOutput { chunks: vec![] }`;
- successful mapping from `TheoryChunkSearchHit` into `TheoryEvidenceChunk`;
- preservation of collection hit order in `TheoryEvidenceRetrievalOutput.chunks`;
- preservation of raw collection-returned `score` values without rounding, normalization, bucketing, or rescaling;
- preservation of raw collection-returned `text` values;
- no post-collection truncation beyond the collection request limit;
- no deduplication of returned chunks;
- pass-through of collection errors through `TheoryEvidenceRetrievalError::Collection`;
- `retrieve(...)` does not require candidate-card, card-hydration, or incident-evidence inputs.

### 4.12) `prompt_context_assembly`

Generated unit tests for the `prompt_context_assembly` runtime module must include all of the following cases:

- constructor rejects empty `prompt_asset_path`;
- constructor rejects unreadable prompt asset file;
- constructor rejects unreadable derived prompt asset schema file;
- constructor rejects invalid prompt asset JSON;
- constructor rejects invalid prompt asset schema JSON;
- constructor rejects prompt asset JSON that does not satisfy the derived prompt asset schema;
- constructor rejects prompt asset missing `{{json_context}}`;
- constructor rejects prompt asset with more than one `{{json_context}}`;
- constructor rejects prompt asset with empty `policy_constraints`;
- constructor succeeds with a valid prompt asset loaded from `PromptContextSettings.prompt_asset_path`;
- constructor derives the prompt asset schema path from the prompt asset directory by replacing the prompt asset file name suffix with `.schema.json`;
- constructor rejection when `evidence_for_match.limit = 0`;
- constructor rejection when `first_check_hint.limit = 0`;
- constructor allows `supporting_explanation.limit = 0`;
- constructor succeeds when `supporting_explanation.limit = 1`;
- constructor allows `alternative_context.limit = 0`;
- constructor allows `mechanism_explanation.limit = 0`;
- constructor rejection when `evidence_for_match.source` is not `PrimaryIncident`;
- constructor rejection when `first_check_hint.source` is not `PrimaryIncident`;
- constructor rejection when `supporting_explanation.source` is not `PrimaryIncident`;
- constructor rejection when `alternative_context.source` is not `AlternativeIncident`;
- constructor rejection when `mechanism_explanation.source` is not `Theory`;
- constructor rejection when `alternative_context.limit > 0` and `per_case_limit = None`;
- constructor rejection when `alternative_context.limit > 0` and `per_case_limit = Some(0)`;
- constructor accepts `alternative_context.per_case_limit = None` when `alternative_context.limit = 0`;
- constructor accepts any `alternative_context.per_case_limit = Some(n)` value when `alternative_context.limit = 0`;
- constructor rejection when `mechanism_explanation.tag_priority` is non-empty;
- missing hydrated primary card fails with `PromptContextAssemblyError::MissingPrimaryCard`;
- output includes a compact prompt-facing primary-card summary as `matched_incident_card`;
- output excludes full alternative cards;
- output copies `NormalizedUserRequest.query` into `user_problem`;
- output builds `normalized_incident_query.recognized_canonical_symptoms` from `QueryStructuringOutput.structured_query.symptoms[*].term`;
- output builds `normalized_incident_query.affected_components` from `QueryStructuringOutput.structured_query.affected_subsystems[*].term`;
- output builds `normalized_incident_query.failure_mode_candidates` from `QueryStructuringOutput.structured_query.failure_modes[*].term`;
- output builds `normalized_incident_query.signals_present` from symptoms, triggers, and observability signals with stable de-duplication;
- output emits `normalized_incident_query.unmapped_user_symptoms`, `observed_phase`, and `missing_signals` as empty arrays in the current version;
- output does not include `evidence_span`, `support_level`, `rejected_nearby_terms`, or `token_usage` in the prompt context;
- output does not fail when a `normalized_incident_query` source array is empty;
- output includes the fixed task value `diagnostic_response`;
- output includes policy constraints loaded from the prompt asset;
- output returns a non-empty filled prompt string;
- rendered prompt contains the strict JSON output schema;
- rendered prompt contains a `JSON context follows:` marker;
- rendered prompt contains a valid serialized JSON context after the `JSON context follows:` marker;
- rendered prompt embeds the selected incident chunks with roles and canonical typed `IncidentChunkTag` values serialized as full tag strings;
- rendered prompt embeds selected supporting explanation chunks with role `supporting_explanation`;
- rendered prompt serializes all prompt evidence roles through the exact snake-case mapping: `evidence_for_match`, `first_check_hint`, `supporting_explanation`, `alternative_context`, and `mechanism_explanation`;
- prompt rendering uses module-private DTOs or helper mapping for `PromptEvidenceRole` serialization rather than deriving `serde::Serialize` on `PromptEvidenceRole`;
- rendered prompt does not embed `competing_precedent_context`;
- rendered prompt embeds selected theory chunks with roles;
- rendered prompt preserves uncertainty instructions when alternative context chunks are selected;
- output returns selected incident chunks separately from the prompt for history;
- output returns selected theory chunks separately from the prompt for history;
- chunks returned separately from the prompt exactly match the chunks embedded in the rendered prompt context;
- `evidence_for_match` selection uses configured tag priority before retrieval score;
- `first_check_hint` selection uses configured tag priority before retrieval score;
- `supporting_explanation` selection uses configured tag priority before retrieval score;
- when two chunks match the same configured tag priority, higher score wins;
- when tag priority and score tie, original retrieval order wins;
- chunks with multiple tags use the best matching configured tag for the current role;
- unknown collection-returned tags are ignored for role matching;
- fallback chunks are considered only when `fallback_to_any_chunk = true`;
- required role selection fails with `MissingRequiredEvidence` when no eligible chunk exists and fallback is disabled;
- selected chunks are not duplicated across required incident roles when a distinct eligible chunk is available;
- duplicate chunk reuse is allowed only as required-role fallback when no distinct chunk can fill the role;
- optional `supporting_explanation` does not reuse a duplicate chunk when no distinct eligible chunk exists;
- `alternative_context.limit = 0` selects no alternative chunks;
- alternative context selection uses deterministic round-robin across case groups;
- alternative context round-robin follows `CardHydrationOutput.alternatives[*].case_id` order when alternatives are present;
- alternative context selection respects `per_case_limit`;
- alternative context is optional when no alternative chunks exist;
- selected primary incident chunks whose `case_id` differs from the hydrated primary card fail with `PromptContextAssemblyError::InconsistentEvidence`;
- selected alternative incident chunks whose `case_id` has no hydrated alternative card fail with `PromptContextAssemblyError::InconsistentEvidence`;
- theory chunk selection is capped at one item when `mechanism_explanation.limit = 1`;
- constructor rejects `mechanism_explanation.limit > 1`;
- `mechanism_explanation.limit = 0` selects no theory chunks;
- empty theory evidence is not an error;
- selected incident chunks preserve raw `chunk_id`, `case_id`, `score`, and `text`;
- selected incident chunks return recognized `chunk_tags` as typed `IncidentChunkTag` values and omit unknown raw source tags;
- selected theory chunks preserve raw `chunk_id`, `score`, and `text`;
- rendered prompt uses compact model-facing incident chunk DTOs with only `role`, `source_document_id`, `chunk_tags`, and `text`;
- rendered prompt uses compact model-facing theory chunk DTOs with only `role`, `source_document_id`, and `text`;
- `matched_incident_card` omits `case_id`, `title`, and `source_name`;
- selected incident chunks are emitted in role order: `EvidenceForMatch`, `FirstCheckHint`, `SupportingExplanation`, then `AlternativeContext`;
- no unselected chunks are included in output.

### 4.13) `orchestrator`

Generated unit tests for the `orchestrator` runtime subtree must include all of
the following cases:

- the crate exposes the top-level `orchestrator` module;
- `orchestrator::run_state` exposes `model`, `view`, and `apply`;
- public model, view, and writer types are importable from their documented
  module paths;
- id newtypes are copyable, comparable, hashable, and serializable;
- unit enums serialize and deserialize successfully;
- unit enums support `strum` parsing and string display;
- `StepRecord::Pending` serializes and deserializes without losing
  `record_id`, `step`, or `started_at`;
- `StepRecord::Finished` with `Ok(StepResultEnvelope)` serializes and
  deserializes without losing the result payload;
- `StepRecord::Finished` with `Err(StepError)` serializes and deserializes
  without losing the error variant;
- `FinishedStepRecord.finished_at >= FinishedStepRecord.started_at` is enforced
  by generated validation helpers when such helpers are generated;
- `StepKind` and successful `StepResultEnvelope` compatibility is enforced by
  generated validation helpers;
- `StepKind` and step-specific `StepError` compatibility is enforced by
  generated validation helpers;
- text variants of `StepError` reject empty `message` values when generated
  constructors or validators are present;
- `RunStateView::new` wraps a borrowed `RunState`;
- `RunStateView::run_id()` returns the underlying run id;
- `RunStateView::status()` returns the underlying run status;
- `RunStateView::iteration_count()` equals `RunState.iterations.len()`;
- `RunStateView::iteration(iteration_id)` returns the underlying stored
  iteration borrow for an existing id and `None` for an unknown id;
- `RunStateView::iterations()` preserves `RunState.iterations` order;
- `RunStateView::last_iteration()` returns the last iteration;
- `IterationView::iteration_id()` returns the underlying iteration id;
- `IterationView::step_count()` equals `RunIteration.step_records.len()`;
- `IterationView::steps()` preserves `RunIteration.step_records` order;
- `IterationView::finished_steps()` returns only finished records and preserves
  their relative order;
- `IterationView::pending_step()` returns `Some` when the current iteration has
  a pending step and `None` otherwise;
- `IterationView::finished_step(kind)` returns the last finished step with the
  requested `StepKind`;
- `StepView` maps `StepRecord::Pending` to `StepView::Pending`;
- `StepView` maps `StepRecord::Finished` to `StepView::Finished`;
- `PendingStepView` returns the underlying `record_id`, `kind`, and
  `started_at`;
- `PendingStepView::to_owned()` clones the underlying pending record without
  changing its fields;
- `FinishedStepView` returns the underlying `record_id`, `kind`, `started_at`,
  `finished_at`, and borrowed `Result<StepResultEnvelope, StepError>`;
- `FinishedStepView::to_owned()` clones the underlying finished record without
  changing its fields;
- `StepView::to_owned()` preserves the pending-versus-finished variant and all
  underlying fields;
- `RunStateWriter::new` wraps a mutable `RunState`;
- `begin_iteration(user_input)` appends a new iteration;
- `begin_iteration(user_input)` returns `PendingStepAlreadyExists` when any
  pending step already exists in the run;
- the new iteration contains exactly one finished
  `StepKind::UserInputReceived` record;
- `begin_iteration(user_input)` returns a `CurrentIterationWriter` for the new
  iteration;
- successful mutating methods update `updated_at` and increment `revision`;
- `current_iteration()` returns `NoCurrentIteration` when no iteration exists;
- `current_iteration()` does not update `updated_at` or increment `revision`;
- `CurrentIterationWriter::begin_step(step)` appends one pending step to the
  current iteration;
- `begin_step(step)` returns `PendingStepAlreadyExists` when any pending step
  already exists in the run;
- `CurrentIterationWriter::pending_step()` returns the pending step in the
  current iteration when present;
- `PendingStepWriter::record_success(result)` replaces the pending record with
  a finished record containing `Ok(result)`;
- `record_success(result)` rejects a result variant that does not match the
  pending step kind;
- `PendingStepWriter::record_failure(error)` replaces the pending record with
  a finished record containing `Err(error)`;
- `record_failure(error)` rejects step-specific error variants that do not
  match the pending step kind;
- successful `record_success` sets `RunStatus::Active`;
- successful `record_failure` sets `RunStatus::Error`;
- `wait_for_user()` sets `RunStatus::WaitingForUser`;
- `wait_for_user()` returns `PendingStepAlreadyExists` when any pending step
  exists in the run;
- `archive_run()` sets `RunStatus::Archived`;
- `archive_run()` succeeds even when a pending step exists in the run;
- `archive_run()` is a pure no-op when the run is already archived and does not
  modify `updated_at` or increment `revision`;
- mutating methods except `archive_run()` return `RunArchived` when the run is
  archived.
- `StepExecutor::new(...)` stores all supplied leaf modules without extra
  validation behavior;
- `StepExecutor::execute(...)` returns `StepError::MissingRequiredInput` when no
  current iteration exists;
- `StepExecutor::execute(...)` returns `StepError::InvalidState` for
  `StepKind::UserInputReceived`;
- each executable `StepKind` dispatches to the correct leaf-module method;
- each executable `StepKind` returns the correct `StepResultEnvelope` variant on
  success;
- missing required finished inputs return `StepError::MissingRequiredInput`;
- prerequisite finished steps recorded as `Err(...)` return
  `StepError::MissingRequiredInput`;
- mismatched prerequisite success-envelope variants return
  `StepError::InvalidState`;
- multi-input steps `IncidentEvidenceRetrieval` and `PromptContextAssembly`
  require all declared finished inputs from the current iteration;
- `StepExecutor::execute(...)` reads only the current iteration and does not
  fall back to earlier iterations.
- `RunRepository::new(...)` stores the supplied `PostgresRunStateStore`
  dependency without extra validation behavior;
- `RunRepository::create_run(...)` rejects a non-empty initial `RunState`;
- `RunRepository::create_run(...)` maps duplicate-run store failure into
  `RunRepositoryError::DuplicateRun`;
- `RunRepository::load_run(...)` returns `Ok(None)` when the requested run does
  not exist;
- `RunRepository::append_iteration(...)` persists exactly one iteration and
  updates the run header;
- `RunRepository::append_step_record(...)` persists exactly one step record and
  updates the run header;
- `RunRepository::finish_step_record(...)` finishes one existing pending step
  record, updates the run header, and does not create a new step-record row;
- `RunRepository::update_run_header(...)` updates only the run header;
- `RunRepository::list_runs(...)` returns rows ordered by `created_at desc`;
- `RunRepository::list_runs(...)` derives `initial_user_query` from the
  successful `UserInputReceived` step of the first iteration;
- `RunRepository::list_runs(...)` derives `final_problem_understanding` from the
  successful `ResponseValidationAndNormalization` step of the first iteration
  when present;
- `RunRepository::list_runs(...)` returns
  `RunRepositoryError::MissingInitialUserQuery` when a stored run cannot provide
  the required first-iteration initial user query.
- `Orchestrator::new(...)` stores the supplied policy, executor, and repository
  dependencies without extra validation behavior;
- `run(user_input)` creates an empty run header before appending the first
  iteration;
- `run(user_input)` begins exactly one new first iteration from the supplied
  `UserRequest`;
- `run(user_input)` persists that first iteration before entering
  `drive_to_outcome(...)`;
- `resume(run_id)` loads the existing run and does not create a new iteration;
- `resume_with_input(run_id, user_input)` loads the existing run, appends
  exactly one new iteration, and preserves prior iterations unchanged;
- `load_existing_run(run_id)` maps repository `Ok(None)` into
  `OrchestratorError::RunNotFound { run_id }`;
- `drive_to_outcome(...)` asks policy for a new `PolicyTransition` on each loop
  iteration;
- when policy returns `ExecuteStep { step }`, orchestrator opens a pending step
  before calling `StepExecutor::execute(...)`;
- when policy returns `ExecuteStep { step }`, orchestrator persists the pending
  step record before calling `StepExecutor::execute(...)`;
- the pending step persisted by orchestrator uses the same `record_id` returned
  by `PendingStepWriter::record_id()`;
- the pending step persisted by orchestrator uses the current iteration last
  position as `step_sequence_no`;
- after successful step execution, orchestrator records success in memory and
  persists the matching finished step through `RunRepository::finish_step_record(...)`;
- after failed step execution, orchestrator records failure in memory and
  persists the matching finished step through `RunRepository::finish_step_record(...)`;
- the finished record persisted by orchestrator uses the same `record_id` as
  the pending record opened earlier in the loop;
- `drive_to_outcome(...)` returns `RunOutcome::Finished` when policy returns
  `FinishWithResult`;
- `drive_to_outcome(...)` returns `RunOutcome::Failed` when policy returns
  `FinishWithError`;
- `drive_to_outcome(...)` never calls `StepExecutor::execute(...)` for
  `StepKind::UserInputReceived`;
- `resume(run_id)` preserves already successful finished steps in the current
  iteration and lets policy continue from the persisted state;
- `resume_with_input(run_id, user_input)` makes the new last iteration the only
  iteration inspected by policy and executor for that invocation;
- orchestrator does not reinterpret `RunOutcome::Finished` as automatic run
  archival or final closure;
- orchestrator unit tests may use generated private or `#[cfg(test)]` fakes,
  spies, adapters, or harness constructors for `StepExecutor` and
  `RunRepository` as long as the documented production public API remains
  unchanged.

### 4.14) `api_clients.postgres.run_state_store`

Generated unit tests for `PostgresRunStateStore` must include all of the
following cases:

- `new(...)` fails when `PostgresRunStateStoreConfig.postgres_url` is empty
  after trimming;
- `insert_run(...)` validates the run header before any database write is
  attempted;
- `insert_run(...)` writes only the canonical run header fields and does not
  implicitly write iterations or step records;
- duplicate `run_id` insert fails with the exact
  `RunStateStoreError::DuplicateRun(...)` variant;
- `insert_iteration(...)` preserves the supplied `sequence_no`;
- `insert_iteration(...)` fails when the parent `run_id` does not exist;
- duplicate `(run_id, sequence_no)` iteration insert fails with the exact
  `RunStateStoreError::DuplicateIteration { ... }` variant;
- `insert_step_record(...)` preserves the supplied `sequence_no`;
- `insert_step_record(...)` serializes successful finished payloads into
  `result_json`;
- `insert_step_record(...)` serializes failed finished payloads into
  `error_json`;
- `insert_step_record(...)` fails when the parent `iteration_id` does not
  exist;
- duplicate `(iteration_id, sequence_no)` step-record insert fails with the
  exact `RunStateStoreError::DuplicateStepRecord { ... }` variant;
- `finish_step_record(...)` updates one existing pending row into its finished
  form and preserves `record_id`, `iteration_id`, `sequence_no`, `step`, and
  `started_at`;
- `finish_step_record(...)` fails with the exact
  `RunStateStoreError::StepRecordNotFound(...)` variant when `record_id` does
  not exist;
- `finish_step_record(...)` fails with the exact
  `RunStateStoreError::StepRecordAlreadyFinished(...)` variant when the stored
  row is already finished;
- `finish_step_record(...)` fails with the exact
  `RunStateStoreError::StepKindMismatch { ... }` variant when
  `finished_record.step` does not match the stored step kind;
- `update_run_header(...)` updates only `status`, `updated_at`, and `revision`
  on the parent run row;
- `update_run_header(...)` fails with the exact
  `RunStateStoreError::MissingParentRun(...)` variant when the target `run_id`
  does not exist;
- `load_run(...)` returns `Ok(None)` when the run does not exist;
- `load_run(...)` reconstructs one canonical `RunState` hierarchy from valid
  stored rows;
- `load_run(...)` preserves iteration order by `sequence_no asc`;
- `load_run(...)` preserves step-record order by `sequence_no asc`;
- `list_run_ids(...)` returns run ids ordered by `created_at desc`;
- `list_run_ids(...)` does not load full run hierarchy;
- `list_run_summaries(...)` returns rows ordered by `created_at desc`;
- `list_run_summaries(...)` derives `initial_user_query` from the successful
  `UserInputReceived` step of the first iteration;
- `list_run_summaries(...)` derives `final_problem_understanding` from the
  successful `ResponseValidationAndNormalization` step of the first iteration
  when present;
- `list_run_summaries(...)` returns `final_problem_understanding = None` when
  the first iteration has no successful final response;
- `list_run_summaries(...)` ignores later iterations when building summary
  fields;
- `with_transaction(...)` commits child writes when the callback returns
  `Ok(...)`;
- `with_transaction(...)` rolls back child writes when the callback returns
  `Err(...)`;
- `with_transaction(...)` rolls back all earlier writes in the same callback
  when a later tx-scoped write fails;
- read mapping rejects unknown run `status` values;
- read mapping rejects unknown step kind values;
- read mapping rejects malformed `result_json`;
- read mapping rejects malformed `error_json`;
- read mapping rejects inconsistent pending-versus-finished row shapes;
- read mapping rejects payload-variant mismatches between stored `step` and
  deserialized payload;
- raw SQLx/database errors do not leak through the public interface.

## 5) Completion Rule

Generation for crate-level runtime unit tests is complete only when all of the following are true:

- required crate-level unit tests from this document exist as executable Rust tests;
- crate-level generated unit tests comply with `Specification/runtime/unit_tests_common.md`;
- child-owned required unit tests remain delegated to their dedicated child specifications without duplication or conflict;
- required crate-level tests are generated in the same generation pass as the corresponding implementation;
- crate-level required tests are not replaced by comments, TODO markers, prose, pseudo-tests, placeholder functions without assertions, or empty test modules.
