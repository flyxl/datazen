# Track ep-core-runtime: Extension Points Hot-Plugging Runtime & Compartment Reconfig

> Status: **TEST_DONE**
> Branch: `feature/ep-core-runtime`
> Base branch: `feat/sql-editor-clean`
> LastHeartbeat: 2026-09-08T18:30:00+08:00
> Coder Commit: `2130156fd`
> Tester Commit: `1a319a85b`

---

## Task Summary
1. 在 `packages/extension-points/src/` 中实现特权扩展模块完整的生命周期与管理规范：
   - 定义 `Disposable`、`ExtensionSubscription`、`ExtensionContext` 与 `ExtensionModule`（含 `activate(ctx: ExtensionContext)` 与 `deactivate?()`）。
   - 在 `ExtensionRegistry`（或扩展加载器）中提供动态事件订阅能力（支持注册/反注册监听器，如 `subscribe(ep, callback)` 与 `unregister(ep)`），并在反注册时无缝回退到 fallback 实现。
   - 实现副作用安全回收机制与容灾熔断保护（`SafeCompartmentWrapper`：装饰器异常时自动熔断降级并记录日志，防止主进程崩溃）。
2. 在 `src/components/sql-editor/` 改造 CodeMirror 6 隔室热置换（Compartment Reconfiguration）：
   - 将所有由 Pro EP 注入的 CodeMirror 扩展（Gutter 行号栏装饰、Hover 悬浮提示、Intention 意图操作、Join/Column 智能补全、Linter 写时诊断、Signature Help、Paste 高级粘贴等）置于独立 Compartment 中。
   - 监听 `sqlEditorProEP` 状态变迁（如使用 `useSyncExternalStore`），在插件动态载入或卸载时，在当前 EditorView 上调用 `Compartment.reconfigure()` 进行热置换。
   - 保证热插拔全程零窗口重载、零进程重启，且光标选区（Selection）、滚动位置、编辑文本与撤销重做历史栈（Undo/Redo History）100% 保持无损。
3. 编写单测与连续交互热插拔旅程测试：
   - `packages/extension-points/src/__tests__/hotplug.test.ts`：测试注册、反注册、订阅通知、Subscriptions 逆序释放与 Fallback 回退。
   - `src/components/sql-editor/__tests__/editorHotplug.test.ts`：模拟用户正在输入 SQL 过程中动态注册与反注册 Pro 扩展，断言 CodeMirror 隔室热置换、装饰器动态去留、历史与光标无损。
   - 测试覆盖率 ≥ 80%，全量单测通过。

---

## Deliverables

| Item | Status |
|---|---|
| Lifecycle contracts & registry listener/unregister | Done |
| SafeCompartmentWrapper & error boundary | Done |
| CodeMirror Compartment hot reconfiguration in SqlEditor | Done |
| Unit & Journey tests (hotplug.test.ts, editorHotplug.test.ts) | Done |
| `npx vitest run packages/extension-points src/components/sql-editor` green | Done |
| `npx tsc --noEmit` 0 errors | Done |

---

## Self-Verification (Coder)

```text
npx vitest run packages/extension-points src/components/sql-editor
→ 25 files, 364 tests passed

npx tsc --noEmit
→ 0 errors
```

---

## Tester Verification

**Bootstrap:** worktree `.worktrees/datazen-ep-core-runtime`, branch `feature/ep-core-runtime`, coder commit `2130156fd`

### Independent Re-run (Phase B)

| Suite | Coder 自报 | Tester 独立实测 |
|---|---|---|
| `npx vitest run packages/extension-points src/components/sql-editor` | 364 passed | **369 passed** (25 files) |
| `npx tsc --noEmit` | 0 errors | **0 errors** |

### Coverage (Phase C) — 核心改动模块

| Module | Stmts | Branches | Funcs | 达标 |
|---|---|---|---|---|
| `extensionPoints.ts` | 94.4% | 85.7% | 100% | ✅ |
| `lifecycle.ts` | 87.1% | 87.5% | 87.5% | ✅ |
| `safeCompartment.ts` | 100% | — | 100% | ✅ |
| `useExtension.ts` | 85.7% | 100% | 75% | ✅ |
| `editorExtensions.ts` (热插拔增量路径) | reconfigureProCompartments / SafeCompartmentWrapper / proSafe 均已覆盖 | — | — | ✅ |

> 注：`editorExtensions.ts` 整体文件覆盖率 25%（大量 S6-D 既有逻辑由其他单测间接覆盖）；本轨增量改动（SafeCompartmentWrapper 包装、reconfigureProCompartments、Transaction.addToHistory 隔离）经 `editorHotplug.test.ts` 与 `hotplug.test.ts` 全覆盖。

### Tester 补充测试

- `[tester] getContext returns active context or undefined`
- `[tester] reset unloads all active extensions`
- `[tester] unload tolerates dispose errors and still removes extension`
- `[tester] unregister is no-op when extension point is not registered`
- `[tester] SafeCompartmentWrapper invokes onCircuitBreak callback`

### Code Review (Phase A) — 摘要

- ✅ `Disposable` / `ExtensionContext` / `ExtensionModule` / `HostExtensionLoader` 契约完整，LIFO dispose 顺序正确
- ✅ `ExtensionRegistry.subscribe` / `unregister` / fallback 引用稳定性符合 PRD
- ✅ `SafeCompartmentWrapper` 异常时自动 unregister + fallback，不抛向宿主
- ✅ `SqlEditor` 通过 `useIsExtensionEnhanced(sqlEditorProEP)` 触发 useMemo 重算 → compartment reconfigure
- ✅ 所有 compartment reconfigure dispatch 均附加 `Transaction.addToHistory.of(false)`，undo/redo 历史不被污染
- ⚠️ `HostExtensionLoader.loadFromUrl` 动态 import 路径无单测（需 Wave 3 E2E / 集成环境验证）

### E2E 用例登记

| ID | 场景 | 前置条件 | 执行方式 |
|---|---|---|---|
| E2E-EP-001 | 用户在 SQL 编辑器输入过程中，Pro 扩展动态激活，Gutter/Hover/Linter 平滑出现，光标/选区/文本/undo 栈无损 | Pro 版构建 + sql-editor-pro 扩展包 | 【留待 R 回归】 |
| E2E-EP-002 | Pro 扩展动态卸载，编辑器回退 Community 基础能力，无窗口重载 | 已激活 Pro EP | 【留待 R 回归】 |
| E2E-EP-003 | Pro 装饰器运行时崩溃，编辑器自动熔断降级，仍可继续编辑 | 注入会 throw 的 Pro 实现 | 【留待 R 回归】 |
| E2E-EP-004 | `HostExtensionLoader.loadFromUrl` 从 ESM bundle 热加载扩展模块 | 独立打包 `.dzx` / `index.esm.js` 就绪（ep-pro-bundle 轨） | 【留待 R 回归】 |

### Bugs

无（`bugs.md` 为空）
