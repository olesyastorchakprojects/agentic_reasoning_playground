## 1) Purpose / Scope

`hybrid_search_client` defines the transport-level Qdrant client for hybrid dense+sparse vector search.

This client:
- builds hybrid search HTTP requests for Qdrant;
- executes outbound hybrid search calls to Qdrant;
- applies shared retry rules;
- validates hybrid-search response shape;
- returns raw payload-bearing hits for higher layers to map.

This client does not:
- know cards, practice chunks, or theory chunks;
- build embeddings from user query text;
- build sparse vectors from text;
- map payloads into domain-specific result types;
- create collections, write points, or manage aliases.

## 2) Public Structure

The module must expose a concrete public structure:

```rust
pub struct QdrantHybridSearchClient {
    http_client: reqwest::Client,
    qdrant_base_url: url::Url,
    retry_policy: RetryPolicyConfig,
}
```

The module must expose a public constructor:

```rust
impl QdrantHybridSearchClient {
    pub fn new(
        qdrant_base_url: url::Url,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, HybridSearchClientError>;
}
```

Constructor rules:
- the constructor must create and own a `reqwest::Client`;
- the constructor must reject invalid configuration through `HybridSearchClientError`;
- `qdrant_base_url` must use `http` or `https`;
- `qdrant_base_url` must contain a host;
- `qdrant_base_url` must not contain query parameters;
- `qdrant_base_url` must not contain a fragment;
- `retry_policy.max_attempts` must be greater than zero;
- retry behavior must follow `Specification/runtime/api_clients/client_common.md`.

## 3) Public Method

The module must expose one public async method:

```rust
impl QdrantHybridSearchClient {
    pub async fn search(
        &self,
        request: &HybridSearchRequest,
    ) -> Result<HybridSearchResponse, HybridSearchClientError>;
}
```

Method responsibility:
- execute one hybrid Qdrant search request against one collection;
- return raw ordered hits with payload attached;
- preserve Qdrant hit order in the returned response.

## 4) Input And Output Types

Required public types from shared contract:

- `QdrantCollectionName`
- `QdrantVectorName`
- `RetryPolicyConfig`
- `Embedding`
- `SparseVector`
- `QdrantMatchAnyFilter`
- `QdrantFilter`
- `RawQdrantPayload`
- `RawQdrantHit`

Source of truth:
- `Specification/runtime/api_clients/qdrant/shared_types.md`

Hybrid-search-specific public types:

```rust
pub struct HybridSearchRequest {
    pub collection_name: QdrantCollectionName,
    pub embedding: Embedding,
    pub sparse_vector: SparseVector,
    pub vector_name: QdrantVectorName,
    pub sparse_vector_name: QdrantVectorName,
    pub filter: Option<QdrantFilter>,
    pub limit: usize,
    pub score_threshold: f32,
}

pub struct HybridSearchResponse {
    pub hits: Vec<RawQdrantHit>,
}
```

Type rules:
- `Embedding.values` must not be empty;
- `SparseVector.indices` and `SparseVector.values` must be aligned arrays of equal length;
- `SparseVector.indices` and `SparseVector.values` must not be empty;
- `limit` must be greater than zero;
- `score_threshold` must be provided explicitly by the caller;
- negative `score_threshold` is an invalid request;
- `NaN`, `+inf`, and `-inf` threshold values are invalid requests;
- `RawQdrantHit` must preserve returned Qdrant score and payload only;
- collection-specific payload mapping is outside this client.

Filter mapping rules:
- `QdrantFilter.must_match_any` must be encoded as Qdrant `filter.must`;
- each `QdrantMatchAnyFilter` must be encoded as:

```json
{
  "key": "<field_name>",
  "match": {
    "any": ["value1", "value2"]
  }
}
```

- when `filter = None`, the outbound request must omit `filter` entirely.

## 5) External Service Usage

Base Qdrant API:
- `https://api.qdrant.tech/`

The client must use:
- `POST /collections/{collection_name}/points/query`

Collection-name path rule:
- `collection_name` is a URL path segment and must be safely path-encoded when constructing the request URL.

The client must build hybrid requests using:
- `prefetch`
- `query.fusion = "rrf"`

Hybrid request-shape rules:
- `prefetch` must contain exactly two branches in this order:
  - dense branch first;
  - sparse branch second;
- the dense branch must use `embedding.values`;
- the sparse branch must use `sparse_vector.indices` and `sparse_vector.values`;
- both prefetch branches must use the same branch `limit` value as top-level `limit` in the current version;
- top-level `query` must be exactly `{ "fusion": "rrf" }` in the current version.

The client must set:
- one dense prefetch branch using `vector_name`;
- one sparse prefetch branch using `sparse_vector_name`;
- `limit`
- `with_payload = true`
- `with_vector = false`

The client may set:
- `score_threshold`
- `filter`

Vector-name rules:
- the dense prefetch branch must set `using` to `vector_name`;
- the sparse prefetch branch must set `using` to `sparse_vector_name`.

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
          "text": "example payload"
        }
      }
    ]
  },
  "status": "ok",
  "time": 0.01
}
```

## 6) Response Validation Rules

The client must validate:
- HTTP status is `2xx`;
- response body parses as JSON object;
- `result.points` exists and is an array;
- each returned point contains `score`;
- each returned point contains `payload`.

Validation rules:
- missing `result.points` is an invalid response;
- missing `score` is an invalid response;
- missing `payload` is an invalid response;
- unsupported payload shapes are invalid responses;
- extra top-level response fields must be ignored;
- extra hit-level fields must be ignored;
- missing top-level `status` is not an error;
- missing top-level `time` is not an error;
- Qdrant hit `id` is not required by this client and must be ignored when present;
- payload is returned as transport-level typed raw data and is not domain-mapped by this client.

Empty-result rule:
- zero hits is a valid successful result;
- if Qdrant returns an empty `result.points` array, the client must return `HybridSearchResponse { hits: vec![] }`.

## 7) Retry Behavior

Retry behavior must follow:
- `Specification/runtime/api_clients/client_common.md`

## 8) Error Model

The module must define:

```rust
pub enum HybridSearchClientError {
    InvalidRequest(&'static str),
    Transport(String),
    UnexpectedStatus(u16),
    InvalidResponse(&'static str),
}
```

Failure-category rules:
- `InvalidRequest` covers request shapes the client must reject before making an outbound call;
- `Transport` covers network, timeout, and low-level HTTP execution failures;
- `UnexpectedStatus` covers non-success HTTP status codes returned by Qdrant;
- `InvalidResponse` covers malformed or incomplete Qdrant response bodies.

Public error rules:
- raw third-party errors must not leak through the public interface;
