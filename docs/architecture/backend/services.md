# Backend Services

> Source of truth: `src-tauri/src/services/`。

Services 是 DataZen backend 中复用性的运行时服务层，不承担 Tauri IPC 的参数边界，也不承担具体 Driver 方言实现。

## 1. 当前服务

| 服务 | 位置 | 职责 |
|---|---|---|
| ConnectionManager | `connection_manager.rs` | Driver 选择、连接建立、session 生命周期、引用计数、idle eviction、隧道解析（`resolve_tunnel_ref` / `maybe_start_tunnel`） |
| QueryExecutor | `query_executor.rs` | 查询执行相关的参数、过滤、排序和执行辅助 |
| Schema metadata | `schema_metadata.rs` / `schema_metadata/columns.rs` | 精确关系身份读取、列批量读取、缓存刷新 |
| DbTools | `db_tools.rs` | 数据库工具类复用能力 |
| JobRegistry | `job_registry.rs` | 长任务/job 生命周期和取消注册 |
| Transaction | `transaction.rs` | 事务相关运行时辅助 |

## 2. ConnectionManager

DataZen 明确区分：

- `connectionId`：持久化连接配置 ID。
- `dbSessionId`：运行时数据库 session ID。

```text
connectionId
   ↓
Store
   ↓
ConnectionManager
   ↓
Driver.connect()
   ↓
dbSessionId
```

Schema Diff、Data Sync、Data Transfer 使用 dedicated session，避免共享主工作区的 database selection 或事务状态。

连接建立时先解析隧道再创建 driver：配置若带 `tunnel_id`，`resolve_tunnel_ref` 会从 `tunnels.json` 还原为内联的 `tunnel_kind` + 各类隧道参数；随后 `maybe_start_tunnel` 在本机监听回环端口并把 driver 看到的 host/port 改写为该回环地址。四种隧道类型（直连 / SSH / HTTP CONNECT / WebSocket）的差异对 driver 不可见。详见 [tunnel.md](tunnel.md)。

## 3. Query execution

普通 Query 由 command 层进入 QueryExecutor / Driver。流式查询可以绑定 `QueryExecutionId`，用于精确取消。

Services 不根据 PostgreSQL/MySQL 等类型拼接方言 SQL；数据库特定能力由 Driver 提供。

## 4. 长任务

Data Sync、Data Transfer 等任务通过 job registry / job id 管理取消和生命周期。取消是显式的 job 操作，不应通过关闭 UI 猜测任务已经停止。

## 5. 依赖方向

```text
Commands
   ↓
Services / Domain modules
   ↓
Driver API / Driver Registry
   ↓
Concrete Drivers
```

持久化通过 Store；Services 不把 React/Zustand 状态下沉到 Rust。

## 6. Schema metadata

宿主在 `execute_driver_command` 网关注册 `list_catalog`、`read_relation_columns`、`read_relation_schema` 和 `refresh_schema_metadata`。会话通过 envelope 的 `dbSessionId` 指定，database/schema/name 全部放在 input 的 `RelationRef` 中；这组命令拒绝重复的 envelope database/schema，避免目标冲突。

`RelationRef` 的 schema 必须显式为字符串或 null。具有 schema 层级的驱动要求精确 schema，读取不会通过 `use_database` 改变会话上下文。目录响应分开返回 schema 名称与关系条目，空 schema 不再伪装成空名称关系。

列读取每次最多接受 256 个目标，按完整身份去重，响应按请求顺序为各关系返回 `ok` 或 `error`。同一 database/schema 的未缓存目标可以使用驱动批量读取；驱动不支持批量读取时使用逐关系读取。完整结构缓存同时填充列缓存；失败信息经过凭据脱敏。

刷新支持 session、database 或精确 relation 范围，返回缓存 revision。缓存写入检查读取开始时的 generation，刷新前启动的读取不能重新填入缓存。旧的六个 Host schema IPC 已删除。查询数据和 ER 图聚合继续使用各自的 Host 命令；AI、Workflow 与驱动扩展通过 Driver Command 访问标准元数据命令。新 metadata commands 与这些入口共用 `SchemaCache`。
