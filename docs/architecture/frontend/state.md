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
| `tableDataStore` | 表数据、筛选、排序、分页和编辑 |
| `panelStore` | 主工作区 Panel、Query 状态和结果 |
| `workspaceTabsStore` | 工作区 Tab |
| `aiStore` | AI 配置/会话相关状态 |
| `dashboardStore` | Dashboard 状态 |
| `extensionStore` | Extension 状态 |
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
