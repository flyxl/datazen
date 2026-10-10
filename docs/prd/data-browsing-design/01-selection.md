# 01 · 单元格与区域选择模型（DB-01）

> **状态**：目标设计，**未实现**。本册描述的能力（单元格坐标、矩形区域选择、`cell`/`row`/`none` 三模式）在今天的数据浏览网格里**完全不存在**：现状只有整行选择 `TableState.selectedRows`。
>
> **依赖**：本册**完全依赖** [00-contracts](00-contracts.md)，尤其是第 4 节（单元格坐标系与选择模型契约，权威、不得另立一套）、第 2 节（兼容性铁律 R1~R5）、第 8 节（统一错误码与英文文案落点）。本册**被 02（键盘导航）/ 03（剪贴板）/ 05（类型化编辑器）/ 06（dirty 高亮）/ 07（外键跳转）依赖**：这五册消费的坐标、区域、模式、DOM 属性、纯函数签名全部在本册冻结。。其中 **02 的选择器契约（`data-dt-row-view`）与本册的行号槽 DOM 形态（`<div role="rowheader">` + 内层 `<button tabIndex={-1}>`）是硬依赖**：01 一旦落地，02 的 6 处行定位断言与 02 的 roving tabindex 都只能在这个 DOM 形态上实现，02 不得再改角色
>
> **预估工作量**：**4.0 人日**（纯逻辑与单测 1.5 人日；组件接线与抽取重构 1.0 人日；**ARIA 角色三元组与行号槽 `rowheader` 改造 0.5 人日**；连续旅程测试与 E2E 1.0 人日）。不含 02~07 的键盘、剪贴板、编辑器、高亮、外键工作量。⚠️ 本数字已含 [契约第 4.4 节](00-contracts.md) 追加的「行号槽 `<button>` → `<div role="rowheader">` + 内层按钮」改造（0.5 人日那一项）；总纲 [第 2 节](../data-browsing-design.md) 的 01 行**已同步改成 4.0**，两处必须一起改，否则总量对不上。
>
> **本文档不包含什么**：
> - 不包含键盘导航与快捷键集（DB-02；本册只保留 `Escape` 退出与"守卫函数"两个边界，方向键 / `Home` / `End` / `PgUp` / `PgDn` / `Shift+方向键` / `Enter` / `Tab` 全部归 02）。
> - 不包含剪贴板读写与右键菜单的复制条目实现（DB-03；本册只提供选区数据与一个可选回调挂点）。
> - 不包含类型化编辑器、Set Value、dirty 高亮、外键跳转、新增行 INSERT、查询结果网格可编辑（分别归 05 / 05 / 06 / 07 / 04 / 10）。
> - 不包含**任何 Rust 侧改动**：DB-01 是纯前端能力，不改 `driver-api`、不改 DTO、不新增 IPC 命令、**不动 `PROTOCOL_VERSION`**。
> - 不包含列重排 / 冻结 / 列宽列序持久化（DB-12）、不包含 `@datazen/ui` 抽取（DB-13）。
> - **ARIA 角色集合的裁定权已归本册**（不再是未决问题 Q1）：`grid` / `row` / `gridcell` / `rowheader` + `aria-rowcount` / `aria-colcount` / `aria-activedescendant` **由本册原子加齐**（契约第 4.4 节）。本册**仍不包含**的是 roving tabindex 与键盘导航——那是 02 的职责，02 只改焦点不碰角色（见 4.4 与 Q1）。

---

## 1. 目标与验收口径

**一句话目标**：把网格的交互坐标从「行」提升到「单元格」，引入唯一一份、由 store 持有的 `GridSelection`（锚点 / 焦点 / 附加区域 / 三模式），使「点击、Shift 连选、Cmd 加选、拖拽矩形」产生可被 02/03/05/06/07 直接消费的坐标与区域，且**选择状态永不参与写操作定位**。

### 1.1 可验证验收标准

| # | 验收项 | 具体命令 | 期望输出 |
| --- | --- | --- | --- |
| A1 | 纯逻辑与组件行为正确 | `npx vitest run src/stores/tableData/__tests__/gridSelection.test.ts src/lib/__tests__/gridEventGuards.test.ts src/components/DataTable/__tests__/DataTable.selection.test.tsx src/components/DataTable/__tests__/GridCell.test.tsx src/components/DataTable/__tests__/DataTable.selection.journey.test.tsx` | 末尾出现 `Test Files 5 passed`、`Tests N passed`，且**没有** `FAIL` 行；退出码 `0` |
| A2 | 测试文件参与类型检查且干净（`tsconfig.json` 不排除 `__tests__/`） | `pnpm typecheck` | 无 `error TS`；退出码 `0` |
| A3 | 既有整行选择与批量操作零回归 | `npx vitest run src/components/DataTable src/windows/connection/__tests__/TableView.test.tsx src/stores/__tests__/tableDataStore.test.ts` | 全绿；右键菜单条目 id 序列与今天**逐字一致**（既有断言不改） |
| A4 | 真实鼠标序列在应用内生效 | `pnpm e2e:minimal` | `e2e/specs/table-cell-selection.ts` 与 `e2e/specs/table-batch-ops.ts`（TC-TABLE-009~014）均通过；`table-edit.ts` 的 `pending-changes-bar` / `pending-preview` / `pending-commit` / `pending-rollback` 断言保持通过 |
| A5 | 单文件不越 800 行红线 | `wc -l src/components/DataTable/DataTable.tsx src/components/DataTable/VirtualBody.tsx src/windows/connection/TableView.tsx src/stores/tableDataStore.ts` | `DataTable.tsx` ≤ 470、`VirtualBody.tsx` ≤ 190、`TableView.tsx` ≤ 660、`tableDataStore.ts` ≤ 700 |
| A6 | 拖拽 `(r2,c1) → (r5,c3)` 的可观察结果 | E2E 断言（A4 内） | 工具栏出现 `4 rows × 3 columns`；`[data-dt-selected="true"]` 的单元格恰好 12 个；右键菜单出现 `copy-selected-cells` 条目（由 03 接线后） |

> 纪律（AGENTS.md）：上述测试/门禁命令的长输出**一律重定向到系统临时目录**再 `tail`，退出码必须单独打印；门禁运行**首尾各记录一次 HEAD 与工作区 sha**；本轨驱动集为 `--drivers=basic`（postgres / mysql / sqlite / redis），"宿主用例全绿"只在该驱动集语义内成立。

---

## 2. 现状代码事实

> 本节的每一条都是**亲自读过源码**后写下的；只列符号，**不写行号**（仓库纪律：行号随重构腐烂且无门禁校验）。带「新增」的条目**今天不存在**，不要当成既有能力。

### 2.1 网格容器与 props

| 文件 | 已核实的符号 / 事实 |
| --- | --- |
| `src/components/DataTable/DataTable.tsx` | `DataTableProps`：含 `columns: ColumnDef[]`、`rows: unknown[][]`、`editingCell`、`editBuffer`（**声明了但组件解构参数里没有它**，等价死 prop）、`selectedRows?: Set<number>`、`onRowSelect?: (index, opts?: {multi?, range?}) => void`、`onSelectAll`、`onRowClick`、`highlightedRow`、`primaryKeyColumns`、`onDeleteRows`、`getContextCellText`、`emptyPlaceholder`、`rowHeight = 28`、`className` |
| 同上 | `DataTable`：`const [scrollEl, setScrollEl] = useState<HTMLDivElement \| null>(null)`；滚动容器的 `ref={setScrollEl}`、`onContextMenu={handleContextMenu}`、`aria-label={t('dataTable.tableLabel')}`；**外层根 div 上挂着 `onKeyDown={handleDeleteKey}`**（根 div 本身不可聚焦） |
| 同上 | `handleDeleteKey`：唯一网格级键盘处理，只认 `Delete`/`Backspace`，且 `if (editingCell) return`、要求 `onDeleteRows` 存在且 `primaryKeyColumns` 非空、`selectedRows.size > 0` |
| 同上 | `handleRowClick`：把 `onRowClick?.(index)` 与 `onRowSelect?.(index, opts)` 串起来（**整行选择的唯一接线点**） |
| 同上 | `handleContextMenu`：右键菜单全部构造逻辑内联在这里（约 220 行），用 `resolveDataTableCellFromEvent` / `resolveDataTableHeaderColFromEvent` 反查，再调 `buildDataTableContextMenuItems` + `showNativeContextMenu` |
| 同上 | `hasSelection = onSelectAll != null && onRowSelect != null`：**决定工具栏（全选复选框 + 选中计数 + 删除按钮 + 导出按钮）是否渲染** |
| 同上 | `columnNames = columns.map((c) => c.name)`；区域判列用的就是这份顺序 |
| 同上 | 工具栏选中计数文案：`t('dataTable.selected')` + `selectedRows.size` + `/` + `rows.length` + `t('common.rows')` |
| `src/components/DataTable/TableHeader.tsx` | `ColumnDef { id: string; name: string; type?: string }`；`TableHeaderProps`：`columns`/`sorts`/`onSort`/`columnWidths`/`onResizeStart?`/`sortable`/`onFilterColumn?` |
| 同上 | `TableHeader`：`const active = sorts[0]`（**只认第一个排序**）；表头单元格挂 `data-col-header={col.name}`，内部还有 `data-col-label`、`data-sort-icon`；双击表头循环 `none → asc → desc → asc`；列宽拖拽手柄是 `absolute right-0 … w-[5px]` 的 div，`onPointerDown` 里 `e.preventDefault()` 后调 `onResizeStart(colIdx, e.clientX)` |
| 同上 | 行号槽（最左 `w-10` 表头格）里只有 `#` 文本，**没有** 全选入口 |
| `src/components/DataTable/VirtualBody.tsx` | `VirtualBodyProps`：`columns`/`rows`/`rowHeight`/`editingCell`/`selectedRows`/`highlightedRow?`/`scrollElement`/`columnWidths?`/`onCellDoubleClick`/`onCellEdit`/`onCellEditCancel`/`onRowSelect` |
| 同上 | `VirtualBody`：`useVirtualTable({ rows, rowHeight, overscan: 12, scrollElement })`；`colNames = columns.map((c) => c.name)`；把 `selected={selectedRows.has(vRow.index)}`、`nextSelected={selectedRows.has(vRow.index + 1)}`、`highlighted` 传给每行 |
| 同上 | `VirtualRow`（`memo`）：外层 `div` 带 `tabIndex={0}`、`onClick={handleClick}`、`onDoubleClick={handleRowDoubleClick}`（双击 → 第 0 列编辑）、`onKeyDown={handleKeyDown}`（`Enter`/`' '` → `onRowSelect(vRow.index)`，**无修饰键**）；`handleClick` 把 `e.metaKey \|\| e.ctrlKey` 映射成 `multi`、`e.shiftKey` 映射成 `range` |
| 同上 | 行号槽是 `<button type="button">`（`title={selectRowLabel}`，无 testid），`onClick={handleSelectButtonClick}`（`e.stopPropagation()` 后调 `onRowSelect(vRow.index, { multi, range })`） |。⚠️ **这一点在加 `role="row"` 之后会变成缺陷**：行容器一旦声明 `role="row"`，其直接子元素的内容模型就被限定为 `gridcell` / `rowheader` / `columnheader`，`<button>` 不在集合内，屏幕阅读器会**把整个行号槽当作无效内容丢弃**（详见 4.4 与 Step 8）
| 同上 | 单元格：`div` 带 `data-testid="data-table-cell"`、`data-dt-row={vRow.index}`、`data-dt-col={col.name}`、`onDoubleClick`（`stopPropagation` 后 `onCellDoubleClick(vRow.index, col.name)`）；`isEditing = editingCell?.row === vRow.index && editingCell.col === col.name`；`colW = columnWidths?.[colIdx] ?? 160` |
| 同上 | 单元格**没有** `data-dt-selected` / `data-dt-editing` / `data-dt-cell-type`，**没有** `role`，**没有** `id` |

### 2.2 表格状态与 store

| 文件 | 已核实的符号 / 事实 |
| --- | --- |
| `src/stores/tableData/types.ts` | `CellEdit { rowIndex, columnName, originalValue, newValue, pkSnapshot }`；`TableState`：`context`/`columns`/`rows: Record<string, unknown>[]`/`totalRows`/`page`/`pageSize`/`filters`/`filterLogic`/`draftFilters`/`draftFilterLogic`/`filterPanelOpen`/`sorts`/`editBuffer: Map<string, CellEdit>`/`pendingChanges`/`rowIdentityAnchors: Map<number, string>`/`previewPlan`/`pendingStatus`/`selectedRows: Set<number>`/`lastSelectedIndex: number \| null`/`editingCell`/`detailRowIndex`/`loading`/`requestRevision`/`loadingRevision`/`error`/`visibleColumns: string[] \| null`。**`TableState` 里没有任何单元格级坐标或区域字段** |
| `src/stores/tableData/connectionState.ts` | `emptyTableState(context?)`：`TableState` 的唯一工厂，`selectedRows: new Set()`、`lastSelectedIndex: null`、`visibleColumns: null`；`rowsToRecords`；`editKey(rowIndex, columnName)` → `` `${rowIndex}:${columnName}` ``；`toCellValue`；`extractErrorMessage`；`TableTarget`；`buildTableContext` |
| `src/stores/tableData/pendingChanges.ts` | `AMBIGUOUS_ROW_IDENTITY_ERROR`；`effectivePendingIdentity`；`rowIdentityIsUnique`；`hasPendingIdentityCollision`；`findPendingForRow`（先用 `ts.rowIdentityAnchors.get(rowIndex)` 再回退按身份键直查）；`rebuildEditBuffer(ts)`（**已实现但从未被网格消费**）；`overlayPendingRows`；`pendingChangesSignature`；`pendingChangesForWire` |
| `src/stores/tableDataStore.ts` | `TableDataStore` 接口；`LoadTableDataParams`；`patchPanel(get, set, panelId, updater)`；`patchPanelForReload(...)`（在 updater 之上 `requestRevision: ts.requestRevision + 1`，**所有翻页 / 筛选 / 排序动作都走它**）；`commitFetchedPage(...)`（校验 `requestRevision` 与 `loadingRevision` 后整体替换 slice，并**把 `selectedRows` 重置为空集、`editingCell` 置空**）；`loadTableData`；`reloadPanel`；`removePanel`；`invalidateCachedData`（同样清空 `selectedRows`/`editBuffer`/`editingCell` 并递增 `requestRevision`）；`reset` |
| 同上 | 动作：`setPage`、`setPageSize`、`addFilter`、`setFilters`、`updateFilter`、`setFilterLogic`、`removeFilter`、`clearFilters`、`applyFilters`、`setFilterPanelOpen`、`setVisibleColumns`、`setSort`、`setDetailRow`、`startEdit`、`cancelEdit`、`selectRow`、`toggleSelectAll`、`stageCellChange`、`applyColumnToRows`、`stageRowDelete`、`deleteRows`、`rollbackPendingChanges`、`previewPendingChanges`、`commitPendingChanges` |
| 同上 | `selectRow(panelId, index, opts)` 的既有语义：`range && lastSelectedIndex !== null` → 把 `[lo, hi]` 全部加进 `selectedRows`；`multi` → 切换单行；**普通单击且该行已是唯一选中行 → 清空选择并把 `lastSelectedIndex` 置 null**；否则 `new Set([index])` |
| 同上 | `toggleSelectAll`：全选中则清空，否则把 `0..rows.length-1` 全加入；`lastSelectedIndex` 置 null |
| 同上 | `setVisibleColumns(panelId, columns)` **只写 `visibleColumns`**，不触碰任何选择状态 |
| 同上 | `stageCellChange` / `stageRowDelete` **完全不触碰 `selectedRows`**（今天就是这样，本册保持） |
| 同上 | DEV 下 `window.__tableDataStore = useTableDataStore`（E2E 直接用 store API 的既有入口） |

### 2.3 右键菜单、事件反查与坐标

| 文件 | 已核实的符号 / 事实 |
| --- | --- |
| `src/lib/dataTableContextMenu.ts` | `resolveDataTableCellFromEvent(target)`：`target.closest('[data-dt-row][data-dt-col]')` 反查，校验 `Number.isInteger(rowIndex) && rowIndex >= 0`，返回 `{ rowIndex, columnName } \| null` |
| 同上 | `resolveDataTableHeaderColFromEvent(target)`：`target.closest('[data-col-header]')` |
| 同上 | `buildDataTableContextMenuItems(args)`：三种布局分支（表头 / 单元格且非多行选区 / 选区作用域），条目 id 为 `copy`、`copy-row`、`filter-by-value`、`delete-row`、`export`、`more-actions`（子菜单含 `copy-as-json` / `copy-as-sql-insert` / `copy-as-update` / `copy-as-csv` / `copy-column-name` / `copy-column-data` / `set-null` / `copy-selected-rows`）；`item()` 在 `action` 为 `undefined` 时返回 `null`（**"有 handler 才出现"的既有范式**） |
| 同上 | `serializeDataTableRowsAsTsv`、`serializeDataTableRowsAsCsv`、`serializeDataTableRowsAsJson`、`rowToNamedRecord`、`serializeDataTableColumnValues`、`formatRowAsSqlInsert`、`formatRowAsSqlUpdate`、`formatRowAsSqlDelete`、`formatRowsAsSqlInsert`、`formatRowsAsSqlUpdate` |
| `src/hooks/useColumnResize.ts` | `useColumnResize({ count, columns, rows })` → `{ columnWidths, onResizeStart }`；`computeInitialColumnWidths`（前 20 行采样 + 类型宽度 + 表头宽度，夹在 60~400）；`adjustWidthsForSort`（给排序列加 `SORT_ICON_WIDTH`）；`onResizeStart(colIndex, startX)` 用 `document` 级 `pointermove`/`pointerup` 监听并临时设置 `document.body.style.userSelect = 'none'` —— **只按列下标改宽度，不改变列序** |
| `src/hooks/useVirtualTable.ts` | `useVirtualTable({ rows, rowHeight, overscan, scrollElement })` → `{ virtualRows, totalHeight, scrollToRow }`（`@tanstack/react-virtual`；`scrollToRow = virtualizer.scrollToIndex`） |
| `src/lib/dataTypeColors.ts` | `classifyDataType(dataType)` → `DataTypeFamily = 'null' \| 'bool' \| 'number' \| 'datetime' \| 'json' \| 'binary' \| 'text'`（**纯类型族归一，与具体数据库名无关**）；`dataTypeTextClass` |
| `src/lib/tid.ts` | `tid`（从 `@datazen/ui` 再导出） |

### 2.4 键盘、焦点与编辑器

| 文件 | 已核实的符号 / 事实 |
| --- | --- |
| `src/hooks/useKeyboardShortcuts.ts` | `ShortcutDef { key, scope: 'global' \| 'editor' \| 'table', action, description }`；`useKeyboardShortcuts(shortcuts)`；内部 `matchShortcut` 用 `e.key.toLowerCase()` 比较且**修饰键精确匹配**；`scope === 'table'` 在焦点位于 `input`/`textarea`/`contentEditable` 时被 `continue` 跳过；`'editor'` 作用域只在 `inField === true` 时可用；命中即 `preventDefault()` 并 `return`（**注册顺序即优先级**） |
| 同上 | **Table 域今天没有任何快捷键注册进它**（`TableView.tsx` 里搜不到 `useKeyboardShortcuts`；`Ctrl/Cmd+A`、`Ctrl/Cmd+C`、`Enter`、`Tab`、方向键**都没有实现**） |
| `src/components/DataTable/EditableCell.tsx` | `EditableCell`；`toEditString`；`coerceAndCommit(raw)`（`raw === initial.current` → `onCancel()`；`raw === ''` → `onCommit(null)`；`int`/`serial`/`bigint` → `Number`；`bool` → `raw === 'true'`；浮点族 → `Number`；`json` → `JSON.parse` 失败则提交原文）；`handleCancel`；input 带 `data-testid="table-edit-input"`、`autoFocus`、`onBlur={() => coerceAndCommit(local)}`；`onKeyDown` 里 `Enter` → 提交、`Escape` → 取消，**两个分支都没有 `isComposing` 守卫**（`TableView.tsx` 的快速筛选输入框对 `Enter` 写了 `!event.nativeEvent.isComposing`，是项目里已有的正确参照；本册不修此缺陷，归 05） |
| `src/components/DataTable/CellRenderer.tsx` | `CellRenderer`；`CellRendererProps { columnName, dataType?, value, isEditing, onCommit, onCancel }`；`isEditing` 为真时渲染 `EditableCell`；文本与 JSON 分支截断 120 字；空值渲染斜体 `NULL` |
| `src/hooks/useAutoScroll.ts` | `useAutoScroll` / `useTrackUnread`：**语义是"聊天面板贴底 + 未读计数"**（`atBottom`/`unreadCount`/`jumpToBottom`），**不可复用**于网格拖拽自动滚动 |
| `src/hooks/useVirtualTable.ts` | 已见 2.3；`scrollToRow` 只能在 `useVirtualTable` 的消费方（`VirtualBody`）内部使用，**当前 `VirtualBody` 没有把它透出给任何调用方** |

### 2.5 宿主接线（谁在消费 `DataTable`）

| 文件 | 已核实的符号 / 事实 |
| --- | --- |
| `src/windows/connection/TableView.tsx` | `isConnectionReadOnly`、`const isEditable = !isConnectionReadOnly`；`driverReadOnly` 来自 `DB_REGISTRY[databaseType]?.readOnly === true`；`showReadOnlyTip` / `readOnlyTipVisible`（3 秒自动隐藏，渲染 `data-testid="table-read-only-tip"` 并显示 `t('tableData.readOnlyEditDisabled')`） |
| 同上 | `handleCellDoubleClick`：`!isEditable` → `showReadOnlyTip()`；否则 `actions.startEdit(row, col)`。`handleCellEdit`：`!isEditable` → 提示 + `actions.cancelEdit()`；否则 `actions.stageCellChange(row, col, value)` |
| 同上 | `handleDeleteRows(rowIndices)`：`!isEditable` 直接返回；`confirmOnDelete` 时用 `confirmDelete({ message: t('tableData.confirmDeleteRows', { count: rowIndices.length }) })` 二次确认 |
| 同上 | `handleTableKeyDown`：只处理 `(metaKey \|\| ctrlKey) && key === 'Enter'` → 提交暂存改动 |
| 同上 | `displayedColumns = useMemo(...)`：`visibleColumns === null` → 全部列；否则 `columns.filter((c) => set.has(c.name))` —— **过滤保持 `TableState.columns` 的原序，`visibleColumns` 不是重排指令**；`columnDefs: ColumnDef[] = displayedColumns.map((c) => ({ id: c.name, name: c.name, type: c.dataType }))`；`rowArrays = rows.map((record) => displayedColumns.map((col) => record[col.name] ?? null))` |
| 同上 | `<DataTable …>` 装配：`editingCell={!isEditable ? null : editingCell}`、`selectedRows`、`onRowSelect={actions.selectRow}`、`onSelectAll={actions.toggleSelectAll}`、`onRowClick={actions.setDetailRow}`、`highlightedRow={detailRowIndex}`、`primaryKeyColumns={columns.filter((c) => c.isPrimaryKey).map((c) => c.name)}`、`onDeleteRows={isEditable ? handleDeleteRows : undefined}`、`headerActions={<TableColumnFilter …/>}`、`emptyPlaceholder={…all-columns-hidden…}` |
| 同上 | 暂存改动条与事务控件全部内联在 `TableView` 里：`data-testid` 为 `pending-changes-bar`、`pending-preview`、`pending-commit`、`pending-rollback`、`table-tx-controls`、`table-tx-begin`、`table-tx-commit`、`table-tx-rollback`（**`e2e/specs/table-edit.ts` 与 `e2e/specs/detail-panel.ts` 直接依赖这些 testid**） |
| `src/windows/connection/result-workspace/ResultTableView.tsx` | 只读结果网格：`<DataTable …>` **不传** `selectedRows` / `onRowSelect` / `onSelectAll` / `onDeleteRows`（因此 `hasSelection === false`），`onCellEdit={(_row, _col, _value) => setEditingCell(null)}`、`enableSetNull={false}` |
| 其他消费者 | `src/components/ai/ExplainPanel.tsx`、`src/windows/connection/ProcessListView.tsx`、`src/windows/dashboard/RunHistoryDrawer.tsx`、`src/windows/dashboard/ChartWidgetTile.tsx`、`src/windows/workflow/WorkflowPage.tsx` 也渲染 `DataTable`（多为只读展示，**本册新增 props 必须全部可选，它们一行都不用改**） |

### 2.6 测试与 i18n 落点

| 文件 | 已核实的符号 / 事实 |
| --- | --- |
| `src/components/DataTable/__tests__/VirtualBody.test.tsx` | 用 `vi.mock` 替换 `useI18n`（`t: (key) => key`）与 `useVirtualTable`（自造 `virtualRows`）；靠 `container.querySelector('[tabindex="0"]')` 找行；断言 `onRowSelect` 收到 `{ multi: true, range: false }`；断言 `container.querySelector('button[title="dataTable.selectRow"]')` |
| `src/components/DataTable/__tests__/DataTable.test.tsx` | mock `useColumnResize`、`useVirtualTable`、`nativeContextMenu`、`commands/file`；用 `container.querySelector('[data-dt-row="0"][data-dt-col="name"]')` 定位单元格；**逐条断言右键菜单的条目 id 序列**（如 `['copy', 'copy-row', 'filter-by-value', 'export']`）；断言 `data-table-delete-rows` 按钮与 `Delete` 键 |
| `src/components/DataTable/__tests__/TableHeader.test.tsx` | 断言排序图标数量与双击表头触发 `onSort` |
| `src/stores/__tests__/tableDataStore.test.ts` | `vi.mock('../../commands/database')`；`sampleColumns` / `sampleResponse` / `deferred<T>()` / `samplePlan` 的自造夹具风格 |
| `src/stores/tableData/__tests__/connectionState.test.ts` | 对 `emptyTableState(ctx)` 做**逐字段**断言（不是整体 `toEqual`），因此新增字段不会破坏它 |
| `e2e/specs/table-batch-ops.ts` | `TC-TABLE-009` ~ `TC-TABLE-014`；用 `browser.execute` 派发 `MouseEvent`，用 `captureJourneyStep(label)` 记录步骤；`TC-TABLE-013` 会 `querySelector('table, [role="grid"]')` 后 `.focus()` 再按 `Meta+a`（**今天既没有 `role="grid"` 也没有 `Ctrl/Cmd+A` 处理**，该用例实际未断言任何选中结果） |
| `e2e/specs/table-edit.ts` | 头部注释明确写了 WKWebView 的键盘输入限制与「用 `window.__tableDataStore` 直接改状态 + 用 `dispatchEvent` 走 React 根监听」的既有混合策略；helpers 提供 `doubleClickCellByText` / `waitForEditInput` / `setSafeMode` / `clickTableInSidebar` |
| `src/locales/en.ts` | **只有一行聚合再导出**（`export { default } from './en/index'`），改它无效 |
| `src/locales/en/index.ts` | 把领域包 `core`/`connection`/`schema`/`query`/`settings`/`chart`/`backup`/`ai`/`sync`/`workflows`/`dashboard`/`mcp`/`onboarding` 展开合并 |
| `src/locales/en/query.ts` | **`dataTable.*` 与 `tableData.*` 两个命名空间的词条都在这里**（如 `'dataTable.selected'`、`'dataTable.selectRow'`、`'dataTable.deleteRow'`、`'dataTable.tableLabel'`、`'tableData.readOnlyEditDisabled'`）；key 是**扁平点号字符串**，不是嵌套对象 |
| `src/locales/en/core.ts` | `common.*` 命名空间在这里（`'common.copy'`、`'common.rows'`、`'common.selectAll'`） |
| `src/locales/index.ts` | `I18nKey = TranslationKey \| (string & {})`；`TranslationKey = keyof typeof zhCN`（`src/locales/zh-CN/index.ts`）—— 因此**只在英文侧新增 key 不会让 `t()` 报类型错**，仓库现状已有 221 个 en-only key |
| `src/locales/t.ts` | 组件外可用的 `t(key, params?)`；插值形如 `{count}` |

---

## 3. 数据结构与接口设计

本册以 TS 为主。**契约冻结的类型逐字来自 00-contracts 第 4.1 节，不得改写。**

### 3.1 契约类型（位置 `src/stores/tableData/gridSelection.ts`，本册新建此文件）

```ts
// —— 以下四个类型是 00-contracts §4.1 的冻结内容，逐字落地，禁止增删字段 ——
export interface CellCoord {
  /** 当前页内的行下标（0 基）。仅用于定位渲染，禁止用于拼 SQL。 */
  rowIndex: number;
  /** 列名，而不是列下标：列下标会随 visibleColumns / 列拖拽失去稳定含义。 */
  columnName: string;
}

export interface CellRange {
  /** 区域的两个角，顺序无意义（由工具函数规范化）。 */
  start: CellCoord;
  end: CellCoord;
}

export interface GridSelection {
  /** 区域选择的锚点（按下鼠标 / 开始 Shift 连选时确定）。 */
  anchor: CellCoord | null;
  /** 区域的另一端，随鼠标拖拽或 Shift+方向键移动。 */
  focus: CellCoord | null;
  /** 额外的非连续区域（Cmd/Ctrl+点击产生）。 */
  extraRanges: CellRange[];
  /** 该选择的交互模式，决定渲染与后续操作语义。 */
  mode: 'none' | 'cell' | 'row';
}
```

### 3.2 逐字段说明、缺省值与不变量

| 字段 | 用途 | 缺省值 | 约束 |
| --- | --- | --- | --- |
| `CellCoord.rowIndex` | 定位当前页内的行；渲染判定与键盘移动的唯一行坐标 | — | 必须是 `≥ 0` 的整数，且 `< rows.length`；**永不出现在任何 SQL / `WHERE` 里** |
| `CellCoord.columnName` | 定位列；区域比较与跨分册引用的唯一列坐标 | — | 必须是当前渲染列集合（`displayedColumns`）中的列名；**不得**存列下标 |
| `GridSelection.anchor` | 区域的固定角；`Shift+单击` / `Shift+方向键` / 拖拽都从它出发 | `null` | 只在 `mode === 'cell'` 时非 `null` |
| `GridSelection.focus` | 区域的移动角；渲染高亮与"当前单元格"的判定依据 | `null` | 只在 `mode === 'cell'` 时非 `null`；与 `anchor` 一起构成主区 `{ start: anchor, end: focus }` |
| `GridSelection.extraRanges` | `Cmd/Ctrl+点击` 产生的非连续区域；填充后主区仍在 | `[]` | 每个元素都必须已过 `normalizeCellRange`；允许与主区重叠（消费方用 `Set` 去重）；只在 `mode === 'cell'` 时可能非空 |
| `GridSelection.mode` | 渲染与语义的开关；`row` 决定"整行高亮来自 `selectedRows`" | `'none'` | 三态互斥；跃迁表见第 4 节 |

**不变量（必须写成单元测试）**

| 编号 | 不变量 |
| --- | --- |
| I1 | `mode === 'none'` ⇒ `anchor === null && focus === null && extraRanges.length === 0` |
| I2 | `mode === 'row'` ⇒ `anchor === null && focus === null && extraRanges.length === 0`（**`row` 模式的整行高亮只来自 `selectedRows`，与 `anchor`/`focus` 无关**；`selectedRows` 允许为空集——例如"点击已选中的唯一行"会清空它，但模式仍是 `row`） |
| I3 | `mode === 'cell'` ⇒ `anchor !== null && focus !== null` |
| I4 | 所有 `CellRange` 在进入状态前都必须经过 `normalizeCellRange`：`start.rowIndex ≤ end.rowIndex`，且 `start.columnName` 在 `columnOrder` 中的下标 `≤ end.columnName` 的下标（**用列序而非列名字典序**） |
| I5 | 每个 `CellCoord.columnName` 都能在当前 `columnOrder` 中查到；列被隐藏时由 `reconcileGridSelection` 收敛（见第 7 节） |
| I6 | `mode === 'cell'` 时 `TableState.selectedRows` 必须为空集（契约 §4.2 第 2 条）；`mode === 'row'` 时 `selectedRows` / `lastSelectedIndex` 的写入**只允许**经既有 `selectRow` / `toggleSelectAll` |
| I7 | `GridSelection` 只由 store 写入；`DataTable` / `VirtualBody` / `GridCell` 一律**只读消费**，不自持副本、不从 DOM 反推 |

### 3.3 `gridSelection.ts` 的纯函数（全部**新增**）

```ts
/** 冻结的空选择常量：让 memo 与依赖数组稳定（禁止各处 `{ mode: 'none' }` 字面量）。 */
export const EMPTY_GRID_SELECTION: GridSelection;

/** 规范化区域：行列都排序到"小→大"（列比较用 columnOrder 下标）。 */
export function normalizeCellRange(range: CellRange, columnOrder: readonly string[]): CellRange;

/** 把 anchor/focus 组成的主区规范化；任一角为 null 时返回 null。 */
export function normalizeSelectionRange(selection: GridSelection, columnOrder: readonly string[]): CellRange | null;

/** 选区包含的全部区域（主区在前，附加区在后），均已规范化。 */
export function selectionRanges(selection: GridSelection, columnOrder: readonly string[]): CellRange[];

/** 某坐标是否落在选区内的任意区域。 */
export function cellRangeContains(
  range: CellRange,
  coord: CellCoord,
  columnOrder: readonly string[],
): boolean;
export function gridSelectionContains(
  selection: GridSelection,
  coord: CellCoord,
  columnOrder: readonly string[],
): boolean;

/** 选区覆盖的"单元格矩形"总面积（主区面积 + 各附加区面积，不扣重叠）。用于 `data-dt-selection-cells` 与埋点。 */
export function gridSelectionCellCount(selection: GridSelection, columnOrder: readonly string[]): number;

/** 选区覆盖的列数（主区 + 附加区并集，按 columnOrder 计数）——工具栏 "N 行 × M 列" 的 M。 */
export function columnsCoveredBySelection(selection: GridSelection, columnOrder: readonly string[]): number;

/**
 * 选区覆盖的行下标集合（去重，无序保证，调用方不得依赖顺序）——工具栏 "N 行" 的 N，
 * 以及"行级批量动作降级为涉及的行集合"的唯一来源。
 * ⚠ 返回的是 UI 行下标，任何写路径都必须再经 `rowIdentityAnchors` 解析（契约 §4.3）。
 *
 * ⚠ 签名已由契约 §4.2.1 冻结为**单参数 + ReadonlySet**：不得加 `columnOrder`
 *   （本函数求的是行覆盖集，与列序无关），不得返回 `number[]`
 *   （03 的既有断言写作 `.size > 0`，数组无 `.size`，typecheck 会失败）。
 *   `mode === 'row'` 时直接返回 `state.selectedRows` 本身，不拷贝。
 */
export function rowsCoveredBySelection(state: GridSelection): ReadonlySet<number>;

/** 精确相等（含模式、锚点、焦点、附加区顺序无关比较）。用于避免无意义 store 写入。 */
export function gridSelectionEquals(
  a: GridSelection,
  b: GridSelection,
  columnOrder: readonly string[],
): boolean;
export function cellCoordEquals(a: CellCoord | null, b: CellCoord | null): boolean;

/** 夹取到合法网格范围；列不存在或行越界时返回 null。 */
export function clampCellCoord(
  coord: CellCoord,
  rowCount: number,
  columnOrder: readonly string[],
): CellCoord | null;

/** 列名 → 列下标（`columnOrder` 顺序）；查不到返回 -1。 */
export function columnIndexFromCoord(coord: CellCoord, columnOrder: readonly string[]): number;

/** 行下标 + 列下标 → 坐标；任一越界返回 null。 */
export function coordAt(rowIndex: number, columnIndex: number, columnOrder: readonly string[]): CellCoord | null;

/** 焦点移动（DB-02 用的地基建在本册）：行列偏移 + 夹取，返回 null 表示无处可去。 */
export function moveCellFocus(
  focus: CellCoord,
  deltaRow: number,
  deltaColumn: number,
  rowCount: number,
  columnOrder: readonly string[],
): CellCoord | null;

/** 把 anchor 与 focus 组成的选择（可选 keepExtra）归一为新的 GridSelection。 */
export function selectionFromAnchorFocus(
  anchor: CellCoord,
  focus: CellCoord,
  mode: 'cell' | 'row',
  extraRanges?: CellRange[],
): GridSelection;

/** 追加 / 移除一个非连续单格区域（Cmd/Ctrl+点击；同格再点一次即取消）。 */
export function toggleExtraRange(
  selection: GridSelection,
  coord: CellCoord,
  columnOrder: readonly string[],
): GridSelection;

/**
 * 列显隐 / 行集变化后的收敛：
 * - 行下标越界或列名已不在 columnOrder 中 → 该角失效；
 * - 主区任一角失效 → 整个选择退回 EMPTY_GRID_SELECTION（mode 变 none）；
 * - 附加区任一角失效 → 丢弃该区域；
 * - 只保留仍有效的部分，且结果必须重新满足 I1~I5。
 */
export function reconcileGridSelection(
  selection: GridSelection,
  rowCount: number,
  columnOrder: readonly string[],
): GridSelection;
```

> **为什么把纯函数和类型放在 `src/stores/tableData/gridSelection.ts`**：00-contracts §4.1 已把该路径定为契约位置。它**不得 import React、不得 import store**，以便 `vitest` 直接单测；React 层（事件翻译）另放 `src/hooks/useGridSelection.ts`（见 3.6），两者职责不同。

### 3.4 `TableState` 增量（`src/stores/tableData/types.ts`）

```ts
export interface TableState {
  // …既有字段全部保持不变…
  /** 单元格/区域选择。唯一真相在 store；`none` 为缺省。 */
  gridSelection: GridSelection;   // 新增
}
```

| 字段 | 用途 | 缺省值 | 不变量 |
| --- | --- | --- | --- |
| `gridSelection` | 选择模型唯一真相，跨虚拟滚动存活 | `EMPTY_GRID_SELECTION`（由 `emptyTableState` 写入） | 每次"重新取数"（翻页 / 改页大小 / 筛选应用 / 清除筛选 / 排序 / 失效缓存 / 提交后刷新）都必须回到 `EMPTY_GRID_SELECTION`；`selectedRows` 保持不变（也一并清空，沿用今天的 `commitFetchedPage` 行为） |

### 3.5 store 动作增量（`src/stores/tableDataStore.ts` 接口 + `src/stores/tableData/selectionActions.ts` 实现）

```ts
// —— TableDataStore 接口新增（4 个）——
/** 直接写入一个已算好的选择（纯函数在 hook 里算，store 只落状态）。 */
setGridSelection: (panelId: string, selection: GridSelection) => void;
/** 清空选择（mode → none）。Escape / 点击空白 / 列显隐收敛统一走它。 */
clearGridSelection: (panelId: string) => void;
/** 单元格模式下点单元格：进入 cell 模式并清空 selectedRows（契约 §4.2 第 2 条）。 */
beginCellSelection: (panelId: string, coord: CellCoord) => void;
/** 列显隐变化后收敛选择（内部调 reconcileGridSelection）。 */
reconcileGridSelectionForColumns: (panelId: string, columnOrder: readonly string[]) => void;
```

```ts
// —— src/stores/tableData/selectionActions.ts（新增）——
// 纯状态转移：输入 TableState（或其相关切片）+ 参数，输出 Partial<TableState>。
// 目的：把选择逻辑从已 722 行的 tableDataStore.ts 里挪出来，给 02/03/05/06 留出余量。

export interface SelectionTransitionDeps {
  /** 当前渲染列顺序（= displayedColumns 的列名数组）。 */
  columnOrder: readonly string[];
  rowCount: number;
}

/** 既有 selectRow 的纯实现（整行选择：range / multi / 单击已选中行 = 清空）。 */
export function applyRowSelect(
  ts: TableState,
  index: number,
  opts: { multi?: boolean; range?: boolean } | undefined,
): Partial<TableState>;

/** 既有 toggleSelectAll 的纯实现。 */
export function applyToggleSelectAll(ts: TableState): Partial<TableState>;

/** 新增：进入 cell 模式（同时清空 selectedRows 与 lastSelectedIndex）。 */
export function applyBeginCellSelection(ts: TableState, coord: CellCoord): Partial<TableState>;

/** 新增：整行槽点击 → row 模式（清 anchor/focus/extraRanges，整行集仍由 applyRowSelect 决定）。 */
export function applyRowModeTransition(ts: TableState, index: number, opts?: { multi?: boolean; range?: boolean }): Partial<TableState>;

/** 新增：写入选择（mode 为 cell 时顺带保证 selectedRows 为空）。 */
export function applyGridSelection(ts: TableState, selection: GridSelection): Partial<TableState>;

/** 新增：清空选择。 */
export function applyClearGridSelection(ts: TableState): Partial<TableState>;

/** 新增：列显隐收敛。 */
export function applyReconcileSelection(ts: TableState, columnOrder: readonly string[]): Partial<TableState>;

/**
 * 新增：**重新取数时的统一重置对象**——把「按 rowIndex 记账的全部选择状态」一次打包，
 * 供三个收敛点（`patchPanelForReload` / `commitFetchedPage` / `invalidateCachedData`）各自 `...reset()`。
 * 返回值固定为 `{ gridSelection: EMPTY_GRID_SELECTION, selectedRows: new Set(), lastSelectedIndex: null }`。
 * 为什么要做成函数而不是三处各写一遍：只要三处有一处漏写一项，缺陷就会重现；
 * 让三处引用**同一个**对象是从结构上消灭"漏写一项"的可能（契约 §4.3 推论）。
 */
export function rowIndexSelectionReset(): Partial<TableState>;
```

> `TableDataStore` 的 `selectRow` / `toggleSelectAll` **签名与语义完全不变**，只是实现改为调用 `applyRowSelect` / `applyToggleSelectAll`。这样 `src/stores/__tests__/tableDataStore.test.ts` 与 `e2e/specs/table-batch-ops.ts` 一行都不用改。

### 3.6 React 层（`src/hooks/useGridSelection.ts`，**新增**）

```ts
export interface UseGridSelectionOptions {
  /** 当前渲染列顺序（列名数组，通常来自 `DataTable` 的 `columns`）。 */
  columnOrder: readonly string[];
  rowCount: number;
  /** 受控值：唯一真相来自 store，本 hook 不持有副本。 */
  selection: GridSelection;
  /** 选区变更回调；本 hook 只在"一次交互结束"时调用一次（拖拽期间不写 store）。 */
  onChange: (next: GridSelection) => void;
  /** 网格滚动容器；用于边缘自动滚动与空白区判定。 */
  scrollElement: HTMLElement | null;
  /** 行号槽被点击时转交给既有整行选择路径（缺省 undefined = 不提供整行入口）。 */
  onRowGutterSelect?: (rowIndex: number, opts: { multi: boolean; range: boolean }) => void;
  /** 是否启用单元格选择；缺省 true。只读网格仍应为 true（只读 ≠ 不可选）。 */
  enabled?: boolean;
  /** 编辑中（`editingCell !== null`）时不处理 Escape，避免与编辑器取消冲突。 */
  editingCell: { row: number; col: string } | null;
}

export interface UseGridSelectionResult {
  /** 直接展开到网格滚动容器上：`<div ref={setScrollEl} {...surfaceProps} />`。 */
  surfaceProps: {
    onPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void;
    onPointerMove: (e: React.PointerEvent<HTMLDivElement>) => void;
    onPointerUp: (e: React.PointerEvent<HTMLDivElement>) => void;
    onPointerCancel: (e: React.PointerEvent<HTMLDivElement>) => void;
    onKeyDown: (e: React.KeyboardEvent<HTMLDivElement>) => void;
    onFocus: () => void;
    onBlur: () => void;
  };
  /** 是否正在拖拽（供 UI 加 `select-none` 与 E2E 判定残缺中间态）。 */
  dragging: boolean;
  /** 渲染判定：某格是否落在当前选择内（走纯函数，不查 DOM）。 */
  isCellSelected: (rowIndex: number, columnName: string) => boolean;
  /** `aria-activedescendant` 用的焦点单元格 id（`useId()` 前缀 + 行列下标）。 */
  activeDescendantId: string | null;
  /** 焦点单元格的滚动可见性请求（虚拟滚动下 02 会复用）。 */
  focusKey: string | null;
}
```

### 3.7 DOM 数据属性契约（`GridCell` 负责输出；契约 §4.4 + 本册新增两条）

| 属性 | 值 | 语义 | 归属 |
| --- | --- | --- | --- |
| `data-dt-row` | 行下标 | 既有；`resolveDataTableCellFromEvent` 与既有测试依赖 | 既有（**必须在最外层单元格元素上，不得下移**） |
| `data-dt-col` | 列名 | 既有；同上 | 既有（同前） |
| `data-testid="data-table-cell"` | — | 既有；`DataTable.test.tsx` 依赖 | 既有 |
| `data-dt-selected` | `"true"` / **不输出** | 该格处于当前选择内 | 本册新增（契约 §4.4） |
| `data-dt-editing` | `"true"` / **不输出** | 该格正被编辑器占用 | 本册新增（契约 §4.4） |
| `data-dt-cell-type` | 本册：`classifyDataType(col.type)`（既有函数，7 个类型族）；**05 合并后**：同一属性的来源升级为 `resolveCellType(col.type)`（`CellType`，11 个值） | 归一化类型名。**收敛口径见下方裁定** | 本册新增（契约 §4.4） |
| `data-dt-dirty` | `"true"` / 不输出 | 该格有未提交改动 | **不属本册**，由 06 在同一个 `GridCell` 里加 |
| `data-dt-surface` | `"true"` | 标记虚拟滚动 filler（"空白处"判定的唯一合法依据） | 本册新增 |
| `data-dt-selection-rows` / `data-dt-selection-cols` / `data-dt-selection-cells` | 数字 | 工具栏摘要的三个计数，供 E2E 逐字断言 | 本册新增 |

### 3.8 `DataTableProps` 增量（全部可选 ⇒ 既有 7 处消费者一行不改）

```ts
export interface DataTableProps {
  // …既有 props 全部保持不变（含死 prop `editBuffer`，本册不动它）…

  /** 单元格/区域选择（受控）。不传 = 不启用单元格选择，行为与今天完全一致。 */
  gridSelection?: GridSelection;                              // 新增
  /** 选区变更回调。与 `gridSelection` 成对出现。 */
  onGridSelectionChange?: (next: GridSelection) => void;       // 新增
  /** 复制选区（DB-03 接线）；提供后右键菜单才出现 `copy-selected-cells`。 */
  onCopySelectedCells?: (selection: GridSelection) => void;    // 新增
}
```

| prop | 用途 | 缺省 | 兼容性 |
| --- | --- | --- | --- |
| `gridSelection` | 受控选区 | `undefined` | 不传 ⇒ `hasCellSelection === false`，不渲染任何选区 DOM 属性、不注册指针处理，**逐位等于今天** |
| `onGridSelectionChange` | 选区回写 | `undefined` | 同上；`gridSelection` 传了但回调缺失时只读渲染（可用于纯展示） |
| `onCopySelectedCells` | 03 的挂点 | `undefined` | `undefined` ⇒ 菜单不出现该条目（沿用 `buildDataTableContextMenuItems` 的 `item()` 范式） |

### 3.9 `GridCell`（`src/components/DataTable/GridCell.tsx`，**新增**）

```ts
export interface GridCellProps {
  rowIndex: number;
  column: ColumnDef;
  value: unknown;
  width: number;
  selected: boolean;
  editing: boolean;
  /** 稳定回调（来自 props 透传，不在渲染里新建箭头函数）以保持 `memo` 有效。 */
  onCellDoubleClick: (rowIndex: number, columnName: string) => void;
  onCellEdit: (rowIndex: number, columnName: string, value: unknown) => void;
  onCellEditCancel: () => void;
}
export const GridCell: React.MemoExoticComponent<(props: GridCellProps) => JSX.Element>;
```

### 3.10 事件守卫（`src/lib/gridEventGuards.ts`，**新增**）

```ts
/** 兼容 DOM 原生事件与 React 合成事件（后者把标志放在 `nativeEvent` 上）。 */
export function isComposingEvent(event: {
  isComposing?: boolean;
  nativeEvent?: { isComposing?: boolean };
}): boolean;

/** 与 `useKeyboardShortcuts` 的 `inField` 判定保持同一语义：input / textarea / contentEditable。 */
export function isEditableTarget(target: EventTarget | null): boolean;
```

> **为什么单独抽出来**：01（Escape 与指针守卫）、02（方向键）、03（粘贴）、05（编辑器内 Enter）都需要同一份判定；重复写三份必然漂移。全部**无 `any`**。

### 3.11 后端契约影响

**零。** 本册不新增 / 不修改任何 `driver-api` trait 方法、DTO、IPC 命令；不涉及 `CellWrite`；不改 `PROTOCOL_VERSION`。数据库之间的差异与本册无关（选择是纯 UI 概念），因此**不存在**任何 `if (dbType === 'mysql')` 式的宿主分支需要写——这正是"零硬编码"在本册的表现形式。

---

## 4. 交互与状态机

### 4.1 三模式总表（契约 §4.2 的落地细则）

| 模式 | 进入条件 | 状态内行为 | 退出跃迁 |
| --- | --- | --- | --- |
| `none` | 初始；按 `Escape`；点击空白处（`data-dt-surface` 或滚动容器自身）；列显隐收敛后失效；任何重新取数 | 无高亮；`anchor`/`focus`/`extraRanges` 全空（I1）；`selectedRows` 可为空集 | 单击单元格 → `cell`；点击行号槽 → `row` |
| `cell` | 单击单元格（`pointerdown` + `pointerup` 落在同一格）；02 的方向键 | `anchor`/`focus` 跟踪；`Shift+单击`/`Shift+方向键` 从 `anchor` 扩区；`Cmd/Ctrl+单击` 追加/移除 `extraRanges`；`selectedRows` **保持空集**（I6） | `Escape` → `none`（编辑中例外，见 4.5）；点击行号槽 → `row`；空白处 → `none`；列显隐失效 → `none` |
| `row` | 点击行号槽；点击已选中的行号槽 | 整行高亮**只**来自 `selectedRows`；`anchor`/`focus`/`extraRanges` 恒为空（I2）；`selectedRows` 语义沿用既有 `selectRow`（含"单击已选中的唯一行 = 清空"，此时模式仍是 `row`） | `Escape` → `none`；点击单元格 → `cell`（并清空 `selectedRows`） |

### 4.2 鼠标

| 手势 | 判定 | 结果 |
| --- | --- | --- |
| 单击单元格（无修饰键） | `pointerdown` 目标可反查出 `{rowIndex, columnName}` | `anchor = focus = 该格`，`extraRanges = []`，`mode = 'cell'`，`selectedRows` 清空；容器取焦点（`scrollEl.focus()`） |
| 单击单元格（已有 `anchor`，无修饰键） | 同上 | **重设主区**为该格（不是扩区；扩区必须用 `Shift`） |
| `Shift+单击` | `shiftKey === true` 且 `anchor !== null` | 主区变为 `anchor → 该格`（规范化），`extraRanges` 保留，`mode = 'cell'` |
| `Shift+单击`（`anchor === null`，残缺中间态） | 同上 | 等价于普通单击：建立新 `anchor`（**不得**丢事件） |
| `Cmd/Ctrl+单击` | `metaKey \|\| ctrlKey`（沿用既有的 `VirtualBody` 判定）且 `mode === 'cell' && anchor !== null` | 把该格作为**单格区域**在 `extraRanges` 里切换（已存在则移除，顺序无关） |
| `Cmd/Ctrl+单击`（`mode !== 'cell'` 或 `anchor === null`） | 同上 | 等价于普通单击：建立新主区（不产生"悬浮的附加区"） |
| 拖拽（按下 → 移动 → 抬起） | `pointerdown` 落在单元格、`pointermove` 时 `buttons & 1` 为真 | 拖拽期间只更新本地会话的 `focus`，**不改 store**；`pointerup` 时一次性提交 `{ anchor: 起点, focus: 终点 }`；`mode = 'cell'`，`selectedRows` 清空 |
| 拖拽起点即终点 | 未发生有效移动 | 等价于单击（`extraRanges` 清空） |
| 点击行号槽 | 目标命中行号槽 `<button>`（`e.stopPropagation()` 已由既有代码处理） | 走 `onRowGutterSelect(rowIndex, { multi, range })` → 既有 `selectRow`；同时把选择切换为 `row` 模式（清 `anchor`/`focus`/`extraRanges`） |
| 点击空白处 | `e.target === scrollElement` 或 `closest('[data-dt-surface]')` 非空，且未命中 `[data-dt-row][data-dt-col]`、未命中行号槽、未命中表头 | `mode → none`，全清；`selectedRows` **清空**（与既有行为一致：今天点空白也不选中任何行） |
| 双击单元格 | 沿用既有 `onDoubleClick` | 不改选择语义（第 1 次与第 2 次单击都落在同一格）；由既有 `onCellDoubleClick` 决定进编辑还是显示只读提示 |
| 右键（`contextmenu`） | 既有 `handleContextMenu` | **先**把"被右键的格"纳入选择：若该格不在当前选区内且没有多行选区，则把选择设为该单格（`mode = 'cell'`）再开菜单；若落在既有选区内则**不动**选区（避免右键破坏用户已拖好的区域）。**唯一例外**：`mode === 'row'` 且右键落在已选中的行上时保持 `row` 模式（今天 `multiRowSelection` 的语义） |
| 中键 / 其他按钮 | `e.button !== 0` | 不参与选择，直接忽略（Quick Look 若将来用中键，由 09/19 自己处理） |

### 4.3 键盘（本册只占两个边界，其余归 02）

| 键 | 本册是否实现 | 行为 |
| --- | --- | --- |
| `Escape` | **是** | `mode → none` 并清空全部选择；**当 `editingCell !== null` 时直接返回**（让编辑器的 Escape 先取消编辑，避免"取消编辑的同时把选区也清了"）。与既有 `handleDeleteKey` 的 `if (editingCell) return` 保持同一条纪律 |
| `Enter` / `Tab` / 方向键 / `Home` / `End` / `PgUp` / `PgDn` / `Shift+方向键` / `Cmd+Home/End` | 否（DB-02） | 本册不注册。02 只需在同一个 `surfaceProps.onKeyDown` 链上追加分支，并复用 `moveCellFocus` / `selectionFromAnchorFocus`（已在 3.3 冻结） |
| `Delete` / `Backspace` | 已存在，**语义不变** | 既有 `handleDeleteKey` 只在 `selectedRows.size > 0` 时生效。**cell 模式下 `selectedRows` 恒为空 ⇒ Delete 什么都不做**——这是刻意的（"清空单元格内容"归 02/05 裁定），必须在 journey 测试里断言，避免将来有人误把"删单元格内容"接到删行上 |
| `Cmd/Ctrl+A` | 否 | 今天没有任何实现（`e2e/specs/table-batch-ops.ts` 的 `TC-TABLE-013` 只 focus + 按 `Meta+a`，未断言结果）。本册不实现，登记在未决问题 Q8（与 02 一起裁定：`Cmd+A` 应是"整行全选"还是"选中当前页所有单元格"） |

### 4.4 焦点

- **焦点宿主是网格滚动容器**（`role="grid"` + `tabIndex={0}` + `aria-activedescendant` 指向焦点单元格的 id）。单元格**不**各自带 `tabIndex`。
- 单元格模式的任何指针交互（落在单元格上的 `pointerdown`）在设置选区的同时调 `scrollEl.focus()`；落在编辑器 input 上时不夺焦点（见 4.5）。
- 行号槽：**外层 `<div role="rowheader">`，内层 `<button type="button" tabIndex={-1}>`**。本册**把行号槽移出 Tab 序列**，因此 01 落地后网格只有行 div 一个可 Tab 停靠点；行 div 的 `tabIndex={0}` **本册仍不改**（既有单测用 `[tabindex="0"]` 找行），「行容器与 02 的滚动容器谁当焦点宿主」记入未决问题 Q1（已裁定归 02 收口）。
- **Q1 已裁定（02 收口）**：分册 02 采纳 roving tabindex，**由 02 把行容器改成 `tabIndex={-1}` 并一次性迁移定位行的那 6 处既有断言**（`VirtualBody.test.tsx` ×2、`DataTable.test.tsx` ×1、`e2e/detail-panel` ×1、`e2e/ops-process-server` ×2；定位方式改用 `[data-dt-row-view]` 或 `[data-dt-row]`）。本册**不做这次迁移**——零回归优先，且迁移与焦点模型同属 02 的职责。中间态（01 落地到 02 落地之间）确实多一个 Tab 停靠点，**已被接受**。
- **ARIA 网格角色必须原子添加（重要，勿拆）**：本册给滚动容器加 `role="grid"`、给单元格加 `role="gridcell"`，因此**必须同时**给行容器加 `role="row"`，并一起提供 `aria-rowcount` / `aria-colcount` / `aria-activedescendant`。**缺少 `role="row"` 的 `role="gridcell"` 是无效 ARIA**——屏幕阅读器会宣布这是一个网格却找不到任何行，体验**比完全不加角色更糟**。所以本册一次性加齐这一组；02 只在此之上改焦点（`tabIndex`）与键盘行为，**不新增也不删除任何 ARIA 角色**。任何分册若只想加其中一部分，必须先回到契约讨论，**禁止半套**。

#### 4.4b 行号槽的 `rowheader` 改造（契约第 4.4 节追加项，**归本册，不可推给 02**）

契约第 4.4 节已经裁定：行号槽从 `<button>` 改为 `<div role="rowheader">` + 内部**独立可聚焦**的 `<button>`。这一项是**给 `role="row"` 扫尾的**，不是独立的可访问性增强——
因此它与 `role="row"` **必须同一个提交落地**，拆开就等于交出一个无效 ARIA 中间态。

```tsx
<div role="rowheader" className="flex w-10 shrink-0 items-center border-r border-edge/30">
  <button
    type="button"
    tabIndex={-1}
    className={cn(
      'flex h-full w-full items-center justify-center text-xs text-fg-muted',
      selected && 'border-l-2 border-l-accent text-accent',
    )}
    onClick={handleSelectButtonClick}
    title={selectRowLabel}
  >
    {vRow.index + 1}
  </button>
</div>
```

**三条必须钉死的约束**：

| # | 约束 | 理由 |
| --- | --- | --- |
| R-1 | 内层按钮 `tabIndex={-1}`，**不得**保留浏览器默认可聚焦 | `role="grid"` 用 `aria-activedescendant` 管理焦点时，格内可聚焦控件会与之打架：屏幕阅读器念的是「第 3 行第 2 列」，用户 Tab 进去却落在行号槽上，读与操作分离。移出 Tab 序列后，键盘焦点只由 02 的 roving tabindex 与 `moveCellFocus` 决定 |
| R-2 | **样式类（`w-10` / `border-r` / 选中态 `border-l-2 border-l-accent text-accent`）从按钮迁到外层 `div`**，按钮补 `h-full w-full` | 否则行号槽宽度塌成内容宽、竖线错位、选中态左边框画到内层按钮上。**列宽与边框属于「格」，字号属于「格内文本」**——这条分界线是本改造最容易做坏的地方 |
| R-3 | `onClick={handleSelectButtonClick}`、`title={selectRowLabel}`、行号文本 `{vRow.index + 1}` **原样保留，不动 `e.stopPropagation()` 的语义** | 行号槽的整行选择语义由既有代码定义；本改造只改 DOM 形态与可聚焦性。若顺手把 `stopPropagation()` 删掉，行的 `onClick` 会与按钮的 `onClick` 双触发 |

**`aria-activedescendant` 与本改造的相容性**：网格永远不会把 active descendant 指到 `rowheader` 上（`aria-activedescendant` 只指向 `r{row}c{col}` 形式的单元格 id，见上条），
所以行号槽不参与焦点托管，只需满足内容模型即可。**新增 `role="rowheader"` 后不要再给它加 `aria-selected`**——行的选中态由行容器的 `data-dt-selection-rows` 与 `aria-selected` 表达，
两处都加会让屏幕阅读器把「已选中」念两次。

**为什么归 01 而不是 02**：02 只改焦点（`tabIndex`），不碰角色集合（契约第 4.4 节原话：「**02 不得**因为 K-7 只加 tabIndex 不加 role 的理由新增或删除任何角色」）。
而本改造新增了一个角色。若放在 02，`role="row"` 与 `role="rowheader"` 之间会横跨两个提交，中间态是「行容器已是 `row`、行号槽仍是裸 `<button>`」——正是本改造要消灭的那个状态。

**对 02 的影响（必须写明，否则 02 会踩）**：02 的 roving tabindex 作用于**行容器**（`data-dt-row-view`），不作用于行号槽按钮。
02 **不得**为了「让行号槽能被方向键选中」而给它加 `tabIndex={0}` 或把它纳入 roving 序列——那会同时违反 R-1 与契约第 4.4 节。
- `aria-activedescendant` 的 id 由 `useId()` 前缀 + `r{rowIndex}c{columnIndex}` 组成（**不含列名**，避免列名里的空格 / 引号产生非法 id）；E2E 一律用 `data-dt-*` 定位，不依赖 id。
- 焦点离开网格（`onBlur`）：**不清空选择**（用户可能去点工具栏的"删除行""导出"）；只有 `Escape`、空白点击、重新取数、列显隐失效才清空。
- 编辑期间焦点在 `EditableCell` 的 input 上；`blur` 会触发既有的 `coerceAndCommit` —— 因此**禁止**在 `pointerdown` 里对编辑器 input 调 `scrollEl.focus()`（会造成"点一下就把编辑提交了"的隐蔽数据问题）。

### 4.5 输入法（IME）

| 阶段 | 行为 |
| --- | --- |
| 组合期（`isComposing === true`）的按键 | **一律不处理**网格级按键（`Escape` / `Enter` / 方向键 / `Delete`）。判定统一走 `isComposingEvent(event)`；DOM 原生事件的标志在 `event.isComposing`，React 合成事件在 `event.nativeEvent.isComposing` |
| 组合期的指针 | 照常处理（点到别的格 = 扩区/重设选区）；但 `pointerdown` 落在 `isEditableTarget(e.target)` 为真的元素（编辑器 input）上时**完全忽略**，让 IME 的候选窗与输入框自己处理 |
| 组合期结束 | 由编辑器自身决定提交或取消；网格不额外触发任何状态跃迁 |
| 已知既有缺陷（本册不修，登记以便 05 处理） | `EditableCell` 的 `onKeyDown` 里 `Enter` 分支**没有** `isComposingEvent` 守卫，中文输入法用 Enter 选字会直接把候选串提交成值。`TableView` 的快速筛选输入框写了 `!event.nativeEvent.isComposing`，是本项目已有的正确参照；01 提供 `src/lib/gridEventGuards.ts` 让 05 复用同一份判定 |

### 4.6 滚动（虚拟滚动导致的行卸载）

| 场景 | 行为 |
| --- | --- |
| 行被卸载（滚出 `overscan`） | 选区**不受影响**：坐标存在 store，`GridCell` 每次渲染都从 `isCellSelected(rowIndex, columnName)` 现算。**禁止**把选中态存进行的局部 state / DOM class |
| 行被复用（`@tanstack/react-virtual` 复用 DOM 节点） | 同上；`GridCell` 的 `data-dt-selected` 完全由 props 派生 ⇒ 复用不会把旧选中态带到新行 |
| 拖拽期间自动滚动 | 指针 Y 距容器上下边缘 < 24px 时启动 `requestAnimationFrame` 循环，每次滚动固定步长（约 `rowHeight`），并在每帧重新解析光标下的单元格；松手 / `pointercancel` / 窗口失焦都结束循环 |
| 指针落在行与行之间的空隙 / 表格两侧留白 | **保留上一个有效坐标**，不得清空（这是"残缺中间态"）；指针完全离开网格区域时把 `focus` 夹到最近的边界行 |
| 指针超出最后一行（下方空白） | `focus` 夹到最后一行（`rowCount - 1`）；超出第一行同理夹到 `0` |
| 列不可见（需要横向滚动） | 拖拽时若命中列在视口外，`elementFromPoint` 取不到该格 ⇒ 同样保留上一个有效坐标；水平自动滚动**本册不做**（登记未决问题 Q6） |

> **坐标来源纪律**：拖拽期间一律用 `document.elementFromPoint(clientX, clientY)` 取回元素，再走既有的 `resolveDataTableCellFromEvent` 读 `data-dt-*`。**禁止**用 `rowHeight × 行号`、`columnWidths` 累加等几何推算反推单元格（缩放、列宽拖拽、虚拟卸载都会让它漂移）。同理，`elementFromPoint` 返回的元素**必须**是单元格或其后代，否则视为"未命中"并保留上一个有效坐标。

---

## 5. 实现步骤

> 每一步都给出：改哪个文件 → 新增/修改什么符号 → 为什么 → 怎么自测。**任何一步都不许出现 `any`、不许改既有符号签名、不许在宿主里按数据库名分支。**

### Step 1 · 纯逻辑落地：`src/stores/tableData/gridSelection.ts`（新增）

- **新增符号**：3.1 的 `CellCoord` / `CellRange` / `GridSelection`（逐字照抄契约），3.3 的全部函数与 `EMPTY_GRID_SELECTION`。
- **为什么先做**：这是 01 的地基，也是 02/03/05/06/07 的公共依赖。纯函数无 React / 无 store 依赖，可以先于任何 UI 改动写完并测透。
- **实现要点**：`columnOrder` 只在函数参数里出现（禁止从模块里 import `TableState`）；`normalizeCellRange` 的列比较必须用 `columnOrder.indexOf(...)`，查不到列时抛不掉就返回 `null` 的变体由调用方决定（本册约定：查不到列的函数一律返回"未命中"而不是猜测位置）。
- **自测**：`npx vitest run src/stores/tableData/__tests__/gridSelection.test.ts`（Step 12 写用例），先把"反向拖拽"“列序与字典序相反"“附加区与主区重叠"三个 case 跑绿。

### Step 2 · 状态字段：`src/stores/tableData/types.ts` + `connectionState.ts`

- **修改符号**：`TableState` 增加 `gridSelection: GridSelection`；`emptyTableState` 增加 `gridSelection: EMPTY_GRID_SELECTION`。
- **为什么**：`emptyTableState` 是 `TableState` 的唯一工厂（已核实：仓库里没有别处手写 `TableState` 字面量），因此只需改这一处初始化。
- **自测**：`pnpm typecheck` 干净；`npx vitest run src/stores/tableData/__tests__/connectionState.test.ts`（既有断言是逐字段的，不会破）。

### Step 3 · 选择状态转移：`src/stores/tableData/selectionActions.ts`（新增）

- **新增符号**：3.5 的 `SelectionTransitionDeps` 与 8 个 `apply*` 函数；其中 `applyRowSelect` / `applyToggleSelectAll` 是**既有实现的搬移**（语义必须逐字一致，包括"单击已选中的唯一行 = 清空 + `lastSelectedIndex = null`"）。
- **为什么**：`tableDataStore.ts` 已 722 行，直接往里加 4 个动作 + 抽离逻辑必然逼近/越过 800 行红线。把纯转移挪到独立模块，既腾出余量，又让 02/03/06 的选择相关变更集中在一个可单测文件里。
- **自测**：`npx vitest run src/stores/__tests__/tableDataStore.test.ts` 必须**一行不改就全绿**（证明搬移无行为漂移）。

### Step 4 · store 接线与"重新取数即清空"

> **⚠ 这一步同时修一个既有的数据丢失级缺陷，不是纯新增。** 详见契约 §4.3 推论。

- **修改文件**：`src/stores/tableDataStore.ts`。
- **修改符号**：
  - `selectRow` / `toggleSelectAll` 改为委托 `applyRowSelect` / `applyToggleSelectAll`；
  - 新增 `setGridSelection` / `clearGridSelection` / `beginCellSelection` / `reconcileGridSelectionForColumns` 四个薄封装（委托 `apply*`）；
  - **新增 `rowIndexSelectionReset()`（见 §3.5）作为三处收敛点唯一的重置来源**，三处各写一次 `...rowIndexSelectionReset()`：
    - `patchPanelForReload`：在并入 `updater(ts)` 的同时并入 `...rowIndexSelectionReset()`；
    - `commitFetchedPage`：把今天已有的 `selectedRows: new Set()` 换成 `...rowIndexSelectionReset()`（**并补齐今天漏掉的 `lastSelectedIndex`**）；
    - `invalidateCachedData`：同上。
  - `setVisibleColumns` 末尾追加一次 `reconcileGridSelectionForColumns`（列被隐藏后收敛，只收敛 `gridSelection`，**不动 `selectedRows`**——列显隐不改变行下标语义）。
- **为什么必须在收敛点做、而不是让六个 action 各自记得清**：`setPage` / `setPageSize` / `setSort` / `setFilters` / `clearFilters` / `applyFilters` **六个动作全部**经 `patchPanelForReload` 走到 `reloadPanel`（已逐条核实）。把这六个动作各自写一遍清空，就是"六份必须永远保持同步的复制品"——新增第七个触发重新取数的动作（04 的 INSERT 后刷新、03 的粘贴后刷新、DB-18 的 keyset 翻页）时漏写一份，缺陷立刻重现。收敛点模式把"必须同步的东西"从 6 份压到 1 份，并由 §9.7 的回归用例钉住。
- **⚠ 这一步同时修一个既有的数据丢失级缺陷，不是新功能**（契约 §4.3 推论）。今天三处的确切行为（逐条读过源码）：
  | 收敛点 | 今天是否清 `gridSelection` | 今天是否清 `selectedRows` | 今天是否清 `lastSelectedIndex` |
  | --- | --- | --- | --- |
  | `patchPanelForReload` | —（字段还不存在） | **否**（它只并入 `updater(ts)` 并把 `requestRevision` 加一） | **否** |
  | `commitFetchedPage`（响应成功落盘） | —（字段还不存在） | **是**（`selectedRows: new Set()`） | **否**（`lastSelectedIndex` 从不在此写入） |
  | `invalidateCachedData` | —（字段还不存在） | **是** | **否** |
  由此产生三类可复现的后果：
  1. **陈旧锚点**：任何一次成功重新取数后 `selectedRows` 虽被清空，`lastSelectedIndex` 仍指向上一次的数据集；用户紧接着做 `Shift+行号槽`（`selectRow(..., { range: true })`）时，锚点用的是旧页的旧下标，选出的连续区间**不是用户依据"当前空选择"能预期的区间**。
  2. **在途 / 失败窗口**：`setPage` / `setSort` / `setFilters` 会**立刻**改写 `page` / `sorts` / `filters` 并递增 `requestRevision`，但 `rows` 与选择状态只有在响应提交（`commitFetchedPage`）时才一起换；`loadTableData` 的 `catch` 分支只写 `loading` / `loadingRevision` / `error`。也就是说请求失败时，分页器与筛选器已经宣称"你在新查询/新页上"，而 `selectedRows` + `lastSelectedIndex` 却仍属于**上一个数据集**——此时任何行级批量动作（删除、导出、`applyColumnToRows`）虽然作用于"界面上仍高亮的那几行"，但用户对"我现在在哪个查询上"的判断与选择状态的来源已经不一致。
  3. **隐式耦合**：今天"重新取数会清掉行选择"这一保证，实际是由**另一个函数**（`commitFetchedPage`）顺带提供的，而不是由"改变了 page/sort/filter 的那个函数"提供。任何 early-return（`requestRevision` / `loadingRevision` 不匹配、在途短路）或失败分支都会**静默**打破这一保证，且没有任何测试覆盖它（已核实：现有用例无一条断言重新取数后的选择状态）。
  结论：**必须把 `gridSelection` + `selectedRows` + `lastSelectedIndex` 三者在三处收敛点统一清空**，只清新增的 `gridSelection` 等于把最危险的那一套留下。分册 03 的 U-1 独立复现了同一结论。
- **自测**：`src/stores/__tests__/tableDataStore.selectionReset.test.ts`（新增，见 §9.7）——**必须先红后绿**，且要按 §9.7 的说明把断言打在"响应提交之前"（在途窗口）与"失败路径"上，否则 `selectedRows` 那一半会因为 `commitFetchedPage` 本来就清而**假绿**。`src/stores/__tests__/tableDataStore.test.ts` 一行都不用改（已核实其中 `setPage` / `applyFilters` / `setSort` 用例只断言 mock 调用参数与 `page` / `rows` / `filters`，没有任何断言依赖"重新取数后保留选择"；见 §11 Q10）。

### Step 5 · 事件守卫：`src/lib/gridEventGuards.ts`（新增）

- **新增符号**：`isComposingEvent`、`isEditableTarget`（3.10）。
- **为什么**：IME 与"焦点是否在输入控件里"这两件判定在 01/02/03/05 都要用；`useKeyboardShortcuts` 的 `inField` 判定是私有的，不能复用，必须有一个共享版本。
- **自测**：`src/lib/__tests__/gridEventGuards.test.ts`（新增）覆盖 DOM 事件、React 合成事件形状、`input`/`textarea`/`contentEditable`/普通 `div`。

### Step 6 · 事件翻译：`src/hooks/useGridSelection.ts`（新增）

- **新增符号**：`UseGridSelectionOptions` / `UseGridSelectionResult` / `useGridSelection`（3.6）。
- **实现要点**：
  1. `pointerdown` 的前置判定顺序（**顺序本身就是优先级**）：`!enabled` → `e.button !== 0` → `isComposingEvent` 相关只作用于键盘 → `isEditableTarget(e.target)` 或目标在 `[data-dt-editing="true"]` 内 → **忽略**；命中行号槽 → 交给 `onRowGutterSelect`；`resolveDataTableCellFromEvent(e.target)` 为 `null` → 交给空白判定；否则开始"潜在拖拽 / 单击"会话并 `setPointerCapture`。
  2. 用 `useRef` 保存拖拽会话（起点坐标 + 当前坐标 + rAF 句柄）。**这是渲染优化，不是第二真相**：store 仍是唯一真相，会话只在 `pointerup` 时提交一次。
  3. `pointerup`：若 `focus === anchor` 且无修饰键 → 单击语义（plain / shift / cmd 三分支）；否则 → 区域语义。
  4. `Escape`：`editingCell !== null` 时直接返回；否则 `onChange(EMPTY_GRID_SELECTION)`。
  5. `isCellSelected(rowIndex, columnName)`：调 `gridSelectionContains`，`useCallback` 稳定引用。
  6. `dragging` 状态用 `useState`（只影响 UI 类名与测试判定），拖拽结束置回 false。
- **为什么放在 hook 而不是组件**：`DataTable.tsx` 已 628 行，键盘/剪贴板还要进来（02/03），事件翻译必须独立成模块；同时它天然可测。
- **自测**：Step 12 的 journey 测试（jsdom 里必须 `vi.spyOn(document, 'elementFromPoint')`，否则测不出任何东西 —— jsdom 无布局）。

### Step 7 · 单元格组件：`src/components/DataTable/GridCell.tsx`（新增）

- **新增符号**：`GridCellProps` / `GridCell`（3.9）。
- **实现要点**：最外层 `div` 必须同时带上既有属性（`data-testid="data-table-cell"`、`data-dt-row`、`data-dt-col`）与新增属性（`data-dt-selected`、`data-dt-editing`、`data-dt-cell-type`、`role="gridcell"`、`aria-selected`、`id`），宽度用 `style={{ width }}`（与今天一致）；`CellRenderer` 原样放在里面。
- **为什么**：把单元格的 DOM 契约收敛到**一个**文件，06 加 `data-dt-dirty`、05 换编辑器、13 换渲染器都只改这里；同时 `memo` 才能生效（见 Step 8）。
- **自测**：`GridCell.test.tsx`（新增）断言 `resolveDataTableCellFromEvent(cell)` 能从最外层命中；`classifyDataType` 与 `data-dt-cell-type` 一致；`selected === false` 时**不输出** `data-dt-selected` 属性。

### Step 8 · 虚拟行改用 `GridCell`：`src/components/DataTable/VirtualBody.tsx` **＋ 行号槽 `rowheader` 改造**

- **修改符号**：`VirtualRowProps` 增加 `isCellSelected`（`(rowIndex, columnName) => boolean`）、`dragSelecting`（透传给样式）；行内单元格渲染换成 `<GridCell …/>`；`VirtualBodyProps` 增加 `gridSelection?: GridSelection`、`columnOrder`（或直接由 `columns` 推）。
- **关键约束**：`GridCell` 的回调必须是**稳定引用**（直接把 `onCellDoubleClick` / `onCellEdit` / `onCellEditCancel` 这三个 props 透传，让 `GridCell` 自己带上 `rowIndex`/`column.name` 调），**禁止**在 `columns.map` 里写内联箭头函数；否则 `memo(GridCell)` 每帧全部失效，100 万行虚拟滚动会退化。同时把 `data-dt-surface` 挂到 `min-w-max` 的 filler 上。
- **为什么**：这是唯一能把"每格选中判定"接进虚拟滚动的入口；行卸载/复用问题在这里一次性解决。
- **自测**：`npx vitest run src/components/DataTable/__tests__/VirtualBody.test.tsx`（既有断言应保持通过，因为 `data-dt-*` 与 `[tabindex="0"]` 都没变）+ 新增的选中属性断言。。⚠️ **唯一会受行号槽改造影响的既有断言**是按标签找行号槽的用例（若有 `getByRole("button")` 之类的模糊查询仍能命中，因为**按钮没被删除**、只是被包进 `role="rowheader"`）；新增断言见第 9 节。**外层 `div` + 内层按钮使本文件净增约 3 行，仍在 `207 → ~185` 的预算内**（`~185` 估的是「改用 `GridCell`」的净减，wrapper 吃掉其中一部分）

### Step 9 · 抽出右键菜单 + `DataTable` 瘦身与接线

- **新增文件**：`src/components/DataTable/useDataTableContextMenu.ts`（把 `handleContextMenu` 整块搬过去，导出 `useDataTableContextMenu(options)` → `{ onContextMenu }`）。新增一个 `selectionSummary` 输入与一个 `onCopySelectedCells` 可选 handler。
- **修改文件**：`src/components/DataTable/DataTable.tsx`：
  - 删除内联 `handleContextMenu`，改为 `const { onContextMenu } = useDataTableContextMenu({ … })`；
  - 引入 `gridSelection` / `onGridSelectionChange` / `onCopySelectedCells` 三个新 props（全可选）；
  - 滚动容器加 `ref={setScrollEl}`（已有）+ `{...surfaceProps}` + `role="grid"` + `tabIndex={0}` + `aria-activedescendant`；
  - `hasCellSelection = gridSelection != null && onGridSelectionChange != null`；
  - 工具栏：`hasCellSelection` 为真且 `mode === 'cell'` 时渲染选区摘要（`data-testid="data-table-selection-summary"` + `data-dt-selection-rows/cols/cells`），文案走 `t('dataTable.selectionSummary', { rows, cols })`；
  - 删除行按钮与 `Delete` 键在 `mode === 'cell'` 时使用 `rowsCoveredBySelection(...)`（**仍调既有的 `onDeleteRows(rowIndices)`**，于是 `TableView.handleDeleteRows` 的二次确认会自动回显同一个行数）；
  - 给 `GridCell` 传 `selected`：由 `isCellSelected` 决定。
- **为什么**：PRD §11 关键串行点 2 明确"`DataTable` 拆分必须先于 DB-01/02/03 合并，否则单文件超限"。628 行 + 选择 + 键盘 + 剪贴板必然越线，所以本册必须先把菜单这最大的一块搬出去。
- **自测**：`npx vitest run src/components/DataTable/__tests__/DataTable.test.tsx` 必须**一行不改就全绿**（菜单条目 id 序列是硬断言，搬移出任何顺序变化都会红）；`wc -l` 确认 `DataTable.tsx` ≤ 470，新文件 ≤ 320。

### Step 10 · 宿主接线 + `TableView` 前置抽取

- **新增文件**：`src/windows/connection/TablePendingChangesBar.tsx`（把 `table-tx-controls` / `pending-changes-bar` 两块 JSX 与相关计数抽成展示组件，回调由 `TableView` 传入）。
- **修改文件**：`src/windows/connection/TableView.tsx`：
  - 用 `<TablePendingChangesBar …/>` 替换内联块（**所有 `data-testid` 逐字保留**）；
  - `<DataTable>` 增加 `gridSelection={ts?.gridSelection ?? EMPTY_GRID_SELECTION}` 与 `onGridSelectionChange={(sel) => actions.setGridSelection(sel)}`；
  - **不新增**任何"清空选区"的代码：清空已由 Step 4 在 store 的重新取数路径里统一处理。
- **为什么**：`TableView.tsx` 已 770 行，只差 30 行就撞红线，而 02/03/05/06 都会继续往这里加接线。抽取是"先腾地方"的必要前置（与 `DataTable` 拆分同一条纪律）。保留 testid 是因为 `e2e/specs/table-edit.ts`、`e2e/specs/detail-panel.ts` 与 `TableView.test.tsx` 直接依赖它们。
- **自测**：`npx vitest run src/windows/connection/__tests__/TableView.test.tsx` 全绿；`wc -l` 确认 `TableView.tsx` ≤ 660。

### Step 11 · 文案：`src/locales/en/query.ts`（唯一允许改的文案文件）

- **新增 key**：见第 8 节（在 `dataTable.*` 命名空间内追加，与既有 `'dataTable.selected'` 等同区）。
- **为什么**：开发期只改英文侧领域包；`dataTable.*` 全部集中在 `src/locales/en/query.ts`（已核实），不新建域名、不改 `src/locales/en.ts`（它只是聚合再导出）。
- **自测**：`npx vitest run src/locales/locales.test.ts`（既有 i18n 一致性用例）；`pnpm typecheck` 干净（`I18nKey` 允许 en-only key）。

### Step 12 · 测试四类补齐

- 见第 9 节。**journey 测试是本步的验收门槛**，不是可选项。
- **自测**：第 1 节 A1~A4 的全部命令。

---

## 6. 文件级改动清单

| 文件路径 | 新增/修改 | 职责 | 预估行数 | 触及 800 行上限？ |
| --- | --- | --- | --- | --- |
| `src/stores/tableData/gridSelection.ts` | **新增** | 契约类型 + 全部纯函数 + `EMPTY_GRID_SELECTION` | ~210 | 否 |
| `src/hooks/useGridSelection.ts` | **新增** | 指针/焦点/Escape 事件翻译、拖拽会话、边缘自动滚动 | ~300 | 否 |
| `src/components/DataTable/GridCell.tsx` | **新增** | 单元格渲染与 `data-dt-*` 契约（05 / 06 / 07 的扩展点） | ~90 | 否 |
| `src/components/DataTable/useDataTableContextMenu.ts` | **新增** | 从 `DataTable` 搬出的右键菜单构造 + 选区感知条目 | ~300 | 否 |
| `src/lib/gridEventGuards.ts` | **新增** | `isComposingEvent` / `isEditableTarget`（01/02/03/05 共享） | ~45 | 否 |
| `src/lib/gridErrors.ts` | **本册创建骨架**（~60 行） | `GridErrorCode` + `classifyGridError()`：`grid.<域>.<原因>: ` 前缀匹配 + `'unknown'` 回退（契约 §8）。**裁定：按「提交顺序最靠前、且确实需要该文件的分册负责建骨架」这一统一规则由 01 建**（契约 §9.1）；04/05/08/09/10 只追加前缀常量 | ~60 | 否 |
| `src/stores/tableData/selectionActions.ts` | **新增** | 选择状态转移纯函数（含既有 `selectRow`/`toggleSelectAll` 的搬移） | ~230 | 否 |
| `src/windows/connection/TablePendingChangesBar.tsx` | **新增** | 从 `TableView` 抽出的暂存改动条 + 事务控件（**必须保留全部既有 testid**）；**归属见下方裁定：由本册首次创建，03/06 只能复用** | ~150 | 否 |
| `src/stores/tableData/types.ts` | 修改 | `TableState` 增加 `gridSelection` | 55 → ~60 | 否 |
| `src/stores/tableData/connectionState.ts` | 修改 | `emptyTableState` 初始化 `gridSelection` | 87 → ~89 | 否 |
| `src/stores/tableDataStore.ts` | 修改 | 4 个新动作（薄封装）+ 委托既有选择动作 + 重新取数路径统一清空 | 722 → ~700 | 否（**本册必须净减少**） |
| `src/components/DataTable/VirtualBody.tsx` | 修改 | 改用 `GridCell`、透传选区判定、`data-dt-surface`、**行号槽 `<button>` → `<div role="rowheader">` + 内层 `tabIndex={-1}` 按钮**、行容器加 `role="row"` | 207 → ~185 | 否 |
| `src/components/DataTable/DataTable.tsx` | 修改 | 3 个新 props、`surfaceProps` 接线、选区摘要、删行降级为行集、菜单外移 | 628 → ~450 | 否（**必须拆分，否则 02/03 必然越线**） |
| `src/windows/connection/TableView.tsx` | 修改 | 接线 `gridSelection` / `onGridSelectionChange`；替换内联暂存条 | 770 → ~660 | 否（**前置抽取后才安全**） |
| `src/components/DataTable/TableHeader.tsx` | **本册不改**（已核对） | 列序由 `TableData.columns` 过滤后保持原序决定；`data-col-header` 已存在；区域判列只用列名 + `columnOrder`，**无需新增 DOM 属性** | 115（不变） | 否 |
| `src/lib/dataTableContextMenu.ts` | 修改 | `BuildDataTableContextMenuArgs` 增加选区相关可选入参 + 新增 `copy-selected-cells` 条目（handler 缺失时不出现） | 398 → ~420 | 否 |
| `src/locales/en/query.ts` | 修改 | 追加 2 个 `dataTable.*` key（第 8 节） | 424 → ~427 | 否 |
| `src/stores/tableData/__tests__/gridSelection.test.ts` | **新增** | 纯逻辑单测 | ~220 | 否 |
| `src/stores/tableData/__tests__/selectionActions.test.ts` | **新增** | 状态转移单测 | ~180 | 否 |
| `src/lib/__tests__/gridEventGuards.test.ts` | **新增** | 守卫单测 | ~70 | 否 |
| `src/components/DataTable/__tests__/GridCell.test.tsx` | **新增** | 单元格 DOM 契约测试 | ~110 | 否 |
| `src/components/DataTable/__tests__/VirtualBody.rowheader.test.tsx` | **新增** | 行号槽 `rowheader` 内容模型与 R-1~R-3 三条约束的 DOM 断言（独立文件，理由见 9.2） | ~70 | 否 |
| `src/components/DataTable/__tests__/DataTable.selection.test.tsx` | **新增** | 选区组件测试 | ~260 | 否 |
| `src/components/DataTable/__tests__/DataTable.selection.journey.test.tsx` | **新增** | 连续旅程测试（指针 + Escape + 编辑 + IME） | ~300 | 否 |
| `e2e/specs/table-cell-selection.ts` | **新增** | 真实鼠标序列 E2E（含明细断言与 `captureJourneyStep`） | ~220 | 否 |

**组件归属裁定（防止三个分册各抽一份）**：`src/windows/connection/TablePendingChangesBar.tsx` **由本册 Step 10 首次创建**；分册 03 与 06 **只能复用**它，或**扩展它的 props**，**不得另建同名职责的组件**（03 曾拟名 `TableViewPendingBar.tsx`、06 曾拟名 `PendingChangesBar.tsx`，二者一律改用本组件，不再各建一份）。任何后续修改都必须逐字保留 `pending-changes-bar` / `pending-preview` / `pending-commit` / `pending-rollback` / `table-tx-controls` / `table-tx-begin` / `table-tx-commit` / `table-tx-rollback` / `table-read-only-tip` 这些既有 testid —— `src/windows/connection/__tests__/TableView.test.tsx`、`e2e/specs/table-edit.ts`、`e2e/specs/detail-panel.ts` 直接依赖它们。改动量确认：本册**人日预估不变（3.5 人日）**，因为 §9.7 新增的是测试文件而非生产代码，且已包含在"纯逻辑与单测 1.5 人日"预算内。

**需要同步修改的既有测试**：**预期为零**。本册的 DOM 属性只做加法（`data-dt-row` / `data-dt-col` / `data-testid="data-table-cell"` 位置不变，行 div 的 `tabIndex={0}` 不撤），右键菜单在无选区时输出逐字不变，store 的 `selectRow` / `toggleSelectAll` 语义不变，`emptyTableState` 的既有断言是逐字段的。若出现必须改的既有用例，说明设计破坏了兼容性，应当先改设计而不是改断言。

**已知会"变多"的两个文件**：`tableDataStore.ts`（本册净减约 20 行）、`DataTable.tsx`（本册净减约 180 行）；02/03/05/06 后续在这里加东西时仍须遵守 `DataTable.tsx ≤ 470` 的自我约束。

---

## 7. 边界与异常清单

| # | 场景 | 期望行为 |
| --- | --- | --- |
| 1 | **空表**（`rows.length === 0`） | 不存在任何可命中的单元格 ⇒ `pointerdown` 不建立选区；`mode` 恒为 `none`（若之前有选区，取数重置已把它清掉）；工具栏不渲染选区摘要；`Escape` 无副作用；`Delete` 无副作用。**不得**因为 `rows.length === 0` 而让 `role="grid"` / `tabIndex` 消失（键盘宿主必须始终存在） |
| 2 | **只有 1 行** | 竖直拖拽越界 → `focus` 夹到第 0 行；摘要显示 `1 rows × M columns`；`rowsCoveredBySelection` 返回**大小为 1 的集合且含 0**（`has(0) === true`）；02 的 `↓` 无处可去（`moveCellFocus` 返回 `null` 时**不改变选择**） |
| 3 | **只有 1 列** | 区域恒为 1 列宽；`normalizeCellRange` 在该列上必须与自身相等（`indexOf` 退化为同一下标）；`Cmd+单击` 同一格两次 = 先追加后移除，最终 `extraRanges` 为空；摘要 `N rows × 1 columns` |
| 4 | **无主键表** | **选择完全可用**（选择不需要行身份）。行级写操作入口保持今天的状态（`onDeleteRows` 不传 ⇒ 删除按钮不渲染）；`rowsCoveredBySelection` 仍可用于复制/导出/摘要。**禁止**因无主键而禁用选择或隐藏摘要。任何写路径（如删除）必须继续走 `stageRowDelete` → `rowIdentityAnchors`（契约 §4.3） |
| 5 | **只读连接**（`isConnectionReadOnly === true`，`TableView.isEditable` 为 false） | 选择、摘要、复制、导出、行号槽整行选择全部**可用**（今天 `selectedRows` 就不受 `isEditable` 影响）；双击单元格不进入编辑，沿用既有 `showReadOnlyTip()` 与既有 key `tableData.readOnlyEditDisabled`；删除行按钮不渲染（`onDeleteRows` 不传）。若后端返回以 `grid.cell.readOnly: ` 开头的消息，前端必须先经 `classifyGridError()`（`src/lib/gridErrors.ts`，**由本册 01 创建骨架**，契约 §8.1 归属总则）分类再渲染，**未命中任何前缀时原样显示原始消息**（契约第 8 节） |
| 6 | **列被隐藏**（`setVisibleColumns`） | `reconcileGridSelectionForColumns` 立即收敛：引用已隐藏列的附加区被丢弃；主区的 `anchor` 或 `focus` 列被隐藏 ⇒ 整个选择退回 `EMPTY_GRID_SELECTION`（`mode → none`）。理由：维护"选择一定可见"这一不变量，避免出现看不见却仍参与 `N 行 × M 列` 与复制范围的幽灵选区。备选方案（迁移到相邻可见列）见未决问题 Q3 |
| 7 | **翻页后**（`setPage` / `setPageSize`） | **三者一起清**：`gridSelection.mode === 'none'`、`selectedRows` 为空、`lastSelectedIndex` 为 `null`（完整口径见第 18 条）。⚠ 今天只有 `commitFetchedPage` 顺带清了 `selectedRows`，且 **`lastSelectedIndex` 在任何路径都不会被清**——所以这不是"沿用既有行为"，是必须一并修的既有缺陷（§5 Step 4 有逐函数核实表） |
| 8 | **筛选 / 排序变化后**（`setFilters` / `clearFilters` / `applyFilters` / `setSort`） | 同第 7 条：六者全部经 `patchPanelForReload` ⇒ 在收敛点统一清三者。**特别注意**：`setSort` 只改顺序、行数可能完全一样，最容易让人误以为"选区还能留"——必须重置，否则高亮的格会指向另一行 |
| 9 | **双击进入编辑中**（`editingCell !== null`） | 选区**保留**（编辑不取消选择）；`data-dt-editing="true"` 挂在被编辑格；`pointerdown` 落在编辑器 input 上被完全忽略（不夺焦点、不重设选区）；`Escape` 优先交给编辑器（网格 Escape 直接返回）；`Delete` 仍被既有 `handleDeleteKey` 的 `if (editingCell) return` 拦住 |
| 10 | **粘贴进行中 / 暂存提交中**（03 引入；`pendingStatus !== 'idle'`） | 选择状态**不冻结**（用户仍可移动选区、查看摘要）；写操作本身由 03/04 拒绝并由它们给出提示。`stageCellChange` / `stageRowDelete` 成功后**不清空**选区（与今天一致：它们不碰 `selectedRows`，本册也不让它们碰 `gridSelection`） |
| 11 | **拖拽超出视口** | 边缘 24px 内启动 rAF 自动滚动，每次步长约 `rowHeight`；`focus` 夹在 `[0, rowCount-1]`；指针落在行间空隙或表格两侧留白时**保留上一个有效坐标**；`pointerup` / `pointercancel` / 窗口失焦结束会话并按当前坐标提交一次 |
| 12 | **选区引用的行在提交后被刷新** | `commitPendingChanges` 末尾会 `loadTableData` ⇒ 走 `commitFetchedPage` ⇒ 选区与行选择一并清空。这与今天 `selectedRows` 的行为一致（刷新后不留旧选择） |
| 13 | **面板被移除**（`removePanel`） | 整个 slice 消失，无需额外清理；不得留下任何以 `panelId` 为键的选区缓存（**本册不引入任何新缓存**，这正是选择状态放在 `TableState` 而不是模块级 Map 的原因） |
| 14 | **`gridSelection` 传了但 `onGridSelectionChange` 没传** | 只渲染、不产生交互：指针处理器不注册（`hasCellSelection === false`），`data-dt-selected` 仍按传入的选择渲染。适用于纯展示/截图/文档场景 |
| 15 | **同一格被 `Cmd+单击` 多次（残缺中间态）** | 第一次追加单格区域、第二次移除，`extraRanges` 回到空且**不影响主区**；不得产生"空区域"（宽或高为 0 的区域不许进入 `extraRanges`） |
| 16 | **列名含空格 / 引号 / 非 ASCII** | `columnName` 原样进 `data-dt-col` 与状态；`aria-activedescendant` 的 id **只用行列下标**，因此不受影响；`columnOrder` 比较用 `indexOf` 精确匹配（不做大小写归一） |
| 17 | **超长值 / 多字节字符** | 与选择模型无关：`CellRenderer` 的 120 字截断与 `EditableCell` 的字符串处理保持今天的行为；选区只关心坐标，不关心值长度 |
| 18 | **翻页 / 改排序 / 改筛选之后**（重新取数） | `gridSelection.mode === 'none'`、`selectedRows` 为空、`lastSelectedIndex` 为 `null` —— **三者必须一起清**。原因是 `rowIndex` 的含义整体平移：只清选区而留下 `selectedRows`，会让"删除选中行"作用到**另一批行**（既有的数据丢失缺陷，见 §5 Step 4 与契约 §4.3 推论）。清空动作必须写在 `patchPanelForReload` / `commitFetchedPage` / `invalidateCachedData` 三个收敛点，**不得**依赖六个 action 各自记得清 |
| 19 | **重新取数失败**（`reloadPanel` 报错） | 同样清空三者：既然无法保证 `rows` 还是导致选中时的那一批，就不能保留任何按行按下标记账的状态。错误态下 `mode` 为 `none`，用户看不到任何"仍在选中"的假象 |

---

## 8. i18n key 清单

**唯一允许修改的文案文件**：**`src/locales/en/query.ts`**（该文件同时拥有 `dataTable.*` 与 `tableData.*` 两个命名空间；key 是**扁平点号字符串**，不是嵌套对象，因此"嵌套层级"体现为点号分段）。

> **不要改** `src/locales/en.ts`（它只有 `export { default } from './en/index'`，改了无效）；**不要新建域名包**；**不要动** `de.ts` / `zh-CN.ts` 等其他语言文件（发布前由 `scripts/i18n-sync-check.mjs` 与 i18n 同步流程统一补齐）。

| 完整 key 路径 | 所在域名文件 | 英文文案 | 用途 |
| --- | --- | --- | --- |
| `dataTable.selectionSummary` | `src/locales/en/query.ts` | `{rows} rows × {cols} columns` | 工具栏选区摘要（`data-testid="data-table-selection-summary"`），对应 PRD DB-01 目标行为 3 的 `N 行 × M 列`。插值语法沿用既有 `{count}` 范式 |
| `dataTable.selectionCellsLabel` | `src/locales/en/query.ts` | `{count} cells selected` | 选区变化时的 `aria-live="polite"` 播报文案（虚拟滚动下 DOM 会大量增删，屏幕阅读器需要一个稳定的宣告源） |

**明确不新增**：

| 场景 | 复用既有 key | 位置 |
| --- | --- | --- |
| 只读连接下双击/改值的提示 | `tableData.readOnlyEditDisabled` | `src/locales/en/query.ts`（已存在） |
| 网格容器的 `aria-label` | `dataTable.tableLabel` | `src/locales/en/query.ts`（已存在） |
| 行号槽按钮 title | `dataTable.selectRow` | `src/locales/en/query.ts`（已存在） |
| 工具栏"已选"前缀与行单位 | `dataTable.selected` + `common.rows` | `src/locales/en/query.ts` + `src/locales/en/core.ts`（均已存在） |
| 区域模式下的删除确认 | `dataTable.deleteRow` / `dataTable.confirmDeleteRows` | `src/locales/en/query.ts`（已存在，`{count}` 会自动回显涉及行数） |

**后续分册预计会追加的 key**（本册只登记归属，不代为实现）：`dataTable.copySelectedCells`（03）、`dataTable.dirtyCell`（06）、`dataTable.setValue*`（05）— 一律进 `src/locales/en/query.ts` 的 `dataTable.*` 区。

---

## 9. 测试清单

> 落点规则：Host 通用 UI 在 `src/components/DataTable/__tests__/`；纯逻辑与 store 在相邻 `__tests__/`；E2E 在 `e2e/specs/`。测试文件**参与 `pnpm typecheck`**，禁止用 `any` 绕过 mock 类型（用 `satisfies` / `Pick<>` / `as unknown as X` 精确表达）。

### 9.1 单元测试

**文件**：`src/stores/tableData/__tests__/gridSelection.test.ts`（新增）

| 用例名 | 断言要点 |
| --- | --- |
| `normalizeCellRange sorts a reversed drag` | 反向拖拽（右下 → 左上）后 `start.rowIndex ≤ end.rowIndex` 且列序下标有序；两个角互换后结果相等 |
| `normalizeCellRange compares by column order, not by name` | 构造 `columnOrder = ['name', 'id']`（显示序与字典序相反），断言以 `id` 为 start 的拖拽**不被**误判为 `start > end` |
| `normalizeCellRange returns null for unknown column` | 传入不在 `columnOrder` 的列名 → `null`（不猜测位置） |
| `single cell range contains exactly one cell` | 单格区域的 `cellRangeContains` 只对自己为真 |
| `gridSelectionContains covers primary and extra ranges` | 主区 + 两个附加区，命中判定覆盖三处且不越界 |
| `rowsCoveredBySelection dedupes overlapping ranges` | 主区与附加区重叠时行下标不重复（`size` 等于并集基数），**不依赖返回值的顺序**（返回 `ReadonlySet`，不得断言升序） |
| `columnsCoveredBySelection counts the union of columns` | 主区 3 列 + 附加区落在第 4 列 → 返回 4 |
| `toggleExtraRange adds then removes the same cell` | 同一格调两次 → 回到原状态；不产生零面积区域 |
| `reconcileGridSelection drops ranges with hidden columns` | 隐藏附加区引用的列 → 只丢该附加区，主区保留 |
| `reconcileGridSelection clears everything when the anchor column is hidden` | 主区角列被隐藏 → `EMPTY_GRID_SELECTION` |
| `reconcileGridSelection clamps rows when the page shrinks` | `rowCount` 变小后越界角失效并按 7.6 收敛 |
| `gridSelectionEquals ignores extra range order` | 附加区顺序不同但集合相同 → `true` |
| `moveCellFocus clamps at the edges` | 左上角按上 → `null`（调用方保持原选择）；右下角按下 → `null` |
| `EMPTY_GRID_SELECTION satisfies I1` | 冻结常量满足 I1 且 `Object.isFrozen` 语义（同引用复用） |

**文件**：`src/stores/tableData/__tests__/selectionActions.test.ts`（新增）

| 用例名 | 断言要点 |
| --- | --- |
| `applyRowSelect keeps the existing range semantics` | `range` 扩展 `[lo,hi]`；`multi` 切换；"单击已选中的唯一行"清空并 `lastSelectedIndex = null`（与搬移前逐字一致） |
| `applyToggleSelectAll selects every page row then clears` | 两次调用后回到空集 |
| `applyBeginCellSelection clears rows and sets cell mode` | `mode === 'cell'`、`selectedRows.size === 0`、`lastSelectedIndex === null`（I6） |
| `applyRowModeTransition nulls the cell anchor` | 由 `cell` 进 `row` 后满足 I2 |
| `applyGridSelection forces empty selectedRows in cell mode` | 传入带 `mode:'cell'` 的选择时顺手清空行集 |

**文件**：`src/lib/__tests__/gridEventGuards.test.ts`（新增）

| 用例名 | 断言要点 |
| --- | --- |
| `isComposingEvent reads the DOM flag` | `{ isComposing: true }` → true；`{}` → false |
| `isComposingEvent reads the React synthetic flag` | `{ nativeEvent: { isComposing: true } }` → true |
| `isEditableTarget matches input/textarea/contentEditable` | 三种都为 true；普通 `div` / `null` 为 false；与 `useKeyboardShortcuts` 的 `inField` 语义一致 |

### 9.2 组件测试

**文件**：`src/components/DataTable/__tests__/GridCell.test.tsx`（新增）

| 用例名 | 断言要点 |
| --- | --- |
| `renders the contract attributes on the outermost element` | `resolveDataTableCellFromEvent(cell)` 命中 `{ rowIndex, columnName }` |
| `emits data-dt-cell-type from classifyDataType` | `type: 'timestamp with time zone'` → `data-dt-cell-type="datetime"` |
| `omits selection/editing attributes when false` | `selected === false` 时**没有** `data-dt-selected` 属性（用 `hasAttribute` 断言，不用 `toBeFalsy`） |
| `delegates value rendering to CellRenderer` | 传 `isEditing` 时出现 `table-edit-input`；`null` 值渲染 `NULL` |

**文件**：`src/components/DataTable/__tests__/DataTable.selection.test.tsx`（新增）

沿用 `DataTable.test.tsx` 的 mock 风格（`useI18n`、`useColumnResize`、`useVirtualTable`、`nativeContextMenu`）。

| 用例名 | 断言要点 |
| --- | --- |
| `plain click enters cell mode and marks exactly one cell` | `onGridSelectionChange` 收到 `mode:'cell'` 且 anchor=focus=点击格；DOM 中 `[data-dt-selected="true"]` 恰好 1 个 |
| `shift click extends a rectangle from the anchor` | 点击 `(0,'id')` 后 Shift+点击 `(1,'name')` → 摘要 `2 rows × 2 columns`，选中格 4 个 |
| `cmd click appends a non-contiguous range` | 主区 1 格 + Cmd+点击另一格 → `extraRanges.length === 1`；两格都带 `data-dt-selected` |
| `row gutter click switches to row mode and clears the cell anchor` | `onRowSelect` 被调用且参数为 `{multi:false, range:false}`；`onGridSelectionChange` 收到 `mode:'row'` 且 `anchor === null` |
| `cell mode clears the row selection` | cell 模式断言后，工具栏"已选 N/总行数"的计数不显示（`selectedRows` 为空） |
| `escape clears the selection` | `fireEvent.keyDown(surface, { key: 'Escape' })` → `mode:'none'`，DOM 无 `data-dt-selected` |
| `escape is ignored while a cell editor is open` | 传 `editingCell` 后按 Escape → **不**调用 `onGridSelectionChange` |
| `clicking blank surface clears the selection` | 对 `[data-dt-surface]` / 滚动容器派发 `pointerDown`+`pointerUp` → `mode:'none'` |
| `delete uses the rows covered by the cell range` | `mode:'cell'` 且区域覆盖第 0~1 行 → 点删除按钮后 `onDeleteRows` 收到 `[0, 1]`（升序去重） |
| `selection summary is exposed for E2E` | 摘要元素存在且 `data-dt-selection-rows/cols/cells` 三个属性与纯函数结果一致 |
| `read-only grid does not open an editor on double click` | `enableSetNull` 无关：传只读装配（不传 `onCellEdit`）时双击单元格不产生编辑输入框 |
| `no gridSelection prop keeps today's DOM` | 不传新 props 时容器**没有** `role="grid"`、单元格**没有** `data-dt-selected`（回归保护） |

**文件**：`src/components/DataTable/__tests__/VirtualBody.rowheader.test.tsx`（新增；**独立文件**，不塞进既有 `VirtualBody.test.tsx`——既有文件归 02 的 6 处行定位迁移，两边同时改一个文件会让 01 与 02 的 diff 缠在一起）

| 用例名 | 断言要点 |
| --- | --- |
| `gutter renders as rowheader not a bare button` | 行容器的**第一个**子元素 `tagName === "DIV"` 且 `getAttribute("role") === "rowheader"`；`row.querySelector("button")` 非空（**按钮没被删除**，只是被包进去） |
| `gutter button is not in tab order` | `rowheader` 内层按钮 `tabIndex === -1`；断言 `screen.getByTitle(selectRowLabel)` 后 `expect(btn).toHaveAttribute("tabindex", "-1")`。这是 R-1 的**唯一可观测断言**，不能省 |
| `row container is row and cells are gridcell` | 行容器 `role === "row"`；行内至少一个 `role === "gridcell"` 且带 `data-dt-row` / `data-dt-col`。**这一条与上一条必须同时存在**——只有 `rowheader` 没有 `row`，或只有 `row` 没有 `rowheader`，都是契约第 4.4 节明令的半套 |
| `gutter click still selects whole row and does not double-fire` | 点行号槽 → `onRowSelect` **恰好被调用 1 次**（`toHaveBeenCalledTimes(1)`），参数为 `rowIndex, { multi: false, range: false }`。「恰好 1 次」而不是「被调用」是 R-3 的回归锁：一旦有人删掉 `stopPropagation()`，行容器与按钮会双触发，这条立刻红 |
| `gutter width and border stay on the rowheader cell` | `rowheader` 的 class 含 `w-10` 与 `border-r`；内层按钮 class **不含** `w-10`。R-2 的结构性断言，防止样式类被留在内层导致宽度塌陷 |

### 9.3 连续旅程测试（journey，**AGENTS.md 强制**）

**文件**：`src/components/DataTable/__tests__/DataTable.selection.journey.test.tsx`（新增；命名沿用仓库既有的 `*.journey.test.ts(x)` 约定）

**必须先做的事**：jsdom **没有布局**，`document.elementFromPoint` 恒为 `null` ⇒ 必须 `vi.spyOn(document, 'elementFromPoint').mockImplementation(() => 目标单元格元素)`，并且这个 mock 必须**随序列推进而改变**（模拟光标下的格在变）。否则 journey 测试会把"坐标来自 `data-dt-*`"这条纪律测成假绿。

| 旅程 | 模拟序列（每一步都断言状态跃迁与残缺中间态） |
| --- | --- |
| `drag journey: down → move → move → up commits one rectangle` | ①`pointerDown(0,'id')`（断言 `mode:'cell'`、选中 1 格）②`pointerMove` 到 `(1,'name')`（断言**尚未**提交第二次 onChange，`dragging === true`）③`pointerMove` 到空隙（mock 返回 filler，断言**保留上一个有效坐标**）④`pointerUp` → 只调用一次 `onChange`，选中 4 格 |
| `escape mid-drag journey exits without committing` | `pointerDown` → `pointerMove` → `Escape`（断言 `mode:'none'`，`dragging === false`）→ `pointerUp`（断言**不再**产生新的 onChange，即"抬起不该复活已取消的选区"） |
| `edit-then-reselect journey` | 单击 `(0,'name')` → 双击进入编辑（`data-dt-editing="true"`）→ 在 input 上 `pointerDown`（断言选区**未变**、input 仍持有焦点）→ Escape（断言网格不清选区）→ 单击 `(1,'id')`（断言模式仍 `cell`、编辑态清空、选中 1 格） |
| `IME journey: composing Enter does not mutate the selection` | 在编辑 input 上派发 `keyDown { key: 'Enter', isComposing: true }` → 断言网格级 `onKeyDown` 分支未执行（选区不变、无提交）；再派发 `isComposing: false` → 由编辑器处理 |
| `row-mode journey: gutter → shift gutter → cell → escape` | 行号槽点击（`mode:'row'`，`onRowSelect` 收到 `{multi:false,range:false}`）→ Shift+行号槽（`{range:true}`，`mode:'row'`，anchor 仍为 `null`）→ 点单元格（`mode:'cell'` 且行集清空）→ `Escape`（`mode:'none'`） |
| `column-hidden journey` | 选中 `(0,'name')` → 触发 `reconcileGridSelectionForColumns(['id'])`（直接调纯函数或经 store 动作）→ 断言选择退回 `none`、摘要消失 |
| `page-change journey` | `setPage(1)` → 断言 `gridSelection.mode === 'none'` 且 `selectedRows.size === 0` |

### 9.4 E2E

**文件**：`e2e/specs/table-cell-selection.ts`（新增）

策略沿用 `e2e/specs/table-edit.ts` 头部注释记录的既有约定：WKWebView 的 WebDriver 对键盘/指针输入支持有限 ⇒ **UI 交互用 `browser.execute` 派发 `PointerEvent`**，状态核对用 `window.__tableDataStore` 与 DOM `data-dt-*` 双通道，数据库核对走查询页。

| 用例 | 断言要点 |
| --- | --- |
| `TC-SEL-001: 单击单元格应进入单元格选择` | 派发 `pointerdown`+`pointerup` 到 `[data-dt-row="0"][data-dt-col="<firstCol>"]` → 该格 `data-dt-selected="true"`，且 `document.querySelectorAll('[data-dt-selected="true"]').length === 1` |
| `TC-SEL-002: 拖拽应产生矩形区域` | 从 `(r2,c1)` 拖到 `(r5,c3)` → `data-dt-selection-rows="4"`、`data-dt-selection-cols="3"`、`data-dt-selection-cells="12"`；工具栏文本含 `4 rows × 3 columns` |
| `TC-SEL-003: Shift+单击应从锚点扩区` | 单击后再 Shift+单击 → 选中格数 = 区域面积 |
| `TC-SEL-004: Escape 应清空选区` | 选区建立后派发 `keydown Escape` → `[data-dt-selected]` 数量为 0，且 store 的 `gridSelection.mode === 'none'` |
| `TC-SEL-005: 行号槽仍走整行选择（回归）` | 点行号槽 → `selectedRows.size === 1`、`gridSelection.mode === 'row'`、`gridSelection.anchor === null` |。补充：点的是 `[role="rowheader"] button`（**新定位方式**），不是坐标；断言 `rowheader` 元素存在且行数 = 可见行数 |
| `TC-SEL-006: 翻页后选区不残留` | 建立选区 → 下一页 → 断言 `mode === 'none'` 且 `selectedRows.size === 0` |
| `TC-SEL-007: 只读连接可选不可编辑` | 用只读连接（或把 `isEditable` 置假）→ 选择与摘要可用；双击后出现 `[data-testid="table-read-only-tip"]`，且**没有** `[data-testid="table-edit-input"]` |
| 每步 | 调 `captureJourneyStep('table-cell-selection-<step>')` 记录步骤截图，便于失败定位 |

**错误文案的断言纪律（契约第 8 节）**：`CommandError` 序列化到 IPC 后是**脱敏字符串，没有机器可读的 code 字段**，因此需要前端精确识别的错误，其消息**必须以 `grid.<域>.<原因>: <人类可读说明>` 前缀开头**，前端用 `classifyGridError()`（`src/lib/gridErrors.ts`，新增）做**前缀匹配**；匹配不到任何前缀时返回 `'unknown'` 并且 **UI 必须回退显示原始消息**（不得吞掉、不得显示空白）。DB-01 不新增任何后端错误生产者，因此本册的测试只覆盖消费侧：

| 用例 | 断言要点 |
| --- | --- |
| `classifyGridError matches the contract prefix`（单元） | `'grid.cell.readOnly: connection is read-only'` → 命中 `'grid.cell.readOnly'`（**前缀**而不是包含匹配） |
| `classifyGridError falls back to unknown and keeps the raw message`（单元） | 任意无关消息 → `'unknown'`，且调用方拿到的是**原始字符串**而非空串 |
| `read-only reason is not swallowed`（组件） | 只读网格双击后显示 `tableData.readOnlyEditDisabled` 的文案；若 store 的 `error` 是 `grid.*` 消息，渲染路径必须先分类再显示，未知前缀时原样透出 |

**回归门禁（必须保持绿，不允许改断言）**：
- `e2e/specs/table-batch-ops.ts` 的 `TC-TABLE-009` ~ `TC-TABLE-014`（整行选择 / 分页 / 批量操作）。
- `e2e/specs/table-edit.ts` 的暂存改动条与提交断言（依赖 `pending-changes-bar` / `pending-preview` / `pending-commit` / `pending-rollback` 四个 testid）。
- `e2e/specs/detail-panel.ts` 中依赖 `pending-commit` / `pending-changes-bar` 的步骤。

**命令与期望输出**（长输出落临时文件，退出码单独打印）：

```bash
LOG="$(mktemp -t datazen-sel).log"
npx vitest run src/stores/tableData/__tests__/gridSelection.test.ts \
  src/stores/tableData/__tests__/selectionActions.test.ts \
  src/lib/__tests__/gridEventGuards.test.ts \
  src/components/DataTable/__tests__ \
  > "$LOG" 2>&1; echo "EXIT=$?"
tail -n 20 "$LOG"                                  # 期望：Test Files 9 passed / Tests N passed
grep -nE 'FAIL|✗|failed' "$LOG" | head -40          # 期望：无输出
```

```bash
LOG2="$(mktemp -t datazen-sel-e2e).log"
pnpm e2e:minimal > "$LOG2" 2>&1; echo "EXIT=$?"     # 期望 EXIT=0
grep -nE 'table-cell-selection|table-batch-ops|table-edit|passing|failing' "$LOG2" | tail -40
```

---

### 9.7 重新取数即清空的回归用例（**修正既有缺陷，必须先红后绿**）

**文件**：`src/stores/__tests__/tableDataStore.selectionReset.test.ts`（新增）

这组用例对应 §5 Step 4 与契约 §4.3 推论。**在实现前它们应当失败**，这正是"缺陷已被测试钉住"的证据；实现后转绿。请在提交信息里写明这一点，**禁止**为了让它们一开始就绿而弱化断言。

#### 9.7.1 "先红后绿"必须真的做红——否则 `selectedRows` 那一半断言会**假绿**

先写下两条**可执行的前置断言**（逐行读源码确认过的事实；若其中任何一条不再成立，说明源码已变，本节的断言时刻必须重新推导）：

| 前置断言（作为测试文件头的注释 + 一条 `it.todo` 记录，让它在被推翻时可见） | 今天的事实 |
| --- | --- |
| `patchPanelForReload` **不清**任何按 `rowIndex` 记账的选择状态 | **成立**：它只 `...updater(ts)` 并把 `requestRevision` 加一；`selectedRows` / `lastSelectedIndex` 都不在它的返回值里 |
| `commitFetchedPage` / `invalidateCachedData` 清 `selectedRows`，但**从不清** `lastSelectedIndex` | **成立**：全仓库 `lastSelectedIndex` 只由 `selectRow` / `toggleSelectAll` / `deleteRows` 写入（外加 `emptyTableState` 初始化），三条重新取数路径一处都不写它 |

**结论：三半断言各自的"做红"能力完全不同，必须区别对待。**

- **`lastSelectedIndex === null`**：三条路径**今天都会红**（没有任何路径清它）⇒ 放在 `await` 成功响应之后断言即可，天然是红的。
- **`selectedRows.size === 0`**：**只有在"响应提交之前"或"失败路径"上才会红**。若照既有 `tableDataStore.test.ts` 的写法 `await vi.waitFor(...)` 等到响应落盘再断言，`commitFetchedPage` 已经"顺手"把 `selectedRows` 清空了 ⇒ 断言**恒真**，用例是假绿、缺陷照旧存在。三种做红方式（至少实现 A 与 B 两种）：
  - **A｜在途窗口**：用既有夹具 `deferred<typeof sampleResponse>()` 让 `getTableData` 返回挂起的 promise，`setPage(...)` 之后**同步**断言三者；此刻 `page` 已变，而 `rows` 与选择状态都还是旧数据集。
  - **B｜失败路径**：`mockDatabaseCommands.getTableData.mockRejectedValueOnce(...)`，`setPage(...)` 后等到 `loading === false && error !== null` 再断言三者；今天 `catch` 分支只写 `loading` / `loadingRevision` / `error`，选择状态原样存活。
  - **C｜陈旧锚点（最能证明用户可见后果）**：不 await 取数，先 `selectRow(PANEL, 0)`，再 `setPage(PANEL, 1)`，紧接着 `selectRow(PANEL, 1, { range: true })`，断言 `selectedRows` 恰为 `{1}`——**今天会得到 `{0, 1}`**，因为旧锚点被当成了当前页的锚点。
- **`gridSelection.mode === 'none'`**：该字段在本册落地前不存在 ⇒ 它不承担"做红"职责，只承担"修好之后不许回归"。不要用它来声称"这组用例是红的"。

> 上面三条 `setPage` / `setSort` / `applyFilters` 用例**至少各有一条采用方式 A 或 B**；方式 C 建议单独一条（它同时是"陈旧锚点"的文档化证据）。若全部采用"等待成功响应后断言"的写法，这组用例在实现前后**都是绿的**，等于没测——这是本节最容易被写成摆设的地方。

| 用例名 | 断言要点 |
| --- | --- |
| `setPage_clears_all_row_index_selection_state` | 先 `selectRow(...)` 使 `selectedRows` 非空、`lastSelectedIndex` 非 `null`，并设一个 `cell` 模式选区；调用 `setPage` 后断言 `selectedRows.size === 0`、`lastSelectedIndex === null`、`gridSelection.mode === 'none'` |
| `setSort_clears_all_row_index_selection_state` | 同上，改排序 |
| `applyFilters_clears_all_row_index_selection_state` | 同上，应用筛选 |
| `setPageSize_clears_all_row_index_selection_state` | 同上，改每页条数（它会重置 `page` 为 0） |
| `commitFetchedPage_clears_all_row_index_selection_state` | 直接驱动取数落盘路径，断言三者被清 |
| `invalidateCachedData_clears_all_row_index_selection_state` | 缓存失效路径同样清 |
| `reload_failure_also_clears_selection` | 令 `reloadPanel` 失败（mock 命令 reject），断言错误态下三者仍被清空（对应 §7 第 19 条） |
| `row_covered_rows_after_paging_never_reference_stale_indices` | 翻页后调用 `rowsCoveredBySelection` 必须返回空集，**不得**返回基于旧下标的行集合 |

> **为什么要单独一个文件**：这组断言跨越 `patchPanelForReload` / `commitFetchedPage` / `invalidateCachedData` 三条路径，放在既有的 `tableDataStore.test.ts` 里容易被后续改动顺手删掉；单独成文件并在文件头写明"这是缺陷回归测试，不是新功能测试"，能显著降低被误删的概率。

## 10. 自查清单

| # | 错误做法 | 正确做法 | 会造成什么后果 |
| --- | --- | --- | --- |
| 1 | 新建一套 `selectedCells` / `selection` 状态存在 `DataTable` 或 `VirtualBody` 的 `useState` 里 | 唯一真相放 `TableState.gridSelection`，组件只读消费（I7）；拖拽会话允许用 `useRef`，但必须在 `pointerup` 时一次性提交 | 虚拟滚动卸载/复用行时选区丢失或错位；02/03/06 各自读一套状态，出现"高亮在这里、复制在那里"的分裂 |
| 2 | 用 `rowIndex × rowHeight` / 累加 `columnWidths` 反推光标下的单元格 | `document.elementFromPoint` 取元素 + 既有 `resolveDataTableCellFromEvent` 读 `data-dt-*` | 列宽被拖过、页面缩放、行被虚拟卸载后坐标全错；选中区域与视觉区域错位（这正是契约 §4.4 明令禁止的做法） |
| 3 | 区域比较用列名字典序（`'id' < 'name'`） | 一律用 `columnOrder.indexOf(columnName)` 比较列序（I4） | 列显示顺序与字典序相反时区域漏格或多格；`normalizeCellRange` 归一方向反转 |
| 4 | `cell` 模式下不清空 `selectedRows` | 进 `cell` 模式时清空行集与 `lastSelectedIndex`（I6，契约 §4.2 第 2 条） | 屏幕上同时出现"整行高亮 + 单元格区域"，随后"删除行"会删掉用户没打算删的行（危险缺陷） |
| 5 | 翻页 / 排序 / 筛选后保留旧坐标 | 集中在 `patchPanelForReload` / `commitFetchedPage` / `invalidateCachedData` 三处重置（本册的关键技巧） | `rowIndex` 语义平移后选区指向**另一行**；用户看着 A 行的高亮，复制/删除的却是 B 行 |
| 6 | 让选择状态参与写操作定位（把 `rowIndex` 拼进 `WHERE`） | 写路径固定走 `rowIdentityAnchors → pendingChanges → rowIdentity`（契约 §4.3） | 翻页/排序后误改误删其它行；这是 TablePlus 无 WHERE UPDATE 事故的同源路径 |
| 7 | 在 `columns.map` 里给 `GridCell` 写内联箭头函数回调 | 透传稳定 props（`onCellDoubleClick` / `onCellEdit` / `onCellEditCancel`），由 `GridCell` 自己带上行列参数 | `memo(GridCell)` 全部失效，每次滚动/输入都重渲染所有可见格，100 万行级网格掉帧 |
| 8 | 在 `pointerdown` 里无条件 `scrollEl.focus()` | 先 `isEditableTarget(e.target)` 判定，命中编辑器 input 时完全忽略 | 点一下编辑器就触发 `onBlur` → `coerceAndCommit` → **用户只是看了一眼就被写入了值** |
| 9 | 拖拽期间每帧调一次 `onGridSelectionChange`（写 store） | 拖拽只更新本地会话，`pointerup` 提交一次；指针落在空隙时保留上一个有效坐标 | 每帧一次 zustand 全量更新 → 拖拽卡顿；"残缺中间态"被清空导致选区闪烁/回退 |
| 10 | 忘记 `isComposing` 守卫，或用 `any` / 索引类型绕过事件形状 | 用 `src/lib/gridEventGuards.ts` 的 `isComposingEvent`（同时认 `event.isComposing` 与 `event.nativeEvent.isComposing`） | 中文/日文输入法按 Enter 选字时触发网格级动作（清选区、删行），用户数据被破坏 |
| 11 | 把选中态写成 DOM class 或用 `querySelectorAll` 反查选中格来做状态推导 | 状态 → 渲染单向流动；DOM 属性只用于 E2E 断言与右键反查 | 虚拟滚动复用 DOM 节点后，旧 class 残留造成"幽灵选中"，且状态无法序列化给 03/06 |
| 12 | 为了省事把选区渲染成"整行假单元格"（把 anchor/focus 扩成整行） | `mode:'cell'` 只高亮矩形内的格；行集语义走 `rowsCoveredBySelection` 派生值 | 视觉上与 `row` 模式无法区分，用户以为选了整行，动作却只作用于矩形内的列 |
| 13 | 各分册各抽一份"暂存改动条 + 事务控件"（01 叫 `TablePendingChangesBar`、03 想叫 `TableViewPendingBar`、06 想叫 `PendingChangesBar`） | 一律复用本册 Step 10 首次创建的 `src/windows/connection/TablePendingChangesBar.tsx`，只允许扩展 props | 三份实现各自漂移：testid 被改名后 `e2e/specs/table-edit.ts` 与 `e2e/specs/detail-panel.ts` **静默失效**；`TableView.tsx` 被三处同时改造成冲突；800 行红线被三份半成品一起突破 |

---

## 11. 未决问题

| # | 问题 | 建议 |
| --- | --- | --- |
| Q10 | **重新取数时清空 `selectedRows` / `lastSelectedIndex` 属于行为变更，是否会被既有测试或使用习惯抵触？** 我已核实 `patchPanelForReload` 今天确实不清它们，且 `setPage` / `setSort` / `applyFilters` 全走它 —— 所以这是修正一个"翻页后删除会删错行"的数据丢失缺陷（契约 §4.3 推论、分册 03 的 U-1 独立复现） | **必须修，不建议保留旧行为**。若既有测试断言了"翻页后 `selectedRows` 保留"，那是把缺陷测试化了，应一并改正并在提交信息写明。**已核实（本册自问自答）**：`src/stores/__tests__/tableDataStore.test.ts` 中 `setPage` / `applyFilters` / `setSort` 的用例只断言 `mockDatabaseCommands.getTableData` 的调用参数（`page` / `skipCount` / `database` / `schema` / `filters`）与 `page` / `rows` / `filters` 的落盘结果，**没有任何一条依赖"重新取数后保留选择"** ⇒ 本项修复**不需要改任何既有断言**，也不需要放宽任何既有用例的语义；唯一新增量是 §9.7 的新测试文件（约 120 行，已含在"纯逻辑与单测 1.5 人日"预算内）。本案**本身不改变总人日**（Q10 的修复早于 ARIA 裁定就已计入预算）。本册当前总预估是 **4.0 人日**，其中 3.5→4.0 的 +0.5 来自 Q1② 的行号槽 `rowheader` 改造，与本项无关——别把两件事记在同一笔账上。唯一需要人工确认的是产品口径：翻页后选择**消失**（本册方案）是否会让高频"跨页多选后批量操作"的用户不满 —— 若确有此需求，正确做法是引入**跨页选择集（按行身份记账）**作为独立特性，而不是保留 `rowIndex` 记账 |
| Q1 | **同一网格出现两个 Tab 停靠点** + **ARIA 角色集合的最终裁定**：本册给滚动容器加 `tabIndex={0}` 作为单元格模式的键盘宿主，但既有 `VirtualBody` 的行 div 仍是 `tabIndex={0}`（既有单测用 `[tabindex="0"]` 找行） | **已裁定，两半各有归属**。① 焦点：建议**本册保留行 div 不变**（零回归优先），由 **02 统一收口**为「容器负责焦点、行 div 改 `tabIndex={-1}`」，并同步更新 `VirtualBody.test.tsx` / `DataTable.test.tsx` 的定位方式（改用 `[data-dt-row-view]` 而不是 `[tabindex="0"]`）。② 角色：**已裁定归本册原子加齐** `grid` / `row` / `gridcell` / `rowheader` + `aria-rowcount` / `aria-colcount` / `aria-activedescendant`（契约第 4.4 节），**02 只改焦点不碰角色**。因此本册 01 已相应把预估从 3.5 上调到 **4.0 人日**（+0.5 = 行号槽 `rowheader` 改造，见 4.4b 与 Step 8），总纲第 2 节 01 行同步改为 4.0。人工只需确认 ①② 这个顺序可接受（代价是 01 落地到 02 落地之间多一个 Tab 停靠点，**已被接受**） |
| Q2 | **macOS 上 `Ctrl+单击` 同时是右键**：加选判定沿用既有 `metaKey \|\| ctrlKey`（`VirtualBody` 的行选择就是这么写的） | 建议：保持一致性，`metaKey \|\| ctrlKey`；同时约定 `contextmenu` 事件优先（先开菜单、不改选区）。代价是 macOS 用户用 Ctrl+点击做加选会同时弹菜单。若裁定不可接受，替代方案是"加选只认 `metaKey`，Windows/Linux 只认 `ctrlKey`"——但这需要平台判定，且与既有行选择行为不一致 |
| Q3 | **列被隐藏时选区怎么办** | 建议：**清空**（本册方案），维护"选择一定可见"。备选：把失效角迁移到相邻可见列（更"聪明"，但用户看到一个自己没选的格被高亮，不可预期）。倾向前者 |
| Q4 | **`src/lib/gridErrors.ts`（`GridErrorCode` + `classifyGridError`）由谁建立** | **已裁定：本册（01）建骨架**，理由见契约 §9.1：01 是提交顺序里**最先合并**的分册，而本册自己就要写 `classifyGridError` 的消费侧用例（前缀匹配 + `'unknown'` 回退必须保留原始消息）——文件不存在则本册的用例无法编译，本册会被一个**下游**分册卡住。改成「本册建骨架、后续分册只追加前缀常量」后，本册不再有下游依赖。本册文件清单因此增加一个 ~60 行新文件（原稿写"本册不建"是基于"01 不产生后端错误"，但没考虑用例的可编译性） |
| Q5 | **PRD 与契约的 `GridSelection` 形态冲突**：PRD §7.2 DB-01 写的是 `ranges: { top, left, bottom, right }[]`（几何式），而 00-contracts §4.1 冻结的是 `{ anchor, focus, extraRanges, mode }`（坐标式） | 建议：**以契约为准**（本册已按契约实现，`to/left/bottom/right` 那种形态会重新引入"列下标"这个不稳定量，与契约 §4.1 的列名纪律直接冲突）。请契约/PRD 持有者把 PRD 那一节标注为过期草稿。此外契约把类型文件定为 `src/stores/tableData/gridSelection.ts`，而 PRD 的落点是 `src/hooks/useGridSelection.ts`——本册**两个都建**（纯逻辑 vs React 接线），建议在契约 §4.1 补一句说明，否则 02/03 可能各自再建一份纯逻辑 |
| Q6 | **水平方向自动滚动**：拖拽时若目标列已在视口外，`elementFromPoint` 取不到单元格，本册只保留上一个有效坐标、不做横向自动滚动 | 建议：按本册方案（先不做）。横向滚动在列很多、列宽很大时才痛；若实测反馈强烈，由 02 或 DB-12 一并做（它们要处理列重排与冻结，横向滚动逻辑天然属于那里） |
| Q7 | **PRD DB-01 目标行为 5「`selectedRows` 一律由区域派生」与契约 §4.2「cell 模式清空 `selectedRows`」正面冲突** | 建议：**以契约为准**——`selectedRows` 只由 `row` 模式维护，区域涉及的行集用**派生只读值** `rowsCoveredBySelection(...)` 表达，不写入 `selectedRows`（否则就是契约明令禁止的"两套真相"）。PRD 该句应改写为"行级批量动作降级为涉及的行集合（派生值）"。需要人工确认，因为这直接决定删除/导出的作用域语义 |
| Q8 | **`Cmd/Ctrl+A` 的语义** | 建议：留给 02 裁定并实现——候选一是"整行全选"（= 既有 `toggleSelectAll`，与 `e2e/specs/table-batch-ops.ts` 的 `TC-TABLE-013` 用例名一致），候选二是"选中当前页全部单元格"（TablePlus 语义更接近整行）。本册不实现的原因是它与 02 的快捷键集强耦合，且今天的测试没有断言结果 |
| ~~Q9~~ | **`data-dt-cell-type` 是否需要区分 NULL** | **已裁定并关闭：不区分。** 该属性表达「**列的类型**」，NULL 是**值**的属性。「该格为 NULL」需要独立 DOM 信号时用 `data-dt-null`，**归属 05**（契约 §4.4 全量表已收录该行），**不得**污染 `data-dt-cell-type` 的语义 |
| Q12 | **`data-dt-cell-type` 的取值粒度：7 个类型族还是 11 个 `CellType`**（05 提出：它的类型化编辑器需要区分 `date`/`time`/`datetime`、`bytes`/`text`、`enum`/`uuid`/`unknown`） | **裁定：权威分类是 `CellType`（11 个值，05 的 `src/lib/cellTypes.ts`）；渲染族只是它的投影（供颜色用）**。但**取值按提交顺序分段收敛**，因为 `cellTypes.ts` 由 05 创建而 01 先合并：本册先用**既有的** `classifyDataType`（7 族，今天已存在于 `dataTypeColors.ts`，本册可直接调用），**05 合并时把同一属性的来源换成 `resolveCellType`**——这是 `GridCell` 里一行的替换，**本册无需为它改任何代码**。理由：这样既避免"01 依赖一个还没被创建的文件"的倒置，也避免维护两套类型分类（08 的算子分派、05 的编辑器分派、本册的属性三者必须同源）。本册的 `emits data-dt-cell-type ...` 用例断言 `datetime`，在两种粒度下都成立，不必改；细粒度用例（`date` / `time` / `enum` / `uuid` 互不相等）**由 05 追加**。 |
