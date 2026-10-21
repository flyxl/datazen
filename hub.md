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
| `grid-selection` | 1 | **测试中**（Tester `7037c93a`） | `9435810ba` / `5b406b05b` / `305cc725f` | vitest 617 文件 / 6473 例通过；typecheck EXIT=0；TableView 803→**640**、tableDataStore 729→**706** | 编码代理自报；待 Tester 独立复测 |
| `insert-backend` | 1 | 编码中 | — | — | 驱动契约 + 宿主 + 测试 |

## 协调者裁定记录

| # | 事项 | 裁定 | 落地动作 |
| --- | --- | --- | --- |
| A1 | 分册 01 实装需要 `grid.selection.*` 三个码，但 00 契约 §8.2 的闭集里没有该域（§8.1 却把域指派给 01） | **判契约有缺口，不改实现**：按 §8.1 的登记门禁由协调者补登记 | `941761463` 把 §8.2 从 42 码扩到 45 码（43–45 归属 01），同步域映射表与 12-testing 的三处「42」 |
| A2 | `rowsCoveredBySelection(state)` 单参无法表达 row 模式 | 接受实现的 `GridSelection & { selectedRows?: ReadonlySet<number> }` 退化形（参数个数未变） | 待 Wave 5 前回写契约 §4.2.1 签名说明 |
| A3 | `normalizeCellRange` 取双参 + 未知列返回 `null`（§3.3 与 Step 1/§9.1 冲突） | 按 Step 1/§9.1 的显式要求（返回 `null`）为准，§3.3 为笔误 | 待回写 §3.3 |
| A4 | `gridEventGuards.ts` 未单独建文件（守卫私有在 hook 内） | 本波接受；**Wave 2 的 `grid-cell` 必须外提**，否则 DataTable 复用不了 | Wave 2 简报必列 |
| A5 | 未接 `DataTable`/`VirtualBody`（无高亮、无 `aria-activedescendant`、无 `role="rowheader"`） | 符合本波范围（Wave 2 `grid-cell` 负责） | Wave 2 简报必列 |
| A6 | `TableView` 落到 640 行（优于 A5 的 ≤710），因为连事务生命周期一起搬 | 接受，且这正是 01 点名的抽法 | 回写 01 §1.1 A5 与 00 §9.2 账本 |


## 已知跨轨约束（派发时必须重申）

1. **`TableView.tsx` 基线 803 行已越 800 红线** —— `grid-selection` 的 Step 10 抽取是后续所有前端轨的前置。
2. **`query_executor` 目录化归属冻结给 08**（`packages/feature-core/src/services/query_executor/`），其他轨不得动。
3. **`00-contracts.md` 是唯一契约源**：坐标系、`CellWrite` 三态、驱动新增方法签名、42 个错误码、行数账本都在那里。
4. 同一 worktree 不得同时被提交方与验证方使用（AGENTS.md）。
