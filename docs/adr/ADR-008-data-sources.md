# ADR-008: v0.1 reads inline rows or an Omni topic through Omni's API; company data never leaves the user's machine

- Status: Accepted; its per-setting lookup of the Omni URL and key is superseded by [ADR-013](ADR-013-data-stays-local.md); extended by [ADR-031](ADR-031-warehouse-sources.md), which adds warehouse tables and dbt models as a third source whose SQL the AI writes and Tiles guards, moves the Omni client's HTTP rules to `tiles-net`, and makes `tiles-omni` one of three network owners
- Date: 2026-10-17

## Context

The owner, 2026-10-17 22:10: Tiles must chart from the Omni topics a company already models its metrics in. A chart from a governed topic is right by construction; a chart from rows the model typed or SQL it wrote may not be (research: text-to-SQL scores 10 to 30% on real warehouses without a semantic layer). Development happens on a separate Mac, which must never see company data or credentials.

Omni's public REST API (docs.omni.co, fetched 2026-10-17):

| Need | Endpoint |
|---|---|
| Base URL | `https://{instance}.omniapp.co/api` |
| Auth | `Authorization: Bearer <key>`, an organization API key or a personal access token |
| Models | `GET /v1/models` |
| Topics in a model | `GET /v1/models/{modelId}/topic` |
| One topic, with its views and fields | `GET /v1/models/{modelId}/topic/{topicName}` |
| Run a semantic query | `POST /v1/query/run`; with `resultType` set (`csv`, `json`, `xlsx`) it returns one document, otherwise newline-delimited JSON with base64 Arrow results. A slow query returns `408` with `remaining_job_ids` to poll |

## Decision

### Two sources, one engine input

The intent's `data` (ADR-003) is exactly one of:

- **`rows`**: inline rows from the model, at most 10,000.
- **`omni`**: `{ "topic": "...", "fields": ["view.field", ...], "sorts": [...], "limit": n }`, where every field is one the topic exposes. Tiles runs the query and binds the rows straight to the engine.

The model finds topics and fields through a second MCP tool that lists topics and their fields with labels, types and descriptions. That is model metadata, not data. Query results go to the engine, not back to the model; the model sees the card and its short text result (ADR-005).

### The Omni client

- Lives in its own crate, `tiles-omni`, the only crate allowed a network client (`scripts/check-network-owners.py`).
- HTTPS only, certificate verification on, no redirects to another host, a connect timeout of 10 s and a total timeout of 60 s including the `408` polling.
- Uses `resultType: "json"` in v0.1, which keeps Arrow out of the binary. Chart-sized results are small; `limit` defaults to 5,000 and is capped at 10,000.
- Queries carry the topic's own default filters and no others: filters are stage 2 (ADR-001).
- Every response is parsed into typed structs, and a response that does not match is an error naming the endpoint, not a guess.

### Credentials

- From the environment (`TILES_OMNI_URL`, `TILES_OMNI_API_KEY`) or the macOS Keychain (service `tiles.omni`), never from a file Tiles writes, the intent, or a command-line argument.
- The key type implements no `Display`, prints as `[redacted]` in `Debug`, and is never logged or included in an error or a tool result. A test plants a fake key and asserts it appears in no output.

### Development without company data

- The client is built and tested against Omni's public API docs, with synthetic fixtures shaped like the documented responses under `fixtures/omni/`.
- Tests run against a local replay server that serves those fixtures and refuses any request it has no fixture for.
- No real Omni instance, key or row is used on the development Mac. The first real call happens on a user's machine, against their own instance.

## Consequences

- A chart from Omni is only as right as the topic, which is the point: Tiles adds no SQL of its own.
- If the docs and the real API differ, the first real run against a live instance finds it; the error messages name the endpoint and field so the user can report it without sharing data.
- A later version may stream Arrow for large results, and may reach other semantic layers (dbt, Cube) through their own source crates, each added to the network allow-list by ADR.

Sources: https://docs.omni.co/api/authentication.md ; https://docs.omni.co/api/models/list-models.md ; https://docs.omni.co/api/topics/list-topics.md ; https://docs.omni.co/api/topics/retrieve-a-topic.md ; https://docs.omni.co/api/queries/run-query.md (fetched 2026-10-17).
