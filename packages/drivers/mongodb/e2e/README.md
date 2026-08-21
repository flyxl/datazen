# MongoDB driver E2E

Optional WebdriverIO specs for the MongoDB path driver. They are **not** part of the default Host `pnpm e2e` run.

## Prerequisites

- Build with MongoDB compiled in: `DATAZEN_DRIVERS=all pnpm tauri:build --debug --features webdriver` (or include `mongodb` in `--drivers=`)
- A reachable MongoDB instance

## Environment

| Variable | Description |
|----------|-------------|
| `E2E_MONGODB_URI` | Connection URI (e.g. `mongodb://127.0.0.1:27017`) |
| `E2E_SKIP_MONGODB` | Set to `1` to force skip |

## Run

```bash
E2E_MONGODB_URI=mongodb://127.0.0.1:27017 \
  pnpm e2e:skip-build -- --spec packages/drivers/mongodb/e2e/mongodb-smoke.ts
```

Without `E2E_MONGODB_URI`, the spec skips cleanly so CI stays green.
