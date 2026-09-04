# E2E Pure-IPC Spec Migration Guide

> Track E (`e2e-ipc-migrate`) — migrate WebDriver-only IPC assertions to Rust integration tests in `src-tauri/tests/`.

## Background

Some E2E specs invoke Tauri commands exclusively via `browser.executeAsync` → `__TAURI_INTERNALS__.invoke`, with **zero** `$()` UI selectors. These tests pay the full cost of:

- WebDriver session startup and browser bridge
- `beforeSuite` app reset (~12–20 s per spec file in the default suite)
- IPC serialization overhead

The same assertions can run **10×+ faster** by calling the Rust `*_impl` command handlers directly (see PoC: `driver_command_ipc.rs`).

## Pure-IPC Candidate Specs (8 files)

These specs have **no `$()` DOM interactions** — only `invokeBackend` / inline `executeAsync` IPC calls (some may still import helpers that open UI for setup; migration replaces that setup with `TestAppState`):

| E2E spec | Primary IPC surface | Migration complexity |
|----------|---------------------|----------------------|
| `driver-commands.ts` | `get_connections`, `connect`, `get_connection_commands`, `execute_driver_command` | **Done (PoC)** — `driver_command_ipc.rs` |
| `app-data-backup.ts` | `export_app_data`, `import_app_data`, `get_system_ui_language` (+ minimal UI for button labels) | Medium — needs temp paths, zip assertions |
| `table-batch-ops.ts` | `execute_driver_command`, `save_settings` (+ helpers for PG seed) | Medium — table seed via mock or PG fixture |
| `execute-sql-file.ts` | `restore_sql_file`, `execute_driver_command` | Medium — uses `overridePath` webdriver form |
| `edit-delete-connection.ts` | `save_connection`, `get_connections`, `delete_connection` | Low |
| `backup-database.ts` | `backup_database`, `execute_driver_command` | Medium — real PG + temp files |
| `dialog-injection.ts` | `test_inject_dialog_result`, export IPCs | High — needs `webdriver` feature / dialog queue |
| `ai-features.ts` | AI IPCs (`ai_*`) | High — API keys or mock provider harness |

## Migration Method

### 1. Prefer `*_impl` + `TestAppState` (recommended)

The host already exposes internal test infrastructure:

```rust
// src-tauri/tests/my_ipc_migration.rs
use datazen::test_harness::{TestAppState, /* *_impl re-exports */};

#[tokio::test]
async fn my_case() {
    let test = TestAppState::new().await;
    let (_, db_session_id) = test.save_and_connect("case-id").await;
    // call get_connection_commands_impl, execute_driver_command_impl, etc.
}
```

Build and run:

```bash
CARGO_TARGET_DIR=target-e2e-ipc cargo test -p datazen \
  --features test-harness --test driver_command_ipc
```

The `test-harness` feature exposes `datazen::test_harness` (mock driver, temp store, keyring guard). It is **not** enabled in release builds.

### 2. Driver-only tests (lighter, narrower coverage)

When the assertion is purely driver dialect behavior (not host IPC wiring), test the driver crate directly:

```rust
// packages/drivers/<id>/tests/ or driver crate #[cfg(test)]
use datazen_driver_api::DatabaseDriver;
```

Do **not** put driver-dialect tests in `src-tauri/tests/` — follow [AGENTS.md](../../AGENTS.md) driver test placement rules.

### 3. Mark the E2E spec (dual coverage period)

After Rust tests cover all cases, add a comment at the top of the E2E file:

```typescript
// NOTE: These IPC tests have Rust equivalents in src-tauri/tests/<name>.rs
// Consider removing this E2E spec once the Rust tests prove stable.
```

Keep the E2E spec until the Rust suite is stable in CI for several releases.

### 4. Remove from default E2E (later)

Once confident, exclude the spec from the default `wdio.conf.ts` glob or the smoke suite — do not delete the file until the full E2E regression pass no longer needs it.

## PoC Reference: `driver-commands.ts`

**E2E flow:**

1. `get_connections` → pick first connection
2. `connect` → `dbSessionId`
3. `get_connection_commands` → assert `query` exists
4. `execute_driver_command` with `SELECT 1 AS n` → assert data
5. Unknown command → `Unsupported driver command`

**Rust equivalent:** `src-tauri/tests/driver_command_ipc.rs` — two `#[tokio::test]` functions mirroring the two `it()` blocks.

Existing unit tests in `src-tauri/src/commands/driver_command.rs` (`#[cfg(test)]`) cover many more edge cases (safe mode, read-only, streaming, F7 targeting). The integration test file documents the **E2E replacement pattern**; lib tests remain the deep regression net.

## Expected Savings

| Item | Per spec (approx.) |
|------|-------------------|
| WebDriver + app launch amortized | ~15–25 s |
| `beforeSuite` reset | ~12–20 s |
| IPC round-trip vs direct Rust | ~1–5 s |
| **Total per migrated spec** | **~30 s** |

Migrating all 8 pure-IPC candidates saves **~4 minutes** from the default serial E2E run, with larger gains when combined with Track A (`beforeSuite` trim) and Track F (multi-instance).

## Checklist for Each Migration

- [ ] Map every `it()` / `describe()` case to a `#[tokio::test]` (or `#[test]`)
- [ ] Use `TestAppState` instead of E2E seeded connections where possible
- [ ] For real-DB cases (PG backup), gate on env like `workflow_tests.rs` (skip if unavailable)
- [ ] Run `cargo test -p datazen --features test-harness --test <name>`
- [ ] Run `cargo check -p datazen` without extra features (release path unchanged)
- [ ] Add E2E comment marker; do not delete E2E spec yet
- [ ] Update this doc’s status table when complete

## Related

- [E2E testing](./e2e-testing.md) — full E2E workflow
- [Testing architecture](../architecture/testing.md) — Host vs driver test placement
