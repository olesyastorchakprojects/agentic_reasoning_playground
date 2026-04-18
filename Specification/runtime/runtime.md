## 1) Purpose / Scope

This document defines the minimal crate-level runtime specification for the application skeleton.

The current version exists to define:
- the crate-level module tree;
- the crate-level module boundaries;
- the crate-level error hierarchy;
- crate-level composition rules for `lib.rs` and `main.rs`;
- how the runtime crate-level specification relates to existing runtime slice specifications.

This document does not define:
- configuration loading;
- observability;
- orchestration flows;
- domain logic;
- request/response workflows above the current runtime API-client layer;
- detailed behavior of individual API-client modules already specified elsewhere.

The current version must be minimal.
It must define only the crate structure and error-model rules required to generate a clean Rust crate skeleton that can be extended later.

This document is the crate-level source of truth for:
- `src/lib.rs`
- `src/main.rs`
- `src/errors/mod.rs`
- `src/api_clients/mod.rs`
- crate-level composition and re-export rules

Detailed API-client behavior and child-module generation remain defined in their dedicated specifications under `Specification/runtime/api_clients/`.

## 2) Crate Structure

The generated runtime must be a Rust crate that includes both:
- a library target;
- a binary target.

The current crate-level structure consists of:
- `lib.rs`
- `main.rs`
- `errors`
- `api_clients`
- `utils`

The current required crate-level module tree is:

- crate root
  - `errors`
  - `api_clients`
    - `embedding_client`
    - `model`
    - `qdrant`
    - `postgres`
  - `utils`
    - `retry`
    - `tokenizer`

Structure rules:
- `lib.rs` is the primary public crate boundary;
- `main.rs` is a thin binary entrypoint;
- `errors` owns only crate-level root error definitions and root error re-exports;
- `api_clients` is the parent boundary for runtime external-service clients;
- `utils` owns reusable crate-wide helpers that are intentionally shared across multiple runtime areas;
- child API-client subtrees keep their own dedicated module contracts;
- the generated crate structure must remain extension-friendly for future runtime layers.

## 3) Module Boundary Rules

### `lib.rs`

`lib.rs` must:
- declare the public crate module tree;
- expose the top-level runtime modules required by this specification;
- serve as the main public boundary for library consumers.

`lib.rs` must not:
- contain concrete API-client implementation logic;
- contain application bootstrapping logic beyond what is required to expose the crate boundary;
- duplicate type or error definitions owned by child modules.

### `main.rs`

`main.rs` must:
- provide the binary crate entrypoint;
- remain thin;
- delegate into library-owned code rather than owning runtime logic itself.

`main.rs` must not:
- contain API-client business logic;
- become the ownership boundary for shared runtime types;
- define a parallel error hierarchy separate from the library crate.

### `errors`

`errors` must:
- define the crate-level root error type;
- re-export parent-module error types where needed by the crate boundary;
- preserve the typed error hierarchy of the crate.

### `api_clients`

`api_clients` must:
- be the parent module for external-service client boundaries;
- define the parent API-client error type;
- expose child API-client subtrees through explicit modules.

`api_clients` must not:
- redefine child-module behavior contracts already specified elsewhere;
- flatten child-module errors into strings;
- own future orchestration or domain-level runtime flows.

### `utils`

`utils` must:
- contain small reusable helper modules that are intentionally shared across multiple runtime areas;
- contain helpers such as retry and tokenizer utilities when those helpers are not specific to a single API-client subtree.

`utils` must not:
- become a catch-all location for subtree-specific request-preparation logic;
- absorb service-specific logic that belongs to one API-client subtree.

## 4) Error Model

The runtime crate must use the `thiserror` crate for error definitions.

Error hierarchy rule:
- each module defines its own error enum;
- each parent module wraps its direct child-module errors through typed enum variants;
- `RuntimeError` includes only errors from top-level runtime subsystems;
- `ApiClientError` includes only errors from API-client child modules;
- the error hierarchy must mirror the crate module hierarchy.

The current required parent error types are:
- `RuntimeError`
- `ApiClientError`
- `ModelApiClientError`
- `QdrantApiClientError`
- `PostgresApiClientError`

### `RuntimeError`

`RuntimeError` is the single root public error type at the crate boundary.

The generated Rust module must define a crate-level enum equivalent in ownership to:

```rust
pub enum RuntimeError {
    ApiClients(ApiClientError),
}
```

Rules:
- `RuntimeError` must include only top-level subsystem errors;
- `RuntimeError` must not directly include leaf API-client errors when an intermediate parent error exists;
- future top-level subsystems may be added as additional `RuntimeError` variants in later iterations.

### `ApiClientError`

`ApiClientError` is the parent error type for the `api_clients` module boundary.

`ApiClientError` must be defined in `src/api_clients/mod.rs`.
`src/errors/mod.rs` must define only `RuntimeError` and may re-export `ApiClientError`.

The generated Rust module must define a parent enum equivalent in ownership to:

```rust
pub enum ApiClientError {
    Embedding(EmbeddingClientError),
    Model(ModelApiClientError),
    Qdrant(QdrantApiClientError),
    Postgres(PostgresApiClientError),
}
```

Rules:
- `ApiClientError` must include only API-client child-module errors;
- `ApiClientError` must wrap child errors through typed variants;
- `ApiClientError` must not flatten child errors into string-only variants;
- `ApiClientError` must not leak raw third-party error types through its public interface.

### API-Client Subsystem Parent Errors

The generated runtime must define explicit subsystem parent errors for multi-module API-client subtrees.

The current required subsystem parent error types are:
- `ModelApiClientError`
- `QdrantApiClientError`
- `PostgresApiClientError`

The generated Rust modules must define parent enums equivalent in ownership to:

```rust
pub enum ModelApiClientError {
    Client(ModelClientError),
}

pub enum QdrantApiClientError {
    DenseSearch(DenseSearchClientError),
    HybridSearch(HybridSearchClientError),
    CardsCollection(CardsCollectionError),
    PracticeChunksCollection(PracticeChunksCollectionError),
    TheoryChunksCollection(TheoryChunksCollectionError),
}

pub enum PostgresApiClientError {
    IncidentCardStore(IncidentCardStoreError),
}
```

Rules:
- subsystem parent errors must wrap their direct child-module errors through typed variants;
- `ApiClientError` must wrap subsystem parent errors when such an intermediate parent exists;
- `ApiClientError` must not bypass subsystem parents by wrapping those subtree leaf errors directly.

### Leaf Error Ownership

Leaf API-client modules must keep their own public error enums as defined by their dedicated specifications.

Rules:
- leaf modules remain the source of truth for their own failure categories;
- parent error enums wrap child errors rather than replacing them;
- generated code must preserve typed child errors through the parent hierarchy;
- raw third-party, transport, parser, or library errors must not leak through public module interfaces;
- error variants must preserve available diagnostic information owned by the failure point.

### Boundary Rules

Rules:
- crate-level public entrypoints must return `RuntimeError`;
- parent-module public entrypoints may return their parent-module error type;
- leaf-module public entrypoints may continue returning their own leaf error enums where their dedicated specifications require that boundary;
- conversion from child-module errors into parent-module errors must be explicit and typed;
- the generated crate must not create competing parallel root error types.

## 5) Composition Rules

The crate-level runtime specification composes existing runtime slice specifications.

Composition rules:
- this document must not redefine detailed behavior already owned by dedicated child specifications;
- child API-client module behavior remains defined by the corresponding files under `Specification/runtime/api_clients/`;
- crate-level generation must wire child modules together structurally without overriding their dedicated contracts;
- parent modules must import child types and child errors rather than regenerating duplicate local definitions;
- re-exports may be used where helpful, but they must not hide the declared module hierarchy.

Compatibility rule:
- this crate-level specification must remain compatible with the child generation-order specifications that already define generated child-module artifacts.

The current child artifact ownership is delegated to:
- `Specification/runtime/api_clients/model/generation_order.md`
- `Specification/runtime/api_clients/qdrant/generation_order.md`

This document must not be interpreted as a replacement for those child generation-order specifications.

## 6) General Implementation Guidance

These rules apply to the current minimal runtime crate skeleton.

Implementation rules:
- the implementation must be modular;
- each parent module must own its own public interface and parent error type;
- cross-module interaction must happen through explicit typed interfaces;
- parent modules must wrap child concerns rather than absorb child-owned implementation logic;
- the generated crate must preserve the declared Rust module layout instead of collapsing logic into one large file;
- behavior already owned by dedicated child specifications must be implemented in the corresponding child modules rather than redefined at crate level;
- reusable retry and tokenizer helpers may live under `src/utils/`;
- Qdrant sparse query preparation must live under the Qdrant subtree rather than under `src/utils/`.

Boundary and ownership rules:
- module boundaries must be preserved in both code structure and error ownership;
- parent modules must not reach into private child implementation details instead of using declared interfaces;
- child-owned types and child-owned errors must be imported and wrapped rather than redefined locally;
- the crate-level structure must stay easy to extend with future runtime layers without rewriting the existing ownership model.
