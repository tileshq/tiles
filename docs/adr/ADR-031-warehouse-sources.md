# ADR-031: Tiles reads warehouse tables and dbt models through tiles-warehouse; the AI writes the SELECT and Tiles dry-runs and guards every query before it runs

- Status: Accepted; its refusal of every persistent routine, and the consequence that a user-defined SQL function cannot be called, are superseded by [ADR-033](ADR-033-warehouse-query-path.md), which reads each routine with `routines.get` and calls SQL and JavaScript functions; ADR-033 also sets the time budget at 300 s; its part 4 is [ADR-034](ADR-034-warehouse-surface.md)
- Date: 2026-10-19
- Extends: [ADR-008](ADR-008-data-sources.md), whose "Tiles adds no SQL of its own" held for semantic layers; [ADR-001](ADR-001-product-scope.md)'s filters-in-stage-2, for warehouse queries; [ADR-005](ADR-005-privacy-and-secrecy.md) and [ADR-024](ADR-024-security-baseline-in-ci.md)'s single network owner

## Context

The owner, 2026-10-19, on where Tiles goes: "simple 1 tile open source... -> mini dashboard (private) -> complex dashboard -> (private) -> hosted window to share -> (private + paid) -> universal semantic layer (private + paid) this also includes connection with dbt and other warehouses ... lightning fast, open source etc."

A dbt team on BigQuery commonly builds prod into the prod project's `dbt_*` datasets, each developer's builds into a personal dataset, and a merge request's into a dataset of its own. A semantic layer such as Omni reads prod marts only, so charts of dev and merge-request builds must come from BigQuery directly. A developer logs in with their own Google account (Application Default Credentials, `authorized_user`).

The design review of 2026-10-19 proposed that Tiles write the SQL from a closed field grammar. The owner pointed out that the harness's own model already writes the query, and that real questions join base, staging, intermediate and mart models through CTEs, which Tiles should handle. A field grammar cannot express a join, so the AI writes the query and Tiles guards it. On cost: "no cap on any budget, unlimited so dont restrict it on any type of query budget." On filters: "lets do C cuz our queries are complex." On design generally: "just pick the best option, I leave it to you! We want speed, craft and a polished product and the best user experience."

On the open-core split (decision 8a of the split plan, [ADR-038](ADR-038-open-core-split.md)), the BigQuery and dbt source is private and paid: `tiles-warehouse` moves to the private `tiles-pro` repository at the split, and this first part adds the seam it registers through.

## Decision

### The source

- **A new crate, `tiles-warehouse`**, holds a `Warehouse` trait (dialect, describe a table, estimate a query, run it), with BigQuery as its first and only `Dialect`. Snowflake and Postgres each add one implementation and one dialect later, by record.
- **The AI writes the query.** The intent carries one GoogleSQL `SELECT`: CTEs, joins across staging, intermediate and mart models, any expression BigQuery accepts. dbt lookup tools let the AI find models, columns and targets, and Tiles rewrites a model name to its relation for the chosen target (prod, dev or `mr:<iid>`) from the manifest dbt wrote for it, never from a derived name.
- **Filters are a time window plus equality filters**, an exception to ADR-001 for warehouse queries, since a table with `require_partition_filter` refuses any query without one.
- **The MR target** resolves through `dbt parse`: Tiles prints the command from settings the user set, and the AI runs it through its own shell behind the harness's approval. Tiles never runs dbt.

### The guards on every query

Every query is dry-run first. Tiles refuses it before it runs when:

- BigQuery's `statementType` for it is anything but `SELECT`;
- it references a table outside the projects the user allowed;
- it calls a persistent routine: the dry run names the routines a query calls but not whether one is a remote function, so Tiles calls none;
- it calls an external, ML or AI function, which would send rows or the query to another service: anything in the `ML`, `AI` or `OBJ` namespace, `EXTERNAL_QUERY`, `EXTERNAL_OBJECT_TRANSFORM` or `VECTOR_SEARCH`.

The dry run's job statistics are the first check where BigQuery reports them (the statement type, the referenced tables and the referenced routines). A deny-list over the SQL text is the second, so a function the statistics do not name is still caught: it skips comments and string literals, reads a backquoted name as written, and refuses text it cannot read the way BigQuery would, such as a string that never ends. The guards are pure functions in `crates/tiles-warehouse/src/policy.rs` over those statistics and that text, tested without a network.

- **No bytes cap and no cost refusal.** The dry run stays for its errors and its byte estimate, which the result reports; nothing refuses a query for its size.
- **The row limit per chart stays** (5,000 by default, 10,000 at most, as Omni's), and a warehouse query gets a longer time budget than Omni's 60 s.

### Network and credentials

- **`tiles-net` holds the HTTP client every source shares**: ureq with rustls on ring and the platform verifier, HTTPS only, no redirects, a 10 s connect timeout, one deadline per call, three attempts on connect errors and 429, 502, 503 and 504, and a size cap on every response. `tiles-omni` and `tiles-warehouse` use it; it stays in the public repository.
- **The network owners** in `scripts/check-network-owners.py` become `tiles-net`, `tiles-omni` and `tiles-warehouse`. Shipped code may name exactly two new hosts, `bigquery.googleapis.com` and `oauth2.googleapis.com`, with no wildcard, so `evil.googleapis.com` still fails the check.
- **Credentials** are the user's own ADC file, read in memory; the refresh token, client secret and access token get types like `ApiKey`, with `Debug` and `Display` printing `[redacted]`, and no error message carries a secret or a row value.

### Approvals and rows

- Warehouse rows follow [ADR-013](ADR-013-data-stays-local.md), [ADR-027](ADR-027-omni-rows-stay-in-memory.md) and [ADR-028](ADR-028-keep-the-model-out-of-the-cache.md) as Omni's do: values stay off the model unless the user opts in, no rows reach the disk, and every Tiles tool asks before it runs while a warehouse is configured.
- **Pi asks before a data call.** Warehouse tools stay off in Pi until its extension asks before a warehouse-backed chart, as it does for Omni since ADR-028.

### The source seam

A data source beyond Omni reaches `tiles-cli` and `tiles-mcp` through one trait, `tiles_mcp::source::Source`, and one registration point, `tiles_cli::main(&[&dyn Source])`. The `tiles` binary's entry point (`crates/tiles-cli/src/main.rs`) passes the list, and the `tiles_cli` library names no source but Omni. A source has a name, which is its `tiles <name>` command, a usage line, `tiles doctor` lines, and the command itself; the query path and the MCP tools add their methods to the same trait in later parts. In this part the entry point passes an empty list and `tiles-cli` does not depend on `tiles-warehouse`; the warehouse registers through the seam in part 4, when it has commands and doctor lines to offer. At the open-core split that registration moves to `tiles-pro`'s entry point beside `tiles-warehouse`, and the public binary's list stays empty.

## Consequences

- A chart is as right as the AI's query. Text-to-SQL scores 10 to 30% on real warehouses without a semantic layer (ADR-008's research), so the dbt lookup tools, the card's subtitle naming the target and relation, and the user's own reading are the controls on correctness; the guards control only what a query may touch.
- A query can bill as much as BigQuery allows; the result names the dry run's estimate so the user sees it.
- The textual deny-list can refuse a query whose own names look like a denied call; the refusal says which name matched. A user-defined SQL function cannot be called until a later record lets Tiles tell it from a remote one.
- This record ships in four parts: the records, the network allow-list, `tiles-net`, the `tiles-warehouse` skeleton and the seam (part 1); ADC login, token refresh, the replay server and the dbt reader (part 2); the intent source, `tables.get`, the dry run, run, poll and cancel (part 3); the warehouse's registration through the seam with its MCP tools, instruction text, `tiles warehouse` commands and doctor lines, install settings and `docs/warehouse.md` (part 4).
- Tests in part 1: `scripts/test_policy_scripts.py` for the two hosts and their look-alikes; `crates/tiles-omni/tests/replay.rs`, unchanged, across the move to `tiles-net`; `crates/tiles-warehouse` for hostile names, planted secrets and every guard, with a property test over spliced queries and the fuzz target `warehouse_sql`; `crates/tiles-cli/src/tests.rs` for the seam.
- Related: [ADR-001](ADR-001-product-scope.md), [ADR-005](ADR-005-privacy-and-secrecy.md), [ADR-008](ADR-008-data-sources.md), [ADR-009](ADR-009-security-baseline.md), [ADR-013](ADR-013-data-stays-local.md), [ADR-018](ADR-018-no-keys-on-our-servers.md), [ADR-024](ADR-024-security-baseline-in-ci.md), [ADR-027](ADR-027-omni-rows-stay-in-memory.md), [ADR-028](ADR-028-keep-the-model-out-of-the-cache.md).
