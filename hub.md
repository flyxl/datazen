# hub.md — 数据浏览 PRD 落地：跨轨协调台账

> 集成分支：`feature/data-browsing-prd`（worktree `.worktrees/datazen-data-browsing-prd`）
> 基线：`main` = `3bb26cf8f`（已并入，merge 零冲突）
> 创建：2026-10-21。**交付时必须删除本文件**（AGENTS.md「进度台账」纪律）。

## 波次规划（按文件冲突面 + 硬依赖）

| Wave | 轨道 | 分册 | 触碰面 | 依赖 |
| --- | --- | --- | --- | --- |
| 1 | `grid-selection` | 01（Step 1–3 + Step 10 抽取） | `src/stores/tableData/*`、`useGridSelection`、`TablePendingChangesBar.tsx`、`TableView.tsx`、`tableDataStore.ts` | — |
| 1 | `insert-backend` | 04 的 **Rust 侧**（驱动契约 + 宿主计划/执行 + 测试） | `packages/driver-api/*`、`src-tauri/src/commands/{data.rs,data_session.rs,data/insert_plan.rs}` | — |
| 2 | `grid-cell` | 01 的 GridCell / VirtualBody / ARIA 段 | `src/components/DataTable/*` | Wave 1 `grid-selection` |
| 2 | `dirty-cells` | 06 | `CellRenderer.tsx`、`VirtualBody.tsx`、`tableDataStore.ts` | Wave 1 `grid-selection` |
| 2 | `insert-frontend` | 04 的前端草稿行 | `tableDataStore.ts`、`src/stores/tableData/draftInserts.ts` | Wave 1 两条 |
| 3 | `keyboard` / `clipboard` | 02 / 03 | 键盘与剪贴板 | Wave 2 |
| 4 | `editors` / `filters` / `count` | 05 / 08 / 09 | 编辑器族 / 筛选 / 行数三态 | Wave 3（08 的 `query_executor` 目录化是 09 的硬前置） |
| 5 | `result-grid` / `fk-nav` / `keyset` | 10 / 07 / 11 | 结果网格 / 外键 / 键集 | Wave 4 |

## 状态

| 轨道 | 波次 | 状态 | 提交 | 门禁实测 | 备注 |
| --- | --- | --- | --- | --- | --- |
| `grid-selection` | 1 | 派发中 | — | — | 3 个提交：纯逻辑 / store+hook / 抽取 |
| `insert-backend` | 1 | 派发中 | — | — | 驱动契约 + 宿主 + 测试 |

## 已知跨轨约束（派发时必须重申）

1. **`TableView.tsx` 基线 803 行已越 800 红线** —— `grid-selection` 的 Step 10 抽取是后续所有前端轨的前置。
2. **`query_executor` 目录化归属冻结给 08**（`packages/feature-core/src/services/query_executor/`），其他轨不得动。
3. **`00-contracts.md` 是唯一契约源**：坐标系、`CellWrite` 三态、驱动新增方法签名、42 个错误码、行数账本都在那里。
4. 同一 worktree 不得同时被提交方与验证方使用（AGENTS.md）。
