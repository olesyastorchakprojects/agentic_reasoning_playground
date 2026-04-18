## 1) Purpose / Scope

This document defines the mandatory generated unit-test contract for the PostgreSQL incident-card store slice.

This document is the single source of truth for:
- required unit-test cases for `incident_card_store`;
- required validation, mapping, and duplicate-handling checks.

This document must be read together with:
- `Specification/runtime/unit_tests_common.md`

## 2) Covered Modules

The current PostgreSQL incident-card test scope covers:

- `postgres.incident_card_store`

## 3) Required Unit Tests

Generated unit tests for `PostgresIncidentCardStore` must include all of the following cases:

- `new(...)` fails when `PostgresIncidentCardStoreConfig.postgres_url` is empty;
- `put_card(...)` validates the input card before any database write is attempted;
- schema-invalid `IncidentCard` fails with the exact `IncidentCardStoreError::Validation` variant;
- serialization failure while constructing storage payload fails with the exact `IncidentCardStoreError::Serialization` variant;
- successful `put_card(...)` writes the exact canonical field values required by the storage contract;
- successful `put_card(...)` preserves ordered array fields in JSON serialization;
- duplicate `case_id` insert fails with the exact `IncidentCardStoreError::DuplicateCaseId(...)` variant;
- `get_card_by_case_id(...)` returns `Ok(None)` when the row does not exist;
- `get_card_by_case_id(...)` reconstructs a canonical `IncidentCard` from a valid stored row;
- `get_cards_by_case_ids(...)` returns `Ok(vec![])` when input `case_ids` is empty;
- `get_cards_by_case_ids(...)` may deduplicate repeated input ids before query execution and still returns at most one card per `case_id`;
- `get_cards_by_case_ids(...)` returns at most one card per `case_id`;
- read mapping prefers `card_json` as the canonical full-card source when it is present and valid;
- read mapping fails when `card_json` conflicts with mirrored required storage fields;
- invalid stored row shape fails with the exact `IncidentCardStoreError::InvalidStoredRow` variant;
- raw database-client errors do not leak through the public interface.
