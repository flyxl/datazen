# Track arch-p1-sdk: Core SDK Splitting & App SDK

> Status: **PASSED**
> Branch: `feature/arch-p1-sdk`
> Base branch: `feat/sql-editor`
> LastHeartbeat: 2026-09-07T23:20:00+08:00

---

## Task Summary
1. 建立 `packages/extension-points/`：
   - 提取 `ExtensionPoint<T>`, `createExtensionPoint`, `ExtensionRegistry`, `extensionRegistry`, `useExtension`, `useIsExtensionEnhanced`。
   - 独立的 `package.json`（name: `@datazen/extension-points`），tsconfig 与干净导出。
2. 建立 `packages/driver-sdk/`：
   - 提取纯净驱动契约：`DatabaseTypeMeta`, `ConnectionMode`, `SqlDialectStrategy`, `SqlDialectProfile`, `BaseTableSqlGenerator`, `TableSqlDialect`, `FunctionEntry`, `ConnectionClipboardParser`, `ConnectionFormState`, `driverCommands`, `syncSchemaTables`, `cachePathItems` 等。
   - 独立的 `package.json`（name: `@datazen/driver-sdk`）。
3. 重命名/升级 `packages/extension-sdk/` 为 `packages/app-sdk/`（`@datazen/app-sdk`）。
4. 在 root `tsconfig.json` 与 `vite.config.ts` 中配置别名：
   - `@datazen/driver-sdk` -> `packages/driver-sdk/src/index.ts`
   - `@datazen/extension-points` -> `packages/extension-points/src/index.ts`
   - `@datazen/app-sdk` -> `packages/app-sdk/src/index.ts`
   - 向后兼容：`@datazen/extension-sdk` -> `packages/app-sdk/src/index.ts`
   - **`@datazen/plugin-sdk`** 暂指向 `src/plugin-sdk/index.ts`（含 UI/settings 的 deprecated 全量 shim），待 arch-p2-callers + `@datazen/ui` 落地后再切换为纯 `driver-sdk` 别名。
5. `src/plugin-sdk/index.ts` 保留 re-export 并添加 `@deprecated` JSDoc。

---

## Coder Result

| Field | Value |
|-------|-------|
| Phase | READY_FOR_TEST |
| Commit | `e7c64f1a5` |

### Self-check
| Suite | Result |
|-------|--------|
| `npx vitest run packages/extension-points packages/driver-sdk` | **PASS** (3 files, 11 tests) |
| `CI=true npx vitest run --config vitest.drivers.config.ts` | **PASS** (18 files, 97 tests) |
| `npx tsc --noEmit` | **PASS** |

### Changed paths
- `packages/extension-points/` (new)
- `packages/driver-sdk/` (new)
- `packages/extension-sdk/` → `packages/app-sdk/` (rename + package name)
- `src/plugin-sdk/index.ts`, `extensionPoints.ts`, `useExtension.ts`, `schemaStoreBridge.ts` (deprecated re-exports)
- `src/plugin-sdk/__tests__/*` (import paths updated)
- `tsconfig.json`, `vite.config.ts`, `vitest.config.ts`, `vitest.drivers.config.ts`, `vitest.e2e-contract.config.ts`

---

## Tester Result

| Field | Value |
|-------|-------|
| Phase | **PASSED** |
| Verified commit | `e7c64f1a5` (coder) |
| Test commit | `test(arch-p1-sdk): verify sdk splitting and backward compatibility` (see branch HEAD) |
| Tester instance | independent (arch-p1-sdk track) |

### Independent re-run (zero-trust)
| Suite | Coder self-report | Tester measured | Match |
|-------|-------------------|-----------------|-------|
| `npx vitest run packages/extension-points packages/driver-sdk` | 3 files, 11 tests PASS | **3 files, 11 tests PASS** | ✓ |
| `CI=true npx vitest run --config vitest.drivers.config.ts` | 18 files, 97 tests PASS | **18 files, 97 tests PASS** | ✓ |
| `npx tsc --noEmit` | PASS | **PASS** | ✓ |
| `npx vitest run src/plugin-sdk/__tests__` (backward compat) | — | **3 files, 11 tests PASS** | — |

### Coverage (new packages, focused include)
| Module | Stmts | Branch | Funcs | Lines |
|--------|-------|--------|-------|-------|
| `packages/extension-points/src/**` | 93.93% | 85.71% | 90.47% | 96.77% |
| `packages/driver-sdk/src/schemaStoreBridge.ts` | 45.83% | 20% | 66.66% | 43.47% |
| **Aggregate (both packages src)** | **81.11%** | **58.33%** | **83.33%** | **82.35%** |

> Aggregate line coverage **82.35%** ≥ 80% threshold. `schemaStoreBridge.ts` sync/subscribe paths (lines 39–71) under-covered; non-blocking for this track (cache paths covered).

### Config & backward-compat review
| Check | Result |
|-------|--------|
| `@datazen/extension-points` package.json (name, exports, peerDeps) | ✓ |
| `@datazen/driver-sdk` package.json (name, exports) | ✓ |
| `@datazen/app-sdk` package.json (name, exports, react subpath) | ✓ |
| tsconfig + vite aliases (driver-sdk, extension-points, app-sdk, extension-sdk compat, plugin-sdk shim) | ✓ |
| `src/plugin-sdk/index.ts` @deprecated JSDoc on all re-exports | ✓ |
| Shim files (`extensionPoints.ts`, `useExtension.ts`, `schemaStoreBridge.ts`) @deprecated | ✓ |

### Code review notes (non-blocking)
- `driver-sdk/src/index.ts` re-exports from `src/lib/**` via relative paths — acceptable interim; full decoupling deferred to later tracks.
- `schemaStoreBridge.ts` throws if store unbound (line 25) — correct fail-fast; covered indirectly via cache tests only.

### E2E registry
| Journey | Status | Notes |
|---------|--------|-------|
| SDK split (no UI path change) | N/A | Pure module extraction; no Host UI interaction change |
| Plugin backward compat via `@datazen/plugin-sdk` | 【留待 R 回归】 | Existing driver E2E indirectly exercises plugin-sdk imports |

### Bugs
None. See `bugs.md` (empty ledger).
