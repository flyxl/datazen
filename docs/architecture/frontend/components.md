# 前端组件与布局

> [返回架构总览](../README.md)

## 1. 大数据量性能方案

### 5.1 核心策略

| 策略           | 适用场景        | 方案                              |
| -------------- | --------------- | --------------------------------- |
| **服务端分页** | 表数据浏览      | LIMIT/OFFSET，每页 50 行          |
| **虚拟滚动**   | 查询结果 & 宽表 | @tanstack/react-virtual           |
| **延迟渲染**   | 长文本单元格    | 截断 + Tooltip                    |
| **列宽缓存**   | 表格列宽计算    | 首次测量后缓存，不每帧计算        |
| **分批 IPC**   | 大结果集传输    | 流式传输 / 分块加载               |
| **Web Worker** | JSON 解析       | 大于 1MB 的结果集在 Worker 中解析 |

### 5.2 虚拟滚动表格

查询结果可能一次返回数千到数万行，必须使用虚拟滚动：

```typescript
// hooks/useVirtualTable.ts
import { useVirtualizer } from '@tanstack/react-virtual';

interface UseVirtualTableOptions {
  rows: unknown[][];
  rowHeight: number; // 40px（与设计稿一致）
  overscan: number; // 预渲染行数，默认 10
  containerRef: RefObject<HTMLDivElement>;
}

export function useVirtualTable({
  rows,
  rowHeight,
  overscan,
  containerRef,
}: UseVirtualTableOptions) {
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

### 5.3.1 时间值的展示规则

`formatTimestamp` 只对**带时区指示**的文本做归一化；不带时区的 `date` / `time` / `datetime2`
文本原样展示，绝不经过 `new Date()` + `toISOString()`：

- 数据库返回的是墙上时间。把它当本地时间解析再输出 UTC 字符串，会让每一格整体偏移本地 UTC
  偏移量（UTC+8 下 `2026-03-01 00:15:30` 显示成 `2026-02-28T16:15:30.000Z`），DATE 列还会被补上
  `T00:00:00.000Z`。
- 判据是文本尾部是否存在 `Z` 或 `±HH:MM` 时区指示（`src/lib/formatters.ts::hasZoneDesignator`）；
  带时区的值仍按原逻辑归一化展示。

### 5.4 性能关键指标

| 指标            | 目标                  | 实现手段                           |
| --------------- | --------------------- | ---------------------------------- |
| 首屏渲染        | < 200ms               | 只渲染可见区域（虚拟滚动）         |
| 滚动帧率        | 60fps                 | overscan + CSS transform 定位      |
| 内存占用        | 当前页数据 + 虚拟窗口 | 不缓存历史页数据                   |
| 切换页响应      | < 100ms               | 加载中骨架屏，数据到达后一次性渲染 |
| 10 万行结果滚动 | 流畅无卡顿            | 虚拟列表 + memo                    |

## 2. 布局与响应式方案

### 6.1 设计原则

1. **固定 + 弹性混合布局**：标题栏/工具栏/状态栏固定高度，内容区弹性填充
2. **可拖拽分割**：侧边栏宽度、编辑器/结果区高度可拖拽调整
3. **最小尺寸保护**：每个区域设置 `min-width` / `min-height`，避免收缩到不可用
4. **不使用百分比字体/绝对像素偏移**：用 Tailwind 的 `rem` 体系 + `flex`/`grid`

### 6.2 窗口布局结构

#### 主窗口 (main → ConnectionPage)

> 旧版连接卡片启动器（`GroupPanel` / `ActionPanel` 等）已移除；主窗口现统一为 `ConnectionPage` 工作区（左侧 `ConnectionNavigatorTree` + 功能 sidebar + 内容 Tab）。

```
┌──────────────────────────────────────────────────┐
│ TitleBar + 功能 sidebar（连接 / Workflow / Dashboard / Settings）│
├──────────┬───────────────────────────────────────┤
│ 连接导航树 │  ConnectionWorkspaceHome / Tab 内容   │
│          │  （QueryPanel、TableView、WorkflowPage…）│
└──────────┴───────────────────────────────────────┘
```

<details>
<summary>已移除：旧版连接卡片启动器布局（F4）</summary>

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

</details>

#### 主工作区连接视图 (main → ConnectionPage)

连接 / Workflow / Dashboard 在同一 OS 窗口内切换；左侧 `ConnectionNavigatorTree`，右侧连接 Tab（结构 / 数据 / 查询等）。查询编辑器内联在 ContentView，**不再**使用独立 `query-window` OS 窗口。独立子窗口仅保留 `backup` / `data-sync` / `schema-diff` / `data-transfer` 四个；新建连接与设置为 main 内嵌，Docs 跳转官网。详见 [窗口管理](../windows.md)。

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
  storageKey?: string; // 持久化到 localStorage
}

export function useResizable({
  direction,
  initialSize,
  minSize,
  maxSize,
  storageKey,
}: UseResizableOptions) {
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

| 保护策略       | 实现                                                           |
| -------------- | -------------------------------------------------------------- |
| 侧边栏最小宽度 | `min-width: 180px`，拖拽时 clamp                               |
| 侧边栏最大宽度 | `max-width: 50%`（基于窗口宽度动态计算）                       |
| 编辑器最小高度 | `min-height: 120px`                                            |
| 结果区最小高度 | `min-height: 120px`                                            |
| 卡片网格自适应 | `grid-template-columns: repeat(auto-fill, minmax(280px, 1fr))` |
| 表格水平滚动   | 列多时 `overflow-x: auto`，表头固定                            |
| 工具栏折叠     | 窗口过窄时工具栏按钮收入 `...` 下拉菜单                        |
| 文字不溢出     | 所有文本使用 `truncate` + `title` tooltip                      |

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
  danger: { DEFAULT: 'var(--c-danger)' },
  dt: {
    null: 'var(--dt-null)', bool: 'var(--dt-bool)', number: 'var(--dt-number)',
    datetime: 'var(--dt-datetime)', json: 'var(--dt-json)', text: 'var(--dt-text)',
  },
},
```

**错误色必须走 `danger` token**：字面量 Tailwind `red-*`（`red-400` / `red-500/20` 等）不读任何 `--c-*` 变量，因此**对主题完全不敏感**——换主题、换外观包时它仍是同一个固定色，等于绕过主题机制。`ErrorBanner`（`packages/ui/`）的 `plain` / `boxed` / `strip` 三个 variant、其 dismiss 按钮，以及全部 14 个调用点均已改用 `text-danger` / `bg-danger/10` / `border-danger/20`。

注意这是**可见改动，不是保色重构**：原先的 `red-400` / `red-500` / `red-300` 与 dismiss 按钮的 `text-red-200` 都是固定色，迁到 token 后实际渲染色会随之变化。`scripts/__tests__/error-banner-call-site-parity.test.ts` 把 14 个调用点的布局类与**精确的**颜色类集合一并钉住。仓库内仍有约 117 处非 `ErrorBanner` 的字面量 red 内联错误条待迁移。

**DataTable / 结构视图类型色**：`src/lib/dataTypeColors.ts` 将 SQL 类型映射到 `text-dt-*`；`CellRenderer`、`StructureView`、`TableHeader`、`DetailPanel`、`ExportDialog`、`IndexesView` 共用。

### 7.3 模式切换（light / dark / system）

`settings.theme` 为 `{ mode, packId }`；`packId` 为 `null` 时仅使用 Host 内置 token。

```typescript
// settingsStore.ts — applyTheme(mode × packId)
async function applyTheme(mode: ThemeMode, packId: string | null) {
  document.documentElement.classList.toggle('dark', resolveIsDark(mode));
  await applyThemePack(packId); // 注入 pack CSS / 图标 / 字体
  syncWebviewBackgroundFromTokens();
}

// 跨窗口 / 菜单同步 mode，不写后端
export async function applyThemeLocally(mode: ThemeMode) {
  const packId = useSettingsStore.getState().settings.theme.packId;
  await applyTheme(mode, packId);
  watchSystemTheme(mode); // system 模式监听 prefers-color-scheme
}

// updateSettings({ theme: { mode, packId } }) → 持久化 + applyTheme + 跨窗口广播
```

### 7.4 扩展主题

Settings「外观」列出已启用 Wapp 的 `contributes.themes[]`；选择后 `settings.theme.packId` 为 `wapp:{wappId}:{themeId}`，由 `themePackApply.ts` 经 `read_wapp_file` 加载：

```
settings.theme.packId  →  read_wapp_file (IPC)
                      →  injectThemePackCss (<style id="datazen-theme-pack">)
                      →  register icon blob URLs + font faces
                      →  optional editor.json / charts.json overlays
```

| 模块     | 路径                                             | 职责                                                                                                       |
| -------- | ------------------------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| 应用逻辑 | `src/lib/themePackApply.ts`                      | 注入/移除 pack CSS、字体、通知跨窗口刷新；把解析后的 `--c-surface` 经 IPC 写入 `{appData}/surface-bg.json` |
| 首屏背景 | `surface-boot` extension `initialization_script` | parse 前注入上次 hex + `html.dark`；主窗口与子窗口同一路径                                                 |
| 图标解析 | `src/lib/iconResolver.ts`                        | pack → Lucide/驱动 → 占位                                                                                  |
| 组件     | `ThemedIcon`, `DbTypeBadge`                      | 消费 IconResolver                                                                                          |
| 设置 UI  | `windows/settings/AppearanceSection.tsx`         | 选择已安装扩展主题                                                                                         |

**图标解析顺序**

- 功能 UI：`pack icons[id]` → Host Lucide → 占位
- DB 角标：`pack icons["db." + type]` → 驱动默认 SVG → shortLabel 色块（`DbTypeBadge`）

**字体**

- Host 定义 `--font-sans`、`--font-mono`、`--font-editor`（`themes.css`）。
- 主题包可通过 `fonts.css` 覆盖；用户显式设置的 `editorFontFamily` **优先于** 主题 `--font-editor`。

后端安装与校验见 [运行时工作区应用](../backend/wapps.md)。

**DataTable 单元格类型色**

| CSS 变量        | Tailwind           | 用途      |
| --------------- | ------------------ | --------- |
| `--dt-null`     | `text-dt-null`     | NULL      |
| `--dt-bool`     | `text-dt-bool`     | 布尔      |
| `--dt-number`   | `text-dt-number`   | 数值      |
| `--dt-datetime` | `text-dt-datetime` | 日期/时间 |
| `--dt-json`     | `text-dt-json`     | JSON      |
| `--dt-text`     | `text-dt-text`     | 普通文本  |

Host 在 `src/styles/themes.css` 提供 light/dark 默认；主题包可在 `tokens.css` 覆盖。实现：`CellRenderer.tsx`。

## 4. Tauri IPC 通信层

### 8.1 参数名序列化约定

Tauri 使用 serde 反序列化前端传入的参数。

- **命令名**：Rust 侧 `#[tauri::command]` 函数名使用 `snake_case`（如 `get_connections`、`execute_query`）。
- **Rust 形参**：命令函数参数在 Rust 源码中为 `snake_case` 标识符（如 `connection_id`、`default_file_name`）。
- **前端 `invoke` 键名**：`src/commands/*` 中传参对象键名使用 **camelCase**（如 `{ connectionId, sql }`、`{ defaultFileName }`）。Tauri 2 会将 camelCase 键名映射到 Rust 的 snake_case 形参。
- **嵌套结构体**：请求/响应 DTO 在 Rust 侧通常标注 `#[serde(rename_all = "camelCase")]`，前后端字段均为 camelCase（如 `ConnectionConfig`、`MultiQueryResult`）。

**约定**：新增 IPC 时，前端 `invoke` 第二参数与 TypeScript 类型保持一致（camelCase）；Rust 命令形参保持 snake_case；复杂 struct 在 Rust 侧显式 `rename_all = "camelCase"`。不要在前端混用 snake_case 键名（如 `connection_id`），现有 `src/commands/` 中已无此类用法。

示例（摘自 `commands/connection.ts`；术语：`connectionId` = 持久化配置连接 id，`dbSessionId` = 运行时会话 id）：

```typescript
invoke<string>('connect', { connectionId }); // 返回运行时 dbSessionId
invoke<boolean>('ping_connection', { dbSessionId });
```

### 8.2 命令封装

> 以下接口与后端 [IPC 命令层](../backend/commands.md) 逐一对齐。

```typescript
// commands/connection.ts
import { invoke } from '@tauri-apps/api/core';

export const connectionCommands = {
  getConnections: () => invoke<ConnectionConfig[]>('get_connections'),

  saveConnection: (config: ConnectionConfig) => invoke<void>('save_connection', { config }),

  deleteConnection: (id: string) => invoke<void>('delete_connection', { id }),

  testConnection: (config: ConnectionConfig) => invoke<ServerInfo>('test_connection', { config }),

  // 入参为持久化配置连接 id（connectionId），返回运行时会话 id（dbSessionId）
  connect: (connectionId: string) => invoke<string>('connect', { connectionId }),

  pingConnection: (dbSessionId: string) => invoke<boolean>('ping_connection', { dbSessionId }),

  releaseConnection: (dbSessionId: string) =>
    invoke<boolean>('release_connection', { dbSessionId }),

  disconnect: (dbSessionId: string) => invoke<void>('disconnect', { dbSessionId }),
};
```

```typescript
// commands/database.ts
import { invoke } from '@tauri-apps/api/core';

export const databaseCommands = {
  getDatabases: (dbSessionId: string) => invoke<string[]>('get_databases', { dbSessionId }),

  getTables: (dbSessionId: string, database: string) =>
    invoke<TableInfo[]>('get_tables', { dbSessionId, database }),

  getTableSchema: (dbSessionId: string, table: string) =>
    invoke<TableSchema>('get_table_schema', { dbSessionId, table }),
};
```

```typescript
// commands/query.ts
import { invoke } from '@tauri-apps/api/core';

// SQL 查询统一经 Driver Command IPC 执行（dbSessionId 标识目标会话）
export const queryCommands = {
  executeQuery: async (dbSessionId: string, sql: string) => {
    const result = await invoke<{ data: MultiQueryResult }>('execute_driver_command', {
      dbSessionId,
      command: 'query',
      input: { sql },
    });
    return result.data;
  },

  getExplain: (dbSessionId: string, sql: string) =>
    invoke<ExplainResult>('get_explain', { dbSessionId, sql }),

  cancelQuery: (dbSessionId: string) => invoke<void>('cancel_query', { dbSessionId }),

  getQueryHistory: (limit: number, connectionId?: string) =>
    invoke<QueryHistoryEntry[]>('get_query_history', { limit, connectionId }),

  clearQueryHistory: () => invoke<void>('clear_query_history'),
};
```

```typescript
// commands/settings.ts
import { invoke } from '@tauri-apps/api/core';

export const settingsCommands = {
  getSettings: () => invoke<AppSettings>('get_settings'),

  saveSettings: (settings: AppSettings) => invoke<void>('save_settings', { settings }),
};
```

### 8.3 错误处理统一

```typescript
// lib/tauri.ts
export class TauriError extends Error {
  constructor(
    public code: string,
    message: string,
  ) {
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

- `@datazen/driver-sdk`（`packages/driver-sdk/src/nativeContextMenu.ts`）：唯一实现——`NativeMenuItemDef` / `normalizeNativeMenuItems` / `nativeEditMenuItems` / `showNativeContextMenu(items, {x,y})` / `bindContextMenuBridge`；宿主 `src/lib/nativeContextMenu.ts` 仅为薄再导出（存量 import 兼容），驱动与宿主统一从包名取用
- `src/stores/contextMenuStore.ts`：`showWebContextMenu`；模块加载时调用 `bindContextMenuBridge({ show: showWebContextMenu, hide })` 完成挂载点注入
- `src/components/ui/WebContextMenu.tsx`：`WebContextMenuHost`（`App.tsx` 挂载）
- `src/lib/contextMenuPosition.ts`：根菜单与**二级菜单**在视口右/下边缘翻转或 clamp，面板不得被窗口裁切

禁止再使用 `@tauri-apps/api/menu` 的 `Menu.popup()`。调用方必须传入 `clientX/clientY`（或 Schema 树 payload 的 `x/y`）。

各场景通过独立 builder 组装 `NativeMenuItemDef[]`，再调用 `showNativeContextMenu` / `showWebContextMenu`：

| Builder              | 路径                                                           | 调用方           |
| -------------------- | -------------------------------------------------------------- | ---------------- |
| SQL 编辑器           | `src/lib/sqlEditorContextMenu.ts`                              | `QueryPanel`     |
| Schema 树            | `src/lib/schemaTreeContextMenu.ts`                             | `ContentView`    |
| DataTable            | `src/lib/dataTableContextMenu.ts`                              | `DataTable`      |
| 连接 Tab             | `src/lib/connectionTabContextMenu.ts`                          | `ContentView`    |
| 收藏 / 历史侧栏      | `src/lib/querySidebarContextMenu.ts`                           | `QueryPanel`     |
| Workflow 列表 / 历史 | `src/lib/workflowListContextMenu.ts`                           | `WorkflowPage`   |
| ER 节点              | `src/lib/erNodeContextMenu.ts`                                 | `ErDiagramView`  |
| Redis Key            | `packages/drivers/redis/ui/key-browser/redisKeyContextMenu.ts` | `RedisWorkbench` |
| 主窗口连接/分组      | `src/lib/mainWindowContextMenu.ts`                             | `ConnectionPage` |

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

| 模块                | 职责                                                 |
| ------------------- | ---------------------------------------------------- |
| `fieldInference.ts` | 基于列名和采样值推断字段类型                         |
| `recommend.ts`      | 基于字段组合的规则引擎，推荐图表类型和轴配置         |
| `transform.ts`      | 直接映射 / 聚合模式数据转换，支持分组和排序          |
| `colors.ts`         | 5 套内置配色方案（default/warm/cool/neon/pastel）    |
| `format.ts`         | 千分位数值格式化、百分比格式化、轴刻度格式化         |
| `nlConfig.ts`       | 自然语言解析图表配置指令（"换成饼图"、"按销量排序"） |
| `export.ts`         | PNG（html-to-image）/ SVG 导出                       |

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

各 webview 加载同一 HTML，通过 URL `?window=`（`getWindowKind()`）区分。连接 / Workflow / Dashboard / Settings 统一进 `main`；Docs 跳转官网，无 in-app 子窗口：

```typescript
// windowKind.ts — legacy aliases map to main
const LEGACY_MAIN_ALIASES = new Set(['connection', 'workflow', 'dashboard', 'settings', 'docs']);

// App.tsx（示意）
switch (getWindowKind()) {
  case 'main': return <MainPage />; // ConnectionPage | SettingsPage | NewConnectionDialog（零连接空状态由 ConnectionWorkspaceHome 承接）
  // backup / data-sync / schema-diff
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

| 区域         | 设计稿像素                  | Tailwind 实现   |
| ------------ | --------------------------- | --------------- |
| 标题栏高度   | 40px                        | `h-10`          |
| 工具栏高度   | 48-56px                     | `h-12` / `h-14` |
| 状态栏高度   | 40px                        | `h-10`          |
| 左侧边栏宽度 | 220–280px（主工作区导航树） | 可拖拽          |
| Tab 栏高度   | 40px                        | `h-10`          |
| 表格行高     | 40-48px                     | `h-10` / `h-12` |
| 卡片圆角     | 12px                        | `rounded-xl`    |
| 输入框高度   | 36px                        | `h-9`           |
| 输入框圆角   | 6px                         | `rounded-md`    |
| 按钮高度     | 32px                        | `h-8`           |
| 按钮圆角     | 6px                         | `rounded-md`    |

### 11.2 色彩对照（暗色主题）

| 设计稿色值 | 用途                        | Tailwind                        |
| ---------- | --------------------------- | ------------------------------- |
| `#0f172a`  | 主背景                      | `bg-slate-900`                  |
| `#1e293b`  | 次背景 (侧边栏/表头/工具栏) | `bg-slate-800`                  |
| `#334155`  | 边框/分割线                 | `border-slate-700`              |
| `#f1f5f9`  | 主文字                      | `text-slate-100`                |
| `#94a3b8`  | 次文字                      | `text-slate-400`                |
| `#64748b`  | 占位/禁用文字               | `text-slate-500`                |
| `#3b82f6`  | 主色调/链接/选中            | `text-blue-500` / `bg-blue-500` |
| `#22c55e`  | 成功/active 状态            | `text-green-500`                |
| `#f59e0b`  | 警告/pending 状态           | `text-amber-500`                |
| `#ef4444`  | 错误/inactive/删除          | `text-red-500`                  |
| `#c084fc`  | SQL 关键字                  | `text-purple-400`               |
| `#fbbf24`  | SQL 数字                    | `text-amber-300`                |
| `#8b5cf6`  | 时间类型                    | `text-violet-500`               |

### 11.3 字体对照

| 场景      | 设计稿                 | CSS                                                                 |
| --------- | ---------------------- | ------------------------------------------------------------------- |
| UI 文字   | Inter 13-15px          | `font-sans text-sm`                                                 |
| 代码/数据 | JetBrains Mono 12-13px | `font-mono text-xs` / `font-mono text-sm`                           |
| 表头      | Inter 12px 600         | `text-xs font-medium text-slate-400`                                |
| 标签文字  | Inter 11px 600 spacing | `text-[11px] font-semibold tracking-wider uppercase text-slate-400` |

## 9. 测试策略

| 层级         | 工具                           | 覆盖范围                                                                                      |
| ------------ | ------------------------------ | --------------------------------------------------------------------------------------------- |
| 组件单测     | Vitest + React Testing Library | DataTable, CellRenderer, FilterBar（Host `src/`）                                             |
| 驱动 UI 单测 | Vitest                         | `packages/drivers/<id>/ui/__tests__/`（`pnpm test:unit:drivers`，不进 Host `pnpm test:unit`） |
| Store 单测   | Vitest                         | 每个 Store 的 action/state 变化                                                               |
| 集成测试     | WebdriverIO                    | 窗口创建/关闭, 连接流程, 查询执行；驱动深度 E2E 在 `packages/drivers/<id>/e2e/`               |
| 性能测试     | WebdriverIO + Chrome DevTools  | 10 万行滚动帧率, 内存占用                                                                     |
| 快照测试     | Storybook                      | 关键 UI 组件视觉回归                                                                          |

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
    │   └── 列级连接点 — 仅对参与连线的列渲染，左右各 2 个（正/反向）
    ├── interactionState.ts — 位置固定 / 悬停强调 / 标签落位（纯函数）
    ├── FK 连线 — 动画箭头 + 列名标签，端点落在 FK 列行
    ├── 搜索栏 — 按表名搜索并高亮匹配节点（其余变暗）
    ├── 导出 — PNG / SVG（基于 html-to-image）
    └── 统计面板 — 表数量 + 关系数量
```

### 8.2 核心模块

| 文件                  | 职责                                                                  |
| --------------------- | --------------------------------------------------------------------- |
| `ErDiagramView.tsx`   | 主视图组件，获取 ER 数据、渲染画布、导出/搜索控制                     |
| `er/TableNode.tsx`    | React Flow 自定义节点，渲染表名 + 列 + PK/FK 标记，支持折叠           |
| `er/buildErGraph.ts`  | `TableSchema[]` → React Flow nodes/edges 转换（含焦点过滤、预测关系） |
| `er/nodeMetrics.ts`   | 节点尺寸**唯一来源**：宽度、表头、列行、折叠页脚、滚动上限            |
| `er/layoutErGraph.ts` | 基于 dagre 的**分层（拓扑）布局**                                     |

### 8.3 数据流

1. 后端 `get_er_data(db_session_id, database)` 批量获取所有表的 `TableSchema`（含外键）
2. `buildErGraph(schemas, focusTable?, predicted?, collapsedTables?)` 生成 nodes 和 edges；
   节点只带**显式尺寸**，坐标由下一步决定
3. `focusTable` 控制焦点模式：仅显示目标表及其直接关联表
4. `layoutErGraph` 按外键方向分层定位
5. React Flow 渲染，支持交互和导出

### 8.4 布局：为什么是分层而不是网格

早期实现是手写网格 —— 按后端返回顺序（`ORDER BY relname`，即字母序）把表
`i` 放在 `(i % cols) * 300, floor(i / cols) * 高度`。字母序与外键拓扑毫无关系，
实测后果：

- **同一行内 y 跨度 672px**（12 表），因为行距用的是**节点自身**高度
- 宽表在上时与下方节点**真实重叠 192px**
- 10 条边跨越**无关节点 18 次**，而 React Flow 的边层在节点之下，穿越处被遮挡

现在改用 dagre 分层布局（`rankdir: 'LR'`）：边方向是 child → parent，
LR 会把 parent 排到右侧，于是边从子表右侧出发、进入父表左侧 —— 与
`TableNode` 既有的 `source`(右) / `target`(左) 句柄方向一致，常见边是直线而非
S 形绕行。同一套形态实测：**反向边 7/10 → 0/10，跨越 18 → 0，重叠 0**。

两条必须同时成立的约束：

1. **尺寸必须显式**。`nodeMetrics.ts` 是唯一来源，`TableNode` 把每个值作为
   inline height 应用（依赖全局 `box-sizing: border-box`，故 `height` 含边框）。
   若二者漂移，布局就会按「不是实际渲染的尺寸」去排 —— 这正是旧网格把 40 列
   的表预留 1000px、实际只渲染 340px 的原因。E2E `ER-010` 用真实 DOM 边界盒
   断言零重叠，是这条约束的兜底。
2. **折叠必须重排**。折叠改变节点高度，因此折叠状态提升到视图层
   （`collapsedTables`）并作为 `buildErGraph` 的输入，折叠/展开都会重跑布局；
   否则被折叠节点仍占着旧footprint，展开后会压到邻居。

预测关系也参与布局，但权重低于声明约束（`DECLARED_WEIGHT` 2 vs
`PREDICTED_WEIGHT` 1）：推测不应比约束更强烈地扭曲图。

### 8.5 连线端点：指向列而不是节点中点

早先每个节点只有**一个** `source`（右）与**一个** `target`（左）句柄，位置固定
在侧边垂直中点 —— 30 列的表，FK 线指向第 ~15 行附近，而不是真正的外键列。

现在按列生成连接点，但**只给参与连线的列**（`buildErGraph` 从边反推出
`handleColumns`）：若每列都渲染，几百张表的 schema 会有上万个句柄。每个参与列
渲染 4 个变体（`source` 左/右、`target` 左/右），因为**有环时关系会朝左**。

方向由布局后的位置决定，而不是猜：分层 `LR` 布局下每条边都跨越不同 rank，因此
**必然是水平的** —— 在无环 / 长链 / 有环三种形态下实测，vertical 边均为 0。于是
只需比较两节点中心的 x：目标在右则从子表右侧出、进父表左侧，反之镜像。

节点级句柄（`node:s-l/s-r/t-l/t-r`）保留为兜底：折叠的表没有渲染任何列行，
自引用也没有左右可言。

### 8.6 为什么取消列列表的内部滚动

连接点必须位于节点内**可预测**的偏移处，而滚动容器里的行会随 `scrollTop` 移动 ——
被滚出视口的列，其句柄会把连线端点甩到节点之外、压在其他节点上。**精确的列级
连线与内部滚动互斥**，本模块保留前者。

代价是宽表会变成很高的节点（60 列 = 1478px），因此 `defaultCollapsedTables` 在
**初次加载时**把超过 `ER_AUTO_COLLAPSE_COLUMNS`(30) 列的表播种为折叠。注意这是
**播种而非规则**：折叠判定只读调用方传入的集合，否则阈值会在每次重排时把表重新
折叠，chevron 变成单向的 —— 用户永远无法展开宽表。

### 8.7 交互状态（`er/interactionState.ts`）

视图状态变换全部抽成**纯函数**，便于测试 —— 渲染 React Flow 需要浏览器才能量出任何
几何信息，把逻辑埋在组件里就等于不可测。

- **手工位置固定**：`applyPinnedPositions` 把用户拖拽过的节点位置叠加到新布局之上，
  只覆盖被钉住的节点。schema 加载 / 折叠 / 聚焦都会重排，没有这一步每次都会丢掉
  手工摆放。`重新布局` 按钮（`er-diagram-relayout`）清空钉子。
- **悬停强调**：`hoveredNeighbourhood` 收集被悬停表及其**双向**关联表，
  `applyHoverToEdges` / `applyHoverToNodes` 加粗关联边、压暗其余。
  用**悬停而非点击**：点击节点会在工作区打开该表并离开 ER 视图，点击驱动的强调
  用户根本看不到。
- **标签按空间落位**：`fitEdgeLabel` 依据实际水平间距决定标签。实测 `ranksep=110`
  时间距恒为 110px，约容纳 16 字符 —— 单列外键全部放得下，复合外键
  （`tenant_id, account_id` 需 138px）放不下。此时**缩短而非丢弃**：退化为
  `tenant_id +1`（即连线实际连接的那一列 + 其余数量），必要时再截断加省略号。
  省略号自身占一个字符额度，预算必须扣除，否则窄间距下截断结果仍会溢出。

**聚焦模式本就自动重排**：`visibleSchemas` 先过滤再交给 `buildErGraph` 布局，
因此聚焦即重排，无需额外处理。

## 9. PathInput 控件

`packages/ui/src/PathInput.tsx`（`@datazen/ui` 导出）— 统一的路径输入/选择控件：

- 左侧：文本输入框（可手动输入路径）
- 右侧：「浏览」按钮，由 `onBrowse` 注入的原生选择器完成选择（见 §9.1）
- `dialogOptions` 透传给选择器：`{ directory, filters, title, defaultPath, multiple }`
- 消费方：设置（AI 上下文目录、日志目录、MCP Server 命令）、连接表单（SQLite
  数据库文件、SSH 私钥、跳板机私钥）、Redis 驱动 TLS 证书

### 9.1 原生选择器由宿主注入

`PathInput` 是纯视图：它自己不认识 Tauri，只在点击「浏览」时调用
`onBrowse: PathPicker`，实现由宿主提供（`src/lib/pathPicker.ts`，全应用唯一一处
`open()` 调用）。`onBrowse` 是**必填**属性 —— 一个点了没反应的浏览按钮比编译错误
更难排查，因此任何漏改的调用点都会直接 `tsc` 报错。

连接表单里的路径字段通过 `ConnectionFormState.pickPath` 传递，驱动（如 Redis 的
TLS 证书）因此也能拿到宿主选择器。驱动包自身不 import 宿主的 `src/**`、宿主
Store 或兄弟 DataZen 包；`packages/drivers/**` 中唯一一处直接 import 宿主运行时的
是 `redis/ui/observe/PubSubPanel.tsx`（`@tauri-apps/api/event` —— 驱动本就运行在
宿主 webview 内，用它监听 Redis 事件），该点不在 §9.2 的守卫范围内。

### 9.2 设计系统纯净性由脚本强制

`@datazen/ui` 被宿主、每个驱动和每个扩展打包，其中若干运行在没有 Tauri
webview 的环境里。因此设计系统必须是依赖图里的**叶子**：只允许 React、纯样式/图标
第三方包（`react` / `react-dom` / `clsx` / `lucide-react` / `tailwind-merge`）与
包内自身，不允许出现宿主运行时、宿主 Store 或兄弟 DataZen 包。该约束由两个脚本
同时执行：

| 脚本                                         | 规则                                | 覆盖                            |
| -------------------------------------------- | ----------------------------------- | ------------------------------- |
| `scripts/check-module-layers.mjs`            | `LAYER_RULES` 的 `packages/ui` 条目 | 相对路径解析后的子树 + 裸包前缀 |
| `scripts/check-driver-import-boundaries.mjs` | `RULES.R4`（blocking）              | 同上，规则表见脚本头部          |

两个脚本**共用扫描面与检测能力，而不是互相兜底**：

- 共用 `scripts/lib/scanTargets.mjs` 的扫描面（`SCAN_EXTENSIONS` 六种后缀、
  `SKIP_DIR_NAMES` 跳过的 vendored/生成目录）。二者曾各写一份，结果是
  `packages/ui/dist/**` 只有一个脚本会报、`.mjs` 只有一个脚本会看；声明在同一处
  之后，它们无法在「看哪些文件」这件事上漂移。（范围仅限这两个 UI 边界守卫：
  `scripts/check-id-terminology.mjs` 有自己的第三份声明，因为它要扫 `.rs`，那是
  另一种差异面，不应被强行合并。）
- 共用 `scripts/lib/scanSourceCode.mjs` 的分词器，扫描文件内的**全部字符串
  字面量**，因此普通 `import` / `export … from`、动态 `import()`、`require()`、
  `vi.mock()` 都会被检出，**注释**不会被误判。新增一类破坏方式时只改规则表，
  不需要改检测逻辑。

**「扫全部字面量」不等于「扫到就算违规」**：两个守卫都是**前缀匹配**，且比较的是
规范化后的值——`forbiddenPackage()` 取 `specifier.startsWith(prefix)`，
`isForbidden()` 要求解析后的仓库相对路径等于禁用前缀或落在其之下。所以禁用名
必须出现在**说明符的开头**：

| 写法                                                   | 是否报                                          |
| ------------------------------------------------------ | ----------------------------------------------- |
| `import { open } from '@tauri-apps/plugin-dialog'`     | 报（`@tauri-apps/` 是字面量的头）               |
| `const A = '../../../src/stores/settingsStore'`        | 报（`..` 开头才会被解析，解析后落在 `src/` 下） |
| `const HINT = '@tauri-apps/plugin-dialog'`             | 报（仍是开头）                                  |
| `const HINT = 'See @tauri-apps/plugin-dialog'`         | **不报**（`See ` 在前，`startsWith` 不成立）    |
| `const A = 'prefix ../../../src/stores/settingsStore'` | **不报**（不以 `.` 开头，不解析）               |
| `// A comment may name a plugin`                       | **不报**（注释被分词器抹掉）                    |

最后三行不是漏洞而是设计：分词器无法区分「字符串正文」与「说明符」，若改成
`includes`，那么任何提到过 `@tauri-apps/` 的文案、错误提示、迁移说明都会变成
阻断项，守卫当天就会被人加豁免。**真实导入不可能把禁用前缀写在后面**，所以按
前缀匹配既覆盖了全部绕过方式，又把散文留在门外。放进变量再引入仍然会被报，
因为字面量本身还在开头（`const NAME = '@tauri-apps/plugin-dialog'` 报）。

**看不见的形态有两种，性质不同：**

1. **把说明符拆成多段拼接**——`const p = '@tauri-' + 'apps/plugin-dialog'` 不报
   （两段都不以禁用前缀开头）。分词器逐个看字面量，看不到它们之间的关系。
2. **带插值的模板字符串**——``const p = `${'@tauri-apps'}/plugin-dialog` `` 不报。
   分词器把整个模板连同它的静态片段一起丢掉，所以连可比较的字符串都不产出。
   注意**没有插值的模板仍然看得见**（``import(`@tauri-apps/plugin-dialog`)`` 两个
   守卫都报），盲区是插值本身，不是反引号。

钉住这两条形态的断言有**两条，都在分词器层**，都在
`scripts/__tests__/check-driver-import-boundaries.test.mjs`：

- ``skips `${}` templates (computed specifiers cannot be judged statically)``
  ——第 2 条。断言分词后 `literals` 里只剩普通字符串 `'./keep'`，带插值的模板
  连静态片段都不产出。
- `collects single-quoted, double-quoted and static template literals with lines`
  ——第 2 条的**另一半**，也就是「没有插值的模板仍然看得见」这半句。断言
  ``import(`./c`)`` 会作为 `{ value: './c', line: 3 }` 进入 `literals`。两个守卫
  走的是 `scripts/lib/scanSourceCode.mjs` 里同一个 `scanCode`，所以这条同时钉住
  `check-module-layers.mjs` 读到的字面量。

**但没有端到端钉住**：守卫套件里没有任何一条把反引号禁用说明符喂给规则，
所以上面「两个守卫都报」这半句是在真实探针文件上实测的，不是用例保证的。

第 1 条**没有任何用例钉住**，只能靠 code review。§9.2 的变异表是 6 行——静态
import、动态 `import()`、`require()`、`vi.mock()`、相对路径爬进 `src/`、
`@datazen/*` 兄弟包——**这 6 行里没有一行是拼接，也没有一行是模板**。

这个行为有对应用例钉住，而且钉的是**反面**：
`scripts/__tests__/check-driver-import-boundaries.test.mjs` 的
`passes a design-system file that only depends on React and on itself` 断言
`"const title = 'uses @tauri-apps/plugin-dialog only in prose';"` 得到
`code === 0`、`err === ''`；
`scripts/__tests__/check-module-layers.test.ts` 的
`does not fire on the design system’s own imports (no false positives)` 里也写着
`"const label = 'pick a @tauri-apps/plugin-dialog path';"` 并期望干净。
改这个语义必须同时改那两条用例，且要先想清楚代价。

规则豁免（`ALLOWLIST`）只存在于 `check-driver-import-boundaries.mjs`，它只压
特定 `(rule, file, specifier)` 三元组，改之前先问「这是谁的代码」。

规则逻辑本身仍是两份独立实现，**没有**「一个变弱另一个会拦住」的保证：删掉 R4，
边界脚本会安静下来而 layer 脚本照常拦，反之亦然。防这件事的是
`scripts/__tests__/` 里的变异用例（把真实违规文件写进真实 `packages/ui/` 树，跑完
再删掉），不是另一个脚本。

一处**已知且刻意保留**的不对称：`check-driver-import-boundaries.mjs` 会把
gitignored 文件里的 blocking 判定降级为 advisory（未跟踪的 codegen / Pro EP 本就
不是本仓库的代码），`check-module-layers.mjs` 没有这层判断，会照报。当前
`packages/ui/` 与 `src/lib/relationMetadata/` 下没有任何 gitignored 文件
（`git ls-files --others --ignored --exclude-standard` 为空），差异是休眠的；没有
补齐，是因为它服务的 git 驱动/Pro EP 树都在 `packages/drivers/` 下而不在
`packages/ui/`，而补齐会让 layer 守卫开始依赖 `git` 在 PATH 上，换来零当前覆盖。

### 9.3 测试 fixture 的子集断言收窄了什么

连接表单相关的测试里，只实现一部分字段的 stub 一律写成
`Pick<ConnectionFormState, …>` 具名子集 + `satisfies` + 末尾一次显式
`as unknown as`（AGENTS.md「只实现子集就用精确断言」）。它相对裸 `as` 的收益是
可验证的：Pick 列表里键名写错、字面量里键名写错、值类型写错三种都会报错，裸
`as` 三种全部照单全收；clipboard 路径真正用到的字段改名也会在 fixture 处报错。

但它**不是**闭合集，有两条已知静默：给 `ConnectionFormState` 新增一个这些 fixture
不提供的必填字段时，诊断只落在真正用它的文件上，Pick 白名单本身不报；从 Pick
列表里删掉一个键也不报（`...overrides: Partial<ConnectionFormState>` 让字面量里
剩下的键都变成已知键，抑制了多余属性检查）。因此它是「收窄了检查面」，不是
「关闭了检查面」——新增必填字段时，类型错误会出现在消费它的测试/实现里，而不是
自动出现在每个 stub 上。

## 10. 闭集 props 组件的 `data-*` 透传契约

设计系统中**自声明 props 列表**的组件（`Select` 等，不继承 DOM `*Attributes`）
必须实现 `packages/ui/src/dataAttrs.ts` 的 `DataAttrProps` 契约：调用方传入的
任意 `data-*` 属性原样透传到**唯一可交互元素**上，`data-testid` 也不例外。
契约本身（`DataAttrProps` / `splitDataAttrs`）已从 `@datazen/ui` 的 barrel 导出，
后续组件直接实现同一份契约，不要在包内重新声明。

`Select` 的落点：非 `searchable` 时是 `<button aria-haspopup="listbox">`；
`searchable` 时是 combobox `<input>`。**不允许落在包裹用的 `<div>` 上** ——
定位到一个不可点击、不可输入的外层壳，等于把同一个缺陷下移一层；E2E 因此
不再需要 `[data-testid="x"] input` 这种穿透写法。

**当前覆盖：上面这条规则目前只有 `Select` 实现。** 同样是闭集 props 的
`Label`、`Tabs`、`Dialog`、`PathInput`、`Slider` 尚未实现，属于**明确延后**，
延后理由是它们当前**没有任何调用点需要 `data-*` 定位** —— 全部调用点
（`Label` 74 处、`Dialog` 62 处、`PathInput` 9 处、`Slider` 3 处、`Tabs` 1 处）
中，带 `data-*` 属性的为 0 处，经 JSX 展开传入的也为 0 处，因此实现契约对
现有代码零收益，等出现第一个需要定位的调用点时再实现。规则本身仍然是**新写
闭集 props 组件时的强制要求**，上表只是记录当前达成度，不是豁免。

`PathInput` 与 `Dialog` 各自有一条既有定位通道，但**两者机制不同、结论也
不同，不能合并陈述**：

- **`PathInput` 是真正已覆盖的。** 它的 `inputTestId?: string` 在
  `packages/ui/src/PathInput.tsx:52` **无条件**落到 `data-testid`，
  不经 `tid()`，因此**任何构建里都存在**（普通构建、`VITE_E2E` 构建、
  生产构建均同）。对 `PathInput` 而言，契约的意图已由既有机制真正满足。
- **`Dialog` 只在 E2E 构建里有定位符，仍未实现契约。** 它的
  `testId?: string` 在 `packages/ui/src/Dialog.tsx:113` 是经 `tid()` 应用的，
  而 `tid()`（`packages/ui/src/tid.ts:6`）返回
  `import.meta.env.VITE_E2E ? { 'data-testid': id } : {}` ——
  **该属性只在 `VITE_E2E` 构建下存在，其他构建一律为空对象**。所以在
  正常生产构建里，`Dialog` 既没有本契约，也没有任何自己的定位符。
  上面「零调用点」这条延后理由对 `Dialog` 依然成立且未变；E2E-only 这个事实
  说明的是它**不紧急**，**不是**说它已经完整。

`Dialog` 的 `testId` 与 `PathInput` 的 `inputTestId` 都是既有 API，
本节不建议改动它们，也不建议改 `tid()` 或 `VITE_E2E` 开关。

真正待补的是 `Label`、`Tabs`、`Slider` —— 这三个连专门的定位 prop 都没有，
任何构建下都无法被定位。

契约两端都很窄，这是刻意的：

- **类型侧**用**模式索引签名** ``[key: `data-${string}`]: string | undefined``，
  而不是 `[key: string]`。TypeScript 的多余属性检查认这个 `data-` 前缀，
  因此 `data-testid` / `data-foo` 通过，而 `dataTestId`（丢了连字符）与
  `onchane`（拼错已声明 prop）**仍然是编译错误**。换成宽索引签名就是把
  响亮的类型错误换成静默失效的 prop。
- **运行时侧** `splitDataAttrs()` 只放行 `data-*`，其余键一律丢弃并（仅开发
  构建）`console.warn` 点名。非 `data-*` 的 prop 若经 `...rest` 透传会落到
  DOM 节点上，触发 React 未知属性告警；`{...rest}` 又不受多余属性检查保护，
  所以开发期告警是覆盖「展开写法」这条路径的唯一护栏。

## 11. 复制反馈（useCopyFeedback）

`packages/ui/src/useCopyFeedback.ts`（`@datazen/ui` 导出）— 统一的「已复制」确认：

```ts
const { copied, copy } = useCopyFeedback(feedbackMs); // feedbackMs 必填，无默认值
```

契约（不得随意更改）：

- **乐观**：点击立刻置位，不等 `navigator.clipboard.writeText` 的 Promise。
- **失败回滚**：写入以**任何**方式失败都回到「复制」态，`copy()` 本身永不抛。
- **按请求绑定**：`copy()` 自增 `requestId`，迟到的失败既不会抹掉后续成功的标记，
  也不会动后续调用的定时器。
- **卸载清理**：组件卸载时 `clearTimeout` 掉未到期的窗口。

第二条的关键在写法。直接写 `navigator.clipboard.writeText(text).catch(…)` 是**错的**：
`navigator.clipboard` 缺失时 TypeError 在**同步**求值阶段就抛出，`.catch` 还没挂上——
回滚永远不会执行，异常还会逃进 React 事件处理器，按钮则顶着「已复制」显示满整个窗口
却什么都没复制。hook 用一个 IIFE 把「读属性 + 调用」整体包在 `try` 里，同步抛就转成
一个 rejected promise 交给既有回滚体：

```ts
const write = ((): Promise<void> => {
  try {
    return navigator.clipboard.writeText(text);
  } catch {
    return Promise.reject(new Error('clipboard write unavailable'));
  }
})();

void write.catch(() => {
  /* requestId 守卫 + 清定时器 + setCopied(false)，原样不动 */
});
```

**`writeText` 仍然是同步调用的**，这点是刻意的。改成
`Promise.resolve().then(() => navigator.clipboard.writeText(text))`（把读属性推迟一个
微任务）同样能成立，但会把 `writeText` 挪出点击那一轮——19 个文件、48 条既有断言
观测的正是这个时序，而写入本身并不是要修的东西。

回归覆盖三种「同步抛」形态（`packages/ui/src/__tests__/useCopyFeedback.test.tsx`）：
`navigator.clipboard` 为 `undefined`、该属性被 `delete`、`writeText` 自身同步抛，
三者都断言「不抛且回滚」。

第三条、第四条是存在的原因：React 18 取消了「卸载后 setState」告警，泄漏的定时器**完全
静默**，既不报错也不留痕。回归测试因此不比对源码，而是 `spyOn(window, 'setTimeout' /
'clearTimeout')` 指认出这次点击排的句柄，再断言卸载时该句柄确实到达了
`clearTimeout`（`src/test/copyFeedbackHarness.ts` 的 `spyOnWindowTimers()`）。
直接比较 `getTimerCount()` 前后的差值并不可靠——挂载本身也可能排队定时器，卸载会把
它们一并清掉，那个下降与复制窗口无关。

**多行场景的组合方式**：hook 只返回一个布尔量，行/块 id 仍由调用方自己保存，渲染时
两者同时成立才算命中（`const copiedRowId = copied ? copiedId : null`）。这样回滚会顺带
丢掉过期标记，迟到失败不会把标记甩回旧行。

### 收敛的 13 个站点

**时长按站点传入，不改默认值**（hook 无默认值可改，`feedbackMs` 是必填位置参数）：
1500ms（`AiCodeBlock`、`WorkflowChatPanel`、`ProgressLog`、`QueryErrorPanel`）、
2000ms（`SqlPreview`、`Nl2SqlPanel`、`ExecutionSummaryCard`、`McpSettingsSection`、
`McpPromoBar`、`GlobalQueryHistoryDialog`、`RecentQueriesList`、`ConnectionWorkspaceHome`）、
1200ms（Redis `KeyHeaderRow`——它的 `data-copied` 是驱动专属按钮态，窗口本来就是
1200ms，改了就是改用户可见行为）。

收敛前的实际形态（逐站点核对基线得到，不是抽样）：

| 类别                                                                   | 站点                                                                                                                                                                                                                                  |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **卸载时定时器泄漏**（句柄直接丢弃）                                   | `AiCodeBlock`、`WorkflowChatPanel`、`SqlPreview`、`GlobalQueryHistoryDialog`、`McpSettingsSection`、`McpPromoBar`、`RecentQueriesList`、`ConnectionWorkspaceHome`、`ExecutionSummaryCard`、`QueryErrorPanel`、`KeyHeaderRow`（11 处） |
| **卸载时定时器泄漏**（句柄存进 ref，但只在再次点击时清，从不随卸载清） | `Nl2SqlPanel`（第 12 处）                                                                                                                                                                                                             |
| **本来就没有泄漏**（ref + 卸载清理俱全）                               | `ProgressLog`                                                                                                                                                                                                                         |
| **悲观写入**（`await writeText()` 之后才置位）                         | `SqlPreview`、`McpSettingsSection`、`ProgressLog`、`KeyHeaderRow`（4 处）                                                                                                                                                             |

即 13 个站点里 **12 个在卸载时泄漏定时器**，只有 `ProgressLog` 本来就是安全的。

**用户可见的行为变化**只有三类，其余一律保持原样：

1. **悲观 → 乐观**（上表 4 处）：慢剪贴板上按钮不再有反馈延迟。
2. **失败回滚**：原先「写入被拒也永远显示已复制」的站点现在会回滚。
3. **悲观 → 乐观的副作用**：写入被拒时按钮会**先闪一下「已复制」再回落**。基线在这条
   失败路径上全程不显示。这只出现在失败路径，是乐观语义的必然代价（暴露窗口 = 一个
   event-loop turn），不是新 bug。

原先写作 `navigator.clipboard?.writeText` 的站点（`KeyHeaderRow`、
`GlobalQueryHistoryDialog`、`ConnectionWorkspaceHome`、`McpPromoBar`、
`RecentQueriesList`、`ExecutionSummaryCard`）不再静默跳过，而是走同一条回滚路径。

**已知限制：本 hook 没有剪贴板降级链。** 它只走 `navigator.clipboard.writeText`，
没有 Tauri `write_clipboard` invoke，也没有 `document.execCommand('copy')` 兜底。
降级实现在 `src/lib/fetchRelationDdl.ts`（已提交测试 `fetchRelationDdl.test.ts` 证明
WebKit 会抛 `NotAllowedError`），**搬不进 `packages/ui`**——它依赖 `@tauri-apps/api`，
而设计系统禁止该导入，`check-module-layers` 会拦。需要降级的站点必须继续用那个 helper。

### 尚未收敛的站点（待办，不是「已解决」）

以下三处**理由成立、本轨未处理**，留作后续：

1. **`CompareSummary`**（`src/windows/data-sync/CompareSummary.tsx`）——它其实**有**复制
   反馈：`useState(false)` + 裸 `setTimeout(..., 2000)` + 渲染时切文案，是第 14 个手搓
   且同样泄漏的站点。不套 hook 的真正原因是**写入在父组件**（`onCopyReport` 回调上抛），
   hook 负责执行写入，硬套会写两次剪贴板。它需要的是**把写入下沉到组件内**，再套 hook，
   而不是直接套 hook。
2. **`SchemaDiffWindow`**（`src/windows/schema-diff/SchemaDiffWindow.tsx`）——
   `ClipboardFeedback` 是三种 kind（`'summary' | 'sql' | 'config'`，`config` 是文件保存
   确认而非剪贴板确认），共用一个状态槽；失败走 `setError(...)` 错误态
   （`schemaDiff.clipboardFailed` / `exportConfigFailed`）。hook 的二值 `copied` +
   单一回滚表达不了三态与错误面。
3. **`DDLView`**（`src/windows/connection/DDLView.tsx`）——依赖上面那条三级降级链。
   降级搬不进 `packages/ui`，所以这条不能在本轨解决；要么保持现状，要么另开一条
   允许 `packages/ui` 触达 Tauri 的设计决策。

另有一批调用点**本就没有复制反馈**（`DataTransferWindow`、`ErrorBoundary`（class 组件，
用不了 hook）、`WorkflowPage`、`DataTable`、`ErDiagramView`、`QuerySidebarSection`、
`DataSyncWindow`、Redis `ValueViewer` / `useKeyRowActions`）或用的是异类反馈
（`SqlSnippetsCard` 走 toast）。给它们加反馈属于新增功能，不在收敛范围内。

## 12. Checkbox / Radio：原子控件的抽取边界

`packages/ui/src/Checkbox.tsx` 与 `Radio.tsx` 抽取自数据迁移三件套
（Schema Diff / Data Sync / Data Transfer）此前各自手写的
`<input type="checkbox">` / `<input type="radio">`。两个组件的 props 都是
`Omit<InputHTMLAttributes<HTMLInputElement>, 'type'>` —— `type` 被剔除后由
组件自己固定，调用方无法误传。

**抽取的边界是「原子控件」，不包含外壳**。两个组件都只渲染裸 `<input>`，
**不自带 `<label>` 包裹**：调用方保留自己的 `<label>` 外壳，因为它同时承担
三件事 —— 点击热区、无障碍标签关联、以及把 `data-testid` 放在预期的位置。
把外壳一并收进组件会让这三件事的调用点全部改写，`data-testid` 契约也就跟着
漂移，收益不抵风险。因此 §10 的 `data-*` 契约在这两个组件上**天然不适用**：
它们是开放 props 组件，`data-*` 由 `...props` 原生透传，不需要 `splitDataAttrs`。

视觉上刻意**不强制盒子尺寸**。`Checkbox` / `Radio` 只给 `accent-accent` 与
`focus-visible` 焦点环，尺寸交给调用方的 `className` 决定 —— 这三件套里
`h-3.5 w-3.5`、`mt-0.5`、不设尺寸三种写法都存在且各有布局含义，统一成
一个高度只会让密集表格行（DiffDetail、MappingPanel）与向导表单
（ColumnMappingEditor、OptionsBar）二者不可兼得。

## 13. 三件套的公共组件收敛

数据迁移三件套此前跳过了设计系统收敛，本次补齐。判定规则按**是否存在忠实
等价物**划分，不是按元素名：

| 元素 | 处置                                                                 |
| ---- | -------------------------------------------------------------------- |
| 文本/数字/搜索输入 | `Input`（密集行用 `className` 收窄为 `h-7 ... text-xs`）           |
| checkbox / radio  | `Checkbox` / `Radio`（§12）                                            |
| 计划/部署切换条   | bar-only `Tabs` + `getTabClassName` 复现下划线选中态                  |
| 自转 spinner      | `Spinner`（size/tone 档位与原 `Loader2` 类一对一）                     |
| 错误提示          | `ErrorBanner`                                                          |
| 包装 `<label>`     | **保留原样**（点击热区 + 布局，见 §12）                               |
| `<textarea>`       | 保留原样（设计系统尚无 `Textarea`）                                     |
| `<table>` 系列    | 保留原样（DiffDetail、SchemaDiffPlanPanel 的数据表）                   |
| 强调色链接按钮    | 保留原样（`text-accent hover:underline`、黄色警示按钮）                |
| 整行列表项按钮    | 保留原样（TableListPanel、SchemaDiffTableListPanel、TransferMappingStep） |

### 13.1 Spinner：三件套内 19 处自转图标

三件套 + `components/migration` 此前手写了 19 处
`<Loader2 className="h-N w-N animate-spin …">`，全部换为 `<Spinner />`。
`Spinner` 的 `size` 档位是**按这批真实调用点的盒宽反推**的
（`md` = `h-3.5`、`lg` = `h-4`、`xl` = `h-5`、`2xl` = `h-6`），`tone` 同样
一对一对应原手写的 `text-accent` / `text-fg-muted` / 继承当前色，因此渲染
出来的盒子尺寸与颜色**逐处不变**。

全部 19 处都**不传 `label`**。设计系统明确禁止在 `<button>` 内传 `label`
（`Spinner.tsx:58`）：无障碍名称计算会走进后代，把隐藏文本拼进控件自身的
可访问名，按钮会被念成 "DeployLoading" 之类的怪串。这些 spinner 绝大多数
就坐在按钮里（next / execute / deploy），旁边还有 `t('transfer.executing')`
这类真实文案承载语义，走装饰性默认形态是正确选择。

### 13.2 ErrorBanner：只收错误，不收警告

7 处纯错误提示换为 `ErrorBanner`，覆盖 SchemaDiffDeployPanel、
SchemaDiffPlanPanel、SqlPreview、DataTransferWindow、
MigrationRunHistoryDialog。

其中 `SchemaDiffPlanPanel` 的两处原本用**裸 `red-500` / `red-400`**
（`bg-red-500/10` / `border-red-500/30`），不是 `danger` token。这是主题盲区
—— 字面 Tailwind 色阶不随 `--c-danger` 走，**换主题包不会变色**。收敛到
`ErrorBanner` 的 `boxed` variant 后改走 `bg-danger/10` / `border-danger/20`，
接入主题机制。**这是有意的可见变更，不是纯清理** —— 与 `ErrorBanner.tsx:29`
记录的 dismiss 按钮同属一类。

`role="alert"` 由 `ErrorBanner` 固定提供，调用点原有的 `role="alert"`
不再重复传。原先靠 `getByRole('alert')` 定位的测试不受影响。

**amber 警告一律不动。** `ErrorBanner.tsx:85` 明确写了它不用于警告：
amber / `role="alert"` 的提示是另一种消息、另一种含义，走错误组件会贴错标签。
`text-amber-600 dark:text-amber-400`（OptionsBar、ExecuteBar、EndpointsBar、
MappingPanel、CompareSummary、DataTransferWindow）与 `text-warning` 同理保留。

`SchemaDiffRightPanel` 的两标签切换条是唯一收敛的 `Tabs` 调用点：bar-only
模式（item 不带 `content`），`getTabClassName` 复现原有的下划线选中样式，
两个 `data-testid`（`schema-diff-plan-tab`、`schema-diff-deploy-tab`）搬进
`item.testId`，定位契约不变。

`data-testid` 净变化为 0 —— 全部原样搬入公共组件，唯一的两处删改是上述标签
testid 从手写 `data-testid` 属性改为 `Tabs` 的 `testId` prop，最终 DOM 上仍是
同一个属性。

## 14. 开发阶段规划

| 阶段                    | 内容                                                                                   | 输出                   |
| ----------------------- | -------------------------------------------------------------------------------------- | ---------------------- |
| **Phase 1: 脚手架**     | Vite + React + Tailwind + shadcn/ui 项目初始化；目录结构搭建；主题系统；Tauri 窗口路由 | 可运行的空壳多窗口应用 |
| **Phase 2: 主窗口**     | 连接管理 Store；连接卡片/分组；新建连接对话框；连接测试                                | 主窗口功能完整         |
| **Phase 3: 连接窗口**   | Schema 树；表结构标签页；数据标签页（DataTable 核心）；虚拟滚动；分页                  | 可浏览表结构和数据     |
| **Phase 4: 数据编辑**   | 行内编辑；新增/删除行；筛选/排序；数据导出                                             | 完整数据编辑功能       |
| **Phase 5: 查询窗口**   | CodeMirror 编辑器集成；查询执行/取消；结果展示；查询历史/收藏；执行计划                | 查询功能完整           |
| **Phase 6: 打磨**       | 主题切换；快捷键；错误处理；性能优化；窗口间通信                                       | 生产就绪               |
| **Phase 7: 图表可视化** | Recharts 集成；5种图表类型；智能推荐；轴配置；NL调整；导出PNG/SVG                      | 查询结果可视化         |
