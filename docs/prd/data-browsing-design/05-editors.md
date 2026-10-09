# 05 · 类型感知编辑器与 Set Value（DB-05）

> **状态**：目标设计，**未实现**（本文所有「新增」符号在 `feature/data-browsing-prd` 基线上都不存在）。
>
> **依赖**：`00-contracts.md`（权威契约：`CellWrite` 三态、坐标系、`DataGridCapabilities`、错误码、模板）与 `01-selection.md`（选择模型，**已落盘**：`src/stores/tableData/gridSelection.ts` 纯逻辑 + `src/hooks/useGridSelection.ts` React 接线 + `src/components/DataTable/GridCell.tsx` 单元格 DOM 契约）。本册只**消费**这三者，**不自行另立**任何区域运算、焦点移动、编辑器 DOM 属性或 IME 判定：键盘进出编辑器复用 `moveCellFocus` / `selectionFromAnchorFocus`，提交时机复用 `src/lib/gridEventGuards.ts` 的 `isComposingEvent`，`data-dt-editing` / `data-dt-cell-type` 由 01 的 `GridCell` 输出、本册只提供取值与编辑器本体。另依赖 DB-04 落地的 `src/lib/tableChanges.ts` 里的 `CellWrite` 前端镜像（契约册 §3.3）。
>
> **被谁依赖**：**10（DB-14 查询结果网格可编辑）**为主；另被 03（粘贴走本册的 `coerceValue`）、06（待提交高亮复用 `data-dt-cell-type` 与写意图）、13（DB-13 特殊类型可视化复用 `CellType`，P1）依赖。
>
> **预估人日**：8 人日（归一化 + 错误码 1.5；编辑器族 3；store/菜单/接线 1.5；单测 + 组件 + journey + E2E 2）。不含 DB-04 的 Rust 侧工作。
>
> **本文档不包含什么**：不设计区域选择与焦点移动（01/02）；不做剪贴板（03）；不做新增行与 `build_insert_sql`（04）；不做 dirty 高亮的视觉（06）；不做 JSON 树 / BLOB hex / 图片预览（13，本册只定义 JSON 的**文本**编辑与二进制「不可就地编辑」的处置）；不改任何 Rust 代码、IPC 命令、`driver-api` trait；不新增驱动能力位以外的后端契约。

---

## 1. 目标与验收口径

**一句话目标**：把「按驱动类型字符串做子串猜测 + 一个单行 `<input>`」的单元格编辑，替换为**一次归一化、类型感知控件分派、写意图三态明确**的编辑层——每一格的可编辑性都有**具体原因**，每一次提交都能无歧义地区分「写 NULL / 写某个值（含空串）/ 不写这一列」。

### 可验证验收标准（命令 + 期望输出）

| # | 命令 | 期望输出 |
| --- | --- | --- |
| A1 | `npx vitest run src/lib/__tests__/cellTypes.test.ts` | 表驱动用例全绿；含回归用例 `resolveCellType('point') !== 'number'` 与 `resolveCellType('timestamp with time zone') === 'datetime'`；`Test Files 1 passed` |
| A2 | `npx vitest run src/lib/__tests__/cellEditorKind.test.ts` | `coerceValue` 对每个 `CellEditorKind` 的合法/非法样本全部命中预期；`Test Files 1 passed` |
| A3 | `npx vitest run src/lib/__tests__/gridErrors.test.ts` | `classifyGridError('grid.cell.readOnly: …') === 'grid.cell.readOnly'`；未知前缀返回 `'unknown'`；`Test Files 1 passed` |
| A4 | `npx vitest run src/lib/__tests__/dataTypeColors.test.ts` | **既有用例零改动通过**（证明与 `classifyDataType` 对齐后渲染分类未回归） |
| A5 | `npx vitest run src/components/DataTable/editors` | 组件与 journey 用例全绿，其中 IME 用例断言「组合期间按 Enter 不提交」 |
| A6 | `npx vitest run src/stores/__tests__/tableDataStore.cellWrite.test.ts` | 三态映射用例全绿：`unset` 不落地计划、`null` 与 `''` 可区分 |
| A7 | `pnpm typecheck` | 退出码 0（含测试文件；无 `any` 新增） |
| A8 | `npx vitest run --coverage` | 覆盖率门禁不下降：`src/lib/**`、`src/components/DataTable/**` 行/函数 ≥ 80%、分支 ≥ 75%（门禁来自 `vitest.config.ts` 的 `coverage.thresholds`） |
| A9 | `pnpm e2e:skip-build -- --spec e2e/specs/table-editors.ts` | 新 spec 全绿；断言用 `data-dt-cell-type` / `data-dt-editing` 定位，不用几何坐标 |
| A10 | `node scripts/i18n-sync-check.mjs`（发布前，本册**不运行**） | `src/locales/en/query.ts` 新增 key 已补齐到其他语言；开发期只改英文侧 |

**本册不破坏的不变量**（必须逐条自证）：
1. `stageCellChange(panelId, row, col, value)` 的对外语义与今天逐字节一致——既有 `src/stores/__tests__/tableDataStore.test.ts`、`src/windows/connection/__tests__/TableView.test.tsx` 零改动通过；
2. `classifyDataType` 的签名与既有测试输入上的输出不变；
3. `EditableCell` 的 `toEditString` 导出路径保留（`src/components/DataTable/__tests__/EditableCell.test.ts` 直接 `import { toEditString } from '../../DataTable/EditableCell'`）；
4. `DataTable` 新增 props **全部可选**，未提供时行为与今天一致（契约册 R4）；
5. DOM 数据属性只新增，不改既有 `data-dt-row` / `data-dt-col` / `data-col-header`；
6. 不改 Rust：本册无 `driver-api`、无 IPC、无 `CommandError` 改动（新增错误码清单见第 8/11 节）。

---

## 2. 现状代码事实

只列我亲自读过的符号。**位置一律「文件路径 + 符号名」，无行号。**

### 2.1 编辑器本体（核心现状）

| 文件 | 符号 | 读到的事实（与 DB-05 直接相关） |
| --- | --- | --- |
| `src/components/DataTable/EditableCell.tsx` | `EditableCell`、`EditableCellProps`、`toEditString`、内部 `coerceAndCommit` / `handleCancel` | 唯一的编辑器，渲染单个 `<input data-testid="table-edit-input">`；`autoFocus`；**固定 `aria-label="Edit cell value"` 硬编码英文**；`onBlur` 无条件走 `coerceAndCommit`；`done` ref 防重复提交 |
| 同上 | `coerceAndCommit` 的类型分派 | 纯子串判断：`int`/`serial`/`bigint` → `Number`；`bool` → `raw === 'true'`（**输入 `TRUE`/`1` 都得到 `false`**）；`float`/`double`/`numeric`/`decimal`/`real` → `Number`；`json` → `JSON.parse` 成功则提交对象、**失败则把原文字符串当值提交**（隐性数据损坏，PRD DB-05 明列为行为变更）；其余 → 原串 |
| 同上 | 空值语义 | `raw === '' && 原值为 null` → `onCancel()`；`raw === initial` → `onCancel()`；`raw === ''` 且原值非 null → `onCommit(null)`。即**空串一律折叠成 NULL**，空串本身无法表达 |
| 同上 | 键位 | 只有 `Enter`（提交）与 `Escape`（取消）两个 `onKeyDown` 分支；**没有任何 `compositionstart` / `compositionend` / `isComposing` 守卫**，也没有 `Tab` 分支 → 输入法组合期间按 Enter 会误提交 |

### 2.2 单元格渲染与属性

| 文件 | 符号 | 事实 |
| --- | --- | --- |
| `src/components/DataTable/CellRenderer.tsx` | `CellRenderer`（`memo`）、`CellRendererProps`、`memo` 内 `type = (dataType ?? '').toLowerCase()` | `isEditing` 为真时把 `type` 透给 `EditableCell`；`null`/`undefined` 渲染斜体 `NULL`；`bool` 分支 `String(value)`；`number` 分支右对齐；`datetime` 分支走 `formatTimestamp`；`json` 分支 `formatCell` 后 120 字符截断；**没有 `binary` 分支** → 二进制走最后的兜底文本分支：`typeof value === 'object' ? JSON.stringify(value) : String(value)` |
| 同上 | 兜底分支 | 结果：BLOB 单元格今天显示 `[222,173,190,239]` 这类数值数组串，且**双击即可就地编辑**（`VirtualBody` 不区分列类型就调 `onCellDoubleClick`），提交时会把 `"[1,2,3]"` 这个**字符串**写回二进制列 |
| `src/components/DataTable/VirtualBody.tsx` | `VirtualBody`、`VirtualBodyProps`、`VirtualRow`、内联 `isEditing` 计算 | 单元格 DOM 带 `data-testid="data-table-cell"`、`data-dt-row={vRow.index}`、`data-dt-col={col.name}`；`isEditing = editingCell?.row === index && editingCell.col === col.name`；`CellRenderer` 的 `onCommit={(v) => onCellEdit(vRow.index, col.name, v)}` 是**渲染期新建的内联箭头函数** → 击穿 `CellRenderer` 的 `memo`，每行每格都重渲 |
| `src/components/DataTable/TableHeader.tsx` | `ColumnDef` | 只有 `id` / `name` / `type?: string` 三个字段——**列元数据（`nullable` / `defaultValue` / `isAutoIncrement` / `isPrimaryKey`）没有进入网格组件层**，`type` 就是驱动报的 `data_type` 串 |
| `src/lib/dataTypeColors.ts` | `DataTypeFamily`、`classifyDataType`、`dataTypeTextClass`、`cellValueTextClass`、`FAMILY_CLASS` | 既有的类型归一化痕迹：`'null' \| 'bool' \| 'number' \| 'datetime' \| 'json' \| 'binary' \| 'text'`；`classifyDataType` 用子串顺序判断（先 `bool` → 再数字族 → 再 `timestamp`/`date`/`time`/`interval` → `json` → `bytea`/`blob`/`binary` → 兜底 `text`）；`cellValueTextClass` 对 nullish 值返回 `null` 族，其余按类型族 |
| 同上 | **已发现的分类缺陷** | `classifyDataType('point')` → `'point'.includes('int')` 为真 → 返回 `'number'`（PG 几何类型被当成数字）；`classifyDataType('timestamp')` 在 SQL Server 语义下同样返回 `datetime`（而那里 `timestamp` 是二进制行版本）；`ularray` 类（PG 报 `ARRAY`）与枚举（PG 报 `USER-DEFINED`）无法从类型串判定 |
| `src/lib/formatters.ts` | `formatTimestamp`、`formatCell`、内部 `hasZoneDesignator` | **时区纪律已成文**：无 zone 标识的库文本（`2026-03-01 00:15:30.123`）是**墙钟**，直接原样返回；只有带 `Z`/`+08:00` 的才经 `new Date().toISOString()`。DB-05 的日期编辑器必须延续这条纪律 |
| `src/hooks/useColumnResize.ts` | `computeInitialColumnWidths` | 第四处类型子串分派（`bool`→80、`serial`/`bigint`/`integer`/`int4`/`int2`/`smallint`→90、`numeric`/`decimal`/`float`/`double`/`real`→100、`timestamp`/`date`→180、`json`→260、`text`/`char`/`varchar`→150，兜底 150）——说明「类型→表现」的映射在宿主里已经散落多处 |

### 2.3 编辑的进出与提交链（组件 → store）

| 文件 | 符号 | 事实 |
| --- | --- | --- |
| `src/components/DataTable/DataTable.tsx` | `DataTableProps` | 已声明 `editingCell?: { row: number; col: string } \| null`、`editBuffer?: Map<string, CellEdit>`、`onCellDoubleClick`、`onCellEdit(row, col, value)`、`onCellEditCancel`、`enableSetNull`、`primaryKeyColumns`、`onDeleteRows`、`databaseType`、`getContextCellText` |
| 同上 | `DataTable` 解构列表 | **`editBuffer` 从未被解构**（契约册 §1.2 已记录，属实）→ 等价死 prop；`onCellEdit` 的第三参是 `unknown`（不是写意图） |
| 同上 | `handleCellDoubleClick` / `handleCellEdit` / `handleCellEditCancel` | 三者都只是转调父级 callback，没有任何类型或可编辑性判断 |
| 同上 | `handleContextMenu`、`canSetNull`、`setNullAllowed` | `setNullAllowed = enableSetNull ?? !!onCellEdit`；`canSetNull = hasCellContext && setNullAllowed && !!onCellEdit && !!hit`；菜单项 `onSetNull` 直接调 `onCellEdit?.(hit.rowIndex, hit.columnName, null)` —— **NULL 是靠「传 null 值」表达的**，与空串在类型上无法区分 |
| 同上 | `handleDeleteKey` | `if (editingCell) return;` —— 编辑器打开时吞掉 Delete/Backspace 删行；挂在最外层 `div` 的 `onKeyDown` 上 |
| `src/lib/dataTableContextMenu.ts` | `BuildDataTableContextMenuArgs`、`buildDataTableContextMenuItems`、内部 `item`/`push`/`submenu` | `canSetNull` 决定 `item('set-null', labels.setNull, handlers.onSetNull)`，**它被放在 `more-actions` 子菜单里**（既有测试 `src/lib/__tests__/dataTableContextMenu.test.ts` 与 `src/components/DataTable/__tests__/DataTable.test.tsx` 明确断言了这一点） |
| 同上 | `resolveDataTableCellFromEvent`、`resolveDataTableHeaderColFromEvent` | 用 `target.closest('[data-dt-row][data-dt-col]')` 反查，是本项目确认过的正确做法（契约册 §1.2） |
| `src/stores/tableData/types.ts` | `CellEdit`、`TableState` | `CellEdit` 的 `newValue: unknown` 同样是**无写意图**的表达；`TableState.editingCell` 是 `{ row, col } \| null` |
| `src/stores/tableData/connectionState.ts` | `toCellValue`、`editKey` | `toCellValue(val)`：`null`/`undefined` → `null`，其余 `val as Value`（**无校验的断言**）；`editKey(rowIndex, columnName)` 拼 `${rowIndex}:${columnName}` |
| `src/stores/tableData/pendingChanges.ts` | `findPendingForRow`、`rebuildEditBuffer`、`rowIdentityIsUnique`、`hasPendingIdentityCollision`、`overlayPendingRows`、`pendingChangesForWire`、`AMBIGUOUS_ROW_IDENTITY_ERROR` | 暂存改动按**行身份键**索引；`rebuildEditBuffer` 依据 `changedColumns` 重建逐格缓冲；`pendingChangesForWire` 会剥掉 `rowIndex` |
| `src/stores/tableDataStore.ts` | `startEdit`、`cancelEdit`、`stageCellChange`、`applyColumnToRows`、`stageRowDelete`、`patchPanel` 调用点 | `startEdit` 只写 `editingCell`；`stageCellChange` 是**唯一**的写入口：无主键 → `error = t('tableData.noPrimaryKey')` 并返回；行身份不唯一/冲突 → `AMBIGUOUS_ROW_IDENTITY_ERROR`；`valuesEqual(original, current)` 时把该列从 `originalValues`/`currentValues`/`changedColumns` 中**删除**（这已经隐式等价于 `Unset`）；结束时 `editingCell: null`；`applyColumnToRows` 逐行调 `stageCellChange` |
| `src/lib/tableChanges.ts` | `valuesEqual`、`buildRowIdentity`、`rowIdentityKey`、`clonePendingRowChange`、`PendingRowChange`、`TableChangeContext`、`RowChangePlan`、`isCompleteTableChangeContext` | `valuesEqual` 用稳定 JSON 序列化比较——**`NaN` 会被序列化成 `null`**，因此编辑器绝不能提交 `NaN`；`PendingRowChange.currentValues[col] = null` 就是「写 NULL」的既有表达，且与「列不在 `changedColumns`」不会混淆 |

### 2.4 挂载点与只读判定链

| 文件 | 符号 | 事实 |
| --- | --- | --- |
| `src/windows/connection/TableView.tsx` | `TableView`、`driverReadOnly`、`isConnectionReadOnly`、`isEditable` | `driverReadOnly = DB_REGISTRY[databaseType]?.readOnly === true`；`isConnectionReadOnly = Boolean(readOnlyProp ?? (savedConnection?.readOnly \|\| driverReadOnly))`；`isEditable = !isConnectionReadOnly` |
| 同上 | `showReadOnlyTip`、`handleCellDoubleClick`、`handleCellEdit` | 不可编辑时双击只弹一个 3 秒提示（文案 `tableData.readOnlyEditDisabled`），**不进入编辑**；`handleCellEdit` 在不可编辑时 `actions.cancelEdit()` |
| 同上 | `useEffect`（`!isEditable && editingCell` → `cancelEdit`） | 已有「能力被撤销时关掉编辑器」的退出跃迁，属既有事实 |
| 同上 | `columnDefs`、`primaryKeyColumns`、`enableSetNull`、`onCellEdit` 的传参 | `ColumnDef` 只带 `id`/`name`/`type: c.dataType`；`primaryKeyColumns={columns.filter(c => c.isPrimaryKey).map(c => c.name)}`；`enableSetNull={isEditable}`；`onCellEdit={isEditable ? handleCellEdit : undefined}` |
| `src/windows/connection/PanelContentRenderer.tsx` | `SqlPanelContent`、`panelIsReadOnly` | `panelIsReadOnly = panelDbMeta?.readOnly === true \|\| savedConnection?.readOnly === true`；表格面板把它作为 `readOnly` 下传 |
| 同上 | **视图面板挂载点的硬编码** | 视图面板（`ViewPanel`）挂 `TableView` 时写死 `readOnly={true}` → 视图今天不可编辑，但其**原因与「连接只读」无法区分**（`TableView` 拿不到 `tableType`） |
| `src/windows/connection/result-workspace/ResultTableView.tsx` | `ResultTableView`、`safeMode` | 结果网格：`editingCell={safeMode ? null : editingCell}`；`onCellEdit={(_row, _col, _value) => setEditingCell(null)}`；`enableSetNull={false}`；`onCellEditCancel={() => setEditingCell(null)}` —— **正是契约册注释里说的「查询结果网格只用它来关闭编辑器」**，本册任何改动必须保持这条兜底 |
| `src/hooks/useKeyboardShortcuts.ts` | `ShortcutDef`、`matchShortcut`、`useKeyboardShortcuts` | 修饰键精确匹配；`'table'` 作用域在焦点位于 `input`/`textarea`/`isContentEditable` 时被跳过；命中即 `preventDefault()` 并 `return`（只执行第一个） |
| `src/commands/query.ts` | `queryCommands.executeQuery` | 既有前端查询封装：`(dbSessionId, sql, params?, database?, schema?)`，`command: 'query'`——枚举候选值取数可复用，无需新 IPC |
| `src/lib/databaseTypes.ts` | `DB_REGISTRY`、`escapeIdent` | 注册表由 `generated.ts` 的 `DRIVER_DB_ENTRIES` 合并；`escapeIdent(name, dbType)` 按 `DB_REGISTRY[dbType].quoteChar` 引号化（`` ` ``、`"`、`[` 三分支） |
| `src/lib/nativeContextMenu.ts` | `showNativeContextMenu` | 再导出 `@datazen/driver-sdk` 的实现——是**页内 Web 菜单**（不是 Tauri 原生菜单）→ 编辑器打开时右键会先触发 `blur` |
| `src/lib/tid.ts` | `tid` | 再导出 `@datazen/ui` 的 `tid`（`{ 'data-testid': id }` 或 `{}`） |

### 2.5 类型来源（为什么必须归一化）

| 文件 | 符号 | 事实 |
| --- | --- | --- |
| `packages/driver-sdk/src/types/schemaMetadata.ts` | `ColumnSchema`、`TableInfo.tableType`、`TableType` | `ColumnSchema = { name, dataType, nullable, defaultValue?, isPrimaryKey?, isAutoIncrement?, comment? }`——**没有 `isGenerated` / `isEnum` / `enumValues` / 精度长度字段**；`TableType = 'table' \| 'view' \| 'materializedView' \| 'systemTable'` |
| `packages/driver-api/src/types.rs` | `Value` | `#[serde(untagged)]`，8 个变体：`Null`/`Bool`/`Integer`/`Float`/`String`/`Bytes(Vec<u8>)`/`Timestamp(String)`/`Json`。`Bytes` 无 `serde_bytes`，因此**线上是 JSON 数字数组**，前端 `Value` 类型里对应 `unknown[]` |
| `packages/drivers/postgres/src/type_decode.rs` | `columns_of_row`、`describe_columns` | 查询结果列名与类型来自 sqlx `c.type_info().to_string()`（`INT4`/`TEXT`/`TIMESTAMPTZ` 这类），且 `nullable: true` **恒为真** |
| `packages/drivers/postgres/src/schema.rs` | 列元数据 SQL | 表结构列类型取自 `information_schema.columns.data_type` → 枚举/复合类型报 `USER-DEFINED`，数组报 `ARRAY` |
| `packages/drivers/mysql/src/mysql.rs` | 列元数据读取 | 类型取自 `COLUMN_TYPE` → 形如 `int(11) unsigned`、`tinyint(1)`、`enum('a','b')`、`varchar(255)` |
| `packages/drivers/sqlserver/src/metadata.rs` | `columns_sql` | 类型是用 `CASE` 现拼的字符串（`nvarchar(max)`、`decimal(10,2)`、`datetime2(7)`、`uniqueidentifier`、`timestamp`/`rowversion` 原样透出）；同一 SQL 里已有 `is_computed`、`is_rowversion`、`generated_always_type`，但**没有进入 `ColumnSchema`** |
| `src/lib/structureEditor/types.ts` | `StructureEditorUiConfig.columnTypes` | 既有「驱动在前端声明类型词汇表」的先例：`columnTypes: { value, label }[]`，声明在各驱动自己的 `packages/drivers/<id>/ui/meta.ts`（如 `postgresqlStructureEditor` / mysql 的 `columnTypes` 列表）。它是**DDL 用的候选列表**，不是匹配器，但证明了「驱动自带类型词汇表」这条通路已存在 |
| `src/lib/databaseMeta.ts` | `DatabaseTypeMeta`、`KvWorkspaceCapabilities` | 有 `readOnly?: boolean`（驱动级只读，`TableView` 已在用）、`structureEditor?`、`exportScope?`；**没有 `DataGridCapabilities`**（契约册 §7 要求新增），也**没有**任何列类型归一化声明字段 |
| `packages/ui/src/index.ts`、`packages/ui/src/TemporalValueInput.tsx`、`packages/ui/src/Select.tsx` | `TemporalValueInput`、`TemporalPickerKind`、`parseTemporal`、`Select`/`SelectProps.searchable`、`Checkbox`、`Dialog`、`Input` | **关键复用面**：`TemporalValueInput` 已是「文本框 + 应用内日历/时间弹层」的成品，产出规范串 `YYYY-MM-DD` / `HH:MM:SS` / `YYYY-MM-DDTHH:MM:SS`，带 `invalid` 与 `disabled`；其 `VALUE_RE` **不接受时区标识**；弹层用 `createPortal(..., document.body)` 渲染；`Select` 支持 `searchable`（可搜索下拉）；`Checkbox` 是唯一主题化复选框 |

### 2.6 既有测试与门禁（我读过的）

- `src/components/DataTable/__tests__/EditableCell.test.ts` —— 只测 `toEditString`（null/undefined/对象/数组/数字/布尔）。
- `src/components/DataTable/__tests__/EditableCell.component.test.tsx` —— 测 `<input>` 的类名（`h-7`、`font-mono`、不含 `px-3`/`focus:ring-2`）、Enter 提交、Escape 取消、未改值 blur 取消、`integer`/`boolean`/`numeric`/`jsonb` 强制转换、`''` → `null`、原值 null 时空输入取消。
- `src/lib/__tests__/dataTypeColors.test.ts` —— `classifyDataType` 的三条断言（`ts with time zone` → `datetime` 等）。
- `src/components/DataTable/__tests__/DataTable.test.tsx`、`src/lib/__tests__/dataTableContextMenu.test.ts` —— 断言 `set-null` **位于 `more-actions` 子菜单内**，且 `enableSetNull` 为假时不出现。
- `src/windows/connection/result-workspace/__tests__/ResultTableView.test.tsx` —— 断言结果网格收到的 `editingCell` 与关闭行为。
- `vitest.config.ts` —— `include` 覆盖 `src/**/*.test.{ts,tsx}`、`packages/ui/src/__tests__/**`；`coverage.include` 含 `src/lib/**` 与 `src/components/DataTable/**`；`thresholds`：行/函数 80、分支 75。

---

## 3. 数据结构与接口设计

### 3.1 为什么必须归一化（不接受在宿主里按数据库名分支）

1. **驱动报上来的 `data_type` 是方言原生文本，不是类型系统**：MySQL 报 `COLUMN_TYPE`（`int(11) unsigned`、`tinyint(1)`、`enum('a','b')`），PostgreSQL 报 `information_schema.columns.data_type`（枚举与复合类型一律 `USER-DEFINED`、数组一律 `ARRAY`），SQL Server 是 `CASE` 现拼的字符串，查询结果列则是 sqlx 的 `type_info().to_string()`。同一个语义类型在四个驱动里有四种写法，在宿主里写 `if (driverType === 'mysql')` 是**禁止**的（AGENTS.md 零硬编码条款）。
2. **归一化把「一次判断」收敛成「一次投影」**：`CellType` 是唯一的类型判断结果，颜色（`classifyDataType`）、编辑器控件、可编辑性、`data-dt-cell-type` 全部从它投影出来，避免「显示按 A 分类、编辑器按 B 分类」的双重标准（该属性的**取值粒度**由 01 的 `GridCell` 输出，统一为 `CellType` 的建议见 §11 未决 #14）。
3. **驱动差异必须可被驱动覆盖**：共享表只表达**通用 SQL 类型名**；方言陷阱（SQL Server 的 `timestamp`/`rowversion`、MySQL 的 `tinyint(1)`）由**驱动自己的 `meta.ts`** 用规则覆盖，宿主一行都不改。
4. **未知类型必须兜底为文本编辑器、绝不报错**：归一化返回 `'unknown'`，编辑器分派把 `'unknown'` 当文本处理；`'unknown'` 同时成为「这张表需要补一条驱动规则」的可测信号（信号出口是 props 里的 `cellType`，是否把它带进 DOM 属性取决于 §11 未决 #14 的裁定）。

### 3.2 Rust 侧（本册不改，引用契约）

```rust
// packages/driver-api/src/types.rs（新增，契约册 §3.2 已冻结，DB-04 落地；本册只消费）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum CellWrite {
    Null,           // 该列进语句，写 SQL NULL
    Value(Value),   // 该列进语句，写此值
    Unset,          // 该列不进语句：UPDATE 不动这一列；INSERT 交给数据库默认值
}
```

线上形态：`{"kind":"null"}` / `{"kind":"value","value":42}` / `{"kind":"unset"}`。

### 3.3 前端类型归一化（新增 `src/lib/cellTypes.ts`）

```ts
import type { DataTypeFamily } from './dataTypeColors';

/** 归一化后的单元格类型。这是全宿主唯一的列类型判断结果。 */
export type CellType =
  | 'text'      // 字符/文本/未知文本族
  | 'number'    // 整数/定点/浮点
  | 'bool'      // 布尔
  | 'date'      // 仅日期
  | 'time'      // 仅时间
  | 'datetime'  // 日期时间（可能带/不带时区）
  | 'json'      // 结构化值（JSON / JSONB / 数组 / VARIANT 这类线上就是对象的类型）
  | 'bytes'     // 二进制
  | 'enum'      // 枚举/低基数列
  | 'uuid'      // UUID / uniqueidentifier
  | 'unknown';  // 共享表与驱动规则都没命中（编辑器按文本兜底，不报错）

/** 规则匹配方式。刻意不用「子串一把梭」：`point`.includes('int') 为真，会把 PG 几何当数字。 */
export type CellTypeMatch =
  | 'exact'       // t === pattern
  | 'prefix'      // t.startsWith(pattern)
  | 'suffix'      // t.endsWith(pattern)
  | 'token'       // 存在 token 恰好等于 pattern（token = 按 /[^a-z0-9_]+/ 切分后的小写片段）
  | 'tokenPrefix' // 存在 token 以 pattern 开头（int32 / int4 / int 命中；point 不命中）
  | 'substring';  // 任意位置包含（只在确认无歧义时使用）

export interface CellTypeRule {
  readonly match: CellTypeMatch;
  readonly pattern: string;   // 必须是已小写的字面量
  readonly cellType: CellType;
}

/**
 * 驱动可声明的类型提示。字段全部可选，缺省即「与今天一致」。
 * 声明位置：驱动自己的 `packages/drivers/<id>/ui/meta.ts`（经 DatabaseTypeMeta.cellTypes 暴露），
 * 绝不写在宿主（对齐 KvWorkspaceCapabilities 的既有范式）。
 */
export interface CellTypeHints {
  /** 驱动规则，先于共享表求值；首个命中者胜出。用于覆盖方言陷阱。 */
  readonly overrides?: readonly CellTypeRule[];
  /** 该驱动把哪些类型名用作枚举（token/tokenPrefix 匹配）→ CellType 'enum'。 */
  readonly enumTypes?: readonly string[];
  /** 该驱动里「带时区」的类型名 → 影响日期编辑器是否允许用选择器（见 3.5）。 */
  readonly zonedTypes?: readonly string[];
  /** 该方言下不可就地编辑的类型名（例如 rowversion）→ 只读原因 'typeNotEditable'。 */
  readonly readOnlyTypes?: readonly string[];
}

/** 共享默认表（有序，首个命中者胜出）。顺序即优先级，必须有顺序敏感的测试。 */
export const DEFAULT_CELL_TYPE_RULES: readonly CellTypeRule[];

export function matchCellTypeRule(type: string, rule: CellTypeRule): boolean;
/** 归一化入口。type 为 undefined/空 → 'unknown'；永不抛异常。 */
export function resolveCellType(dataType: string | undefined | null, hints?: CellTypeHints): CellType;
/** 与既有渲染分类的唯一投影（见 3.4）。 */
export function cellTypeToDataTypeFamily(cellType: CellType): DataTypeFamily;
export function isZonedCellType(dataType: string | undefined | null, hints?: CellTypeHints): boolean;
export function isReadOnlyCellType(dataType: string | undefined | null, hints?: CellTypeHints): boolean;
```

**共享默认表（`DEFAULT_CELL_TYPE_RULES`，按此顺序求值）**：

| 序 | match | pattern | → CellType | 覆盖的典型类型名 / 理由 |
| --- | --- | --- | --- | --- |
| 1 | `tokenPrefix` | `enum` | `enum` | MySQL `enum('a','b')`、ClickHouse `Enum8(...)`/`Enum16(...)` |
| 2 | `tokenPrefix` | `set` | `enum` | MySQL `set('a','b')`（低基数列，同一编辑器） |
| 3 | `substring` | `json` | `json` | `json`/`jsonb`/`jsonpath`；**必须早于 bytes**（`binary json` 这类组合名） |
| 4 | `token` | `array` | `json` | PG `information_schema` 把数组报成 `ARRAY`；线上是 JSON 数组 |
| 5 | `suffix` | `[]` | `json` | PG `text[]`/`integer[]`（若驱动报带元素类型的写法） |
| 6 | `tokenPrefix` | `fixedstring` | `bytes` | ClickHouse `FixedString(16)` |
| 7 | `tokenPrefix` | `bytea` / `blob` / `binary` | `bytes` | `bytea`、`tinyblob`/`mediumblob`/`longblob`、`varbinary(max)`、`binary(16)` |
| 8 | `exact` | `raw` / `image` | `bytes` | Oracle `RAW`、SQL Server `image` |
| 9 | `tokenPrefix` | `uuid` | `uuid` | PG `uuid` |
| 10 | `exact` | `uniqueidentifier` | `uuid` | SQL Server |
| 11 | `substring` | `bool` | `bool` | `boolean`/`bool` |
| 12 | `substring` | `timestamp` | `datetime` | `timestamp(6)`、`timestamptz`、`timestamp with time zone`、`timestamp without time zone` |
| 13 | `substring` | `datetime` | `datetime` | `datetime`/`smalldatetime`/`datetime2(7)`/`datetimeoffset`/ClickHouse `DateTime64(3)` |
| 14 | `token` | `interval` | `text` | PG `interval` 是文本形态，不给日期选择器 |
| 15 | `exact` | `date` | `date` | 必须晚于 12/13，否则 `datetime` 会被 `date` 抢走（`prefix` 匹配下 `datetime`.startsWith('date') 为真） |
| 16 | `tokenPrefix` | `date` | `date` | ClickHouse `Date32` |
| 17 | `exact` | `time` | `time` | `time`；必须晚于 12 |
| 18 | `tokenPrefix` | `time` | `time` | `time(6)`/`timetz`；同样晚于 12 |
| 19 | `tokenPrefix` | `int` | `number` | `int`/`int4`/`int32`/`bigint`/`smallint`/`tinyint(1)`/`mediumint`/`Nullable(Int32)` |
| 20 | `substring` | `serial` | `number` | `serial`/`bigserial` |
| 21 | `tokenPrefix` | `numeric` `decimal` `dec` `float` `double` `real` `money` | `number` | `numeric(10,2)`/`decimal(38,0)`/`double precision`/`real`/`float8` |
| 22 | `tokenPrefix` | `number` | `number` | Oracle/Snowflake `NUMBER(38,0)` |
| 23 | `token` | `bit` / `varbit` / `xml` / `inet` / `cidr` / `macaddr` / `geometry` / `geography` / `point` / `polygon` / `linestring` / `hstore` / `year` | `text` | 显式阻断 `point`→`number`（序 19 之前必须命中）；这些值库端就是文本形态 |
| 24 | （兜底） | — | `text` | 其余字符族：`varchar(n)`/`character varying`/`text`/`citext`/`name`/`LowCardinality(String)` |

> **兜底顺序的硬约束**：序 23 必须在序 19 之前。这是本表存在的第一个具体理由——今天的 `classifyDataType` 正是在这里出错。表必须带不变量测试：`resolveCellType('point') === 'text'`。

**驱动覆盖的完整示例（写清「放哪、怎么覆盖」，宿主无一处 mysql/sqlserver 分支）**：

```ts
// packages/drivers/mysql/ui/meta.ts（示意，属驱动自己的文件）
const mysqlCellTypes: CellTypeHints = {
  // MySQL 的 BOOLEAN 在 COLUMN_TYPE 里就是 tinyint(1)：只有驱动知道自己想要复选框
  overrides: [{ match: 'exact', pattern: 'tinyint(1)', cellType: 'bool' }],
};
export const mysqlMeta = { /* … */ cellTypes: mysqlCellTypes };

// packages/drivers/sqlserver/ui/meta.ts（示意）
const sqlserverCellTypes: CellTypeHints = {
  // SQL Server 的 timestamp/rowversion 是 8 字节行版本，不是时间
  overrides: [
    { match: 'exact', pattern: 'timestamp', cellType: 'bytes' },
    { match: 'exact', pattern: 'rowversion', cellType: 'bytes' },
  ],
  zonedTypes: ['datetimeoffset'],
  readOnlyTypes: ['timestamp', 'rowversion'],
};
```

`DatabaseTypeMeta` 新增可选字段（**新增**，与 `structureEditor` 同级，`src/lib/databaseMeta.ts`）：

```ts
export interface DatabaseTypeMeta {
  /* …既有字段不变… */
  /** 列类型归一化的驱动提示；缺省 = 只用共享表，等于不声明时的行为。 */
  cellTypes?: CellTypeHints;
}
```

### 3.4 与 `classifyDataType` 的对齐（契约要求，不可两套标准）

`CellType` 是 `DataTypeFamily` 的**细化**：每个 `CellType` 必须恰好投影到一个既有族，投影函数是**穷尽 switch（无 `default`）**，这样以后新增 `CellType` 会编译失败而不是静默漂移。

| `CellType` | `DataTypeFamily` | 说明 |
| --- | --- | --- |
| `text` | `text` | |
| `number` | `number` | |
| `bool` | `bool` | |
| `date` / `time` / `datetime` | `datetime` | 渲染族把三者合并为一族（既有行为不变） |
| `json` | `json` | 数组也落这里 |
| `bytes` | `binary` | |
| `enum` / `uuid` / `unknown` | `text` | 这三者渲染上就是文本色 |
| — | `null` | **类型无关**：`null` 族只由「值为 nullish」决定（`cellValueTextClass` 的既有语义），`cellTypeToDataTypeFamily` 永不返回它 |

**改造方式（单一路径）**：`classifyDataType(dataType)` 改为 `cellTypeToDataTypeFamily(resolveCellType(dataType))`，签名与调用点不变。已验证既有 `dataTypeColors.test.ts` 的输入（`integer`、`varchar(255)`、`timestamp with time zone`、`boolean`、`jsonb`、`bytea`、`''`、`numeric`）在新表下**输出不变**；`point` 这类输入会从 `number` 修正为 `text`（属缺陷修复，需在 CHANGELOG 标注）。

### 3.5 编辑器选择逻辑（新增 `src/lib/cellEditorKind.ts`）

```ts
import type { CellType } from './cellTypes';
import type { CellWrite, Value } from './tableChanges';   // CellWrite 见契约册 §3.3

/** 控件族。沿用 PRD DB-05 的命名，新增 'time'/'datetime' 由 TemporalValueInput 的 kind 表达，不另立控件。 */
export type CellEditorKind =
  | 'text' | 'number' | 'boolean' | 'enum'
  | 'date' | 'json' | 'binary' | 'longText';

export interface CellEditorKindInput {
  readonly cellType: CellType;
  /** 值为带时区的日期时间类型（来自 isZonedCellType）。 */
  readonly zoned: boolean;
  /** 驱动/元数据已给出枚举候选值。 */
  readonly hasCandidates: boolean;
  /** 取代码点数（不是 UTF-16 长度，见 §7 的 emoji 条目）。 */
  readonly valueCodePoints: number;
}

export function resolveCellEditorKind(input: CellEditorKindInput): CellEditorKind;

/** 内联文本框的字符阈值：超过则走弹层（longText）。 */
export const INLINE_TEXT_MAX_CODE_POINTS = 200;
/** 就地编辑的字节阈值：超过则只读展示（契约册未提供大值加载入口，见 §7/§11）。 */
export const CELL_EDIT_MAX_BYTES = 64 * 1024;

export type CellValueErrorReason =
  | 'emptyNotNull' | 'invalidNumber' | 'invalidInteger' | 'invalidDate'
  | 'invalidTime' | 'invalidDateTime' | 'invalidJson' | 'invalidUuid'
  | 'tooLarge' | 'notEditable';

export type CoerceResult =
  | { readonly ok: true; readonly write: CellWrite }
  | { readonly ok: false; readonly reason: CellValueErrorReason };

export interface CoerceInput {
  readonly kind: CellEditorKind;
  readonly cellType: CellType;
  readonly text: string;
  readonly nullable: boolean;
  readonly originalValue: unknown;
}

/**
 * 文本 → 写意图。行为对齐 PRD DB-05 的 `coerceValue`，但返回**失败原因**而不是单一哨兵值：
 * 编辑器与粘贴（DB-03）都需要「为什么非法」才能给出具体文案。
 * 纯函数：不读 store、不发 IPC、不碰 DOM。
 */
export function coerceValue(input: CoerceInput): CoerceResult;
```

**`CellType` → `CellEditorKind` 分派表**：

| `CellType` | 额外条件 | `CellEditorKind` | 控件（全部复用 `@datazen/ui`） |
| --- | --- | --- | --- |
| `bool` | — | `boolean` | 可空列：3 项 `Select`（`NULL` / `true` / `false`）；非空列：`Checkbox` |
| `enum` | — | `enum` | `Select searchable`；候选值缺失时先显示「加载候选值」按钮 |
| `date` | — | `date` | `TemporalValueInput kind="date"` |
| `time` | — | `date` | `TemporalValueInput kind="time"` |
| `datetime` | `zoned === false` | `date` | `TemporalValueInput kind="datetime"` |
| `datetime` | `zoned === true` | `text` | **纯文本**：`TemporalValueInput` 的正则不接受 `Z`/`+08:00`，用选择器会静默丢掉偏移量（见 §7 时区） |
| `number` | — | `number` | 单行输入 + 正则前置校验 |
| `json` | — | `json` | 弹层 `textarea`（monospace）+ 校验/格式化/压缩 |
| `bytes` | — | `binary` | **不就地编辑**：只读展示 + 尺寸信息（专门入口属 DB-13） |
| `text` | 代码点数 > `INLINE_TEXT_MAX_CODE_POINTS` | `longText` | 弹层 `textarea` |
| `text` / `uuid` / `unknown` | 其余 | `text` | 单行输入（`uuid` 加软校验） |

### 3.6 `CellWrite` 的 UI 映射（本册与契约册 §3 的接合点）

**三个语义操作**（右键 Set Value 子菜单、`⌥+Return` 同一入口）：

| UI 操作 | i18n key | `CellWrite` | UPDATE 行的真实语义 | INSERT 草稿行的真实语义 | 允许条件 |
| --- | --- | --- | --- | --- | --- |
| **Set NULL** | 复用既有 `'dataTable.setNull'` | `{ kind: 'null' }` | `SET c = NULL` | `VALUES (NULL)` | 列 `nullable === true` |
| **Set column default** | `'cellEditor.setDefault'` | `{ kind: 'unset' }` | 该列**不进语句**（= 撤销该格改动） | 该列不进列清单，由数据库填 `DEFAULT`/自增/生成列/触发器 | 始终可用 |
| **Set empty string** | `'cellEditor.setEmpty'` | `{ kind: 'value', value: '' }` | `SET c = ''` | `VALUES ('')` | 在该格可编辑时始终可用 |
| **Revert this cell**（UPDATE 专用别名） | `'cellEditor.revertCell'` | `{ kind: 'unset' }` | 同上（撤销该格改动） | — | 仅 UPDATE |
| **Load candidate values** | `'cellEditor.loadCandidates'` | 不写入（只填候选） | — | — | `cellType === 'enum'` 且无候选 |

**为什么 UPDATE 下不能叫「Set DEFAULT」**：`CellWrite::Unset` 在 UPDATE 里只是「这一列不出现在 `SET` 里」，它**不会**重新应用 DDL 默认值（那需要 `SET c = DEFAULT` 这个 SQL 表达式，三态契约表达不了）。所以：
- UPDATE 行的菜单项文案是 **Revert this cell**（在 UPDATE 语境下 `setDefault` 与 `revertCell` 是同一件事，UI 只显示一个，避免两个入口一个语义）；
- 只有 INSERT 草稿行显示 **Set column default**；
- 两个 key 都必须存在（同一 `CellWrite` 两种语境），由 `rowKind: 'update' | 'insert'` 选择显示哪个。

**`NOW()` / `CURRENT_TIMESTAMP` 不作为本册交付项**：它们是 SQL 表达式，`CellWrite` 的三个变体都表达不了；若用 `{ kind: 'value', value: 'CURRENT_TIMESTAMP' }` 提交，会把**字面字符串**写进库（静默数据损坏）。因此本册**隐藏**这两个入口（R4：能力缺失时隐藏而不是渲染一个点了报错的按钮），是否给 `CellWrite` 增加 `Expression(String)` 变体见 §11 未决 #1。

**归一化规则（防御性）**：编辑器与菜单产出的 `CellWrite` 必须先过 `normalizeCellWrite`：
```ts
/** 新增于 src/lib/tableChanges.ts：Value 类型允许 null，但三态里 null 必须走 'null' 分支。 */
export function normalizeCellWrite(write: CellWrite): CellWrite;  // {kind:'value', value:null} → {kind:'null'}
```

**与既有 store 表示的投影**（前端暂存不需要新字段，`pendingChanges` 已经能表达三态）：

| `CellWrite` | `PendingRowChange` 里的投影 | 判定 |
| --- | --- | --- |
| `Null` | `col ∈ changedColumns` 且 `currentValues[col] === null` | 契约册 §3.4 |
| `Value(v)` | `col ∈ changedColumns` 且 `currentValues[col]` 与 `v` 用 `valuesEqual` 相等 | 同上 |
| `Unset` | `col ∉ changedColumns`；同时 `rows[row][col]` 恢复为 `originalValues[col]` | 同上 |

> 这正是「UPDATE 的空值歧义」在**前端**不成立的原因：`col ∈ changedColumns` 已经区分了「写 NULL」与「不改这一列」。歧义只存在于线上的 `CellUpdate.value: Option<Value>`，DB-04 负责在那里改。

### 3.7 只读判定（新增 `resolveCellEditable`，置于 `src/lib/cellEditorKind.ts`）

```ts
/** 不可编辑的具体原因。每个原因都必须映射到一句能解释「为什么」的文案（契约册 §8.2）。 */
export type CellReadOnlyReason =
  | 'connectionReadOnly'   // 连接或驱动级只读
  | 'driverCapability'     // 驱动显式声明 dataGrid.editable === false
  | 'view'                 // 视图 / 物化视图 / 系统表
  | 'noPrimaryKey'         // 表无主键，无法安全定位行
  | 'ambiguousRowIdentity' // 主键值 NULL / 重复 / 暂存冲突
  | 'autoIncrement'        // 自增列（UPDATE 时不就地编辑）
  | 'generated'            // 生成列（当前元数据无法识别，见 §11）
  | 'binaryColumn'         // 二进制列
  | 'largeValue'           // 超过 CELL_EDIT_MAX_BYTES
  | 'typeNotEditable';     // 驱动声明该类型不可就地编辑（hints.readOnlyTypes）

export interface CellEditabilityInput {
  readonly connectionReadOnly: boolean;          // TableView 的 isConnectionReadOnly
  readonly mountReason?: CellReadOnlyReason;     // 挂载点能给出的更精确原因（视图面板传 'view'）
  readonly relationKind: TableType;              // 'table' | 'view' | 'materializedView' | 'systemTable'
  readonly capabilityEditable?: boolean;         // DatabaseTypeMeta.dataGrid?.editable（字段名已冻结；缺省 true）
  readonly hasPrimaryKey: boolean;
  readonly rowIdentityUnique: boolean;
  readonly column: Pick<ColumnSchema, 'name' | 'nullable' | 'isAutoIncrement'>;
  readonly cellType: CellType;
  readonly inMemoryByteSize: number;
  readonly typeReadOnly: boolean;                // isReadOnlyCellType(...)
  readonly rowKind: 'update' | 'insert';
}

export interface CellEditability {
  readonly editable: boolean;
  readonly reason: CellReadOnlyReason | null;
  /** 契约册 §8.2 的错误码；可编辑时为 null。 */
  readonly code: GridErrorCode | null;           // 不可编辑时固定 'grid.cell.readOnly'
  /** 完整 i18n key 路径，见第 8 节。 */
  readonly messageKey: string;
}
export function resolveCellEditable(input: CellEditabilityInput): CellEditability;
```

**判定表（顺序即优先级，先命中者决定原因）**：

| 序 | 条件 | 可编辑？ | 原因 | 用户可见文案要点 |
| --- | --- | --- | --- | --- |
| 1 | `connectionReadOnly` | 否 | `connectionReadOnly` | 「该连接（或驱动）是只读的」 |
| 2 | `capabilityEditable === false`（**显式声明**） | 否 | `driverCapability` | 「该数据库驱动声明不支持写入」 |
| 3 | `relationKind !== 'table'` | 否 | `view` | 「视图/物化视图/系统表不可就地编辑」 |
| 4 | `!hasPrimaryKey` | 否 | `noPrimaryKey` | 「表没有主键，无法唯一定位这一行」 |
| 5 | `!rowIdentityUnique` | 否 | `ambiguousRowIdentity` | 「主键为 NULL 或有重复，无法唯一定位」 |
| 6 | `column.isAutoIncrement && rowKind === 'update'` | 否 | `autoIncrement` | 「自增列不能在既有行上改写」（INSERT 草稿行允许填） |
| 7 | `typeReadOnly`（`hints.readOnlyTypes`） | 否 | `typeNotEditable` | 「该列类型在此数据库中不可写」（如 rowversion） |
| 8 | `cellType === 'bytes'` | 否 | `binaryColumn` | 「二进制值不能就地编辑」（入口见 DB-13） |
| 9 | `inMemoryByteSize > CELL_EDIT_MAX_BYTES` | 否 | `largeValue` | 「该值过大，无法在网格内编辑」 |
| 10 | 其余 | 是 | — | — |

**`editable` 的缺省语义（契约册 §7 已冻结，与本节实现一致）**：字段名是 `DatabaseTypeMeta.dataGrid?: DataGridCapabilities`（与既有 `kvWorkspace` 的「能力域 + Capabilities」风格一致），`editable` **缺省为 `true`**——读不到 `meta.dataGrid?.editable` 时视为**可编辑**，等价于今天 `isEditable = !isConnectionReadOnly`，符合契约 R4「默认值必须等于今天的语义」（若缺省为 `false`，所有未声明该能力的驱动会一夜之间变只读，那是严重回归而不是保守）。实现口径：**先用既有 `DatabaseTypeMeta.readOnly`（驱动级只读，`TableView` 已在用），再用 `dataGrid` 作为可选增强；只有显式 `editable: false` 才关闭写入**。因此第 2 行的 `driverCapability` 原因**只在驱动显式声明时**出现。

**与既有判定链的关系（不重复、不冲突）**：
- `TableView` 的 `isEditable`（连接级）继续有效，`resolveCellEditable` 只是把它细化为**可解释的原因**；
- `stageCellChange` 里的无主键 / 行身份唯一性校验**一行都不删**——它是最后一道防线（契约册 §8.4 的写路径纪律），`resolveCellEditable` 只是把它**提前**成 UI 层的可见原因；
- 视图不可编辑由挂载点既有事实保证（`PanelContentRenderer` 视图面板写死 `readOnly={true}`）；DB-05 新增可选 prop `readOnlyReason` 把原因从「连接只读」区分为「视图」，不改任何既有行为。

### 3.8 编辑器组件的公共契约（新增 `src/components/DataTable/editors/`）

```ts
/** 所有类型编辑器的统一出口。emits CellWrite，而不是「值」。 */
export interface CellEditorProps {
  readonly columnName: string;
  readonly dataType: string | undefined;
  readonly cellType: CellType;
  readonly kind: CellEditorKind;
  readonly originalValue: unknown;
  readonly nullable: boolean;
  /** 提交（已过校验）。调用方负责 normalizeCellWrite 与写意图落地。 */
  readonly onCommit: (write: CellWrite) => void;
  readonly onCancel: () => void;
  /** 提交后请求把焦点右移/左移一格；无选择模型时调用方忽略。 */
  readonly onCommitAndMove?: (direction: 'next' | 'prev') => void;
  /** 内部校验失败时上报（用于行内错误与「N 格待修正」计数）。 */
  readonly onInvalid?: (reason: CellValueErrorReason) => void;
  /** 枚举候选值：仅在用户显式点击后调用一次。 */
  readonly loadCandidates?: () => Promise<readonly string[]>;
}
```

`onCommit` 的实参类型是 `CellWrite` 而不是 `unknown`：这是本册最关键的接口决定——**类型系统保证三态不会在组件边界丢失**。

### 3.9 宿主暴露给网格的两个入口（新增）

编辑器不注册任何网格级快捷键（那是 02 的地盘），因此「请求编辑」与「导航前守卫」必须以**入口函数**的形式由宿主提供，避免 01/02/05 各写一份 `editingCell` 跃迁逻辑：

```ts
/** 新增于 src/windows/connection/tableEditing.ts。
 *  「请对这两格打开编辑器」的唯一入口：内部先跑 resolveCellEditable，
 *  可编辑才 startEdit，否则只提示原因（不抛错、不静默）。 */
export function requestEdit(panelId: string, coord: CellCoord): void;

/** 新增于 src/windows/connection/tableEditing.ts。
 *  导航（翻页/改页大小/排序/筛选/刷新）前的守卫：编辑器有草稿时，
 *  合法则提交、非法则返回 ok:false 让调用方中止导航并保留编辑器。 */
export type EditorNavigationGuard =
  | { readonly ok: true }
  | { readonly ok: false; readonly reason: CellValueErrorReason };
export function resolveEditorBeforeNavigation(panelId: string): EditorNavigationGuard;
```

调用关系（明确归属，避免重复实现）：01 的双击 → `requestEdit`；02 的 `Enter` → `requestEdit`、`Tab` → 先提交再 `moveCellFocus` + `setGridSelection` + 对新格 `requestEdit`；03 的粘贴 → 直接 `coerceValue` + `stageCellWrite`（不开编辑器）；07 的前缀进导航与 09/11 的翻页排序 → 先 `resolveEditorBeforeNavigation`。

---

## 4. 交互与状态机

### 4.1 状态与所有权

| 状态 | 宿主 | 说明 |
| --- | --- | --- |
| 焦点单元格 / 选择区域 | `GridSelection`（`src/stores/tableData/gridSelection.ts` + `src/hooks/useGridSelection.ts`，**01 的落点**） | DB-05 **不维护第二份焦点移动逻辑** |
| 正在编辑的格 | `TableState.editingCell`（既有） | 只表达「哪一格被编辑器占用」；其**移动**由选择模型驱动 |
| 草稿文本 / 校验错误 | 编辑器组件内部 `useState` | 不落 store（避免每次击键触发整面板重渲） |
| 已提交的写意图 | `TableState.pendingChanges`（既有形状） | 见 §3.6 投影表 |
| 枚举候选缓存 | `TableState.enumCandidates: Map<string, readonly string[]>`（**新增**，键为列名） | 只在用户显式点击后填充；翻页/换表时随面板重置 |

### 4.2 进入条件（三要素之「进入」）

| 入口 | 前置 | 行为 |
| --- | --- | --- |
| 双击单元格（既有 `onCellDoubleClick`） | `resolveCellEditable(...).editable === true` | `startEdit(row, col)`；同时把选择模型设为 `cell` 模式并把焦点设到该格（由 01 的 API 完成） |
| 焦点格上按 `Enter` | 同上 | 与双击同一条路径（DB-02 调用本册暴露的 `requestEdit(coord)`，不自己开编辑器） |
| Set Value 菜单 / `⌥+Return` | 该格可编辑（`unset` 项额外要求存在待提交改动或是 INSERT 草稿行） | **不开编辑器**，直接落地 `CellWrite`（快速路径） |
| 粘贴（DB-03） | 目标格可编辑 | 走 `coerceValue` → `stageCellWrite`；**不逐个开编辑器** |
| 不可编辑的格被双击 | `editable === false` | 不进入编辑；显示 `grid.cell.readOnly` + **该格的具体原因文案**（第 8 节 key），并将 `data-dt-readonly="<reason>"` 暴露给测试 |

### 4.3 状态内行为

1. **首次聚焦**：`text` 类编辑器自动全选原文（沿用 `autoFocus` 行为）；数值/日期编辑器光标置于末尾（避免误覆盖）。
2. **实时校验**：`number`/`date`/`json`/`uuid` 在 `onChange` 时跑 `coerceValue`，非法时行内标红 + `aria-invalid="true"`；`onInvalid` 上报原因为 `invalidJson` 等，供「N 格待修正」计数（DB-06 消费）。**非法输入不阻止用户继续输入**（软校验），只阻止提交。
3. **IME 组合**（本册必须修的既有缺陷）：
   - **判定复用 01 的 `isComposingEvent(event)`（`src/lib/gridEventGuards.ts`，01 新增；它同时兼容 DOM 原生事件的 `event.isComposing` 与 React 合成事件的 `event.nativeEvent.isComposing`）——本册不另写一份判定**；
   - 编辑器自身另持一个 `composingRef`，由 `onCompositionStart` / `onCompositionEnd` 维护：`keydown` 走 `isComposingEvent(e) || composingRef.current`（双条件，因为部分实现里 `compositionend` 早于最后一次 `keydown`），而 `blur` 不是键盘事件、只能看 `composingRef`；
   - `Enter` 在组合期间**必须放行**（由输入法消费来确认候选词）；
   - `Escape` 在组合期间**必须放行**（取消候选词，不得关闭编辑器）；01 已在网格级 `Escape` 上做了同样的让位（`editingCell !== null` 时直接返回），两侧不冲突；
   - `onCompositionEnd` 清标志。
4. **组合期间不提交**还包括 `Tab` 与 `blur`：`blur` 触发提交时必须再判一次组合标志。
5. **枚举候选值**：`cellType === 'enum'` 且 `hasCandidates === false` 时只显示按钮；点击后 `loadCandidates()` 一次（`SELECT DISTINCT … LIMIT 200`，见 §3.9 的纯构造器），结果写入 `enumCandidates` 缓存；**打开编辑器时绝不自动发查询**（PRD 明确要求）。候选超过 200 条时显示「仅显示前 200 个不同值」。
6. **JSON 编辑器**：`textarea` + 「格式化」「压缩」按钮；提交前 `JSON.parse` 校验，失败即 `{ ok: false, reason: 'invalidJson' }`（**行为变更**：今天 `JSON.parse` 失败会把原文字符串提交）。
7. **大值/二进制**：`binary` 与 `largeValue` 都不进入编辑态（`resolveCellEditable` 已拦）；列表里只显示尺寸提示，绝不 `JSON.stringify` 整个字节数组（今天 `CellRenderer` 的兜底分支会 `JSON.stringify`，几 MB 的 BLOB 会在每次重渲时产生几 MB 字符串——本册不改渲染口径，但新增的编辑器路径不得重复这一开销）。

### 4.4 退出跃迁（完整枚举，缺一条即为死锁）

| 退出动作 | 触发 | 行为 |
| --- | --- | --- |
| 提交 | `Enter`（非组合期） | `coerceValue` → 合法：`onCommit(normalizeCellWrite(write))`；非法：**留在编辑态** + 行内错误 + 不写 store |
| 取消 | `Escape`（非组合期） | 丢弃草稿，`onCancel()` → `cancelEdit()`，不写 store |
| 提交并右移 | `Tab` | 与提交同路径；成功后调 `onCommitAndMove('next')`，挂载点用 01 的纯函数 `moveCellFocus(...)` 算出下一格、`actions.setGridSelection(...)` 写回选择，再对新格调本册的 `requestEdit(coord)`（可编辑才开编辑器）。**本册不自己算列序，也不自己改 `editingCell` 之外的坐标** |
| 提交并左移 | `Shift+Tab` | 同上，方向 `'prev'` |
| 提交（失焦） | 焦点离开编辑器 | 提交；**但以下三种失焦不提交、也不关闭**：① 焦点进入本编辑器拥有的 portal（`TemporalValueInput` 的日历弹层、候选值下拉）——用编辑器根的 `data-dt-editor-owned` 标记 + `relatedTarget` 判定；② Set Value 右键菜单打开时（`showNativeContextMenu` 是页内菜单，右键必然先触发 `blur`）；③ 组合期未结束 |
| 点击另一格 | 鼠标按下其他单元格 | 先按失焦规则提交当前格；提交成功后由 01 把焦点移到新格；**若当前值非法则中止移动并保留编辑器** |
| 编辑中的行被卸载（翻页/筛选/排序/刷新） | 导航类动作 | 导航前先 `resolveEditorBeforeNavigation()`：合法则提交，非法则**中止导航**并保留编辑器与行内错误（禁止静默丢弃用户的键入） |
| 行被卸载（防御） | React 卸载编辑器 | `useEffect` cleanup **只清本地状态，绝不提交**（卸载期提交等于静默写入一个用户没确认的值）；若确有未提交草稿，记一条 i18n 文案 `'cellEditor.discardedUncommitted'` 供调用方提示 |
| 能力被撤销 | `isEditable` 变假（既有 effect） | 既有行为：`actions.cancelEdit()`——保持 |
| 外部写操作关掉编辑器 | `stageCellChange`/`stageCellWrite` 结束时 `editingCell: null`（既有） | 保持；编辑器不得因此再提交一次（`done` 语义保留） |

### 4.5 性能契约（大表滚动不得因编辑器而卡顿）

1. **同时存在的编辑器恒为 0 或 1 个**：只有 `editingCell` 命中的那一格渲染 `CellEditorHost`，其余格仍是纯文本 `CellRenderer`。虚拟滚动下可视行约为「视口高度 / 行高 + 12 行 overscan」，因此新增的 DOM 成本是一个编辑器，不是一个列表。
2. **草稿文本只存在编辑器内部**：击键只重渲编辑器本身，不写 `TableState`、不触发 `VirtualBody` 重渲（`TableState` 每次变更都会让整个面板重算 `rows` 与 `editBuffer`）。
3. **类型归一化按列做一次**：在 `TableView`/`DataTable` 用 `useMemo` 把 `columns` 算成 `ReadonlyMap<string, CellType>`，单元格只读 map，**不在渲染期**调 `resolveCellType`（否则每帧 × 行数 × 列数）。
4. **`memo` 必须真的生效**：`VirtualBody` 里 `onCommit={(v) => …}` 的内联箭头原本击穿了 `CellRenderer` 的 `memo`；**01 的 Step 8 已把这条纪律写进 `GridCell`（回调必须稳定引用透传，禁止在 `columns.map` 里写内联箭头）**，本册只需保证新增的 `onCellWrite` / `rowIndex` / `columnName` 同样是稳定 props，不再引入新的内联箭头。
5. **禁止在渲染期做大值序列化**：字节数组（`Value::Bytes` 线上是 JSON 数字数组）与超长文本不得在渲染路径上 `JSON.stringify`；编辑器只用 `value.length` 级别的元数据。
6. **候选值有硬上限**：`LIMIT 200`，超限显示截断提示；候选列表不做逐项监听器，搜索走 `Select` 既有的 `searchable` 过滤。
7. **不做全局监听**：编辑器不注册 `window` 级 `keydown`/`mousemove`（既有 `useKeyboardShortcuts` 已是全局单例，且 `'table'` 作用域在输入框内自动跳过），避免每开一次编辑器多一个监听器。

---

## 5. 实现步骤

> 每步都给「改哪个文件 / 加什么符号 / 为什么 / 怎么自测」。**Step 0 不通过就停止**，不要在本册里再造一份依赖。

**Step 0（前置门禁）**：确认 ① `src/lib/tableChanges.ts` 已有契约册 §3.3 的 `CellWrite` / `cellWriteNull` / `cellWriteValue` / `cellWriteUnset`（由 DB-04 的「提交 1」落地）；② 01 的三件已落下：`src/stores/tableData/gridSelection.ts`、`src/hooks/useGridSelection.ts`、`src/components/DataTable/GridCell.tsx`；③ `src/lib/gridEventGuards.ts` 已导出 `isComposingEvent` / `isEditableTarget`（01 新增，本册的 IME 与失焦判定直接复用，**不得另写一份**）。
**为什么**：这三者是**冻结契约**，本册只消费。**自测**：`npx vitest run src/lib/__tests__/tableChanges.test.ts`（DB-04 的用例）绿；`gridSelection.ts` 导出 `CellCoord` / `CellRange` / `GridSelection` / `EMPTY_GRID_SELECTION` / `normalizeCellRange` / `rowsCoveredBySelection` / `moveCellFocus` / `selectionFromAnchorFocus`；`GridCell.tsx` 导出 `GridCell` / `GridCellProps`；`gridEventGuards.ts` 导出那两个函数（§11 未决 #10 列出本册消费的完整清单）。

**Step 1** `src/lib/cellTypes.ts`（**新增**）：`CellType`、`CellTypeMatch`、`CellTypeRule`、`CellTypeHints`、`DEFAULT_CELL_TYPE_RULES`、`matchCellTypeRule`、`resolveCellType`、`cellTypeToDataTypeFamily`、`isZonedCellType`、`isReadOnlyCellType`；`src/lib/databaseMeta.ts`（**修改**）加 `DatabaseTypeMeta.cellTypes?: CellTypeHints`。
**为什么**：全部类型判断的唯一入口；驱动差异靠 `hints.overrides` 表达，宿主零驱动分支。**自测**：`npx vitest run src/lib/__tests__/cellTypes.test.ts`（表驱动，含 `point`、`ARRAY`、`USER-DEFINED`、`Nullable(Int32)`、`tinyint(1)`、`enum('a','b')`、`varbinary(max)`、`FixedString(16)`、`datetimeoffset`、空串；以及「顺序不变量」：`datetime` 不被 `date` 抢走）。

**Step 2** `src/lib/cellEditorKind.ts`（**新增**）：`CellEditorKind`、`CellEditorKindInput`、`resolveCellEditorKind`、阈值常量、`CellValueErrorReason`、`CoerceResult`、`coerceValue`、`CellReadOnlyReason`、`CellEditabilityInput`、`CellEditability`、`resolveCellEditable`。
**为什么**：控件分派、值强制转换、可编辑性三件事共享同一份类型判断，且必须可脱离 React 单测。**自测**：`npx vitest run src/lib/__tests__/cellEditorKind.test.ts`（含 `Number('')`/`Number(' ')`/`Number('0x10')`/`Number('١٢')`/`NaN`/`Infinity` 全部被拒；`coerceValue` 返回 `CellWrite` 三态；`resolveCellEditable` 十种原因各有用例）。

**Step 3** `src/lib/gridErrors.ts`（**新增**，契约册 §8.1 指定的位置）：`GridErrorCode` 联合类型（含 `grid.cell.readOnly` 与 §8.2 全表）、`classifyGridError(message): GridErrorCode`（**前缀匹配**，未命中返回 `'unknown'`）、`readOnlyReasonMessageKey(reason): string`。
**为什么**：只读原因必须落成可断言的前端分类结果；未知前缀必须回退显示原始消息。**自测**：`npx vitest run src/lib/__tests__/gridErrors.test.ts`。

**Step 4** `src/lib/cellEditorCandidates.ts`（**新增**）：`buildDistinctCandidatesSql({ context, column, limit })`（纯函数，用 `escapeIdent` 引号化，`limit` 固定 200，单条语句，不含 `;` 与多语句）。
**为什么**：候选值取数需要 SQL，但 SQL 构造必须可单测且只有一份；执行仍走既有 `queryCommands.executeQuery`。**自测**：单测断言引号化（含列名里有引号/反引号的用例）、`LIMIT 200`、拒绝空列名；grep 断言宿主里没有 `if (driverType ===`。

**Step 5** `src/lib/dataTypeColors.ts`（**修改**）：`classifyDataType` 改为 `cellTypeToDataTypeFamily(resolveCellType(dataType))`，签名与 `FAMILY_CLASS` 不变。
**为什么**：契约要求「显示分类」与「编辑器分类」同一标准。**自测**：`npx vitest run src/lib/__tests__/dataTypeColors.test.ts`（**既有断言零改动通过**）+ 新增 `dataTypeColors.parity.test.ts` 断言 `classifyDataType(t) === cellTypeToDataTypeFamily(resolveCellType(t))` 对代表性类型串逐条成立。

**Step 6** `src/components/DataTable/editors/`（**新增**）：`toEditString.ts`（把 `EditableCell` 里的实现搬过来，`EditableCell` 改为再导出以保住既有 import 路径）、`CellEditorHost.tsx`（分派 + 键盘/IME/blur 状态机，**唯一**处理提交时机的地方；IME 判定**复用** 01 的 `isComposingEvent`，见 Step 0 与 §4.3）、`TextCellEditor.tsx`、`NumberCellEditor.tsx`、`BooleanCellEditor.tsx`、`EnumCellEditor.tsx`、`TemporalCellEditor.tsx`、`JsonCellEditor.tsx`、`LongTextCellEditor.tsx`、`BinaryCellEditor.tsx`（只读提示）、`setValueMenu.ts`（纯函数：产出 Set Value 子菜单项与禁用规则）。
**为什么**：`EditableCell.tsx` 单文件承载所有类型会把文件与认知都撑爆（PRD 亦要求按族拆分、单文件 < 200 行）。**自测**：`npx vitest run src/components/DataTable/editors`（组件 + journey，含 IME 用例）。

**Step 7** `src/lib/dataTableContextMenu.ts`（**修改**）：`DataTableContextMenuLabels` 加 `setValue`/`setDefault`/`setEmpty`/`revertCell`/`loadCandidates`；`Handlers` 加 `onSetValue?: (id: SetValueActionId) => void`、`onLoadCandidates?: () => void`；`BuildDataTableContextMenuArgs` 加 `setValue?: { canNull: boolean; canEmpty: boolean; rowKind: 'update' | 'insert'; hasPending: boolean }`；`buildDataTableContextMenuItems` 在 `setValue` 存在时输出 `submenu('set-value', …)`；**`onSetValue` 未提供时保持今天的 `more-actions` + `set-null` 布局不变**。
**为什么**：既有测试断言 `set-null` 在 `more-actions` 内、`enableSetNull` 为假时不出现——新入口不能顺手改掉旧行为（旧调用方如结果网格仍需旧菜单）。**自测**：既有 `dataTableContextMenu.test.ts`、`DataTable.test.tsx` 零改动通过；新增用例断言两条分支各自的菜单形状。

**Step 8** `src/stores/tableData/stageCellWrite.ts`（**新增**，纯函数）+ `src/stores/tableDataStore.ts`（**修改**）：新增 action `stageCellWrite(panelId, row, col, write: CellWrite)`；`stageCellChange` 改为 `stageCellWrite(..., cellWriteValue(toCellValue(value)))` 的薄壳；`TableState` 增 `enumCandidates: Map<string, readonly string[]>` 与 `setEnumCandidates`/`clearEnumCandidates`。
**为什么**：三态必须有一个入口，同时 `stageCellChange` 的既有语义不能变（`applyColumnToRows` 与既有测试都依赖它）；把纯逻辑放到新文件是为了**不让 `tableDataStore.ts` 越过 800 行**（它今天 722 行，01 的 Step 3/4 把选择转移搬走后约 700 行）。**自测**：`npx vitest run src/stores/__tests__/tableDataStore.cellWrite.test.ts` + 既有 `tableDataStore.test.ts` 零改动通过。

**Step 9** `src/components/DataTable/GridCell.tsx`（**01 新增，本册扩展**）+ `CellRenderer.tsx`（**修改**）：`GridCellProps` 增可选 `cellType?: CellType`、`editorKind?: CellEditorKind`、`editable?: boolean`、`readOnlyReason?: CellReadOnlyReason | null`、`onCellWrite?: (write: CellWrite) => void`，并把「哪一格正在编辑」的判定与 `data-dt-editing` 交给 01 已冻结的输出方式；`CellRendererProps` 同样增这几个字段，且其 `isEditing` 分支由 `<EditableCell>` 换成 `<CellEditorHost>`。
**为什么**：`data-dt-*` 的 DOM 契约由 01 收敛在 `GridCell`（契约册 §4.4 + 01 §3.7），本册只做**加法**，不动它的取值语义；编辑器分派只依赖 props 里的 `cellType`，**不依赖属性字符串**，因此两册可以各自演进（粒度统一见 §11 未决 #14）。**本册不改 `VirtualBody.tsx`**：它已由 01 的 Step 8 改为渲染 `GridCell` 并保证回调稳定，本册只需保证新增 props 稳定透传。**自测**：`npx vitest run src/components/DataTable`（既有 `VirtualBody.test.tsx`、`CellRenderer.*.test.tsx` 断言零改动通过；01 的 `GridCell.test.tsx` 保持通过）+ 新增断言 `onCellWrite` 收到 `CellWrite`。

**Step 10** `DataTable.tsx`（**修改**）：`DataTableProps` 增可选 `onCellWrite?: (row: number, col: string, write: CellWrite) => void`、`cellTypes?: ReadonlyMap<string, CellType>`、`cellEditability?: (row: number, col: string) => CellEditability`、`readOnlyReason?: CellReadOnlyReason`、`setValueHandlers?: { onSetValue; onLoadCandidates }`、`enumCandidates?: ReadonlyMap<string, readonly string[]>`；全部**稳定引用**透传给 `GridCell`（经 `VirtualBody`）与右键菜单构造（`onSetValue` 存在时菜单走 `set-value` 子菜单，否则保持既有 `more-actions` 形状）。
**为什么**：容器只做组合与透传，判断逻辑留在纯函数里。**自测**：`npx vitest run src/components/DataTable/__tests__/DataTable.test.tsx` 既有用例零改动通过；新增用例断言 `onCellWrite` 收到 `CellWrite`。

**Step 11** `src/windows/connection/tableEditing.ts`（**新增**，纯逻辑：把 `TableState` + 列元数据算成 `columnCellTypes` / `resolveCellEditable` 的输入 / 候选值 provider 工厂）+ `src/windows/connection/TableView.tsx`（**修改**）：接上面新增的 props，注入 `onCellWrite` → `actions.stageCellWrite`、`loadCandidates` → `queryCommands.executeQuery(buildDistinctCandidatesSql(...))`、导航前 `resolveEditorBeforeNavigation()`；`PanelContentRenderer.tsx`（**修改**）给视图面板传 `readOnlyReason="view"`。
**为什么**：`TableView.tsx` 原本已 770 行，接线逻辑必须落在独立模块（01 的 Step 10 抽出 `TablePendingChangesBar` 后约 660 行，本册只净增接线部分）。**与 01 的收敛点对齐**：`setPage` / `setPageSize` / `setSort` / `applyFilters` / `commitFetchedPage` / `invalidateCachedData` 都会把 `gridSelection` 与 `selectedRows` 重置（01 §4.3 的收敛点），因此这些路径必须**先** `resolveEditorBeforeNavigation()`（合法提交、非法中止导航），否则会出现「编辑器还开着、但它的焦点格已不在选择里」的不一致状态。**自测**：`npx vitest run src/windows/connection/__tests__/TableView.test.tsx`（既有用例零改动通过）+ 新用例：无主键表双击不调 `startEdit` 且给出原因文案；`readOnly` + `view` 时原因为 `view`；翻页时若有非法草稿则导航被中止。

**Step 12** i18n + CHANGELOG + 收尾：在 `src/locales/en/query.ts` 添加第 8 节列出的 key（**只改英文侧**）；在 `CHANGELOG.md` 记录两条行为变更：JSON 非法输入由「提交原文」改为「阻止提交」、`point` 类几何类型由 `number` 色改为 `text` 色；补齐覆盖率。**自测**：`pnpm typecheck`、`npx vitest run --coverage`、`pnpm e2e:skip-build -- --spec e2e/specs/table-editors.ts`。

---

## 6. 文件级改动清单

| 文件 | 新增/修改 | 职责 | 预估行数 | 800 行上限 |
| --- | --- | --- | --- | --- |
| `src/lib/cellTypes.ts` | 新增 | 类型归一化表 + `resolveCellType` + 与渲染族的投影 | 180 | 否 |
| `src/lib/cellEditorKind.ts` | 新增 | 控件分派、`coerceValue`、`resolveCellEditable` | 200 | 否 |
| `src/lib/gridErrors.ts` | 新增 | `GridErrorCode` + `classifyGridError` 前缀匹配 + 原因→key | 95 | 否 |
| `src/lib/cellEditorCandidates.ts` | 新增 | `SELECT DISTINCT … LIMIT 200` 纯构造器 | 70 | 否 |
| `src/lib/dataTypeColors.ts` | 修改 | 既有 API 不变，内部改为投影单一路径 | 56 → 60 | 否 |
| `src/lib/databaseMeta.ts` | 修改 | 增 `DatabaseTypeMeta.cellTypes?: CellTypeHints` | 211 → 218 | 否 |
| `src/lib/dataTableContextMenu.ts` | 修改 | 增 Set Value 子菜单参数与纯构造 | 398 → 450 | 否（离红线尚远） |
| `src/lib/tableChanges.ts` | 修改 | 增 `normalizeCellWrite`（`CellWrite` 本体由 DB-04 加） | 227 → 236 | 否 |
| `src/components/DataTable/EditableCell.tsx` | 修改（**瘦身**） | 退化为兼容壳：再导出 `toEditString` 与 `CellEditorHost` | 119 → **30** | 否（**本册通过拆分把它从 119 行降到壳层**） |
| `src/components/DataTable/editors/toEditString.ts` | 新增 | 值→编辑文本（含对象 JSON 化） | 30 | 否 |
| `src/components/DataTable/editors/CellEditorHost.tsx` | 新增 | 按 `CellEditorKind` 分派 + 键盘/IME/blur 状态机 | 170 | 否（>200 需再拆） |
| `src/components/DataTable/editors/TextCellEditor.tsx` | 新增 | 单行文本 | 80 | 否 |
| `src/components/DataTable/editors/NumberCellEditor.tsx` | 新增 | 数值输入 + 整数/浮点分支校验 | 100 | 否 |
| `src/components/DataTable/editors/BooleanCellEditor.tsx` | 新增 | 三态布尔（3 项 Select / Checkbox） | 110 | 否 |
| `src/components/DataTable/editors/EnumCellEditor.tsx` | 新增 | 可搜索下拉 + 「加载候选值」 | 130 | 否 |
| `src/components/DataTable/editors/TemporalCellEditor.tsx` | 新增 | 包装 `TemporalValueInput`（date/time/datetime） | 90 | 否 |
| `src/components/DataTable/editors/JsonCellEditor.tsx` | 新增 | 弹层 textarea + 校验/格式化/压缩 | 150 | 否 |
| `src/components/DataTable/editors/LongTextCellEditor.tsx` | 新增 | 弹层多行（超阈值文本） | 100 | 否 |
| `src/components/DataTable/editors/BinaryCellEditor.tsx` | 新增 | 只读提示 + 尺寸（不编辑） | 60 | 否 |
| `src/components/DataTable/editors/setValueMenu.ts` | 新增 | Set Value 菜单项的纯构造与禁用规则 | 120 | 否 |
| `src/components/DataTable/GridCell.tsx` | 修改（**01 新增**） | 只做加法：透传 `cellType` / `editorKind` / `editable` / `readOnlyReason` / `onCellWrite`；`data-dt-*` 的取值语义不动 | ~90 → ~115 | 否 |
| `src/components/DataTable/CellRenderer.tsx` | 修改 | `isEditing` 分支由 `EditableCell` 改为 `CellEditorHost`；接 `cellType`/`editorKind`/`onCellWrite`，保留既有渲染分支 | 72 → 115 | 否 |
| `src/components/DataTable/VirtualBody.tsx` | **本册不改** | 01 的 Step 8 已改用 `GridCell` 并保证回调稳定（`data-dt-surface` 等一并落地） | 207 → ~188（01 的改动） | 否 |
| `src/components/DataTable/DataTable.tsx` | 修改 | 新 props 透传 + 菜单接线 | 628 → ~495 | **不再触红线**：01 的 Step 9 先把右键菜单搬到 `useDataTableContextMenu.ts` 并降到 ~450，本册在**它的基线上**再加 ~45 |
| `src/windows/connection/tableEditing.ts` | 新增 | 面板级纯逻辑：列类型映射、可编辑性输入、候选值 provider 工厂 | 150 | 否 |
| `src/windows/connection/TableView.tsx` | 修改 | 注入 `onCellWrite` / `loadCandidates` / 导航前解析编辑器 | 770 → ~685 | **不再触红线**：01 的 Step 10 先抽出 `TablePendingChangesBar` 降到 ~660，本册净增 ≤ 25；新增逻辑一律放 `tableEditing.ts` |
| `src/stores/tableData/types.ts` | 修改 | `TableState.enumCandidates` 字段 + 缺省值 | 55 → 62 | 否 |
| `src/components/DataTable/TableHeader.tsx` | 不变（仅消费） | `ColumnDef` 不扩字段：列元数据经 `cellTypes`/`cellEditability` 回调传入，避免把 `ColumnSchema` 塞进组件层 | — | 否 |
| `src/stores/tableData/stageCellWrite.ts` | 新增 | 三态写入的纯逻辑（投影进 `PendingRowChange`） | 130 | 否 |
| `src/stores/tableDataStore.ts` | 修改 | 加 `stageCellWrite` 动作 + `enumCandidates` 缓存；`stageCellChange` 变薄壳 | 722 → ~750 | **不再触红线**：01 的 Step 3/4 先把选择转移搬到 `src/stores/tableData/selectionActions.ts`（净减到 ~700），本册在**它的基线上**净增约 50；纯逻辑放 `stageCellWrite.ts` 就是为了守住这条线 |
| `src/windows/connection/PanelContentRenderer.tsx` | 修改 | 视图面板传 `readOnlyReason="view"` | 558 → 562 | 否 |
| `packages/drivers/<id>/ui/meta.ts` | 可选修改 | 声明 `cellTypes` 覆盖（sqlserver 的 `timestamp`/`rowversion`、mysql 的 `tinyint(1)`） | +5~15/驱动 | 否 |
| `src/locales/en/query.ts` | 修改 | 第 8 节新增 key（**只改英文侧**） | +34 条 | 否 |
| 测试：`src/lib/__tests__/cellTypes.test.ts`、`cellEditorKind.test.ts`、`gridErrors.test.ts`、`cellEditorCandidates.test.ts`、`dataTypeColors.parity.test.ts` | 新增 | 纯逻辑单测（含表驱动） | ~450 合计 | 否 |
| 测试：`src/components/DataTable/editors/__tests__/*.test.tsx`、`src/components/DataTable/__tests__/cellEditor.journey.test.tsx` | 新增 | 每类编辑器组件测试 + 连续旅程 | ~420 合计 | 否 |
| 测试：`src/stores/__tests__/tableDataStore.cellWrite.test.ts`、`src/windows/connection/__tests__/TableView.editors.test.tsx` | 新增 | 三态映射 + 只读原因 | ~220 合计 | 否 |
| `e2e/specs/table-editors.ts` | 新增 | 类型化编辑器 E2E | ~180 | 否 |
| `e2e/specs/table-edit.ts` | 修改 | 既有编辑旅程扩展（三态菜单） | +40 | 否 |

**拆分方案（为什么 `EditableCell.tsx` 必须按族拆）**：现文件 119 行只承载「一个 input + 子串分派 + Enter/Escape/blur」。加上 IME 守卫（复用 01 的 `isComposingEvent` + 本地组合标志）、三类弹层（JSON/LongText/候选值）、日期选择器包装、blur 归属判定与校验状态后，单文件会到 600 行以上并混装 5 类交互。因此拆为：`CellEditorHost.tsx` 独占**提交时机**（键盘/IME/blur/卸载），各 `*CellEditor.tsx` 只管**值编辑与校验**，`setValueMenu.ts` 与 `toEditString.ts` 是无 React 依赖的纯函数。这样每一族都能独立单测与独立演进（DB-13 换掉 JSON/BINARY 展示时只动对应文件）。

---

## 7. 边界与异常清单

| # | 场景 | 期望行为 |
| --- | --- | --- |
| 1 | **值为 NULL** | 编辑器显示空 + 占位提示；不改动直接退出（Enter/blur）→ **不提交**（`coerceValue` 对「原文等于原值」返回无改动，调用方不落 `pendingChanges`）；要写 NULL 用 Set NULL 菜单 |
| 2 | **空字符串 vs NULL** | 二者互不等价：`GetCellText === ''` 在可空列上解释为 **NULL**（保留今天的行为）；**空串只能**由「Set empty string」产生，或由用户在输入框里输入后经 JSON/数值校验不适用时……不成立——因此实现上：文本编辑器为空 → `Null`，`''` 一律走菜单/粘贴；`{kind:'value', value:null}` 必须经 `normalizeCellWrite` 折叠为 `{kind:'null'}` |
| 3 | 非空列的编辑器被清空 | 不允许提交：`coerceValue` 返回 `emptyNotNull`，行内文案 `'cellEditor.notNull'`，编辑器保持打开；**不得**静默转成 `''` 或 `NULL` |
| 4 | **超长文本** | 代码点数 > `INLINE_TEXT_MAX_CODE_POINTS`(200) → `longText`（弹层 textarea，双击直接进弹层）；内存字节 > `CELL_EDIT_MAX_BYTES`(64 KiB) → 只读 + `largeValue` 原因（契约册没有大值加载入口，见 §11 未决 #6） |
| 5 | **含换行的文本** | 单行 input 会把换行显示为空白但值里仍带 `\n`（危险）；因此含 `\n`/`\r` 的值一律走 `longText` 弹层（`valueCodePoints` 之外再加一条 `hasLineBreak` 判定），并在弹层里原样保留换行 |
| 6 | **非法数字** | `coerceValue` 前置正则（`^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$`）拒绝：`''`、`' '`、`'0x10'`、`'١٢'`（阿拉伯-印度数字）、`'12abc'`、`'NaN'`、`'Infinity'`、`'-Infinity'`；整数列额外要求 `Number.isInteger`；`NaN` **绝不允许进入 store**（`valuesEqual` 用 `JSON.stringify`，`NaN` 会被序列化成 `null`，导致「写 NaN」被当成「写 NULL」） |
| 7 | **非法日期** | `date`：必须 `YYYY-MM-DD` 且为真实日历日（拒绝 `2026-02-30`）；`time`：`HH:MM:SS`（允许省略秒则补 `:00`）；`datetime`：`YYYY-MM-DDTHH:MM:SS`；均以 `TemporalValueInput` 产出的规范串为准，非法时行内 `'cellEditor.invalidDate'` 系列文案并阻止提交 |
| 8 | **时区** | 无 zone 的库文本是**墙钟**，原样读写，**禁止**经 `new Date()`/`toISOString()` 往返（`formatTimestamp` 已把这条纪律写成注释）；带 zone 的列（`isZonedCellType` 为真，如 `timestamptz`/`datetimeoffset`，值形如 `2026-03-01T00:15:30Z`）**v1 只给文本编辑器**：`TemporalValueInput` 的正则不接受 zone 后缀，用选择器会把瞬时悄悄改掉。若用户只改日期部分，必须保留原 offset 后缀或直接走文本 |
| 9 | **JSON 语法错误** | `cellType === 'json'` 且输入不是合法 JSON → `{ok:false, reason:'invalidJson'}`，阻止提交，行内 `'cellEditor.invalidJson'`；**行为变更**（今天是「解析失败提交原文」）必须进 CHANGELOG。注意：JSON 列里合法的**字符串值**（如 `"abc"`）也是合法 JSON，不得因为「不以 `{`/`[` 开头」就判非法 |
| 10 | **emoji 与多字节** | 阈值一律按**代码点**计（`[...text].length`），不能按 `text.length`（UTF-16 单元）：否则 100 个 emoji 会被算成 200 而误进 `longText`；写回时不做任何编码转换（`Value::String` 直接给库） |
| 11 | **无法识别的列类型** | `resolveCellType` 返回 `'unknown'` → 文本编辑器；**不报错、不阻塞**，也不写 `console` 噪音（`point` 这类几何/网络类型同样落 `text`，这是对既有 `classifyDataType` 缺陷的修复） |
| 12 | **只读列** | `resolveCellEditable` 返回具体原因，双击显示 `grid.cell.readOnly` + 原因文案（每个原因一句，禁止只说「不可编辑」）；`data-dt-readonly` 暴露原因给测试 |
| 13 | **编辑器已打开时点击另一格** | 先提交当前格（非组合期、非编辑器自有 portal）；提交失败则中止移动并保留编辑器；提交成功后由 01 把焦点移到新格，并在新格可编辑时打开编辑器 |
| 14 | **翻页/筛选/排序导致编辑中的行被卸载** | 导航前 `resolveEditorBeforeNavigation()`：合法提交、非法中止导航；组件卸载的 cleanup **只清本地状态，不提交**，必要时提示 `'cellEditor.discardedUncommitted'` |
| 15 | `bytes` 列被赋值 | 不进入编辑态（`binaryColumn`）；`CellRenderer` 的兜底分支仍会 `JSON.stringify` 字节数组——本册不改这条渲染口径，但要求新增编辑器路径不得 `JSON.stringify` 大字节数组（避免几次「打开设置」就把 UI 卡住） |
| 16 | 空表 / 单行 | 无行数据时不存在编辑器路径；单行表不受影响（`rowIdentityAnchors` 仍按行身份） |
| 17 | 无主键表 | 双击不进入编辑，原因 `noPrimaryKey`；`stageCellWrite` 内部的无主键防线**保留**（双保险） |
| 18 | 暂存改动冲突 | 目标行身份与既有暂存冲突时 `resolveCellEditable` 的 `rowIdentityUnique` 为假 → `ambiguousRowIdentity`；`stageCellWrite` 仍会返回 `AMBIGUOUS_ROW_IDENTITY_ERROR`（不改） |
| 19 | 结果网格（只关闭编辑器） | `ResultTableView` 的 `onCellEdit={() => setEditingCell(null)}` 语义在 DB-05 后依然成立：当父级不提供 `onCellWrite` 时，`CellEditorHost` 走「提交即关闭、不落 store」的兼容分支（DB-14 再改成真正可编辑） |
| 20 | 枚举列无候选值 | 打开编辑器**不发任何查询**；`Select` 只显示「加载候选值」按钮；加载失败 → `'cellEditor.candidatesFailed'`，不改变单元格值 |
| 21 | 数值列收到字符串（PG `money`/`numeric` 的字符串形态） | `cellType` 由类型串决定，但强制转换以**输入文本**为准：`money` 归 `text`（默认表序 23 之后再定，见 §11 未决 #8），不做金额解析 |
| 22 | `tinyint(1)`（MySQL 布尔别名） | 共享表判 `number`；**只有驱动声明覆盖**（`{match:'exact',pattern:'tinyint(1)',cellType:'bool'}`）才变 `bool`。未声明时是数字输入——可接受、可解释，且不牺牲零硬编码 |
| 23 | 查询结果的 `nullable` 恒为真 | 查询结果列（`ColumnInfo.nullable: true` 恒真）上的「空即 NULL」判断会顺理成章；但写入前仍以驱动/数据库约束为准（失败走既有 `CommandError` 脱敏链路） |

---

## 8. i18n key 清单

**唯一改动的文件：`src/locales/en/query.ts`**（`src/locales/en.ts` 只是 `export { default } from './en/index'` 的再导出入口，改它无效；开发期**不动**任何其他语言文件）。

复用既有 key（**不要**新建同义 key）：`'dataTable.setNull'`、`'common.cancel'`、`'tableData.readOnlyEditDisabled'`、`'tableData.noPrimaryKey'`。

新增 key（完整路径 = `src/locales/en/query.ts` 的 key）：

| # | key | 英文值（建议） |
| --- | --- | --- |
| 1 | `'cellEditor.setValue'` | `Set Value` |
| 2 | `'cellEditor.setEmpty'` | `Set empty string` |
| 3 | `'cellEditor.setDefault'` | `Use column default` |
| 4 | `'cellEditor.revertCell'` | `Revert this cell` |
| 5 | `'cellEditor.loadCandidates'` | `Load candidate values` |
| 6 | `'cellEditor.loadingCandidates'` | `Loading candidate values…` |
| 7 | `'cellEditor.candidatesEmpty'` | `No candidate values found` |
| 8 | `'cellEditor.candidatesFailed'` | `Failed to load candidate values` |
| 9 | `'cellEditor.candidatesTruncated'` | `Showing the first {count} distinct values` |
| 10 | `'cellEditor.commit'` | `Apply` |
| 11 | `'cellEditor.formatJson'` | `Format JSON` |
| 12 | `'cellEditor.minifyJson'` | `Minify` |
| 13 | `'cellEditor.ariaLabel'` | `Edit cell value`（把 `EditableCell` 里硬编码的 `aria-label` 换成 key） |
| 14 | `'cellEditor.invalidNumber'` | `Enter a valid number` |
| 15 | `'cellEditor.invalidInteger'` | `Enter a whole number` |
| 16 | `'cellEditor.invalidDate'` | `Enter a valid date (YYYY-MM-DD)` |
| 17 | `'cellEditor.invalidTime'` | `Enter a valid time (HH:MM:SS)` |
| 18 | `'cellEditor.invalidDateTime'` | `Enter a valid date and time (YYYY-MM-DDTHH:MM:SS)` |
| 19 | `'cellEditor.invalidJson'` | `Invalid JSON — fix it before applying` |
| 20 | `'cellEditor.invalidUuid'` | `Enter a valid UUID` |
| 21 | `'cellEditor.notNull'` | `This column does not allow NULL` |
| 22 | `'cellEditor.valueTooLarge'` | `This value is too large to edit in the grid` |
| 23 | `'cellEditor.binaryNotEditable'` | `Binary values cannot be edited in the grid` |
| 24 | `'cellEditor.discardedUncommitted'` | `Your edit was discarded because the row is no longer displayed` |
| 25 | `'cellEditor.readOnlyTitle'` | `Cell is read-only` |
| 26 | `'cellEditor.readOnly.connection'` | `This connection is read-only, so cells cannot be edited.` |
| 27 | `'cellEditor.readOnly.driverCapability'` | `This database driver does not support writing.` |
| 28 | `'cellEditor.readOnly.view'` | `Views and materialized views are read-only in the data grid.` |
| 29 | `'cellEditor.readOnly.noPrimaryKey'` | `This table has no primary key, so a row cannot be identified for an update.` |
| 30 | `'cellEditor.readOnly.ambiguousRowIdentity'` | `This row cannot be identified uniquely (its key is NULL or duplicated).` |
| 31 | `'cellEditor.readOnly.autoIncrement'` | `Auto-increment columns cannot be edited in place.` |
| 32 | `'cellEditor.readOnly.generated'` | `Generated columns cannot be edited.` |
| 33 | `'cellEditor.readOnly.typeNotEditable'` | `This column type cannot be edited in place in this database.` |
| 34 | `'cellEditor.readOnly.binaryColumn'` | `Binary columns cannot be edited in place.` |
| 35 | `'cellEditor.readOnly.largeValue'` | `This value is too large to edit in place.` |

`readOnlyReasonMessageKey(reason)` 负责 `reason → 'cellEditor.readOnly.<reason>'` 的映射（`connectionReadOnly` → `'cellEditor.readOnly.connection'`，其余同名），**唯一映射表**，测试逐条断言。

---

## 9. 测试清单

### 9.1 纯逻辑单测（`npx vitest run`）

| 文件 | 用例名与断言要点 |
| --- | --- |
| `src/lib/__tests__/cellTypes.test.ts` | `normalizes representative type names from every driver`（**表驱动**，覆盖 postgres `int4`/`text`/`timestamptz`/`bytea`/`jsonb`/`uuid`/`USER-DEFINED`/`ARRAY`/`timestamp with time zone`、mysql `int(11) unsigned`/`tinyint(1)`/`enum('a','b')`/`mediumblob`/`varchar(255)`、sqlserver `nvarchar(max)`/`uniqueidentifier`/`datetime2(7)`/`timestamp`、clickhouse `Nullable(Int32)`/`LowCardinality(String)`/`DateTime64(3)`/`FixedString(16)`/`Enum8('a'=1)`、duckdb/sqlite `INTEGER`/`TEXT`/`REAL`/`NUMERIC`）；`does not classify geometry or network types as numbers`（`point`/`polygon`/`inet`/`cidr`/`macaddr` → `text`，`point` 是必守回归）；`prefers datetime over date for combined names`；`returns unknown for unclassified and empty input`；`driver overrides win over the shared table`（`timestamp` + sqlserver hints → `bytes`；`tinyint(1)` + mysql hints → `bool`）；`every cell type projects to a data type family` |
| `src/lib/__tests__/cellEditorKind.test.ts` | `dispatches one editor kind per cell type`（表驱动）；`uses the text editor for zoned datetimes`；`routes long text and binary to their dedicated kinds`；`coerceValue maps empty text to null for nullable columns`；`coerceValue refuses empty text on NOT NULL columns`；`rejects javascript numeric quirks`（`''`/`' '`/`'0x10'`/`'١٢'`/`'NaN'`/`'Infinity'`/`'1e400'`）；`rejects invalid calendar dates`（`2026-02-30`、`2026-13-01`、`'lorem'`）；`rejects malformed json but accepts json strings`；`preserves zone-less wall-clock timestamps verbatim`；`resolveCellEditable returns a specific reason for each read-only case`（10 条逐条） |
| `src/lib/__tests__/gridErrors.test.ts` | `classifies every documented grid error by prefix`（含 `grid.cell.readOnly`）；`returns unknown for unrecognized messages and keeps the original text`；`readOnlyReasonMessageKey is total over CellReadOnlyReason` |
| `src/lib/__tests__/cellEditorCandidates.test.ts` | `quotes identifiers per dialect`（含列名内有引号/反引号）；`always limits to 200 rows and stays a single statement`；`refuses an empty column name` |
| `src/lib/__tests__/dataTypeColors.parity.test.ts` | `classifyDataType is the projection of resolveCellType for representative types` |
| `src/lib/__tests__/tableChanges.test.ts`（DB-04 的文件，本册加用例） | `normalizes value-null into the null variant` |
| `src/components/DataTable/editors/__tests__/setValueMenu.test.ts` | `hides Set NULL for NOT NULL columns`；`shows Use column default only for insert rows`；`shows Revert this cell only when the cell has a pending change`；`never exposes NOW or CURRENT_TIMESTAMP`（防止有人把 SQL 表达式当值提交） |

### 9.2 组件测试（RTL）

| 文件 | 用例名与断言要点 |
| --- | --- |
| `editors/__tests__/CellEditorHost.component.test.tsx` | `does not commit while an IME composition is active`（`compositionstart` → `change('日本語')` → `keyDown Enter`（`isComposing: true`）→ 断言 `onCommit` **未**调用 → `compositionend` → `keyDown Enter` → `onCommit` 调用**一次**；判定走 01 的 `isComposingEvent`，因此两种形态各写一条：`e.isComposing` 与 `e.nativeEvent.isComposing`）；`does not cancel the editor when Escape ends a composition`；`does not commit when focus moves into an editor-owned portal`（模拟焦点进入挂在 `body` 上的弹层节点）；`does not commit when the context menu steals focus`；`commits once on blur` |
| `editors/__tests__/NumberCellEditor.test.tsx` | 非法输入标红且 `onCommit` 不触发；合法输入提交 `{kind:'value',value:42}` |
| `editors/__tests__/BooleanCellEditor.test.tsx` | 可空列三项（NULL/true/false）各自产出对应 `CellWrite`；非空列 Checkbox 两态 |
| `editors/__tests__/EnumCellEditor.test.tsx` | 打开时**不发** `loadCandidates`；点击后才调用一次；候选写入后可在下拉中搜索并选中 |
| `editors/__tests__/TemporalCellEditor.test.tsx` | `date`/`time`/`datetime` 三种 kind 产出规范串；带 zone 的列走文本编辑器 |
| `editors/__tests__/JsonCellEditor.test.tsx` | 非法 JSON 阻止提交并显示行内错误；格式化/压缩按钮不改变语义 |
| `__tests__/cellEditor.journey.test.tsx`（**连续旅程**） | `journey: double click, type, Enter commits a value write`；`journey: double click, retype, Escape writes nothing`；`journey: Chinese IME selection then Enter commits the composed text once`（逐步击键，含组合中间态）；`journey: Tab commits and requests the next cell`；`journey: clicking another cell commits the current one first`；`journey: an invalid value blocks the move and keeps the editor open`；`journey: Set NULL then Set empty string produce different writes on the same cell` |
| `src/components/DataTable/__tests__/DataTable.test.tsx`（既有 + 新增） | 既有用例零改动通过；新增 `routes the new Set Value submenu when onSetValue is provided`、`keeps the legacy more-actions menu when it is not` |
| `src/windows/connection/__tests__/TableView.editors.test.tsx` | `double click on a table without a primary key explains the reason and does not start editing`；`a view panel reports the view reason, not the connection reason` |

### 9.3 三态映射（store）

`src/stores/__tests__/tableDataStore.cellWrite.test.ts`：
- `stageCellWrite with Null stores null and lists the column as changed`；
- `stageCellWrite with Value stores the value and lists the column`；
- `stageCellWrite with Unset removes the column change and restores the original row value`；
- `stageCellWrite with Unset on a row without pending changes is a no-op and never creates an all-unset entry`（断言 `pendingChanges.size === 0`，对应契约册 §3.6）；
- `stageCellChange keeps today's behaviour bit for bit`（与 `stageCellWrite(..., cellWriteValue(value))` 结果一致）；
- `empty string and null produce different pending values`。

### 9.4 E2E（`e2e/specs/table-editors.ts`，新增）

`pnpm e2e:skip-build -- --spec e2e/specs/table-editors.ts`；断言只用 `data-testid`（`tid`）、`data-dt-row`/`data-dt-col`、`data-dt-cell-type`（仅断言存在与所属族，见 §11 未决 #14）、`data-dt-editing`，**禁止几何坐标**；只读断言用「`grid.cell.readOnly` 与具体原因文案」，**不依赖尚未获批的 `data-dt-readonly`**（§11 未决 #7）：
1. 数值列输入非法值 → 编辑器保持打开、行内错误可见、无待提交计数；
2. 布尔列三项切换 → 待提交计数变化；
3. JSON 列非法输入阻止提交，合法输入提交后落库（提交链路复用既有 `table-edit.ts` 的提交步骤）；
4. 只读连接/视图：双击单元格出现**具体原因**文案，且 `data-dt-readonly` 有值；
5. `Set NULL` / `Set empty string` / `Revert this cell` 三者的最终落库值可区分（需要一个既有种子表提供可空文本列）。

### 9.5 必须同步修改的既有测试（要么零改动通过，要么显式改写）

- `src/components/DataTable/__tests__/EditableCell.component.test.tsx`：断言的是旧 `EditableCell` 的类名与「`integer`/`boolean`/`numeric`/`jsonb` 强制转换」。这些断言**迁移**到 `editors/__tests__/*`；`EditableCell` 变兼容壳后，凡断言 `h-7`/`font-mono`/Enter/Escape 的用例应改为渲染 `CellEditorHost`。**不得**通过删除断言让它变绿。
- `src/components/DataTable/__tests__/EditableCell.test.ts`：`toEditString` 的导入路径必须继续有效（Step 6 的再导出）。
- `src/lib/__tests__/dataTypeColors.test.ts`：**零改动**通过（这是对齐 `classifyDataType` 的验收证据）。
- `src/components/DataTable/__tests__/DataTable.test.tsx`、`src/lib/__tests__/dataTableContextMenu.test.ts`：`set-null` 在 `more-actions` 内的既有断言**零改动**通过（靠「仅当未提供 `onSetValue` 时走旧布局」保证）。

---

## 10. 自查清单

| # | 错误做法 | 正确做法 | 后果 |
| --- | --- | --- | --- |
| 1 | 在新编辑器里再写一遍 `type.includes('int')` 之类的子串判断 | 一律经 `resolveCellType` / `resolveCellEditorKind`；类型规则只存在于 `cellTypes.ts` 与驱动 `meta.ts` | `point` 被判成数字、`datetime` 被 `date` 抢走；第 4 处、第 5 处分派互相漂移（宿主今天已有 4 处） |
| 2 | 用 `null` 表达「不写这一列」，或在输入框里用空串表达 NULL | 三态用 `CellWrite`；`{kind:'value',value:null}` 必须经 `normalizeCellWrite` 折叠为 `{kind:'null'}` | 用户「把一列改成 NULL」在某个 `.filter(Boolean)` 之后静默变成「这一列没改动」，提交后数据库里还是旧值 |
| 3 | 打开 JSON/日期编辑器时顺手拉一次枚举候选值或分区信息 | 只在用户显式点击「加载候选值」时发**一次** `SELECT DISTINCT … LIMIT 200` | 大表上每次双击都触发一次全表 DISTINCT，网格「点一下就卡」 |
| 4 | 日期用 `new Date(text).toISOString()` 归一化 | 无 zone 的库文本是墙钟，原样透传；带 zone 的列 v1 只给文本编辑器 | 每格时间戳整体平移本地 UTC 偏移并凭空多出一个 `Z`（`formatTimestamp` 的注释已记录过这个缺陷） |
| 5 | 沿用「`JSON.parse` 失败就把原文当值提交」 | 非法 JSON **阻止提交**并给行内错误 | 隐性数据损坏：JSON 列里被写进一段非 JSON 文本，直到下游解析才炸 |
| 6 | 在 `onBlur` 里无条件提交 | blur 提交前排除三种失焦：编辑器自有 portal、页内右键菜单抢焦点、IME 组合中 | 「点日历选日期」会先提交并关闭编辑器，用户永远选不了日期 |
| 7 | 只判 `e.isComposing`，或自己再写一份判定 | **复用 01 的 `isComposingEvent(e)`**（它已同时兼容 `e.isComposing` 与 `e.nativeEvent.isComposing`），**再加**本地 `compositionstart/end` 维护的 ref 供 `blur` 使用；不得在 `CellEditorHost` 里另写一份 `isComposing` 判定 | 中文/日文输入法选词那一下 `Enter` 被当成提交，写进半成品文本（`EditableCell` 今天正是如此）；两份判定漂移后其中一条路径必然漏判 |
| 8 | 直接 `Number(raw)` 或 `parseFloat(raw)` | 先用 ASCII 数字正则前置校验，再 `Number()`，整数列再 `Number.isInteger` | `Number('')===0`、`Number(' ')===0`、`Number('0x10')===16`、`Number('١٢')===12` 全部静默写入错误数值 |
| 9 | 把 `Unset` 在 UPDATE 语境下也叫「Set DEFAULT」 | UPDATE 显示 **Revert this cell**（撤销该格改动），INSERT 才显示 **Use column default** | `Unset` 不会重新应用 DDL 默认值；文案错误会让用户以为「恢复默认值」，实际只是撤销 |
| 10 | 顺手把 `stageCellChange` 的 `value: unknown` 改成 `CellWrite` | 新增 `stageCellWrite`，`stageCellChange` 变薄壳（行为逐字节不变） | `applyColumnToRows`、DB-03 粘贴、既有 store 测试全线连带改动，一次提交同时改两条写路径 |
| 11 | 让编辑器自己实现「Tab 到下一格」/ 自己维护焦点 | 提交后调 `onCommitAndMove`，焦点与区域一律走 01 的 `gridSelection.ts` + `useGridSelection.ts` | 出现第二份焦点真相，Shift 连选/翻页后两套状态必然打架（契约册 §4.2 明令） |
| 12 | 用行下标拼写操作定位 | 走 `rowIdentityAnchors` → `pendingChanges` → `rowIdentity`（契约册 §4.3） | 翻页/筛选后 `rowIndex` 含义整体平移，可能改到**另一行**（TablePlus 出过无 WHERE UPDATE 的同类事故） |

---

## 11. 未决问题

| # | 问题 | 建议选项 |
| --- | --- | --- |
| 1 | `NOW()` / `CURRENT_TIMESTAMP` 快捷项无法用冻结的 `CellWrite` 三态表达（`Value('CURRENT_TIMESTAMP')` 会写入字面字符串） | 建议 **v1 隐藏这两个入口**（R4：能力缺失就隐藏），并在契约册 §3 记一条待办：是否新增 `CellWrite::Expression(String)` + 驱动侧渲染。备选：插入调用方本地墙钟文本，并在 UI 上明确标为「值」 |
| 2 | 可空布尔列的三态控件：3 项 `Select` vs `Checkbox` + 修饰键/菜单置 NULL | 建议 **3 项 `Select`**（零歧义、可键盘操作、复用既有 primitives）；`Checkbox` 只在非空列用 |
| 3 | 带时区列（`timestamptz` / `datetimeoffset`）：v1 只给文本编辑器 vs 给选择器并在写回时拼回原 offset | 建议 **v1 只给文本编辑器**（选择器会静默改瞬时）；若产品坚持要选择器，必须新增「保留原 offset」的显式行为与其单测，并把 `TemporalValueInput` 的 zone 支持作为独立条目排期 |
| 4 | 方言陷阱（SQL Server `timestamp`/`rowversion`、MySQL `tinyint(1)`、ClickHouse `FixedString`）靠共享表还是靠驱动 `cellTypes` 覆盖 | 建议 **共享表负责通用 SQL 名，驱动覆盖负责方言陷阱**；本册必须为 sqlserver 的 `timestamp` 与 mysql 的 `tinyint(1)` 各写**一条驱动 hints 的单测**（不需要真驱动） |
| 5 | 生成列（generated/computed）的识别：`ColumnSchema`（`packages/driver-sdk`）没有 `isGenerated` 字段；SQL Server 的 `columns_sql` 虽已查出 `is_computed`/`generated_always_type`，但没有映射到 `ColumnSchema` | 建议本批只落地「自增列」判定；生成列作为 `driver-api` 的**新增可选字段**（`is_generated`，`#[serde(default)]`，不需要提升 `PROTOCOL_VERSION`）单独立项。本册把 `generated` 原因保留在联合类型里，当前恒不触发 |
| 6 | 大值（> 64 KiB）的就地编辑：PRD DB-13 提到的 `load_cell_value` **不在**契约册 §5 冻结的 5 个新方法内 | 建议 **v1 不新增后端入口**：大值只读展示 + 尺寸 + 「复制到文件」，等 DB-13 落地专用入口后再开编辑。理由是契约册只冻结了 5 个方法，本册不得越权发明第 6 个 |
| 7 | 新增 DOM 属性 `data-dt-readonly="<reason>"`（测试与 E2E 需要；契约册 §4.4 与 01 的 §3.7 都只冻结了 `data-dt-selected` / `data-dt-dirty` / `data-dt-editing` / `data-dt-cell-type`，**尚未**包含它） | 建议**并入契约册 §4.4 与 01 的 `GridCell`**（与其同族，只增不改，输出点仍在 `GridCell`）；在获批前本册的测试不得依赖它，只读原因改用「`grid.cell.readOnly` + 具体文案」断言 |
| 8 | 无效输入是否需要独立错误码（如 `grid.cell.invalidValue`） | 建议 **v1 不进契约册 §8.2**：编辑器行内错误 + 「N 格待修正」计数已足够。若 DB-06/DB-10 需要跨面板统一呈现「哪些格待修正」，再作为 §8.2 的新条目提案 |
| 9 | `DataTable.tsx` 与 `TableView.tsx` 的行数预算（DB-02/03/06/10 都会动这两个文件） | **已由 01 的前置抽取解决**：01 的 Step 9 把右键菜单搬到 `useDataTableContextMenu.ts`（`DataTable.tsx` 628 → ~450），Step 10 把暂存条搬到 `TablePendingChangesBar.tsx`（`TableView.tsx` 770 → ~660），Step 3/4 把选择转移搬到 `selectionActions.ts`（`tableDataStore.ts` 722 → ~700）。本册因此在其基线上分别净增 ~45 / ~25 / ~50，**不再触红线**；但 02/03/06/10 仍须各自核算，集成方统一复核 |
| 10 | 01 的接口面是否够本册用 | **已确认够用（01 已落盘）**。本册消费：`gridSelection.ts` 的 `CellCoord` / `CellRange` / `GridSelection` / `EMPTY_GRID_SELECTION` / `normalizeCellRange` / `gridSelectionContains` / `rowsCoveredBySelection(selection, columnOrder): number[]` / `moveCellFocus` / `selectionFromAnchorFocus`；`useGridSelection.ts` 的 `surfaceProps` / `isCellSelected` / `activeDescendantId` / `focusKey`；`GridCell` 与 `GridCellProps`；store 的 `setGridSelection` / `clearGridSelection` / `beginCellSelection` / `reconcileGridSelectionForColumns`；`gridEventGuards.ts` 的 `isComposingEvent` / `isEditableTarget`。**不需要 01 再新增任何导出**——本册的「Tab 提交后进入下一格」用 `moveCellFocus`（纯函数）+ `setGridSelection`（写回）+ 本册的 `requestEdit(coord)` 即可表达。**唯一仍然需要 02 的**：把 `Enter` / `Tab` 接进 `surfaceProps.onKeyDown` 并调用本册暴露的 `requestEdit`；本册只提供入口，不注册网格级快捷键 |
| 11 | Set Value 是否作用于整个选择区域（多格） | 建议 **v1 只作用于右键命中的那一格**（`multiRowSelection` 时沿用既有的「不做单格动作」规则），多格批量走 DB-03 粘贴路径；理由是批量 `Set NULL` 需要一个明确的「N 格将被改为 NULL」确认，属独立交互设计。若将来要做多格，目标集必须取 01 的 `rowsCoveredBySelection` / `selectionRanges`，**不得**把派生行集写回 `selectedRows`（契约册 §4.3） |
| 12 | 枚举候选值的取数实现：宿主拼 `SELECT DISTINCT` vs 新增 Driver Command | 建议 **宿主拼**（复用 `queryCommands.executeQuery` + `escapeIdent`，已有「`formatRowAsSqlInsert` 在宿主拼 SQL」的先例），但必须写清边界：只构造单条 `SELECT DISTINCT`、只用于候选值、不参与写路径；若后续有驱动声明「不支持 DISTINCT 列采样」，再升级为 Driver Command |
| 13 | ~~契约册 §7 `DataGridCapabilities.editable` 缺省值矛盾~~ | **已按本册建议冻结（契约册 §7 已改）**：字段名冻结为 `DatabaseTypeMeta.dataGrid?: DataGridCapabilities`，`editable` **缺省 `true`**（只有显式 `false` 才关闭写入，等价于今天 `isEditable = !isConnectionReadOnly`，符合 R4）。本册 §3.7 与正文已同步，不再需要「字段名未冻结时的退路」兜底逻辑 |
| 14 | `data-dt-cell-type` 的**取值粒度**：01 的 §3.7 把它冻结为 `classifyDataType(col.type)` 的结果（渲染族 `bool`/`number`/`datetime`/`json`/`binary`/`text`/`null`），而本册的类型感知编辑器需要一个更细的 `CellType`（区别 `date`/`time`/`datetime`、`bytes`/`text`、`enum`/`uuid`/`unknown`） | 建议**统一为 `CellType`**：渲染族是它的投影（本册 Step 5 已把 `classifyDataType` 实现成该投影），因此把属性值换成 `CellType` 是**信息增强而非语义变更**，01 的断言只需把期望值从族名换成细化名。**本册不单方面改 01 的 `GridCell`**：编辑器分派只依赖 props 的 `cellType`，不读该属性；请在 00 册裁定后由 01 一次性改 `GridCell`（若 00 维持族粒度，本册的 E2E 只断言「属性存在」而不断言具体值，`unknown` 的兜底信号改由 `data-dt-cell-type` 之外的方式表达） |
