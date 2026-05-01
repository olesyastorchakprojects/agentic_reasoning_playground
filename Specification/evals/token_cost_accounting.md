## 1) Purpose

This document defines token and cost accounting semantics for the diagnostics
eval engine.

The goal is to preserve a clean separation between:

- runtime model usage;
- judge model usage;
- derived totals shown in reports and dashboards.

Token and cost accounting is required for the first version of the new eval
engine.

## 2) Usage Domains

The engine must preserve these usage domains:

1. `runtime`
2. `judge_request_suites`
3. `judge_total`
4. `run_total`

The names above are the MVP reporting scopes.

Later versions may add more judge-stage scopes, but the MVP must not reuse the
previous project's `judge_generation` / `judge_retrieval` scope names.

## 3) Canonical Sources

### 3.1) Runtime Usage

The canonical source of runtime usage is runtime-owned persisted output
information derived from `RunState`.

The runtime usage loader must project from runtime step outputs the fields:

- prompt tokens
- completion tokens
- total tokens
- prompt cost usd
- completion cost usd
- total cost usd

For the current project, some runtime steps already expose token usage through
`ModelTokenUsage`-style fields, while cost may still be added incrementally.

The eval spec must nevertheless reserve cost fields now.

### 3.2) Judge Usage

The canonical source of judge usage is `judge_llm_calls`.

Judge rollups must be derived only from factual judge call rows.

## 4) Required Formulas

For every rollup level:

- `total_tokens = prompt_tokens + completion_tokens`
- `total_cost_usd = prompt_cost_usd + completion_cost_usd`

For eval-run totals:

- `judge_total = sum(all judge stage scopes in the eval run)`
- `run_total = runtime_total + judge_total`

The formula:

- `run_total = runtime_total + judge_total`

must appear explicitly in the run-report contract.

## 5) Required MVP Report Outputs

The run report must expose:

- runtime usage section;
- judge usage section;
- explicit run total formula;
- final run total cost value.

The old report pattern is a useful reference, but the new scope names and
stage names must match the new engine.

## 6) Required MVP Aggregate Outputs

The summary layer must expose enough material to support:

- aggregated token usage per eval run;
- aggregated cost per eval run;
- dashboard tables for usage totals by scope;
- comparison of eval runs by total usage and total cost.

## 7) Precision And Formatting

Internal persisted cost fields must preserve precise numeric values suitable for
aggregation.

Human-readable reports may apply formatting rules such as:

- thousands separators for token counts;
- fixed decimal rendering for USD values.

Formatting must not replace persisted numeric precision.

## 8) Ownership Boundaries

The judge worker owns:

- computing factual judge usage rows.

The runtime owns:

- computing runtime token usage in runtime outputs.

The summary/report layer owns:

- loading both usage domains;
- computing rollups;
- rendering human-facing scope sections;
- computing `run_total`.
