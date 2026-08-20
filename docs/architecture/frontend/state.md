# 前端状态管理

> [返回架构总览](../README.md)

## 1. 技术选型

| 技术 | 版本 | 用途 |
|------|------|------|
| React | 18 | UI 框架 |
| TypeScript | 5.x | 类型安全 |
| Vite | 5.x | 构建工具 |
| Tailwind CSS | 3.x | 样式系统 |
| shadcn/ui | latest | 组件库 |
| Zustand | 4.x | 状态管理 |
| @tanstack/react-virtual | 3.x | 虚拟滚动 |
| CodeMirror | 6.x | SQL 编辑器（按 databaseType 动态切换方言） |
| Lucide React | latest | 图标库 |
| @tauri-apps/api | 2.x | Tauri IPC |

## 2. 项目目录结构

```
src/
├── main.tsx / App.tsx          # 入口；按 windowKind 分发
├── windows/
│   ├── connection/             # 统一主工作区壳（连接 / Workflow / Dashboard 导航）
│   │   ├── ConnectionWindow.tsx
│   │   ├── ConnectionNavigatorTree.tsx
│   │   ├── ConnectionWorkspaceHome.tsx
│   │   ├── ContentView.tsx / QueryPanel.tsx / …
│   │   └── schema-tree/
│   ├── main/                   # MainWindow → 直接渲染 ConnectionWindow
│   ├── workflow/ / dashboard/  # 工作区内嵌视图
│   ├── settings/ / backup/ / data-sync/ / schema-diff / docs / new-connection/
├── components/
│   ├── DataTable/              # CellRenderer 使用 text-dt-*（主题 --dt-*）
│   ├── ui/ / chart/ / ai/ …
├── stores/                     # Zustand（connection / panel / schema / settings / …）
├── commands/                   # Tauri IPC 封装
├── lib/                        # windowManager、windowKind、themePackApply、databaseTypes…
└── styles/themes.css           # Host --c-* / --dt-* / --cm-* 默认 token
```

## 3. 状态管理设计

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
│ connectionStore │ schemaStore  │ tableDataStore │ panelStore    │
│              │              │              │               │
│ - connections│ - databases  │ - rows       │ - panels      │
│ - groups     │ - tables     │ - columns    │ - activePanelId│
│ - loading    │ - views      │ - filters    │ - queryExec   │
│ - error      │ - expanded   │ - sorts      │ - history     │
│              │ - selected   │ - editBuffer │ - favorites   │
│              │              │ - page       │               │
├──────────────┼──────────────┼──────────────┼───────────────┤
│ activeConnectionStore       │ settingsStore │ uiStore       │
│                             │              │               │
│ - connectionId              │ - theme      │ - sidebarWidth│
│ - status (connected/idle)   │ - language   │ - editorHeight│
│ - serverInfo                │ - editor     │ - activeDialog│
│ - currentDatabase           │ - shortcuts  │ - isFullscreen│
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

#### panelStore — 面板 + 查询执行状态

统一管理所有连接的面板（Tab）和查询执行状态。面板元数据（轻量）和查询执行数据（重量级）分开存储。

详细设计参见 [Unified Panel Store RFC](../../architecture/rfc/unified-panel-store.md)。

```typescript
type Panel = TablePanel | ViewPanel | QueryPanel | DatabaseObjectPanel | RedisDbPanel;

interface QueryExecState {
  sql: string;
  results: StatementResult[];
  activeResultIdx: number;
  error: string | null;
  running: boolean;
  executionTimeMs: number | null;
  chartConfig?: ChartConfig;
  resultViewMode?: 'table' | 'chart';
  resultDetailRowIndex: number | null;
}

interface PanelState {
  panels: Panel[];                          // 轻量面板元数据
  activePanelId: string | null;
  queryExec: Map<string, QueryExecState>;   // 重量级查询状态（panelId → state）
  queryHistory: QueryHistoryEntry[];
  queryFavorites: FavoriteQuery[];
  historyVisible: boolean;
  favoritesVisible: boolean;
}

interface PanelActions {
  addPanel: (panel: Panel, activate?: boolean) => void;
  removePanel: (panelId: string) => void;
  removeAllForConnection: (configId: string) => void;
  setActivePanel: (panelId: string) => void;
  updatePanel: (panelId: string, patch: Partial<Panel>) => void;
  closeOtherPanels: (panelId: string) => void;
  closeAllPanels: () => void;

  updateSql: (panelId: string, sql: string) => void;
  executeQuery: (panelId: string, params?: BindParams) => Promise<void>;
  cancelQuery: (panelId: string) => Promise<void>;
  loadHistory: (configId?: string) => Promise<void>;
  loadFavorites: (configId?: string) => Promise<void>;
  addFavorite: (title: string, sql: string, configId: string) => Promise<void>;
  deleteFavorite: (id: string) => Promise<void>;
  // ...more actions
}
```

辅助 hook `useQueryExec(panelId)` 提供字段级订阅，避免 Map 变更导致的不必要重渲染。

#### settingsStore — 全局设置

```typescript
interface ThemePreference {
  mode: 'light' | 'dark' | 'system';
  packId: string | null;
}

interface AppSettings {
  theme: ThemePreference;
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

## 4. 事件处理设计

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
