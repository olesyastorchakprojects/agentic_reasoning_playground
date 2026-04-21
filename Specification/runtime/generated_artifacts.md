## 1) Purpose / Scope

This document defines the mandatory crate-level generated-artifact set for the runtime skeleton.

This document exists to define:
- the required crate-level Rust artifacts;
- crate-level structural glue artifacts;
- delegated artifact ownership for child runtime slices;
- conflict-avoidance rules between crate-level and child-level generation specifications;
- crate-level generation completion rules.

This document does not redefine:
- child API-client artifacts already owned by dedicated generation-order specifications;
- detailed behavior contracts for child runtime modules;
- future runtime layers that are not yet part of the current skeleton;
- pre-ingest incident-card chunk generation and its generated JSONL artifacts.

Generation for the runtime crate skeleton is incomplete if any required crate-level artifact from this document is missing.

Related non-crate artifact specification:
- `Specification/card_to_chunk_converter/incident_card_chunk_conversion.md`

That specification owns:
- conversion of canonical incident cards from PostgreSQL into pre-ingest
  `chunks.jsonl` files;
- the generated card-derived chunk payload contract used before hybrid ingest;
- the card-to-chunk converter CLI and its file-level artifact behavior.

## 2) Required Crate-Level Artifacts

Generation must create or update the runtime crate with these crate-level artifacts:

- `Cargo.toml`
- `src/lib.rs`
- `src/main.rs`
- `src/errors/mod.rs`
- `src/shared_types.rs`
- `src/config/mod.rs`
- `src/config/settings.rs`
- `src/config/load.rs`
- `src/observability/mod.rs`
- `src/api_clients/mod.rs`
- `src/api_clients/model/mod.rs`
- `src/api_clients/qdrant/mod.rs`
- `src/api_clients/postgres/mod.rs`
- `src/request_pipeline/mod.rs`
- `src/request_pipeline/input_normalization.rs`
- `src/request_pipeline/query_structuring.rs`
- `src/request_pipeline/candidate_card_retrieval.rs`
- `src/request_pipeline/card_hydration.rs`
- `src/request_pipeline/incident_evidence_retrieval.rs`
- `src/request_pipeline/theory_evidence_retrieval.rs`
- `src/request_pipeline/prompt_context_assembly.rs`
- `src/utils/mod.rs`
- `src/utils/retry.rs`
- `src/utils/tokenizer.rs`

Artifact rules:
- `Cargo.toml` must define a valid Rust crate for the generated runtime skeleton;
- `src/lib.rs` must declare the crate-level public module tree;
- `src/main.rs` must exist as the binary entrypoint;
- `src/main.rs` must implement the crate-level CLI contract from `Specification/runtime/runtime.md` and delegate config loading to library-owned code;
- `src/errors/mod.rs` must define the parent error hierarchy required by `Specification/runtime/runtime.md`;
- `src/shared_types.rs` must define the shared cross-module runtime types required by `Specification/runtime/runtime.md`;
- `src/config/mod.rs` must expose the parent config interface and parent config error type;
- `src/config/settings.rs` must define the resolved typed settings model required by `Specification/runtime/runtime.md`;
- `src/config/load.rs` must define config loading and merge logic for runtime TOML, ingest TOML, and environment values;
- `src/observability/mod.rs` must define startup-time observability initialization from typed settings;
- `src/request_pipeline/mod.rs` must expose request-pipeline child modules required by active runtime slice specifications;
- parent `mod.rs` files are required structural artifacts and must not be omitted;
- the current crate-level artifact contract is structural and compositional, not behavior-duplicating.

## 3) Delegated Child Artifact Ownership

This crate-level artifact specification delegates child artifact ownership to existing child generation-order specifications.

Child artifacts for the model API-client subtree are owned by:
- `Specification/runtime/api_clients/model/generation_order.md`

Child artifacts for the Qdrant API-client subtree are owned by:
- `Specification/runtime/api_clients/qdrant/generation_order.md`

Child artifacts for the request-pipeline leaf module `input_normalization` are owned by:
- `Specification/runtime/request_pipeline/input_normalization.md`

Child artifacts for the request-pipeline leaf module `query_structuring` are owned by:
- `Specification/runtime/request_pipeline/query_structuring.md`

Child artifacts for the request-pipeline leaf module `candidate_card_retrieval` are owned by:
- `Specification/runtime/request_pipeline/candidate_card_retrieval.md`

Child artifacts for the request-pipeline leaf module `card_hydration` are owned by:
- `Specification/runtime/request_pipeline/card_hydration.md`

Child artifacts for the request-pipeline leaf module `incident_evidence_retrieval` are owned by:
- `Specification/runtime/request_pipeline/incident_evidence_retrieval.md`

Child artifacts for the request-pipeline leaf module `theory_evidence_retrieval` are owned by:
- `Specification/runtime/request_pipeline/theory_evidence_retrieval.md`

Child artifacts for the request-pipeline leaf module `prompt_context_assembly` are owned by:
- `Specification/runtime/request_pipeline/prompt_context_assembly.md`

Child artifacts for the tokenizer utility module are owned by:
- `Specification/runtime/utils/tokenizer.md`

Child artifact delegation rules:
- this document must not redefine the child generated-file lists already owned by those specifications;
- crate-level generation must create the required parent structural files that allow those child artifacts to live in the correct crate hierarchy;
- child subtree generation must remain compatible with the crate-level structural files from this document;
- when a single-module child specification does not have its own `generation_order.md`, that single-module specification is itself the source of truth for its generated `.rs` file.

## 4) Child-Spec Compatibility Rules

Compatibility rules:
- crate-level artifact requirements must not conflict with child generation-order specifications;
- when a child generation-order specification already defines generated Rust modules, this document must treat that child specification as the source of truth for those modules;
- crate-level files may reference child modules structurally, but must not replace child-owned artifact contracts;
- the generated parent `mod.rs` files must expose child modules in a way that remains consistent with the child-owned generated files;
- crate-level generation must not invent alternative child module layouts that contradict the delegated child specifications.

Current delegated child specifications:
- `Specification/runtime/api_clients/model/generation_order.md`
- `Specification/runtime/api_clients/qdrant/generation_order.md`
- `Specification/runtime/request_pipeline/input_normalization.md`
- `Specification/runtime/request_pipeline/query_structuring.md`
- `Specification/runtime/request_pipeline/candidate_card_retrieval.md`
- `Specification/runtime/request_pipeline/card_hydration.md`
- `Specification/runtime/request_pipeline/incident_evidence_retrieval.md`
- `Specification/runtime/request_pipeline/theory_evidence_retrieval.md`
- `Specification/runtime/request_pipeline/prompt_context_assembly.md`
- `Specification/runtime/utils/tokenizer.md`
- `Specification/card_to_chunk_converter/incident_card_chunk_conversion.md`

## 5) Structural Generation Rules

Rules:
- the generated crate must include both library and binary entrypoints;
- the generated crate must include a dedicated parent `errors` module;
- the generated crate must include a dedicated parent `config` module;
- the generated crate must include a dedicated parent `observability` module;
- the generated crate must include a dedicated parent `api_clients` module;
- the generated crate must include a dedicated parent `request_pipeline` module when request-pipeline child specifications are active;
- the generated crate must include a dedicated `utils` module for reusable crate-wide helpers;
- `src/api_clients/mod.rs` must expose the delegated root child module `embedding_client` in addition to the structural child subtrees;
- the generated crate must include structural parent modules for `model`, `qdrant`, `postgres`, and `request_pipeline`;
- the generated Qdrant subtree may include internal decomposition files such as `sparse_preparation.rs` when required by child specifications;
- structural parent modules must remain present even when most child implementation files are generated from delegated child specifications;
- structural files must contain real Rust module declarations and required parent-level definitions, not placeholders or TODO-only stubs.
- the generated config subtree must contain executable config-loading code and typed settings definitions rather than prose placeholders.

## 6) Generation Completion Rule

Generation for the runtime crate skeleton is complete only when all of the following are true:

- every required crate-level artifact from section `2)` exists;
- crate-level artifacts are structurally consistent with `Specification/runtime/runtime.md`;
- delegated child artifact ownership remains unambiguous;
- no child-owned artifact list from the delegated child generation-order specifications has been redefined by this document;
- the generated crate-level structure is sufficient to host the child-generated runtime modules without layout conflicts;
- required structural files are real Rust source files rather than comments, placeholders, or empty stubs;
- when active specifications require executable Rust tests for the generated crate-level files, those tests must be generated in the same generation pass as the implementation;
- required tests for generated crate-level files must not be replaced by comments, TODO markers, prose, pseudo-tests, placeholder test functions without assertions, or empty test modules;
- the generated crate must be valid Rust and pass `cargo check`.
