## 1) Purpose

This document defines the module contract for the eval crate's `report`
module.

This module is the markdown rendering boundary for `run_report.md`.

## 2) Responsibilities

The report module owns:

- building the final markdown report from already-materialized eval data;
- rendering required report sections in the documented order;
- formatting tables, counts, rates, and usage sections for human reading;
- rendering worst-case previews from prepared summary inputs.

## 3) Non-Responsibilities

The report module must not own:

- SQL fetching logic that belongs in repositories;
- raw aggregate formulas that belong in the summary module;
- judge transport;
- orchestration state transitions.

## 4) Public Types

The module should expose types conceptually equivalent to:

- `RunReportRenderer`
- `RunReportInput`
- `RunReportArtifact`
- `ReportModuleError`

It may also expose smaller section-rendering helpers if useful, but those
should remain internal by default.

## 5) Public Interfaces

The main entrypoint should be conceptually equivalent to:

```rust
fn render_run_report(input: &RunReportInput) -> Result<String, ReportModuleError>
```

The module may also expose:

- `write_run_report(path, rendered_markdown)`

if artifact persistence is kept close to rendering.

## 6) Inputs

The report module should consume already-assembled report-facing data such as:

- manifest metadata
- final `eval_run_summaries` row
- selected `eval_iteration_summaries` rows for worst-case preview
- optional suite metadata for section labels

This keeps report rendering cleanly downstream of summary materialization.

## 7) Outputs

The report module outputs:

- rendered markdown content for `run_report.md`

It may optionally return a typed artifact wrapper if the orchestrator wants a
more explicit write boundary.

## 8) Dependency Rules

The report module may depend on:

- `summary` output types
- `storage` row types where useful

It should not depend on:

- `judge`
- `snapshot`

This keeps the report module firmly at the downstream end of the eval crate.

## 9) Rendering Ownership

The report module is the only module that should own:

- markdown heading order
- section formatting
- user-facing wording in the report
- compact preview formatting for worst cases

Other modules may compute data, but they should not format markdown sections
inline.

