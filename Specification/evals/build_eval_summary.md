## 1) Purpose

`build_eval_summary` derives iteration-level summary rows and refreshes
eval-run-level aggregate artifacts from completed judge outputs and usage
ledgers.

This stage is the eval-owned summary and reporting boundary.

For each eligible subject it must:

- read completed `judge_results` rows;
- read completed `judge_llm_calls` rows;
- read runtime usage via `RunState`-derived projections;
- compute that subject's iteration-level summary;
- refresh rolling eval-run-level summaries;
- refresh a report view that is consistent with currently completed subjects.

This stage must not:

- generate new judge calls;
- mutate runtime `RunState`;
- rewrite already-correct judge verdicts;
- silently ignore missing required upstream suites.

## 2) Scope

For the current MVP, `build_eval_summary` is a subject-level stage in the
eval-owned pipeline.

Each invocation operates on exactly one
`(eval_run_id, runtime_run_id, iteration_id)` subject.

The stage may also refresh rolling eval-run aggregates, but its processing
state completion boundary is still the current subject.

## 3) Required Inputs

The stage must read:

- eval-run manifest;
- eval-owned processing state;
- `judge_results`;
- `judge_llm_calls`;
- runtime usage projected from the frozen set of runtime runs;
- suite catalog metadata when needed for completeness checks.

## 4) Readiness Rule

The summary stage may mark one subject completed only when that subject
satisfies all required judge suites.

If required suites are missing for the current subject:

- the stage must not mark the subject completed;
- the stage must fail or remain incomplete rather than silently building a
  misleading partial summary row.

## 5) Required Outputs

Across its subject-level executions, the stage must produce at least:

- `eval_iteration_summaries`
- `eval_run_summaries`

The final `run_report.md` remains the first-priority human-facing output for
MVP, but terminal report completion is finalized by the orchestrator after the
subject-level summary stage has drained successfully across the frozen scope.

Dashboards and dashboard-oriented tables may evolve later, but the summary
logic must already compute the underlying aggregates they need.

## 6) Iteration-Level Summary Responsibilities

For each evaluated iteration subject, the stage must derive:

- suite scores;
- category scores;
- gate outcomes;
- usable-first-response signal;
- per-iteration judge usage rollups;
- per-iteration runtime usage rollups;
- per-iteration total usage rollups.

## 7) Eval-Run-Level Summary Responsibilities

For the eval run as a whole, the stage must maintain or refresh:

- counts of runtime runs and evaluated iterations;
- suite-level aggregated scores and pass/fail rates;
- category-level aggregate scores;
- gate breakdowns;
- failure-attribution aggregates;
- runtime usage totals;
- judge usage totals;
- run-total usage and cost.

## 8) MVP Aggregate Set

The first version of the summary stage must support the agreed MVP aggregate
set from the judge-eval design, including at least:

- per-suite `avg_score`
- per-suite `pass_rate_strict`
- per-suite `fail_rate`
- `usable_first_response_rate`
- `gate_fail_breakdown`
- `bad_final_due_to_query_rate`
- `bad_final_due_to_evidence_rate`
- `bad_final_with_good_query_and_evidence_rate`

The exact formulas belong to the aggregate specification, but this stage owns
computing and persisting them.

## 9) Token Usage Section Ownership

This stage owns:

- loading runtime usage for the eval-run scope;
- loading judge usage for the current `eval_run_id`;
- deriving `judge_total`;
- deriving `run_total`;
- supplying the token/cost values used by the final report.

The explicit formula:

- `run_total = runtime_total + judge_total`

must be rendered in the report.

## 10) Run Report Structure

The current MVP report must remain intentionally human-readable and debugging
oriented.

The report should include at least:

- run metadata;
- suite version metadata;
- aggregated judge metrics;
- label/distribution style sections where applicable;
- failure-oriented previews;
- token and cost usage sections.

The old eval report is a useful reference shape, but the new report must be
centered on the new diagnostic judge suites and aggregates.

## 11) Completion Rule

The current subject may be marked `build_eval_summary/completed` only after:

- its iteration summary row has been persisted successfully;
- any rolling eval-run aggregates written by this stage are internally
  consistent;
- the stage has not detected missing required upstream suites for that subject.

The eval run as a whole may be marked `completed` only later, by the
orchestrator, after:

- every subject has reached `build_eval_summary/completed`;
- final eval-run summary materialization has succeeded;
- the final `run_report.md` has been written successfully;
- terminal manifest update has succeeded.

## 12) Failure Rule

If report construction or summary persistence fails:

- the current subject must not be marked completed;
- the eval-run manifest must capture terminal failure when the orchestrator
  exits through its failure boundary;
- the already-written judge verdicts and usage rows must remain preserved for
  resume or debugging.
