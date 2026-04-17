## 1) Purpose / Scope

This document defines the shared generated unit-test contract for the runtime code generated from the current specification set.

This document is the single source of truth for:
- shared unit-test generation rules;
- shared test-placement rules;
- shared test-helper expectations;
- shared test-environment rules.

This document does not define:
- module-specific required test cases;
- module-specific payload or request-shape assertions;
- integration or end-to-end test scope outside unit-test generation.

## 2) General Generation Rules

Generated unit tests must satisfy all of the following rules:

- unit tests must be generated in the same generation pass as the runtime implementation;
- unit tests must use the actual function names, type names, field names, and error enum variants present in the generated Rust code;
- unit tests must be deterministic;
- unit tests must not depend on external network access;
- unit tests must not depend on running Docker containers, Qdrant processes, model processes, or external tokenizer registries;
- unit tests must execute locally inside the Rust test process;
- unit tests must assert exact contract-relevant outcomes;
- success-path tests must assert exact returned values and exact request payload structure when request construction is part of the contract;
- failure-path tests must assert the exact returned error variant;
- if a returned error variant contains structured fields, tests must assert the relevant field values;
- required tests must be implemented as executable Rust tests;
- comments, TODO items, prose test plans, pseudo-tests, placeholder test functions without assertions, and empty test modules do not satisfy any required unit-test case from this document;
- tests must not introduce new public runtime APIs solely for testability;
- tests must cover public entrypoints and contract-relevant private helpers;
- tests for HTTP-calling modules must use local mock HTTP servers created inside Rust tests;
- tests for startup/config/artifact-loading logic must use temporary files and temporary directories created inside Rust tests.

## 3) Required Test Placement Rules

- required module unit tests live inline under `#[cfg(test)] mod tests` inside the corresponding Rust module file;
- shared test helpers may live in an internal test-only Rust module under the crate source tree;
- generation is incomplete if a required module test set is replaced by comments, TODO markers, or a non-executable checklist.

## 4) Required Shared Test Helpers

Generated unit tests must include shared helper code that provides all of the following capabilities:

- local HTTP mock-server support that:
  - listens on `127.0.0.1` on an ephemeral port;
  - accepts test requests from the runtime under test;
  - returns preconfigured HTTP responses in deterministic order;
  - records observed request bodies for later assertions;
- temporary-directory support for artifact-loading tests;
- temporary-file support for artifact-loading tests;
- temporary working-directory setup and restoration when relative artifact paths must be resolved from repository root.

The generated test helper code must remain internal to test builds.
The generated production API must not expose test-only helpers.

## 5) Environment-Sensitive HTTP Test Rules

- tests that require local mock HTTP servers are normal non-ignored tests when the execution environment permits loopback socket binding;
- if the execution environment initially forbids loopback socket binding or other capabilities required by generated required tests, the generator or validating agent must request the elevated privileges needed to execute those tests normally;
- required environment-sensitive HTTP tests must remain normal non-ignored tests and must be executed once the required capabilities are granted;
- inability to execute a required test in the current sandbox is not a reason to replace that test with `#[ignore]`;
- `#[ignore]` is forbidden for required core module tests and required environment-sensitive HTTP tests.
