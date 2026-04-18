# Runtime Env Contract

This document defines the contract for the repository-owned runtime `.env` file.

## Format

The runtime `.env` file is a dotenv-like text file with `KEY=value` format.

It is the source of truth for:
- service endpoint URLs;
- connection strings;
- API keys and similar secret-like values;
- observability endpoint values that will be used by later runtime stages.

Rules:
- empty lines are allowed;
- comment lines are allowed;
- a line without `=` is an env-file format error.

Invalid `.env`:
- the `.env` contract is invalid if the file cannot be read, if a line violates `KEY=value` format, or if a required key from this contract is missing;
- this is a startup-time configuration error for runtime initialization.

## Expected Fields

- `QDRANT_URL`: base URL of the Qdrant HTTP API
- `OLLAMA_URL`: base URL of the Ollama HTTP API
- `POSTGRES_URL`: PostgreSQL connection string used by the runtime
- `OPENAI_COMPATIBLE_URL`: base URL of the Together-compatible chat-completions API
- `TOGETHER_API_KEY`: API key used with `OPENAI_COMPATIBLE_URL`
- `RERANKER_ENDPOINT`: reranker endpoint reserved for later runtime stages
- `VOYAGEAI_RERANK_URL`: VoyageAI reranker base URL reserved for later runtime stages
- `VOYAGEAI_API_KEY`: VoyageAI API key reserved for later runtime stages
- `TRACING_ENDPOINT`: OTLP tracing endpoint reserved for observability initialization
- `METRICS_ENDPOINT`: OTLP metrics endpoint reserved for observability initialization
- `RUST_LOG`: runtime log filter string reserved for observability/logging initialization

## Current Runtime Usage

The current runtime startup path consumes directly:
- `QDRANT_URL`
- `OLLAMA_URL`
- `POSTGRES_URL`
- `OPENAI_COMPATIBLE_URL`
- `TOGETHER_API_KEY`

The current spec keeps these keys in the contract but does not yet require runtime execution logic to consume them:
- `RERANKER_ENDPOINT`
- `VOYAGEAI_RERANK_URL`
- `VOYAGEAI_API_KEY`
- `TRACING_ENDPOINT`
- `METRICS_ENDPOINT`
- `RUST_LOG`

## Mapping To Resolved Settings

Required current mapping rules:
- `Settings.retrieval.qdrant_url` <- `QDRANT_URL`
- `Settings.embedding_model.url` <- `OLLAMA_URL`
- `Settings.postgres.url` <- `POSTGRES_URL`
- `Settings.model.transport = ModelTransportSettings::Ollama(OllamaModelSettings { url, .. })` <- `OLLAMA_URL`
- `Settings.model.transport = ModelTransportSettings::Together(TogetherModelSettings { url, .. })` <- `OPENAI_COMPATIBLE_URL`
- `Settings.model.transport = ModelTransportSettings::Together(TogetherModelSettings { api_key, .. })` <- `TOGETHER_API_KEY`
