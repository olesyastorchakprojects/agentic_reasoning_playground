## 1) Purpose

This document defines the mandatory generated unit-test contract for the new
diagnostics eval engine.

The eval engine implementation is incomplete if it omits the required unit
tests described here.

## 2) General Rules

Unit tests must:

- be independently runnable;
- avoid dependence on live external services;
- use fakes or mocks for judge transport boundaries;
- avoid dependence on live PostgreSQL instances unless a specific integration
  test layer is introduced later.

## 3) `eval_orchestrator`

Required tests must cover:

- bootstrap creates one new eval run with frozen runtime-run scope;
- bootstrap persists the frozen evaluated subject scope with explicit
  `iteration_id` values;
- bootstrap excludes runtime runs already absorbed into existing eval runs;
- resume reuses the same `eval_run_id`;
- resume reuses the same `run_scope_runtime_run_ids`;
- resume reuses the same `run_scope_subjects`;
- resume rejects manifests in invalid terminal states;
- completion is not declared while required stage outputs are incomplete;
- terminal success writes a completed manifest and final report artifact;
- terminal failure writes failed manifest with `last_error`.

## 4) `iteration_snapshot`

Required tests must cover:

- snapshot extraction succeeds when all required finished step outputs exist;
- snapshot extraction fails when required step output is missing;
- snapshot extraction ignores pending step records;
- `user_request.golden_question` is preserved into the snapshot;
- runtime token usage fields are projected correctly when present;
- optional future-history fields remain empty or absent in MVP cases.

## 5) `judge_request_suites`

Required tests must cover:

- one invocation processes at most one eligible subject;
- existing suite rows are treated as already satisfied;
- only missing suites trigger judge calls on resume;
- a factual `judge_llm_calls` row is written whenever a judge response exists;
- normalization failure preserves factual usage row and marks the subject
  failed;
- successful suite normalization writes one `judge_results` row with the
  correct identity key;
- stage completion requires the exact full required suite set.

## 6) `judge_llm_calls`

Required tests must cover:

- provider-native usage is preferred when valid;
- local fallback estimation is used when provider usage is absent and enough
  information exists;
- prompt, completion, and total tokens are computed correctly;
- prompt, completion, and total cost are computed correctly;
- `token_count_source` records the actual counting path used.

## 7) `judge_results`

Required tests must cover:

- normalized rows preserve `score`, `explanation`, and suite-specific JSON
  payloads;
- duplicate writes for the same unique key are prevented or treated as
  idempotent;
- `failure_code` is preserved when supplied;
- raw response payload is preserved.

## 8) `build_eval_summary`

Required tests must cover:

- iteration summaries are built only when all required suites exist;
- one successful invocation completes only the current subject;
- category scores are computed from the correct suites;
- gate booleans are computed from the correct suite thresholds;
- `usable_first_response` follows the documented formula;
- failure-attribution booleans and rates follow the documented formulas;
- eval-run summary rows aggregate iteration rows correctly;
- final report materialization occurs only after all frozen subjects reach
  subject-level summary completion;
- runtime usage, judge usage, and run totals use the documented formula;
- the summary stage does not silently skip missing required upstream results.

## 9) `run_report`

Required tests must cover:

- the report includes the required sections in the documented order;
- run metadata fields render correctly;
- aggregated metrics section renders required MVP aggregates;
- token usage section renders runtime, judge, and run-total values correctly;
- the report renders the explicit formula:
  `run_total = runtime_total + judge_total`;
- worst-case preview renders stable identifiers for the weakest cases.

## 10) `prompts.json`

Required tests must cover:

- all required MVP suite entries exist;
- every suite has id, version, category, scope, and prompt template;
- every suite marks `required_for_mvp` correctly;
- every suite's declared input variables are non-empty;
- duplicate suite ids or duplicate suite names are rejected.

## 11) Forward Compatibility Guardrails

Required tests must also ensure that the current MVP design does not hardcode
the assumption that one runtime run always contains exactly one iteration.

At minimum:

- subject identity includes `iteration_id`;
- summary keys include `iteration_id` where required;
- snapshot construction can target a selected iteration rather than only an
  implicit global run payload.
