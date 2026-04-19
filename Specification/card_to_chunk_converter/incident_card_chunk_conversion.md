## 1) Purpose / Scope

This document defines the v1 contract for converting canonical incident cards
into a `chunks.jsonl` file compatible with the chunk payload schema.

This contract defines:
- the canonical source of cards for conversion;
- the required CLI shape of the Python converter script;
- the exact mapping from `IncidentCard` into chunk records;
- the rules for building retrieval text stored in `chunk.text`;
- validation, determinism, and output-file behavior.

This contract does not define:
- Qdrant collection creation;
- embedding generation;
- sparse-vector construction;
- Qdrant upsert behavior;
- retrieval or ranking behavior;
- hybrid ingest config authoring;
- PostgreSQL storage schema for incident cards.

In the current architecture:
- PostgreSQL stores the canonical `IncidentCard`;
- the converter reads canonical cards from PostgreSQL;
- the converter writes a pre-ingest `chunks.jsonl` file;
- downstream ingest reads that file and adds ingest-time metadata later.

## 2) Canonical Source Of Truth

The canonical source of truth for incident cards is PostgreSQL.

Rules:
- canonical cards must first be stored in PostgreSQL;
- all downstream processes, including card-to-chunk conversion, must read cards
  only from PostgreSQL;
- human-friendly exports may exist, but they are derived artifacts and are not
  the canonical source of truth.

For the current version, the canonical storage table is:
- `diagnostics.incident_cards`

## 3) Source Card Contract

The source semantic object for conversion is:
- `Specification/contracts/storage/incident_card.md`

The storage contract for PostgreSQL column layout and SQL types is:
- `Specification/contracts/storage/incident_cards_storage.md`

The machine-readable schema for the source card is:
- `Execution/schemas/incident_card.schema.json`

The converter reads incident-card fields from SQL columns in
`diagnostics.incident_cards`.

Rules:
- the converter must use the table columns as the primary input source for
  conversion;
- the converter must construct the incident-card-shaped input object from the
  table fields defined by the incident card schema;
- the converter must interpret PostgreSQL column types according to the
  incident-card storage contract, including `jsonb` storage for list-valued
  fields and nullable SQL columns for optional scalar fields;
- the converter must not use `card_json` as the primary input source for v1;
- the constructed object must then be validated against the incident card
  schema before chunk conversion.

## 4) Output Chunk Contract

The converter output is a newline-delimited JSON file where each line is one
chunk object.

The output chunk contract is:
- `Specification/contracts/storage/chunk.md`

The machine-readable chunk schema is:
- `Execution/schemas/chunk.schema.json`

Output rules:
- one line = one JSON object;
- the file must be encoded as UTF-8;
- the file must contain one chunk object per line;
- the current version produces pre-ingest chunks, not post-ingest enriched
  chunks.

## 5) Python Script Interface

The converter script is a Python CLI program.

The current version must accept:
- `--postgres-url`
- `--output-path`
- `--incident-card-schema-path`
- `--chunk-schema-path`

Rules:
- `--postgres-url` provides the PostgreSQL connection string used to read cards;
- `--output-path` provides the target `chunks.jsonl` path;
- `--incident-card-schema-path` provides the filesystem path to the incident
  card JSON schema used for input validation;
- `--chunk-schema-path` provides the filesystem path to the chunk JSON schema
  used for output validation.

The current version must not accept:
- a partial selection filter;
- an alternate table name;
- a Qdrant URL;
- embedding settings;
- sparse configuration settings.

The source table name must be fixed in code as:
- `diagnostics.incident_cards`

## 6) Input Read Scope And Ordering

The current version reads all cards from the canonical source table.

Rules:
- the converter must read all cards from `diagnostics.incident_cards`;
- the current version does not support conversion of only a subset of cards;
- output records must preserve the input order returned by the converter's
  source read path;
- the implementation must use one stable read order consistently so that
  repeated runs over unchanged source data remain deterministic;
- for v1, the converter must read rows using `ORDER BY case_id ASC`.

## 7) One-Card-To-One-Chunk Rule

For the current version:
- one incident card produces exactly one chunk record;
- the converter must not split a single card into multiple chunks;
- the converter must not merge multiple cards into one chunk.

Identity rules:
- `case_id` is the canonical stable identity of the card;
- the converter must use the name `case_id` directly and must not introduce
  alias names such as `card_id`.

## 8) Canonical Field Mapping

The required field mapping for each produced chunk is:

| Chunk field | Value source / rule |
| --- | --- |
| `schema_version` | integer literal `1` |
| `doc_id` | `IncidentCard.case_id` |
| `chunk_id` | `IncidentCard.case_id` |
| `url` | synthetic local URL `local://incident_cards/<title>` |
| `document_title` | `IncidentCard.title` |
| `section_title` | string literal `"incident_card"` |
| `section_path` | array literal `["incident_card"]` |
| `chunk_index` | integer literal `0` |
| `page_start` | integer literal `1` |
| `page_end` | integer literal `1` |
| `tags` | omitted in v1 |
| `content_hash` | hash of final stored `text` using `sha256:<hex>` |
| `chunking_version` | string literal `"v1"` |
| `chunk_created_at` | conversion-time timestamp in UTC, serialized as ISO 8601 with second precision and `Z` suffix |
| `text` | retrieval text assembled by the rules in this contract |
| `ingest` | omitted in pre-ingest output |

Additional rules:
- `doc_id`, `chunk_id`, and PostgreSQL `case_id` must all carry the exact same
  identity value;
- `document_title` must carry the exact title value from the canonical
  incident card;
- `page_start` and `page_end` are technical sentinel values for card-derived
  chunks in the current version.

## 9) URL Derivation Rule

The converter must populate `chunk.url` using a synthetic local URL:
- `local://incident_cards/<title>`

Rules:
- the `<title>` portion must be based on `IncidentCard.title`;
- the `<title>` portion must be normalized by trimming leading and trailing
  whitespace, replacing embedded newlines with spaces, and collapsing repeated
  internal whitespace to a single space;
- the current contract freezes the conceptual form of the synthetic URL;
- if the implementation later needs a more constrained path-safe encoding,
  that change must be treated as a contract change if it changes stored output.

## 10) Tags Policy

In the current version:
- `tags` must not be emitted;
- the field must be omitted entirely from produced chunk objects.

Rationale:
- `tags` are optional in the chunk contract;
- the current version does not use tag-based filtering for card-derived chunks;
- omitting the field keeps the converter contract simpler and more explicit.

## 11) Retrieval Text Purpose

`chunk.text` is the retrieval-oriented text representation derived from the
canonical incident card.

Rules:
- `chunk.text` must be a deterministic structured text document;
- it must be assembled from a fixed subset of incident-card fields;
- it must not include unapproved extra fields in v1;
- it must be designed for downstream embedding and sparse ingest.

## 12) Retrieval Text Field Set

The converter must build `chunk.text` from exactly these incident-card fields:
- `title`
- `short_summary`
- `canonical_symptoms`
- `affected_components`
- `failure_mode_candidates`
- `diagnostic_patterns`
- `root_cause_summary`
- `violated_properties`
- `claimed_guarantees`
- `mitigations_or_workarounds`

Field-presence notes:
- `root_cause_summary` is optional in the incident-card schema;
- all other fields listed above are required by the current incident-card
  schema.

All incident-card fields not listed above must be excluded from `chunk.text`
in v1.

## 13) Retrieval Text Layout

`chunk.text` must be assembled as labeled sections, not as unlabeled
concatenation.

Each included section must use this format:
- `Label: value`

The fixed section order is:
1. `Title`
2. `Summary`
3. `Canonical symptoms`
4. `Affected components`
5. `Failure mode candidates`
6. `Diagnostic patterns`
7. `Root cause summary`
8. `Violated properties`
9. `Claimed guarantees`
10. `Mitigations or workarounds`

Section-to-field mapping:
- `Title` <- `title`
- `Summary` <- `short_summary`
- `Canonical symptoms` <- `canonical_symptoms`
- `Affected components` <- `affected_components`
- `Failure mode candidates` <- `failure_mode_candidates`
- `Diagnostic patterns` <- `diagnostic_patterns`
- `Root cause summary` <- `root_cause_summary`
- `Violated properties` <- `violated_properties`
- `Claimed guarantees` <- `claimed_guarantees`
- `Mitigations or workarounds` <- `mitigations_or_workarounds`

## 14) Retrieval Text Normalization

Normalization must make output deterministic and clean without semantically
rewriting source content.

Scalar-string normalization rules:
- trim leading and trailing whitespace;
- replace embedded newlines with spaces;
- collapse repeated internal whitespace to a single space.

Nullable-scalar rules:
- if a nullable scalar field value is `null`, its section must be omitted
  entirely;
- if a nullable scalar field becomes empty after normalization, its section
  must be omitted entirely.

List-item normalization rules:
- normalize each list item using the same scalar-string rules;
- drop list items that become empty after normalization;
- deduplicate list items while preserving first-occurrence order;
- join normalized list items using `; `.

List-section rules:
- if a list-valued field is empty before normalization, its section must be
  omitted entirely;
- if a list-valued field becomes empty after item-level normalization, its
  section must be omitted entirely.

Section rules:
- omit empty sections entirely;
- do not emit blank labeled sections;
- join sections using newline characters;
- do not insert extra unlabeled prose.

Forbidden normalization behavior:
- do not paraphrase field content;
- do not lowercase full field values as a normalization step;
- do not reorder sections;
- do not reorder list items except for deduplication-by-first-occurrence;
- do not semantically transform the source text.

## 15) Content Hash Rules

The converter must populate `content_hash` using:
- `sha256:<hex>`

Hashing rules:
- hash the final stored `chunk.text` exactly as written;
- hashing must happen after normalization and section assembly;
- identical valid input cards must produce identical `chunk.text`;
- identical valid input cards must therefore produce identical `content_hash`
  values.

## 16) Validation Rules

The converter must validate both the source card and the produced chunk.

Input validation rules:
- each input card must validate against the incident card schema supplied via
  `--incident-card-schema-path`;
- the converter must not continue conversion of an invalid card.

Output validation rules:
- each produced chunk must validate against the chunk schema supplied via
  `--chunk-schema-path`;
- the converter must not write invalid chunk objects to the output file.

The current validation boundary is intentionally simple:
- incident-card schema for input validity;
- chunk schema for output validity.

## 17) Failure Behavior

The current version uses fail-fast behavior.

Rules:
- the converter must abort immediately on the first validation or conversion
  failure;
- the converter must not skip invalid cards and continue;
- the converter must not write partial ambiguous output beyond what was
  already durably written before the failure point;
- the converter must write diagnostics and error messages to `stderr`;
- because the current version fails if the target file already exists, reruns
  remain operationally explicit.

## 18) Output File Behavior

The converter writes one output file.

Rules:
- output format is newline-delimited JSON objects in UTF-8;
- each line must contain one chunk object;
- the converter must fail if the target output file already exists;
- the converter must not overwrite an existing file;
- the converter must not append to an existing file;
- the converter must write to a temporary file in the target directory and move
  that file into place only after successful completion of the full conversion
  run;
- the temporary filename must be `<output-path>.tmp`.

## 19) Ingest Field Policy

The converter produces pre-ingest chunk objects.

Rules:
- the converter must not populate the optional `ingest` object;
- the `ingest` field must be absent from pre-ingest output;
- downstream ingest is responsible for adding ingest-time metadata.

## 20) Timestamp Serialization Rule

The current version uses one fixed serialization form for
`chunk_created_at`.

Rules:
- `chunk_created_at` must be serialized in UTC;
- the serialized value must use ISO 8601 date-time format;
- the serialized value must use second precision;
- the serialized value must end with `Z`.

## 21) Compatibility With Downstream Ingest

The converter output must be directly consumable by downstream ingest without
additional conversion rules.

For the current version, compatibility means:
- retrieval text is stored in `chunk.text`;
- identity fields are written exactly as defined in this contract;
- `content_hash` is derived from final stored `chunk.text`;
- produced chunks conform to the chunk schema expected by downstream ingest.

This contract does not embed hybrid-ingest configuration itself.

## 22) Config Dependency Scope

The converter itself is independent from hybrid ingest config.

Rules:
- the converter does not require hybrid ingest config to perform card-to-chunk
  conversion;
- ingest config is prepared separately for the downstream ingest step;
- conversion and ingest-config preparation are separate concerns in v1.

## 23) Minimal Operational Flow

The current version operational flow is:
1. connect to PostgreSQL using `--postgres-url`;
2. read all canonical cards from `diagnostics.incident_cards` using
   `ORDER BY case_id ASC`;
3. validate each card against the incident-card schema;
4. convert each card into exactly one chunk object;
5. validate each produced chunk against the chunk schema;
6. write the chunk objects to a temporary UTF-8 JSONL file in the target
   directory;
7. move the completed temporary file into `--output-path`;
8. stop immediately if any step fails.

## 24) Minimal Valid Example

Illustrative example:

```json
{
  "schema_version": 1,
  "doc_id": "mongodb_4_2_6_case",
  "chunk_id": "mongodb_4_2_6_case",
  "url": "local://incident_cards/MongoDB primary failover write disruption",
  "document_title": "MongoDB primary failover write disruption",
  "section_title": "incident_card",
  "section_path": ["incident_card"],
  "chunk_index": 0,
  "page_start": 1,
  "page_end": 1,
  "content_hash": "sha256:...",
  "chunking_version": "v1",
  "chunk_created_at": "2026-04-19T12:00:00Z",
  "text": "Title: MongoDB primary failover write disruption\nSummary: Clients observed transient write failures during failover.\nCanonical symptoms: write failures; primary election; transient unavailability\nAffected components: primary node; replica set; client writes\nFailure mode candidates: failover gap; election window\nDiagnostic patterns: errors spike during topology transition\nRoot cause summary: Clients continued to target a node during primary transition before the topology stabilized.\nViolated properties: availability\nClaimed guarantees: automatic failover\nMitigations or workarounds: retry writes; tune client failover handling"
}
```

This example is illustrative only.
The normative rules are the mapping and normalization rules above.

## 25) Non-Goals

This contract does not define:
- how Qdrant collections are created;
- how embeddings are generated;
- how sparse vectors are generated;
- how chunks are upserted into Qdrant;
- how retrieval scoring works;
- how ranking or prompting uses the produced chunks.

The contract is limited to:
- canonical card-to-chunk conversion;
- pre-ingest chunk-file generation.
