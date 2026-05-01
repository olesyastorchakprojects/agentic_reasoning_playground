# 1) Purpose / Scope

This document defines the OpenInference span contract for the runtime crate.

It defines:
- the OpenInference span hierarchy;
- OpenInference span ownership;
- required OpenInference span names and span kinds;
- required common OpenInference attributes;
- per-span input/output payload contracts;
- required success and error recording rules;
- the relationship between OpenInference spans and the standard runtime spans
  defined in `spans.md`.

This document does not define:
- OTLP exporter wiring;
- Phoenix or Tempo deployment details;
- ordinary runtime span hierarchy outside the OpenInference surface;
- dashboard queries;
- cross-request analytics.

The current generated Rust artifact owner for the OpenInference span factories
is:

- `src/observability/mod.rs`

# 2) Role Of OpenInference Spans

The runtime emits OpenInference spans as a second trace surface alongside the
standard runtime spans defined in `Specification/runtime/observability/spans.md`.

The OpenInference surface exists to:
- expose request-pipeline execution in Phoenix-friendly semantic form;
- attach structured input/output payload summaries to key chain, retriever, LLM,
  and guardrail stages;
- preserve stage-local success/error state using the OpenInference attribute
  model.

OpenInference spans do not replace the ordinary runtime spans.
Both surfaces are required in the current contract.

# 3) Relationship To Standard Runtime Spans

The runtime contains two concurrent span surfaces:

- standard runtime spans such as `diagnostics.run`,
  `request_pipeline.query_structuring`, and `llm.call.query_structuring`;
- OpenInference spans such as `oi.chain.diagnostic_iteration` and
  `oi.llm.query_structuring`.

Relationship rules:
- standard runtime spans remain the source of truth for orchestration-wide trace
  shape;
- OpenInference spans provide a semantic companion hierarchy for Phoenix /
  OpenInference consumers;
- a request-pipeline module may emit both its standard runtime span and its
  OpenInference span in the same execution path;
- OpenInference spans must be nested within the same execution flow as the
  corresponding runtime work and must not describe synthetic work that did not
  occur;
- standard runtime spans and OpenInference spans must not diverge on success vs
  failure outcome for the same stage.

# 4) Root OpenInference Contract

Each orchestrator invocation that operates on a current iteration must create
exactly one iteration-scoped OpenInference root span.

The root OpenInference span name is:
- `oi.chain.diagnostic_iteration`

`oi.chain.diagnostic_iteration` rules:
- it is created by the orchestrator after the current `diagnostics.iteration`
  span becomes active;
- it is a child of the active `diagnostics.iteration` span rather than a second
  trace root;
- it is stored in `OpenInferenceContext.root_span`;
- it is the required parent for all request-pipeline OpenInference spans created
  during that iteration-scoped invocation;
- it closes after the iteration-scoped invocation outcome is known;
- it must remain present on both success and failure paths.

When there is no current iteration:
- the orchestrator does not create `oi.chain.diagnostic_iteration`;
- `Context::noop()` may carry `tracing::Span::none()` as a non-recording
  placeholder;
- business modules must treat that noop span as a passive context object rather
  than as a signal to invent synthetic OpenInference traces.

# 5) Attribute Naming Rules

OpenInference spans use the attribute names required by the current
implementation and must not rename them ad hoc.

Required shared attribute names across this document are:
- `openinference.span.kind`
- `input.value`
- `input.mime_type`
- `output.value`
- `output.mime_type`
- `status`
- `error.type`
- `error.message`

LLM-specific shared attributes are:
- `llm.model_name`
- `llm.provider`
- `llm.invocation_parameters`
- `llm.token_count.prompt`
- `llm.token_count.completion`
- `llm.token_count.total`

Additional query-structuring LLM attribute:
- `llm.raw_response`

Value rules:
- `status` uses the same fixed values as the standard runtime span contract:
  `ok | error`;
- `openinference.span.kind` values in the current contract are fixed:
  `CHAIN | LLM | RETRIEVER | GUARDRAIL`;
- JSON-valued payload attributes in `input.value`, `output.value`, and
  `llm.invocation_parameters` must be written as serialized JSON strings;
- `input.mime_type` and `output.mime_type` use `application/json` for every
  OpenInference span in the current contract.

# 6) Mandatory OpenInference Span Hierarchy

The current mandatory OpenInference hierarchy is:

- `oi.chain.diagnostic_iteration`
- `oi.llm.query_structuring`
- `oi.chain.query_structuring_metrics`
- `oi.retriever.candidate_cards`
- `oi.chain.candidate_card_retrieval_metrics`
- `oi.retriever.incident_evidence.primary`
- `oi.retriever.incident_evidence.alternatives`
- `oi.chain.incident_evidence_retrieval_metrics`
- `oi.retriever.theory_evidence`
- `oi.chain.theory_evidence_retrieval_metrics`
- `oi.chain.prompt_context_assembly`
- `oi.llm.diagnostic_response`
- `oi.guardrail.response_validation`

Hierarchy rules:
- every listed span is a descendant of `oi.chain.diagnostic_iteration`;
- every listed span is created only by the module that owns that stage;
- the OpenInference hierarchy is stage-local and does not introduce synthetic
  nesting between sibling pipeline stages;
- `oi.chain.query_structuring_metrics` is a child of `oi.llm.query_structuring`;
- `oi.chain.query_structuring_metrics` is created only when query-structuring
  metrics were actually computed for the current request;
- `oi.chain.candidate_card_retrieval_metrics` is a child of
  `oi.retriever.candidate_cards`;
- `oi.chain.candidate_card_retrieval_metrics` is created only when candidate
  card retrieval metrics were actually computed for the current request;
- `oi.retriever.incident_evidence.primary` and
  `oi.retriever.incident_evidence.alternatives` are siblings under
  `oi.chain.diagnostic_iteration`;
- `oi.chain.incident_evidence_retrieval_metrics` is a child of
  `oi.chain.diagnostic_iteration`;
- `oi.chain.incident_evidence_retrieval_metrics` is created only when incident
  evidence retrieval metrics were actually computed for the current request;
- `oi.chain.incident_evidence_retrieval_metrics` must be emitted only after
  both incident retrieval branches have completed successfully;
- `oi.chain.theory_evidence_retrieval_metrics` is a child of
  `oi.retriever.theory_evidence`;
- `oi.chain.theory_evidence_retrieval_metrics` is created only when theory
  evidence retrieval metrics were actually computed for the current request;
- `oi.chain.prompt_context_assembly`, `oi.llm.diagnostic_response`, and
  `oi.guardrail.response_validation` are siblings under
  `oi.chain.diagnostic_iteration`;
- no additional OpenInference root span may be introduced under the same
  iteration without updating this document.

# 7) Ownership

Ownership is fixed by module:

- `oi.chain.diagnostic_iteration` belongs to `orchestrator`;
- `oi.llm.query_structuring` belongs to `request_pipeline::query_structuring`;
- `oi.chain.query_structuring_metrics` belongs to
  `request_pipeline::query_structuring`;
- `oi.retriever.candidate_cards` belongs to
  `request_pipeline::candidate_card_retrieval`;
- `oi.chain.candidate_card_retrieval_metrics` belongs to
  `request_pipeline::candidate_card_retrieval`;
- `oi.retriever.incident_evidence.primary` and
  `oi.retriever.incident_evidence.alternatives` belong to
  `request_pipeline::incident_evidence_retrieval`;
- `oi.chain.incident_evidence_retrieval_metrics` belongs to
  `request_pipeline::incident_evidence_retrieval`;
- `oi.retriever.theory_evidence` belongs to
  `request_pipeline::theory_evidence_retrieval`;
- `oi.chain.theory_evidence_retrieval_metrics` belongs to
  `request_pipeline::theory_evidence_retrieval`;
- `oi.chain.prompt_context_assembly` belongs to
  `request_pipeline::prompt_context_assembly`;
- `oi.llm.diagnostic_response` belongs to
  `request_pipeline::llm_structured_generation`;
- `oi.guardrail.response_validation` belongs to
  `request_pipeline::response_validation_and_normalization`.

Ownership rules:
- a module creates only its own OpenInference spans;
- a module records only the OpenInference attributes owned by its span contract;
- cross-module OpenInference mutation is forbidden except for orchestrator-owned
  writes to `oi.chain.diagnostic_iteration`.

# 8) Required Common Attributes

Every OpenInference span listed in section `6)` contains:
- `openinference.span.kind`
- `input.value`
- `input.mime_type`
- `output.value`
- `output.mime_type`
- `status`

If an OpenInference span ends with an error, it also contains:
- `error.type`
- `error.message`

Common rules:
- `input.value` and `input.mime_type` must be recorded before the stage begins
  the owned business work represented by that span;
- `output.value` and `output.mime_type` must be recorded on successful stage
  completion when the stage has a defined output payload contract;
- `status = "ok"` must be recorded on successful completion;
- on failure, the span must record `status = "error"` via the shared
  `record_error(...)` helper or an equivalent implementation;
- implementations must not silently leave failed OpenInference spans without
  `status` and error fields.

# 9) Detailed Span Contracts

## 9.1 `oi.chain.diagnostic_iteration`

- `openinference.span.kind`
  - value: `CHAIN`
- required attributes:
  - `run.id`
  - `iteration.id`
  - `iteration.sequence_no`
  - `input.value`
  - `input.mime_type`
  - `output.value`
  - `output.mime_type`
  - `run.outcome`
  - `status`

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` must be a JSON object describing the iteration-local user input
  context for the current invocation;
- at minimum it must include the request-local problem statement carried into
  the current iteration;
- the current implementation may leave `input.value` empty until that payload is
  explicitly added, but future generated code must treat this attribute as part
  of the required contract.

Output payload contract:
- `output.mime_type = "application/json"` when output is recorded;
- on successful terminal iteration completion,
  `output.value` must be the serialized `RunResult` JSON returned by the
  invocation;
- on failure, `output.value` is not required.

Outcome rules:
- on success it records:
  - `run.outcome = "success"`
  - `status = "ok"`
- on failure it records:
  - `run.outcome = "failure"`
  - `status = "error"`
  - `error.type`
  - `error.message`

## 9.2 `oi.llm.query_structuring`

- `openinference.span.kind`
  - value: `LLM`

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` is a JSON object with:
  - `normalized_query`
  - `input_token_count`

Required LLM metadata:
- `llm.model_name`
- `llm.provider`
- `llm.invocation_parameters`
- `llm.token_count.prompt`
- `llm.token_count.completion`
- `llm.token_count.total`
- `llm.raw_response`

Current invocation-parameters contract:
- `llm.invocation_parameters` must serialize the effective query-structuring LLM
  call settings;
- the current implementation records:
  - `temperature`
  - `response_format`

Output payload contract:
- `output.mime_type = "application/json"`
- `output.value` is the serialized `StructuredUserQuery` JSON returned on
  successful parsing and validation.

Error rules:
- model-call failures, invalid finish reasons, parse failures, and invalid model
  output shape must record:
  - `status = "error"`
  - `error.type`
  - `error.message`
- invalid-model-output failures may still record `llm.raw_response` when raw
  content was available.

## 9.3 `oi.chain.query_structuring_metrics`

- `openinference.span.kind`
  - value: `CHAIN`

Parent rule:
- this span must be created as a child of `oi.llm.query_structuring`;
- it must not be attached directly to `oi.chain.diagnostic_iteration` when the
  query-structuring LLM span exists.

Creation rule:
- this span is created only when `QueryStructuringOutput.metrics = Some(...)`;
- when `QueryStructuringOutput.metrics = None`, this span and its events must
  not be emitted.

Required helper attributes:
- `qs.metrics.present`
  - value: `true`
- `qs.metrics.version`
  - value: `v1`

Payload role split:
- span attributes are the machine-friendly flat metric index used for search and
  filtering in Tempo / Phoenix;
- events under this span are the human-readable grouped metric payloads for
  inspection.

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` is a compact JSON object describing the metrics computation
  context;
- `input.value` must have exactly this JSON shape:

```json
{
  "golden_backed": true,
  "source": "structured_query"
}
```

- `golden_backed` is `true` when the metric bundle was computed from
  `context.golden_question = Some(...)`;
- `source` is the fixed string `structured_query` in the current contract.

Output payload contract:
- `output.mime_type = "application/json"`
- `output.value` is the full serialized `QueryStructuringMetrics` bundle.

Flat attribute naming contract:
- every metric computed inside `QueryStructuringMetrics` must also be written as
  a flat span attribute;
- the full metric surface currently contains 110 flat metric attributes:
  - 5 global aggregate attributes
  - 4 vocabulary-backed fields × 22 attributes each
  - 7 non-vocabulary attributes;
- generated Rust code must not attempt to predeclare all 110 flat metric
  attributes inside one `tracing::info_span!` or `tracing::span!` macro call;
- the factory function in `src/observability/mod.rs` for
  `oi.chain.query_structuring_metrics` must predeclare only the static fields
  needed at span creation time:
  - `openinference.span.kind`
  - `qs.metrics.present`
  - `qs.metrics.version`
  - `input.value`
  - `input.mime_type`
  - `output.value`
  - `output.mime_type`
  - `status`
  - `error.type`
  - `error.message`
- all flat metric attributes must then be attached after span creation through
  `tracing_opentelemetry::OpenTelemetrySpanExt::set_attribute(key, value)` or an
  equivalent OpenTelemetry attribute API;
- this post-creation attribute attachment is required because it avoids the
  compile-time field-count limit of the `tracing` span macros while still
  exporting the flat metric index to OpenTelemetry / Phoenix;
- the canonical prefixes are:
  - `qs.core.global.aggregate.<name>`
  - `qs.core.vocab.<field>.<category>.<name>`
  - `qs.diag.vocab.<field>.<category>.<name>`
  - `qs.diag.non_vocab.<field>.<category>.<name>`
- `field` for vocabulary-backed metrics is one of:
  - `symptoms`
  - `affected_subsystems`
  - `failure_modes`
  - `system_properties`
- `field` for non-vocabulary metrics is one of:
  - `entities`
  - `constraints`
  - `triggers`
  - `observability_signals`
  - `unresolved_terms`
  - `intent`
  - `scenario`
- non-vocabulary count metrics must end with:
  - `.count.value`
- non-vocabulary presence booleans must end with:
  - `.presence.present`

Required flat global attributes:
- `qs.core.global.aggregate.macro_precision_soft`
- `qs.core.global.aggregate.macro_recall_strict`
- `qs.core.global.aggregate.macro_recall_soft`
- `qs.core.global.aggregate.overall_grounded_strict_recall`
- `qs.core.global.aggregate.all_fields_core_success_rate`

Required flat per-field categories for each vocabulary-backed field:
- core:
  - `selection`
  - `grounding`
  - `success`
- diagnostic:
  - `contract`
  - `selection`
  - `graded`
  - `grounding`
  - `support`
  - `success`

Required flat core vocabulary metrics for each vocabulary-backed field:
- `qs.core.vocab.<field>.selection.precision_soft`
- `qs.core.vocab.<field>.selection.recall_strict`
- `qs.core.vocab.<field>.selection.recall_soft`
- `qs.core.vocab.<field>.grounding.grounded_strict_recall`
- `qs.core.vocab.<field>.success.field_core_success`
- `qs.core.vocab.<field>.success.field_grounded_success`

Required flat diagnostic vocabulary metrics for each vocabulary-backed field:
- `qs.diag.vocab.<field>.contract.invalid_vocab_count`
- `qs.diag.vocab.<field>.contract.duplicate_term_count`
- `qs.diag.vocab.<field>.selection.num_false_positive`
- `qs.diag.vocab.<field>.selection.num_false_negative_strict`
- `qs.diag.vocab.<field>.selection.num_predicted_terms`
- `qs.diag.vocab.<field>.graded.graded_coverage`
- `qs.diag.vocab.<field>.graded.average_selected_score`
- `qs.diag.vocab.<field>.graded.zero_score_selection_count`
- `qs.diag.vocab.<field>.grounding.unsupported_selected_term_rate`
- `qs.diag.vocab.<field>.grounding.missing_evidence_span_count`
- `qs.diag.vocab.<field>.grounding.invalid_evidence_span_count`
- `qs.diag.vocab.<field>.grounding.evidence_span_near_substring_rate`
- `qs.diag.vocab.<field>.support.weak_inference_rate`
- `qs.diag.vocab.<field>.support.strict_terms_weak_inference_rate`
- `qs.diag.vocab.<field>.support.weak_false_positive_rate`
- `qs.diag.vocab.<field>.success.empty_when_gold_exists`

Required flat non-vocabulary attributes:
- `qs.diag.non_vocab.entities.count.value`
- `qs.diag.non_vocab.constraints.count.value`
- `qs.diag.non_vocab.triggers.count.value`
- `qs.diag.non_vocab.observability_signals.count.value`
- `qs.diag.non_vocab.unresolved_terms.count.value`
- `qs.diag.non_vocab.intent.presence.present`
- `qs.diag.non_vocab.scenario.presence.present`

Required events under this span:
- `query_structuring_metrics.core`
- `query_structuring_metrics.vocab.symptoms`
- `query_structuring_metrics.vocab.affected_subsystems`
- `query_structuring_metrics.vocab.failure_modes`
- `query_structuring_metrics.vocab.system_properties`
- `query_structuring_metrics.non_vocab`

Event rules:
- event grouping must mirror the metric prefix grouping rather than inventing a
  second naming taxonomy;
- event payloads may use nested JSON objects for readability;
- event payloads must preserve the same metric leaf names as the flat span
  attributes;
- there is intentionally no separate `query_structuring_metrics.global` event in
  the current contract because global core metrics are included inside
  `query_structuring_metrics.core`.

`query_structuring_metrics.core` payload contract:
- the payload must contain:
  - `global`
  - `vocab`
- `global` must contain:
  - `aggregate`
- `global.aggregate` must include exactly the five flat global core metrics;
- `vocab` must contain one object per vocabulary-backed field;
- each field object under `vocab` must contain:
  - `selection`
  - `grounding`
  - `success`
- these nested objects must include exactly the flat core vocabulary metrics for
  that field.

`query_structuring_metrics.vocab.<field>` payload contract:
- one event is emitted for each of the four vocabulary-backed fields;
- the payload must contain:
  - `field`
  - `core`
  - `diag`
- `field` must equal the canonical field name of the event;
- `core` must contain:
  - `selection`
  - `grounding`
  - `success`
- `diag` must contain:
  - `contract`
  - `selection`
  - `graded`
  - `grounding`
  - `support`
  - `success`

`query_structuring_metrics.non_vocab` payload contract:
- the payload must contain:
  - `diag`
- `diag` must contain one nested object per non-vocabulary field;
- count fields must be represented as:
  - `{ "count": { "value": ... } }`
- presence booleans must be represented as:
  - `{ "presence": { "present": ... } }`

Status rules:
- on successful metric emission, the span records:
  - `status = "ok"`
- helper computation failure from `compute_query_structuring_metrics(...)` is a
  semantic query-structuring failure:
  - it must be propagated through `QueryStructuringError::MetricsComputation`
  - it must prevent successful request completion for that stage
  - in this case `oi.chain.query_structuring_metrics` is not emitted because no
    successful metric bundle exists to attach;
- observability recording failure after a valid `QueryStructuringMetrics` bundle
  already exists is non-fatal:
  - it must be treated as an observability-side failure
  - it must not mutate or discard the already computed
    `QueryStructuringOutput.metrics`
  - it must not change successful query-structuring stage completion into a
    semantic failure;
- on terminal metrics-span failure, the span must record:
  - `status = "error"`
  - `error.type`
  - `error.message`

## 9.4 `oi.retriever.candidate_cards`

- `openinference.span.kind`
  - value: `RETRIEVER`

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` is a JSON object with:
  - `normalized_query`
  - `top_k`
  - `score_threshold`
  - `max_alternatives`

Output payload contract:
- `output.mime_type = "application/json"`
- when at least one candidate exists, `output.value` is a JSON object:
  - `primary`
  - `alternatives`
- `primary` is either `null` or an object with:
  - `document.id`
  - `document.score`
  - `role`
- each entry in `alternatives` is an object with:
  - `document.id`
  - `document.score`
  - `role`
- when retrieval returns no hits, `output.value` must be exactly:
  - `{"primary":null,"alternatives":[]}`

Error rules:
- Qdrant / collection failures must record `status = "error"` with
  `error.type` and `error.message`.

## 9.4a `oi.chain.candidate_card_retrieval_metrics`

- `openinference.span.kind`
  - value: `CHAIN`

Parent and creation rules:
- this span must be created as a child of `oi.retriever.candidate_cards`;
- this span is created only when `CandidateCardRetrievalOutput.metrics = Some(...)`;
- when `CandidateCardRetrievalOutput.metrics = None`, this span and its events
  must not be emitted.

Required helper attributes:
- `rt.metrics.present`
  - value: `true`
- `rt.metrics.version`
  - value: `v1`

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` must have exactly this JSON shape:

```json
{
  "golden_backed": true,
  "source": "candidate_cards"
}
```

Output payload contract:
- `output.mime_type = "application/json"`
- `output.value` is the full serialized
  `CandidateCardRetrievalMetrics` bundle.

Flat attribute naming contract:
- every metric in `CandidateCardRetrievalMetrics.retrieval_relevant_cards`
  must also be written as a flat span attribute;
- the canonical flat attributes are:
  - `rt.candidate_cards.recall_soft`
  - `rt.candidate_cards.recall_strict`
  - `rt.candidate_cards.rr_soft`
  - `rt.candidate_cards.rr_strict`
  - `rt.candidate_cards.ndcg`
  - `rt.candidate_cards.evaluated_k`
  - `rt.candidate_cards.first_relevant_rank_soft`
  - `rt.candidate_cards.first_relevant_rank_strict`
  - `rt.candidate_cards.num_relevant_soft`
  - `rt.candidate_cards.num_relevant_strict`
- generated Rust code must predeclare only the static fields needed at span
  creation time and must attach the flat metric attributes after span creation
  through `tracing_opentelemetry::OpenTelemetrySpanExt::set_attribute(...)` or
  an equivalent OpenTelemetry attribute API.

Required events under this span:
- `retrieval_metrics.candidate_cards`

Event payload contract:
- the event payload is one JSON object containing exactly:
  - `recall_soft`
  - `recall_strict`
  - `rr_soft`
  - `rr_strict`
  - `ndcg`
  - `evaluated_k`
  - `first_relevant_rank_soft`
  - `first_relevant_rank_strict`
  - `num_relevant_soft`
  - `num_relevant_strict`

Status rules:
- on successful metric emission, the span records:
  - `status = "ok"`
- helper computation failure from `compute_retrieval_metrics(...)` is a
  semantic candidate-card retrieval failure:
  - it must be propagated through
    `CandidateCardRetrievalError::MetricsComputation`
  - it must prevent successful request completion for that stage;
- observability recording failure after a valid metrics bundle already exists is
  non-fatal and must not mutate the already computed
  `CandidateCardRetrievalOutput.metrics`.

## 9.5 `oi.retriever.incident_evidence.primary`

- `openinference.span.kind`
  - value: `RETRIEVER`

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` is a JSON object with:
  - `normalized_query`
  - `case_id`
  - `top_k`
  - `score_threshold`
  - `tags`

Output payload contract:
- `output.mime_type = "application/json"`
- on executed or skipped success, `output.value` is a JSON object with:
  - `documents`
- each `documents` entry contains:
  - `document.id`
  - `document.score`
  - `document.metadata`
- `document.metadata` currently contains:
  - `case_id`
- when the primary branch is skipped because there is no primary candidate, or
  when the search returns no hits, `output.value` must be exactly:
  - `{"documents":[]}`

## 9.6 `oi.retriever.incident_evidence.alternatives`

- `openinference.span.kind`
  - value: `RETRIEVER`

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` is a JSON object with:
  - `normalized_query`
  - `case_ids`
  - `top_k`
  - `score_threshold`
  - `tags`

Output payload contract:
- `output.mime_type = "application/json"`
- on executed or skipped success, `output.value` is a JSON object with:
  - `documents`
- each `documents` entry contains:
  - `document.id`
  - `document.score`
  - `document.metadata`
- `document.metadata` currently contains:
  - `case_id`
- when the alternatives branch is skipped because there are no alternative
  candidates, or when the search returns no hits, `output.value` must be
  exactly:
  - `{"documents":[]}`

## 9.6a `oi.chain.incident_evidence_retrieval_metrics`

- `openinference.span.kind`
  - value: `CHAIN`

Parent and creation rules:
- this span must be created as a child of `oi.chain.diagnostic_iteration`;
- this span is created only when `IncidentEvidenceRetrievalOutput.metrics = Some(...)`;
- when `IncidentEvidenceRetrievalOutput.metrics = None`, this span and its
  events must not be emitted;
- this span must only be emitted after both `incident_primary` and
  `incident_alternatives` metric bundles have been computed successfully.

Required helper attributes:
- `rt.metrics.present`
  - value: `true`
- `rt.metrics.version`
  - value: `v1`

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` must have exactly this JSON shape:

```json
{
  "golden_backed": true,
  "source": "incident_evidence"
}
```

Output payload contract:
- `output.mime_type = "application/json"`
- `output.value` is the full serialized
  `IncidentEvidenceRetrievalMetrics` bundle.

Flat attribute naming contract:
- every metric in
  `IncidentEvidenceRetrievalMetrics.primary_card_evidence_query.relevance_judgments`
  must also be written as a flat span attribute with the prefix
  `rt.incident_primary.`;
- every metric in
  `IncidentEvidenceRetrievalMetrics.alternative_cards_evidence_query.relevance_judgments`
  must also be written as a flat span attribute with the prefix
  `rt.incident_alternatives.`;
- the required flat attributes for each of the two prefixes are:
  - `recall_soft`
  - `recall_strict`
  - `rr_soft`
  - `rr_strict`
  - `ndcg`
  - `evaluated_k`
  - `first_relevant_rank_soft`
  - `first_relevant_rank_strict`
  - `num_relevant_soft`
  - `num_relevant_strict`
- generated Rust code must predeclare only the static fields needed at span
  creation time and must attach the flat metric attributes after span creation
  through `tracing_opentelemetry::OpenTelemetrySpanExt::set_attribute(...)` or
  an equivalent OpenTelemetry attribute API.

Required events under this span:
- `retrieval_metrics.incident_primary`
- `retrieval_metrics.incident_alternatives`

Event payload contract:
- each event payload is one JSON object containing exactly:
  - `recall_soft`
  - `recall_strict`
  - `rr_soft`
  - `rr_strict`
  - `ndcg`
  - `evaluated_k`
  - `first_relevant_rank_soft`
  - `first_relevant_rank_strict`
  - `num_relevant_soft`
  - `num_relevant_strict`

Status rules:
- on successful metric emission, the span records:
  - `status = "ok"`
- helper computation failure from `compute_retrieval_metrics(...)` is a
  semantic incident-evidence retrieval failure:
  - it must be propagated through
    `IncidentEvidenceRetrievalError::MetricsComputation`
  - it must prevent successful request completion for that stage;
- observability recording failure after a valid metrics bundle already exists is
  non-fatal and must not mutate the already computed
  `IncidentEvidenceRetrievalOutput.metrics`.

## 9.7 `oi.retriever.theory_evidence`

- `openinference.span.kind`
  - value: `RETRIEVER`

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` is a JSON object with:
  - `normalized_query`
  - `collection`
  - `top_k`
  - `score_threshold`

Output payload contract:
- `output.mime_type = "application/json"`
- `output.value` is a JSON object with:
  - `documents`
- each `documents` entry contains:
  - `document.id`
  - `document.score`
- unlike the incident-evidence spans, the current theory-evidence output does
  not include `document.metadata`.

## 9.7a `oi.chain.theory_evidence_retrieval_metrics`

- `openinference.span.kind`
  - value: `CHAIN`

Parent and creation rules:
- this span must be created as a child of `oi.retriever.theory_evidence`;
- this span is created only when `TheoryEvidenceRetrievalOutput.metrics = Some(...)`;
- when `TheoryEvidenceRetrievalOutput.metrics = None`, this span and its events
  must not be emitted.

Required helper attributes:
- `rt.metrics.present`
  - value: `true`
- `rt.metrics.version`
  - value: `v1`

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` must have exactly this JSON shape:

```json
{
  "golden_backed": true,
  "source": "theory_evidence"
}
```

Output payload contract:
- `output.mime_type = "application/json"`
- `output.value` is the full serialized
  `TheoryEvidenceRetrievalMetrics` bundle.

Flat attribute naming contract:
- every metric in `TheoryEvidenceRetrievalMetrics.mechanism_explanation`
  must also be written as a flat span attribute;
- the canonical flat attributes are:
  - `rt.theory_evidence.recall_soft`
  - `rt.theory_evidence.recall_strict`
  - `rt.theory_evidence.rr_soft`
  - `rt.theory_evidence.rr_strict`
  - `rt.theory_evidence.ndcg`
  - `rt.theory_evidence.evaluated_k`
  - `rt.theory_evidence.first_relevant_rank_soft`
  - `rt.theory_evidence.first_relevant_rank_strict`
  - `rt.theory_evidence.num_relevant_soft`
  - `rt.theory_evidence.num_relevant_strict`
- generated Rust code must predeclare only the static fields needed at span
  creation time and must attach the flat metric attributes after span creation
  through `tracing_opentelemetry::OpenTelemetrySpanExt::set_attribute(...)` or
  an equivalent OpenTelemetry attribute API.

Required events under this span:
- `retrieval_metrics.theory_evidence`

Event payload contract:
- the event payload is one JSON object containing exactly:
  - `recall_soft`
  - `recall_strict`
  - `rr_soft`
  - `rr_strict`
  - `ndcg`
  - `evaluated_k`
  - `first_relevant_rank_soft`
  - `first_relevant_rank_strict`
  - `num_relevant_soft`
  - `num_relevant_strict`

Status rules:
- on successful metric emission, the span records:
  - `status = "ok"`
- helper computation failure from `compute_retrieval_metrics(...)` is a
  semantic theory-evidence retrieval failure:
  - it must be propagated through
    `TheoryEvidenceRetrievalError::MetricsComputation`
  - it must prevent successful request completion for that stage;
- observability recording failure after a valid metrics bundle already exists is
  non-fatal and must not mutate the already computed
  `TheoryEvidenceRetrievalOutput.metrics`.

## 9.8 `oi.chain.prompt_context_assembly`

- `openinference.span.kind`
  - value: `CHAIN`

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` is a JSON object with:
  - `normalized_query`
  - `structured_query`
  - `primary_card_present`
  - `alternative_cards_count`
  - `primary_incident_chunks_count`
  - `alternative_incident_chunks_count`
  - `theory_chunks_count`

Output payload contract:
- `output.mime_type = "application/json"`
- `output.value` is a JSON object with:
  - `selected_counts`
  - `context_json_chars`
  - `prompt_chars`
- `selected_counts` currently contains:
  - `evidence_for_match`
  - `first_check_hint`
  - `supporting_explanation`
  - `alternative_context`
  - `mechanism_explanation`
  - `total`

Error rules:
- missing primary card, missing required evidence, inconsistent evidence, and
  prompt-asset serialization failures must record `status = "error"` with
  `error.type` and `error.message`.

## 9.9 `oi.llm.diagnostic_response`

- `openinference.span.kind`
  - value: `LLM`

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` is a JSON object with:
  - `prompt_chars`
  - `prompt_empty`

Required LLM metadata:
- `llm.model_name`
- `llm.provider`
- `llm.invocation_parameters`
- `llm.token_count.prompt`
- `llm.token_count.completion`
- `llm.token_count.total`

Current invocation-parameters contract:
- `llm.invocation_parameters` must serialize the effective response-generation
  call settings;
- the current implementation records:
  - `temperature`
  - `response_format`
  - `max_output_tokens`

Output payload contract:
- `output.mime_type = "application/json"`
- `output.value` is the parsed top-level JSON object returned by the model after
  successful JSON parsing and object-shape validation.

Error rules:
- empty prompt, model-call failures, invalid finish reasons, JSON parse failure,
  and non-object top-level responses must record `status = "error"` with
  `error.type` and `error.message`.

## 9.10 `oi.guardrail.response_validation`

- `openinference.span.kind`
  - value: `GUARDRAIL`

Input payload contract:
- `input.mime_type = "application/json"`
- `input.value` is the serialized JSON object emitted by the diagnostic-response
  LLM stage.

Output payload contract:
- `output.mime_type = "application/json"` when output is recorded;
- `output.value` is the serialized normalized `DiagnosticResponse` JSON returned
  by validation and normalization.

Error rules:
- invalid response shape and business-rule violations must record
  `status = "error"` with `error.type` and `error.message`;
- if normalization fails before a normalized response exists, `output.value` is
  not required.

# 10) Success And Error Recording Rules

Success rules:
- each successful OpenInference span records `status = "ok"`;
- successful spans with defined output contracts record `output.value` and
  `output.mime_type = "application/json"`;
- empty retrieval results are still successful OpenInference outcomes and must
  record the explicit empty JSON payload defined by the owning span contract.

Error rules:
- every failed OpenInference span must record:
  - `status = "error"`
  - `error.type`
  - `error.message`
- the current implementation uses `crate::observability::record_error(...)` as
  the common mechanism for this recording pattern;
- modules must not record `status = "ok"` after an error has become the terminal
  outcome of that stage.

# 11) Input / Output MIME Rules

For the current contract:
- every OpenInference span uses `application/json` for `input.mime_type`;
- every successful OpenInference span with output uses `application/json` for
  `output.mime_type`;
- this document does not authorize raw-text payload mime types for
  OpenInference, even when the underlying business stage temporarily handles raw
  model text internally.

# 12) Safety Constraints

OpenInference spans obey the same top-level observability safety constraints as
`observability.md`.

Current stage-specific rules:
- OpenInference payloads may contain structured JSON summaries derived from
  request-pipeline inputs and outputs;
- OpenInference payloads must not contain secrets, API keys, or authorization
  headers;
- the current contract allows the bounded `llm.raw_response` field on
  `oi.llm.query_structuring` as an explicit exception to the general raw-model-
  output prohibition, because that stage expects one JSON-object response that
  is later parsed into `StructuredUserQuery`;
- future tightening of prompt/document redaction rules must update both
  `observability.md` and this document together.

# 13) Source Of Truth Relationship

This document is the source of truth for:
- required OpenInference span names;
- required OpenInference span kinds;
- OpenInference hierarchy and ownership;
- per-span payload semantics.

Related source-of-truth boundaries:
- `Specification/runtime/observability/spans.md`
  - owns the standard runtime span hierarchy and ordinary orchestrator trace
    shape;
- `Specification/runtime/observability/observability.md`
  - owns the top-level observability model and document split;
- `Specification/runtime/orchestrator/orchestrator.md`
  - owns execution-time `Context` assembly and the rule that the orchestrator
    provides `OpenInferenceContext.root_span` to context-aware modules;
- `Specification/runtime/runtime.md`
  - owns the shared runtime type declarations for `OpenInferenceContext` and
    `Context`.
