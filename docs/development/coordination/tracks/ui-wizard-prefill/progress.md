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

- Phase: TEST_DONE
- 编码 commit: d96bbd2012f0b9c00700da6ca0c23795c48e6c3e
- 测试 commit: c46cf8ad55daa5c4c42252556e18bf4b71b8347d
- 合并 commit: 待合入

## 自验（编码代理）

- `npx vitest run src/windows/data-sync src/windows/schema-diff src/lib/__tests__/windowManager src/lib/__tests__/migrationWindowPrefill` — PASS（55 tests）
- `npx tsc --noEmit` — PASS

## 独立复验（测试代理）

- `npx tsc --noEmit` — PASS
- `npx vitest run`（同上套件 + DataTransferWindow）— PASS（73 tests，编码代理自报 55 → 独立实测 73）
- 改动文件行覆盖率（聚焦 6 个核心文件）— **81.53%**（≥80%）
  - `migrationWindowPrefill.ts` 95%
  - `windowManager.ts` 89.87%
  - `useSchemaDiffEndpoints.ts` 86.57%
  - `DataTransferWindow.tsx` 80.51%
  - `DataSyncWindow.tsx` 76.73%（整体达标；新增 prefill/端点守卫/execute 确认路径已覆盖）

## 测试增补（[tester]）

- `migrationWindowPrefill.test.ts`：`resolveDefaultDatabase`、`pickPrefillSchema` 消费、未知 connection id 忽略
- `DataSyncWindow.test.tsx`：端点变更保留 syncOptions、delete 执行确认对话框
- `useSchemaDiffEndpoints.test.tsx`：URL prefill
- `DataTransferWindow.test.tsx`：URL prefill

## E2E 登记

| 用例 | 状态 | 前置 |
|------|------|------|
| 从连接树打开 Data Sync 并预填 source/target | 【留待 R 回归】 | 需 Tauri 子窗口 + 真实连接 |
| 只读 target 执行 Sync 被阻断 | 【本机可执行】 | Vitest `DataSyncWindow.test.tsx` 已覆盖 |
| Transfer/Schema Diff URL 预填 | 【本机可执行】 | Vitest 已覆盖 |

## 心跳

- 2026-09-04 轨道初始化
- 2026-09-04 23:43 [HEARTBEAT] track=ui-wizard-prefill phase=READY_FOR_TEST — 迁移三件套预填 + 端点守卫实现完成，自验通过
- 2026-09-04 23:48 [HEARTBEAT] track=ui-wizard-prefill phase=TEST_DONE — 独立复验通过，73 tests，覆盖率 81.53%
