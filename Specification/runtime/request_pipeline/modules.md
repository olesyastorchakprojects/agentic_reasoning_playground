## 1) Purpose

This temporary document enumerates the planned MVP request-pipeline modules and their responsibilities.

The current version is intentionally short.
It exists to anchor the module boundaries before detailed per-module specifications are written.

## 2) Planned Modules

| Module | Responsibility |
| --- | --- |
| `input_normalization` | Transform the raw user query into a deterministic normalized query string and enforce input validation and token-ceiling rules. |
| `query_structuring` | Convert the normalized user query into a structured domain JSON representation using system terminology such as symptoms, affected subsystems, and other normalized fields. |
| `information_adequacy_analyzer` | Deterministically assess whether a structured initial request or structured observation contains enough information to proceed, and produce canonical follow-up questions when it does not. |
| `observation_extraction` | Accept a supported context-resolved observation from `observation_boundary_resolver`, extract one or more atomic diagnostic observations, and assess whether more context is required before downstream diagnostic update. |
| `candidate_card_retrieval` | Retrieve candidate cards from Qdrant and classify them into `primary` and `alternatives` using the retrieval score rule. |
| `card_branch_reranking` | Re-rank the fresh candidate-card set against prior card-selection history and produce the current `primary` and `alternatives` branches. |
| `card_hydration` | Load full card records from Postgres for the selected candidate card identifiers returned by retrieval. |
| `incident_evidence_retrieval` | Retrieve incident-report evidence chunks linked to the selected cards by card ids and/or card-linked metadata. |
| `theory_evidence_retrieval` | Retrieve theory chunks independently from the theory corpus using the normalized query and retrieval policy. |
| `prompt_context_assembly` | Combine normalized input, hydrated cards, and retrieved evidence into a structured prompt context for model generation. |
| `llm_structured_generation` | Call the model with the structured prompt context and require a strict JSON output contract. |
| `response_validation_and_normalization` | Validate the model JSON output against schema and business rules, then normalize it into the trusted final response shape. |
| `observation_boundary_resolver` | Decide whether a continuation input can safely be treated as a new diagnostic observation and, when safe, rewrite it into a standalone context-resolved observation. |

## 3) Notes

- Candidate partitioning is currently treated as part of `candidate_card_retrieval`, not as a separate top-level module.
- Leaf module specifications define module-local contracts only; orchestration policy such as whether or when a module is invoked belongs to the orchestrator layer rather than these module documents.
- This file is temporary and is expected to be replaced or complemented by dedicated specifications for each module.
