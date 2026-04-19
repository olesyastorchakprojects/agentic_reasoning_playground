## 1) Purpose

This temporary document enumerates the planned MVP request-pipeline modules and their responsibilities.

The current version is intentionally short.
It exists to anchor the module boundaries before detailed per-module specifications are written.

## 2) Planned Modules

| Module | Responsibility |
| --- | --- |
| `input_normalization` | Transform the raw user query into a deterministic normalized query string and enforce input validation and token-ceiling rules. |
| `query_structuring` | Convert the normalized user query into a structured domain JSON representation using system terminology such as symptoms, affected subsystems, and other normalized fields. |
| `candidate_card_retrieval` | Retrieve candidate cards from Qdrant and classify them into `primary` and `alternatives` using the retrieval score rule. |
| `card_hydration` | Load full card records from Postgres for the selected candidate card identifiers returned by retrieval. |
| `incident_evidence_retrieval` | Retrieve incident-report evidence chunks linked to the selected cards by card ids and/or card-linked metadata. |
| `theory_evidence_retrieval` | Retrieve theory chunks independently from the theory corpus using the normalized query and retrieval policy. |
| `prompt_context_assembly` | Combine normalized input, hydrated cards, and retrieved evidence into a structured prompt context for model generation. |
| `llm_structured_generation` | Call the model with the structured prompt context and require a strict JSON output contract. |
| `response_validation_and_normalization` | Validate the model JSON output against schema and business rules, then normalize it into the trusted final response shape. |

## 3) Notes

- Candidate partitioning is currently treated as part of `candidate_card_retrieval`, not as a separate top-level module.
- Leaf module specifications define module-local contracts only; orchestration policy such as whether or when a module is invoked belongs to the orchestrator layer rather than these module documents.
- This file is temporary and is expected to be replaced or complemented by dedicated specifications for each module.
