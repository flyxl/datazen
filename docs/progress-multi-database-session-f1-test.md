# F1 Test Report — MySQL/MariaDB `use_database`

| Field | Value |
|-------|-------|
| **Feature** | F1 — MySQL/MariaDB `use_database` implementation |
| **Date** | 2026-08-07 |
| **Tester** | test-agent (fresh, testing-only) |
| **Branch** | `feat/multi-database-session-ui` |
| **Scope** | Rust driver (`src-tauri/src/db/mysql.rs`) + gated live IT (`src-tauri/tests/mysql_use_database.rs`); no UI / IPC (F2) |

## Requirements (from progress doc)

1. Validate and quote database identifier; execute `USE \`db\``.
2. Track active database in `active_databases` (per `pool_id`).
3. Re-apply `USE` on each pooled connection acquire (`query`, `execute`, `explain`, `get_columns`, `get_table_schema`).
4. No-op when switching to the already-active database (including trimmed name match).
5. Empty / whitespace-only / NUL name → `InvalidConfig`.
6. Unknown or inaccessible database → `QueryFailed`.
7. MySQL and MariaDB share `MysqlDriver`; both must override the trait.
8. **Live fixes:** `USE` via text protocol (COM_QUERY); `SELECT DATABASE()` via text protocol; `clear_cached_statements` after `USE`.

## Test commands run

### 1. Unit tests

```text
$ cargo test -p datazen mysql::tests

running 12 tests
test sync::adapters::mysql::tests::mysql_format_bool_literal ... ok
test sync::adapters::mysql::tests::mysql_target_types ... ok
test db::mysql::tests::quote_identifier_escapes_backticks ... ok
test db::mysql::tests::build_use_database_sql_quotes_and_trims ... ok
test db::mysql::tests::build_use_database_sql_rejects_empty_or_invalid ... ok
test sync::adapters::mysql::tests::mysql_enum_to_text ... ok
test sync::adapters::mysql::tests::mysql_unsigned_int ... ok
test sync::adapters::mysql::tests::mysql_tinyint1_is_bool ... ok
test sync::adapters::mysql::tests::mysql_json ... ok
test sync::adapters::mysql::tests::mysql_varchar_to_ir ... ok
test db::mysql::tests::use_database_is_wired_for_mysql_and_mariadb ... ok
test db::mysql::tests::use_database_noop_when_already_active ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 189 filtered out
```

F1-specific (`db::mysql::tests::*`): **5 passed, 0 failed**.

### 2. Gated integration test — no env (must skip/pass)

```text
$ env -u TEST_MYSQL_HOST -u TEST_MYSQL_PORT -u TEST_MYSQL_USER \
  -u TEST_MYSQL_PASSWORD -u TEST_MYSQL_DATABASE -u TEST_MYSQL_DATABASE_B \
  cargo test -p datazen --test mysql_use_database -- --nocapture

running 1 test
⏭  Skipping mysql_use_database: no TEST_MYSQL_* in env or .env
test use_database_switches_and_rejects_invalid ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

No repo-root `.env` with `TEST_MYSQL_*` keys present. Clean skip confirmed.

### 3. Gated integration test — with env (MySQL available)

Environment: MySQL **9.6.0** on `127.0.0.1:3306`, user `root`, empty password; DBs `datazen_test` (has `users`), `datazen_sync_mysql_tgt` (no `users`).

```text
$ TEST_MYSQL_HOST=127.0.0.1 TEST_MYSQL_PORT=3306 TEST_MYSQL_USER=root \
  TEST_MYSQL_PASSWORD= TEST_MYSQL_DATABASE=datazen_test \
  TEST_MYSQL_DATABASE_B=datazen_sync_mysql_tgt \
  cargo test -p datazen --test mysql_use_database -- --nocapture

running 1 test
▶  use_database live: datazen_test → datazen_sync_mysql_tgt on 127.0.0.1:3306
✅  MysqlDriver::use_database live checks passed
test use_database_switches_and_rejects_invalid ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
```

## Static spot-check

| Check | Location | Verdict |
|-------|----------|---------|
| `USE` via text protocol (`Executor::execute(&str)`, not `sqlx::query`) | `execute_use_on_conn` L86–94; `use_database` L1025–1030 | **OK** |
| `clear_cached_statements` after `USE` | `execute_use_on_conn` L95–99 | **OK** |
| `active_databases` tracking (connect seed, use_database insert, disconnect remove, apply on acquire) | struct L17–19; connect L542; disconnect L557; `use_database` L1005–1038; `apply_active_database` L104–124 | **OK** |
| `SELECT DATABASE()` via text protocol | `current_database_on_conn` L57–67 | **OK** |
| Pool re-`USE` on acquire | `apply_active_database` called from `query`, `query_multi`, `execute`, `explain`, `get_columns`, `get_table_schema` (L626, L657, L756, L788, L882, L924) | **OK** |
| `pub mod db` for integration test access | `src-tauri/src/lib.rs` L5 | **OK** |

## Full case table

| ID | Steps | Expected | Actual | Result |
|----|-------|----------|--------|--------|
| F1-UT-001 | `build_use_database_sql_quotes_and_trims` | Quoted `USE`, trim, backtick escape | All assertions pass | **Pass** |
| F1-UT-002 | `build_use_database_sql_rejects_empty_or_invalid` | `""`, whitespace, `\0` → `InvalidConfig` | All assertions pass | **Pass** |
| F1-UT-003 | `use_database_is_wired_for_mysql_and_mariadb` | Empty → `InvalidConfig`; missing pool → `ConnectionFailed` for mysql + mariadb | All assertions pass | **Pass** |
| F1-UT-004 | `use_database_noop_when_already_active` | Same tracked DB returns `Ok` without pool | Pass without registered pool | **Pass** |
| F1-STATIC-001 | Review `USE` execution path | Text protocol COM_QUERY, not prepared | `execute_use_on_conn` uses `(&mut **conn).execute(use_sql)` | **Pass** |
| F1-STATIC-002 | Review statement cache invalidation | `clear_cached_statements` after every `USE` | Called in `execute_use_on_conn` | **Pass** |
| F1-STATIC-003 | Review `active_databases` lifecycle | Seed on connect, update on switch, remove on disconnect, re-apply on pool acquire | Matches design | **Pass** |
| F1-IT-SKIP | `cargo test --test mysql_use_database` without `TEST_MYSQL_*` | Clean skip, test exits 0 | `⏭ Skipping mysql_use_database: no TEST_MYSQL_* in env or .env` | **Pass** |
| F1-LIVE-001 | IT: `use_database(A)` → unqualified `SELECT COUNT(*) FROM users` ×5 | Resolves in A | All 5 pooled queries succeed | **Pass** |
| F1-LIVE-002 | IT: `use_database(B)` → unqualified `users` ×5 | `QueryFailed` (no `users` in B) | All 5 return `QueryFailed` | **Pass** |
| F1-LIVE-003 | IT: switch back to A → unqualified `users` | Resolves in A again | Query succeeds | **Pass** |
| F1-LIVE-004 | IT: `use_database(nonexistent_db_xyz_f1_test)` | `QueryFailed`; active DB stays A | Error mentions unknown DB; `users` still works | **Pass** |
| F1-LIVE-005 | IT: `use_database("   ")` | `InvalidConfig` | `InvalidConfig` returned | **Pass** |
| F1-LIVE-006 | IT: connect without default database | Driver switches via `use_database` only | Connect with `database: None`; switches work | **Pass** |
| F1-LIVE-007 | Live MariaDB `use_database` | Same behavior on MariaDB server | No MariaDB instance on test host | **Blocked** — shared code; unit test covers mariadb wiring only |
| F1-E2E-001 | UI session DB switch | Connection Window uses selected DB | F2 not implemented; no IPC `use_database` | **Blocked** — out of F1 scope |

## Bugs found

**None.** No production code was modified during this test pass.

## Observations (not bugs)

1. **MariaDB live parity untested** — `MysqlDriver::new(true)` shares implementation; only MySQL 9.6.0 exercised live.
2. **`use_database` not wired to app IPC yet** — expected with F2 session UI.

## Overall verdict

**PASS**

All F1 driver deliverables verified: unit tests (5/5 F1-specific, 12/12 filter), gated live integration test (skip without env, full scenario with MySQL), and static review of text-protocol `USE`, `clear_cached_statements`, and `active_databases`. MariaDB live and UI E2E remain blocked/out-of-scope for F1.
