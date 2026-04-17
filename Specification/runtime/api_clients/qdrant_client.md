## 1) Purpose / Scope

`qdrant_client` is the runtime read-side client for Qdrant.

This module:
- reads incident card matches from Qdrant;
- reads practice chunks from Qdrant;
- reads theory chunks from Qdrant;
- sends dense or hybrid query requests to Qdrant;
- maps Qdrant response payloads into typed runtime results.

This module does not:
- create collections;
- write or upsert points;
- manage aliases;
- build dense query embeddings;
- build sparse query vectors;
- select cards or pack evidence outside the data returned by search.

## 2) Public Structure

The module must expose a concrete public structure:

```rust
pub struct QdrantRuntimeClient {
    http_client: reqwest::Client,
    config: QdrantRuntimeClientConfig,
}
```

The module must expose a public constructor:

```rust
impl QdrantRuntimeClient {
    pub fn new(config: QdrantRuntimeClientConfig) -> Result<Self, QdrantRuntimeError>;
}
```

Constructor rules:
- the constructor must create and own a `reqwest::Client`;
- the constructor must not accept business request data;
- the constructor must not accept embedding model settings;
- the constructor must not accept tokenizer or sparse-vocabulary settings;
- retry behavior must follow `Specification/runtime/api_clients/client_common.md`.

## 3) Public Methods

The module must expose exactly these public async methods:

```rust
impl QdrantRuntimeClient {
    pub async fn search_incident_cards(
        &self,
        request: &CardSearchRequest,
    ) -> Result<CardSearchResult, QdrantRuntimeError>;

    pub async fn search_practice_chunks(
        &self,
        request: &PracticeChunkSearchRequest,
    ) -> Result<ChunkSearchResult, QdrantRuntimeError>;

    pub async fn search_theory_chunks(
        &self,
        request: &TheoryChunkSearchRequest,
    ) -> Result<ChunkSearchResult, QdrantRuntimeError>;
}
```

Method responsibilities:

`search_incident_cards`
- searches the incident-card collection using one dense query vector;
- returns ranked card candidates in Qdrant response order;
- returns only the minimal card identity needed by downstream runtime logic.

`search_practice_chunks`
- searches the practice-chunk collection using dense or hybrid search;
- restricts retrieval to the supplied `card_ids`;
- restricts retrieval to the supplied `chunk_tags`;
- returns ranked chunk hits for evidence packing.

`search_theory_chunks`
- searches the theory-chunk collection using dense or hybrid search;
- does not require `card_ids`;
- returns ranked theory chunks for optional mechanism explanation.

## 4) Configuration Contract

The constructor config must be:

```rust
pub struct QdrantCollectionName(pub String);

pub struct QdrantVectorName(pub String);

pub struct EmbeddingDimension(pub usize);

pub struct RetryPolicyConfig {
    pub max_attempts: u32,
    pub backoff: RetryBackoffKind,
}

pub struct DenseVectorConfig {
    pub vector_name: Option<QdrantVectorName>,
    pub embedding_dimension: EmbeddingDimension,
}

pub struct HybridVectorConfig {
    pub dense_vector_name: QdrantVectorName,
    pub sparse_vector_name: QdrantVectorName,
    pub dense_embedding_dimension: EmbeddingDimension,
}

pub struct CollectionRoutingConfig {
    pub incident_cards_collection: QdrantCollectionName,
    pub practice_chunks_collection: QdrantCollectionName,
    pub theory_chunks_collection: QdrantCollectionName,
}

pub struct QdrantRuntimeClientConfig {
    pub qdrant_base_url: String,
    pub collections: CollectionRoutingConfig,
    pub card_vectors: HybridVectorConfig,
    pub practice_chunk_vectors: HybridVectorConfig,
    pub theory_chunk_vectors: HybridVectorConfig,
    pub retry_policy: RetryPolicyConfig,
}
```

Configuration rules:
- `qdrant_base_url` must be the base URL used for all outbound Qdrant calls;
- collection names must be configured explicitly through typed collection-name wrappers;
- vector names must be configured explicitly through typed vector-name wrappers;
- card search, practice chunk search, and theory chunk search must all be configured for hybrid-capable retrieval in the current version;
- the constructor must validate that configured embedding dimensions are greater than zero;
- retry fields must follow `Specification/runtime/api_clients/client_common.md`.

## 5) Input And Output Types

Required public types:

```rust
pub struct DenseVector {
    pub values: Vec<f32>,
}

pub struct SparseVector {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

pub struct CardSearchRequest {
    pub dense_vector: DenseVector,
    pub sparse_vector: SparseVector,
    pub limit: usize,
    pub score_threshold: Option<f32>,
}

pub struct PracticeChunkSearchRequest {
    pub dense_vector: DenseVector,
    pub sparse_vector: Option<SparseVector>,
    pub card_ids: Vec<String>,
    pub chunk_tags: Vec<String>,
    pub limit: usize,
    pub score_threshold: Option<f32>,
}

pub struct TheoryChunkSearchRequest {
    pub dense_vector: DenseVector,
    pub sparse_vector: Option<SparseVector>,
    pub limit: usize,
    pub score_threshold: Option<f32>,
}

pub struct CardSearchResult {
    pub hits: Vec<CardSearchHit>,
}

pub struct CardSearchHit {
    pub card_id: String,
    pub score: f32,
}

pub struct ChunkSearchResult {
    pub hits: Vec<ChunkSearchHit>,
}

pub struct ChunkSearchHit {
    pub chunk_id: String,
    pub score: f32,
    pub card_id: Option<String>,
    pub chunk_tags: Vec<String>,
    pub text: String,
}
```

Type rules:
- `limit` must be greater than zero;
- `DenseVector.values` must not be empty;
- `SparseVector.indices` and `SparseVector.values` must be aligned arrays of equal length;
- `SparseVector.indices` and `SparseVector.values` must not be empty;
- `PracticeChunkSearchRequest.card_ids` must not be empty;
- `PracticeChunkSearchRequest.chunk_tags` must not be empty;
- result hit order must preserve Qdrant response order;
- `score` must be the score returned by Qdrant.

Constructor and request validation rules:
- `CardSearchRequest` always represents hybrid search in the current version and therefore must include both `dense_vector` and `sparse_vector`;
- `PracticeChunkSearchRequest` and `TheoryChunkSearchRequest` support both dense-only and hybrid search;
- when `PracticeChunkSearchRequest.sparse_vector = Some(...)`, configured dense and sparse vector names must both exist;
- when `TheoryChunkSearchRequest.sparse_vector = Some(...)`, configured dense and sparse vector names must both exist;
- if a method receives a sparse vector but its configured dense or sparse vector name is missing, the client must fail with `InvalidRequest`;
- the client must validate that each dense vector length matches the configured embedding dimension for the target collection before making the outbound request.

## 6) External Service Usage

Base Qdrant API:
- `https://api.qdrant.tech/`

All three methods must use:
- `POST /collections/{collection_name}/points/query`

All three methods must set:
- `limit`
- `with_payload = true`
- `with_vector = false`

All three methods may set:
- `score_threshold`

Dense-only rule:
- when an optional `sparse_vector = None`, the client must send a dense query request.

Hybrid rule:
- when a method receives both dense and sparse vectors for hybrid search, the client must send a hybrid request using `prefetch` and `query.fusion = "rrf"`.
- in hybrid mode the dense prefetch branch must use the configured dense vector name for the target collection;
- in hybrid mode the sparse prefetch branch must use the configured sparse vector name for the target collection.

### 6.1 Card Search JSON Shape

Hybrid request example:

```json
{
  "prefetch": [
    {
      "query": [0.1, 0.2, 0.3],
      "using": "dense",
      "limit": 5
    },
    {
      "query": {
        "indices": [1, 7, 42],
        "values": [1.0, 1.0, 1.0]
      },
      "using": "sparse",
      "limit": 5
    }
  ],
  "query": {
    "fusion": "rrf"
  },
  "limit": 5,
  "score_threshold": 0.2,
  "with_payload": true,
  "with_vector": false
}
```

Successful response example:

```json
{
  "result": {
    "points": [
      {
        "id": "point-id",
        "score": 0.78,
        "payload": {
          "card_id": "mongodb_4_2_6"
        }
      }
    ]
  },
  "status": "ok",
  "time": 0.01
}
```

### 6.2 Practice Chunk Search JSON Shape

Dense request example:

```json
{
  "query": [0.1, 0.2, 0.3],
  "filter": {
    "must": [
      {
        "key": "card_id",
        "match": {
          "any": ["mongodb_4_2_6", "mysql_8_0_34"]
        }
      },
      {
        "key": "chunk_tags",
        "match": {
          "any": ["chunk_role:symptom", "chunk_role:diagnostic_step"]
        }
      }
    ]
  },
  "limit": 6,
  "score_threshold": 0.2,
  "with_payload": true,
  "with_vector": false
}
```

Hybrid request example:

```json
{
  "prefetch": [
    {
      "query": [0.1, 0.2, 0.3],
      "using": "dense",
      "limit": 6
    },
    {
      "query": {
        "indices": [1, 7, 42],
        "values": [1.0, 1.0, 1.0]
      },
      "using": "sparse",
      "limit": 6
    }
  ],
  "query": {
    "fusion": "rrf"
  },
  "filter": {
    "must": [
      {
        "key": "card_id",
        "match": {
          "any": ["mongodb_4_2_6", "mysql_8_0_34"]
        }
      },
      {
        "key": "chunk_tags",
        "match": {
          "any": ["chunk_role:symptom", "chunk_role:diagnostic_step"]
        }
      }
    ]
  },
  "limit": 6,
  "score_threshold": 0.2,
  "with_payload": true,
  "with_vector": false
}
```

Successful response example:

```json
{
  "result": {
    "points": [
      {
        "id": "point-id",
        "score": 0.54,
        "payload": {
          "chunk_id": "chunk-123",
          "card_id": "mongodb_4_2_6",
          "chunk_tags": ["chunk_role:symptom"],
          "text": "Settings applied at the database or collection level do not automatically transfer into transactional contexts."
        }
      }
    ]
  },
  "status": "ok",
  "time": 0.01
}
```

### 6.3 Theory Chunk Search JSON Shape

Dense request example:

```json
{
  "query": [0.1, 0.2, 0.3],
  "limit": 1,
  "score_threshold": 0.2,
  "with_payload": true,
  "with_vector": false
}
```

Hybrid request example:

```json
{
  "prefetch": [
    {
      "query": [0.1, 0.2, 0.3],
      "using": "dense",
      "limit": 1
    },
    {
      "query": {
        "indices": [1, 7, 42],
        "values": [1.0, 1.0, 1.0]
      },
      "using": "sparse",
      "limit": 1
    }
  ],
  "query": {
    "fusion": "rrf"
  },
  "limit": 1,
  "score_threshold": 0.2,
  "with_payload": true,
  "with_vector": false
}
```

Successful response example:

```json
{
  "result": {
    "points": [
      {
        "id": "point-id",
        "score": 0.61,
        "payload": {
          "chunk_id": "theory-456",
          "chunk_tags": ["mechanism_explanation"],
          "text": "Recovery amplification occurs when clients reconnect and drain backlog into a recovering dependency."
        }
      }
    ]
  },
  "status": "ok",
  "time": 0.01
}
```

## 7) Payload Mapping Contract

Card-search payload rules:
- payload must contain `card_id` as string;
- returned Qdrant hit must contain `score`;
- `CardSearchHit.card_id` must be built from payload `card_id`;
- `CardSearchHit.score` must be built from Qdrant `score`.

Chunk-search payload rules:
- payload must contain `chunk_id` as string;
- payload must contain `chunk_tags` as array of strings;
- payload must contain `text` as string;
- payload may contain `card_id` as string;
- returned Qdrant hit must contain `score`;
- `ChunkSearchHit.score` must be built from Qdrant `score`.

Payload validation rules:
- missing required payload fields is a mapping failure;
- invalid field types is a mapping failure;
- payload that cannot be mapped to the required public types is a mapping failure.

## 8) Error Model

The module must define:

```rust
pub enum QdrantRuntimeError {
    InvalidRequest(&'static str),
    Transport(String),
    UnexpectedStatus(u16),
    InvalidResponse(&'static str),
    PayloadMapping(&'static str),
}
```

Failure-category rules:
- `InvalidRequest` covers request shapes the client must reject before making an outbound call;
- `Transport` covers network, timeout, and low-level HTTP execution failures;
- `UnexpectedStatus` covers non-success HTTP status codes returned by Qdrant;
- `InvalidResponse` covers malformed or incomplete Qdrant response bodies;
- `PayloadMapping` covers payloads that do not satisfy the mapping contract.

Public error rules:
- raw third-party errors must not leak through the public interface;
- retry behavior must follow `Specification/runtime/api_clients/client_common.md`.
