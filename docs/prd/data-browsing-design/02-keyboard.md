# 02 · 键盘导航与快捷键（DB-02）

> **状态**：**目标设计，未实现**。截至本册写作时，仓库内没有任何方向键导航、没有 `Cmd/Ctrl+A` 处理函数、没有 `F2`、没有网格容器的焦点宿主（`role` / `tabIndex` 均不存在于网格容器上）。
> **依赖**：`00-contracts.md`（§1.3 `ShortcutDef` 与匹配语义、§4 单元格坐标系与选择模型、§4.4 DOM 属性）、`01-selection.md`（`src/stores/tableData/gridSelection.ts` 冻结类型与纯函数、`src/hooks/useGridSelection.ts` 的 `surfaceProps.onKeyDown` 唯一入口、`src/lib/gridEventGuards.ts`、`src/stores/tableData/selectionActions.ts`、`TableState.gridSelection`）。
> **被谁依赖**：**03**（剪贴板要按「方向键 / `Cmd+A` 之后」的选区取值）、**05**（编辑器必须与网格态键位互斥，且 `Tab`/`Enter` 的归属要与本册一致）、**06**（`Cmd/Ctrl+Z` 归 06，本册必须显式不注册它）、**10**（焦点格的滚动可见性由 `onRequestScrollIntoView` 挂点接入）。
> **预估人日**：**2.5 人日**（纯逻辑 `gridKeyboard.ts` + 单测 0.75；React 接线 0.5；组件测试与连续旅程测试 0.75；E2E 与既有选择器迁移 0.5）。
> **不包含**：`Cmd/Ctrl+C/X/V`（03）；`Cmd/Ctrl+Z` / `Shift+Cmd/Ctrl+Z`（06）；单元格编辑的取值、提交与类型强制（05）；`Delete`/`Backspace` 清空单元格内容（本册**明确不动**，其归属见 K-8）；快速查找与行跳转（08）；排序/筛选列的键盘操作（08/09）；行级批量操作的业务实现（01/06）；`role` 三元组（`grid`/`row`/`gridcell`）的完整无障碍重构（K-7）。

---

## 1. 目标与验收口径

**一句话目标**：为网格建立一套**与既有整行选择、与单元格编辑器都不打架**的键盘导航：方向键/`Home`/`End`/`PgUp`/`PgDn` 移动焦点格，`Shift+方向键`扩区，`Enter`/`F2` 进入编辑，`Cmd/Ctrl+A` 整行全选，`Tab` 离开网格，且**任何键在输入法组合期间都不被网格截获**。

**可验证的验收标准**（命令 + 期望输出；本册不跑，交由实现方执行）：

| # | 命令 | 期望 |
| --- | --- | --- |
| A1 | `npx vitest run src/lib/__tests__/gridKeyboard.test.ts` | 全绿；`Tests` 数 ≥ §9 单测清单条目数 |
| A2 | `npx vitest run src/components/DataTable/__tests__/gridKeyboardJourney.test.tsx` | 全绿；journey 用例断言了**每一个中间态**，不允许只断言终态 |
| A3 | `npx vitest run src/components/DataTable/__tests__/VirtualBody.test.tsx src/components/DataTable/__tests__/DataTable.test.tsx` | 全绿（这两个文件里的「定位行 div」表达式已随焦点裁定迁移，见 §4.4） |
| A4 | `pnpm typecheck` | 退出码 0；`gridKeyboard.ts` / `useGridKeyboardNav.ts` / 测试文件零 `any`、零 `@ts-ignore` |
| A5 | `pnpm e2e:minimal` | `TC-TABLE-013` 由**假绿**变为真断言并全绿（改造方式见 §5 Step 7）；新增 `table-keyboard-nav` 用例全绿 |
| A6 | `wc -l src/components/DataTable/DataTable.tsx src/components/DataTable/VirtualBody.tsx src/lib/gridKeyboard.ts src/hooks/useGridKeyboardNav.ts` | 每个文件 **≤ 800**；`DataTable.tsx` 必须显著低于 800（起点已 628 行，见 §6） |

---

## 2. 现状代码事实

**以下符号均为本册作者亲自读取，不含推测；按纪律不写行号。**

### 2.1 `src/hooks/useKeyboardShortcuts.ts`（**全文读过**）

| 符号 | 事实 |
| --- | --- |
| `interface ShortcutDef { key: string; scope: 'global' \| 'editor' \| 'table'; action: () => void; description: string }` | 三个作用域；`key` 是形如 `mod+c` / `shift+enter` / `escape` / `delete` / `space` 的字符串 |
| `function matchShortcut(def, ctx)` | 修饰键**精确匹配**：`needsMod !== ctx.mod` 即不命中，所以 `mod+c` 在 `mod+shift+c` 下不触发；`key` 走 `e.key.toLowerCase()`；`escape` / `delete`（同时匹配 `Delete` 与 `Backspace`）/ `enter` / `space` 有专名分支 |
| `useKeyboardShortcuts(shortcuts)` | `useEffect` 里 `window.addEventListener('keydown', …)`，一次注册、依赖数组为空；`shortcutsRef` 每次渲染刷新，因此不必把 `shortcuts` 放进依赖 |
| 作用域判定（同一函数内） | `const inField = tag === 'input' \|\| tag === 'textarea' \|\| target?.isContentEditable === true`；`scope === 'table'` 且 `inField` 时 `continue` 跳过；`scope === 'editor'` 在**非**字段上跳过；**`scope === 'global'` 不受 `inField` 限制** |
| 命中后的行为 | `e.preventDefault()` → `shortcut.action()` → `return`，**只执行第一个命中的快捷键**（注册顺序即优先级） |
| 注册位置事实 | 全仓 `useKeyboardShortcuts` 的引用点：`src/lib/keymap.ts`、`src/windows/connection/ContentView.tsx`（及其测试 `src/windows/connection/__tests__/ContentView.test.tsx`）。**网格组件（`DataTable.tsx` / `VirtualBody.tsx`）里没有任何引用**——即网格的键位今天完全没走这套 hook |

### 2.2 `src/components/DataTable/VirtualBody.tsx`（**读了 `handleClick`、`handleSelectButtonClick`、`handleKeyDown`、行容器与单元格的 `div`**）

| 符号 | 事实 |
| --- | --- |
| `handleClick` | 行容器 `onClick` → `onRowSelect(vRow.index, { multi: e.metaKey \|\| e.ctrlKey, range: e.shiftKey })`（既有整行选择入口，`row` 模式必须继续复用它） |
| `handleKeyDown` | 行容器 `onKeyDown`：`e.key === 'Enter' \|\| e.key === ' '` → `e.preventDefault()` + `onRowSelect(vRow.index)`。**这是 `Enter` 的既有占用者**，也是行容器 `tabIndex` 存在的唯一理由 |
| `handleRowDoubleClick` | 行容器 `onDoubleClick` → `onCellDoubleClick(vRow.index, colNames[0])`（双击首列） |
| `handleSelectButtonClick` | 行号槽 `<button type="button">` 的 `onClick`：`e.stopPropagation()` 后走同一个 `onRowSelect(vRow.index, { multi, range })` |
| 行容器 `div` | `tabIndex={0}`；`style={{ top: vRow.start, height: rowHeight }}`；绑定 `onClick` / `onKeyDown` / `onDoubleClick`。**全仓网格里唯一的 `tabIndex`**（`grep` 结果：`src/components/DataTable/*.tsx` 与 `src/windows/connection/TableView.tsx` 共命中 1 处，即此处） |
| 单元格 `div` | `data-testid="data-table-cell"`、`data-dt-row={vRow.index}`、`data-dt-col={col.name}`、`onDoubleClick` → `onCellDoubleClick(vRow.index, col.name)`；内层 `CellRenderer` 按 `isEditing` 渲染（`editingCell?.row === vRow.index && editingCell.col === col.name`） |

### 2.3 `src/components/DataTable/EditableCell.tsx`（**读了键盘分支**）

| 符号 | 事实 |
| --- | --- |
| `onKeyDown` | `e.key === 'Enter'` → `preventDefault()` + `coerceAndCommit(local)`；`e.key === 'Escape'` → `preventDefault()` + `handleCancel()` |
| `onBlur` | `() => coerceAndCommit(local)` ——**任何导致焦点离开输入框的键（含 `Tab`）都会提交**，这是本册 `Tab` 裁定的关键约束 |
| `isComposing` 守卫 | **不存在**（`Enter` 分支没有 IME 守卫）。01 §4.5 已把该缺陷登记给 05，本册与其一致：**不修 `EditableCell`**，只在网格容器侧做守卫 |
| 正向参照 | `TableView.tsx` 的快速筛选输入框写了 `!event.nativeEvent.isComposing`（由 01 §4.5 记载），是本项目已有的正确写法 |

### 2.4 `src/windows/connection/TableView.tsx` 与 `src/components/DataTable/DataTable.tsx`

| 事实 | 证据 |
| --- | --- |
| 两文件**都没有** `useKeyboardShortcuts` | `grep -n 'useKeyboardShortcuts\|tabIndex'` 对这两个文件命中 0 处（退出码 1） |
| `DataTable.tsx` 没有网格容器 `role` | `grep -n 'role='` 只命中 `role="status"` 两处（加载/空态），**没有 `role="grid"`** |
| 文件规模 | `DataTable.tsx` 628 行、`TableView.tsx` 770 行、`VirtualBody.tsx` 207 行、`EditableCell.tsx` 119 行 |
| 既有测试依赖 `[tabindex="0"]` 定位行 div | `src/components/DataTable/__tests__/VirtualBody.test.tsx` 2 处、`src/components/DataTable/__tests__/DataTable.test.tsx` 1 处、`e2e/specs/detail-panel.ts` 1 处、`e2e/specs/ops-process-server.ts` 2 处 |

### 2.5 `e2e/specs/table-batch-ops.ts` 的 `TC-TABLE-013`

| 事实 | 证据 |
| --- | --- |
| 用例名 | `'TC-TABLE-013: Ctrl+A 应选中当前页所有行'` |
| 用例体 | `browser.execute(() => { const table = document.querySelector('table, [role="grid"]'); if (table) (table as HTMLElement).focus(); })` → `browser.keys(['Meta', 'a'])` → `browser.pause(500)` → `captureJourneyStep('table-select-all')` |
| 为什么是**假绿** | ① 选择器 `table, [role="grid"]` 在当前 DOM 里**匹配不到任何元素**（§2.4 已证实网格容器没有 `role="grid"`，也没有 `<table>`），`if (table)` 分支不执行，焦点从未落到网格上；② 用例体**没有任何断言**，只有 `pause` 与截图步骤；③ 全仓没有任何 `Cmd/Ctrl+A` 处理函数（网格没引用 `useKeyboardShortcuts`，`ShortcutDef` 里也没有 `mod+a`）。三条叠加 ⇒ 无论功能是否存在，用例都必然通过 |

### 2.6 由 §2 推出的三个硬结论

1. **`Enter` 已被占用**：`VirtualBody` 行容器的 `onKeyDown` 把 `Enter`/`Space` 映射为「选整行」。若把 `Enter` 裁定给「进入编辑」而不处理这处，会出现两个处理器竞争同一个键。
2. **`Tab` 今天会从行容器逃走**：行 `div` 是 `tabIndex={0}`，`Tab` 只能跳到下一个行 `div`。一旦按焦点裁定改成 `tabIndex={-1}`，行不再是 Tab 停靠点，`Tab` 的语义必须由本册重新定义。
3. **`[role="grid"]` 不存在**：任何以它为前提的断言（含 `TC-TABLE-013`）都是空的；本册必须给出一个**真实存在**的容器定位符。

---

## 3. 数据结构与接口设计

### 3.1 新增纯逻辑模块 `src/lib/gridKeyboard.ts`

位置：`src/lib/gridKeyboard.ts`（**新增**）。**禁止 import React、禁止 import store、禁止读 DOM**（与 01 的 `gridSelection.ts` 同一纪律）；仅 `import type` 01 的冻结类型，以及复用 01 的纯函数。

```ts
import type { CellCoord, CellRange, GridSelection } from '../stores/tableData/gridSelection';

/** 键盘事件的结构化投影：只取判定需要的字段，**不用 any**，也便于单测直接构造。 */
export interface GridKeyEventLike {
  /** 已做 `toLowerCase()` 归一（与 useKeyboardShortcuts 的匹配语义保持一致）。 */
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  /** 原生事件读 `e.isComposing`；React 合成事件由调用方摊平为 `e.nativeEvent.isComposing`。 */
  isComposing: boolean;
}

/** 键 → 意图。`ignored` 与 `pass` 的区别见 3.2：ignored 是「02 认领但本次不做事」，pass 是「不归 02」。 */
export type GridKeyIntent =
  | { kind: 'move'; deltaRow: number; deltaColumn: number }
  | { kind: 'move-page'; direction: 'up' | 'down' }
  | { kind: 'move-edge'; edge: 'row-start' | 'row-end' | 'grid-start' | 'grid-end' }
  | { kind: 'select-all-rows' }
  | { kind: 'begin-edit' }
  | { kind: 'leave-grid' }
  | { kind: 'ignored' }
  | { kind: 'pass' };

export interface GridKeyboardContext {
  /** 当前渲染列顺序（= displayedColumns 的列名数组）。方向键只在可见列之间移动。 */
  columnOrder: readonly string[];
  rowCount: number;
  mode: GridSelection['mode'];
  /** 当前焦点格；`null` 表示 `none` 模式下的冷启动（本册用进入光标兜底，见 resolveEntryCursor）。 */
  cursor: CellCoord | null;
  /** 虚拟滚动的可见行数（≥ 1），用于 PgUp/PgDn 的步长；由调用方提供，本模块不做几何推算。 */
  viewportRowCount: number;
  /** 编辑器是否打开（= `editingCell !== null`）。 */
  editing: boolean;
}

/** 02 对 React 层的唯一输出：告诉它「这一键要变成什么」，不自己写 store。 */
export type GridKeyCommand =
  | { kind: 'set-selection'; selection: GridSelection }
  | { kind: 'row-select'; rowIndex: number; opts: { multi: boolean; range: boolean } }
  | { kind: 'select-all-rows' }
  | { kind: 'begin-edit'; coord: CellCoord }
  | { kind: 'consume' }   // 02 认领并已 preventDefault，但状态不变（边界 no-op、leave-grid）
  | { kind: 'pass' };     // 不归 02：**不得** preventDefault、不得 stopPropagation

/** `row` 模式下的空选择常量：满足 01 的不变量 I2（整行高亮只来自 selectedRows）。 */
export const EMPTY_ROW_MODE_SELECTION: GridSelection;

/** 把 React 合成事件摊平为结构投影；避免把 React 类型渗进纯逻辑模块。 */
export function toGridKeyEventLike(e: {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  nativeEvent: { isComposing?: boolean };
}): GridKeyEventLike;

/** 组合期守卫：`isComposing` 或 `key === 'process'`（Chromium 在 IME 组合期上报的键名）均为真。 */
export function isImeComposingKey(e: GridKeyEventLike): boolean;

/** 键 → 意图。纯映射，不看 state；优先级顺序即 §4.1 表的行序。 */
export function readGridKeyIntent(e: GridKeyEventLike, ctx: GridKeyboardContext): GridKeyIntent;

/** 从 `selectedRows`/`lastSelectedIndex` + 选区派生「键盘光标」；row 模式取 `lastSelectedIndex` × 首列。 */
export function resolveKeyboardCursor(
  selection: GridSelection,
  lastSelectedIndex: number | null,
  columnOrder: readonly string[],
): CellCoord | null;

/** `none` 模式重新进入时的光标：优先复用上次光标（仍有效时），否则首格；两者皆不可用返回 null。 */
export function resolveEntryCursor(
  remembered: CellCoord | null,
  rowCount: number,
  columnOrder: readonly string[],
): CellCoord | null;

/** 意图 → 命令。**这是本册唯一的语义中心**，所有键盘行为都在这里可单测。 */
export function resolveGridKeyCommand(
  intent: GridKeyIntent,
  ctx: GridKeyboardContext,
  selection: GridSelection,
): GridKeyCommand;

/** PgUp/PgDn 步长：`max(1, viewportRowCount - 1)`（保留一行上下文，避免翻屏后失去参照）。 */
export function resolvePageDelta(viewportRowCount: number): number;

/** 光标与坐标比较（复用 01 的语义等价判定，避免各处写 `a.rowIndex === b.rowIndex && …`）。 */
export function gridCursorEquals(a: CellCoord | null, b: CellCoord | null): boolean;

/** 列显隐 / 行集变化后收敛光标：越界或列已隐藏 ⇒ `null`（与 01 的 reconcileGridSelection 同步调用）。 */
export function clampGridCursor(
  cursor: CellCoord | null,
  rowCount: number,
  columnOrder: readonly string[],
): CellCoord | null;
```

**内部实现纪律（必须落在代码注释里）**：

- `move` 一律经 01 的 `moveCellFocus(cursor, deltaRow, deltaColumn, rowCount, columnOrder)`；返回 `null` 即「无处可去」⇒ 命令为 `{ kind: 'consume' }`（**保留原选区、不塌陷**，见 §7 E4/E5）。
- `move-edge` 一律经 01 的 `coordAt(rowIndex, columnIndex, columnOrder)` + `clampCellCoord`，**禁止**在 02 里重算列序。
- 选区构造一律经 01 的 `selectionFromAnchorFocus(anchor, focus, 'cell', extraRanges)`；`anchor` 为 `null` 时（残缺中间态）退回「以新光标为单格」。
- **不新增任何区域运算**：本册不使用 `normalizeCellRange` / `cellRangeContains` 的私有副本，只调用 01 导出。

### 3.2 `ignored` 与 `pass` 的语义分界（必须逐字实现）

| 意图 | 含义 | React 层的义务 |
| --- | --- | --- |
| `{ kind: 'consume' }` | 02 认领了这个键，但本次不改状态（边界 no-op、`Tab` 离开网格） | **必须** `preventDefault()`（防止 `Tab` 把焦点送到行号槽、防止 `Cmd+Home/End` 触发页面滚动） |
| `{ kind: 'ignored' }` | 02 认领了键名，但当前上下文下 02 不做任何事（如 `none` 模式下的 `Enter`） | `preventDefault()` **可选**；选择「不 preventDefault」以免吃掉浏览器默认行为时必须写注释说明 |
| `{ kind: 'pass' }` | 不归 02（`Cmd+C/V/X` → 03、`Cmd+Z` → 06、`Escape` → 01、IME 组合、编辑态按键） | **严禁** `preventDefault()`、**严禁** `stopPropagation()`；原样放行 |

> 之所以把 `consume` 与 `ignored` 分开：`Tab` 必须被 `preventDefault` 掉但状态不变（`consume`），而 `none` 模式下的 `Enter` 我们希望未来能给 05/09 使用（`ignored`）——把两者混成一个 `handled: boolean` 会在实现时把「放行」写成「吞掉」，正是 00-contracts §1.3 第 4 条（命中即 `preventDefault` 并 `return`）最容易出事的地方。

### 3.3 新增 React 接线模块 `src/hooks/useGridKeyboardNav.ts`

```ts
import type { CellCoord, GridSelection } from '../stores/tableData/gridSelection';

/** 02 需要的外部动作：全部来自 01/既有的 props，02 不新增 store action。 */
export interface GridKeyboardDispatch {
  /** 01 的受控回写（`DataTableProps.onGridSelectionChange`）。 */
  onGridSelectionChange: (next: GridSelection) => void;
  /** 既有双击入口；`Enter`/`F2` 复用它，从而自动继承「只读连接 / 无主键」的既有提示语义。 */
  onCellDoubleClick: (rowIndex: number, columnName: string) => void;
  /** 01 的行号槽路径（内部即既有 `selectRow`）；`row` 模式的键盘移动走它。 */
  onRowGutterSelect: (rowIndex: number, opts: { multi: boolean; range: boolean }) => void;
  /** 既有 `toggleSelectAll`（`Cmd/Ctrl+A` 的唯一实现体）。 */
  onToggleSelectAll: () => void;
  /** 10 的挂点：请求把光标格滚入可见区。未提供时为 no-op，不得阻塞导航。 */
  onRequestScrollIntoView?: (coord: CellCoord) => void;
}

export interface UseGridKeyboardNavOptions extends GridKeyboardDispatch {
  columnOrder: readonly string[];
  rowCount: number;
  /** 受控选区（唯一真相在 store）。 */
  selection: GridSelection;
  /** 既有 `TableState.lastSelectedIndex`，**只读**用于 row 模式光标。 */
  lastSelectedIndex: number | null;
  /** 编辑器占用态（既有 `editingCell !== null`）。 */
  editing: boolean;
  /** 虚拟滚动的可见行数；调用方注入，hook 自己不做几何推算。 */
  viewportRowCount: number;
  enabled?: boolean;   // 缺省 true
}

export interface UseGridKeyboardNavResult {
  /** 交给 01 的 `surfaceProps.onKeyDown` **在最前面调用一次**，随后按返回值决定是否继续。 */
  handleKeyDown: (e: React.KeyboardEvent<HTMLDivElement>) => void;
  /** 供渲染层标注 `data-dt-active="true"` 的当前光标格。 */
  activeCoord: CellCoord | null;
  /** `data-dt-row-view` 的行级焦点标记（row 模式用 `lastSelectedIndex`）。 */
  activeRowIndex: number | null;
}

export function useGridKeyboardNav(
  options: UseGridKeyboardNavOptions,
): UseGridKeyboardNavResult;
```

**唯一入口纪律**：键盘事件**只能**有一个监听点。01 的 `surfaceProps.onKeyDown` 是契约入口（01 §4.3 已写「02 只需在同一个 `surfaceProps.onKeyDown` 链上追加分支」）；`useGridKeyboardNav` 只导出 `handleKeyDown` 而不自行 `addEventListener`。**严禁**在本册再挂一个 `window` / 容器 keydown（那样会与 `useKeyboardShortcuts` 的 `window` 监听叠加，出现同一个键双触发）。

### 3.4 `DataTableProps` 增量（全部可选 ⇒ 既有消费者一行不改）

```ts
export interface DataTableProps {
  // …既有 props 全部保持不变…

  /** 是否启用键盘导航；缺省 `false` ⇒ 逐位等于今天（不拦截任何键、不加容器 tabIndex）。 */
  keyboardNavigation?: boolean;                                  // 新增
  /** 10 的滚动挂点。 */
  onRequestScrollIntoView?: (coord: CellCoord) => void;           // 新增
}
```

| prop | 用途 | 缺省 | 兼容性说明 |
| --- | --- | --- | --- |
| `keyboardNavigation` | 网格键位总开关 | `false` | 与 01 的 `gridSelection`（缺省未启用）**成对开启**：单元格选择未启用时方向键无目标，因此本册不提供「有键盘导航但无选区」的组合；应用层（`TableView`）同一次改动里把两个 prop 一起传 |
| `onRequestScrollIntoView` | 光标格可见性 | `undefined` | `undefined` ⇒ 不滚动，键盘仍可用（只是可能移到视口外） |

### 3.5 DOM 属性增量（本册新增，单位是「谁输出」）

| 属性 | 值 | 语义 | 输出者 | 备注 |
| --- | --- | --- | --- | --- |
| `data-dt-surface` | `"true"` | 网格滚动容器标记；**焦点宿主**与 E2E 聚焦点的唯一稳定选择器 | 容器 `div`（`DataTable.tsx`） | 与 01 **共用**同一元素（同一个属性名，不是两个属性）；`role` 决策见 K-7 |
| `data-dt-row-view` | 行下标 | 行容器标记（`VirtualBody` 的行 `div`） | 行容器 `div` | **必须**与 `data-dt-row` 区分：`data-dt-row` 在单元格上，被 `closest('[data-dt-row]')` 用于反查单元格；行容器若也叫 `data-dt-row`，`closest` 会先命中行容器而丢掉列名，**直接破坏既有单元格解析** |
| `data-dt-active` | `"true"` / **不输出** | 该格是键盘光标格（`:focus` 之外的「当前单元格」标记） | 单元格 `div`（`VirtualBody`/01 的 `GridCell`） | 只由 props 派生，禁止写入局部 state / DOM class（01 §4.6 同一纪律） |

---

## 4. 交互与状态机

### 4.0 优先级（冲突时谁赢）——**按序判定，第一条命中即定案**

| 序 | 条件 | 结果 | 理由 |
| --- | --- | --- | --- |
| 1 | `keyboardNavigation === false` | 一切键 `pass` | 逐位兼容今天 |
| 2 | `isImeComposingKey(e)`（`isComposing` 或 `key === 'process'`） | `pass` | 见 §4.5；这是最高优先级，**先于**所有编辑态与导航判定 |
| 3 | `editing === true` 或 `isEditableTarget(e.target)` | `pass`（编辑器/浏览器默认行为胜出） | 见 §4.3；复用 `useKeyboardShortcuts` 中 `inField` 的**同一判定语义**（01 的 `isEditableTarget`），而不是复用它的 API |
| 4 | `Escape` | `pass`，由 01 的 `surfaceProps.onKeyDown` 分支处理（01 §4.3 规定 `editingCell !== null` 时直接返回） | 避免两处都清选区；`Escape` **只有一个**注册者 |
| 5 | `mod + c/x/v`、`mod + z`（含 `mod+shift+z`） | `pass`（03 / 06 的实现者注册） | 本册**不注册**，也不 `preventDefault` |
| 6 | §4.1 表中的本册键位 | `consume` / `ignored` / 具体命令 | — |
| 7 | 其余（含所有可打印字符、`Space`、`Delete`/`Backspace`、`Alt+*`） | `pass` | `Delete` 的既有处理（`handleDeleteKey`）语义不变；`Space` 见 K-6 |

> **为什么不把网格键注册进 `useKeyboardShortcuts`**：该 hook 监听 `window`，`ShortcutDef` 里**没有**「焦点必须在网格容器内」这一维度。用 `scope: 'table'` 只能挡住 `input`/`textarea`/`contentEditable`（§2.1 的 `inField`），挡不住「焦点在工具栏按钮上」「同屏有两个网格」「焦点在另一个 Tab 的数据表上」——`mod+a` 一旦按窗口级注册就会跨网格抢键。因此：**`scope: 'table'` 的「输入框内跳过」语义在本册被复刻为实现细节（用 01 的 `isEditableTarget`），而不是被复用为注册方式**；`useKeyboardShortcuts` 继续只承载真正的全局动作（`ContentView` / `keymap` 既有用法不动）。

### 4.1 键位归属表（Q1）

「模式」列：`none` / `cell` / `row` 为 00-contracts §4.2 的三态；「编辑态」指 `editingCell !== null`。

| 键 | 网格态（`none`/`cell`/`row`）行为 | 编辑态行为（±） | 归属分册 | 冲突裁定（谁赢） |
| --- | --- | --- | --- | --- |
| `↑` `↓` `←` `→`（无修饰） | `cell`：焦点格移动一格（列序移动，**不换行、越界 no-op**），主区塌陷为单格，`extraRanges` 清空；`row`：整行选择上/下移一行（走 `onRowGutterSelect(next, {range:false})`）；`none`：**进入 `cell`**，光标取 `resolveEntryCursor`（上次光标／首格） | `pass`（输入框内移动插入符由 05/浏览器负责） | **02** | 02 赢（容器是焦点宿主）。**今天无占用者**，无冲突 |
| `Shift+↑↓←→` | 从 `anchor` 扩区到新焦点（`anchor === null` 时以旧焦点建锚，即「进 1 格 + 扩 0 格」，不丢事件）；`extraRanges` **保留**；`row` 模式走 `onRowGutterSelect(next, {range:true})` 从 `lastSelectedIndex` 扩 | `pass` | **02** | 同上 |
| `Home` / `End` | 行首 / 行尾（**当前行**的可见列首/尾）；`Shift` 变体扩区；`row` 模式落到该行首列 | `pass` | **02** | 02 赢；`Cmd+Home/End` 需 `preventDefault`（否则触发页面/容器滚动） |
| `mod+←` / `mod+→` | 与 `Home` / `End` 等价（macOS 笔记本键盘没有独立 `Home`/`End`，无此映射则 Mac 用户拿不到行首/行尾） | `pass` | **02** | 同上 |
| `PgUp` / `PgDn` | 焦点行 ± `resolvePageDelta(viewportRowCount)`（`viewportRowCount - 1`，下限 1），**不取数**；`Shift` 变体扩区 | `pass` | **02** | **02 赢，且明确不与分页取数绑定**：跨页取数会触发契约 §4.3 的「重新取数必须清空全部选择」⇒ 按一下 `PgDn` 就丢掉选区，与「一屏内移动光标」的直觉冲突（K-3） |
| `mod+Home` / `mod+End` | 网格首格 / 末格（`coordAt(0,0)` / `coordAt(rowCount-1, lastCol)`） | `pass` | **02** | 必须 `preventDefault` |
| `Tab` / `Shift+Tab` | `consume` + `preventDefault`：**离开网格**（焦点不进入网格内部任何元素；因为按焦点裁定后网格内已无其他 Tab 停靠点，`preventDefault` 只为挡住行号槽 `<button>` 的 Tab 停靠） | `pass`：**05 注册**（`preventDefault` + 提交 + 焦点格右移一格，末列则下移一行首列） | 网格态 **02**；编辑态 **05** | 编辑态 05 赢。理由：`EditableCell.onBlur` 会 `coerceAndCommit`（§2.3），所以「不处理 `Tab`」也表现为提交，但焦点会逃到行号槽按钮 ⇒ 必须由 05 显式接管（K-1） |
| `Enter` | `cell`：对 `focus` 执行 `onCellDoubleClick`（= 进入编辑或只读提示）；`row`：对 `lastSelectedIndex` × `columnOrder[0]` 执行同一调用；`none`：`ignored`（不隐式建立选择） | `pass`：`EditableCell` 已 `preventDefault` + `coerceAndCommit`（§2.3） | **02**（网格态） | **02 赢，但必须删除 `VirtualBody` 行容器的 `Enter`/`Space` → `onRowSelect` 分支**（§5 Step 4）。理由见 §4.2 |
| `F2` | 与 `Enter` 网格态完全一致（进入编辑） | `pass` | **02** | 今天无占用者 |
| `Escape` | `pass` → 01 处理（`mode → none` + 清空全部选择；`editingCell !== null` 时 01 直接 return） | `pass`：`EditableCell` 取消编辑（§2.3） | **01** | 01 赢；02 **不注册** `Escape` |
| `mod+A` | `select-all-rows`：既有 `toggleSelectAll()` + 选区切到 `row` 模式（`EMPTY_ROW_MODE_SELECTION`）；再按一次即清空（`toggleSelectAll` 的既有语义） | `pass`（输入框内是「全选文本」，浏览器原生） | **02** | **裁定：整行全选，不是「当前页全部单元格」**，理由见 §4.6 |
| `mod+Z` / `mod+shift+Z` | `pass`（**02 不注册**） | `pass`（浏览器原生 input undo 胜出） | **06** | 06 赢。06 未落地前按 `mod+Z` 应当**无任何效果**，不得回退成「清空选区」之类 |
| `mod+C/X/V` | `pass` | `pass`（浏览器原生文本剪贴板） | **03** | 03 赢 |
| `Delete` / `Backspace` | `pass` → 既有 `handleDeleteKey`；`cell` 模式下 `selectedRows` 恒空 ⇒ **什么都不做**（§4.2 第 2 条的必然推论） | `pass`（输入框内删字符） | 既有代码 | 语义**不变**；「清空单元格内容」不属本册（K-8） |
| `Space` | `pass`（浏览器滚动容器默认行为） | `pass`（输入空格） | 未分配 | 见 K-6 |
| `Alt+*` / 可打印字符 | `pass` | `pass` | — | 02 不参与 |

（±）编辑态所有键归编辑器，**包括方向键**——输入框内的左右键必须移动插入符，不能被网格抢去移动光标格。

### 4.2 Q2：方向键移动整行还是单元格？如何不破坏既有整行选择

**裁定：由 `mode` 决定，而不是由键决定。**

| `mode` | 方向键语义 | 实现的既有符号 |
| --- | --- | --- |
| `cell` | 移动**单元格**光标，主区随之塌陷/扩区 | 01 的 `moveCellFocus` + `selectionFromAnchorFocus` → `onGridSelectionChange` |
| `row` | 移动**整行**选择（`↑`/`↓`） | 01 的 `onRowGutterSelect` → 既有 `selectRow(index, { multi: false, range })` |
| `none` | **进入 `cell`** 模式（键盘可达性入口） | 同上第一条 |

**不破坏既有整行选择的三条保证**：

1. **`row` 模式一行代码都不新建**：整行高亮继续来自既有 `selectedRows`，锚点继续是既有 `lastSelectedIndex`（契约 §4.2「必须遵守」第 1 条）；`lastSelectedIndex` 由既有 `selectRow` 维护，02 **只读**它。
2. **`cell` 模式清空 `selectedRows`**（契约 §4.2「必须遵守」第 2 条）由 01 的 `applyBeginCellSelection` / `applyGridSelection` 保证；02 只通过 01 的 `onGridSelectionChange` 回写 `GridSelection`，**从不直接写 `selectedRows`**。
3. **行级操作的作用域一律用派生的 `rowsCoveredBySelection`**（契约 §4.2「必须遵守」第 3 条），**禁止**把 `rowsCoveredBySelection` 的结果写回 `selectedRows`。这一点对本册尤其关键：`cell` 模式下用户按 `Shift+↓` 扩到 12 行，界面上看到 12 行被覆盖，但 `selectedRows` **必须仍为空**；此时按 `Delete` 必须是 no-op（既有 `handleDeleteKey` 只看 `selectedRows`），行级删除只能由显式的行级入口（行号槽 / `Cmd+A`）发起。这条必须写进 journey 测试。

**边界处必须 no-op 而非塌陷**：`row` 模式下已在第 0 行再按 `↑`，**不得**调用 `onRowGutterSelect(0, { range: false })`——`selectRow` 的既有语义里「单击已选中的唯一行 = 清空」会让上边界按键把选择整个清掉。实现上由 `resolveGridKeyCommand` 判定 `next === cursor.rowIndex` 时返回 `{ kind: 'consume' }`。

### 4.3 Q3：与编辑器的冲突

| 网格态按键 | 编辑态（`editingCell !== null` 或 `isEditableTarget(e.target)`）行为 | 依据 |
| --- | --- | --- |
| 方向键 | 移动插入符（05/浏览器），02 完全不介入 | §4.0 序 3 早退 |
| `Tab` | 05 接管：`preventDefault` + 提交 + 光标格右移（末列换行）；02 `pass` | §4.1 `Tab` 行 |
| `Enter` | 05/`EditableCell` 既有行为：`preventDefault` + `coerceAndCommit` | §2.3 |
| `Escape` | 05/`EditableCell` 既有行为：`preventDefault` + `handleCancel`；01 的 `Escape` 分支因 `editingCell !== null` 而 return，**不会**顺带清选区 | 01 §4.3 + §2.3 |
| `mod+A` | 浏览器原生「全选输入框文本」 | 若注册成 `scope: 'global'` 会**抢掉**输入框的全选文本，这是本册不注册进 `useKeyboardShortcuts` 的第二个理由 |

**如何利用「`scope: 'table'` 在输入框内被跳过」的既有语义**：见 §4.0 序 3 的说明——**复刻语义，不复刻注册方式**。具体做法是 02 的早退条件必须是 `isEditableTarget(e.target)`（01 提供的同一份判定，与 `useKeyboardShortcuts` 的 `inField` 同义），而不是「`e.target === 容器`」。只判 `e.target === 容器` 会漏掉**从编辑器输入框冒泡上来的 keydown**（输入框是行容器的后代，事件必然冒泡到容器的 `onKeyDown`）——这正是最容易被写错的一处。

**另一个必须写进实现注释的点**：01 §4.4 已禁止在 `pointerdown` 里对编辑器 `input` 调 `scrollEl.focus()`（会让 `onBlur` 提交数据）。键盘侧同理：`Enter`/`F2` 只在网格态调用 `onCellDoubleClick`，**不得**在调用后再手动 `.focus()` 输入框；输入框的聚焦由既有 `CellRenderer`/`EditableCell` 挂载后的 autofocus 逻辑决定。

### 4.4 Q4：焦点裁定（**结论**）

**裁定：采用 roving tabindex —— 网格滚动容器 `tabIndex={0}`（唯一 Tab 停靠点），行容器 `tabIndex={-1}`。**

三要素：

| 要素 | 内容 |
| --- | --- |
| **进入条件** | ① 用户 `Tab` 到容器（`data-dt-surface` + `tabIndex={0}`）；② 鼠标点单元格后由 01 调 `scrollEl.focus()`（01 §4.2 已写）；③ `Enter`/`F2` 提交/取消后焦点回到容器。进入后若有 `cursor`（cell/row 模式）则渲染 `data-dt-active`，`aria-activedescendant` 指向该格 id（id 生成归 01，§4.4 已定用 `useId()` 前缀，**不含列名**） |
| **状态内行为** | 所有键在**容器**这一个监听点上处理（§3.3 唯一入口纪律）；行容器与单元格内**不再有** Tab 停靠点；焦点永不因按键逃到行号槽按钮 |
| **退出跃迁** | `Tab`/`Shift+Tab` → `consume` + `preventDefault` 后**由浏览器把焦点移出网格**（我们只挡住网格内部的 Tab 目标，不主动改焦点，避免自造焦点跳转链）；点击网格外元素 → 焦点自然移出，**选区与光标记忆保留**（§4.7）；`Escape` → 01 清选择但**不**移出焦点（焦点仍在容器上，符合 ARIA 复合控件惯例） |

**对既有可访问性断言与 E2E `Tab` 行为的影响（必须一并落地，否则测试会红）**：

| 受影响项 | 今天 | 改后 | 处置 |
| --- | --- | --- | --- |
| `src/components/DataTable/__tests__/VirtualBody.test.tsx`（2 处） | `container.querySelector('[tabindex="0"]')` 取行 `div` | 行 `div` 变 `tabIndex={-1}`，该选择器会命中**容器**（或为 `null`） | 改为 `container.querySelector('[data-dt-row-view]')` |
| `src/components/DataTable/__tests__/DataTable.test.tsx`（1 处） | 同上 | 同上 | 同上 |
| `e2e/specs/detail-panel.ts`（1 处） | `span.closest('[tabindex="0"]')` | 行不再是唯一候选 | 改为 `span.closest('[data-dt-row-view]')`（语义等价：从格内元素上溯到行容器） |
| `e2e/specs/ops-process-server.ts`（2 处） | `cell.closest('[tabindex="0"]')`、`document.querySelectorAll('[tabindex="0"]')` | 同上 | 改为 `[data-dt-row-view]`（两处都改） |
| `e2e/specs/table-batch-ops.ts` `TC-TABLE-013` | `document.querySelector('table, [role="grid"]')`，**匹配不到任何元素** | 容器有了 `data-dt-surface` | 改为 `[data-dt-surface]`（§5 Step 7） |
| 键盘可访问性（真收益） | 每行一个 Tab 停靠点：100 行 = 按 100 次 `Tab` 才能穿过去 | 1 个停靠点，行内用方向键 | 这是本次改动的**主要收益**，必须在 PR 描述里写明 |
| 行内的可聚焦元素 | 行号槽 `<button>` 实测是 Tab 停靠点（`e2e/specs/ops-process-server.ts` 用 `cell.closest('[tabindex="0"]').click()` 间接依赖行可聚焦） | 行号槽仍是停靠点 ⇒ 容器 `Tab` 必须 `preventDefault` 才能真的离开网格 | 见 §4.1 `Tab` 行；`data-dt-row-view` 迁移后 `ops-process-server.ts` 的点击路径改走行容器，不再依赖 `tabindex` |

**与 01 §4.4 的关系（我是裁定方，给出对齐结论）**：01 §4.4 写「既有的行 div `tabIndex={0}` **本册不改**（既有单测用 `[tabindex="0"]` 找行），由此产生的『同一网格两个 Tab 停靠点』记入未决问题 Q1」。**02 作为 Q1 的裁定方，裁定改为 roving tabindex，并一次性完成 5 个既有定位点的迁移**（3 个单测文件位置 + 2 个 E2E spec，共 6 处表达式）。01 需要在下一版把 §4.4 的这句话改写为「行 `tabIndex={-1}`，由 02 落地并迁移既有定位点」，**两册不得各写一套**。

### 4.5 Q-IME：输入法组合期间（**结论**）

**裁定：组合期网格一律 `pass`，判定用 `isImeComposingKey(e)`（`isComposing === true` 或 `key === 'process'`），且该判定必须排在 §4.0 优先级表的第 2 位——先于编辑态判定。**

| 阶段 | 行为 | 理由 |
| --- | --- | --- |
| 组合开始（输入框内） | 网格不做任何事；候选窗由浏览器与输入框处理 | 组合期的 `keydown` 会**冒泡到容器**（输入框是容器后代），若只按 `editing === true` 判定，一旦 05 的实现让编辑器处于「半开」态（如双击后尚未挂载 input）就会漏判 |
| 组合期按 `Enter` | `pass`：由输入法**选字**，不得进入编辑/提交单元格（`e.key` 在 Chromium 上报 `'Process'`，`isComposing` 也为真，两条都覆盖） | `EditableCell` 今天没有这个守卫（§2.3），所以「网格态误吞 `Enter`」与「编辑器误提交候选串」是同一类缺陷的两侧 |
| 组合期按方向键 / `Tab` | `pass`：候选窗内移动选择 / 切换候选 | — |
| 组合期按 `Escape` | **第一次** `pass`：取消候选（浏览器行为）；**第二次**（`isComposing === false`）才归 01 清选区 | 用户预期「`Escape` 先关候选、再取消选择」；必须写进 journey 测试的残缺中间态 |
| 组合期结束（`compositionend`） | 网格不额外触发任何状态跃迁；选区不受影响 | 网格不监听 `compositionend` |
| 组合期点其它格 | 照常扩区/重设选区（指针路径不归本册，01 §4.5 已定）；但 `pointerdown` 落在 `isEditableTarget` 为真的元素上时忽略 | 与 01 一致 |

**`isComposing` 的取值口径**（01 §3.10 已给 `isComposingEvent`）：原生事件读 `event.isComposing`，React 合成事件读 `event.nativeEvent.isComposing`。02 的 `toGridKeyEventLike` 负责摊平，**禁止**在业务分支里各写一遍 `e.nativeEvent?.isComposing`。

### 4.6 Q5：`Cmd/Ctrl+A` 语义裁定（**结论**）

**裁定：`Cmd/Ctrl+A` = 整行全选，实现体就是既有的 `toggleSelectAll`，并把选区切到 `row` 模式。**

**理由（五条，按权重）**：

1. **与既有测试名一致**：`TC-TABLE-013` 的名字就是「Ctrl+A 应选中当前页所有行」；把它改成「全选单元格」等于在同一个键上推翻已经写进 E2E 的意图命名（而该用例的**名字是唯一可信的部分**，它的断言是空的，见 §2.5）。
2. **数据网格惯例**：DBeaver / Navicat / TablePlus 一类数据库客户端里 `Cmd+A` 的语义都是「选中当前结果集全部行」，因为紧随其后的动作是导出/删除/复制整行；「全选单元格」是电子表格（Excel/Sheets）惯例。
3. **与行级操作的取值域对齐**：契约 §4.2 第 3 条把行级操作的取值域定为 `rowsCoveredBySelection(state)`。`Cmd+A` 走 `toggleSelectAll` 后 `mode === 'row'`，`rowsCoveredBySelection` 直接返回 `selectedRows`——**零派生、零歧义**。反之若裁定为「全选单元格」，则 `mode === 'row'` 的全选行为要退化成 `cell` 模式下的整页区域，`anchor/focus` 指向整页矩形，用户按 `Delete` 会因 `selectedRows` 为空而**什么都不发生**——恰恰是「以为选中了却删不掉」的经典事故。
4. **`Cmd+A` 是 `cell` 模式的逃生舱**：用户在 `cell` 模式下不可能通过方向键获得「整行选择」，若 `Cmd+A` 也只做单元格区域，则「先扩区再整行删」只能靠鼠标点行号槽，键盘用户被卡死。
5. **成本**：`toggleSelectAll` 已存在，本裁定不新增任何 store 动作（§3.3 `onToggleSelectAll` 就是它的透传）。

**规范性细节**：

- 顺序：先 `onToggleSelectAll()`，再 `onGridSelectionChange(EMPTY_ROW_MODE_SELECTION)`（`anchor`/`focus` 为 `null`、`extraRanges` 为 `[]`、`mode: 'row'`，恒等于 01 的不变量 I2）。01 的 `applyGridSelection` 只在 `mode === 'cell'` 时清 `selectedRows`，因此 `mode: 'row'` 的写入**不会**回吞 `toggleSelectAll` 的结果（与调用顺序无关，但实现按上面顺序写更好读）。
- `Cmd+A` **必须 `preventDefault`**：容器是普通 `div`，不 `preventDefault` 会让浏览器执行「全选整个页面文本」，松手后留下一片全选高亮（今天正是这个行为）。
- `Cmd+A` 在执行前**不**检查 `mode`：`none`/`cell`/`row` 三态下都是「全选整行」，这是它作为逃生舱的前提。
- `row` 模式的进入条件因此增加一条键盘入口（`Cmd/A`）；这是对 01 §4.2 跃迁表的**追加一行**，不触及 00-contracts §4.2 的两条「必须遵守」（继续用既有 `selectedRows`/`toggleSelectAll`）。需在 01 下一版同步（K-5）。

**`TC-TABLE-013` 该怎么补断言（逐条给出）**：

```ts
it('TC-TABLE-013: Ctrl+A 应选中当前页所有行', async () => {
  // ① 把焦点真正放到网格容器上（今天的选择器匹配不到元素，这是假绿的根因）
  await browser.execute(() => {
    const grid = document.querySelector('[data-dt-surface]');
    if (grid) (grid as HTMLElement).focus();
  });

  // ② 先断言前提成立：焦点确实在网格内，且此刻还没有行被选中
  expect(await browser.execute(() => document.activeElement?.hasAttribute('data-dt-surface') ?? false)).toBe(true);
  const rowsBefore = await browser.execute(() => Number(
    document.querySelector('[data-dt-selection-rows]')?.getAttribute('data-dt-selection-rows') ?? '0',
  ));
  expect(rowsBefore).toBe(0);

  await browser.keys(['Meta', 'a']);
  await browser.pause(200);

  // ③ 断言「整行全选」而不是「全选单元格」：rows = 本页行数，cells = 0
  const { rows, cells } = await browser.execute(() => {
    const el = document.querySelector('[data-dt-selection-rows]');
    return {
      rows: Number(el?.getAttribute('data-dt-selection-rows') ?? '-1'),
      cells: Number(el?.getAttribute('data-dt-selection-cells') ?? '-1'),
    };
  });
  const pageRowCount = await browser.execute(() => document.querySelectorAll('[data-dt-row-view]').length);
  expect(rows).toBeGreaterThan(0);
  expect(rows).toBeLessThanOrEqual(pageRowCount);   // 虚拟滚动下只渲染可见行，用 ≤ 而非 =
  expect(cells).toBe(0);                            // row 模式 ⇒ gridSelection 为空 ⇒ 单元格计数 0

  // ④ 断言整行高亮真的渲染出来了（DOM 事实，不靠截图）
  const selectedRowMarkers = await browser.execute(
    () => document.querySelectorAll('[data-dt-row-view][data-dt-selected="true"]').length,
  );
  expect(selectedRowMarkers).toBeGreaterThan(0);

  // ⑤ 再按一次必须清空（toggleSelectAll 的既有语义）
  await browser.keys(['Meta', 'a']);
  await browser.pause(200);
  expect(await browser.execute(() => Number(
    document.querySelector('[data-dt-selection-rows]')?.getAttribute('data-dt-selection-rows') ?? '-1',
  ))).toBe(0);

  await captureJourneyStep('table-select-all');
});
```

**依赖前置**：`data-dt-selection-rows` / `data-dt-selection-cols` / `data-dt-selection-cells` 三个属性由 01 输出（01 §3.7）；`data-dt-row-view` 与「行容器输出 `data-dt-selected`」由本册与 01 共同补（本册负责 `data-dt-row-view`；行级 `data-dt-selected` 的输出来源需 01 确认，否则第 ④ 步退化为断言 `rows > 0`）。**若 01 不提供行级 `data-dt-selected`，第 ④ 步改用行号槽的既有视觉类不再是唯一依据——此时本册裁定：行容器必须补 `data-dt-selected`（值为 `"true"`），因为 E2E 不能靠 class 名断言**（`data-*` 解耦纪律，AGENTS.md「交互与补全开发原则」）。

### 4.7 Q6：失焦后选择是否保留；鼠标与键盘混合

| 场景 | 行为 | 依据 |
| --- | --- | --- |
| 焦点离开网格（点击工具栏/侧栏） | **保留** `GridSelection`、`selectedRows`、`lastSelectedIndex`，以及 02 的**光标记忆**（`useGridKeyboardNav` 内部 `useRef<CellCoord \| null>`，随 `activeCoord` 每次渲染刷新） | 01 §4.4 已裁定「焦点离开网格不清空选择」（用户可能去点工具栏的『删除行』『导出』）；本册追加「光标记忆同样保留」 |
| 重新聚焦容器 | `cell` 模式：光标 = `selection.focus`（store 是唯一真相，**不用**记忆值覆盖）；`row` 模式：光标行 = `lastSelectedIndex`；`none` 模式：光标 = `resolveEntryCursor(记忆值, …)` | 记忆值只在 `none` 模式下使用——这正是 `Escape` 后按方向键重新进入时不该跳回首格的解法 |
| 记忆值失效（列被隐藏 / 行数变少 / 翻页取数） | `clampGridCursor` 返回 `null` ⇒ 回退首格；翻页时 01 的 `reconcileGridSelection` 已把 `mode` 归 `none`，因此回退路径可覆盖 | 契约 §4.3 推论；与 01 §3.3 `reconcileGridSelection` 同步调用 |
| 窗口失焦（`blur` 到其它窗口） | 同「焦点离开网格」；**不得**在 `blur` 里清选择 | 同上 |
| `Shift+单击` 后用键盘扩区 | `anchor` **保持不变**（01 §4.2：`Shift+单击` 只改主区另一端），随后 `Shift+↓` 从**原 anchor** 继续扩到新光标 ⇒ 区域连续增长，不会从点击处重新起算 | 这是混合操作最容易写错的一条：若 `Shift+单击` 顺手把 `anchor` 重置成被点格，键盘接手后区域会「跳」 |
| `Cmd/Ctrl+单击`（`extraRanges`）后用 `Shift+方向键` | `extraRanges` **保留**（与 01 对 `Shift+单击` 的规定一致：只改主区） | 01 §4.2 |
| `Cmd/Ctrl+单击` 后按**无修饰**方向键 | 主区塌陷为单格（锚点重置为新光标），`extraRanges` **清空** | 与「无修饰单击重设主区」对称，否则会出现「键盘动了一格，别处的附加区还亮着」的幽灵选区 |
| 拖拽中途按方向键 | `consume`（no-op）；指针路径是唯一权威，松手后主区以拖拽结果为准 | 01 §4.2 拖拽期间「只更新本地会话的 focus，不改 store」 |
| 右键菜单打开时按方向键 | 菜单是 Web Context Menu（浏览器级），焦点已不在容器 ⇒ 02 收到的是 `pass` | 无需特殊处理 |

---

## 5. 实现步骤

> 每步格式：**改哪个文件 / 加什么符号 / 为什么 / 怎么自测**。顺序即依赖顺序，Step 1 完成前不要动 React 层。

### Step 1 —— 纯逻辑地基

- **文件**：`src/lib/gridKeyboard.ts`（新增）。
- **符号**：§3.1 全部导出（`GridKeyEventLike`、`GridKeyIntent`、`GridKeyboardContext`、`GridKeyCommand`、`EMPTY_ROW_MODE_SELECTION`、`toGridKeyEventLike`、`isImeComposingKey`、`readGridKeyIntent`、`resolveKeyboardCursor`、`resolveEntryCursor`、`resolveGridKeyCommand`、`resolvePageDelta`、`gridCursorEquals`、`clampGridCursor`）。
- **为什么**：键盘语义必须能脱离浏览器单测（AGENTS.md「状态机思维」+「连续旅程测试」要求）；把语义集中在纯函数里，React 层退化为「读事件 → 算命令 → 调既有回调」，改动面最小。
- **怎么自测**：`npx vitest run src/lib/__tests__/gridKeyboard.test.ts`（Step 6 的用例）全绿；`pnpm typecheck` 退出码 0；人工确认文件里 `grep -n 'any'` 无命中、无 `import React`、无 `useState`。

### Step 2 —— 容器成为焦点宿主

- **文件**：`src/components/DataTable/DataTable.tsx`（修改；起点 628 行）。
- **符号**：承载虚拟滚动的容器 `div` 上新增 `data-dt-surface="true"`、`tabIndex={keyboardNavigation ? 0 : -1}`、`aria-label`（走 i18n）、`aria-activedescendant`（值来自 01 的 `useGridSelection().activeDescendantId`）、`data-dt-selected` 之外的 `data-dt-surface`（01 已定）；新增 prop `keyboardNavigation` 与 `onRequestScrollIntoView`（§3.4）。**不新增** `role`（K-7）。
- **为什么**：键盘导航必须有唯一的焦点与唯一的事件落点；今天的网格**完全没有**焦点宿主（§2.4 证实 `role`/`tabIndex` 均不存在），这是所有键盘能力的前置条件。
- **怎么自测**：组件测试里 `container.querySelector('[data-dt-surface]')` 非空且 `keyboardNavigation` 缺省时 `tabIndex` 为 `-1`（保证缺省行为逐位等于今天）；`pnpm typecheck`。

### Step 3 —— 行容器退出 Tab 序列（焦点裁定落地）

- **文件**：`src/components/DataTable/VirtualBody.tsx`（修改；起点 207 行）。
- **符号**：行容器 `div` 的 `tabIndex={0}` → `tabIndex={-1}`；新增 `data-dt-row-view={vRow.index}`；行容器新增 `data-dt-selected={selected ? 'true' : undefined}`（§4.6 ④ 的断言依据）。
- **为什么**：§4.4 的裁定。`data-dt-row-view` 的名称必须与 `data-dt-row` 不同（§3.5 备注：同名会破坏 `closest('[data-dt-row]')` 的单元格反查）。
- **怎么自测**：`npx vitest run src/components/DataTable/__tests__/VirtualBody.test.tsx` —— 该文件里 2 处 `[tabindex="0"]` 先按 Step 6 迁移到 `[data-dt-row-view]`，再跑绿；同时断言 `container.querySelectorAll('[tabindex="0"]')` 的**行**命中数为 0。

### Step 4 —— 删除行容器上被取代的键盘分支

- **文件**：`src/components/DataTable/VirtualBody.tsx`（修改）。
- **符号**：移除行容器的 `handleKeyDown`（`Enter`/`' '` → `onRowSelect`）与 `onKeyDown` 绑定；`handleClick`、`handleSelectButtonClick`、`handleRowDoubleClick` **保持不变**。
- **为什么**（两个必须删的理由）：① `Enter` 的归属已裁定给「进入编辑」（§4.1），保留旧分支会产生两个语义；② 行变为 `tabIndex={-1}` 后它不再接收独立的键盘事件，**唯一还能触发它的情况是焦点在行号槽按钮上按 `Enter`** —— 此时 `<button type="button">` 的原生 `click` 与冒泡到行容器的 `onKeyDown` 会**各调一次 `onRowSelect`**（一次单击、一次 `onRowSelect` 无 opts），形成难以复现的双触发。删除分支即根除。
- **怎么自测**：组件测试用例「焦点在行号槽按钮上按 `Enter`，`onRowSelect` 恰好被调用 1 次、参数为 `{ rowIndex, undefined }`」；再用同样手段按 `Space`，断言仍为 1 次。

### Step 5 —— React 接线

- **文件**：`src/hooks/useGridKeyboardNav.ts`（新增，§3.3）；`src/hooks/useGridSelection.ts`（修改，01 的新增文件）；`src/components/DataTable/DataTable.tsx`（修改，把 props 透传给 `VirtualBody` → 01 的 `GridCell`）。
- **符号**：`useGridKeyboardNav` 全部导出；01 的 `surfaceProps.onKeyDown` 内**在最前面**调用 `keyboardNav.handleKeyDown(e)`；容器上把 `data-dt-active` 透传到光标格（01 的 `GridCell` 接收 `active: boolean`，本册新增该 prop 的传递，**渲染判定仍由 props 派生**）。
- **为什么**：唯一入口纪律（§3.3）；02 不新增 store action，全部动作来自 01/既有 props。
- **怎么自测**：组件测试「容器上按 `ArrowDown` 后 `onGridSelectionChange` 被调用一次且 `focus` 行 +1」；「按 `mod+z` 断言 `onGridSelectionChange` 未被调用且事件未被 `preventDefault`」。

### Step 6 —— 测试与既有定位点迁移

- **文件**：`src/lib/__tests__/gridKeyboard.test.ts`（新增）、`src/components/DataTable/__tests__/gridKeyboardJourney.test.tsx`（新增）、`src/components/DataTable/__tests__/VirtualBody.test.tsx`（修改 2 处 `[tabindex="0"]`）、`src/components/DataTable/__tests__/DataTable.test.tsx`（修改 1 处）、`e2e/specs/detail-panel.ts`（修改 1 处）、`e2e/specs/ops-process-server.ts`（修改 2 处）。
- **为什么**：焦点裁定改变了行容器的 `tabIndex`，这些定位表达式若不迁移，测试会以「选择器为 null」的形式失败（而不是以功能失败的形式），且这类失败最容易被误判为「测试过时」而被随手删掉。
- **怎么自测**：`npx vitest run`（Host 全量）退出码 0；`pnpm e2e:minimal` 中 `detail-panel` / `ops-process-server` 用例全绿。

### Step 7 —— E2E：把假绿改成真断言 + 新增键盘旅行

- **文件**：`e2e/specs/table-batch-ops.ts`（`TC-TABLE-013` 按 §4.6 的逐条断言重写）、`e2e/specs/table-keyboard-nav.ts`（新增）。
- **符号**：无源码符号；新用例名见 §9。
- **为什么**：`TC-TABLE-013` 今天是三条假绿因素叠加（选择器匹配不到、无处理函数、无断言），必须一次性补齐；键盘导航是纯交互功能，组件测试覆盖不到「真实浏览器里 `Meta+a` 与焦点、虚拟滚动、行号槽按钮的交互」。
- **怎么自测**：`pnpm e2e:minimal`；**先验证假绿已被消除**——把 `Cmd+A` 的处理分支临时注释掉再跑一次 `TC-TABLE-013`，**必须变红**（这一步是本次改动的验收关键，不做就等于没验证）。

### Step 8 —— i18n 与文档收尾

- **文件**：`src/locales/en/query.ts`（修改，§8）。
- **为什么**：快捷键提示文案是用户可见字符串，必须入库；`en.ts` 只是一行再导出，改它无效。
- **怎么自测**：`pnpm typecheck`；人工确认新增 key 全部被至少一处引用（不得留死 key）。

---

## 6. 文件级改动清单

| 文件 | 新增/修改 | 职责 | 预估行数 | 是否触及 800 行上限 |
| --- | --- | --- | --- | --- |
| `src/lib/gridKeyboard.ts` | **新增** | 键→意图→命令的纯逻辑、光标派生与夹取 | +190 | 否（新增文件，余量充足） |
| `src/hooks/useGridKeyboardNav.ts` | **新增** | React 接线：事件摊平、光标记忆、命令分发 | +130 | 否 |
| `src/hooks/useGridSelection.ts` | 修改（01 的新增文件） | 在 `surfaceProps.onKeyDown` 最前面调用 02 的 handler（**只加 3~5 行**） | +5 | 取决于 01 的预估；本册只加 5 行，不得把 02 的逻辑写进来 |
| `src/components/DataTable/DataTable.tsx` | 修改 | 容器 `data-dt-surface`/`tabIndex`/`aria-*`、两个新 prop 透传 | **628 → ~668** | **未触及，但余量只剩约 132 行**：本册只允许加接线，**禁止**把键盘语义、命令分发、journey 辅助函数写进这个文件 |
| `src/components/DataTable/VirtualBody.tsx` | 修改 | 行 `tabIndex={-1}`、`data-dt-row-view`、行级 `data-dt-selected`、删除旧 `handleKeyDown` | 207 → ~215 | 否 |
| `src/components/DataTable/EditableCell.tsx` | **不触及** | IME `Enter` 守卫归 05（01 §4.5 已登记） | 0 | 否（119 行，保持不动以避免与 05 撞车） |
| `src/windows/connection/TableView.tsx` | 修改（仅传 prop） | 把 `keyboardNavigation` 与 `onRequestScrollIntoView` 传下去 | **770 → ~774** | **未触及，但余量只剩约 26 行**：这是本册**不把键盘接线放在这一层**的硬理由。若实现方发现需要在此加超过 5 行，必须停下来重新裁定（拆文件）而不是继续加 |
| `src/locales/en/query.ts` | 修改 | 8 个快捷键提示 key（§8） | 424 → ~432 | 否 |
| `src/lib/__tests__/gridKeyboard.test.ts` | **新增** | 单测（§9.1） | +330 | 否 |
| `src/components/DataTable/__tests__/gridKeyboardJourney.test.tsx` | **新增** | 连续旅程测试（§9.3） | +260 | 否 |
| `src/components/DataTable/__tests__/VirtualBody.test.tsx` | 修改 | `[tabindex="0"]` → `[data-dt-row-view]`（2 处）+ 新增「行不是 Tab 停靠点」断言 | +25 | 否 |
| `src/components/DataTable/__tests__/DataTable.test.tsx` | 修改 | 同上（1 处） | +10 | 否 |
| `e2e/specs/table-batch-ops.ts` | 修改 | `TC-TABLE-013` 补断言（§4.6） | +35 | 否 |
| `e2e/specs/table-keyboard-nav.ts` | **新增** | 键盘旅行 E2E（§9.4） | +150 | 否 |
| `e2e/specs/detail-panel.ts` | 修改 | 定位表达式迁移（1 处） | +2 | 否 |
| `e2e/specs/ops-process-server.ts` | 修改 | 定位表达式迁移（2 处） | +4 | 否 |

**规模红线**：`DataTable.tsx`（628）与 `TableView.tsx`（770）**都不允许**因本册越过 800 行。本册的解法是把全部语义压进 `src/lib/gridKeyboard.ts`（无 React 依赖）与 `src/hooks/useGridKeyboardNav.ts`（新文件），两个既有大文件只留接线。

---

## 7. 边界与异常清单

| # | 场景 | 期望行为 |
| --- | --- | --- |
| E1 | **空表**（`rowCount === 0`） | 所有导航键 `consume`（no-op，不抛错）；`Enter`/`F2` `ignored`；`Cmd+A` 仍调用 `toggleSelectAll`（既有语义在空集上是 no-op）并**允许**把 `mode` 切到 `row`（不显示任何高亮，I2 允许 `selectedRows` 为空）；容器**仍可聚焦**（`data-dt-surface` 存在，`tabIndex={0}`），保证键盘用户不会「卡在一个按不动的东西上」 |
| E2 | **单元格为空**（`NULL` / `''`） | 焦点与选择不受值影响（`data-dt-selected` / `data-dt-active` 只看坐标，不看值）；`Enter`/`F2` 走既有 `onCellDoubleClick`，由它决定「空值进编辑得到空串」还是别的既有语义，02 不做判定 |
| E3 | **编辑器打开**（`editingCell !== null`） | 除 `Tab`（05 接管）外全部 `pass`；网格**不得**因按键改变选区；`Escape` 由编辑器取消编辑，**不得**同时清空选区（01 已用 `editingCell !== null` 守卫）；`Enter` 由编辑器提交，**不得**因此移动光标格（提交后的光标移动是 05 的可选项，02 不代劳） |
| E4 | **上/左边界按无修饰方向键** | `moveCellFocus` 返回 `null` ⇒ `consume`（no-op）。**严禁**塌陷成「以边界格为单格」——那会让「已选中 3×3 区域，按一下 `↑` 想看看能否再扩」变成区域消失 |
| E5 | **下/右边界按无修饰方向键** | 同上；**严禁**跨页取数（契约 §4.3 要求取数即清空全部选择） |
| E6 | **`row` 模式在上边界按 `↑`** | no-op；**严禁**调 `onRowGutterSelect(0, { range: false })`（既有 `selectRow` 对「唯一选中行」是清空语义，会把选择整个抹掉） |
| E7 | **IME 组合期间**（`isComposing` 或 `key === 'process'`） | 全部 `pass`；`Enter` 用于选字不得进入编辑；第一次 `Escape` 只取消候选；方向键在候选窗内移动。判定顺序必须早于编辑态判定（§4.5） |
| E8 | **跨页移动**（翻到下一页 / 上一页） | 导航键本身**不**触发取数；翻页由既有分页控件完成，且 `patchPanelForReload` / `commitFetchedPage` / `invalidateCachedData` 三处收敛点必须已清空 `gridSelection` 与 `selectedRows`/`lastSelectedIndex`（契约 §4.3 推论，归 01）；02 叠加：翻页后 `clampGridCursor` 判失效 ⇒ 光标回退首格，`mode === 'none'` 时按方向键从首格进入 |
| E9 | **列被隐藏**（焦点列不再在 `columnOrder` 中） | `←`/`→` 只在可见列之间移动，被隐藏列永不成为焦点；若焦点列被隐藏，01 的 `reconcileGridSelection` 把 `mode` 归 `none`，02 的 `clampGridCursor` 同步返回 `null`（两处必须一起调，否则 `none` 模式下按方向键会回到一个**已隐藏**的列而 `coordAt` 返回 `null`） |
| E10 | **只读连接** | 导航与选择**全部可用**（只读 ≠ 不可选）；`Enter`/`F2` 复用既有 `onCellDoubleClick`，只读提示或拒绝进入编辑由它决定，02 **不新增任何只读分支**（零硬编码的体现形式） |
| E11 | **单行表** | `↑`/`↓` 全部 no-op（含 `row` 模式）；`PgUp`/`PgDn` 夹到同一行 ⇒ `consume`；`mod+Home`/`mod+End` 落在同一格 |
| E12 | **单列表** | `←`/`→` no-op；`Home`/`End` 落在同一格；`mod+←`/`mod+→` 同 |
| E13 | **超长值 / 多字节字符 / emoji** | 方向键按**单元格**移动，不进入「文本光标」模式（容器是焦点宿主，格内文本不是可编辑目标）；`data-dt-active` 与值的字符数无关。多字节/emoji 值不得影响 `aria-activedescendant` 的 id（01 已定 id 只用 `r{rowIndex}c{columnIndex}`，不含列名与值） |
| E14 | **虚拟滚动行被卸载** | 光标格滚出视口后被卸载 DOM：`data-dt-active` 随该格一并消失（无 DOM 即无标记），但 store 里的 `focus` 不变；滚回后重新渲染即恢复标记。**禁止**把 `data-dt-active` 存进任何局部 state 或 DOM class（01 §4.6 同一纪律） |
| E15 | **同屏两个网格** | 事件只落在被聚焦的那个容器上（容器级 `onKeyDown`），另一个网格的选区不受影响；`Cmd+A` 只作用于焦点网格。这正是本册不把键注册进 `window` 级 `useKeyboardShortcuts` 的第三个理由 |
| E16 | **拖拽进行中按方向键** | `consume`（no-op）；不中断 01 的拖拽会话，松手后以拖拽结果为准 |
| E17 | **`Space`** | `pass`：容器作为可滚动元素，`Space` 触发浏览器翻页滚动。**不得**注册成「选整行」（与 `Enter` 的旧占用一并退役，见 K-6） |
| E18 | **`Cmd+A` 后立刻 `Delete`** | `mode === 'row'` 且 `selectedRows` 非空 ⇒ 既有 `handleDeleteKey` 生效（这是用户真正想要的效果）；反之 `cell` 模式下 `Delete` 必须 no-op（§4.2 第 3 条），**不得**降级成「删除区域覆盖的行」 |
| E19 | **`Escape` 在 `mode === 'none'`** | 幂等：不发任何状态变更（01 的实现应早退），02 侧因 `pass` 而不参与 |
| E20 | **`Cmd+Home`/`Cmd+End`/`PgUp`/`PgDn` 的默认行为** | 必须 `preventDefault`（否则容器会滚动、macOS 上还会触发页面级滚动），滚动只能由 10 的 `onRequestScrollIntoView` 驱动 |

---

## 8. i18n key 清单

**领域包**：`src/locales/en/query.ts`（**不是** `core`、**不是** `en.ts`——`en.ts` 只有一行 `export { default } from './en/index'`，改它无效）。该文件的既有形态是**扁平点号字符串键**（如 `'tableView.loadingData'`），因此新键沿用同一形态的字符串键，**不新建子对象、不新建域名、不动其它语言**。

| 完整 key | 英文值 | 用途 | 引用点 |
| --- | --- | --- | --- |
| `tableView.a11y.gridLabel` | `Data grid` | 容器 `aria-label` | `DataTable.tsx` 容器 |
| `tableView.shortcut.hint` | `Arrow keys move, Shift+Arrow extends, Enter edits` | 快捷键提示（帮助浮层 / `aria-describedby` 目标） | `DataTable.tsx` 提示元素 |
| `tableView.shortcut.enterEdit` | `Enter or F2 to edit the focused cell` | 提示条目 | 同上 |
| `tableView.shortcut.extendSelection` | `Shift+Arrow keys extend the selection` | 提示条目 | 同上 |
| `tableView.shortcut.moveByPage` | `PageUp/PageDown move one screen` | 提示条目 | 同上 |
| `tableView.shortcut.moveToEdge` | `Home/End go to row edge, Cmd+Home/End to grid edge` | 提示条目 | 同上 |
| `tableView.shortcut.selectAllRows` | `Cmd/Ctrl+A selects all rows` | 提示条目 | 同上 |
| `tableView.shortcut.tabLeavesGrid` | `Tab leaves the grid` | 提示条目 | 同上 |

**纪律**：① 只改英文侧领域包；② 键名不得包含 `Cmd`/`Ctrl`/`⌘` 之外的实现细节（文案里写「Cmd/Ctrl」是既有惯例的延续，避免为 macOS/Windows 各开一套 key）；③ 新增 key 必须至少被一处引用（Step 8 自测）；④ **不得**把「当前行数」之类的动态数字写进 i18n（01 的 `data-dt-selection-rows` 是 DOM 属性，不是文案；若后续要文案：「若项目 i18n 未启用插值，改为静态前缀 + 数字分列渲染」，不得硬编码拼接）。

---

## 9. 测试清单

### 9.1 单测 `src/lib/__tests__/gridKeyboard.test.ts`（纯逻辑）

| 用例名 | 断言要点 |
| --- | --- |
| `readGridKeyIntent: 方向键映射为 move 意图` | 四个方向各一条；`deltaRow`/`deltaColumn` 符号正确 |
| `readGridKeyIntent: 修饰键精确匹配` | `shift+↓` → `move{1,0}` 且 shift 语义由命令层处理；`alt+↓` → `pass`；`mod+c` → `pass`；`mod+z` → `pass`；`mod+a` → `select-all-rows` |
| `readGridKeyIntent: Editor 与 Delete 一律 pass` | `escape` / `delete` / `backspace` / `mod+c` / `mod+v` / `mod+x` / `mod+z` / `mod+shift+z` 全部 `pass` |
| `readGridKeyIntent: 编辑态下所有键 pass` | 遍历方向键/`Enter`/`Tab`/`Home`/`PgDn`，`ctx.editing === true` 时全部 `pass` |
| `readGridKeyIntent: 组合期优先于编辑态` | `isComposing === true` 且 `editing === false` ⇒ `pass`；`key === 'process'` ⇒ `pass` |
| `readGridKeyIntent: Tab 与 Shift+Tab 均为 consume 意图` | `leave-grid`；`Shift+Tab` 同 |
| `resolveGridKeyCommand: none 模式方向键进入 cell 并取入口光标` | 记忆值为 `{rowIndex: 7, columnName: 'name'}` 时进入该格；记忆值越界时进入首格；`selection.mode === 'cell'`、`anchor === focus`、`extraRanges` 为 `[]` |
| `resolveGridKeyCommand: 边界 no-op 不塌陷` | 已在 `(0, 首列)` 按 `↑` / `←` ⇒ `{ kind: 'consume' }`，返回的选区**等于入参选区** |
| `resolveGridKeyCommand: 无修饰移动塌陷主区并清 extraRanges` | 入参 `extraRanges` 非空 ⇒ 结果 `extraRanges` 为空；`anchor === focus === 目标格` |
| `resolveGridKeyCommand: Shift 移动保留 anchor 与 extraRanges` | `anchor` 与入参相同；`focus` 为新格；`extraRanges` 逐个相等 |
| `resolveGridKeyCommand: Shift 移动在 anchor === null 时建锚（残缺中间态）` | 不抛错、不丢事件：结果为单位置区域，`anchor === focus` |
| `resolveGridKeyCommand: row 模式 ↑↓ 走 row-select 且 range 由 shift 决定` | `↑` ⇒ `{rowIndex: n-1, opts:{multi:false, range:false}}`；`Shift+↑` ⇒ `range:true` |
| `resolveGridKeyCommand: row 模式边界同格不发命令` | 第 0 行按 `↑` ⇒ `{ kind: 'consume' }`，**不**产生 `row-select` |
| `resolveGridKeyCommand: PgUp/PgDn 步长为 viewportRowCount - 1` | `viewportRowCount = 1` ⇒ 步长 1（下限）；`= 20` ⇒ 19 |
| `resolveGridKeyCommand: Cmd+A 无论何种模式都返回 select-all-rows` | `none` / `cell` / `row` 三态各一条 |
| `resolveGridKeyCommand: Enter/F2 在 cell 模式取 focus、在 row 模式取 lastSelectedIndex × 首列、在 none 模式 ignored` | 三条；`row` 模式下 `lastSelectedIndex === null` ⇒ `ignored` |
| `resolveGridKeyCommand: Home/End/mod+Home/mod+End 的四个边界` | 用 `columnOrder` 顺序而非列名字典序断言（复现 01 §4.1 的排序陷阱：构造列名顺序与字典序相反的场景） |
| `clampGridCursor: 行越界/列已隐藏返回 null` | 三条 |
| `resolveEntryCursor: 记忆值无效时回退首格` | 列不存在、行越界、记忆为 `null` |
| `resolveKeyboardCursor: row 模式用 lastSelectedIndex × 首列` | 含 `lastSelectedIndex === null` ⇒ `null` |
| `EMPTY_ROW_MODE_SELECTION 满足不变量 I2` | `anchor === null && focus === null && extraRanges.length === 0 && mode === 'row'` |

### 9.2 组件测试（`src/components/DataTable/__tests__/`）

| 用例名 | 断言要点 |
| --- | --- |
| `VirtualBody: 行容器不再是 Tab 停靠点` | `[tabindex="0"]` 在行上命中 0 次；`[data-dt-row-view]` 命中行数 = 渲染行数 |
| `VirtualBody: 行号槽按钮按 Enter 只触发一次 onRowSelect` | Step 4 的回归断言（删除旧 `handleKeyDown` 的证据） |
| `VirtualBody: 行容器输出 data-dt-selected` | `selected` 为真时属性存在、值为 `"true"` |
| `DataTable: keyboardNavigation 缺省时容器不可 Tab 聚焦` | 缺省 `tabIndex === -1`、无 `data-dt-active`（逐位兼容今天） |
| `DataTable: 开启后容器可聚焦且方向键回写选区` | `onGridSelectionChange` 被调用一次，`focus` 行 +1 |
| `DataTable: 编辑器打开时方向键不改选区` | `editingCell !== null` ⇒ 回调零调用 |
| `DataTable: mod+z 不被网格吞掉` | 回调零调用，且 `defaultPrevented === false` |
| `DataTable: 容器按 Tab 时 preventDefault` | `defaultPrevented === true` |

### 9.3 连续旅程测试 `src/components/DataTable/__tests__/gridKeyboardJourney.test.tsx`（**必须有**）

> 纪律要求：模拟**完整击键序列**，逐步断言状态跃迁，**涵盖残缺中间态**；禁止只断言终态。

| 用例名 | 击键序列与逐步断言 |
| --- | --- |
| `journey: 鼠标起手 → 键盘扩区 → Cmd+A 收口 → Escape 复位` | ① 点 `(2, 'name')` ⇒ `mode='cell'`、`selectedRows` 空；② `Shift+↓` ⇒ `focus.rowIndex=3`、`anchor.rowIndex=2`；③ `Shift+→` ⇒ 列扩一列、`extraRanges` 仍空；④ `Cmd+单击 (5,'id')` ⇒ `extraRanges` 长度 1；⑤ `Shift+↓` ⇒ `extraRanges` **仍为 1**（残缺中间态的保留语义）；⑥ `↓`（无修饰）⇒ `extraRanges` 归零、主区塌陷；⑦ `Cmd+A` ⇒ `mode='row'`、`anchor===null`、`onToggleSelectAll` 被调用一次；⑧ `Escape` ⇒ `mode='none'` 且 `selectedRows` 空；⑨ `↓` ⇒ 回到**记忆光标**那一行（不是第 0 行） |
| `journey: Shift+单击接手键盘扩区时 anchor 不被重置` | ① 点 `(1,'name')`；② `Shift+↓` ⇒ 焦点 2、anchor 1；③ `Shift+单击 (6,'id')` ⇒ 主区 1..6；④ `Shift+↓` ⇒ 焦点 7、**anchor 仍为 1**（若实现把 anchor 重置成 6，此断言必红） |
| `journey: 进入编辑 → 输入 → Escape 取消 → Tab 提交` | ① `Enter` ⇒ `onCellDoubleClick` 调用一次；② 渲染输入框并键入；③ 输入框内按 `↓` ⇒ 选区**不变**（编辑态 `pass`）；④ `Escape` ⇒ 编辑取消、**选区仍非空**（证明 01 的 `editingCell` 守卫生效）；⑤ 再次 `Enter` + 输入 + `Tab` ⇒ 提交一次、光标格右移一格 |
| `journey: IME 组合期 Enter 选字不触发进入编辑` | ① 点单元格；② 派发 `keydown{key:'Process', isComposing:true}` ⇒ 零命令；③ `Enter` 且 `isComposing:true` ⇒ **零** `onCellDoubleClick` 调用；④ `Escape` 且 `isComposing:true` ⇒ 选区仍在；⑤ `Escape` 且 `isComposing:false` ⇒ 选区清空（同一序列内的两次 `Escape`，覆盖「第一次只关候选」的残缺中间态） |
| `journey: Delete 在 cell 模式是 no-op、在 row 模式删行` | ① `Shift+↓` 扩到 3 行 ⇒ 按 `Delete` ⇒ 行删除回调**零调用**（契约 §4.2 第 3 条的落地证据）；② `Cmd+A` + `Delete` ⇒ 行删除回调被调用一次且行集 = `selectedRows` |
| `journey: 列隐藏使焦点失效后方向键不回到幽灵列` | ① 焦点放在 `'name'`；② 隐藏 `'name'` 并触发 `reconcileGridSelectionForColumns`；③ 断言 `mode='none'`；④ `↓` ⇒ 入口光标落在 `columnOrder[0]`，**不是** `'name'` |
| `journey: 边界连按不塌陷` | ① 焦点 `(0, 'id')`；② 连按 5 次 `↑` 与 5 次 `←` ⇒ 每一步选区恒等（`gridSelectionEquals`）、无回调抖动（`onGridSelectionChange` 调用次数为 0） |

### 9.4 E2E（`e2e/specs/table-keyboard-nav.ts` 新增 + `table-batch-ops.ts` 修改）

| 用例名 | 断言要点 |
| --- | --- |
| `TC-TABLE-013: Ctrl+A 应选中当前页所有行`（**改写**） | §4.6 的 5 段断言；**必须验证假绿已消除**（临时移除处理分支后该用例必须变红） |
| `TC-KBD-001: 容器可聚焦并进入单元格模式` | 点 `[data-dt-surface]` 后 `document.activeElement` 具备 `data-dt-surface`；`Enter` 后出现 `[data-dt-editing="true"]`（01 的属性） |
| `TC-KBD-002: 方向键移动与 Shift 扩区` | `[data-dt-active]` 的 `data-dt-row`/`data-dt-col` 逐步变化；`[data-dt-selected="true"]` 计数随 `Shift+↓` 单调增加 |
| `TC-KBD-003: Tab 离开网格` | 焦点格连续 `Tab` 后 `activeElement` 不再位于 `[data-dt-surface]` 内；网格内 `[tabindex="0"]` 命中 0 次 |
| `TC-KBD-004: 编辑器内方向键不移动单元格光标` | 进入编辑后在输入框内按方向键；`[data-dt-active]` 的坐标不变 |
| `TC-KBD-005: PgDn 不触发取数` | 记下当前页首行值 → `PgDn` → 首行值不变（无 `setPage`）；`[data-dt-active]` 行号变大 |
| `TC-KBD-006: Cmd+A 后 Escape 再按方向键不跳回首行` | 覆盖 §4.7 的光标记忆语义 |

**E2E 通用纪律**：一律用 `data-dt-*` 定位（01 §4.4 + AGENTS.md「数据属性解耦」），禁止 `[tabindex]` 或几何坐标；每个用例结束调 `captureJourneyStep`（沿用 `table-batch-ops.ts` 的既有范式）。

---

## 10. 自查清单（实习生最容易做错的 5~10 条）

| # | 错误做法 | 正确做法 | 后果 |
| --- | --- | --- | --- |
| 1 | 在 `DataTable` 里再挂一个 `window.addEventListener('keydown')`，或把网格键注册进 `useKeyboardShortcuts`（`scope:'table'`） | 只通过 01 的 `surfaceProps.onKeyDown` 单入口分发；`useKeyboardShortcuts` 的「输入框内跳过」只被复刻为 `isEditableTarget` 判定 | 同屏两个网格互相抢键；`mod+a` 在工具栏焦点下也生效；`window` 级注册顺序还会让别的快捷键永远不触发（§2.1 第 4 条：命中即 return） |
| 2 | 早退条件写成 `e.target === container`（只认容器本体） | 用 `isEditableTarget(e.target)` + `editing !== null` | 编辑器输入框的 `keydown` 冒泡到容器，网格在编辑时抢走方向键/`Enter`，用户打字时单元格光标乱跳甚至误提交 |
| 3 | IME 守卫放在编辑态判定**之后**，或只检查 `isComposing` 不检查 `key === 'process'` | 组合期判定排在优先级第 2 位；两个条件都覆盖 | 中文用户按 `Enter` 选字时要么被吞、要么进编辑；`Escape` 一次同时关候选又清选区 |
| 4 | `Shift+单击` 时顺手把 `anchor` 重置成被点格 | `Shift+单击` 只改主区另一端，`anchor` 保持 | 鼠标选定区域后一按 `Shift+↓` 区域「跳」到点击处，用户以为选择丢了 |
| 5 | 边界按键时把选区塌陷为「以边界格为单格」 | 返回 `consume` + 原样返回入参选区 | 在边界多按一下方向键，3×3 选区整个消失；`row` 模式在上边界会把选择**清空**（既有 `selectRow` 的唯一选中行语义） |
| 6 | 把 `Cmd+A` 实现成「选中当前页所有单元格」（`cell` 模式整页区域） | 复用既有 `toggleSelectAll` + `mode='row'` | 用户 `Cmd+A` 后按 `Delete` 什么都不发生（`selectedRows` 为空），以为删掉了却仍在库里；且推翻 `TC-TABLE-013` 的既有语义命名 |
| 7 | 保留 `VirtualBody` 行容器的 `onKeyDown`（`Enter`/`Space` → `onRowSelect`），同时又让 `Enter` 进入编辑 | 删除该分支 | 焦点在行号槽按钮上按 `Enter`：原生 `click` + `onKeyDown` 各调一次 `onRowSelect`（双触发）；`Enter` 同时「选整行」和「进编辑」 |
| 8 | 把派生行集写回 `selectedRows`（如 `cell` 模式扩区时同步 `selectedRows`） | 行级作用域一律用 01 的 `rowsCoveredBySelection` 派生；`cell` 模式下 `selectedRows` 恒空 | 契约 §4.2 第 3 条明令禁止：出现「区域」和「整行选中」两套真相，Shift 连选与翻页重算必然打架；`cell` 模式下按 `Delete` 会意外删行 |
| 9 | 把选中态/光标态写进行的局部 state 或 DOM class（因为虚拟滚动会复用 DOM） | `data-dt-selected` / `data-dt-active` 完全由 props 派生 | 滚动后「旧选中态」被复用节点带到新行上，出现幽灵高亮；E2E 也无法稳定断言 |
| 10 | 为了让新测试通过而直接删掉 `[tabindex="0"]` 的老断言，或把 journey 测试写成「只断言终态」 | 迁移到 `[data-dt-row-view]` 并保留断言强度；journey 逐步断言每个中间态 | 删除的是「行是 Tab 停靠点」这一行为的**证据**，而焦点裁定恰恰改的就是它；只断言终态则残缺中间态（`anchor === null`、组合期两次 `Escape`、拖拽未抬手）全部漏测，正是 00/01 反复点名的缺陷成因 |

---

## 11. 未决问题

| # | 问题 | 建议选项（含推荐） |
| --- | --- | --- |
| K-1 | 网格内部是否需要「`Tab` 在单元格间横向移动」（电子表格惯例，`Tab` 到末列自动换行） | **推荐：不做**（v1 只做「`Tab` 离开网格」）。理由：容器是唯一 Tab 停靠点的 ARIA 复合控件惯例；且编辑态的 `Tab` 已归 05「提交 + 右移」，网格态再做一套会让「按一次 `Tab` 有时换格有时离开」不可教。若未来要做，必须是显式开关 `tabMovesWithinGrid`，缺省 `false` |
| K-2 | `none` 模式下的方向键是否进入 `cell` 模式（本册裁定为「进入」，但 01 §4.2 的跃迁表未列该入口） | **推荐：保持「进入」，并请 01 在跃迁表的 `none → cell` 行补上「方向键」**。理由：容器是焦点宿主，若 `none` 下方向键不进入，键盘用户无法从零开始选择（Tab 进来后按方向键毫无反应，是不可接受的死路） |
| K-3 | `PgUp`/`PgDn` 是否在到达末屏后自动翻页取数（并在新页把光标放到首行） | **推荐：不做**。理由：取数即触发契约 §4.3「重新取数必须清空全部选择」，一个翻屏键顺手清掉用户刚拉好的选区，代价远大于收益。若将来要做，必须与 09/11 的分页策略一起裁定，并且必须给用户可见提示 |
| K-4 | `mod+←` / `mod+→` 映射为 `Home`/`End`（行首/行尾）是否会与浏览器/WebView 的默认行为冲突 | **推荐：采纳映射并 `preventDefault`**。理由：macOS 笔记本键盘没有独立 `Home`/`End`，不映射等于 Mac 用户拿不到行首/行尾。副作用是覆盖了「光标到行首」的原生语义，但在网格里没有 caret，无副作用 |
| K-5 | `Cmd/Ctrl+A` 让 `row` 模式多了一条键盘进入条件，01 §4.2 的跃迁表需要同步 | **推荐：由 01 在下一版补一行「`Cmd/Ctrl+A` → `row`」，两册不留双份描述**。理由：本册已按契约 §4.2 的两条「必须遵守」实现（复用 `selectedRows` / `toggleSelectAll`），不构成契约冲突，但跃迁表必须单点维护 |
| K-6 | `Space` 是否为「切换当前光标行整行选中」保留 | **推荐：不注册，保持 `pass`**（浏览器滚动）。理由：`Enter` 已承担「进入编辑」、`Cmd+A` 承担「全选行」、行号槽 `Enter` 承担「选该行」，`Space` 再叠一层只会制造第 4 种「选行」路径。09/19 若要用 `Space` 做预览（Quick Look），需回到本册重裁 |
| ~~K-7~~ | 容器的 `role`（本册原主张：只加 `tabIndex` + `aria-*` + `data-dt-surface`，**不加** `role="grid"`） | **已裁定并关闭：本册的主张被撤销。** 维持 01 的方案——`role="grid"` + 行容器 `role="row"` + 单元格 `role="gridcell"` 连同 `aria-rowcount` / `aria-colcount` / `aria-activedescendant` **由 01 一次性原子加齐**（契约 §4.4「ARIA 网格角色必须原子添加」）。随之，01 必须一并完成行号槽 `<button>` → `<div role="rowheader">` + 内层 `<button tabIndex={-1}>` 的改造（原 `<button>` 不是 `role="row"` 的合法直接子元素，会被屏幕阅读器整块丢弃）。**本册的职责收窄为：只改焦点与键位，不新增、不删除任何 ARIA 角色**；测试选择器跟随 01 改造后的 DOM。容器属性名统一为 `data-dt-surface`（01 已冻结的全量表里的唯一名字），**不再另立 `data-dt-grid`** |
| K-8 | `Delete`/`Backspace` 在 `cell` 模式下是否改为「清空选区覆盖单元格的内容」 | **推荐：本册不动，归 05/06 与 PRD 一起裁定**。理由：那是**写操作**，必须走契约 §4.3 的 `rowIdentityAnchors → pendingChanges` 链路并新增批量 `CellWrite`，与键盘导航不是同一风险等级；本册只保证「现状：no-op」被 journey 测试固化，避免将来被误接到删行上 |
| K-9 | 光标记忆（`none` 模式重新进入时的落点）是否应该进 store（`TableState`），因为它影响渲染吗 | **推荐：不进 store**。理由：它**不影响渲染**（`none` 模式下没有任何 `data-dt-active`），只影响下一次按键的落点；放进 store 会平白新增一个「重新取数时要清空」的字段（契约 §4.3 的收敛点会多一处遗漏风险）。放在 `useGridKeyboardNav` 的 `useRef` 里，配合 `clampGridCursor` 兜底 |
| K-10 | `PgUp`/`PgDn` 的「一屏」是否等于 `viewportRowCount`，以及 `viewportRowCount` 由谁提供 | **推荐：由调用方（虚拟滚动的持有者）注入，步长 = `max(1, viewportRowCount - 1)`**。理由：本册禁止几何反查单元格坐标（01 同纪律），而「可见行数」是视口度量、不是坐标反查；由虚拟滚动方提供比 `scrollEl.clientHeight / rowHeight` 更准确（`rowHeight` 可能被行高自适应改掉） |
