# ID 术语规范 — connectionId 与 dbSessionId

> [返回架构总览](README.md)

本文是 `connectionId` 与 `dbSessionId` 的唯一权威定义：**`connectionId` 是持久化连接配置 ID；`dbSessionId` 是 Runtime 创建的运行时数据库会话 ID。** 两者不可互换，也没有双模回退。

## 1. 定义

| | `connectionId` / `connection_id` | `dbSessionId` / `db_session_id` |
|---|---|---|
| 含义 | 一条持久化连接配置的 ID（`ConnectionConfig.id`） | 一次 Runtime session 的 ID |
| 存储 | Store 的连接配置，落盘 | Runtime 内存态，永不落盘 |
| 生命周期 | 配置删除时结束 | Runtime session 显式关闭、空闲回收或进程退出时结束；失效后不能用同一 ID 透明重建 |
| 前端形态 | `connectionId` | `dbSessionId` |
| Rust 形态 | `connection_id` | `db_session_id` |

## 2. 生命周期

### `connectionId`

- 新建连接配置时由前端生成（`src/components/connection/shared.tsx` 的 `newId()`），通过 `save_connection` 持久化。
- 重启及修改配置内容不会改变 ID。
- 删除连接配置时生命周期结束；持久化配置的归属、权限和调度语义均使用此 ID。

### `dbSessionId`

- Runtime 的 connection use case 创建 session 并生成 `dbSessionId`；桌面 `PlatformAdapter` 装配 `RuntimeConnectionUseCases` 与 `DesktopSessionBackend`，后者从 Store 解析连接配置并经 DriverRegistry 打开物理资源。
- Runtime `SessionRegistry` 管理会话状态；`DesktopSessionBackend` 管理物理资源、预算与随资源生存的隧道句柄。
- 正常业务活动顺延空闲期限。桌面默认期限为 30 分钟、扫描间隔 15 秒；detach 后收紧到 60 秒 grace。到期后 Runtime 回滚并关闭资源，失效的 ID 返回 `SessionLost`，必须显式打开新 session 才能继续操作。
- 物理资源被关闭后，不会通过 `connectionId` 映射以同一个 `dbSessionId` 自动重连。

当前实现入口：[`PlatformAdapter`](../../src-tauri/src/platform/adapter.rs)、[`Runtime idle policy`](../../modules/src/execution/application/orchestration/idle.rs)、[`DesktopSessionBackend`](../../src-tauri/src/platform/session_backend.rs)。

## 3. 前端绑定

- `src/stores/activeConnectionStore.ts`：连接状态以 `connectionId` 为 key，`ConnectionEntry` 保存当前 `dbSessionId`；连接成功后广播 `datazen:connection-ready { connectionId, dbSessionId }`，断开时将该 session 交给 Host 关闭。
- `src/stores/panelStore.ts`：SQL 面板同时保存 `{ connectionId, dbSessionId }`。执行和取消使用 `dbSessionId`；查询历史与收藏按 `connectionId` 过滤。

## 4. 流转

```text
connections.json / Store
        │ connectionId
        ▼
Runtime connection use case
        │ 创建 Runtime session
        ├── SessionRegistry：dbSessionId、owner、生命周期
        └── DesktopSessionBackend：Store profile → DriverRegistry → 物理资源
                                      └── 隧道随物理资源持有
        │
        ▼
前端绑定 { connectionId, dbSessionId }
        │
        ├── Query / Schema / Driver Command：使用 dbSessionId
        └── idle deadline 到期：rollback + close → SessionLost
```

## 5. 选择哪个 ID

| 场景 | 使用 | 理由 |
|---|---|---|
| 连接配置 CRUD、权限与凭据来源 | `connectionId` | 语义指向可持久化配置 |
| Workflow 目标、Monitor 任务、MCP / AI 工具入参 | `connectionId` | 配置和调度引用需跨重启稳定 |
| 查询历史 / 收藏过滤、UI 连接归属 | `connectionId` | 记录属于哪份配置 |
| 已建立会话上的 SQL、取消、流式查询、Schema 浏览、Driver Command | `dbSessionId` | 操作必须绑定到具体 Runtime session |

一句话：先问“哪个连接”还是“哪条会话”。前者用 `connectionId`，后者用 `dbSessionId`。运行时 ID 永不落盘，也不能在空闲回收后复活。

## 6. MCP / Workflow 与 GUI 的边界

MCP、Workflow 和 AI database tools 的配置入口接收 `connectionId`，由共享 feature / application 端口解析配置并创建或借用 Runtime 管理的执行资源。GUI 已建立会话后的 IPC 接收 `dbSessionId`；将 `connectionId` 传入会话命令会明确失败，不会隐式建连。

Wapp 的 command bridge 接收 `connectionId`，由 Host 按 bridge 契约解析活动会话；没有匹配的活动会话时拒绝调用。具体执行仍通过 Driver Command / Runtime 资源路径。

## 7. 历史改名映射

| 旧标识符 | 当前标识符 | 说明 |
|---|---|---|
| `configId` / `config_id` | `connectionId` / `connection_id` | 持久化连接配置 ID |
| 旧语义中的 `connectionId`（运行时会话） | `dbSessionId` / `db_session_id` | Runtime session ID |
| Schema Diff v1 的 `configId` | v2 `sourceConnectionId` / `targetConnectionId` | v1 格式导入被明确拒绝 |
| SyncTask 的 `sourceConfigId` / `targetConfigId` | `sourceDbSessionId` / `targetDbSessionId` 与 `sourceConnectionId` / `targetConnectionId` | 会话与配置归属分开表达 |
| Wapp `command.invoke` 的 `configId` | `connectionId` | 无兼容别名 |
| SQLite 历史列 `query_history.config_id` | `connection_id` | 启动迁移保留历史数据 |

破坏性变更清单见 [CHANGELOG.md](../../CHANGELOG.md)。

## 8. 静态守护

`scripts/check-id-terminology.mjs` 扫描 `src/`、`packages/` 与 `e2e/`，发现禁用旧命名或把配置 ID 用作 session ID 的情况时失败。白名单必须说明保留理由。

```bash
npm run test:ids
```
