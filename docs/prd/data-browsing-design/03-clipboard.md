# 03 · 剪贴板复制与粘贴（DB-03）

> **状态**：目标设计，**未实现**。本册描述的是「要写成什么样」，文中所有标「新增」的符号当前在仓库里都不存在；标「修改」的符号存在，且第 2 节逐条给出了我读到的现状。
>
> **依赖**：`00-contracts.md`（强制约束：第 3 节 `CellWrite` 三态、第 4 节坐标系与选择模型、第 6 节 IPC、第 8 节错误码、第 10 节分册模板）、`01-selection.md`。`01` 尚未落盘，本册**按 `00` 第 4 节（含 2026-10 增补）冻结的接口形状消费它**，不改其定义。本册消费 `01` 的**两个**文件——`src/stores/tableData/gridSelection.ts`（纯逻辑：`GridSelection` / `CellCoord` / `CellRange` / `normalizeCellRange` / 区域求交合并 / 行集推导）与 `src/hooks/useGridSelection.ts`（React 接线），以及派生选择器 `rowsCoveredBySelection(state): ReadonlySet<number>`。**本册不复制这两处的区域遍历逻辑，也不把派生行集写回 `selectedRows`**（`00` §4.2 第 3 条）。`01` 落地后若形状有变，本册的适配点集中在 `src/lib/gridClipboard.ts` 的 `toClipboardRange` 与 `src/hooks/useGridClipboard.ts` 的两个调用点。
>
> **被谁依赖**：`10-result-grid`（DB-14，查询结果网格可编辑，依赖 01/02/03/04/05/06）、DB-10（复制格式补齐 HTML / Markdown / Plain，依赖本册的 `serializeCells`）。
>
> **预估人日**：5.5 人日（纯函数与单测 2.0、store 批量化与既有 `stageCellChange` 等价重构 1.0、hook 与 DOM 接线 1.0、右键菜单与 i18n 0.5、E2E 与 journey 1.0）。
>
> **本文档不包含什么**：不设计 HTML / Markdown / Plain 复制格式（DB-10）；不设计新增行 INSERT（DB-04，本册只消费它的 `CellWrite` 三态并把「粘贴行」的入口交给它）；不设计类型感知编辑器的 `coerceValue`（DB-05，本册只声明它的入参形状与「非法值不静默吞掉」的下游要求）；不设计单元格/区域选择模型本身（DB-01，本册只消费）；不改任何 Rust 代码、不改 `driver-api`、不新增 IPC 命令（剪贴板读写命令已存在，见第 2 节）；不实现撤销/重做。

---

## 1. 目标与验收口径

**一句话目标**：让数据网格的复制/粘贴与外部表格工具（Excel / Numbers / Google Sheets / 文本编辑器）行列对齐地互通，且粘贴**只能**经由既有的暂存改动 → 计划 → 指纹 → 提交链路落库，绝不开辟第二条写入通路。

### 1.1 可验证验收标准

| # | 命令 | 期望输出 / 断言 |
| --- | --- | --- |
| A1 | `npx vitest run src/lib/__tests__/gridClipboard.test.ts` | 退出码 `0`；`Test Files  1 passed (1)`；`Tests` 全部 `passed`，含第 9 节列出的畸形文本用例 |
| A2 | `npx vitest run src/stores/tableData/__tests__/stageCellWrite.test.ts` | 退出码 `0`；断言「单格 `stageCellWritesInto` 与既有 `stageCellChange` 产出的 `rows` / `pendingChanges` / `rowIdentityAnchors` **深度相等**」（等价性回归）；断言 5 000 格批量只产生 **1** 次状态写入 |
| A3 | `npx vitest run src/components/DataTable/__tests__/clipboard.journey.test.tsx` | 退出码 `0`；journey 用例逐击键断言：聚焦单元格 → `⌘+V`（派发 `paste`）→ 待提交计数 `{cells}` → 又一次粘贴覆盖 → `⌘+C`（派发 `copy`）→ `clipboardData` 文本与源区域逐字符相等 |
| A4 | `npx vitest run src/lib/__tests__/gridErrors.test.ts` | 退出码 `0`；`classifyGridError('grid.paste.tooLarge: ...')` 返回 `'grid.paste.tooLarge'`；`classifyGridError('UPDATE ... affected 3 rows')` 返回 `'unknown'`（未知**不得**吞掉，UI 回退显示原文） |
| A5 | `pnpm typecheck` | 退出码 `0`，`error TS` 计数为 `0`；本册新增文件无 `any` |
| A6 | `npx vitest run src/lib/__tests__/dataTableContextMenu.test.ts` | 退出码 `0`；既有断言全绿（回归门槛：新增菜单项不得改变既有 `ids()` 序列在只读场景下的取值），并新增 `copy-selected-cells` / `paste-selected-cells` 的 `enabled` 断言 |
| A7 | `pnpm e2e:skip-build -- --spec e2e/specs/table-clipboard.ts` | 退出码 `0`；用例名与断言见第 9.4 节 |
| A8 | 手工（黑盒，`test/`） | 从 Excel 复制 3 行 × 4 列（其中一格为 `"a,b"`、一格为跨行引号文本）→ 网格聚焦单格 `⌘+V` → 12 格值与类型正确、待提交计数为 12；随后区域复制 `⌘+C` → 粘回 Excel → 行列完全对齐 |

> **A8 是唯一不能自动化的验收项**（WKWebView 的 WebDriver 无法可靠驱动系统剪贴板，见第 7 节「跨页选择」与第 11 节 U-6）。A1~A7 必须全绿才允许合并。

### 1.2 不变量（本册不得破坏）

1. **唯一写通路**：粘贴 → `pendingChanges` → `preview_pending_changes` → `commit_pending_changes`。任何「直接执行 SQL」的粘贴实现都是返工。
2. **`rowIndex` 不得进入 SQL**（`00` §4.3）。粘贴的落点是 `rowIndex`，写入定位永远是 `rowIdentityAnchors` → `PendingRowChange.rowIdentity`。
3. **`CellWrite` 三态不得被压成两态**（`00` §3）。「不写」与「写 NULL」在粘贴路径上必须是两个不同的动作。
4. **`stageCellChange` 的对外行为逐字节不变**。本册要重构它的内部实现（第 5 节 Step 4/5），既有测试与本册 A2 的等价性断言共同守住这条。

---

## 2. 现状代码事实

只列**我亲自读过**的符号。位置一律「文件路径 + 符号名」。

### 2.1 可复用 / 必须复用的既有能力

| 文件路径 | 符号 | 读到的事实 |
| --- | --- | --- |
| `src/lib/dataTableContextMenu.ts` | `serializeDataTableRowsAsTsv` | 唯一实现是 `row.map(cell => cell == null ? '' : String(cell)).join('\t')`——**零转义**：值里含制表符会产生多余列，含换行会产生多余行；对象值走 `String(obj)`，得到 `[object Object]` |
| `src/lib/dataTableContextMenu.ts` | `serializeDataTableRowsAsCsv` | 有表头；`escapeCsvCell` 只在含 `,` / `"` / `\n` / `\r` 时加引号，`"` 双写；对象值走 `JSON.stringify` |
| `src/lib/dataTableContextMenu.ts` | `serializeDataTableRowsAsJson` | 调 `rowToNamedRecord`，输出**对象数组**（带列名），不是二维数组 |
| `src/lib/dataTableContextMenu.ts` | `rowToNamedRecord` | `obj[name] = row[i] ?? null`——按**位置**取 `row[i]`，列名只作为键；`null` / `undefined` / 缺失都塌成 `null` |
| `src/lib/dataTableContextMenu.ts` | `serializeDataTableColumnValues` | 整列序列化；对象值走 `JSON.stringify`（与 TSV 那条路径**不一致**） |
| `src/lib/dataTableContextMenu.ts` | `formatRowAsSqlInsert` / `formatRowAsSqlUpdate` / `formatRowsAsSqlInsert` / `formatRowsAsSqlUpdate` | 复制用 SQL 文本。**它们是剪贴板输出，不是写入通道**；`formatRowAsSqlUpdate` 在无主键列时**静默退化**为「第一列做 WHERE」，因此它绝不能当作权威定位依据 |
| `src/lib/dataTableContextMenu.ts` | `resolveDataTableCellFromEvent` | `target.closest('[data-dt-row][data-dt-col]')` 反查单元格；`Number.isInteger` 校验行下标 |
| `src/lib/dataTableContextMenu.ts` | `resolveDataTableHeaderColFromEvent` | `target.closest('[data-col-header]')` 反查表头列名 |
| `src/lib/dataTableContextMenu.ts` | `buildDataTableContextMenuItems` | 三种菜单布局（表头 / 单元格 / 选择集）。单元格布局已把低频项收进 `more-actions` 子菜单。用 `item()` 工厂函数——**`action` 为 `undefined` 时该项直接被丢弃**（不是置灰） |
| `src/lib/dataTableContextMenu.ts` | `DataTableContextMenuLabels` / `DataTableContextMenuHandlers` | 标签与处理函数是两个平铺类型，新增项必须同时改这两个类型 + `buildDataTableContextMenuItems` 的两个分支 |
| `packages/driver-sdk/src/types/menu.ts` | `NativeMenuItemDef` | `kind: 'item'` 支持 `shortcut?: string` 与 **`enabled?: boolean`**——置灰是既有能力，不需要新造 |
| `src/lib/fetchRelationDdl.ts` | `copyToClipboard` | **三级降级链**：`navigator.clipboard.writeText` → `invoke('write_clipboard')` → `document.execCommand('copy')`；返回 `boolean`；**`copyToClipboard('')` 直接返回 `false`** |
| `packages/ui/src/useCopyFeedback.ts` + `src/components/ui/useCopyFeedback.ts` | `useCopyFeedback` | 请求绑定的乐观反馈（`{ copied, copy }`，`copy(text)` 返回 `void`）；文件注释**明确声明它没有降级链**，并指明「需要降级的调用点继续用 `src/lib/fetchRelationDdl.ts` 的 helper」 |
| `packages/backend-client/src/transport.ts` | `PlatformServices.writeClipboard` / `PlatformServices.readClipboard` | 平台能力接口，读写各一个方法 |
| `packages/backend-client/src/transport.ts` | `requirePlatformServices` | 未绑定时**抛错**（不静默降级）。`readClipboard` 目前在 `src/` 内**没有任何调用方**（我 grep 过 `writeClipboard` / `readClipboard`） |
| `src/platform/tauriBackendTransport.ts` | `createDesktopPlatformServices` | 桌面实现：`writeClipboard` → `invoke('write_clipboard')`，`readClipboard` → `invoke('read_clipboard')` |
| `src-tauri/src/commands/clipboard.rs` | `write_clipboard` / `read_clipboard` | 两个 Tauri 命令**已存在**，走 `tauri_plugin_clipboard_manager` 的 `ClipboardExt`；已在 `src-tauri/src/bootstrap/run.rs` 的 `generate_handler!` 里注册。**本册不需要新增任何 IPC 命令** |
| `src/components/connection/useConnectionClipboardFill.ts` | `useConnectionClipboardFill` | 项目里**既有的原生粘贴先例**：`window.addEventListener('paste', onPaste)` + `event.clipboardData?.getData('text/plain')`；并且它的注释记录了实测结论——macOS 上**没有用户手势**的自动 `navigator.clipboard.readText()` 会走粘贴授权路径并**卡住渲染进程**（连接表单冻结、WebDriver click 挂起） |
| `src/components/sql-editor/paste/parseDelimitedValues.ts` | `parseDelimitedValues` / `detectDelimiter` / `MAX_SOURCE_BYTES` / `MAX_VALUE_COUNT` | 一维解析（分隔符 → 扁平 `string[]`），`trim` 默认 `true`，`emptyItems` 默认 `'skip'`（**丢弃空项**），上限 1 MiB / 10 000 值。**不能**用于二维网格粘贴：它丢结构、丢空格 |
| `src/lib/exportData.ts` / `src/lib/exportStream.ts` | 各自的局部 `escapeTSV` | 与 `serializeDataTableRowsAsTsv` **第三套**策略：制表符与换行**替换为空格**、`\r` 删除。于是仓库里 TSV 转义有三份实现、两种策略，而 `serializeDataTableRowsAsTsv` 那份连转义都没有 |
| `src/hooks/useKeyboardShortcuts.ts` | `matchShortcut` / `useKeyboardShortcuts` | 修饰键**精确匹配**；命中即 `e.preventDefault()` 并 `return`（只执行第一条）；`scope: 'table'` 在 `input` / `textarea` / `contentEditable` 内**跳过** |
| `src/windows/connection/ContentView.tsx` | `useKeyboardShortcuts` 调用点 | 全仓仅此一处注册快捷键，两条都是 `scope: 'global'`（新建查询 / 关闭标签页）。**网格域没有任何注册** |
| `src/windows/connection/TableView.tsx` | `handleTableKeyDown` | 直接在根 `div` 的 `onKeyDown` 上处理 `mod+enter` 提交；**不经过** `useKeyboardShortcuts` |
| `src/components/DataTable/DataTable.tsx` | `handleDeleteKey` | 同样的「根 div `onKeyDown`」模式，只处理 `Delete` / `Backspace` 删行 |
| `src/components/DataTable/DataTable.tsx` | `DataTable` 内的局部 `copyText` | **现状复制路径的全部**：`void navigator.clipboard.writeText(text)`。没有 `copyToClipboard` 的降级链、没有 `.catch()`（未处理的 rejection）、没有成功/失败反馈 |
| `src/components/DataTable/DataTable.tsx` | `DataTableProps.getContextCellText` + `handleContextMenu` 内的 `cellTextForCopy` | 取值优先级是「命中的单元格值 → `getContextCellText()` → `window.getSelection()?.toString()`」。前两者是内存里的**原始值**，**第三者是 DOM 里的可见文本**——于是它会把 `CellRenderer` 截断后的字符串复制出去（见下一条） |
| `src/components/DataTable/VirtualBody.tsx` | `VirtualRow` | 单元格挂 `data-testid="data-table-cell"` + `data-dt-row` + `data-dt-col`（`data-dt-col` 是**列名**）；单元格 `onDoubleClick` 里 `stopPropagation()`，但**单击不 stopPropagation**，所以点单元格会冒泡到行 → `onRowSelect` |
| `src/components/DataTable/VirtualBody.tsx` | `VirtualBody` | 行由 `useVirtualTable` 虚拟化（`overscan: 12`）；行 `div` 带 `tabIndex={0}`，滚动容器**没有** `tabIndex` |
| `src/components/DataTable/CellRenderer.tsx` | `CellRenderer` | `null` / `undefined` 渲染为斜体 `NULL`（`--dt-null`），因此**组件层能区分 NULL 与空串**，是 `serializeDataTableRowsAsTsv` 把它们塌成同一个 `''` |
| `src/components/DataTable/CellRenderer.tsx` | `CellRenderer` 的文本 / json 分支 | 渲染文本**硬截断到 120 字**并以 `…` 收尾（`jsonText.length > 120 ? \`${jsonText.slice(0, 120)}…\` : jsonText`，普通文本分支同构；完整值放在 `title` 里）。因此**屏幕上的单元格文本可能短于真实值**，而 `window.getSelection()` 只能拿到屏幕上那份。裁定见 §3.8 |
| `src/components/DataTable/EditableCell.tsx` | `EditableCell` / `coerceAndCommit` / `toEditString` | 编辑态是单行 `<input>`，`onBlur` 提交；`onKeyDown` 的 `Enter` 分支**没有** `isComposing` 检查（IME 缺陷，DB-02 负责） |
| `src/components/DataTable/TableHeader.tsx` | `ColumnDef` | `{ id: string; name: string; type?: string }`——**只有类型字符串，没有 `nullable` / `isPrimaryKey` / `isAutoIncrement`**，不足以驱动粘贴的列级规则 |

**本册依赖但不拥有的选择模型文件（DB-01 新增，我读到的现状是「尚不存在」）**

| 文件路径 | 符号 | 契约（`00` §4，2026-10 增补） | 本册怎么用 |
| --- | --- | --- | --- |
| `src/stores/tableData/gridSelection.ts` | `CellCoord` / `CellRange` / `GridSelection` / `normalizeCellRange` / 区域求交合并 / 行集推导 / 键构造 | 纯逻辑，不依赖 React，可独立单测 | **区域遍历一律调它**：`toClipboardRange` 内部先 `normalizeCellRange`，再借用它的行集推导把区域展开成坐标；`planPaste` 的「矩阵 ∩ 区域」求交复用它。本册**不写任何 `for (row) for (col)` 的区域遍历** |
| `src/hooks/useGridSelection.ts` | `useGridSelection` | React 接线，接 `TableState`、订阅鼠标键盘、产出记忆化选择器 | `DataTable` 的 `gridSelection` prop 由它供给；本册只读不改 |
| `src/stores/tableData/gridSelection.ts`（或 `01` 指定的导出位置） | `rowsCoveredBySelection(state): ReadonlySet<number>` | `mode==='row'` → `selectedRows`；`mode==='cell'` → 区域（含 `extraRanges`）覆盖的行下标集合（派生、不落库）；`mode==='none'` → 空集 | **所有行级取值域都用它**：整行复制、`⌘+D` 复制行、删除行、导出选中。本册**禁止**把它的返回值写回 `selectedRows`（`00` §4.2 第 3 条） |

### 2.2 状态与写入链路

| 文件路径 | 符号 | 读到的事实 |
| --- | --- | --- |
| `src/stores/tableData/types.ts` | `TableState` | `rows: Record<string, unknown>[]`（**按列名索引的记录**）；`visibleColumns: string[] \| null`；`selectedRows: Set<number>`；`editingCell`；`pendingChanges`；`rowIdentityAnchors`；`requestRevision` / `loadingRevision`；**没有** `gridSelection`（`01` 新增） |
| `src/stores/tableData/types.ts` | `CellEdit` | 逐格编辑缓冲的元素类型（`rowIndex` + `columnName` + 原值 + 新值 + `pkSnapshot`） |
| `src/stores/tableDataStore.ts` | `TableDataStore.stageCellChange` | **单格写入的唯一入口**。流程：取 `ts.rows[row]` → 无主键列则 `patchPanel({ error: t('tableData.noPrimaryKey') })` 返回 → `buildRowIdentity` + `rowIdentityIsUnique` 失败则 `error: AMBIGUOUS_ROW_IDENTITY_ERROR` → `findPendingForRow` 复用既有 `originalValues` → `valuesEqual` 相等则**删除**该列的改动（回到无改动）→ 否则记入 → **重算整个 `editBuffer`**（`rebuildEditBuffer`）→ 每次调用一次 `patchPanel`（新建 `Map`） |
| `src/stores/tableDataStore.ts` | `TableDataStore.applyColumnToRows` | 就是 `for (const row of rows) get().stageCellChange(...)`——**N 格 = N 次状态写入 + N 次全量 `rebuildEditBuffer`**，是批量写入在今天的唯一形态，也是本册要替换的反例 |
| `src/stores/tableDataStore.ts` | `patchPanel` / `patchPanelForReload` | `patchPanel` 新建 `byPanel` Map 并 `set`；`patchPanelForReload` **只**并入 `updater(ts)` 并把 `requestRevision` 加一（竞态防护，`00` §1.1 第 2 条），**不清任何选择状态**。`00` §4.3 推论已把这条确认为既有缺陷并指定 DB-01 在 `patchPanelForReload` / `commitFetchedPage` / `invalidateCachedData` 三个收敛点统一修复 |
| `src/stores/tableDataStore.ts` | `setPage` / `setPageSize` / `setSort` | **都不清空 `selectedRows`**（只有 `invalidateCachedData` 会清）。翻页/排序后 `selectedRows` 里留着上一页的行下标，而 `rowIndex` 的含义已整体平移。`00` §4.3 推论已确认这是既有缺陷并指派 DB-01 修复；本册只依赖该保证，不自行修补（见 B21 与 U-1） |
| `src/stores/tableData/pendingChanges.ts` | `findPendingForRow` / `rowIdentityIsUnique` / `hasPendingIdentityCollision` / `rebuildEditBuffer` / `overlayPendingRows` | 行身份解析与冲突检测的既有工具箱；`rebuildEditBuffer` 是 O(rows × pending) 的全量重算 |
| `src/stores/tableData/connectionState.ts` | `toCellValue` | `val === null \|\| val === undefined` → `null`；否则原样 `as Value`。**没有类型转换**：`'abc'` 进整数列就是字符串 `'abc'`，由数据库在提交时报错 |
| `src/lib/tableChanges.ts` | `PendingRowChange` / `RowChangePlan` / `PlannedStatement` | 计划模型**只有 `updates` / `deletes`**（`00` §1.4 事实 1）；`valuesEqual` 是稳定 JSON 比较（`00` §3.4 要求复用它） |
| `src/lib/tableChanges.ts` | `buildRowIdentity` / `rowIdentityKey` | 主键快照 → 稳定键；主键值为 `null` 或不可稳定序列化时返回 `null` |
| `src/types/index.ts` | `Value` | `string \| number \| boolean \| null \| Record<string, unknown> \| unknown[]` |
| `packages/driver-sdk/src/types/schemaMetadata.ts` | `ColumnSchema` | `name` / `dataType` / `nullable` / `defaultValue?` / `isPrimaryKey?` / `isAutoIncrement?` / `comment?`。**没有 `isGenerated`**（生成列无法与「有默认值的普通列」区分） |
| `src/windows/connection/TableView.tsx` | `TableView` 的 `rowArrays` | `rows.map(record => displayedColumns.map(col => record[col.name] ?? null))`——**组件层 `rows: unknown[][]` 是 `displayedColumns` 的位置数组**，列映射必须靠 `columnDefs[i].name`，绝不能把下标当成 `ts.columns` 的下标 |
| `src/windows/connection/TableView.tsx` | `TableView` 的 `isEditable` | `!isConnectionReadOnly`；只读来源 = `readOnly` prop ?? （连接配置的 `readOnly` \|\| `driverReadOnly`）；`driverReadOnly` 读 `DB_REGISTRY[databaseType]?.readOnly === true`（零硬编码的既有范式） |
| `src/windows/connection/TableView.tsx` | `handleCellEdit` | 不可编辑时 `showReadOnlyTip()` + `cancelEdit()`；可编辑时 `actions.stageCellChange(...)` |
| `src/lib/databaseMeta.ts` | `DatabaseTypeMeta.readOnly` / `exportScope` / `defaultPageSize` | 前端能力声明字段（`DataGridCapabilities` **尚不存在**，是 `00` §7 的目标设计） |
| `src/lib/databaseMeta.ts` | `KvWorkspaceCapabilities` | 「能力而非驱动 id、字段可选、缺省等于今天」的既有范式（`00` §1.8） |
| `src/hooks/useConfirmDialog.tsx` | `useConfirmDialog` | 命令式确认框：`[confirmFn, DialogElement]`，`confirmFn(options) => Promise<boolean>`，`kind: 'warning' \| 'info'` |
| `src/stores/tableDataStore.ts` | 文件末尾的 `__tableDataStore` 探针 | `if (import.meta.env.DEV)` 时把 store 挂到 `window.__tableDataStore`，E2E 用它做值变更（见 `e2e/specs/table-edit.ts` 头部注释） |

### 2.3 行数预算（第 6 节的约束来源）

| 文件路径 | 当前总行数 | 与 800 行上限的关系 |
| --- | --- | --- |
| `src/stores/tableDataStore.ts` | 722 | 逼近；本册要求**净减** |
| `src/windows/connection/TableView.tsx` | 770 | **已逼近**（96%）；本册要求先拆分腾出余量 |
| `src/components/DataTable/DataTable.tsx` | 628 | 逼近；本册要求净减 |
| `src/lib/dataTableContextMenu.ts` | 398 | 安全 |
| `src/lib/tableChanges.ts` | 227 | 安全 |

### 2.4 现状缺陷汇总（本册的存在理由）

| # | 缺陷 | 证据（符号） | 后果 |
| --- | --- | --- | --- |
| G1 | 网格**没有任何剪贴板入口** | `DataTable.tsx` 无 `onPaste` / `onCopy`；`TableView.tsx` 无粘贴绑定；`ContentView.tsx` 的 `useKeyboardShortcuts` 只有两条 global | 外部批量数据进不来；`00` §1.3 结论「网格没有 Ctrl/Cmd+C/V」被核实 |
| G2 | TSV 零转义 | `serializeDataTableRowsAsTsv` | 值内含制表符/换行时行列错位，且**无声** |
| G3 | TSV 塌掉 NULL 与空串 | `serializeDataTableRowsAsTsv` 的 `cell == null ? '' : String(cell)` | 用户无法从剪贴板文本还原语义；粘回后 `NULL` 变成空串 |
| G4 | 三份 TSV 实现、两种策略 | `serializeDataTableRowsAsTsv` vs `exportData.ts` 的 `escapeTSV` vs `exportStream.ts` 的 `escapeTSV` | 同一份数据「复制」与「导出」结果不同，缺陷无法一次修好 |
| G5 | 复制无降级链、无失败反馈 | `DataTable.tsx` 的局部 `copyText` | Tauri 的 WKWebView 在失去同步用户手势（菜单项回调经过 IPC 往返）时会拒绝 `writeText`，而 rejection 无人接管：**用户看到「点了没事发生」** |
| G6 | 位置数组与列名脱钩 | `TableView.tsx` 的 `rowArrays` + `resolveDataTableCellFromEvent` | 任何「按列下标取数」的实现都会在列显隐 / 拖拽后静默错列 |
| G7 | 翻页/排序不清选择 | `setPage` / `setPageSize` / `setSort` / `setFilters` / `applyFilters` 全走 `patchPanelForReload`，而后者不清选择状态 | 复制与粘贴会把「上一页的下标」作用到当前页的行上——**误写**方向。`00` §4.3 推论已确认并指派 **DB-01** 在三个收敛点统一修复；本册只依赖该保证，不自行修补 |
| G8 | 列级写保护不存在 | `stageCellChange` 只检查主键与身份歧义 | 自增列/生成列可被逐格改写，粘贴会把这个洞放大成批量 |
| G9 | 批量写入是 N 次全量重算 | `applyColumnToRows` + `rebuildEditBuffer` | 10 万格粘贴在现状形态下不可行 |
| G10 | 列元数据不够用 | `ColumnDef` 只有 `name` / `type` | 粘贴的类型与可空判定必须拿 `ColumnSchema`，因此必须在 `TableView` 层做，不能塞进 `DataTable` |
| G11 | 复制可能落到「屏幕上的截断文本」 | `CellRenderer` 的 120 字截断 + `handleContextMenu` 的 `window.getSelection()` 回退 | 用户复制的值与看到的一致、却与数据库里的值**不一致**；长 JSON / 长文本被静默截掉尾部。裁定见 §3.8 |
| G12 | **跨表粘贴会按位置静默写错列** | 本册 Step 3 的 `planPaste` 只收 `columnOrder`，把矩阵第 *i* 列写进目标第 *i* 列；而剪贴板是**操作系统全局资源**（今天已有 `navigator.clipboard.writeText` / 系统剪贴板，无需任何应用内通道） | 用户从 `users(id, name, email)` 复制，切到 `orders(id, user_id, amount, created_at)` 粘贴 → `name` 落进 `user_id`、`email` 落进 `amount`，**全程零提示、零错误码**，`planPaste` 认为这是一次完全合法的粘贴。一旦提交就是**另一张表**的数据损坏，且因为 `nullable` 判定也过了，数据库不会报错 |

---

## 3. 数据结构与接口设计

### 3.1 `CellWrite` 三态（`00` §3.3 的落地，与 DB-04 共用）

位置：`src/lib/tableChanges.ts`（**修改**，与 `00` §3.3 指定的文件一致）。

```ts
import type { ColumnSchema, Value } from '../types';

/** 一次列写入的意图。三态是刻意的：`Unset` 与 `null` 在 SQL 里是两件不同的事。 */
export type CellWrite =
  | { kind: 'null' }
  | { kind: 'value'; value: Value }
  | { kind: 'unset' };

/** 便捷构造器，避免各处手写字面量。 */
export const cellWriteNull = (): CellWrite => ({ kind: 'null' });
export const cellWriteValue = (value: Value): CellWrite => ({ kind: 'value', value });
export const cellWriteUnset = (): CellWrite => ({ kind: 'unset' });

/** 单元格坐标 + 写意图。`rowIndex` 只用于定位，禁止进入 SQL（`00` §4.3）。 */
export interface CellWriteInput {
  rowIndex: number;
  columnName: string;
  write: CellWrite;
}

/** 列是否可被**粘贴**写入。刻意不复用到单格双击编辑路径上（那会改变既有行为）。 */
export interface ColumnWritability {
  writable: boolean;
  /** `writable === false` 时的分类码，用于提示文案选择。 */
  reason: 'auto-increment' | 'unknown-column' | null;
}
export function columnWritability(column: ColumnSchema | undefined): ColumnWritability;
```

| 字段 | 用途 | 缺省 / 边界 |
| --- | --- | --- |
| `CellWrite.kind === 'null'` | 写 SQL NULL | 目标列 `nullable === false` 时由粘贴规划器拦下（`grid.paste.notNullEmpty`），单格编辑路径不受影响 |
| `CellWrite.kind === 'value'` | 写具体值 | 值**不做类型转换**（与 `toCellValue` 一致，保持 `00` R4「缺省等于今天」） |
| `CellWrite.kind === 'unset'` | 该列**不进语句** | 粘贴路径上「目标格没被矩阵覆盖」「目标列不可写」「矩阵比区域小」都产生 `unset`。它同时是 DB-04 INSERT 的「让数据库填默认值」 |
| `CellWriteInput.rowIndex` | 当前页内行下标，用于从 `TableState.rows` 取记录 | 必须 `Number.isInteger` 且 `>= 0`；越界即 `grid.paste.outOfBounds` |
| `CellWriteInput.columnName` | **列名**，不是下标（`00` §4.1） | 不在 `TableState.columns` 中即 `columnWritability().reason === 'unknown-column'` → 跳过并计 `grid.cell.readOnly` |
| `columnWritability` | 判定粘贴能否写该列 | 现状只有 `isAutoIncrement === true` 可判定（`isGenerated` 不存在，见 U-3）。**返回 `writable: false` 时不抛错、不写 `ts.error`**，由调用方决定提示 |

> **与 `00` §3.6 的关系**：契约说「生成列被显式改则允许，交数据库报错」。粘贴场景下用户**没有**显式选择该列，所以本册在**规划阶段**把自增列整列标为跳过（保守、不误写），并在提示里告诉用户「N 格因列只读被跳过」。这与契约不冲突（契约约束的是写意图存在时不得篡改），但属于本册的从严选择，记入 U-3。

### 3.2 剪贴板纯函数模块

位置：`src/lib/gridClipboard.ts`（**新增**）。零 React、零 IPC、零 store 依赖——便于单测，也便于 DB-10 直接复用。

```ts
import type { ColumnSchema, Value } from '../types';
import { type CellWrite, type CellWriteInput, cellWriteNull, cellWriteUnset, cellWriteValue } from './tableChanges';

/**
 * 与 `01` 的 `CellCoord` **结构等价**的本地类型。
 *
 * 刻意不 `import` `src/stores/tableData/gridSelection.ts`：`src/lib/` 目前不依赖
 * `src/stores/`（既有 `src/lib/tableChanges.ts` 也只依赖 `../types`），保持这个
 * 分层方向可以让 `gridClipboard.ts` 不引入任何 store 依赖，单测无需任何 mock。
 * 适配由 `src/hooks/useGridClipboard.ts` 一处完成（它两边都能 import）。
 */
export interface ClipboardCellCoord {
  rowIndex: number;
  columnName: string;
}

/** 剪贴板支持的解析分隔符。制表符优先（Excel/Numbers/Sheets 的默认复制形态）。 */
export type ClipboardDelimiter = '\t' | ',' | ';';

/** 一块矩形落点。列用**列名**，行用当前页下标（`00` §4.1 / §4.3）。 */
export interface GridClipboardRange {
  topRow: number;
  bottomRow: number;
  leftColumn: string;
  rightColumn: string;
}

export interface GridClipboardAnchor {
  rowIndex: number;
  columnName: string;
}

/** 解析结果：二维文本 + 逐格引号标记。 */
export interface ClipboardMatrix {
  /** 单元格文本；引号已剥离，字段内 `""` 已还原为 `"`。行数 ≥ 1，每行字段数一致。 */
  cells: string[][];
  /** 与 `cells` 同形。`true` = 源文本里该格被引号包裹，因此空串是**显式空串**而不是 NULL。 */
  quoted: boolean[][];
  delimiter: ClipboardDelimiter;
  /** 源文本含换行（用于区分「一格文本」与「N 行 × M 列」的提示）。 */
  multiline: boolean;
  /** 源文本 UTF-8 字节数，用于上限判定与提示文案。 */
  byteLength: number;
}

export type ClipboardParseResult =
  | { ok: true; matrix: ClipboardMatrix }
  | { ok: false; code: 'grid.paste.empty' | 'grid.paste.tooLarge'; cells: number; byteLength: number };

/** 解析上限：超过即拒绝（不做部分粘贴）。 */
export const GRID_PASTE_MAX_BYTES = 10 * 1024 * 1024;
export const GRID_PASTE_MAX_CELLS = 100_000;
/** 超过此格数时弹一次确认框；以下静默执行（粘贴是高频动作）。 */
export const GRID_PASTE_CONFIRM_CELLS = 2_000;
/** 单次状态写入覆盖的最大格数；超过则分批，避免一帧内超过 ~200ms。 */
export const GRID_PASTE_CHUNK_CELLS = 5_000;

/** 二维解析：RFC 4180 风格的引号语义，嗅探分隔符，拒绝 ragged 输入。 */
export function parseClipboardMatrix(text: string): ClipboardParseResult;

/** 列名 + 行下标；与 `01` 的 `CellCoord` 结构等价，本册不另立坐标语义。 */
export type GridClipboardAnchor = ClipboardCellCoord;

/**
 * 两个角 + 列序 → 矩形落点。
 *
 * `start` / `end` **必须**已经是 `01` 的 `normalizeCellRange` 的输出（调用方在
 * `src/hooks/useGridClipboard.ts` 里调用它）。本册**不重新实现**「哪一列在前」的
 * 列序比较——`00` §4.1 明确要求只有一份实现。
 */
export function toClipboardRange(
  normalized: { start: ClipboardCellCoord; end: ClipboardCellCoord },
  columnOrder: readonly string[],
): GridClipboardRange | null;

/**
 * 区域 → 位置矩阵。**这是「列下标映射回列名」的唯一实现**：按 `coords` 给出的
 * 列名取记录字段。
 *
 * `coords` 必须由 `src/stores/tableData/gridSelection.ts` 的区域展开函数产出
 * （本册不写 `for (row) for (col)` 遍历），本函数只负责「坐标 → 值」。
 */
export function buildSelectionMatrix(
  rows: readonly Record<string, unknown>[],
  coords: readonly ClipboardCellCoord[],
): unknown[][];

/**
 * 行级取值域 → 位置矩阵（整行 / 选中行复制）。
 *
 * `rowIndices` **必须**来自 `rowsCoveredBySelection(state)`（`00` §4.2 第 3 条）；
 * 本册禁止在调用点自行从 `selectedRows` 或区域重算一份行集。
 */
export function buildRowsMatrix(
  rows: readonly Record<string, unknown>[],
  columnOrder: readonly string[],
  rowIndices: Iterable<number>,
): unknown[][];

export type ClipboardFormat = 'tsv' | 'csv' | 'json';

export interface SerializeCellsOptions {
  /** 输出格式。默认 `'tsv'`。 */
  format: ClipboardFormat;
  /** 列名，按选择区域的列序给出。 */
  columnNames: readonly string[];
  /** 仅 tsv/csv：首行是否写列名。缺省 `false`（tsv 对齐 Excel）/ `true`（csv，对齐既有 `serializeDataTableRowsAsCsv`）。 */
  includeHeader?: boolean;
  /** NULL 的文本表示。缺省 `''`（Excel 友好，代价是 NULL 与空串不可分）；设 `'NULL'` 时两者可分。 */
  nullLiteral?: string;
  /** 仅 json：`true` 输出列名对象数组，`false` 输出二维数组。缺省 `false`（对齐 PRD DB-03）。 */
  jsonObjects?: boolean;
}

/**
 * 位置矩阵 → 剪贴板文本。**全仓唯一的序列化内核**。
 *
 * 既有 8 个函数（`serializeDataTableRowsAsTsv` / `serializeDataTableRowsAsCsv` /
 * `serializeDataTableRowsAsJson` / `formatRowAsSqlInsert` / `formatRowAsSqlUpdate` /
 * `formatRowsAsSqlInsert` / `formatRowsAsSqlUpdate` / `serializeDataTableColumnValues`）
 * **保留函数名与签名，作为唯一的对外入口**，内部委托到本函数（见 Step 9）。
 * 本册因此不新增任何平行的对外序列化 API，只为它们提供一份共享实现——这正是
 * PRD DB-03「与既有 `serializeDataTableRowsAsTsv` / `serializeDataTableRowsAsCsv`
 * 合并，避免两套 TSV 转义实现」的字面要求。
 */
export function serializeCells(
  rows: readonly (readonly unknown[])[],
  options: SerializeCellsOptions,
): string;

export type PasteOverflow = 'expand' | 'clip';

export interface PastePlanOptions {
  matrix: ClipboardMatrix;
  anchor: GridClipboardAnchor;
  /** 当前页列序（`visibleColumns` 归一化后的顺序）。 */
  columnOrder: readonly string[];
  /** 当前页列元数据（来自 `TableState.columns`）。 */
  columns: readonly ColumnSchema[];
  /** 区域粘贴时的原始区域；单格粘贴传 `null`。 */
  region: GridClipboardRange | null;
  /** 当前页行数：扩展与截断都以它为界。 */
  pageRowCount: number;
  /** 溢出策略。缺省 `'expand'`（PRD 行为）。 */
  overflow?: PasteOverflow;
  /** NULL 文本字面量（大小写不敏感）。缺省 `'NULL'`；额外恒识别 `\N`。 */
  nullLiteral?: string;
  /** 已在 `pendingChanges` 里的 `(rowIndex, columnName)` 键集合，用于统计将被覆盖的暂存改动。 */
  pendingCellKeys?: ReadonlySet<string>;
}

export interface PastePlan {
  /** **只含要写入的格**；`unset` 的格不会出现在这里。 */
  writes: CellWriteInput[];
  /** 实际覆盖范围；`writes` 为空时为 `null`。 */
  bounds: GridClipboardRange | null;
  /** 逐格与整批级问题，用于提示与 `data-dt-cell-error`。 */
  problems: PasteProblem[];
  /** 被跳过的格数（只读列、不可空列、越界）。 */
  skipped: number;
  /** 将要覆盖的既有暂存改动数。 */
  overwrittenPending: number;
  /** 目标是自增列的列名（去重），用于「N 列被跳过」的提示。 */
  skippedColumns: string[];
}

export function planPaste(options: PastePlanOptions): PastePlan;

/** `(rowIndex, columnName)` → 稳定键，供 `pendingCellKeys` 与测试使用。 */
export function pendingCellKey(rowIndex: number, columnName: string): string;
```

**逐字段要点**

| 字段 / 参数 | 用途 | 缺省与理由 |
| --- | --- | --- |
| `quoted` | **NULL 与空串的判据**。只有它能在粘回时还原语义 | 无缺省；解析器恒产出与 `cells` 同形的矩阵 |
| `delimiter` | 嗅探结果，用于提示「按逗号分隔的 4 列」 | 嗅探顺序：制表符 > 逗号 > 分号。全无则按单列处理（**不是**逗号），否则单格文本会被逗号切碎 |
| `includeHeader` | 复制是否带表头 | tsv `false`（PRD §4.7 的 `CSV with Header` 是**另一个**格式）；csv `true`（不改变既有 `serializeDataTableRowsAsCsv` 语义） |
| `nullLiteral` | NULL 的文本表示 | 复制缺省 `''`；粘贴缺省识别 `'NULL'`（大小写不敏感）+ `'\N'` |
| `overflow` | 矩阵超出区域时的行为 | 缺省 `'expand'`（PRD DB-03 明文要求「超出区域自动扩展」）；`'clip'` 为保守模式，见 U-2 |
| `pageRowCount` | 扩展的硬边界 | 无缺省。越界一律截断 + `grid.paste.outOfBounds`，**绝不**自动翻页（那会跨页写入不可见行） |
| `pendingCellKeys` | 统计「覆盖了多少待提交改动」 | 缺省空集合；`TableView` 从 `TableState.pendingChanges` + `rowIdentityAnchors` 构造 |
| `cancelled`（`PastePlan` 未含） | — | 取消由调用方在分批写入之间检查，不进数据类型 |

### 3.2b 跨表粘贴：来源追踪与列名对齐（G12 的修复，**本册新增，不可省略**）

**问题重述（不是「要不要支持跨表粘贴」，而是「跨表粘贴不能静默写错列」）**：

- 「从 A 表复制、粘到 B 表」本身是**正当且高频**的用法（灌数据、对拍两表），本册**不禁止**它；
- 缺陷在于**按位置对齐且不告知**。目标表列序与来源表列序毫无关系时，位置对齐等于让用户把 `email` 的内容写进 `amount`。

**修复分三层，缺一层都留洞**：

| 层 | 做什么 | 拦住什么 | 拦不住什么 |
| --- | --- | --- | --- |
| L1 **来源追踪** | 应用内复制时把来源表与来源列名写进模块级 `lastCopySource` | 能证明「同表」→ 免确认直接写 | 证明**不了**（外部复制、另一个 DataZen 窗口、后台进程改写剪贴板） |
| L2 **列名对齐** | 来源已知时按**列名**而非列序对齐 | 列序不同的两表互相粘贴 | 来源未知（Excel / 外部应用）时的错列 |
| L3 **强制确认** | L1 证明不了同表、或 L2 对不齐时，`planPaste` 置 `requiresConfirmation`，调用方在 `stageCellWrites` **之前**弹确认 | 兜住 L1/L2 的全部漏网 | —（这就是它作为最后一层存在的理由） |

**L1 为什么不可靠，必须写进代码注释**：模块级 `lastCopySource` 记的是「**上一次经由本应用复制的**内容」，
它与此刻系统剪贴板里的真实内容**没有强绑定**——用户在 Excel 里复制一次，它就过期了。
因此它**只能用来抬高门（raise the gate），绝不能用来自动放行或自动改写对齐**。任何「因为 L1 说来源是 A 表所以直接按 A 表列名对齐」的写法都是错的。

```ts
/** 一次复制的来源。`columnNames[i]` 对应矩阵第 `i` 列；`null` 表示该侧未知。 */
export interface ClipboardSource {
  /** 表身份键，由 `tableChangeKey(context)` 生成（见下）。 */
  tableKey: string;
  /** 来源列名，顺序与矩阵列一致。**tsv 复制没有表头，但这里照样有值**——这正是 L1 的价值所在。 */
  columnNames: readonly string[];
  /** 来源列的类型族，用 `DataTypeFamily`（`classifyDataType` 的 7 族；05 落地后换成 `resolveCellType` 的 11 `CellType`）。 */
  columnTypes: readonly DataTypeFamily[];
  // 提交顺序里 03 排在 05 之前 ⇒ 此处只能是 DataTypeFamily，禁止提前引用 05 才创建的 CellType
}

/** 模块级「上次复制来源」。**刻意做成模块状态而不是 store**：它描述的是系统剪贴板，不是当前会话的表状态。 */
let lastCopySource: ClipboardSource | null = null;
export function recordClipboardSource(source: ClipboardSource): void;
/** 测试可注入；生产只读。 */
export function peekClipboardSource(): ClipboardSource | null;
export function clearClipboardSource(): void;

/** 表身份键。`connectionId` 恒为**配置归属**，`database` / `schema` 的 `null` 归一为 `''`。 */
export function tableChangeKey(context: TableChangeContext | null): string;
```

**`tableChangeKey` 的三条陷阱（照抄即错）**：

1. **不能用 `dbSessionId`**——重连一次就换 id，同表粘贴会被误判成跨表，天天弹确认。AGENTS.md 的命名规范就是这么定的：归属/配置语义用 `connectionId`。
2. **必须整体转小写再比较**——`schema` 在 PG 下大小写敏感、在 MySQL 下不敏感；不归一就会在 MySQL 上天天误报「跨表」。
3. **`null` 与 `''` 是同一张表**——`TableChangeContext.database` 允许 `null`（SQLite），与显式空串必须归一后再比。

**L2 的对齐算法（`planPaste` 内部，Step 3 实现）**：

```ts
const sourceNames = options.source?.columnNames;
if (sourceNames && sourceNames.length === matrix.width) {
  const byName = mapColumnsByName(sourceNames, options.columnOrder);
  if (byName.matched > 0) {
    // 用 byName.mapping 取代位置对齐；byName.dropped 里的来源列进 PastePlan.droppedColumns + grid.paste.columnMismatch
    return { mode: "byName", ... };
  }
}
// matched === 0 ⇒ 名称完全对不上 ⇒ 这就是跨表的强证据，退回位置对齐并强制确认
return { mode: "byPosition", crossTable: true, requiresConfirmation: true, ... };
```

| 场景 | `mode` | `requiresConfirmation` | 用户看到什么 |
| --- | --- | --- | --- |
| 同表同序（最常见） | `byName` | **false** | 无提示，与无此功能时一致（零回归） |
| 同表但列被隐藏/拖拽过 | `byName` | **false** | 无提示。**这正是按名对齐的价值**——按位置对齐在这种场景下是错的 |
| 跨表，列名部分重合（如都叫 `id`） | `byName` | `true` | 「来自 <源表>，只有 2/5 列同名将写入：id、created_at」 |
| 跨表，列名完全不重合 | `byPosition` | `true` | 「内容来自另一张表或外部应用，将按当前位置写入 5 列」 |
| 从 Excel / 外部应用粘贴（`source === undefined`） | `byPosition` | **`true`** | 同上 |

**`requiresConfirmation` 的缺省必须是 `true`**，只有「L1 证明同表」才能置 `false`。
注意这与契约 R4「默认值等于今天缺省时的实际行为」并不冲突：**今天根本没有粘贴功能**（§2.4 G1），
所以这里没有「今天的行为」需要保持一致——`planPaste` 的每个默认值都是新行为，不适用 R4。

**`PastePlan` 新增四个字段**（全部只读诊断，不参与写入决策）：

```ts
export interface PastePlan {
  /* …既有字段不变… */
  /** 来源与目标是否同一张表。`null` = 来源未知（无法判断），不等于 `false`。 */
  crossTable: boolean | null;
  /** 实际采用的对齐方式。`unknown` = 矩阵未进规划（如超限/空），未做任何对齐。 */
  alignment: 'byName' | 'byPosition' | 'unknown';
  /** 按名对齐时被丢弃的来源列名（去重、保序）。**这些列一个格都不会写。** */
  droppedColumns: string[];
  /** 调用方**必须**先弹确认框再 `stageCellWrites`；`confirmPaste(...)` 的默认焦点与默认动作恒为「取消」。 */
  requiresConfirmation: boolean;
}
```

**确认门必须落在 `onPasteText` 的 `stageCellWrites` 之前**（Step 7）：
`planPaste` → `requiresConfirmation` → `confirmPaste(文案, { confirmLabel: 粘贴, cancelLabel: 取消 })` → 用户确认才 `stageCellWrites`。
默认焦点与默认动作都是**取消**——误粘贴的代价是改错另一张表，回退成本远高于多点一次。

### 3.3 错误码与提示

位置：`src/lib/gridErrors.ts`（**由 01 创建骨架，本册按契约 §8.1 三步追加**：`GridErrorCode` 联合类型加成员 → `GRID_ERROR_CODES` 加条目 → §6 变更表登记「修改（追加）」）。**禁止整文件重写**——并行开发下整文件覆盖不产生任何编译错误，只会静默抹掉其他分册的码。

```ts
/** 前端可精确识别的网格错误码。后端经 `CommandError` 消息前缀产出，前端经本联合类型归一化。 */
export type GridErrorCode =
  // —— `00` §8.2 既有清单，本册不改其语义 ——
  | 'grid.filter.unsupportedOperator'
  | 'grid.filter.incomplete'
  | 'grid.insert.noWritableColumn'
  | 'grid.insert.returningUnavailable'
  | 'grid.commit.stalePlan'
  | 'grid.commit.affectedMismatch'
  | 'grid.commit.conflictingIntents'
  | 'grid.cell.readOnly'
  // —— 本册新增（DB-03）——
  | 'grid.cell.noRowIdentity'
  | 'grid.cell.ambiguousIdentity'
  | 'grid.paste.empty'
  | 'grid.paste.tooLarge'
  | 'grid.paste.raggedRow'
  | 'grid.paste.unclosedQuote'
  | 'grid.paste.outOfBounds'
  | 'grid.paste.noWritableCell'
  | 'grid.paste.notNullEmpty'
  | 'grid.paste.identityChanged'
  | 'grid.paste.crossTable'
  | 'grid.paste.columnMismatch'
  | 'grid.paste.identityChanged'
  | 'grid.clipboard.writeFailed'
  | 'grid.clipboard.readFailed';

/** 供前缀匹配与测试遍历的**唯一**码表，顺序即匹配顺序。 */
export const GRID_ERROR_CODES: readonly GridErrorCode[];

/**
 * 前缀匹配。匹配不到返回 `'unknown'`——调用方**必须**回退到显示原始消息，
 * 不得吞掉错误或显示空白（`00` §8.1）。
 */
export function classifyGridError(message: string): GridErrorCode | 'unknown';
```

```ts
/** 一次粘贴的结构化问题。码是**直接给出**的，不走消息解析。 */
export interface PasteProblem {
  code: GridErrorCode;
  /** 0 基，剪贴板矩阵坐标；整批级问题为 `null`。 */
  row: number | null;
  column: string | null;
  /** 附加说明（例如列名）；**禁止**放单元格值（PII，见 PRD §8.4）。 */
  detail?: string;
}
```

**为什么粘贴问题不靠 `classifyGridError` 解析**：粘贴问题全部产生在前端，值是结构化的，直接携带码即可；`classifyGridError` 只为**后端**消息保留（`00` §8.1 的机制不变）。两者共用 `GridErrorCode` 以保证同一张码表。

**新增码的后端消息形态**（一旦有后端需要使用同一语义，消息必须以码为前缀）：

| 码 | 触发条件 | 用户可见文案要求 |
| --- | --- | --- |
| `grid.cell.noRowIdentity` | 表无主键列，无法建立行身份 | 复用既有 `tableData.noPrimaryKey` 文案（单格路径逐字不变）；粘贴路径用 `dataTable.pasteNoRowIdentity` |
| `grid.cell.ambiguousIdentity` | 行身份为 NULL / 不唯一 / 与既有暂存改动冲突 | 单格路径继续返回既有 `AMBIGUOUS_ROW_IDENTITY_ERROR` 字符串常量（保持既有测试），粘贴路径改用结构化码 |
| `grid.paste.empty` | 剪贴板为空或只有空白 | 说明「剪贴板没有可粘贴的文本」 |
| `grid.paste.tooLarge` | 超过 `GRID_PASTE_MAX_CELLS` 或 `GRID_PASTE_MAX_BYTES` | **必须**给出实际格数/字节数与上限，并指向「导入」功能 |
| `grid.paste.raggedRow` | 矩阵行内字段数不一致 | 指出第一处不一致的行号与两侧字段数，并声明「未写入任何格」 |
| `grid.paste.unclosedQuote` | 引号未闭合 | 说明「引号后的全部文本被当成一格」，**不是**错误拒绝，只要提示 |
| `grid.paste.outOfBounds` | 目标超出当前页剩余行 | 说明被截断的格数与「未自动翻页」 |
| `grid.paste.noWritableCell` | 所有目标列都不可写 | 说明原因（全部是自增列 / 全部是未知列） |
| `grid.paste.notNullEmpty` | 未加引号的空字段落到 `nullable === false` 的列 | 指出行号与列名，说明 NULL 未写入 |
| `grid.paste.identityChanged` | 粘贴会改动主键列，且改动后行身份无法唯一解析 | 指出行号与列名，说明「改动后无法唯一定位该行」 |
| `grid.paste.crossTable` | 来源表与目标表不是同一张表，或来源未知 | **必须**给出「来自 <源表全名>」与「将写入 <目标表全名>」两段文案；`source === undefined` 时写「来源未知（可能是从 Excel 或其他应用复制的）」，**不得**写「来自未知表」这种等于没说的话 |
| `grid.paste.columnMismatch` | 按列名对齐时来源列在目标表找不到同名列 | 列出被丢弃的来源列名（最多 5 个 + `…`），并声明「这些列不会被写入」；**禁止**把它们按位置补写到目标表末尾 |
| `grid.clipboard.writeFailed` | `copyToClipboard` 返回 `false` | 说明复制失败，不谎报成功 |
| `grid.clipboard.readFailed` | `readClipboard()` 抛错（权限/未绑定平台服务） | 说明无法读取剪贴板，建议改用 `⌘+V` |

### 3.4 store 批量化接口

位置：`src/stores/tableData/stageCellWrite.ts`（**新增**，纯函数，不依赖 Zustand、不调 `t()`）。

```ts
import type { ColumnSchema } from '../../types';
import type { GridErrorCode } from '../../lib/gridErrors';
import type { CellWriteInput, PendingRowChange } from '../../lib/tableChanges';
import type { TableState } from './types';

export interface CellWriteProblem {
  rowIndex: number;
  columnName: string;
  code: GridErrorCode;
  detail?: string;
}

export interface StageCellWritesOutcome {
  rows: Record<string, unknown>[];
  pendingChanges: Map<string, PendingRowChange>;
  rowIdentityAnchors: Map<number, string>;
  /** 真正落在 `pendingChanges` 上的格数（与输入格数可能不同：相等即回到「无改动」）。 */
  staged: number;
  /** 被跳过的格与原因。**调用方负责决定是否写 `ts.error`**。 */
  problems: CellWriteProblem[];
}

/**
 * 一次写入 N 格。**纯函数**：输入一份 `TableState`，输出新的三件套（`rows` /
 * `pendingChanges` / `rowIdentityAnchors`），不产生任何副作用、不 `set()`、不 `t()`。
 *
 * 单格路径（`stageCellChange`）与粘贴路径共用它，以保证「同一格在同一状态下由谁写都得到同一结果」。
 */
export function stageCellWritesInto(
  ts: TableState,
  writes: readonly CellWriteInput[],
  primaryKeyColumns: readonly ColumnSchema[],
): StageCellWritesOutcome;

/** `editBuffer` 的一次性重算入口；本册要求它**每次批量写入只跑一次**。 */
export function rebuildEditBufferOnce(
  ts: TableState,
  outcome: Pick<StageCellWritesOutcome, 'rows' | 'pendingChanges'>,
): Map<string, CellEdit>;
```

**`TableDataStore` 的两个动作**（`src/stores/tableDataStore.ts` 修改）：

```ts
export interface StageCellWritesResult {
  staged: number;
  skipped: number;
  problems: CellWriteProblem[];
}

interface TableDataStore {
  // 既有，签名不变；内部改为 `stageCellWritesInto` 的薄包装
  stageCellChange: (panelId: string, row: number, col: string, value: unknown) => void;
  // 新增
  stageCellWrites: (panelId: string, writes: readonly CellWriteInput[]) => StageCellWritesResult;
  // DB-01 拥有（在 `patchPanelForReload` / `commitFetchedPage` / `invalidateCachedData`
  // 三个收敛点统一生效，见 `00` §4.3 推论）；本册只**读**它的结果，不实现、不在各 action 里重复清空
  clearGridSelection: (panelId: string) => void;
}
```

**行为契约**

| 场景 | `stageCellWrites` 行为 |
| --- | --- |
| `writes` 为空 | 直接返回 `{ staged: 0, skipped: 0, problems: [] }`，**不 `set()`**（避免无意义渲染） |
| 表无主键列 | 返回 `problems: [{ code: 'grid.cell.noRowIdentity' }]`，`staged: 0`；**不写 `ts.error`**（粘贴路径自己提示；单格路径的包装函数照旧写 `ts.error`） |
| 某格身份歧义 | 跳过该格并记 `grid.cell.ambiguousIdentity`；**其余格照常写入**（一格坏掉不该毁掉 999 格） |
| 某格 `write.kind === 'unset'` | 不写、不计入 `staged`，也不记 problem（这是正常的「不写」语义） |
| 写入后该值与原值相等 | 沿用既有语义：**删除**该列的改动（`valuesEqual`），若该行已无改动且未标删除则整条 `pendingChanges` 删除。`staged` 计数为「实际进入 `pendingChanges` 的格数」，因此相等时不计入 |
| 每次调用 | **至多一次** `set()`；`rebuildEditBuffer` **至多一次**；`requestRevision` 不变（写入不改变「当前请求的是哪一页」） |
| `previewPlan` / `pendingStatus` | 置 `null` / `'idle'`（与 `stageCellChange` 一致：任何暂存改动都让既有预览作废） |

### 3.5 复制反馈 hook

位置：`src/hooks/useClipboardCopyFeedback.ts`（**新增**，宿主层，允许 import `@tauri-apps/api` 间接依赖）。

```ts
export interface ClipboardCopyFeedback {
  /** 最近一次复制是否成功；成功态持续 `feedbackMs`。 */
  copied: boolean;
  /** 执行复制。返回是否成功；**不会**抛出，失败时 `copied` 保持 `false`。 */
  copy: (text: string) => Promise<boolean>;
}

/**
 * 与 `packages/ui` 的 `useCopyFeedback` 保持同一套请求绑定语义（乐观反馈、失败回滚、
 * 每次点击独占完整窗口、卸载清计时器），但底层走 `copyToClipboard` 的三级降级链。
 *
 * 位置必须在宿主：`packages/ui` 不允许 import `@tauri-apps/api`，而降级链需要
 * `invoke('write_clipboard')`。
 */
export function useClipboardCopyFeedback(feedbackMs: number): ClipboardCopyFeedback;
```

| 关注点 | 取值 | 理由 |
| --- | --- | --- |
| `feedbackMs` | 调用方传入；网格固定 `1200` | 与既有 copy-feedback 收敛轨迹一致（站点各自持有窗口长度，测试逐边界断言） |
| 空文本 | `copy('')` 立即返回 `false`，**不**乐观亮起 | `copyToClipboard('')` 的既有语义就是 `false`；亮起即是谎报 |
| 失败 | 回滚 `copied` 并把原因交给调用方（`onProblem` 回调由调用方提供） | 满足 G5：不再出现「点了没事发生」 |

### 3.6 `DataTable` 的新增 props（全部可选，缺省等于今天）

位置：`src/components/DataTable/DataTable.tsx`（**修改**）。

```ts
import type { GridClipboardRange, GridClipboardAnchor, PasteProblem } from '../../lib/gridClipboard';

/** 网格剪贴板的对外桥。`DataTable` 不认识 `ColumnSchema`、不认识 store：规则全在 `TableView` 层。 */
export interface GridClipboardHandlers {
  /** `copy` 事件（⌘C / Ctrl+C）。返回要写入剪贴板的文本；`null` = 不拦截，让浏览器默认行为生效。 */
  buildCopyText: (target: GridCopyTarget) => string | null;
  /** 复制结果反馈（计时器由调用方持有）。 */
  onCopied: (target: GridCopyTarget, ok: boolean) => void;
  /** 原生 `paste` 事件。返回 `true` = 已消费（调用方已 `preventDefault()`）。 */
  onPasteText: (text: string, anchor: GridClipboardAnchor) => boolean;
  /** 右键菜单发起的粘贴：需要异步读系统剪贴板。 */
  onPasteFromClipboard: (anchor: GridClipboardAnchor) => void;
  /** 复制入口可用性（只读连接 / 无选择时为 `false`）。菜单项据此 `enabled: false`，**不是**隐藏。 */
  canCopy: boolean;
  /** 粘贴入口可用性（只读连接 / 无主键表时为 `false`）。 */
  canPaste: boolean;
}

export type GridCopyTarget =
  | { mode: 'row'; rowIndices: number[] }
  | { mode: 'cell'; range: GridClipboardRange };

export interface DataTableProps {
  // …既有 props 不变…
  /** 选择态（DB-01 提供）；缺省 `null` = 无区域，`copy` 事件退化为整行复制。 */
  gridSelection?: { anchor: GridClipboardAnchor | null; focus: GridClipboardAnchor | null; mode: 'none' | 'cell' | 'row' } | null;
  /** 剪贴板桥；缺省时网格不注册 `onCopy` / `onPaste`（行为等价于今天）。 */
  clipboard?: GridClipboardHandlers;
  /** 上传解析/规划问题，供调用方渲染提示。 */
  onClipboardProblem?: (problem: PasteProblem) => void;
}
```

| 字段 | 用途 | 缺省 |
| --- | --- | --- |
| `gridSelection` | 决定 `mode === 'cell'` 时 `copy` 事件复制区域还是整行 | `null` → 视为 `mode: 'none'`，`copy` 走进既有整行路径 |
| `clipboard` | 桥；`undefined` 时 `DataTable` 完全不绑定剪贴板事件 | `undefined` |
| `onClipboardProblem` | 问题上报 | `undefined`（问题只进控制台与 `data-dt-cell-error`） |

### 3.7 选择 → 复制的语义（多区域、非连续）

**两件事必须分开**：① 「复制一个**矩形区域**」（二维文本，TSV 无法表达洞）；② 「行级操作的**取值域**」是什么（`00` §4.2 第 3 条的派生选择器）。混为一谈是本节最容易犯的错。

**① 区域复制（`mod+c` 且 `mode === 'cell'`）**

| 情形 | 行为 | 理由 |
| --- | --- | --- |
| 主区域 | 复制 `normalizeCellRange(anchor, focus)` 得到的矩形，**只含区域内的列** | PRD C-04；`01` 负责规范化，本册只消费 |
| `extraRanges.length > 0` | **只复制主区域**，且结果提示**必须**回显被忽略的附加区域数（`dataTable.copySelectionIgnoredRanges`） | TSV 无法表达洞。静默填洞会把用户**没有选中**的值写进系统剪贴板——这是数据泄漏方向，不是体验问题。**注意**：这条只约束「矩形复制」；行级操作另见 ② |
| 落点已清（`mode === 'none'`） | 不拦截（`buildCopyText` 返回 `null`），走浏览器默认 | 不劫持无选择时的原生行为 |

**② 行级操作的取值域（一律 `rowsCoveredBySelection(state)`）**

| 操作 | 取值域 | 行为 |
| --- | --- | --- |
| `mode === 'row'` + `mod+c` | `rowsCoveredBySelection` 返回既有的 `selectedRows` | 复制整行 TSV（**与今天逐字一致**，仍走 `serializeDataTableRowsAsTsv` 的重构后实现） |
| `mode === 'cell'` + 行级动作（`⌘+D` 复制行 / 删除行 / 导出选中） | `rowsCoveredBySelection` 返回**区域（含 `extraRanges`）覆盖到的行下标集合** | 集合是**派生**的，只读消费；`buildRowsMatrix` 接受它作为 `rowIndices` |
| `mode === 'none'` | 空集 | 行级动作不出现（菜单项缺失，不是置灰） |

**三条硬约束**

1. **禁止**把 `rowsCoveredBySelection` 的结果写回 `selectedRows`（`00` §4.2 第 3 条）。本册的复制路径只用它做一次读取。
2. `mode === 'cell'` 时行级动作的作用域**必须**在 UI 上说清（例如提示「作用于区域覆盖的 12 行」而不是「删除 1 行」），否则用户会以为只影响当前行（`00` §4.2 第 3 条末段）。
3. 本册**不实现**区域展开与行集推导：坐标来自 `src/stores/tableData/gridSelection.ts`。`buildSelectionMatrix` 只做「坐标 → 值」，`toClipboardRange` 只做「两个已规范化的角 → 矩形」。

### 3.8 裁定：屏幕截断与剪贴板值

**问题的来源是真实的**：`CellRenderer` 把文本与 JSON 的渲染内容**硬截断到 120 字**（完整值只进 `title`），而 `DataTable.handleContextMenu` 的取值回退链最后一级是 `window.getSelection()?.toString()`——那拿到的正是**屏幕上那份被截断的文本**。于是同一格可能有两个不同的「复制结果」，且用户看不出区别。

**裁定（唯一答案）：剪贴板一律以数据源为准，永不使用 DOM 可见文本。**

| # | 规则 | 落地 |
| --- | --- | --- |
| R-A | 复制区域时，值取自 `TableState.rows`（按列名的记录），经 `buildSelectionMatrix` → `serializeCells`。**完全不读 DOM** | Step 7；`buildCopyText` 的入参里没有 DOM 元素 |
| R-B | 移除 `handleContextMenu` 里 `window.getSelection()?.toString()` 这条回退（**修 G11**）。右键 `Copy` 在无命中单元格时改为**不出现该菜单项**（`action` 不传 ⇒ 项被丢弃），而不是退化成「复制页面上恰好选中的文字」 | Step 10 |
| R-C | `getContextCellText` prop **保留**（签名不变，避免破坏调用方），但在其 JSDoc 里写清：**只能返回内存中的原始值，禁止返回 DOM 文本**。grep 确认当前 `src/` 内**没有任何调用方**为它传值，因此这条约束不会立刻伤到谁 | Step 10 |
| R-D | 复制**不因渲染截断而截断**：200 字的字符串、500 字的 JSON 复制出去都是完整值。本册**不提供**「复制屏幕上那份截断文本」的入口 | 由 R-A 自动成立 |
| R-E | 粘贴**不截断**：写入的是矩阵里的完整文本。因此「复制长值 → 粘贴到另一格」往返无损（B14 的往返断言同时覆盖长值） | Step 3 的 `planPaste` |
| R-F | 编辑器（`EditableCell`）里的复制/粘贴走原生输入框路径，`toEditString` 用 `JSON.stringify` 还原对象值——它本来就不是截断文本，无需改动 | 4.4 |

**为什么不能反过来（「复制所见即所得」）**：剪贴板在本功能里承担的是**数据交换**职责（粘回 Excel、粘回另一个数据库工具、生成 JSON/INSERT），不是截图或文本摘录；静默丢掉尾部 120 字之后的内容是不可发现的**数据损坏**，而「复制出来的比屏幕上多」是用户可以理解并接受的。真正的问题在**显示层**（120 字截断是否该改为 CSS 省略以便用户能选中完整文本），那属于显示/特殊类型分册，记入 U-11，本册不越界。

---

## 4. 交互与状态机

### 4.1 状态机：网格剪贴板

| 状态 | 进入条件 | 状态内行为 | 退出跃迁 |
| --- | --- | --- | --- |
| `idle` | 初始；`Escape`；`TableView` 的 `isEditable` 变 `false`；面板切走 | 不监听剪贴板；`onCopy` / `onPaste` 未绑定 | 点击/键盘产生焦点单元格 → `copyReady`；`editingCell != null` → `editing` |
| `copyReady` | `gridSelection.mode === 'cell'` 且有 `focus`；或 `mode === 'row'` 且 `rowsCoveredBySelection(state).size > 0` | `copy` 事件按 3.7 分派；右键菜单出现 `Copy Selected Cells`；**不**监听 `paste`（无落点）。行级取值域只**读**派生选择器，绝不写回 `selectedRows` | 产生落点（单击/方向键）→ `pasteReady`；`mode` 变 `none` → `idle`；进入编辑 → `editing` |
| `pasteReady` | `mode === 'cell'` 且有 `anchor`。**落点恒为 `normalizeCellRange(anchor, focus)` 的左上角**（单格时即该格），不是 `anchor` 本身——反向拖拽时 `anchor` 在右下角 | 原生 `paste` 触发 `planPaste` + 分批写入；右键 `Paste to Selected Cells` 走异步读剪贴板；`Escape` 不清落点 | 写入成功 → `pasteReported`；写入失败 → `pasteReported` |
| `pasteReported` | 一次粘贴结束（成功或失败） | 展示结果提示（`已填充 N 格` / `跳过 M 格` / 覆盖 K 处待提交改动）；`data-dt-cell-error` 标出问题格 | 2.5s 后自动回 `pasteReady`；用户下一次点击/按键立即回 `pasteReady` |
| `editing` | `editingCell != null`（双击或 `Return`，由 DB-05 提供） | **`onPaste` 里第一件事就是 `if (editingCell) return;`**——不 `preventDefault`，让输入框收到原生粘贴；`onCopy` 同样直接返回 | 编辑提交/取消 → 回 `pasteReady`（若落点仍在）否则 `copyReady` |

**三要素齐备性**：每个状态都有进入条件、状态内行为、退出跃迁；`editing` 有明确退出（提交/取消/`isEditable` 变假），不存在只进不出的单向锁。

### 4.2 触发路径与「为什么不注册进 `useKeyboardShortcuts`」

| 手势 | 实现路径 | 为什么 |
| --- | --- | --- |
| `mod+c` | **原生 `copy` 事件**（容器 `onCopy`） | ① 原生事件里可以**同步** `e.clipboardData.setData('text/plain', text)`，这是唯一能同时写入 `text/plain` 与将来 DB-10 的 `text/html` 而不触发「失去用户手势」的路径；② `useKeyboardShortcuts` 命中即 `preventDefault()`，若把 `mod+c` 注册进去，原生 `copy` 事件**根本不会发生**，而异步的 `writeText` 在 Tauri 的 WKWebView 里正是 G5 那条会失败的路径 |
| `mod+v` | **原生 `paste` 事件**（容器 `onPaste`） | 同理，且更严重：keydown 上 `preventDefault()` 会直接**取消** paste 事件的派发，粘贴将彻底失效。此外原生事件自带 `clipboardData`，无需异步权限——`useConnectionClipboardFill` 的注释已记录 macOS 上无手势的 `readText()` 会卡住渲染进程 |
| `mod+shift+c` | `useKeyboardShortcuts`（`scope: 'table'`） | 它不做剪贴板写入，只**打开**「Copy Selected Cells As…」菜单/面板。因此不存在「preventDefault 吞掉原生事件」的问题；`scope: 'table'` 在输入框内自动跳过也正合需要。修饰键**精确匹配**（`00` §1.3 第 1 条）保证了它与 `mod+c` 互不干扰 |
| `mod+shift+v` | **不实现** | 它没有对应的原生事件可供拦截，只能异步 `readClipboard()`，正是上面那条会卡渲染进程的路径。区域粘贴已由 `mod+v` 覆盖（见 3.7 与 4.3） |
| 右键菜单 | `buildDataTableContextMenuItems` | `Copy …` / `Paste to Selected Cells`。菜单回调经过 IPC 往返后**已失去同步用户手势**，所以复制必须走 `copyToClipboard` 的降级链（落到 `invoke('write_clipboard')`），粘贴必须走 `requirePlatformServices().readClipboard()`。这两条都是既有命令，**不新增 IPC** |

**已知的既有快捷键注册顺序风险**（`00` §1.3 第 4 条：命中即 `return`，注册顺序即优先级）：`useKeyboardShortcuts` 在 `ContentView.tsx` 已注册两条 `global`。本册新增的 `mod+shift+c` 必须与它们**无组合键重叠**（`newQuery` / `closeTab` 的默认键位不含 `mod+shift+c`），否则后注册的那条永远不生效。Step 7 的自测包含这条检查。

### 4.3 焦点与落点规则

| 场景 | 落点 | 行为 |
| --- | --- | --- |
| 单格粘贴（区域面积 1） | 该格 | 从该格按矩阵铺开；行/列不够则扩展（`overflow: 'expand'`） |
| 区域粘贴 | 区域**左上角**（`normalizeCellRange` 后的 `topRow` + `leftColumn`） | 铺开矩阵；矩阵比区域小 → 只写覆盖到的格，区域其余格 **`unset`**（不写 NULL）；矩阵比区域大 → 按 `overflow` |
| 无落点（`mode === 'none'`） | 无 | **不拦截** `paste`，浏览器默认行为生效；右键菜单不出现粘贴项 |
| 矩阵越过当前页剩余行 | 截断 | 记 `grid.paste.outOfBounds`，提示「N 格未写入，未自动翻页」 |
| 落点列在 `visibleColumns` 之外 | — | 不可能发生：落点来自 DOM 的 `data-dt-col` 或 `gridSelection`，两者都只在可见列上产生 |

**焦点要求**：滚动容器必须能获得焦点才能收到 `paste`。现状滚动容器**没有** `tabIndex`（`VirtualBody` 的行 `div` 才有）。本册给滚动容器加 `tabIndex={0}` + `data-dt-surface`，并**移除行 `div` 的 `tabIndex={0}`**——否则 `Tab` 会在每行之间跳（`VirtualBody` 的 `VirtualRow` 已把它设成 0，且行上还有 Enter/Space 的 `handleKeyDown`）。行键盘可达性改由 `data-dt-row` + 网格自身的 roving tabindex 承担。这是一个**对既有可访问性行为的修正**，记入 U-5。

### 4.4 IME 与组合输入

| 关注点 | 规则 |
| --- | --- |
| 粘贴期间 IME 组字 | `paste` / `copy` 事件与 IME 无关，不需要 `isComposing` 检查 |
| 方向键移动落点 | `useGridSelection` 在 `e.nativeEvent.isComposing === true` 时**忽略**方向键（DB-01/DB-02 负责实现，本册依赖它，因为落点漂移会让顶层 `⌘+V` 粘到错误的格） |
| 编辑器内粘贴 | `onPaste` 首行 `if (editingCell) return;`；同时 `if ((e.target as Element).closest('input,textarea,[contenteditable]')) return;`。**不 `preventDefault`**，让 `<input>` 走原生粘贴 |
| 编辑器内 `⌘+C` | `onCopy` 同样两条前置返回。在 `<input>` 内复制选中文本必须保持原生行为 |
| 组合输入与失焦提交 | `EditableCell` 的 `onBlur` 提交在 `⌘+V` 打开菜单时可能触发；因此「Copy Selected Cells As…」面板打开前必须先 `cancelEdit()`（或确保 `editingCell` 为 `null`） |

---

## 5. 实现步骤

每一步都写「改哪个文件 / 加什么符号 / 为什么 / 怎么自测」。顺序即提交顺序。

### Step 1 · 错误码表

- **改**：`src/lib/gridErrors.ts`（01 已建骨架，本册**只追加** `grid.paste.*` 码，**禁止整文件重写**）。
- **加**：`GridErrorCode`、`GRID_ERROR_CODES`、`classifyGridError`。
- **为什么**：`00` §8 要求前端有唯一的分类入口；本册新增 15 个码，必须在**同一处**登记，否则「前缀匹配顺序」会散落在各调用点。
- **自测**：`npx vitest run src/lib/__tests__/gridErrors.test.ts` —— 逐码断言 `classifyGridError(code + ': 任意说明') === code`；断言 `GRID_ERROR_CODES` 无重复；断言未知消息返回 `'unknown'`。

### Step 2 · `CellWrite` 三态与列可写判定

- **改**：`src/lib/tableChanges.ts`（新增 3.1 的全部符号）。
- **为什么**：`00` §3.3 指定该文件；`00` §3.4 要求「是否真的改动」复用 `valuesEqual`（该文件已有）。`columnWritability` 只在粘贴规划器里用，**不接进单格编辑路径**，避免改变今天的双击编辑行为。
- **自测**：`npx vitest run src/lib/__tests__/tableChanges.test.ts` —— `columnWritability({ isAutoIncrement: true })` → `writable: false`；`undefined` 列 → `reason: 'unknown-column'`；`{ isPrimaryKey: true }` → `writable: true`（主键**允许**粘贴，见 Step 8）。

### Step 3 · 剪贴板纯函数

- **改**：`src/lib/gridClipboard.ts`（新增）。
- **加**：`parseClipboardMatrix`、`toClipboardRange`、`buildSelectionMatrix`、`serializeCells`、`planPaste`、`pendingCellKey`、4 个上限常量、5 个类型。
- **为什么**：这是本册的核心。放纯函数模块是为了让第 9 节的全部畸形文本用例都能脱离 React 跑。
- **关键实现约束**：
  1. `parseClipboardMatrix` 逐**码元**扫描，只识别 ASCII 分隔符与 `"`——多字节字符与 emoji（含代理对）因此天然不被撕裂。**禁止**用 `String.prototype.split('')` 或按 `length` 切片。
  2. 引号语义：字段以 `"` 开头则读到闭合 `"`；字段内 `""` → `"`；引号内的制表符/逗号/换行都是普通字符。未闭合 → 读到文本末尾并记 `problems`（**不**抛异常）。
  3. 行分隔：`\r\n` → `\n` → 裸 `\r` 逐个归一为一次换行。**引号内的换行不切行**。
  4. 尾随换行**不产生幻影行**（`'a\n'` = 1 行 1 列）。
  5. 分隔符嗅探在**引号之外**统计：制表符 > 逗号 > 分号；三者皆 0 → 单列（不猜逗号）。
  6. ragged（行内字段数不一致）→ `ok: false` 不可用，改为抛结构化的拒绝：`parseClipboardMatrix` 返回 `{ ok: false, code: 'grid.paste.raggedRow', ... }` 需要在联合类型里补一个分支（实现时加入，签名如上扩展 `ClipboardParseResult`）。
  7. `serializeCells` 的 TSV 转义：字段含 `\t` / `\n` / `\r` / `"` 时用 `"` 包裹并双写内部 `"`。这**修掉 G2**，且与 Excel 的双向解析一致。
  8. `planPaste` 的 NULL 判定（唯一权威规则）：
     - 引号包裹的空字段 `""` → `cellWriteValue('')`（显式空串）
     - 未引号空字段 → `column.nullable === false` 时记 `grid.paste.notNullEmpty` 并 `unset`；否则 `cellWriteNull()`
     - 未引号且内容大小写不敏感等于 `nullLiteral`（缺省 `'NULL'`）或等于 `\N` → `cellWriteNull()`（`nullable === false` 时同上记问题）
     - 引号包裹的 `"NULL"` → `cellWriteValue('NULL')`（字符串）
     - 其余 → `cellWriteValue(text)`
  9. `planPaste` 的可写判定：`columnWritability(column).writable === false` → 该列**所有**目标格 `unset` + 记一次 `grid.cell.readOnly`（`detail` = 列名，去重进 `skippedColumns`）。
  10. `planPaste` 的溢出：`overflow === 'expand'` 时把 bounds 扩到矩阵尺寸，但 `bottomRow` 被 `pageRowCount - 1` 截断，被截断的格计 `skipped` + 一次 `grid.paste.outOfBounds`。
  11. `planPaste` 超限：格数 > `maxCells` 或 `byteLength` > `GRID_PASTE_MAX_BYTES` → 返回 `writes: []` + `grid.paste.tooLarge`（**不部分写入**）。
  12. **列对齐（G12，最高优先级，编号靠后但必须先实现）**：`source?.columnNames` 存在且长度等于 `matrix.width` 时按列名对齐（`alignment: "byName"`），无同名列进 `droppedColumns` + `grid.paste.columnMismatch`；同名列为 0 ⇒ 判定跨表，退回位置对齐 + `crossTable: true` + `requiresConfirmation: true`。
  13. **`requiresConfirmation` 的缺省恒为 `true`**；只有 `source && source.tableKey === tableChangeKey(context)` 才置 `false`。
  14. `recordClipboardSource` / `peekClipboardSource` / `clearClipboardSource` / `tableChangeKey` 一并导出；`tableChangeKey` 的小写与 `null`→`''` 归一是必测项。
- **自测**：`npx vitest run src/lib/__tests__/gridClipboard.test.ts`，用例见 9.1。

### Step 4 · 抽出批量写入纯函数

- **改**：`src/stores/tableData/stageCellWrite.ts`（新增）。
- **加**：`stageCellWritesInto`、`rebuildEditBufferOnce`、`CellWriteProblem`、`StageCellWritesOutcome`。
- **为什么**：G9 与 G7 的根因都在「逐格 `patchPanel` + 每次全量 `rebuildEditBuffer`」。把逐格逻辑抽成**纯函数**（输入 `TableState`、输出新三件套）是本册唯一能让「单格」与「批量」共用同一套行身份校验的做法；也是 `tableDataStore.ts` **净减行数**的唯一办法（该文件已 722 行）。
- **怎么抽**：把 `stageCellChange` 的函数体逐行搬进 `stageCellWritesInto`，只做三处改造：(a) 循环 N 格；(b) 把 `patchPanel` 的副作用换成往 `outcome` 里累积；(c) 把依赖 `t()` 的文案换成 `problems[].code`。**`findPendingForRow` / `rowIdentityIsUnique` / `hasPendingIdentityCollision` / `valuesEqual` 的调用顺序与条件必须一模一样**——A2 的深度相等断言就是这条的守门人。
- **自测**：`npx vitest run src/stores/tableData/__tests__/stageCellWrite.test.ts` —— ① 单格等价性（与重构前的快照逐字段比对）；② 空输入不产生新对象（引用相等）；③ 5 000 格只产生一次 `outcome`；④ 一格歧义不影响其余 4 999 格。

### Step 5 · store 接线

- **改**：`src/stores/tableDataStore.ts`。
- **改/加**：`stageCellChange` 变薄包装（调 `stageCellWritesInto` 后把 `problems[0].code` 映射回既有 `t('tableData.noPrimaryKey')` / `AMBIGUOUS_ROW_IDENTITY_ERROR`，行为逐字不变）；新增 `stageCellWrites`。
- **为什么**：对外契约（`TableDataStore` 接口）不变，内部批量化；`tableDataStore.ts` 因此净减行数（它已 722 行）。
- **依赖而不重复实现**：翻页/排序/筛选后的选择清空**由 DB-01 负责**，落地在 `patchPanelForReload` / `commitFetchedPage` / `invalidateCachedData` 三个收敛点（`00` §4.3 推论）。本册**不**在 `setPage` / `setSort` / `applyFilters` 里各加一行清空——那正是缺陷当初的产生方式，两册各修一半会留下更隐蔽的半修状态。
- **自测**：`npx vitest run src/stores/__tests__/tableDataStore*.test.ts` 全绿（既有回归门槛）；另加一条**守卫测试**：`setPage` 后 `gridSelection.mode === 'none'` 且 `selectedRows.size === 0`。若 DB-01 尚未落地，这条测试会红——这是正确的失败，应如实上报而不是在本册补一行让它变绿。

### Step 6 · 复制反馈 hook

- **改**：`src/hooks/useClipboardCopyFeedback.ts`（新增）。
- **加**：`useClipboardCopyFeedback`。
- **为什么**：G5。`packages/ui` 的 `useCopyFeedback` 明确不能带降级链（不允许 import `@tauri-apps/api`），所以宿主需要一个同语义、带降级链的实现。复制长 TSV 到系统剪贴板必须在 WKWebView 上可靠。
- **自测**：`npx vitest run src/hooks/__tests__/useClipboardCopyFeedback.test.tsx` —— 复用 `src/test/copyFeedbackHarness.ts`（`installControlledClipboard` / `installResolvedClipboard` / `advanceBy` / `spyOnWindowTimers`）的三条既有保证：窗口逐边界、卸载清计时器、迟到 rejection 不擦掉新一次成功。**注意**：该 harness 只 mock `navigator.clipboard.writeText`，本 hook 还要走 `invoke` 降级，因此需在其上补一个 `write_clipboard` 的 invoke mock（`vi.mock('@tauri-apps/api/core')`）。

### Step 7 · 剪贴板 hook（网格接线）

- **改**：`src/hooks/useGridClipboard.ts`（新增）。
- **加**：`useGridClipboard(options)`，返回 `{ handlers: GridClipboardHandlers; copySelectedCellsAs: (format) => void; copySelectedCellsMenuOpen; ... }`。
- **输入**：`{ panelId, isEditable, columns: readonly ColumnSchema[], columnOrder, rows, pendingChanges, gridSelection, title, onProblem }`。
- **行为**：
  - `buildCopyText(target)`：`row` → `serializeCells(buildSelectionMatrix(...), { format: 'tsv' })`（全列）；`cell` → `toClipboardRange` + `buildSelectionMatrix` + `serializeCells`。返回 `null` 的情形：无选择、`range === null`。
  - `onPasteText(text, anchor)`：`parseClipboardMatrix` → `planPaste` → 若 `writes.length > GRID_PASTE_CONFIRM_CELLS` 先 `confirmPaste(...)`（用 `useConfirmDialog`）→ 分批调 `store.stageCellWrites` → 汇总 `PasteOutcome` → 返回 `true`。。**注意**：本步必须在调用 `planPaste` 前先 `const source = peekClipboardSource()` 传进去，并在 `planPaste` 之后、`stageCellWrites` **之前**判断 `requiresConfirmation`——顺序反了就成了「先写再问」，与 3.2b 的设计意图相反
  - `onPasteFromClipboard(anchor)`：`await requirePlatformServices().readClipboard()`（try/catch → `grid.clipboard.readFailed`）→ 走同一条 `onPasteText` 逻辑。
  - 只读拦截：`!isEditable` 时 `onPasteText` 立即 `showReadOnlyTip()` 并返回 `true`（消费掉，避免落到浏览器默认），复制仍可用（只读表当然可以复制）。
  - 超大批量：`writes.length > GRID_PASTE_CHUNK_CELLS` 时分批 `stageCellWrites`，每批之间 `await new Promise<void>(resolve => { window.requestAnimationFrame(() => resolve()); })`，并在提示里给进度。
- **为什么放 hook**：G10 —— 列元数据（`ColumnSchema`）在 `TableView` 层，`DataTable` 拿不到；同时 `TableView.tsx` 已 770 行，只能容纳「一次 hook 调用 + 3 个 prop」。
- **自测**：`npx vitest run src/hooks/__tests__/useGridClipboard.test.tsx`（渲染一个最小宿主，断言 onClick/onPaste 的调用序列与 store 调用次数）。

### Step 8 · 右键菜单项

- **改**：`src/lib/dataTableContextMenu.ts`。
- **加**：`DataTableContextMenuLabels` 增 `copySelectedCells` / `copySelectedCellsAs` / `pasteSelectedCells`；`DataTableContextMenuHandlers` 增 `onCopySelectedCells` / `onCopySelectedCellsAs` / `onPasteSelectedCells`；`BuildDataTableContextMenuArgs` 增 `hasCellRange?: boolean`（缺省 `false`）、`canCopySelection?: boolean`（缺省 `false`）、`canPasteSelection?: boolean`（缺省 `false`）、`selectionIgnoredRanges?: number`（缺省 `0`）。
- **行为**：`cell` 布局下把三项放在 `frequent` 段（复制粘贴是高频）；`row` 布局下只加 `Paste rows`（由 DB-04 提供 handler，本册只留位置）；`enabled: false` 用于只读/无落点场景（`NativeMenuItemDef` 支持 `enabled`）。`item()` 工厂的「`action` 为 `undefined` 即丢弃」语义必须保留：**不可用 = `enabled: false` 且仍有 action**，**不存在 = 不传 action**。两者不可混淆。
- **为什么**：`00` §1.2 要求右键菜单统一走 Web Context Menu；`buildDataTableContextMenuItems` 是全仓唯一入口。
- **自测**：`npx vitest run src/lib/__tests__/dataTableContextMenu.test.ts`。既有用例的 `ids()` 断言在**不传**新增 args 时必须与今天逐字相同（缺省不出现新项）。

### Step 9 · 统一序列化（薄包装，禁止双实现）

- **改**：`src/lib/dataTableContextMenu.ts`。
- **改**：`serializeDataTableRowsAsTsv` / `serializeDataTableRowsAsCsv` / `serializeDataTableRowsAsJson` 改为调用 `serializeCells` 的薄包装。
- **为什么**：G4。三份 TSV 实现必须收敛，否则「复制」与「导出」永远不一致。`serializeDataTableRowsAsTsv` 的**对外契约需要一次刻意的行为变更**（加引号转义、对象值从 `[object Object]` 改为 `JSON.stringify`），这是修复而不是回归，但会让既有快照/断言变化——第 9.3 节列出必须同步修改的既有测试。
- **边界**：`exportData.ts` / `exportStream.ts` 的 `escapeTSV` **本册不动**（导出链路有自己的流式与内存约束）；在 U-4 记录「三份仍有三处，DB-10 收口」。

### Step 10 · `DataTable` 接线与 `VirtualBody` 属性

- **改**：`src/components/DataTable/DataTable.tsx`：① 新增 3 个 props；② 把滚动容器 `onCopy` / `onPaste` 绑到 `clipboard`；③ 滚动容器加 `tabIndex={0}` + `data-dt-surface`；④ 把 `handleContextMenu` 的主体搬去 `useDataTableContextMenu`（Step 11）；⑤ 删除局部 `copyText`。
- **改**：`src/components/DataTable/VirtualBody.tsx`：① 单元格加 `data-dt-cell-error={problem ? 'true' : undefined}`（契约 §4.4 之外的**增量**属性，见 U-7）；② 移除行 `div` 的 `tabIndex={0}`（焦点改由网格容器承担）；③ 单元格单击时 `stopPropagation` 的取舍由 DB-01 决定（本册不碰）。
- **为什么**：`00` §4.4 要求定位走 `data-*`；`tabIndex` 是「原生 paste 能不能收到」的前提。
- **自测**：`npx vitest run src/components/DataTable/` 全绿；新增 `DataTable.clipboard.test.tsx` 断言「不传 `clipboard` 时容器没有 `onCopy`/`onPaste` 的可观测效果」。

### Step 11 · 接线 `TableView.tsx`（复用 01 已抽出的组件，不新建）

- **改**：`src/windows/connection/TableView.tsx`：调用 `useGridClipboard`，把 `clipboard` / `gridSelection` / `onClipboardProblem` 传给 `DataTable`，并加 `confirmPaste` 对话框。
- **前置（由 DB-01 提供，本册不创建）**：`src/windows/connection/TablePendingChangesBar.tsx` —— 待提交改动工具条，抽取 `table-tx-controls` + `pending-changes-bar` 两块，逐字保留既有 testid；由**分册 01 的 Step 10 首次创建**（它是 02/03/05/06/07 的共同依赖，提交顺序最前），命名在 01/03/06 之间统一为 `TablePendingChangesBar`。
- **本册对它的用法**：**只复用，不另建**。若需要额外展示（例如粘贴结果摘要、复制反馈），通过**给它新增可选 props** 扩展。**禁止**在 `src/windows/connection/` 下另建平行组件——统一名是 `TablePendingChangesBar`（分册 01 的 Step 10 首次创建）；`TableViewPendingBar` / `PendingChangesBar` 这类变体名都不允许出现。
- **为什么**：`TableView.tsx` 已 770 行（96%），接线约 +20 行，工具条由 01 抽走约 −150 行，净得 ~90 行余量给 01/02/05/06。**不依赖 01 的抽取就一定会爆 800**。
- **自测**：`pnpm typecheck` + `npx vitest run src/windows/connection/` 全绿；`wc -l src/windows/connection/TableView.tsx` < 800。

---

## 6. 文件级改动清单

| 文件路径 | 新增/修改 | 职责 | 预估行数 | 触及 800 行上限？ |
| --- | --- | --- | --- | --- |
| `src/lib/gridErrors.ts` | **修改（追加）** | 01 已建骨架；本册按契约 §8.1 三步追加 `grid.paste.*` 码，**禁止整文件覆盖** | +15 | 否 |
| `src/lib/tableChanges.ts` | 修改 | 增 `CellWrite` 三态、构造器、`CellWriteInput`、`columnWritability` | +55（227 → ~282） | 否 |
| `src/lib/gridClipboard.ts` | 新增 | 二维解析、区域→矩阵、序列化、粘贴规划、超限常量 | ~340 | 否 |（本册新建；**含 3.2b 的 `ClipboardSource` / `recordClipboardSource` / `peekClipboardSource` / `tableChangeKey`**）
| `src/stores/tableData/stageCellWrite.ts` | 新增 | 批量写入纯函数（从 `stageCellChange` 抽出） | ~160 | 否 |
| `src/stores/tableDataStore.ts` | 修改 | `stageCellChange` 变薄包装；新增 `stageCellWrites`；翻页/排序清 `gridSelection` | **−80 / +25**（722 → ~667） | 否（净减） |
| `src/stores/tableData/types.ts` | 修改 | 增 `gridSelection`（若 01 已加则不改） | +3 | 否 |
| `src/hooks/useClipboardCopyFeedback.ts` | 新增 | 带降级链的请求绑定复制反馈 | ~90 | 否 |
| `src/hooks/useGridClipboard.ts` | 新增 | 复制/粘贴编排、确认、分批、问题提示 | ~190 | 否 |
| `src/lib/dataTableContextMenu.ts` | 修改 | 新增 3 个菜单项 + labels/handlers/args；既有序列化收敛为 `serializeCells` 薄包装 | +60 / −15（398 → ~443） | 否 |
| `src/components/DataTable/useDataTableContextMenu.ts` | 新增 | 从 `DataTable.handleContextMenu` 搬出的菜单组装（含复制反馈接线） | ~240 | 否 |
| `src/components/DataTable/DataTable.tsx` | 修改 | 新增 3 props、`onCopy`/`onPaste`/`tabIndex` 接线、删除局部 `copyText` 与 `handleContextMenu` 主体 | **−170 / +45**（628 → ~503） | 否（净减） |
| `src/components/DataTable/VirtualBody.tsx` | 修改 | `data-dt-cell-error`；移除行 `tabIndex` | +12 / −2（207 → ~217） | 否 |
| `src/windows/connection/TablePendingChangesBar.tsx` | **01 新增，本册只复用** | 待提交改动工具条（`table-tx-controls` + `pending-changes-bar`）；命名在 01/03/06 之间统一，由 01 的 Step 10 首次创建 | ~150（**不计入本册**） | 否 |
| `src/windows/connection/TableView.tsx` | 修改 | `useGridClipboard` 接线 + 粘贴确认框；工具条由 01 移出（本册依赖该前置，不自行拆分） | **−（01 的抽取）/ +20**（770 → ~640） | 否（依赖 01 抽取后安全） |
| `src/locales/en/query.ts` | 修改 | 剪贴板文案（第 8 节） | +32（424 → ~456） | 否 |
| `src/lib/__tests__/gridClipboard.test.ts` | 新增 | 解析/序列化/规划单测 | ~330 | — |（含 `tableChangeKey` 的大小写与 `null` 归一）
| `src/lib/__tests__/gridClipboard.crossTable.test.ts` | **新增** | 3.2b 三层对齐的纯逻辑单测（同表免确认 / 按名对齐 / 名不匹配退回位置 / 来源未知强制确认）。**独立文件**，便于单独验收 G12 | ~90 | 否 |
| `src/lib/__tests__/gridErrors.test.ts` | 新增 | 错误码分类单测 | ~45 | — |
| `src/stores/tableData/__tests__/stageCellWrite.test.ts` | 新增 | 批量写入与单格等价性 | ~180 | — |
| `src/hooks/__tests__/useClipboardCopyFeedback.test.tsx` | 新增 | 复制反馈三条保证 | ~120 | — |
| `src/hooks/__tests__/useGridClipboard.test.tsx` | 新增 | 编排行为 | ~150 | — |
| `src/components/DataTable/__tests__/clipboard.journey.test.tsx` | 新增 | **连续旅程测试** | ~220 | — |
| `src/components/DataTable/__tests__/DataTable.clipboard.test.tsx` | 新增 | 组件级绑定与只读行为 | ~120 | — |
| `e2e/specs/table-clipboard.ts` | 新增 | Host E2E | ~180 | — |

**净行数变化**：源码文件合计约 **−90 行**（三个既有大文件净减，新逻辑全在新文件里），符合「单源码文件 ≤ 800 行」且不触碰上限。

**必须同步修改的既有测试**（因为 Step 9 的序列化收敛）：

| 既有测试 | 变更原因 |
| --- | --- |
| `src/lib/__tests__/dataTableContextMenu.test.ts` | `serializeDataTableRowsAsTsv` 的断言：含制表符/换行/引号/对象值的用例期望值会变（这是修 G2/G3/G4） |
| `src/components/DataTable/DataTable.test.tsx` | 若断言了拷贝文本或菜单项 id 序列，需按新增项更新；**不传新 args 时不得出现新菜单项** |
| `src/components/DataTable/__tests__/CellRenderer.test.tsx` / `VirtualBody.test.tsx` | 移除行 `tabIndex` 后若有可访问性断言需更新 |

---

## 7. 边界与异常清单

| # | 场景 | 期望行为 |
| --- | --- | --- |
| B1 | **空表**（`rows.length === 0`，有列） | 复制：无选择，不拦截 `copy`。粘贴：无落点（`mode` 必为 `none`），不拦截 `paste`；右键菜单不出现粘贴项 |
| B2 | **空列**（`columns.length === 0`，全部隐藏） | `DataTable` 走 `emptyPlaceholder` 分支，`VirtualBody` 不渲染，无落点。粘贴不出现 |
| B3 | **单格复制** | `mode === 'cell'` 且区域面积 1 → 复制该格文本（**不含**引号包裹，除非值本身含特殊字符）。空串仍然可以复制（`text` 为 `''`，但 `copyToClipboard('')` 返回 `false` → `grid.clipboard.writeFailed`。**这是既有 helper 的语义**：坚持不谎报成功，提示「无法复制空内容」） |
| B4 | **单格粘贴** | 从该格铺开整块矩阵，行列自动扩展；右下角越出当前页 → 截断 + `grid.paste.outOfBounds` |
| B5 | **整表复制**（`⌘+A` 后 `⌘+C`） | `mode === 'cell'` 时 `⌘+A` 由 DB-02 定义为「全选当前页」，于是复制当前页全部可见列 × 全部行。**不含表头**。大表时只复制当前页（`totalRows` 跨页部分**不**复制），并在提示里说明「已复制当前页 N 行」 |
| B6 | **粘贴到只读列**（`isAutoIncrement === true`） | 该列全部目标格 `unset`；记一次 `grid.cell.readOnly`（`detail` = 列名）；提示 `dataTable.pasteReadOnlyColumn` 的聚合文案（`已跳过 N 列：a, b`）。**其余列照常写入** |
| B7 | **粘贴到只读连接 / 只读驱动** | `isEditable === false`：粘贴入口整体不可用（菜单项 `enabled: false` + 文案 `dataTable.readOnlyPasteDisabled`）；原生 `paste` 被消费但不写任何格；复制**仍然可用** |
| B8 | **无主键表** | 复制可用；粘贴被拒（`grid.cell.noRowIdentity`），提示 `dataTable.pasteNoRowIdentity`。与今天 `stageCellChange` 拒绝编辑的语义一致 |
| B9 | **粘贴含 `NULL` 文本** | 未引号、内容为 `NULL`/`null`/`\N` → 写 SQL NULL；引号包裹的 `"NULL"` → 写字符串 `'NULL'`。落到 `nullable === false` 列 → 记 `grid.paste.notNullEmpty`，该格 `unset`，其余格照常 |
| B10 | **粘贴含空字段** | 未引号空字段 → NULL（可空列）/ 报 `grid.paste.notNullEmpty`（不可空列）。引号包裹的 `""` → 空串 `''`。**这一条是 B9 的判据来源，必须有专门的单测覆盖「同一行里 NULL 与空串并存」** |
| B11 | **含引号的值** | 复制时 `he said "hi"` → `"he said ""hi"""`；粘回 → `he said "hi"`。往返必须逐字符相等 |
| B12 | **含制表符的值** | 复制时被引号包裹；粘回后仍是**一格**。**这是修 G2 的核心用例** |
| B13 | **含换行的值** | 复制时被引号包裹；粘回后仍是**一格多行值**，不是两行。若用户从 Excel 复制带内嵌换行的单元格，Excel 也会加引号，因此往返成立 |
| B14 | **多字节与 emoji** | `'😀😀'`、`'中文'`、组合字符（`'é'` = `e` + U+0301）在解析与序列化中**逐码元通过**，不得被撕裂或转义。单测断言 `parseClipboardMatrix(serializeCells([[...]]))` 的往返等于原值 |
| B15 | **行列不一（ragged）** | 解析阶段拒绝：`grid.paste.raggedRow`，指出第一处不一致的行号与两侧字段数，**不写入任何格**（不做部分粘贴） |
| B16 | **矩阵形状与区域不匹配** | 矩阵 ≤ 区域：只写覆盖到的子矩形，区域其余格 `unset`（**不**写 NULL）。矩阵 > 区域且 `overflow === 'expand'`：扩展至矩阵尺寸（受 `pageRowCount` 限）。`overflow === 'clip'`：截断到区域尺寸 + `grid.paste.outOfBounds` |
| B17 | **超大粘贴** | 格数 > 100 000 或字节 > 10 MiB → 拒绝整次（`grid.paste.tooLarge`），提示实际值与上限并引导到导入功能。2 000~100 000 格 → 先弹确认框（回显将写入的格数、行数、列数、将覆盖的待提交改动数）。≤ 2 000 格 → 静默执行 + 结果提示 |
| B18 | **超大批量的分批** | 每 5 000 格一次 `stageCellWrites`，批间让出一次 `requestAnimationFrame`；全程只在最后重算一次 `editBuffer`；任一批失败即停止后续批次并如实报告「已写入 N 格」（**不谎报全成**） |
| B19 | **粘贴的字面量绝不执行** | 粘贴内容只经 `Value` → `pendingChanges` → 驱动 `build_update_sql` 渲染；`formatRowAsSqlInsert` / `formatRowsAsSqlUpdate` 等**只用于复制**，不得被粘贴路径调用。禁止任何形式的「把粘贴文本拼成 SQL 再跑」 |
| B20 | **粘贴内容进入日志/埋点** | 禁止。PRD §8.4：不采集单元格值。埋点只记格数、类型族、失败格数（`grid.clipboard.paste`，见 PRD §10） |
| B21 | **跨页选择** | 选择模型只覆盖当前页（`00` §4.1：`rowIndex` 是**当前页内**下标），复制只作用于当前页。翻页/排序/筛选变化后的清空**由 DB-01 保证**：`00` §4.3 推论要求在 `patchPanelForReload` / `commitFetchedPage` / `invalidateCachedData` 三个收敛点统一清空 `gridSelection` + `selectedRows` + `lastSelectedIndex`。**本册不自行修补**（两册各修一半会留下更隐蔽的半修状态）；本册只在第 9 节写一条**守卫测试**断言清空确实生效。若守卫测试读到越界下标，按 DB-01 的缺陷上报，**不得**在本册静默丢弃下标——静默丢弃会把 01 的缺陷藏起来，而它的其他消费者（删除选中行、导出选中）仍在错 |
| B22 | **编辑中粘贴** | `onPaste` 首行返回，不 `preventDefault`；文本进入 `<input>`。`⌘+C` 在输入框内保持原生行为 |
| B23 | **剪贴板读失败**（权限、平台服务未绑定） | `grid.clipboard.readFailed`，提示改用 `⌘+V`。**禁止**静默失败，禁止回退到「猜内容」 |
| B24 | **剪贴板写失败** | `grid.clipboard.writeFailed`，`copied` 保持 `false`，不回滚成功态以外的任何状态 |
| B25 | **`pendingChanges` 覆盖** | 粘贴覆盖既有暂存改动时**不弹窗**（否则高频操作被打断），但结果提示**必须**回显覆盖了几处（`dataTable.pasteOverwrotePending`） |
| B26 | **主键列被粘贴** | 允许写入，但每格都要过 `stageCellWritesInto` 的 `hasPendingIdentityCollision` / `rowIdentityIsUnique`；会导致身份不唯一的格记 `grid.paste.identityChanged` 并跳过，其余格照常 |
| B27 | **剪贴板内容来自另一张表**（应用内复制，`source.tableKey !== tableChangeKey(context)`） | **允许写入，但必须先确认**：文案含源表与目标表全名、列名对齐结果（`matched`/`total`）与被丢弃列。默认动作「取消」。确认后按 `alignment` 写，**被丢弃的列一个格都不写** |
| B28 | **剪贴板内容来自应用之外**（Excel / Numbers / 另一个 DataZen 窗口，`source === undefined`） | 一律按位置对齐 + **强制确认**（文案明说来源未知）。这是 B27 的兜底：L1 的 `lastCopySource` 与真实剪贴板**没有强绑定**，用户在 Excel 里复制一次它就过期了，所以「来源未知」必须按「可能跨表」处理，不能按「同表」处理 |
| B27 | **目标格的值与原值相等** | 沿用既有语义：该列改动被**删除**；该行若无其它改动且未标删除则整条 `pendingChanges` 移除。因此「粘贴 12 格但其中 3 格值未变」时待提交计数是 9，**不是** 12。结果提示用实际 `staged` |
| B28 | **粘贴后预览已失效** | `previewPlan` 置 `null`、`pendingStatus` 置 `'idle'`（与 `stageCellChange` 一致）。指纹纪律（`00` §1.4 事实 5）因此不被绕过 |

---

## 8. i18n key 清单

**域名文件**：`src/locales/en/query.ts`（`dataTable.*` 与 `tableData.*` 两组既有 key 都在此文件，新增文案必须落在同一文件，就近插入）。

**严禁**改 `src/locales/en.ts`（它只是 `export { default } from './en/index'`）；**不要**新建域名；**不要**动任何其他语言文件。

| Key（完整路径） | 英文文案 | 用途 |
| --- | --- | --- |
| `dataTable.copySelectedCells` | `Copy Selected Cells` | 右键菜单 |
| `dataTable.copySelectedCellsCount` | `Copy Selected Cells ({rows} × {cols})` | 菜单标签带面积 |
| `dataTable.copySelectedCellsAs` | `Copy Selected Cells As…` | 菜单与 `⌘+⇧+C` |
| `dataTable.copySelectedCellsAsTsv` | `Copy as TSV` | 子项 |
| `dataTable.copySelectedCellsAsTsvWithHeader` | `Copy as TSV with Header` | 子项 |
| `dataTable.copySelectedCellsAsCsv` | `Copy as CSV` | 子项 |
| `dataTable.copySelectedCellsAsJson` | `Copy as JSON (2D array)` | 子项 |
| `dataTable.copySelectionIgnoredRanges` | `Copied the primary selection only; {count} extra range(s) ignored.` | B/3.7 |
| `dataTable.copiedCells` | `Copied {count} cell(s).` | 复制反馈 |
| `dataTable.copyFailed` | `Could not write to the clipboard.` | `grid.clipboard.writeFailed` |
| `dataTable.pasteToSelectedCells` | `Paste to Selected Cells` | 右键菜单 |
| `dataTable.pasteRows` | `Paste Rows` | 菜单占位（**handler 由 DB-04 提供**） |
| `dataTable.pasteEmpty` | `There is no text on the clipboard to paste.` | `grid.paste.empty` |
| `dataTable.pasteResult` | `Pasted {cells} cell(s) ({rows} row(s) × {cols} column(s)).` | 结果提示 |
| `dataTable.pasteResultSkipped` | `Pasted {cells} cell(s); {skipped} skipped.` | 结果提示 |
| `dataTable.pasteOverwrotePending` | `{count} staged change(s) were overwritten.` | B25 |
| `dataTable.pasteReadOnlyColumn` | `Skipped {count} read-only column(s): {columns}` | B6 |
| `dataTable.pasteClippedToPage` | `{count} cell(s) fell outside the current page and were not written.` | B4 / B16 |
| `dataTable.pasteRaggedRow` | `Row {row} has {count} field(s) but the block uses {expected}. Nothing was pasted.` | B15 |
| `dataTable.pasteUnclosedQuote` | `A quoted field was never closed; the remaining text was treated as one cell.` | `grid.paste.unclosedQuote` |
| `dataTable.pasteTooLarge` | `The block is too large ({cells} cell(s), limit {maxCells}); use Import instead.` | B17 |
| `dataTable.pasteNotAllowedEmpty` | `Row {row}, column {column} cannot be empty; NULL was not written.` | B9 / B10 |
| `dataTable.pasteIdentityChanged` | `Row {row}, column {column} is part of the row identity; the change was skipped.` | B26 |
| `dataTable.pasteNoWritableColumn` | `None of the pasted columns can be written here.` | `grid.paste.noWritableCell` |
| `dataTable.pasteCrossTableTitle` | `Paste content from another table` | `grid.paste.crossTable` |
| `dataTable.pasteCrossTableBody` | `The clipboard content came from a different table ({source}). It will be written to {target}. Only {matched} of {total} columns match by name.` | `grid.paste.crossTable` |
| `dataTable.pasteUnknownSourceBody` | `The clipboard source is unknown (it may come from Excel or another app). {total} columns will be written to {target} by position.` | `grid.paste.crossTable` |
| `dataTable.pasteDroppedColumns` | `{count} pasted columns have no matching column here and will not be written: {names}.` | `grid.paste.columnMismatch` |
| `dataTable.pasteNoRowIdentity` | `Cannot paste: this table has no primary key.` | B8 |
| `dataTable.pasteConfirmTitle` | `Paste {cells} cells?` | B17 确认框标题 |
| `dataTable.pasteConfirmMessage` | `Writes {rows} row(s) × {cols} column(s) and overwrites {overwritten} staged change(s).` | B17 确认框正文 |
| `dataTable.pasteReadFailed` | `Could not read the clipboard. Use {shortcut} instead.` | `grid.clipboard.readFailed` |
| `dataTable.readOnlyPasteDisabled` | `Cannot paste cells: connection or driver is read-only.` | B7（与既有 `tableData.readOnlyEditDisabled` 并列，措辞对齐） |

**复用而不新增**：`tableData.readOnlyEditDisabled`（只读提示）、`tableData.noPrimaryKey`（单格路径逐字不变）、`tableData.commit`（确认框确认按钮）、`common.copy`（菜单复制主项）、`export.export`。

**设置项文案**（`src/locales/en/settings.ts`）**仅当 U-2 裁定为「暴露设置」时才需要**：`settings.grid.pasteOverflow`、`settings.grid.pasteOverflowExpand`、`settings.grid.pasteOverflowClip`。缺省方案是把 `overflow` 作为 `useGridClipboard` 的参数（默认 `'expand'`），不暴露 UI。

**文案纪律**：`{columns}` 插值只放**列名**，绝不放单元格值（PRD §8.4）。

---

## 9. 测试清单

### 9.1 纯逻辑单测 · `src/lib/__tests__/gridClipboard.test.ts`

**解析（`parseClipboardMatrix`）——畸形输入必须逐条覆盖**

| 用例名 | 输入 | 断言要点 |
| --- | --- | --- |
| `parses empty text as grid.paste.empty` | `''`、`'   '` | `ok: false`，`code === 'grid.paste.empty'` |
| `parses a single value without delimiters` | `'abc'` | 1 行 1 列，`delimiter === '\t'`（无分隔符 → 单列） |
| `parses a single row with tabs` | `'a\tb\tc'` | 1 行 3 列 |
| `parses CRLF rows` | `'a\tb\r\nc\td'` | 2 行 2 列 |
| `parses bare CR rows` | `'a\tb\rc\td'` | 2 行 2 列 |
| `drops a single trailing newline` | `'a\tb\n'` | 1 行 2 列（**不是** 2 行） |
| `keeps interior blank lines as blank rows` | `'a\n\nb'` | 3 行（中间空行保留，因为行数影响落点） |
| `sniffs comma when no tab present` | `'a,b\nc,d'` | `delimiter === ','`，2×2 |
| `sniffs semicolon` | `'a;b'` | `delimiter === ';'` |
| `prefers tab over comma` | `'a,b\tc'` | `delimiter === '\t'` |
| `keeps a quoted tab inside one cell` | `'"a\tb"\tc'` | 1 行 2 列，`cells[0][0] === 'a\tb'`，`quoted[0][0] === true` |
| `keeps a quoted newline inside one cell` | `'"a\nb"\tc'` | 1 行 2 列，`cells[0][0] === 'a\nb'` |
| `unescapes doubled quotes` | `'"a""b"'` | `cells[0][0] === 'a"b'` |
| `keeps a quoted comma inside one cell` | `'a,"b,c"'` | 1 行 2 列 |
| `marks quoted empty as explicit empty string` | `'""\tNULL'` | `quoted[0][0] === true`，`cells[0][0] === ''` |
| `flags an unclosed quote instead of throwing` | `'"abc'` | `ok: true` + problem `grid.paste.unclosedQuote`，`cells[0][0] === 'abc'` |
| `rejects ragged rows` | `'a\tb\nc'` | `ok: false`，`code === 'grid.paste.raggedRow'`，指出第 2 行 |
| `preserves surrogate pairs` | `'😀\t中文\té'` | 三格逐码元相等（含组合字符 `e` + U+0301） |
| `rejects text over the byte limit` | 11 MiB 的 `'a'` | `ok: false`，`code === 'grid.paste.tooLarge'` |
| `rejects more than the cell limit` | 100 001 格的制表符文本 | `ok: false`，`code === 'grid.paste.tooLarge'` |

**序列化（`serializeCells`）**

| 用例名 | 断言要点 |
| --- | --- |
| `quotes a value containing a tab` | 输出 `"a\tb"` 形态；再解析回来仍是 1 格 |
| `quotes a value containing a newline` | 同上 |
| `doubles embedded quotes` | `he said "hi"` → `"he said ""hi"""` |
| `renders null as empty by default` | `null` → `''` |
| `renders null as the configured literal` | `nullLiteral: 'NULL'` → `'NULL'`，且空串仍为 `''`（两者可分） |
| `serializes objects as JSON not [object Object]` | `{a: 1}` → `{"a":1}`（修 G4） |
| `omits the header by default for tsv` | 首行是数据 |
| `includes the header when asked` | `includeHeader: true` → 首行为列名 |
| `csv keeps its header default` | `format: 'csv'` 且未给 `includeHeader` → 首行是列名（不回退今天的行为） |
| `json 2d array by default` | `[['a','b']]` → `[["a","b"]]` |
| `json objects when asked` | 输出带列名的对象数组（复用 `rowToNamedRecord`） |
| `round-trips a whole matrix` | `parse(serialize(m)) === m`（属性测试式断言，含引号/换行/emoji 混排） |
| `keeps values longer than 120 characters intact` | 500 字字符串与 500 字 JSON → 输出**完整值**、不含 `…`（§3.8 R-D；这是显示截断不得渗进剪贴板的守门用例） |

**规划（`planPaste` / `buildSelectionMatrix` / `toClipboardRange`）**

| 用例名 | 断言要点 |
| --- | --- |
| `maps columns by name not by index` | 传入 `columnOrder` 与 `rows` 记录，且**故意把 `visibleColumns` 顺序与 `columns` 顺序错开** → 取值正确（G6 的守门用例） |
| `writes null for an unquoted empty field` | `write.kind === 'null'` |
| `writes an empty string for a quoted empty field` | `write.kind === 'value'`，`value === ''` |
| `reports notNullEmpty on a NOT NULL column` | problem `grid.paste.notNullEmpty`，该格不在 `writes` 里 |
| `skips auto-increment columns entirely` | 该列所有格 `unset`，`skippedColumns` 含列名 |
| `does not write unset cells` | 矩阵小于区域时，区域其余格不出现在 `writes` 里 |
| `expands beyond the region up to the page end` | `overflow: 'expand'`，bounds 覆盖矩阵，越页部分计 `skipped` + `grid.paste.outOfBounds` |
| `clips to the region in clip mode` | `overflow: 'clip'`，bounds 等于区域 |
| `counts overwritten pending changes` | 传入 `pendingCellKeys` → `overwrittenPending` 正确 |
| `rejects the whole batch when over the cell limit` | `writes: []` + `grid.paste.tooLarge`（无部分写入） |
| `aligns by column name when the source table is known` | `source.columnNames = ["name","email"]` + `columnOrder = ["email","name","id"]` ⇒ `alignment === "byName"`，`email` 的值写进 `email` 列（**不是** `name` 列）；`droppedColumns === []`；`requiresConfirmation === false` |
| `drops source columns that have no same-named target column` | 来源多出的 `created_at` ⇒ `droppedColumns === ["created_at"]` + problem `grid.paste.columnMismatch`；断言该列**没有任何格出现在 `writes` 里**（不能只断言 `droppedColumns`，那证明不了真没写） |
| `falls back to position and demands confirmation when no name matches` | 来源列名与目标零交集 ⇒ `alignment === "byPosition"`、`crossTable === true`、`requiresConfirmation === true`，且 `writes` 的列序是位置序 |
| `requires confirmation when the source is unknown` | 不传 `source` ⇒ `crossTable === null`（**不是 `false`**）、`requiresConfirmation === true`、`alignment === "byPosition"` |
| `does not require confirmation for the same table` | `source.tableKey === tableChangeKey(context)` ⇒ `requiresConfirmation === false`。**这是唯一的免确认路径**，其余全部必须为 `true` |
| `tableChangeKey is stable across reconnect and schema case` | `{connectionId:"c1", database:"db", schema:"Public", table:"T"}` 与 `{connectionId:"c1", database:"db", schema:"public", table:"t"}` ⇒ **key 相同**；`database: null` 与 `database: ""` ⇒ key 相同；换 `dbSessionId` ⇒ key **不变**；换 `connectionId` ⇒ key **变** |
| `normalizes reverse selections` | 反向区域（`focus` 在 `anchor` 左上）→ 走的仍是 `topRow`/`leftColumn` |

### 9.2 纯逻辑单测 · 其余

- `src/lib/__tests__/gridErrors.test.ts`：逐码前缀匹配；`GRID_ERROR_CODES` 无重复；未知 → `'unknown'`；空串 → `'unknown'`。
- `src/stores/tableData/__tests__/stageCellWrite.test.ts`：
  - `matches legacy single-cell staging exactly`（A2 等价性；含「写回原值 → 改动被删除」「无主键列」「身份歧义」「主键改动导致身份冲突」四条既有分支）
  - `writes 5000 cells with a single state patch`
  - `keeps good cells when one cell is ambiguous`
  - `ignores unset writes and reports nothing`
  - `returns no new objects for empty input`
  - `clears previewPlan and resets pendingStatus`

### 9.3 组件测试

- `src/components/DataTable/__tests__/DataTable.clipboard.test.tsx`：
  - `does not bind clipboard handlers without the clipboard prop`（缺省 = 今天）
  - `copy event writes selection TSV to clipboardData`（构造 `ClipboardEvent`，断言 `getData('text/plain')`）
  - `row mode copies whole rows unchanged`
  - `paste event is consumed and forwarded with the anchor`
  - `paste inside the editor is not intercepted`（`editingCell` 非空 → 无 `preventDefault`）
  - `clicking a cell exposes data-dt-selected`（依赖 DB-01，若未就绪则 `skip` 并注明）
  - `copy never reads the DOM`（§3.8 R-A/R-B）：`vi.spyOn(window, 'getSelection')` 后派发 `copy`，断言**未被调用**，且剪贴板文本等于 `TableState.rows` 里的完整值；同时把 `CellRenderer` 渲染出的截断文本作为反例断言二者**不相等**（证明用例真的能区分两条路径）
  - `copy of a cell with a 500-char value yields the full value`
- `src/hooks/__tests__/useClipboardCopyFeedback.test.tsx`：复用 `src/test/copyFeedbackHarness.ts` 的三条保证 + invoke 降级路径。
- `src/hooks/__tests__/useGridClipboard.test.tsx`：分批调用次数（`GRID_PASTE_CHUNK_CELLS + 1` 格 → 2 次）、确认框阈值（`GRID_PASTE_CONFIRM_CELLS + 1` 格 → 调 `confirmPaste`）、只读拦截、`readClipboard` 抛错 → `grid.clipboard.readFailed`。
- **必须同步修改的既有测试**：`src/lib/__tests__/dataTableContextMenu.test.ts`、`src/components/DataTable/DataTable.test.tsx`、`src/components/DataTable/__tests__/VirtualBody.test.tsx`（见第 6 节末表格）。

### 9.4 连续旅程测试（journey）

`src/components/DataTable/__tests__/clipboard.journey.test.tsx` —— **禁止只断言静态终态**，必须逐击键断言状态跃迁，并覆盖残缺中间态。

| 用例名 | 旅程步骤（每一步都断言） |
| --- | --- |
| `journey: external 3x4 block reaches pending changes and commits` | ① 渲染真实 store + `TableView` 级宿主（含 `ColumnSchema`）→ ② 单击 `(0, 'name')` → `gridSelection.mode === 'cell'` → ③ 派发 `paste`（Excel 形态的 3×4 文本，含 `"a,b"` 与一格引号内换行）→ ④ 断言待提交计数 = 12、`data-dt-dirty` 出现在 12 格 → ⑤ 再派发一次 2×2 粘贴到同一落点 → 断言覆盖 4 格、计数仍为 12、提示回显 `overwrotePending` → ⑥ 派发 `copy` → 断言剪贴板文本与源区域逐字符相等 |
| `journey: paste into a locked table is refused but copy still works` | ① 只读连接 → ② `paste` → 断言 `preventDefault` 被调用、`pendingChanges` 仍为空、只读提示出现 → ③ `copy` → 断言剪贴板有内容（只读不阻止复制） |
| `journey: a ragged block leaves the grid untouched` | ① 有 12 格待提交的基础状态 → ② `paste` ragged 文本 → ③ 断言 `pendingChanges` 计数**未变**、错误提示含行号、`data-dt-cell-error` 未出现 |
| `journey: escaping enters and leaves nil semantics` | ① 先粘 `'""\tNULL'`（显式空串 + NULL）→ 断言两格分别是 `''` 与 `null` → ② 撤销该行改动（`rollbackPendingChanges`）→ ③ 再粘 `'\t'`（两个未引号空字段）→ 断言两格都是 `null`（可空列）→ ④ 复制回来 → 断言文本为 `'\t'`（两者都渲染为空，符合缺省 `nullLiteral: ''`） |
| `journey: editing swallows paste, then grid resumes` | ① 双击进编辑（`editingCell` 非空）→ ② 派发 `paste` → 断言 `preventDefault` **未**被调用、`editingCell` 仍非空 → ③ `Escape` 取消编辑 → ④ 再派发 `paste` → 断言这次被消费且写入成功 |
| `journey: page change drops the paste landing point` | ① 建立区域落点 → ② `setPage(1)` → ③ 断言 `gridSelection.mode === 'none'` **且** `selectedRows.size === 0`、`lastSelectedIndex === null`（清空保证来自 DB-01，见 B21 / `00` §4.3 推论；本册只是它的守卫）→ ④ 派发 `paste` → 断言未消费、零写入 |
| `journey: IME composition does not move the landing point` | ① 落点在 `(0,'a')` → ② 派发 `compositionstart` 后连发方向键 → 断言落点未移动 → ③ `compositionend` 后方向键 → 断言落点移动并按新落点粘贴 |

### 9.5 E2E · `e2e/specs/table-clipboard.ts`

用例名（`TC-CLIP-001`…），每条的断言要点：

| 用例 | 断言要点 |
| --- | --- |
| `TC-CLIP-001 copy selected cells via context menu` | 双击定位（沿用既有 helpers 的 `doubleClickCellByText`）→ 派发 `copy` → 断言结果提示出现、`copied` 反馈类名出现 |
| `TC-CLIP-002 paste a 2x2 block into the grid` | 通过 store 探针断言 `pendingChanges` 的格数与值；页面出现待提交计数 |
| `TC-CLIP-003 paste refuses a read-only connection` | `setSafeMode` / 只读连接下粘贴 → 提示出现、`pendingChanges` 大小不变 |
| `TC-CLIP-004 paste a ragged block reports the row` | 错误提示含行号，`pendingChanges` 不变 |
| `TC-CLIP-005 copy result round-trips through the store probe` | 区域复制后从探针读出该区域的值，与序列化文本比对 |

**E2E 的已知限制（必须写进 spec 头注释）**：`e2e/specs/table-edit.ts` 的既有注释已记录 WKWebView 的 WebDriver **不能可靠驱动键盘输入**，因此：键盘路径用 `dispatchEvent`；`ClipboardEvent` 的 `clipboardData` 构造在 WebKit 上不保证可用——若不可用，降级为「通过 `import.meta.env.DEV` 的探针注入文本」（与既有 `window.__tableDataStore` 探针同一范式），并在 U-6 记录该裁定。

---

## 10. 自查清单（实习生最容易做错的 10 条）

| # | 错误做法 | 正确做法 | 后果 |
| --- | --- | --- | --- |
| 1 | 用组件层的 `rows: unknown[][]` 按**列下标**取列值来拼剪贴板文本 | 用 `TableState.rows`（按列名的记录）+ `columnOrder`（列名），下标只在**最后**与 `ColumnSchema` 对齐 | 列显隐/拖拽后静默错列；复制出的数据看起来对、实际错位（G6） |
| 2 | 粘贴时把单元格文本拼进 SQL 并执行（哪怕只是「顺手」） | 只产出 `CellWrite` → `stageCellWrites` → `pendingChanges`；SQL 由驱动渲染 | 绕过指纹/预览/`affected == 1` 校验，等于在网格里开了一个任意 SQL 执行通道（B19） |
| 3 | 把「不写」实现成「写 NULL」（或反之） | 严格用 `CellWrite` 三态；矩阵未覆盖到的格 = `unset` | 区域粘贴会把用户没选中的格**清空**（B16）；迁移到 INSERT 后自增列失效（`00` §3.1） |
| 4 | 用 `cell == null ? '' : String(cell)` 序列化（照抄 `serializeDataTableRowsAsTsv`） | 复用 `serializeCells`：引号转义 + `JSON.stringify` 对象 + `nullLiteral` 可配 | NULL 与空串不可分、含制表符的值把一列变成两列（G2/G3） |
| 5 | 把 `mod+c` / `mod+v` 注册进 `useKeyboardShortcuts` | `⌘C`/`⌘V` 走容器原生 `onCopy`/`onPaste`；只有打开菜单的 `mod+shift+c` 进快捷键表 | `preventDefault()` 会吞掉原生 `copy`/`paste` 事件，粘贴**完全失效**（4.2） |
| 6 | 在 `onPaste` 里不判断 `editingCell` | 首行 `if (editingCell) return;` 且不 `preventDefault` | 编辑器里粘贴不了、或粘贴把编辑器内容顶掉（B22） |
| 7 | 循环调 `store.stageCellChange` 做批量粘贴（照抄 `applyColumnToRows`） | 调一次 `stageCellWrites`，内部分 5 000 格批，`editBuffer` 只重算一次 | 10 万格 = 10 万次状态写入 + 10 万次 O(rows×pending) 重算，界面卡死（G9） |
| 8 | 逐格错误写 `ts.error`（照抄 `stageCellChange`） | 批量路径只返回 `problems`，由调用方聚合成一条提示 + `data-dt-cell-error` | 后一格的错误覆盖前一格，用户只看到最后一次失败（B6/B26） |
| 9 | 粘贴时按数据库名判断「这列是不是自增/要不要引号」 | 只用 `ColumnSchema` 的 `isAutoIncrement` / `nullable` / `isPrimaryKey`；缺字段就去补 DTO，不在宿主里写驱动名分支 | 违反零硬编码铁律（`00` §1.8、AGENTS.md）；新驱动上线即错 |
| 10 | 复制失败时仍然显示「已复制」 | 用 `useClipboardCopyFeedback`：`copyToClipboard` 返回 `false` 则保持未复制态并提示 | WKWebView 上「点了没事发生」却显示成功，用户以为数据已在剪贴板（G5） |

---

## 11. 未决问题（需人工裁定）

| # | 问题 | 建议 |
| --- | --- | --- |
| **U-1** | G7：`patchPanelForReload` 只并入 `updater(ts)` + `requestRevision`，**不清任何选择状态**，而 `setPage` / `setPageSize` / `setSort` / `setFilters` / `applyFilters` 全走它 → 翻页后 `selectedRows` 指向另一批行 | **已裁定，无需再议**：`00` §4.3 推论确认这是既有缺陷，**已由 DB-01（分册 01）认领**——在 `patchPanelForReload` / `commitFetchedPage` / `invalidateCachedData` 三个收敛点统一清空 `gridSelection` + `selectedRows` + `lastSelectedIndex`，并由 01 写三条用例（`setPage` / `setSort` / `applyFilters`）。本册**不再自行修补**（B21），只保留一条守卫测试（9.4 的 `journey: page change drops the paste landing point`）。两册结论一致，以契约册为准 |
| **U-2** | `overflow` 缺省 `'expand'`（PRD 明文的「超出区域自动扩展」）意味着粘贴会写入用户**未选中**的格。这既是 PRD 要求，也是误写风险点 | **建议保持 `'expand'` 为缺省**（与 PRD 一致、与 Excel/Sheets 习惯一致），但加两条护栏：① 目标越出当前页即截断，绝不自动翻页；② 结果提示必须回显实际覆盖范围。**不建议**把 `'clip'` 做成缺省——它会与 Excel 的直觉相反。是否把该策略暴露为设置项（`settings.grid.pasteOverflow`）留给 UX 裁定：建议**先不暴露**，只在 `useGridClipboard` 留参数 |
| **U-3** | 自增/生成列在粘贴时的处理：本册**规划阶段就整列跳过**（保守），而 `00` §3.6 允许「显式改则交数据库报错」；另外 `ColumnSchema` **没有 `isGenerated`**，生成列无法与「有默认值的普通列」区分 | **建议**：① 粘贴路径保持「自增列整列跳过 + 提示」，因为粘贴不是显式逐列操作；② 把 `isGenerated` 作为 `ColumnSchema` 的**新增可选字段**（默认 `false`）提交给 `driver-api` 的所有者，与 DB-04 一并落地。在 `isGenerated` 到位前，**不要**用 `defaultValue != null` 代替——那会把「有默认值的可写列」误判为只读 |
| **U-4** | 仓库里 TSV 转义有三份实现（`serializeDataTableRowsAsTsv` / `exportData.ts` 的 `escapeTSV` / `exportStream.ts` 的 `escapeTSV`），本册只收口第一份 | **建议**：本册只收口剪贴板路径（`serializeCells`），导出链路留给 DB-10 一次性收敛，理由是导出有流式与内存约束、改动面与验证面都更大。接受「合并后仍有两套」的过渡状态，并在 DB-10 的分册里列为必须项 |
| **U-5** | 为让网格容器能收到原生 `paste`，本册给滚动容器加 `tabIndex={0}` 并**移除** `VirtualBody` 行 `div` 的 `tabIndex={0}` | **建议照做**。理由：行级 `tabIndex` 会让 `Tab` 逐行跳（当前每行都是 tab stop），这本身就是可访问性缺陷；焦点应收敛到网格容器 + roving tabindex。但该改动会影响既有可访问性断言与 E2E 的 Tab 行为，需 DB-02（键盘导航）会签；若 DB-02 已定义 roving tabindex，则本册直接复用它，不再自行加 `tabIndex` |
| **U-6** | E2E 如何构造带 `clipboardData` 的 `paste` / `copy` 事件：WKWebView 上 `new ClipboardEvent('paste', { clipboardData })` 是否可用未经实测；且既有注释（`e2e/specs/table-edit.ts`）已说明键盘输入不可靠 | **建议**：先在 `e2e/specs/table-clipboard.ts` 里实测 `dispatchEvent` 路径；若不可用，降级为 `import.meta.env.DEV` 的 `window.__gridClipboard` 探针（注入文本、读取上次写入文本），与既有 `window.__tableDataStore` 同一范式。**不接受**为了 E2E 把剪贴板逻辑从原生事件改回 `useKeyboardShortcuts`——那是拿产品缺陷换测试便利 |
| ~~U-7~~ | 本册新增 `data-dt-cell-error` DOM 属性，超出 `00` §4.4 的属性表 | **已裁定并关闭：已并入契约 §4.4 全量属性总表**（创建归属 **03**）。无需在本册合并时回写契约——总表已预先收录，不必依赖「合并时同步更新 `00`」这种时序动作 |
| **U-8** | `⌘+⇧+C` 的语义：PRD 沿用 TablePlus 的「复制选中单元格」，但本册让 `⌘+C` 在 `cell` 模式下已经就是区域复制，于是 `⌘+⇧+C` 被指派给「打开 Copy Selected Cells As… 菜单」 | **建议**维持本册方案（一个组合键一个语义，且分属两条不同实现路径：`⌘C` 走原生事件、`⌘⇧C` 走快捷键表）。若将来 UX 坚持「`⌘+⇧+C` 直接复制 TSV」，则它必须与 `⌘+C` 在 `cell` 模式下产出**完全相同**的文本，否则同一区域两种复制结果会成为新的缺陷源 |
| **U-10** | **跨表粘贴是否应该在确认后按名对齐，还是干脆一律按位置对齐**（3.2b 已给出建议方案：优先按名，退回位置 + 确认） | **建议按 3.2b 实现**。人工需要确认的是产品口径：**从 A 表复制、粘到 B 表**是正当用法，所以不能禁；但「同名才写、不同名就丢」对**跨表结构相似**的数据迁移是有损的（`user_name` vs `nickname` 这类列名差异会让整批列被丢弃）。若产品更看重「一次粘过去再手工修」，可以保留纯位置对齐 + 只加确认门（把 `alignment` 恒定为 `byPosition`、`droppedColumns` 恒为空），代价是同表内列被隐藏/拖拽后的粘贴仍然按位置错列。**这是一个产品取舍，不是技术问题，需要产品给一句话** |
| **U-9** | 「粘贴前确认」的阈值（2 000 格）与硬上限（100 000 格 / 10 MiB）是引用 PRD 的定性要求后由本册定的数 | **建议**先按本册常量实现（集中定义在 `src/lib/gridClipboard.ts`，便于调参），并在 PRD §10 的 `grid.clipboard.paste` 埋点观察实际分布后再调整。**不要**把阈值散落在 hook 里 |
| **U-9** | 「粘贴前确认」的阈值（2 000 格）与硬上限（100 000 格 / 10 MiB）是引用 PRD 的定性要求后由本册定的数 | **建议**先按本册常量实现（集中定义在 `src/lib/gridClipboard.ts`，便于调参），并在 PRD §10 的 `grid.clipboard.paste` 埋点观察实际分布后再调整。**不要**把阈值散落在 hook 里 |
| **U-10** | `src/hooks/useClipboardCopyFeedback.ts` 与 `packages/ui` 的 `useCopyFeedback` 是两份语义相同、底层不同的实现（前者带降级链，后者不带宽） | **建议**短期接受两份（`packages/ui` 不允许 import `@tauri-apps/api`，这是硬约束），但在 `useClipboardCopyFeedback` 的注释里交叉引用两处，并把「两者的语义一致性」写成一个共享测试形状（第 9.3 节复用同一个 harness）。中期可考虑让宿主注入「写剪贴板」函数给 `@datazen/ui`，从而只留一份实现——那是独立重构，不在本册 |
| **U-11** | `CellRenderer` 的 120 字硬截断是**显示层**决策，本册只在 §3.8 裁定「剪贴板以数据源为准、不受截断影响」（R-A~R-F），**没有**改显示 | **建议由显示 / 特殊类型分册（DB-13 一系）裁定**是否把硬截断改为「完整文本 + CSS 省略（`text-overflow: ellipsis` / `truncate`）」，使屏幕选中、辅助技术与 `window.getSelection()` 都能拿到完整值。本册不越界：即使显示层改成不截断，剪贴板的取值口径（R-A：读 `TableState.rows`）也不变；反之显示层不改，本册的 G11 也已通过 R-B 移除 DOM 回退而修复 |
