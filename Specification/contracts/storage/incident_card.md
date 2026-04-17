## 1) Purpose / Scope

`IncidentCard` is the canonical structured storage object for one practical failure case.

This contract defines:
- the canonical field set for one stored incident card;
- the semantic intent of those fields;
- the invariants that must hold before the card is persisted or served back from canonical storage.

This contract does not define:
- PostgreSQL table layout;
- SQL column names;
- Qdrant retrieval representation;
- prompt assembly rules;
- runtime orchestration logic.

`IncidentCard` is the full-card source of truth used by canonical storage.

In the current architecture:
- PostgreSQL stores the canonical `IncidentCard`;
- Qdrant stores a retrieval-oriented representation derived from the card.

## 2) Canonical Shape

The canonical `IncidentCard` shape is:

```yaml
case_id:
title:
source_type:
source_name:
source_path:
vendor_or_project:
system_type:
version_tested:
report_date:

short_summary:

canonical_symptoms:
affected_components:
failure_mode_candidates:
observed_phases:

incident_phases:
  - phase_name:
    context:
    symptoms:
    user_visible_impact:
    observations:
    actions_taken:
    changes_after_actions:

turning_points:

candidate_explanations:
diagnostic_patterns:
discriminating_checks:
expected_observations:
investigation_steps:

root_cause_summary:
reasoning_summary:
mitigations_or_workarounds:
prevention_or_design_followups:

claimed_guarantees:
violated_properties:
resolution_status:
fix_versions:
confidence_notes:
source_refs:
```

## 3) Canonical Rust Shape

The generated or handwritten typed representation should be structurally equivalent to:

```rust
pub struct IncidentCard {
    pub case_id: String,
    pub title: String,
    pub source_type: String,
    pub source_name: String,
    pub source_path: String,
    pub vendor_or_project: Option<String>,
    pub system_type: Option<String>,
    pub version_tested: Option<String>,
    pub report_date: Option<chrono::NaiveDate>,
    pub short_summary: String,
    pub canonical_symptoms: Vec<String>,
    pub affected_components: Vec<String>,
    pub failure_mode_candidates: Vec<String>,
    pub observed_phases: Vec<String>,
    pub incident_phases: Vec<IncidentPhase>,
    pub turning_points: Vec<String>,
    pub candidate_explanations: Vec<String>,
    pub diagnostic_patterns: Vec<String>,
    pub discriminating_checks: Vec<DiscriminatingCheck>,
    pub expected_observations: Vec<ExpectedObservation>,
    pub investigation_steps: Vec<String>,
    pub root_cause_summary: Option<String>,
    pub reasoning_summary: Option<String>,
    pub mitigations_or_workarounds: Vec<String>,
    pub prevention_or_design_followups: Vec<String>,
    pub claimed_guarantees: Vec<String>,
    pub violated_properties: Vec<String>,
    pub resolution_status: Option<String>,
    pub fix_versions: Vec<String>,
    pub confidence_notes: Vec<String>,
    pub source_refs: Vec<String>,
}

pub struct IncidentPhase {
    pub phase_name: String,
    pub context: String,
    pub symptoms: Vec<String>,
    pub user_visible_impact: Vec<String>,
    pub observations: Vec<String>,
    pub actions_taken: Vec<String>,
    pub changes_after_actions: Vec<String>,
}

pub struct DiscriminatingCheck {
    pub question: String,
    pub why: String,
}

pub struct ExpectedObservation {
    pub observation: String,
    pub effect: String,
}
```

The exact Rust type location is implementation-specific.
This contract defines the required semantic shape only.

## 4) Field Intent

Core identity and provenance fields:
- `case_id` is the stable unique identity of the card.
- `title` is the human-readable card title.
- `source_type`, `source_name`, `source_path`, and `source_refs` preserve provenance.

System-description fields:
- `vendor_or_project`, `system_type`, `version_tested`, and `report_date` describe the tested system and source context.

First-pass matching fields:
- `canonical_symptoms`, `affected_components`, `failure_mode_candidates`, and `observed_phases` support future matching, filtering, and interpretation of noisy user reports.

Time-aware diagnostic fields:
- `incident_phases` preserves symptom dynamics over time;
- `turning_points` captures state transitions that materially changed interpretation;
- `changes_after_actions` records how observations changed after interventions.

Reasoning-support fields:
- `candidate_explanations`, `diagnostic_patterns`, `discriminating_checks`, `expected_observations`, and `investigation_steps` support the diagnostic loop and next-step guidance.

Explanation and remediation fields:
- `root_cause_summary` and `reasoning_summary` support evidence-backed explanation;
- `mitigations_or_workarounds` and `prevention_or_design_followups` support practical operator guidance.

Guarantee and resolution fields:
- `claimed_guarantees`, `violated_properties`, `resolution_status`, and `fix_versions` capture how the incident relates to system guarantees and remediation state.

Confidence fields:
- `confidence_notes` stores uncertainty and interpretation limits rather than hiding them.

## 5) Invariants

Required invariants:
- `case_id` must be non-empty after trimming;
- `title` must be non-empty after trimming;
- `source_type` must be non-empty after trimming;
- `source_name` must be non-empty after trimming;
- `source_path` must be non-empty after trimming;
- `short_summary` must be non-empty after trimming;
- `canonical_symptoms` must not be empty in the current version;
- `incident_phases` must not be empty in the current version;
- every `IncidentPhase.phase_name` must be non-empty after trimming;
- every `IncidentPhase.context` must be non-empty after trimming;
- every `DiscriminatingCheck.question` must be non-empty after trimming;
- every `DiscriminatingCheck.why` must be non-empty after trimming;
- every `ExpectedObservation.observation` must be non-empty after trimming;
- every `ExpectedObservation.effect` must be non-empty after trimming;
- `source_refs` must not be empty in the current version.

Consistency invariants:
- `observed_phases` should be semantically compatible with `incident_phases.phase_name`;
- `turning_points` should describe meaningful interpretation changes rather than duplicate plain symptoms;
- `reasoning_summary` must not contradict `root_cause_summary` when both are present;
- `fix_versions` may be empty when remediation is unknown or not applicable.

## 6) Storage Role

`IncidentCard` is the canonical storage object.

Rules:
- canonical storage must preserve the full structured card;
- retrieval-optimized card representations must be derived from this object, not treated as independent truth;
- storage implementations must not silently drop `incident_phases`, `discriminating_checks`, `expected_observations`, or other reasoning-critical fields;
- storage implementations must preserve list ordering where order carries reasoning meaning, especially for `incident_phases`, `turning_points`, `investigation_steps`, and `source_refs`.

## 7) Machine-Readable Schema

The machine-readable schema for this contract must live at:

- `Execution/schemas/incident_card.schema.json`

The machine-readable schema must stay semantically aligned with this contract.
