# Testing

## Postgres integration tests

`run_state_store` and `incident_card_store` integration tests are separated from regular unit tests and use a dedicated test database.

The test suite reads `TEST_DATABASE_URL` from:

1. process environment;
2. `Execution/distributed_diagnostics/.env.test`;
3. `.env.test` in the current working directory.

Recommended setup:

1. Create and initialize the dedicated test database:

```bash
Execution/distributed_diagnostics/scripts/setup_test_database.sh
```

2. Create a local env file from the example:

```bash
cp Execution/distributed_diagnostics/.env.test.example Execution/distributed_diagnostics/.env.test
```

3. Run the Postgres integration suites:

```bash
cargo test --manifest-path Execution/distributed_diagnostics/Cargo.toml --features postgres-integration --test run_state_store_postgres
cargo test --manifest-path Execution/distributed_diagnostics/Cargo.toml --features postgres-integration --test incident_card_store_postgres
```

The tests require `TEST_DATABASE_URL` to point to the dedicated `distributed_diagnostics_test` database and will fail fast if another database name is used.

## Live retrieval linkage test

`retrieval_linkage_live` is a read-only live integration test for the retrieval
chain:

- `CandidateCardRetrieval`
- `CardHydration`
- `IncidentEvidenceRetrieval`

It uses:

- `POSTGRES_URL` for live Postgres incident cards;
- `QDRANT_URL` for live Qdrant collections;
- `OLLAMA_URL` for live embedding generation used by the Qdrant-backed modules.

The test loads real settings from:

- `Execution/distributed_diagnostics/runtime.toml`
- `Execution/distributed_diagnostics/ingest.toml`

and will fail fast if any of those live dependencies are unavailable.

## Commands

Regular test suite without Postgres integration tests:

```bash
cargo test --manifest-path Execution/distributed_diagnostics/Cargo.toml
```

Only `run_state_store` Postgres integration tests:

```bash
cargo test --manifest-path Execution/distributed_diagnostics/Cargo.toml --features postgres-integration --test run_state_store_postgres
```

Only `incident_card_store` Postgres integration tests:

```bash
cargo test --manifest-path Execution/distributed_diagnostics/Cargo.toml --features postgres-integration --test incident_card_store_postgres
```

Only the live retrieval linkage test:

```bash
cargo test --manifest-path Execution/distributed_diagnostics/Cargo.toml --features postgres-integration --test retrieval_linkage_live
```

All Postgres integration tests:

```bash
cargo test --manifest-path Execution/distributed_diagnostics/Cargo.toml --features postgres-integration --tests
```
