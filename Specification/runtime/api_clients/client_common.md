## 1) Purpose / Scope

This document defines retry behavior shared by runtime API clients.

In the current version it defines only:
- retry ownership;
- retryable failure classes;
- required retry/backoff implementation rules.

This document does not define:
- client-specific request/response shapes;
- client-specific payload mapping;
- client-specific constructor fields;
- observability behavior.

## 2) Shared Retry Rules

Runtime API clients may retry transient outbound calls.

Retry behavior must be configuration-driven.

Each client-specific config that supports retries must define:
- `max_attempts`
- `backoff`

`backoff` must be represented as a typed internal configuration value.
Unchecked stringly typed retry mode selection is forbidden.

## 3) Retryable Failure Classes

Clients may retry only failures that are likely transient.

Retryable failures:
- transport failure;
- connection failure;
- timeout;
- HTTP `5xx`;
- HTTP `429`, if the current client supports rate-limit retry handling.

Non-retryable failures:
- invalid request built by the client;
- HTTP `4xx` other than `429`;
- invalid response body shape;
- payload mapping failure;
- explicit unsupported feature or unsupported configuration.

## 4) Backoff Rules

Retry execution must use the `backon` crate.

Handwritten retry loops are forbidden.

Retry execution must be built from a shared reusable retry helper or retry policy abstraction.

When `backoff = exponential`:
- delay between attempts must grow exponentially;
- bounded jitter must be applied;
- jitter must not remove exponential growth behavior.

## 5) Exhaustion Rule

When all retry attempts are exhausted, the client must return its own public error type.

Raw third-party retry or transport errors must not leak through the public client interface.
