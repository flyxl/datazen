# Wave 4 跨轨台账

> 开发期产物，**交付时必须删除**。结论要落成代码、测试与 `docs/` 正式文档。

## 轨道状态

| 轨 | 分支 | worktree | 状态 |
| --- | --- | --- | --- |
| A `dirty-cells` | `feature/dirty-cells` | `.worktrees/datazen-dirty-cells` | 最后一轮修复中（DC-D1 护栏 + DC-D2 注释） |
| B `fk-navigation` | `feature/fk-navigation` | `.worktrees/datazen-fk-navigation` | 最终复测中 |

## Wave 4 的两个前置拆分（轨 A）

| 文件 | 前 | 后 | 拆出 |
| --- | --- | --- | --- |
| `src/windows/connection/TableView.tsx` | 776 | **592** | `useTableActions.ts`(144) / `TableToolbar.tsx`(208) / `TableStatusBanners.tsx`(60) |
| `src/stores/tableDataStore.ts` | 758 | **565** | `pendingEditActions.ts`(245) |

**为什么要拆**：剩余 9 册里 **8 册**要碰这两个文件，余量只有 24 / 42 行。不拆就破 800，挤就冲突。
**拆分纪律**：纯搬移、行为逐字等价，用**探针 + golden**证明（`git show <旧版>` 复制成探针 → 同 props 渲染 → 逐字比 `innerHTML`）。

## 本波的三条方法论教训

**① 「golden 抓不住」——它抓的是 markup，不是 hook。**
Tester 变异 `useTableActions` 的 `setFilters` → `setSort`：**6905 条测试全绿**。golden 固化的是渲染结果，而 action 表是 hook、**不渲染任何 DOM**。⇒ **拆分一个「不渲染的部分」时，golden 天然失效**，必须另配"哪个绑定调哪个 store 方法"的断言。已要求补（DC-D1）。

**② mock 不解析 SQL ⇒ 整类缺陷对测试套件隐形。**
轨 B 第 2 轮重写探测 SQL 构造器时漏了 `WHERE`，**49 条用例全绿**（`MockDriver::query` 完全不解析 SQL）。⇒ 已要求用 **sqlparser** 断言"探测语句必须是合法 SQL"，并三次变异自证护栏咬得住（去 `WHERE` 红 4 条、去 `limit+1` 红 2 条、无条件 `ORDER BY` 红 1 条）。

**③ 门禁命令本身可能有 bug。**
06 的 A6 写 `grep -rn "editBuffer\|rebuildEditBuffer\|CellEdit" src/` → **永远不可能为空**（`onCellEdit` 族回调含子串，143 处命中）。⇒ 契约门禁必须带**词边界**（`\b`）。同批还查出 06 §2 的一条"现状事实"已过期（Wave 2 已落地 `data-dt-selected`/`data-dt-cell-type`）。

## 跨轨债务（Wave 5 前要处理）

| # | 事项 | 负责 | 触发条件 |
| --- | --- | --- | --- |
| E1 | 07 的**前端半**（FK 箭头 / 面包屑 / 返回栈 store / `src/types/connection.ts` 的 DTO / `database.ts` 薄封装 / i18n / E2E） | Wave 5 前端轨 | 它要碰 `DataTable`/`VirtualBody`/`GridCell`/`useGridSelection` |
| E2 | `src/types/connection.ts` 的 DTO：端口那套名字（`ForeignKeyTarget*`）已占位，前端**直接沿用** | 同上 | 同上 |
| E3 | 探针**回落路径**：`clickhouse`/`duckdb`/`rqlite`/`turso` 会把**值内联进 SQL 文本**（IPC 与本地日志仍零值） | 已知残留，接受 | 已写进代码注释 |
| E4 | 真实 PG / SQL Server 的外键跳转端到端 | R 回归 | 本环境无 DB；本轨只有 sqlparser 层的保证 |
| E5 | `src/test/setup.ts` 的 `scrollIntoView` 桩是**全局测试基建改动**（轨 A 的独立提交 `4b2d14169`，可单独 revert） | 已知，需在合并报告点名 | 合并报告 |
| E6 | `i18n-sync-check` EXIT=1：轨 A 新增 7 个英文-only key，且**改写**了 `tableData.pendingChanges` 的英文文案（其余 9 种语言仍是旧文案） | 发布前 i18n 同步流程 | 发布前 |
| E7 | 06 的 `dirtyCellKey` 是**只在测试里活着的导出**（生产引用 0；§3.1 要求该符号存在） | 已加注释 | 后续册若按单元格键查询即可用上 |
| E8 | 覆盖率阈值（lines 80 / branches 75）**未实测**（`npx vitest run` 不带 `--coverage` 不触发） | CI | CI 跑 |
| E9 | 06 §6 的"预估行数"表已过期（VirtualBody 写 207 实际 549、DataTable 写 628 实际 589、tableDataStore 写 729 实际 758） | 按"行数唯一账本"规则不再逐册记数 | 契约侧已改为引账本 |

## Wave 5 排法（Wave 4 合流后执行）

| 轨 | 内容 | 边界 |
| --- | --- | --- |
| FE | **07 的前端半**（FK 箭头 + 跳转 + 面包屑 + 返回栈 store + DTO 薄封装 + i18n） | 碰 `DataTable`/`VirtualBody`/`GridCell`/`useGridSelection` ⇒ **本波唯一前端轨** |
| BE | **08 筛选**（`query_executor/filter_sql.rs` 落 feature-core） | `query_executor` **链①**（09→11 依次在后两波） |

**交接给 Wave 5 的硬信息**：
1. `TableView.tsx` 已拆出 `useTableActions`（**后续所有册加 action 的地方**）、`TableToolbar`、`TableStatusBanners`；
2. `tableDataStore.ts` 已拆出 `pendingEditActions` slice（06 的五个暂存动作都在那）；
3. **脏态只有一个真源**：`buildDirtyIndex`（`src/stores/tableData/dirtyIndex.ts`），生产代码**仅一处调用**（`useDirtyIndex`）⇒ 网格与计数条读同一份；
4. `grid.fk.*` 五个码**已登记**进 `src/lib/gridErrors.ts`（集成分支 `7234c4beb`）；
5. 08 的 `filter.mixedLogic` 门禁：`grep -rn "filter.mixedLogic" src/` 必须**零命中**（现存 11 处，08 要删净）；
6. `query_executor` 链的顺序：**08 → 09 → 11**（同文件，串行）。
