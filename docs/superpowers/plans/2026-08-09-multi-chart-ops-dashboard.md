# Multi-Chart Ops Dashboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a saveable multi-chart ops dashboard with persisted run history, app-data/single-file import-export, tray-resident background refresh, threshold alerts (desktop + webhook), and monitor connections isolated from UI sessions.

**Architecture:** Rust owns dashboard persistence, `WidgetRun` history, and `MonitorEngine` (tokio intervals + alert channels). Frontend adds a `dashboard` window with a CSS-grid of compact `ChartWidgetTile`s that render via existing `ChartCanvas`/`transformData`. Monitor traffic uses dedicated `monitor:{configId}` handles so UI sqlx pools are never drained.

**Tech Stack:** Tauri v2, Rust (`tokio`, existing `Store`/`QueryExecutor`/`ConnectionManager`), React 18, Zustand, Recharts, Vitest, `cargo test -p datazen --lib`, optional `tauri-plugin-notification`.

**Spec:** [docs/superpowers/specs/2026-08-09-multi-chart-ops-dashboard-design.md](../specs/2026-08-09-multi-chart-ops-dashboard-design.md)

## Global Constraints

- Scope of **this plan = M1 + M2**. M3 (email send, `--monitor` headless, add-from-query, SQLite mutex) is deferred to a follow-up plan — do not implement in these tasks.
- Monitor connections **must not** reuse UI session handles; key `monitor:{configId}`; prefer driver pool `max_connections = 1` for monitor handles.
- `refreshSec` minimum **30**; default query timeout **60s**; `maxConcurrentQueries` default **2**; same `configId` monitor ticks serialize.
- Run retention defaults: **200** runs/widget, **30** days, **500** rows/snapshot; prune on write.
- IPC frontend invoke arg keys follow existing style (`connectionId` camelCase) matching other `src/commands/*`.
- Persist under `{appData}/dashboards.json` and `{appData}/dashboard-runs/...`; never store DB passwords in dashboard files.
- Single-file export is definition-only (no runs, no webhook URL).
- Branch / worktree: `docs/multi-chart-ops-dashboard-feasibility` → implement on `feat/multi-chart-ops-dashboard` (create when starting Task 1 if still on docs branch).
- YAGNI: no react-grid-layout dependency in M1 — CSS grid + layout fields edited in drawer; no cross-widget filters.

## File map

| File | Responsibility |
|------|----------------|
| `src/types/dashboard.ts` | TS types mirroring Rust dashboard/run/alert/settings |
| `src-tauri/src/dashboard/mod.rs` | Module root; re-exports |
| `src-tauri/src/dashboard/types.rs` | `Dashboard`, `DashboardWidget`, `AlertRule`, `MonitorSettings`, serde |
| `src-tauri/src/dashboard/store.rs` | Load/save `dashboards.json` + monitor settings helpers |
| `src-tauri/src/dashboard/runs.rs` | Write/list/get/prune `WidgetRun` files + `index.jsonl` |
| `src-tauri/src/dashboard/alert.rs` | Pure alert evaluation + cooldown bookkeeping helpers |
| `src-tauri/src/dashboard/export.rs` | Single-file dashboard JSON validate import/export |
| `src-tauri/src/monitor/mod.rs` | `MonitorEngine` start/stop/reload/pause |
| `src-tauri/src/monitor/connections.rs` | Monitor-only connect map on `ConnectionManager` or sibling |
| `src-tauri/src/commands/dashboard.rs` | Tauri IPC for CRUD, runs, manual refresh, import/export |
| `src-tauri/src/app_data_archive.rs` | Optional skip of `dashboard-runs/` when flag false |
| `src-tauri/src/lib.rs` | Register modules, commands, tray hooks, engine in `AppState` |
| `src/commands/dashboard.ts` | Frontend IPC wrappers + event listen helpers |
| `src/stores/dashboardStore.ts` | Zustand: current dashboard, latest runs, pause flag |
| `src/windows/dashboard/DashboardWindow.tsx` | Window shell + grid |
| `src/windows/dashboard/ChartWidgetTile.tsx` | Compact chart tile |
| `src/windows/dashboard/WidgetEditorDrawer.tsx` | Edit SQL/chart/alert |
| `src/windows/dashboard/RunHistoryDrawer.tsx` | History picker → snapshot chart |
| `src/lib/windowKind.ts` / `windowManager.ts` / `App.tsx` | `dashboard` kind + open helper |
| `src-tauri/capabilities/default.json` | Allow `dashboard-*` window labels |
| `src/locales/en.ts` + `zh-CN.ts` (+ generate menu labels) | i18n keys |
| Tests under `src/types/__tests__/`, `src/lib/...`, `src-tauri/src/dashboard/` | Unit coverage |

**Deferred (M3 plan):** email SMTP send, `--monitor` headless binary mode, “Add to dashboard” from QueryPanel, SQLite UI↔monitor mutex.

---

## Phase M1 — Dashboard CRUD, UI, history, import/export

### Task 1: Shared dashboard types (TS + Rust)

**Files:**
- Create: `src/types/dashboard.ts`
- Create: `src/types/__tests__/dashboard.test.ts`
- Create: `src-tauri/src/dashboard/mod.rs`
- Create: `src-tauri/src/dashboard/types.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod dashboard;`)
- Modify: `src/types/index.ts` (re-export dashboard types if that file re-exports peers)

**Interfaces:**
- Produces (TS):
  - `Dashboard`, `DashboardWidget`, `AlertRule`, `WidgetRun`, `MonitorSettings`, `DashboardLayout`, `WidgetLayout`
  - `DEFAULT_MONITOR_SETTINGS`, `clampRefreshSec(n: number): number` (≥30)
- Produces (Rust, `camelCase` serde): same shapes in `dashboard::types`

- [ ] **Step 1: Write failing Vitest**

```ts
import { describe, expect, it } from 'vitest';
import { clampRefreshSec, DEFAULT_MONITOR_SETTINGS } from '../dashboard';

describe('clampRefreshSec', () => {
  it('enforces minimum 30', () => {
    expect(clampRefreshSec(5)).toBe(30);
    expect(clampRefreshSec(30)).toBe(30);
    expect(clampRefreshSec(120)).toBe(120);
  });
});

describe('DEFAULT_MONITOR_SETTINGS', () => {
  it('has safe defaults', () => {
    expect(DEFAULT_MONITOR_SETTINGS.maxConcurrentQueries).toBe(2);
    expect(DEFAULT_MONITOR_SETTINGS.runRetentionCount).toBe(200);
    expect(DEFAULT_MONITOR_SETTINGS.runRetentionDays).toBe(30);
    expect(DEFAULT_MONITOR_SETTINGS.exportIncludeDashboardRuns).toBe(true);
  });
});
```

- [ ] **Step 2: Run — expect FAIL**

```bash
pnpm exec vitest run src/types/__tests__/dashboard.test.ts
```

Expected: FAIL (module not found)

- [ ] **Step 3: Implement `src/types/dashboard.ts` and Rust `dashboard/types.rs`**

Include all fields from the spec §1.1. Reuse `ChartConfig` from `src/types/chart.ts` on the TS side; on Rust either embed a mirrored `ChartConfig` struct or `serde_json::Value` for `chartConfig` — prefer a mirrored struct in `dashboard/types.rs` matching `ChartConfig` fields so IPC stays typed.

Wire `mod dashboard;` in `lib.rs`.

- [ ] **Step 4: Run Vitest + compile check**

```bash
pnpm exec vitest run src/types/__tests__/dashboard.test.ts
cargo check -p datazen
```

Expected: PASS / compile OK

- [ ] **Step 5: Commit**

```bash
git add src/types/dashboard.ts src/types/__tests__/dashboard.test.ts src-tauri/src/dashboard src-tauri/src/lib.rs
git commit -m "feat(dashboard): add shared dashboard types"
```

---

### Task 2: Persist dashboards.json via Store helpers

**Files:**
- Create: `src-tauri/src/dashboard/store.rs`
- Create: `src-tauri/src/dashboard/store_tests.rs` (or `#[cfg(test)]` in store.rs)
- Modify: `src-tauri/src/dashboard/mod.rs`
- Modify: `src-tauri/src/store/mod.rs` — add `MonitorSettings` fields on `AppSettings` **or** keep monitor settings inside `dashboards.json` meta; **prefer** `monitor` nested object on `AppSettings` with `#[serde(default)]` to match settings UI later

**Interfaces:**
- Produces:
  - `DashboardStore::list/get/save/delete` operating on `{data_dir}/dashboards.json` as `Vec<Dashboard>`
  - `fn load_monitor_settings(settings: &AppSettings) -> MonitorSettings`
  - Methods may live as free functions taking `&Store` / `Path` to avoid bloating `Store` — pattern like theme helpers is fine

- [ ] **Step 1: Write failing Rust test** (tempdir)

```rust
#[tokio::test]
async fn save_and_list_dashboard_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let dash = sample_dashboard(); // helper with one widget
    crate::dashboard::store::save_dashboard(dir.path(), dash.clone()).unwrap();
    let list = crate::dashboard::store::list_dashboards(dir.path()).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, dash.id);
    assert_eq!(list[0].widgets[0].sql, "SELECT 1 AS v");
}
```

- [ ] **Step 2: Run — expect FAIL**

```bash
cargo test -p datazen --lib dashboard::store -- --nocapture
```

Expected: FAIL (module/function missing)

- [ ] **Step 3: Implement load/save/delete** with atomic write (write temp + rename), create file as `[]` if missing. Clamp `refreshSec` on save.

- [ ] **Step 4: Run tests — expect PASS**

```bash
cargo test -p datazen --lib dashboard::
```

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(dashboard): persist dashboards.json"
```

---

### Task 3: WidgetRun history write / list / prune

**Files:**
- Create: `src-tauri/src/dashboard/runs.rs`
- Modify: `src-tauri/src/dashboard/mod.rs`

**Interfaces:**
- Produces:
  - `pub fn write_run(data_dir: &Path, run: &WidgetRun, retention: &MonitorSettings) -> Result<(), ...>`
  - `pub fn list_run_index(data_dir, dashboard_id, widget_id, limit) -> Result<Vec<RunIndexEntry>, ...>`
  - `pub fn get_run(data_dir, dashboard_id, widget_id, run_id) -> Result<WidgetRun, ...>`
  - `RunIndexEntry { id, started_at, status, alert_fired }`
- Path layout per spec: `dashboard-runs/{dashboardId}/{widgetId}/{yyyy}/{mm}/{runId}.json` + `index.jsonl`

- [ ] **Step 1: Failing test — prune keeps newest N**

```rust
#[test]
fn prune_keeps_retention_count() {
    let dir = tempfile::tempdir().unwrap();
    let settings = MonitorSettings { run_retention_count: 3, run_retention_days: 30, ..Default::default() };
    for i in 0..5 {
        let run = sample_run(i);
        write_run(dir.path(), &run, &settings).unwrap();
    }
    let idx = list_run_index(dir.path(), "d1", "w1", 100).unwrap();
    assert_eq!(idx.len(), 3);
}
```

- [ ] **Step 2: Run — expect FAIL**

```bash
cargo test -p datazen --lib dashboard::runs -- --nocapture
```

- [ ] **Step 3: Implement write + index append + prune (by count and by age). Cap rows at 500 before write.**

- [ ] **Step 4: Tests PASS**

```bash
cargo test -p datazen --lib dashboard::runs
```

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(dashboard): persist and prune widget runs"
```

---

### Task 4: Dashboard IPC commands (CRUD + runs + manual execute)

**Files:**
- Create: `src-tauri/src/commands/dashboard.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs` (`generate_handler!`)
- Create: `src/commands/dashboard.ts`

**Interfaces:**
- Produces IPC:
  - `list_dashboards() -> Vec<Dashboard>`
  - `get_dashboard(id) -> Dashboard`
  - `save_dashboard(dashboard) -> Dashboard`
  - `delete_dashboard(id) -> ()`
  - `list_widget_runs(dashboardId, widgetId, limit) -> Vec<RunIndexEntry>`
  - `get_widget_run(dashboardId, widgetId, runId) -> WidgetRun`
  - `run_dashboard_widget(dashboardId, widgetId) -> WidgetRun` — **M1 uses monitor connection path stub**: call shared `execute_widget_once` that opens monitor handle (Task 7 may refine), runs SQL via `QueryExecutor`, writes run, returns it
- For M1 before Task 7 exists: implement `execute_widget_once` in `dashboard` module using `ConnectionManager::connect` **only if** labeled as monitor — if Task 7 not done yet, temporarily use a separate `connect` (new handle) and **do not** call `get_or_connect` for an existing UI session. Document TODO resolved in Task 7.

- [ ] **Step 1: Add frontend command stubs + a minimal Rust unit test for command validation (e.g. unknown dashboard → NotFound)**

- [ ] **Step 2: Register commands; `cargo test -p datazen --lib` for new tests; `pnpm exec tsc --noEmit` if project supports**

- [ ] **Step 3: Implement commands with `CommandError` + `CmdExt` logging like peers**

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(dashboard): add dashboard IPC commands"
```

---

### Task 5: Single-file import/export + app-data runs exclusion flag

**Files:**
- Create: `src-tauri/src/dashboard/export.rs`
- Modify: `src-tauri/src/commands/dashboard.rs` (`export_dashboard_with_dialog`, `import_dashboard_with_dialog`)
- Modify: `src-tauri/src/app_data_archive.rs` — `export_app_data` gains options or wrapper:
  - `pub struct ExportOptions { pub include_dashboard_runs: bool }`
  - `should_exclude` / export walk skips `dashboard-runs` when flag false
- Modify: `src-tauri/src/commands/config.rs` — pass `exportIncludeDashboardRuns` from settings into export
- Modify: tests in `app_data_archive.rs`

**Interfaces:**
- Single file schema:
  ```json
  {
    "format": "datazen.dashboard",
    "version": 1,
    "dashboard": { /* Dashboard without secrets */ }
  }
  ```
- Strip any accidental webhook fields; keep `configId` references.
- Import: validate version + shape; new uuid if id collision; return dashboard

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn export_skips_dashboard_runs_when_disabled() {
    // create data_dir with dashboards.json + dashboard-runs/x.json
    // export_app_data_with_options(..., ExportOptions { include_dashboard_runs: false })
    // unzip and assert no path starts with dashboard-runs/
}

#[test]
fn import_dashboard_json_rejects_bad_format() {
    let err = parse_dashboard_file(br#"{"format":"nope"}"#).unwrap_err();
    assert!(err.to_string().contains("format"));
}
```

- [ ] **Step 2: Implement until tests PASS**

```bash
cargo test -p datazen --lib app_data_archive
cargo test -p datazen --lib dashboard::export
```

- [ ] **Step 3: Wire frontend `dashboardCommands.exportWithDialog` / `importWithDialog`**

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(dashboard): import/export dashboard and optional runs in app zip"
```

---

### Task 6: Dashboard window shell + grid of tiles (manual refresh)

**Files:**
- Modify: `src/lib/windowKind.ts` — add `'dashboard'`
- Modify: `src/lib/windowManager.ts` — `openDashboardWindow(dashboardId: string)`, label `dashboard-${id}`, capability sample
- Modify: `src-tauri/capabilities/default.json` — `"dashboard-*"`
- Modify: `src/App.tsx` — lazy `DashboardWindow`
- Create: `src/stores/dashboardStore.ts`
- Create: `src/windows/dashboard/DashboardWindow.tsx`
- Create: `src/windows/dashboard/ChartWidgetTile.tsx`
- Create: `src/windows/dashboard/WidgetEditorDrawer.tsx`
- Modify: `src/windows/main/MainWindow.tsx` (or menu) — entry to open/list dashboards
- Modify: `src/locales/en.ts`, `src/locales/zh-CN.ts` — keys; run `pnpm menu:labels` if menu entries added in Rust

**Interfaces:**
- `ChartWidgetTile` props:
  ```ts
  {
    widget: DashboardWidget;
    run: WidgetRun | null;
    busy?: boolean;
    onEdit: () => void;
    onHistory: () => void;
    onRefresh: () => void;
  }
  ```
- Convert `WidgetRun` → chart: build a minimal `StatementResult`-compatible object or adapt `transformData` — if `transformData` requires `StatementResult`, add `widgetRunToStatementResult(run)` helper in `src/lib/dashboard/runToResult.ts`.
- Grid: CSS `grid-template-columns: repeat(12, 1fr)`; place tiles with `gridColumn` / `gridRow` from `widget.layout`.

- [ ] **Step 1: Vitest for `widgetRunToStatementResult`**

```ts
it('maps columns/rows into StatementResult shape', () => {
  const sr = widgetRunToStatementResult(sampleRun);
  expect(sr.columns.map(c => c.name)).toEqual(['v']);
  expect(sr.rows).toHaveLength(1);
});
```

- [ ] **Step 2: Implement helper + window routing + empty dashboard create flow**

- [ ] **Step 3: Tile renders `ChartCanvas` when run ok; shows error/empty states otherwise**

- [ ] **Step 4: Manual refresh button calls `run_dashboard_widget` and updates store**

- [ ] **Step 5: Commit**

```bash
git commit -am "feat(dashboard): add dashboard window with chart tiles"
```

---

### Task 7: Run history drawer + editor drawer MVP

**Files:**
- Create: `src/windows/dashboard/RunHistoryDrawer.tsx`
- Modify: `WidgetEditorDrawer.tsx` — connection select (`configId`), SQL textarea, refreshSec, basic chart type/axes (reuse patterns from `AxisConfigurator` sparingly), optional alert fields
- Modify: `DashboardWindow.tsx` — wire drawers

- [ ] **Step 1: History drawer lists `list_widget_runs`; selecting loads `get_widget_run` and renders chart from snapshot**

- [ ] **Step 2: Editor save calls `save_dashboard` with clamped refreshSec**

- [ ] **Step 3: Smoke Vitest for any pure helpers; manual check in `pnpm tauri:dev`**

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(dashboard): widget editor and run history drawer"
```

**M1 exit criteria:** create dashboard with ≥2 widgets, manual refresh stores runs, history replay works, single-file export/import works, app zip includes `dashboards.json` (+ runs by default).

---

## Phase M2 — Monitor engine, isolation, tray, alerts

### Task 8: Monitor connection isolation API

**Files:**
- Create: `src-tauri/src/monitor/mod.rs`
- Create: `src-tauri/src/monitor/connections.rs`
- Modify: `src-tauri/src/services/connection_manager.rs` **or** keep a `MonitorConnectionRegistry` beside it that calls `driver.connect` and stores handles keyed by `monitor:{config_id}`
- Modify: `execute_widget_once` from Task 4 to use monitor registry only
- Add integration-style unit test with mock/driver if feasible; otherwise document manual test

**Interfaces:**
- Produces:
  - `async fn get_or_connect_monitor(&self, config_id: &str) -> Result<ConnectionHandle, ...>`
  - `async fn disconnect_monitor(&self, config_id: &str)`
  - Never inserts into the UI `config_id_map` session used by windows
- Prefer asking drivers for smaller pools: if `DatabaseDriver` has no hook, document that monitor still uses normal `connect` (pool size 3) but **separate** pool instance — isolation still prevents stealing UI pool slots. Optional follow-up: add `connect_with_pool_size` to driver API (out of M2 unless trivial).

- [ ] **Step 1: Test that two logical keys (ui session vs monitor) can both exist for same config_id without sharing pool_id** — if hard to test without DB, test registry map semantics with fake handles.

- [ ] **Step 2: Implement registry + switch `run_dashboard_widget` / engine to it**

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(monitor): isolate monitor connections from UI sessions"
```

---

### Task 9: Pure alert evaluation + cooldown

**Files:**
- Create: `src-tauri/src/dashboard/alert.rs`
- Create tests in same file

**Interfaces:**
- Produces:
  ```rust
  pub fn extract_metric(columns: &[String], rows: &[Vec<serde_json::Value>], rule: &AlertRule) -> Option<f64>;
  pub fn eval_threshold(value: f64, op: &str, threshold: f64) -> bool;
  pub struct CooldownBook { /* HashMap<(widget_id), Instant> */ }
  impl CooldownBook {
      pub fn should_notify(&mut self, widget_id: &str, cooldown_sec: u64, now: Instant) -> bool;
      pub fn mark_sent(&mut self, widget_id: &str, now: Instant);
  }
  ```

- [ ] **Step 1: Failing tests for last/max/avg and each op; cooldown blocks second notify**

- [ ] **Step 2: Implement until PASS**

```bash
cargo test -p datazen --lib dashboard::alert
```

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(dashboard): alert metric evaluation and cooldown"
```

---

### Task 10: MonitorEngine scheduler

**Files:**
- Create: `src-tauri/src/monitor/engine.rs`
- Modify: `src-tauri/src/monitor/mod.rs`
- Modify: `AppState` in `lib.rs` — hold `Arc<MonitorEngine>`
- Modify: `save_dashboard` / settings — call `engine.reload().await`
- Emit event: `dashboard:run-updated` with `{ dashboardId, widgetId, run }`

**Interfaces:**
- Produces:
  - `MonitorEngine::start(app_handle)`
  - `reload_from_store()`
  - `set_paused(bool)`
  - `tick_widget(...)` shared with manual refresh
- Use `tokio::sync::Semaphore` for `maxConcurrentQueries`
- Per-`configId` `Mutex<()>` for serialization
- Skip widgets/`dashboards` with `enabled == false`

- [ ] **Step 1: Unit-test scheduling table build from dashboards (which widgets scheduled, clamped intervals) without real DB**

- [ ] **Step 2: Implement engine loop; on each success/error write_run + emit**

- [ ] **Step 3: Frontend `dashboardStore` listens `dashboard:run-updated` when window open**

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(monitor): background MonitorEngine with run events"
```

---

### Task 11: Alert channels — desktop notification + webhook (+ email stub)

**Files:**
- Modify: `src-tauri/Cargo.toml` — add `tauri-plugin-notification` if needed
- Modify: frontend package if plugin requires `@tauri-apps/plugin-notification`
- Modify: capabilities permissions for notifications
- Create: `src-tauri/src/monitor/channels.rs`
- Modify: engine to call channels when alert edge fires / recover

**Interfaces:**
- `async fn notify_desktop(app: &AppHandle, title: &str, body: &str)`
- `async fn notify_webhook(url: &str, payload: &AlertPayload) -> Result<(), ...>` using existing `reqwest`
- `async fn notify_email_stub(...)` — `tracing::info!(channel = "email", "stub; not sent")`
- Payload JSON: `{ dashboardId, dashboardName, widgetId, widgetTitle, value, threshold, op, at }`

- [ ] **Step 1: Webhook test with `mockito` or `httptest` if already in tree; else test payload serialization unit + manual webhook.bingo**

Check if mockito exists; if not, unit-test JSON body shape only.

- [ ] **Step 2: Wire channels; consecutive query failures ≥3 → desktop warning**

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(monitor): desktop and webhook alert channels"
```

---

### Task 12: System tray + close-to-tray + settings UI

**Files:**
- Modify: `src-tauri/src/lib.rs` — build tray when monitor enabled / settings.trayEnabled
- Tray menu: Open dashboards / Pause monitoring / Quit
- Modify: window close handlers — if `closeToTray` and monitoring active, hide instead of exit
- Modify: `src/windows/settings/SettingsWindow.tsx` — Monitor section (tray, webhook URL, maxConcurrent, retention, exportIncludeDashboardRuns; email fields disabled)
- Sync `MonitorSettings` on `AppSettings`

- [ ] **Step 1: Implement tray + pause wiring to `MonitorEngine::set_paused`**

- [ ] **Step 2: Settings save roundtrip test (Rust defaults) + i18n keys en/zh-CN**

- [ ] **Step 3: Manual checklist in commit message body optional; Commit**

```bash
git commit -am "feat(monitor): system tray and monitor settings"
```

**M2 exit criteria:** with app in tray and dashboard window closed, widgets still refresh; threshold breach sends desktop notification and webhook; UI query on same connection remains responsive (separate pools).

---

## Phase M3 — Deferred (do not implement in this plan)

Create a separate plan later covering:

1. SMTP email channel implementation  
2. `--monitor` headless mode reusing `MonitorEngine`  
3. QueryPanel “Add to dashboard”  
4. SQLite UI↔monitor mutex / short-query guidance in UI  

---

## Self-review (plan vs spec)

| Spec requirement | Task |
|------------------|------|
| Multi-chart page | Task 6 |
| Persist dashboard definition | Task 2 |
| Run history storage + UI | Tasks 3, 7 |
| App data ZIP includes dashboards/runs | Task 5 |
| Single-file dashboard import/export | Task 5 |
| MonitorEngine + refresh | Task 10 |
| Tray resident R3 MVP | Task 12 |
| Desktop + webhook; email stub | Task 11 |
| Connection isolation | Task 8 |
| Alert rules + cooldown | Task 9 |
| M3 items | Deferred section |

No TBD placeholders in task steps. Types consistently use `configId`, `refreshSec`, `WidgetRun`, `MonitorSettings` across tasks.
