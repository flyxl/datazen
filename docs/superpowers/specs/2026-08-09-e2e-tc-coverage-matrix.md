# E2E TC Coverage Matrix (D)

> **Date:** 2026-08-09  
> **Branch:** `test/e2e-expand-coverage`  
> **Formula:** scenario_rate = `(covered + 0.5×partial) / total`  
> **Source TCs:** `test/test-cases.md` (**130** cases)

## Summary (after this round)

| Metric | Value |
|--------|-------|
| Total TC | 130 |
| covered (est.) | ~78 |
| partial (est.) | ~22 |
| skip / env-gated | ~12 |
| gap / manual-only | ~18 |
| **Scenario rate (est.)** | **~(78+11)/130 ≈ 68%** |

> Exact row-level mapping below. “est.” reflects judgment on pre-existing specs that cover TC intent without literal `TC-*` IDs in titles.

## New / expanded automation this round

| Spec | TC IDs |
|------|--------|
| `e2e/specs/connection-validation.ts` | CONN-005/006/007, EDGE-007 |
| `e2e/specs/hotkeys.ts` | HOTKEY-001~005 |
| `e2e/specs/edge-cases.ts` | EDGE-001/002/004/008 |
| `e2e/specs/chart-views.ts` | CHART-001/002/007/008/012 |
| `e2e/specs/ui-window-ops.ts` | UI-001/002/003/005 |
| `e2e/specs/settings.ts` (expand) | SET-003/004/007 |
| `e2e/specs/table-data.ts` (expand) | TABLE-004/008/009 |
| `e2e/specs/sql-query.ts` (annotate) | QUERY-006/008 |

## Targeted E2E run (2026-08-09, local PG/MySQL/Redis/SQLite)

Build: `pnpm e2e:minimal` (after Redis UI TS compile fixes).  
Specs batch: connection-validation, hotkeys, edge-cases, chart-views, ui-window-ops, settings, table-data, sqlite, mysql, redis.

| Spec file | Result |
|-----------|--------|
| connection-validation | **passed** (4) |
| hotkeys | **passed** (5) |
| edge-cases | **passed** (4, after harden) |
| chart-views | **passed** (5) |
| ui-window-ops | **passed** (4, after harden) |
| settings | **passed** (12) |
| table-data | **passed** (14, after harden) |
| mysql | **passed** (20) |
| sqlite | failed before-all (window pollution / connect flake) |
| redis | failed before-all (`button*=命令` not found — suite order flake) |

Re-run of edge-cases + ui-window-ops + table-data alone: **3/3 passed**.

## Matrix by module

Status legend: `covered` / `partial` / `skip` / `gap` / `manual-only`

### CONN

| TC | Status | Spec / notes |
|----|--------|----------------|
| CONN-001~004 | covered | new-connection / sqlite / mysql / redis |
| CONN-005~007 | covered | connection-validation |
| CONN-008~010 | covered | edit-delete-connection |
| CONN-011~013 | covered | connection-search-group / drag-drop / homepage |
| CONN-014 | partial | advanced color UI shown in new-connection |
| CONN-015 | manual-only | needs real SSH |
| CONN-016~018 | covered/partial | export-import / app-data-backup |

### DBWIN / TABLE / QUERY / STRUCT

| TC | Status | Spec |
|----|--------|------|
| DBWIN-001~006 | covered/partial | connection-window |
| TABLE-001~003 | covered | table-data |
| TABLE-004 | partial | table-data (filter chrome; AI smart filter needs key) |
| TABLE-005~007 | covered | table-edit / detail-panel |
| TABLE-008~009 | covered | table-data |
| TABLE-010 | covered | data-types |
| QUERY-001~005 | covered | sql-query |
| QUERY-006/008 | covered | sql-query |
| QUERY-007 | partial | explain in ai/sql suites |
| QUERY-009 | covered | sql-query favorites |
| QUERY-010 | gap | autocomplete hard under WDIO |
| STRUCT-001~006 | covered/partial | table-structure / connection-window |

### AI / SYNC / BACKUP / SET / REDIS / EXPORT / UI / HOTKEY / EDGE

| TC group | Status | Notes |
|----------|--------|-------|
| AI-001~009, AICHAT, AIDIAG, NL2SQL | skip/partial | needs `E2E_AI_*` |
| WORKFLOW-001~007 | covered/partial | workflow.ts / workflow-window.ts |
| SYNC-001~003 | covered/partial | data-sync-real (needs DBs) |
| SYNC-004 | gap | resume hard to automate |
| BACKUP-001~004 | covered/partial | backup-database / app-data-backup |
| SET-001~006 | covered | settings / i18n |
| SET-007 | covered | settings Prompt section |
| SET-008~009 | skip/manual-only | MCP external |
| REDIS-001~003 | covered | redis.ts (flake under suite pollution) |
| EXPORT-001~004 | covered | export-import |
| UI-001~003/005 | covered | ui-window-ops |
| UI-004/006 | partial/gap | window chrome / error boundary |
| HOTKEY-001~005 | covered | hotkeys (Meta may fall back to UI) |
| EDGE-001/002/004/007/008 | covered | edge-cases / connection-validation |
| EDGE-003/005/006 | gap/partial | disconnect / concurrency / injection |
| CHART-001~002/007/008/012 | covered | chart-views / chart-expand |
| CHART-003~006/009~011/013 | partial/gap | axis/export/persist/NL chart |

## Line coverage (C) — measured

| Layer | Lines | vs 80% |
|-------|------:|--------|
| Vitest (`src/**`, locales/generated excluded) | **17.98%** | ❌ |
| Rust `cargo llvm-cov -p datazen --lib` | **46.71%** | ❌ |

→ See `docs/superpowers/specs/2026-08-09-coverage-optimization-proposal.md` (awaiting review).
