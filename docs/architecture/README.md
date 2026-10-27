# DataZen 系统架构

> 本文正文描述 **main 分支当前实现**。下方单独标明的 `platform/` 设计是用户明确要求保存的演进交付：已落地部分以代码和对应架构文档为准，未落地部分仍标记为目标设计。

## 架构文档索引

后端、前端与横切文档一览（入口为本文）：

### 后端

- [Drivers](backend/drivers.md)
- [Services](backend/services.md)
- [Tunnel](backend/tunnel.md)
- [Commands](backend/commands.md)
- [Cache](backend/cache.md)
- [Store](backend/store.md)
- [AI](backend/ai.md)
- [MCP](backend/mcp.md)
- [Workflow](backend/workflow.md)
- [Dashboard](backend/dashboard.md)
- [Data Transfer](backend/data-transfer.md)
- [Data Sync](backend/data-sync.md)
- [Schema Diff](backend/schema-diff.md)
- [Result Write](backend/result-write.md)（查询结果写回的两条通道、写入守卫与失败分支）
- [Wapps](backend/wapps.md)（含主题应用与首屏背景缓存；旧 `theme.md` 已合并至此）

### 前端

- [State](frontend/state.md)
- [Components](frontend/components.md)
- [Data Table](frontend/data-table.md)（数据网格：虚拟化、`data-dt-*` 契约、选择与键盘、编辑生命周期、分页与外键跳转）
- [AI](frontend/ai.md)
- [Extensibility](frontend/extensibility.md)

### 横切

- [Naming](naming.md)
- [Security](security.md)
- [Windows](windows.md)
- [Testing](testing.md)

### 平台演进设计（部分已实现，未落地部分为目标设计）

- [桌面 / Web 系统概要设计](platform/system-overview.md)
- [连接管理详细设计与验收用例](platform/connection-management.md)
- [共享应用边界与端口](platform/shared-boundaries-and-ports.md)
- [共享 feature 服务边界](platform/feature-core.md)
- [假资源夹具与基准](platform/fake-runtime-fixtures.md)
- [持久化模型](platform/persistence-model.md)
- [迁移三件套与 JobRuntime](platform/data-migration-jobs.md)
- [Workflow 资源模型](platform/workflow-resource-model.md)
- [AI/MCP/Wapp 与辅助任务接入](platform/consumer-adapters.md)
- [团队服务与认证](platform/team-server-and-auth.md)
- [多 worker 协调](platform/multi-worker-coordination.md)
- [分阶段开发计划](../development/platform-development-plan.md)
- [团队服务部署、升级与恢复](../development/team-service-operations.md)

P1（共享边界）、P3（Runtime 会话/资源/网关）、P4（桌面会话迁移）与 P5（JobRuntime 与迁移三件套）已经落地；P6（Workflow / AI / MCP / Wapp 等消费者迁移）仍在进行；P7–P10（团队 Web、多实例协调、全驱动发布验收）仍属目标设计。已完成部分应继续按实现事实维护，未完成部分继续服从目标设计。

> 除上述明确请求的设计外，本目录只描述 **已落地** 的实现。方案草稿、RFC 与实施计划不长期留在 `architecture/`：
> 结论落地后改写为本目录的事实条目，未落地的提案不入库。演进方向的公开叙述见 [blogs/](../blogs/README.md)。

## 1. 总体结构

DataZen 当前是一个基于 Tauri v2 的桌面数据库客户端，同时支持 GUI 模式和 headless MCP stdio 模式。前端通过 Tauri IPC（或 `@datazen/backend-client` 端口）进入宿主组装层，业务内核位于共享 Rust workspace。

```text
React / TypeScript (src/)
  │  Tauri IPC / @datazen/backend-client
  ▼
src-tauri (desktop host)
  ├─ commands/       IPC 边界与参数/错误处理
  ├─ platform/       Tauri 组装根：PlatformAdapter、DesktopSessionBackend、身份/策略/仓储适配
  ├─ services/       薄宿主服务：Schema metadata、JobRegistry、Runtime command source
  ├─ workflow/       Workflow host、YAML registry、executor、scheduler、runtime entry
  ├─ mcp/            MCP Server / Client
  ├─ dashboard/      Dashboard 主机侧适配
  ├─ monitor/        MonitorEngine、定时授权与 occurrence
  ├─ store/          桌面持久化（SQLite + 加密文件）
  ├─ tunnel/ + ssh_tunnel.rs   隧道装配
  ├─ wapps/          datazen:// 协议桥、WappManager
  └─ ai/             宿主 metadata source（规则与 provider 契约在共享包）
  │
  ▼
packages/
  ├─ platform-api/      跨形态、无传输依赖的契约（ID、RequestContext、target、ports）
  ├─ platform-infra/    形态中立基础设施原语（脱敏、路径安全）
  ├─ application/       应用层用例（target、identity policy、sessions、settings、DTO）
  ├─ runtime/           运行时内核：session/resource/lease/budget/gateway/job/sql_guard/schema_cache/tunnel
  ├─ feature-core/      共享 feature 逻辑：Workflow/AI/MCP/Dashboard/导出/备份恢复/Wapp 内核
  ├─ schema-diff/       方言中立 Schema Diff 引擎
  ├─ data-sync/         同族 Data Sync 引擎
  ├─ data-transfer/     异构 Data Transfer 引擎
  ├─ migration-common/  迁移引擎共享的 pair/recordset 边界
  ├─ driver-api/        Driver trait 与 Command/迁移/资源契约
  ├─ ai-api/, ai-providers/   AI Provider 契约与协议实现
  └─ backend-client/, driver-sdk/, wapp-sdk/, extension-points/, ui/   JS/TS 契约与 UI 包
```

依赖方向是单向的：`platform-api` 不依赖上层；`application` 依赖 `platform-api`；`runtime` 依赖 `application` 与 `platform-api`；`feature-core` 依赖 `runtime`、`application`、领域引擎与契约；`src-tauri` 是唯一把 Tauri、存储、网络和这些内核接起来的组装根。`scripts/check-platform-crate-boundaries.mjs` 对 Tauri/HTTP/UI 依赖做硬门禁。

当前代码没有独立的 Web Server / Domain Service 层；桌面应用直接通过 Tauri IPC 进入 Rust backend。团队 Web 形态仍属于目标设计，不应在当前架构文档中描述为已实现能力。

## 2. Driver 架构

`packages/driver-api` 是 Host 与独立 Driver 的稳定编译期契约。Driver 通过 `inventory` 注册到 Host，**不是**运行时加载动态 Rust library。

核心接口包括：

- `DatabaseDriver`：连接、Schema、Query、事务、Command、EXPLAIN、查询流等。
- `DatabaseDriverFactory`：创建 Driver 实例并声明协议版本。
- `DriverCommandDefinition`：向 Workflow / UI 暴露 Driver Command。
- `SqlDialect` / `CatalogReader` / `CommandCatalog` / `SchemaMigration` / `BackupDialect`：已提取的窄契约，旧 trait 方法由 `Legacy*` 适配器单向委托。
- `MigrationRenderer` / `MigrationCapabilities` / `TypeNormalizer`：Schema Diff 的方言渲染与类型归一化。
- `SyncSourceAdapter` / `SyncTargetAdapter`：Data Transfer 的异构类型与值转换。

Driver API 当前 `PROTOCOL_VERSION = 6`，最低兼容版本为 1。具体公共依赖边界见 [Drivers](backend/drivers.md)。

## 3. 连接与 Session

持久化连接配置使用 `connectionId`；实际数据库运行时会话使用 `dbSessionId`。

```text
connectionId
   │
   ▼
Store → application 用例 → Runtime SessionRegistry → DesktopSessionBackend → Driver resource
                                                        │
                                                        ▼
                                                   dbSessionId
```

Runtime 管理 session 状态、物理资源、预算和空闲回收；`DesktopSessionBackend` 负责从 Store 解析连接配置、经 DriverRegistry 打开资源，并让隧道跟随物理资源生命周期。会话过期后明确失效，不以同一个 `dbSessionId` 透明重连。Schema Diff / Data Sync / Data Transfer 使用专用 Runtime session，避免与主工作区共享 database selection、事务等状态。

## 3.1 隧道

连接不可直连时，DataZen 在本机监听 `127.0.0.1:<随机端口>`，把流量经 **SSH / HTTP CONNECT 代理 / WebSocket 中继** 转到真实 `host:port`，并把交给 Driver 的地址改写为该回环地址。四种状态（直连 / SSH / HTTP / WebSocket）互斥，**对 Driver 完全不可见**：加隧道不需要任何驱动改动。隧道配置可保存复用（`SavedTunnel` + `tunnels.json`），连接只持 `tunnel_id` 引用。

详见 [Tunnel](backend/tunnel.md)。

## 4. 查询执行

常规 SQL 从前端 `src/commands/` 进入 Rust command/service，再进入 Driver。查询支持多 statement、参数查询和 streaming；Driver capability 决定 EXPLAIN、OFFSET、取消等能力。

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

Schema Diff 领域代码位于 `modules/src/migration/schema_diff/`：

- `compare.rs`：列、PK、索引差异。
- `ir.rs`：Snapshot → dialect-neutral `MigrationOperation`。
- `operations.rs`：中间操作及风险级别。
- `dependencies.rs`：操作依赖排序。
- `plan.rs`：渲染 MigrationStatement、能力检查和计划结果。
- `deploy.rs`：目标库部署。
- `types.rs`：Snapshot、Diff、Plan DTO。
- `job/`：桌面 JobRuntime 的 prepare/apply 适配。
- `unified*.rs`：统一计划与跨对象依赖复核。

Host 侧只剩 `src-tauri/src/commands/schema_diff.rs` 门面及其子模块，负责 IPC、job 适配与桌面目标解析。**方言 SQL 属于 Driver API / Driver 层。** Host 不按 PostgreSQL/MySQL 等数据库类型复制 SQL。Driver 提供 `SchemaMigration` 背后的 `MigrationRenderer`、`MigrationCapabilities` 和 `TypeNormalizer`。

Source 是 desired state，Target 是 apply site。例如 source 为 `VARCHAR(255)`、target 为 `VARCHAR(100)` 时，计划方向是把 target 修改为 `VARCHAR(255)`。

## 6. Data Sync 与 Data Transfer

三者职责明确：

| 能力 | 目的 | 当前实现 |
|---|---|---|
| Schema Diff | 修改目标结构 | `modules/src/migration/schema_diff/` |
| Data Sync | 同族数据库按行比较并同步 | `modules/src/migration/data_sync/` |
| Data Transfer | 异构数据库/映射/结构+数据迁移 | `modules/src/migration/data_transfer/` + `modules/src/migration/common/` |

Data Sync 使用 Compare → Review → Preview → Execute，生产 compare 路径支持 keyset pagination 和 job cancellation，执行阶段使用参数化 SQL、事务和失败/取消 rollback。

Data Transfer 使用 Endpoints → Setup → Objects → Mapping → Preview → Result，跨方言通过 IR adapter 完成类型和值转换；DDL 由 target adapter 渲染。当前模型、计划消费与恢复边界见 [Data Transfer 架构](backend/data-transfer.md)。

## 7. Workflow / AI / MCP / Dashboard

- **Workflow**：共享模型、条件、模板、执行器和调度逻辑位于 `packages/feature-core/src/workflow/`；`src-tauri/src/workflow/` 提供 YAML registry、Command runtime 绑定、history、桌面 runtime entry 与真实驱动验收。Workflow v1 的连接管理经 `feature-core command_runtime → feature-core resource_execution → Runtime`。
- **AI**：Provider 契约与协议实现位于 `packages/ai-api/` 与 `packages/ai-providers/`；Prompt、Schema 上下文、NL2SQL/诊断/分析等共享逻辑位于 `packages/feature-core/src/ai/`；AI 用的数据库元数据源位于 `packages/feature-core/src/services/schema_context/`（宿主只注入 `ResourceExecutionHost` 与 Runtime 门面）。调用点直接引用共享包。
- **MCP**：`packages/feature-core/src/mcp/` 持有共享工具/资源逻辑，`src-tauri/src/mcp/` 提供 MCP Server 和 Client 的宿主传输、会话与权限接线，并支持 headless stdio。
- **Dashboard**：`packages/feature-core/src/dashboard/` 负责 Widget 创建/归档/调度策略，`src-tauri/src/dashboard/` 负责桌面存储与执行适配；`src-tauri/src/monitor/` 负责 MonitorEngine、定时授权与 occurrence。

## 8. 四维扩展架构

DataZen 采用四维扩展体系：

- **数据库驱动（Driver）**：编译时注入，基于 `@datazen/driver-sdk` 与 `packages/driver-api`，通过 inventory 注册。承载数据库连接、SQL 方言、DDL 和 Driver Command。
- **沙箱工作区应用（Wapp）**：基于 `@datazen/wapp-sdk`、`manifest.json`、`datazen://` 协议与沙箱 `<iframe>`，运行目录 `{appData}/wapps/{publisher}.{name}/`，向用户暴露独立工作区页面与外观主题，通过受控 postMessage 桥通信；取数一律走 `execute_driver_command`；形态无关的规则/安装/存储在 `packages/feature-core` 的 `workspace_apps` 模块。详见 [wapps.md](backend/wapps.md)。
- **特权扩展点（Host Extension Points）**：基于 `@datazen/extension-points` 的 `ExtensionPoint<T>` 契约与 CodeMirror Compartment，宿主进程内（In-Process）接入，专用于 SQLEditor 增强等高性能、深交互核心模块。受根目录 **DataZen Plugin, Driver & Extension Linking Exception** 保护，支持独立许可与分发。
- **外观主题（Theme）**：由 Wapp 以 `contributes.themes[]` 贡献的纯静态资源包（`tokens.css` / `editor.json` / `charts.json` / `icons/`），零代码执行。旧 v1 ThemePack（`packages/themes/`）已存档，`{appData}/themes/` 运行时入口已移除。详见 [wapps.md](backend/wapps.md)「主题应用」与「首屏背景缓存」。

此外，`@datazen/ui`（`packages/ui/`）是与宿主 Store/IPC 解耦的纯 React 基础视图组件库，供宿主、驱动 UI、Wapp 与扩展共享；它不构成第五维扩展。

## 9. 持久化与安全

Desktop 持久化由 `src-tauri/src/store/` 管理，包含连接配置、设置、AI 配置、同步任务和历史等。敏感配置使用 AES-256-GCM，主密钥由 `key_store` 管理，可使用 OS keychain 或开发/CI 文件后端。

查询历史目前位于 `history.sqlite`，SQL 文本和错误信息并非整体加密；因此应用数据目录应按敏感数据处理。

## 10. 前端状态

React 前端使用 Zustand。主要 Store 位于 `src/stores/`：

- `connectionStore`：持久化连接配置。
- `activeConnectionStore`：运行时连接/session 状态。
- `schemaStore`：Schema 元数据。
- `tableDataStore`：表数据、筛选、分页和编辑状态。
- `panelStore`：统一工作区 Panel 与查询结果。
- `workspaceTabsStore`：工作区 Tab。
- `aiStore`、`dashboardStore`、`wappStore`、`settingsStore`、`uiStore`：对应领域状态。

前端通过 `@datazen/backend-client` 端口访问 backend；桌面实现 `src/platform/tauriBackendTransport.ts` 是唯一直接接触 `@tauri-apps/api` 的适配层。跨窗口不共享 React/Zustand 内存状态，通过 Tauri Event 进行同步。

## 11. 测试

测试分布在 Driver、Host Rust、共享 crate、Frontend Vitest 和 E2E 五层。Driver-specific 行为放在 `packages/drivers/<id>/`；Host 不复制 Driver 方言测试。E2E 主要覆盖真实窗口流程和 IPC contract。

详细测试策略见 [testing.md](testing.md)。
