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

  switch (type) {
    case 'boolean':
      return <span className="text-purple-400">{String(value)}</span>;
    case 'integer':
    case 'bigint':
    case 'float':
    case 'double precision':
      return <span className="text-amber-400 font-mono">{value}</span>;
    case 'timestamp':
    case 'timestamptz':
      return <span className="text-slate-400 font-mono text-xs">{formatTimestamp(value)}</span>;
    case 'json':
    case 'jsonb':
      return <JsonCell value={value} />;
    default:
      // 长文本截断 + title tooltip
      const text = String(value ?? 'NULL');
      return (
        <span className={value === null ? 'text-slate-600 italic' : 'text-slate-100'} title={text}>
          {text.length > 120 ? text.slice(0, 120) + '…' : text}
        </span>
      );
  }
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

#### 连接窗口 (connection-window)

```
┌──────────────────────────────────────────────────┐  ← h-10
│ 标题栏                                           │
├──────────────────────────────────────────────────┤  ← h-12
│ 工具栏                                           │
├──────────┬───────────────────────────────────────┤
│          │ Tab栏: 结构 | 数据 | 索引 | 外键 | DDL │  ← h-10
│ 数据库树  ├───────────────────────────────────────┤
│ (280px   │                                       │
│  可拖拽)  │          Tab 内容区                    │  ← flex-1
│          │                                       │
├──────────┴───────────────────────────────────────┤  ← h-10
│ 状态栏                                           │
└──────────────────────────────────────────────────┘
```

#### 查询窗口 (query-window)

```
┌──────────────────────────────────────────────────┐  ← h-10
│ 标题栏                                           │
├──────────────────────────────────────────────────┤  ← h-12
│ 工具栏 (执行/取消/格式化/保存/收藏/历史/执行计划)  │
├──────────────────────────────────────────────────┤
│ 编辑器 Tab栏                                     │  ← h-8
├──────────────────────────────────────────────────┤
│                                                  │
│   CodeMirror SQL 编辑器                            │  ← 可拖拽高度
│                                                  │
├──────────────────────────────────────────────────┤  ← 拖拽分割线
│ 结果 Tab栏 (结果 | 消息)                          │  ← h-8
├──────────────────────────────────────────────────┤
│ 查询信息 (成功/行数/耗时)                         │  ← h-8
├──────────────────────────────────────────────────┤
│                                                  │
│   结果表格 (虚拟滚动)                             │  ← flex-1
│                                                  │
├──────────────────────────────────────────────────┤  ← h-10
│ 分页控制                                         │
├──────────────────────────────────────────────────┤  ← h-10
│ 状态栏                                           │
└──────────────────────────────────────────────────┘
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

```css
/* styles/themes.css */
:root {
  /* 亮色主题 */
  --bg-primary: #ffffff;
  --bg-secondary: #f8fafc;
  --bg-tertiary: #f1f5f9;
  --border: #e2e8f0;
  --text-primary: #0f172a;
  --text-secondary: #64748b;
  --text-muted: #94a3b8;
  --accent: #3b82f6;
  --success: #22c55e;
  --warning: #f59e0b;
  --danger: #ef4444;
}

.dark {
  --bg-primary: #0f172a;
  --bg-secondary: #1e293b;
  --bg-tertiary: #334155;
  --border: #334155;
  --text-primary: #f1f5f9;
  --text-secondary: #94a3b8;
  --text-muted: #64748b;
  --accent: #60a5fa;
  --success: #4ade80;
  --warning: #fbbf24;
  --danger: #f87171;
}
```

### 7.2 Tailwind 配置

```typescript
// tailwind.config.ts
export default {
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        bg: {
          primary: 'var(--bg-primary)',
          secondary: 'var(--bg-secondary)',
          tertiary: 'var(--bg-tertiary)',
        },
        border: 'var(--border)',
        text: {
          primary: 'var(--text-primary)',
          secondary: 'var(--text-secondary)',
          muted: 'var(--text-muted)',
        },
        accent: 'var(--accent)',
      },
      fontFamily: {
        sans: ['Inter', 'sans-serif'],
        mono: ['JetBrains Mono', 'monospace'],
      },
    },
  },
};
```

### 7.3 主题切换

```typescript
// settingsStore.ts
updateTheme(theme: 'light' | 'dark' | 'system') {
  if (theme === 'system') {
    const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    document.documentElement.classList.toggle('dark', isDark);
  } else {
    document.documentElement.classList.toggle('dark', theme === 'dark');
  }
}
```

## 4. Tauri IPC 通信层

### 8.1 参数名序列化约定

Tauri 使用 serde 反序列化前端传入的参数。后端 Rust 使用 `snake_case`，前端 TypeScript 使用 `camelCase`。
**统一方案**：前端 `invoke` 传参时使用 `snake_case` key 与后端保持一致，避免序列化歧义。

### 8.2 命令封装

> 以下接口与后端 `backend-architecture.md` 第六节 Tauri Commands 层逐一对齐。

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

### 8.3 后端需补齐的 Command

前端需要以下两个 Command，但后端 `backend-architecture.md` 第六节未定义，需要补充：

```rust
/// 获取表数据（带分页、筛选、排序）
#[tauri::command]
pub async fn get_table_data(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
    page: u32,
    page_size: u32,
    filters: Option<Vec<FilterCondition>>,
    sorts: Option<Vec<SortCondition>>,
) -> Result<TableDataResult, String> {
    // 调用 QueryExecutor.get_table_data(...)
}

/// 取消正在执行的查询
#[tauri::command]
pub async fn cancel_query(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    let (driver, handle) = state.connection_manager
        .get_connection(&connection_id).await
        .map_err(|e| e.to_string())?;

    driver.cancel_query(&handle).await
        .map_err(|e| e.to_string())
}
```

### 8.4 错误处理统一

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
├── DataExportDialog    # 数据导出对话框（CSV/TSV/JSON/SQL INSERT/SQL UPDATE）
└── ContextMenu         # 右键菜单（含导出选中行）
```

**数据导出功能**：
- 工具栏「导出」按钮导出全部数据
- 右键菜单导出选中行
- 支持 5 种格式：CSV、TSV、JSON、SQL INSERT、SQL UPDATE
- 通过 Tauri 原生对话框选择保存路径

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
- **配置持久化**：图表配置绑定到 `queryStore.QueryTab`，切换标签页/重新执行不丢失
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

图表配置通过 `queryStore` 中的 `QueryTab` 进行持久化：

```typescript
interface QueryTab {
  // ...existing fields...
  chartConfig?: ChartConfig;
  resultViewMode?: 'table' | 'chart';
}
```

通过 `setChartConfig(tabId, config)` 和 `setResultViewMode(tabId, mode)` 更新。

## 7. 窗口路由与多窗口管理

### 10.1 窗口入口分发

Tauri 多窗口模式下，每个窗口加载同一个 HTML，通过 URL 参数或 window label 区分窗口类型：

```typescript
// main.tsx
import { getCurrentWindow } from '@tauri-apps/api/window';

const windowLabel = getCurrentWindow().label;

function App() {
  if (windowLabel === 'main') return <MainWindow />;
  if (windowLabel.startsWith('connection-')) return <ConnectionWindow />;
  if (windowLabel.startsWith('query-')) return <QueryWindow />;
  return <div>Unknown window</div>;
}
```

### 10.2 窗口创建

```typescript
// lib/tauri.ts
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

export async function openConnectionWindow(connectionId: string, connectionName: string) {
  const label = `connection-${connectionId}`;
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    await existing.setFocus();
    return;
  }

  new WebviewWindow(label, {
    url: `/?window=connection&id=${connectionId}`,
    title: `${connectionName} - DataZen`,
    width: 1400,
    height: 900,
    minWidth: 800,
    minHeight: 600,
    decorations: false,       // 自定义标题栏
  });
}

export async function openQueryWindow(connectionId: string, database: string) {
  const label = `query-${Date.now()}`;
  new WebviewWindow(label, {
    url: `/?window=query&connectionId=${connectionId}&db=${database}`,
    title: `查询 - ${database} - DataZen`,
    width: 1400,
    height: 900,
    minWidth: 800,
    minHeight: 600,
    decorations: false,
  });
}
```

## 8. 设计稿还原规范

### 11.1 布局尺寸对照

| 区域 | 设计稿像素 | Tailwind 实现 |
|------|-----------|--------------|
| 标题栏高度 | 40px | `h-10` |
| 工具栏高度 | 48-56px | `h-12` / `h-14` |
| 状态栏高度 | 40px | `h-10` |
| 左侧边栏宽度 | 220px (主窗口) / 280px (连接窗口) | 可拖拽，默认值 |
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
| 组件单测 | Vitest + React Testing Library | DataTable, CellRenderer, FilterBar |
| Store 单测 | Vitest | 每个 Store 的 action/state 变化 |
| 集成测试 | WebdriverIO | 窗口创建/关闭, 连接流程, 查询执行 |
| 性能测试 | WebdriverIO + Chrome DevTools | 10 万行滚动帧率, 内存占用 |
| 快照测试 | Storybook | 关键 UI 组件视觉回归 |

## 8. ER 图（Entity-Relationship Diagram）

基于 **React Flow**（`@xyflow/react`）的交互式数据库 ER 图组件。

### 8.1 组件结构

```
SqlConnectionView
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
