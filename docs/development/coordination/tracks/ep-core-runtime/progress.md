# Track ep-core-runtime: Extension Points Hot-Plugging Runtime & Compartment Reconfig

> Status: **READY_FOR_TEST**
> Branch: `feature/ep-core-runtime`
> Base branch: `feat/sql-editor-clean`
> LastHeartbeat: 2026-09-08T18:26:00+08:00
> Coder Commit: `2130156fd`
> Tester Commit: `pending`

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
| `npx vitest run packages/extension-points src/components/sql-editor` green | Done (364 tests) |
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
*(待 Coder 完成后由 Tester 独立复测登记)*
