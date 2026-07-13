# DataZen 前端技术方案

## 一、技术选型总览

| 技术 | 版本 | 用途 |
|------|------|------|
| React | 18 | UI 框架 |
| TypeScript | 5.x | 类型安全 |
| Vite | 5.x | 构建工具 |
| Tailwind CSS | 3.x | 样式系统 |
| shadcn/ui | latest | 组件库 |
| Zustand | 4.x | 状态管理 |
| @tanstack/react-virtual | 3.x | 虚拟滚动 |
| Monaco Editor | 0.50+ | SQL 编辑器 |
| Lucide React | latest | 图标库 |
| @tauri-apps/api | 2.x | Tauri IPC |

---

## 二、项目目录结构

```
src/
├── main.tsx                    # 应用入口，Tauri 窗口路由分发
├── App.tsx                     # 根组件
├── windows/                    # 窗口级页面（一个窗口对应一个入口）
│   ├── main/                   # 主窗口 - 连接管理
│   │   ├── MainWindow.tsx
│   │   ├── ConnectionCard.tsx
│   │   ├── GroupPanel.tsx
│   │   └── EmptyState.tsx
│   ├── connection/             # 连接窗口 - 数据库浏览与编辑
│   │   ├── ConnectionWindow.tsx
│   │   ├── SchemaTree.tsx
│   │   ├── tabs/
│   │   │   ├── StructureTab.tsx
│   │   │   ├── DataTab.tsx
│   │   │   ├── IndexesTab.tsx
│   │   │   ├── ForeignKeysTab.tsx
│   │   │   └── DdlTab.tsx
│   │   └── DataToolbar.tsx
│   └── query/                  # 查询窗口 - SQL 编辑器
│       ├── QueryWindow.tsx
│       ├── SqlEditor.tsx
│       ├── ResultPanel.tsx
│       ├── QueryTabs.tsx
│       └── HistoryPanel.tsx
├── components/                 # 共享组件
│   ├── ui/                     # shadcn/ui 基础组件
│   ├── DataTable/              # 核心数据表格组件
│   │   ├── DataTable.tsx       # 表格容器
│   │   ├── VirtualBody.tsx     # 虚拟滚动行渲染
│   │   ├── TableHeader.tsx     # 表头（排序/筛选/拖拽列宽）
│   │   ├── EditableCell.tsx    # 可编辑单元格
│   │   ├── CellRenderer.tsx    # 按类型渲染单元格
│   │   └── types.ts
│   ├── Toolbar.tsx             # 通用工具栏
│   ├── StatusBar.tsx           # 状态栏
│   ├── FilterBar.tsx           # 筛选条件栏
│   └── dialogs/                # 对话框
│       ├── NewConnectionDialog.tsx
│       ├── ConfirmDialog.tsx
│       └── ExportDialog.tsx
├── stores/                     # Zustand 状态管理
│   ├── connectionStore.ts      # 连接配置管理
│   ├── activeConnectionStore.ts# 活动连接状态
│   ├── schemaStore.ts          # Schema 树状态
│   ├── tableDataStore.ts       # 表数据 & 编辑状态
│   ├── queryStore.ts           # SQL 查询状态
│   ├── settingsStore.ts        # 应用设置
│   └── uiStore.ts              # UI 临时状态
├── commands/                   # Tauri IPC 调用封装
│   ├── connection.ts
│   ├── database.ts
│   ├── query.ts
│   └── settings.ts
├── hooks/                      # 自定义 Hooks
│   ├── useVirtualTable.ts      # 虚拟滚动表格逻辑
│   ├── useCellEditor.ts        # 单元格编辑逻辑
│   ├── useKeyboardShortcuts.ts # 快捷键管理
│   ├── useResizable.ts         # 可拖拽调整大小
│   └── useTauriEvent.ts        # Tauri 事件监听
├── lib/                        # 工具函数
│   ├── tauri.ts                # Tauri API 封装
│   ├── formatters.ts           # 数据格式化
│   ├── sql.ts                  # SQL 解析/构建
│   └── cn.ts                   # className 工具
└── styles/
    ├── globals.css             # Tailwind 入口 + CSS 变量
    └── themes.css              # 亮色/暗色主题变量
```

---

## 三、状态管理设计

### 3.1 设计原则

1. **单一职责**：每个 Store 只管理一个领域的数据
2. **最小订阅**：组件通过 selector 只订阅所需的字段，避免不必要的重渲染
3. **命令与状态分离**：Tauri IPC 调用（副作用）封装在 `commands/` 中，Store 只管理状态转换
4. **无冗余派生**：可从已有状态计算出的值不单独存储，用 getter 或 `useMemo` 派生

### 3.2 Store 拆分与职责

```
┌─────────────────────────────────────────────────────────────┐
│                        前端状态全景                          │
├──────────────┬──────────────┬──────────────┬───────────────┤
│ connectionStore │ schemaStore  │ tableDataStore │ queryStore  │
│              │              │              │               │
│ - connections│ - databases  │ - rows       │ - tabs        │
│ - groups     │ - tables     │ - columns    │ - activeTabId │
│ - loading    │ - views      │ - filters    │ - results     │
│ - error      │ - expanded   │ - sorts      │ - history     │
│              │ - selected   │ - editBuffer │ - favorites   │
│              │              │ - page       │ - running     │
├──────────────┼──────────────┼──────────────┼───────────────┤
│ activeConnectionStore       │ settingsStore │ uiStore       │
│                             │              │               │
│ - connectionId              │ - theme      │ - sidebarWidth│
│ - status (connected/idle)   │ - language   │ - editorHeight│
│ - serverInfo                │ - editor     │ - activeDialog│
│ - currentDatabase           │ - shortcuts  │ - contextMenu │
└─────────────────────────────┴──────────────┴───────────────┘
```

### 3.3 核心 Store 定义

#### connectionStore — 连接配置管理

```typescript
interface ConnectionConfig {
  id: string;
  name: string;
  databaseType: 'postgresql' | 'mysql' | 'mariadb' | 'sqlite';
  host?: string;
  port?: number;
  database?: string;
  username?: string;
  password?: string;
  sslMode: 'disable' | 'prefer' | 'require';
  colorTag?: string;
  group?: string;
  lastConnectedAt?: string;
}

interface ConnectionStore {
  // --- 状态 ---
  connections: ConnectionConfig[];
  groups: string[];
  selectedGroup: string | null;      // null = 全部
  searchQuery: string;
  loading: boolean;
  error: string | null;

  // --- 派生（通过 selector 计算） ---
  // filteredConnections: 由 selectedGroup + searchQuery 计算

  // --- 操作 ---
  fetchConnections: () => Promise<void>;
  saveConnection: (config: ConnectionConfig) => Promise<void>;
  deleteConnection: (id: string) => Promise<void>;
  testConnection: (config: ConnectionConfig) => Promise<ServerInfo>;
  setSelectedGroup: (group: string | null) => void;
  setSearchQuery: (query: string) => void;
}
```

**使用方式**：

```typescript
// 组件只订阅 filteredConnections，不因 searchQuery 变更触发整棵树重渲染
const connections = useConnectionStore(
  (s) => filterConnections(s.connections, s.selectedGroup, s.searchQuery)
);
```

#### tableDataStore — 表数据 & 编辑状态

```typescript
interface CellEdit {
  rowIndex: number;
  columnName: string;
  originalValue: unknown;
  newValue: unknown;
}

interface TableDataStore {
  // --- 状态 ---
  tableName: string | null;
  columns: ColumnSchema[];
  rows: Record<string, unknown>[];   // 当前页数据
  totalRows: number;
  page: number;
  pageSize: number;
  filters: FilterCondition[];
  sorts: SortCondition[];
  editBuffer: Map<string, CellEdit>; // key = `${rowIndex}:${columnName}`
  selectedRows: Set<number>;
  editingCell: { row: number; col: string } | null;
  loading: boolean;

  // --- 操作 ---
  loadTableData: (table: string) => Promise<void>;
  setPage: (page: number) => void;
  setPageSize: (size: number) => void;
  addFilter: (filter: FilterCondition) => void;
  removeFilter: (index: number) => void;
  clearFilters: () => void;
  setSort: (sort: SortCondition) => void;
  startEdit: (row: number, col: string) => void;
  updateCell: (row: number, col: string, value: unknown) => void;
  cancelEdit: () => void;
  commitChanges: () => Promise<void>;
  discardChanges: () => void;
  toggleRowSelection: (index: number) => void;
  selectAllRows: () => void;
  deleteSelectedRows: () => Promise<void>;
}
```

**核心设计**：`editBuffer` 使用 Map 而非数组，O(1) 查找是否有未提交修改；`rows` 只持有当前页数据，避免内存膨胀。

#### queryStore — SQL 查询状态

```typescript
interface QueryTab {
  id: string;
  title: string;
  sql: string;
  result: QueryResult | null;
  error: string | null;
  running: boolean;
  executionTimeMs: number | null;
}

interface QueryStore {
  // --- 状态 ---
  tabs: QueryTab[];
  activeTabId: string;
  historyVisible: boolean;
  history: QueryHistoryEntry[];

  // --- 操作 ---
  createTab: () => void;
  closeTab: (id: string) => void;
  setActiveTab: (id: string) => void;
  updateSql: (tabId: string, sql: string) => void;
  executeQuery: (tabId: string) => Promise<void>;
  executeSelection: (tabId: string, sql: string) => Promise<void>;
  cancelQuery: (tabId: string) => Promise<void>;
  loadHistory: () => Promise<void>;
  toggleHistory: () => void;
}
```

#### settingsStore — 全局设置

```typescript
interface AppSettings {
  theme: 'light' | 'dark' | 'system';
  language: string;
  queryResultLimit: number;
  editorFontSize: number;
  editorFontFamily: string;
  confirmOnDelete: boolean;
  autoCommit: boolean;
  defaultPageSize: number;
}

interface SettingsStore {
  settings: AppSettings;
  loadSettings: () => Promise<void>;
  updateSettings: (partial: Partial<AppSettings>) => Promise<void>;
}
```

### 3.4 数据流全景

```
用户操作 (点击/输入/快捷键)
     │
     ▼
事件处理函数 (组件内 / Hook)
     │
     ├─── 纯 UI 操作 ──────▶ uiStore.setState(...)
     │
     └─── 需要后端数据 ────▶ Store Action (async)
                                │
                                ├── 1. set({ loading: true })
                                │
                                ├── 2. await commands.xxx()  ← Tauri IPC
                                │
                                ├── 3. set({ data, loading: false })
                                │
                                └── 4. (失败) set({ error, loading: false })
                                         │
                                         ▼
                              React 重渲染受影响的组件
```

### 3.5 跨窗口状态同步

Tauri 多窗口架构下，每个窗口是独立的 WebView 进程，状态不共享。通过 **Tauri Event System** 实现窗口间通信：

```typescript
// commands/tauri.ts
import { emit, listen } from '@tauri-apps/api/event';

type EventPayload =
  | { type: 'connection:updated'; data: ConnectionConfig }
  | { type: 'connection:deleted'; data: { id: string } }
  | { type: 'settings:changed'; data: Partial<AppSettings> }
  | { type: 'schema:refreshed'; data: { connectionId: string } };

export function emitGlobal(payload: EventPayload) {
  emit('datazen:global', payload);
}

export function onGlobal(handler: (payload: EventPayload) => void) {
  return listen<EventPayload>('datazen:global', (event) => {
    handler(event.payload);
  });
}
```

```typescript
// hooks/useTauriEvent.ts
export function useTauriEvent() {
  useEffect(() => {
    const unlisten = onGlobal((payload) => {
      switch (payload.type) {
        case 'connection:updated':
          useConnectionStore.getState().fetchConnections();
          break;
        case 'settings:changed':
          useSettingsStore.getState().loadSettings();
          break;
      }
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);
}
```

---

## 四、事件处理设计

### 4.1 设计原则

1. **事件分层**：UI 事件 → Handler → Store Action → IPC Command，每层职责明确
2. **Hook 封装**：复杂交互逻辑封装到 Hook 中，组件只负责绑定
3. **快捷键集中管理**：全局快捷键在窗口顶层注册，避免分散

### 4.2 键盘快捷键系统

```typescript
// hooks/useKeyboardShortcuts.ts
interface ShortcutDef {
  key: string;            // 'mod+n', 'mod+enter', 'f5', 'escape'
  scope: 'global' | 'editor' | 'table';
  action: () => void;
  description: string;
}

export function useKeyboardShortcuts(shortcuts: ShortcutDef[]) {
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const mod = e.metaKey || e.ctrlKey;
      const shift = e.shiftKey;
      const key = e.key.toLowerCase();

      for (const shortcut of shortcuts) {
        if (matchShortcut(shortcut.key, { mod, shift, key })) {
          e.preventDefault();
          shortcut.action();
          return;
        }
      }
    }
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [shortcuts]);
}
```

**各窗口快捷键注册**：

```typescript
// windows/main/MainWindow.tsx
useKeyboardShortcuts([
  { key: 'mod+n', scope: 'global', action: openNewConnectionDialog, description: '新建连接' },
  { key: 'mod+,', scope: 'global', action: openSettings, description: '打开设置' },
]);

// windows/query/QueryWindow.tsx
useKeyboardShortcuts([
  { key: 'mod+enter', scope: 'editor', action: executeQuery, description: '执行查询' },
  { key: 'mod+shift+enter', scope: 'editor', action: executeSelection, description: '执行选中' },
  { key: 'mod+shift+f', scope: 'editor', action: formatSql, description: '格式化 SQL' },
  { key: 'mod+h', scope: 'editor', action: toggleHistory, description: '查询历史' },
]);

// components/DataTable/DataTable.tsx
useKeyboardShortcuts([
  { key: 'f5', scope: 'table', action: refreshData, description: '刷新' },
  { key: 'f2', scope: 'table', action: editSelectedCell, description: '编辑单元格' },
  { key: 'escape', scope: 'table', action: cancelEdit, description: '取消编辑' },
  { key: 'delete', scope: 'table', action: deleteSelectedRows, description: '删除选中行' },
]);
```

### 4.3 单元格编辑事件流

```
双击单元格 / 按 F2
     │
     ▼
useCellEditor.startEdit(row, col)
     │
     ├── tableDataStore.startEdit(row, col)  → editingCell = { row, col }
     │
     └── 渲染 EditableCell (input 获得焦点)
          │
          ├── onChange ──▶ 本地 state 更新 (不写 Store，避免每次击键触发重渲染)
          │
          ├── Enter ────▶ tableDataStore.updateCell(row, col, value)
          │                  → editBuffer.set(`${row}:${col}`, { original, new })
          │                  → editingCell = null
          │
          ├── Tab ──────▶ updateCell → 移动到下一个单元格 → startEdit
          │
          └── Escape ───▶ tableDataStore.cancelEdit()
                           → editingCell = null (丢弃输入)
```

### 4.4 筛选/排序事件流

```
用户点击列头排序图标
     │
     ▼
tableDataStore.setSort({ column: 'name', desc: false })
     │
     ├── 1. set({ sorts: [...], page: 0 })     ← 排序变更时重置到第 1 页
     └── 2. loadTableData(tableName)            ← 重新请求后端数据
              │
              └── await commands.getTableData({
                    table, page: 0, pageSize,
                    filters, sorts               ← 筛选排序条件传给后端
                  })
```

**关键决策**：筛选/排序由后端 SQL 执行（`WHERE ... ORDER BY ...`），前端不做本地排序。原因是数据可能有百万行，前端只持有当前页数据。

---

## 五、大数据量性能方案

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

---

## 六、布局与响应式方案

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
│   Monaco SQL 编辑器                               │  ← 可拖拽高度
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

---

## 七、主题系统

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

---

## 八、Tauri IPC 通信层

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

  // 注意：需要后端新增 #[tauri::command] cancel_query（见下方补齐说明）
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

---

## 九、核心组件设计

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
└── Pagination          # 分页控制
```

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

---

## 十、窗口路由与多窗口管理

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

---

## 十一、设计稿还原规范

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

---

## 十二、测试策略

| 层级 | 工具 | 覆盖范围 |
|------|------|----------|
| 组件单测 | Vitest + React Testing Library | DataTable, CellRenderer, FilterBar |
| Store 单测 | Vitest | 每个 Store 的 action/state 变化 |
| 集成测试 | Playwright | 窗口创建/关闭, 连接流程, 查询执行 |
| 性能测试 | Playwright + Chrome DevTools | 10 万行滚动帧率, 内存占用 |
| 快照测试 | Storybook | 关键 UI 组件视觉回归 |

---

## 十三、开发阶段规划

| 阶段 | 内容 | 输出 |
|------|------|------|
| **Phase 1: 脚手架** | Vite + React + Tailwind + shadcn/ui 项目初始化；目录结构搭建；主题系统；Tauri 窗口路由 | 可运行的空壳多窗口应用 |
| **Phase 2: 主窗口** | 连接管理 Store；连接卡片/分组；新建连接对话框；连接测试 | 主窗口功能完整 |
| **Phase 3: 连接窗口** | Schema 树；表结构标签页；数据标签页（DataTable 核心）；虚拟滚动；分页 | 可浏览表结构和数据 |
| **Phase 4: 数据编辑** | 行内编辑；新增/删除行；筛选/排序；数据导出 | 完整数据编辑功能 |
| **Phase 5: 查询窗口** | Monaco 编辑器集成；查询执行/取消；结果展示；查询历史/收藏；执行计划 | 查询功能完整 |
| **Phase 6: 打磨** | 主题切换；快捷键；错误处理；性能优化；窗口间通信 | 生产就绪 |

---

## 十四、数据库类型扩展架构（2026-07 重构后）

### 14.1 设计原则

1. **注册表驱动**：`src/lib/databaseTypes.ts` 的 `DB_REGISTRY` 是所有 DB 类型行为的单一数据源
2. **路由层 if-else**：仅在组件选择边界使用 if-else，不在组件内部散布方言/类型分支
3. **策略模式**：SQL 方言逻辑集中在 `src/lib/sqlDialects/`，连接视图集中在 `src/lib/connectionViews/`

### 14.2 核心模块

```
src/lib/
├── databaseTypes.ts          # DB_REGISTRY + 行为标志 + normalizeRedisDatabaseField
├── sqlDialects/              # DDL / 索引 / 备份 方言策略
│   ├── postgresql.ts, mysql.ts, sqlite.ts
│   └── index.ts              # getSqlDialect(dbType)
└── connectionViews/          # 连接窗口视图注册表
    └── index.ts              # CONNECTION_VIEWS + getConnectionView(mode)

src/components/connection/    # 共享连接表单
├── useConnectionForm.ts      # 表单状态 + Kiwi 登录 + draft 构建
├── ConnectionFormBody.tsx    # 按 connectionForm 路由到 Fields 组件
├── KiwiConnectionFields.tsx
├── FileConnectionFields.tsx
├── IndexConnectionFields.tsx
└── StandardConnectionFields.tsx

src/windows/connection/
├── ConnectionWindow.tsx        # 薄壳：TitleBar + getConnectionView()
├── SqlConnectionView.tsx       # SQL 连接全部 UI
├── RedisConnectionView.tsx     # KV 连接 UI
└── schema-tree/
    ├── SchemaTree.tsx          # 路由（< 30 行）
    ├── StandardSchemaTree.tsx
    └── MultiDatabaseSchemaTree.tsx
```

### 14.3 DB_REGISTRY 行为标志

| 字段 | 用途 |
|------|------|
| `connectionView` | 路由到 `CONNECTION_VIEWS`（sql / keyvalue / document） |
| `connectionForm` | 路由到连接表单 Fields 组件（standard / kiwi / file / index） |
| `sqlDialect` | 路由到 `sqlDialects/` 策略 |
| `hasMultiDatabase` | 路由到 `MultiDatabaseSchemaTree` |
| `defaultPageSize` | 覆盖默认分页（如 Kiwi 1000 行） |
| `supportsBackup` | BackupWindow 过滤 + 方言备份选项 |

### 14.4 添加新 DB 类型检查清单

1. `types/index.ts` — 添加 `DatabaseType` 联合成员
2. `databaseTypes.ts` — 添加 `DB_REGISTRY` 条目（含 `connectionForm` / `connectionView`）
3. （可选）`components/connection/` — 新表单 Fields 组件
4. （可选）`connectionViews/` — 注册新视图组件
5. （可选）`sqlDialects/` — 新方言策略文件
6. （可选）`schema-tree/` — 新 Schema 树变体
