# DuckDB driver E2E

Optional WebdriverIO specs for the DuckDB path driver. Not included in default `pnpm e2e`.

## Prerequisites

- Build with DuckDB compiled in: `DATAZEN_DRIVERS=all pnpm tauri:build --debug --features webdriver`
- A DuckDB database file (or use `:memory:` via file path in a temp directory)

## Environment

| Variable | Description |
|----------|-------------|
| `E2E_DUCKDB_PATH` | Path to `.duckdb` file (created if missing when spec runs) |
| `E2E_SKIP_DUCKDB` | Set to `1` to force skip |

## Run

```bash
E2E_DUCKDB_PATH=/tmp/datazen-e2e.duckdb \
  pnpm e2e:skip-build -- --spec packages/drivers/duckdb/e2e/duckdb-smoke.ts
```

Without `E2E_DUCKDB_PATH`, the spec skips cleanly.
