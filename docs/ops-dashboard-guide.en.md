# DataZen Ops Dashboard Guide

> This document describes how to use DataZen **Ops Dashboards**: multi-chart layouts, background refresh, alerts, run history, the system tray, and import/export.  
> Source of truth: `src-tauri/src/dashboard/`, `src-tauri/src/monitor/`, `src/windows/dashboard/`, and the design doc `docs/superpowers/specs/2026-08-09-multi-chart-ops-dashboard-design.md`.  
> Dashboards reference saved connections by **`configId`** (same as MCP). Dashboard files never store database passwords.

---

## 1. Overview

An Ops Dashboard is a dedicated window for **on-call / ops / business monitoring**: place multiple chart widgets on one canvas. Each widget binds SQL + chart config. The backend **MonitorEngine** runs them on an interval and can fire desktop notifications or webhooks when a metric crosses a threshold.

| Capability | Description |
|------------|-------------|
| Multi-chart grid | 12-column layout; each widget has its own tile size |
| Timed monitoring | Per-widget `refreshSec` (minimum 30s); scheduling can continue after the dashboard window closes |
| Connection isolation | Monitor uses `monitor:{configId}` handles, separate from UI query sessions |
| Alerts | Numeric threshold + cooldown; channels: desktop, webhook (email settings reserved, not sent yet) |
| History | Every run is persisted; pick a past run to redraw from snapshot (no re-query) |
| Tray | Optional tray: open dashboards, pause/resume monitoring, quit |
| Import/export | Single-file dashboard definition; app-data ZIP can include definitions + run history |

**Vs. table/chart toggle in a connection window**

- Connection-window charts visualize a **one-off query result** and are not scheduled.  
- Ops Dashboards are a **saved monitoring surface** with many widgets, refresh, alerts, and history. They reuse ChartConfig / Recharts, but the product path is separate.

**Current limitations**

- Not Tableau-class BI (no cross-filtering, semantic models, maps).  
- Headless `--monitor` / login-item service is planned later.  
- SMTP email alerts are settings-only; **no mail is sent**.  
- One SQL statement per widget; multi-statement takes the first result set (row cap applies).

---

## 2. Concepts and data model

### 2.1 Dashboard

| Field | Meaning |
|-------|---------|
| `id` / `name` | Identity and display name |
| `layout` | Grid: 12 columns by default; `rowHeight` controls row size |
| `widgets` | Widget list |
| `enabled` | Whether this dashboard participates in background scheduling |

### 2.2 Widget

| Field | Meaning |
|-------|---------|
| `title` | Tile title |
| `configId` | Saved connection id (create the connection first) |
| `sql` | Query driving the chart |
| `chartConfig` | Chart type, axes, aggregation (same shape as query-result charts) |
| `layout` | `{ x, y, w, h }` on the grid |
| `refreshSec` | Interval in seconds (**minimum 30**) |
| `alert` | Optional alert rule |
| `enabled` | Whether this widget is scheduled |

### 2.3 AlertRule

| Field | Meaning |
|-------|---------|
| `metric` | Column (with optional `agg`) |
| `agg` | `last` / `max` / `min` / `avg` / `sum` |
| `op` | `>` / `>=` / `<` / `<=` / `==` / `!=` |
| `threshold` | Numeric threshold |
| `cooldownSec` | Cooldown (default ~300s) |
| `channels` | `desktop` / `webhook` / `email` (email not sent yet) |

Alerts evaluate only after a **successful** run with a computable metric. Failed runs do not clear alert state as a false “recovery”.

### 2.4 WidgetRun

Each scheduled or manual refresh produces a run: `ok` / `error` / `timeout`, with columns, rows (capped ~500), and alert flags for the tile and history drawer.

### 2.5 On-disk layout

```
{app data}/dashboards.json
{app data}/dashboard-runs/
  {dashboardId}/
    {widgetId}/
      {yyyy}/{mm}/{runId}.json
      index.jsonl
```

Default retention is about **200 runs / 30 days** per widget (Settings → Monitor). Credentials stay in the encrypted connections store.

---

## 3. Quick start

### 3.1 Prerequisites

1. At least one **saved** connection (`configId`).  
2. SQL must match the target dialect; monitor opens its own connection.  
3. (Optional) Allow DataZen notifications in the OS.  
4. (Optional) A webhook URL that accepts POST JSON, or set a default in Settings.

### 3.2 Create and open a dashboard

1. In the main window action panel, click **Dashboards** (gauge icon).  
2. Enter a name → **Create**, or select an existing dashboard → **Open**.  
3. The dashboard window shows the grid canvas.

### 3.3 Add the first widget

1. Click **Add widget** (or use the empty-state CTA).  
2. In the editor drawer set title, connection, SQL, refresh interval (≥ 30s), and chart axes.  
3. Save; use **Refresh all** to run immediately.  
4. Keep dashboard and widget `enabled` so the engine schedules on the interval.

### 3.4 Minimal SQL example

```sql
SELECT 'Alpha' AS category, 100 AS amount
UNION ALL SELECT 'Beta', 200
UNION ALL SELECT 'Gamma', 150
```

Chart: bar, `xAxis = category`, `yAxes = [amount]`.

---

## 4. Dashboard window

### 4.1 Toolbar

| Action | Effect |
|--------|--------|
| Refresh all | Run all enabled widgets once now |
| Pause / resume monitoring | Global MonitorEngine pause (all dashboards) |
| Add widget | Open editor for a new widget |
| Import / export | Single-file definition (see §8) |

Closing the dashboard window does **not** stop monitoring by itself.

### 4.2 Tiles

- Show the latest successful run; errors surface on the tile.  
- **Edit** SQL, connection, chart, alert, interval.  
- **History** picks a past snapshot to redraw.  
- Layout uses the 12-column grid.

### 4.3 Pause vs enabled flags

| Control | Scope |
|---------|--------|
| Pause monitoring | Entire engine |
| Dashboard `enabled = false` | That dashboard |
| Widget `enabled = false` | That widget only |

---

## 5. Chart tips

- Prefer small aggregated result sets.  
- ~500-row cap per run; truncation can distort charts.  
- Stable column names for axes and alerts; avoid brittle `SELECT *`.  
- Tune intervals and **Settings → Monitor → max concurrent queries** so you do not overload the database.

---

## 6. Alerts

1. Edit the widget → configure metric, operator, threshold, cooldown, channels.  
2. **Desktop**: OS notification.  
3. **Webhook**: POST JSON (falls back to Settings → Monitor default URL when unset).  
4. **Email**: reserved; not sent in the current release.

Cooldown and cooldown apply only after successful evaluation.

---

## 7. Background monitor, tray, and close behavior

### 7.1 MonitorEngine

On app start the engine schedules enabled widgets by `refreshSec`, limits concurrency, and serializes work per `configId`. Default query timeout is about **60s**.

### 7.2 Tray (Settings → Monitor)

| Setting | Meaning |
|---------|---------|
| Show tray icon | Tray when monitoring is active |
| Close to tray | Closing the **main** window hides to tray instead of quitting (when tray + monitoring allow it) |
| Default webhook URL | Fallback for alerts |
| Max concurrent monitor queries | Global parallelism |
| Run history count / days | Per-widget retention |
| Include run history in app-data export | When off, ZIP skips `dashboard-runs/` |

Tray actions typically include open dashboards, pause/resume, quit.

### 7.3 Close ≠ quit

- Close dashboard window → scheduling continues (unless paused / disabled).  
- Close main window → may stay in tray.  
- Quit from tray (or a non-tray quit path) stops the app.

---

## 8. Import / export and backup

### 8.1 Single-file dashboard

Export writes definition-only JSON (no run history, no passwords). Import may assign a new id on conflict. Target machines need matching `configId` connections or you must rebind widgets.

### 8.2 App-data ZIP

Backup includes `dashboards.json` and, by default, `dashboard-runs/` (toggle in Monitor settings).

---

## 9. Relationship to Workflows

| | Workflow | Ops Dashboard |
|--|----------|---------------|
| Goal | Orchestrate query + AI + branches on demand | Continuous metrics, alerts, history |
| Format | YAML | `dashboards.json` / dashboard JSON |
| Scheduling | Manual / MCP / UI run | MonitorEngine intervals |
| AI | First-class steps | No built-in AI steps |

They complement each other (e.g. Workflow writes aggregates; dashboard SQL reads them).

---

## 10. Troubleshooting

| Symptom | Likely cause | What to try |
|---------|--------------|-------------|
| `Driver not found` | Driver not linked in this build | Use a build with that driver / set `DATAZEN_DRIVERS` in dev |
| Widget always errors | Bad `configId`, SQL, network/ACL | Rebind connection; validate SQL in a query tab |
| No auto refresh | Global pause / disabled flags / interval | Check pause button and enabled; wait or Refresh all |
| No tray | Tray off or no active widgets | Settings → Monitor; enable widgets |
| Main close quits app | Close-to-tray off | Enable tray + close-to-tray |
| No alerts | Channels off, cooldown, bad metric, OS permission | Check rule, notification permission, webhook |
| Empty chart | Axis mismatch / empty result | Inspect History columns and rows |
| Docs window “already exists” | Older singleton race | Update; clicking again should focus the open window |

---

## 11. Recommended practices

1. One metric per widget with a clear title.  
2. Tier intervals (critical 30–60s, others minutes).  
3. Validate alerts with desktop notifications before wiring webhooks.  
4. Lower max concurrent queries when many widgets hit one database.  
5. Version-control definition JSON (no secrets); use full backup for disaster recovery.  
6. Use History for incident review instead of hammering live SQL.

---

## 12. Entry points

| Entry | Where |
|-------|--------|
| Dashboard list | Main window → Dashboards |
| Monitor settings | Settings → Monitor |
| This guide | Help / Docs → Ops Dashboard |
| Design (developers) | `docs/superpowers/specs/2026-08-09-multi-chart-ops-dashboard-design.md` |
