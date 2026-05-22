# 1) Purpose / Scope

This document defines the span contract for the runtime orchestrator.

It defines:
- root span contract;
- orchestration span hierarchy;
- span ownership;
- span scope boundaries;
- required span attributes;
- OpenTelemetry display-name rules;
- root span implementation requirements.

# 2) Root Span Contract

Each run trace contains exactly one root span.

The root span name is:
- `diagnostics.run`

`diagnostics.run` rules:
- it is created at the beginning of orchestrator entrypoint handling;
- it is the ancestor of every other span in the run trace;
- it closes only after final success or terminal failure is known for that run invocation;
- it remains present in partial and failed traces;
- it must not be replaced by per-step spans, repository spans, or pipeline spans.

Entrypoint rules:
- one invocation of `Orchestrator::run(...)` creates one `diagnostics.run` span;
- one invocation of `Orchestrator::resume(...)` creates one `diagnostics.run` span;
- one invocation of `Orchestrator::resume_with_input(...)` creates one `diagnostics.run` span;
- the root span required attribute `run.entrypoint` records which of those three entrypoints created the span.

# 3) Attribute Naming Rules

Attribute names in this document are local to the span that owns them.

Rules:
- attribute names in this document do not repeat the span name prefix;
- shared attributes keep their full names:
  - `span.module`
  - `span.stage`
  - `status`
  - `error.type`
  - `error.message`
- all other attributes are interpreted in the scope of the span section that defines them.

Enum string rules:
- `step.kind` uses the canonical Rust enum variant name from `StepKind`;
- `transition.kind` uses the canonical Rust enum variant name from `PolicyTransition`;
- the required format is Rust variant casing, not snake_case and not kebab-case.

Error field rules:
- stable error classification uses `error.type`;
- human-readable failure detail uses `error.message`;
- full error text is allowed in `error.message` for this demo project when it does not violate the explicit safety constraints in this document;
- `error.kind` is not used in this contract.

# 4) Mandatory Span Hierarchy

Each traced orchestrator invocation contains the following mandatory hierarchy:

- `diagnostics.run`
- `diagnostics.iteration`
- `orchestrator.policy.next_transition`
- `orchestrator.step`
- `repository.step.append_pending`
- `step_executor.dispatch`
- `repository.step.finish`

Hierarchy rules:
- every listed span is a descendant of `diagnostics.run`;
- `diagnostics.iteration` is created only when the current invocation has a current iteration;
- in `resume(...)`, `diagnostics.iteration` is created for the loaded current iteration when one exists, even if that iteration was not created by the current invocation;
- `diagnostics.iteration` is not created when the current invocation has no current iteration to work with;
- `orchestrator.policy.next_transition` is a child of the active iteration span when an iteration exists;
- `orchestrator.step` is created only for `PolicyTransition::ExecuteStep`;
- `repository.step.append_pending`, `step_executor.dispatch`, and `repository.step.finish` are children of the owning `orchestrator.step` span;
- request-pipeline spans are nested under `step_executor.dispatch`;
- dependency spans are nested inside the stage or module that owns the dependency call;
- sibling spans are not used to represent parent-child execution;
- duplicate spans for the same action are forbidden.

# 5) Span Ownership

Span ownership is fixed by module:

- `diagnostics.run` and `diagnostics.iteration` belong to `orchestrator`;
- `orchestrator.policy.next_transition` belongs to `transition_policy`;
- `orchestrator.step` belongs to `orchestrator`;
- `repository.step.append_pending` and `repository.step.finish` belong to `run_repository`;
- `step_executor.dispatch` belongs to `step_executor`;
- `request_pipeline.*` spans belong to the owning request-pipeline module;
- dependency spans such as `llm.call` and `qdrant.search` belong to the module that performs that dependency call.

Ownership rules:
- a module creates only its own spans;
- a module does not create spans for another module;
- business modules do not create or close the root run span;
- repository code does not create orchestrator spans;
- cross-module telemetry side effects are forbidden.

# 6) Required Attributes For Mandatory Spans

Every mandatory span listed in sections `2)` and `4)` contains:
- `span.module`;
- `span.stage`;
- `status`.

If a mandatory span ends with an error, it also contains:
- `error.type`;
- `error.message`.

Allowed `status` values are fixed:
- `ok`;
- `error`.

Outcome field rules:
- `step.outcome` is used only on step-oriented spans that represent business-step execution outcome;
- `run.outcome` is used only on `diagnostics.run`;
- repository persistence spans describe persistence outcome through `status`, and may additionally describe persisted business outcome through `persisted.step.outcome`.

# 7) Detailed Span Contracts

`diagnostics.run`

- begins:
  - before orchestrator request handling begins for one invocation of `run`, `resume`, or `resume_with_input`
- ends:
  - after the invocation returns final success or terminal failure
- includes:
  - full orchestration work performed during that invocation
- required attributes:
  - `run.id`
    - type: string
    - source: orchestrator state
    - value: run UUID string
  - `run.entrypoint`
    - type: string
    - source: orchestration-derived
    - value: `run | resume | resume_with_input`
  - `span.module`
    - type: string
    - source: constant
    - value: `orchestrator`
  - `span.stage`
    - type: string
    - source: constant
    - value: `run`
  - `status`
    - type: string
    - source: result-derived
    - value: `ok | error`
- conditional attributes:
  - `run.outcome`
    - type: string
    - source: result-derived
    - value: `success | failure`
  - `terminal.transition`
    - type: string
    - source: orchestrator-derived
    - value format: canonical `PolicyTransition` variant name such as `FinishWithResult` or `FinishWithError`
    - emitted when the invocation ends in a terminal policy transition
  - `failed_step.kind`
    - type: string
    - source: orchestrator-derived
    - value format: canonical `StepKind` variant name
    - emitted only when the invocation fails because a step execution failed

`diagnostics.run` rules:
- `run.entrypoint` is required only on `diagnostics.run`;
- child spans do not duplicate `run.entrypoint`;
- `diagnostics.run` must still be emitted when the invocation ends in `PolicyTransition::FinishWithError`;
- `diagnostics.run` must still be emitted when step execution or persistence returns an error;
- the root span must remain the active parent while the orchestrator awaits downstream async work.
- on invocation failure, `diagnostics.run` must record `status = "error"` and `run.outcome = "failure"`;
- on invocation success, `diagnostics.run` must record `status = "ok"` and `run.outcome = "success"`.

`diagnostics.iteration`

- begins:
  - before work for the current iteration begins inside the current invocation
- ends:
  - after iteration-scoped orchestration work in that invocation completes
- includes:
  - work against the current iteration for that invocation, including a current iteration loaded by `resume(...)`
- required attributes:
  - `run.id`
    - type: string
    - source: orchestrator state
  - `iteration.id`
    - type: string
    - source: orchestrator state
  - `iteration.sequence_no`
    - type: integer
    - source: orchestrator state
  - `span.module`
    - type: string
    - source: constant
    - value: `orchestrator`
  - `span.stage`
    - type: string
    - source: constant
    - value: `iteration`
  - `status`
    - type: string
    - source: result-derived
    - value: `ok | error`

`diagnostics.iteration` rules:
- when step execution fails inside the current invocation, the active `diagnostics.iteration` span records `status = "error"`;
- when iteration-scoped work completes without step failure, the active `diagnostics.iteration` span records `status = "ok"`.

`orchestrator.policy.next_transition`

- begins:
  - immediately before one call to `TransitionPolicy::next_transition(...)`
- ends:
  - immediately after that call returns
- required attributes:
  - `run.id`
  - `iteration.id`
  - `span.module`
    - value: `transition_policy`
  - `span.stage`
    - value: `next_transition`
  - `status`
- conditional attributes:
  - `step.sequence_no`
    - type: integer
    - source: orchestrator-derived
    - emitted only when `transition.kind = "ExecuteStep"`
  - `transition.kind`
    - type: string
    - source: policy-derived
    - emitted when the policy call succeeds
    - value format: canonical `PolicyTransition` variant name such as `ExecuteStep`, `FinishWithResult`, or `FinishWithError`
  - `step.kind`
    - type: string
    - source: policy-derived
    - emitted only when `transition.kind = "ExecuteStep"`
    - value format: canonical `StepKind` variant name such as `InputNormalization`
  - `policy.finished_steps_count`
    - type: integer
    - source: policy-input-derived
  - `policy.pending_step_present`
    - type: boolean
    - source: policy-input-derived
  - `policy.last_finished_step.kind`
    - type: string
    - source: policy-input-derived
    - value format: canonical `StepKind` variant name
    - omitted when there is no previously finished step

`orchestrator.step`

- begins:
  - immediately before orchestrator bookkeeping for one `PolicyTransition::ExecuteStep`
- ends:
  - after pending-step persistence, step execution, state mutation, and finished-step persistence complete
- required attributes:
  - `run.id`
  - `iteration.id`
  - `step.kind`
    - type: string
    - source: orchestrator-derived
    - value format: canonical `StepKind` variant name such as `InputNormalization`
  - `span.module`
    - value: `orchestrator`
  - `span.stage`
    - value: `step`
  - `status`
- conditional attributes:
  - `step.sequence_no`
    - type: integer
    - source: orchestrator-derived
  - `record.id`
    - type: string
    - source: orchestrator-derived
    - emitted after the pending step record exists
  - `step.outcome`
    - type: string
    - source: result-derived
    - value: `success | failure`
  - `otel.name`
    - type: string
    - source: constant-derived from `step.kind`
    - value format: `step.<StepKind>`

`orchestrator.step` rules:
- the tracing span name remains `orchestrator.step`;
- for OpenTelemetry export, the implementation sets `otel.name = "step.<StepKind>"` to improve trace UI readability;
- when step execution fails, `orchestrator.step` records `status = "error"` and `step.outcome = "failure"`;
- when step execution succeeds, `orchestrator.step` records `status = "ok"` and `step.outcome = "success"`.

`repository.step.append_pending`

- begins:
  - immediately before persisting the pending step record
- ends:
  - immediately after the persistence call returns
- required attributes:
  - `run.id`
  - `iteration.id`
  - `step.kind`
  - `record.id`
  - `span.module`
    - value: `run_repository`
  - `span.stage`
    - value: `append_pending`
  - `status`
- conditional attributes:
  - `step.sequence_no`
    - type: integer
    - source: orchestrator-derived

`step_executor.dispatch`

- begins:
  - immediately before dispatching one step to `StepExecutor`
- ends:
  - immediately after `StepExecutor` returns
- required attributes:
  - `run.id`
  - `iteration.id`
  - `step.kind`
  - `span.module`
    - value: `step_executor`
  - `span.stage`
    - value: `dispatch`
  - `status`
- conditional attributes:
  - `step.sequence_no`
    - type: integer
    - source: orchestrator-derived
  - `step.outcome`
    - type: string
    - source: result-derived
    - value: `success | failure`
  - `otel.name`
    - type: string
    - source: constant-derived from `step.kind`
    - value format: `executor.<StepKind>`
    - recommended but optional

`step_executor.dispatch` rules:
- the tracing span name remains `step_executor.dispatch`;
- the implementation may set `otel.name = "executor.<StepKind>"` to improve trace UI readability;
- when step execution fails, `step_executor.dispatch` records `status = "error"` and `step.outcome = "failure"`;
- when step execution succeeds, `step_executor.dispatch` records `status = "ok"` and `step.outcome = "success"`.

`repository.step.finish`

- begins:
  - immediately before persisting the finished step record
- ends:
  - immediately after the persistence call returns
- required attributes:
  - `run.id`
  - `iteration.id`
  - `step.kind`
  - `record.id`
  - `span.module`
    - value: `run_repository`
  - `span.stage`
    - value: `finish`
  - `status`
- conditional attributes:
  - `step.sequence_no`
    - type: integer
    - source: orchestrator-derived
  - `persisted.step.outcome`
    - type: string
    - source: orchestrator-derived from the finished step record
    - value: `success | failure`

`repository.step.finish` rules:
- `repository.step.finish` describes repository operation success or failure through `status`;
- if it successfully persists a failed business-step result, it records `status = "ok"` and `persisted.step.outcome = "failure"`;
- repository persistence spans must not be marked as failed solely because the business step they persisted failed.

# 8) OpenTelemetry Display Name Rules

The tracing span names in this contract remain stable:
- `orchestrator.step`
- `step_executor.dispatch`

To improve trace UI readability, the implementation may set OpenTelemetry export names through `otel.name`.

Rules:
- `orchestrator.step` exports `otel.name = "step.<StepKind>"`;
- `step_executor.dispatch` may export `otel.name = "executor.<StepKind>"`;
- dynamic OpenTelemetry display names do not replace the contract-level tracing span names;
- `step.kind` remains required as an attribute even when it is also visible through `otel.name`.

# 9) Root Span Implementation Requirements

The root span lifecycle is fixed.

Required implementation pattern:
- create `diagnostics.run` exactly once inside each orchestrator entrypoint method;
- create it before any downstream async orchestration work starts;
- enter that span before awaiting the downstream future that performs orchestration work;
- keep that entered root span active for the full lifetime of the awaited orchestration future;
- record final success or error status on the root span before it closes;
- drop the entered root span only after the awaited orchestration future has resolved.

The implementation must not:
- create the root span inside `drive_to_outcome(...)`;
- create a fresh root span per iteration;
- create a fresh root span per step;
- enter the root span and then start downstream work outside that entered scope;
- rely on child functions to recreate parentage when the entrypoint failed to keep the root span active.

Async safety rules:
- the root span must be entered in the same orchestrator entrypoint scope that awaits downstream orchestration;
- the entered root span must remain active across `await`;
- child spans must be created while the root span is active, not retroactively attached afterward;
- ad hoc recreation of a root-like span in lower layers is forbidden.

Tracing primitive rules:
- spans are created with `tracing::span!`;
- the runtime uses `tracing_opentelemetry` only as the bridge from `tracing` spans to OpenTelemetry export;
- business code and the internal observability API must not create orchestration spans through OpenTelemetry SDK span constructors directly.

Parentage propagation rules:
- parent-child span relationships are established through the active `tracing` span scope;
- `run_repository`, `step_executor`, and request-pipeline modules create their own spans locally with `tracing::span!`;
- those child spans become descendants of the active parent span because the caller keeps the parent span entered while awaiting downstream work;
- the internal observability API must not require explicit OpenTelemetry span-context passing between orchestrator, repository, executor, and pipeline layers.

Failure propagation rules:
- when step execution fails, the executor span records the failure;
- the owning `orchestrator.step` span records the failure;
- the active `diagnostics.iteration` span records the failure;
- the root `diagnostics.run` span records the failure;
- repository persistence spans describe persistence success or failure, not business-step success or failure.

Event rules:
- do not add step lifecycle events such as `step.pending_opened`, `step.pending_persisted`, `step.execution_started`, `step.execution_finished`, or `step.finished_persisted`;
- the current contract uses spans, not duplicate lifecycle events, to represent those boundaries.

Rationale:
- root span parentage is fragile in async Rust when the root span is created in one place and the awaited work runs outside the entered scope;
- the contract in this section exists to prevent orphan child spans, split traces, and accidental per-step root traces.

# 10) Safety Constraints

The following values must not be written to spans:
- secrets;
- API keys;
- authorization headers;
- environment variable values;
- raw prompt text;
- raw retrieved document text;
- raw model output text.

# 11) Leaf Request-Pipeline Spans

This section defines the tracing contract for leaf request-pipeline modules in `distributed_diagnostics`.

The orchestrator layer remains the parent trace structure:

- `diagnostics.run`
- `diagnostics.iteration`
- `orchestrator.policy.next_transition`
- `orchestrator.step`
- `repository.step.append_pending`
- `step_executor.dispatch`
- `repository.step.finish`

Leaf-module spans are nested under `step_executor.dispatch`.

This section covers only:
- `request_pipeline.*` spans;
- leaf-module child dependency spans;
- leaf-module diagnostic events.

# 12) Leaf Goals

Leaf-module tracing in this repository is intentionally diagnosis-oriented for a demo project.

The leaf trace should help answer:
- what each module received;
- what policy or configuration it used;
- what external calls it made;
- what it selected, filtered, rejected, normalized, or validated;
- what exactly failed, with a clear human-readable message.

Visibility is preferred when it does not violate the explicit trace safety constraints in this document.

# 13) Leaf Data Visibility Policy

The following values may be written to leaf-module span attributes or events in this demo project:
- raw user query;
- normalized user query;
- serialized structured query JSON;
- serialized final structured output JSON;
- case IDs;
- chunk IDs;
- chunk tags;
- chunk scores;
- selected chunk roles;
- validation failure fields;
- full model or client error text when it does not violate the explicit safety constraints in this document;
- full human-readable diagnostic messages.

The following values must still not be written to leaf-module span attributes or events:
- large chunk text;
- full rendered prompt text;
- raw Qdrant transport payloads;
- vectors or embeddings;
- full PostgreSQL row dumps;
- very large arrays;
- very large raw model output.

Rules:
- full rendered prompt text is forbidden;
- large retrieved text is forbidden;
- raw model output is forbidden unless it is first normalized into a bounded structured representation;
- serialized structured query JSON is allowed because it is central to diagnosing pipeline behavior;
- serialized final structured output JSON and final trusted response JSON are allowed for this demo project;
- if serialized structured payloads become too large, they must be omitted, summarized, or truncated explicitly.

# 14) Global Leaf Attributes

Every `request_pipeline.<module>` span contains:
- `module.name`
- `module.outcome`
- `status`

Allowed `module.outcome` values are:
- `success`
- `failure`

On failure, a leaf-module span also contains:
- `error.type`
- `error.message`

`error.message` is required for every leaf-module error path.

The message must explain what happened in plain language.
Full error text is allowed and preferred for this demo project when it does not violate the explicit safety constraints in this document.

Examples:
- `Response validation failed: required field first_check is missing.`
- `Prompt context assembly failed: no chunk with role first_check_hint was available for the primary card.`
- `LLM structured generation failed: model returned invalid JSON object.`

Do not rely only on `error.type`.

Leaf identity rules:
- leaf-module spans inherit run, iteration, step, sequence, and record identity from their parent orchestration spans;
- leaf-module spans do not duplicate `run.id`, `iteration.id`, `step.kind`, `step.sequence_no`, or `record.id` unless a specific querying use case explicitly requires that duplication;
- the default leaf tracing contract keeps orchestration identity on the orchestrator and executor spans, not on every leaf span.

Leaf input-availability rule:
- a leaf module must not be required to emit an attribute that is not present in its natural runtime input;
- `query.raw` is required only for modules that actually receive the raw user query;
- downstream modules that operate on normalized requests record `query.normalized` and must not synthesize `query.raw`.

# 15) Leaf Event Policy

Leaf modules may emit diagnostic events more freely than orchestrator spans, but only when those events add information that is not already obvious from span attributes alone.

Event rules:
- leaf-module events must carry compact diagnostic facts, not large text blobs;
- every allowed event in this contract must declare its required payload fields explicitly;
- if an event does not have explicit required payload fields in this contract, that event must not be emitted;
- orchestrator lifecycle events remain forbidden;
- leaf-module events must not duplicate orchestrator lifecycle boundaries already represented by spans;
- events are for internal module decisions, not for replacing required leaf spans.

Good event use cases:
- branch skipped with a concrete reason;
- role-selection decisions;
- parse or validation failures with a compact decision payload.

Bad event use cases:
- generic `*_completed` events that only restate span status;
- generic `*_received` events that repeat existing span attributes;
- generic `*_checked` events without a compact decision payload.

# 16) Leaf Hierarchy And Ownership

Leaf-module span ownership is fixed by module:
- `request_pipeline.card_branch_reranking`
- `request_pipeline.information_adequacy_analysis`
- `request_pipeline.input_normalization`
- `request_pipeline.query_structuring`
- `request_pipeline.candidate_card_retrieval`
- `request_pipeline.card_hydration`
- `request_pipeline.incident_evidence_retrieval`
- `request_pipeline.theory_evidence_retrieval`
- `request_pipeline.prompt_context_assembly`
- `request_pipeline.llm_structured_generation`
- `request_pipeline.response_validation_and_normalization`
- `request_pipeline.observation_boundary_resolver`
- `request_pipeline.observation_extraction`
- `request_pipeline.diagnostic_update_prompt_context_assembly`

Leaf child dependency spans are created by the owning module, for example:
- `llm.call.query_structuring`
- `qdrant.cards.search`
- `postgres.incident_cards.get_by_case_ids`
- `qdrant.practice_chunks.search.primary`
- `qdrant.practice_chunks.search.alternatives`
- `qdrant.theory_chunks.search`
- `llm.call.diagnostic_response`
- `llm.call.observation_boundary_resolver`
- `llm.call.observation_extraction`

Rules:
- `request_pipeline.*` spans are children of `step_executor.dispatch`;
- dependency spans are children of the leaf module that performs the dependency call;
- a leaf module creates only its own leaf span and its own dependency spans;
- leaf-module child events belong to the leaf span that emitted them.

# 17) Leaf Span Contracts

`request_pipeline.input_normalization`

- child dependency spans:
  - none required
- required attributes:
  - global leaf attributes from section `14)`
  - `input.raw_query`
  - `input.raw_chars`
  - `input.normalized_query`
  - `input.normalized_chars`
  - `input.normalized_token_count`
  - `input.max_tokens`
  - `input.within_limit`
  - `normalization.trimmed`
  - `normalization.collapsed_whitespace`
  - `normalization.changed`
- failure rules:
  - stable error classification uses `error.type` values such as `InputNormalization.EmptyQuery`, `InputNormalization.InputTooLong`, or `InputNormalization.Tokenizer`

`request_pipeline.query_structuring`

- child dependency spans:
  - `llm.call.query_structuring`
- required attributes:
  - global leaf attributes from section `14)`
  - `query.normalized`
  - `query.input_token_count`
  - `asset.prompt.name`
  - `asset.prompt.version`
  - `asset.prompt.template_placeholders_valid`
  - `asset.vocabulary.name`
  - `asset.vocabulary.version`
  - `model.provider`
  - `model.name`
  - `model.response_mode`
  - `model.temperature`
  - `model.max_output_tokens`
  - `model.finish_reason`
  - `model.prompt_tokens`
  - `model.completion_tokens`
  - `model.total_tokens`
  - `structured.intent_present`
  - `structured.symptoms_count`
  - `structured.affected_subsystems_count`
  - `structured.failure_modes_count`
  - `structured.constraints_count`
  - `structured.confidence`
- required attributes on `llm.call.query_structuring`:
  - `llm.task = "query_structuring"`
  - `model.provider`
  - `model.name`
  - `model.response_mode`
  - `model.temperature`
  - `model.max_output_tokens`
  - `model.finish_reason`
  - `model.prompt_tokens`
  - `model.completion_tokens`
  - `model.total_tokens`
- allowed events:
  - `structured_query_payload`
  - `query_structuring.output_parsed`
- required fields on `structured_query_payload`:
  - `structured_query.json`
  - `event.name = "structured_query_payload"`
- required fields on `query_structuring.output_parsed`:
  - `structured.intent_present`
  - `structured.symptoms_count`
  - `structured.affected_subsystems_count`
  - `structured.failure_modes_count`
  - `structured.constraints_count`
- failure rules:
  - use `error.type` values such as `QueryStructuring.InvalidConfig`, `QueryStructuring.AssetRead`, `QueryStructuring.AssetParse`, `QueryStructuring.InvalidPromptAsset`, `QueryStructuring.InvalidControlledVocabulary`, `QueryStructuring.Model`, or `QueryStructuring.InvalidModelOutput`

`request_pipeline.candidate_card_retrieval`

- child dependency spans:
  - `qdrant.cards.search`
- required attributes:
  - global leaf attributes from section `14)`
  - `query.normalized`
  - `retrieval.collection`
  - `retrieval.top_k`
  - `retrieval.score_threshold`
  - `retrieval.max_alternatives`
  - `retrieval.request_limit`
  - `retrieval.hits_count`
  - `retrieval.selected_total_count`
  - `candidate.primary.present`
  - `candidate.primary.case_id`
  - `candidate.primary.score`
  - `candidate.alternatives.count`
  - `candidate.output.total_count`
- required attributes on `qdrant.cards.search`:
  - `qdrant.collection`
  - `qdrant.operation = "search"`
  - `retrieval.limit`
  - `retrieval.score_threshold`
  - `retrieval.hits_count`
  - `retrieval.scores`
- failure rules:
  - use `error.type` values such as `CandidateCardRetrieval.Collection`, `CandidateCardRetrieval.InvalidHit`, or `CandidateCardRetrieval.Mapping`

- allowed events:
  - `candidate_alternative_case_ids`
- required fields on `candidate_alternative_case_ids`:
  - `candidate.alternatives.case_ids`
  - `event.name = "candidate_alternative_case_ids"`

`request_pipeline.card_hydration`

- child dependency spans:
  - `postgres.incident_cards.get_by_case_ids`
- required attributes:
  - global leaf attributes from section `14)`
  - `hydration.input.primary_present`
  - `hydration.input.primary_case_id`
  - `hydration.input.alternatives_count`
  - `hydration.requested_case_ids_count`
  - `hydration.postgres_call_executed`
  - `hydration.cards_returned_count`
  - `hydration.primary_hydrated`
  - `hydration.alternatives_hydrated_count`
  - `hydration.order_reconstructed`
  - `hydration.partition_preserved`
- required attributes on `postgres.incident_cards.get_by_case_ids`:
  - `db.system = "postgresql"`
  - `db.operation = "get_incident_cards_by_case_ids"`
  - `db.requested_case_ids_count`
  - `db.returned_rows_count`
- failure rules:
  - use `error.type` values such as `CardHydration.MissingCard` or `CardHydration.Store`
- allowed events:
  - `hydration_input_alternative_case_ids`
  - `hydration_requested_case_ids`
  - `hydration_returned_case_ids`
  - `hydration_missing_case_ids`
- required fields on `hydration_input_alternative_case_ids`:
  - `hydration.input.alternative_case_ids`
  - `event.name = "hydration_input_alternative_case_ids"`
- required fields on `hydration_requested_case_ids`:
  - `hydration.requested_case_ids`
  - `event.name = "hydration_requested_case_ids"`
- required fields on `hydration_returned_case_ids`:
  - `hydration.returned_case_ids`
  - `event.name = "hydration_returned_case_ids"`
- required fields on `hydration_missing_case_ids`:
  - `hydration.missing_case_ids`
  - `event.name = "hydration_missing_case_ids"`

`request_pipeline.incident_evidence_retrieval`

- child dependency spans:
  - `qdrant.practice_chunks.search.primary`
  - `qdrant.practice_chunks.search.alternatives`
- required attributes:
  - global leaf attributes from section `14)`
  - `query.normalized`
  - `incident_evidence.primary_search.executed`
  - `incident_evidence.alternative_search.executed`
  - `incident_evidence.primary.case_id`
  - `incident_evidence.top_k`
  - `incident_evidence.score_threshold`
  - `incident_evidence.primary_chunks.count`
  - `incident_evidence.alternative_chunks.count`
  - `incident_evidence.total_chunks.count`
  - `incident_evidence.primary_tag_set`
  - `incident_evidence.alternative_tag_set`
- required attributes on child search spans:
  - `retrieval.branch`
  - `retrieval.collection = "practice_chunks"`
  - `retrieval.case_ids_count`
  - `retrieval.chunk_tags_filter.count`
  - `retrieval.chunk_tags_filter`
  - `retrieval.limit`
  - `retrieval.score_threshold`
  - `retrieval.hits_count`
  - `retrieval.hit_scores`
- allowed events:
  - `incident_evidence.primary_search_skipped`
  - `incident_evidence.alternative_search_skipped`
  - `incident_primary_hit_ids`
  - `incident_primary_chunk_ids`
  - `incident_alternative_hit_ids`
  - `incident_alternative_chunk_ids`
  - `incident_alternative_case_ids`
- required fields on `incident_primary_hit_ids`:
  - `retrieval.hit_chunk_ids`
  - `event.name = "incident_primary_hit_ids"`
- required fields on `incident_primary_chunk_ids`:
  - `incident_evidence.primary_chunks.ids`
  - `event.name = "incident_primary_chunk_ids"`
- required fields on `incident_alternative_hit_ids`:
  - `retrieval.hit_chunk_ids`
  - `event.name = "incident_alternative_hit_ids"`
- required fields on `incident_alternative_chunk_ids`:
  - `incident_evidence.alternative_chunks.ids`
  - `event.name = "incident_alternative_chunk_ids"`
- required fields on `incident_alternative_case_ids`:
  - `incident_evidence.alternative.case_ids`
  - `event.name = "incident_alternative_case_ids"`
- required fields on `incident_evidence.primary_search_skipped`:
  - `skip.reason`
  - `primary.case_id`
- required fields on `incident_evidence.alternative_search_skipped`:
  - `skip.reason`
  - `alternative.case_ids`
- failure rules:
  - use `error.type` values such as `IncidentEvidenceRetrieval.Collection`, `IncidentEvidenceRetrieval.InvalidHit`, or `IncidentEvidenceRetrieval.Mapping`

`request_pipeline.theory_evidence_retrieval`

- child dependency spans:
  - `qdrant.theory_chunks.search`
- required attributes:
  - global leaf attributes from section `14)`
  - `query.normalized`
  - `theory_retrieval.collection`
  - `theory_retrieval.top_k`
  - `theory_retrieval.score_threshold`
  - `theory_retrieval.search_executed`
  - `theory_retrieval.hits_count`
  - `theory_retrieval.scores`
  - `theory_retrieval.empty_result`
  - `theory_retrieval.order_preserved`
- required attributes on `qdrant.theory_chunks.search`:
  - `retrieval.collection = "theory_chunks"`
  - `retrieval.limit`
  - `retrieval.score_threshold`
  - `retrieval.hits_count`
  - `retrieval.hit_scores`
- allowed events:
  - `theory_retrieval_hit_ids`
  - `theory_retrieval_output_ids`
- required fields on `theory_retrieval_hit_ids`:
  - `retrieval.hit_chunk_ids`
  - `event.name = "theory_retrieval_hit_ids"`
- required fields on `theory_retrieval_output_ids`:
  - `theory_retrieval.chunk_ids`
  - `event.name = "theory_retrieval_output_ids"`
- failure rules:
  - use `error.type` values such as `TheoryEvidenceRetrieval.Collection`, `TheoryEvidenceRetrieval.InvalidHit`, or `TheoryEvidenceRetrieval.Mapping`

Notes for Qdrant-backed retrieval:
- record the full score list as the primary contract: `retrieval.scores`, `retrieval.hit_scores`, or module-local `*.scores`;
- `top_score` / `min_score` may be emitted as optional convenience attributes, but they are not part of the required contract and must not replace the full score list.

`request_pipeline.prompt_context_assembly`

- child dependency spans:
  - none required
- required attributes:
  - global leaf attributes from section `14)`
  - `query.normalized`
  - `prompt.asset.name`
  - `prompt.asset.version`
  - `prompt.asset.policy_constraints_count`
  - `prompt.input.primary_card_present`
  - `prompt.input.primary_card.case_id`
  - `prompt.input.alternative_cards_count`
  - `prompt.input.primary_incident_chunks_count`
  - `prompt.input.alternative_incident_chunks_count`
  - `prompt.input.theory_chunks_count`
  - `prompt.selected.total_chunks_count`
  - `prompt.selected.evidence_for_match.count`
  - `prompt.selected.first_check_hint.count`
  - `prompt.selected.supporting_explanation.count`
  - `prompt.selected.alternative_context.count`
  - `prompt.selected.mechanism_explanation.count`
  - `prompt.rendered_chars`
  - `prompt.context_json_chars`
- allowed events:
  - `prompt_structured_query_payload`
  - `prompt_input_alternative_card_case_ids`
  - `prompt_input_primary_incident_chunk_ids`
  - `prompt_input_alternative_incident_chunk_ids`
  - `prompt_input_theory_chunk_ids`
  - `prompt_context.role_selection_completed`
- required fields on `prompt_structured_query_payload`:
  - `structured_query.json`
  - `event.name = "prompt_structured_query_payload"`
- required fields on `prompt_input_alternative_card_case_ids`:
  - `prompt.input.alternative_card.case_ids`
  - `event.name = "prompt_input_alternative_card_case_ids"`
- required fields on `prompt_input_primary_incident_chunk_ids`:
  - `prompt.input.primary_incident_chunk_ids`
  - `event.name = "prompt_input_primary_incident_chunk_ids"`
- required fields on `prompt_input_alternative_incident_chunk_ids`:
  - `prompt.input.alternative_incident_chunk_ids`
  - `event.name = "prompt_input_alternative_incident_chunk_ids"`
- required fields on `prompt_input_theory_chunk_ids`:
  - `prompt.input.theory_chunk_ids`
  - `event.name = "prompt_input_theory_chunk_ids"`
- required fields on `prompt_context.role_selection_completed`:
  - `role.name`
  - `role.eligible_chunk_ids`
  - `role.selected_chunk_ids`
  - `role.selected_count`
- failure rules:
  - use `error.type` values such as `PromptContextAssembly.InvalidSettings`, `PromptContextAssembly.PromptAsset`, `PromptContextAssembly.MissingPrimaryCard`, `PromptContextAssembly.MissingRequiredEvidence`, or `PromptContextAssembly.InconsistentEvidence`

`request_pipeline.llm_structured_generation`

- child dependency spans:
  - `llm.call.diagnostic_response`
- required attributes:
  - global leaf attributes from section `14)`
  - `llm.task = "diagnostic_response"`
  - `llm.prompt_chars`
  - `llm.prompt_empty`
  - `llm.response_mode`
  - `llm.temperature`
  - `llm.max_output_tokens`
  - `llm.finish_reason`
  - `llm.prompt_tokens`
  - `llm.completion_tokens`
  - `llm.total_tokens`
  - `llm.output.parse_success`
  - `llm.output.top_level_type`
  - `llm.output.object_field_count`
  - `llm.output.has_markdown_fence`
  - `llm.output.content_chars`
- allowed conditional attributes:
  - `llm.output.truncated`
  - `llm.output.truncation_limit_chars`
- required attributes on `llm.call.diagnostic_response`:
  - `model.provider`
  - `model.name`
  - `model.response_mode`
  - `model.temperature`
  - `model.max_output_tokens`
  - `model.finish_reason`
  - `model.prompt_tokens`
  - `model.completion_tokens`
  - `model.total_tokens`
- allowed events:
  - `llm_output_payload`
  - `llm_generation.json_parsed`
- required fields on `llm_output_payload`:
  - `llm.output.parsed_json`
  - `event.name = "llm_output_payload"`
- required fields on `llm_generation.json_parsed`:
  - `llm.output.parse_success`
  - `llm.output.top_level_type`
  - `llm.output.object_field_count`
  - `llm.output.has_markdown_fence`
- failure rules:
  - use `error.type` values such as `LlmStructuredGeneration.InvalidConfig`, `LlmStructuredGeneration.InvalidInput`, `LlmStructuredGeneration.Model`, or `LlmStructuredGeneration.InvalidModelOutput`

`request_pipeline.response_validation_and_normalization`

- child dependency spans:
  - none required
- required attributes:
  - global leaf attributes from section `14)`
  - `validation.input.top_level_type`
  - `validation.input.top_level_field_count`
  - `validation.required_fields.present_count`
  - `validation.required_fields.missing_count`
  - `validation.required_fields.missing`
  - `validation.unknown_top_level_fields_count`
  - `validation.unknown_top_level_fields`
  - `validation.result_interpretation.present`
  - `validation.active_hypotheses.count`
  - `validation.active_hypotheses.valid_count_range`
  - `validation.competing_interpretation.present`
  - `validation.inconclusive_if.present`
  - `validation.prohibited_final_diagnosis_language_found`
  - `normalization.trimmed_fields_count`
  - `normalization.success`
- allowed events:
  - `validation_input_payload`
  - `final_response_payload`
  - `response_validation.failed`
- required fields on `validation_input_payload`:
  - `validation.input.raw_json`
  - `event.name = "validation_input_payload"`
- required fields on `final_response_payload`:
  - `final_response.json`
  - `event.name = "final_response_payload"`
- required fields on `response_validation.failed`:
  - `validation.failure.reason`
  - `validation.failure.path`
  - `error.message`
- failure rules:
  - use `error.type` values such as `ResponseValidation.InvalidResponseShape` or `ResponseValidation.BusinessRuleViolation`

`request_pipeline.card_branch_reranking`

- child dependency spans:
  - none
- required attributes:
  - global leaf attributes from section `14)`
  - `reranking.fresh_candidates_count`
  - `reranking.previous_primary_card_id`
  - `reranking.previous_primary_status`
    - value: `Tentative | Sticky`
  - `reranking.retention_window`
  - `reranking.new_primary_card_id`
  - `reranking.new_primary_status`
    - value: `Tentative | Sticky`
  - `reranking.new_primary_retained`
    - type: boolean; `true` if previous primary was preserved, `false` if replaced
  - `reranking.alternatives_count`
- conditional attributes:
  - `reranking.previous_primary_fresh_rank`
    - type: integer; 1-based rank of the previous primary in the fresh list
    - omitted when the previous primary is absent from the fresh list
- failure rules:
  - use `error.type` values such as `CardBranchReranking.EmptyCardSelectionHistory`,
    `CardBranchReranking.MissingFreshPrimary`,
    `CardBranchReranking.FreshPrimaryMismatch`, or
    `CardBranchReranking.DuplicateFreshCandidate`

`request_pipeline.information_adequacy_analysis`

- child dependency spans:
  - none
- required attributes:
  - global leaf attributes from section `14)`
  - `adequacy.mode`
    - value: `initial | supported_observation | unsupported_observation`
  - `adequacy.status`
    - value: `Blocking | WeakButRunnable | Sufficient`
  - `adequacy.missing_topics_count`
  - `adequacy.summary_reason`
- required attributes when `adequacy.mode = "initial"`:
  - `adequacy.input.symptom_signal_count`
  - `adequacy.input.diagnostic_anchor_count`
  - `adequacy.input.scope_count`
  - `adequacy.input.trigger_count`
  - `adequacy.input.failure_mode_count`
  - `adequacy.input.unresolved_count`
- required attributes when `adequacy.mode = "supported_observation"`:
  - `adequacy.input.observation_count`
  - `adequacy.input.needs_more_context`
  - `adequacy.input.confidence`
    - value: `Low | Medium | High`
- failure rules:
  - use `error.type` values such as
    `InformationAdequacyAnalyzer.InvalidObservationExtractionOutput`
  - note: `analyze_initial` and `analyze_supported_observation` have no error paths
    in the current implementation; only `analyze_unsupported_observation` can fail

`request_pipeline.observation_boundary_resolver`

- child dependency spans:
  - `llm.call.observation_boundary_resolver`
- required attributes:
  - global leaf attributes from section `14)`
  - `query.normalized`
  - `asset.prompt.version`
  - `model.response_mode`
  - `model.temperature`
  - `model.max_output_tokens`
  - `model.finish_reason`
  - `model.prompt_tokens`
  - `model.completion_tokens`
  - `model.total_tokens`
  - `resolution.supported`
  - `resolution.confidence`
- required attributes on `llm.call.observation_boundary_resolver`:
  - `llm.task = "observation_boundary_resolver"`
  - `model.finish_reason`
  - `model.prompt_tokens`
  - `model.completion_tokens`
  - `model.total_tokens`
- failure rules:
  - use `error.type` values such as `ObservationBoundaryResolver.InvalidContext`,
    `ObservationBoundaryResolver.ModelClient`, or
    `ObservationBoundaryResolver.InvalidModelOutput`

`request_pipeline.observation_extraction`

- child dependency spans:
  - `llm.call.observation_extraction`
- required attributes:
  - global leaf attributes from section `14)`
  - `query.normalized`
  - `asset.prompt.version`
  - `model.response_mode`
  - `model.temperature`
  - `model.max_output_tokens`
  - `model.finish_reason`
  - `model.prompt_tokens`
  - `model.completion_tokens`
  - `model.total_tokens`
  - `extraction.needs_more_context`
  - `extraction.observations_count`
- required attributes on `llm.call.observation_extraction`:
  - `llm.task = "observation_extraction"`
  - `model.finish_reason`
  - `model.prompt_tokens`
  - `model.completion_tokens`
  - `model.total_tokens`
- failure rules:
  - use `error.type` values such as
    `ObservationExtraction.UnsupportedBoundaryInput`,
    `ObservationExtraction.ModelClient`, or
    `ObservationExtraction.InvalidModelOutput`

`request_pipeline.diagnostic_update_prompt_context_assembly`

- child dependency spans:
  - none required
- required attributes:
  - global leaf attributes from section `14)`
  - `prompt.asset.name`
  - `prompt.asset.version`
  - `prompt.selected.total_chunks_count`
  - `prompt.rendered_chars`
- failure rules:
  - use `error.type` values such as
    `InvalidProblemUnderstanding`, `InvalidResolvedObservation`,
    `InvalidHypothesisState`, or `JsonSerializationFailed`

# 18) Leaf Acceptance Criteria

The leaf tracing contract is satisfied when:
1. each implemented leaf module emits a `request_pipeline.<module>` span under `step_executor.dispatch`;
2. each leaf span includes `module.outcome`, `status`, and clear error information on failure;
3. every leaf error path includes `error.message` with a human-readable explanation;
4. raw user query is visible only in leaf spans that naturally receive it, and normalized query is visible in downstream leaf spans that operate on normalized requests;
5. structured query JSON is visible after `QueryStructuring` and in downstream leaf spans where useful;
6. final structured output JSON or final trusted response JSON is visible only in the relevant downstream leaf spans;
7. full rendered prompt text is not written to the trace;
8. large chunk text is not written to the trace;
9. chunk IDs, case IDs, tags, scores, and selected roles are visible where relevant for retrieval and prompt assembly diagnosis;
10. prompt-context tracing shows eligible versus selected chunk decisions per role when that information is implemented;
11. LLM generation tracing shows finish reason, token usage, JSON parse result, and parsed structured output when available;
12. response validation tracing shows exact missing, unknown, or wrong fields together with a clear validation failure message.
