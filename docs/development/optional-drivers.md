# Optional path drivers

DataZen ships a **Basic** SKU with four core drivers (`postgres`, `mysql`, `sqlite`, `redis`). Additional database engines live as **path drivers** under `packages/drivers/` and are listed in [`drivers-registry.json`](../../drivers-registry.json) at the repo root.

This document covers the four optional engines called out in the P2 roadmap: **MongoDB**, **ClickHouse**, **DuckDB**, and **SQL Server**. Git-based drivers (Kiwi, OLAP, Superset) are documented in [`plugin-development.md`](independent-plugin-development.en.md).

## Inventory (2026-08-21)

| Registry id | Source | Crate | Category | In Basic SKU | In All SKU |
|-------------|--------|-------|----------|--------------|------------|
| `mongodb` | path (`packages/drivers/mongodb`) | `datazen-driver-mongodb` | document | no | yes |
| `clickhouse` | path | `datazen-driver-clickhouse` | sql | no | yes |
| `duckdb` | path | `datazen-driver-duckdb` | sql | no | yes |
| `sqlserver` | path | `datazen-driver-sqlserver` | sql | no | yes |

All four are **path drivers** (not git clones). They register at link time via `inventory` and appear in the frontend `DB_REGISTRY` only when selected at build time.

## Enabling at build / dev time

Driver selection is compile-time. Use either the CLI flag or the environment variable (env wins only when the flag is omitted):

```bash
# All path drivers (MongoDB, ClickHouse, DuckDB, SQL Server, Elasticsearch, …)
DATAZEN_DRIVERS=all pnpm tauri:dev

# Explicit subset
pnpm tauri:dev --drivers=postgres,mongodb,clickhouse,duckdb,sqlserver

# Default when omitted: basic = postgres, mysql, sqlite, redis
pnpm tauri:dev
```

Presets are resolved in [`scripts/resolve-drivers.mjs`](../../scripts/resolve-drivers.mjs):

| Preset | Registry ids |
|--------|----------------|
| `basic` (default) | `postgres`, `mysql`, `sqlite`, `redis` |
| `all` | every **path** entry in `drivers-registry.json` (excludes git drivers) |
| `stub` | empty registry (codegen smoke tests) |

Release SKUs (see [packaging.md](packaging.md) and GitHub release notes):

| SKU | Drivers |
|-----|---------|
| **Basic** | postgres, mysql, sqlite, redis |
| **All** | all path drivers (includes the four above; **excludes** Kiwi / OLAP / Superset) |
| **Akulaku** | Basic + mongodb + kiwi + superset |

## What the Host expects (zero hardcoding)

The Host does **not** branch on `pluginId === 'mongodb'` (or similar). Behavior comes from each driver's `ui/meta.ts` → generated `DB_REGISTRY` and from the Driver Command API:

| Capability | Host entry | Driver responsibility |
|------------|------------|------------------------|
| Schema browse | Connection tree, `list_objects` / `get_tables` | Implement `get_databases`, `get_tables`, `get_table_schema` |
| Query / execute | SQL editor, `execute_driver_command` (`query` / `execute`) | Implement `query`, `execute`; non-SQL engines use JSON command bodies |
| Export | `DataTable` export, batch export | Standard `QueryResult` rows; no per-driver Host export code |
| Connection UI | `connectionView`: `sql` \| `document` \| `keyvalue` | Set in `meta.ts` |
| EXPLAIN | Query panel when `supportsExplain: true` | Override `supports_explain()` in Rust factory if needed |
| Structure editor | When `structureEditor.enabled` (default true) | Column types / index methods in `meta.ts` |
| Admin DDL | Context menu when `supportsCreateDatabase` etc. | Optional `admin_commands` (e.g. SQL Server) |

Frontend dialect helpers (`src/lib/sqlDialects/extra.ts`) exist for formatting and DDL snippets; they are keyed by `sqlDialect` from meta, not by hardcoded driver lists in feature code.

## Per-driver status

### MongoDB (`mongodb`)

- **Category:** document — `connectionView: 'document'`, `supportsSQL: false`
- **Schema browse:** databases → collections; inferred column types from sample documents
- **Query:** JSON command editor (`find`, aggregation pipeline, insert/update/delete) — see driver crate header in `packages/drivers/mongodb/src/mongodb.rs`
- **Export:** result grid export works like other connections
- **Structure editor:** disabled in meta (`structureEditor.enabled: false`)
- **Data Sync / Transfer:** not supported (document category; pairing rejects non-SQL families)
- **Sync IR adapter:** `MongodbSyncAdapter` exists for future Transfer structure paths

### ClickHouse (`clickhouse`)

- **Category:** SQL — HTTP interface, `sqlDialect: 'clickhouse'`
- **Schema browse:** databases, tables, structure editor with ClickHouse types
- **Query / EXPLAIN:** supported (`supportsExplain: true`)
- **Export:** standard Host export path
- **Backup:** meta marks `supportsBackup: true` (driver-specific backup commands if implemented)
- **Notes:** default `supports_offset()` is true (driver-api); large scans may need driver-specific pagination tuning

### DuckDB (`duckdb`)

- **Category:** SQL — embedded file DB (`connectionMode: 'file'`, `databaseFieldType: 'path'`)
- **Schema browse:** tables/views; optional object kinds include triggers and sequences
- **Query / EXPLAIN:** supported
- **Export:** standard Host export path
- **Notes:** no SSH; single-file workflows similar to SQLite

### SQL Server (`sqlserver`)

- **Category:** SQL — TDS, `sqlDialect: 'sqlserver'`
- **Schema browse:** multi-database; schemas (`dbo` default); routines/triggers/types via `supportedObjectKinds`
- **Query / EXPLAIN:** supported
- **Admin commands:** create/drop database, schema, user (`admin_commands.rs`)
- **Export:** standard Host export path; transaction prefix uses `BEGIN TRANSACTION` via dialect-aware export helper

## Gaps and follow-ups (driver crate scope)

These are **not** Host blockers; improve inside each path driver crate:

| Area | MongoDB | ClickHouse | DuckDB | SQL Server |
|------|---------|------------|--------|------------|
| In-crate unit tests | yes (`mongodb.rs`, `sync_adapter.rs`) | yes (`clickhouse.rs`, `structure.rs`) | yes (`duckdb.rs`, `structure.rs`) | yes (`sqlserver.rs`, `admin_commands.rs`) |
| `tests/` integration | yes (`sync_adapter_smoke`, `command_definitions`; no live server) | yes (`sync_adapter_smoke`, `command_definitions`, `http_query_wiremock`) | yes (`sync_adapter_smoke`, `command_definitions`, `duckdb_embedded_smoke` `:memory:`) | yes (`sync_adapter_smoke`, `command_definitions`; no live server) |
| Driver `e2e/` | yes (`e2e/mongodb-smoke.ts`; skips without env) | yes (`e2e/clickhouse-smoke.ts`; skips without env) | yes (`e2e/duckdb-smoke.ts`; skips without env) | yes (`e2e/sqlserver-smoke.ts`; skips without env) |
| Host contract matrix | N/A (document) | not in `e2e/contract` | not in `e2e/contract` | not in `e2e/contract` |
| Data Sync | rejected (document category) | same-family only when compiled in | same-family only | same-family only |
| Data Transfer execute | N/A (document) | same-family Insert / Truncate+Insert | same-family Insert / Truncate+Insert | same-family Insert / Truncate+Insert |

Host E2E contract journeys intentionally target Basic SQL drivers (PostgreSQL / MySQL / SQLite). Optional SQL drivers should add instance-backed tests under `packages/drivers/<id>/tests/` or `e2e/` when CI fixtures exist.

## Local driver development

See [plugin-development.md](independent-plugin-development.en.md) — path drivers use the same `DatabaseDriver` trait and `.drivers-dev.json` symlink workflow as git plugins, but the source stays in `packages/drivers/<id>/`.

## Related docs

- [AGENTS.md](../../AGENTS.md) — driver selection, codegen, capabilities
- [plugin-development.md](independent-plugin-development.en.md) — git plugins and trait reference
- [packaging.md](packaging.md) — release SKUs and install channels
