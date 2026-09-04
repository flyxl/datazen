# Track `rem-frontend-split` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-frontend-split，Wave 2）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

大 store/大组件拆分 + 性能 + 类型守卫 + 运行时校验。见计划 §2。Wave 2：等 rem-host-decouple 合并后启动。

## 状态

- Phase: PASSED
- 编码 commit: b3bd3e75e
- 修复 commit: 1023ad3b2
- 测试 commit: ff6e7c8d2
- 合并 commit: —

## 心跳

- 2026-09-04T21:29+08:00 Coder 完成 F1–F8 实现与自验

## 自验结果

- `npx tsc --noEmit`: 0 errors
- `npx vitest run src/stores/tableData/__tests__/pendingChanges.test.ts`: 8 passed / 8
- `npx vitest run`: 2423 passed / 2423 (296 files) — 初版编码自验
- 对外 store selector / re-export API 保持兼容

## Tester 复验结果（2026-09-04T21:45+08:00）

- `npx tsc --noEmit`: 0 errors
- `npx vitest run src/stores/tableData/__tests__/pendingChanges.test.ts`: 8 passed / 8
- 改动相关套件: 594 passed / 594 (55 files)
- BUG-001 修复 commit `1023ad3b2` 审查通过
- 测试 commit: `ff6e7c8d2`

## 修复记录

- **BUG-001** `effectivePendingIdentity` 过滤 `isPrimaryKey` 列后再调用 `buildRowIdentity`，避免全量 columns 传入时返回 null

## 改动摘要

- **F1** `tableDataStore` → `src/stores/tableData/{types,filterUtils,connectionState,pendingChanges}.ts`，主文件 re-export
- **F2** `schemaStore` → `schemaStoreHelpers.ts` + `schemaStoreState.ts`；`setState` dead branch 合并
- **F3** `panelStore` → `panelTypes.ts` + `panelQueryContext.ts`
- **F4** `ConnectionPage` → `connectionPageUtils.ts` + `useConnectionTabs.ts`
- **F5** `DataSyncWindow` → `useDataSyncWizardState.ts`
- **F6** `VirtualBody` memo + `VirtualRow` + `DataTable` useCallback 行 handler
- **F7** `WorkflowPage` type guards（`workflowPanelGuards.ts`）；`WorkflowChatPanel` `parseValidatedWorkflowDefinition`
- **F8** `extensionBridge` targetOrigin 注释；生产 `console.log` DEV guard

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| 大表滚动/分页/编辑无回归 | 需 GUI | 留待 R 回归 |
| Workflow 页面面板切换无回归 | 需 GUI | 留待 R 回归 |
