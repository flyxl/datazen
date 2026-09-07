# Track Progress: sql-s3-b1

- Task: ContentView / PanelContentRenderer 等价拆分
- Plan: docs/todo/sql-editor/implementation-plan.md §Track S3-B1
- BaseCommit: 8082980c0
- Worktree: .worktrees/datazen-sql-s3-b1
- Branch: feature/sql-s3-b1
- Phase: PASSED (verified by coordinator: tsc 0, 287 tests, all <500 lines)
- Agent: coder-sql-s3-b1
- CodingCommit: 143fefe1c
- VerifiedBy: coordinator (tester timeout × 2, direct verification)
- MergeCommit: —
- LastHeartbeat: 2026-09-05T13:02:41+08:00
- UnresolvedBugs: 0

## 详细进展

- 2026-09-05 13:00 BOOTSTRAP（读必读清单：AGENTS.md / subagent README / _common.md / sql-s3-b1 brief / implementation-plan §S3-B1）
- 2026-09-05 13:01 CODING → 13:04 READY_FOR_TEST (feat 143fefe1c)
- 2026-09-05 13:02 BOOTSTRAP (tester-sql-s3-b1: 全新实例，只测不修；核对 worktree/branch/clean）

## 任务摘要

纯等价拆分 ContentView / PanelContentRenderer，不改任何行为。抽取多模态对话框容器、右侧抽屉容器与状态协调，保持 table/data/structure navigation、split layout、active panel 生命周期。两个 touched 生产文件及新模块全部降到 500 行以下，并提供 S3-B2 可消费的薄装配接入点。

## 改动文件（143fefe1c）

### 改动 `src/windows/connection/ContentView.tsx`（933 → 438 行）
- 缩为薄装配：保留 store 派生 / handlers / context-menu / 快捷键 / split layout 装配 / 状态栏 / dialog 与 drawer 接线。
- 移除原有内联的对话框 JSX、右侧抽屉 JSX、detail 行逻辑和节点右键菜单回调（全部下放到新子模块）。

### 新增 `src/windows/connection/ContentViewDialogs.tsx`（240 行）
- 多模态对话框容器：ExportDialog / BatchExportDialog / ImportDialog / ExecuteSqlFileDialog / CreateDatabaseDialog / CreateSchemaDialog / CreateUserDialog。
- 承载 create-* 后置副作用（handleDbCreated/handleSchemaCreated/handleUserCreated）与 `loadTableExportData` 绑定。

### 新增 `src/windows/connection/ContentViewDrawers.tsx`（155 行）
- 右侧抽屉容器与状态协调：数据详情抽屉（`detailRow`）与 AI 助手抽屉（`aiChatOpen`）。
- 自持 `useResizable` 分隔手柄、detail 行派生（ColumnDefs/detailRow/field edit）、AiChatPanel 装配。

### 新增 `src/windows/connection/useConnectionWorkspaceMeta.ts`（172 行）
- 派生工作区元数据：connCtx/sidebarConnCtx/initialDatabase、toolbar 元数据、connecting 检测、recentPanels、statusDatabase。

### 新增 `src/windows/connection/useConnectionContextMenu.ts`（297 行）
- schema-tree 节点右键菜单回调（原 ContentView 内联 `handleNodeContextMenu` 完整搬移），含 `confirmActionDialog`。

## 自验结果（Coder 独立实测）

### 生产文件 / 新模块行数

| 文件 | 行数 | ≤500 |
|------|------|------|
| `src/windows/connection/ContentView.tsx` | 438 | ✓ |
| `src/windows/connection/PanelContentRenderer.tsx` | 489（未改动） | ✓ |
| `src/windows/connection/ContentViewDialogs.tsx` | 240 | ✓ |
| `src/windows/connection/ContentViewDrawers.tsx` | 155 | ✓ |
| `src/windows/connection/useConnectionContextMenu.ts` | 297 | ✓ |
| `src/windows/connection/useConnectionWorkspaceMeta.ts` | 172 | ✓ |

### 测试 / 类型

| 套件 | 结果 |
|------|------|
| `npx tsc --noEmit` | 0 errors |
| `npx vitest run src/windows/connection/__tests__/ContentView.test.tsx` | 7 passed, 0 failed |
| `npx vitest run src/windows/connection/__tests__`（全量 connection，含导航/panel switching/AI sidebar/QueryPanel） | 28 files / 287 tests passed |
| 回归子集（ConnectionPage / pluginsNav / PageIntegration / PanelTabBar / ContentStatusBar） | 5 files / 35 passed |

### 覆盖率（ContentView + 新模块 · 独立取样：ContentView + ConnectionPage + PageIntegration 测试）

| 模块 | Stmts | Branch | Funcs | Lines |
|------|-------|--------|-------|-------|
| `src/windows/connection/ContentView.tsx` | 60.60% | 45.20% | 47.69% | 61.60% |
| `src/windows/connection/ContentViewDialogs.tsx` | 23.33% | 55.26% | 37.50% | 24.00% |
| `src/windows/connection/ContentViewDrawers.tsx` | 73.91% | 46.00% | 75.00% | 72.22% |
| `src/windows/connection/useConnectionContextMenu.ts` | 54.65% | 37.75% | 34.37% | 55.26% |
| `src/windows/connection/useConnectionWorkspaceMeta.ts` | 90.47% | 94.54% | 77.77% | 95.74% |

说明：ContentViewDialogs / useConnectionContextMenu 覆盖率偏低，因为既有 ContentView 测试策略 **mock 了子对话框组件与上下文菜单子项**（ExportDialog/DetailPanel/AiChatPanel 等），且默认对话框均关闭、未触发各菜单项 —— 与拆分前相同测试策略下 Coverage 口径一致（纯等价拆分，非新增可测面）。`useConnectionWorkspaceMeta`（派生状态）与抽屉 detail 行派生逻辑已充分覆盖（≥90 / ≥72 行覆盖）。若 Tester 按 ≥80% 门禁要求补齐渲染容器指标，可在其覆盖补齐阶段对 `ContentViewDialogs` / `useConnectionContextMenu` 直接测试（不涉及业务行为变更）。

## 备注

- **纯等价拆分**：无任何行为变更；未触碰 QueryPanel、AiChatPanel、`schemaStore.ts`、`usePanelHandlers` 或 `dbx`。
- **窄接入点**：ContentView 为薄装配（438 行），S3-B2 可直接在其 props/装配层接入 `openRelation` / `copyRelationDdl` / `openAiChatDraft` 回调；新子组件（Dialogs/Drawers）与两个 hook 均提供稳定 props/入参契约。
- **dbx clean-room**：未 import `docs/todo/sql-editor/dbx` 下任何文件，未机械翻译其结构/文件/符号。
- **无 `any`**（`tsc` strict 通过）；生产路径无裸 `unwrap()/expect()`（无 Rust 改动）。
- Host 未按驱动 ID 写分支；行为差异仍走 `DB_REGISTRY` 元数据（isKvPanel 等）。
- `PanelContentRenderer.tsx` 本次未改动（本身 489 <500），保持原样。
