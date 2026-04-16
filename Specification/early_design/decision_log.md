# Decision Log

This file is the single working log for project decisions.

Rule for now:
- We do not create separate design documents for early ideas.
- We record decisions, assumptions, and open questions here.
- Only decisions that prove stable should later become dedicated files or specs.

---

## 2026-04-14

### Working approach

Decision:
- Keep one shared log file for project decisions instead of creating multiple design documents early.

Reason:
- The project direction is still exploratory.
- We want to avoid freezing temporary ideas into too many files.

Implication:
- This file is the current source of truth for active decisions.
- Existing markdown files in the repo are treated as draft thinking, not final design authority.

---

### Phase 1 scope

Decision:
- Phase 1 focuses on building the first practical retrieval-assisted assistant loop, not the full diagnostic orchestrator.

Included in Phase 1:
- Build a `practice_corpus`.
- Collect practical distributed-systems documents.
- Chunk those documents.
- Ingest them into a retrieval collection.
- Add practice-case cards into PostgreSQL.
- Decide how to store the source documents.
- Retrieve from both:
  - theory chunks from the previous RAG project;
  - practice chunks from the new practical corpus.
- Merge retrieval results.
- Generate an answer over the merged evidence.

Explicitly not required yet:
- Multi-step diagnostic loop.
- Hypothesis update state machine.
- Procedure corpus.
- Pattern corpus.
- Full agentic orchestration.

Reason:
- This gives a useful first system slice with practical value while staying close to the previous RAG foundation.

---

### Practice corpus direction

Decision:
- Start with a practical document corpus as the main new asset of this project.

Current intended flow:
1. Gather practical distributed-systems documents.
2. Parse and normalize them.
3. Chunk them.
4. Ingest chunks into retrieval storage.
5. Create structured practice-case cards in PostgreSQL.

Initial retrieval/indexing direction:
- Preferred first path: structural chunking.
- Preferred first sparse retrieval path: hybrid retrieval with BM25.
- Dense retrieval may also be added if it is cheap to reuse from the previous project.

Status:
- Direction chosen.
- Exact final ingest matrix is still open.

Open questions:
- Whether Phase 1 should launch with:
  - hybrid BM25 only;
  - dense only;
  - dense + hybrid together.
- What minimal metadata every practice chunk must carry.
- How much structure the PostgreSQL case cards need in the first version.

---

### Source document storage

Decision:
- Source document storage is required as an explicit design concern already in Phase 1.

Candidate options:
- Store source documents on disk.
- Store source documents in object storage.

Current status:
- Not decided yet.

Current leaning:
- Start with disk storage if it keeps ingestion and provenance simpler.
- Revisit object storage when corpus management or portability needs make it worthwhile.

Open questions:
- What storage layout should be stable enough for provenance and re-ingest.
- Whether the source of truth should be local files or externalized object storage.

---

### Postgres cards for practice cases

Decision:
- In addition to chunk retrieval, Phase 1 should store structured practice-case cards in PostgreSQL.

Why:
- Chunks help semantic retrieval.
- Structured cards create a bridge toward later filtering, grouping, hypothesis support, and practical-case reasoning.

Current status:
- Confirmed as part of Phase 1.
- Exact schema is still open.

Likely minimal card content:
- `case_id`
- `title`
- `source_type`
- `system_or_product`
- `summary`
- `symptoms`
- `failure_modes`
- `mitigations`
- `source_refs`

---

### Retrieval composition

Decision:
- First-step retrieval should combine:
  - theory chunks from the previous RAG project;
  - practice chunks from the new corpus.

Goal:
- Preserve conceptual explanation from the existing system while adding practical precedent memory.

Current status:
- Confirmed as part of Phase 1.

Open questions:
- Whether retrieval should happen against two separate collections and merge later, or through a unified indexing approach.
- How ranking should balance theory relevance versus practical relevance.
- Whether merged retrieval should preserve corpus identity in every hit.

---

### First generation behavior

Decision:
- Phase 1 should already generate answers using merged retrieval evidence.

Intended behavior:
- The system can answer with both conceptual grounding and practical references.
- It does not yet need to run a full iterative diagnostic loop.

Non-goal for now:
- Do not force early implementation of full hypothesis-driven orchestration before the retrieval foundation is working.

---

### Guiding principle

Decision:
- Build the smallest useful bridge from the previous RAG system into a practical distributed-systems assistant.

Interpretation for Phase 1:
- Reuse what is already strong in the previous project.
- Add practical corpus ingestion and merged retrieval first.
- Delay heavier orchestration decisions until the retrieval/evidence layer proves useful.

---

### Incident card design direction

Decision:
- The project should use a diagnostic, time-aware incident card rather than a static incident summary card.

Reason:
- The assistant must support multi-turn troubleshooting.
- User problems arrive first as symptoms, not as known root causes.
- Symptoms can change after recovery actions, failover, rollback, rate limiting, or other interventions.
- The card must help the system choose the next uncertainty-reducing check, not only summarize a past report.

Core principle:
- Card = structured prior for investigation.
- Card is not the diagnosis itself.

Required design properties:
- Cards must preserve symptom dynamics over time.
- Cards must support hypothesis scaffolding.
- Cards must support discriminating checks and expected observation updates.
- Cards must support later normalization of messy user language into canonical incident-query fields.

Agreed incident card shape:

```yaml
case_id:
title:
source_type:
source_name:
source_path:
vendor_or_project:
system_type:
version_tested:
report_date:

short_summary:

canonical_symptoms:
affected_components:
failure_mode_candidates:
observed_phases:

incident_phases:
  - phase_name:
    context:
    symptoms:
    user_visible_impact:
    observations:
    actions_taken:
    changes_after_actions:

turning_points:

candidate_explanations:
diagnostic_patterns:
discriminating_checks:
expected_observations:
investigation_steps:

root_cause_summary:
reasoning_summary:
mitigations_or_workarounds:
prevention_or_design_followups:

claimed_guarantees:
violated_properties:
resolution_status:
fix_versions:
confidence_notes:
source_refs:
```

Field intent:
- `canonical_symptoms`, `affected_components`, `failure_mode_candidates`, and `observed_phases` should support future `NormalizedIncidentQuery` matching.
- `incident_phases` should preserve how symptoms changed over time and after actions.
- `diagnostic_patterns`, `discriminating_checks`, `expected_observations`, and `investigation_steps` should support the diagnostic loop.
- `root_cause_summary` and `reasoning_summary` should support evidence-backed user explanations and later synthesis.

---

## 2026-04-15

### First response JSON shape

Decision:
- The first model response should be returned as JSON.
- But this JSON should still read like a user-facing answer in English, not like an internal diagnostic dump.

Reason:
- JSON is easier to parse and orchestrate than free text.
- However, the values should already be suitable for direct display to the user.
- We do not want a second rendering layer to translate internal labels into readable English unless that later becomes necessary.

Implication:
- Fields should contain short, natural English explanations.
- Avoid filling user-facing response fields with internal identifiers such as:
  - `weak_default_read_write_concerns`
  - `transaction_retry_bug`
  - `network_partition_exposes_transactional_anomalies`
- Canonical/internal labels can still exist elsewhere in the pipeline, but not as the final wording shown to the user in the first response.

Current preferred first-response shape:

```json
{
  "problem_understanding": "string",
  "similar_practical_context": "string",
  "active_hypotheses": ["string", "string"],
  "first_check": "string",
  "result_interpretation": {
    "supports_primary_if": "string",
    "supports_competing_if": "string",
    "inconclusive_if": "string | null"
  },
  "competing_interpretation": "string | null"
}
```

Current wording rule:
- `problem_understanding` should sound like: `I understood the problem as ...`
- `similar_practical_context` should explain what family of incident this resembles.
- `active_hypotheses` should be 2-3 short English hypotheses written as user-facing reasoning, not as enum-like tags.
- `first_check` should be one concrete next check.
- `result_interpretation` should explain what outcome strengthens the primary explanation, what keeps the competing interpretation alive, and what remains inconclusive.
- `competing_interpretation` should contain one compact alternative explanation when evidence is not unique.

Tone rule:
- The first response must preserve uncertainty.
- Avoid language like `this confirms the root cause` or `this proves the diagnosis`.
- Prefer language like:
  - `this strengthens the primary explanation`
  - `this keeps the competing interpretation alive`
  - `this is still inconclusive`

What we learned from manual runs:
- Prompt-only nudging helped the model preserve alternative context a little.
- Adding an explicit `competing_interpretation` field worked better.
- Structuring `result_interpretation` into:
  - `supports_primary_if`
  - `supports_competing_if`
  - `inconclusive_if`
  produced the cleanest result for downstream orchestration.

Current best practical interpretation:
- The first response JSON should be machine-readable and user-readable at the same time.
- It should look like an answer to the user, only encoded in structured form.

Example style we want:

```json
{
  "problem_understanding": "I understood the problem as: the object storage API was almost unavailable, and during recovery a second wave of errors appeared while the metadata layer came under pressure.",
  "similar_practical_context": "This resembles a class of incidents where a critical dependency outage is followed by recovery amplification: the primary service begins to recover, clients reconnect or retry aggressively, and the recovery traffic overloads a metadata or control layer.",
  "active_hypotheses": [
    "The primary API or storage layer was unavailable.",
    "The recovery itself amplified the incident: reconnects, retries, or backlog drain overloaded the metadata layer.",
    "The metadata layer may already have been an independent bottleneck before the recovery phase."
  ],
  "first_check": "Compare reconnect or retry rate with metadata CPU, queue depth, or latency during the recovery window.",
  "result_interpretation": {
    "supports_primary_if": "If reconnect or retry traffic spikes before metadata saturation begins, that strengthens the recovery-amplification explanation.",
    "supports_competing_if": "If metadata saturation begins before reconnect or retry traffic rises, that keeps the independent-metadata-bottleneck interpretation alive.",
    "inconclusive_if": "If both begin at nearly the same time right after the API starts recovering, recovery amplification plus backlog drain becomes more plausible, but this is still not a final root cause."
  },
  "competing_interpretation": "The metadata layer may have been degraded independently, and the recovery only made that bottleneck more visible."
}
```

---

### First-response chunk budget under multiple plausible cards

Decision:
- For the first response, a hard small chunk budget still appears viable even when more than one card is plausible.

Current working packing rule:
- `1 primary card`
- `2 primary-card chunks`
- `1 chunk from each of up to 2 competing cards`
- `0-1 theory chunk`

What this means:
- We do not need chunk count to grow linearly with the number of plausible cards in the first response.
- Competing cards should usually contribute `alternative_context` chunks, not full card payloads.

Why this currently looks workable:
- Manual runs showed that one primary card can keep the response anchored.
- A small mixed evidence pack can still preserve alternative interpretations.
- This worked best after adding:
  - `competing_interpretation`
  - structured `result_interpretation`
  - user-facing JSON wording

Current confidence:
- Good enough for first-response experiments.
- Not yet proven for larger test sets or later steps of the diagnostic loop.

---

### Project goal framing

Decision:
- The project goal should be framed not only as “a troubleshooting assistant for distributed systems”.
- The deeper goal is to build a specification-first, observable, evaluable, controllable AI agentic system with:
  - iterative diagnostic reasoning;
  - explicit intermediate state;
  - context memory;
  - multiple knowledge corpora;
  - inspectable transitions between steps.

First domain:
- Distributed systems troubleshooting is the first proving ground for this architecture.

Why this framing matters:
- The project is not only about producing good RAG answers.
- The project is about building an AI system whose reasoning loop can be inspected, constrained, evaluated, and improved.
- This explains why the repository should not be organized around code alone.
- It also explains why specifications, state contracts, orchestration boundaries, observability, and evaluation artifacts are central assets of the project.

Short working formulation:

```text
Build a specification-first, observable, evaluable AI agent
with iterative diagnostic reasoning, explicit state, memory,
and multi-corpus retrieval.

First domain:
distributed systems troubleshooting.
```

Non-goal:

```text
This project is not only about building another RAG assistant.
It is about building a controllable AI architecture through a
demanding real-world reasoning task.
```

Related architecture note:
- A future normalization layer should produce both canonical structured fields and unmapped/raw user meaning.
- Cards should therefore contain canonical fields without forcing the system to ignore novelty in user phrasing.
