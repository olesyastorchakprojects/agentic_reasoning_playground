## 1) Purpose

This document defines the eval-run manifest contract for the new diagnostics
eval engine.

The manifest is the canonical artifact that records:

- eval-run identity;
- frozen runtime-run membership;
- judge suite versions;
- judge runtime metadata;
- terminal run status;
- resume eligibility.

The manifest must be sufficient to:

- resume a failed eval run;
- reconstruct which runtime runs belong to that eval run;
- rebuild or inspect the corresponding report artifacts later;
- support auditability of eval configuration and scope.

## 2) Artifact Location

Each eval run must write artifacts under:

- `Evidence/evals/runs/<eval_run_started_at>_<eval_run_id>/`

That directory must contain at least:

- `run_manifest.json`
- `run_report.md`

The folder name is a filesystem-safe convenience only.

The canonical run identity remains `eval_run_id`.

## 3) Required Manifest Fields

The manifest must contain at least:

- `eval_run_id`
- `run_type`
- `status`
- `started_at`
- `stages`
- `judge_provider`
- `judge_base_url`
- `judge_model`
- `suite_versions`
- `runtime_run_count`
- `run_scope_runtime_run_ids`
- `subject_count`
- `run_scope_subjects`

Terminal manifests must also contain:

- `completed_at`

Failed terminal manifests must also contain:

- `last_error`

## 4) Semantic Field Rules

### 4.1) `eval_run_id`

- unique id for the eval run;
- generated once at bootstrap;
- reused during resume;
- must not be replaced on retry.

### 4.2) `run_type`

For the current MVP this identifies the batch-eval mode, such as:

- golden dataset experiment;
- offline validation batch;
- local dev eval batch.

### 4.3) `status`

Allowed statuses:

- `running`
- `completed`
- `failed`

The manifest must not be considered terminal while `status = running`.

### 4.4) `stages`

For the current MVP the manifest must record:

- `judge_request_suites`
- `build_eval_summary`

The ordered stage list is part of the run contract and documents what the
eval-run lifecycle consists of.

### 4.5) `suite_versions`

`suite_versions` must record the prompt version used for each suite name in the
current eval run.

This field exists so that later report comparisons can distinguish score shifts
caused by rubric/prompt changes from score shifts caused by runtime changes.

### 4.6) `runtime_run_count`

- must equal the count of ids in `run_scope_runtime_run_ids`;
- must remain stable for the life of the eval run.

### 4.7) `run_scope_runtime_run_ids`

This is the frozen runtime-run membership of the eval run.

Rules:

- the field must be written at bootstrap;
- it must be non-empty for a non-trivial eval run;
- it must not change during resume;
- newly created eligible runtime runs must not be appended to it later.

### 4.8) `subject_count`

- must equal the count of entries in `run_scope_subjects`;
- must remain stable for the life of the eval run.

### 4.9) `run_scope_subjects`

This is the frozen evaluated subject scope of the eval run.

Each entry must record at least:

- `runtime_run_id`
- `iteration_id`

Rules:

- the field must be written at bootstrap;
- it must be derived from the selected target iteration for each runtime run;
- it must not change during resume;
- resume must not recompute iteration selection from current runtime state.

## 5) Bootstrap Rule

At new eval-run bootstrap the orchestrator must:

1. select eligible runtime runs;
2. freeze them into one ordered scope;
3. freeze the target iteration subject for each runtime run into one ordered
   subject scope;
4. generate one `eval_run_id`;
5. write the initial manifest with `status = running`.

The bootstrap order of runtime runs must be stable and deterministic.

For MVP, that stable order should be defined by a persisted runtime-run
timestamp plus `runtime_run_id` as a tiebreaker.

## 6) Resume Rule

When resuming a failed eval run:

- the eval engine must locate the existing manifest for the requested
  `eval_run_id`;
- the manifest must be in `running` or `failed` state;
- the engine must reuse the exact same `run_scope_runtime_run_ids`;
- the engine must reuse the exact same `run_scope_subjects`;
- the engine must not generate a new `eval_run_id`;
- the engine must set manifest `status = running` again before re-entry;
- any previous `completed_at` must be cleared for resumed non-terminal state;
- any previous `last_error` may be cleared before resume attempt begins.

Resume must preserve identity and scope.

## 7) Completion Rule

An eval run may be marked `completed` only when:

- every runtime run in `run_scope_runtime_run_ids` has satisfied all required
  judge suites for the target iteration(s);
- iteration-level summaries have been built for all required subjects;
- eval-run-level summaries have been built successfully;
- the final run report has been written successfully.

The manifest must then record:

- `status = completed`
- `completed_at`

## 8) Failure Rule

If the eval engine hits a run-level failure boundary, the terminal manifest
must record:

- `status = failed`
- `completed_at`
- `last_error`

Stage-local failures that are resumable must not automatically imply that the
entire eval run is permanently unrecoverable.

But once the orchestrator exits through its terminal failure boundary, the
manifest must make that failure explicit.

## 9) Recommended MVP Shape

The following shape is recommended for the current MVP:

```json
{
  "eval_run_id": "string",
  "run_type": "string",
  "status": "running|completed|failed",
  "started_at": "RFC3339 timestamp",
  "completed_at": "RFC3339 timestamp",
  "stages": [
    "judge_request_suites",
    "build_eval_summary"
  ],
  "judge_provider": "string",
  "judge_base_url": "string",
  "judge_model": "string",
  "suite_versions": {
    "suite_name": "version"
  },
  "runtime_run_count": 10,
  "subject_count": 10,
  "run_scope_runtime_run_ids": [
    "runtime-run-id-1",
    "runtime-run-id-2"
  ],
  "run_scope_subjects": [
    {
      "runtime_run_id": "runtime-run-id-1",
      "iteration_id": "iteration-1"
    }
  ],
  "last_error": "string"
}
```
