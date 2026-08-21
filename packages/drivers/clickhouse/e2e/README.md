# ClickHouse driver E2E

Optional WebdriverIO specs for the ClickHouse path driver. Not included in default `pnpm e2e`.

## Prerequisites

- Build with ClickHouse compiled in: `DATAZEN_DRIVERS=all pnpm tauri:build --debug --features webdriver`
- HTTP interface reachable (default port 8123)

## Environment

| Variable | Description |
|----------|-------------|
| `E2E_CLICKHOUSE_HOST` | Host (default `127.0.0.1`) |
| `E2E_CLICKHOUSE_PORT` | Port (default `8123`) |
| `E2E_SKIP_CLICKHOUSE` | Set to `1` to force skip |

## Run

```bash
E2E_CLICKHOUSE_HOST=127.0.0.1 E2E_CLICKHOUSE_PORT=8123 \
  pnpm e2e:skip-build -- --spec packages/drivers/clickhouse/e2e/clickhouse-smoke.ts
```

Without host env or when unreachable, the spec skips cleanly.
