## 1) Purpose / Scope

`incident_card_store` persists and reads canonical incident cards from PostgreSQL.

This module:
- receives fully assembled canonical `IncidentCard` values;
- validates `IncidentCard` before attempting persistence;
- maps `IncidentCard` into the storage representation required by canonical card storage;
- writes one canonical incident card row to PostgreSQL;
- reads one or many canonical incident cards by `case_id`.

This module does not:
- assemble `IncidentCard`;
- rank cards;
- retrieve cards semantically;
- generate prompts;
- talk to Qdrant;
- read raw TOML directly;
- read raw environment variables directly.

Shared-type source of truth:
- `IncidentCard`
- `IncidentPhase`
- `DiscriminatingCheck`
- `ExpectedObservation`

These shared types are defined in:
- `Specification/runtime/runtime.md`

## 2) Public Interface

The generated Rust module must define:

```rust
pub struct PostgresIncidentCardStoreConfig {
    pub postgres_url: String,
}

pub struct PostgresIncidentCardStore {
    pool: sqlx::PgPool,
}

impl PostgresIncidentCardStore {
    pub async fn new(
        config: PostgresIncidentCardStoreConfig,
    ) -> Result<Self, IncidentCardStoreError>;

    pub async fn put_card(
        &self,
        card: &IncidentCard,
    ) -> Result<(), IncidentCardStoreError>;

    pub async fn get_card_by_case_id(
        &self,
        case_id: &str,
    ) -> Result<Option<IncidentCard>, IncidentCardStoreError>;

    pub async fn get_cards_by_case_ids(
        &self,
        case_ids: &[String],
    ) -> Result<Vec<IncidentCard>, IncidentCardStoreError>;
}
```

Interface rules:
- all public methods must be async;
- `put_card` must not mutate the input card;
- read methods must return canonical `IncidentCard` objects, not storage-specific row structs;
- module-internal logic may use storage row mappers before returning domain values.

## 3) Input And Output Types

Input types:
- `IncidentCard`
- `PostgresIncidentCardStoreConfig`
- `case_id`
- `case_ids`

Source of truth:
- canonical card contract: `Specification/contracts/storage/incident_card.md`
- canonical storage contract: `Specification/contracts/storage/incident_cards_storage.md`
- shared runtime types: `Specification/runtime/runtime.md`
- machine-readable schema: `Execution/schemas/incident_card.schema.json`
- executable SQL schema: `Execution/docker/postgres/init/101_diagnostics_incident_cards.sql`

Output types:
- none on successful write;
- `Option<IncidentCard>` for `get_card_by_case_id`;
- `Vec<IncidentCard>` for `get_cards_by_case_ids`.

Type rules:
- `IncidentCard` must satisfy the canonical storage contract;
- `case_id` must be non-empty after trimming;
- `case_ids` may be empty;
- duplicate `case_ids` in input must not produce duplicate output cards.

Import rule for the generated Rust module:
- `IncidentCard`, `IncidentPhase`, `DiscriminatingCheck`, and `ExpectedObservation` must be imported from `crate::shared_types`;
- `incident_card_store` must not privately own these types once they are shared across runtime module boundaries.

## 4) Configuration Usage

The current version requires only:
- `PostgresIncidentCardStoreConfig.postgres_url`

Configuration rules:
- `PostgresIncidentCardStoreConfig.postgres_url` must be non-empty after trimming;
- the module must not read raw TOML directly;
- the module must not read raw config maps directly;
- the module must not read raw environment variables directly.
- the leaf store constructor must not accept crate-level `Settings` directly.

## 4.1) Settings Propagation Rules

The crate-level runtime settings model must not be used directly as the config type
for `PostgresIncidentCardStore`.

The current crate-level source settings path is:
- `Settings.postgres`

The full crate-level source field path is:
- `Settings.postgres.url`

Propagation rules:
- a future bootstrap or parent runtime module may construct `PostgresIncidentCardStoreConfig` before calling `PostgresIncidentCardStore::new(...)`;
- `PostgresIncidentCardStoreConfig.postgres_url` <- `Settings.postgres.url`

Rules:
- `PostgresIncidentCardStore` must receive only `PostgresIncidentCardStoreConfig`;
- `PostgresIncidentCardStore` must not depend on crate-level `Settings`;
- the current minimal crate-skeleton stage does not require production code that performs this wiring;
- whenever a higher-level runtime layer performs this wiring, conversion from crate-level settings slices into module-owned config types must happen before store construction.

## 5) Validation Rules Before Write

Before attempting persistence, the module must validate the incoming `IncidentCard`.

Validation must follow:
- `Execution/schemas/incident_card.schema.json`

Validation must also enforce semantic invariants derived from:
- `Specification/contracts/storage/incident_card.md`
- `Specification/contracts/storage/incident_cards_storage.md`

Validation failure must be reported before any database write is attempted.

## 6) Storage Mapping Contract

The canonical storage target is:
- `diagnostics.incident_cards`

The canonical storage mapping contract is defined in:
- `Specification/contracts/storage/incident_cards_storage.md`

The module must define two internal mapping helpers:
- `IncidentCardStorageRowMapper`
- `IncidentCardStorageReadMapper`

`IncidentCardStorageRowMapper` responsibilities:
- accept a validated `IncidentCard`;
- construct the exact storage-facing write payload used for insertion into `diagnostics.incident_cards`;
- serialize JSON-array and JSON-object fields using `serde_json`;
- construct `card_json` from the full canonical card;
- preserve ordered array fields during serialization;
- exclude storage-produced fields such as `created_at` and `updated_at`.

`IncidentCardStorageReadMapper` responsibilities:
- accept a storage row returned from `diagnostics.incident_cards`;
- reconstruct a canonical `IncidentCard`;
- prefer `card_json` as the canonical full-card source when it is present and valid;
- fail if required canonical fields cannot be reconstructed consistently;
- fail if `card_json` conflicts with mirrored storage columns for fields that are required to agree by the storage contract.

Mapping rules:
- write mapping must preserve the same field meanings as the canonical storage contract;
- read mapping must not silently drop unknown future fields preserved in `card_json`;
- read mapping must not invent fallback values for missing required canonical fields.

## 7) Write Behavior

Write behavior rules:
- `put_card` must execute one parameterized insert into `diagnostics.incident_cards`;
- handwritten SQL string interpolation with embedded values is forbidden;
- handwritten JSON string construction is forbidden;
- the current version must use insert-only semantics;
- if a row with the same `case_id` already exists, `put_card` must fail with a duplicate-card error;
- the current version must not silently overwrite, upsert, or merge an existing row.

## 8) Read Behavior

Read behavior rules:
- `get_card_by_case_id` must query by exact `case_id`;
- `get_cards_by_case_ids` must query by the supplied identity set only;
- `get_cards_by_case_ids` may deduplicate repeated input ids before issuing the query;
- `get_cards_by_case_ids` may return results in storage order or query order, but it must return at most one card per `case_id`;
- `get_cards_by_case_ids` must not promise preservation of input `case_ids` order in the current version;
- if `case_ids` is empty, `get_cards_by_case_ids` must return `Ok(vec![])` without issuing a query;
- missing `case_id` on single read must return `Ok(None)`, not an error.

## 9) Error Model

The generated Rust module must define:

```rust
pub enum IncidentCardStoreError {
    InvalidConfig(&'static str),
    Validation(&'static str),
    Serialization(&'static str),
    Connection(String),
    Insert(String),
    Query(String),
    DuplicateCaseId(String),
    InvalidStoredRow(&'static str),
}
```

Required failure categories:
- invalid config;
- card validation failure;
- serialization failure;
- PostgreSQL connection failure;
- insert execution failure;
- duplicate `case_id`;
- query execution failure;
- invalid stored row;
- unexpected internal state.

Error rules:
- raw database client errors must not leak through the public module interface;
- module-level errors must preserve available diagnostic information;
- duplicate primary-key violations must map to `DuplicateCaseId(...)`.

## 10) Implementation Notes

The generated implementation must use:
- `jsonschema` for validation against `Execution/schemas/incident_card.schema.json`;
- `serde` for typed serialization support;
- `serde_json` for JSON serialization and deserialization;
- `sqlx` for PostgreSQL access;
- `chrono` for date handling.

Implementation rules:
- SQL statements must use parameterized queries;
- PostgreSQL constraints are the final storage-level enforcement layer, not the first validation layer;
- `sqlx::PgPool` must be created once in `new(...)` and reused for the store lifetime.
- `new(...)` must validate `PostgresIncidentCardStoreConfig.postgres_url` before attempting pool creation.
