## 1) Purpose / Scope

`dense_search_client` defines the transport-level Qdrant client for dense vector search.

This client:
- builds dense search HTTP requests for Qdrant;
- executes outbound dense search calls to Qdrant;
- applies shared retry rules;
- validates dense-search response shape;
- returns raw payload-bearing hits for higher layers to map.

This client does not:
- know cards, practice chunks, or theory chunks;
- build embeddings from user query text;
- build sparse vectors;
- map payloads into domain-specific result types;
- create collections, write points, or manage aliases.

## 2) Public Structure

The module must expose a concrete public structure:

```rust
pub struct QdrantDenseSearchClient {
    http_client: reqwest::Client,
    qdrant_base_url: url::Url,
    retry_policy: RetryPolicyConfig,
}
```

The module must expose a public constructor:

```rust
impl QdrantDenseSearchClient {
    pub fn new(
        qdrant_base_url: url::Url,
        retry_policy: RetryPolicyConfig,
    ) -> Result<Self, DenseSearchClientError>;
}
```

Constructor rules:
- the constructor must create and own a `reqwest::Client`;
- the constructor must reject invalid configuration through `DenseSearchClientError`;
- `qdrant_base_url` must use `http` or `https`;
- `qdrant_base_url` must contain a host;
- `qdrant_base_url` must not contain query parameters;
- `qdrant_base_url` must not contain a fragment;
- `retry_policy.max_attempts` must be greater than zero;
- retry behavior must follow `Specification/runtime/api_clients/client_common.md`.

## 3) Public Method

The module must expose one public async method:

```rust
impl QdrantDenseSearchClient {
    pub async fn search(
        &self,
        request: &DenseSearchRequest,
    ) -> Result<DenseSearchResponse, DenseSearchClientError>;
}
```

Method responsibility:
- execute one dense Qdrant search request against one collection;
- return raw ordered hits with payload attached;
- preserve Qdrant hit order in the returned response.

## 4) Input And Output Types

Required public types from shared contract:

- `QdrantCollectionName`
- `QdrantVectorName`
- `RetryPolicyConfig`
- `Embedding`
- `QdrantMatchAnyFilter`
- `QdrantFilter`
- `RawQdrantPayload`
- `RawQdrantHit`

Source of truth:
- `Specification/runtime/api_clients/qdrant/shared_types.md`

Dense-search-specific public types:

```rust
pub struct DenseSearchRequest {
    pub collection_name: QdrantCollectionName,
    pub embedding: Embedding,
    pub vector_name: Option<QdrantVectorName>,
    pub filter: Option<QdrantFilter>,
    pub limit: usize,
    pub score_threshold: f32,
}

pub struct DenseSearchResponse {
    pub hits: Vec<RawQdrantHit>,
}
```

Type rules:
- `Embedding.values` must not be empty;
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

The client must set:
- `query`
- `limit`
- `with_payload = true`
- `with_vector = false`

The client may set:
- `score_threshold`
- `filter`
- `using`, when `vector_name` is present

Vector-name rules:
- when `vector_name = None`, the outbound request must omit `using` entirely;
- when `vector_name = Some(...)`, the outbound request must set `using` to that configured vector name.

Dense request example:

```json
{
  "query": [0.1, 0.2, 0.3],
  "using": "dense",
  "filter": {
    "must": [
      {
        "key": "card_id",
        "match": {
          "any": ["mongodb_4_2_6"]
        }
      }
    ]
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
- if Qdrant returns an empty `result.points` array, the client must return `DenseSearchResponse { hits: vec![] }`.

## 7) Retry Behavior

Retry behavior must follow:
- `Specification/runtime/api_clients/client_common.md`

## 8) Error Model

The module must define:

```rust
pub enum DenseSearchClientError {
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
