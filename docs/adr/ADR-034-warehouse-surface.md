# ADR-034: The warehouse joins `tiles` and the MCP server through the source seam, with two lookup tools, an approval before every call, one time budget per tool call and no value in a result

- Status: Accepted; its consequences that a result names only what the run processed, that an installed Codex keeps its own tool timeout and that the columns listing marks no policy-tagged column are superseded by [ADR-041](ADR-041-warehouse-bill-policy-tags-codex-timeout-and-whole-refusals.md), which adds the bytes billed, the Codex install's `tool_timeout_sec`, the `[policy tag]` mark and a whole refusal in the CLI
- Date: 2026-10-20
- Completes: [ADR-031](ADR-031-warehouse-sources.md)'s part 4, and decides what [ADR-033](ADR-033-warehouse-query-path.md) left to it: how a dashboard's cards share the time budget

## Context

ADR-031 has the AI write one GoogleSQL `SELECT` and Tiles guard it; ADR-033 built the path from `data.warehouse` to rows. Nothing a user or a harness sees reached that path yet. The owner asked how Tiles would know "which tables, fields, and filters to apply, which joins to do", and that it handle "joining payments data to organization data to support data ... creating CTE's and then joining those CTEs". The model needs to find models, columns and builds before it writes such a query, and the user needs to set the warehouse up, check it and approve each call. ADR-031's source seam, `tiles_mcp::source::Source`, carried only a command and doctor lines, and the warehouse must reach the MCP server through it so that `tiles-cli` names no warehouse and `tiles-warehouse` can move to the private `tiles-pro` at the open-core split.

## Decision

### The seam

- `Source` gains what a configured source offers a session (`offer`: its MCP tools, the sentences it adds to the session instruction, the line it adds to `tiles_chart`'s description and the longest a chart call may take), a call to one of its tools, the rows of a `data.warehouse` intent (`fetch`), its own opt-in for values (`values_to_model`) and the hosts it reaches, for the `tiles doctor` network line. Each has a default, so a source adds only what it serves.
- `tiles_mcp::source::Reach` is what a session can fetch from: Omni when configured, and each source's offer. It decides the tools' hints, the approvals and the instruction, where a bool for Omni did before.
- The warehouse implements `Source` as `tiles_warehouse::surface::Surface`, behind the crate's `surface` feature, which alone pulls in `tiles-mcp`; the fuzz targets build `tiles-warehouse` without it and never link Skia. The `tiles` binary's entry point registers it.
- The warehouse counts as configured when Tiles' settings file has a `warehouse` key or `TILES_WAREHOUSE_PROJECT` is set, read without reaching the network. A configuration that breaks a rule still counts, so the tools keep asking and the refusal names the setting.

### What the model gets

- **`tiles_warehouse_models`** lists the models of one build (`prod`, `dev`, `mr:<iid>`) from the manifest on this machine: name, materialization, table and the first 120 characters of the description, at most 200, narrowed by part of a name. It reads only local files.
- **`tiles_warehouse_columns`** lists one model's or table's columns with their types and dbt descriptions, its rows, size and last change, and the partition column a query must filter on. It calls only `tables.get`. A table named in full must be in an allowed project, so the lookup reaches no project a query could not.
- **The instruction** tells the model to write one `SELECT` with each dbt model as `{{ ref('model') }}` and the build under `target`, to ask the user which build when they have not said, to look models and columns up first and filter on a required partition column, and, when Tiles answers that an MR's manifest is missing, to run the `dbt parse` command it prints once the user approves. It points the model at no shell command that reaches the warehouse.

### Approvals

- With the warehouse configured, `tiles_chart` and the warehouse tools publish `readOnlyHint: false` and `openWorldHint: true`, as ADR-013 has the Omni tools do. `tiles_theme` keeps its own hints: it reaches no data.
- `tiles run` pre-approves Tiles' tools only when the user passed `--allow-<name>` for every source the session reaches (`--allow-omni`, `--allow-warehouse`). `tiles install` writes `ask` for every Tiles tool in OpenCode's config and `prompt` for Codex's tiles server, and `tiles warehouse setup`, once it makes the warehouse configured, gives the installs Tiles made before the same settings, as `tiles omni set-key` does.
- Pi's extension asks before a chart that runs a warehouse query, naming the builds it reads, and before each `tiles_warehouse_models` and `tiles_warehouse_columns` call, naming the build, model or table, with Allow, Allow for this session and Decline; Allow for this session covers both. `tiles run --allow-warehouse pi` sets `TILES_ALLOW_WAREHOUSE=1`, which skips the question. The extension registers the two lookup tools from `tool.json`, which lists them only while the warehouse is configured, and runs `tiles warehouse models` or `tiles warehouse columns` with the arguments as argv, never through a shell.

### Running and results

- **One time budget per tool call.** The cards of one `tiles_chart` call share the warehouse's budget (300 s unless set), counted from when the call began: a dashboard of four warehouse cards takes at most the budget, not four times it. `tiles run codex` sets `tool_timeout_sec` to the budget plus 30 s, so Codex waits out a long query.
- **The card names its build.** A lone warehouse card with no subtitle gets one naming where its rows were read from, `dev build · personal_ada_lovelace.fct_payments · built 2 h ago`, and a dashboard with no note gets that note when every warehouse card shares it, so a chart read from the wrong build shows it in the picture, as ADR-031 counts on. The call file keeps the label, so a redraw in another theme keeps it.
- **No value reaches the model** unless the user sets `TILES_WAREHOUSE_VALUES_TO_MODEL=1`, as with Omni. The result says the rows came from BigQuery and adds notes with no value: each model read, with its build and table, the dry run's estimate, what the run processed, the job id, a dev model changed since its build, and a cut. The call file records `"fetched": ["BigQuery"]` and the query, never a row; the rows stay in the fetching session's memory (ADR-027).
- `tiles mcp`, `tiles card` (Pi's path) and `tiles render` fetch a `data.warehouse` intent's rows through the first configured source that runs warehouse queries; with none configured they refuse with a line pointing at `tiles doctor`.
- **`tiles warehouse`** has `setup` (checks each flag, then saves the `warehouse` settings), `check` (refreshes the login and dry-runs `SELECT 1` in the billing project), `models`, `columns` and `rows`, which prints the row count, each column's kind, the estimate, what the run processed and the job id, never a value. `tiles doctor` names the billing project, location, allowed projects, time budget, the login file's type and path, and the dbt project and its prod and dev manifests with their ages, with no secret.

## Consequences

- A model learns table and column names and descriptions from the two tools, and that metadata reaches its provider; the tools ask before they run.
- `getQueryResults` reports bytes processed, not bytes billed, so the result and `rows` name what the run processed; `bq show -j <job>` gives the bill.
- An installed Codex keeps Codex's own tool timeout, since `codex mcp add` has no flag for it, so outside `tiles run` a long query can outlast Codex's wait.
- The columns listing does not mark a policy-tagged column; the dry run refuses a query that reads one, with its own message.
- Tests: `crates/tiles-mcp/src/server/tests.rs` for the tools with and without a warehouse, a planted row value absent from the result and the call file, and the hostile corpus through the new tool outputs, notes and refusals; `crates/tiles-cli/src/harness.rs`, `install.rs`, `doctor.rs`, `run.rs` and `tests.rs` for approvals, Codex's timeout, `--allow-warehouse`, the doctor lines and the seam; `crates/tiles-warehouse/tests/surface.rs` over the replay server for every command and tool, each guard refusing before a job runs, and no planted secret or cell in any output; `adapters/pi/index.test.ts` for Pi's question before charts and lookups.
- Related: [ADR-005](ADR-005-privacy-and-secrecy.md), [ADR-011](ADR-011-surfaces-v0-1.md), [ADR-013](ADR-013-data-stays-local.md), [ADR-027](ADR-027-omni-rows-stay-in-memory.md), [ADR-028](ADR-028-keep-the-model-out-of-the-cache.md), [ADR-031](ADR-031-warehouse-sources.md), [ADR-033](ADR-033-warehouse-query-path.md).
