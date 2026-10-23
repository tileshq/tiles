# ADR-033: A warehouse query reads its dbt models by ref(), calls only SQL and JavaScript functions, runs for at most five minutes and reads one row past its limit

- Status: Accepted; [ADR-034](ADR-034-warehouse-surface.md) decides that the cards of one tool call share the time budget
- Date: 2026-10-20
- Amends: [ADR-031](ADR-031-warehouse-sources.md): its refusal of every persistent routine, and the consequence that a user-defined SQL function cannot be called, are superseded here; it also fixes the time budget ADR-031 left as "longer than Omni's 60 s"

## Context

ADR-031 has the AI write one GoogleSQL `SELECT` and Tiles guard it. Its first part refused every persistent routine the dry run names, because the dry run says a query calls `acme-prod.udfs.to_eur` but not whether that is a SQL function or a remote one that sends rows to another service. ADR-031 recorded the cost as a consequence: a user-defined SQL function cannot be called. Analytics SQL leans on such functions (a currency conversion, a fiscal calendar), so the refusal turned away ordinary queries.

`routines.get` answers the question the dry run leaves open: a routine's `routineType`, its `language`, and `remoteFunctionOptions` when it is remote. This part, the query path from an intent's `data.warehouse` to rows, adds that call and settles what ADR-031 left open: how a query names a dbt model, how long it may run, and how a result longer than a chart is reported.

## Decision

### Naming dbt models

- A query writes a dbt model as dbt's own model files do, `{{ ref('fct_payments') }}`, `{{ ref('package', 'fct_payments') }}` or either with `v=2`, and names any other table in full, `` `project.dataset.table` ``. Like dbt, Tiles reads a placeholder wherever it sits, comments and strings included, and refuses any other `{{ ... }}`, `source()` among them.
- `data.warehouse.target` (`prod`, `dev`, `mr:<iid>`, `mr:<iid>:full`) is needed with a `ref()` and refused without one, so a query that names its tables in full never looks as if a target applied to it.
- Tiles replaces each `ref()` with the relation the target's manifest names, through the dbt reader, and checks it with `tables.get`. A `404` is refused by target: for `dev`, build the model; for an MR, a partial build holds only the models the MR changed; for `prod`, refresh the prod manifest. A dev model whose SQL file changed after its table was built gets a note, not a refusal.
- ADR-031's time window and equality filters are the query's own `WHERE`: the AI writes them, so the intent has no filter key of its own.

### Routines

- `routines.get` joins the closed list of calls Tiles makes: the token endpoint, `tables.get`, `routines.get`, `jobs.insert` (the dry run and the run), `jobs.getQueryResults` and `jobs.cancel`.
- For each persistent routine the dry run names, Tiles reads it first and refuses a remote function (`remoteFunctionOptions`), anything that is not a function (a procedure), a function in a language other than SQL and JavaScript or with `externalRuntimeOptions` (Python and Java can reach the network), and a routine `routines.get` cannot find. A plain SQL or JavaScript function passes.
- A SQL function's body passes the same text deny-list as the query, and the routines its body calls are read and checked the same way, each once, however many there are. A body's `project.dataset.name(...)` must find a routine; a body's `dataset.name(...)` is read in the routine's own project, and one that finds none is taken for a built-in function, since BigQuery would fail the query otherwise.
- A dry run that reports `externalServiceCosts` is refused as a call to a service outside BigQuery.
- BigQuery's `referencedTables` leaves tables out for a query reading more than 50, so a dry run listing 50 or more is refused: Tiles cannot check that every table the query reads is in an allowed project.

### Running

- **The time budget is 300 s**, five times Omni's 60 s, since a query joining staging, intermediate and mart models can scan for minutes. `TILES_WAREHOUSE_TIMEOUT` or the `warehouse.timeout_seconds` setting changes it, from 10 to 3,600 s; the variable wins. One budget covers `tables.get`, the dry run, `routines.get`, the run and the rows.
- The run is `jobs.insert` with the job's own `jobTimeoutMs` set to what is left of the budget, so BigQuery stops it even if Tiles has gone. Tiles then calls `getQueryResults`, each call waiting up to 10 s on BigQuery's side, until `jobComplete`. When the budget ends, or reading fails after the job started, Tiles calls `jobs.cancel`, says whether BigQuery took it, and names the configured budget, not the time left when the run started.
- The row limit stays as ADR-031 has it (5,000 by default, 10,000 at most) and is read as limit + 1, across pages where BigQuery splits them, so a longer result is cut with the note "The query returned more than N rows; Tiles kept the first N."
- A `401` refreshes the token once; a `500` is tried once more; `tiles-net` retries rate limits and gateway errors as before. A retried insert that BigQuery already has (`409`) reads the job it started.
- The billing project is `TILES_WAREHOUSE_PROJECT` or `warehouse.billing_project`; the location is `TILES_WAREHOUSE_LOCATION` or `warehouse.location`, and without one BigQuery picks it from the tables; the projects a query may read are `warehouse.projects`.

### Results and messages

- A result has at most 16 columns, each named once, of a type the engine draws, checked on the dry run's schema before the query runs. `INT64`, `FLOAT64`, `NUMERIC` and `BIGNUMERIC` become numbers and a non-finite one is refused; `BOOL` a boolean; `STRING`, `DATE`, `DATETIME` and `TIME` text as BigQuery writes it; `TIMESTAMP`, read as microseconds, ISO 8601 text in UTC. `BYTES`, `GEOGRAPHY`, `JSON`, `INTERVAL`, `RANGE`, a `STRUCT` and an `ARRAY` are refused by name, with a `CAST` as the fix.
- The messages of the dry run, `tables.get` and `routines.get` describe the query and the schema, so Tiles passes them on, sanitized and cut to 300 characters; a policy tag, a refused table, a missing table, a dataset in another location and a billing project the account cannot run jobs in get their own fix. A run's message can quote a cell, so Tiles reports only its reason and job id and points at `bq show -j <id>`.
- The dry run's byte estimate is kept for the result. There is still no bytes cap and no cost refusal.

## Consequences

- A SQL function such as `acme-prod.udfs.to_eur` runs, at the cost of one `routines.get` per routine before the query, its own and the ones they call; the time budget bounds those calls.
- A query reading 50 tables or more is refused, even when every table is in an allowed project.
- A body's `dataset.name(...)` that names neither a routine in the routine's project nor a built-in fails in BigQuery, not in Tiles' guard.
- A query that runs the whole budget holds the tool call for five minutes; the setting can shorten it, and part 4 decides how a dashboard's cards share it.
- Tests: `crates/tiles-intent/src/tests.rs` and `tests/property.rs` for the source and its references; `crates/tiles-warehouse/src/policy.rs` for every routine rule; `crates/tiles-warehouse/tests/bigquery.rs` and `tests/query.rs` over the replay server for `tables.get`, the dry run, routines, polling, the deadline and cancel, the cut, the `500` retry, a failed run and every column type; `tests/property.rs` for result pages; the fuzz targets `warehouse_source` and `bigquery_rows`.
- Related: [ADR-001](ADR-001-product-scope.md), [ADR-008](ADR-008-data-sources.md), [ADR-013](ADR-013-data-stays-local.md), [ADR-024](ADR-024-security-baseline-in-ci.md), [ADR-031](ADR-031-warehouse-sources.md).
