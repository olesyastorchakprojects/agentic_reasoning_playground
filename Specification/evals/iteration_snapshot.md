## 1) Purpose

This document defines the logical projection from runtime-owned `RunState` into
the eval-owned `DiagnosticEvalIterationSnapshot`.

The snapshot is the canonical input boundary for judge suites.

The eval engine must not send raw `RunState` directly to suites.

## 2) Snapshot Build Flow

The current snapshot flow is:

1. load one canonical `RunState`;
2. select an iteration by `LastCompletedIteration` or `ExactIteration`;
3. derive one iteration snapshot;
4. build suite-specific payloads from that snapshot.

This indirection exists to:

- keep judge payloads stable even if `RunState` grows;
- isolate eval logic from runtime internal storage layout;
- support multi-iteration runs cleanly.

## 3) Eval Subject

The snapshot subject is:

- one `runtime_run_id`;
- one `iteration_id`;
- one set of finished step outputs for that iteration.

The snapshot must also classify the selected iteration as:

- `initial`
- `continuation`

Current classification rule:

- `continuation` is detected by the presence of a finished
  `ObservationBoundaryResolver` step in the selected iteration;
- otherwise the iteration is `initial`.

## 4) Required Current Snapshot Fields

The snapshot currently exposes at least:

- `runtime_run_id`
- `iteration_id`
- `iteration_kind`
- `run_created_at`
- `run_updated_at`
- `user_request`
- `golden_question`
- `normalized_user_request`
- `query_structuring_output`
- `candidate_card_retrieval_output`
- `card_hydration_output`
- `incident_evidence_retrieval_output`
- `theory_evidence_retrieval_output`
- `prompt_context_assembly_output`
- `llm_structured_generation_output`
- `response_validation_and_normalization_output`
- `observation_boundary_resolver_output`
- `observation_extraction_output`
- `previous_snapshot`
- `config_snapshot`
- `runtime_token_usage`

## 5) Continuation Projection Rules

When `iteration_kind = continuation`, the snapshot must additionally:

- find the immediately preceding completed iteration in the same run;
- recursively project that prior iteration as `previous_snapshot`;
- require `ObservationBoundaryResolver`;
- require `ObservationExtraction`;
- require the continuation prompt-context assembly output.

Current implementation detail:

- the continuation snapshot reuses `query_structuring_output` from the previous
  completed snapshot instead of expecting a fresh query-structuring step in the
  current iteration.

## 6) Runtime Usage Projection

The snapshot currently projects runtime stage usage for:

- query structuring;
- observation boundary resolver;
- observation extraction;
- llm structured generation;
- total.

For continuation iterations, current usage semantics intentionally record zero
current-iteration query-structuring token usage.

## 7) Missing Data Rule

The snapshot builder must fail if a required step is missing, failed, or has an
unexpected payload type.

For continuation iterations, this failure rule also applies to the required
prior completed iteration context.
