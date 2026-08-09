# Task 7 Report: E2E + docs polish

**Branch:** `feat/redis-deep-ops-e1`  
**Scope:** E1 write-path E2E, competitive doc, design spec status

## Deliverables

### 1. `e2e/specs/redis.ts`

Extended existing Redis spec (RD-001~RD-015 → RD-001~RD-021):

| ID | Case |
|----|------|
| RD-016 | Size column header (`redis.size`) visible on Items tab |
| RD-017 | Flush DB / Flush All buttons **absent** when `allowFlush` default off |
| RD-018 | Create string key via workbench dialog |
| RD-019 | Edit string value in detail panel + save |
| RD-020 | Batch-delete single selected key |
| RD-021 | Batch-delete two keys |

**Skip conditions (graceful, like PG/Kiwi specs):**

- `E2E_SKIP_REDIS=1`
- TCP unreachable to `E2E_REDIS_HOST:E2E_REDIS_PORT` (default `127.0.0.1:6379`)
- Connection setup / driver test failure in `before`

Uses `beforeEach` + `shouldSkip` so the suite does not fail CI without Redis.

**Not automated in E2E (documented):**

- Toggling `pluginSettings.redis.allowFlush` via Settings → Extensions (multi-window + persisted settings = flaky in WebdriverIO). Flush **deny** when setting off is covered by Rust unit tests in `packages/drivers/redis` (`flush_*` rejects when `allow_flush: false`). E2E asserts UI hidden by default (RD-017).

**Selectors:** Text/i18n keys only (`t('redis.createKey')`, etc.). No `RedisWorkbench` changes required.

### 2. `docs/competitive-comparison-dbx.md`

Redis row updated: DataZen now notes E1 CRUD/batch/Size/gated Flush landed; console/INFO → E2; Cluster/TLS → E3; Pub/Sub·Stream writes → E4.

### 3. `docs/superpowers/specs/2026-08-09-redis-deep-ops-e1-design.md`

Status: **Approved → Implemented**.

## E2E run

```bash
pnpm e2e:skip-build -- --spec e2e/specs/redis.ts
```

**Result:** Not executed in this session — no webdriver debug binary under `src-tauri/target/debug/datazen` in the worktree. Run after `pnpm tauri build --debug --features webdriver` with Redis listening locally.

## Self-review

- [x] Extends existing `redis.ts` patterns; no full rebuild required when Redis absent (skip).
- [x] Covers create / edit / delete / batch-delete / Size / flush-hidden-default.
- [x] Competitive doc distinguishes E1 vs E2/E3/E4.
- [x] Spec status updated after docs + e2e land.
- [x] No `RedisWorkbench` edits for this task.
- [ ] Live E2E pass — pending binary + Redis on agent machine.

## Files changed (Task 7 only)

- `e2e/specs/redis.ts`
- `docs/competitive-comparison-dbx.md`
- `docs/superpowers/specs/2026-08-09-redis-deep-ops-e1-design.md`
- `.superpowers/sdd/task-7-report.md`
