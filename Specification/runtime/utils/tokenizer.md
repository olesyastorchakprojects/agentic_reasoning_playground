## 1) Purpose / Scope

This document defines the runtime utility contract for `src/utils/tokenizer.rs`.

This utility exists to:
- load a Hugging Face tokenizer artifact for runtime use;
- reuse a local tokenizer cache when available;
- download and cache `tokenizer.json` when the configured tokenizer is not yet cached;
- expose tokenization that strips common subword marker prefixes from emitted token strings.

This document is the source of truth for:
- `src/utils/tokenizer.rs`
- tokenizer cache-path rules;
- tokenizer download and validation behavior;
- token marker stripping behavior;
- utility-owned error boundary;
- tokenizer utility unit-test expectations.

This document does not define:
- crate-level settings construction;
- raw TOML or environment loading rules;
- request-level input normalization behavior;
- sparse-vector weighting or vocabulary lookup;
- domain-specific token filtering such as lowercasing, minimum token length, or dropping non-alphanumeric tokens.

## 2) Public Boundary

The generated Rust utility module must define a public tokenizer wrapper equivalent in ownership to:

```rust
pub struct HfTokenizer {
    // implementation-owned tokenizer instance
}

impl HfTokenizer {
    pub async fn load(source: &str) -> Result<Self, TokenizerError>;

    pub fn tokenize(&self, text: &str) -> Vec<String>;
}
```

The generated Rust utility module must define a public utility-owned error type equivalent in ownership to:

```rust
pub enum TokenizerError {
    Load { path: String, reason: String },
    Download { url: String, reason: String },
    InvalidJson { reason: String },
    MissingModelField,
    CacheWrite { path: String, reason: String },
}
```

Rules:
- `HfTokenizer` is a reusable utility boundary, not a domain-specific module;
- callers are responsible only for supplying the tokenizer `source`;
- this utility must not depend on crate-level `Settings` directly;
- this utility must not own request-level normalization policy.

## 3) Loading Contract

`HfTokenizer::load(source)` must treat:
- `source` as a Hugging Face repository identifier.

For the current version, the utility must resolve:
- tokenizer cache path:
  - `artifacts/tokenizers/{source}/tokenizer.json`
- download URL:
  - `https://huggingface.co/{source}/resolve/main/tokenizer.json`

Loading rules:
- if the cache file exists at the resolved cache path, the utility must load it directly from disk;
- if the cache file does not exist, the utility must download tokenizer bytes from the resolved Hugging Face URL;
- downloaded bytes must be validated before they are accepted;
- after successful validation, the utility must create parent cache directories as needed;
- after successful validation, the utility must write the downloaded bytes to the resolved cache path;
- after successful download and validation, the tokenizer must be constructed from the downloaded bytes;
- successful download must make the tokenizer available for later cache-based reuse.

The utility must not:
- derive model names from other runtime state;
- perform ad hoc search across multiple cache locations;
- require callers to pass a cache directory or full download URL;
- mutate downloaded tokenizer JSON before parsing.

## 4) Download Behavior

When a tokenizer artifact is not present in the local cache, the utility must download `tokenizer.json` over HTTP(S).

For the current version:
- download timeout is `10` seconds;
- maximum attempts is `3`;
- retry backoff must be exponential with jitter;
- a non-success HTTP status must fail the download step;
- transport failures must fail the download step.

Rules:
- download retry policy is utility-owned behavior and is not currently caller-configurable;
- retry attempts apply only to the download path, not to cache-file reads;
- download behavior must remain deterministic with respect to the resolved URL and cache path.

## 5) JSON Validation Contract

Downloaded tokenizer bytes must be validated before caching and parsing.

Validation rules:
- bytes must parse as valid JSON;
- the top-level JSON object must contain a `model` field;
- if JSON parsing fails, the utility must fail with `TokenizerError::InvalidJson`;
- if the `model` field is absent, the utility must fail with `TokenizerError::MissingModelField`.

The utility does not currently validate:
- tokenizer vocabulary size;
- tokenizer revision metadata;
- tokenizer special-token semantics;
- compatibility with any particular embedding or chat model beyond successful parsing by the tokenizer library.

## 6) Tokenization Contract

`HfTokenizer::tokenize(text)` must:
- encode the supplied text with the loaded tokenizer;
- read the emitted token strings from the tokenizer encoding;
- strip common subword marker prefixes from each emitted token string;
- return token strings in left-to-right order.

For the current version, the utility must strip:
- GPT-2-style prefix: `Ġ`
- SentencePiece-style prefix: `▁`
- WordPiece-style prefix: `##`

Rules:
- marker stripping is applied token-by-token to the emitted token strings;
- token order must be preserved;
- the utility returns raw stripped token strings only;
- the utility must not lowercase tokens;
- the utility must not filter short tokens;
- the utility must not drop non-alphanumeric tokens;
- the utility must not drop unknown placeholder tokens;
- any such filtering or normalization beyond marker stripping is owned by downstream modules.

If the underlying tokenizer fails to encode the supplied text:
- `tokenize(text)` must return an empty vector in the current version.

## 7) Helper Rules

The utility may define implementation-private helpers for:
- building the Hugging Face download URL;
- resolving the tokenizer cache path;
- validating downloaded tokenizer JSON;
- constructing the download retry backoff;
- stripping tokenizer marker prefixes.

These helpers are implementation details and must not become separate public runtime module boundaries in the current version.

## 8) Error Boundary

`TokenizerError` is the utility-owned direct error type for this module.

Variant semantics:
- `Load { path, reason }`
  - loading or parsing a tokenizer artifact failed after a local file path or downloaded bytes had already been selected for parsing;
- `Download { url, reason }`
  - download transport setup, request execution, non-success HTTP status, or response-body retrieval failed;
- `InvalidJson { reason }`
  - downloaded tokenizer bytes were not valid JSON;
- `MissingModelField`
  - downloaded tokenizer JSON lacked the required top-level `model` field;
- `CacheWrite { path, reason }`
  - creating cache directories or writing the cache file failed.

Rules:
- callers must not depend on private helper failures that are not represented through `TokenizerError`;
- parent modules that use this utility may wrap `TokenizerError` through their own typed error boundary;
- parent module specifications must not duplicate the full internal behavior of this utility.

## 9) Dependency Rules

The tokenizer utility depends on:
- the Rust `tokenizers` crate for tokenizer parsing and encoding;
- `reqwest` for download transport;
- `backon` for retry behavior;
- `serde_json` for JSON validation.

Rules:
- this utility must not depend on Python runtime components;
- this utility must not read raw TOML or raw environment variables directly;
- this utility must not depend on crate-level startup orchestration.

## 10) Unit Test Requirements

Generated unit tests for `utils.tokenizer` must include all of the following cases:

- marker stripping removes the GPT-2 `Ġ` prefix;
- marker stripping removes the SentencePiece `▁` prefix;
- marker stripping removes the WordPiece `##` prefix;
- plain tokens remain unchanged when no supported marker is present;
- cache path resolution uses `artifacts/tokenizers/{source}/tokenizer.json`;
- loading succeeds from cache when the cache file already exists;
- loading downloads, validates, and caches a tokenizer artifact when the cache file does not exist;
- loading returns `TokenizerError::Download` on HTTP failure;
- JSON validation accepts valid tokenizer JSON that contains a top-level `model` field;
- JSON validation returns `TokenizerError::MissingModelField` when the `model` field is absent;
- JSON validation returns `TokenizerError::InvalidJson` for non-JSON bytes;
- successful tokenization returns stripped tokens in left-to-right order.

Unit tests for this utility must not require:
- a running external Hugging Face service;
- a live model process;
- Docker containers;
- crate-level runtime settings construction.

## 11) Related Contracts

This utility is referenced by:
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_text_space.md`
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_vocabulary.md`
- `Specification/runtime/api_clients/qdrant/hybrid/sparse_query_preparation.md`

Future request-pipeline modules may also depend on this utility through their own module-owned settings slices rather than through crate-level `Settings`.
