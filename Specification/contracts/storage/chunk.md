# Chunk Schema

This document describes the payload schema for a chunk stored as a point in Qdrant.

## Notes

- Format: JSON object
- One object = one chunk / one Qdrant point payload
- `ingest` fields are optional at chunk creation time and are filled during ingest

## Schema

```json
{
  "schema_version": "integer",
  "doc_id": "string",
  "chunk_id": "string",
  "url": "string",
  "document_title": "string",
  "section_title": "string | null",
  "section_path": ["string"],
  "chunk_index": "integer",
  "page_start": "integer",
  "page_end": "integer",
  "tags": ["string"],
  "content_hash": "string",
  "chunking_version": "string",
  "chunk_created_at": "string (ISO 8601 datetime)",
  "text": "string",
  "ingest": {
    "embedding_model": "string",
    "embedding_model_dimension": "integer",
    "ingest_config_version": "string",
    "ingested_at": "string (ISO 8601 datetime)"
  }
}
```

## Field Definitions

| Field | Type | Required | Filled at stage | Description |
| --- | --- | --- | --- | --- |
| `schema_version` | `integer` | yes | chunking | Payload schema version. Current value is `1`. |
| `doc_id` | `string` | yes | chunking | Stable document identifier shared by all chunks of the same document. |
| `chunk_id` | `string` | yes | chunking | Stable unique chunk identifier. |
| `url` | `string` | yes | chunking | Source location for the document. It may be a local URL or local host address. |
| `document_title` | `string` | yes | chunking | Human-readable document title. |
| `section_title` | `string \| null` | no | chunking | Title of the leaf section containing the chunk, if available. |
| `section_path` | `string[]` | no | chunking | Full section path for navigation and filtering, ordered from top-level section to leaf section. |
| `chunk_index` | `integer` | yes | chunking | Zero-based chunk order within the document. |
| `page_start` | `integer` | yes | chunking | First page covered by the chunk. |
| `page_end` | `integer` | yes | chunking | Last page covered by the chunk. Equal to `page_start` for single-page chunks. |
| `tags` | `string[]` | no | chunking | Free-form tags for filtering or grouping. |
| `content_hash` | `string` | yes | chunking | Hash of the `text` field exactly as stored in the payload, without preprocessing or normalization. Useful for deduplication and re-ingest. |
| `chunking_version` | `string` | yes | chunking | Version of the chunking logic that produced the chunk. |
| `chunk_created_at` | `string` | yes | chunking | Chunk creation timestamp in ISO 8601 format. |
| `text` | `string` | yes | chunking | Chunk text used for embedding and retrieval. |
| `ingest` | `object` | no | ingest | Ingest-time metadata. May be absent before ingest. |
| `ingest.embedding_model` | `string` | no | ingest | Embedding model used when the vector was generated. |
| `ingest.embedding_model_dimension` | `integer` | no | ingest | Embedding vector dimension used when the vector was generated. |
| `ingest.ingest_config_version` | `string` | no | ingest | Version of the ingest configuration used during ingest. |
| `ingest.ingested_at` | `string` | no | ingest | Ingest timestamp in ISO 8601 format. |

## Conventions To Freeze

- `section_path`: use a string array ordered from root to leaf, for example `["Part I", "Chapter 1", "Communication"]`
- `chunk_index`: indexing starts from `0`
- `content_hash`: hash the `text` field exactly as stored, without trimming, normalization, or metadata
- `url`: store the source location of the document; it may be an external URL, local file URL, or local host address

## Versioning Rules

- `schema_version` is the version of the payload contract.
- `chunking_version` is the version of the chunking logic that produced the payload.

Increase `schema_version` when the payload contract changes:
- a field is added or removed
- a field type changes
- a field becomes required or optional
- a field changes its meaning or representation

Increase `chunking_version` when the chunker behavior changes but the payload contract stays the same:
- chunk boundaries change
- section assignment changes
- heading or anchor logic changes
- text generation changes in a way that affects `text` or `content_hash`

Current convention:
- `schema_version` is an integer, for example `1`
- `chunking_version` is a string label, for example `"v1"`

This difference is intentional:
- `1` means contract version for machines and validation
- `"v1"` means producer / algorithm version for humans and debugging

If this feels too asymmetric later, both fields can be normalized to the same style, but for now the semantic split is useful and explicit.

## Minimal Valid Example

```json
{
  "schema_version": 1,
  "doc_id": "2e9b0ce6-b07e-4b1c-9918-289627c74577",
  "chunk_id": "b48b106e-58ab-4e14-a0ae-c46b607e9e24",
  "url": "local://Understanding-Distributed-Systems-2nd-Edition.pdf",
  "document_title": "Understanding Distributed Systems (2nd Edition)",
  "section_title": "Introduction",
  "section_path": ["Introduction", "Introduction"],
  "chunk_index": 0,
  "page_start": 19,
  "page_end": 19,
  "tags": ["distributed-systems", "book", "architecture"],
  "content_hash": "sha256:...",
  "chunking_version": "v1",
  "chunk_created_at": "2026-02-17T09:00:00Z",
  "text": "Chapter 1 Introduction ...",
  "ingest": {
    "embedding_model": "text-embedding-3-large",
    "embedding_model_dimension": 3072,
    "ingest_config_version": "v1",
    "ingested_at": "2026-02-17T09:05:00Z"
  }
}
```
