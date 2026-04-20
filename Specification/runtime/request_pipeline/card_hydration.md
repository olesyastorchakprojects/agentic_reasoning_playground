## 1) Purpose / Scope

This document defines the runtime leaf-module contract for `card_hydration`.

This module exists to:
- accept the shared `CandidateCardRetrievalOutput`;
- depend on the PostgreSQL-backed `PostgresIncidentCardStore` client through dependency injection;
- load full canonical incident-card records from PostgreSQL by `case_id`;
- preserve the incoming `primary` and `alternatives` partitioning;
- preserve the incoming retrieval order;
- return the shared `CardHydrationOutput`.

This document is the source of truth for:
- the `card_hydration` leaf-module boundary;
- the module public interface;
- the module-owned hydration behavior from candidate ids into full `IncidentCard` values;
- the module-owned partition-preservation behavior;
- the module-owned error boundary.

This document does not define:
- semantic card retrieval from Qdrant;
- PostgreSQL storage schema or SQL mapping details owned by `incident_card_store`;
- downstream evidence retrieval;
- prompt assembly;
- response generation behavior.

Shared request and response types are defined by:
- `Specification/runtime/runtime.md`

PostgreSQL card-store behavior is defined by:
- `Specification/runtime/api_clients/postgres/incident_card_store.md`

The generated Rust module file for the current version is:
- `src/request_pipeline/card_hydration.rs`

## 2) Required Shared Types

This module must use the shared runtime types:
- `CandidateCardRetrievalOutput`
- `IncidentCard`
- `CardHydrationOutput`

These shared types are defined in:
- `Specification/runtime/runtime.md`

The current generated Rust runtime must define shared types equivalent in
ownership to:

```rust
pub struct CardHydrationOutput {
    pub primary: Option<IncidentCard>,
    pub alternatives: Vec<IncidentCard>,
}
```

Shared-type rules:
- `CardHydrationOutput` is the shared cross-module output of `card_hydration`;
- `CardHydrationOutput.primary` contains the hydrated full-card form of the retrieved primary candidate when one exists;
- `CardHydrationOutput.primary` must be `None` when the incoming `CandidateCardRetrievalOutput.primary` is `None`;
- `CardHydrationOutput.alternatives` contains the hydrated full-card forms of the retrieved alternative candidates;
- `CardHydrationOutput` must preserve the incoming `primary` / `alternatives` partitioning exactly;
- retrieval scores from `CandidateCard` are not part of `CardHydrationOutput` in the current version.

Import rule for the generated Rust module:
- shared input and output types used by this module, including `CandidateCardRetrievalOutput`, `IncidentCard`, and `CardHydrationOutput`, must be imported from `crate::shared_types`;
- `PostgresIncidentCardStore` must be imported through `crate::api_clients::postgres::...`.

## 3) Store Dependency

This module must depend on the PostgreSQL client:
- `PostgresIncidentCardStore`

`PostgresIncidentCardStore` is defined in:
- `Specification/runtime/api_clients/postgres/incident_card_store.md`

The generated Rust module must store the dependency equivalent in ownership to:

```rust
std::sync::Arc<PostgresIncidentCardStore>
```

Dependency rules:
- the module must accept `PostgresIncidentCardStore` through dependency injection;
- the module must not construct PostgreSQL pools itself;
- the module must not accept raw `PostgresSettings` directly when a ready store dependency is provided;
- the module must reuse the injected store for all hydration requests.

## 4) Public Interface

The generated Rust module must define a public module boundary equivalent in
ownership to:

```rust
pub struct CardHydration {
    // implementation-owned fields
}

impl CardHydration {
    pub fn new(
        incident_card_store: std::sync::Arc<PostgresIncidentCardStore>,
    ) -> Self;

    pub async fn hydrate(
        &self,
        candidates: &CandidateCardRetrievalOutput,
    ) -> Result<CardHydrationOutput, CardHydrationError>;
}
```

For the current version, the implementation-owned fields must contain exactly:
- `incident_card_store: Arc<PostgresIncidentCardStore>`

Rules:
- `new(...)` must retain the injected dependency for reuse;
- `hydrate(...)` must be the single public request-time entrypoint of this module;
- `hydrate(...)` must not mutate the input `CandidateCardRetrievalOutput`;
- `hydrate(...)` must not re-rank candidates;
- `hydrate(...)` must not expose PostgreSQL row-mapper or storage-internal types in its public return value.

Debug rules:
- `CardHydration` may derive `Debug` in the current version if `PostgresIncidentCardStore` implements `Debug`;
- otherwise the generated Rust module must provide a manual `impl Debug for CardHydration`;
- the generated Rust module must not choose arbitrarily between derived and manual `Debug`; it must follow the capability of the stored dependency type.

## 5) Hydration Behavior

This module must hydrate candidates by `case_id`.

Hydration rules:
- the module must gather requested `case_id` values from `CandidateCardRetrievalOutput.primary` and `CandidateCardRetrievalOutput.alternatives`;
- the module must call `PostgresIncidentCardStore::get_cards_by_case_ids(...)` at most once per hydration request in the current version when the incoming candidates are non-empty;
- the module must not deduplicate requested `case_id` values;
- the module must preserve the incoming candidate order when constructing the final output;
- the module must preserve the incoming `primary` / `alternatives` partitioning exactly;
- retrieval scores from `CandidateCard` must be discarded after hydration in the current version.

Empty-input rule:
- if the incoming candidates are:

```rust
CandidateCardRetrievalOutput {
    primary: None,
    alternatives: vec![],
}
```

then the module must return:

```rust
CardHydrationOutput {
    primary: None,
    alternatives: vec![],
}
```

without issuing a PostgreSQL read.

## 6) Mapping And Ordering Rules

The store read API may return cards in storage order or query order.

Module-owned ordering rules:
- the module must reconstruct a lookup map keyed by `IncidentCard.case_id`;
- the module must rebuild `primary` and `alternatives` using the original order of `CandidateCardRetrievalOutput`;
- the module must not rely on `get_cards_by_case_ids(...)` preserving input order;
- `CardHydrationOutput.primary` must be hydrated from `CandidateCardRetrievalOutput.primary.case_id` when a primary candidate exists;
- `CardHydrationOutput.alternatives` must be hydrated from `CandidateCardRetrievalOutput.alternatives[*].case_id` in the original alternatives order.

## 7) Missing-Card Rule

The current version requires fail-fast behavior for missing cards.

Rules:
- the module must check that each requested `case_id` value from the input candidates is present in the returned lookup map keyed by `IncidentCard.case_id`;
- if any requested `case_id` value is absent from that lookup map, hydration must fail with `CardHydrationError::MissingCard`;
- partial success is forbidden in the current version;
- the module must not silently drop missing cards;
- the module must not substitute placeholder cards for missing `case_id` values.

## 8) Error Boundary

The generated Rust module must define a public error enum equivalent in
ownership to:

```rust
#[derive(Debug, thiserror::Error)]
pub enum CardHydrationError {
    #[error("missing hydrated card for case_id: {case_id}")]
    MissingCard { case_id: String },
    #[error("incident card store error: {0}")]
    Store(IncidentCardStoreError),
}
```

Error rules:
- `hydrate(...)` must return `CardHydrationError`;
- PostgreSQL read failures must be wrapped as `CardHydrationError::Store`;
- missing requested cards must be reported as `CardHydrationError::MissingCard`;
- empty-input hydration is not an error and must return a successful empty output.

## 9) Tests

Detailed unit-test requirements for this module must be defined in:
- `Specification/runtime/unit_tests.md`
