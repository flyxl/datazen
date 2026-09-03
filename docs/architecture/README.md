# DataZen 系统架构

> 本文档描述 **main 分支当前实现**，不是未来版本规划。具体实现以 `src-tauri/`、`packages/driver-api/` 和 `src/` 为准。

## 1. 总体结构

DataZen 当前是一个基于 Tauri v2 的桌面数据库客户端，同时支持 GUI 模式和 headless MCP stdio 模式。

```text
React / TypeScript
  │
  │ Tauri IPC
  ▼
src-tauri
  ├─ commands/       IPC 边界与参数/错误处理
  ├─ services/       连接、查询、事务、Job 等运行时服务
  ├─ db/             Driver Registry + Driver API re-export
  ├─ schema_diff/    Schema Diff 领域逻辑
  ├─ data_sync/      同族数据库 Data Sync
  ├─ data_transfer/  异构 Data Transfer
  ├─ transfer/       IR / DDL / adapter 能力
  ├─ workflow/       Workflow runtime
  ├─ ai/             AI provider / schema context
  ├─ mcp/            MCP Server / Client
  ├─ dashboard/      Dashboard runtime
  ├─ store/          Desktop 持久化
  └─ cache/          Schema cache
       │
       ▼
packages/driver-api
  │
  ├─ DatabaseDriver / Factory
  ├─ Driver Command
  ├─ Query streaming
  ├─ Schema objects
  ├─ Schema migration renderer/capabilities
  └─ Sync IR adapters
       │
       ▼
packages/drivers/*
```

当前代码没有独立的 Web Server / Domain Service 层；桌面应用直接通过 Tauri IPC 进入 Rust backend。Web 平台化属于未来架构，不应在当前架构文档中描述为已实现能力。

## 2. Driver 架构

`packages/driver-api` 是 Host 与独立 Driver 的稳定编译期契约。Driver 通过 `inventory` 注册到 Host，**不是**运行时加载动态 Rust library。

核心接口包括：

- `DatabaseDriver)：连接、Schema、Query、事务、Command、EXPLAIN、查询流等。
- `DatabaseDriverFactory)：创建 Driver 实例并声明协议版本。
- `DriverCommandDefinition)：向 Workflow / UI 暴露 Driver Command。
- `MigrationRenderer` / `MigrationCapabilities` / `TypeNormalizer)：Schema Diff 的方言渲染与类型归一化。
- `SyncSourceAdapter` / `SyncTargetAdapter)：Data Transfer 的异构类型与值转换。

Driver API 当前 `PROTOCOL_VERSION = 3`，最低兼容版本为 1。具体公共依赖边界见 [Driver API Dependency Boundary](backend/drivers.md)。

## 3. 连接与 Session

持久化连接配置使用 `connectionId`；实际数据库运行时会话使用 `dbSessionId`。

```text
connectionId
   │
   ▼
Store → ConnectionManager → Driver.connect()
                         │
                         ▼
                    dbSessionId
```

`ConnectionManager` 负责 Driver 选择、SSH Tunnel、session 生命周期、引用计数和 idle eviction。Schema Diff / Data Sync / Data Transfer 使用 dedicated session，避免与主工作区共享 database selection、事务等状态。

## 4. 查询执行

常规 SQL 从前端 `commands/` 进入 Rust command/service，再进入 Driver。查询支持多 statement、参数查询和 streaming；Driver capability 决定 EXPLAIN、OFFSET、取消等能力。

取消查询使用 opaque `QueryExecutionId`。支持精确取消的 Driver 在 execution lifecycle 中登记并处理该 ID；Host 不再把 session-wide cancel 当作新取消路径的 fallback。

## 5. Schema Diff

Schema Diff 的核心链路是：

```text
Schema Snapshot
    ↓
Schema Compare
    ↓
MigrationOperation (dialect-neutral)
    ↓
Dependency / Plan
    ↓
Driver MigrationRenderer
    ↓
MigrationStatement / Plan
    ↓
Review
    ↓
Deploy on target
```

Schema Diff 领域代码位于 `src-tauri/src/schema_diff/`：

- `compare.rs)：列、PK、索引差异。
- `ir.rs)：Snapshot → dialect-neutral `MigrationOperation`。
- `operations.rs)：中间操作及风险级别。
- `dependencies.rs)：操作依赖排序。
- `plan.rs)：渲染 MigrationStatement、能力检查和计划结果。
- `deploy.rs)：目标库部署。
- `types.rs)：Snapshot、Diff、Plan DTO。

**方言 SQL 属于 Driver API / Driver 层。** Host 不按 PostgreSQL/MySQL 等数据库类型复制 SQL。Driver 提供 `MigrationRenderer`、`MigrationCapabilities` 和 `TypeNormalizer`。

Source 是 desired state，Target 是 apply site。例如 source 为 `VARCHAR(255))、target 为 `VARCHAR(100)` 时，计划方向是把 target 修改为 `VARCHAR(255)`。

## 6. Data Sync 与 Data Transfer

三者职责明确：

| 能力 | 目的 | 当前实现 |
|---|---|---|
| Schema Diff | 修改目标结构 | `schema_diff/` |
| Data Sync | 同族数据库按行比较并同步 | `data_sync/` |
| Data Transfer | 异构数据库/映射/结构+数据迁移 | `data_transfer/` + `transfer/` |

Data Sync 使用 Compare → Review → Preview → Execute，生产 compare 路径支持 keyset pagination 和 job cancellation，执行阶段使用参数化 SQL、事务和失败/取消 rollback。

Data Transfer 使用 Endpoints → Setup → Objects → Mapping → Preview → Result，跨方言通过 IR adapter 完成类型和值转换；DDL 由 target adapter 渲染。

## 7. Workflow / AI / MCP / Dashboard

- **Workflow**：`src-tauri/src/workflow/` 提供 YAML executor、Command runtime、Condition、ForEach、AI、history 和 scheduler。
- **AI**：`src-tauri/src/ai/` 提供 OpenAI、Anthropic、DeepSeek、Ollama、Custom 等 provider/protocol 适配以及 schema context。
- **MCP**：`src-tauri/src/mcp/` 同时提供 MCP Server 和 Client，并支持 headless stdio。
- **Dashboard**：`src-tauri/src/dashboard/` 负责 Widget、执行、历史、告警和导出；Monitor 位于 `src-tauri/src/monitor/`。

## 8. 持久化与安全

Desktop 持久化由 `src-tauri/src/store/` 管理，包含连接配置、设置、AI 配置、同步任务和历史等。敏感配置使用 AES-256-GCM，主密钥由 `key_store` 管理，可使用 OS keychain 或开发/CI 文件后端。

查询历史目前位于 `history.sqlite`，SQL 文本和错误信息并非整体加密；因此应用数据目录应按敏感数据处理。

## 9. 前端状态

React 前端使用 Zustand。主要 Store 位于 `src/stores/`：

- `connectionStore)：持久化连接配置。
- `activeConnectionStore)：运行时连接/session 状态。
- `schemaStore)：Schema 元数据。
- `tableDataStore)：表数据、筛选、分页和编辑状态。
- `panelStore)：统一工作区 Panel 与查询结果。
- `workspaceTabsStore)：工作区 Tab。
- `aiStore)、`dashboardStore)、`extensionStore)、`settingsStore)、`uiStore)：对应领域状态。

跨窗口不共享 React/Zustand 内存状态，通过 Tauri Event 进行同步。

## 10. 测试

测试分布在 Driver、Host Rust、Frontend Vitest 和 E2E 四层。Driver-specific 行为放在 `packages/drivers/<id>/`；Host 不复制 Driver 方言测试。E2E 主要覆盖真实窗口流程和 IPC contract。

详细测试策略见 [testing.md](testing.md)。
