# RFC: 统一 Panel Store + History/Favorites config_id 迁移

> Status: Draft (Review Round 2)
> Author: AI Assistant
> Date: 2026-08-19
> Last Review: 2026-08-19 (2nd round — all Critical/High findings addressed)

## 动机

当前查询 tab 的数据分散在 `panelStore`（panel 元数据）和 `queryStore`（执行数据）两个 store 中，通过 `queryTabId` 做跨 store 关联。`queryStore` 使用按 `connectionId` 分区的 `Map<connectionId, ConnectionQueryState>` 结构，存在以下问题：

1. **跨 store ID 依赖脆弱**：`QueryPanel.queryTabId` 引用 `queryStore` 中的 tab，同步复杂且容易出 bug
2. **`activeConnectionId` 竞争**：多个 `useEffect` 竞争设置 `queryStore.activeConnectionId`，导致创建 tab 时分区错误（已产生实际 bug）
3. **`resultDetailRowIndex` 粒度不对**：按连接存储，应该按 panel 存储
4. **History 使用 runtime `connectionId`**：不具备跨会话持久性，重连后无法按连接过滤
5. **Favorites 没有连接关联**：无法按连接过滤；且当前使用 JSON 文件存储，与 history（SQLite）不一致

## 方案概述

1. 将 `queryStore` 的查询执行功能合并到 `panelStore`
2. 使用 `queryExec: Map<panelId, QueryExecState>` 存储执行数据，与 `panels[]` 平行
3. 移除 `queryTabId`，使用 `panel.id` 作为唯一标识
4. 后端 `QueryHistoryEntry.connection_id` 替换为 `config_id`
5. 后端 `FavoriteQuery` 添加 `config_id` 字段，并从 JSON 迁移到 SQLite
6. 前端 history / favorites 按 `configId` 过滤显示

## 数据结构

### panelStore（合并后）

```typescript
interface PanelStore {
  // ── Panel 元数据（轻量，TabBar/Toolbar 订阅） ──
  panels: Panel[];
  activePanelId: string | null;

  // ── 查询执行状态（重量级，仅 QueryPanel 订阅） ──
  queryExec: Map<string, QueryExecState>;  // key = panel.id

  // ── 全局查询功能 ──
  queryHistory: QueryHistoryEntry[];
  queryFavorites: FavoriteQuery[];
  historyVisible: boolean;
  favoritesVisible: boolean;

  // ── Panel 操作 ──
  addPanel: (panel: Panel, activate?: boolean) => void;
  removePanel: (panelId: string) => void;
  removeAllForConnection: (configId: string) => void;
  setActivePanel: (panelId: string) => void;
  updatePanel: (panelId: string, patch: Partial<Panel>) => void;
  closeOtherPanels: (panelId: string) => void;
  closeAllPanels: () => void;
  closePanelsToTheRight: (panelId: string) => void;
  closePanelsToTheLeft: (panelId: string) => void;

  // ── 查询执行操作 ──
  updateSql: (panelId: string, sql: string) => void;
  executeQuery: (panelId: string, params?: BindParams) => Promise<void>;
  executeSelection: (panelId: string, sql: string, params?: BindParams) => Promise<void>;
  cancelQuery: (panelId: string) => Promise<void>;
  setActiveResult: (panelId: string, idx: number) => void;
  setResultDetailRow: (panelId: string, index: number | null) => void;
  updateResultCell: (panelId: string, resultIdx: number, row: number, col: string, value: unknown) => void;
  setChartConfig: (panelId: string, config: ChartConfig) => void;
  setResultViewMode: (panelId: string, mode: 'table' | 'chart') => void;

  // ── History / Favorites ──
  loadHistory: () => Promise<void>;
  toggleHistory: () => void;
  loadFavorites: () => Promise<void>;
  addFavorite: (title: string, sql: string, configId: string) => Promise<void>;
  deleteFavorite: (id: string) => Promise<void>;
  toggleFavorites: () => void;
}
```

> **Note**: `historyVisible` / `favoritesVisible` 保持全局状态。若未来需要按连接隔离侧边栏可见性，可改为 per-panel 字段。

### QueryPanel 类型简化

```typescript
// Before
interface QueryPanel extends PanelBase {
  type: 'query';
  queryTabId: string;  // ← 移除，消除跨 store 依赖
  title: string;
}

// After
interface QueryPanel extends PanelBase {
  type: 'query';
  title: string;
  // panel.id 直接作为 queryExec 的 key
}
```

### QueryExecState

```typescript
interface QueryExecState {
  sql: string;
  results: StatementResult[];
  activeResultIdx: number;
  error: string | null;
  running: boolean;
  executionTimeMs: number | null;
  chartConfig?: ChartConfig;
  resultViewMode?: 'table' | 'chart';
  streamRunId?: number;
  resultDetailRowIndex: number | null;  // per-panel 粒度（原 per-connection）
}
```

### queryStore

完全移除。其所有功能由 `panelStore` 承担。

## Panel 生命周期与 queryExec 清理

所有 panel 移除路径都必须同步清理 `queryExec` 中的条目，避免内存泄漏。

### addPanel

```typescript
addPanel: (panel, activate = true) => {
  const nextExec = panel.type === 'query'
    ? new Map(get().queryExec).set(panel.id, emptyQueryExecState())
    : get().queryExec;
  set(s => ({
    panels: [...s.panels, panel],
    activePanelId: activate ? panel.id : s.activePanelId,
    queryExec: nextExec,
  }));
},
```

### removePanel（含 cancel running query）

```typescript
removePanel: (panelId) => {
  const { panels, activePanelId, queryExec } = get();
  // Best-effort cancel if query is running
  const exec = queryExec.get(panelId);
  if (exec?.running) {
    const panel = panels.find(p => p.id === panelId);
    if (panel) {
      queryCommands.cancelQuery(panel.connectionId).catch(() => {});
    }
  }
  const nextActive = resolveNextActive(panels, panelId, activePanelId);
  const nextExec = new Map(queryExec);
  nextExec.delete(panelId);
  set({
    panels: panels.filter(p => p.id !== panelId),
    activePanelId: nextActive,
    queryExec: nextExec,
  });
},
```

### removeAllForConnection（批量清理）

```typescript
removeAllForConnection: (configId) => {
  const { panels, activePanelId, queryExec } = get();
  // Cancel running queries for this connection
  for (const panel of panels) {
    if (panel.configId === configId && panel.type === 'query') {
      const exec = queryExec.get(panel.id);
      if (exec?.running) {
        queryCommands.cancelQuery(panel.connectionId).catch(() => {});
      }
    }
  }
  const remaining = panels.filter(p => p.configId !== configId);
  const nextExec = new Map(queryExec);
  for (const panel of panels) {
    if (panel.configId === configId) nextExec.delete(panel.id);
  }
  const activeStillExists = remaining.some(p => p.id === activePanelId);
  set({
    panels: remaining,
    activePanelId: activeStillExists ? activePanelId : (remaining.at(-1)?.id ?? null),
    queryExec: nextExec,
  });
},
```

### closeAllPanels / closeOtherPanels / closePanelsToTheRight / closePanelsToTheLeft

所有这些方法都必须同步执行：
1. Best-effort cancel 被关闭的 query panels 的 running 查询
2. 从 `queryExec` 中删除被关闭的 panel 条目

```typescript
// 通用 helper，供所有批量 close 方法复用
function cancelAndCleanupExec(
  panelsToRemove: Panel[],
  currentExec: Map<string, QueryExecState>,
): Map<string, QueryExecState> {
  const nextExec = new Map(currentExec);
  for (const panel of panelsToRemove) {
    if (panel.type === 'query') {
      const exec = nextExec.get(panel.id);
      if (exec?.running) {
        queryCommands.cancelQuery(panel.connectionId).catch(() => {});
      }
      nextExec.delete(panel.id);
    }
  }
  return nextExec;
}
```

### usePanelHandlers close 路径统一

**关键原则**：`usePanelHandlers` 中所有 close 操作只调用 `panelStore` 的方法，不再独立循环处理 `queryStore`。

```typescript
// Before（重复逻辑）
const handleCloseAllPanels = useCallback(() => {
  for (const panel of usePanelStore.getState().panels) {
    if (panel.type === 'query') closeQueryTab((panel as QueryPanel).queryTabId);
  }
  usePanelStore.getState().closeAllPanels();
}, [closeQueryTab]);

// After（统一路径）
const handleCloseAllPanels = useCallback(() => {
  usePanelStore.getState().closeAllPanels();
  // panelStore.closeAllPanels 内部已处理 cancel + queryExec 清理
}, []);
```

## 后端变更

### QueryHistoryEntry — `connection_id` → `config_id`

```rust
// src-tauri/src/store/models.rs
pub struct QueryHistoryEntry {
    pub id: String,
    pub config_id: String,       // 替换 connection_id（持久化连接配置 ID）
    pub database: String,
    pub sql: String,
    pub executed_at: DateTime<Utc>,
    pub execution_time_ms: u64,
    pub rows_affected: Option<u64>,
    pub success: bool,
    pub error_message: Option<String>,
}
```

### FavoriteQuery — 添加 `config_id`，迁移到 SQLite

```rust
// Before: JSON file 存储，无 config_id
pub struct FavoriteQuery {
    pub id: String,
    pub title: String,
    pub sql: String,
    pub created_at: DateTime<Utc>,
}

// After: SQLite 存储，添加 config_id
pub struct FavoriteQuery {
    pub id: String,
    pub config_id: String,       // 新增
    pub title: String,
    pub sql: String,
    pub created_at: DateTime<Utc>,
}
```

### History DB Migration（版本化）

引入 `schema_version` 表实现版本化迁移，避免重复执行：

```rust
// history_db.rs — 在 HistoryDb::open 中调用
fn ensure_schema_version(conn: &Connection) -> Result<i32> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);"
    )?;
    let version: i32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0)
    )?;
    Ok(version)
}

fn migrate_to_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        -- 1. 清空旧 history（旧 connection_id 跨会话无效）
        DELETE FROM query_history;
        -- 2. rename column
        ALTER TABLE query_history RENAME COLUMN connection_id TO config_id;
        -- 3. 创建 config_id 索引（按连接过滤的常规查询）
        CREATE INDEX IF NOT EXISTS idx_query_history_config_id ON query_history(config_id);
        -- 4. favorite_queries 新建表（原 favorites 在 JSON 文件中，不在 SQLite）
        CREATE TABLE IF NOT EXISTS favorite_queries (
            id TEXT PRIMARY KEY,
            config_id TEXT NOT NULL,
            title TEXT NOT NULL,
            sql TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_favorite_queries_config_id ON favorite_queries(config_id);
        -- 5. 记录版本
        INSERT INTO schema_version (version) VALUES (2);
    ")?;
    Ok(())
}
```

> **注意**：所有 SQL 字面量中的列名（`INSERT`、`SELECT`、dedup 查询等）都必须同步从 `connection_id` 改为 `config_id`。

### History 去重按 config_id 分区

当前去重逻辑查询全局最近一条 SQL，迁移后需按 `config_id` 分区：

```sql
-- Before: 全局去重
SELECT sql FROM query_history ORDER BY executed_at DESC LIMIT 1

-- After: 按 config_id 去重
SELECT sql FROM query_history WHERE config_id = ?1 ORDER BY executed_at DESC LIMIT 1
```

### Favorites 存储迁移

当前 favorites 存储在 `favorites/queries.json`（JSON 文件），需要迁移到 SQLite：

1. **新增 SQLite CRUD**：在 `history_db.rs` 中添加 `add_favorite_query`、`get_favorite_queries`、`delete_favorite_query` 方法（表 `favorite_queries`）
2. **移除 JSON 存储**：删除 `history.rs` 中的 `load_json_file::<Vec<FavoriteQuery>>("favorites/queries.json")` 相关代码
3. **移除 StoreCache 内存层**：删除 `StoreCache` 中的 `favorite_queries: Vec<FavoriteQuery>` 字段、`favorite_queries_loaded: bool` 标志、以及 `ensure_favorite_queries_loaded()` 懒加载方法。Favorites 完全走 `history_db` SQLite CRUD，不再有内存缓存层
4. **旧数据处理**：旧的 `favorites/queries.json` 中的条目**没有 `config_id`**，不具备迁移价值。首次启动时：
   - 如果 `favorites/queries.json` 存在，log warning 并重命名为 `.bak`
   - UI 首次启动 toast 通知用户「旧收藏无法迁移，请重新收藏」
5. **Release notes**：说明升级后旧收藏会丢失，用户需要重新收藏

### config_id 反查

写入 history 时需要从 runtime `connection_id` 反查持久化 `config_id`。**不需要新增方法**，复用现有 `ConnectionManager` 的 `config_id_map`：

```rust
// 封装统一反查函数，供 query.rs、driver_command.rs 和 ai.rs 共用
async fn resolve_config_id(state: &AppState, connection_id: &str) -> Result<String, CommandError> {
    state.connection_manager
        .config_id_map
        .read()
        .await
        .get(connection_id)
        .cloned()
        .ok_or_else(|| {
            tracing::warn!(connection_id, "config_id not found for connection");
            CommandError::NotFound(format!("config_id not found for connection {connection_id}"))
        })
}
```

> **重要**：解析失败时 **拒绝写入**（而非写入空 `config_id`），避免产生不可追溯的脏数据。调用方应 `?` 传播错误或 `match` 处理。

### Rust IPC 命令变更

```rust
// add_favorite_query: 增加 config_id 参数
pub async fn add_favorite_query(
    state: State<'_, AppState>,
    config_id: String,    // 新增
    title: String,
    sql: String,
) -> Result<FavoriteQuery, CommandError> { ... }

// get_favorite_queries: 可选按 config_id 过滤
pub async fn get_favorite_queries(
    state: State<'_, AppState>,
    config_id: Option<String>,  // 可选过滤
) -> Result<Vec<FavoriteQuery>, CommandError> { ... }

// get_query_history: 可选按 config_id 过滤（减少前端 filter 开销）
pub async fn get_query_history(
    state: State<'_, AppState>,
    limit: usize,
    config_id: Option<String>,  // 新增可选过滤
) -> Result<Vec<QueryHistoryEntry>, CommandError> { ... }
```

## 前端变更

### QueryPanel 组件

```typescript
// Before
interface QueryPanelProps {
  connectionId: string;
  configId: string;
  queryTabId: string;     // ← 移除
  databaseType?: string;
}

// After
interface QueryPanelProps {
  panelId: string;        // 唯一标识
  connectionId: string;
  configId: string;
  databaseType?: string;
}
```

Selector 使用：

```typescript
// Before
const tab = useQueryStore((s) => s.findTab(queryTabId));

// After — 推荐使用 useQueryExec hook 做字段级订阅
const { sql, running, results, error, ... } = useQueryExec(panelId);
```

### `useQueryExec` Hook（推荐封装）

```typescript
// src/hooks/useQueryExec.ts
import { usePanelStore } from '../stores/panelStore';
import { useShallow } from 'zustand/react/shallow';

// 模块级常量，避免每次 selector 运行创建新对象导致 useShallow 判定变化
const EMPTY_QUERY_EXEC: QueryExecState = Object.freeze({
  sql: '',
  results: [],
  activeResultIdx: 0,
  error: null,
  running: false,
  executionTimeMs: null,
  resultDetailRowIndex: null,
});

export function useQueryExec(panelId: string) {
  return usePanelStore(
    useShallow((s) => s.queryExec.get(panelId) ?? EMPTY_QUERY_EXEC)
  );
}

// 字段级订阅（避免其他字段变化触发重渲染）
export function useQueryExecField<K extends keyof QueryExecState>(
  panelId: string,
  field: K,
): QueryExecState[K] {
  return usePanelStore((s) => (s.queryExec.get(panelId) ?? EMPTY_QUERY_EXEC)[field]);
}
```

### History / Favorites 按 configId 过滤（必做）

**后端过滤**是必做项，不是可选：`loadHistory(configId)` 和 `loadFavorites(configId)` 传参给后端，后端 `WHERE config_id = ?` 过滤后返回。

```typescript
// panelStore actions
loadHistory: async (configId: string) => {
  const history = await queryCommands.getQueryHistory(100, configId);
  set({ queryHistory: history });
},
loadFavorites: async (configId: string) => {
  const favorites = await queryCommands.getFavoriteQueries(configId);
  set({ queryFavorites: favorites });
},
```

> **重要**：避免在 selector 中调用 `.filter()` 创建新数组引用（每次 selector 运行都产生新数组 → 触发订阅者全量重渲染）。后端过滤后前端直接使用 `queryHistory` / `queryFavorites`，不做二次 filter。

### usePanelHandlers

```typescript
// Before
const handleNewQuery = useCallback((initialSql?: string) => {
  createQueryTab(sidebarConnCtx.connectionId);
  const latestTab = getLastTabForConnection(sidebarConnCtx.connectionId);
  const panel: QueryPanel = {
    ...sidebarConnCtx,
    type: 'query',
    id: nextPanelId('qry'),
    queryTabId: latestTab.id,
    title: ...
  };
  addPanel(panel);
}, ...);

// After
const handleNewQuery = useCallback((initialSql?: string) => {
  const panelId = nextPanelId('qry');
  const panel: QueryPanel = {
    ...sidebarConnCtx,
    type: 'query',
    id: panelId,
    title: ...
  };
  addPanel(panel);  // addPanel 自动初始化 queryExec
  if (initialSql) updateSql(panelId, initialSql);
}, ...);
```

### PanelContentRenderer

```typescript
// Before
if (panel.type === 'query') {
  return <QueryPanel
    connectionId={panel.connectionId}
    configId={panel.configId}
    queryTabId={(panel as QueryPanel).queryTabId}
    databaseType={panel.databaseType}
  />;
}

// After
if (panel.type === 'query') {
  return <QueryPanel
    panelId={panel.id}
    connectionId={panel.connectionId}
    configId={panel.configId}
    databaseType={panel.databaseType}
  />;
}
```

### ContentView

```typescript
// Before
const activeQueryTabId = ...;
const activeQueryTab = useQueryStore(s =>
  activeQueryTabId ? s.findTab(activeQueryTabId) : undefined
);

// After
const activeQueryExec = usePanelStore(s =>
  activePanel?.type === 'query' ? s.queryExec.get(activePanel.id) : undefined
);

// resultDetailRowIndex 现在是 per-panel
const detailRowIdx = activePanel?.type === 'table'
  ? detailRowIndex  // from tableDataStore
  : activeQueryExec?.resultDetailRowIndex ?? null;
```

### ConnectionWindow

```typescript
// 移除 queryStore 同步
function syncStoresActiveConnection(connectionId: string | null) {
  useSchemaStore.getState().setActiveConnection(connectionId);
  // useQueryStore.getState().setActiveConnection(connectionId);  ← 移除
  useTableDataStore.getState().setActiveConnection(connectionId);
}

function removeConnectionFromStores(connectionId: string) {
  useSchemaStore.getState().removeConnection(connectionId);
  // useQueryStore.getState().removeConnection(connectionId);  ← 移除
  useTableDataStore.getState().removeConnection(connectionId);
}
```

### DocumentConnectionView

Document 模式约定：**每个 connection 懒创建一个隐式 query panel**。

```typescript
// Before（直接使用 queryStore）
const tab = useQueryStore((s) => s.tabs[0]);
useEffect(() => { if (!tab) createTab(); }, [tab, createTab]);

// After（走 panelStore 统一路径）
function DocumentQueryPanel({ connectionId, configId }: { connectionId: string; configId: string }) {
  const queryPanelId = usePanelStore(s =>
    s.panels.find(p => p.type === 'query' && p.connectionId === connectionId)?.id
  );
  const exec = usePanelStore(s =>
    queryPanelId ? s.queryExec.get(queryPanelId) : undefined
  );

  useEffect(() => {
    if (!queryPanelId) {
      const panelId = nextPanelId('doc-qry');
      usePanelStore.getState().addPanel({
        type: 'query',
        id: panelId,
        connectionId,
        configId,
        databaseType: ...,
        title: 'Query',
      }, false);  // activate = false，Document 模式不改变主 tab bar
    }
  }, [queryPanelId, connectionId, configId]);

  if (!queryPanelId || !exec) return null;
  return <SqlEditor sql={exec.sql} ... />;
}
```

> **注意**：Document 模式的 query panel `addPanel(panel, false)` 不激活（`activate = false`），避免影响主 ContentView 的 tab bar。

### executeQuery 合并后示例

```typescript
// panelStore 中合并后的 executeQuery
executeQuery: async (panelId, params) => {
  const { panels, queryExec } = get();
  const panel = panels.find(p => p.id === panelId);
  if (!panel || panel.type !== 'query') return;
  const exec = queryExec.get(panelId);
  if (!exec) return;

  const sql = exec.sql.trim();
  if (!sql) return;

  // 标记 running
  set(s => ({
    queryExec: new Map(s.queryExec).set(panelId, {
      ...exec,
      running: true,
      error: null,
      results: [],
    }),
  }));

  try {
    const results = await queryCommands.executeQuery(panel.connectionId, sql, params);
    set(s => ({
      queryExec: new Map(s.queryExec).set(panelId, {
        ...s.queryExec.get(panelId)!,
        running: false,
        results,
        activeResultIdx: 0,
        executionTimeMs: results[0]?.executionTimeMs ?? null,
      }),
    }));
    // 写入 history（使用 panel.configId）
    await queryCommands.addQueryHistory(panel.configId, sql, ...);
    // 刷新当前连接的 history
    get().loadHistory(panel.configId);
  } catch (err) {
    set(s => ({
      queryExec: new Map(s.queryExec).set(panelId, {
        ...s.queryExec.get(panelId)!,
        running: false,
        error: String(err),
      }),
    }));
  }
},
```

### QueryPanel 内 ResultTable 子组件适配

`ResultTable` 当前按连接级订阅 `resultDetailRowIndex`，迁移后需改为 per-panel：

```typescript
// Before
const setResultDetailRow = useQueryStore((s) => s.setResultDetailRow);
const resultDetailRowIndex = useQueryStore((s) => s.resultDetailRowIndex);

// After — 接收 panelId prop
interface ResultTableProps {
  panelId: string;
  // ...existing props
}

const resultDetailRowIndex = useQueryExecField(panelId, 'resultDetailRowIndex');
const setResultDetailRow = usePanelStore(s => s.setResultDetailRow);
// 调用: setResultDetailRow(panelId, index)
```

### addFavorite 调用链更新

所有 `addFavorite` 路径必须传入 `configId`：

```typescript
// QueryPanel 中
const handleAddFavorite = useCallback((name: string, sql: string) => {
  usePanelStore.getState().addFavorite(name, sql, configId);  // configId from props
}, [configId]);

// menu:add-favorite 事件桥接
window.addEventListener('menu:add-favorite', () => {
  const activePanel = usePanelStore.getState().panels.find(
    p => p.id === usePanelStore.getState().activePanelId
  );
  if (activePanel?.type === 'query') {
    // 从 activePanel 获取 configId
    usePanelStore.getState().addFavorite(name, sql, activePanel.configId);
  }
});
```

### 前端类型变更

```typescript
// src/types/index.ts
export interface QueryHistoryEntry {
  id: string;
  configId: string;       // 替换 connectionId
  database: string;
  sql: string;
  executedAt: string;
  executionTimeMs: number;
  rowsAffected?: number;
  success: boolean;
  errorMessage?: string;
}

export interface FavoriteQuery {
  id: string;
  configId: string;       // 新增
  title: string;
  sql: string;
  createdAt: string;
}
```

### IPC 命令变更

```typescript
// src/commands/query.ts
addFavoriteQuery: (configId: string, title: string, sql: string) =>
  invoke<FavoriteQuery>('add_favorite_query', { configId, title, sql }),

getFavoriteQueries: (configId?: string) =>
  invoke<FavoriteQuery[]>('get_favorite_queries', { configId }),

getQueryHistory: (limit: number, configId?: string) =>
  invoke<QueryHistoryEntry[]>('get_query_history', { limit, configId }),
```

## 性能保证

| 操作 | 影响范围 | 原因 |
|------|---------|------|
| 流式查询结果更新 | 仅 `useQueryExec(panelId)` 订阅者 | `panels` 引用不变，其他 panel 的 queryExec 引用不变 |
| 切换 tab | 仅 `activePanelId` | 不影响 queryExec |
| 打开/关闭 tab | `panels` + 可能 `queryExec` | 低频操作 |
| 显示 history | 后端按 configId 过滤 | 减少前端 filter 开销 |

**Zustand 不可变更新规则**：

- 更新 `queryExec` 时必须 `new Map(old).set(panelId, newState)`，不可直接 `map.set()`
- 每个 stream event 只更新目标 panel 的 `QueryExecState`，其他 key 的对象引用不变
- 仅 active panel 挂载 QueryPanel 组件（非 keep-alive），降低多 panel 同时订阅压力

**推荐 selector 模式**：

```typescript
// ✅ 字段级订阅（细粒度，避免不必要重渲染）
const sql = usePanelStore(s => s.queryExec.get(panelId)?.sql ?? '');
const running = usePanelStore(s => s.queryExec.get(panelId)?.running ?? false);

// ✅ useShallow 订阅（适合一次取多个字段）
const exec = usePanelStore(useShallow(s => s.queryExec.get(panelId)));

// ❌ 避免在 selector 中创建新对象/数组
const bad = usePanelStore(s => ({ sql: s.queryExec.get(panelId)?.sql }));
```

**DevTools 注意**：Zustand DevTools 对 `Map` 的序列化不友好。开发环境可提供 `queryExecToObject()` 辅助调试。

## `clearQueryHistory` 行为说明

当前 `clearQueryHistory()` 清空**全部** history，引入 configId 过滤后，用户在某个连接的 QueryPanel 中点击"清除历史"会清空所有连接的 history。

**决策**：保持当前全局清空行为。若未来需要按连接清空，可新增 `clearQueryHistory(configId)` 后端命令。

## 受影响文件

### 后端（Rust）

| 文件 | 变更 |
|------|------|
| `src-tauri/src/store/models.rs` | `QueryHistoryEntry.connection_id` → `config_id`；`FavoriteQuery` 添加 `config_id`；移除 `StoreCache.favorite_queries*` 字段 |
| `src-tauri/src/store/history_db.rs` | 版本化 DB migration + favorites CRUD + config_id 索引 + dedup 按 config_id 分区 |
| `src-tauri/src/store/history.rs` | 移除 favorites JSON CRUD 和 `ensure_favorite_queries_loaded`，改用 history_db；方法签名加 `config_id` 过滤 |
| `src-tauri/src/store/tests.rs` | 测试更新（QueryHistoryEntry、FavoriteQuery 构造 + 版本化 migration 测试） |
| `src-tauri/src/commands/query.rs` | 写 history 时传入 config_id（通过 `resolve_config_id`） |
| `src-tauri/src/commands/driver_command.rs` | `record_sql_command_outcome` 传入 config_id |
| `src-tauri/src/commands/ai.rs` | **`ai_analyze_queries` 按 `connection_id` 过滤 history → 需先 `resolve_config_id` 再按 `config_id` 过滤** |
| `src-tauri/src/commands/history.rs` | 测试中构造 QueryHistoryEntry 更新 |
| `src-tauri/src/commands/ai_integration_tests.rs` | 构造 QueryHistoryEntry 更新 |
| `src-tauri/src/commands/ai_mock_provider_tests.rs` | 构造 QueryHistoryEntry 更新 |
| `src-tauri/src/mcp/server.rs` | `datazen://query-history` resource 字段更新（**MCP 破坏性变更**：`connectionId` → `configId`） |
| `src-tauri/src/services/connection_manager.rs` | 添加 `resolve_config_id` 辅助函数（或复用 `config_id_map`） |
| `src-tauri/src/lib.rs` | IPC 命令注册更新（若参数变化导致签名不兼容） |

### 前端（TypeScript）

| 文件 | 变更 |
|------|------|
| `src/stores/panelStore.ts` | 合并查询执行功能 + queryExec Map + history/favorites + `cancelAndCleanupExec` helper |
| `src/stores/queryStore.ts` | **移除** |
| `src/hooks/useQueryExec.ts` | **新增**（封装 queryExec selector，模块级 `EMPTY_QUERY_EXEC` 常量） |
| `src/windows/connection/QueryPanel.tsx` | 改用 panelStore / useQueryExec；**内部 `ResultTable` 改为接收 `panelId` prop** |
| `src/windows/connection/ContentView.tsx` | resultDetailRowIndex、activeQueryTab 更新 |
| `src/windows/connection/usePanelHandlers.ts` | 移除 createQueryTab 和独立 close 循环，简化 handleNewQuery，统一走 panelStore close 路径 |
| `src/windows/connection/PanelContentRenderer.tsx` | QueryPanel props 变更 |
| `src/windows/connection/ConnectionWindow.tsx` | 移除 queryStore 同步 |
| `src/windows/connection/DocumentConnectionView.tsx` | 从 `useQueryStore(s.tabs[0])` 改为 panelStore；懒创建隐式 query panel |
| `src/commands/query.ts` | `addFavoriteQuery` 增加 configId；`getFavoriteQueries` 增加 configId 过滤；`getQueryHistory` 增加 configId 过滤 |
| `src/types/index.ts` | `QueryHistoryEntry.configId`；`FavoriteQuery.configId` |

### 测试

| 文件 | 变更 |
|------|------|
| `src/stores/__tests__/panelStore.test.ts` | 增加 queryExec 生命周期、executeQuery、cancel、history/favorites 加载测试 |
| `src/stores/__tests__/queryStore.test.ts` | **移除**（相关测试迁移到 panelStore 测试后） |
| `src/windows/connection/__tests__/ContentView.test.tsx` | 移除 queryStore mock |
| `src/windows/connection/__tests__/ConnectionWindow.test.tsx` | 移除 queryStore mock |
| `src/stores/__tests__/aiStore.test.ts` | analyze queries 行为/过滤变更测试 |
| `src-tauri/src/store/history_db.rs` tests | 版本化 migration 测试 + favorites CRUD + config_id filter + dedup 分区测试 |
| `e2e/specs/sql-query.ts` | `menu:add-favorite` 事件桥接 + favorite 按连接隔离验证 |

### 文档

| 文件 | 变更 |
|------|------|
| `docs/architecture/frontend/state.md` | 更新：移除 queryStore 描述，补充 panelStore 合并说明 |
| MCP resource CHANGELOG | `datazen://query-history` 字段 `connectionId` → `configId` breaking change |

## 迁移策略

### 阶段 1：后端 config_id 迁移

1. 引入 `schema_version` 表 + `migrate_to_v2()` 版本化迁移
2. `QueryHistoryEntry`：`connection_id` → `config_id` + config_id 索引
3. `FavoriteQuery`：添加 `config_id`，从 JSON 迁移到 SQLite + config_id 索引
4. 移除 `StoreCache` 中 favorites 相关字段和 JSON 读写逻辑
5. `resolve_config_id` 辅助函数（失败时拒绝写入）
6. 所有 history/favorites 写入点更新（包括 `ai.rs` 的 `ai_analyze_queries`）
7. History dedup 加 `WHERE config_id = ?`
8. `get_query_history(limit, configId?)` + `get_favorite_queries(configId?)` 后端过滤
9. IPC 命令签名更新
10. 后端测试更新（含版本化 migration 测试）

### 阶段 2：前端 panelStore 重构

1. `panelStore.ts` 合并 queryExec + history/favorites + `cancelAndCleanupExec` helper
2. 查询执行逻辑拆到 `queryExecActions.ts`
3. `useQueryExec` hook（含模块级 `EMPTY_QUERY_EXEC` 常量）
4. `QueryPanel.tsx` 改用 panelStore / useQueryExec；`ResultTable` 接收 `panelId`
5. `ContentView.tsx` 更新
6. `usePanelHandlers.ts` 简化（统一走 panelStore close 路径，移除独立 queryStore 循环）
7. `PanelContentRenderer.tsx` props
8. `DocumentConnectionView.tsx` 适配（懒创建隐式 query panel）
9. `ConnectionWindow.tsx` 移除 queryStore 同步
10. `queryStore.ts` 移除
11. `loadHistory(configId)` / `loadFavorites(configId)` 传参
12. `addFavorite` 调用链传入 `configId`

### 阶段 3：测试 + 文档更新

1. 前端测试迁移
2. E2E 测试验证
3. `docs/architecture/frontend/state.md` 更新
4. MCP CHANGELOG 标注 breaking change

## 不变的部分

- `tableDataStore` 和 `schemaStore` 的 per-connection 分区保持不变（表数据和 schema 确实是连接级别的）
- `PanelContentRenderer`、`ContentToolbar`、`PanelTabBar` 等 UI 组件结构基本不变
- Panel 类型系统（除 QueryPanel 简化外）不变

## 风险

| 风险 | 严重度 | 缓解措施 |
|------|--------|---------|
| Favorites 旧数据丢失 | 中 | Release notes 说明；首次启动 toast + log warning；旧文件 rename `.bak` |
| 旧 History 不可见 | 低 | 清空旧数据，明确行为 |
| `queryExec` 内存泄漏 | 高 | 所有 panel 移除路径统一走 `cancelAndCleanupExec` helper |
| 关闭 panel 不 cancel 查询 | 中 | removePanel 时 best-effort cancel |
| IPC 破坏性变更 | 中 | 前后端同步发版 |
| MCP 破坏性变更 | 中 | `datazen://query-history` 字段 rename；CHANGELOG 标注 |
| panelStore 代码量膨胀 | 中 | 查询执行逻辑拆到 `queryExecActions.ts` 纯函数（**纳入实施计划**） |
| `ai_analyze_queries` 过滤失效 | 高 | 已纳入受影响文件清单；需 `resolve_config_id` 后再过滤 |
| History dedup 跨连接误判 | 中 | dedup 查询加 `WHERE config_id = ?` |
| `resolve_config_id` 失败写入脏数据 | 高 | 解析失败返回 `Err` 拒绝写入 |
| `panel.connectionId` 可能过期 | 中 | 执行时可从 `activeConnectionStore.connections[panel.configId]` 解析最新 runtime ID（follow-up） |

## 必做增强（随本 RFC 一起实施）

- `get_query_history(limit, configId?)` 后端 `WHERE config_id = ?` 过滤
- `loadHistory(configId)` / `loadFavorites(configId)` 前端传参
- 查询执行逻辑拆到 `src/stores/queryExecActions.ts`
- `config_id` 索引（`query_history` + `favorite_queries`）
- History dedup 按 `config_id` 分区
- `StoreCache` 移除 favorites 相关字段

## 可选增强（后续迭代）

- `historyVisible` / `favoritesVisible` 按 `configId` 隔离
- `clearQueryHistory(configId?)` 支持按连接清空
- 查询执行逻辑进一步拆到 `src/stores/queryExec/` 子模块
- `panel.connectionId` 过期时从 `configId` 解析最新 runtime ID
