# Connection Pool Size + Dashboard Label/Icon

**Date:** 2026-08-10  
**Status:** Approved

## Goals

1. Add a global **connection pool size** setting (default **10**), editable in the Settings window; applies only to **newly opened** connections.
2. Rename main-window「运营看板」to「数据看板」and use a dashboard Lucide icon.
3. Delete local/remote branches already merged into `main`.

## Non-goals

- Live resize of existing sqlx pools
- Per-connection pool overrides in the connection form
- Pushing pool size through `pluginSettings`
- Changing SQLite (`max_connections = 1`) or Redis connection semantics

## Design — pool size (Approach B)

### Settings

- Field: `AppSettings.connectionPoolSize: u32` (serde `camelCase`)
- Default: `10` via `#[serde(default = "default_connection_pool_size")]`
- UI: Settings → **Data Browsing**; number input clamped to **1–100**
- Hint copy: change takes effect on next connect / reconnect

### ConnectionConfig

- Field: `max_pool_size: u32` (serde `camelCase`, default `10`)
- Host `ConnectionManager` on connect: clone config, set `max_pool_size` from current `AppSettings.connection_pool_size` (clamped), pass to `driver.connect`
- Do **not** persist the injected value back into the saved connection record

### Drivers

| Driver     | Behavior                                      |
|------------|-----------------------------------------------|
| PostgreSQL | `connect` uses `config.max_pool_size` as sqlx `max_connections` (replace hard-coded 3); `test_connection` stays at 1 |
| MySQL      | Same as PostgreSQL (replace hard-coded 3)     |
| SQLite     | Unchanged (`max_connections = 1`)             |
| Others     | Ignore / unchanged                            |

`min_connections` for Postgres may remain a small fixed warm-up (e.g. `min(2, max_pool_size)`) — not user-configurable in this change.

### Frontend types

- Mirror `connectionPoolSize` on TS `AppSettings`
- Default in settings store / tests: `10`

## Design — dashboard label & icon

- zh-CN (and any remaining「运营看板」user-facing strings for the main action / window title as agreed):「数据看板」
- `HOST_LUCIDE_MAP['action.dashboard']` → `LayoutDashboard`
- Register `LayoutDashboard` in `ThemedIcon` `LUCIDE_MAP` (required; missing entries render `?`)

## Design — branch cleanup

- Delete branches merged into `main` (local + `origin` when safe)
- If a branch is checked out in a worktree, remove or note the worktree before deleting

## Testing

- Rust: `AppSettings` default / serde round-trip for `connection_pool_size`
- Rust: `ConnectionConfig` default for `max_pool_size`
- Frontend: settings draft includes the new field; icon map resolves to `LayoutDashboard`
- Manual / smoke: change pool size, reconnect Postgres/MySQL session

## Error handling

- Out-of-range UI values clamped to 1–100 before save
- Missing field in old `settings.json` → default 10
