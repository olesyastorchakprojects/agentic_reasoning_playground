## 1) Purpose / Scope

This document defines the canonical storage contract for persisted incident cards.

It specifies:
- the PostgreSQL namespace used by incident-card storage;
- the canonical storage target table;
- how `IncidentCard` maps into storage fields;
- duplicate-handling rules;
- storage-level invariants that must hold independently of runtime code.

This document does not define:
- runtime store-module public interface;
- SQL migration file layout;
- Qdrant retrieval representation;
- orchestration behavior.

## 2) Canonical Storage Target

The canonical storage target for incident cards is:

- schema: `diagnostics`
- table: `diagnostics.incident_cards`

Namespace rule:
- incident-card tables must not be created in `public`;
- the `diagnostics` schema is the required namespace for the current version;
- future diagnostic-loop tables that belong to this project should use the same `diagnostics` schema unless a stronger isolation reason appears later.

## 3) Canonical Storage Shape

The canonical storage contract for `diagnostics.incident_cards` is:

```text
diagnostics.incident_cards
  case_id text primary key
  title text not null
  source_type text not null
  source_name text not null
  source_path text not null
  vendor_or_project text null
  system_type text null
  version_tested text null
  report_date date null
  short_summary text not null
  canonical_symptoms jsonb not null
  affected_components jsonb not null
  failure_mode_candidates jsonb not null
  observed_phases jsonb not null
  incident_phases jsonb not null
  turning_points jsonb not null
  candidate_explanations jsonb not null
  diagnostic_patterns jsonb not null
  discriminating_checks jsonb not null
  expected_observations jsonb not null
  investigation_steps jsonb not null
  root_cause_summary text null
  reasoning_summary text null
  mitigations_or_workarounds jsonb not null
  prevention_or_design_followups jsonb not null
  claimed_guarantees jsonb not null
  violated_properties jsonb not null
  resolution_status text null
  fix_versions jsonb not null
  confidence_notes jsonb not null
  source_refs jsonb not null
  card_json jsonb not null
  created_at timestamptz not null default now()
  updated_at timestamptz not null default now()
```

## 4) Why This Shape

This table intentionally stores both:
- selected scalar fields as first-class columns;
- the full canonical card body as `card_json`.

Rationale:
- first-class columns support direct reads, sanity checks, filtering, and maintenance tasks;
- `card_json` preserves the complete canonical object and makes schema evolution easier;
- the current version should not depend on reconstructing the full card from dozens of relational joins.

## 5) Mapping Contract

The source semantic object is:
- `Specification/contracts/storage/incident_card.md`

Mapping rules:
- `IncidentCard.case_id` -> `case_id`
- `IncidentCard.title` -> `title`
- `IncidentCard.source_type` -> `source_type`
- `IncidentCard.source_name` -> `source_name`
- `IncidentCard.source_path` -> `source_path`
- `IncidentCard.vendor_or_project` -> `vendor_or_project`
- `IncidentCard.system_type` -> `system_type`
- `IncidentCard.version_tested` -> `version_tested`
- `IncidentCard.report_date` -> `report_date`
- `IncidentCard.short_summary` -> `short_summary`
- `IncidentCard.root_cause_summary` -> `root_cause_summary`
- `IncidentCard.reasoning_summary` -> `reasoning_summary`
- `IncidentCard.resolution_status` -> `resolution_status`

List and structured-field mapping rules:
- `canonical_symptoms` must be serialized to `jsonb` as an ordered JSON array of strings;
- `affected_components` must be serialized to `jsonb` as an ordered JSON array of strings;
- `failure_mode_candidates` must be serialized to `jsonb` as an ordered JSON array of strings;
- `observed_phases` must be serialized to `jsonb` as an ordered JSON array of strings;
- `incident_phases` must be serialized to `jsonb` as an ordered JSON array of phase objects;
- `turning_points` must be serialized to `jsonb` as an ordered JSON array of strings;
- `candidate_explanations` must be serialized to `jsonb` as an ordered JSON array of strings;
- `diagnostic_patterns` must be serialized to `jsonb` as an ordered JSON array of strings;
- `discriminating_checks` must be serialized to `jsonb` as an ordered JSON array of objects;
- `expected_observations` must be serialized to `jsonb` as an ordered JSON array of objects;
- `investigation_steps` must be serialized to `jsonb` as an ordered JSON array of strings;
- `mitigations_or_workarounds` must be serialized to `jsonb` as an ordered JSON array of strings;
- `prevention_or_design_followups` must be serialized to `jsonb` as an ordered JSON array of strings;
- `claimed_guarantees` must be serialized to `jsonb` as an ordered JSON array of strings;
- `violated_properties` must be serialized to `jsonb` as an ordered JSON array of strings;
- `fix_versions` must be serialized to `jsonb` as an ordered JSON array of strings;
- `confidence_notes` must be serialized to `jsonb` as an ordered JSON array of strings;
- `source_refs` must be serialized to `jsonb` as an ordered JSON array of strings.

Full-card mapping rule:
- `card_json` must contain the complete canonical `IncidentCard` serialized as JSON;
- `card_json` must preserve the same field meanings as `IncidentCard`;
- `card_json` must not omit reasoning-critical fields even when mirrored columns exist.

Timestamp rule:
- on insert, both `created_at` and `updated_at` must default to the insertion time;
- the current version does not define update semantics for `updated_at` because canonical incident-card storage is insert-only.

## 6) Nullability Rules

Rules:
- scalar optional fields from `IncidentCard` must map to SQL `NULL` when absent;
- non-optional list fields must map to non-`NULL` `jsonb` arrays, not SQL `NULL`;
- optional summaries such as `root_cause_summary`, `reasoning_summary`, and `resolution_status` may be SQL `NULL`;
- `card_json` must never be SQL `NULL`.

## 7) Duplicate Handling

Rules:
- `case_id` is the stable identity key;
- if a row with the same `case_id` already exists, insert-only storage behavior must fail with a duplicate-card error;
- the current version must not silently overwrite, upsert, or merge existing rows;
- update behavior, if later introduced, must be explicit and version-aware rather than implicit overwrite.

## 8) Storage-Level Invariants

The storage layer must preserve:
- full-card readability by `case_id`;
- ordered array semantics for phase and reasoning fields;
- semantic consistency between mirrored columns and `card_json`.

Required consistency rules:
- `case_id` column value must equal `card_json.case_id`;
- `title` column value must equal `card_json.title`;
- `source_type` column value must equal `card_json.source_type`;
- `source_name` column value must equal `card_json.source_name`;
- `source_path` column value must equal `card_json.source_path`;
- `short_summary` column value must equal `card_json.short_summary`;
- `canonical_symptoms` column JSON must equal `card_json.canonical_symptoms`;
- `incident_phases` column JSON must equal `card_json.incident_phases`;
- `discriminating_checks` column JSON must equal `card_json.discriminating_checks`;
- `expected_observations` column JSON must equal `card_json.expected_observations`;
- `source_refs` column JSON must equal `card_json.source_refs`.

## 9) Read Behavior Expectations

Canonical read behavior:
- `get by case_id` must be a first-class supported storage operation;
- `get many by case_id list` must preserve one-card-per-identity semantics;
- canonical read paths should rebuild the served `IncidentCard` from `card_json` or from a guaranteed-equivalent storage row mapping;
- read behavior must not silently drop unknown future fields that are preserved in `card_json`.

Operational indexing note:
- the current version should keep only a small set of direct operational indexes;
- storage must not be overloaded with speculative secondary indexes before real query patterns appear.

## 10) Executable SQL Schema

The executable SQL schema for this storage contract must live at:

- `Execution/docker/postgres/init/101_diagnostics_incident_cards.sql`

That SQL file must stay semantically aligned with this contract.
