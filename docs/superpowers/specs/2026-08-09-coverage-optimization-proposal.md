# Coverage Optimization Proposal (<80%)

> **Status:** Approved as **Option C** (scoped **80% lines** + Connection/Workflow/Settings windows) — **implemented** on `test/e2e-expand-coverage`  
> **Date:** 2026-08-09  
> **Branch:** `test/e2e-expand-coverage`  
> **Context:** Design `2026-08-09-e2e-expand-coverage-design.md` §9 gate

## Option C results (post-implementation)

| Scope | Lines | Gate |
|-------|------:|------|
| Vitest gated include (overall) | **91.59%** | pass (`vitest.config.ts` per-glob) |
| `src/lib/**` | ~97% | pass |
| `src/stores/**` | ~92% | pass |
| `src/components/DataTable/**` | ~93% | pass |
| `src/components/ai/**` | ~81% | pass |
| Connection / Workflow / Settings windows | 85–96% | pass |
| Rust `store/` | **84.0%** | pass |
| Rust `workflow/` | **90.7%** | pass |
| Rust `theme/` | **89.3%** | pass |
| Rust `sync/adapters/` | **95.1%** | pass |
| Rust `app_data_archive.rs` | **91.6%** | pass |
| Rust `commands/context.rs` | **85.9%** | pass |
| Rust `mcp/permission.rs` | **91.7%** | pass |

Primary fail metric is **lines ≥80%**. React UI packages use softer statement/function/branch floors (see `vitest.config.ts`). Whole-crate Rust remains ~56% (IPC/tray/MCP server out of Option C denominator).

## Measured baseline (pre-Option C)

| Track | Command | Lines | Gap to 80% |
|-------|---------|------:|-----------:|
| Frontend Vitest | `pnpm exec vitest run --coverage` (include `src/**`, exclude locales/generated/tests) | **17.98%** (1426 / 7930) | **−62.02 pp** |
| Rust llvm-cov | `cargo llvm-cov -p datazen --lib --summary-only` | **46.71%** (9654 / 20691 covered lines inverse of missed) | **−33.29 pp** |
| E2E scenario (D) | TC matrix estimate | **~68%** | N/A (no 80% hard gate for D) |

Also fixed compile blocker while measuring: `ConnectionConfig.options` missing in `src-tauri/src/monitor/connections.rs` test stub; Redis UI TS errors blocking `pnpm build` (ImportExport cursor/size, JsonEditor i18n key, RedisConsole font resolver arity, unused import).

## Why numbers are low (honest)

1. **Vitest include was expanded** from 2 language files → nearly all `src/**`. Large React windows (`MainWindow`, `SettingsWindow`, `ConnectionWindow`, DataSync, Workflow) have **0%** line coverage by design (UI exercised in E2E, not jsdom).
2. **Rust host** has heavy IPC/UI glue (`commands/*`, `tray`, `mcp/server`, connection_manager) that unit tests barely touch; adapters/theme/workflow have higher local coverage but dilute the TOTAL.
3. Reaching **true 80% of all lines** would require either massive shallow UI tests or **narrowing the denominator** (coverage scopes). Design forbids shrinking include to fake the number without your approval.

## Recommended strategy (pick one primary path)

### Option A — Scoped 80% gates (recommended)

Define **product-critical packages** with ≥80% lines, leave UI shells to E2E:

**Frontend scopes (Vitest):**
- `src/lib/**` (esp. chart, sqlDialects, export, themePackApply, iconResolver, windowManager)
- `src/stores/**`
- `src/commands/**` (thin wrappers — still valuable)
- `src/components/DataTable/**`, `src/components/chart/**`, `src/components/ai/**` (logic-heavy)
- Explicitly **exclude from gate**: `src/windows/**`, `src/locales/**`, `src/plugins/generated.ts`

**Rust scopes (llvm-cov --html / llvm-cov report with filters):**
- `store/`, `workflow/`, `theme/`, `sync/adapters/`, `app_data_archive.rs`, `commands/context.rs`, `mcp/permission.rs`
- Soft / later: `commands/connection.rs`, `services/*`, `tray.rs`, `mcp/server.rs`

**Pros:** Achievable in 1–2 weeks; matches architecture (E2E for windows).  
**Cons:** Headline “whole-repo 80%” not met — must document scoped gate in CI.

### Option B — Whole-repo 80% (expensive)

- Generate RTL smoke for every window (mount + click primary buttons)
- Rust: integration tests with temp store + mock drivers for all commands
- Estimate: **multi-week**, high maintenance, many brittle tests

**Pros:** Literal 80% on current include.  
**Cons:** Low ROI; fights Tauri/WebKit reality.

### Option C — Hybrid (A now, selective B later)

Ship Option A gates this sprint; only promote specific high-risk windows (e.g. NewConnection, Settings AI section) into unit coverage if regressions recur.

## Concrete next tasks (if Option A approved)

### Frontend (priority order)

1. Raise Vitest coverage on `src/lib/chart/**`, `src/lib/sqlDialects/**`, `src/lib/exportData.ts`, `src/lib/themePackApply.ts` to ≥80% each.
2. Fill store gaps: `connectionStore`, `settingsStore`, `aiStore` edge paths.
3. DataTable: filter bar, pagination pure helpers (already partial).
4. Update `vitest.config.ts` thresholds **only for scoped globs** (or use nyc/istanbul per-directory scripts).
5. Keep `src/windows/**` out of CI fail gate; rely on E2E groups `e2e:core` / `e2e:db`.

### Rust (priority order)

1. Unit tests for `commands/config.rs` validation, `commands/context.rs` (already strong — push remaining branches).
2. Expand `workflow/workflows.rs` error/condition paths (currently ~51% lines).
3. `services/query_executor.rs` / `connection_manager.rs` with mock `DatabaseDriver`.
4. Leave `tray.rs` / full MCP server HTTP at integration level.
5. CI job: `cargo llvm-cov report --fail-under-lines 80` **only on path filters** once tooling supports it; until then, document per-module HTML report checks.

### E2E (scenario, not line coverage)

1. Stabilize suite isolation (sqlite/redis before-hooks failed under pollution) — reset to main window + close extras between specs in `wdio.conf.ts` `after`.
2. Continue mapping remaining `gap` TCs that are automatable without SSH/MCP.
3. Optional: commit a machine-readable `e2e/tc-map.json` for CI scenario %.

## Out of scope unless requested

- Changing product behavior solely to make code easier to cover
- Fake coverage via exclude of large untested trees without documenting scoped gates
- Raising global CI threshold to 80% on current full `src/**` include

## Decision request

Please choose:

1. **Approve Option A** (scoped 80% gates) — I will implement scoped thresholds + priority unit tests next.
2. **Approve Option C** (A + list of windows to unit-test).
3. **Approve Option B** (whole-repo 80%) — expect a separate long plan.
4. **Reject / revise** — tell me the acceptable denominator (e.g. only `src/lib` + `src-tauri/src/store`).
