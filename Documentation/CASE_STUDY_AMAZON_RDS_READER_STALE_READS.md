# Case Study: Amazon RDS Reader Stale Reads Across Three Iterations

This case study walks through one runtime run with three iterations and uses it to show how the application actually works on a concrete example.

The chosen case is useful because the diagnostic frame does not stay fixed. The system starts with two plausible explanations, then updates its understanding as new observations arrive:

- a weaker cross-endpoint snapshot model on the reader;
- ordinary replication lag on the replica.

Across the three iterations, the system keeps the same primary precedent, changes the competing context, narrows the first check, and shifts confidence toward the replication-lag explanation.

## Contents

- [Source Artifacts](#source-artifacts)
- [Case Summary](#case-summary)
- [Iteration Timeline](#iteration-timeline)
- [Iteration 1: How The Initial Diagnostic Frame Was Built](#iteration-1-how-the-initial-diagnostic-frame-was-built)
- [Iteration 2: How The First Observation Update Changed The Frame](#iteration-2-how-the-first-observation-update-changed-the-frame)
- [Iteration 3: How Lag Correlation Further Tightened The Update](#iteration-3-how-lag-correlation-further-tightened-the-update)
- [What This Case Shows](#what-this-case-shows)

## Source Artifacts

This document is based on these trace exports:

- [Trace-a29c79-2026-05-13 20_36_20.json](<case study traces/Trace-a29c79-2026-05-13 20_36_20.json>)
- [Trace-c3e5ee-2026-05-13 20_36_44.json](<case study traces/Trace-c3e5ee-2026-05-13 20_36_44.json>)
- [Trace-da1e4c-2026-05-13 20_36_52.json](<case study traces/Trace-da1e4c-2026-05-13 20_36_52.json>)

The main precedent documents used by the run originate from these card files in the repo:

- [amazon_rds_postgresql_17_4_incident_card.yaml](../Evidence/incident_cards/amazon_rds_postgresql_17_4_incident_card.yaml) → `amazon_rds_postgresql_17_4_jepsen_2025_04_29`
- [redis_raft_1b3fbf6_incident_card.yaml](../Evidence/incident_cards/redis_raft_1b3fbf6_incident_card.yaml) → `redis_raft_1b3fbf6_jepsen_2020_06_23`
- [mysql_8_0_34_incident_card.yaml](../Evidence/incident_cards/mysql_8_0_34_incident_card.yaml) → `mysql_8_0_34_jepsen_2023_12_19`
- [ravendb_6_0_2_incident_card.yaml](../Evidence/incident_cards/ravendb_6_0_2_incident_card.yaml) → `ravendb_6_0_2_jepsen_2024_01_31`
- [cockroachdb_beta_20160829_incident_card.yaml](../Evidence/incident_cards/cockroachdb_beta_20160829_incident_card.yaml) → `cockroachdb_beta_20160829_jepsen_2017_02_16`
- [scylla_4_2_rc3_incident_card.yaml](../Evidence/incident_cards/scylla_4_2_rc3_incident_card.yaml) → `scylla_4_2_rc3_jepsen_2020_12_23`

These repo files are the source artifacts. During data preparation they are loaded into PostgreSQL and indexed into Qdrant. At runtime, the application does not read these YAML files directly: it hydrates the selected structured cards from PostgreSQL and retrieves semantic candidates from Qdrant using the corresponding document ids.

## Case Summary

The initial problem report was:

> The cluster exhibits inconsistent reads: a transaction committed on the writer endpoint is not returned by queries against the reader endpoint, even though no failover has occurred.

Two continuation observations then arrived:

1. the reader misses the new value only for a few seconds and then catches up;
2. CloudWatch lag spikes coincide with those stale-read windows, and the stale window shrinks when lag returns to normal.

That sequence is important because it gives the system a chance to do real continuation work rather than simply restating the original answer.

## Iteration Timeline

The timeline is easiest to read vertically because each iteration carries a fairly large amount of state.

| Field | Iteration 1 | Iteration 2 | Iteration 3 |
|---|---|---|---|
| `input` | Amazon RDS for PostgreSQL reader endpoint misses writes that are already visible on the writer endpoint. No failover has occurred. | If we read the same row from the writer endpoint immediately after commit, it is visible there, but the reader endpoint still misses it for a few seconds, and the reader returns the new value only after that delay passes. | CloudWatch shows replica lag spikes at the same moments when the stale reads from the reader endpoint happen, and the stale-read window shrinks again when the lag returns to normal. |
| `problem understanding` | The cluster exhibits inconsistent reads: a transaction committed on the writer endpoint is not returned by queries against the reader endpoint, even though no failover has occurred. | The cluster shows a brief period (a few seconds) after a transaction commits where the writer endpoint returns the new value but the reader endpoint continues to return the old value, after which the reader eventually reflects the write. | The cluster exhibits brief periods after commits where the writer endpoint returns the new value but the reader endpoint still returns the old value; these stale-read intervals coincide with spikes in replica replication lag and shrink when lag returns to normal. |
| `hypothesis 1` | `[Active / "High"]` The reader endpoint implements a weaker Parallel Snapshot Isolation, so its snapshot may omit writes that are already visible on the primary even when replication is caught up. `[Source: Primary incident]` | `[weakened / "Medium ↓"]` The reader implements a weaker Parallel Snapshot Isolation, so its snapshot may omit writes that are already visible on the primary even when replication is caught up. `[Source: Primary incident]` | `[weakened / "Low"]` The reader endpoint uses a weaker Parallel Snapshot Isolation, so its snapshot may omit recent writes even when replication appears caught up. `[Source: Theory/mechanism]` |
| `hypothesis 2` | `[Active / "Medium"]` The reader is lagging behind the primary due to asynchronous WAL replay, causing stale reads of recent writes. `[Source: Alternative incident]` | `[active / "High"]` The reader lags behind the primary because WAL replay on the secondary is asynchronous, producing a temporary stale view. `[Source: Primary incident]` | `[active / "High"]` The reader lags behind the primary because asynchronous WAL replay temporarily falls behind, causing replication-lag spikes that produce stale reads. `[Source: Primary incident]` |
| `check` | Query the current WAL LSN on the writer and on the reader (e.g., `SELECT pg_last_wal_replay_lsn()`) and compute the lag between them. | Immediately after a commit, issue a `READ COMMITTED` query on the reader endpoint and see whether it returns the new value. | Query the replica’s replication lag (e.g., `pg_stat_replication` or CloudWatch metric) at the moment a stale read is observed to determine if lag is elevated. |
| `supports primary if` | The LSN gap is negligible (e.g., zero or a few bytes) yet the read still misses the write, suggesting the divergence is due to snapshot-isolation differences rather than replication delay. | The `READ COMMITTED` query on the reader returns the new value immediately, indicating that the delay is due to asynchronous WAL replay rather than a weaker snapshot model. | If the measured lag is high (seconds of delay) at the time of the stale read, this strengthens the replication-lag hypothesis. |
| `supports competing if` | The LSN gap is sizable (e.g., several seconds or many WAL bytes), indicating the reader is behind the primary and can legitimately miss recent writes. | The `READ COMMITTED` query on the reader does not return the new value, indicating that the reader’s snapshot is lagging behind the primary’s state despite replication being up-to-date. | If the measured lag is low or within normal range when the stale read occurs, the weaker snapshot-isolation hypothesis remains plausible. |

This progression is the core of the case. The system does not just keep the same answer and decorate it with new wording. The problem understanding becomes more specific, the weaker-snapshot hypothesis loses strength, and the next check becomes more directly tied to the new evidence.

## Iteration 1: How The Initial Diagnostic Frame Was Built

This first iteration is the bootstrap step. The system starts from one short raw problem report and has to build the initial diagnostic frame: what kind of problem this is, which precedent is closest, which alternative explanations are still plausible, and what first check would best separate them.

The step sequence for this iteration is:

1. `input_normalization`
Purpose: convert the raw problem report into the deterministic normalized text used by the downstream initial-iteration pipeline.

2. `query_structuring`
Purpose: extract structured fields that can later help evaluate query completeness and shape the final answer.

3. `information_adequacy_analyzer`
Purpose: decide whether the structured input is sufficient for a real diagnostic move or whether the system should stop and ask follow-up questions instead.

4. `candidate_card_retrieval`
Purpose: find the closest precedent card and a small set of alternatives.

5. `card_hydration`
Purpose: load the full structured incident cards for the selected `card_id`s.

6. `incident_evidence_retrieval`
Purpose: pull practical chunks from the primary and alternative cards.

7. `theory_evidence_retrieval`
Purpose: add mechanism-level evidence that explains why the same symptom could arise from different causes.

8. `prompt_context_assembly`
Purpose: compress the retrieved material into the evidence pack actually sent to generation.

9. `llm_structured_generation`
Purpose: turn that assembled context into the first diagnostic update: problem understanding, hypotheses, and a discriminating check.

10. `response_validation_and_normalization`
Purpose: validate the generated structured response and convert it into the trusted runtime result returned for the iteration.

### Input

The raw input to the run was:

```text
Amazon RDS for PostgreSQL reader endpoint misses writes that are already visible on the writer endpoint. No failover has occurred.
```

### Query Structuring

The first mechanism that mattered was `query_structuring`.

This step is performed by a model. Its input is user query and output is the structured JSON artifact shown below.

This artifact is useful mainly because it helps evaluate input completeness and shapes the final answer. It is less central to retrieval than the raw query itself: semantic search still uses the original user query rather than this full JSON object.

```json
{
  "intent": "diagnose why a read-only reader endpoint in Amazon RDS for PostgreSQL can miss writes already visible on the writer endpoint without any failover",
  "scenario": "Amazon RDS PostgreSQL reader endpoint returns a snapshot that does not include writes already visible on the primary even though the cluster is healthy and no failover occurred",
  "symptoms": [
    {
      "term": "inconsistent_reads_between_primary_and_reader",
      "evidence_span": "reader endpoint misses writes that are already visible on the writer endpoint",
      "support_level": "explicit"
    }
  ],
  "affected_subsystems": [
    {
      "term": "reader_endpoint",
      "evidence_span": "reader endpoint misses writes that are already visible on the writer endpoint",
      "support_level": "explicit"
    },
    {
      "term": "primary_endpoint",
      "evidence_span": "writer endpoint",
      "support_level": "explicit"
    }
  ],
  "failure_modes": [
    {
      "term": "reader_endpoint_visibility_divergence",
      "evidence_span": "reader endpoint misses writes that are already visible on the writer endpoint",
      "support_level": "explicit"
    }
  ],
  "system_properties": [
    {
      "term": "durability of acknowledged writes under weak or surprising concern configurations",
      "evidence_span": "misses writes that are already visible on the writer endpoint",
      "support_level": "implicit"
    },
    {
      "term": "snapshot-consistent reads across transaction modes",
      "evidence_span": "reader endpoint misses writes that are already visible on the writer endpoint",
      "support_level": "explicit"
    }
  ],
  "entities": [
    "Amazon RDS",
    "PostgreSQL",
    "reader endpoint",
    "writer endpoint"
  ],
  "constraints": [],
  "triggers": [],
  "observability_signals": [],
  "unresolved_terms": [],
  "rejected_nearby_terms": [
    {
      "term": "stale_reads",
      "reason": "more generic term; query specifically mentions inconsistency between primary and reader"
    },
    {
      "term": "dirty_reads",
      "reason": "refers to reads that return uncommitted data, not the case described"
    },
    {
      "term": "read_skew",
      "reason": "describes ordering anomalies rather than missing writes"
    }
  ],
  "confidence": "medium"
}
```

#### Vocabulary-Backed Term Selection

The `query_structuring` step produces four vocabulary-backed fields—fields where the model selects terms from or is evaluated against the controlled vocabulary built from the incident corpus. The other eight fields (intent, scenario, entities, constraints, triggers, observability_signals, unresolved_terms, rejected_nearby_terms) are extracted freely without vocabulary constraints.

During evaluation, the four vocabulary-backed fields are compared against golden expectations for this specific question, which define strict and soft acceptable terms and their relevance scores.

For this case, the model's term selections versus golden expectations were:

| Field | Model Selected | In Vocabulary? | Expected (Strict) | Match? |
|-------|---|---|---|---|
| **symptoms** | inconsistent_reads_between_primary_and_reader | ✅ Yes | inconsistent_reads_between_primary_and_reader | ✅ |
| **affected_subsystems** | reader_endpoint | ✅ Yes | reader_endpoint | ✅ |
| **affected_subsystems** | primary_endpoint | ✅ Yes | (soft: primary_endpoint) | ✅ |
| **failure_modes** | reader_endpoint_visibility_divergence | ✅ Yes | reader_endpoint_visibility_divergence | ✅ |
| **system_properties** | durability of acknowledged writes... | ✅ Yes | Snapshot Isolation across primary and reader endpoints | ❌ |
| **system_properties** | snapshot-consistent reads across transaction modes | ✅ Yes | (same ↑) | ❌ |

Three fields matched perfectly. However, **system_properties** reveals why query-structuring metrics are challenging: all selected terms are valid vocabulary terms, yet two of them (rows 5-6) do not match golden expectations. The expected strict term "Snapshot Isolation across primary and reader endpoints" is also in the vocabulary, but the model did not select it—instead selecting semantically-related but distinct terms. This creates:
- **false positives**: 2 selected terms in vocabulary but outside golden expectation set
- **false negative**: 1 expected vocabulary term not selected by model
- Lower precision and recall scores

The controlled vocabulary itself comes from the incident card collection (stored in PostgreSQL). During query structuring, the model received this vocabulary as JSON in the prompt and could:
- Select terms directly from the vocabulary (as happened here);
- Propose new terms if they had clear grounding in the user query (not needed in this case).

#### Why Query Structuring Quality Metrics Matter

For this specific case, the metrics reveal the challenge: while 3 of 4 vocabulary-backed fields matched golden expectations perfectly, the **system_properties** field mismatch cascades:

- **false_positive_count** for system_properties: 2 (selected terms not in golden set)
- **false_negative_count** for system_properties: 1 (missed the expected term)
- **precision_soft**: ~0.5 (only half of selected terms were in golden soft set)
- **recall_strict**: 0.75 (only 3 of 4 expected strict terms found across all fields)

The model's selections ("durability of acknowledged writes", "snapshot-consistent reads") are reasonable synonyms for snapshot isolation, yet they do not match the golden expectations. This is a real engineering pattern: controlled vocabulary term selection is nuanced—the model must choose exact terms from a finite vocabulary that the golden set specifies, even when semantically-close alternatives exist.

Query structuring is therefore a leverage point: improvements in structuring directly improve downstream retrieval and answer quality.

### Candidate Retrieval

The candidate-card retriever then selected one strong primary precedent and two alternatives.

Primary card:

- `amazon_rds_postgresql_17_4_jepsen_2025_04_29`

Alternative cards:

- `redis_raft_1b3fbf6_jepsen_2020_06_23`
- `mysql_8_0_34_jepsen_2023_12_19`

Why this retrieval result is sensible:

- the Amazon RDS card is the closest direct precedent because it is about healthy-cluster reader/writer inconsistency in RDS PostgreSQL without failover;
- Redis-Raft and MySQL are weaker analogies, but both keep the space of stale or inconsistent read behaviors open instead of collapsing immediately to one vendor-specific explanation.

So even at the card-retrieval level, the system is already doing two jobs:

- finding the most likely precedent;
- preserving competing context.

### Card Hydration

After the candidate-card retriever returns `card_id`s, the runtime loads the full cards from Postgres.

These cards are not raw incident-report text. Each one is already a structured summary of an incident report, with fields such as:

- `short_summary`
- `canonical_symptoms`
- `affected_components`
- `failure_mode_candidates`
- `diagnostic_patterns`
- `investigation_steps`
- `root_cause_summary`
- `mitigations_or_workarounds`

That matters because part of this structured card content later goes into the final prompt. The generation step does not reason only over free-form chunks; it also receives normalized incident-card fields.

Here is a small excerpt from the primary card used in this run:

```yaml
case_id: amazon_rds_postgresql_17_4_jepsen_2025_04_29
title: "Amazon RDS for PostgreSQL 17.4 Jepsen analysis: multi-AZ reader endpoint violates snapshot isolation"

short_summary: >
  Jepsen found that Amazon RDS for PostgreSQL multi-AZ clusters violate Snapshot Isolation across
  the primary and reader endpoints even under healthy conditions and without failover.

canonical_symptoms:
  - "inconsistent_reads_between_primary_and_reader"
  - "snapshot_isolation_violation"
  - "long_fork"
  - "healthy_cluster_anomaly"

failure_mode_candidates:
  - "reader_endpoint_visibility_divergence"
  - "long_fork"
  - "primary_vs_secondary_order_disagreement"

diagnostic_patterns:
  - >
    healthy managed cluster -> writes succeed normally -> read-only queries on secondaries disagree about
    order of committed writes -> inconsistent snapshot appears without obvious outage
```

### Incident Evidence Retrieval

After cards were selected, the runtime pulled incident evidence chunks from both the primary card and the alternative cards.

At this step, retrieval is filtered in two ways:

- by the card ids selected on the previous step;
- by chunk tag families inside those cards.

So the primary evidence retrieval searches within the Amazon RDS card context, and the alternative retrieval searches within the competing-card context.

Primary tag families:

- `chunk_role:symptom`
- `chunk_role:impact`
- `chunk_role:timeline`
- `chunk_role:symptom_change`
- `chunk_role:investigation`
- `chunk_role:diagnostic_step`
- `chunk_role:hypothesis_update`
- `chunk_role:recovery`

Alternative tag families:

- `chunk_role:failure_mode`
- `chunk_role:root_cause`
- `chunk_role:contributing_factor`
- `chunk_role:uncertainty`
- `chunk_role:lesson`

This division of evidence is already informative:

- primary chunks anchor the closest known incident and provide the concrete investigation pattern;
- alternative chunks keep broader competing explanations available, especially around stale reads and weaker-than-expected consistency behavior;
- the tag filtering helps keep the retrieved evidence role-specific instead of mixing every part of every card into one flat context.

### Theory Retrieval

The theory retriever added mechanism-level evidence on top of incident precedents.

These chunks play a different role from incident evidence. They are not there to say “this exact incident happened before.” They are there to explain the mechanism space behind the case:

- visibility divergence across endpoints;
- replication delay as a competing explanation;
- why the same surface symptom can arise from more than one consistency mechanism.

### Prompt Context Assembly

The prompt-context assembly step then compressed the retrieved material into the context that the final structured generation step would actually use.

From trace:

- primary card: Amazon RDS PostgreSQL 17.4 Jepsen analysis
- alternative cards: Redis-Raft and MySQL 8.0.34
- primary incident chunks retrieved: `4`
- alternative incident chunks retrieved: `10`
- theory chunks retrieved: `6`
- selected `evidence_for_match`: `1`
- selected `first_check_hint`: `1`
- selected `supporting_explanation`: `1`
- selected `alternative_context`: `2`
- selected `mechanism_explanation`: `1`
- selected total chunk count: `6`

The selection algorithm is role-based rather than score-only. The assembly step tries to fill a small fixed set of prompt roles:

| Role | Source | Chunk Limit | Chunk Tags / Selection Priority |
|---|---|---|---|
| `evidence_for_match` | primary incident | `1` | `chunk_role:symptom` -> `chunk_role:failure_mode` -> `chunk_role:contributing_factor` |
| `first_check_hint` | primary incident | `1` | `chunk_role:diagnostic_step` -> `chunk_role:investigation` -> `chunk_role:lesson` |
| `supporting_explanation` | primary incident | `1` | `chunk_role:failure_mode` -> `chunk_role:contributing_factor` -> `chunk_role:uncertainty` -> `chunk_role:hypothesis_update` |
| `alternative_context` | alternative incident | `2` total, `1` per card | `chunk_role:symptom` -> `chunk_role:failure_mode` -> `chunk_role:uncertainty` |
| `mechanism_explanation` | theory | `1` | theory chunk, no tag-priority list |

That is why the final pack size is exactly `6`: `1 + 1 + 1 + 2 + 1`.

The selection rules on this iteration work like this:

1. Primary-incident chunks are used to fill the three primary roles.
For each role, the module ranks eligible chunks by configured tag priority first and retrieval score second.

2. The same chunk is not supposed to fill every role.
Once a chunk is selected for one role, later primary-role selection tries to use a different chunk if one is available.

3. Alternative context is selected separately from the alternative-card pool.
It is not just “take the top 2 alternative chunks”. The module does round-robin selection across alternative cards and respects `per_case_limit = 1`, so this iteration ends up with one alternative chunk from Redis-Raft and one from MySQL rather than two chunks from only one competing card.

4. Mechanism explanation is selected from theory chunks.
The current configuration allows only one theory chunk in the prompt, so theory evidence stays compact and explanatory rather than taking over the whole context window.

On this example, that means a much larger retrieval set is compressed into one compact pack with five distinct roles:

- one chunk that best anchors the primary match;
- one chunk that best hints at a discriminating first check;
- one chunk that adds supporting explanation from the primary precedent;
- two alternative-context chunks, one per competing card;
- one theory chunk that carries the mechanism explanation.

So the model does not receive “the top 6 chunks overall”. It receives a deliberately role-balanced pack assembled from three evidence sources: hydrated incident cards, incident chunks, and theory chunks.

The formal module contract for this step is described in [prompt_context_assembly.md](../Specification/runtime/request_pipeline/prompt_context_assembly.md).

### Diagnostic Update Produced

At this point the system has to turn the assembled context into the first actual diagnostic update.

What goes into the model is already much richer than just the user query. On this iteration the generation step receives:

- the original user problem;
- selected fields from `query_structuring`, especially:
  - `intent`
  - `scenario`
  - `symptoms`
  - `affected_subsystems`
  - `failure_modes`
  - `system_properties`
  - `rejected_nearby_terms`
- selected fields from the hydrated primary incident card, especially:
  - `short_summary`
  - `canonical_symptoms`
  - `failure_mode_candidates`
  - `diagnostic_patterns`
  - `discriminating_checks`
  - `investigation_steps`
- the selected incident evidence chunks with prompt roles;
- the selected theory chunk with its prompt role;
- the evidence topology summary:
  - primary roles present: `evidence_for_match`, `first_check_hint`, `supporting_explanation`
  - alternative context present for `redis_raft_1b3fbf6_jepsen_2020_06_23` and `mysql_8_0_34_jepsen_2023_12_19`
  - theory evidence present
- the prompt asset and policy constraints, including:
  - `return strict JSON only`
  - `do not claim final root cause`
  - `give exactly one first check`
  - `state uncertainty when evidence is not unique`

The `evidence_topology` field gives the model a compact map of which evidence branches are present and which prompt roles were successfully filled. This helps the model attribute hypotheses to the right source and avoid relying on evidence branches that were not actually provided.

From trace we can also see the LLM usage for this step:

- prompt tokens: `3736`
- completion tokens: `888`
- total tokens: `4624`

The raw model output is also small enough to show directly:

```json
{
  "response": {
    "problem_understanding": "The cluster exhibits inconsistent reads: a transaction committed on the writer endpoint is not returned by queries against the reader endpoint, even though no failover has occurred.",
    "similar_practical_context": "The observed behavior matches the Long-Fork/Parallel-Snapshot-Isolation anomalies reported for Amazon RDS PostgreSQL multi-AZ clusters, and is analogous to stale-read cases seen in Redis-Raft and MySQL read-replica experiments.",
    "hypotheses": [
      {
        "id": "c1a2f3e4-5b6d-7e8f-9a0b-1c2d3e4f5a6b",
        "text": "The reader endpoint implements a weaker Parallel Snapshot Isolation, so its snapshot may omit writes that are already visible on the primary even when replication is caught up.",
        "status": "Active",
        "source": "PrimaryIncident",
        "confidence": "High"
      },
      {
        "id": "d7e8f9a0-b1c2-3d4e-5f6a-7b8c9d0e1f2a",
        "text": "The reader is lagging behind the primary due to asynchronous WAL replay, causing stale reads of recent writes.",
        "status": "Active",
        "source": "AlternativeContext",
        "confidence": "Medium"
      }
    ],
    "first_check": "Query the current WAL LSN on the writer and on the reader (e.g., SELECT pg_last_wal_replay_lsn()) and compute the lag between them.",
    "result_interpretation": {
      "supports_primary_if": "The LSN gap is negligible (e.g., zero or a few bytes) yet the read still misses the write, suggesting the divergence is due to snapshot-isolation differences rather than replication delay.",
      "supports_competing_if": "The LSN gap is sizable (e.g., several seconds or many WAL bytes), indicating the reader is behind the primary and can legitimately miss recent writes.",
      "inconclusive_if": null
    },
    "competing_interpretation": "The symptom resembles stale-read anomalies observed in other replicated stores (e.g., Redis-Raft and MySQL read-replica tests) where replication lag causes reads on secondaries to return older data."
  }
}
```

This output is well matched to the evidence mix:

- the Amazon RDS precedent justifies taking the weaker-snapshot explanation seriously;
- the alternative context keeps replication lag alive as a legitimate competing explanation;
- the first check is explicitly discriminating rather than descriptive.

### Why This Iteration Matters

The first iteration matters because it sets the entire diagnostic frame for the rest of the run.

If query structuring had reduced the case to generic stale reads, or if retrieval had returned only one narrow precedent with no competing context, the later continuation updates would have had much less room to behave intelligently. Instead, the system starts with a frame that is both specific and still open to multiple explanations. That is exactly what makes the later hypothesis updates meaningful.

## Iteration 2: How The First Observation Update Changed The Frame

The second iteration is the first true continuation step. The system no longer starts from a raw standalone problem description. It now has to combine prior diagnostic state with one new observation and decide whether that observation strengthens, weakens, or leaves the current hypotheses unchanged.

The step sequence for this iteration is:

1. `observation_boundary_resolver`
Purpose: normalize the new observation against prior context and decide what the standalone supported observation should be.

2. `observation_extraction`
Purpose: extract the concrete evidence-bearing pieces from that resolved observation.

3. `information_adequacy_analyzer`
Purpose: decide whether the extracted observation is sufficient for a real continuation update or whether more detail is still needed.

4. `candidate_card_retrieval`
Purpose: refresh the closest precedent and the competing context in light of the new observation.

5. `card_branch_reranking`
Purpose: reorder the retrieved candidate branch so the continuation flow carries forward the strongest refreshed primary card and competing context before hydration.

6. `card_hydration`
Purpose: load the structured cards for the refreshed `card_id`s.

7. `incident_evidence_retrieval`
Purpose: pull practical chunks from the primary and alternative cards using the updated framing.

8. `theory_evidence_retrieval`
Purpose: add mechanism-level evidence that can explain the new observation.

9. `diagnostic_update_prompt_context_assembly`
Purpose: assemble the compact continuation evidence pack for the next model step.

10. `llm_structured_generation`
Purpose: update problem understanding, hypotheses, and the next discriminating check.

### Input

The raw continuation observation was:

```text
If we read the same row from the writer endpoint immediately after commit, it is visible there, but the reader endpoint still misses it for a few seconds, and the reader returns the new value only after that delay passes.
```

### Observation Boundary Resolution

This step is model-based. Its job is to resolve the new observation against prior context and produce a clean standalone update that can be processed further.

It also does an important classification job before that normalization becomes useful: it decides whether the new input actually looks like a continuation observation at all, rather than, for example, a question, an unrelated statement, or some other message that should not be treated as diagnostic evidence.

The resolved observation was:

```text
After a commit, the writer endpoint immediately shows the new value, but the reader endpoint does not return the new value for a few seconds, only after that delay.
```

This is already a meaningful change in representation. The observation becomes shorter, more canonical, and easier to feed into the continuation pipeline than the original free-form wording.

### Observation Extraction

After boundary resolution, the resolved observation is passed into `observation_extraction`.

This step is also model-based. Its job is to turn one resolved observation into a small set of explicit atomic observations that the continuation update can reason over.

The raw extraction output is compact enough to show directly:

```json
{
  "normalized_user_input": "If we read the same row from the writer endpoint immediately after commit, it is visible there, but the reader endpoint still misses it for a few seconds, and the reader returns the new value only after that delay passes.",
  "resolved_observation": {
    "text": "After a commit, the writer endpoint immediately shows the new value, but the reader endpoint does not return the new value for a few seconds, only after that delay."
  },
  "confidence": "High",
  "observations": [
    {
      "statement": "Writer endpoint shows the new value immediately after a commit",
      "confidence": "High",
      "condition": null,
      "polarity": "present",
      "time_relation": "immediate",
      "source_span": "writer endpoint immediately shows the new value"
    },
    {
      "statement": "Reader endpoint does not return the new value for a few seconds after a commit",
      "confidence": "High",
      "condition": null,
      "polarity": "absent",
      "time_relation": "few seconds delay",
      "source_span": "reader endpoint does not return the new value for a few seconds"
    }
  ],
  "needs_more_context": false,
  "missing_context_questions": []
}
```

This step matters because the next step, `information_adequacy_analyzer`, needs a structured observation rather than free-form continuation text. The extracted atomic observations make it possible to assess whether the new observation is complete enough for a real continuation update or whether more detail is still needed.

### Information Adequacy Analyzer

After observation extraction, the structured continuation signal goes into `information_adequacy_analyzer`.

This step is deterministic and does not call a model. For supported observations like this one, it uses a small rule set over the extracted observation object:

- `observation_count`
- `present_count`
- `absent_count`
- `corrected_count`
- `question_count`
- confidence levels on the extracted observations
- the top-level flag `needs_more_context`

At a high level, the rules are:

- `Blocking` if there are no extracted observations, if `needs_more_context = true`, or if the extracted observation is too weak to support a real continuation move;
- `WeakButRunnable` if there is some usable signal but it is still diagnostically thin, for example only one extracted observation or only medium-confidence evidence;
- `Sufficient` if the observation contains enough concrete signal for normal downstream progression.

On this iteration, the extracted observation is clearly strong enough to continue:

- `observation_count = 2`
- both extracted observations have `confidence = High`
- `needs_more_context = false`
- `missing_context_questions = []`

So this observation is not treated as a vague continuation hint. It is treated as a sufficient update signal, which allows the runtime to refresh retrieval and rebuild the diagnostic frame instead of pausing for more clarification.

### Candidate Cards Retrieval

The primary precedent remained the same:

- `amazon_rds_postgresql_17_4_jepsen_2025_04_29`

The alternative cards changed:

- `redis_raft_1b3fbf6_jepsen_2020_06_23`
- `ravendb_6_0_2_jepsen_2024_01_31`

For semantic search on continuation turns, the retrieval query is built from the previous iteration's problem understanding plus the new observation as a simple string concatenation.

This step does not manually replace cards. It reruns candidate search against that updated continuation query, rebuilds the ranked list, keeps the top hit as the primary card, and refreshes the alternatives from the remaining ordered hits.

This is already a meaningful retrieval shift.

Iteration 1 had used Redis-Raft and MySQL as the competing context. After the new observation made the case look more like a temporary visibility lag, the alternative set changed. RavenDB enters as a useful analogy for weaker-than-expected read semantics and fractured views, while the primary Amazon RDS precedent remains stable.

### Card Hydration

As in the first iteration, the selected `card_id`s are hydrated into full structured cards from Postgres.

That means the continuation step again has access not just to free-form chunks, but also to the same structured card artifact that was used on the initial turn.

The important point is that continuation does not bypass the same evidence architecture as the initial turn. It refreshes that architecture using the new observation.

### Incident Evidence Retrieval

The filtering logic is the same as before:

- retrieval is constrained by the selected `card_id`s;
- then by the configured tag families inside those cards.

The primary evidence continues to come from the Amazon RDS card, but the continuation signal shifts the emphasis toward a slightly different tag mix:

- `chunk_role:symptom`
- `chunk_role:timeline`
- `chunk_role:symptom_change`
- `chunk_role:investigation`
- `chunk_role:diagnostic_step`
- `chunk_role:hypothesis_update`

Compared with the initial turn, tags like `timeline`, `symptom_change`, and `hypothesis_update` matter more here because the new observation is explicitly about a short-lived stale-read window rather than only about a static mismatch between writer and reader.

The alternative evidence also shifts a bit. It still comes from competing-card families, but now the useful tags are the ones that preserve competing explanations for temporary divergence:

- `chunk_role:failure_mode`
- `chunk_role:contributing_factor`
- `chunk_role:uncertainty`
- `chunk_role:lesson`

### Theory Retrieval

The theory retriever again adds mechanism-level evidence on top of the practical precedents.

At this point, theory is especially useful because the case still has two live explanations:

- asynchronous replication lag;
- weaker snapshot semantics on the reader.

The role of theory evidence here is to help separate these mechanisms conceptually, so that the next check can become more discriminating rather than merely restating the symptom.

### Prompt Context Assembly

This is a different module: `diagnostic_update_prompt_context_assembly`, not the initial-turn `prompt_context_assembly`.

Its job is not just to repack evidence. It builds a continuation-oriented prompt context around the previous diagnostic state plus the new observation.

From trace:

- primary card: Amazon RDS PostgreSQL 17.4 Jepsen analysis
- alternative cards: Redis-Raft and RavenDB 6.0.2
- selected total chunk count: `6`

The prompt context on this iteration includes:

- the previous iteration's `problem_understanding`;
- the `resolved_observation`;
- the extracted atomic `observations`;
- the prior diagnostic state:
  - `active_hypotheses`
  - `rejected_hypotheses`
  - `last_check`
- a compact prompt-facing view of the primary incident card;
- selected incident evidence buckets;
- selected theory evidence.

The selection logic is still role-based, but the role set is slightly different on continuation turns:

- one chunk for `evidence_for_match`
- one chunk for `next_check_hint`
- one chunk for `supporting_explanation`
- two chunks for `alternative_context`
- one chunk for `mechanism_explanation`

The renamed `next_check_hint` bucket matters. On continuation turns, the model is not producing a diagnosis from scratch. It is updating an existing diagnostic state and proposing the next discriminating move in light of the newly added observation.

The compact primary-card view is also narrower than the hydrated card artifact itself. The prompt gets a structured subset organized as:

- `context`
- `hypotheses`
- `checks`

The system does not simply reuse the previous prompt context unchanged. It rebuilds a fresh continuation prompt around the updated observation, the prior diagnostic state, and a newly reselected evidence pack.

### Diagnostic Update Produced

The generation step uses the continuation prompt assembled above and applies the same policy constraints as the first iteration, including:

- `return strict JSON only`
- `do not claim final root cause`
- `give exactly one first check`
- `state uncertainty when evidence is not unique`

From trace:

- prompt tokens: `3668`
- completion tokens: `1470`
- total tokens: `5138`

Raw model output:

```json
{
  "competing_interpretation": "The reader endpoint uses a weaker Parallel Snapshot Isolation, causing its snapshot to omit recent writes even when replication is caught up.",
  "first_check": "Immediately after a commit, issue a READ COMMITTED query on the reader endpoint and see whether it returns the new value.",
  "hypotheses": [
    {
      "confidence": "high",
      "id": "d7e8f9a0-b1c2-3d4e-5f6a-7b8c9d0e1f2a",
      "rejection_reason": null,
      "source": "primary_incident",
      "status": "active",
      "text": "The reader lags behind the primary because WAL replay on the secondary is asynchronous, producing a temporary stale view."
    },
    {
      "confidence": "medium",
      "id": "c1a2f3e4-5b6d-7e8f-9a0b-1c2d3e4f5a6b",
      "rejection_reason": null,
      "source": "primary_incident",
      "status": "weakened",
      "text": "The reader implements a weaker Parallel Snapshot Isolation, so its snapshot may omit writes that are already visible on the primary even when replication is caught up."
    }
  ],
  "problem_understanding": "The cluster shows a brief period (a few seconds) after a transaction commits where the writer endpoint returns the new value but the reader endpoint continues to return the old value, after which the reader eventually reflects the write.",
  "result_interpretation": {
    "inconclusive_if": null,
    "supports_competing_if": "The READ COMMITTED query on the reader does not return the new value, indicating that the reader’s snapshot is lagging behind the primary’s state despite replication being up‑to‑date.",
    "supports_primary_if": "The READ COMMITTED query on the reader returns the new value immediately, indicating that the delay is due to asynchronous WAL replay rather than a weaker snapshot model."
  },
  "similar_practical_context": "Stale reads observed in healthy Amazon RDS PostgreSQL clusters without failover, where the secondary endpoint briefly lags behind the primary."
}
```

The resulting update is directionally very important:

- `problem understanding` now describes a brief delay window rather than a generic inconsistency;
- the replication-lag hypothesis becomes the leading active explanation;
- the weaker-snapshot hypothesis remains alive but is explicitly weakened;
- the new `first_check` becomes more targeted:
  - immediately after commit, issue a `READ COMMITTED` query on the reader endpoint and see whether it returns the new value.

This is a stronger continuation response than simply repeating the LSN-based check from iteration 1. The system has used the new observation to tighten the diagnostic move.

### Why This Iteration Matters

This iteration is where the case starts to look like real diagnostic progress rather than a restatement of the initial answer.

The new observation does not fully settle the case, but it changes the balance of evidence:

- replication lag now looks more plausible because the symptom is explicitly time-bounded;
- the weaker-snapshot explanation is still plausible, but no longer equally strong;
- the next check becomes more directly tied to the observed behavior.

That is the core continuation behavior we want from the application: preserve prior state, accept a related but not identical new observation, and produce a disciplined update instead of starting over.

## Iteration 3: How Lag Correlation Further Tightened The Update

Step sequence on this iteration:

1. `observation_boundary_resolver`
   Why: decide whether the new message is a usable continuation observation and rewrite it into a standalone context-resolved observation.
2. `observation_extraction`
   Why: turn that resolved observation into explicit atomic facts that the continuation pipeline can reason over.
3. `information_adequacy_analyzer`
   Why: decide whether the extracted observation set is diagnostically sufficient to continue without asking follow-up questions.
4. `candidate_card_retrieval`
   Why: rerun semantic retrieval on the updated continuation query.
5. `card_branch_reranking`
   Why: rerank the retrieved continuation branch so the strongest refreshed primary card and competing context are carried into downstream evidence loading.
6. `card_hydration`
   Why: load the selected structured incident cards from Postgres.
7. `incident_evidence_retrieval`
   Why: retrieve tagged incident chunks from the selected cards.
8. `theory_evidence_retrieval`
   Why: add mechanism-level explanatory evidence.
9. `diagnostic_update_prompt_context_assembly`
   Why: build a continuation-oriented prompt context around the prior diagnostic state and the new observation.
10. `llm_structured_generation`
   Why: produce the next structured diagnostic update.

### Input

```text
CloudWatch shows replica lag spikes at the same moments when the stale reads from the reader endpoint happen, and the stale-read window shrinks again when the lag returns to normal.
```

### Observation Boundary Resolution

This step is again model-based. It both normalizes the continuation observation and checks that the new input is in fact observation-like rather than a question or an unrelated statement.

On this iteration the resolver keeps the message in the same diagnostic lane and turns it into this standalone continuation observation:

```text
Replica lag spikes coincide with stale reads from the reader endpoint, and the stale-read window shrinks when lag returns to normal.
```

This is a stronger continuation signal than the second iteration because it adds correlation: the stale-read window is now explicitly tied to measured lag spikes rather than only to a brief delay after commit.

### Observation Extraction

The `resolved_observation` is then passed into the model-based `observation_extraction` step, which turns one continuation message into a set of explicit atomic observations.

```json
{
  "normalized_user_input": "CloudWatch shows replica lag spikes at the same moments when the stale reads from the reader endpoint happen, and the stale-read window shrinks again when the lag returns to normal.",
  "resolved_observation": {
    "text": "Replica lag spikes coincide with stale reads from the reader endpoint, and the stale-read window shrinks when lag returns to normal."
  },
  "confidence": "High",
  "observations": [
    {
      "statement": "Replica lag spikes coincide with stale reads from the reader endpoint",
      "confidence": "High",
      "condition": null,
      "polarity": "present",
      "time_relation": null,
      "source_span": "Replica lag spikes coincide with stale reads from the reader endpoint"
    },
    {
      "statement": "stale-read window shrinks",
      "confidence": "High",
      "condition": null,
      "polarity": "present",
      "time_relation": "when lag returns to normal",
      "source_span": "the stale-read window shrinks when lag returns to normal"
    }
  ],
  "needs_more_context": false,
  "missing_context_questions": [],
  "token_usage": {
    "prompt_tokens": 418,
    "completion_tokens": 439,
    "total_tokens": 857
  }
}
```

This step matters because the next stage, `information_adequacy_analyzer`, needs a structured observation set rather than free-form continuation text.

### Information Adequacy Analyzer

As in the second iteration, this is a deterministic step rather than a model call.

It uses the extracted observation set to decide whether the continuation signal is:

- `Blocking`
- `WeakButRunnable`
- `Sufficient`

The decision uses the same general cues as before:

- `observation_count`
- `present/absent/corrected`
- `question_count`
- confidence levels
- `needs_more_context`

On this iteration:

- `observation_count = 2`
- both observations are `High`
- `needs_more_context = false`
- `missing_context_questions = []`

So the new signal is sufficient to continue. The important difference from iteration 2 is that the extracted observations are now not just temporal, but also correlational.

### Candidate Retrieval

The primary precedent again remains the same:

- `amazon_rds_postgresql_17_4_jepsen_2025_04_29`

The alternative cards change again:

- `cockroachdb_beta_20160829_jepsen_2017_02_16`
- `scylla_4_2_rc3_jepsen_2020_12_23`

For semantic search on continuation turns, the retrieval query is built from the previous iteration's problem understanding plus the new observation as a simple string concatenation.

This step does not manually replace cards. It reruns candidate search against that updated continuation query, rebuilds the ranked list, keeps the top hit as the primary card, and refreshes the alternatives from the remaining ordered hits.

The retrieval shift also says something useful about the case. The continuation signal is now less about a generic read anomaly and more about observable lag-correlated divergence, so the competing context shifts toward systems known for stale-read surprises and weaker-than-expected consistency semantics under timing-sensitive conditions.

### Card Hydration

As in the previous iterations, the selected `card_id`s are hydrated into full structured cards from Postgres.

That means the continuation step again works with the same structured card artifact as the rest of the pipeline, rather than switching to a looser free-text-only mode.

### Incident Evidence Retrieval

This step again filters within the selected cards:

- first by the selected `card_id`s;
- then by the configured tag families inside those cards.

The primary evidence still comes from the Amazon RDS card, but the continuation signal pushes the useful tag mix even more toward tags that capture correlation and update dynamics:

- `chunk_role:timeline`
- `chunk_role:symptom_change`
- `chunk_role:investigation`
- `chunk_role:diagnostic_step`
- `chunk_role:hypothesis_update`

Alternative evidence again comes from the competing-card families used to preserve non-primary explanations:

- `chunk_role:failure_mode`
- `chunk_role:root_cause`
- `chunk_role:contributing_factor`
- `chunk_role:uncertainty`
- `chunk_role:lesson`

At this point the evidence is no longer just saying "reader lags for a few seconds." It is now saying that the stale-read window covaries with measured lag. That makes diagnostic-step and hypothesis-update style chunks especially useful.

### Theory Retrieval

The theory retriever again adds mechanism-level evidence on top of the practical precedents.

Here it is especially useful because the weaker-snapshot hypothesis is still alive, but now it is being pressured by more concrete operational evidence. Theory helps keep that mechanism legible without letting it dominate the update.

### Prompt Context Assembly

This is again the continuation module `diagnostic_update_prompt_context_assembly`, not the initial-turn `prompt_context_assembly`.

It builds a continuation-oriented prompt context around:

- the previous iteration's `problem_understanding`;
- the `resolved_observation`;
- the extracted atomic `observations`;
- the prior diagnostic state:
  - `active_hypotheses`
  - `rejected_hypotheses`
  - `last_check`
- a compact prompt-facing view of the primary incident card;
- selected incident evidence buckets;
- selected theory evidence.

From trace:

- primary card: Amazon RDS PostgreSQL 17.4 Jepsen analysis
- alternative cards: CockroachDB beta-20160829 and Scylla 4.2-rc3
- selected total chunk count: `6`
- prompt asset: `diagnostic_update_response_prompt_baseline`
- prompt asset version: `v5`
- rendered prompt chars: `17309`

The role-based packing remains:

- one chunk for `evidence_for_match`
- one chunk for `next_check_hint`
- one chunk for `supporting_explanation`
- two chunks for `alternative_context`
- one chunk for `mechanism_explanation`

The `next_check_hint` bucket matters here because the continuation update is now ready to move from a generic discriminating check to a direct lag-correlation check.

The system again rebuilds the prompt context from the updated continuation state instead of reusing the previous prompt unchanged.

### Diagnostic Update Produced

The generation step uses the continuation prompt assembled above and applies the same policy constraints as the prior iterations, including:

- `return strict JSON only`
- `do not claim final root cause`
- `give exactly one first check`
- `state uncertainty when evidence is not unique`

From trace:

- prompt tokens: `3945`
- completion tokens: `1604`
- total tokens: `5549`

Raw model output:

```json
{
  "competing_interpretation": "The reader may be using a weaker Parallel Snapshot Isolation that allows its snapshot to exclude recent writes even though replication is caught up.",
  "first_check": "Query the replica’s replication lag (e.g., pg_stat_replication or CloudWatch metric) at the moment a stale read is observed to determine if lag is elevated.",
  "hypotheses": [
    {
      "confidence": "high",
      "id": "d7e8f9a0-b1c2-3d4e-5f6a-7b8c9d0e1f2a",
      "rejection_reason": null,
      "source": "primary_incident",
      "status": "active",
      "text": "The reader lags behind the primary because asynchronous WAL replay temporarily falls behind, causing replication‑lag spikes that produce stale reads."
    },
    {
      "confidence": "low",
      "id": "c1a2f3e4-5b6d-7e8f-9a0b-1c2d3e4f5a6b",
      "rejection_reason": null,
      "source": "theory_mechanism",
      "status": "weakened",
      "text": "The reader endpoint uses a weaker Parallel Snapshot Isolation, so its snapshot may omit recent writes even when replication appears caught up."
    }
  ],
  "problem_understanding": "The cluster exhibits brief periods after commits where the writer endpoint returns the new value but the reader endpoint still returns the old value; these stale‑read intervals coincide with spikes in replica replication lag and shrink when lag returns to normal.",
  "result_interpretation": {
    "inconclusive_if": "If lag measurements are unavailable or ambiguous, the result does not clearly separate the hypotheses.",
    "supports_competing_if": "If the measured lag is low or within normal range when the stale read occurs, the weaker snapshot‑isolation hypothesis remains plausible.",
    "supports_primary_if": "If the measured lag is high (seconds of delay) at the time of the stale read, this strengthens the replication‑lag hypothesis."
  },
  "similar_practical_context": "Similar to known replication‑lag‑induced read‑your‑writes anomalies observed in asynchronous PostgreSQL replica setups."
}
```

The resulting update matters for three reasons:

- `problem understanding` now explicitly includes correlation between stale reads and lag spikes;
- the replication-lag hypothesis stays primary and becomes more specific;
- the weaker-snapshot hypothesis is still present, but now weakened further to low confidence and sourced through theory/mechanism rather than the main practical branch.

The suggested check also becomes more operationally direct. The system no longer asks for an indirect read-path probe. It now asks for lag measurement at the moment of the anomaly.

### Why This Iteration Matters

This is the iteration where the case becomes much tighter.

The new observation does not just add another symptom. It adds a relationship between the symptom and a measurable operational signal:

- stale reads happen when lag spikes;
- the stale-read window shrinks when lag returns to normal.

That relationship is exactly the kind of continuation evidence that should move a hypothesis from plausible to strongly favored without pretending that the case is fully closed.

## What This Case Shows

This run is a good example of the application behaving as intended.

- It keeps one strong primary precedent while still reopening the competing context after each new observation.
- It does not treat continuation as free-form chat. Each update goes through boundary resolution, atomic observation extraction, sufficiency checking, retrieval refresh, and prompt reassembly.
- It becomes more specific over time without becoming overconfident too early. The weaker-snapshot hypothesis is not dropped immediately; it is weakened as better lag-correlated evidence arrives.
- The suggested check evolves in the same direction: from a broad discriminating probe to a direct operational lag measurement tied to the observed anomaly.

The main takeaway is that the final answer is not produced in one shot. It emerges by preserving state, integrating new observations, and rebuilding the evidence pack around each continuation update.
