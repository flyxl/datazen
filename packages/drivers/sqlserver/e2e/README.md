# SQL Server driver E2E

Optional WebdriverIO specs for the SQL Server path driver. Not included in default `pnpm e2e`.

## Prerequisites

- Build with SQL Server compiled in: `DATAZEN_DRIVERS=all pnpm tauri:build --debug --features webdriver`
- A reachable SQL Server instance (local Docker or remote)

## Environment

| Variable | Description |
|----------|-------------|
| `E2E_SQLSERVER_HOST` | Host (default `127.0.0.1`) |
| `E2E_SQLSERVER_PORT` | Port (default `1433`) |
| `E2E_SQLSERVER_USER` | Login user |
| `E2E_SQLSERVER_PASSWORD` | Password |
| `E2E_SKIP_SQLSERVER` | Set to `1` to force skip |

## Run

```bash
E2E_SQLSERVER_HOST=127.0.0.1 E2E_SQLSERVER_USER=sa E2E_SQLSERVER_PASSWORD='YourPassword' \
  pnpm e2e:skip-build -- --spec packages/drivers/sqlserver/e2e/sqlserver-smoke.ts
```

Without credentials or when unreachable, the spec skips cleanly.
