# SQL Server driver E2E

Optional WebdriverIO specs for the SQL Server path driver. Not included in default `pnpm e2e`.

| Spec | Purpose |
|------|---------|
| `sqlserver-smoke.ts` | Reachability placeholder: skips unless a SQL Server instance answers on `host:port`. |
| `sqlserver-live-e2e.ts` | Live journey through the app's own IPC (`save_connection` → `connect` → `execute_query` → `get_table_data`). Asserts dialect pagination (`OFFSET … FETCH NEXT`, never `LIMIT`), batch-only DDL such as `CREATE SCHEMA`/`CREATE VIEW`, temporal rendering and filtered/sorted paging. Creates only `dz_e2e_*` objects and drops them in `after`. |
| `sqlserver-live-ui.ts` | Live journey through the **real window**: connects via the navigator card, expands `<db> → <schema> → Tables`, opens the scratch table in the data grid and asserts every column header, the temporal cell text (`2026-03-01`, never `Date(`/`increments`) and next-page navigation. Reuses the host harness helpers (`e2e/helpers.ts`) for tree expansion, so it also exercises the same navigator code path as the other drivers' specs. |
| `sqlserver-metadata.ts` | Live journey through `get_table_schema` IPC. Creates parent/child scratch tables with composite primary keys, a secondary index, a composite foreign key and a CHECK constraint; verifies catalog names, columns, actions and key ordering. It also checks that a non-default column collation is preserved or rejected explicitly, then drops the scratch objects. |

## Prerequisites

- Build with SQL Server compiled in and the webdriver feature. The driver set **must**
  include `basic`: `generated.ts` derives `DatabaseType` from the selected drivers, so a
  `sqlserver`-only build narrows the union and breaks `pnpm build`/`pnpm typecheck` for the
  existing fixtures.
  `DATAZEN_DRIVERS=basic,sqlserver pnpm e2e -- --spec packages/drivers/sqlserver/e2e/sqlserver-live-ui.ts`
- The DMG packaging step can fail after the binary is already produced (`Built application
  at: …/target/debug/datazen`); run the specs with `pnpm e2e:skip-build` in that case.
- A reachable SQL Server / Azure SQL Database instance whose login may create and drop
  scratch schemas, tables and views.

## Environment

| Variable | Description |
|----------|-------------|
| `E2E_SQLSERVER_HOST` | Host (no default; the live spec skips without it) |
| `E2E_SQLSERVER_PORT` | Port (default `1433`) |
| `E2E_SQLSERVER_USER` | Login user |
| `E2E_SQLSERVER_PASSWORD` | Password |
| `E2E_SQLSERVER_DATABASE` | Database to connect to (optional) |
| `E2E_SQLSERVER_SCHEMA` | Default schema used by the connection config (default `dbo`) |
| `E2E_SQLSERVER_SSL_MODE` | `disable` \| `prefer` \| `require` \| `verifyCa` \| `verifyFull` (default `require`) |
| `E2E_SQLSERVER_TRUST_CERT` | Set to `0` to validate the server certificate chain (default trusts it) |
| `E2E_SKIP_SQLSERVER` | Set to `1` to force skip |

## Run

```bash
# placeholder smoke
E2E_SQLSERVER_HOST=127.0.0.1 E2E_SQLSERVER_USER=sa E2E_SQLSERVER_PASSWORD='YourPassword' \
  pnpm e2e:skip-build -- --spec packages/drivers/sqlserver/e2e/sqlserver-smoke.ts

# live journey (builds first when --skip-build is omitted)
E2E_SQLSERVER_HOST=127.0.0.1 E2E_SQLSERVER_PORT=1433 \
E2E_SQLSERVER_USER=sa E2E_SQLSERVER_PASSWORD='YourPassword' \
E2E_SQLSERVER_DATABASE=master \
  pnpm e2e -- --spec packages/drivers/sqlserver/e2e/sqlserver-live-e2e.ts

# live GUI journey (navigator → data grid; same binary)
E2E_SQLSERVER_HOST=127.0.0.1 E2E_SQLSERVER_USER=sa E2E_SQLSERVER_PASSWORD='YourPassword' \
E2E_SQLSERVER_DATABASE=master \
  pnpm e2e:skip-build -- --spec packages/drivers/sqlserver/e2e/sqlserver-live-ui.ts

# catalog metadata journey through get_table_schema IPC
E2E_SQLSERVER_HOST=127.0.0.1 E2E_SQLSERVER_USER=sa E2E_SQLSERVER_PASSWORD='YourPassword' \
E2E_SQLSERVER_DATABASE=master \
  pnpm e2e:skip-build -- --spec packages/drivers/sqlserver/e2e/sqlserver-metadata.ts
```

Without credentials, the specs skip cleanly. The live specs make no assumption about
pre-existing user tables: everything they touch is created with the `dz_e2e_` prefix
(the GUI spec's `dz_e2e_ui_` schemas are dropped in `after`, and `sqlserver-live-e2e.ts`
also sweeps stale `dz_e2e_` schemas left behind by earlier runs).
Credential-free runs never print the password; keep it in the environment only.

## Gotchas

- **A leftover app instance hijacks the run.** `run.mjs` always starts the app on
  `E2E_WD_PORT` (default `4445`); if an app from another checkout or worktree is still
  listening there, WebDriver attaches to *that* one and the run reports its drivers —
  the classic symptom is `Driver not found for type: sqlserver` (or a driver list that
  does not match your build) even though the freshly built binary is correct. Check for
  a stray `DataZen.app/Contents/MacOS/datazen` process and/or run with a free port:
  `E2E_WD_PORT=4465 pnpm e2e:skip-build -- --spec …`.
- `pnpm e2e` builds the frontend and embeds it in the binary, so **any `src/` (frontend)
  change requires a rebuild** before `pnpm e2e:skip-build` can observe it.
- The GUI spec saves the connection over IPC into the store, but the sidebar only picks
  it up after a reload; it reloads the window once before clicking the card.
- The grid's page size is 50, so the GUI spec seeds 120 rows — with fewer rows the
  next-page button is (correctly) disabled and paging cannot be exercised.
