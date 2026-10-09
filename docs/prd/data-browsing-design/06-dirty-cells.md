# 06 · 待提交单元格高亮（DB-06）

> **状态**：目标设计，未实现。本册所有「现状」事实均按 00 契约的基线提交核对，且只写我亲自读过的符号。
>
> **依赖**：[00-contracts](00-contracts.md)（`TableState` / `PendingRowChange` / `data-dt-*` 属性约定 / 错误码机制 / `valuesEqual` 复用铁律，以及**已按增补后版本核对**的第 4 节：选择模型的落点是 `src/stores/tableData/gridSelection.ts` + `src/hooks/useGridSelection.ts`、行级取值域用派生选择器 `rowsCoveredBySelection(state): ReadonlySet<number>`）。本册**只消费**选择模型、**不实现也不复制**它（边界见 2.3 与 4.6）。
>
> **被谁依赖**：10-result-grid（查询结果网格复刻本册的脏索引选择器与 `data-dt-dirty` 属性）。
>
> **预估人日**：2.5（选择器与渲染 1.5、清理时机与旅程测试 0.5、E2E 与主题验证 0.5）。不含 DB-03 的批量粘贴批处理化（第 11 节 D-6）。
>
> **本文档不包含**：① INSERT 草稿行 `inserted` 状态的**产生**（属 DB-04，本册只定义其接入后如何显示与计数）；② 类型化编辑器与 Set Value（DB-05）；③ 区域选择与键盘导航（DB-01 / DB-02）；④ 提交链路、计划指纹与事务（Rust 侧，00 契约已冻结）；⑤ `valuesEqual` 判等语义的升级（第 11 节 D-1，属跨册契约决定）；⑥ 后端任何改动（本册是**纯前端**分册）。

---

## 1. 目标与验收口径

**一句话目标**：把「已暂存、尚未提交」的改动从只存在于 `TableState.pendingChanges` 的内部数据，变成由**单一真源派生**的单元格级 / 行级 / 面板级三层可见状态，并在提交、回滚、刷新、翻页、筛选、断连六种时序下都保持归属正确、计数与高亮永远一致。

**验收标准（命令 + 期望输出）**

| # | 命令 | 期望输出 |
| --- | --- | --- |
| A1 | `npx vitest run src/stores/tableData/__tests__/dirtyIndex.test.ts` | `Test Files  1 passed (1)`、`Tests  N passed (N)`（N ≥ 16），退出码 0 |
| A2 | `npx vitest run src/components/DataTable/__tests__/VirtualBody.dirty.test.tsx` | 全绿；其中断言 `[data-dt-dirty="true"]` 的元素数与选择器 `counts.cellsOnPage` 逐位相等 |
| A3 | `npx vitest run src/stores/__tests__/tableDataStore.dirty.test.ts` | 全绿；必须包含 `journey: edit → highlight → edit back → clean` 与 `journey: stage on page 1 → next page → back` 两条用例 |
| A4 | `npx vitest run` | `Test Files` 全绿、`Tests` 中无 failed；无 `FAIL` 行 |
| A5 | `pnpm typecheck` | 退出码 0，无 `error TS` 行 |
| A6 | `grep -rn "editBuffer\|rebuildEditBuffer\|CellEdit" src/` | **空输出**（死 prop 与派生缓存均已删除） |
| A7 | `pnpm e2e --spec e2e/specs/table-dirty-cells.ts` | 亮色 / 暗色 / 第三方主题（`slate-blue`）三种情况下 `[data-dt-dirty="true"]` 均 `isDisplayed()` 为真 |
| A8 | 人工（长输出纪律见 `AGENTS.md`：重定向到系统临时目录再 `tail`） | 同一行改 3 格 → 面板计数显示 `3 cells changed`（不是 1），网格上恰好 3 个格子带 `data-dt-dirty="true"` |

**本册一句话不变量**：`counts.cellsOnPage`（选择器算出的当前页脏格数）恒等于 DOM 中 `[data-dt-dirty="true"]` 的数量；`counts.totalCells` 恒等于 `pendingChanges` 中所有非删除行的 `changedColumns` 长度之和。

---

## 2. 现状代码事实

以下每条都是我逐个读过源码后写下的事实，位置一律「文件路径 + 符号名」，不写行号。

### 2.1 状态与写路径

| 位置 | 符号 | 事实 |
| --- | --- | --- |
| `src/stores/tableData/types.ts` | `CellEdit` | 字段为 `rowIndex: number`、`columnName: string`、`originalValue: unknown`、`newValue: unknown`、`pkSnapshot: Record<string, unknown>` |
| `src/stores/tableData/types.ts` | `TableState.editBuffer` | `Map<string, CellEdit>`；注释写作「逐格编辑缓冲（键由单元坐标拼成）」 |
| `src/stores/tableData/types.ts` | `TableState.pendingChanges` | `Map<string, PendingRowChange>`；注释写明「键是稳定的 PK 身份」 |
| `src/stores/tableData/types.ts` | `TableState.rowIdentityAnchors` | `Map<number, string>`；注释写明「行号只是临时锚点」 |
| `src/stores/tableData/connectionState.ts` | `editKey(rowIndex, columnName)` | 返回 `` `${rowIndex}:${columnName}` `` —— **位置键**，行号参与构成 |
| `src/stores/tableData/connectionState.ts` | `emptyTableState` | 初始化 `editBuffer: new Map()`、`pendingChanges: new Map()`、`rowIdentityAnchors: new Map()` |
| `src/stores/tableData/connectionState.ts` | `rowsToRecords` | 逐列取 `row[i] ?? null`，因此**数据库 NULL 到达前端就是 `null`，不是 `undefined`** |
| `src/stores/tableData/pendingChanges.ts` | `rebuildEditBuffer(ts)` | 全仓库**唯一**构造 `CellEdit` 的地方；键用 `editKey(rowIndex, column)`；`pkColumns.length === 0` 时直接返回空 Map；`!match \|\| match.change.deleteMarked` 时 `return`（**删除标记行完全不进 buffer**）；只遍历 `ts.rows`（**只覆盖当前页**） |
| `src/stores/tableData/pendingChanges.ts` | `findPendingForRow` | 先查 `rowIdentityAnchors.get(rowIndex)` 命中的键，再回退到 `rowIdentityKey(identity)`；`rowIdentityIsUnique` 为假时返回 `null` |
| `src/stores/tableData/pendingChanges.ts` | `rowIdentityIsUnique` / `hasPendingIdentityCollision` / `effectivePendingIdentity` | 行身份唯一性守卫与「主键列改动后仍用原始身份」的解析 |
| `src/stores/tableData/pendingChanges.ts` | `overlayPendingRows` | 把 `currentValues` 覆盖到已取回的当前页行上，因此暂存值在刷新后仍显示 |
| `src/stores/tableData/pendingChanges.ts` | `pendingChangesSignature` / `pendingChangesForWire` | 提交期的签名守卫与线上裁剪（`pendingChangesForWire` 会去掉 `rowIndex`） |
| `src/stores/tableData/pendingChanges.ts` | `AMBIGUOUS_ROW_IDENTITY_ERROR` | 行身份为 NULL / 不稳定 / 歧义时的既有用户可见文案常量 |
| `src/stores/tableDataStore.ts` | `stageCellChange` | **唯一产生暂存改动的入口**；写 `rows`、`pendingChanges`、`rowIdentityAnchors`、`editBuffer`（经 `rebuildEditBuffer`）、`editingCell: null`、`error: null`；用 `valuesEqual(originalValue, currentValue)` 判定「改回原值」，为真则从 `originalValues` / `currentValues` 删键、从 `changedColumns` 移除，且 `changedColumns` 空且无删除标记时**整条键从 `pendingChanges` 删除** |
| `src/stores/tableDataStore.ts` | `stageRowDelete` | 写 `deleteMarked: true`，**保留**既有 `changedColumns` 与两份值快照；同样重算 `editBuffer` |
| `src/stores/tableDataStore.ts` | `deleteRows` | 先 `selectRow` 语义地选中要删的行，再转 `stageRowDelete` |
| `src/stores/tableDataStore.ts` | `applyColumnToRows` | 对每个行号**逐格循环调用** `stageCellChange`（每次一个 `set()`） |
| `src/stores/tableDataStore.ts` | `rollbackPendingChanges` | 清空 `pendingChanges`、`previewPlan`、`editBuffer`，然后 `reloadPanel` |
| `src/stores/tableDataStore.ts` | `commitPendingChanges` | 成功且 `pendingChangesSignature(latest.pendingChanges) === signature` 时清空 `pendingChanges` / `previewPlan` / `editBuffer`；**签名变了就不清**（提交在途期间暂存的改动被保留）；失败时**只**写 `error`，`pendingChanges` 原样保留 |
| `src/stores/tableDataStore.ts` | `previewPendingChanges` | `pendingChanges.size === 0` 直接返回 `null`；结果只在签名未变时写 `previewPlan` |
| `src/stores/tableDataStore.ts` | `commitFetchedPage` | 重建 `rowIdentityAnchors` → `overlayPendingRows` → `rebuildEditBuffer`；`error` 在存在重复身份键时置为 `AMBIGUOUS_ROW_IDENTITY_ERROR`；每次取数都把 `selectedRows` 清空 |
| `src/stores/tableDataStore.ts` | `invalidateCachedData` | `pendingChanges.size > 0` 的 panel **整体跳过**（不动行、不动暂存改动）；其余 panel 清 `columns` / `rows` / `editBuffer` 并 bump `requestRevision` |
| `src/stores/tableDataStore.ts` | `patchPanel` / `patchPanelForReload` | 唯一的 slice 写入口；`patchPanelForReload` 额外 bump `requestRevision`（竞态防护） |
| `src/stores/tableDataStore.ts` | `editBuffer` 写点汇总 | 共 **6 处**：`commitFetchedPage`（`rebuildEditBuffer`）、`invalidateCachedData`（空 Map）、`stageCellChange`（`rebuildEditBuffer`）、`stageRowDelete`（`rebuildEditBuffer`）、`rollbackPendingChanges`（空 Map）、`commitPendingChanges`（空 Map）。加上 `emptyTableState` 的初始化共 **7 次赋值**，而**读取点为 0** |

### 2.2 既有真源与查找路径：本册的复用 / 扩展关系

上一节列出的 `pendingChanges.ts` 函数不是「待清理的旧代码」，而是**已经存在的真源与查找路径**。本册建立它们之上，只做两件事：**复用**与**把单行查找扩展成整页批量查找**，绝不另立并行机制。

| 既有符号（`src/stores/tableData/pendingChanges.ts`） | 本册关系 | 说明 |
| --- | --- | --- |
| `pendingChanges`（`TableState` 字段） | **唯一真源，复用** | 脏标记、行级删除态、面板计数全部由它派生，不再有第二个容器 |
| `findPendingForRow` | **复用其语义，扩展其粒度** | 它是「单行 → 暂存条目」的既有权威，且**优先查 `rowIdentityAnchors`**、再回退 `rowIdentityKey`。本册的 `buildDirtyIndex` 是它的**整页批量版本**：同一套优先级（先锚点、后身份键）、同一套唯一性守则（身份不唯一则不标脏），差别只是把「每行一次线性查找」变成「一次建索引 + O(改动数) 归类」。两者**必须逐位等价**，见第 9.1 节的等价性用例 |
| `rowIdentityIsUnique` / `hasPendingIdentityCollision` | **复用** | 身份唯一性守则的唯一实现；`buildDirtyIndex` 用 `duplicateRowIdentityKeys` 一次性判定同一件事，不新写判定逻辑 |
| `overlayPendingRows` | **不动** | 它负责「刷新后暂存值仍显示」，是本册高亮能落在正确值上的前提 |
| `pendingChangesSignature` | **不动** | 提交期签名守卫；它保证「提交成功才清标记」，本册的清理时机依赖它 |
| `pendingChangesForWire` | **不动** | 线上裁剪；与渲染无关 |
| `rebuildEditBuffer` | **废弃并删除** | 唯一被称为「脏标记数据源」的候选，但它有损（见 2.7）。它的**能力**由 `buildDirtyIndex` 以无损方式接管 |
| `AMBIGUOUS_ROW_IDENTITY_ERROR` | **复用** | 歧义行的用户可见解释；本册不新增错误码 |

**因此本册新增的只有一个纯函数选择器模块**（`src/stores/tableData/dirtyIndex.ts`），它不新增状态、不新增真源、不改写路径。`dirtyCellKey` 是新增的**键构造函数**，与既有的 `rowIdentityKey` 处在同一键空间（前缀就是行身份键）。

### 2.3 选择模型的既有落点（本册只消费，不实现）

00 契约第 4 节在本次增补中把选择模型的代码落点冻结为**两个文件**，02/03/05/06/07/10 必须复用：

| 位置 | 符号 | 本册关系 |
| --- | --- | --- |
| `src/stores/tableData/gridSelection.ts`（新增） | `CellCoord` / `CellRange` / `GridSelection` / `normalizeCellRange` / 区域求交合并 / 行集推导 / 键构造 | **本册不实现、不修改、不复制**。脏标记是**正交**维度：`data-dt-selected`（选择）与 `data-dt-dirty`（未提交改动）可以同时为真，也可以各自单独为真 |
| `src/hooks/useGridSelection.ts`（新增） | React 接线：接 `TableState`、订阅鼠标与键盘、产出记忆化选择器 | 同上。本册不注册任何鼠标 / 键盘事件（第 4 节明确「不改键盘路径」） |
| `src/stores/tableData/gridSelection.ts`（新增） | `rowsCoveredBySelection(state): ReadonlySet<number>` | **本册不改**「删除行」的作用域来源。本册的行级删除态渲染只依赖 `pendingChanges`，与「哪些行被选中」无关；`pendingChanges` 的清理也**绝不**因选择变化而触发 |

两条必须写进本册实现纪律的推论：

1. **脏判定不参与选择派生，选择不参与脏判定。** `buildDirtyIndex` 的输入只有 `rows` / `columns` / `pendingChanges` / `rowIdentityAnchors`，**没有** `GridSelection` 参数；`rowsCoveredBySelection` 的输入里也没有 `pendingChanges`。任何交叉引用都会让「选中一格」或「取消选择」意外影响高亮。
2. **行级删除态的来源是行身份，不是选中集。** 一行之所以显示删除线，是因为 `pendingChanges` 里那条条目 `deleteMarked === true`；不是因为它被选中。这样翻页 / 筛选 / 选择模式切换都不会改变删除态的归属。

### 2.4 渲染与消费方

| 位置 | 符号 | 事实 |
| --- | --- | --- |
| `src/components/DataTable/DataTable.tsx` | `DataTableProps.editBuffer` | 声明为 `editBuffer?: Map<string, CellEdit>`，注解写着「⚠ 已声明但组件内部从未解构使用」。**核对结论：解构列表里没有它，函数体内零次引用 → 确认为死 prop**；同文件的 `CellEdit` 类型导入也只为这一个 prop 存在 |
| `src/components/DataTable/DataTable.tsx` | `handleContextMenu` | 用 `resolveDataTableCellFromEvent` / `resolveDataTableHeaderColFromEvent` 反查单元格；菜单项由 `buildDataTableContextMenuItems` 的 `labels` / `handlers` / 能力布尔组装 |
| `src/components/DataTable/VirtualBody.tsx` | `VirtualRow` / `VirtualBody` | 两者都是 `memo`；单元格 `div` 带 `data-testid="data-table-cell"`、`data-dt-row`、`data-dt-col`；行号按钮 `title={selectRowLabel}`，**无任何状态标记**；`isEditing` 由 `editingCell?.row === vRow.index && editingCell.col === col.name` 推出 |
| `src/components/DataTable/CellRenderer.tsx` | `CellRenderer` | `memo`；按 `classifyDataType` 分支渲染 `<span>`，颜色来自 `cellValueTextClass`；NULL 渲染为 `text-dt-null` 的 `NULL` |
| `src/components/DataTable/EditableCell.tsx` | `EditableCell` / `coerceAndCommit` | Enter / blur 提交，Escape 取消；`raw === ''` 且原值为 `null` 时走 `onCancel`；`raw === ''` 且原值非空时**提交 `null`**（即内联编辑器**无法产出空字符串**，只有粘贴路径能产出）；无 `isComposing` 判断 |
| `src/lib/dataTypeColors.ts` | `classifyDataType` / `cellValueTextClass` / `FAMILY_CLASS` | `--dt-*` 是**前景色**（`text-dt-*`），7 个家族：null / bool / number / datetime / json / text / binary |
| `src/lib/dataTableContextMenu.ts` | `buildDataTableContextMenuItems` | 入参为 `labels`（`DataTableContextMenuLabels`）、`handlers`（`DataTableContextMenuHandlers`）与能力布尔（`canDelete` / `canSetNull` / `canFilterByValue` / `exportEnabled` 等）；菜单项为空 action 时不生成 |
| `src/lib/dataTableContextMenu.ts` | `resolveDataTableCellFromEvent` | 用 `closest('[data-dt-row][data-dt-col]')` 反查，是项目确认过的正确做法 |
| `src/windows/connection/TableView.tsx` | `pendingChanges` / `pendingUpdateCount` / `pendingDeleteCount` | 计数从 `pendingChanges` 现算：更新行 = `!deleteMarked && changedColumns.length > 0`，删除行 = `deleteMarked` |
| `src/windows/connection/TableView.tsx` | `pending-changes-bar` | 面板计数条（`data-testid`），文案 `t('tableData.pendingChanges', { count: pendingChanges.size })` + 更新数 + 删除数 + Preview/Commit/Rollback 按钮 |
| `src/windows/connection/TableView.tsx` | `actions` | panel 作用域的 action 绑定表（`useMemo`），每个 action 都闭包了 `panelId` |
| `src/windows/connection/TableView.tsx` | `handleCellDoubleClick` / `handleCellEdit` | 只读时提示并拒绝；否则 `startEdit` / `stageCellChange` |
| `src/windows/connection/TableView.tsx` | `DataTable` 调用点 | **没有传 `editBuffer`**（因为它是死 prop），也没有任何脏状态相关 prop |
| `src/locales/en/query.ts` | `tableData.pendingChanges` | 文案 `'{count} unsaved changes'`，**但调用方传的是 `pendingChanges.size`（行数）** —— 同一行改 3 格会显示 `1 unsaved changes`，这是「计数与高亮不一致」缺陷形态的现状载体 |
| `src/locales/en/query.ts` | `tableData.pendingUpdates` / `tableData.pendingDeletes` / `tableData.errorPendingHint` | 既有行级计数文案与「有未提交改动时出错」的提示 |
| `src/styles/themes.css` | `:root` / `.dark` 的 `--dt-*` | 只有 7 个类型色 token，注释声明「overridable by theme packs」；**没有任何脏标记 token** |
| `tailwind.config.ts` | `colors.dt` | 把 7 个 `--dt-*` 映射为 `text-dt-*` 等工具类 |
| `src/locales/locales.test.ts` | `resolves every literal t() key used in host source` | 扫描宿主源码里 `t('字面量键')` 并要求键存在于 `en` 词典 —— **新增文案必须同时加 `en` 词条，且调用点必须写成字面量键** |
| `src/lib/gridErrors.ts` | — | **基线里不存在**（00 契约把它标为「新增」，落点建议在 DB-08）。因此本册不得依赖 `classifyGridError()`，错误展示一律按 00 契约「未知则回退显示原始消息」 |

### 2.5 既有测试（读法参考，不在本册改动范围外乱动）

- `src/stores/tableData/__tests__/pendingChanges.test.ts`：含 `rebuildEditBuffer skips delete-marked rows`；用 `emptyTableState()` 手工搭 `TableState`。
- `src/stores/__tests__/tableDataStore.test.ts`：含 `retains null originals and removes a change when reverted`、`commit failure preserves pending changes`、`invalidateCachedData keeps panels with staged edits untouched`、`preview and successful commit clear pending changes and request refresh`。
- `src/components/DataTable/__tests__/CellRenderer.test.tsx`：用 `container.querySelector('span')` + `className` 断言类型色 —— **改 `CellRenderer` 的 DOM 结构会打破这 5 条断言**。
- `src/components/DataTable/__tests__/VirtualBody.test.tsx`：mock 掉 `useVirtualTable` 与 `useI18n`，逐个断言 `data-testid` / `title` / 事件回调。
- `e2e/specs/table-edit.ts`：依赖 `data-testid="pending-changes-bar"`、`pending-preview`、`pending-commit`、`pending-rollback`、`pending-plan-dialog`、`table-edit-input`。

### 2.6 我核实过的、与「脏标记」相关的既有视觉状态

`grep -rniE "dirty|unsaved|modified"` 在 `src/` 的命中只有两类，**都不能复用**：

1. `src/components/FilterEditor.tsx` 的局部变量 `dirty` = 「筛选草稿与已应用筛选不相等」，与网格改动无关。
2. `src/lib/kvSlotState.ts`（Redis 槽位）的 `getDirty()` / `setDirty()` —— 宣称「no unsaved edits」但那是 KV 编辑器自己的状态机，`src/lib/__tests__/kvSlotState.test.ts` 覆盖。

`grep -rn "data-dt-dirty\|data-dt-selected\|data-dt-editing\|data-dt-cell-type" src/ e2e/` → **零命中**：00 契约 4.4 定义的四个属性今天一个都不存在。已有的只有 `data-dt-row` / `data-dt-col` / `data-col-header`。

### 2.7 真源结论（本册最重要的判断）

**结论：`pendingChanges` 是唯一真源；`editBuffer` 是**有损**的派生缓存，本册**废弃并删除**它（连同死 prop）。**

理由（每条都有上面的代码事实支撑）：

1. **`editBuffer` 表达不了删除状态**。`rebuildEditBuffer` 对 `deleteMarked` 行直接 `return`，因此「整行标记删除」在 buffer 里根本不存在；而删除是高优先级功能（PRD DB-06 明确要求删除线 + 恢复）。
2. **`editBuffer` 只覆盖当前页**。它遍历 `ts.rows`，所以离屏的暂存改动不在其中；而 `pendingChanges` 是跨页的、且**离屏改动依然可以被提交**（提交用 `rowIdentity`，与行号无关）。若以 buffer 为渲染源或计数源，必然出现「计数比实际少」。
3. **`editBuffer` 的键是位置键**。`editKey(rowIndex, columnName)` 里含行号；翻页 / 排序 / 筛选后同一 `rowIndex` 指向别的物理行。契约 4.3 明令禁止用行号参与写路径定位，脏标记虽然不是写路径，但**「画在哪个格子上」同样必须由行身份决定**。
4. **它是缓存，不是状态**。它有 7 次赋值、0 次读取，且必须在每个改动点上手动重算；`pendingChanges` 是状态，谁改谁就是真源。两者同时存在 = 双源漂移的教科书场景（漂移只在特定时序复现，最难查）。
5. **删除它零行为变化**。渲染侧从未读过它（`DataTableProps.editBuffer` 是死 prop，`VirtualRow` 也没有相关 prop），唯一读取者是它自己的测试。

**避免双源漂移的机制（比「删掉旧代码」更重要）**：新的脏状态**不进 store**。它是由 `pendingChanges` + `rows` + `rowIdentityAnchors` 纯函数算出的 `DirtyIndex`，只作为组件的 `useMemo` 返回值存在，任何写路径都不读它。这样「第二个真源」在结构上不可能存在——不是靠纪律，是靠类型与数据流。

**迁移路径（四步，可独立回滚）**：

1. 删 `DataTableProps.editBuffer` 与它的 `CellEdit` 类型导入（死 prop，零行为变化）。
2. 删 `TableState.editBuffer` 字段、`emptyTableState` 的初始化、`editKey`。
3. 删 `rebuildEditBuffer` 与 `tableDataStore` 的 6 处写点、`pendingChanges.ts` 的相关 import。
4. 删 `pendingChanges.test.ts` 的 `rebuildEditBuffer skips delete-marked rows` 用例（它验证的能力由本册的新用例覆盖）。

---

## 3. 数据结构与接口设计

新增一个纯函数模块 `src/stores/tableData/dirtyIndex.ts`。类型全部是**视图面向的只读快照**，不含任何 store 写入入口。

### 3.1 复合键构造函数

```ts
/**
 * 单元格级脏标记的复合键：`行身份键 + NUL + 列名`。
 *
 * 为什么分隔符必须是 NUL：
 * - `rowIdentityKey()` 的产物是 JSON 形态（如 `["id":1]`），本身含 `"` `:` `[` `]` `,`；
 * - 列名同样可能含这些字符（例如 `user:name`）；
 * - 因此 `:` 或 `|` 都不是单射，两个不同的 (行, 列) 会拼出同一个键；
 * - `stableSerialize` / `JSON.stringify` 会把控制字符转义成 `\u0000` 这样的**两字符序列**，
 *   产物里不存在裸 NUL，而 SQL 标识符也不允许含裸 NUL → NUL 在两侧都不可出现。
 */
export function dirtyCellKey(identityKey: string, columnName: string): string {
  return `${identityKey}\u0000${columnName}`;
}
```

**禁止混用**：`connectionState.ts` 的 `editKey(rowIndex, columnName)` 是**位置键**，随本册一并删除；`rowIdentityKey(identity)` 是**行身份键**，是 `pendingChanges` 的键；`dirtyCellKey` 是**单元格键**，只用于集合查询与测试断言。三者语义不同，不得互相替代。**不得只用行号**：翻页 / 排序 / 筛选后 `rowIndex` 的含义整体平移（契约 4.3）。

### 3.2 完整 TS 定义

```ts
import {
  buildRowIdentity,
  duplicateRowIdentityKeys,
  rowIdentityKey,
  valuesEqual,
  type PendingRowChange,
  type Value,
} from '../../lib/tableChanges';
import type { ColumnSchema } from '../../types';
import type { TableState } from './types';

/**
 * 行级脏状态。一个行身份在同一时刻只处于其中一态。
 * - `dirty`    ：该行有未提交的列改动；
 * - `deleted`  ：该行被标记删除（`changedColumns` 仍可能非空，见 `cells`）；
 * - `inserted` ：**本册不产生**。DB-04（INSERT）接入后在草稿行上产生，届时只需在此 union 与
 *                `VirtualRow` 渲染分支各加一处，选择器其余部分不变。
 */
export type DirtyRowState = 'dirty' | 'deleted' | 'inserted';

/** 一格未提交改动的只读快照。缺省：不构造。 */
export interface DirtyCell {
  /** 列名（不是列下标：列下标随 `visibleColumns` 失去稳定性）。 */
  readonly columnName: string;
  /** 暂存前的原值快照（来自 `PendingRowChange.originalValues`）。缺省语义：`null` 表示原值就是 SQL NULL。 */
  readonly original: Value | null;
  /** 暂存后的当前值（来自 `PendingRowChange.currentValues`）。缺省语义：`null` 表示将写入 SQL NULL。 */
  readonly next: Value | null;
}

/** 一行（按行身份归位后）的脏状态。 */
export interface DirtyRowEntry {
  /** 稳定行身份键 = `rowIdentityKey(change.rowIdentity)`，与 `pendingChanges` 的键同一空间。 */
  readonly identityKey: string;
  readonly state: DirtyRowState;
  /** 该行真正有改动的列名。`deleted` 行恒为空数组（改动让位于行级删除态）。 */
  readonly changedColumns: readonly string[];
  /** 与 `changedColumns` 同序等长的值快照，供 tooltip 与「恢复此行」使用。 */
  readonly cells: readonly DirtyCell[];
}

export interface DirtyCounts {
  /** 有列改动且未被标记删除的行数。 */
  readonly updateRows: number;
  /** 被标记删除的行数。 */
  readonly deleteRows: number;
  /** `updateRows + deleteRows`。 */
  readonly totalRows: number;
  /** 所有非删除行的 `changedColumns` 长度之和（跨页全量）。 */
  readonly totalCells: number;
  /** 其中落在当前页、且能被当前页行解析命中的格数。 */
  readonly cellsOnPage: number;
  /** `totalCells - cellsOnPage`：在别的页/被筛选挡住的格数。 */
  readonly cellsOffPage: number;
}

export interface DirtyIndex {
  /** 当前页 `rowIndex` → 该行脏状态。只含当前页能解析出**唯一**行身份的行。 */
  readonly byRowIndex: ReadonlyMap<number, DirtyRowEntry>;
  /** 当前页之外的未提交改动身份键（仍可提交，绝不丢弃）。 */
  readonly offPageKeys: readonly string[];
  readonly counts: DirtyCounts;
}
```

### 3.3 选择器

```ts
/** 空索引单例：`dirtyByRow` 缺省时用它，保证引用稳定。 */
export const EMPTY_DIRTY_INDEX: DirtyIndex;

/**
 * 由 TableState 派生脏索引。纯函数，不写 store。
 *
 * @param ts        表格态（只需要 `rows` / `columns` / `pendingChanges` / `rowIdentityAnchors`）
 * @param previous  上一轮索引，用于**结构共享**；不传则全部新建
 */
export function buildDirtyIndex(ts: TableState, previous?: DirtyIndex | null): DirtyIndex;

/**
 * 面板级计数。**只依赖 `pendingChanges`**，因此在当前页行被清空、或数据还没加载时依然正确。
 */
export function selectDirtyCounts(
  pendingChanges: ReadonlyMap<string, PendingRowChange>,
): DirtyCounts;

/** 单行查询的便捷入口（TableView 的右键/双击守卫用）。 */
export function selectDirtyRow(
  index: DirtyIndex | null | undefined,
  rowIndex: number,
): DirtyRowEntry | undefined;
```

`buildDirtyIndex` 的算法（六步，每一步都是刻意选择）：

1. **一次遍历当前页建身份索引**：对 `ts.rows` 的每个 `rowIndex`，用 `buildRowIdentity(row, pkColumns)` → `rowIdentityKey`，得到 `Map<identityKey, rowIndex>`；`buildRowIdentity` 返回 `null`（主键含 NULL 或不稳定值）的行直接跳过——它们本来就不可能被写，也就不该被标脏。`pkColumns` 取自 `ts.columns.filter((c) => c.isPrimaryKey)`，与 `stageCellChange` / `findPendingForRow` 的现有取法一致（**不**改用后端的 `effective_primary_keys`：它是 Rust 侧方法，前端拿不到；前端内部一致即可）。
2. **重复身份键排除**：复用既有的 `duplicateRowIdentityKeys(rows, pkColumns)`，命中重复身份的行**一律不标脏**——这正是 `findPendingForRow` 里 `rowIdentityIsUnique` 的守则。理由：把改动画到两行中的任意一行都是错的，宁可不画（并且 `commitFetchedPage` 已经把 `AMBIGUOUS_ROW_IDENTITY_ERROR` 写进 `error`，界面会解释）。
3. **遍历 `pendingChanges` 逐个归类**：`deleteMarked` → 只计入删除行，`changedColumns` / `cells` 记为空数组（**删除行的格子不标脏**，避免删除线 + 脏标记双重表达同一件事）；否则逐列过滤 `valuesEqual(change.originalValues[col], change.currentValues[col]) === false` 才保留。**这层过滤是双保险**：即使未来某个新写路径漏了「改回原值就删键」，渲染层也不会误标（`stageCellChange` 今天已经做了这件事，见 2.1）。
4. **页内命中 → `byRowIndex`；页外 → `offPageKeys`**：命中判定用第 1 步的 `Map`，**不再**逐行调 `findPendingForRow`（那是 O(页行数²)，`pageSize` 调到 1000 时是 10⁶ 级；见第 6 节性能说明）。`rowIdentityAnchors` 作为**补充**来源：当 `pendingChanges` 的键与当前行身份算出的键不一致（主键列被改过、锚点仍指向原始身份）时，用 `rowIdentityAnchors.get(rowIndex)` 回查一次 `pendingChanges`，命中则用锚点键——这与 `findPendingForRow` 的优先级（先锚点、后直查）**逐位一致**，`rowIdentityAnchors` 的陈旧尾巴（页行数变少后残留的高位下标）永远不会被读到，因为渲染只请求 `rowIndex < rows.length`。
5. **结构共享**：若 `previous` 中存在同一 `identityKey`、同 `state`、且 `changedColumns` 与 `cells` 内容逐项相等的条目，**直接复用旧对象引用**。这样「只改一行」不会让其它行的 `DirtyRowEntry` 引用变化，`VirtualRow` 的 `memo` 才能命中。内容比较用一次浅比较（列数极少，通常 1~3 列）。
6. **与 `findPendingForRow` 的等价契约（不是可选项）**：`buildDirtyIndex` 只是 `findPendingForRow` 的整页批量版本，语义必须逐位相同。因此每个 `rowIndex < rows.length` 都必须满足：

   ```
   buildDirtyIndex(ts).byRowIndex.get(i)?.identityKey
     === findPendingForRow(ts, i, ts.rows[i], pkColumns)?.key
   ```

   两个方向都要成立：批量版认为某行脏、单行版认为不脏（或反之）即为**缺陷**。这条等式由第 9.1 节的等价性用例逐行断言（含重复身份、主键被改、无主键、`rows.length > pendingChanges.size` 等组合）。**优化可以改性能，不可以改语义**；谁想再优化这个索引，先让这条测试继续绿。

`selectDirtyCounts` 与 `buildDirtyIndex` 的计数口径必须一致，实现上让前者只依赖 `pendingChanges` 并作为后者的子过程被调用，**只有一份计数实现**。

### 3.4 组件接口变更（修改既有 props）

```ts
// src/components/DataTable/DataTable.tsx
export interface DataTableProps {
  // …既有字段一律不动…

  /**
   * 当前页行级脏状态，来自 `buildDirtyIndex(ts).byRowIndex`。
   * 缺省 / `null` = 与今天完全一致：不渲染任何脏标记（满足 00 契约 R4）。
   */
  dirtyByRow?: ReadonlyMap<number, DirtyRowEntry> | null;
  /** 「恢复此行」：撤销一行的删除标记。缺省时不渲染该菜单项。 */
  onRestoreRow?: (rowIndex: number) => void;

  // 删除：editBuffer?: Map<string, CellEdit>;
}

// src/components/DataTable/VirtualBody.tsx
export interface VirtualBodyProps {
  // …既有字段一律不动…
  /** 当前页脏状态；缺省 = 不渲染脏标记。 */
  dirtyByRow?: ReadonlyMap<number, DirtyRowEntry> | null;
}

interface VirtualRowProps {
  // …既有字段一律不动…
  /** 该行自己的脏状态。undefined = clean。**只喂本行**，见第 6 节。 */
  dirtyRow?: DirtyRowEntry;
}
```

**`CellRenderer` 不改**（`CellRendererProps` 保持 5 个字段）。偏离 PRD DB-06 技术落点原文的理由与替代方案见第 11 节 D-2；核心是三条：① 标记要覆盖单元格整块（含 padding），属于单元格盒子而不是文本节点；② 既有 5 条断言依赖 `container.querySelector('span')` 与 `className`，改结构会连带改测试；③ 少一层 props 传递，`CellRenderer` 的 `memo` 契约不动。

### 3.5 为什么 `DirtyIndex` 不是第二真源

| 维度 | 废弃的 `editBuffer` | 新的 `DirtyIndex` |
| --- | --- | --- |
| 存在位置 | `TableState` 字段，`set()` 写入 | 组件 `useMemo` 返回值，**不进 store** |
| 写点数量 | 7 次赋值 | 0（只读） |
| 是否可表达删除 | 否（删除行被 `return` 掉） | 是（`state: 'deleted'`） |
| 是否跨页 | 否（只遍历当前页） | 是（`offPageKeys` + `totalCells`） |
| 键 | 位置键 `rowIndex:column` | 行身份键（+ NUL + 列名做集合查询） |
| 失效方式 | 每个写点手动重算，漏一处就漂移 | 由 `pendingChanges` 引用变化驱动，漏不掉 |

**判据**：任何「可能被写路径读取、或被 `set()` 写入」的派生结果都是第二真源；`DirtyIndex` 两条都不满足。

---

## 4. 交互与状态机

### 4.1 单元格：`clean ⇄ dirty`

| 阶段 | 内容 |
| --- | --- |
| **进入条件** | `stageCellChange` 成功后，该行的 `pendingChanges` 条目存在、该列在 `changedColumns` 中、且 `valuesEqual(original, next) === false`；该行在当前页且行身份唯一（非重复身份）。 |
| **状态内行为** | 单元格 `div` 得到 `data-dt-dirty="true"`；左侧 `border-l-2` + `border-l-dt-dirty-border`；右上角 4px 三角角标 `bg-dt-dirty-mark`；底色 `bg-dt-dirty-bg`（**与「已选中」底色叠加的取舍见第 11 节 D-8**）；原生 `title` 显示「原值 → 新值」；`aria-label` 用 `tableData.dirtyCellAria`。**单元格文本颜色不变**（`--dt-*` 仍是前景类型色，见 4.4）。 |
| **退出跃迁 ①** | 用户把值改回原值 → `stageCellChange` 走 `valuesEqual` 为真的分支删键 → 该行若无其它改动，整条 `pendingChanges` 键被删 → `dirtyByRow` 中不再出现 → 标记消失、计数减一。 |
| **退出跃迁 ②** | 提交成功**且签名未变** → `pendingChanges` 清空 → 所有标记消失（`DirtyIndex` 由空 Map 算出）。**分支**：提交成功但签名已变（在途期间又暂存了别的改动）→ 既有守卫使整批都不清，**包括刚提交成功的那批也仍标脏**（见 4.7 第 1b 行与第 11 节 D-10）。本册不修改该守卫，但必须让高亮如实反映它。 |
| **退出跃迁 ③** | 回滚 → `pendingChanges` 清空（同步）→ 标记立即消失；值在随后的 reload 响应到达时才回到原样。 |
| **退出跃迁 ④** | 该行被标记删除 → 该行升格为行级 `deleted` 态，其格子**不再**带 `data-dt-dirty`（`changedColumns` 对 `deleted` 行记为 `[]`）。这是刻意的：同一件事只用一种视觉表达。 |
| **退出跃迁 ⑤** | 翻页 / 排序 / 筛选导致该行离屏 → 格子在 DOM 中不存在，改动进入 `offPageKeys`，面板计数仍包含它。**改动绝不因离屏而丢弃。** |
| **退出跃迁 ⑥** | 行身份歧义（重复身份键 / `AMBIGUOUS_ROW_IDENTITY_ERROR`）→ 该行不标脏，错误横幅解释原因。 |

### 4.2 行：`clean → dirty → deleted ⇄ dirty`

| 阶段 | 内容 |
| --- | --- |
| **进入条件（dirty）** | 该行身份在 `pendingChanges` 中且 `deleteMarked === false`、`changedColumns` 非空。 |
| **进入条件（deleted）** | `stageRowDelete` 写入 `deleteMarked: true`。 |
| **状态内行为（dirty）** | 行号槽按钮内渲染一个小圆点（`tid('row-dirty-dot')`），颜色 `--dt-dirty-mark`；行背景沿用既有的选中 / 详情高亮，脏标记不覆盖它。 |
| **状态内行为（deleted）** | 整行内容 `line-through` + `opacity-60`；行号槽圆点切删除态（`tid('row-deleted-dot')`，色 `--dt-danger` 家族，不属于 `--dt-*` 类型色域）；右键菜单出现「恢复此行」；**双击不再进入编辑**（`handleCellDoubleClick` 先查 `selectDirtyRow`，命中 `deleted` 直接返回，避免编辑一个即将被删的行）。 |
| **退出跃迁（deleted → dirty）** | 右键「恢复此行」→ 新增的 `unstageRowDelete(panelId, rowIndex)`：`changedColumns.length === 0` 则从 `pendingChanges` 删键（回 `clean`），否则写 `deleteMarked: false`（回 `dirty`）。 |
| **退出跃迁（→ clean）** | 提交成功 / 回滚 / 改回原值（仅对无删除标记的行可达）。 |
| **行级汇总引用** | `state: 'inserted'` 的 `+` 徽标由 DB-04 接入后渲染，本册只保留 union 分支与 `data-dt-row-state` 取值。 |

### 4.3 面板：计数条

| 条件 | 行为 |
| --- | --- |
| `pendingChanges.size === 0` | 整条不渲染（现状保持，`e2e/specs/table-edit.ts` 依赖它消失）。 |
| 有改动 | 显示 `counts.totalRows`（`tableData.pendingChanges`，文案修正为按行）、`counts.updateRows` / `counts.deleteRows`（复用既有键）、以及**新增**的 `counts.totalCells`（`tableData.pendingCells`）。 |
| `counts.cellsOffPage > 0` | 追加一行 `tableData.pendingOffPage`：「另有 {count} 处改动不在当前页」。这是把「网格高亮只覆盖当前页」这件事**显式化**，否则用户会以为改动丢了。 |
| 三个 `data-testid` | `pending-changes-bar` 保留原名（E2E 依赖），条内新增 `pending-cells-count`（`tid`）。 |

**计数口径不变量**（写进测试）：

```
counts.totalRows  === pendingChanges.size 中 deleteMarked 为真与 changedColumns 非空的条目数
counts.totalCells === Σ_{非删除条目} (过滤 valuesEqual 后的 changedColumns.length)
counts.cellsOnPage === DOM 中 [data-dt-dirty="true"] 的数量（渲染完成后）
counts.cellsOnPage + counts.cellsOffPage === counts.totalCells
```

### 4.4 颜色与类型色的区分（必须说清）

- `--dt-*`（`null` / `bool` / `number` / `datetime` / `json` / `text` / `binary`）是**前景色**，只作用于 `color`，由 `cellValueTextClass` 消费。
- 新增的脏标记 token（`--dt-dirty-border` / `--dt-dirty-bg` / `--dt-dirty-mark` / `--dt-deleted-fg`，落点见第 6 节 `src/styles/themes.css` 行与 Step 7）**只作用于 `border-color` / `background-color` / `text-decoration`**，**永不作用于 `color`**。因此「脏」与「这一列是什么类型」是两个正交的视觉通道，不会互吃。
- **色相刻意选在类型色与状态色之外的品红紫族**：类型色占用了灰（null）、蓝（bool/json）、暖橙棕（number）、青（datetime）、红（binary）；状态色占用绿（success）、琥珀（warning）、红（danger）、青（accent）。若脏标记用琥珀，数字列（`--dt-number` 暖橙）会看起来像警告；若用蓝色，会与 bool/json 混淆。
- **形状是主通道，颜色是辅通道**：左侧 2px 边 + 右上角角标 + tooltip 三重表达，任一通道在色盲或低对比主题下失效时仍可辨识。
- **不用 `color-mix()`**：宿主运行在 macOS WKWebView 与 Windows WebView2，避免为了取 accent 的透明变体引入兼容性风险；亮 / 暗两套值直接写字面量。
- **第三方主题可见性**：主题包（如 `packages/wapps/community.slate-blue/themes/slate-blue/tokens.css`）只覆盖它声明的属性，未声明 `--dt-dirty-*` 时 `themes.css` 的 `:root` / `.dark` 基础值仍然生效 → 高亮不会消失。这一点是 E2E 用例 A7 的断言目标。

### 4.5 键盘 / 鼠标 / 焦点 / IME

- **本册不改任何键盘路径**：进入编辑仍是双击（`handleCellDoubleClick`）、Enter/blur 提交、Escape 取消，全部沿用现状。DB-02 负责键盘导航，届时脏标记只需保证「按行身份判定」，不参与焦点模型。
- **鼠标**：悬停 tooltip 用**原生 `title`**，不引入浮层、不做测距、不监听 `mouseenter`。理由：WKWebView / WebView2 都原生支持；避免与 DB-03 的右键菜单、DB-01 的区域拖拽抢事件；也避免任何几何坐标反查（违反项目纪律）。
- **焦点**：脏标记不改变 `tabIndex`、不参与焦点序；已删除的行仍在 DOM 中（不卸载），行号槽圆点不是可聚焦元素。
- **IME**：既有的 `EditableCell` 没有 `isComposing` 判断（组字期间按 Enter 会提交）——这是 DB-05 的范围，本册明确**不顺手改**，避免把「高亮」提交与「输入法」提交混在一个 PR 里。本册新增的 tooltip 不参与键盘事件，不引入新的 IME 风险。
- **无主键 / 只读**：`stageCellChange` / `stageRowDelete` 已经在无主键时报错并拒绝写入，因此**任何情况下都不会产生脏标记**；只读面板根本不传 `onCellEdit`，也就没有 `pendingChanges`。本册只需要保证「无标记 = 无计数条目」这条等价关系（测试里有对应用例）。

### 4.6 与选择模型（00 契约第 4 节）的正交关系

00 契约把选择模型冻结在 `src/stores/tableData/gridSelection.ts` + `src/hooks/useGridSelection.ts` 两处，并要求行级操作取值域用派生选择器 `rowsCoveredBySelection(state): ReadonlySet<number>`。本册与它的边界如下，**边界本身就是实现要求**：

| 维度 | 选择（DB-01 / DB-02 / DB-03 拥有） | 脏标记（本册拥有） |
| --- | --- | --- |
| DOM 属性 | `data-dt-selected` | `data-dt-dirty` |
| 状态来源 | `GridSelection`（`anchor` / `focus` / `extraRanges` / `mode`） | `pendingChanges`（+ 当前页行身份解析） |
| 行级取值域 | `rowsCoveredBySelection(state)`（派生，不落库） | 不涉及：行级删除态由 `deleteMarked` 决定 |
| 清理时机 | `Escape` / 点空白 / 翻页重取 | 提交 / 回滚 / 改回原值 / 恢复此行；**绝不因选择变化而清理** |

三条必须遵守的推论：

1. **两个属性可以同时为真**：一个格子既被选中又未提交改动时，`data-dt-selected="true"` 与 `data-dt-dirty="true"` 同时存在。它们挂在同一个单元格 `div` 上（`VirtualBody` 的单元格），因此**DB-01 与 DB-06 会改同一个元素的属性列表**——这是第 6 节的协调项，不是冲突：一个加属性、一个加属性，互不覆盖。
2. **脏判定不吃选择参数**：`buildDirtyIndex(ts, previous)` 的签名里没有选择状态；本册不订阅任何鼠标 / 键盘事件（那些是 `useGridSelection` 的职责），也不实现 `normalizeCellRange` 之类的区域运算（00 契约明令「不得各自再实现一遍区域运算」）。
3. **选择不因脏而变**：`stageCellChange` 今天不改 `selectedRows`（它只清 `editingCell`），本册保持这一行为；`commitFetchedPage` 清空 `selectedRows` 是既有行为，本册不改。反过来，`rowsCoveredBySelection` 也不读 `pendingChanges`——「删除区域覆盖的行」的作用域只由选择决定，删掉的行会变成删除态，这是两步而不是一步。

> 与 PRD DB-01 早期草稿的分歧提示：草稿曾写「`selectedRows` 一律由区域派生」，00 契约已**明确反转为禁止写回派生行集**。本册不涉及该实现，但第 9.3 节要加一条守护用例，断言「改脏 / 清脏不会改变 `selectedRows`」，避免本册的改动意外踩到 DB-01 的取值域纪律。

### 4.7 六种时序下的清理与保留（逐条，最容易写错的地方）

| # | 时序 | 触发符号（已读过） | 脏标记 | 计数 | 值 |
| --- | --- | --- | --- | --- | --- |
| 1 | **提交成功后**（签名未变） | `commitPendingChanges` → 签名相等 → 清空 `pendingChanges` / `previewPlan`，随后 `loadTableData` 重取 | **全部消失**（`DirtyIndex` 由空 Map 算出） | 计数条整条消失 | 来自数据库（已落库） |
| 1b | **提交成功但签名已变**（在途期间又暂存了别的改动） | 同一函数：签名不等 → **整个 `patchPanel` 跳过** | **一条都不清**：包括刚刚提交成功的那批也仍被标脏 | 计数不变 | reload 后 `overlayPendingRows` 把已提交的值再盖一遍（视觉相同，但仍显示为「未提交」） |
| 2 | **回滚后** | `rollbackPendingChanges`（清空后 `reloadPanel`） | **同步消失**（先于 reload 响应到达） | 计数条消失 | reload 响应到达后回到数据库值 |
| 3a | **刷新数据后**（刷新按钮 / 排序 / 翻页引起的 `reloadPanel`） | `reloadPanel` → `commitFetchedPage` | **保留**：重建 anchors + `overlayPendingRows` 后按身份重新归位（行号可能变） | 不变 | 暂存值仍显示（被 overlay 盖住） |
| 3b | **刷新数据后**（别处执行了写 SQL / 侧边栏刷新 / 单表刷新） | `invalidateCachedData` | 有暂存的 panel **整体跳过** → 标记不动；无暂存的 panel 清空 `rows`/`columns`，本来也无标记 | 不受影响（计数只依赖 `pendingChanges`） | 无暂存的 panel 下一次渲染触发重新取数 |
| 4 | **翻页后** | `setPage` / `setPageSize` → `patchPanelForReload` + `reloadPanel({ skipCount: true })` | 当前页按身份重新归位；离屏改动进入 `offPageKeys` | `cellsOffPage > 0` → 显示「另有 N 处改动不在当前页」 | 不在当前页的值不回显（未取数） |
| 5 | **筛选变化后** | `setFilters` / `applyFilters` / `clearFilters`（均重置 `page = 0` 并 reload） | 同翻页：留在筛选内的按身份归位，被筛掉的计入 `cellsOffPage` | 同翻页 | 同翻页 |
| 6 | **连接断开后** | 连接断开 → 该连接的 panel 被移除 → `ContentView` 的 `byPanel` 清理 effect 调 `tableDataStore.removePanel(panelId)` | **整块 slice 消失** → 标记与计数一起消失，**改动丢弃** | 消失 | 随 slice 一起消失 |

第 6 条与第 3b 条的区别必须记住：`invalidateCachedData` **不删 slice**、也不动有暂存的 panel（这是「刷新缓存」）；`removePanel` **删 slice**（这是「panel 生命周期结束」）。因此**断开连接不属于保留暂存的场景**：要保住改动，必须在断开前提交，见第 11 节 D-5 / D-10。

---

## 5. 实现步骤

每一步都写清「改哪个文件、加什么符号、为什么、这一步怎么自测」。顺序刻意让**删除旧机制**排在**新增渲染**之前：先消灭第二真源，再建第一真源的视图。

### Step 1 · 新增纯函数模块 `src/stores/tableData/dirtyIndex.ts`

- **加什么**：`dirtyCellKey`、`DirtyRowState`、`DirtyCell`、`DirtyRowEntry`、`DirtyCounts`、`DirtyIndex`、`EMPTY_DIRTY_INDEX`、`buildDirtyIndex`、`selectDirtyCounts`、`selectDirtyRow`（第 3 节的定义逐字落地）。
- **为什么**：派生逻辑必须可纯函数单测、不依赖 React，且必须住在 store 旁边（`src/stores/**` 在覆盖率门禁内，`vitest.config.ts` 的 `coverage.include` 覆盖它）；放组件里会被 800 行上限与渲染测试绑住。**它建立在既有真源之上**（第 2.2 节）：真源仍是 `pendingChanges`，行查找仍以 `findPendingForRow` 的语义为准，本模块只是它的整页批量版本 + 计数派生，**不新增状态、不新增选择逻辑**（区域运算属于 `src/stores/tableData/gridSelection.ts`，本册不碰）。
- **必须先落地的两个断言**：第 3.3 节第 6 步的 `buildDirtyIndex === findPendingForRow` 逐行等价性，以及第 9.1 节的「不吃选择状态」用例。这两个断言是本步的**验收条件**，不是附加项。
- **自测**：`npx vitest run src/stores/tableData/__tests__/dirtyIndex.test.ts`（Step 9 写用例）；此步可先写模块 + 等价性断言跑通。

### Step 2 · 删除 `editBuffer` 全链路

- **改哪些文件 / 删什么符号**：
  - `src/stores/tableData/types.ts`：删 `CellEdit` 接口与 `TableState.editBuffer` 字段。
  - `src/stores/tableData/connectionState.ts`：删 `editKey`，删 `emptyTableState` 里的 `editBuffer: new Map()`。
  - `src/stores/tableData/pendingChanges.ts`：删 `rebuildEditBuffer`、删 `editKey` 与 `CellEdit` 的 import。
  - `src/stores/tableDataStore.ts`：删 `rebuildEditBuffer` import 与 **6 处** `editBuffer:` 写点（`commitFetchedPage` / `invalidateCachedData` / `stageCellChange` / `stageRowDelete` / `rollbackPendingChanges` / `commitPendingChanges`）。
  - `src/components/DataTable/DataTable.tsx`：删 `DataTableProps.editBuffer` 与 `import type { CellEdit }`。
  - `src/stores/tableData/__tests__/pendingChanges.test.ts`：删 `rebuildEditBuffer skips delete-marked rows`。
- **为什么**：真源只能一个（2.7 节）。**死 prop 必须一起删**，否则下一个人看到 `editBuffer` 会以为只是「没接上」，再写一次 `rebuildEditBuffer` —— PRD 的原始方案正是这么写的，这就是风险来源。
- **自测**：`pnpm typecheck` 退出码 0；`grep -rn "editBuffer\|rebuildEditBuffer\|CellEdit" src/` 空输出；`npx vitest run src/stores/__tests__/tableDataStore.test.ts src/stores/tableData/__tests__/pendingChanges.test.ts` 全绿。

### Step 3 · `tableDataStore` 新增 `unstageRowDelete`

- **改哪个文件 / 加什么符号**：`src/stores/tableDataStore.ts` 的 `TableDataStore` 接口与实现各加 `unstageRowDelete: (panelId: string, rowIndex: number) => void`；同时把 `stageCellChange` 里「构造下一版 `PendingRowChange`」的纯计算外提到 `src/stores/tableData/stageEdit.ts`（新增 `computeStageResult`），供 `stageCellChange` 与 `unstageRowDelete` 共用。
- **为什么**：(1) PRD DB-06 明确要求「右键可恢复此行」，而今天**没有任何撤销删除标记的入口**；(2) `tableDataStore.ts` 已经 722 行，直接内联两个新 action 会顶到 800 上限，所以把纯计算抽出去（既降行数，也让「改回原值就删键」这段关键逻辑第一次变得可单测）。
- **实现要点**：解析 `rowIndex` 的行身份（复用 `findPendingForRow`，与既有纪律一致，**不自己用行号拼**）→ 取条目 → `changedColumns.length === 0 ? pendingChanges.delete(key) : 写 deleteMarked: false` → 清 `previewPlan`、`pendingStatus: 'idle'`、`error: null`。
- **自测**：单测断言「既改过列又标记删除的行，恢复后 `deleteMarked === false` 且 `changedColumns` 保持原样」与「只标记删除的行，恢复后该键消失」。

### Step 4 · `TableView.tsx` 建索引并接线

- **改哪个文件 / 加什么符号**：`src/windows/connection/TableView.tsx`
  - `const dirtyIndex = useMemo(() => buildDirtyIndex(ts ?? emptyTableState(), prevRef.current), [pendingChanges, rowIdentityAnchors, rows, columns])`，并用 `useRef` 保存上一轮结果供结构共享；
  - `const dirtyCounts = useMemo(() => selectDirtyCounts(pendingChanges), [pendingChanges])`；
  - `DataTable` 传 `dirtyByRow={dirtyIndex.byRowIndex}` 与 `onRestoreRow={actions.unstageRowDelete}`；
  - `handleCellDoubleClick` 增加守卫：`selectDirtyRow(dirtyIndex, row)?.state === 'deleted'` 时 return。
- **为什么**：索引必须在**唯一一处**计算，网格与计数条共用同一份 `DirtyIndex` —— 这是从结构上消灭「工具条计数与网格高亮不一致」缺陷形态的手段，而不是靠两边各算一遍再对答案。
- **自测**：组件测试断言 `[data-dt-dirty="true"]` 数量 === `t('tableData.pendingCells', { count: counts.totalCells })` 渲染出的数字。

### Step 5 · `VirtualBody.tsx` 渲染三层标记

- **改哪个文件 / 加什么符号**：`src/components/DataTable/VirtualBody.tsx` 的 `VirtualBody` / `VirtualRow` 新增 `dirtyByRow` / `dirtyRow` prop；单元格 `div` 增加
  `{...((dirtyCellNames.has(col.name)) ? { 'data-dt-dirty': 'true' } : {})}`（**干净时不输出属性**，符合契约 4.4 的「`"true"` / 不存在」）；
  行容器增加行级态（属性名见第 11 节 D-3：`data-dt-row-state="dirty" | "deleted"`）；行号槽按钮内条件渲染 `tid('row-dirty-dot')` / `tid('row-deleted-dot')` 的小圆点；删除态行加 `line-through opacity-60`。
- **为什么**：标记必须落在单元格盒子（含 `px-2` padding）上，且**不触碰 `CellRenderer`** —— 后者是 5 条既有断言的锚点（`container.querySelector('span')` + `className`）。tooltip 用 wrapper 的 `title`，`CellRenderer` 一行不改。
- **细节**：`dirtyCellNames` 用 `useMemo` 从 `dirtyRow?.changedColumns` 构造 `Set`，但**不要**在 JSX 里现造集合（每次渲染新引用）；`Set` 的构造依赖 `dirtyRow`（引用稳定，见结构共享）。
- **自测**：`VirtualBody.dirty.test.tsx` 断言脏格数、tooltip 文本、删除行不带 `data-dt-dirty`、缺省不输出属性。

### Step 6 · 复用分册 01 Step 10 抽取的 `TablePendingChangesBar` 并加脏计数

- **不加新组件**：**复用分册 01 Step 10 已抽取的 `src/windows/connection/TablePendingChangesBar.tsx`**（该步把 `table-tx-controls` + `pending-changes-bar` 两块搬进去，逐字保留既有 `data-testid`，约 150 行）。**一个文件，不新建职责重叠的第二个组件**：分册 01 建骨架（事务控制 + 计数条 + 三个按钮），本册**只向它新增脏计数展示**。
- **我向该组件新增什么**：新增可选 props（`counts: DirtyCounts`，以及把既有的 `pendingUpdateCount` / `pendingDeleteCount` 收敛进同一入口；缺省时行为与今天一致），并在计数条内新增 `tid('pending-cells-count')` 子元素与 `tableData.pendingOffPage` 提示行。`pendingBusy` / `onPreview` / `onCommit` / `onRollback` 由分册 01 的版本提供，本册不改它们的语义。
- **为什么**：`TableView.tsx` 已 770 行，逼近 800 上限，抽取是让 DB-02 / DB-03 / DB-06 后续还能进这个文件的前提；三册都要这一条，各建一份必然分叉（最小的是「谁先建谁被覆盖」）。本册**服从统一命名与单一组件**：文件名以分册 01 的为准，本册只做增量。
- **自测**：`npx vitest run` 后 `e2e/specs/table-edit.ts` 依赖的 `pending-changes-bar` / `pending-preview` / `pending-commit` / `pending-rollback` 仍全部存在，且新增的 `pending-cells-count` 数字与 `counts.totalCells` 相等（组件测试断言 + 第 9.4 节 E2E）。

### Step 7 · 主题 token 与 i18n

- **改哪些文件**：`src/styles/themes.css`（`:root` 与 `.dark` 各加 4 个 token）、`tailwind.config.ts`（`colors.dt` 加 4 个映射）、`src/locales/en/query.ts`（加第 8 节的 key）。
- **为什么**：零硬编码（不写字面色值、不按主题名分支）；第三方主题未声明新 token 时基础值兜底；文案集中在 `en/query.ts` 领域包。
- **自测**：`npx vitest run src/locales/locales.test.ts` 全绿（它会扫描新 `t('…')` 字面量键是否都在 `en`）；E2E 三主题可见性用例。

### Step 8 · 右键菜单「恢复此行」

- **改哪些文件**：`src/lib/dataTableContextMenu.ts` 的 `DataTableContextMenuLabels` 加 `restoreRow`、`DataTableContextMenuHandlers` 加 `onRestoreRow`、`BuildDataTableContextMenuArgs` 加 `canRestoreRow`，并在 `buildDataTableContextMenuItems` 里按 `canRestoreRow` 生成一条 `item('restore-row', labels.restoreRow, handlers.onRestoreRow)`；`DataTable.tsx` 的 `handleContextMenu` 计算 `canRestoreRow = !!hit && dirtyByRow?.get(hit.rowIndex)?.state === 'deleted' && !!onRestoreRow` 并接线。
- **为什么**：PRD DB-06 要求删除行可恢复；能力位缺省为假，等于今天的行为（R4）。
- **自测**：组件测试断言删除行右键菜单含 `restore-row`、普通行不含。

### Step 9 · 测试（第 9 节的全部用例）

- **加什么**：`src/stores/tableData/__tests__/dirtyIndex.test.ts`、`src/components/DataTable/__tests__/VirtualBody.dirty.test.tsx`、`src/stores/__tests__/tableDataStore.dirty.test.ts`、`e2e/specs/table-dirty-cells.ts`。
- **为什么**：`src/stores/**` 与 `src/components/DataTable/**` 在 `vitest.config.ts` 的覆盖率门禁内（lines 80 / branches 75），新模块必须自带测试；两条「最容易错」的用例（改回原值、翻页归属）在旅程测试里。
- **自测**：A1~A8 全部命令（长输出重定向到系统临时目录后取结论行，遵守 `AGENTS.md`）。

---

## 6. 文件级改动清单

| 文件 | 新增 / 修改 | 职责 | 预估行数 | 触及 800 行上限 |
| --- | --- | --- | --- | --- |
| `src/stores/tableData/dirtyIndex.ts` | 新增 | 复合键、脏索引与计数选择器（纯函数） | ~170 | 否 |
| `src/stores/tableData/stageEdit.ts` | 新增 | 从 `stageCellChange` 抽出的纯计算（下一版 `PendingRowChange` + 改回原值删键） | ~110 | 否 |
| `src/stores/tableData/types.ts` | 修改 | 删 `CellEdit` 与 `editBuffer` 字段 | 55 → 43 | 否 |
| `src/stores/tableData/connectionState.ts` | 修改 | 删 `editKey` 与 `emptyTableState` 的初始化 | 87 → 81 | 否 |
| `src/stores/tableData/pendingChanges.ts` | 修改 | 删 `rebuildEditBuffer` | 127 → 105 | 否 |
| `src/stores/tableDataStore.ts` | 修改 | 删 6 处 `editBuffer` 写点；`stageCellChange` 改调用抽出的纯计算；新增 `unstageRowDelete` | 722 → 约 660 | 否（**但必须靠 Step 3 的外提才留出预算**） |
| `src/components/DataTable/DataTable.tsx` | 修改 | 删死 prop；新增 `dirtyByRow` / `onRestoreRow` 透传与 `canRestoreRow` | 628 → 约 640 | 否（仍紧；DB-02 / DB-03 也会加代码，需协调） |
| `src/components/DataTable/VirtualBody.tsx` | 修改 | 单元格脏标记、行删除态、行号槽圆点 | 207 → 约 255 | 否 |
| `src/components/DataTable/CellRenderer.tsx` | **不改** | 标记挂在单元格 wrapper；避免改 DOM 结构与 5 条既有断言 | 72 → 72 | 否 |
| `src/stores/tableData/gridSelection.ts` | **不改（DB-01 拥有）** | 选择模型纯逻辑与 `rowsCoveredBySelection`；本册只保证脏判定不吃选择参数 | 由 DB-01 定 | 由 DB-01 定 |
| `src/hooks/useGridSelection.ts` | **不改（DB-01 拥有）** | 选择状态的 React 接线；本册不注册鼠标 / 键盘事件 | 由 DB-01 定 | 由 DB-01 定 |
| `src/lib/dataTableContextMenu.ts` | 修改 | `restoreRow` 标签 / handler / 能力位 | 398 → 约 412 | 否 |
| `src/windows/connection/TablePendingChangesBar.tsx` | **复用（分册 01 Step 10 创建）** | 分册 01 建骨架（事务控制 + 计数条 + 三按钮，~150 行）；本册只**新增 props 与子元素**（脏计数 + `tid('pending-cells-count')` + off-page 提示），约 +20 行。**最终一个文件，不另建组件**（命名与归属由 01 裁定统一） | 01 的 ~150 + 本册 ~20 | 否 |
| `src/windows/connection/TableView.tsx` | 修改 | 建 `dirtyIndex` / `dirtyCounts`；接线；把内联计数块改为调用 `TablePendingChangesBar` 并传 `counts` | 770 → 约 730 | 否（抽取后反而更安全；不抽必超） |
| `src/styles/themes.css` | 修改 | `:root` / `.dark` 各加 4 个脏标记 token | 161 → 约 171 | 否 |
| `tailwind.config.ts` | 修改 | `colors.dt` 映射新 token | 72 → 约 77 | 否 |
| `src/locales/en/query.ts` | 修改 | 新增 / 修正第 8 节的 key | 424 → 约 432 | 否 |
| `src/stores/tableData/__tests__/dirtyIndex.test.ts` | 新增 | 选择器单测（含与 `findPendingForRow` 的等价性用例） | ~220 | 否 |
| `src/components/DataTable/__tests__/VirtualBody.dirty.test.tsx` | 新增 | 渲染测试 | ~130 | 否 |
| `src/stores/__tests__/tableDataStore.dirty.test.ts` | 新增 | 连续旅程测试 | ~250 | 否 |
| `e2e/specs/table-dirty-cells.ts` | 新增 | E2E | ~120 | 否 |

**跨册协调项**（会影响本册的文件，需在合并顺序上对齐）：

1. `DataTable.tsx` 与 `TableView.tsx` 同时被 DB-02（键盘）、DB-03（剪贴板）、DB-05（编辑器）改；`src/locales/en/query.ts` 被所有分册改。建议按 00 契约第 9 节的提交顺序，把 DB-06 排在 DB-01 之后、DB-03 之前。
2. **同一个单元格 `div` 会被 DB-01 与 DB-06 同时改**：DB-01 加 `data-dt-selected`、DB-06 加 `data-dt-dirty`，DB-05 还会加 `data-dt-editing` / `data-dt-cell-type`。四者都是**独立属性**，实现上必须写成「各自按需展开属性对象」而不是「互相覆盖的一次 `{...}` 重写」；建议 `VirtualBody` 里先把单元格属性收集在一个对象里再展开一次，避免后合并的分册把前一个的属性覆盖掉。这类覆盖在 code review 里几乎看不出来，只有 E2E 会以「属性莫名消失」的形式暴露。
3. `VirtualBody.tsx` 的 `VirtualRowProps` 会被 DB-01（选择态）、DB-05（编辑器类型）、DB-06（脏态）各加字段。本册要求：**新字段一律可选且缺省等于今天**，谁加的字段谁负责在缺省时零行为变化。
4. **计数条组件已裁定统一**：`src/windows/connection/TablePendingChangesBar.tsx`，由分册 01 Step 10 首次创建（骨架含事务控制 + 计数条 + `pending-preview` / `pending-commit` / `pending-rollback`），分册 03 与 06 只做增量 props / 子元素。**不得**另建 `PendingChangesBar.tsx` / `TableViewPendingBar.tsx` 等重叠组件；三册各自的计数展示一律进这一个文件（本册新增的是 `counts` 与 `tid('pending-cells-count')`）。

**性能说明（为什么不能把 `Map` 直接塞进依赖数组 / 为什么不会整表重渲染）**

1. **依赖数组只放 store 里的引用**。`pendingChanges` 与 `rowIdentityAnchors` 在每次 `patchPanel` 时都被 `new Map(...)` 替换，因此引用相等**就是**一次正确的变更信号。反过来：把 `[...pendingChanges.values()]`、`new Set(pendingChanges.keys())`、`{ ...something }` 这类**现造值**放进 deps 或 props，会在每次渲染产生新引用 → `useMemo` 每次重算、`memo` 永不命中 → 大表每次击键整表重渲染。这是本册最容易被写错的一处。
2. **不要用 `pendingChanges.size` 当依赖或判据**。同一行连续改两格时 `size` 恒为 1，用它做依赖会让高亮停在旧值上（这正是 PRD 提到的「计数只算第一个」缺陷形态的同源错误）。
3. **`VirtualRow` 只接收本行条目**。把整个 `ReadonlyMap` 传进每一行虽然引用稳定，但任何一行的改动都会让「行 → 该行条目」的取值结果全体变化，`memo` 比较的 props 就变了；因此按 `dirtyByRow.get(vRow.index)` 取出单行条目再传，配合 Step 1 的**结构共享**，未被改动的行 props 引用不变 → 只有脏行重渲染。
4. **索引构建是 O(页行数)**。先用一次遍历建 `Map<identityKey, rowIndex>`，再对 `pendingChanges` 做 O(改动数) 的归类；**不要**对每行调 `findPendingForRow`（它内含 `rowIdentityIsUnique` 的全页扫描，`pageSize = 1000` 时是 10⁶ 级）。`duplicateRowIdentityKeys` 也只调用一次。
5. **虚拟滚动本身不受影响**：`useVirtualTable` 的 `count` 只依赖 `rows.length`；脏标记只改已渲染行的属性，不改变行高、不触发滚动重算。

---

## 7. 边界与异常清单

| # | 情景 | 期望行为 |
| --- | --- | --- |
| 1 | **改了又改回原值** | `stageCellChange` 的 `valuesEqual(original, current)` 为真 → 从 `originalValues` / `currentValues` 删键、从 `changedColumns` 移除；若 `changedColumns` 空且无删除标记，整条 `pendingChanges` 键被删 → 单元格 `data-dt-dirty` 消失、`totalCells` 与 `totalRows` 各减一；若该行仍被标记删除，则退化为纯 `deleted` 行（行级删除线保留）。**必须有一条旅程用例**。 |
| 2 | **改了主键列** | 允许（除非与其它行身份冲突）。脏标记画在主键列那一格；`rowIdentityAnchors` 仍绑定**原始**身份键，`effectivePendingIdentity` 在需要时用 `currentValues` 覆盖主键列，因此刷新后即使数据库返回新主键值，标记仍落在同一视觉行上。冲突时报 `AMBIGUOUS_ROW_IDENTITY_ERROR`，**不产生任何标记**（保守优先）。 |
| 3 | **只改了空格** | `'Alice'` → `'Alice '` 是真实改动（`valuesEqual` 用 `JSON.stringify`，长度不同的字符串不等）→ 标脏。tooltip 必须能看出差异，因此不得对值做 `trim()`（**任何 `trim` 都是错**）。空字符串与 NULL 不同：`''` vs `null` 判为不等（`'""'` ≠ `'null'`）→ 标脏。注意内联编辑器把 `''` 提交成 `null`（`EditableCell.coerceAndCommit`），所以 `next === ''` 只可能来自粘贴路径，tooltip 用 `tableData.dirtyValueEmpty` 渲染 `(empty string)` 以区别于 `NULL`。 |
| 4 | **把值改成 `null`** | 原值非 `null` 时标脏（与其它值同等对待，**不给 NULL 单独的标记样式**：NULL 是一个值，不是一种状态）。tooltip 用 `tableData.dirtyValueNull` 渲染 `NULL`。原值本就是 `null` 时不算改动（`valuesEqual(null, null)` 真）。 |
| 5 | **批量粘贴产生的改动** | 每个格子都应产生自己的标记与计数（`totalCells` = 粘贴格数）；同一行多格只算一行（`totalRows` +1）。今天的 `applyColumnToRows` 是逐格 `stageCellChange`，每格一次 `set()` → 1000 格 = 1000 次写与 1000 次索引重建。**正确但慢**：本册要求语义正确，性能批处理见第 11 节 D-6（`stageCellChanges` 批量 action）。 |
| 6 | **提交失败后标记是否保留** | **保留**。`commitPendingChanges` 失败时只写 `error`，`pendingChanges` 原样（既有测试 `commit failure preserves pending changes`）。索引由未变的 Map 重算 → 标记逐格不变，计数不变；错误横幅用原始消息（`gridErrors.ts` 尚不存在，见第 8 节）。 |
| 7 | **翻页后回来** | 改动在 `pendingChanges` 中按身份保存，与页码无关。回到原页时，`buildDirtyIndex` 通过「页内身份索引」或 `rowIdentityAnchors` 命中 → 标记重新出现在**同一个物理行**上（同名主键），而**不是**同一个行号上。若该行已不在结果集（被他人删除），标记从网格消失但 `offPageKeys` / `totalCells` 仍计入，`pendingOffPage` 提示可见。**必须有一条旅程用例。** |
| 8 | **筛选变化后** | 与翻页同构：`setFilters` / `clearFilters` / `applyFilters` 会重置 `page` 并 reload，**不会**清 `pendingChanges`（改动绝不因筛选而静默丢弃）。落在筛选之外的改动 → 从网格消失，计数里体现为 `cellsOffPage`。若筛选后参与筛选的列被改过，提示仍按现状由 `tableData.errorPendingHint` 承担。 |
| 9 | **数据被他人改动导致刷新** | 取回的原始值变了，但暂存快照不变：`commitFetchedPage` 先 `overlayPendingRows` 把 `currentValues` 盖回去，所以单元格仍显示暂存值、仍标脏，`originalValues` 仍是用户编辑时的快照。提交时 `WHERE` 用的是 `rowIdentity`（主键快照）→ 若他人改了**非主键**列，UPDATE 命中 1 行、后写覆盖（last-writer-wins），标记正常清除；若他人改了**主键**或删了该行，影响行数为 0 → 整批因 `grid.commit.affectedMismatch` 失败（00 契约 1.4 的纪律），**标记必须全部保留**，用户可回滚或重新编辑。 |
| 10 | **同值不同类型（`1` 与 `'1'`）** | 判为改动并标脏：`JSON.stringify(1)` = `'1'`、`JSON.stringify('1')` = `'"1"'`，不等。**禁止**为了「避免误标」改用 `==` 或 `String()` 比较：那会把真实的类型变化吞掉，写进库的值与用户看到的类型不一致。 |
| 11 | **行身份歧义 / 无主键** | 无主键：`stageCellChange` / `stageRowDelete` 已经拒绝写入并写 `error`（`tableData.noPrimaryKey`）→ `pendingChanges` 为空 → 零标记、计数条不渲染。重复身份键：`commitFetchedPage` 写 `AMBIGUOUS_ROW_IDENTITY_ERROR` → 歧义行不标脏。 |
| 12 | **只读连接 / 只读驱动** | `TableView` 不传 `onCellEdit` / `onDeleteRows`，`editingCell` 强制为 `null` → 不可能产生暂存改动 → 零标记。即使存在历史暂存条目（先可写后切只读），标记与计数条仍如实显示（它们是数据事实，不是可操作性提示），但提交按钮的可用性由既有 `driverReadOnly` / `pendingBusy` 控制，本册不改。 |
| 13 | **空表 / 单行表** | 空表：`byRowIndex` 为空 Map，计数来自 `pendingChanges`（通常为 0）→ 无标记、无计数条。单行表：一切正常；注意 `overflow-hidden` 的单元格里 tooltip 依赖原生 `title`，不依赖溢出可见性。 |
| 14 | **超长值 / 多字节字符** | 标记不参与文本截断（`CellRenderer` 仍按 120 字符截断显示）；`title` 里的原值/新值可能很长，用既有的 truncate 约定在文案层截断到 120 字符后拼接，**不得**把整个 BLOB 塞进 `title`。多字节字符（中文 / emoji）只影响显示宽度，不影响键构造：`dirtyCellKey` 用列名，不做大小写或宽度归一化（**列名必须精确相等**，`Name` 与 `name` 是两个不同的列）。 |
| 15 | **`valuesEqual` 的已知边界** | 两个键序不同的 JSON 对象会被判为不等（假脏）；`NaN` 与 `null` 被判为相等（`JSON.stringify(NaN)` = `'null'`）；`undefined` 与 `null` 被判为相等（`left ?? null`，**这是需要的**：DB NULL 到前端是 `null`，编辑器取消路径可能给出 `undefined`）。这些是现状语义，本册**不改**，但要写进单测把行为钉住，并作为第 11 节 D-1 提请裁定。 |
| 16 | **关闭标签页** | `removePanel` 删除整块 slice → `pendingChanges` 与标记一起消失，改动静默丢弃（仅当页面报错时有 `tableData.errorPendingHint` 提示）。本册把这条写成明确事实，是否加拦截弹窗见第 11 节 D-5。 |

---

## 8. i18n key 清单

**落点：`src/locales/en/query.ts`**（领域包，**不是** `src/locales/en.ts` —— 后者只有一行 `export { default } from './en/index'`，改它无效）。**不要**新建域名，不要动其它语言文件。

| 完整 key | 值（英文） | 新增 / 修改 | 用途 |
| --- | --- | --- | --- |
| `tableData.pendingChanges` | `'{count} rows with unsaved changes'` | **修改**（原为 `'{count} unsaved changes'`） | 语义修正：调用方传的确实是行数，原文案的「changes」会让「同一行改 3 格显示 1」看起来像 bug |
| `tableData.pendingCells` | `'{count} cells changed'` | 新增 | 面板级单元格级计数（PRD「计数与高亮一致」的直接承载） |
| `tableData.pendingOffPage` | `'{count} more cells changed on other pages'` | 新增 | `cellsOffPage > 0` 时显式告知改动仍在 |
| `tableData.dirtyCellAria` | `'Changed cell: {original} → {next}'` | 新增 | 单元格 `aria-label`（读屏可用；`title` 的等价文本） |
| `tableData.dirtyValueNull` | `'NULL'` | 新增 | tooltip 里渲染 SQL NULL |
| `tableData.dirtyValueEmpty` | `'(empty string)'` | 新增 | tooltip 里区分空字符串与 NULL |
| `tableData.dirtyDeletedRow` | `'Marked for deletion'` | 新增 | 删除行行号槽圆点的 `title` / `aria-label` |
| `tableData.restoreRow` | `'Restore This Row'` | 新增 | 右键菜单项（`buildDataTableContextMenuItems` 的 `labels.restoreRow`） |

**复用既有、不改**：`tableData.pendingUpdates`、`tableData.pendingDeletes`、`tableData.commit`、`tableData.rollback`、`tableData.preview`、`tableData.errorPendingHint`、`dataTable.selectRow`、`common.loading`。

**纪律（会被测试直接卡住）**：

1. `src/locales/locales.test.ts` 的 `resolves every literal t() key used in host source` 会用正则扫宿主源码里的 `t('字面量键')`，**要求键存在于 `en`**。因此新增文案必须同时加 `en` 词条，且调用点写成字面量（不要 `` t(`tableData.dirty${kind}`) `` 这种动态拼接 —— 既扫不到，也不可检索）。
2. 插值参数必须与调用点一致（`{count}` / `{original}` / `{next}`）。
3. **只改英文侧**。其余语言在发布前由 `node scripts/i18n-sync-check.mjs` 与 i18n 同步流程补齐。已核实这是本仓库既定做法：`en/query.ts` 现有 389 个键中有 **18 个**在 `zh-CN/query.ts` 中不存在（例如 `tableData.beginTx`、`tableData.commitTx`、`query.session.revisionConflict`），而基线 `pnpm typecheck` 通过。

**错误码：本册不新增任何错误码。** 理由与落点说明：

- 本册涉及的失败态全部复用**既有**标识：行身份歧义用 `AMBIGUOUS_ROW_IDENTITY_ERROR`（已存在于 `src/stores/tableData/pendingChanges.ts` 并在 `commitFetchedPage` / `stageCellChange` / `stageRowDelete` 里写入 `error`）；提交期的计划过期 / 影响行数不符由提交链路产生 `grid.commit.stalePlan` / `grid.commit.affectedMismatch`（00 契约 8.2 已定义），本册只负责**标记保留**。
- 因此本册**不改** `src/lib/gridErrors.ts`，也**不依赖**它：该文件在基线中**不存在**（00 契约 8.1 把它标为「新增」并建议了位置，但未指派分册）。按 00 契约「匹配不到前缀时返回 `'unknown'`，UI 必须回退显示原始消息」，本册的错误展示一律使用原始消息；若 DB-06 落地时该模块已存在，则把错误文本交给 `classifyGridError()` 分类后再展示，行为不变。
- 将来若确需脏标记专属错误，前缀用 `grid.dirty.<原因>: <说明>`，并在 `classifyGridError()` 的前缀表里加一条。

---

## 9. 测试清单

### 9.1 单测 `src/stores/tableData/__tests__/dirtyIndex.test.ts`（新增）

| 用例名 | 断言要点 |
| --- | --- |
| `dirtyCellKey is injective for column names that contain the identity separators` | 构造列名 `a\u0000b`、`a:b`，断言不同 `(identityKey, col)` 组合不产生相同键（防分隔符退化） |
| `buildDirtyIndex marks only the columns that really changed` | 一行 `changedColumns: ['name','score']` 但 `currentValues.score === originalValues.score` 时只标 `name`（**双保险过滤生效**） |
| `reverting a cell to its original value removes the dirty mark` | 同 `identityKey` 的条目在删键后不再出现在 `byRowIndex`，且 `counts.totalCells === 0` |
| `delete-marked rows produce no cell marks and count as deleteRows` | `state === 'deleted'`、`changedColumns` 为空数组、`counts.deleteRows === 1`、`totalCells` 不含该行 |
| `duplicate row identities are never marked dirty` | 两行同主键 → 该身份不在 `byRowIndex`（对齐 `rowIdentityIsUnique` 守则） |
| `null and undefined values compare equal (DB NULL reaches the UI as null)` | `valuesEqual(undefined, null) === true`；暂存 `null` 覆盖 `null` 不产生标记 |
| `same-looking values of different types are a real change` | `1` 与 `'1'` 标脏；`''` 与 `null` 标脏 |
| `valuesEqual pins its object, NaN and undefined semantics` | `{a:1,b:2}` 与 `{b:2,a:1}` 现行为不等（记录既有边界）；`NaN` 与 `null` 现行为相等 |
| `counts split cellsOnPage and cellsOffPage for one changed row` | 改动命中当前页 → `cellsOnPage === 1`、`cellsOffPage === 0`；身份不在当前页 → `0` / `1`，且 `totalCells` 恒为 1 |
| `anchored key wins over the recomputed identity key` | 主键被改过、`rowIdentityAnchors` 指向原始身份时，条目仍归位（与 `findPendingForRow` 优先级一致） |
| `structure sharing keeps untouched row entries referentially stable` | `prev.byRowIndex.get(1) === next.byRowIndex.get(1)`；被改行是新对象 |
| `selectDirtyCounts ignores page state entirely` | 传入 `rows: []` 的 TableState 时计数不变（保证刷新/断连时计数仍正确） |
| `empty pendingChanges yields EMPTY_DIRTY_INDEX semantics` | `byRowIndex.size === 0`、`offPageKeys` 为空、计数全 0 |
| `batch index agrees with findPendingForRow for every row` | **等价性契约**（见 3.3 第 6 步）：对同一 `TableState`，逐行断言 `buildDirtyIndex(ts).byRowIndex.get(i)?.identityKey === findPendingForRow(ts, i, ts.rows[i], pkColumns)?.key`；覆盖四种组合：普通行 / 主键被改过的行（锚点优先）/ 重复身份行（两边都应为「不标脏」）/ 无主键表。任一行不一致即失败 |
| `batch index agrees with findPendingForRow after a page reorder` | 同一批暂存改动、`rows` 顺序颠倒后重跑等价性断言（证明归属跟身份走，不跟行号走） |
| `dirty derivation never reads selection state` | 给 `ts` 挂上（未来 DB-01 会加的）选择字段与不同取值，断言 `buildDirtyIndex` 的输出**逐字节不变**（防止本册误吃选择参数） |

### 9.2 组件测试 `src/components/DataTable/__tests__/VirtualBody.dirty.test.tsx`（新增）

沿用 `VirtualBody.test.tsx` 的 mock 手法（mock `useVirtualTable` 与 `useI18n`）。

| 用例名 | 断言要点 |
| --- | --- |
| `renders data-dt-dirty only on the changed cells` | `container.querySelectorAll('[data-dt-dirty="true"]').length === 1`，且它同时带 `data-dt-col="name"` |
| `clean cell does not carry the attribute at all` | `[data-dt-col="id"]` 上没有 `data-dt-dirty` 属性（不是 `"false"`） |
| `dirty cell exposes the original → next tooltip` | `title` 含原值与新值；`next === null` 时含 `NULL` |
| `delete-marked row is struck through and its cells are not dirty` | 行容器有 `line-through`；该行内 `[data-dt-dirty]` 数为 0 |
| `row-number gutter shows a dot for dirty and for deleted rows` | `tid('row-dirty-dot')` / `tid('row-deleted-dot')` 各自出现 |
| `no dirty attributes and no dots when dirtyByRow is omitted` | 缺省渲染与今天逐位一致（零回归护栏） |
| `dirty marker does not replace the type color on the value span` | 值 `<span>` 仍带 `text-dt-number` 等类型类（标记与类型色正交） |
| `deleted row double-click does not request an edit` | 不调用 `onCellDoubleClick`（守卫在 TableView，但组件层要保证不额外触发） |

### 9.3 连续旅程测试 `src/stores/__tests__/tableDataStore.dirty.test.ts`（新增）

用 `useTableDataStore` 真实 store + mock 的 `databaseCommands`（沿用 `tableDataStore.test.ts` 的脚手架），每一步都断言状态跃迁。

| 用例名 | 断言要点 |
| --- | --- |
| `journey: edit → highlight → edit back → clean` | `stageCellChange('name','Updated')` → `totalCells === 1` 且 `byRowIndex.get(0).state === 'dirty'`；再 `stageCellChange('name','Alice')` → `pendingChanges.size === 0`、`totalCells === 0`、`byRowIndex` 无该行（**必需用例**） |
| `journey: stage on page 1 → next page → back: the mark stays on the original row` | 第 0 页改 `id=1` 的 `name` → `setPage(1)`（mock 返回另一页数据）→ 该页无标记、`cellsOffPage === 1` → `setPage(0)`（mock 返回原页）→ 标记回到 `id=1` 那一行，且**不是**回到原来的行号（把第 0 页数据顺序颠倒后再断言）（**必需用例**） |
| `journey: two cells in one row → totalCells 2, totalRows 1` | 对准 PRD 的「计数只算第一个」缺陷形态 |
| `journey: sort reload keeps the mark on the same identity` | `setSort` 后 mock 返回重排行序的数据 → 标记跟随主键而不是行号 |
| `journey: filter change keeps off-filter changes staged and counted offPage` | `setFilters` → 改动仍可提交（`previewPendingChanges` 仍收到该条目）、`cellsOffPage === 1` |
| `journey: commit failure keeps every mark` | 失败后 `totalCells` 不变、`error` 有值、`previewPlan` 保留 |
| `journey: commit success clears marks only when the signature is unchanged` | 签名未变：`totalCells === 0`。**再跑一次在途场景**：在 `commitPendingChanges` 的 await 期间又 `stageCellChange` 一格 → 签名不等 → 断言**一条都不清**（含已提交那批），把 4.7 第 1b 行的现行为钉住（这是缺陷登记，不是期望行为，见 D-10） |
| `journey: rollback clears marks synchronously, values revert after the reload` | 调用后立刻 `totalCells === 0`；reload 完成后 `rows` 回到原值 |
| `journey: unstageRowDelete returns a delete-marked row that also had cell edits to dirty` | `deleteMarked === false`、`changedColumns` 保留、行级态从 `deleted` 变 `dirty` |
| `journey: invalidateCachedData keeps a panel with staged edits and its marks` | 断开/失效后该 panel 的 `totalCells` 与行不变（既有行为的回归护栏） |
| `journey: read-only and no-primary-key tables never create a mark` | 无主键表取数后 `stageCellChange` → `error` 有值、`totalCells === 0` |
| `journey: staging and clearing a change never rewrites selectedRows` | 选中两行 → 改一格 → 清回原值 → 全程断言 `selectedRows` 与 `lastSelectedIndex` 逐位不变（守住 00 契约 4.2 第 3 条：派生行集不写回；也守住本册 4.6 的「选择不因脏而变」） |
| `journey: a write executed elsewhere keeps the marks and the overlaid values` | 先暂存一格 → 模拟别处写 SQL 触发 `invalidateCachedData(dbSessionId)` → 断言有暂存的 panel 行与标记逐一不变；无暂存的 panel 被清空且计数仍为 0 |
| `journey: connection teardown drops the slice and every mark` | 调 `removePanel(panelId)`（`ContentView` 在连接断开时走的就是这条）→ `byPanel` 无该 slice、无标记、无计数（把 4.7 第 6 条的「改动随 slice 丢弃」写成事实，而不是留给用户猜） |

### 9.4 E2E `e2e/specs/table-dirty-cells.ts`（新增）

沿用 `e2e/specs/table-edit.ts` 的脚手架（`connectSeededPgInWorkspace` / `openQueryTab` / `executeSQLChecked` / `doubleClickCellByText` / `confirmWebDialog`）。

| 用例名 | 断言要点 |
| --- | --- |
| `改 3 格后网格高亮与计数一致` | `document.querySelectorAll('[data-dt-dirty="true"]').length === 3`；`tid('pending-cells-count')` 文本含 `3` |
| `改回原值后脏标记与计数一并消失` | 同上改回 → 该格无 `data-dt-dirty`，计数条隐藏 |
| `翻页再回来脏标记仍归属原行` | 先记下被改行的主键文本，翻页、翻回，断言该主键所在行的格子仍带 `data-dt-dirty` |
| `删除 2 行后删除线可见，恢复后消失` | 行容器有删除态、计数条显示 2；右键 `restore-row` → 删除线与计数一并回退 |
| `亮色 / 暗色 / 第三方主题下高亮均可见` | 三种主题下 `$('[data-dt-dirty="true"]').isDisplayed()` 为真（主题切换后等待重绘，不用几何坐标断言） |

---

## 10. 自查清单

| # | ❌ 错误做法 | ✅ 正确做法 | 后果 |
| --- | --- | --- | --- |
| 1 | 留着 `editBuffer`「顺手也更新一下」 | 连死 prop 一起彻底删除 | 两套派生缓存在「删除标记行」与「离屏行」上必然分叉，且只在特定时序复现 |
| 2 | 用 `rowIndex` 做脏标记的键 | 用 `dirtyCellKey(identityKey, columnName)` | 翻页 / 排序 / 筛选后把 A 行的改动画到 B 行上 |
| 3 | 用 `===` 判「是否真的改动」 | 用 `valuesEqual` | 对象 / JSON 列每次编辑都判脏（引用不等），用户改回原值也清不掉标记 |
| 4 | 用 `pendingChanges.size` 当「改动数」 | 行数用 `totalRows`、格数用 `totalCells` | 「同一行改 3 格显示 1」，正是 PRD 点名的计数与高亮不一致缺陷 |
| 5 | 把 `new Map()` / `new Set(...)` / 展开数组放进 `useMemo` deps 或 props | deps 只放 store 里的 `pendingChanges` / `rowIdentityAnchors` 引用 | 每次渲染引用都变 → `memo` 全失效 → 大表每次击键整表重渲染 |
| 6 | 在 `VirtualRow` 里读整个 `pendingChanges` | 只通过 `dirtyRow` 传本行条目 | 任一行改动都让所有可见行重渲染 |
| 7 | 用 `pendingChanges.size` 作为记忆化依赖 | 依赖整个 Map 引用 | 同一行连续改两格时 `size` 不变 → 高亮停在旧值上 |
| 8 | 删 `editBuffer` 时漏删 `DataTableProps.editBuffer` 与 `CellEdit` 导入 | 三处一起删（字段 / 初始化 / 死 prop） | 留下一个永远填不满的死 prop，下一个人会照 PRD 再写一遍 `rebuildEditBuffer` |
| 9 | 脏标记只用颜色区分 | 左边框 + 角标 + tooltip 三重通道，颜色走 token | 色盲用户与低对比主题下完全看不出改动 |
| 10 | 用 `--dt-number` 之类的**类型色**做脏背景 | 新增 `--dt-dirty-*` 且只作用于 `border` / `background`，永不动 `color` | 数字列（暖橙）上的标记与类型色互吃，或让数字看起来像警告 |
| 11 | 提交失败后清掉标记 | 只写 `error`，标记与计数原样保留 | 用户以为改动已提交，实际没写库，数据静默不一致 |
| 12 | 对值做 `trim()` 再比较 | 原样比较（`valuesEqual`） | 只改空格这类真实改动被判为「无变化」，用户的编辑被静默丢弃 |
| 13 | 把 tooltip 做成自定义浮层并测矩形 | 用原生 `title` | 与 DB-01 区域拖拽 / DB-03 右键菜单抢事件，并引入几何坐标反查（违反项目纪律） |
| 14 | 在 `dirtyIndex.ts` 里顺手实现区域求交 / 行集推导，或让 `buildDirtyIndex` 接收 `GridSelection` | 区域运算只放 `src/stores/tableData/gridSelection.ts`（DB-01 拥有），脏索引的输入只有 `rows` / `columns` / `pendingChanges` / `rowIdentityAnchors` | 两套区域实现必然分叉（00 契约 4.1 明令禁止）；脏索引吃选择参数会让「选中一格」意外改变高亮 |
| 15 | 用「选中了哪些行」来决定哪些行显示删除线 | 只看 `pendingChanges` 条目的 `deleteMarked` | 翻页 / 切换选择模式 / `cell` 模式清空 `selectedRows` 之后，删除态会整批消失或错位 |
| 16 | 为了「省事」重写一份行查找（自己算身份 + 自己判唯一性） | 以 `findPendingForRow` 的语义为权威，用逐行等价性用例锁住批量版 | 单行路径与批量路径判定漂移，同一行在计数条与网格上结论不同，且只在重复主键 / 主键被改的组合下复现 |

---

## 11. 未决问题

| # | 问题 | 建议选项与推荐 |
| --- | --- | --- |
| **D-1** | `valuesEqual` 的判等语义是否升级？**事实**：它用 `JSON.stringify(left ?? null)`，因此 `{a:1,b:2}` 与 `{b:2,a:1}` 判为**不等**（JSON 列会假脏），`NaN` 与 `null` 判为**相等**，`undefined` 与 `null` 判为**相等**（这一条是需要的）。同文件里已有更稳的私有 `stableSerialize`，但未导出。00 契约 3.4 要求「一律复用既有 `valuesEqual`」，因此本册不改它。 | **推荐 A**：本册保持 `valuesEqual` 原样，只补一条把上述行为逐条钉住的单测；把「`valuesEqual` 内部改用 `stableSerialize`（并导出它）」作为独立小改动，排在 DB-03 / DB-05 之后统一做。**备选 B**：现在就改，好处是顺手消灭 JSON 假脏；代价是它在同一次提交里动到**写路径**（`changedColumns` 的计算来源），违反「一次提交只做一件事」。 |
| **D-2** | PRD DB-06 技术落点要求「`CellRenderer` 增加 `dirty` / `originalValue` / `state`」，本册改为把标记挂在 `VirtualBody` 的单元格 wrapper 上。 | **推荐 A（本册方案）**：标记覆盖单元格盒子（含 padding），`CellRenderer` 一行不改，既有的 5 条 `querySelector('span')` + `className` 断言零风险。**备选 B**：按 PRD 原文给 `CellRenderer` 加 `dirty` 并只做文本级弱化（不改 DOM 结构），代价是要同步改 4 条既有断言，且「左边框 + 角标」仍需 wrapper 承担 —— 等于两处表达同一件事。建议采纳 A 并把结论回写 PRD。 |
| **D-3** | 行级脏状态需要一个可断言的 DOM 属性，但 00 契约 4.4 只冻结了 `data-dt-selected` / `data-dt-dirty` / `data-dt-editing` / `data-dt-cell-type`，没有行级属性。 | **推荐 A**：新增 `data-dt-row-state="dirty" \| "deleted" \| "inserted"`（挂在 `VirtualRow` 的行容器上），由 00 契约 owner 追认后写进 4.4 表。理由：E2E 断言不依赖样式，`line-through` / `opacity` 这类断言太脆。**备选 B**：不加行级属性，E2E 只断言单元格 `data-dt-dirty` 与行号槽圆点（`tid('row-deleted-dot')`）。 |
| **D-4** | 是否提供**单元格级**「恢复原值」右键项？ | **推荐**：本册**不实现**。PRD 只要求「恢复此行」；单元格级恢复会与 DB-03 的菜单改造撞在同一段代码上。留到 DB-03 批次统一做（实现成本很低：把 `originalValues[col]` 经 `stageCellChange` 写回即可，`valuesEqual` 自动判定回退）。 |
| **D-5** | 携带未提交改动关闭标签页是否要拦截？**事实**：`removePanel` 直接删 slice，改动静默丢失；只有页面报错时有 `tableData.errorPendingHint` 提示。 | **推荐 A**：本册只把「关标签页 = 丢弃」写成明确事实，并靠 `tableData.pendingOffPage` 让用户看得见还剩多少改动；拦截弹窗属于窗口 / 会话职责，交 DB-16 或会话分册裁定。**备选 B**：本册顺手加一个确认弹窗（跨了职责边界，且 `PanelTabBar` 不在本册落点内）。 |
| **D-6** | 批量粘贴的写放大：`applyColumnToRows` 逐格 `stageCellChange`，1000 格 = 1000 次 store 写与 1000 次索引重建。 | **推荐 A**：本册先定义并实现 `stageCellChanges(panelId, edits: ReadonlyArray<{ row: number; col: string; value: unknown }>)`（一次构键、一次 `set()`），DB-03 直接复用；同时留一条「逐格调用也语义正确」的守护测试。**备选 B**：本册不管，等 DB-03 提性能 —— 风险是 DB-03 落地时才发现要改 store 签名，又要回改本册的测试。 |
| **D-7** | 当改动不在当前页时，除了提示要不要**自动跳转**到第一条改动的页？ | **推荐**：不跳转，只提示。跳转会打断用户当前的筛选 / 排序上下文，而 `totalCells` + `pendingOffPage` 已经足够让用户判断「改动没丢」。若后续要跳转，应做成计数条上的可点击链接（用户主动触发），而不是自动导航。 |
| **D-8** | 同一个单元格既是**选中**又是**脏**时，两个背景通道如何叠加？**事实**：`data-dt-selected`（DB-01）与 `data-dt-dirty`（本册）会挂在同一个单元格 `div` 上；行级选中用 `bg-accent/15`，本册的脏标记也想给一个底色。两块半透明底色直接叠加会得到第三种颜色，可能让「脏」在选中区域里看不出或看起来像另一种状态。 | **推荐 A**：**划分通道** —— 选中只占背景（`bg-accent/*`），脏只占左边框 + 右上角角标 + tooltip，**脏不再给背景色**（`--dt-dirty-bg` 只用于未选中格，或干脆取消该 token）；这样两态同格时仍然各自可辨，且与 4.4 的「形状是主通道」一致。**备选 B**：脏背景叠加在选中背景之上并刻意提亮（需要 DB-01 确认优先级顺序，且要在亮 / 暗 / 第三方主题三套下都验证对比度）。倾向前者：少一个需要逐主题校色的组合。 |
| **D-9** | 面板计数条是否也要显示「选中区域覆盖 N 行」这类**选择域**信息（00 契约 4.2 要求行级操作在 `cell` 模式下说清作用域）？ | **推荐**：本册**不做**。计数条的语义是「有多少未提交改动」（数据事实），选择域是「这次操作会作用到哪些行」（操作范围），两者混在一条上会让「3 cells changed」与「将删除 12 行」互相干扰。选择域提示归 DB-01 / DB-02 的工具条，落在同一面板的**另一条**状态行。 |
| **D-10** | **提交在途期间暂存新改动，会导致「已提交成功的那批也仍被标脏」。** 事实：`commitPendingChanges` 里 `pendingChangesSignature(latest.pendingChanges) === signature` 不成立时，整个 `patchPanel` 被跳过 → 一条都不清；随后 reload 又用 `overlayPendingRows` 把已提交的值盖回去，于是「值已经是数据库的值，界面却仍说是未提交」。本册的高亮完全跟随 `pendingChanges`，因此会如实显示这个状态。 | **推荐 A**：本册**不改**该守卫（它属提交链路 / 事务分册），只把它写成事实（4.7 第 1b 行）、用一条旅程用例把它钉住、并在报错/提示文案上避免误导；把「守卫粒度从整批改为按已提交身份集合清理」作为独立缺陷提交给提交链路分册。**备选 B**：本册顺手把守卫改成按身份差集清理 —— 代价是一次提交同时动提交语义与高亮渲染，回归面过大，且 `pendingChangesSignature` 的语义被写进既有测试，牵一发动全身。 |
| **D-11** | 计数条组件的归属与拆分。**已裁定**：统一为 `src/windows/connection/TablePendingChangesBar.tsx`，分册 01 Step 10 首次创建，03 / 06 只加增量 props 与子元素。 | **本册服从，不保留拆分方案**。登记理由：该组件只有一条计数条 + 三个按钮，拆成两个组件会同时留下两个 `pending-changes-bar` 的排序/可见性判断，必然出现「两个条同时渲染」或「条消失」的时序问题。若后续有人主张拆分，必须先在 01 的骨架里把「谁负责渲染条的外层容器」定死，否则不拆。 |
