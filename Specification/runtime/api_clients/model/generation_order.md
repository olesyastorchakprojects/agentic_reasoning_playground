## 1) Purpose / Scope

This document defines the recommended code-generation order for the runtime model-client specification set.

It exists to make generation dependency-aware.

This document defines:
- which specification files belong to the runtime model-client slice;
- what code artifacts should be generated from them;
- the recommended generation order;
- when unit-test generation should run.

## 2) Model-Client Spec Set

The current runtime model-client specification set consists of:

- `Specification/runtime/api_clients/client_common.md`
- `Specification/runtime/api_clients/model/shared_types.md`
- `Specification/runtime/api_clients/model/model_client.md`
- `Specification/runtime/api_clients/model/openai_client.md`
- `Specification/runtime/api_clients/model/ollama_client.md`
- `Specification/runtime/unit_tests_common.md`
- `Specification/runtime/api_clients/model/unit_tests.md`

## 3) Target Generated Files

The runtime model-client slice should generate code for these Rust files first:

- `distributed_diagnostics::api_clients::model::shared_types`
- `distributed_diagnostics::api_clients::model::model_client`
- `distributed_diagnostics::api_clients::model::openai_client`
- `distributed_diagnostics::api_clients::model::ollama_client`

The current version does not require generating dedicated Rust modules from:
- `client_common.md`
- `unit_tests_common.md`
- `model/unit_tests.md`

Those files are dependency and behavior contracts used while generating the modules above.

## 4) Recommended Generation Order

Generate in this order:

1. `shared_types`
2. `model_client`
3. `openai_client`
4. `ollama_client`
5. generate unit tests for all modules above using:
   - `Specification/runtime/unit_tests_common.md`
   - `Specification/runtime/api_clients/model/unit_tests.md`

## 5) Generation Rules

Rules:
- generation must respect type dependencies from earlier steps;
- concrete client modules must import generated shared and trait modules, not regenerate duplicate local types;
- behavior-only spec files must be treated as source-of-truth references during generation, not as standalone module outputs;
- unit tests must be generated only after the corresponding runtime modules already exist.
