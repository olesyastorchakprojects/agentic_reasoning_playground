# First Iteration Entities and Data Model

This document defines the core entities, data types, and interaction model for the first implementation iteration of the distributed-systems diagnostic assistant.

The goal is:

- to implement the smallest useful system slice now;
- to keep the design compatible with future multi-step diagnostic orchestration;
- to avoid a rewrite when the project grows from first response to a fuller diagnostic loop.

This document builds on:

- `AI_evolution_distributed_systems_agentic_rag.md`
- `implementation_foundation_first_response_and_evidence_packing.md`
- `card_retrieval_architecture_qdrant_ranking_postgres_canonical.md`
- `agentic_rag_ascii_architecture_evolution_v2.md`

---

## 1. Design principles

The entity model should follow these rules:

1. Separate canonical state from retrieval/index state.
2. Separate structured reasoning objects from raw text evidence.
3. Make first-response entities directly reusable in later multi-step diagnosis.
4. Keep first-iteration data structures small, but shape them so they can be extended rather than replaced.
5. Treat orchestration as a state machine over typed step results, even if the first implementation only uses a small subset of steps.

Short version:

```text
Postgres stores truth.
Qdrant stores retrieval projections.
Chunks store evidence.
RunState stores the evolving investigation state.
```

---

## 2. Entity groups

The first iteration needs five groups of entities:

1. Input and normalization entities
2. Card and corpus entities
3. Retrieval and evidence entities
4. Diagnostic reasoning entities
5. Orchestration and runtime entities

---

## 3. Input and normalization entities

These entities turn raw user language into a usable investigation input.

### 3.1. `RawUserProblem`

Purpose:

- exact user input as received by the system

Suggested shape:

```json
{
  "request_id": "uuid",
  "session_id": "uuid | null",
  "user_text": "string",
  "received_at": "timestamp",
  "context": {
    "system_name": "string | null",
    "environment": "string | null",
    "user_metadata": "object | null"
  }
}
```

Why it matters:

- immutable source record
- debugging normalization and retrieval later
- linking later observations to the original case

---

### 3.2. `NormalizedIncidentQuery`

Purpose:

- structured interpretation of the user problem
- auxiliary reasoning object, not the mandatory replacement for raw user text

Suggested first-iteration shape:

```json
{
  "recognized_canonical_symptoms": ["string"],
  "unmapped_user_symptoms": ["string"],
  "affected_components": ["string"],
  "failure_mode_candidates": ["string"],
  "observed_phase": ["string"],
  "signals_present": ["string"],
  "missing_signals": ["string"]
}
```

Why it matters:

- helps explain how the system understood the problem
- helps prompt construction and transparency
- can optionally enrich retrieval
- can be reused in later diagnostic steps without redesign

Important clarification:

- for semantic card retrieval, `RawUserProblem.user_text` should remain the primary retrieval input;
- `NormalizedIncidentQuery` should not over-constrain or replace the raw problem text;
- it should assist retrieval and reasoning, not become a hard gate before candidate generation.

Recommended first-iteration use:

```text
RawUserProblem.user_text
  -> main input for semantic card retrieval

NormalizedIncidentQuery
  -> optional retrieval hint
  -> explanation layer
  -> prompt context
  -> later observation / hypothesis-update support
```

Future-safe extension:

- add confidence per field
- add extracted timelines
- add structured environment clues
- add user-goal or urgency hints

---

## 4. Card and corpus entities

These entities are the structured memory layer of the system.

---

### 4.1. `IncidentCard`

Purpose:

- canonical structured representation of one practical precedent

Canonical storage:

- `Postgres`

Why:

- cards are reasoning objects, not just retrieval documents
- cards evolve
- cards need structured maintenance and possible schema migrations

Current recommended shape:

```json
{
  "case_id": "string",
  "title": "string",
  "source_type": "string",
  "source_name": "string",
  "source_path": "string",
  "vendor_or_project": "string",
  "system_type": "string",
  "version_tested": "string | null",
  "report_date": "date | null",
  "short_summary": "string",
  "canonical_symptoms": ["string"],
  "affected_components": ["string"],
  "failure_mode_candidates": ["string"],
  "observed_phases": ["string"],
  "incident_phases": [
    {
      "phase_name": "string",
      "context": "string",
      "symptoms": ["string"],
      "user_visible_impact": ["string"],
      "observations": ["string"],
      "actions_taken": ["string"],
      "changes_after_actions": ["string"]
    }
  ],
  "turning_points": ["string"],
  "candidate_explanations": ["string"],
  "diagnostic_patterns": ["string"],
  "discriminating_checks": [
    {
      "question": "string",
      "why": "string"
    }
  ],
  "expected_observations": [
    {
      "observation": "string",
      "effect": "string"
    }
  ],
  "investigation_steps": ["string"],
  "root_cause_summary": "string",
  "reasoning_summary": "string",
  "mitigations_or_workarounds": ["string"],
  "prevention_or_design_followups": ["string"],
  "claimed_guarantees": ["string"],
  "violated_properties": ["string"],
  "resolution_status": "string | null",
  "fix_versions": ["string"],
  "confidence_notes": ["string"],
  "source_refs": ["string"]
}
```

Why this should exist already in iteration 1:

- it gives stable diagnostic structure;
- it lets chunks play a supporting role instead of carrying all reasoning burden;
- it is compatible with future multi-step updates.

---

### 4.2. `CardRetrievalProjection`

Purpose:

- retrieval-friendly representation of the card
- optimized for ranking candidate cards

Primary store:

- `Qdrant`

Suggested shape:

```json
{
  "case_id": "string",
  "title": "string",
  "short_summary": "string",
  "canonical_symptoms": ["string"],
  "candidate_explanations": ["string"],
  "diagnostic_patterns": ["string"],
  "phase_summary": ["string"],
  "embedding_text": "string",
  "metadata": {
    "source_type": "string",
    "vendor_or_project": "string",
    "system_type": "string"
  }
}
```

Why this should be separate from `IncidentCard`:

- retrieval representation will change;
- embeddings will be reindexed;
- canonical card body should remain stable.

Future-safe extension:

- multiple projections per card
- multiple embedding models
- sparse + dense retrieval metadata

---

### 4.3. `SourceDocument`

Purpose:

- metadata about the original report/postmortem/analysis

Suggested shape:

```json
{
  "document_id": "string",
  "corpus_kind": "practice | theory",
  "title": "string",
  "source_path": "string",
  "source_url": "string | null",
  "document_format": "pdf | md | html | txt",
  "tags": ["string"],
  "ingest_version": "string"
}
```

Why it matters:

- provenance
- rebuild chunk projections
- traceability from cards and chunks back to source

---

## 5. Retrieval and evidence entities

These entities represent what retrieval found and what was packed for the model.

---

### 5.1. `EvidenceChunk`

Purpose:

- one chunk from theory or practice corpus

Primary store:

- `Qdrant` for retrieval
- original chunk record may also exist in file/object storage pipelines

Suggested shape:

```json
{
  "chunk_id": "string",
  "document_id": "string",
  "corpus_kind": "practice | theory",
  "document_title": "string",
  "section_title": "string | null",
  "text": "string",
  "tags": ["string"],
  "metadata": {
    "chunking_strategy": "string | null",
    "vendor_or_project": "string | null"
  }
}
```

Current crucial idea:

- tags are not just descriptive metadata;
- they help assign a role to the chunk in the final prompt.

---

### 5.2. `CardCandidate`

Purpose:

- one ranked card candidate returned by card retrieval

Suggested shape:

```json
{
  "case_id": "string",
  "score": "number",
  "rank": "integer",
  "retrieval_source": "qdrant_card_index",
  "retrieval_explanation": {
    "matched_symptoms": ["string"],
    "notes": ["string"]
  }
}
```

Why it matters:

- supports primary vs competing card choice
- allows ambiguity-aware ranking
- can influence whether `competing_interpretation` is mandatory

---

### 5.3. `ChunkCandidate`

Purpose:

- one candidate chunk before final packing

Suggested shape:

```json
{
  "chunk_id": "string",
  "document_id": "string",
  "score": "number",
  "rank": "integer",
  "tags": ["string"],
  "candidate_role": "string | null"
}
```

This can stay lightweight.

It mainly exists to support pack assembly.

---

### 5.4. `EvidencePack`

Purpose:

- compact final chunk set sent to the model

Suggested shape:

```json
{
  "primary_card_case_id": "string",
  "competing_card_case_ids": ["string"],
  "practical_chunks": [
    {
      "role": "evidence_for_match | first_check_hint | alternative_context",
      "source_case_id": "string | null",
      "chunk_id": "string",
      "text": "string"
    }
  ],
  "theory_chunks": [
    {
      "role": "mechanism_explanation",
      "chunk_id": "string",
      "text": "string"
    }
  ]
}
```

First-iteration packing rule:

```text
1 primary card
+ 2 chunks from primary card
+ 1 chunk from each of up to 2 competing cards
+ 0-1 theory chunk
```

Why this entity matters:

- it captures the tested packing logic explicitly;
- later prompt construction should consume this object, not reconstruct it ad hoc.

---

## 6. Diagnostic reasoning entities

These entities represent the evolving reasoning state, not just retrieval output.

---

### 6.1. `DiagnosticHypothesis`

Purpose:

- one active or historical explanation in the investigation

Suggested first-iteration shape:

```json
{
  "hypothesis_id": "string",
  "claim": "string",
  "status": "active | weakened | strengthened | eliminated",
  "confidence": "low | medium | high",
  "supporting_evidence": ["string"],
  "contradicting_evidence": ["string"],
  "source_refs": ["string"]
}
```

Why this should exist early:

- even the first response already produces an implicit hypothesis set;
- later loop steps need a stable hypothesis object to update.

Future-safe extension:

- add links to checks
- add expected observations
- add explanation lineage by step

---

### 6.2. `DiscriminatingCheck`

Purpose:

- one next check that helps separate active hypotheses

Suggested shape:

```json
{
  "check_id": "string",
  "question": "string",
  "why_this_check": "string",
  "requested_data": ["string"],
  "result_interpretation": {
    "supports_primary_if": "string",
    "supports_competing_if": "string",
    "inconclusive_if": "string | null"
  }
}
```

Why it matters:

- “one next check” is central to the product behavior;
- this should be a first-class object, not just a text fragment.

---

### 6.3. `DiagnosticObservation`

Purpose:

- structured record of what the user observed after a check

Suggested shape:

```json
{
  "observation_id": "string",
  "source_check_id": "string | null",
  "user_text": "string",
  "normalized_observation": {
    "signals_present": ["string"],
    "signals_absent": ["string"],
    "measurements": ["string"],
    "notes": ["string"]
  }
}
```

Why this should exist in iteration 1:

- update-step prompts already depend on user observation;
- it will later become central to multi-step state transitions.

---

### 6.4. `HypothesisUpdate`

Purpose:

- result of interpreting one observation against current hypotheses

Suggested shape:

```json
{
  "strengthened": ["string"],
  "weakened": ["string"],
  "still_active": ["string"],
  "notes": ["string"]
}
```

This is intentionally simple at first.

Later it can reference hypothesis IDs directly.

---

### 6.5. `FirstResponse`

Purpose:

- first model-generated user-facing diagnostic frame

Suggested shape:

```json
{
  "problem_understanding": "string",
  "similar_practical_context": "string",
  "active_hypotheses": ["string"],
  "first_check": "string",
  "result_interpretation": {
    "supports_primary_if": "string",
    "supports_competing_if": "string",
    "inconclusive_if": "string | null"
  },
  "competing_interpretation": "string | null"
}
```

Why it matters:

- it is both machine-readable and user-facing;
- it is the entry point into the later diagnostic loop.

---

### 6.6. `DiagnosticUpdateResponse`

Purpose:

- model-generated response after one observation

Suggested shape:

```json
{
  "updated_problem_understanding": "string",
  "hypothesis_update": {
    "strengthened": ["string"],
    "weakened": ["string"],
    "still_active": ["string"]
  },
  "next_check": "string",
  "why_this_check_now": "string",
  "result_interpretation": {
    "supports_primary_if": "string",
    "supports_competing_if": "string",
    "inconclusive_if": "string | null"
  }
}
```

This shape was already manually exercised in multi-step tests.

---

## 7. Orchestration and runtime entities

These entities are what make future growth organic instead of rewrite-heavy.

The first implementation may not need a full orchestrator, but it should align with one.

---

### 7.1. `RunState`

Purpose:

- canonical evolving state of one diagnostic session

Suggested shape:

```json
{
  "run_id": "uuid",
  "session_id": "uuid | null",
  "raw_user_problem": "RawUserProblem",
  "normalized_query": "NormalizedIncidentQuery | null",
  "card_candidates": ["CardCandidate"],
  "primary_card_case_id": "string | null",
  "competing_card_case_ids": ["string"],
  "evidence_pack": "EvidencePack | null",
  "active_hypotheses": ["DiagnosticHypothesis"],
  "last_check": "DiscriminatingCheck | null",
  "observations": ["DiagnosticObservation"],
  "step_history": ["StepResult"],
  "status": "running | awaiting_user | finalized | low_confidence_stop"
}
```

Why this is the critical compatibility entity:

- first response can be produced from it;
- later multi-step updates can mutate it;
- a future orchestrator can route purely by inspecting it.

---

### 7.2. `StepType`

Purpose:

- enum-like label for what kind of step the runtime is currently executing

Suggested first version:

```text
normalize_problem
retrieve_cards
select_cards
hydrate_cards
retrieve_chunks
pack_evidence
generate_first_response
intake_observation
update_hypotheses
select_next_check
finalize_response
```

Future-safe extension:

```text
retrieve_more_theory
retrieve_more_practice_cases
ask_for_missing_data
plan_mitigation
stop_due_to_low_confidence
```

---

### 7.3. `StepResult`

Purpose:

- typed result of one runtime step

Suggested shape:

```json
{
  "step_id": "uuid",
  "step_type": "string",
  "status": "success | failure | awaiting_user",
  "summary": "string",
  "output_ref": "string | null",
  "created_at": "timestamp"
}
```

This can remain generic as long as real typed payloads live elsewhere.

---

### 7.4. `TransitionPolicy`

Purpose:

- decide what step should happen next based on current `RunState`

Why it should exist conceptually already:

- even if iteration 1 uses a simpler flow, this abstraction prevents future rewrite;
- the system can begin with a simple deterministic policy and evolve into a more flexible orchestrator.

Conceptual interface:

```text
TransitionPolicy:
  input  = RunState
  output = Next StepType
```

Possible future decisions:

```text
retrieve_more_theory
retrieve_more_practice_cases
ask_for_missing_data
select_discriminating_check
interpret_observation
update_hypotheses
plan_mitigation
finalize_report
stop_due_to_low_confidence
```

First-iteration practical version:

- deterministic;
- small;
- still expressed as a policy boundary.

---

### 7.5. `StepExecutor`

Purpose:

- execute one chosen step against `RunState`

Conceptual interface:

```text
StepExecutor:
  input  = (StepType, RunState)
  output = StepResult + RunState update
```

Why this abstraction matters early:

- separates orchestration from execution;
- lets early steps stay simple and imperative;
- avoids hard-coding the entire future loop into one large function.

---

## 8. Recommended storage split

This split looks strongest for iteration 1 and beyond:

```text
Postgres
  - IncidentCard
  - SourceDocument metadata
  - optionally RunState / step history

Qdrant
  - CardRetrievalProjection
  - EvidenceChunk retrieval records

Filesystem / object storage / source repo
  - raw documents
  - parser outputs
  - intermediate artifacts
```

Why this is future-safe:

- cards can evolve without changing retrieval index structure;
- retrieval index can be rebuilt without moving canonical content;
- embeddings become disposable;
- card schema remains canonical and maintainable.

---

## 9. Minimum first-iteration interaction model

Here is the practical runtime slice that can be built now without overcommitting to a full orchestrator:

```text
RawUserProblem
  -> optional NormalizedIncidentQuery
  -> CardCandidate[]
  -> IncidentCard(primary + competing)
  -> EvidencePack
  -> FirstResponse
```

Then extend into:

```text
DiagnosticObservation
  -> HypothesisUpdate
  -> DiscriminatingCheck
  -> DiagnosticUpdateResponse
```

This is the smallest useful loop that still remains compatible with:

- `RunState`
- `TransitionPolicy`
- `StepExecutor`

---

## 10. Recommended Rust project layout

The implementation is expected to be in Rust.

That means the project layout should favor:

- stable shared types as `struct`s and `enum`s;
- clear module boundaries for external access, orchestration, and business logic;
- strongly typed JSON schemas via `serde`.

For the first iteration, one crate is enough.

Current recommendation:

- use one crate;
- split by modules inside `src/`;
- do not introduce multiple crates too early.

One practical Rust-first layout:

```text
dist_sys_assistant/
|
+-- docs/
|   +-- architecture/
|   +-- experiments/
|   +-- design/
|
+-- src/
|   +-- app/
|   |   +-- mod.rs
|   |
|   +-- types/
|   |   +-- mod.rs
|   |   +-- raw_user_problem.rs
|   |   +-- normalized_incident_query.rs
|   |   +-- incident_card.rs
|   |   +-- card_candidate.rs
|   |   +-- evidence_chunk.rs
|   |   +-- evidence_pack.rs
|   |   +-- diagnostic_hypothesis.rs
|   |   +-- discriminating_check.rs
|   |   +-- diagnostic_observation.rs
|   |   +-- first_response.rs
|   |   +-- diagnostic_update_response.rs
|   |   +-- run_state.rs
|   |   +-- step_type.rs
|   |   +-- step_result.rs
|   |
|   +-- api_clients/
|   |   +-- mod.rs
|   |   +-- postgres.rs
|   |   +-- qdrant.rs
|   |   +-- llm.rs
|   |
|   +-- orchestrator/
|   |   +-- mod.rs
|   |   +-- transition_policy.rs
|   |   +-- step_executor.rs
|   |   +-- steps.rs
|   |
|   +-- domain_logic/
|   |   +-- mod.rs
|   |   +-- cards.rs
|   |   +-- chunks.rs
|   |   +-- theory.rs
|   |   +-- prompts.rs
|   |   +-- responses.rs
|   |   +-- updates.rs
|   |
|   +-- lib.rs
|   +-- main.rs
|
+-- migrations/
+-- practice_corpus/
+-- retrieve/
+-- manual_test_runs/
+-- scripts/
+-- tests/
```

This exact folder structure is not mandatory, but the current intended boundary choices are:

- one crate;
- one shared `types` module;
- one `api_clients` boundary for external systems;
- one `orchestrator` module for session-step control;
- one `domain_logic` module for project-specific meaning.

---

## 11. Recommended modules

These are the core first-iteration modules and their responsibilities.

### `types`

Responsibility:

- stable shared types
- long-lived `struct`s and `enum`s
- serialization contracts
- types reused across the rest of the crate

Rust guidance:

- prefer plain `struct`s for core entities
- prefer `enum`s for state variants like `StepType`
- derive `Serialize`, `Deserialize`, `Clone`, `Debug` where appropriate
- keep these types independent from client-specific request/response shapes

### `api_clients`

Responsibility:

- clients for external services
- request/response handling
- auth / retry / transport details

Rust guidance:

- keep service-specific details local to these modules
- do not let business logic spread here

### `normalization`

Responsibility:

- optional structured interpretation of problems and observations
- must assist retrieval and reasoning, not replace raw user input

### `domain_logic`

Responsibility:

- project-specific business logic
- card retrieval orchestration
- chunk retrieval orchestration
- theory retrieval orchestration
- evidence-pack construction
- prompt context building
- response interpretation and update logic

Subareas that can live inside `domain_logic`:

- `cards`
- `chunks`
- `theory`
- `prompts`
- `responses`
- `updates`

### `retrieval`

Responsibility:

- retrieve ranked card candidates
- retrieve practical chunks
- retrieve optional theory chunks

Rust guidance:

- this responsibility can live inside `domain_logic`
- retrieval policy belongs here more than in client modules

### `packing`

Responsibility:

- choose chunk roles
- build compact evidence packs
- enforce small budget

### `prompting`

Responsibility:

- convert `RunState` or partial state into prompt context
- keep response schemas stable

### `generation`

Responsibility:

- call the model
- parse and validate structured responses

### `orchestration`

Responsibility:

- decide next step
- execute step
- update runtime state

Rust guidance:

- keep `TransitionPolicy` and `StepExecutor` as explicit abstractions
- in the first iteration they can be simple module-level components
- no need to over-engineer them into a large trait hierarchy yet
- prefer typed step outputs over loosely structured maps

### `app`

Responsibility:

- crate composition root
- dependency wiring
- startup flow

---

## 12. Entity/module interaction diagram

The first-iteration interaction model can be visualized like this:

```text
                           +----------------------+
                           |   RawUserProblem     |
                           +----------+-----------+
                                      |
                                      v
                           +----------------------+
                           |  normalization       |
                           |----------------------|
                           | problem_normalizer   |
                           +----------+-----------+
                                      |
                   +------------------+------------------+
                   |                                     |
                   v                                     v
      +-----------------------------+      +-----------------------------+
      | RawUserProblem.user_text    |      | NormalizedIncidentQuery     |
      | primary semantic input      |      | auxiliary reasoning input   |
      +---------------+-------------+      +---------------+-------------+
                      |                                    |
                      +------------------+-----------------+
                                         |
                                         v
                           +------------------------------+
                           | retrieval.card_retriever     |
                           +---------------+--------------+
                                           |
                                           v
                           +------------------------------+
                           | CardCandidate[]              |
                           +---------------+--------------+
                                           |
                                           v
                           +------------------------------+
                           | storage.postgres             |
                           | card_repository              |
                           +---------------+--------------+
                                           |
                                           v
                           +------------------------------+
                           | IncidentCard(primary+comp.)  |
                           +---------------+--------------+
                                           |
                      +--------------------+---------------------+
                      |                                          |
                      v                                          v
        +------------------------------+          +------------------------------+
        | retrieval.chunk_retriever    |          | retrieval.theory_retriever   |
        +---------------+--------------+          +---------------+--------------+
                        |                                         |
                        v                                         v
              +-------------------+                    +-------------------+
              | ChunkCandidate[]  |                    | theory chunks     |
              +---------+---------+                    +---------+---------+
                        |                                        |
                        +----------------+-----------------------+
                                         |
                                         v
                           +------------------------------+
                           | packing.evidence_pack_builder|
                           +---------------+--------------+
                                           |
                                           v
                           +------------------------------+
                           | EvidencePack                 |
                           +---------------+--------------+
                                           |
                                           v
                           +------------------------------+
                           | prompting + generation       |
                           +---------------+--------------+
                                           |
                                           v
                           +------------------------------+
                           | FirstResponse                |
                           +---------------+--------------+
                                           |
                                           v
                           +------------------------------+
                           | DiagnosticObservation        |
                           +---------------+--------------+
                                           |
                                           v
                           +------------------------------+
                           | orchestration                |
                           | TransitionPolicy            |
                           | StepExecutor               |
                           +---------------+-------------+
                                           |
                                           v
                           +------------------------------+
                           | HypothesisUpdate /           |
                           | DiscriminatingCheck /        |
                           | DiagnosticUpdateResponse     |
                           +---------------+--------------+
                                           |
                                           v
                           +------------------------------+
                           | RunState                     |
                           +------------------------------+
```

Key point:

```text
RunState is the bridge between the first-response slice
and the future diagnostic loop.
```

Rust implementation note:

```text
shared typed structs flow through the whole diagram
api_clients sit at the boundary to external systems
domain_logic gives each step its meaning
orchestrator moves RunState forward
```

---

## 13. What should stay stable even as the project grows

These entities should be treated as long-lived:

- `RawUserProblem`
- `NormalizedIncidentQuery`
- `IncidentCard`
- `CardRetrievalProjection`
- `EvidenceChunk`
- `EvidencePack`
- `DiagnosticHypothesis`
- `DiscriminatingCheck`
- `DiagnosticObservation`
- `RunState`
- `TransitionPolicy`
- `StepExecutor`

These may evolve more freely:

- prompt text
- retrieval heuristics
- chunk packing heuristics
- model response wording
- theory chunk usage policy

This is the key anti-rewrite principle:

```text
stabilize the entity boundaries early
allow prompt and ranking behavior to evolve
```

Rust-specific version:

```text
stabilize shared types and module boundaries early
allow prompts, retrieval strategies, and client implementations to evolve
```

---

## 14. Final short formula

```text
Cards are structured diagnostic memory.
Chunks are evidence units.
RunState is the investigation memory.
TransitionPolicy chooses what happens next.
StepExecutor performs it.
```

That is the smallest first-iteration data model that looks implementation-ready now and still scales naturally into the future diagnostic assistant.
