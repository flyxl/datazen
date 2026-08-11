# Plugin-Owned Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Redis/Kiwi plugin tests out of Host defaults so `pnpm test:unit`, `cargo test -p datazen --lib`, and `pnpm e2e` never run them; Redis tests live under `packages/drivers/redis`, Kiwi E2E is deleted from Host.

**Architecture:** Approach C from the approved spec. Host Vitest `include` drops `packages/drivers/**`; new `test:unit:drivers` runs driver package Vitest only. Redis E2E moves to `packages/drivers/redis/e2e/` and is only invoked via explicit `e2e:redis`. Kiwi Host spec is deleted. Host sync roundtrip module that imports driver crates is removed; per-driver crate tests remain.

**Tech Stack:** Vitest, WebdriverIO, Cargo, pnpm scripts, existing `e2e/helpers.ts`.

**Spec:** [docs/superpowers/specs/2026-08-11-plugin-owned-tests-design.md](../specs/2026-08-11-plugin-owned-tests-design.md)

## Global Constraints

- Branch: `refactor/plugin-owned-tests` only (never commit to `main` unless asked).
- Host `pnpm test:unit` must not execute any file under `packages/drivers/`.
- Host default E2E (`e2e/specs/**` via `wdio.conf.ts`) must not include kiwi or redis specs.
- Do not add kiwi/redis E2E to `.github/workflows/ci.yml`.
- Keep Host `useConnectionForm` kiwi routing tests.
- Keep `adapter_registry` `force_link_driver_sync_adapters` smoke.

---

### Task 1: Move Redis host Vitest into the redis package + exclude drivers from `test:unit`

**Files:**
- Move: `src/windows/connection/__tests__/redisWorkbench.test.tsx` → `packages/drivers/redis/ui/__tests__/redisWorkbench.test.tsx`
- Move: `src/windows/connection/__tests__/redisConsole.test.ts` → `packages/drivers/redis/ui/__tests__/redisConsole.test.ts`
- Move: `src/windows/connection/__tests__/infoParse.test.ts` → `packages/drivers/redis/ui/__tests__/infoParse.test.ts`
- Move: `src/lib/__tests__/redisSettingsSchema.test.ts` → `packages/drivers/redis/ui/__tests__/redisSettingsSchema.test.ts` (or merge into existing `settings.test.ts` if duplicate)
- Modify: `vitest.config.ts` — remove `packages/drivers/**/*.test.{ts,tsx}` from `test.include`
- Create: `vitest.drivers.config.ts` — same as root config but `include: ['packages/drivers/**/*.test.{ts,tsx}', 'packages/drivers/**/__tests__/**/*.{ts,tsx}']`
- Modify: `package.json` — add `"test:unit:drivers": "vitest run --config vitest.drivers.config.ts"`

**Interfaces:**
- Consumes: existing redis UI modules under `packages/drivers/redis/ui/`
- Produces: imports in moved tests use **relative** paths like `../KeyEditors` (not `../../../../packages/drivers/...`)

- [ ] **Step 1: Move the four host test files and fix imports**

Example import rewrite in `redisWorkbench.test.tsx`:

```ts
import {
  invokeCreateKey,
  invokeHashSet,
  invokeSetString,
  invokeSetTtl,
  type PluginInvokeFn,
} from '../KeyEditors';
import {
  invokeBatchDeletePattern,
  invokeDeleteKeys,
} from '../BatchBar';
```

Same pattern for `redisConsole` → `../redisCommands`, `../consoleHistory`; `infoParse` → `../infoParse`; `redisSettingsSchema` → `../settings`.

Delete the old files under `src/`.

- [ ] **Step 2: Update Vitest configs and package.json scripts**

`vitest.config.ts` `include` must be only:

```ts
include: [
  'src/**/*.test.{ts,tsx}',
  'scripts/__tests__/**/*.test.{ts,mjs}',
],
```

`vitest.drivers.config.ts`:

```ts
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@datazen/plugin-sdk': resolve(__dirname, 'src/plugin-sdk'),
    },
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    include: [
      'packages/drivers/**/*.test.{ts,tsx}',
      'packages/drivers/**/__tests__/**/*.{ts,tsx}',
    ],
  },
});
```

`package.json`:

```json
"test:unit:drivers": "vitest run --config vitest.drivers.config.ts"
```

Do **not** add pretest inject for `test:unit:drivers` unless a test needs `generated.ts` (Redis UI tests should not).

- [ ] **Step 3: Verify Host unit suite excludes drivers**

Run: `pnpm test:unit 2>&1 | tee /tmp/unit-host.txt | tail -20`

Expected: all pass; `rg 'packages/drivers' /tmp/unit-host.txt` finds **no** test file paths under `packages/drivers`.

- [ ] **Step 4: Verify driver unit suite**

Run: `pnpm test:unit:drivers`

Expected: pass (includes redis UI tests).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "$(cat <<'EOF'
refactor(test): move Redis Vitest into driver package

Host pnpm test:unit no longer includes packages/drivers; use
pnpm test:unit:drivers for path-driver UI unit tests.
EOF
)"
```

---

### Task 2: Relocate Redis E2E, delete Kiwi Host E2E, update scripts

**Files:**
- Move: `e2e/specs/redis.ts` → `packages/drivers/redis/e2e/redis.ts`
- Move: `e2e/specs/redis-topology.ts` → `packages/drivers/redis/e2e/redis-topology.ts`
- Delete: `e2e/specs/kiwi.ts`
- Modify: import paths inside moved specs (`../helpers.js` → `../../../e2e/helpers.js`, `../i18n.js` → `../../../e2e/i18n.js`)
- Modify: `package.json` `e2e:redis`, `e2e:kiwi`
- Modify: `e2e/helpers.ts` — keep `createAndConnectKiwi` for now **or** leave helpers (no Host kiwi spec callers); if only kiwi used them, can leave dead helpers (YAGNI: leave helpers; document kiwi ownership)

**Interfaces:**
- Consumes: `e2e/helpers.ts`, `e2e/i18n.js` from Host
- Produces: `e2e:redis` points at `packages/drivers/redis/e2e/*.ts`

- [ ] **Step 1: Move redis specs and fix relative imports**

In both moved files, change:

```ts
import { t } from '../../../e2e/i18n.js';
import { /* ... */ } from '../../../e2e/helpers.js';
```

(Adjust if the file used `../i18n.js` / `../helpers.js`.)

- [ ] **Step 2: Delete `e2e/specs/kiwi.ts`**

- [ ] **Step 3: Update package.json scripts**

```json
"e2e:redis": "node e2e/run.mjs --skip-build -- --spec packages/drivers/redis/e2e/redis.ts,packages/drivers/redis/e2e/redis-topology.ts",
"e2e:kiwi": "node -e \"console.error('Kiwi E2E lives in the datazen-driver-kiwi repo; Host no longer ships e2e/specs/kiwi.ts'); process.exit(1)\""
```

Confirm `e2e/wdio.conf.ts` still has `specs: ['./specs/**/*.ts']` only (Host `e2e/specs`), so default `pnpm e2e` does **not** pick up `packages/drivers/redis/e2e`.

- [ ] **Step 4: Smoke-check default spec discovery**

Run: `node -e "const fg=require('fast-glob');"` — or simply:

```bash
ls e2e/specs | rg -i 'redis|kiwi' || echo 'OK: no redis/kiwi in host specs'
ls packages/drivers/redis/e2e
```

Expected: host specs have neither; redis e2e dir has two files.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "$(cat <<'EOF'
refactor(e2e): own Redis E2E in driver package; drop Host Kiwi spec

Default wdio specs stay under e2e/specs; e2e:redis is explicit-only.
EOF
)"
```

---

### Task 3: Remove Host sync roundtrip_tests

**Files:**
- Delete: `src-tauri/src/sync/adapters/roundtrip_tests.rs`
- Modify: `src-tauri/src/sync/adapters/mod.rs` — remove `mod roundtrip_tests;`
- Keep: `adapter_registry.rs` force_link + tests

- [ ] **Step 1: Remove roundtrip module**

`adapters/mod.rs` becomes:

```rust
//! Host-side sync adapter leftovers.
//!
//! Concrete adapters live in path/git driver crates and self-register via inventory.

/// No residual host adapters; path/git drivers register via `inventory`.
#[inline(never)]
pub fn force_link() {}
```

Delete `roundtrip_tests.rs`.

- [ ] **Step 2: Run Host lib tests with basic inject**

Run:

```bash
node scripts/with-plugin-inject.mjs --drivers=basic -- cargo test -p datazen --lib
```

Expected: pass (registry smoke still OK).

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "$(cat <<'EOF'
refactor(sync): drop Host driver roundtrip tests

Per-driver sync_adapter unit tests in packages/drivers own IR mapping;
Host keeps registry link smoke only.
EOF
)"
```

---

### Task 4: Documentation + mark spec approved

**Files:**
- Modify: `docs/superpowers/specs/2026-08-11-plugin-owned-tests-design.md` — status → 已批准；link plan
- Modify: `docs/e2e-testing.md`
- Modify: `docs/architecture/testing.md`
- Modify: `AGENTS.md` (E2E / test commands section)

- [ ] **Step 1: Update docs**

Document clearly:

```markdown
## Plugin-owned tests
- Redis unit: `pnpm test:unit:drivers` (not in `pnpm test:unit`)
- Redis E2E: `pnpm e2e:redis` (explicit; not in `pnpm e2e`)
- Kiwi E2E: `datazen-driver-kiwi` repository only
```

Remove kiwi/redis from Host default E2E file tables; point redis to `packages/drivers/redis/e2e/`.

- [ ] **Step 2: Commit**

```bash
git add -A && git commit -m "$(cat <<'EOF'
docs: document plugin-owned unit and E2E ownership
EOF
)"
```

---

### Task 5: Re-run previously failed Host E2E specs (exclude kiwi)

**Files:** none (verification only)

Failed list from prior run minus kiwi:

`ai-ask-question.ts`, `bugfix-verification.ts`, `connection-window.ts`, `data-sync-real.ts`, `data-types.ts`, `detail-panel.ts`, `edge-cases.ts`, `edit-delete-connection.ts`, `export-import.ts`, `homepage-features.ts`, `i18n-10-locales.ts`, `mysql.ts`, `path-ipc-hardening.ts`, `sql-query.ts`, `sqlite.ts`, `table-data.ts`, `table-edit.ts`, `table-structure.ts`, `workflow-window.ts`, `workflow.ts`

- [ ] **Step 1: Ensure webdriver basic binary exists or rebuild**

If needed:

```bash
DATAZEN_DRIVERS=basic pnpm e2e -- --spec e2e/specs/main-window.ts
```

(or full build once). Prefer `--skip-build` when binary already valid.

- [ ] **Step 2: Re-run the failed Host specs in one command**

```bash
pnpm e2e:skip-build -- --spec \
e2e/specs/ai-ask-question.ts,\
e2e/specs/bugfix-verification.ts,\
e2e/specs/connection-window.ts,\
e2e/specs/data-sync-real.ts,\
e2e/specs/data-types.ts,\
e2e/specs/detail-panel.ts,\
e2e/specs/edge-cases.ts,\
e2e/specs/edit-delete-connection.ts,\
e2e/specs/export-import.ts,\
e2e/specs/homepage-features.ts,\
e2e/specs/i18n-10-locales.ts,\
e2e/specs/mysql.ts,\
e2e/specs/path-ipc-hardening.ts,\
e2e/specs/sql-query.ts,\
e2e/specs/sqlite.ts,\
e2e/specs/table-data.ts,\
e2e/specs/table-edit.ts,\
e2e/specs/table-structure.ts,\
e2e/specs/workflow-window.ts,\
e2e/specs/workflow.ts
```

- [ ] **Step 3: Record results in the PR/summary**

Note pass/fail per file. Environment failures (`datazen_readonly`, missing `product` table) are reported, not silently ignored. Do **not** expand scope to fix unrelated env issues unless trivial.

---

## Spec coverage check

| Spec requirement | Task |
|------------------|------|
| Redis out of `pnpm test:unit` | Task 1 |
| `test:unit:drivers` | Task 1 |
| Redis E2E in package; default e2e excludes | Task 2 |
| Delete Host kiwi E2E | Task 2 |
| Drop Host roundtrip_tests | Task 3 |
| Docs / AGENTS | Task 4 |
| Re-run failed Host specs | Task 5 |
| No CI job for plugin E2E | satisfied by not adding one |

## Placeholder scan

None intentional.
