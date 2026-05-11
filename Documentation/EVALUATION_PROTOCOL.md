# Evaluation Protocol

The evaluation layer exists to answer one practical question: what is the current quality level of the application, and where is that quality being lost?

For this project, quality is not only a property of the final response text. The assistant depends on structured query interpretation, precedent retrieval, evidence packing, and continuation updates over prior diagnostic state. A useful evaluation protocol therefore needs to produce a current quality slice of the whole application, not only of the last answer-generation step.

## What An Eval Run Measures

The runtime and the eval layer are separate systems.

The runtime produces persisted diagnostic runs in `RunState`. The eval layer reads completed runtime runs, selects the iterations that should be judged, and produces derived artifacts about their quality. `RunState` remains the source of truth for what the application actually did. The eval layer adds interpretation, scoring, aggregation, and reporting on top of that runtime history.

The main evaluation subject is one iteration inside one runtime run. That choice is deliberate. Even when many runs currently contain only one useful first-response iteration, the system itself is iteration-based, and continuation behavior is one of the central product behaviors. Evaluation therefore treats an iteration, not the whole run, as the basic unit of judgment.

## Initial And Continuation

The protocol distinguishes two iteration kinds:

- `initial`
- `continuation`

An initial iteration is judged as a first diagnostic move on a new problem report.

A continuation iteration is judged as a state update:

- prior diagnostic state;
- one new observation or check result;
- updated diagnostic response.

This distinction matters because continuation quality is not the same as first-response quality. A continuation can fail even when the final response still sounds reasonable, for example if it misreads the new observation, fails to update the problem framing, or repeats the previous next check without making progress. The evaluation protocol therefore keeps continuation-specific suites and continuation-specific summary signals rather than folding everything into one generic final-answer score.

## Eval Inputs

One eval run operates on a frozen set of completed runtime runs, usually from a golden-dataset batch.

Its main inputs are:

- persisted runtime runs and their iteration history;
- the selected target `(runtime_run_id, iteration_id)` subjects;
- golden expectations for those subjects;
- judge suite definitions and prompts;
- runtime-produced artifacts needed to judge the subject, such as structured query outputs, selected evidence, validated responses, and continuation-specific update artifacts.

The frozen-scope rule is important. An eval run should keep the same membership for its entire lifetime, including resume after failure. That makes the resulting report reproducible and keeps comparisons between eval runs meaningful.

## Evaluation Flow

At a high level, the current eval flow looks like this:

1. discover the eligible completed runtime runs;
2. freeze the eval-run scope and the target iteration subjects;
3. load each runtime run and project the selected iteration into an eval snapshot;
4. execute the required judge suites for that snapshot;
5. persist factual judge-call usage and normalized judge verdicts;
6. compute iteration summaries;
7. compute eval-run summaries and aggregates;
8. materialize the final eval report.

The evaluation protocol is therefore not just a batch of judge prompts. It is a pipeline that turns runtime history into a stable, interpretable quality slice of the application.

## Metric Layers

The current eval model uses more than one metric layer.

Judge-based quality metrics evaluate the semantic quality of the system's behavior: whether query structuring is disciplined, whether the evidence pack is useful, whether the final answer behaves correctly, and whether continuation updates are handled well.

Runtime gold metrics evaluate whether runtime outputs match the expected labels or expected evidence in the golden set. These metrics are especially useful for query structuring and retrieval because they expose whether the system selected the expected terms, cards, and evidence before any judge interprets the final response.

Runtime diagnostics provide lower-level supporting signals such as hit counts, retrieval traces, and configuration-sensitive behavior. These are not the main quality verdicts, but they help explain why a quality metric moved.

Together, these layers let the report answer not only “how good is the system?” but also “where in the chain did it break?”

## What Gets Judged

At the judge layer, the protocol is organized around a small number of categories rather than one undifferentiated score.

The main current categories are:

- query structuring;
- evidence pack quality;
- final answer quality;
- continuation update quality when continuation iterations are present.

Within those categories, suites are intentionally narrow. For example, one suite may check field-boundary discipline in structured queries, another may check whether the packed evidence is sufficient for a useful first move, and another may check whether the first diagnostic check actually discriminates between explanations. Continuation suites likewise focus on specific update behaviors such as hypothesis-update discipline, problem-understanding update quality, next-check progression, and observation-resolution quality.

This design keeps failure attribution visible. A weak application result should not collapse immediately into “the model did badly.” The protocol tries to show whether the weakness comes from upstream structuring, insufficient evidence support, final answer behavior, or continuation update logic.

The current suite set can be seen in the latest eval report: [run_report.md](../Evidence/evals/runs/2026-05-09T12-29-57.245141346+00-00_6889ebdc-2d44-4ef9-b01a-ce05194940e8/run_report.md).

| Suite | Applies To | What It Checks |
|---|---|---|
| `query_structuring_field_boundary_correctness` | initial only | Whether `symptoms`, `affected_subsystems`, `failure_modes`, and `system_properties` respect their intended meanings. |
| `query_structuring_grounding_conservatism` | initial only | Whether selected vocabulary terms are sufficiently supported by the user query and whether the model avoids weak over-inference. |
| `evidence_pack_role_fit` | initial only | Whether each selected chunk fits its assigned prompt role: `evidence_for_match`, `first_check_hint`, `supporting_explanation`, `alternative_context`, or `mechanism_explanation`. |
| `evidence_pack_sufficiency` | initial only | Whether the selected evidence pack is enough to support a useful first diagnostic move. |
| `final_no_root_cause_claim` | shared | Whether the answer avoids claiming or implying a final root cause. |
| `final_first_check_discriminates` | shared | Whether `first_check` is exactly one actionable check that distinguishes between active hypotheses or primary vs competing interpretation. |
| `final_hypothesis_source_alignment` | shared | Whether each hypothesis is supported by its declared source: `primary_incident`, `alternative_context`, or `theory_mechanism`. |
| `final_alternative_context_handling` | shared | Whether alternative context is used when genuinely useful and not forced when weak. |
| `final_result_interpretation_usefulness` | shared | Whether `supports_primary_if`, `supports_competing_if`, and `inconclusive_if` explain how to interpret the first check result. |
| `continuation_hypothesis_update_discipline` | continuation only | Whether a continuation response updates hypotheses and the surrounding diagnostic frame in a disciplined way after a new observation. |
| `continuation_problem_understanding_update` | continuation only | Whether `problem_understanding` is correctly updated to reflect the new observation without semantic inversion or loss of important state. |
| `continuation_next_check_progression` | continuation only | Whether the continuation response proposes a next check that genuinely advances diagnosis after the new observation. |
| `continuation_observation_resolution_context_recovery` | continuation only | Whether a short or referential continuation observation was reconstructed into a faithful and useful standalone observation using prior context. |

## Summary Signals

The protocol is designed to produce summary signals that are easy to interpret at both iteration level and eval-run level.

At iteration level, the system stores compact summary rows with:

- per-suite scores;
- category rollups;
- critical gate outcomes;
- usability signals;
- runtime usage;
- judge usage;
- combined token and cost totals.

At eval-run level, those iteration summaries are aggregated into a reportable quality slice of the application. The current aggregate set is intentionally engineering-oriented. It includes:

- category-level quality scores;
- strict and soft pass rates;
- usable first-response rate;
- usable continuation-response rate when continuation suites are enabled;
- gate-failure breakdowns;
- failure-attribution aggregates;
- runtime, judge, and total usage/cost.

These aggregates are meant to answer concrete questions such as:

- how often the system produces a usable first diagnostic response;
- whether the main failures come from query structuring, evidence support, or final response construction;
- whether continuation behavior is learning from new observations or merely rephrasing earlier output;
- what the current evaluation cost is.

## The Role Of Gates

The protocol uses a small set of critical gates because some failures matter more than others.

A system can have a respectable average score while still violating a product-critical rule, such as claiming a final root cause too early or proposing a non-discriminating next check. Gate-oriented metrics make those failures visible instead of letting them disappear inside an average.

That is why the report emphasizes both category scores and gate breakdowns. The category scores show the general quality level. The gates show where the product contract is being broken.

## Outputs

One completed eval run produces several outputs:

- normalized judge-result rows;
- factual judge-usage rows;
- iteration-level summary rows;
- eval-run-level summary rows and aggregates;
- a run manifest that records the frozen scope and run metadata;
- a human-readable `run_report.md`.

The report is the main human-facing artifact. It is meant to show:

- what batch was evaluated;
- which suites were active;
- the current quality slice of the application;
- the main failure concentrations;
- the worst-case examples worth inspecting next;
- the runtime and judge cost of the evaluation.

## How To Read The Current Quality Slice

The most useful reading order is:

1. look at the eval report's executive and aggregate sections to see the current overall quality level;
2. inspect category-level and suite-level results to see where quality is being lost;
3. inspect gate breakdowns and failure-attribution sections to understand whether the dominant problems are upstream, evidence-related, final-answer-related, or continuation-related;
4. drill into the weakest iteration subjects when concrete debugging is needed.

Used this way, the protocol is not just a reporting layer. It is the mechanism that turns runtime behavior into a stable and explainable current quality slice of the application, so that prompt, retrieval, and orchestration changes can be judged against the same frame.
