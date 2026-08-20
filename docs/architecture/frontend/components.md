# 前端组件与布局

> [返回架构总览](../README.md)

## 1. 大数据量性能方案

### 5.1 核心策略

| 策略 | 适用场景 | 方案 |
|------|----------|------|
| **服务端分页** | 表数据浏览 | LIMIT/OFFSET，每页 50 行 |
| **虚拟滚动** | 查询结果 & 宽表 | @tanstack/react-virtual |
| **延迟渲染** | 长文本单元格 | 截断 + Tooltip |
| **列宽缓存** | 表格列宽计算 | 首次测量后缓存，不每帧计算 |
| **分批 IPC** | 大结果集传输 | 流式传输 / 分块加载 |
| **Web Worker** | JSON 解析 | 大于 1MB 的结果集在 Worker 中解析 |

### 5.2 虚拟滚动表格

查询结果可能一次返回数千到数万行，必须使用虚拟滚动：

```typescript
// hooks/useVirtualTable.ts
import { useVirtualizer } from '@tanstack/react-virtual';

interface UseVirtualTableOptions {
  rows: unknown[][];
  rowHeight: number;          // 40px（与设计稿一致）
  overscan: number;           // 预渲染行数，默认 10
  containerRef: RefObject<HTMLDivElement>;
}

export function useVirtualTable({ rows, rowHeight, overscan, containerRef }: UseVirtualTableOptions) {
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => containerRef.current,
    estimateSize: () => rowHeight,
    overscan,
  });

  return {
    virtualRows: virtualizer.getVirtualItems(),
    totalHeight: virtualizer.getTotalSize(),
    scrollToRow: virtualizer.scrollToIndex,
  };
}
```

```tsx
// components/DataTable/VirtualBody.tsx
function VirtualBody({ rows, columns, rowHeight }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const { virtualRows, totalHeight } = useVirtualTable({
    rows,
    rowHeight,
    overscan: 10,
    containerRef,
  });

  return (
    <div ref={containerRef} className="overflow-auto flex-1">
      <div style={{ height: totalHeight, position: 'relative' }}>
        {virtualRows.map((vRow) => (
          <div
            key={vRow.index}
            style={{
              position: 'absolute',
              top: vRow.start,
              height: rowHeight,
              width: '100%',
            }}
          >
            <TableRow row={rows[vRow.index]} columns={columns} index={vRow.index} />
          </div>
        ))}
      </div>
    </div>
  );
}
```

### 5.3 单元格渲染优化

```typescript
// components/DataTable/CellRenderer.tsx
// 使用 React.memo 避免非编辑行的重渲染

const CellRenderer = memo(function CellRenderer({ value, type, isEditing }: Props) {
  if (isEditing) return <EditableCell value={value} type={type} />;

  // Colors come from theme tokens `--dt-*` (Host defaults; packs may override).
  if (value == null) return <span className="text-dt-null italic">NULL</span>;
  if (type.includes('bool')) return <span className="text-dt-bool font-mono">{String(value)}</span>;
  if (isNumeric(type)) return <span className="text-dt-number font-mono">{value}</span>;
  if (type.includes('timestamp') || type.includes('date'))
    return <span className="text-dt-datetime font-mono text-xs">{formatTimestamp(value)}</span>;
  if (type.includes('json')) return <span className="text-dt-json font-mono">{formatCell(value)}</span>;
  return <span className="text-dt-text" title={String(value)}>{truncate(String(value))}</span>;
});
```

### 5.4 性能关键指标

| 指标 | 目标 | 实现手段 |
|------|------|----------|
| 首屏渲染 | < 200ms | 只渲染可见区域（虚拟滚动） |
| 滚动帧率 | 60fps | overscan + CSS transform 定位 |
| 内存占用 | 当前页数据 + 虚拟窗口 | 不缓存历史页数据 |
| 切换页响应 | < 100ms | 加载中骨架屏，数据到达后一次性渲染 |
| 10 万行结果滚动 | 流畅无卡顿 | 虚拟列表 + memo |

## 2. 布局与响应式方案

### 6.1 设计原则

1. **固定 + 弹性混合布局**：标题栏/工具栏/状态栏固定高度，内容区弹性填充
2. **可拖拽分割**：侧边栏宽度、编辑器/结果区高度可拖拽调整
3. **最小尺寸保护**：每个区域设置 `min-width` / `min-height`，避免收缩到不可用
4. **不使用百分比字体/绝对像素偏移**：用 Tailwind 的 `rem` 体系 + `flex`/`grid`

### 6.2 窗口布局结构

#### 主窗口 (main-window)

```
┌──────────────────────────────────────────────────┐  ← 固定 h-10 (40px)
│ 标题栏 (macOS traffic lights + 居中标题)          │
├──────────────────────────────────────────────────┤  ← 固定 h-14 (56px)
│ 工具栏 (搜索框 + 新建连接按钮 + 视图切换)         │
├──────────┬───────────────────────────────────────┤
│          │                                       │
│ 分组面板  │          连接卡片网格                  │  ← flex-1 填充
│ (220px   │     (CSS Grid, auto-fill)             │
│  可拖拽)  │                                       │
│          │                                       │
├──────────┴───────────────────────────────────────┤  ← 固定 h-10 (40px)
│ 状态栏                                           │
└──────────────────────────────────────────────────┘
```

CSS 实现：

```tsx
<div className="flex flex-col h-screen overflow-hidden">
  {/* 标题栏 */}
  <header className="h-10 shrink-0 bg-slate-800" data-tauri-drag-region />

  {/* 工具栏 */}
  <div className="h-14 shrink-0 bg-slate-800 border-b border-slate-700" />

  {/* 内容区 */}
  <div className="flex flex-1 min-h-0">
    {/* 分组面板 - 可拖拽宽度 */}
    <aside style={{ width: sidebarWidth }} className="shrink-0 bg-slate-800 border-r border-slate-700">
      <GroupPanel />
    </aside>
    <ResizeHandle onResize={setSidebarWidth} />
    {/* 卡片网格 */}
    <main className="flex-1 overflow-auto p-6">
      <ConnectionGrid />
    </main>
  </div>

  {/* 状态栏 */}
  <footer className="h-10 shrink-0 bg-slate-800 border-t border-slate-700" />
</div>
```

#### 主工作区连接视图 (main → ConnectionPage)

连接 / Workflow / Dashboard 在同一 OS 窗口内切换；左侧 `ConnectionNavigatorTree`，右侧连接 Tab（结构 / 数据 / 查询等）。查询编辑器内联在 ContentView，**不再**使用独立 `query-window` OS 窗口。独立子窗口仅保留新建连接、设置、备份、同步、Schema Diff、文档等。详见 [窗口管理](../windows.md)。

```
┌──────────────────────────────────────────────────┐
│ 标题栏 + 工作区导航（Connections / Workflow / Dashboard）│
├──────────┬───────────────────────────────────────┤
│ 连接导航树 │ Tab 栏 + ContentView（结构/数据/查询…） │
│ (可拖拽)  │                                       │
└──────────┴───────────────────────────────────────┘
```

### 6.3 可拖拽分割器

```typescript
// hooks/useResizable.ts
interface UseResizableOptions {
  direction: 'horizontal' | 'vertical';
  initialSize: number;
  minSize: number;
  maxSize: number;
  storageKey?: string;       // 持久化到 localStorage
}

export function useResizable({ direction, initialSize, minSize, maxSize, storageKey }: UseResizableOptions) {
  const [size, setSize] = useState(() => {
    if (storageKey) {
      const saved = localStorage.getItem(`resize:${storageKey}`);
      if (saved) return Math.max(minSize, Math.min(maxSize, Number(saved)));
    }
    return initialSize;
  });

  const handleRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handle = handleRef.current;
    if (!handle) return;

    let startPos = 0;
    let startSize = 0;

    function onPointerDown(e: PointerEvent) {
      startPos = direction === 'horizontal' ? e.clientX : e.clientY;
      startSize = size;
      handle.setPointerCapture(e.pointerId);
      document.body.style.cursor = direction === 'horizontal' ? 'col-resize' : 'row-resize';
      document.body.style.userSelect = 'none';
    }

    function onPointerMove(e: PointerEvent) {
      if (!handle.hasPointerCapture(e.pointerId)) return;
      const delta = (direction === 'horizontal' ? e.clientX : e.clientY) - startPos;
      const newSize = Math.max(minSize, Math.min(maxSize, startSize + delta));
      setSize(newSize);
    }

    function onPointerUp(e: PointerEvent) {
      handle.releasePointerCapture(e.pointerId);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      if (storageKey) localStorage.setItem(`resize:${storageKey}`, String(size));
    }

    handle.addEventListener('pointerdown', onPointerDown);
    handle.addEventListener('pointermove', onPointerMove);
    handle.addEventListener('pointerup', onPointerUp);

    return () => {
      handle.removeEventListener('pointerdown', onPointerDown);
      handle.removeEventListener('pointermove', onPointerMove);
      handle.removeEventListener('pointerup', onPointerUp);
    };
  }, [size, direction, minSize, maxSize, storageKey]);

  return { size, handleRef };
}
```

### 6.4 窗口缩放保护

| 保护策略 | 实现 |
|----------|------|
| 侧边栏最小宽度 | `min-width: 180px`，拖拽时 clamp |
| 侧边栏最大宽度 | `max-width: 50%`（基于窗口宽度动态计算） |
| 编辑器最小高度 | `min-height: 120px` |
| 结果区最小高度 | `min-height: 120px` |
| 卡片网格自适应 | `grid-template-columns: repeat(auto-fill, minmax(280px, 1fr))` |
| 表格水平滚动 | 列多时 `overflow-x: auto`，表头固定 |
| 工具栏折叠 | 窗口过窄时工具栏按钮收入 `...` 下拉菜单 |
| 文字不溢出 | 所有文本使用 `truncate` + `title` tooltip |

### 6.5 连接卡片网格自适应

```tsx
<div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-4 p-6">
  {connections.map((conn) => (
    <ConnectionCard key={conn.id} connection={conn} />
  ))}
</div>
```

效果：窗口宽度 > 1200px 时展示 3 列，缩小到 900px 时变为 2 列，再缩小变为 1 列，卡片始终在 280px~1fr 之间弹性伸缩。

## 3. 主题系统

### 7.1 CSS 变量方案

Host 语义 token 定义在 `src/styles/themes.css`（非旧版 `--bg-primary` 命名）：

```css
:root {
  --c-surface: #ffffff;
  --c-fg: #0f172a;
  --c-accent: #3b82f6;
  /* … surface / edge / status … */
  --dt-null: var(--c-fg-muted);
  --dt-bool: #a855f7;
  --dt-number: #d97706;
  --dt-datetime: #7c3aed;
  --dt-json: var(--c-fg);
  --dt-text: var(--c-fg);
  --cm-keyword: #7c3aed;
  /* … CodeMirror … */
}

.dark {
  --c-surface: #0f172a;
  --dt-number: #fcd34d;
  /* … */
}
```

### 7.2 Tailwind 配置

```typescript
// tailwind.config.ts — 摘录
colors: {
  surface: { DEFAULT: 'var(--c-surface)', alt: 'var(--c-surface-alt)', /* … */ },
  fg: { DEFAULT: 'var(--c-fg)', secondary: 'var(--c-fg-secondary)', muted: 'var(--c-fg-muted)' },
  accent: { DEFAULT: 'var(--c-accent)' },
  dt: {
    null: 'var(--dt-null)', bool: 'var(--dt-bool)', number: 'var(--dt-number)',
    datetime: 'var(--dt-datetime)', json: 'var(--dt-json)', text: 'var(--dt-text)',
  },
},
```

**DataTable / 结构视图类型色**：`src/lib/dataTypeColors.ts` 将 SQL 类型映射到 `text-dt-*`；`CellRenderer`、`StructureView`、`TableHeader`、`DetailPanel`、`ExportDialog`、`IndexesView` 共用。

### 7.3 模式切换（light / dark / system）

`settings.theme` 为 `{ mode, packId }`；`packId` 为 `null` 时仅使用 Host 内置 token。

```typescript
// settingsStore.ts — applyTheme(mode × packId)
async function applyTheme(mode: ThemeMode, packId: string | null) {
  document.documentElement.classList.toggle('dark', resolveIsDark(mode));
  await applyThemePack(packId);           // 注入 pack CSS / 图标 / 字体
  syncWebviewBackgroundFromTokens();
}

// 跨窗口 / 菜单同步 mode，不写后端
export async function applyThemeLocally(mode: ThemeMode) {
  const packId = useSettingsStore.getState().settings.theme.packId;
  await applyTheme(mode, packId);
  watchSystemTheme(mode);                 // system 模式监听 prefers-color-scheme
}

// updateSettings({ theme: { mode, packId } }) → 持久化 + applyTheme + 跨窗口广播
```

### 7.4 运行时主题包

用户从设置页（`ThemePackSection`）安装本地 ZIP，启用后由 `themePackApply.ts` 加载：

```
settings.theme.packId  →  read_theme_pack_file (IPC)
                      →  injectThemePackCss (<style id="datazen-theme-pack">)
                      →  register pack icon blob URLs + font faces
                      →  optional editor.json / charts.json overlays
```

| 模块 | 路径 | 职责 |
|------|------|------|
| IPC 封装 | `src/commands/theme.ts` | `listThemePacks`, `installThemePackWithDialog`, `removeThemePack`, `readThemePackFile` |
| 应用逻辑 | `src/lib/themePackApply.ts` | 注入/移除 pack CSS、字体、通知跨窗口刷新；把解析后的 `--c-surface` 经 IPC 写入 `{appData}/surface-bg.json` |
| 首屏背景 | `surface-boot` plugin `initialization_script` | parse 前注入上次 hex + `html.dark`；主窗口与子窗口同一路径 |
| 图标解析 | `src/lib/iconResolver.ts` | pack → Lucide/驱动 → 占位 |
| 组件 | `ThemedIcon`, `DbTypeBadge` | 消费 IconResolver |
| 设置 UI | `windows/settings/ThemePackSection.tsx` | 安装、启用、删除主题包 |

**图标解析顺序**

- 功能 UI：`pack icons[id]` → Host Lucide → 占位
- DB 角标：`pack icons["db." + type]` → 驱动默认 SVG → shortLabel 色块（`DbTypeBadge`）

**字体**

- Host 定义 `--font-sans`、`--font-mono`、`--font-editor`（`themes.css`）。
- 主题包可通过 `fonts.css` 覆盖；用户显式设置的 `editorFontFamily` **优先于** 主题 `--font-editor`。

后端安装与校验见 [运行时主题包](../backend/theme.md)。

**DataTable 单元格类型色**

| CSS 变量 | Tailwind | 用途 |
|----------|----------|------|
| `--dt-null` | `text-dt-null` | NULL |
| `--dt-bool` | `text-dt-bool` | 布尔 |
| `--dt-number` | `text-dt-number` | 数值 |
| `--dt-datetime` | `text-dt-datetime` | 日期/时间 |
| `--dt-json` | `text-dt-json` | JSON |
| `--dt-text` | `text-dt-text` | 普通文本 |

Host 在 `src/styles/themes.css` 提供 light/dark 默认；主题包可在 `tokens.css` 覆盖。实现：`CellRenderer.tsx`。

## 4. Tauri IPC 通信层

### 8.1 参数名序列化约定

Tauri 使用 serde 反序列化前端传入的参数。

- **命令名**：Rust 侧 `#[tauri::command]` 函数名使用 `snake_case`（如 `get_connections`、`execute_query`）。
- **Rust 形参**：命令函数参数在 Rust 源码中为 `snake_case` 标识符（如 `connection_id`、`default_file_name`）。
- **前端 `invoke` 键名**：`src/commands/*` 中传参对象键名使用 **camelCase**（如 `{ connectionId, sql }`、`{ defaultFileName }`）。Tauri 2 会将 camelCase 键名映射到 Rust 的 snake_case 形参。
- **嵌套结构体**：请求/响应 DTO 在 Rust 侧通常标注 `#[serde(rename_all = "camelCase")]`，前后端字段均为 camelCase（如 `ConnectionConfig`、`MultiQueryResult`）。

**约定**：新增 IPC 时，前端 `invoke` 第二参数与 TypeScript 类型保持一致（camelCase）；Rust 命令形参保持 snake_case；复杂 struct 在 Rust 侧显式 `rename_all = "camelCase"`。不要在前端混用 snake_case 键名（如 `connection_id`），现有 `src/commands/` 中已无此类用法。

示例（摘自 `commands/query.ts` / `commands/connection.ts`）：

```typescript
invoke<MultiQueryResult>('execute_query', { connectionId, sql });
invoke<string>('connect', { configId });
invoke<number | null>('export_connections_with_dialog', { password, defaultFileName });
```

### 8.2 命令封装

> 以下接口与后端 [IPC 命令层](../backend/commands.md) 逐一对齐。

```typescript
// commands/connection.ts
import { invoke } from '@tauri-apps/api/core';

export const connectionCommands = {
  getConnections: () =>
    invoke<ConnectionConfig[]>('get_connections'),

  saveConnection: (config: ConnectionConfig) =>
    invoke<void>('save_connection', { config }),

  deleteConnection: (id: string) =>
    invoke<void>('delete_connection', { id }),

  testConnection: (config: ConnectionConfig) =>
    invoke<ServerInfo>('test_connection', { config }),

  connect: (configId: string) =>
    invoke<string>('connect', { config_id: configId }),

  disconnect: (connectionId: string) =>
    invoke<void>('disconnect', { connection_id: connectionId }),
};
```

```typescript
// commands/database.ts
import { invoke } from '@tauri-apps/api/core';

export const databaseCommands = {
  getDatabases: (connectionId: string) =>
    invoke<string[]>('get_databases', { connection_id: connectionId }),

  getTables: (connectionId: string, database: string) =>
    invoke<TableInfo[]>('get_tables', { connection_id: connectionId, database }),

  getTableSchema: (connectionId: string, table: string) =>
    invoke<TableSchema>('get_table_schema', { connection_id: connectionId, table }),

  // 注意：需要后端新增 #[tauri::command] get_table_data（见下方补齐说明）
  getTableData: (params: {
    connectionId: string;
    table: string;
    page: number;
    pageSize: number;
    filters?: FilterCondition[];
    sorts?: SortCondition[];
  }) => invoke<TableDataResult>('get_table_data', {
    connection_id: params.connectionId,
    table: params.table,
    page: params.page,
    page_size: params.pageSize,
    filters: params.filters,
    sorts: params.sorts,
  }),
};
```

```typescript
// commands/query.ts
import { invoke } from '@tauri-apps/api/core';

export const queryCommands = {
  executeQuery: (connectionId: string, sql: string) =>
    invoke<QueryResult>('execute_query', { connection_id: connectionId, sql }),

  getExplain: (connectionId: string, sql: string) =>
    invoke<ExplainResult>('get_explain', { connection_id: connectionId, sql }),

  cancelQuery: (connectionId: string) =>
    invoke<void>('cancel_query', { connection_id: connectionId }),

  getQueryHistory: (limit: number) =>
    invoke<QueryHistoryEntry[]>('get_query_history', { limit }),

  clearQueryHistory: () =>
    invoke<void>('clear_query_history'),
};
```

```typescript
// commands/settings.ts
import { invoke } from '@tauri-apps/api/core';

export const settingsCommands = {
  getSettings: () =>
    invoke<AppSettings>('get_settings'),

  saveSettings: (settings: AppSettings) =>
    invoke<void>('save_settings', { settings }),
};
```

### 8.3 错误处理统一

```typescript
// lib/tauri.ts
export class TauriError extends Error {
  constructor(public code: string, message: string) {
    super(message);
  }
}

export async function safeInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (err) {
    const message = typeof err === 'string' ? err : (err as Error).message;
    throw new TauriError(extractErrorCode(message), message);
  }
}
```

## 5. 核心组件设计

### 9.1 DataTable 组件架构

```
DataTable (容器)
├── TableHeader         # 固定表头：列名 + 排序图标 + 筛选图标
│   └── ResizableColumn # 可拖拽调整列宽
├── VirtualBody         # 虚拟滚动区域
│   └── TableRow        # 单行
│       └── CellRenderer  # 按数据类型渲染
│           └── EditableCell  # 编辑模式
├── FilterBar           # 当前筛选条件展示
├── Pagination          # 分页控制
└── DataExportDialog    # 数据导出对话框（CSV/TSV/JSON/SQL INSERT/SQL UPDATE）
```

右键菜单走 Web Context Menu（见下方「Web Context Menu」），由 `buildDataTableContextMenuItems` 构建（对齐 TablePlus）：
Copy / Copy Row / Copy as JSON / Copy as SQL INSERT / Copy as UPDATE / Copy as CSV /
Copy Column Name / Set NULL（可编辑表；Query 结果通过 `enableSetNull={false}` 隐藏）/
Filter by This Value / Delete Row（需主键；`commit_row_deletes`）/
Copy Selected Rows / Export。
Safe Mode 开启时 Schema 树隐藏 Truncate / Drop（后端 `sql_guard` 拦截无 WHERE 的 UPDATE/DELETE，以及 TRUNCATE/DROP）；索引页删除按钮同样隐藏。

**数据导出功能**：
- 工具栏「导出」按钮导出全部数据
- 右键菜单导出选中行（或当前页）
- 支持 5 种格式：CSV、TSV、JSON、SQL INSERT、SQL UPDATE
- 通过 Tauri 原生对话框选择保存路径

**导出**（Connection Window，非单表 DataTable 导出；原「批量导出」）：
- 顶栏「导出」按钮（权限按钮之后，`data-testid=conn-toolbar-export`）→ `BatchExportDialog`；Schema 树 database / blank / table / view 右键「导出…」（`schemaTreeContextMenu` → `onBatchExport`）
- 范围：全部表或所选表；模式：仅结构 / 仅数据 / 数据+结构
- 逻辑：`src/lib/batchExport.ts`（组装）+ `batchExportJob.ts`（执行/ZIP）+ `loadBatchExportTable.ts`（DDL + 分页全量）
- UI：`src/windows/connection/BatchExportDialog.tsx`；表多选、格式、单文件/ZIP
- E2E：可断言顶栏按钮；Schema 树右键可断言 `data-testid="web-context-menu"` 与 `web-context-item-*`
- 编辑表结构页（`TableStructureEditor` alter）：「导出表结构」→ DDL 另存为 `.sql`（`exportTableStructure.ts`）

Props 接口：

```typescript
interface DataTableProps {
  columns: ColumnDef[];
  rows: unknown[][];
  totalRows: number;
  page: number;
  pageSize: number;
  sorts: SortCondition[];
  filters: FilterCondition[];
  editBuffer: Map<string, CellEdit>;
  editingCell: { row: number; col: string } | null;
  selectedRows: Set<number>;
  loading: boolean;
  onSort: (sort: SortCondition) => void;
  onFilter: (filter: FilterCondition) => void;
  onPageChange: (page: number) => void;
  onPageSizeChange: (size: number) => void;
  onCellDoubleClick: (row: number, col: string) => void;
  onCellEdit: (row: number, col: string, value: unknown) => void;
  onCellEditCancel: () => void;
  onRowSelect: (index: number) => void;
  onSelectAll: () => void;
}
```

### 9.1.1 Web Context Menu

右键菜单统一使用 Web 浮层（portal 到 `document.body`），入口：

- `src/lib/nativeContextMenu.ts`：`NativeMenuItemDef` / `normalizeNativeMenuItems` / `nativeEditMenuItems` / `showNativeContextMenu(items, {x,y})`
- `src/stores/contextMenuStore.ts`：`showWebContextMenu`
- `src/components/ui/WebContextMenu.tsx`：`WebContextMenuHost`（`App.tsx` 挂载）
- `src/lib/contextMenuPosition.ts`：根菜单与**二级菜单**在视口右/下边缘翻转或 clamp，面板不得被窗口裁切

禁止再使用 `@tauri-apps/api/menu` 的 `Menu.popup()`。调用方必须传入 `clientX/clientY`（或 Schema 树 payload 的 `x/y`）。

各场景通过独立 builder 组装 `NativeMenuItemDef[]`，再调用 `showNativeContextMenu` / `showWebContextMenu`：

| Builder | 路径 | 调用方 |
|---------|------|--------|
| SQL 编辑器 | `src/lib/sqlEditorContextMenu.ts` | `QueryPanel` |
| Schema 树 | `src/lib/schemaTreeContextMenu.ts` | `ContentView` |
| DataTable | `src/lib/dataTableContextMenu.ts` | `DataTable` |
| 连接 Tab | `src/lib/connectionTabContextMenu.ts` | `ContentView` |
| 收藏 / 历史侧栏 | `src/lib/querySidebarContextMenu.ts` | `QueryPanel` |
| Workflow 列表 / 历史 | `src/lib/workflowListContextMenu.ts` | `WorkflowPage` |
| ER 节点 | `src/lib/erNodeContextMenu.ts` | `ErDiagramView` |
| Redis Key | `packages/drivers/redis/ui/redisKeyContextMenu.ts` | `RedisWorkbench` |
| 主窗口连接/分组 | `src/lib/mainWindowContextMenu.ts` | `ConnectionPage` |

Connection Window 菜单项对齐 TablePlus：Schema（Open Structure / New Query / Copy DDL / Truncate / Drop / New Table / Import）、SQL 编辑器（Run / Run Selection / Format / Comment）、Tab（Close to the Right/Left）、DDL 视图右键 Copy。

约定：调用方传入 i18n labels 与 handlers；`preventDefault` + `stopPropagation` 后按坐标弹出。E2E 断言 `data-testid="web-context-menu"` / `web-context-submenu` / `web-context-item-*`；二级菜单贴窗口边缘时必须完整可见。主窗口「移到分组」是典型 submenu 用例。

### 9.2 SQL 编辑器集成

```typescript
// windows/query/SqlEditor.tsx
import Editor, { OnMount } from '@monaco-editor/react';

function SqlEditor({ value, onChange, onExecute }: Props) {
  const handleMount: OnMount = (editor, monaco) => {
    // 注册 SQL 自动补全 Provider
    monaco.languages.registerCompletionItemProvider('sql', {
      provideCompletionItems: (model, position) => {
        const suggestions = buildCompletionItems(
          useSchemaStore.getState().tables,
          useSchemaStore.getState().columns,
          monaco
        );
        return { suggestions };
      },
    });

    // 注册执行快捷键
    editor.addAction({
      id: 'execute-query',
      label: 'Execute Query',
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter],
      run: () => onExecute(),
    });
  };

  return (
    <Editor
      language="sql"
      theme="datazen-dark"       // 自定义主题，匹配设计稿配色
      value={value}
      onChange={(v) => onChange(v ?? '')}
      onMount={handleMount}
      options={{
        fontSize: 13,
        fontFamily: 'JetBrains Mono, monospace',
        lineHeight: 20,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        automaticLayout: true,    // 自动适应容器大小变化
        padding: { top: 8 },
      }}
    />
  );
}
```

### 9.3 Schema 树组件

```typescript
// windows/connection/SchemaTree.tsx
interface TreeNode {
  id: string;
  label: string;
  type: 'database' | 'table' | 'view' | 'folder';
  children?: TreeNode[];
  rowCount?: number;
  icon: ReactNode;
}

function SchemaTree() {
  const { databases, tables, views, expanded, selected, toggleExpand, setSelected } = useSchemaStore();

  const nodes = useMemo(() => buildTreeNodes(databases, tables, views), [databases, tables, views]);

  return (
    <div className="flex flex-col h-full">
      {/* 数据库选择器 */}
      <DatabaseSelector />

      {/* 表/视图分组 */}
      <div className="flex-1 overflow-auto">
        {nodes.map((node) => (
          <TreeItem
            key={node.id}
            node={node}
            depth={0}
            isExpanded={expanded.has(node.id)}
            isSelected={selected === node.id}
            onToggle={() => toggleExpand(node.id)}
            onClick={() => setSelected(node.id)}
            onDoubleClick={() => openDataTab(node.id)}
          />
        ))}
      </div>
    </div>
  );
}
```

## 6. 图表可视化系统

### 6.1 架构总览

查询结果支持表格/图表双视图切换，基于 **Recharts** 实现。核心设计原则：

- **零配置启动**：通过字段类型推断 + 规则引擎自动推荐最佳图表
- **配置持久化**：图表配置绑定到 `panelStore.QueryExecState`，切换标签页/重新执行不丢失
- **渐进增强**：先看到合理的默认图表，再通过 UI 或自然语言微调

### 6.2 组件结构

```
QueryPanel
├── [📋 表格] [📈 图表]       ← 视图切换按钮
├── ResultTable               ← 表格视图
└── ChartView                 ← 图表视图（入口组件）
    ├── ChartToolbar           — 图表类型选择 + 选项开关 + NL输入框 + 导出
    ├── AxisConfigurator       — 轴映射 + 字段列表 + 聚合/排序/配色
    └── ChartCanvas            — 渲染层（absolute定位，解决ResponsiveContainer高度问题）
        ├── BarChartRenderer
        ├── LineChartRenderer
        ├── PieChartRenderer
        ├── ScatterChartRenderer
        └── AreaChartRenderer
```

### 6.3 数据流

```
StatementResult
  → inferAllFields()       字段类型推断（numeric/datetime/categorical）
  → recommendChart()       规则引擎推荐图表类型 + 轴映射
  → ChartConfig            用户可覆盖的配置对象
  → transformData()        数据转换 + 聚合 + 排序 → ChartDataPoint[]
  → Renderer               Recharts 渲染
```

### 6.4 核心模块（src/lib/chart/）

| 模块 | 职责 |
|------|------|
| `fieldInference.ts` | 基于列名和采样值推断字段类型 |
| `recommend.ts` | 基于字段组合的规则引擎，推荐图表类型和轴配置 |
| `transform.ts` | 直接映射 / 聚合模式数据转换，支持分组和排序 |
| `colors.ts` | 5 套内置配色方案（default/warm/cool/neon/pastel） |
| `format.ts` | 千分位数值格式化、百分比格式化、轴刻度格式化 |
| `nlConfig.ts` | 自然语言解析图表配置指令（"换成饼图"、"按销量排序"） |
| `export.ts` | PNG（html-to-image）/ SVG 导出 |

### 6.5 功能特性

- **5 种图表类型**：柱状图、折线图、饼图、散点图、面积图
- **智能推荐**：根据字段类型自动选择最佳图表
- **多 Y 轴 + 分组**：支持多数值列同时展示、按分类列分组
- **5 种聚合**：sum / avg / count / min / max
- **图表↔表格联动**：点击数据点切换到表格并高亮对应行
- **NL2SQL 自动图表化**：「应用并图表化」按钮一键执行 SQL 并展示图表
- **自然语言配置调整**：通过文本输入修改图表类型、排序、聚合等
- **导出**：PNG / SVG 格式
- **大数据集保护**：>1000 行自动截断采样

### 6.6 配置持久化

图表配置通过 `panelStore` 中的 `QueryExecState` 进行持久化：

```typescript
interface QueryExecState {
  // ...existing fields...
  chartConfig?: ChartConfig;
  resultViewMode?: 'table' | 'chart';
}
```

通过 `setChartConfig(panelId, config)` 和 `setResultViewMode(panelId, mode)` 更新。

## 7. 窗口路由与多窗口管理

### 10.1 窗口入口分发

各 webview 加载同一 HTML，通过 URL `?window=`（`getWindowKind()`）区分。连接 / Workflow / Dashboard 统一进 `main`：

```typescript
// windowKind.ts — legacy aliases map to main
const LEGACY_MAIN_ALIASES = new Set(['connection', 'workflow', 'dashboard']);

// App.tsx（示意）
switch (getWindowKind()) {
  case 'main': return <MainPage />; // → ConnectionPage 统一工作区
  case 'settings': return <SettingsWindow />;
  case 'new-connection': return <NewConnectionWindow />;
  // backup / data-sync / schema-diff / docs …
}
```

### 10.2 打开连接（主工作区 Tab）

```typescript
// lib/windowManager.ts — 不再创建 connection-* OS 窗口
export function openConnectionWindow(opts, connectionName, database?, databaseType?, action?) {
  localStorage.setItem(PENDING_CONNECTION_KEY, JSON.stringify(payload));
  void emitCrossWindow('datazen:open-connection', payload);
  void focusMainWindow();
}
```

## 8. 设计稿还原规范

### 11.1 布局尺寸对照

| 区域 | 设计稿像素 | Tailwind 实现 |
|------|-----------|--------------|
| 标题栏高度 | 40px | `h-10` |
| 工具栏高度 | 48-56px | `h-12` / `h-14` |
| 状态栏高度 | 40px | `h-10` |
| 左侧边栏宽度 | 220–280px（主工作区导航树） | 可拖拽 |
| Tab 栏高度 | 40px | `h-10` |
| 表格行高 | 40-48px | `h-10` / `h-12` |
| 卡片圆角 | 12px | `rounded-xl` |
| 输入框高度 | 36px | `h-9` |
| 输入框圆角 | 6px | `rounded-md` |
| 按钮高度 | 32px | `h-8` |
| 按钮圆角 | 6px | `rounded-md` |

### 11.2 色彩对照（暗色主题）

| 设计稿色值 | 用途 | Tailwind |
|-----------|------|----------|
| `#0f172a` | 主背景 | `bg-slate-900` |
| `#1e293b` | 次背景 (侧边栏/表头/工具栏) | `bg-slate-800` |
| `#334155` | 边框/分割线 | `border-slate-700` |
| `#f1f5f9` | 主文字 | `text-slate-100` |
| `#94a3b8` | 次文字 | `text-slate-400` |
| `#64748b` | 占位/禁用文字 | `text-slate-500` |
| `#3b82f6` | 主色调/链接/选中 | `text-blue-500` / `bg-blue-500` |
| `#22c55e` | 成功/active 状态 | `text-green-500` |
| `#f59e0b` | 警告/pending 状态 | `text-amber-500` |
| `#ef4444` | 错误/inactive/删除 | `text-red-500` |
| `#c084fc` | SQL 关键字 | `text-purple-400` |
| `#fbbf24` | SQL 数字 | `text-amber-300` |
| `#8b5cf6` | 时间类型 | `text-violet-500` |

### 11.3 字体对照

| 场景 | 设计稿 | CSS |
|------|--------|-----|
| UI 文字 | Inter 13-15px | `font-sans text-sm` |
| 代码/数据 | JetBrains Mono 12-13px | `font-mono text-xs` / `font-mono text-sm` |
| 表头 | Inter 12px 600 | `text-xs font-medium text-slate-400` |
| 标签文字 | Inter 11px 600 spacing | `text-[11px] font-semibold tracking-wider uppercase text-slate-400` |

## 9. 测试策略

| 层级 | 工具 | 覆盖范围 |
|------|------|----------|
| 组件单测 | Vitest + React Testing Library | DataTable, CellRenderer, FilterBar（Host `src/`） |
| 驱动 UI 单测 | Vitest | `packages/drivers/<id>/ui/__tests__/`（`pnpm test:unit:drivers`，不进 Host `pnpm test:unit`） |
| Store 单测 | Vitest | 每个 Store 的 action/state 变化 |
| 集成测试 | WebdriverIO | 窗口创建/关闭, 连接流程, 查询执行；驱动深度 E2E 在 `packages/drivers/<id>/e2e/` |
| 性能测试 | WebdriverIO + Chrome DevTools | 10 万行滚动帧率, 内存占用 |
| 快照测试 | Storybook | 关键 UI 组件视觉回归 |

## 8. ER 图（Entity-Relationship Diagram）

基于 **React Flow**（`@xyflow/react`）的交互式数据库 ER 图组件。

### 8.1 组件结构

```
ContentView
├── 工具栏「ER 图」按钮 → 打开数据库级 ER 图
├── Schema Tree 右键菜单「聚焦此表」→ 以该表为焦点的 ER 图
└── ErDiagramView (src/windows/connection/ErDiagramView.tsx)
    ├── ReactFlow 画布（拖拽、缩放、平移）
    ├── TableNode — 自定义节点（表名、列列表含 PK/FK 徽章、可折叠列）
    ├── FK 连线 — 动画箭头 + 列名标签
    ├── 搜索栏 — 按表名搜索并高亮匹配节点（其余变暗）
    ├── 导出 — PNG / SVG（基于 html-to-image）
    └── 统计面板 — 表数量 + 关系数量
```

### 8.2 核心模块

| 文件 | 职责 |
|------|------|
| `ErDiagramView.tsx` | 主视图组件，获取 ER 数据、渲染画布、导出/搜索控制 |
| `er/TableNode.tsx` | React Flow 自定义节点，渲染表名 + 列 + PK/FK 标记，支持折叠 |
| `er/buildErGraph.ts` | `TableSchema[]` → React Flow nodes/edges 转换，自动布局 |

### 8.3 数据流

1. 后端 `get_er_data(connection_id, database)` 批量获取所有表的 `TableSchema`（含外键）
2. `buildErGraph(schemas, focusTable?)` 生成 nodes 和 edges
3. `focusTable` 参数控制焦点模式：仅显示目标表及其直接关联表
4. React Flow 渲染，支持交互和导出

## 9. PathInput 控件

`src/components/ui/PathInput.tsx` — 统一的路径输入/选择控件：
- 左侧：文本输入框（可手动输入路径）
- 右侧：「浏览」按钮（调用 Tauri Dialog API 选择文件或目录）
- 支持 `mode` 属性：`file` / `directory` / `save`
- 已在所有需要路径输入的位置替换（SQLite 数据库路径、备份路径、上下文目录等）

## 10. 开发阶段规划

| 阶段 | 内容 | 输出 |
|------|------|------|
| **Phase 1: 脚手架** | Vite + React + Tailwind + shadcn/ui 项目初始化；目录结构搭建；主题系统；Tauri 窗口路由 | 可运行的空壳多窗口应用 |
| **Phase 2: 主窗口** | 连接管理 Store；连接卡片/分组；新建连接对话框；连接测试 | 主窗口功能完整 |
| **Phase 3: 连接窗口** | Schema 树；表结构标签页；数据标签页（DataTable 核心）；虚拟滚动；分页 | 可浏览表结构和数据 |
| **Phase 4: 数据编辑** | 行内编辑；新增/删除行；筛选/排序；数据导出 | 完整数据编辑功能 |
| **Phase 5: 查询窗口** | CodeMirror 编辑器集成；查询执行/取消；结果展示；查询历史/收藏；执行计划 | 查询功能完整 |
| **Phase 6: 打磨** | 主题切换；快捷键；错误处理；性能优化；窗口间通信 | 生产就绪 |
| **Phase 7: 图表可视化** | Recharts 集成；5种图表类型；智能推荐；轴配置；NL调整；导出PNG/SVG | 查询结果可视化 |
