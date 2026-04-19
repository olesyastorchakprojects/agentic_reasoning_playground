## 1) Purpose / Scope

This document defines the runtime leaf-module contract for `input_normalization`.

This module exists to:
- accept the raw shared `UserRequest`;
- normalize the request query into a deterministic canonical string form;
- compute the token count for the normalized query;
- enforce the configured input-token ceiling;
- return the shared `NormalizedUserRequest`.

This document is the source of truth for:
- the `input_normalization` leaf-module boundary;
- the module public interface;
- module-owned behavior for deterministic query normalization;
- token-counting behavior at this module boundary;
- the module-owned error boundary;
- unit-test expectations for this module.

This document does not define:
- crate-level config loading or settings construction;
- tokenizer cache/download behavior;
- semantic query structuring;
- retrieval logic;
- prompt assembly;
- model generation;
- orchestration policy.

Tokenizer utility behavior is defined by:
- `Specification/runtime/utils/tokenizer.md`

Shared request types are defined by:
- `Specification/runtime/runtime.md`

## 2) Required Shared Types

This module must use the shared runtime types:
- `UserRequest`
- `NormalizedUserRequest`

These shared types are defined in:
- `Specification/runtime/runtime.md`

This module must not redefine those shared types locally.

## 3) Settings Dependency

This module must receive the typed settings slice:
- `InputNormalizationSettings`

`InputNormalizationSettings` is defined at the crate-level runtime boundary in:
- `Specification/runtime/runtime.md`

For the current version, `InputNormalizationSettings` contains exactly:

```rust
pub struct InputNormalizationSettings {
    pub max_input_tokens: usize,
    pub tokenizer_source: String,
}
```

Rules:
- this module must receive `InputNormalizationSettings` through its constructor;
- this module must not read raw TOML, raw environment variables, or crate-level `Settings` directly;
- this module must not redefine config-loading rules that belong to the `config` subsystem.

## 4) Public Interface

The generated Rust module must define a public module boundary equivalent in ownership to:

```rust
pub struct InputNormalization {
    // implementation-owned fields
}

impl InputNormalization {
    pub async fn new(
        settings: InputNormalizationSettings,
    ) -> Result<Self, InputNormalizationError>;

    pub fn normalize(
        &self,
        request: UserRequest,
    ) -> Result<NormalizedUserRequest, InputNormalizationError>;
}
```

For the current version, the implementation-owned fields must contain exactly:
- `tokenizer: HfTokenizer`
- `max_input_tokens: usize`

Rules:
- `new(...)` must initialize the tokenizer once and retain it for reuse;
- `normalize(...)` must be synchronous once the module has been constructed;
- this module must not require callers to pass tokenizer cache paths, artifact roots, or raw tokenizer loader dependencies.

## 5) Constructor Rules

`InputNormalization::new(settings)` must:
- load the tokenizer defined by `settings.tokenizer_source`;
- use the tokenizer utility contract from `Specification/runtime/utils/tokenizer.md`;
- store the loaded tokenizer for later reuse;
- store `settings.max_input_tokens` for later ceiling enforcement.

Constructor rules:
- constructor failure caused by tokenizer utility failure must surface through this module's typed error boundary;
- this module must not defer tokenizer loading until the first request in the current version;
- this module must not mutate `settings.tokenizer_source`;
- this module must not own independent retry or download policy for tokenizer loading.

## 6) Normalization Rules

`normalize(request)` must process `request.query` in the following order:

1. read the raw `UserRequest.query` string;
2. trim leading and trailing whitespace;
3. collapse all internal whitespace runs, including spaces, tabs, newlines, and other whitespace separators, into one ASCII space;
4. if the resulting normalized query is empty, fail with the module-owned empty-query error;
5. compute the token count for the normalized query using the configured tokenizer;
6. if the computed token count is `0`, fail with the module-owned empty-query error;
7. if the computed token count exceeds `max_input_tokens`, fail with the module-owned input-too-long error;
8. return `NormalizedUserRequest { query, input_token_count }`.

Rules:
- normalization must be deterministic;
- normalization must not paraphrase, rewrite, expand, or semantically reinterpret the user query;
- normalization must not preserve internal newline structure in the output;
- the output query must be a single canonical string line separated only by ASCII spaces between retained segments;
- token counting must be performed on the final normalized query string, not on the raw input string.

## 7) Token Counting Rule

This module must compute `NormalizedUserRequest.input_token_count` using the tokenizer utility defined in:
- `Specification/runtime/utils/tokenizer.md`

Rules:
- token counting must use the tokenizer loaded from `InputNormalizationSettings.tokenizer_source`;
- token counting must be computed from the normalized query string that will be returned in `NormalizedUserRequest.query`;
- `NormalizedUserRequest.input_token_count` has type `usize`;
- `input_token_count` must equal the `usize` count of token strings produced by the tokenizer utility for that normalized query;
- this module must enforce `InputNormalizationSettings.max_input_tokens` against that computed token count.

## 8) Error Boundary

This module must define a module-owned direct error type equivalent in ownership to:

```rust
pub enum InputNormalizationError {
    EmptyQuery,
    InputTooLong {
        token_count: usize,
        max_input_tokens: usize,
    },
    Tokenizer(#[from] TokenizerError),
}
```

Variant rules:
- `EmptyQuery`
  - must be returned when the normalized query is empty after deterministic normalization, or when the computed token count for the normalized query is zero;
- `InputTooLong { token_count, max_input_tokens }`
  - must be returned when the computed token count for the normalized query exceeds the configured ceiling;
- `Tokenizer(TokenizerError)`
  - must wrap failures surfaced from the tokenizer utility used by this module.

Rules:
- this module must not duplicate the tokenizer utility's internal error variants in its own public error boundary;
- this module must wrap tokenizer utility failure through one typed dependency variant;
- the `Tokenizer(...)` variant must use `#[from]` so constructor code may propagate tokenizer utility failure with `?`;
- this module must not flatten its error boundary into string-only results.

## 9) Behavioral Invariants

The current version of this module must preserve all of the following invariants:

- identical `UserRequest.query` input and identical `InputNormalizationSettings` must produce identical `NormalizedUserRequest` output;
- `NormalizedUserRequest.query` must never contain leading or trailing whitespace;
- `NormalizedUserRequest.query` must never contain adjacent whitespace characters;
- `NormalizedUserRequest.query` must never contain newline characters;
- `NormalizedUserRequest.input_token_count` must be greater than zero on successful output;
- successful output must never exceed `max_input_tokens`.

## 10) Testing Ownership

Unit-test ownership for runtime modules is defined by:
- `Specification/runtime/unit_tests.md`

Required unit-test cases for this module must be defined in:
- `Specification/runtime/unit_tests.md`

Tokenizer utility tests remain defined by:
- `Specification/runtime/utils/tokenizer.md`

## 11) Non-Goals

For the current version, this module must not:
- build structured semantic JSON from the user query;
- call any generation model;
- decide whether semantic query structuring should run;
- access retrieval settings;
- access model transport settings;
- access observability settings directly;
- perform query classification, synonym expansion, or domain-term mapping.
