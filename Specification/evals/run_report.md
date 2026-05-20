## 1) Purpose

This document defines the markdown report contract for one eval run.

`run_report.md` is the primary MVP human-facing artifact for offline eval
inspection.

The report must help a human quickly answer:

- what batch was evaluated;
- how good the system was overall;
- where it failed most often;
- what the worst cases were;
- how much runtime and judge usage/cost the eval consumed.

## 2) Artifact Ownership

The report is a final eval-run artifact materialized by the orchestrator from
summary-owned aggregates and must be written under the eval-run artifact
directory beside `run_manifest.json`.

The report must be reproducible from persisted eval artifacts.

## 3) Required Sections

The current MVP report must include at least these sections, in order:

1. `# Eval Run Report`
2. `## Run Metadata`
3. `## Aggregated Metrics`
4. `## Suite Distributions` or equivalent score/distribution section
5. `## Gate Breakdown`
6. `## Failure Attribution`
7. `## Worst-Case Preview`
8. `## Token Usage`

When continuation iterations are present in the evaluated scope, the report
must also include:

9. `## Iteration Detail`
10. `## Initial vs Continuation Quality`

Optional sections may be added later, but these are the MVP minimum.

When runtime gold metrics are present, the report should distinguish between:

- graded quality signals;
- overreach / unsupported-selection diagnostics;
- strict contract pass checks.

## 4) Run Metadata Section

The run metadata section must include at least:

- `eval_run_id`
- `run_type`
- `status`
- `started_at`
- `completed_at`
- `runtime_run_count`
- `iterations_evaluated_count`
- judge model metadata
- suite version metadata

This section must make the batch identity and configuration auditable.

## 5) Aggregated Metrics Section

The aggregated metrics section must present the top-level eval quality metrics
for the whole eval run.

At minimum it must include:

- category-level aggregate scores;
- `usable_first_response_rate`;
- `usable_continuation_response_rate` when continuation suites are enabled;
- strict pass rates where available;
- hard-fail rate.

The exact metric set must match the aggregate spec.

## 6) Suite Distributions Section

Because the current suites use `score: 0|1|2`, the report must include a suite
distribution view that helps the operator see how many:

- full successes;
- borderline cases;
- hard failures

exist per suite.

The report may render this either as:

- score distribution tables;
- label distribution tables where the suite produces meaningful normalized
  classes;
- or both.

## 7) Gate Breakdown Section

The report must show which critical gates failed most often, including at
least:

- gate name;
- fail count;
- fail rate.

This section is required because gate failures are one of the most actionable
triage signals.

## 8) Failure Attribution Section

The report must expose the key failure-attribution aggregates, including at
least:

- `bad_final_due_to_query_rate`
- `bad_final_due_to_evidence_rate`
- `bad_final_with_good_query_and_evidence_rate`

This section should stay concise and engineering-oriented.

## 9) Worst-Case Preview Section

The report must include a compact preview of the worst cases in the eval run.

For the current MVP this should include a small number of runtime runs or
iteration subjects with the weakest key signals, such as:

- lowest final-answer quality;
- lowest evidence-pack sufficiency;
- strongest gate failures.

The section must include identifiers sufficient to trace the case back to raw
artifacts and stored rows.

## 9a) Iteration Detail Section

When continuation iterations are present, the report must include one compact
per-iteration section that shows at least:

- `runtime_run_id`
- `iteration_id`
- `iteration_kind`
- the enabled suite scores for that iteration
- the iteration-level usability signal appropriate to that kind

For continuation iterations, the report should also show the new observation
and the updated next check in compact form.

## 10) Token Usage Section

The report must include a runtime usage subsection and a judge usage
subsection.

The judge subsection must include:

- per-stage scope totals for judge usage;
- `judge_total`.

The report must explicitly render the formula:

- `Run total cost usd = runtime total cost usd + judge total cost usd`

and the resolved numeric equality for the current eval run.

This is a required MVP feature.

## 11) Formatting Goals

The report should:

- remain human-readable in plain markdown;
- favor compact tables and short bullet metadata;
- expose identifiers and key counts without overwhelming raw detail;
- support manual debugging before dashboards are fully mature.

The report should make it easy to distinguish:

- first-response quality problems;
- continuation-update quality problems;
- whole-trajectory degradation across later iterations.

When runtime gold metrics are shown for query structuring, the report should
also make it easy to distinguish:

- acceptable but non-canonical term selection;
- strict-target misses;
- real overreach such as zero-score selections or unsupported grounded terms.

When both graded query-structuring metrics and binary strict-contract checks
are shown, the report should not present the strict-contract checks as the
primary runtime quality verdict. They should be rendered as stricter
compliance diagnostics beside the graded quality and overreach views.
