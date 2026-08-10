# E2E Expand Coverage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (inline in this session; user already approved). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expand automatable E2E against `test/test-cases.md`, measure Vitest + llvm-cov (80% gate → propose only), and publish TC coverage matrix (C+D).

**Architecture:** Hybrid: fix local env/`E2E_PG_USER` first, add focused WDIO specs using existing `e2e/helpers.ts` patterns, widen Vitest coverage include for honest reporting, run full suite, then document gaps.

**Tech Stack:** WebdriverIO, Vitest v8 coverage, cargo-llvm-cov, Tauri webdriver E2E (`pnpm e2e`).

**Spec:** `docs/superpowers/specs/2026-08-09-e2e-expand-coverage-design.md`

## Global Constraints

- Work only in worktree branch `test/e2e-expand-coverage`.
- Do not commit `e2e/.env`.
- Do not mock frozen `__TAURI_INTERNALS__.invoke`.
- Do not raise CI coverage thresholds to 80% this round; report only.
- If lines <80%, write optimization proposal and stop (await review).
- Annotate new `it(...)` titles with `TC-*` ids.
- Default PG user in code fallbacks may stay `'postgres'` for CI, but local `.env` must set `E2E_PG_USER=wuxiaolong`; fix hardcoded `username: 'postgres'` in specs that ignore env.

---

### Task 1: Environment bootstrap

**Files:**
- Create (local only): `e2e/.env`
- Modify: `e2e/.env.example` (comment only)
- Run: `pnpm install`, `node e2e/create-sqlite-test-db.mjs`

- [ ] **Step 1: Install deps**

```bash
cd /Users/wuxiaolong/code/rust-projects/datazen/.worktrees/test-e2e-expand-coverage
pnpm install
```

- [ ] **Step 2: Write `e2e/.env`**

```bash
cat > e2e/.env <<'EOF'
E2E_PG_HOST=127.0.0.1
E2E_PG_PORT=5432
E2E_PG_DB=postgres
E2E_PG_USER=wuxiaolong
E2E_PG_PASSWORD=
E2E_MYSQL_HOST=127.0.0.1
E2E_MYSQL_PORT=3306
E2E_MYSQL_DB=datazen_test
E2E_MYSQL_USER=root
E2E_MYSQL_PASSWORD=
E2E_REDIS_HOST=127.0.0.1
E2E_REDIS_PORT=6379
E2E_REDIS_PASSWORD=
EOF
```

- [ ] **Step 3: Generate SQLite fixture**

```bash
node e2e/create-sqlite-test-db.mjs
test -f e2e/fixtures/test.db
```

- [ ] **Step 4: Note in `.env.example`** that Homebrew installs often use the OS username instead of `postgres`.

---

### Task 2: Unify PG credentials to env

**Files:**
- Modify: `e2e/specs/edit-delete-connection.ts` (TEST_CONN.username/password/database from env)
- Modify: `e2e/wdio.conf.ts` (prefer `E2E_PG_*` over `PG_*`)
- Verify: `e2e/helpers.ts` already reads `E2E_PG_USER` (keep fallback)

- [ ] **Step 1: In `edit-delete-connection.ts`, set:**

```ts
username: process.env.E2E_PG_USER || 'postgres',
password: process.env.E2E_PG_PASSWORD || '',
database: process.env.E2E_PG_DB || 'postgres',
host: process.env.E2E_PG_HOST || 'localhost',
port: Number(process.env.E2E_PG_PORT) || 5432,
```

- [ ] **Step 2: In `wdio.conf.ts` seed hook, prefer `E2E_PG_USER` then `PG_USER` then `postgres`.**

---

### Task 3: `connection-validation.ts` (CONN-005/006/007, EDGE-007)

**Files:**
- Create: `e2e/specs/connection-validation.ts`

- [ ] **Step 1: Add suite** that opens 新建连接, attempts 测试连接 with empty required fields / invalid host / wrong password / empty password; assert error UI or no crash; cleanup windows via `closeExtraWindows`.

Pattern: mirror `new-connection.ts` window open/close; use PostgreSQL form defaults; wrong password against local PG.

---

### Task 4: `hotkeys.ts` (HOTKEY-001~005)

**Files:**
- Create: `e2e/specs/hotkeys.ts`

- [ ] **Step 1: Assert Cmd+N opens new-connection window; Cmd+, opens settings; in connection window Cmd+Enter executes when possible; Cmd+B toggles sidebar; Cmd+W closes tab/window.** On WebKit failure, fall back to clicking the same UI control and mark TC as `partial` in matrix later.

---

### Task 5: Expand `sql-query.ts` (QUERY-006/008)

**Files:**
- Modify: `e2e/specs/sql-query.ts`

- [ ] **Step 1: Add tests** for cancel long-running query (`pg_sleep` / equivalent) and open query history panel; reuse existing `executeSQL` / connection setup.

---

### Task 6: `edge-cases.ts` (EDGE-001/002/004/008)

**Files:**
- Create: `e2e/specs/edge-cases.ts`

- [ ] **Step 1: IPC/UI tests** for very long connection name (save + list), special-char SQLite path or PG schema name, SELECT large generate_series, rapid double-click connect without crash.

---

### Task 7: `chart-views.ts` (CHART-002~008/012)

**Files:**
- Create: `e2e/specs/chart-views.ts`

- [ ] **Step 1: Reuse chart fixture table pattern from `chart-expand.ts`**; assert chart type buttons, empty state, recommend, table↔chart toggle. Skip PNG/SVG export if dialog-bound.

---

### Task 8: Expand `settings.ts` (SET-003/004/007)

**Files:**
- Modify: `e2e/specs/settings.ts`

- [ ] **Step 1: Open settings window/section**; assert editor font controls, page size / data browse settings, Prompt customization entry exists (no live LLM).

---

### Task 9: `ui-window-ops.ts` (UI-001/002/003/005)

**Files:**
- Create: `e2e/specs/ui-window-ops.ts`

- [ ] **Step 1: Open two connection windows; re-open same connection; assert sidebar resize handle or width change; assert status bar text contains connection/db info.**

---

### Task 10: Expand `table-data.ts` (TABLE-004/008/009)

**Files:**
- Modify: `e2e/specs/table-data.ts`

- [ ] **Step 1: Add filter UI, multi-row select, empty table** cases using existing seeded table helpers.

---

### Task 11: Vitest coverage include (report-only)

**Files:**
- Modify: `vitest.config.ts`

- [ ] **Step 1: Set coverage include to `src/**/*.{ts,tsx}` with excludes for tests, `src/locales/**`, `src/plugins/generated.ts`; disable failing thresholds (comment or remove) so report always completes.**

---

### Task 12: Run measurements + TC matrix + optional optimization doc

**Files:**
- Create: `docs/superpowers/specs/2026-08-09-e2e-tc-coverage-matrix.md`
- Create if needed: `docs/superpowers/specs/2026-08-09-coverage-optimization-proposal.md`

- [ ] **Step 1:** `pnpm test:unit:coverage` → record lines %
- [ ] **Step 2:** `cargo llvm-cov -p datazen --lib --summary-only` → record lines %
- [ ] **Step 3:** `pnpm e2e` (or build once then skip-build groups); record pass/fail/skip
- [ ] **Step 4:** Write TC matrix from `test/test-cases.md` + new specs
- [ ] **Step 5:** If either coverage <80%, write optimization proposal; do not implement

---

## Spec coverage check

| Spec section | Task |
|--------------|------|
| §4 Local env | 1 |
| §6.1 env/helpers fix | 2 |
| connection-validation | 3 |
| hotkeys | 4 |
| sql-query expand | 5 |
| edge-cases | 6 |
| chart-views | 7 |
| settings expand | 8 |
| ui-window-ops | 9 |
| table-data expand | 10 |
| §7 Vitest/llvm-cov | 11–12 |
| §8 matrix / §9 proposal | 12 |
