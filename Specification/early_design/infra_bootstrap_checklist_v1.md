# Infrastructure Bootstrap Checklist for V1

This checklist captures the non-code setup needed for the first working iteration of the project.

## Must Have

- `Qdrant` collection for incident cards.
- `Qdrant` collection or collections for report chunks and theory chunks.
- `Qdrant` alias strategy for painless reindex and cutover.
- `Postgres` tables for canonical incident cards.
- `Postgres` tables for diagnostic sessions and loop state.
- `Postgres` tables for step history, model call metadata, and retrieval audit records.
- storage location for source documents and canonical card files.
- configuration layout for local development: endpoints, collection names, model names, feature flags.
- bootstrap script or documented command sequence to create collections, aliases, tables, and indexes.
- ingest path for cards into `Postgres`.
- ingest path for searchable projections into `Qdrant`.
- health checks for `Qdrant`, `Postgres`, and model endpoint.

## Strongly Recommended Early

- structured application logs with stable `request_id` and `session_id`.
- tracing for one end-to-end diagnostic loop.
- metrics for retrieval count, model latency, token usage, step count, and stop reason.
- Grafana dashboard provisioning for this project.
- saved manual test artifacts: packed prompt, evidence bundle, model response, evaluation notes.
- migration mechanism for `Postgres`.
- collection rebuild workflow for re-embedding and alias switch.

## Nice To Have Soon After

- object storage for larger source artifacts if local disk becomes awkward.
- retention policy for prompts, responses, and diagnostic traces.
- feature flags for retrieval packing strategies and prompt variants.
- background jobs for card reindex and corpus refresh.
- offline replay runner for evaluating historical sessions.

## Notes

- Keep canonical card bodies outside `Qdrant`; use `Postgres` or file storage as the source of truth.
- Use `Qdrant` for ranking and candidate selection, not as the only durable store of card content.
- The observability stack from the previous project can be reused if this project emits its own service labels, trace attributes, and dashboard definitions.
- One Grafana instance can serve multiple projects if dashboards are provisioned as separate folders or providers and each project uses distinct labels, datasources, or dashboard namespaces.
