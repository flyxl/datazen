# Track arch-p2-callers: Drivers & Codegen SDK Migration

> Status: **PASSED**
> Branch: `feature/arch-p2-callers`
> Base branch: `feat/sql-editor`
> LastHeartbeat: 2026-09-07T23:31:30+08:00

---

## Task Summary
1. 驱动包迁移：
   - 所有 `packages/drivers/*/ui/` 中 `meta.ts` / `dialect.ts` / `functions.ts` 的 `@datazen/plugin-sdk` 引用已替换为 `@datazen/driver-sdk`；
   - `packages/drivers/redis/ui/` 中 UI 控件（Button, Input, Select, Label, Dialog）改为 `@datazen/ui`；元数据/方言/命令类型改为 `@datazen/driver-sdk`；消除 `@datazen/plugin-sdk` 别名引用；
2. 自动化代码生成工具升级：
   - `scripts/resolve-drivers.mjs` / `scripts/driver-deinject.mjs`：驱动契约类型导入指向 `@datazen/driver-sdk`；
   - 已运行 `node scripts/resolve-drivers.mjs --codegen-only --drivers=basic`，`generated.ts` 中 `DatabaseTypeMeta` 等类型来自 `@datazen/driver-sdk`；
3. 宿主调用方：Host UI 组件已在 arch-p1-ui 轨道迁移至 `@datazen/ui` re-export，本轨无额外宿主改动；
4. `packages/driver-sdk` 补充导出 `TableSchema`（clickhouse dialect 需要）；
5. `vitest.drivers.config.ts` 增加 `@datazen/ui` alias。

---

## Coder Result

| Field | Value |
|-------|-------|
| Phase | READY_FOR_TEST |
| Commit | `5b8bed48a` |

### Self-check
| Suite | Result |
|-------|-------|
| `CI=true npx vitest run --config vitest.drivers.config.ts` | **PASS** (18 files, 97 tests) |
| `npx vitest run packages/driver-sdk packages/ui` | **PASS** (2 files, 11 tests) |
| `npx tsc --noEmit` | **PASS** |

### Changed paths
- `packages/driver-sdk/src/index.ts` (TableSchema export)
- `packages/drivers/*/ui/` (meta, dialect, functions — all path drivers)
- `packages/drivers/redis/ui/*.tsx` (UI → `@datazen/ui`, types → `@datazen/driver-sdk`)
- `scripts/resolve-drivers.mjs`, `scripts/driver-deinject.mjs`
- `vitest.drivers.config.ts`

---

## Tester Result

| Field | Value |
|-------|-------|
| Phase | **PASSED** |
| Verified Coder Commit | `5b8bed48a` |
| Tester Commit | `9a5d9116f` |

### Independent re-run (zero-trust)
| Suite | Coder Self-check | Tester Verified | Match |
|-------|------------------|-----------------|-------|
| `CI=true npx vitest run --config vitest.drivers.config.ts` | 18 files, 97 tests PASS | 18 files, 97 tests PASS | ✓ |
| `npx vitest run packages/driver-sdk packages/ui` | 2 files, 11 tests PASS | 2 files, 11 tests PASS | ✓ |
| `npx tsc --noEmit` | PASS | PASS (0 errors) | ✓ |

### Code review checklist
| Item | Result |
|------|--------|
| `packages/drivers/redis/ui/` — Button/Input/Select/Label/Dialog → `@datazen/ui` | ✓ PASS (no `@datazen/plugin-sdk` imports remain) |
| `packages/drivers/redis/ui/` — types → `@datazen/driver-sdk` | ✓ PASS |
| All path driver `ui/` — no `@datazen/plugin-sdk` | ✓ PASS |
| `scripts/resolve-drivers.mjs` — codegen imports `@datazen/driver-sdk` | ✓ PASS |
| `scripts/driver-deinject.mjs` — codegen imports `@datazen/driver-sdk` | ✓ PASS |
| `src/plugins/generated.ts` — `DatabaseTypeMeta` etc. from `@datazen/driver-sdk` | ✓ PASS (`PluginSettingsContribution` still from `@datazen/plugin-sdk` — expected, settings contribution type) |
| `packages/driver-sdk/src/index.ts` — `TableSchema` export | ✓ PASS |
| `vitest.drivers.config.ts` — `@datazen/ui` alias | ✓ PASS |

### Coverage notes
- Changed files are primarily import-path migrations; existing driver UI tests provide behavioral coverage.
- Driver vitest suite: 18 files / 97 tests, 100% pass rate.
- No new Host UI interaction paths introduced; E2E registration not required for this track.

### E2E Registry
| Journey | Status | Notes |
|---------|--------|-------|
| _(none)_ | N/A | Import-only migration; no new Host UI paths |
