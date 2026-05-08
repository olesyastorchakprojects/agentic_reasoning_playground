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
- `golden_eval_input`
- `startup`
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
- conversion from `GoldenEvalInputError` into `RuntimeError` produces the exact `RuntimeError::GoldenEvalInput(...)` variant;
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
- resolved settings preserve `observation_boundary_resolver.provider`, `observation_boundary_resolver.model`, `observation_boundary_resolver.prompt_asset_path`, and `observation_boundary_resolver.max_output_tokens`;
- resolved settings preserve `observation_extraction.provider`, `observation_extraction.model`, `observation_extraction.prompt_asset_path`, and `observation_extraction.max_output_tokens`;
- resolved settings preserve `prompt_context.prompt_asset_path`;
- resolved prompt-context settings preserve chunk role sources, limits, per-case limits, fallback flags, and typed tag priorities;
- resolved settings preserve `incident_evidence_retrieval.retrieval.*` and both tag-profile sections `profiles.initial` and `profiles.continuation`;
- resolved settings preserve `diagnostic_update_prompt_context.prompt_asset_path`;
- resolved diagnostic-update prompt-context settings preserve chunk role sources, limits, per-case limits, fallback flags, and typed tag priorities;
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
- `structure(...)` delegates to `structure_with_context(..., &Context::noop())`;
- `structure_with_context(...)` builds exactly two messages in order: one
  `system`, one `user`;
- `structure_with_context(...)` serializes the loaded vocabulary into compact
  JSON inside the user message;
- `structure_with_context(...)` sends `JsonSchema` response mode carrying the loaded prompt-asset schema;
- `structure_with_context(...)` sends `temperature = 0.0`;
- `structure_with_context(...)` sends the configured `max_output_tokens`;
- when `context.golden_question = Some(...)`, `structure_with_context(...)`
  computes metrics from `expected_query_structuring` and returns them as
  `QueryStructuringOutput.metrics = Some(...)`;
- when `context.golden_question = None`, `structure_with_context(...)` returns
  `QueryStructuringOutput.metrics = None`;
- metrics-helper failure under `context.golden_question = Some(...)` is wrapped
  as `QueryStructuringError::MetricsComputation`;
- successful model output preserves `prompt_tokens`, `completion_tokens`, and `total_tokens` in `QueryStructuringOutput.token_usage`;
- successful model output preserves the exact shared `QueryStructuringMetrics`
  value inside `QueryStructuringOutput.metrics` when metrics were computed;
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

### 4.7c) `information_adequacy_analyzer`

Generated unit tests for the `information_adequacy_analyzer` runtime module
must include all of the following cases:

- `new()` constructs a stateless analyzer successfully;
- repeated `analyze_initial(...)` calls with the same `StructuredUserQuery`
  input return exactly equal `AdequacyAssessment` values;
- repeated `analyze_supported_observation(...)` calls with the same
  `ObservationExtractionOutput` input return exactly equal
  `AdequacyAssessment` values;
- repeated `analyze_unsupported_observation(...)` calls with the same
  `ObservationBoundaryResolverOutput` input return exactly equal
  `AdequacyAssessment` values;
- `analyze_initial(...)` returns `AdequacyStatus::Blocking` when
  `symptom_signal_count == 0`;
- `analyze_initial(...)` returns `AdequacyStatus::Blocking` when
  `diagnostic_anchor_count <= 1`;
- `analyze_initial(...)` returns `AdequacyStatus::Blocking` when a symptom is
  present but `scope_count == 0`, `trigger_count == 0`, and
  `failure_mode_count == 0`;
- `analyze_initial(...)` returns `AdequacyStatus::Blocking` when
  `unresolved_count >= 2` and the request remains weak under the documented
  ambiguity rule;
- `analyze_initial(...)` returns `AdequacyStatus::WeakButRunnable` when
  exactly one symptom-like signal is present and no blocking rule applies;
- `analyze_initial(...)` returns `AdequacyStatus::WeakButRunnable` when
  `scope_count == 0` and no blocking rule applies;
- `analyze_initial(...)` returns `AdequacyStatus::WeakButRunnable` when
  `trigger_count == 0` and `failure_mode_count == 0` and no blocking rule
  applies;
- `analyze_initial(...)` returns `AdequacyStatus::WeakButRunnable` when
  `weak_inference_term_count > explicit_term_count` and no blocking rule
  applies;
- `analyze_initial(...)` returns `AdequacyStatus::Sufficient` when none of the
  blocking rules and none of the weak rules apply;
- `analyze_supported_observation(...)` returns `AdequacyStatus::Blocking` when
  `observation_count == 0`;
- `analyze_supported_observation(...)` returns `AdequacyStatus::Blocking` when
  `needs_more_context == true`;
- `analyze_supported_observation(...)` returns `AdequacyStatus::Blocking` when
  `observation_count == 1` and module confidence is `Low`;
- `analyze_supported_observation(...)` returns `AdequacyStatus::Blocking` for the
  documented correction-only low-confidence rule;
- `analyze_supported_observation(...)` returns `AdequacyStatus::WeakButRunnable` when
  exactly one observation is present and no blocking rule applies;
- `analyze_supported_observation(...)` returns `AdequacyStatus::WeakButRunnable` when
  `missing_context_questions` is non-empty and no blocking rule applies;
- `analyze_supported_observation(...)` returns `AdequacyStatus::WeakButRunnable` when
  `medium_or_higher_count == 0` and no blocking rule applies;
- `analyze_supported_observation(...)` returns `AdequacyStatus::WeakButRunnable` for the
  documented correction-only non-high-confidence rule when no blocking rule
  applies;
- `analyze_supported_observation(...)` returns `AdequacyStatus::Sufficient` when none of
  the blocking rules and none of the weak rules apply;
- `analyze_unsupported_observation(...)` returns `AdequacyStatus::Blocking`
  when `boundary_output.resolution = Unsupported`;
- when `status = Sufficient`, the returned `AdequacyAssessment` contains
  `missing_information_topics = []` and `follow_up_questions = []`;
- when `status = Blocking`, the returned `AdequacyAssessment` contains a
  non-empty `missing_information_topics` list and a non-empty
  `follow_up_questions` list;
- when `status = WeakButRunnable`, the returned `AdequacyAssessment` preserves
  the invariant that `follow_up_questions` is empty if and only if
  `missing_information_topics` is empty;
- `missing_information_topics` never contains duplicates;
- `follow_up_questions` never contains duplicates;
- `follow_up_questions.len()` always equals `missing_information_topics.len()`;
- `missing_information_topics.len()` never exceeds `3`;
- `follow_up_questions.len()` never exceeds `3`;
- initial-request topic selection preserves the documented priority order:
  `SymptomDescription`, `AffectedComponent`, `TriggerOrRecentChange`,
  `FailureMechanismHint`, `ExpectedVsActual`, `TermClarification`;
- observation topic selection preserves the documented priority order:
  `ObservedResult`, `CheckOutcome`, `ExecutionContext`,
  `ScopeOrBlastRadius`, `CorrectionTarget`;
- `analyze_initial(...)` selects `SymptomDescription` when
  `symptom_signal_count == 0`;
- `analyze_initial(...)` selects `SymptomDescription` when
  `symptom_signal_count == 1`;
- `analyze_initial(...)` selects `AffectedComponent` when `scope_count == 0`;
- `analyze_initial(...)` selects `TriggerOrRecentChange` when
  `trigger_count == 0`;
- `analyze_initial(...)` selects `FailureMechanismHint` only when
  `failure_mode_count == 0` and `diagnostic_anchor_count < 2`;
- `analyze_initial(...)` selects `ExpectedVsActual` only when
  `symptom_signal_count > 0` and `diagnostic_anchor_count < 2`;
- `analyze_initial(...)` selects `TermClarification` when
  `unresolved_count >= 2`;
- `analyze_initial(...)` truncates selected topics to the first three
  documented priority matches;
- `analyze_supported_observation(...)` selects `ObservedResult` when
  `observation_count == 0`;
- `analyze_supported_observation(...)` selects `ObservedResult` when
  `observation_count == 1` and module confidence is `Low`;
- `analyze_supported_observation(...)` selects `CheckOutcome` when
  `needs_more_context == true` and `missing_context_questions` is non-empty;
- `analyze_supported_observation(...)` selects `ExecutionContext` when
  `needs_more_context == true`;
- `analyze_supported_observation(...)` selects `ScopeOrBlastRadius` only under the
  documented mixed-question and observed-signal rule;
- `analyze_supported_observation(...)` selects `CorrectionTarget` only when at least one
  extracted observation has polarity `Corrected`;
- `analyze_supported_observation(...)` truncates selected topics to the first three
  documented priority matches;
- `analyze_unsupported_observation(...)` selects `ObservedResult`,
  `ExecutionContext`, and `CheckOutcome` in the documented priority order;
- `analyze_unsupported_observation(...)` truncates selected unsupported topics
  to the first two documented priority matches;
- `analyze_initial(...)` never emits observation-specific topics;
- `analyze_supported_observation(...)` never emits initial-request-specific topics;
- `analyze_unsupported_observation(...)` never emits initial-request-specific topics;
- each `MissingInformationTopic` maps to its exact canonical follow-up question
  literal;
- `follow_up_questions` are constructed only by projecting the selected topics
  through the canonical mapping;
- the module never paraphrases or otherwise rewrites canonical follow-up
  question literals;
- the order of `follow_up_questions` exactly matches the order of
  `missing_information_topics`;
- `summary_reason` is non-empty after trimming;
- `analyze_initial(...)` returns the exact summary literal
  `"The request does not describe any concrete symptom or observable behavior."`
  for the documented no-symptom blocking case;
- `analyze_initial(...)` returns the exact summary literal
  `"The request contains too little anchored diagnostic context to proceed safely."`
  for the documented anchor-count blocking case;
- `analyze_initial(...)` returns the exact summary literal
  `"The request names a symptom but does not anchor it to component, trigger, or failure pattern context."`
  for the documented isolated-symptom blocking case;
- `analyze_initial(...)` returns the exact summary literal
  `"The request remains too ambiguous because key terms are unresolved."`
  for the documented unresolved-term blocking case;
- `analyze_initial(...)` returns the exact summary literal
  `"The request contains a usable signal but is still diagnostically thin."`
  for weak initial-request cases;
- `analyze_initial(...)` returns the exact summary literal
  `"The request contains enough diagnostic context to continue."`
  for sufficient initial-request cases;
- `analyze_supported_observation(...)` returns the exact summary literal
  `"The observation does not contain a concrete new diagnostic fact."`
  for the documented empty-observation blocking case;
- `analyze_supported_observation(...)` returns the exact summary literal
  `"The observation requires more context before diagnostic update can proceed safely."`
  for the documented `needs_more_context == true` blocking case;
- `analyze_supported_observation(...)` returns the exact summary literal
  `"The observation is too weak and low-confidence for a safe diagnostic update."`
  for the documented single low-confidence observation blocking case;
- `analyze_supported_observation(...)` returns the exact summary literal
  `"The observation only corrects a prior assumption and is still too weak for a safe diagnostic update."`
  for the documented correction-only blocking case;
- `analyze_supported_observation(...)` returns the exact summary literal
  `"The observation contains a usable update signal but still lacks diagnostic strength."`
  for weak observation cases;
- `analyze_supported_observation(...)` returns the exact summary literal
  `"The observation contains enough concrete diagnostic information to continue."`
  for sufficient observation cases;
- `analyze_unsupported_observation(...)` returns the exact summary literal
  `"The latest user message is not yet a supported standalone diagnostic observation."`
  for unsupported continuation input;
- structurally invalid initial analyzer input fails through
  `InformationAdequacyAnalyzerError::InvalidStructuredUserQuery`;
- structurally invalid supported-observation analyzer input fails through
  `InformationAdequacyAnalyzerError::InvalidObservationExtractionOutput`;
- `analyze_unsupported_observation(...)` fails through
  `InformationAdequacyAnalyzerError::InvalidObservationExtractionOutput` when
  `boundary_output.resolution = Supported(...)`;
- semantic weakness never fails through the typed error boundary and is always
  reported through `AdequacyAssessment`.

### 4.7a) `query_structuring_metrics`

Generated unit tests for the `query_structuring_metrics` runtime helper module
must include all of the following cases:

- per-field vocabulary metric computation for `symptoms`,
  `affected_subsystems`, `failure_modes`, and `system_properties`;
- duplicate selected terms do not increase recall, precision, or graded metrics
  and are counted by `duplicate_term_count`;
- invalid controlled-vocabulary terms are counted by `invalid_vocab_count`;
- set-based metrics compute the documented `precision_soft`, `recall_strict`,
  `recall_soft`, `num_false_positive`, `num_false_negative_strict`, and
  `num_predicted_terms` values;
- graded metrics compute the documented `graded_coverage`,
  `average_selected_score`, and `zero_score_selection_count` values;
- grounding metrics compute the documented `grounded_strict_recall`,
  `unsupported_selected_term_rate`, `missing_evidence_span_count`,
  `invalid_evidence_span_count`, and `evidence_span_near_substring_rate`
  values;
- support-level metrics compute the documented `weak_inference_rate`,
  `strict_terms_weak_inference_rate`, and `weak_false_positive_rate` values;
- field-success metrics compute the documented `field_core_success`,
  `field_grounded_success`, and `empty_when_gold_exists` values;
- non-vocabulary field metrics compute the documented count/presence outputs for
  `entities`, `constraints`, `triggers`, `observability_signals`,
  `unresolved_terms`, `intent_present`, and `scenario_present`;
- cross-field aggregates compute the documented `macro_precision_soft`,
  `macro_recall_strict`, `macro_recall_soft`,
  `overall_grounded_strict_recall`, and `all_fields_core_success_rate` values;
- the helper returns the exact shared `QueryStructuringMetrics` shape;
- invalid golden target duplicates or invalid graded relevance shape are
  rejected as helper input errors;
- invalid golden target score assignments are rejected when:
  - a strict term is not scored `1.0`;
  - a soft-only term is not scored `0.5`;
  - a non-soft term is scored `1.0` or `0.5`;
- an all-whitespace raw user query is rejected as helper input error;
- when every field has empty `StrictGold`, `overall_grounded_strict_recall`
  evaluates to `1.0`;
- grounding metrics expose separate `missing_evidence_span_count` and
  `invalid_evidence_span_count` diagnostics with the documented semantics;
- `field_grounded_success` fails when any selected soft or false-positive term
  remains unsupported even if all strict terms are grounded;
- invalid helper inputs fail through one typed internal helper error boundary
  rather than stringly typed failures.

### 4.7b) `retrieval_metrics`

Generated unit tests for the `retrieval_metrics` runtime helper module must
include all of the following cases:

- recall metrics compute the documented `recall_soft` and `recall_strict`
  values over `ActualTopK_dedup`;
- reciprocal-rank metrics compute the documented `rr_soft`, `rr_strict`,
  `first_relevant_rank_soft`, and `first_relevant_rank_strict` values;
- relevant-count metrics compute the documented `num_relevant_soft` and
  `num_relevant_strict` values;
- `nDCG` computes the documented value over `ActualTopK_dedup` and
  deterministic `IdealTopK`;
- later duplicate ids in the ranked output do not increase recall, reciprocal
  rank, relevant counts, or nDCG after the first occurrence;
- `IdealTopK` ordering is deterministic under score ties through ascending
  lexical `id`;
- `IdealTopK` excludes grade-`0.0` ids in the current contract;
- the helper returns the exact shared `RetrievalEvaluationMetrics` shape;
- invalid normalized golden targets are rejected when:
  - `strict_positive_ids` is empty;
  - `soft_positive_ids` is empty;
  - `StrictRel` is not a subset of `SoftRel`;
  - duplicate ids appear in `strict_positive_ids`;
  - duplicate ids appear in `soft_positive_ids`;
  - duplicate ids appear in `graded_relevance`;
  - any id becomes empty after trimming;
- invalid graded-relevance assignments are rejected when:
  - a strict id is not scored `1.0`;
  - a soft-only id is not scored `0.5`;
  - a non-soft id is scored `1.0` or `0.5`;
  - a score is outside `0.0`, `0.5`, `1.0`;
- `k = 0` fails through `RetrievalMetricsError::InvalidK`;
- invalid helper inputs fail through one typed internal helper error boundary
  rather than stringly typed failures.

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
- `ranked_candidates` preserves the full retrieval order up to `top_k` even when compatibility `alternatives` are truncated;
- `ranked_candidates[0]` matches `primary.case_id` whenever `primary = Some(...)`;
- pass-through of collection errors through `CandidateCardRetrievalError::Collection`;
- request construction using the unchanged `RetrievalQueryInput.query_text`;
- request construction using `limit = top_k`;
- request construction using the configured `score_threshold`;
- when `context.golden_question = Some(...)` and candidate-card golden targets
  are non-empty, `CandidateCardRetrievalOutput.metrics = Some(...)` contains
  the exact shared `CandidateCardRetrievalMetrics` value;
- when `context.golden_question = Some(...)` and candidate-card golden targets
  contain an empty `strict_card_ids` or `soft_card_ids` list,
  `CandidateCardRetrievalOutput.metrics = None`;
- metrics-helper failure under `context.golden_question = Some(...)` is wrapped
  into `CandidateCardRetrievalError`.

### 4.9) `main`

Generated unit tests for the crate-level `main` CLI boundary must include all of the following cases:

- CLI argument parsing accepts `--config` and `--ingest-config` and preserves the supplied paths exactly;
- CLI argument parsing accepts `--golden-cases-file` and `--golden-cases-schema` together and preserves the supplied paths exactly;
- missing required `--config` fails as a startup-time CLI argument error;
- missing required `--ingest-config` fails as a startup-time CLI argument error;
- supplying `--golden-cases-file` without `--golden-cases-schema` fails as a startup-time CLI argument error;
- supplying `--golden-cases-schema` without `--golden-cases-file` fails as a startup-time CLI argument error;
- startup loads `.env` through library-owned config-loading code rather than requiring a dedicated CLI path argument for the env contract;
- startup fails before later runtime initialization when a required environment variable is absent after config loading;
- the CLI delegates config loading to library-owned code rather than parsing TOML content inside `main.rs`;
- the CLI delegates golden-file validation and parsing to the dedicated `golden_eval_input` module rather than implementing that logic inline in `main.rs`.
- in interactive mode, the first user message is routed through `orchestrator.run(...)`;
- after `RunOutcome::Finished { run_id, ... }` in interactive mode, the CLI retains that `run_id` as the current active run id;
- interactive mode offers exactly two post-success choices:
  - finish the run;
  - add an observation;
- interactive mode routes `add observation` through `orchestrator.resume_with_input(run_id, user_request)`;
- interactive mode routes `finish the run` by clearing the CLI-owned current run id and starting the next user message through `orchestrator.run(...)`;
- after `RunOutcome::WaitingForUser { run_id, follow_up_questions }` in interactive mode, the CLI surfaces the exact `follow_up_questions` and routes the next user message through `orchestrator.resume_with_input(run_id, user_request)`;
- after `RunOutcome::WaitingForUser { .. }`, interactive mode does not route the next user message through `orchestrator.resume(run_id)`;
- after `RunOutcome::WaitingForUser { .. }`, interactive mode treats the next user message as a new iteration inside the same run rather than as a continuation of the short iteration;
- interactive mode does not require an explicit archive action when the operator chooses `finish the run`.

### 4.10) `golden_eval_input`

Generated unit tests for the `golden_eval_input` runtime module must include
all of the following cases:

- reading a missing golden-cases file fails with the exact `GoldenEvalInputError::GoldenCasesRead { ... }` variant;
- reading a missing golden-cases schema file fails with the exact `GoldenEvalInputError::GoldenCasesSchemaRead { ... }` variant;
- malformed golden JSON fails with the exact `GoldenEvalInputError::InvalidJson { ... }` variant;
- schema validation failure fails with the exact `GoldenEvalInputError::SchemaValidation { ... }` variant;
- typed parsing failure after successful schema validation fails with the exact `GoldenEvalInputError::TypedParse { ... }` variant;
- successful loading returns one `UserRequest` per golden-case item;
- each returned `UserRequest.query` is copied from `golden_case.query.raw`;
- each returned `UserRequest.golden_question` is `Some(...)` and preserves the typed golden-case structure;
- the module does not create `RunState`, does not assemble `Context`, and does not invoke orchestrator entrypoints.

### 4.10a) `startup`

Generated unit tests for the `startup` runtime module must include all of the
following cases:

- `build_orchestrator(settings)` succeeds when all typed child-module constructors succeed;
- `build_orchestrator(settings)` returns `StartupError` rather than `RuntimeError` or untyped string errors;
- child-module construction failures are preserved through typed `StartupError` variants rather than flattened into strings;
- `build_orchestrator(settings)` constructs and returns an `Orchestrator` whose dependencies are wired from the supplied resolved `Settings`;
- startup wiring constructs `DiagnosticLoopTransitionPolicy` as the default policy for the continuation-capable diagnostic-loop runtime stage;
- startup wiring constructs `ObservationBoundaryResolver` from `Settings.observation_boundary_resolver` and the provider-selected model client;
- startup wiring constructs `ObservationExtraction` from `Settings.observation_extraction` and the provider-selected model client;
- startup wiring constructs `InformationAdequacyAnalyzer` through its infallible stateless constructor;
- startup wiring constructs `IncidentEvidenceRetrieval` from `Settings.incident_evidence_retrieval`;
- startup wiring constructs `DiagnosticUpdatePromptContextAssembly` from `Settings.diagnostic_update_prompt_context`;
- startup wiring constructs `CardBranchReranking` through its infallible stateless constructor;
- startup wiring populates `StepExecutorModules` with both initial-path and continuation-path leaf modules.

### 4.11) `incident_evidence_retrieval`

Generated unit tests for the `incident_evidence_retrieval` runtime module must include all of the following cases:

- constructor rejection of `settings.retrieval.top_k = 0`;
- empty `primary_card_id` fails through `IncidentEvidenceRetrievalError`;
- primary-only input issues exactly one collection call using one `case_id`, the selected primary tag profile, `limit = settings.retrieval.top_k`, and `score_threshold = settings.retrieval.score_threshold`;
- alternatives-only input issues exactly one collection call using alternative `case_id`s in original order, the selected alternative tag profile, `limit = settings.retrieval.top_k`, and `score_threshold = settings.retrieval.score_threshold`;
- combined input with both primary and alternatives issues exactly two collection calls;
- `iteration_profile = Initial` selects `settings.profiles.initial`;
- `iteration_profile = Continuation` selects `settings.profiles.continuation`;
- successful mapping from `PracticeChunkSearchHit` into `IncidentEvidenceChunk`;
- preservation of primary-search hit order in `primary_chunks`;
- preservation of alternative-search hit order in `alternative_chunks`;
- pass-through of collection errors through `IncidentEvidenceRetrievalError::Collection`;
- whole-module failure when one branch succeeds and the other branch returns a collection error;
- no deduplication of returned chunks;
- when `context.golden_question = Some(...)` and both incident-evidence golden
  branches are non-empty, `IncidentEvidenceRetrievalOutput.metrics = Some(...)`
  contains the exact shared `IncidentEvidenceRetrievalMetrics` value with both
  branch-local bundles preserved;
- when either incident-evidence golden branch contains an empty
  `strict_chunk_ids` or `soft_chunk_ids` list,
  `IncidentEvidenceRetrievalOutput.metrics = None`;
- metrics-helper failure under `context.golden_question = Some(...)` is wrapped
  into `IncidentEvidenceRetrievalError`.

### 4.12) `theory_evidence_retrieval`

Generated unit tests for the `theory_evidence_retrieval` runtime module must include all of the following cases:

- constructor rejection of `top_k = 0`;
- constructor rejection of negative `score_threshold`;
- constructor rejection of `score_threshold = f32::NAN`;
- constructor rejection of `score_threshold = f32::INFINITY`;
- constructor rejection of `score_threshold = f32::NEG_INFINITY`;
- constructor success when `top_k > 0`;
- `retrieve(...)` issues exactly one collection call for a valid request;
- request construction uses the unchanged `RetrievalQueryInput.query_text`;
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
- `retrieve(...)` does not require candidate-card, card-hydration, or incident-evidence inputs;
- when `context.golden_question = Some(...)` and theory-evidence golden targets
  are non-empty, `TheoryEvidenceRetrievalOutput.metrics = Some(...)` contains
  the exact shared `TheoryEvidenceRetrievalMetrics` value;
- when `context.golden_question = Some(...)` and theory-evidence golden targets
  contain an empty `strict_chunk_ids` or `soft_chunk_ids` list,
  `TheoryEvidenceRetrievalOutput.metrics = None`;
- metrics-helper failure under `context.golden_question = Some(...)` is wrapped
  into `TheoryEvidenceRetrievalError`.

### 4.13) `prompt_context_assembly`

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

### 4.13a) `diagnostic_update_prompt_context_assembly`

Generated unit tests for the `diagnostic_update_prompt_context_assembly` runtime module must include all of the following cases:

- constructor rejects empty `prompt_asset_path`;
- constructor rejects unreadable prompt asset file;
- constructor rejects unreadable derived prompt asset schema file;
- constructor rejects invalid prompt asset JSON;
- constructor rejects invalid prompt asset schema JSON;
- constructor rejects prompt asset JSON that does not satisfy the derived prompt asset schema;
- constructor rejects prompt asset missing `{{json_context}}`;
- constructor rejects prompt asset with more than one `{{json_context}}`;
- constructor succeeds with a valid prompt asset loaded from `DiagnosticUpdatePromptContextSettings.prompt_asset_path`;
- constructor derives the prompt asset schema path from the prompt asset directory by replacing the prompt asset file name suffix with `.schema.json`;
- missing hydrated primary card fails with `DiagnosticUpdatePromptContextAssemblyError::MissingPrimaryCard`;
- output includes `problem_understanding` and does not duplicate a separate `user_problem` field;
- output includes `resolved_observation.text`;
- output includes extracted observations in original order;
- output includes `diagnostic_state.active_hypotheses[*].hypothesis_id` and `text`;
- output includes `diagnostic_state.rejected_hypotheses[*].hypothesis_id`, `text`, and `rejection_reason`;
- output includes `diagnostic_state.last_check.text` when a last check is supplied;
- output includes a compact prompt-facing primary-card summary as `primary_incident_card`;
- output omits empty top-level and nested fields from the serialized `json_context`;
- rendered prompt contains the strict JSON output schema;
- rendered prompt contains a valid serialized JSON context after the `JSON context follows:` marker;
- rendered prompt serializes incident evidence roles through the exact snake-case mapping: `evidence_for_match`, `next_check_hint`, `supporting_explanation`, and `alternative_context`;
- rendered prompt serializes theory evidence with role `mechanism_explanation`;
- output returns selected incident chunks separately from the prompt for history;
- output returns selected theory chunks separately from the prompt for history;
- `evidence_topology.primary_evidence_roles` preserves the documented primary role order;
- `evidence_topology.alternative_context_present` is `true` when alternative-context chunks are selected;
- `evidence_topology.theory_evidence_present` is `true` when theory chunks are selected.

### 4.14) `orchestrator`

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
- successful compatibility validation accepts `ObservationBoundaryResolver`,
  `ObservationExtraction`, `CardBranchReranking`, and
  `DiagnosticUpdatePromptContextAssembly`;
- successful compatibility validation accepts `InformationAdequacyInitial`,
  `InformationAdequacySupportedObservation`, and
  `InformationAdequacyUnsupportedObservation`;
- step-specific error compatibility validation accepts
  `ObservationBoundaryResolver`, `ObservationExtraction`,
  `CardBranchReranking`, and `DiagnosticUpdatePromptContextAssembly`;
- step-specific error compatibility validation accepts
  `InformationAdequacyInitial`,
  `InformationAdequacySupportedObservation`, and
  `InformationAdequacyUnsupportedObservation`;
- text variants of `StepError` reject empty `message` values when generated
  constructors or validators are present;
- `RunStateView::new` wraps a borrowed `RunState`;
- `RunStateView::run_id()` returns the underlying run id;
- `RunStateView::status()` returns the underlying run status;
- `RunStateView::iteration_count()` equals `RunState.iterations.len()`;
- `RunStateView::iteration(iteration_id)` returns the underlying stored
  iteration borrow for an existing id and `None` for an unknown id;
- `RunStateView::iterations()` preserves `RunState.iterations` order;
- `RunStateView::normal_iterations()` preserves the relative order of normal
  iterations only;
- `RunStateView::short_iterations()` preserves the relative order of short
  iterations only;
- `RunStateView::last_iteration()` returns the last iteration;
- `IterationView::iteration_id()` returns the underlying iteration id;
- `IterationView::status()` returns the underlying iteration status;
- `IterationView::step_count()` equals `RunIteration.step_records.len()`;
- `IterationView::steps()` preserves `RunIteration.step_records` order;
- `IterationView::finished_steps()` returns only finished records and preserves
  their relative order;
- `IterationView::pending_step()` returns `Some` when the current iteration has
  a pending step and `None` otherwise;
- `IterationView::finished_step(kind)` returns the last finished step with the
  requested `StepKind`;
- `IterationView::is_normal_iteration()` returns `true` for
  `RunIterationStatus::Active` and `RunIterationStatus::FinishedWithSuccess`;
- `IterationView::is_short_iteration()` returns `true` only for
  `RunIterationStatus::FinishedWithWaitInput`;
- `RunIterationStatus::FinishedWithError` makes both
  `IterationView::is_normal_iteration()` and
  `IterationView::is_short_iteration()` return `false`;
- `IterationView::is_normal_iteration()` and
  `IterationView::is_short_iteration()` never both return `true` for the same
  iteration;
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
- the new iteration starts with `RunIterationStatus::Active`;
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
- `finish_current_iteration_error()` sets `RunStatus::Error`;
- `wait_for_user()` sets `RunStatus::WaitingForUser`;
- `wait_for_user()` returns `PendingStepAlreadyExists` when any pending step
  exists in the run;
- `wait_for_user()` returns `CurrentIterationClosed` when the current
  iteration is not active;
- `wait_for_user()` does not append a new iteration or a new step record;
- `wait_for_user()` leaves the current last iteration structurally unchanged
  apart from the iteration-status mutation and the matching run-header
  mutation;
- `wait_for_user()` sets the current iteration status to
  `RunIterationStatus::FinishedWithWaitInput`;
- `finish_current_iteration_success()` sets the current iteration status to
  `RunIterationStatus::FinishedWithSuccess`;
- `finish_current_iteration_error()` sets the current iteration status to
  `RunIterationStatus::FinishedWithError`;
- `record_failure(...)` sets the current iteration status to
  `RunIterationStatus::FinishedWithError`;
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
- `StepExecutor::execute(...)` delegates to `StepExecutor::execute_with_context(...)`
  using `Context::noop()`;
- `StepExecutor::execute_with_context(...)` passes the supplied execution-time
  `Context` unchanged into context-aware leaf-module methods;
- `StepExecutor::execute(...)` returns `StepError::InvalidState` for
  `StepKind::UserInputReceived`;
- each executable `StepKind` dispatches to the correct leaf-module method;
- each executable `StepKind` returns the correct `StepResultEnvelope` variant on
  success;
- continuation-only executable steps `ObservationBoundaryResolver`,
  `ObservationExtraction`, `CardBranchReranking`, and
  `DiagnosticUpdatePromptContextAssembly` dispatch to the correct leaf-module
  method;
- `InformationAdequacyInitial` dispatches to `analyze_initial(...)`;
- `InformationAdequacySupportedObservation` dispatches to
  `analyze_supported_observation(...)`;
- `InformationAdequacyUnsupportedObservation` dispatches to
  `analyze_unsupported_observation(...)`;
- `InformationAdequacyUnsupportedObservation` returns
  `StepError::InvalidState` when the current iteration stores a successful
  `ObservationBoundaryResolver` result whose resolution is not `Unsupported`;
- missing required finished inputs return `StepError::MissingRequiredInput`;
- prerequisite finished steps recorded as `Err(...)` return
  `StepError::MissingRequiredInput`;
- mismatched prerequisite success-envelope variants return
  `StepError::InvalidState`;
- multi-input steps `IncidentEvidenceRetrieval` and `PromptContextAssembly`
  require all declared finished inputs from the current iteration;
- `StepExecutor::execute(...)` does not use older iterations as substitutes for
  missing direct prerequisite steps in the current iteration;
- `StepExecutor::execute(...)` may read older iterations only when building
  history-derived projections such as `DiagnosticContext` and
  `CardSelectionContext`;
- continuation-path `IncidentEvidenceRetrieval` builds `IterationProfile::Continuation`;
- initial-path retrieval-facing steps build `RetrievalQueryInput.query_text` from
  `NormalizedUserRequest.query`;
- continuation-path retrieval-facing steps build `RetrievalQueryInput.query_text`
  as the concatenation of the latest closed `ProblemUnderstanding.text` and
  supported `ResolvedObservation.text`;
- continuation-path retrieval-facing steps place the previous closed
  problem-understanding string before `ResolvedObservation.text` and separate
  them with exactly one ASCII space;
- continuation-path `CardBranchReranking` builds `CardSelectionContext` from
  persisted run-state history;
- continuation-path `DiagnosticUpdatePromptContextAssembly` builds
  `HydratedCardBranchesInput` from the current iteration's `CardHydrationOutput`;
- continuation-path `DiagnosticUpdatePromptContextAssembly` uses the latest
  closed `ProblemUnderstanding` entry rather than a current-iteration partial
  entry whose `text` is `None`;
- continuation-path `LlmStructuredGeneration` accepts prompt context from
  either `PromptContextAssembly` or `DiagnosticUpdatePromptContextAssembly`;
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
- `RunRepository::update_iteration_status(...)` updates exactly one iteration
  status row and the matching run header;
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
- when the current iteration `UserRequest` contains `golden_question = Some(...)`,
  the assembled execution-time `Context` preserves the same
  `golden_question` value;
- `load_existing_run(run_id)` maps repository `Ok(None)` into
  `OrchestratorError::RunNotFound { run_id }`;
- `drive_to_outcome(...)` asks policy for a new `PolicyTransition` on each loop
  iteration;
- when policy returns `ExecuteStep { step }`, orchestrator opens a pending step
  before calling `StepExecutor::execute_with_context(...)`;
- when policy returns `ExecuteStep { step }`, orchestrator persists the pending
  step record before calling `StepExecutor::execute_with_context(...)`;
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
- when policy returns `WaitForUser`, orchestrator calls
  `RunStateWriter::wait_for_user()` exactly once;
- when policy returns `WaitForUser`, orchestrator persists the resulting
  iteration-status mutation and matching run-header mutation through
  `RunRepository::update_iteration_status(...)`;
- when policy returns `WaitForUser`, orchestrator returns
  `RunOutcome::WaitingForUser`;
- `RunOutcome::WaitingForUser` preserves the exact
  `follow_up_questions` payload returned by policy;
- `RunOutcome::WaitingForUser` closes the current iteration for further step
  execution and classifies it as a short iteration;
- `PolicyTransition::FinishWithResult` marks the current iteration
  `RunIterationStatus::FinishedWithSuccess` before returning;
- failed step execution persists the current iteration status as
  `RunIterationStatus::FinishedWithError`;
- `PolicyTransition::FinishWithError` persists the current iteration status as
  `RunIterationStatus::FinishedWithError` before returning
  `RunOutcome::Failed`;
- `drive_to_outcome(...)` returns `RunOutcome::Finished` when policy returns
  `FinishWithResult`;
- `drive_to_outcome(...)` returns `RunOutcome::Failed` when policy returns
  `FinishWithError`;
- `RunOutcome::WaitingForUser` represents a non-failure pause of the current
  iteration and does not imply run archival or permanent run closure;
- `RunOutcome::Finished` represents successful completion of the current
  iteration and does not imply run archival or permanent run closure;
- `drive_to_outcome(...)` never calls `StepExecutor::execute_with_context(...)`
  for
  `StepKind::UserInputReceived`;
- `resume(run_id)` preserves already successful finished steps in the current
  iteration and lets policy continue from the persisted state when the run is
  not waiting for user input;
- `resume_with_input(run_id, user_input)` makes the new last iteration the only
  iteration inspected by policy and executor for that invocation;
- after `RunOutcome::WaitingForUser`, `resume_with_input(run_id, user_input)`
  appends a new iteration rather than trying to continue step execution inside
  the short iteration;
- after `RunOutcome::WaitingForUser`, `resume(run_id)` is not the supported
  path for answering follow-up questions;
- `resume(run_id)` on a run with `RunStatus::WaitingForUser` returns the exact
  `OrchestratorError::WaitingForUserRequiresNewInput { run_id }` variant;
- orchestrator does not reinterpret `RunOutcome::Finished` as automatic run
  archival or final closure;
- `DiagnosticLoopTransitionPolicy` chooses the initial canonical order when the
  current iteration index is `0`;
- `DiagnosticLoopTransitionPolicy` chooses the continuation canonical order
  when the current iteration index is greater than `0`;
- in the initial canonical order, `InformationAdequacyInitial` is selected
  after `QueryStructuring` and before `CandidateCardRetrieval`;
- in the continuation canonical order, policy selects
  `InformationAdequacyUnsupportedObservation` immediately after
  `ObservationBoundaryResolver` when
  `ObservationBoundaryResolverOutput.resolution = Unsupported`;
- in the continuation canonical order, policy selects
  `InformationAdequacySupportedObservation` after `ObservationExtraction` and
  before `CandidateCardRetrieval`;
- initial adequacy `Blocking` and `WeakButRunnable` results produce
  `PolicyTransition::WaitForUser` with the exact
  `AdequacyAssessment.follow_up_questions`;
- supported-observation adequacy `Blocking` and `WeakButRunnable` results
  produce `PolicyTransition::WaitForUser` with the exact
  `AdequacyAssessment.follow_up_questions`;
- unsupported-observation adequacy results produce
  `PolicyTransition::WaitForUser` with the exact
  `AdequacyAssessment.follow_up_questions`;
- `DiagnosticLoopTransitionPolicy` returns `FinishWithResult` from a successful
  `ResponseValidationAndNormalization` step in either iteration profile;
- orchestrator unit tests may use generated private or `#[cfg(test)]` fakes,
  spies, adapters, or harness constructors for `StepExecutor` and
  `RunRepository` as long as the documented production public API remains
  unchanged.

### 4.15) `api_clients.postgres.run_state_store`

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
- `insert_iteration(...)` preserves the supplied `RunIteration.status`;
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
- `update_iteration_status(...)` updates only the target iteration-row status
  and no step-record rows;
- `update_iteration_status(...)` fails when the target `iteration_id` does not
  exist;
- `update_iteration_status(...)` plus the matching repository-level header
  update do not insert a synthetic iteration marker or a synthetic step-record
  marker for a wait-for-user pause;
- `update_run_header(...)` fails with the exact
  `RunStateStoreError::MissingParentRun(...)` variant when the target `run_id`
  does not exist;
- `load_run(...)` returns `Ok(None)` when the run does not exist;
- `load_run(...)` reconstructs one canonical `RunState` hierarchy from valid
  stored rows;
- `load_run(...)` reconstructs `RunIteration.status` from persisted iteration
  rows;
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

### 4.16) `diagnostic_context`

Generated unit tests for the `diagnostic_context` module must include all of the following cases:

**`from_run_state` — empty and partial first normal iteration:**

- an empty `RunState` (no iterations) produces a `DiagnosticContext` whose `run_id` matches and all `Vec` fields are empty;
- a `RunState` whose first normal iteration contains only a successful `InputNormalization` step produces one `ProblemUnderstanding` entry with `text = None` and `source = InitialRequest(normalized_query)`;
- a `RunState` whose first normal iteration contains both `InputNormalization` and `ResponseValidationAndNormalization` produces one `ProblemUnderstanding` entry with `text = Some(response.problem_understanding)` and `source = InitialRequest(normalized_query)`;
- `ProblemUnderstanding.source = InitialRequest(query)` carries exactly `NormalizedUserRequest.query` from the `InputNormalization` step result.

**`from_run_state` — hypothesis construction from the first normal iteration:**

- each `Hypothesis` in `DiagnosticResponse.hypotheses` produces one `TrackedHypothesis` with `hypothesis_id = hypothesis.id`, `text = hypothesis.text`, and exactly one `HypothesisState` entry in `state_history`;
- `HypothesisState.status`, `HypothesisState.confidence`, and `HypothesisState.source` are copied directly from the corresponding `Hypothesis` fields in the response;
- `HypothesisState.problem_understanding` is the `ProblemUnderstanding` entry built for the same iteration;
- `TrackedHypothesis.hypothesis_id` is taken from `Hypothesis.id` in the response and is not re-generated;
- a successful `ResponseValidationAndNormalization` step in the first normal iteration produces one `SuggestedCheck` with `text = response.first_check`.

**`from_run_state` — iteration N construction:**

- a `RunState` with a successful `ObservationBoundaryResolver` step in iteration N whose `resolution = Supported(...)` sets `DiagnosticUpdate.observation = Some(Observation { normalized_user_input, resolved, ... })` in that iteration's `ProblemUnderstanding` entry;
- `DiagnosticUpdate.observation` is `None` when the `ObservationBoundaryResolver` step is absent or failed for that iteration;
- `DiagnosticUpdate.observation` is `None` when the `ObservationBoundaryResolver` step succeeds with `resolution = Unsupported`;
- `DiagnosticUpdate.problem_understanding` is populated from the `text` field of the previous iteration's `ProblemUnderstanding` entry, not from the current iteration's model output;
- a successful `ResponseValidationAndNormalization` step in iteration N sets the current iteration's `ProblemUnderstanding.text = Some(response.problem_understanding)`;
- `ProblemUnderstanding.text` is `None` for an iteration whose `ResponseValidationAndNormalization` step is absent or failed.

**`from_run_state` — hypothesis update across iterations:**

- a hypothesis whose `id` appears in the `DiagnosticResponse.hypotheses` of iteration N and was already introduced in a prior iteration has one new `HypothesisState` appended to its existing `state_history`;
- a hypothesis whose `id` appears in iteration N but was not present in any prior iteration creates a new `TrackedHypothesis` with one initial `HypothesisState`;
- a hypothesis whose latest `HypothesisState.status` is `Rejected(reason)` carries the rejection reason string from the corresponding `HypothesisStatus::Rejected` payload;
- a `RunState` with three closed iterations produces `state_history` entries in iteration order for each `TrackedHypothesis` that appeared in all three responses.

**`from_run_state` — error and skip behavior:**

- an iteration whose required step result is absent is silently skipped without returning an error;
- an iteration whose `FinishedStepRecord.result` is `Err(_)` is silently skipped without returning an error;
- a short iteration returned by `RunStateView::short_iterations()` is silently
  skipped and contributes no entries to the resulting `DiagnosticContext`;
- `DiagnosticContextError::InvalidStepPayload` is returned when a step result is present and successful but its payload cannot be projected into the expected domain types;
- `InvalidStepPayload` carries the `iteration_id` of the offending iteration.

**Observation status rule:**

- a single observation from iteration 1 has `status = Pending` after `from_run_state` completes;
- two observations from iterations 1 and 2 have `status = Processed` for iteration 1 and `status = Pending` for iteration 2;
- at most one `Observation` entry has `status = Pending` in any `DiagnosticContext` produced by `from_run_state`.

**Ordering invariants:**

- `problem_understanding` entries are in iteration sequence order after `from_run_state`;
- `observations` entries are in iteration sequence order after `from_run_state`;
- `suggested_checks` entries are in iteration sequence order after `from_run_state`;
- `state_history` entries within each `TrackedHypothesis` are in iteration sequence order;
- only normal iterations contribute `DiagnosticContext` entries;
- the first `problem_understanding` entry, when present, has `source = InitialRequest(_)`;
- all `problem_understanding` entries after the first have `source = DiagnosticUpdate { ... }`.

**View methods:**

- `current_problem_understanding()` returns `None` for an empty `DiagnosticContext`;
- `current_problem_understanding()` returns the last element of `problem_understanding`;
- `active_hypotheses()` returns only `TrackedHypothesis` entries whose last `HypothesisState.status` is `Active` or `Weakened`;
- `active_hypotheses()` excludes `TrackedHypothesis` entries whose last `HypothesisState.status` is `Rejected(_)`;
- `rejected_hypotheses()` returns only `TrackedHypothesis` entries whose last `HypothesisState.status` is `Rejected(_)`;
- `rejected_hypotheses()` excludes `TrackedHypothesis` entries whose last `HypothesisState.status` is `Active` or `Weakened`;
- `active_hypotheses()` preserves the order of entries in `self.hypotheses`;
- `rejected_hypotheses()` preserves the order of entries in `self.hypotheses`;
- `last_check()` returns `None` for an empty `DiagnosticContext`;
- `last_check()` returns the last element of `suggested_checks`;
- `current_observation()` returns `None` for an empty `DiagnosticContext`;
- `current_observation()` returns the last element of `observations`;
- `active_hypotheses()` returns an empty vec when all hypotheses have been rejected;
- `rejected_hypotheses()` returns an empty vec when all hypotheses are active or weakened.

### 4.16a) `card_selection_context`

Generated unit tests for the `card_selection_context` module must include all of the following cases:

- an empty `RunState` with no iterations returns `CardSelectionContext { history: vec![] }`;
- the first normal iteration is projected from successful `CandidateCardRetrievalOutput`;
- the first normal iteration sets `primary_card_status = PrimaryCardStatus::Tentative`;
- the first normal iteration copies `primary.case_id` into `primary_card_id`;
- the first normal iteration copies compatibility `alternatives[*].case_id` in order;
- the first normal iteration sets `challenger_card_ids = vec![]`;
- later iterations are projected from successful `CardBranchRerankingOutput`;
- later iterations copy `primary_card_id`, `primary_card_status`, `alternative_card_ids`, and `challenger_card_ids` exactly from reranking output;
- missing successful `CandidateCardRetrievalOutput` in the first normal iteration returns the typed missing-step error;
- an active first normal iteration with no successful `CandidateCardRetrievalOutput` yet is silently skipped without error;
- successful `CandidateCardRetrievalOutput` in the first normal iteration with `primary = None` returns `CardSelectionContextError::MissingInitialPrimaryCard`;
- missing successful `CardBranchRerankingOutput` in iteration `N > 0` returns the typed missing-step error for that iteration id;
- an active later normal iteration with no successful `CardBranchRerankingOutput` yet is silently skipped without error;
- a short iteration returned by `RunStateView::short_iterations()` is silently
  skipped and contributes no `CardSelectionSnapshot`;
- a card id that appears in more than one branch of the same snapshot returns `CardSelectionContextError::DuplicateCardAcrossBranches` with that card id;
- `history` preserves iteration order.

### 4.16b) `card_branch_reranking`

Generated unit tests for the `card_branch_reranking` module must include all of the following cases:

- `new()` constructs a stateless reranker successfully;
- empty `CardSelectionContext.history` fails with `CardBranchRerankingError::EmptyCardSelectionHistory`;
- missing `fresh_candidates.primary` fails with `CardBranchRerankingError::MissingFreshPrimary`;
- duplicate ids in `ranked_candidates` fail with `CardBranchRerankingError::DuplicateFreshCandidate`;
- mismatch between `primary.case_id` and `ranked_candidates[0].case_id` fails with `CardBranchRerankingError::FreshPrimaryMismatch`;
- fewer than five `ranked_candidates` fail with `CardBranchRerankingError::InsufficientFreshCandidateWindow`;
- initial tentative primary is preserved when it remains within the tentative retention window;
- initial tentative primary is replaced by fresh `top-1` when it falls outside the tentative retention window;
- preserved tentative primary becomes sticky on the resulting output;
- sticky primary is preserved when it remains within the sticky retention window;
- sticky primary is replaced by fresh `top-1` when it falls outside the sticky retention window;
- any replacement primary returns with `primary_card_status = PrimaryCardStatus::Tentative`;
- preserved alternatives retain historical order;
- alternatives are refilled from fresh retrieval order after preserved alternatives;
- `primary_card_id` never appears in `alternative_card_ids`;
- `challenger_card_ids` is always returned as an empty vector in the current version.

### 4.17) `observation_boundary_resolver`

Generated unit tests for the `observation_boundary_resolver` runtime module must include all of the following cases:

- constructor rejects empty `prompt_asset_path`;
- constructor rejects `max_output_tokens = 0`;
- constructor rejects an unreadable prompt asset file;
- constructor rejects an unreadable derived prompt-asset schema file;
- constructor rejects invalid prompt asset JSON;
- constructor rejects invalid prompt-asset schema JSON;
- constructor succeeds with a valid prompt asset loaded from `ObservationBoundaryResolverSettings.prompt_asset_path`;
- request execution sends `response_mode = JsonSchema` carrying the loaded prompt-asset schema;
- request execution sends the configured `max_output_tokens`;
- prompt assembly reads problem understanding through `diagnostic_context.current_problem_understanding()`, active hypotheses through `diagnostic_context.active_hypotheses()`, and the latest check through `diagnostic_context.last_check()`;
- prompt assembly injects only the latest closed problem understanding, active hypotheses as strings, the latest suggested check, and `NormalizedUserRequest.query`;
- request execution fails through the typed error boundary when `diagnostic_context.current_problem_understanding()` returns `None`;
- request execution fails through the typed error boundary when `diagnostic_context.last_check()` returns `None`;
- request execution fails through the typed error boundary when `diagnostic_context.current_problem_understanding().text` is `None`;
- valid model output with `supported = true` maps into `ObservationBoundaryResolution::Supported(ResolvedObservation { text })`;
- valid model output with `supported = false` maps into `ObservationBoundaryResolution::Unsupported`;
- model output with `supported = true` and `full_query = null` fails with the module-owned invalid-model-output error;
- model output with `supported = false` and non-null `full_query` fails with the module-owned invalid-model-output error;
- model output with an unknown `confidence` value fails with the module-owned invalid-model-output error;
- `normalized_user_input` in successful output exactly matches `NormalizedUserRequest.query`;
- invalid JSON returned by the model fails through the typed error boundary;
- provider/model selection is not stored inside the narrow `ObservationBoundaryResolverSettings` constructor type;
- model output with `supported = true` and `full_query` equal to an empty or whitespace-only string fails with the module-owned invalid-model-output error;
- model output with a non-`Stop` `finish_reason` (e.g. `length`) fails with the module-owned invalid-model-output error;
- constructor rejects a `user_template` that contains an extra `{{...}}` placeholder beyond the two required ones.

### 4.18) `observation_extraction`

Generated unit tests for the `observation_extraction` runtime module must include all of the following cases:

- constructor rejects empty `prompt_asset_path`;
- constructor rejects `max_output_tokens = 0`;
- constructor rejects an unreadable prompt asset file;
- constructor rejects an unreadable derived prompt-asset schema file;
- constructor rejects invalid prompt asset JSON;
- constructor rejects invalid prompt-asset schema JSON;
- constructor rejects a `user_template` that contains an extra `{{...}}` placeholder beyond the required `{{user_message}}`;
- constructor succeeds with a valid prompt asset loaded from `ObservationExtractionSettings.prompt_asset_path`;
- provider/model selection is not stored inside the narrow `ObservationExtractionSettings` constructor type;
- `input.resolution = Unsupported` returns `ObservationExtractionError::UnsupportedBoundaryInput` without issuing a model call;
- request execution sends `response_mode = JsonSchema` carrying the loaded prompt-asset schema;
- request execution sends the configured `max_output_tokens`;
- prompt assembly substitutes the resolved standalone observation text into `{{user_message}}`;
- valid model output maps into `ObservationExtractionOutput` with correct field values;
- `needs_more_context = false` with an empty `observations` array fails with the module-owned invalid-model-output error;
- `needs_more_context = false` with a non-empty `missing_context_questions` array fails with the module-owned invalid-model-output error;
- `needs_more_context = true` with zero `missing_context_questions` fails with the module-owned invalid-model-output error;
- `needs_more_context = true` with more than two `missing_context_questions` fails with the module-owned invalid-model-output error;
- `source_span` that is not an exact substring of the resolved observation text fails with the module-owned invalid-model-output error;
- `source_span` is trimmed before substring verification;
- `polarity = "present"` maps to `ObservationPolarity::Present`;
- `polarity = "absent"` maps to `ObservationPolarity::Absent`;
- `polarity = "corrected"` maps to `ObservationPolarity::Corrected`;
- model output with an unknown `confidence` value fails with the module-owned invalid-model-output error;
- invalid JSON returned by the model fails through the typed error boundary;
- model transport failure propagates as `ObservationExtractionError::ModelClient`;
- model output with a non-`Stop` `finish_reason` (e.g. `length`) fails with the module-owned invalid-model-output error;
- `normalized_user_input` in successful output exactly matches `input.normalized_user_input`;
- `resolved_observation` in successful output exactly matches the `ResolvedObservation` carried by `input.resolution`.

## 5) Completion Rule

Generation for crate-level runtime unit tests is complete only when all of the following are true:

- required crate-level unit tests from this document exist as executable Rust tests;
- crate-level generated unit tests comply with `Specification/runtime/unit_tests_common.md`;
- child-owned required unit tests remain delegated to their dedicated child specifications without duplication or conflict;
- required crate-level tests are generated in the same generation pass as the corresponding implementation;
- crate-level required tests are not replaced by comments, TODO markers, prose, pseudo-tests, placeholder functions without assertions, or empty test modules.
