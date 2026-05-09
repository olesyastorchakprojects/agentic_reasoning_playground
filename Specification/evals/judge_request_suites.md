## 1) Purpose

`judge_request_suites` is the generic suite-driven judge worker for the new
diagnostic eval engine.

This stage:

- selects the next eligible runtime-run / iteration subject inside the current
  eval-run scope;
- loads the corresponding `RunState`;
- derives one `DiagnosticEvalIterationSnapshot`;
- executes all required missing judge suites for that subject;
- writes factual judge-call usage rows;
- writes normalized judge verdict rows;
- marks the subject complete for this stage only after all required suite rows
  exist.

This stage does not:

- mutate runtime `RunState`;
- own eval-run bootstrap;
- build eval-run-level aggregates;
- write the final run report.

## 2) Stage Shape

For the current MVP, `judge_request_suites` is one generic iteration-level
judge stage.

Its semantics are intentionally unified because:

- query-structuring suites;
- evidence-pack suites;
- final-answer suites

all differ primarily in prompt and payload shape, not in scheduling or
persistence behavior.

This same generic stage must remain the execution boundary for continuation
iteration suites; continuation evaluation does not require a separate
storage-stage concept.

## 3) Public Boundary

The stage must expose one in-process callable boundary similar in style to the
previous eval engine workers.

The canonical function boundary should be equivalent in ownership to:

```python
run_judge_request_suites(params: JudgeRequestSuitesParams) -> bool
```

One invocation must process at most one eligible evaluation subject.

Return semantics:

- `True` means one subject was processed or attempted;
- `False` means no eligible work existed.

## 4) Required Parameters

The stage must receive explicit parameters including at least:

- `postgres_url`
- `eval_run_id`
- `judge_settings`

The stage must not:

- infer `eval_run_id` from ambient process state;
- read raw environment variables directly as its primary configuration source;
- infer its subject set from filesystem artifacts alone.

## 5) Scheduling Source

The eval engine must maintain eval-owned processing state sufficient to support:

- frozen batch membership;
- resumable progress;
- per-subject stage status;
- idempotent replay after interruption.

This processing-state mechanism must remain eval-owned even though `RunState`
remains the runtime source of truth.

The stage must schedule only subjects that:

- belong to the current eval-run scope;
- are not yet completed for `judge_request_suites`;
- are eligible under stable FIFO ordering.

## 6) Subject Identity

The canonical subject identity for this stage is:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`

All normalized judge result rows for this stage must be keyed by that identity
plus `suite_name`.

## 7) Input Construction

For one eligible subject, the stage must:

1. load the runtime `RunState` for `runtime_run_id`;
2. select the target iteration;
3. derive `DiagnosticEvalIterationSnapshot`;
4. inspect which required suites are already satisfied;
5. build prompt payloads only for missing suites.

When the selected iteration is a continuation iteration, input construction
must include the required prior-iteration context and continuation-specific
step outputs before any continuation suite payload is rendered.

The canonical prompt source of truth is:

- `Specification/evals/prompts.json`

Each suite must read from the catalog:

- `id`
- `version`
- `prompt_template`
- `input_variables`
- `applies_to`
- category metadata

The stage must not hardcode alternative prompt content that diverges from the
catalog.

## 8) Required MVP Suites

The current MVP stage must support request-level or iteration-level suites for:

- query structuring;
- evidence pack quality;
- final diagnostic answer quality.

The exact required suite set is defined by the suite catalog and eval
configuration.

The suite catalog must also support suite subsets that apply only to:

- initial iterations
- continuation iterations

The canonical suite-catalog field for this is:

- `applies_to = "shared" | "initial_only" | "continuation_only"`

Semantics:

- `shared`
  - run on both initial and continuation iterations
- `initial_only`
  - run only on initial iterations
- `continuation_only`
  - run only on continuation iterations

The first continuation suite slice should cover at least:

- observation extraction fidelity
- problem-understanding update quality
- hypothesis-update discipline
- next-check progression
- result-interpretation alignment

The stage must treat a suite as complete for a subject iff the corresponding
normalized result row already exists for:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `suite_name`

The worker must not require continuation-only suites for an initial iteration,
or initial-only suites for a continuation iteration, unless the suite catalog
explicitly marks that suite as applicable to both kinds.

A suite definition without an explicit `applies_to` classification is invalid.

## 9) Judge Call And Usage Persistence

For every factual judge call that returns a response, the stage must write one
usage row to `judge_llm_calls`.

That row must include at least:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `suite_name`
- judge provider and model metadata
- prompt version
- token counts
- cost fields
- token count source
- serialized raw response payload

This factual usage row must be written even if later normalization of the suite
response fails.

## 10) Normalized Judge Result Persistence

For every successful suite normalization, the stage must write one row to
`judge_results`.

That row must include at least:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`
- `suite_name`
- `category`
- `scope`
- `score`
- `normalized_result_json`
- `explanation`
- `failure_code`
- `raw_response`
- `judge_model`
- `judge_prompt_version`

The stage must not rewrite already-satisfied suite rows for the same subject.

## 11) Completion Rule

The stage is complete for one subject iff all required suite rows now exist for:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`

Completion must be checked against the exact required suite set, not merely
against the existence of some suite rows.

## 12) Resume And Idempotency

The stage must be resumable and logically idempotent.

Rules:

- subjects in `pending`, `running`, or `failed` state may be reconsidered;
- before issuing new judge calls, the stage must inspect existing
  `judge_results` rows;
- only missing suites may be re-executed;
- already-written usage rows and normalized rows must not be duplicated
  semantically for the same subject and suite.

Resume behavior must preserve:

- `eval_run_id`
- frozen runtime-run scope
- suite completion already achieved before interruption

## 13) Failure Boundary

If a suite call or suite normalization fails for the current subject:

- the stage must mark the current subject as failed for this stage;
- it must persist the stage-local error message into eval-owned processing
  state;
- it must not mark the subject completed;
- it must not silently skip the failed suite.

This failed state must remain resumable.

## 14) Observability

The stage must emit operator-visible traces and logs sufficient to show:

- eval-run id;
- runtime-run id;
- iteration id;
- suite currently executing;
- whether the stage is building payloads, waiting on the judge model, writing
  usage rows, writing result rows, or failing.

The stage must not emit full sensitive prompt payloads or full normalized
snapshot payloads into trace attributes.
