# Agentic Reasoning Golden Dataset Seed 10 — Qdrant Split v4

This version fixes graded relevance shape and score scale.

Changes from v3:

- `graded_relevance` is now an array of objects, not a map.
- Chunk relevance uses this explicit shape:

```json
{
  "chunk_id": "ecaebcf6-12bc-47b7-9b93-40a30598fb87",
  "score": 1.0
}
```

- Vocabulary relevance uses:

```json
{
  "term": "lost_writes",
  "score": 1.0
}
```

- Card relevance uses:

```json
{
  "card_id": "etcd_3_4_3_jepsen_2020_01_30",
  "score": 1.0
}
```

- The only allowed scores are `1.0`, `0.5`, and `0.0`.
- Removed the previous `0.25` card fallback score.
- `expected_theory_evidence.mechanism_explanation.graded_relevance` now includes explicit `0.0` chunks.
- Theory evidence chunks are selected from the structural chunker output:
  `Evidence/parsing/understanding_distributed_systems/chunks/chunks.jsonl`.

Incident evidence still reflects the two Qdrant calls:

1. `incident_evidence.primary`
2. `incident_evidence.alternatives`

Metric runner rule:

- Use explicit `score` from `graded_relevance` when present.
- Treat any retrieved item not present in `graded_relevance` as `0.0`.

Files:

- `metadata.json`
- `questions.json`
- `golden_cases.json`
- `golden_cases.jsonl`
- `schema_notes.json`
- `README.md`


## v5 change

`expected_query_structuring.*.graded_relevance` is now complete over the relevant controlled vocabulary.

For each case:

- `symptoms.graded_relevance` contains every term from `canonical_symptoms`
- `affected_subsystems.graded_relevance` contains every term from `affected_components`
- `failure_modes.graded_relevance` contains every term from `failure_mode_candidates`
- `system_properties.graded_relevance` contains every term from `violated_properties`

Scores:

- `1.0` = strict expected vocabulary term
- `0.5` = soft acceptable vocabulary term
- `0.0` = explicit non-relevant vocabulary term for this query

This makes query-structuring graded relevance complete rather than positive-only.


## v6 change

`expected_query_structuring.*.graded_relevance` is compact again:

- max 10 items per query-structuring field
- sorted by score descending: `1.0`, then `0.5`, then `0.0`
- still uses the same score scale: `1.0 / 0.5 / 0.0`

This avoids full-vocabulary expansion inside every case while still giving explicit positive and negative labels for eval runner development.
