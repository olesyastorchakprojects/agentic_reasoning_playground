## 1) Purpose

This document defines the architecture of the offline eval engine for the
distributed diagnostics project.

The eval engine exists to evaluate completed runtime executions against golden
dataset expectations and judge-model rubrics, then produce:

- persisted judge verdict rows;
- persisted iteration-level and eval-run-level summary rows;
- a human-readable eval run report;
- dashboard-friendly aggregates;
- factual token and cost accounting for judge work.

The eval engine must treat the runtime system and the eval system as distinct
layers:

- runtime execution produces `RunState` records;
- eval execution reads completed runtime runs and produces eval artifacts about
  them.

This document is the top-level source of truth for:

- eval-run identity and scope;
- runtime-run membership in one eval-run;
- the relationship between `RunState`, iteration snapshots, judge suites,
  reports, and aggregates;
- MVP batch-eval behavior;
- forward compatibility with future multi-iteration agent loops.

## 2) Core Concepts

The eval engine uses these canonical concepts:

### 2.1) Runtime Run

A runtime run is one persisted diagnostic execution owned by the runtime
orchestrator and identified by `RunState.run_id`.

For the current project, one runtime run is the unit that stores:

- one or more iterations;
- step history for each iteration;
- runtime-produced typed outputs;
- runtime token usage produced by model-facing steps.

### 2.2) Iteration

An iteration is one item in `RunState.iterations`.

For MVP, most evaluated runtime runs are expected to contain exactly one
iteration.

The eval engine must nevertheless treat the iteration, not the whole run, as
the fundamental evaluation subject, because later agent-loop versions will add
multiple user turns to the same runtime run.

### 2.3) Eval Run

An eval run is one offline batch evaluation over a frozen set of runtime runs.

One eval run:

- has its own `eval_run_id`;
- owns one frozen set of runtime-run ids;
- may be resumed after failure;
- aggregates many runtime runs into one report and one set of summary rows.

Its frozen scope must preserve both:

- the member `runtime_run_id` values;
- the selected evaluated subjects
  `(runtime_run_id, iteration_id)`.

An eval run is not the same thing as a runtime run.

The eval engine must never conflate:

- `eval_run_id`
- `runtime_run_id`
- `iteration_id`

### 2.4) Eval Subject

The primary semantic subject of judge evaluation is:

- one runtime run;
- one concrete iteration inside that run;
- one suite-specific view over that iteration.

This subject must be materialized logically as
`DiagnosticEvalIterationSnapshot`.

## 3) MVP Operating Mode

The required MVP operating mode is offline batch evaluation over golden-dataset
runtime runs.

The intended workflow is:

1. a batch of golden questions is executed through the runtime;
2. each question creates one runtime run;
3. the eval engine discovers the eligible runtime runs that have not yet been
   absorbed into an eval run;
4. the eval engine freezes that set as one eval-run scope;
5. the eval engine freezes one target iteration subject per runtime run;
6. the eval engine evaluates every eligible subject in that frozen scope;
7. the eval engine writes judge rows, summary rows, and a run report for the
   eval run as a whole.

The current MVP does not require interactive per-request ad hoc eval as the
main mode.

Single-runtime-run evaluation may be added later as a separate operational
mode, but this must not distort the batch-oriented MVP architecture.

## 4) Source Of Truth

The runtime source of truth for eval inputs is `RunState`.

The eval engine must not define a parallel request-capture system as its
primary semantic source of truth for the diagnostic runtime.

Instead, the eval engine must:

- load one persisted `RunState`;
- select one target iteration;
- project that iteration into a logical eval snapshot;
- build suite-specific judge payloads from that snapshot.

The eval engine may persist its own:

- judge result rows;
- judge usage rows;
- iteration summary rows;
- eval-run summary rows;
- run manifest and report artifacts.

But these are eval-owned derived artifacts, not replacements for runtime state.

## 5) Engine Shape

The current eval engine must use one generic suite-driven judge stage, not
separate domain stages equivalent to the previous project's
`judge_generation` and `judge_retrieval`.

The canonical MVP stage order is:

1. `judge_request_suites`
2. `build_eval_summary`

Reasoning:

- the current judge suites are primarily request-level or iteration-level;
- their main difference is prompt and payload shape, not worker semantics;
- a single generic judge stage minimizes duplicated orchestration logic;
- the design remains extensible when new suites are added.

## 6) Batch Membership And Resume

Each eval run must own a frozen scope of runtime runs.

The eval engine must:

- discover eligible runtime runs at bootstrap;
- persist the resulting runtime-run id set into the eval-run manifest;
- persist the resulting evaluated subject scope into the eval-run manifest;
- treat that set as immutable for the life of the eval run;
- resume failed eval runs against the exact same frozen scope;
- avoid absorbing newly created runtime runs into an already-created eval run.

This frozen-scope rule is mandatory because:

- reports must remain reproducible;
- aggregates must remain stable under resume;
- dashboard comparisons between eval runs require clear membership boundaries.

## 7) Judge Suites

Judge suites are defined by a suite catalog in `Specification/evals/prompts.json`.

Each suite must define at least:

- stable suite id;
- stable suite name;
- version;
- category;
- scope;
- prompt template;
- declared input variables;
- expected normalized output shape.

For the current MVP, the judge suites are expected to cover:

- query structuring;
- evidence pack quality;
- final diagnostic answer quality.

## 8) Required Outputs

For each completed eval run, the engine must produce:

- normalized suite verdicts in eval storage;
- factual judge usage rows with token and cost accounting;
- iteration-level summaries;
- eval-run-level summaries;
- one `run_manifest.json`;
- one `run_report.md`.

The implementation order for product value is:

1. report correctness;
2. aggregate correctness;
3. dashboard consumption.

## 9) Token And Cost Accounting

Token and cost accounting is required in the first version of the new eval
engine.

The engine must preserve two distinct usage domains:

- runtime usage;
- judge usage.

The canonical formula is:

- `run_total = runtime_total + judge_total`

The canonical source of runtime usage is runtime-owned persisted outputs
derived from `RunState`.

The canonical source of judge usage is `judge_llm_calls`.

The engine must expose both raw and aggregated usage in:

- persisted summary rows;
- run reports;
- dashboard-facing queries.

## 10) Forward Compatibility With Agent Loops

The architecture must be prepared for future runtime runs with multiple user
iterations.

Therefore, all eval-owned identities and summaries must be designed so that:

- one runtime run may contain multiple iterations;
- one eval run may aggregate many runtime runs;
- judge verdicts are keyed by iteration, not only by runtime run;
- iteration-level summaries remain valid when later turns are added;
- future loop-level judge suites can be added without redefining eval-run
  identity.

The current MVP does not require multi-iteration judge suites, but the data
model must not assume `1 runtime run = 1 iteration` as a permanent invariant.
