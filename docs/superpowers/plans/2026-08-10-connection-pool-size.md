# Connection Pool Size + Dashboard Icon Implementation Plan

> **For agentic workers:** Execute task-by-task. Steps use checkbox syntax.

**Goal:** Global connection pool size (default 10) in Settings; dashboard rename/icon; delete merged branches.

**Architecture:** `AppSettings.connectionPoolSize` → Host injects into `ConnectionConfig.max_pool_size` on connect → PG/MySQL sqlx pools. Dashboard icon via Lucide `LayoutDashboard`.

**Tech Stack:** Tauri/Rust, React/TS, sqlx, Lucide

## Global Constraints

- Pool change applies only to newly opened connections
- Clamp pool size to 1–100
- Do not persist injected `max_pool_size` onto saved connections
- SQLite stays max_connections=1
- Do not commit unless user asks

---

### Task 1: AppSettings + ConnectionConfig (Rust)

**Files:**
- Modify: `packages/driver-api/src/types.rs`
- Modify: `src-tauri/src/store/settings.rs`
- Modify: `src-tauri/src/store/tests.rs` (if needed)
- Modify: any `ConnectionConfig { ... }` literals that break

- [ ] Add `max_pool_size` to `ConnectionConfig` (default 10)
- [ ] Add `connection_pool_size` to `AppSettings` (default 10)
- [ ] Fix compile breakages from new fields
- [ ] `cargo test -p datazen-driver-api --lib` and settings-related tests

### Task 2: Host inject on connect

**Files:**
- Modify: `src-tauri/src/services/connection_manager.rs`
- Possibly monitor registry connect path if it also calls `driver.connect`

- [ ] Before `driver.connect`, set `effective_config.max_pool_size` from store settings (clamped 1–100)
- [ ] Same for any parallel connect path (monitor) that should honor the setting

### Task 3: Postgres / MySQL drivers

**Files:**
- Modify: `packages/drivers/postgres/src/postgres.rs`
- Modify: `packages/drivers/mysql/src/mysql.rs`

- [ ] `connect` uses `config.max_pool_size` (at least 1)
- [ ] `test_connection` remains max_connections=1

### Task 4: Frontend settings + types

**Files:**
- Modify: `src/types/index.ts`
- Modify: `src/stores/settingsStore.ts` (defaults)
- Modify: `src/windows/settings/SettingsWindow.tsx`
- Modify: `src/locales/en.ts`, `zh-CN.ts` (+ other locales keys as needed)
- Modify: settings tests defaults

- [ ] Add `connectionPoolSize` to TS AppSettings / defaults
- [ ] Data Browsing number input + hint
- [ ] i18n keys

### Task 5: Dashboard label + icon

**Files:**
- Modify: `src/locales/zh-CN.ts` (remaining 运营看板 if any for action/win)
- Modify: `src/lib/hostLucideMap.ts`
- Modify: `src/components/ThemedIcon.tsx`

- [ ] Map `action.dashboard` → `LayoutDashboard`
- [ ] Import/register `LayoutDashboard` in ThemedIcon

### Task 6: Delete merged branches

- [ ] List `git branch --merged main`
- [ ] Delete safe local/remote merged branches (handle worktree)

### Task 7: Verify

- [ ] `cargo test -p datazen --lib` (or targeted)
- [ ] Frontend unit tests for settings if touched
