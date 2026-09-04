# Track `ui-wizard-prefill` — progress

> Initiative: UI/UX Enhancement
> Plan: `docs/development/coordination/ui-enhancement-plan.md`（Wave 3）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/ui-enhancement`

## 范围

迁移三件套（Sync/Transfer/Diff）向导守卫与参数预填统一：
1. `src/lib/windowManager.ts`：三迁移窗口（DataSync / DataTransfer / SchemaDiff）统一支持预填充上下文参数（`sourceId`、`targetId`、`sourceDatabase`、`targetDatabase`、`sourceSchema`、`targetSchema`），与已有的 Backup 预填机制保持一致。
2. `src/windows/data-sync/DataSyncWindow.tsx`：
   - 改端点（source/target/database/schema）时，只清空上一次比对结果与校验缓存，保留用户已勾选的配置选项（如模式、batchSize、各类高级选项等）。
   - `handleExecute` 中针对只读目标（`targetReadOnly`）进行明确拦截并弹出提示，而不是静默 `return`。
3. `src/windows/schema-diff/SchemaDiffWindow.tsx`：
   - 检查并支持预填参数；改端点时保留已配置 options。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: 待提交
- 测试 commit: 待测试
- 合并 commit: 待合入

## 自验

- `npx vitest run src/windows/data-sync src/windows/schema-diff src/lib/__tests__/windowManager src/lib/__tests__/migrationWindowPrefill` — PASS（55 tests）
- `npx tsc --noEmit` — PASS

## 心跳

- 2026-09-04 轨道初始化
- 2026-09-04 23:43 [HEARTBEAT] track=ui-wizard-prefill phase=READY_FOR_TEST — 迁移三件套预填 + 端点守卫实现完成，自验通过
