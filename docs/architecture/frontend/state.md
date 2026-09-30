# 前端状态管理

> Source of truth: `src/stores/`、`src/commands/` 和当前 `src/windows/`。

DataZen 前端使用 React + TypeScript + Zustand。状态按领域拆分，Tauri IPC wrapper 集中在 `src/commands/`。

## 1. Store

当前主要 Store：

| Store | 职责 |
|---|---|
| `connectionStore` | 持久化连接配置、分组等 |
| `activeConnectionStore` | 当前运行连接及 `dbSessionId` |
| `schemaStore` | database/schema/table/object 元数据 |
| `tableDataStore` | 表数据、筛选、排序、分页和编辑（按 `panelId` 分片） |
| `panelStore` | 主工作区 Panel、Query 状态和结果 |
| `workspaceTabsStore` | 工作区 Tab |
| `aiStore` | AI 配置/会话相关状态 |
| `dashboardStore` | Dashboard 状态 |
| `wappStore` | Workspace App 状态 |
| `settingsStore` | 应用设置 |
| `uiStore` | UI 层状态 |
| `contextMenuStore` | Context Menu 状态 |

不要在组件中复制这些 Store 已经拥有的领域状态。

## 2. IPC 数据流

```text
UI event
  ↓
component / hook
  ↓
Store action
  ↓
src/commands/*
  ↓
Tauri IPC
  ↓
Rust command
  ↓
Store / service / domain / Driver
  ↓
IPC result/event
  ↓
Zustand state
  ↓
React
```

纯 UI 状态可以直接进入 Zustand，不需要经过 Rust。

## 3. Panel 与 Query

`panelStore` 是主工作区 Panel 的统一状态入口。Query result 属于 Query Panel execution state，而不是另建一个独立页面级状态源。

Schema Diff、Data Sync、Data Transfer 是独立子窗口，各自维护自己的流程状态。

## 4. Connection / Session

前端同样遵循 `connectionId` / `dbSessionId` 区分：

- connectionId：连接配置的稳定身份。
- dbSessionId：本次运行时数据库会话。

不得把 connectionId 当作数据库 session handle 使用。

## 5. 跨窗口同步

Tauri 多窗口的 WebView 不共享 Zustand 内存。需要同步的数据通过 Tauri Event 传播，各窗口再刷新自己的 Store。

## 6. 测试

Store tests 位于 `src/stores/__tests__/`；窗口级测试位于对应 `src/windows/**/__tests__/`。

## 7. Schema 身份与异步读取

公共元数据类型由 `packages/driver-sdk/src/types/schemaMetadata.ts` 定义，宿主 `src/types/index.ts` 重导出兼容类型。`schemaClient` 使用统一 Driver Command 网关，列读取返回带完整 `RelationRef` 的结果；列名与类型从同一份列定义派生。

`schemaStore` 的会话状态保存按 database 分组的 `tableCatalogs` 和按 session/database/schema/name 编码的 `relationColumns`。`relationKey` 使用 JSON tuple，名称中的分隔符不会导致身份碰撞。列名和类型只保存于 `relationColumns`，编辑器通过 `useConnectionColumnMaps` 在消费边界按上下文派生，投影使用 memo 保持引用稳定；同名关系身份不唯一时不猜测 schema。Query Builder 从绑定 database/schema 的关系缓存取值。

`schemaRequestTracker` 为目录请求和会话生命周期提供写入屏障。切库、断连、表目录替换后，失效请求不能改写新状态；后台目录发布只更新目标 database 快照。SDK bridge 显式传递 session，不通过切换 active session 发布数据。

结构与 DDL 缓存共用 `schemaResourceCache` 的 TTL、失败退避和请求去重。失效操作移除缓存与在途请求，旧 promise 的返回和清理不能覆盖或删除新请求。编辑器关系缓存另外检查 database/schema 上下文与请求身份。显式刷新补全先失效前端缓存，再调用后端刷新，并在成功后重新加载目录。

`schemaStore` 顶层只持有 `schemas`、`activeDbSessionId` 和 action，不复制活动会话字段，也不维护默认会话或 `dbSessionId` 别名。所有会话 action 必须显式传入 `dbSessionId`；切换活动指针独立于更新会话数据。组件通过 `useConnectionSchemaField` 读取绑定会话，缺失会话返回稳定的空快照。

宿主目录调用方通过 `databaseCommands.listTables` 接入 `schemaClient.listCatalog`，database 列表通过 `schemaClient.listDatabases` 获取。`TableInfo[]` 转换与空 schema sentinel 只在旧树模型适配边界生成。结构视图、索引、外键、导出、结构编辑器与 Query Builder 共用 `getCachedTableSchema`，其传输入口为 `read_relation_schema`。SQL 生成使用精确裸名称与独立 schema，完整结构不可用时读取同一关系的 typed columns；不再按数据库类型拼接名称并重试。QueryPanel 通过 `useConnectionSchemaField` 读取绑定 session 的字段。
