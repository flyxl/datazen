# Arch Review Remaining (A→D) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clear remaining items from `docs/progress-arch-review-remaining.md` in order A→D, parallelizing independent tracks.

**Architecture:** Sync IR/traits move into `datazen-driver-api`; concrete adapters live in path driver crates and self-register via `inventory`. Host keeps registry + orchestration. Backup fidelity improves in `sql_dump` + driver overrides. P2/P3 are docs/meta-only. God-module splits follow Sync move.

**Tech Stack:** Rust (driver-api, drivers, src-tauri), TypeScript (meta, commands docs), Vitest/cargo test.

## Global Constraints

- Do not change `PROTOCOL_VERSION` unless Sync trait export requires it (prefer additive module).
- Keep git stubs empty (`generated.ts` / `plugin_init.rs`); restore after inject.
- ClickHouse Sync / duckdb / elasticsearch Sync dialects are **out of scope** (doc deferred).
- Merge to main / push remote are **out of scope**.
- Prefer small focused commits only when user asks; do not commit unless asked.
- Brand badge SVGs remain primary; `iconColor`/`iconBg` are fallbacks — map to semantic tokens (`accent`/`danger`/`success`/`warning`/`surface`/`fg`).
- IPC docs must match actual `src/commands/*` invoke key style (mostly camelCase params).

---

## Track A — Sync adapters → driver packages

### Task A1: Move Sync IR + traits into driver-api

**Files:**
- Create: `packages/driver-api/src/sync/{mod,ir,adapter}.rs`
- Modify: `packages/driver-api/src/lib.rs`
- Modify: host `src-tauri/src/sync/*` to re-export / use API types

- [ ] Copy `ir.rs` + adapter traits into `datazen-driver-api::sync`
- [ ] Change `SyncAdapterFactory` to `create: fn() -> Arc<dyn SyncBoth>` (or separate source/target) so drivers do not depend on host `SyncAdapterRegistry`
- [ ] Host `adapter_registry` calls `create()` and inserts
- [ ] `cargo test -p datazen-driver-api` + host sync unit tests pass

### Task A2: Move PG/MySQL/SQLite/SQL Server adapters into drivers

**Files:**
- Create: `packages/drivers/{postgres,mysql,sqlite,sqlserver}/src/sync_adapter.rs`
- Modify: each driver `lib.rs` to `mod sync_adapter`
- Remove (or thin) host `src-tauri/src/sync/adapters/{postgresql,mysql,sqlite,sqlserver}.rs`
- Keep host `trino.rs` (no path driver) behind `force_link`

- [ ] Port adapters; `inventory::submit!` SyncAdapterFactory in each driver crate
- [ ] Ensure feature-gated drivers still register when linked
- [ ] Move/adapt `roundtrip_tests` to driver-api or host tests that exercise inventory
- [ ] `cargo test -p datazen --lib` (with basic drivers) passes alias tests

### Task A3: Update progress doc for A

- [ ] Mark Sync 下沉 done in `docs/progress-arch-review-remaining.md`

---

## Track B — Backup fidelity

### Task B1: Statement splitter (restore)

**Files:** `packages/driver-api/src/sql_dump.rs` (+ tests)

- [ ] Replace naive `split(';')` with splitter that respects single/double quotes, `--` / `/* */` comments, and PostgreSQL `$$` / `$tag$` dollar quotes
- [ ] Unit tests for quoted semicolons and dollar-quoted bodies

### Task B2: Wire more dump options + fidelity

**Files:**
- `packages/driver-api/src/types.rs` (`BackupDumpOptions`)
- `src-tauri/src/commands/backup.rs` (`parse_backup_options`)
- `packages/drivers/mysql/src/mysql.rs`, `packages/drivers/postgres/src/postgres.rs`
- Optionally trim UI options that remain unsupported

- [ ] Extend options: at least honor `no-owner` (PG comment/skip OWNER), `single-transaction` (wrap restore or dump notes), `routines`/`triggers` when cheap via SHOW / catalogs; mark `format-custom` unsupported in UI or reject with clear error
- [ ] Prefer `SHOW CREATE TABLE` / PG `format_type` paths already in drivers for DDL fidelity
- [ ] Do **not** require shelling out to `pg_dump` unless binary detection is clean and optional; prefer in-process fidelity first

### Task B3: Update progress for B

- [ ] Note remaining gaps (e.g. no external pg_dump) explicitly

---

## Track C — P2/P3 (parallel with A)

### Task C1: Driver meta semantic colors

**Files:** `packages/drivers/*/ui/meta.ts`

- [ ] Replace palette classes (`text-blue-400`, `bg-green-700`, …) with semantic tokens (`text-accent`, `bg-accent`, `text-danger`, `bg-danger`, `text-success`, `bg-success`, `text-warning`, `bg-warning`, …)
- [ ] Keep Redis as reference; map families consistently

### Task C2: IPC docs vs implementation

**Files:** `docs/architecture/frontend/components.md` (and any AGENTS/architecture lines that claim exclusive snake_case invoke keys)

- [ ] Document reality: Rust command args are snake_case fields; frontend `invoke` object keys are typically camelCase matching serde rename on command args / or note actual pattern after sampling `src/commands/`
- [ ] Fix misleading “统一方案：一律 snake_case key” if code uses camelCase

### Task C3: Connection group Chinese hardcode scan

- [ ] Grep UI/code for hardcoded group labels outside i18n; fix to `t('…')` / `preset:*` where needed
- [ ] Skip locale files themselves

---

## Track D — God module splits (after A)

### Task D1: Split `commands/sync.rs`

- [ ] Extract modules under `commands/sync/` (e.g. `compare`, `table_sync`, `types`) keeping public command fns

### Task D2: Split Store / aiStore / large windows (pragmatic)

- [ ] `store/mod.rs`: extract cohesive submodules without behavior change
- [ ] `aiStore.ts`: extract helpers/slices if natural seams exist
- [ ] Only split Window components if clear seam; avoid drive-by refactors

### Task D3: Final progress doc update

- [ ] Refresh `docs/progress-arch-review-remaining.md` checklist

## Parallelism

| Wave | Tracks |
|------|--------|
| 1 | A1–A2 ∥ C1–C3 |
| 2 | B1–B2 (after/with A if no file clash) |
| 3 | D after A2 lands |

## Verification

```bash
cargo test -p datazen-driver-api
cargo test -p datazen --lib
npx vitest run  # if frontend meta/i18n tests exist
```
