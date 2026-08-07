# F3 Test Report — PostgreSQL multi-DB driver (`get_tables` + `use_database`)

| Field | Value |
|-------|-------|
| **Feature** | F3 — PostgreSQL: `get_tables` respects database catalog + `use_database` reconnects pool |
| **Date** | 2026-08-07 |
| **Tester** | test-agent (fresh, testing-only) |
| **Branch** | `feat/multi-database-session-ui` |
| **Scope** | Rust driver (`src-tauri/src/db/postgres.rs`) + gated live IT (`src-tauri/tests/postgres_use_database.rs`); no UI (F4) |

## Requirements verified (from progress doc)

| # | Requirement | Unit | Live IT | Static |
|---|-------------|------|---------|--------|
| 1 | `get_tables(handle, database)` targets named catalog (not ignore `_database`) | — | ✅ | ✅ |
| 2 | Active DB hit → reuse handle pool | — | ✅ | ✅ |
| 3 | Non-active named DB → temporary pool, closed after fetch | — | ✅ | ✅ |
| 4 | Empty `database` → tables on currently connected pool | — | — | ✅ |
| 5 | `use_database` reconnects PgPool to target DB (Postgres has no `USE`) | ✅ | ✅ | ✅ |
| 6 | Same DB (trimmed match) → no-op | ✅ | — | ✅ |
| 7 | Empty / whitespace / NUL name → `InvalidConfig` | ✅ | ✅ | ✅ |
| 8 | Unknown DB → `QueryFailed`; active DB unchanged after failure | ✅ | ✅ | ✅ |
| 9 | Empty `config.database` connect → defaults to `postgres` | ✅ | — | ✅ |
| 10 | Gated IT skips cleanly without `TEST_PG_*` | — | ✅ | — |

## Test commands run

### 1. Unit tests — PASS

```text
$ cargo test -p datazen --lib postgres::tests

running 5 tests
test db::postgres::tests::resolve_connect_database_defaults_to_postgres ... ok
test db::postgres::tests::validate_database_name_rejects_empty_or_invalid ... ok
test db::postgres::tests::validate_database_name_trims_and_accepts ... ok
test db::postgres::tests::use_database_is_wired ... ok
test db::postgres::tests::use_database_noop_when_already_active ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 201 filtered out
```

### 2. Gated integration test — no env (must skip/pass) — PASS

No repo-root `.env` with `TEST_PG_*` keys. `e2e/.env` uses `E2E_PG_*` only (not read by this IT).

```text
$ env -u TEST_PG_HOST -u TEST_PG_PORT -u TEST_PG_USER \
  -u TEST_PG_PASSWORD -u TEST_PG_DATABASE -u TEST_PG_DATABASE_B \
  cargo test -p datazen --test postgres_use_database -- --nocapture

running 1 test
⏭  Skipping postgres_use_database: no TEST_PG_* in env or .env
test use_database_switches_and_get_tables_respects_catalog ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Clean skip confirmed (early `return` in test; cargo reports **ok**, not `ignored`).

### 3. Gated integration test — with env (PostgreSQL available) — PASS

Environment: PostgreSQL on `127.0.0.1:5432`, user `postgres`, empty password; DBs `goecoride` (has `users`), `postgres` (no `users`). `pg_isready` accepting connections.

```text
$ TEST_PG_HOST=127.0.0.1 TEST_PG_PORT=5432 TEST_PG_USER=postgres \
  TEST_PG_PASSWORD= TEST_PG_DATABASE=goecoride \
  TEST_PG_DATABASE_B=postgres \
  cargo test -p datazen --test postgres_use_database -- --nocapture

running 1 test
▶  use_database live: goecoride → postgres on 127.0.0.1:5432
✅  PostgresDriver::use_database live checks passed
test use_database_switches_and_get_tables_respects_catalog ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
```

Live scenarios exercised:

- `connect` with no default DB (driver falls back to `postgres`)
- `get_tables(A)` sees `users`; `get_tables(B)` does not (catalog isolation before switch)
- `use_database(A)` → 5 pooled unqualified `SELECT COUNT(*) FROM users` succeed
- After switch to A, `get_tables(B)` still returns B catalog (no leak)
- `use_database(B)` → 5 pooled unqualified `users` queries fail with `QueryFailed`
- Switch back to A → `users` visible again
- Invalid DB → `QueryFailed`, active remains A
- Empty name → `InvalidConfig`

## Static spot-check

| Check | Location | Verdict |
|-------|----------|---------|
| `get_tables`: empty → current pool | `postgres.rs` L438–442 | **OK** |
| `get_tables`: active match → reuse pool | `postgres.rs` L445–455 | **OK** |
| `get_tables`: other named DB → temp pool + close | `postgres.rs` L458–463 | **OK** |
| `fetch_tables_from_pool` uses `information_schema.tables` on connected catalog | `postgres.rs` L86–113 | **OK** |
| `use_database`: validate + same-DB no-op | `postgres.rs` L871–877 | **OK** |
| `use_database`: `pool_for_named_database` → insert new pool, close old | `postgres.rs` L882–897 | **OK** |
| `active_databases` updated on successful switch | `postgres.rs` L890–893 | **OK** |
| Unknown DB surfaced as `QueryFailed` via `pool_for_named_database` | `postgres.rs` L134–141 | **OK** |
| `resolve_connect_database` empty/whitespace → `postgres` | `postgres.rs` + unit test | **OK** |

## Full case table

| ID | Scenario | Result |
|----|----------|--------|
| F3-UT-001 | `validate_database_name` trim/accept | **Pass** |
| F3-UT-002 | `validate_database_name` reject empty/NUL | **Pass** |
| F3-UT-003 | `resolve_connect_database` defaults | **Pass** |
| F3-UT-004 | `use_database` wiring (InvalidConfig / ConnectionFailed) | **Pass** |
| F3-UT-005 | `use_database` same-DB no-op (no reconnect) | **Pass** |
| F3-IT-SKIP | No `TEST_PG_*` → clean skip | **Pass** |
| F3-IT-LIVE-001 | `get_tables` per-catalog before switch | **Pass** |
| F3-IT-LIVE-002 | `use_database(A)` pooled queries on A | **Pass** |
| F3-IT-LIVE-003 | `get_tables(B)` after switch to A (no leak) | **Pass** |
| F3-IT-LIVE-004 | `use_database(B)` unqualified query fails | **Pass** |
| F3-IT-LIVE-005 | Switch back to A | **Pass** |
| F3-IT-LIVE-006 | Invalid DB + empty name errors | **Pass** |
| F3-STATIC-001~009 | Driver implementation paths | **Pass** |

## Bugs found

**None.**

## Overall verdict

**PASS**

- 5/5 Rust unit tests green.
- Gated IT skips cleanly without `TEST_PG_*`; passes full live matrix with local PostgreSQL (`goecoride` / `postgres`).
- Static review confirms `get_tables` honors named catalog and `use_database` replaces the handle pool.
