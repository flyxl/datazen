# 数据表网格（DataTable）实现事实

> 本文只写**当前代码已实现**的行为：每个结论都能在下列文件里指认到具体符号。组件清单与视觉规范见 [components.md](components.md)，Store 分层见 [state.md](state.md)，结果网格的服务端写回见 [result-write.md](../backend/result-write.md)，面向用户的操作说明见 [data-browsing-guide.zh-CN.md](../../features/data-browsing-guide.zh-CN.md)。
>
> 维护规则：改网格行为时同步改本文；本文与代码不一致时以代码为准，并把差异改回来。

## 1. 文件地图

表数据网格由两部分组成：**共享组件**（`src/components/DataTable/`）与**连接窗口里的宿主面板**（`src/windows/connection/`）。组件不知道面板、Store 和 IPC 的存在，行为都在 hook 与 `src/lib/` 的纯函数里。

| 关注点 | 文件 | 关键符号 |
|---|---|---|
| 容器：布局、工具条、状态栏、键盘/选择接线 | `src/components/DataTable/DataTable.tsx` | `DataTable`、`surfaceProps`、`viewportRowCount` |
| 虚拟行、行号槽、草稿行 | `src/components/DataTable/VirtualBody.tsx` | `VirtualBody`、行容器 `tabIndex`、`data-testid="draft-add-row"` |
| 单元格 DOM 契约（唯一拥有者） | `src/components/DataTable/GridCell.tsx` | `GridCell`、`data-dt-*` 渲染 |
| 值显示与编辑器挂载 | `src/components/DataTable/CellRenderer.tsx` | `CellRenderer`、`resolveCellEditorKind` |
| 旧编辑入口（草稿行仍在用） | `src/components/DataTable/EditableCell.tsx` | `EditableCell`（`CellEditorHost` 的兼容壳） |
| 编辑器族 | `src/components/DataTable/editors/` | `CellEditorHost`、`activeEditor`、`setValueMenu`、`toEditString`、`Text/Number/Boolean/Enum/Temporal/Json/LongText/Binary` 八个 `*CellEditor` |
| 表头与排序 | `src/components/DataTable/TableHeader.tsx` | `TableHeader`、双击循环 |
| 页脚 | `src/components/DataTable/Pagination.tsx` | `Pagination`、`paginationReducer`、`PagingHint` |
| 默认序选择器 | `src/components/DataTable/DefaultOrderSelect.tsx` | `DefaultOrderSelect` |
| 右键菜单 | `src/components/DataTable/useDataTableContextMenu.ts` | 菜单构建、`buildDataTableContextMenuItems` 的标签/处理器接线 |
| 剪贴板解析/序列化（纯逻辑） | `src/lib/gridClipboard.ts` | `parseClipboardMatrix`、`serializeCells`、`buildSelectionMatrix` |
| 粘贴规划（纯逻辑） | `src/lib/gridPastePlan.ts` | `planPaste`、`tableChangeKey`、`recordClipboardSource` |
| 剪贴板编排 | `src/hooks/useGridClipboard.ts` | `useGridClipboard`、`describePasteProblem`、分批与确认 |
| 剪贴板 DOM 接线 | `src/components/DataTable/useDataTableClipboardDom.ts`、`gridClipboardBridge.ts` | `onCopy` / `onPaste` 判定、`GridClipboardHandlers` |
| 面板接线 | `src/windows/connection/useTableViewClipboard.ts`、`TableClipboardStatus.tsx` | 读取面板切片、粘贴结果行 |
| 批量暂存 | `src/stores/tableData/stagePasteWrites.ts` | `stagePasteWritesInto`（一次 `set()`） |
| 行高 | `src/components/DataTable/rowHeight.ts` | `DATA_TABLE_ROW_HEIGHT = 28` |
| 选择状态（纯逻辑） | `src/stores/tableData/gridSelection.ts` | `CellCoord`、`GridSelection`、`clampCellCoord`、`moveCellFocus` |
| 选择动作（写 Store） | `src/stores/tableData/selectionActions.ts` | `applyRowSelect`、`applyToggleSelectAll`、`ROW_MODE_GRID_SELECTION` |
| 指针/键盘接线 | `src/hooks/useGridSelection.ts` | `useGridSelection`、`surfaceProps.onKeyDown` |
| 键盘命令执行 | `src/hooks/useGridKeyboardNav.ts` | `useGridKeyboardNav`、`applyCommand` |
| 键盘意图解析（纯逻辑） | `src/lib/gridKeyboard.ts` | `readGridKeyIntent`、`GridKeyCommand` |
| 编辑准入与提交 | `src/windows/connection/tableEditing.ts` | `resolvePanelCellEditable`、`requestEdit`、`guardNavigation` |
| 面板本体 | `src/windows/connection/TableView.tsx` | `TableView`、`pagingHint`、`handleCommitPendingChanges` |
| 面板动作（都先过导航守卫） | `src/windows/connection/useTableActions.ts` | `useTableActions`、`countExact`、`setSort`、`reload` |
| 草稿行 | `src/stores/tableData/draftInserts.ts` | `withDraftValue`、`draftsForWire` |
| 脏标记索引 | `src/stores/tableData/dirtyIndex.ts` | `buildDirtyIndex`、`byRowIndex`、`offPageKeys` |
| 分页/行数 | `src/stores/tableDataStore.ts`、`src/lib/keysetPaging.ts`、`src/lib/totalRowsDisplay.ts` | `setPage`、`nextRequestCursor`、`canPageForward` |
| 外键跳转 | `src/lib/fkNavigation.ts`、`src/lib/fkNavigationActions.ts`、`src/hooks/useTableForeignKeys.ts` | 目标解析、跳转 9 步、`FK_NAV_MAX_DEPTH` |
| 结果网格编辑判定 | `src/lib/resultEditability.ts`、`src/stores/panelResultEdit.ts` | `resolveResultWriteBack`、`RESULT_ORIGIN_WRITE_BACK_ENABLED` |
| 错误码注册表 | `src/lib/gridErrors.ts` | `GRID_ERROR_CODES`、`classifyGridError` |

## 2. 组件结构与虚拟化

### 2.1 虚拟滚动

`useVirtualTable`（`src/hooks/useVirtualTable.ts`）包 `@tanstack/react-virtual` 的 `useVirtualizer`：行数 = 当前页行数，尺寸恒为 `rowHeight`（表数据网格固定 `DATA_TABLE_ROW_HEIGHT = 28`），`overscan` 默认 10。`VirtualBody` 用绝对定位渲染可见行，并保留一个总高度的占位块。

由此产生三条必须知道的事实：

1. **DOM 里只有可见行。** 任何「遍历所有行」的逻辑（选中、脏标记、外键、复制）都必须基于 Store 里的数据或行索引，不能靠 `querySelectorAll`。
2. **一页的行在滚动后才存在。** 滚动到窗口外再回来，行容器是新节点；选择、焦点、编辑都是按 `(rowIndex, columnName)` 重建的，不靠节点身份。
3. **行高是常量而不是测量值。** 外键跳转把行索引翻译成滚动偏移、把偏移翻译回行，都依赖这个常量（`rowHeight.ts` 的注释即为此）。改成动态行高会同时破坏外键视口定位与 `viewportRowCount` 的整屏步进。

### 2.2 `data-dt-*` 属性族

属性是网格对外**唯一稳定**的契约：选择、脏标记、编辑器、外键、E2E 都只读属性，不依赖 class 名或节点层级。全部为「有值才出现」——布尔态写 `"true"`，假值**不写属性**（从不写 `"false"`）。

| 属性 | 挂在哪 | 何时出现 | 值 |
|---|---|---|---|
| `data-dt-surface` | 滚动容器（`role="grid"`） | 只有开启单元格选择时才渲染 | `"true"` |
| `data-dt-row` | 单元格 | 恒有 | 行索引（UI 索引，**永不用于拼 SQL**） |
| `data-dt-col` | 单元格 | 恒有 | **列名**（不是列索引） |
| `data-dt-selected` | 单元格、行容器 | 该格/该行在选中区域内 | `"true"` |
| `data-dt-active` | 单元格 | 该格是选择焦点（锚点/焦点所在格） | `"true"` |
| `data-dt-cell-type` | 单元格 | 列类型已归一到 `CellType` | `text`/`enum`/`number`/`bool`/`date`/`time`/`datetime`/`json`/`bytes`/`uuid`/`unknown` |
| `data-dt-editing` | 单元格 | 该格编辑器打开 | `"true"` |
| `data-dt-null` | 单元格 | 值为 `null` 或 `undefined` | `"true"` |
| `data-dt-readonly` | 单元格 | 该格不可编辑 | 只读原因码（见 §6.1、§11） |
| `data-dt-dirty` | 单元格 | 该格有暂存改动 | `"true"` |
| `data-dt-fk` | 单元格 | 该列属于某个外键组（**无论该组当前是否可跳**） | 目标标签：`customers.id` / `sales.parents.(x, y)` |
| `data-dt-fk-click` | 单元格内的箭头 `<button>` | 该格可跳转且未在编辑（解析中时按钮 `disabled`，但属性仍在） | `"true"` |
| `data-dt-row-view` | 行容器 | 恒有 | 行索引 |
| `data-dt-row-state` | 行容器 | 行有暂存状态 | `dirty`/`deleted`/`inserted` |
| `data-dt-editor-owned` | 编辑器根 | 编辑器打开 | `"true"` |
| `data-dt-editor-kind` | 编辑器根 | 编辑器打开 | 编辑器种类（见 §6.2） |
| `data-dt-editor-error` | 编辑器根 | 当前输入无法解析 | 错误原因码 |
| `data-dt-selection-rows` / `-cols` / `-cells` | 状态栏 | 有单元格选择 | 选择覆盖的行数 / 显示列数 / 单元格数 |

两条容易踩的细节：

- `data-dt-selected` 同时存在于**行容器与单元格**上，所以定位行必须写 `[data-dt-row-view][data-dt-selected]`，只写 `[data-dt-selected]` 会命中单元格。
- `data-dt-readonly` 的值是**原因码**而不是布尔量；要判断「能不能编辑」用属性有无，要区分「为什么不能」读它的值。
- E2E 定位行只能写 `[data-dt-row-view]`：`data-dt-row` 是单元格属性，`[tabindex="0"]` 只匹配滚动容器（行容器在单元格选择开启时是 `tabIndex={-1}`），这两种写法都定位不到行。

### 2.3 ARIA 与焦点宿主

`role="grid"` 的容器在**开启单元格选择**时才存在。它同时承担四件事：ARIA 网格宿主、焦点宿主、`data-dt-surface` 标记、键盘事件的唯一接收者。

- `aria-rowcount` = 数据行数 + 表头 1 行 + 占位行（加载中或空页时为 1）；`aria-colcount` = 显示列数 + 行号槽 1 列。
- `role="row"` / `role="columnheader"` / `role="rowheader"` / `role="gridcell"` 逐层对齐（`gridSemantics` 开关）。
- `aria-activedescendant` 指向 `active` 格的 `id`，格式 `${idPrefix}-r{rowIndex}c{colIndex}`，由 `useGridSelection` 生成。列索引来自**当前显示顺序**，隐藏列会让同一格的 id 变化。
- 滚动容器 `tabIndex={0}`，是键盘焦点的唯一落点。

`useGridKeyboardNav` 的提示文本（`GridKeyboardHints`）是 `sr-only` 的 `aria-describedby` 目标，按 `useI18n` 的领域包取词。

## 3. 选择模型

### 3.1 三态

`GridSelection`（`gridSelection.ts`）是 `{ anchor, focus, extraRanges, mode }`，`mode` 取值 `'none' | 'cell' | 'row'`：

| mode | 几何字段 | 行高亮的来源 |
|---|---|---|
| `none` | `anchor`/`focus` 为 `null`，`extraRanges` 为空 | 无 |
| `cell` | `anchor`、`focus` 必填，`extraRanges` 可多段 | 由矩形覆盖推出（`rowsCoveredBySelection`） |
| `row` | `anchor`/`focus` 为 `null`，`extraRanges` 为空 | `selectedRows`（行模式**不携带几何**） |

`row` 模式刻意丢弃 `anchor`/`focus`（`selectionFromAnchorFocus` 对 `row` 直接返回空几何），避免出现「行高亮有两个真源」。`EMPTY_GRID_SELECTION` 是冻结的空选择常量，`DataTable` 在没有 Store 选择时用它兜底（`gridSelection ?? EMPTY_GRID_SELECTION`），所以空态是共享引用而不是每次新建对象。

### 3.2 坐标与钳制

坐标是 `CellCoord { rowIndex, columnName }`：**列用名字，行用 UI 索引**。列名在隐藏/重排列后仍是同一列；行索引只在当前页内有效，任何写库路径都必须经 `rowIdentityAnchors` 换成行标识。

两个纯函数定义「坐标还存在吗」：

- `clampCellCoord(coord, rowCount, columnOrder)`：空网格（`rowCount <= 0`）、非整数行索引、列名不在当前显示序里 → 返回 `null`；行索引超界 → 夹到 `[0, rowCount-1]`。列名**从不替换**（换一列就是换一个选择，不是钳制）。
- `moveCellFocus(focus, deltaRow, deltaColumn, rowCount, columnOrder)`：逐轴夹取；夹完没有位移（已在边界）→ 返回 `null`，由调用方归一为「这个键被处理了但没有动作」（见 §4.3）。列在显示序里移动，所以「右移一格」在隐藏列之后跳过的是隐藏列。

`null` 的含义统一是**「这里没有合法坐标」**，不是错误：空网格、列被隐藏、边界处无位移，全部走 `null` 分支。

`reconcileGridSelection`（纯函数，`gridSelection.ts`）与 Store 动作 `reconcileGridSelectionForColumns`（`selectionActions.ts`）在行数变小、列被隐藏、数据重载之后收敛选择：主矩形任一角不再可显示就整体回到 `none`；附加段逐段检查，不可显示的段被丢弃并标记 `droppedAny`；行模式下几何被清空。`setVisibleColumns` 会调用后者，所以隐藏列不会留下指向隐藏列的坐标。

### 3.3 指针交互

`useGridSelection.onPointerDown` 是唯一入口，规则按顺序：

1. 只处理鼠标主键（`event.button === 0`）；`isEditableTarget(event.target)` 为真（点在编辑器或输入控件里）直接让路。
2. 命中行号槽（`[role="rowheader"]`）→ 交给 `onRowGutterSelect(rowIndex, { multi, range })`，**不走单元格选择**。行号槽里的按钮是真实 `<button>`，但 `tabIndex={-1}`。
3. 落在单元格上：开始拖动会话（`pointer capture`），焦点转到滚动容器。落点是空白 surface → 会话的 `anchor` 为 `null`，`pointerup` 时「点击即清空、拖动忽略」。
4. 松手时按会话状态决定：拖动过 → 从锚点到落点成矩形；`Cmd/Ctrl` 按下 → `toggleExtraRange`（追加/移除单格附加区，落在主区内是 no-op）；`Shift` 按下且当前是 `cell` 模式 → 从既有锚点扩到落点；否则单选该格。
5. `Escape` 清空选择（先结束拖动会话再清）。编辑器打开时 `Escape` 归编辑器，选择不动。
6. `blur` 只结束拖动会话，**不清选择**——用户可能正要去点工具条。

行模式与单元格模式的互斥由 `applyRowSelect`（`selectionActions.ts`）保证：行模式写入 `selectedRows` 并置 `ROW_MODE_GRID_SELECTION`（冻结的空几何 `row` 选择）；再次点击唯一选中的行是**清空**（取消选择），不是「重新选中」。

### 3.4 键盘驱动的选择

键盘产生的选择变化一律经 `useGridKeyboardNav` 的 `applyCommand` 调 `onChange`，与指针共用同一组纯函数（`moveCellFocus`、`selectionFromAnchorFocus`、`select-all-rows`）。`Shift` 扩展 = 保留 `anchor` 只改 `focus`；无 `Shift` 移动 = 锚点与焦点同时移动。

### 3.5 行模式与 `selectedRows`

- `selectedRows: Set<number>` 是**行级**选择的真源；单元格选择的行覆盖只经 `rowsCoveredBySelection` **按需导出**，绝不回写成 `selectedRows`（那是第二条真源，会与 `mode` 打架）。
- `applyToggleSelectAll` 在「已全选」时清空、否则选中整页（当页，不是全表）。
- 行容器的 `Enter`/`Space` 是**行选择**，且只在**单元格选择关闭**时生效；开启时行容器 `tabIndex={-1}` 且不响应这两个键（网格层的 `Enter` 是「编辑当前格」），避免同一个键有两种含义。

### 3.6 选择域没有错误码

选择是钳制语义：越界被夹取、不可用坐标返回 `null`、隐藏列后收敛。没有「选择失败」这种状态，因此 `grid.*` 注册表里**不存在** `grid.selection.*` 一族——新增时不要加，它会成为无人产出的死码（`gridErrors.registration.test.ts` 的「每个登记码都要有产出方」会红）。

## 4. 键盘

### 4.1 唯一入口

键盘只有一条通路：

```
滚动容器 onKeyDown
  → useGridSelection.onKeyDown 先调 onKeyboardNav（= useGridKeyboardNav 的 handleKeyDown）
  → 若已被 preventDefault 则直接返回
  → 否则处理 IME / Escape（清选择）
```

`useGridKeyboardNav` **不注册 window 监听、不写 Store**：它只把命令翻译成「选中哪一格 / 选中哪一行 / 全选 / 开始编辑」的回调，由宿主注入。因此任何「在网格上加个全局快捷键」的实现都必须回到这个单一入口，而不是新加 `addEventListener`——否则焦点在编辑器里也会触发。

面板级快捷键是**另一层**，挂在 `TableView` 最外层 div 的 `onKeyDown`（`handleTableKeyDown`）上，靠事件冒泡在网格之后执行：`Cmd/Ctrl + Enter` 提交暂存、`Cmd/Ctrl + I` 插入草稿行、`Cmd/Ctrl + D` 复制当前草稿行、`Escape` 丢弃空草稿行、`Delete`/`Backspace` 删除当前草稿行、以及外键后退/前进。顺序很重要——网格先决定自己要不要这个键，`event.defaultPrevented` 为真时面板层直接让路。

### 4.2 `readGridKeyIntent` 的判定顺序

`gridKeyboard.ts` 是纯逻辑（不碰 DOM、不碰 React、不碰 Store），按固定优先级返回意图：

1. **IME 组合中**（`isComposingEvent`，含 `keyCode === 229` 与 Chrome 的 `key === 'process'`）→ `pass`。中文输入法选词时的方向键不能拿去移格。
2. **编辑器打开** → `pass`。`Enter`/`Escape`/`Tab` 归编辑器（见 §6.3）。
3. `Escape` → `pass`：键盘层不认领，随后同一事件里的选择层清空选择（编辑器打开时第 2 条已先返回，选择层也不会清）。
4. 带 `Alt` 的组合 → `pass`（外键前进/后退在宿主层处理，见 §10.5）。
5. 其余按表：

| 键 | 动作 |
|---|---|
| 方向键 | 移动焦点一格（`Shift` 扩展） |
| `Cmd/Ctrl + ←/→` | 跳到本行首/尾 |
| `Home` / `End` | 本行首/尾；带 `Cmd/Ctrl` 时是网格首/尾 |
| `PageUp` / `PageDown` | 上下移动 `max(1, viewportRowCount - 1)` 行：一屏减一行，保留一行重叠，翻页后仍能看到原来的上下文 |
| `Tab` | `leave-grid` → `consume`（`preventDefault`），让焦点离开网格 |
| `Enter` / `F2` | 有选中格 → `begin-edit`；无选中 → `ignored`；带 `Cmd/Ctrl` 的 `Enter` → `pass`（执行查询是全局面板快捷键，不是网格的） |
| `Cmd/Ctrl + A` | `select-all-rows`（宿主没提供该路径时降级为 `ignored`） |
| 空格、`Delete`/`Backspace`、可打印字符 | `pass`（不属于网格；行容器的 `Space` 选行见 §3.5） |

### 4.3 `consume` / `ignored` / `pass`

`GridKeyCommand` 的三态是**「谁负责这个键」**的答案，`applyCommand` 严格照此执行：

| 结果 | `preventDefault` | `stopPropagation` | 语义 |
|---|---|---|---|
| `consume` 且无状态变化（如已到边界） | 是 | 否 | 「这个键归网格，但这次没有位移」——保持选择不塌缩 |
| `consume` 且有状态变化 | 是 | 否 | 正常的移动/选择/编辑 |
| `ignored` | 否 | 否 | 「网格不认识」或「认识但无事可做」，浏览器默认行为保留 |
| `pass` | 否 | 否 | 明确让给别的层（IME/编辑器/浏览器/宿主快捷键） |

`ignored` 与 `consume` 的区别必须保留：把边界情况改成 `ignored` 会让浏览器把方向键当滚动，视口乱跳；把 `Enter` 的「无选中」改成 `consume` 会吞掉本该上浮的默认行为。

### 4.4 roving tabindex 的后果

- **开启单元格选择**：滚动容器 `tabIndex={0}`，行容器 `tabIndex={-1}`，行号槽按钮恒 `tabIndex={-1}`。结果是整张网格在 Tab 序里只占**一格**：`Tab` 进网格，`Tab` 再离开网格（`consume` 且 `preventDefault`），中间不会穿行到行号槽或每个单元格。
- **关闭单元格选择**（老路径）：行容器 `tabIndex={0}`，行仍可 Tab 到，行级 Enter/空格选中该行。
- 行号槽按钮永远是 `-1`：它只能点，不能 Tab 到，也不参与 roving。

这条约束是「键盘焦点只在容器上」的物理原因，改 `tabIndex` 会同时改变 `DataTable`、`VirtualBody` 与 E2E 的定位契约。

### 4.5 与文本输入的关系

`isEditableTarget`（`src/lib/gridEventGuards.ts`）把编辑器与输入控件（含 `[data-dt-editing="true"]`）判为「自己管键盘」。所有网格快捷键、外键快捷键、面板级快捷键都先过这一道；IME 判定再过一道。两层都过不去才轮到网格。

## 5. 复制与粘贴（剪贴板）

复制与粘贴只有**一套**序列化与**一条**写回路径，这是本章存在的原因：过去的实现里有三份互不一致的转义（菜单私有 `escapeCsvCell`、导出的 `escapeCSV`、TSV 完全不转义），值里一旦出现制表符、换行或引号，粘贴回来的行列数就变了。

### 5.1 一份转义规则

`serializeCells`（`src/lib/gridClipboard.ts`）是唯一的单元格序列化入口；下面这些函数都只是它的薄封装，签名不变：

| 调用点 | 参数 | 结果 |
|---|---|---|
| `serializeDataTableRowsAsTsv` / 复制的行 | `tsv`，无表头 | 制表符分列、`\n` 分行 |
| `serializeDataTableRowsAsCsv` | `csv`，默认带表头 | 逗号分列 |
| `serializeDataTableRowsAsJson` | `json`，`jsonObjects` | 对象数组 |
| `serializeDataTableColumnValues` | `tsv`，单列 | 换行分隔的值列表 |

转义规则：值内含分隔符、`"`、`\n` 或 `\r` 时整格用 `"` 包裹并把内部 `"` 双写；`null` 默认渲染为空串（可传 `nullLiteral`，粘贴侧认 `NULL` 与 `\N`）；对象值走 `JSON.stringify` 而不是 `String(v)`；长值不截断。

因此「复制整列」这类原本不做转义的路径也会输出 `"{""a"":1}"`。这不改变读回的内容：`parseClipboardMatrix` 与电子表格都会还原成 `{"a":1}`。

### 5.2 写出到系统剪贴板

菜单项与 `⌘C` 都走 `copyToClipboard`（`src/lib/fetchRelationDdl.ts`）的三级降级：`navigator.clipboard.writeText` → `invoke('write_clipboard')` → `document.execCommand('copy')`。WKWebView 在用户手势结束后会拒绝第一级，菜单点击恰好就是这种情形（缺陷 G5），所以只写 `writeText` 的复制会静默失败。空文本按**失败**处理（`copyToClipboard('')` 返回 `false`），不会点亮「已复制」。

复制成功后由 `recordClipboardSource(source, text)` 记下「来源表 + 列名 + **复制出去的那段文本**」。粘贴时只经 `sourceForClipboardText(pasteText)` 取来源：**文本对不上就等于来源未知**，退回「外部粘贴」路径（按位置对齐 + 强制确认）。

这条绑定不是洁癖。来源记录是模块级变量，与系统剪贴板当前内容毫无关系——用户在 Excel 里复制一次，它照样声称「来自本表」。没有文本绑定时，这会让一次外部粘贴**跳过确认**并按**陈旧的列名**写入，即 G12「静默写错列」换个入口再来一次。有了绑定，「来源」与「剪贴板内容」是同一个事实而不是两个：证明不了就不证明，失败方向永远是「问用户」，不是「静默写」。

换行比较前会归一（`\r\n?` → `\n`），因为 Windows 经手剪贴板会把 `\n` 改写成 `\r\n`；逐字节比会让同表粘贴在那个平台上每次都弹确认。

`⌘C` / `⌘V` 绑定在滚动容器上（`useDataTableClipboardDom`），三条守卫：没有 `clipboard` 桥则完全不挂监听（老行为）；编辑器或任何输入控件打开时不拦截；没有落点单元格时不拦截 `paste`。

### 5.3 落进暂存，而不是直接写库

粘贴的写入路径与单元格编辑完全相同：`pendingChanges` → `preview_pending_changes` → `commit_pending_changes`。差别只在批量：

1. `parseClipboardMatrix` 解析文本，畸形输入当场拒绝并给出结构化原因（空文本、超限、行列不齐、引号未闭合）；
2. `planPaste`（`src/lib/gridPastePlan.ts`）决定「写到哪」：同表按列名对齐、跨表或来源未知时按位置对齐并强制确认，不可写列（自增列）整列跳过，`NOT NULL` 列的空值逐格拒绝，越界默认「扩展」到页尾并统计被裁掉的格数；
3. `stagePasteWritesInto` 一次性算完整个块，Store 只 `set()` **一次**（5 000 格也只触发一次通知），每格复用 `computeStageCellValueResult`，与单格编辑逐字段等价；
4. 超过 `GRID_PASTE_CONFIRM_CELLS`（2 000 格）或跨表时先弹确认框。

三态语义因此原样保留：`null` 写 SQL NULL、空串写 `''`、块里没有的格子根本不写。把某格粘回它自己的原值时，暂存项会消失而不是留下一条空变更。

### 5.4 失败怎么显示

粘贴拒绝用 12 个 `grid.paste.*` / `grid.clipboard.*` 码表达（`src/lib/gridErrors.ts`），全部在**前端**产生，因此登记在 `gridErrors.registration.test.ts` 的 `PRODUCER_PENDING` 里。逐格失败用 `data-dt-cell-error` 标记该单元格；整批失败用 `TableClipboardStatus` 一行文字说明，不覆盖用户已有的暂存修改。

### 5.5 已知边界

- `ColumnSchema` 没有 `isGenerated`，所以只有 `isAutoIncrement` 列被当作不可写列跳过；生成列目前会照常写入并由数据库拒绝。
- 复制只取主选区（`selection.anchor → focus`）的矩形；`extraRanges` 不参与复制，菜单标签会以 `(N…)` 提示被忽略的区域。
- 粘贴按位置对齐时以选区**左上角**为起点，越界默认扩展，与电子表格一致。

## 6. 编辑生命周期

### 6.1 准入门

一次编辑的准入由 `requestEdit`（`src/windows/connection/tableEditing.ts`）判定，顺序是「面板级原因 → 列级原因」：

1. 面板级 `resolvePanelReadOnlyReason`：挂载原因（`mountReason`）→ 连接只读 → 驱动不支持写 → 非表对象（视图 `view`）→ 有列但没有主键（`noPrimaryKey`）。
2. 列级 `resolvePanelCellReadOnlyReason`：`autoIncrement`、`generated`、`typeNotEditable`、`binaryColumn`、`largeValue`，以及行标识层面的 `ambiguousRowIdentity`。
3. 结果网格另有一条完全不同的判定链（§11）。

拒绝是**返回值**（`{ ok: false, reason }`），不是抛异常：调用方据此显示原因并保持原状态。原因码最终渲染为 `data-dt-readonly` 的值，并对应 `cellEditor.readOnly.*` 的文案。

### 6.2 编辑器种类

`resolveCellEditorKind`（`src/lib/cellEditorKind.ts`）只按**归一化后的 `CellType`** 选控件，绝不字符串匹配驱动的类型名（那是 `cellTypes.ts` 的职责）：

| kind | 触发条件 |
|---|---|
| `binary` | `bytes` 类型；值超过 `CELL_EDIT_MAX_BYTES`（64 KiB）时同样落到不可编辑路径 |
| `longText` | `text`/`unknown` 且值含换行或超过 `INLINE_TEXT_MAX_CODE_POINTS`（200 个码点） |
| `json` | `json` 类型 |
| `enum` | `enum` 类型；候选值由用户显式请求后按 `SELECT DISTINCT … LIMIT 200` 拉取 |
| `date` | `date`/`time`/`datetime`，且**不带时区**（`TemporalCellEditor`，日期弹层） |
| `boolean` | `bool` |
| `number` | `number`（可分整数/小数校验） |
| `text` | 其余，含 `uuid` 与**带时区**的 `time`/`datetime`（`timetz`/`timestamptz` 的偏移量没有弹层能装，所以退回文本） |

### 6.3 什么时候写入（`CellEditorHost`）

**只有一个地方决定一次编辑何时落地**，即 `editors/CellEditorHost.tsx`。它的规则是三条，缺一不可：

1. `Enter` 提交（IME 组合中的 `Enter` 只提交候选词，不提交编辑）；`Escape` 取消；`Tab` 提交并让焦点按表格方向移动。
2. `blur` 只在焦点**离开编辑器自己拥有的区域**时提交：`[data-dt-editor-owned]`、`[data-testid="select-listbox"]`、`[data-testid="cell-editor-temporal-popover"]` 之间移动不算离开。下拉、日期弹层因此不会顺手把半成品写进去。
3. **组件卸载永不写入**。重读、翻页、切表导致编辑器卸载时，草稿直接丢弃——这就是为什么翻页/排序/刷新前必须先过导航守卫（§6.6）。

`activeEditor` 注册表保存的是**回调**而不是值：导航方只能问「你这份草稿能落地吗」，拿不到也不该改写用户正在敲的文本。

### 6.4 值转换与三态写意图

`coerceValue`（`cellEditorKind.ts`）把输入文本转成 `Value | null | undefined`，三态含义固定：

| 值 | 含义 |
|---|---|
| `undefined` | **不写这一列**（草稿行的「默认值」、`SET` 子句里省略该列） |
| `null` | 显式写 SQL `NULL` |
| 具体值 | 写该值 |

拒绝解析时返回 `CellValueErrorReason`（11 种：`emptyNotNull`、`invalidNumber`、`invalidInteger`、`invalidBoolean`、`invalidDate`、`invalidTime`、`invalidDateTime`、`invalidJson`、`invalidUuid`、`tooLarge`、`notEditable`），由 `cellValueErrorMessageKey` 映射到文案，并以 `data-dt-editor-error` 暴露。空输入落到 `NOT NULL` 列是拒绝（`emptyNotNull`），不是静默写 `NULL`——旧编辑器在这里出过错。

### 6.5 暂存与脏标记

提交进 Store 的改动叫**暂存**（pending change），与「已提交」严格分开：

- `stageCellValue` / `stageCellChange` 写入 `pendingChanges`（按行标识索引）；`applyColumnToRows` 把一列的值批量暂存；`stageRowDelete` 标记删除。
- `dirtyIndex` 是**派生**索引：`dirty`/`deleted`/`inserted` 三态，`byRowIndex` 供渲染，`offPageKeys` 供「其他页面还有 N 个单元格已更改」提示，`counts` 供状态栏。
- 单元格脏标记（`data-dt-dirty`）与行状态（`data-dt-row-state`）是两条通道：被标记删除的行不逐格显示脏，改用整行删除线，避免同一事实画两遍。
- 选择与暂存正交：改选、翻页、折叠侧栏都不丢暂存。**重读也不清**——新读到的行会经 `overlayPendingRows` 把暂存值重新叠上去（`rows: rowsWithPending`），所以刷新后看到的仍是「原值 + 你的改动」。只有提交成功或回滚才清空（提交写回 `pendingChanges: new Map()`）。

### 6.6 提交：预览 → 指纹 → 执行

`TablePendingChangesBar` + `PendingPlanDialog` + `TableDataStore` 构成固定流程：

1. **预览** `previewPendingChanges`：把 `pendingChangesForWire(...)` 与 `draftsForWire(ts)` 一起发给宿主，得到 `RowChangePlan`（语句序列、参数摘要、`warnings`、`fingerprint`）。预览执行的语句顺序是 **新增 → 更新 → 删除**，预览文本**不是**最终执行的语句（宿主会按计划元数据重建绑定语句）。
2. **用户在预览里改主意**：`pendingWorkSignature` 把「有哪些改动」压成一个签名，预览返回时若签名已变，计划**不写入 Store**（`previewPlan` 保持旧值或 `null`）。这是「看到的是 A、提交的是 B」的第一道拦截；真正的拦截在提交时由宿主按 `plan.fingerprint` 复核。
3. **提交** `commitPendingChanges`：带上 `plan.fingerprint` 与手动事务句柄；宿主校验会话与计划后执行。若 `previewPlan` 缺失、或它与当前表上下文不同（`tableChangeContextKey`），前端会**先重建预览**；重建后仍不同就直接拒绝（`Table context changed; preview must be rebuilt before commit.`），不发 IPC。
4. **提交后**：签名未变时清空暂存与草稿、关掉编辑器与新高亮；随后重读当前页，按「插入行定位」规则高亮新行（§7），并给出 `insertLocation` 结论。重读失败时提交并没有失败，但 `refreshRequired` 为 `true`、`refreshStatus` 为 `failed`——不要把「刷新失败」显示成「提交失败」。

冲突与拒绝的错误码（后端产出，前端按前缀分类）：

| 错误码 | 触发 |
|---|---|
| `grid.commit.stalePlan` | 计划的会话/指纹与当前不符（预览后连接配置或数据变了） |
| `grid.commit.conflictingIntents` | 同一行既有删除标记又有列写；新增行带行标识或原始值；同一批里既有新行又被删 |
| `grid.commit.affectedMismatch` | INSERT 或按行标识的写影响了非 1 行（`0` 行也表示目标已被别人改掉） |
| `grid.insert.unknownColumn` | 新增行写了不属于该表的列（只在 INSERT 路径检查） |
| `grid.insert.returningUnavailable` | 驱动没有 `INSERT … RETURNING`，新行主键取不回来（**警告**，不是失败） |

### 6.7 导航守卫

改页码、改页大小、改过滤、改排序、刷新都要先问编辑器：`resolveEditorBeforeNavigation` → `guardNavigation`。提交成功则继续导航；草稿不可接受（解析失败、`NOT NULL` 未填）则**阻断导航**并保持编辑器打开。这是 §6.3 第 3 条（卸载永不写入）的配套：既然卸载会丢草稿，就不要在没有确认的情况下卸载。

## 7. 插入行

草稿行是**第三条数据通道**，既不是 `rows` 也不是 `pendingChanges`：

- `draftInserts.ts` 维护 `draftInserts`（`Map`）+ `draftInsertOrder`（数组）。草稿不写进 `ts.rows`，因此排序、过滤、分页都看不见它；它也从不进 `pendingChanges`，因此脏单元格索引不会把它算成「已改单元格」。
- `withDraftValue` 的 `undefined` 是**删键**（该列走默认值），`null` 是写 `NULL`。这两个意图在 UI 上分别对应「默认值」与「设为 NULL」。
- 顺序即语义：`draftsForWire` 严格按 `draftInsertOrder` 输出，**宿主不排序**。多个草稿行的提交顺序决定自增值归属与引用关系，所以不要在中间插入/重排。
- 空草稿行合法：没有可写列时按 `INSERT … DEFAULT VALUES` 生成。
- 编辑草稿行同样受编辑器规则约束；`Escape` 只丢弃**空**草稿行，填过内容的保留。

提交后的定位（`insertLocation.ts`）：

1. 提交结果若带行标识 → 在刷新后的页里精确匹配，命中即 `located`。
2. 否则退化为「单列可比较主键的最大值且相比提交前增长」启发式；不满足则 `unavailable`。
3. `unavailable` 时前端**明确说**「无法自动定位新行的主键，页面已刷新」（`tableData.insertReturningUnavailable`），不假装高亮；定位成功时高亮 1.5 秒后自动消退。

实现边界要说清：驱动契约里有 `insert_returning_clause`，但**写路径从不调用它**——所以「插入后立刻拿到主键并高亮」目前不存在，别按 RETURNING 已实现的前提设计交互。

## 8. 过滤与排序

### 8.1 过滤

- 条件形状是 `FilterCondition { column, operator, value?, connector? }`。`connector` 是**逐条**的 `and`/`or`（首条自身无意义，后端忽略），缺省回落到面板级 `filterLogic`。
- 操作符共 22 个：前 10 个（`eq`/`ne`/`gt`/`lt`/`gte`/`lte`/`like`/`in`/`isNull`/`isNotNull`）是冻结基线；9 个共享扩展（`notIn`/`between`/`notBetween`/`contains`/`notContains`/`startsWith`/`endsWith`/`isEmpty`/`isNotEmpty`）；3 个方言专属（`regex`/`notRegex`/`jsonContains`），只有驱动声明后才可选。
- 可选项 = 按 `CellType` 的操作符表（`src/lib/filterOperators.ts`）∩ 驱动白名单。同一列换成不同类型，可选操作符就变。
- **不完整条件**的定义（前后端同一套语义）：没有列；`One` 类操作符没有值；`between`/`notBetween` 缺任一端；`Many`（`in`/`notIn`）没有任何 token；正则模式超长。
- 编辑期表现：不完整的条件在编辑器内标记（`filter.incomplete`），Apply 禁用。`applyFilters` 再加一道闸——遇到不完整条件直接 `return`，**不静默丢弃**。丢弃会让结果集悄悄变宽，用户以为是筛选生效了。
- 生效即重置：`setFilters`/`applyFilters` 把 `page` 归零并清空键集游标（`invalidatedKeyset`）。

### 8.2 排序

- 面板只保留**一个**排序（`sorts[0]`）：`setSort` 覆盖而不是追加，无法通过点击构造多列排序。
- 表头双击按 `none → asc → desc → asc` 循环，**没有「取消排序」这一步**；表头右侧箭头按钮只在 `asc`/`desc` 之间切换。想回到「无显式排序」只能改默认序（§8.3）。
- 排序变化 = `page: 0`、清空键集游标、`skipCount: true` 重读（不重算总数）。
- 没有生效顺序时可分页但不稳定：`unstablePaging` 为真，页脚显示「未设置排序——跨页的行可能重复或被跳过」。用户一旦设置显式排序，这条警告消失。

### 8.3 默认序

`resolveDefaultTableSort`（`src/lib/defaultSort.ts`）的优先级：**面板临时覆盖 > 全局设置 `defaultTableSort` > 驱动默认**。面板覆盖只作用于当前标签页、不持久化，选项为 `inherit`（显示为「跟随设置」）、`primaryKeyAsc`、`primaryKeyDesc`、`none`。复合主键时 `primaryKeyDesc` 被禁用并给出原因（`defaultSort.primaryKeyDescUnavailable`＝「暂不支持按复合主键降序排列。」；排序通道只带一列，悄悄只按第一列排会变成另一个顺序）。没有主键的表在面板顶部显示一条提示（`data-testid="table-no-primary-key-tip"`，文案 `defaultSort.noPrimaryKey`＝「没有主键——按第一列排序。」），默认序退化为第一列。

## 9. 分页与行数

### 9.1 页脚只有一个方向

页脚提供：上一页、下一页、每页条数（25/50/100/200/500）、以及**仅在三态为 `estimated` 时**出现的「计数」按钮。「计数」（`useTableActions.countExact`）触发的是**完整重读**（`skipCount: false`、`countStrategy: 'exact'`），会把估算换成真实值；它同样先过导航守卫，所以编辑器里有未落地的草稿时会先尝试提交，提交不了就不重读。

**没有跳页输入框，也没有 `setPageExact` 这种 API。** `setPage(panelId, page)` 接受任意页码，不对页码做夹取：

- 目标页与当前页**相邻**：用键集游标继续 seek。
- 目标页**非相邻**（或第一页之外没有边界）：按 OFFSET 读取该页，并回报 `pagingHint.reason = 'notAdjacent'`，前端据此显示「快速分页已从第 N 页重新开始」。此时语句仍带**全序**（`seek_order`），不是丢弃顺序。
- 按 OFFSET 读时，页大小与页码是唯一输入，没有「从上次结束处继续」的保证。

### 9.2 键集分页

- 键集分页是**驱动显式 opt-in**（`supports_keyset_pagination` 默认 `false`）。未 opt-in 的驱动，语句逐字节不变（`ORDER BY` 也一样）——这是整个契约的前提。
- 前端维护游标栈（`KEYSET_STACK_LIMIT = 200`），栈满丢最旧的；`orderSignature`/`filterSignature` 变了就清栈重来。
- tie-breaker 列只属于 **seek 的总序**（`seek_order`）：真正按边界读时才把 tie-breaker 并进 `ORDER BY`；按 OFFSET 读的页用 `seek_order` 的全序写，保证同一页的行集合稳定。
- 驱动的边界值不可用时回报 `KeysetUnavailableReason`（`driverUnsupported`/`noStableOrder`/`nullableKeyColumn`/`nullBoundary`/`predicateUnavailable`/`noPaginationClause`/`staleCursor`/`notAdjacent`），前端按 §9.4 显示或隐藏。

### 9.3 行数三态

`totalRowsKind` 只有三态，缺失时按 `exact` 处理（不认识三态的后端就是精确计数）：

| kind | 渲染 | 翻页判定 |
|---|---|---|
| `exact` | `1-50 / 1234` | 页数由总数算出，`current < totalPages - 1` 才可前进；页码会被夹到最后一页 |
| `estimated` | `1-50 / 约 1234` + tooltip | 估算只是上界，**恒可前进**，页码从不因估算回退或夹取 |
| `unsupported` | `1-50 / 未知` + 只有「第 N 页」 | 只有整页（`rowCount >= pageSize`）才能前进 |

`canPageForward` 的三态规则与页脚渲染必须同步改：把 `unsupported` 当成 0 行会显示「第 1/1 页」并把用户锁在第一页。

与估算相关的三个提示：估算值可能小于真实行数，翻到空页时提示「实际行数可能低于估算值」；请求了估算但驱动不会估算时提示「此数据库无法估算行数」（这不是错误，读取已回落到精确计数）；翻页过程中数据变动则提示「行数已刷新」（`driftSuspected`）。

### 9.4 降级提示

`PagingHint = { reason, driftSuspected, mode }`，由 `TableView` 从 `ts.keyset` 派生。渲染规则：

- `showPagingReason(reason)` = `reason !== 'driverUnsupported'`。驱动**本来就不会** seek 时，宿主照实上报 `driverUnsupported`，但前端不渲染这条提示——那不是降级，是能力缺失，天天提示等于噪音。这是「事实归宿主、显示归前端」的分工：后端只描述这次读取怎么发生的，不决定要不要告诉用户。
- 模式标签（`pagination.mode.keyset`「快速分页」/ `pagination.mode.offset`「偏移分页」）只在两种情况下出现：发生了回退且原因要展示；或者键集模式且 `totalRows > pageSize`（不止一页）。判断的是行数而不是 `totalPages`，所以 `unsupported`（无总数）永远走不到后半条。
- 有原因必带模式标签，避免用户只看到「为什么慢」却不知道现在用的是什么。
- 模式标签与原因文本都是 `max-w-[12rem]` / `max-w-[22rem]` 加 `truncate`，全文挂在 `title` 上。德语、俄语这类长语言在窄窗口里会被截断成省略号，这是预期表现——不要为此加宽页脚或换行。

## 10. 外键导航

### 10.1 可跳条件

`fkNavigation.ts` 把列的 `foreignKeys` 聚成**组**（复合键是一组）。可跳要求：组完整（本地列与目标列数量一致）、目标表与目标列可解析、单元格值全部非 `NULL`。箭头只画在**首个可见成员**上：复合键的成员列被隐藏时，箭头跟着挪到下一个可见成员，不会出现两个箭头指向同一行。

`NULL` 不降级成 `IS NULL`：外键 `NULL` 的含义是「没有引用」，跳过去只会看到无关结果。

### 10.2 宿主解析，前端不拆表名

前端不拼 SQL、不拆表名：它把 `(表, 本地列, 值…)` 交给宿主，由 `resolve_foreign_key_targets`（`packages/feature-core/src/services/fk_navigation.rs`）在目标库/模式里定位。前端只做显示层的事：`targetLabel` 的文案、`exists` 的结果、错误码的分类。

### 10.3 目标不存在不是错误

`exists: false`（被引用的行已删除）是**成功跳转 + 空结果**：标签页会打开、过滤条件会应用、页面显示「被引用的行已不存在」。把它当失败会阻止用户看到「引用已经悬空」这个真实状态。

### 10.4 跳转的固定顺序

`fkNavigationActions.ts` 的跳转是 9 步固定序列，顺序本身是契约：

1. 请求形状检查（复合键的成员数与目标列数必须一致），在**任何 IPC 之前**。
2. 宿主解析目标。
3. 解析失败**不开标签页**：错误显示在来源标签页（`TableFkErrorBanner`），用户留在原地。
4. 该跳转若已在导航栈/已打开，复用而不是新开。
5. 跳转条件（等值过滤）在标签页存在**之前**就建好——条件不成立就不该有标签页。
6. 新建标签页走跳转专用路径，**按表名去重**，不是「打开表」那条路径。
7. 条件在**同一个事件轮**里种进新标签页的数据切片，避免先闪一页全表。
8. 记录导航栈，标签页带上「从哪来」（面包屑的数据源）。
9. 目标行可能不存在：那是**成功**的跳转 + 空结果（§10.3），不是失败。

### 10.5 面包屑、栈与快捷键

- 面包屑显示 `orders.customer_id → customers.id`（复合键两侧都列全），并提供返回/前进/隐藏。它是纯展示，导航栈在 Store。
- 前进/后退：`Cmd/Ctrl + Alt + ←/→`，由 `useFkNavigationShortcuts`（`src/hooks/useFkNavigationShortcuts.ts`）处理，挂在**面板容器**（`TableView` 的最外层 div）而不是 window——全局监听会和 SQL 编辑器、所有输入框抢键。它在 `handleTableKeyDown` 里**第一个**被调用；IME 组合中、焦点在输入控件里、或跳转能力未启用时直接返回，不 `preventDefault`。
- 栈深上限 `FK_NAV_MAX_DEPTH = 32`，超出丢最早的一步并置 `truncated`，面包屑提示「较早的导航步骤已被丢弃」。

### 10.6 权限

外键跳转**没有自己的权限判定**：没有 grant/权限检查代码路径。权限不足表现为拿不到目标元数据（`grid.fk.targetUnresolved`）或探针失败（`grid.fk.probeFailed`）。换言之，「能不能跳」由目标是否可解析决定，不由外键声明决定。

## 11. 结果网格的只读/可编辑判定链

结果网格（`src/windows/connection/result-workspace/ResultTableView.tsx`）与表数据网格共用同一套单元格/键盘/选择组件，但**编辑准入完全不同**：表数据网格问「这张表能不能写」，结果网格问「这一行的来源能不能定位回一行」。

判定顺序（任一环节为否即只读，并必须给出原因）：

1. **Safe Mode** → `canEdit` 为假，格子点不开。注意它**不产生只读原因条**（原因条只由判定链给），所以「安全模式」下的安静只读是有意的：状态由设置决定，不需要每一页解释一遍。
2. **没有 `onCellEdit` 回调**（宿主没接编辑） → 只读。
3. **来源判定未完成**：默认 `UNJUDGED`，原因按 `unknownOrigin` 处理。判定失败（IPC 错误）也落到这里——「拿不到判定」不是「可以写」。
4. **来源判定为不可写**：宿主返回 11 种原因之一（`ambiguousOrigin`/`multiTable`/`aggregate`/`distinct`/`setOperation`/`derivedSource`/`view`/`noPrimaryKey`/`keyNotSelected`/`sessionReadOnly`/`unknownOrigin`），前端照原样映射到 `query.gridReadOnly.<reason>` 文案。
5. **行级寻址**：`resolveRowAddressability` 在身份列已知时逐行判断，`keyValueNull`/`duplicateIdentity`/`rowNotFound` 三种拒绝原因挂在行上（`data-dt-grid-row-refusal`）；身份列**未知**时返回 `null`（不拒绝，交给后续环节）。
6. **列级可编辑**：仅 origin 通道使用（见下）。
7. **窄通道写回**：`commitQueryResultCell` 真正执行时才判定。

两条通道的开关是硬编码常量：`RESULT_ORIGIN_WRITE_BACK_ENABLED = false`（`src/lib/resultEditability.ts`）。因此 `originChannelWritable` 恒为 `false`，第 6 步的列级守卫目前**不生效**；实际写回只走窄通道（`updateResultCell` → `commitQueryResultCell`），它的服务端规则见 [result-write.md](../backend/result-write.md)。

窄通道还有一条容易忽略的前端事实：**一次执行只能改一格**。写成功后 `panelStore.updateResultCell` 会把该结果的 `runtimeBinding` 置 `null`，第二次编辑会被拒绝并提示需要刷新/重跑查询（`query.resultEditRequiresRefresh`）。这是服务端「保留结果只在绑定仍然有效时可写」的直接后果，不是前端小气。

只读原因的呈现契约：容器带 `data-dt-grid-readonly-reason`（`ResultTableView` 与 `ContentViewDrawers` 都有），tooltip 走 `query.gridReadOnly.tooltip`。**没有原因的只读是不允许的**——新增只读分支必须同时给出原因码与文案。

## 12. `grid.*` 错误码注册表

`src/lib/gridErrors.ts` 是**手工维护**的单一注册表，配一个 `GridErrorCode` 联合类型，并按领域分段。没有运行时枚举，也没有从 Rust 自动生成的清单——两边靠测试对齐：`src/lib/__tests__/gridErrors.registration.test.ts` 断言「注册表里的每个码都有产出方，或明确列在 `PRODUCER_PENDING`」，另有一条断言「`PRODUCER_PENDING` 里不许出现已有产出方的码」。

**不要在文档里写死码的总数**：注册表按领域追加（每个领域一段），数量随功能增长，数字一写就会过期。要判断某个码存不存在，看文件本身。

分类规则：`classifyGridError` 取**最长前缀**且要求 `:` 分隔、大小写敏感，整条消息恰好等于某个码也算命中。因此 `grid.cell.readOnlyBecause: …` 不会被误判成 `grid.cell.readOnly`；码出现在普通散文里也不会被当成错误。未知前缀不猜，保留原文渲染。

`PRODUCER_PENDING` 的语义是「**这个码不由 Rust 产出**」，不是「还没做完」。典型成员是那些在前端就地拒绝、值根本没到后端的码（例：`grid.cell.invalidValue`，类型化编辑器拒绝一个无法解析的输入）。名单以测试文件为准，同样不要在这里抄一份。

选择域没有码（§3.6）；行数/估算/降级也不产出错误码（§9.4）——它们是状态，不是失败。新增错误码时要动三处：`GRID_ERROR_CODES` 清单、`GridErrorCode` 联合、产出方（Rust 字面量或前端调用点），并让 `gridErrors.registration.test.ts` 能扫到该产出方。**翻译不在这里**：前端只用前缀决定分支，用户看到的是后端消息原文（已脱敏），所以给码加 i18n 文案不会生效。

## 13. 改动影响速查

| 想改什么 | 会连带碰到 |
|---|---|
| 行高 | `DATA_TABLE_ROW_HEIGHT`、外键视口定位、`viewportRowCount` 整屏步进、`PageUp/Down` 步长 |
| `data-dt-*` 属性 | GridCell（唯一拥有者）、选择/脏标记渲染、E2E 定位、`useGridSelection` 的命中判定 |
| 键盘键表 | `gridKeyboard.ts` 的优先级、`applyCommand` 的三态、`GridKeyboardHints` 文案、journey 测试 |
| `tabIndex` 策略 | 网格 Tab 序、行级 Enter/空格、行号槽可达性、`gridEventGuards.isEditableTarget` |
| 选择语义 | `gridSelection` 纯函数、`selectionActions`、状态栏计数、批量动作的行覆盖来源 |
| 编辑器何时写入 | `CellEditorHost` 三条规则、`activeEditor`、导航守卫、`coerceValue` 三态 |
| 分页 `setPage` | 键集栈、`PagingHint` 渲染、`canPageForward` 三态、`skipCount` |
| 行数三态 | `totalRowsDisplay`、页脚文案、估算提示、计数按钮可见性 |
| 外键组规则 | `fkNavigation.ts` 聚合、箭头落点、跳转 9 步、面包屑标签 |
| 结果网格只读原因 | `GridReadOnlyReason`（Rust）↔ `GridReadOnlyReason`（TS）、`query.gridReadOnly.*` 文案、`data-dt-grid-readonly-reason` |
