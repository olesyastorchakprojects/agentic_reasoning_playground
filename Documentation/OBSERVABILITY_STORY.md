# Observability Story

The observability layer exists to answer a practical question: what exactly did the system do, where did time and cost go, and how can we inspect that behavior without relying on guesswork?

For this project, that question matters at two levels.

- The runtime must be inspectable as a multi-step diagnostic system with orchestration, retrieval, model calls, and validation.
- The evaluation layer must also be inspectable as its own execution flow, because measuring quality has its own latency, token usage, and cost profile.

Observability therefore is not only about collecting traces. It is the layer that turns runtime and evaluation execution into something we can search, compare, explain, and debug.

## Why Observability Matters

The distributed diagnostics assistant is not a single model call. A diagnostic run passes through multiple steps, and each step can fail or drift in different ways:

- query interpretation can be semantically wrong even when the final answer sounds reasonable;
- retrieval can miss or mis-rank evidence before the final model ever answers;
- a continuation iteration can mishandle a new observation even if the text still looks plausible;
- evaluation itself can become slow or expensive because of the number of judged subjects and suites.

Without observability, these failures flatten into vague impressions like “the answer felt off” or “the run seemed slow.” With observability, they can be localized to concrete steps, spans, dashboards, and artifacts.

## Two Layers: Runtime And Evaluation

The project now has two distinct observability stories.

### Runtime Observability

Runtime observability answers questions such as:

- which orchestration steps ran for this diagnostic request?
- how long did each step take?
- which retrieval or model spans dominated latency?
- how many tokens and how much cost accumulated on the AI-facing path?

The core runtime roots are `diagnostics.run` and `diagnostics.iteration`, with AI-facing spans under the OpenInference hierarchy such as:

- `oi.chain.*`
- `oi.llm.*`
- `oi.retriever.*`
- `oi.guardrail.*`

### Evaluation Observability

Evaluation observability answers different questions:

- which eval runs were executed in a time window?
- how much did evaluation itself cost?
- which runtime subjects were judged?
- which suite calls dominated eval latency or token usage?

The core eval trace hierarchy is:

- `eval.run`
- `eval.judge_request_suites.subject`
- `eval.judge_request_suites.suite`

This makes the evaluation layer inspectable as its own execution system rather than just as a final markdown report.

## Trace Backends And Their Roles

The current setup uses two trace-oriented surfaces with different jobs.

An important part of this setup is the OpenTelemetry Collector. The collector is what makes it possible to split the observability flow by backend instead of sending the exact same span stream everywhere.

In practice, that means:

- Tempo receives the broader operational trace stream for runtime execution;
- Phoenix receives the OpenInference-oriented span stream, which is the part of the trace tree that is most useful for AI-facing inspection.

This separation is what allows the project to keep full runtime orchestration visibility while still giving Phoenix a cleaner AI-centric view rather than a dump of every operational span.

### Tempo

Tempo is the operational trace surface for runtime execution. It is the best place to search for runtime traces and inspect the end-to-end step hierarchy of a `diagnostics.run`.

It is especially useful for:

- finding runs by time range;
- seeing the full orchestration tree;
- understanding wall-clock latency across runtime steps.

### Phoenix

Phoenix is the AI-facing trace surface. It is the best place to inspect:

- OpenInference chain / retriever / llm spans for runtime iterations;
- token usage and cost on AI-facing spans;
- evaluation traces rooted at `eval.run`.

Phoenix is particularly valuable because it keeps latency, token, and cost information visible on the same trace surface.

The OpenInference spans also carry the heavy AI-facing attributes that are actually useful for inspection:

- prompts;
- model inputs;
- model outputs;
- chain inputs and outputs.

At the same time, they intentionally do not carry the heaviest raw data payloads such as retrieved chunk bodies or other bulky artifacts that would make the trace stream noisy and expensive to work with. This keeps Phoenix useful for reasoning inspection without turning it into a storage dump of every large runtime object.

## Tracing A Runtime Run

The first useful runtime question is often simply: how do we find the run we care about?

In Tempo, runtime traces can be searched by service and time range. The root runtime trace is `diagnostics.run`.

![Tempo Runtime Trace Search](images/Screenshot%202026-05-13%20164328.png)

Once a runtime trace is selected, Tempo shows the full step hierarchy as a waterfall.

![Tempo Runtime Trace Waterfall](images/Screenshot%202026-05-13%20164405.png)

This view is the best operational surface for answering questions such as:

- which step dominated wall-clock latency?
- did the orchestration move through the expected step sequence?
- where did runtime time accumulate before the AI-facing spans are inspected in more detail?

## Tracing An Eval Run

Evaluation runs are also traced and should be inspected as first-class execution objects.

In Phoenix, the root eval trace is `eval.run`, with nested subject-level and suite-level spans.

![Phoenix Eval Run Trace](images/Screenshot%202026-05-13%20173014.png)

This view makes several things immediately visible:

- how many subjects were evaluated under one eval run;
- how many suite calls were executed for each subject;
- which suite calls dominated latency;
- how token usage and cost accumulated inside the eval itself.

This is especially useful when the question is not “what quality score did we get?” but “why did this eval run take this long or cost this much?”

## Dashboard Views

The trace views are not the only observability surfaces. The dashboards provide higher-level operational and comparison views around the same execution data.

### DSA Eval Usage Overview

`DSA Eval Usage Overview` is the aggregate operational view of evaluation activity over time.

![DSA Eval Usage Overview](images/Screenshot%202026-05-13%20171445.png)

This dashboard shows:

- the list of eval runs in the selected time window;
- the runtime, judge, and total token usage across the selected run set;
- the runtime, judge, and total cost across the selected run set.

This is the right surface when the question is:

- what eval activity happened this week?
- how much did the selected group of eval runs cost?
- how much of that cost came from runtime versus judge work?

### DSA Eval Runs Compare

`DSA Eval Runs Compare` is the side-by-side comparison view for two concrete eval runs.

![DSA Eval Runs Compare](images/Screenshot%202026-05-13%20171412.png)

This dashboard is useful when the question is:

- did a candidate improve usable first-response quality?
- did retrieval quality move?
- did quality improve at the cost of higher runtime or judge spend?

It surfaces deltas for:

- executive summary metrics;
- judge-based aggregates;
- failure attribution;
- total usage and cost.

## Cost And Token Visibility

One of the strongest parts of the current observability setup is that tokens and cost are visible at more than one level.

At the runtime level, AI-facing spans expose token and cost information on the chain / retriever / llm path.

At the evaluation level, cost is visible:

- in [`run_report.md`](../Evidence/evals/runs/2026-05-13T12-55-23.083192284+00-00_42a1f939-caea-4d1c-ba4e-fa62900d6cbe/run_report.md) for one eval run;
- in `DSA Eval Usage Overview` across a selected set of eval runs;
- in `DSA Eval Runs Compare` for two concrete runs;
- in Phoenix `eval.run` traces down to suite-level spans.

The eval report is also one of the clearest places to inspect the full per-run token and cost breakdown in durable form.

A recent full report example can be opened here: [run_report.md](../Evidence/evals/runs/2026-05-13T12-55-23.083192284+00-00_42a1f939-caea-4d1c-ba4e-fa62900d6cbe/run_report.md).

![Eval Report Token And Cost Breakdown](images/Screenshot%202026-05-13%20194351.png)

This view is useful when one concrete eval run needs:

- runtime-by-stage token usage;
- judge token usage;
- runtime, judge, and total cost;
- model-level prompt/completion cost breakdowns.

This means observability is not only about latency. It is also the layer that keeps the cost of runtime behavior and the cost of measuring quality visible in the same working environment.

## How To Read The System In Practice

The most useful reading order is:

1. open `DSA Eval Usage Overview` to see what evaluation activity happened in the selected time range and what the selected run set cost;
2. open `DSA Eval Runs Compare` when the question is whether one run improved or regressed relative to another;
3. open [`run_report.md`](../Evidence/evals/runs/2026-05-13T12-55-23.083192284+00-00_42a1f939-caea-4d1c-ba4e-fa62900d6cbe/run_report.md) when one eval run needs to be understood as a coherent quality slice with failure attribution, appendices, and cost breakdowns;
4. open Tempo when a concrete runtime run needs to be inspected as an end-to-end orchestration trace;
5. open Phoenix when the AI-facing path or the eval execution tree needs step-level token, cost, and latency inspection.

Used this way, observability is not an accessory to the system. It is the layer that makes both runtime behavior and evaluation behavior explainable enough to improve with confidence.
