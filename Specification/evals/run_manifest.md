## 1) Purpose

This document defines the current eval-run manifest contract for the diagnostics
eval engine.

The manifest records:

- eval-run identity;
- frozen runtime-run membership;
- frozen subject scope;
- judge runtime metadata;
- declared stage list;
- best-effort suite-version metadata.

## 2) Artifact Location

Each eval run writes artifacts under:

- `Evidence/evals/runs/<eval_run_started_at>_<eval_run_id>/`

That directory currently contains at least:

- `run_manifest.json`
- `run_report.md`

The canonical run identity is still `eval_run_id`, not the folder name.

## 3) Required Manifest Fields

The current manifest contains at least:

- `eval_run_id`
- `run_type`
- `status`
- `started_at`
- `completed_at`
- `stages`
- `judge_provider`
- `judge_base_url`
- `judge_model`
- `suite_versions`
- `runtime_run_count`
- `run_scope_runtime_run_ids`
- `subject_count`
- `run_scope_subjects`
- `last_error`

## 4) Current Field Semantics

### 4.1) `status`

Allowed values:

- `running`
- `completed`
- `failed`

Current implementation note:

- bootstrap writes `status = running`;
- full terminal status transitions are not yet comprehensively orchestrated in
  the current code path and remain an area for future tightening.

### 4.2) `stages`

The current ordered stage list is:

- `judge_request_suites`
- `build_eval_summary`

### 4.3) `suite_versions`

The current implementation writes a map keyed by enabled suite name.

Current value semantics:

- when suite enablement comes from config/CLI, the value is currently a marker
  string such as `enabled_from_config`.

This field is therefore presently best treated as enabled-suite metadata rather
than as a fully trustworthy persisted prompt-version map.

### 4.4) `run_scope_runtime_run_ids`

Current implementation semantics:

- runtime run ids are derived from the discovered frozen subjects;
- the list is sorted and deduplicated before writing;
- `runtime_run_count` equals the deduplicated length.

### 4.5) `run_scope_subjects`

Each subject entry records:

- `runtime_run_id`
- `iteration_id`

Current implementation allows multiple frozen subjects from the same runtime
run when multiple iterations satisfy eligibility.

## 5) Bootstrap Rule

At new eval-run bootstrap the system must:

1. discover frozen subjects;
2. derive the corresponding runtime-run membership from those subjects;
3. build the initial manifest with `status = running`;
4. write `run_manifest.json` into the eval-run artifact directory.

## 6) Lookup Rule

Current resume lookup behavior:

- artifact lookup scans directories under the artifact root;
- for each candidate directory, the engine reads `run_manifest.json`;
- the matching directory is the one whose manifest `eval_run_id` matches the
  requested id.

This lookup is artifact-based convenience, not a second source of truth for the
eval run itself.

## 7) Resume Rule

When resuming an existing eval run:

- the manifest is used to recover artifact identity and frozen-scope metadata;
- the same `eval_run_id` must be reused;
- no new subject discovery may be mixed into that run.

Current implementation note:

- the manifest is read during resume, but the authoritative per-subject workset
  is still the persisted `eval_processing_state` table.

## 8) Recommended Future Tightening

The manifest contract should eventually be strengthened so that:

- `suite_versions` stores actual prompt or catalog versions;
- terminal success/failure transitions are always written explicitly;
- resume reasserts manifest status and clears stale terminal fields when
  appropriate.

Those are desired future behaviors, but they are not yet the full current
implementation truth.
