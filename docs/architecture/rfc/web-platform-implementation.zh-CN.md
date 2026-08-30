# DataZen v0.2.0 Web 平台化技术实现方案

> 状态：Proposed
> 目标版本：v0.2.0
> 更新时间：2026-08-29
> 对应 PRD：[Web 平台化 PRD](../../features/web-platform-prd.zh-CN.md)

## 1. 方案摘要

DataZen Web 采用“共享 SQL/运行时 Core + 上层应用服务 + Transport/Persistence Adapter”架构：

- datazen-core 只承载 Driver Command、SQL prepare/execute、连接运行时、结果流、取消以及可复用的 Workflow/AI/Dashboard 执行运行时。
- src-tauri 保留桌面入口、Tauri IPC、窗口、原生对话框、托盘、更新器与 OS Keychain。
- datazen-server 提供静态 SPA、REST API、SSE、认证、workspace 资源服务、MySQL Persistence、SQL Audit 和后台 Worker。
- React 前端通过 PlatformClient 调用领域 API；Desktop 使用 TauriTransport，Web 使用 HttpTransport。
- Desktop 直接调用 Core，并写现有 SQLite Query History；Web Application Service 在调用 Core 前后处理认证、workspace、审计和 MySQL 持久化。
- Core 在执行前返回不可变 `PreparedSql`，执行后返回 `ExecutionReport`；上层决定是否审计、审计哪些字段以及审计存储位置。

实施采用纵切迁移，不进行一次性大重构。第一条纵切固定为：

    Bootstrap/Auth
      → Workspace
      → Connection CRUD
      → Connect
      → Schema
      → Query Job
      → SSE Rows
      → Cancel
      → History/Audit

这条链路打通后，再依次迁移 Workflow、AI 和 Dashboard。

## 2. 设计约束

### 2.1 必须遵守

1. datazen-core 不依赖 Tauri、Axum、Cookie、窗口或操作系统插件。
2. HTTP Router 与 Tauri Command 只做 DTO 转换、上下文建立和 transport 输出。
3. Core API 不接收 User、Principal、RequestSource、workspace 或 HTTP RequestContext；Web Application Service 在调用 Core 前完成认证、授权和资源归属检查。
4. connectionId 是持久化配置 ID；dbSessionId 是运行时 ID，永不落盘。
5. Server 中的 dbSessionId 由 Server SessionBroker 绑定 user、workspace 和 connection；Core 只接收已经解析的 session handle。
6. 所有 Web 控制面资源查询必须在 Server Repository/Persistence 层包含 workspace 条件。
7. Driver 方言与专属行为仍由对应 driver crate 实现和测试。
8. Server 后台 Worker 在 Application Service 层使用显式的 Worker 权限和资源范围，不依赖用户登录 Session。
9. 浏览器不得获得数据库、SSH、AI 或 Webhook Secret。
10. Desktop Standalone 必须保持可离线运行。
11. Core Runtime 不依赖身份、workspace、SQLite/MySQL 客户端、持久化方言或具体 transaction 类型。
12. SQL 必须先由 Driver 生成不可变 PreparedSql，经审计后再执行；审计后不得继续改写。

### 2.2 本方案不解决

- SaaS 计费、组织邀请、SSO、OIDC、LDAP。
- 多节点、高可用和分布式调度。
- Desktop 连接远程 DataZen Server。
- Web Runtime Extension、Data Sync、Transfer、Schema Diff、Backup/Restore。

## 3. 目标代码结构

### 3.1 Cargo Workspace

建议新增五个 workspace member：

    packages/core/
      Cargo.toml
      src/
        lib.rs
        error.rs
        sql.rs
        ports/
        services/
        runtime/
        dto/

    packages/server/
      Cargo.toml
      src/
        main.rs
        config.rs
        state.rs
        http/
        auth/
        middleware/
        sse/
        jobs/
        workers/
        store/
        crypto/

    packages/persistence-api/
      src/
        lib.rs
        repositories.rs
        unit_of_work.rs
        records.rs
        error.rs

    packages/persistence-sqlite/
      migrations/
      src/

    packages/persistence-mysql/
      migrations/
      src/

现有目录职责调整：

    src-tauri/
      src/
        commands/          # Tauri adapter，逐步删除业务实现
        desktop/           # tray/window/dialog/updater/keychain
        bootstrap.rs       # Desktop CoreState 组装

    src/
      platform/
        client.ts
        runtime.ts
        capabilities.ts
        tauri/
        http/

### 3.2 Cargo 依赖方向

允许（箭头左侧为被依赖方）：

    driver-api       ← core-runtime ← server application
                              ↑
                       src-tauri application

    persistence-api  ← persistence-mysql ← server application
    persistence-api  ← persistence-sqlite ← src-tauri application

    ai-api      ← core

禁止：

    core → tauri
    core → axum
    core → server
    core → persistence-sqlite / persistence-mysql
    driver crate → server

packages/core 可依赖 serde、tokio、uuid、chrono、thiserror、tracing、driver-api、ai-api 等运行时基础依赖，但不依赖 persistence-api。HTTP、Cookie、CORS、压缩、认证和 workspace 相关依赖只进入 application/server；rusqlite 只进入 persistence-sqlite，sqlx MySQL 只进入 persistence-mysql。控制面 MySQL Persistence 与用户连接使用的 MySQL Driver 是两套独立依赖和连接池。

### 3.3 迁移期兼容

现有 src-tauri 中大量函数已存在 xxx_impl 与 Tauri wrapper 两层。迁移时：

1. 先将纯执行 impl 所依赖的输入、状态和回调改为 Core Runtime 类型。
2. 将 SQL/Driver/Workflow 执行 impl 移入 packages/core/runtime；Web 的认证、workspace、审计和持久化编排留在 application/server，Desktop 的本地编排留在 src-tauri。
3. Tauri Command 保留原命令名和 camelCase/snake_case 映射。
4. 旧前端 command facade 保持导出签名，内部改用 PlatformClient。
5. 领域迁移完成后删除 src-tauri 中重复 impl。

不允许长期保留“HTTP 新实现 + Tauri 旧实现”双业务路径。

## 4. 上层调用上下文与职责边界

### 4.1 Core 不持有身份、来源和 workspace

Core Runtime 的输入只描述执行所需的对象：已解析的数据库 session handle、SQL、参数、执行选项和取消信号。它不接收 `User`、`Principal`、`RequestSource`、`workspaceId`、Cookie 或 HTTP `RequestContext`，也不负责查 users、sessions、memberships。

Core 也不做 SQL Audit。它只返回不可变的 `PreparedSql` 和执行后的 `ExecutionReport`；调用层决定是否审计、审计哪些字段、是否阻断以及写入何种审计后端。Desktop v0.2.0 不持久化合规级 SQL Audit，只保留 Query History。

### 4.2 Web Application Service 的职责

Web 的 `WebApplicationService` 在调用 Core 前完成：

- Cookie/Session 认证和角色判断。
- workspace 与 connection/workflow/dashboard 的归属检查。
- dbSession owner、连接状态和资源版本检查。
- SQL Audit policy 的 pre-execute decision。

调用 Core 后由 Web Application Service 持久化 outcome、history 和审计事件。所有 workspace 隔离都位于 Web Repository/Persistence 查询和 Web Application Service，不下沉到 Core。

### 4.3 Desktop 的职责

Desktop 直接使用 Core Runtime，不引入登录、Session、membership、workspace 或 Web RBAC。Desktop v0.2.0 使用本地连接配置和 SQLite Query History，不增加合规级 SQL Audit；Safe Mode 和危险操作确认仍按现有产品策略执行。未来如需要企业端点审计，应作为独立能力设计本地加密、留存、导出和防篡改策略，不反向污染 Core。

### 4.4 后台 Worker 与复用

Workflow、Dashboard、AI Tool、MCP 等调用方先在各自的 Application Service 中解析资源和权限，再把已经解析的执行对象交给 Core。后台 Worker 使用自己的调度/资源范围记录，但 Core 不需要知道它是 Worker，也不需要区分 Web、Desktop 或 MCP 来源。

### 4.5 Workspace 与 Audit 的精确定义

#### Workspace

Workspace 是 Web Application 层的资源归属和权限边界，属于控制面概念。它约束的是 DataZen 的 connection、Workflow、Dashboard、知识库、告警、Job、History 和 Audit 可见性，不是 Core Runtime 的参数。

Workspace 不隔离目标数据库里的 schema、table 或 row。目标数据库能访问什么，由连接账号的原生权限决定；同一个物理数据库可以被多个 workspace 配置为不同 connection，但每个 Web connection 只归属一个 workspace。v0.2.0 使用 MySQL 控制面中的 `workspace_id` 做逻辑隔离，不要求每个 workspace 单独建库。

实现上，Server 先从 Session 得到 user，再从 membership 得到 workspace/role，随后以 `(workspace_id, resource_id)` 查询资源和 SessionBroker 绑定关系。Repository、唯一约束和跨资源外键共同保证边界；Core 只接收已解析的 `DbSessionHandle`/执行对象。Desktop 不创建 local workspace，沿用本地单用户 SQLite 模型。

#### Audit

Audit 是上层 Application Service 记录的可追溯操作证据，不是 Core 的职责，也不是权限系统本身。若要达到更高等级的不可抵赖性，还需要外部不可变存储、签名或合规系统；v0.2.0 不宣称仅靠应用表实现这些能力。Web 中分为：

- `SecurityAudit`：认证、成员/角色变化、Secret 变化和越权尝试。
- `SqlAudit`：submitted SQL、Core 返回的 effective SQL、rewrite chain、decision 和 outcome。
- `QueryHistory`：面向用户复用的可清理历史，不能作为合规审计替代品。
- `Tracing/Log`：排障信息，默认脱敏，不能作为完整审计替代品。

`actor` 和 `source` 只存在于上层 AuditRecord：前者表示用户或调度任务，后者表示 Query Editor、Workflow、Dashboard、AI、MCP 或 API 入口；它们不传入 Core。workspace-scoped 事件必须有 workspace_id，instance-level 登录/配置事件可以没有 workspace_id，越权事件应保留尝试访问的 workspace（如果可确定）。

SQL 审计时，Web Application Service 按以下顺序调用：

    auth + resource check
      → Core.prepare_sql()                 # 得到最终执行表示
      → 上层 AuditPolicy 决定 allow/warn/block
      → 上层持久化 prepared/started
      → Core.execute_prepared_sql()
      → 上层持久化 terminal outcome/history

Core 返回 `PreparedSql`/`ExecutionReport`，其中的 `effective_sql_template + bound_parameters` 是目标数据库协议层的实际执行表示。上层可以只保存 hash、生成脱敏 preview，或把全文放入加密审计 payload；Core 不替上层选择策略。

## 5. Core Runtime API 与上层 Application Service

### 5.1 Core Runtime Ports

Core 只抽象数据库执行所需的端口：

    trait DatabaseDriver
    trait DbSessionHandle
    trait ResultSink
    trait Clock
    trait CancellationSignal

`DbSessionHandle` 是不透明的运行时句柄，不携带 user、workspace 或 source。Core 只保证句柄有效、连接可用和执行生命周期正确，不判断句柄属于哪个 Web 用户。

### 5.2 Persistence/Auth/Audit Ports 属于上层

以下接口供 Web/Desktop Application Service 使用，不属于 Core Runtime：

    trait UserRepository
    trait SessionRepository
    trait WorkspaceRepository
    trait MembershipRepository
    trait ConnectionRepository
    trait SecretStore
    trait SettingsRepository
    trait QueryHistoryRepository
    trait WorkflowRepository
    trait WorkflowRunRepository
    trait DashboardRepository
    trait AuditRepository
    trait JobRepository
    trait AuthorizationPolicy
    trait AuditPolicy
    trait EventSink

Web Repository 方法显式接收 workspace 条件，例如 `get_connection(workspace_id, connection_id)`；Desktop 使用本地连接存储，不需要构造 workspace 条件。上述策略由对应 Application Service 和 Persistence Adapter 实现。

### 5.3 Core Runtime 服务

建议 Core 只暴露与执行相关的接口：

    trait SqlRuntime {
        fn prepare_sql(&self, request: PrepareSqlRequest) -> Result<PreparedSql, CoreError>;
        async fn execute_prepared_sql(
            &self,
            session: &DbSessionHandle,
            prepared: PreparedSql,
            sink: &dyn ResultSink,
        ) -> Result<ExecutionReport, CoreError>;
    }

`SqlRuntime`：

- 不接收 RequestContext、Principal、workspace 或 source。
- 不查询 Repository，不调用 AuthorizationPolicy，不写 Audit。
- `prepare_sql` 返回不可变的最终执行表示，调用方可以在执行前检查或保存它。
- `execute_prepared_sql` 不再隐式改写 SQL；`ExecutionReport` 回带本次执行使用的 `PreparedSql`、statement outcome、rows affected、duration 和 driver error。
- Workflow/AI/Dashboard Runtime 只负责执行编排，资源加载、权限判断、history/audit 仍由上层 Application Service 完成。

### 5.4 CoreError 与上层错误

Core 只返回执行错误：

    CoreError {
      InvalidSqlRequest
      DriverUnavailable
      QueryTimeout
      QueryCancelled
      ExecutionFailed
      ResultSinkClosed
      Internal
    }

`Unauthenticated`、`Forbidden`、`WorkspaceForbidden`、`RateLimited`、`SessionExpired` 属于 Web Application Service 的错误，不应出现在 CoreError。Tauri Adapter 和 HTTP Adapter 只负责把各自上层错误映射为 CommandError 或 HTTP error envelope。

## 6. AppState 重构

### 6.1 CoreState

建议：

    pub struct CoreState {
        pub driver_registry: Arc<DriverRegistry>,
        pub connection_manager: Arc<ConnectionManager>,
        pub schema_cache: Arc<SchemaCache>,
        pub workflow_runtime: Arc<WorkflowRuntime>,
        pub ai_registry: Arc<AiProviderRegistry>,
        pub prompt_resolver: Arc<PromptResolver>,
        pub services: Arc<RuntimeServiceRegistry>,
    }

### 6.2 DesktopState

    pub struct DesktopState {
        pub core: Arc<CoreState>,
        pub persistence: Arc<SqlitePersistence>,       # Desktop adapter 可选使用
        pub monitor_runtime: Arc<MonitorRuntime>,
        pub extensions: Arc<ExtensionManager>,
        pub native_window: NativeWindowServices,
    }

Tauri State 注入 DesktopState，Command 从中取得 Core Runtime 和本地持久化 adapter；不构造 Web User/Workspace context。

### 6.3 ServerState

    pub struct ServerState {
        pub core: Arc<CoreState>,
        pub persistence: Arc<MySqlPersistence>,
        pub auth: Arc<AuthService>,
        pub application: Arc<WebApplicationServices>,
        pub event_hub: Arc<EventHub>,
        pub jobs: Arc<JobManager>,
        pub config: Arc<ServerConfig>,
        pub shutdown: CancellationToken,
    }

### 6.4 Application Worker 构造

Monitor Worker 不放入 CoreState，也不把身份、workspace 或 AuditSink 传入 Core Runtime。由上层按运行环境组装：

    WebMonitorWorker::new(
        mysql_dashboard_repository,
        web_session_broker,
        core.workflow_runtime,
        web_audit_service,
        server_event_sink,
        notification_dispatcher,
        config,
    )

    MonitorRuntime::new(
        local_dashboard_repository,
        core.workflow_runtime,
        desktop_notification_dispatcher,
        config,
    )

Worker 在上层读取资源、检查权限并决定审计；Core Runtime 只执行已解析的 Workflow/SQL。

## 7. Server 技术栈与启动

### 7.1 建议依赖

- Axum：Router、extractor、response。
- Tower / tower-http：middleware、trace、compression、request ID、CORS、timeout。
- axum-extra 或 cookie：Cookie 解析与设置。
- Argon2：密码 hash，复用现有依赖。
- sqlx（mysql feature）：Web 元数据库连接池、事务与 migration；只进入 persistence-mysql。
- rusqlite：Desktop SQLite 与本地 migration；只进入 persistence-sqlite，复用现有 bundled 配置。
- tokio-stream / futures-util：SSE 与 Job stream。
- schemars：DTO schema；可配合生成 OpenAPI/TypeScript。
- sha2、rand、aes-gcm：Session token hash 与 Secret 加密。

具体版本与仓库 Cargo.lock 对齐，避免各 crate 引入重复 major。

### 7.2 Server 启动顺序

    1. 解析 ServerConfig
    2. 初始化 tracing，安装 Secret redaction
    3. 校验 DATAZEN_MASTER_KEY / secret file
    4. 校验 DATAZEN_DATABASE_URL，建立 MySQL control-plane pool
    5. 获取 MySQL migration advisory lock
    6. 执行 MySQL migration，失败则 fail closed
    7. 初始化 MySqlPersistence 与 CoreState
    8. 将 interrupted jobs 标记为 interrupted；未闭合 SQL audit 追加 outcome_unknown
    9. 启动 Workflow Scheduler / Monitor Worker / Cleanup Worker
    10. 构建 Router
    11. 绑定端口
    12. ready = true

任一步骤失败时 ready 保持 false；涉及 key、MySQL 连接或 migration 的错误直接退出。v0.2.0 仍限定单个 datazen-server 进程，但不依赖 SQLite 文件锁实现该限制；部署层应固定一个 Server replica，启动时再用实例租约防止误启动第二个 Worker。

Server 只组装 `MySqlPersistence`，不提供“Web 默认 SQLite”的 fallback。`DATAZEN_DATABASE_URL` 解析失败、MySQL 不可达、账号权限不足或 schema 不兼容都属于启动失败；连接串和其中的凭据不得进入日志、health response 或诊断页面。

### 7.3 Router 分层

顺序建议：

    RequestId
      → TrustedProxy / ClientIp
      → Trace / Redaction
      → SecurityHeaders
      → BodyLimit
      → Compression
      → AuthSession
      → CSRF
      → WorkspaceContext
      → Route Handler

登录、health、bootstrap 使用独立 public router；业务 API 使用 authenticated router。

### 7.4 路由组织

    /health/live
    /health/ready
    /api/v1/bootstrap/*
    /api/v1/auth/*
    /api/v1/users/*
    /api/v1/workspaces/*
    /api/v1/workspaces/{workspaceId}/connections/*
    /api/v1/workspaces/{workspaceId}/db-sessions/*
    /api/v1/workspaces/{workspaceId}/query-jobs/*
    /api/v1/workspaces/{workspaceId}/schema/*
    /api/v1/workspaces/{workspaceId}/workflows/*
    /api/v1/workspaces/{workspaceId}/workflow-runs/*
    /api/v1/workspaces/{workspaceId}/dashboards/*
    /api/v1/workspaces/{workspaceId}/ai-jobs/*
    /api/v1/workspaces/{workspaceId}/audit-events/*
    /api/v1/workspaces/{workspaceId}/events
    /assets/*
    /*

SPA fallback 只能作用于非 /api 和非 /health 路径。

## 8. 认证与 Session 实现

### 8.1 用户密码

- 使用 Argon2id。
- password_hash 保存 PHC 字符串，包含算法参数。
- 登录成功时如果参数落后于当前策略，后台重新 hash。
- username 保存 normalized_username 唯一索引；展示名可重复。

### 8.2 Session Token

登录成功：

1. 生成 32-byte 随机 token。
2. Cookie 保存 base64url raw token。
3. 数据库只保存 SHA-256(token)。
4. Cookie 名建议 datazen_session。
5. Cookie 设置 HttpOnly、SameSite=Lax、Path=/、生产环境 Secure。

auth_sessions 字段：

    id
    token_hash
    user_id
    created_at
    last_seen_at
    idle_expires_at
    absolute_expires_at
    revoked_at
    user_agent_hash
    client_ip_prefix_hash

建议 idle 12 小时、absolute 7 天，实例管理员可配置安全范围。

### 8.3 CSRF

- 登录后生成独立 CSRF token，Server 只保存 hash。
- GET /auth/session 返回当前用户信息与新的 CSRF token。
- 前端仅在内存保存，mutation 使用 X-CSRF-Token。
- 同时校验 Origin/Host。
- 登录、bootstrap、退出使用专门规则；退出仍要求 CSRF。

### 8.4 Bootstrap

    GET  /api/v1/bootstrap/status
    POST /api/v1/bootstrap/initialize

初始化请求必须提供部署时配置的一次性 token。成功后：

- 事务内创建 instance state、首个 admin、首个 workspace、membership。
- 写 SecurityAudit。
- 标记 initialized。
- bootstrap token 后续不可用。

### 8.5 角色撤销

每次业务请求根据 session.user_id 查询 membership，不把角色长期固化在 Cookie。

若后续增加短 TTL cache：

- membership 更新发布 authz.invalidated 事件。
- cache key 包含 userId/workspaceId。
- 移除成员后主动关闭其该 workspace 的 dbSession、Job stream 与 workspace SSE。

## 9. Workspace 数据模型与持久化抽象

### 9.1 Persistence API

`packages/persistence-api` 定义控制面持久化契约，由 Web/Desktop Application Service 依赖，Core Runtime 不依赖该 crate。由于 Desktop 没有 users、workspace 和审计强制要求，接口拆成公共资源契约与 Web 控制面扩展，避免让 SQLite 适配器实现无意义的 Web 身份模型：

    #[async_trait]
    pub trait ResourcePersistence: Send + Sync {
        fn kind(&self) -> PersistenceKind;
        fn connections(&self) -> Arc<dyn ConnectionRepository>;
        fn workflows(&self) -> Arc<dyn WorkflowRepository>;
        fn dashboards(&self) -> Arc<dyn DashboardRepository>;
        fn query_history(&self) -> Arc<dyn QueryHistoryRepository>;
        fn unit_of_work(&self) -> Arc<dyn ResourceUnitOfWork>;
    }

    #[async_trait]
    pub trait WebControlPlanePersistence: ResourcePersistence {
        fn users(&self) -> Arc<dyn UserRepository>;
        fn workspaces(&self) -> Arc<dyn WorkspaceRepository>;
        fn memberships(&self) -> Arc<dyn MembershipRepository>;
        fn jobs(&self) -> Arc<dyn JobRepository>;
        fn audits(&self) -> Arc<dyn AuditRepository>;
        fn web_unit_of_work(&self) -> Arc<dyn WebUnitOfWork>;
    }

    #[async_trait]
    pub trait ResourceUnitOfWork: Send + Sync {
        async fn save_connection(&self, input: ConnectionRecord) -> Result<(), PersistenceError>;
    }

    #[async_trait]
    pub trait WebUnitOfWork: ResourceUnitOfWork {
        async fn bootstrap_instance(&self, input: BootstrapRecord) -> Result<(), PersistenceError>;
        async fn save_connection_with_secret(&self, input: ConnectionSecretRecord) -> Result<(), PersistenceError>;
        async fn append_execution_started(&self, input: ExecutionStartedRecord) -> Result<(), PersistenceError>;
        async fn finalize_execution(&self, input: ExecutionTerminalRecord) -> Result<(), PersistenceError>;
    }

Repository 负责 Web 控制面 workspace-scoped 读写；`UnitOfWork` 只暴露确实需要跨 Repository 原子性的业务操作，由适配器内部选择原生 transaction。Core Runtime 不接触 `rusqlite::Transaction`、`sqlx::Transaction`、SQL 字符串或连接池。

错误统一映射为稳定语义：`NotFound`、`Conflict`、`ConstraintViolation`、`Unavailable`、`SerializationFailure`、`MigrationRequired`。不得把 SQLite/MySQL 原始错误码直接暴露给 Router 或 UI。

### 9.2 双实现与一致性约束

- `SqlitePersistence`：供 Desktop 使用，实现 `ResourcePersistence`，复用现有 `datazen.sqlite`，WAL + busy timeout + 短事务，不实现 WebControlPlanePersistence。
- `MySqlPersistence`：供 Web 使用，实现 `WebControlPlanePersistence`，连接 MySQL 8.0+，InnoDB + utf8mb4 + UTC，会话时区显式设置为 `+00:00`。
- ID 在 API 层统一为 UUID/string，不依赖 MySQL auto increment；时间统一 UTC；布尔、JSON、二进制和可空字段由 adapter 显式转换。
- 分页统一使用稳定排序键与 cursor；不得依赖 SQLite/MySQL 默认排序或不同 NULL 排序语义。
- 公共资源的冲突/幂等、唯一约束、乐观版本、删除语义必须在共享契约测试中一致；workspace 隔离、成员撤销和 SQL 审计只在 WebControlPlane 契约中测试。
- 不做运行时 SQL 方言翻译，不允许 Web 通过用户数据库的 MySQL Driver访问控制面元数据库。

公共契约测试以同一组测试函数分别运行：SQLite 使用临时数据库，MySQL 使用 CI service/test container。MySQL 额外覆盖 workspace 越权、成员撤销、审计 append-only 和 Web Job 恢复；两者都覆盖事务回滚、并发唯一冲突、cursor 分页和 migration upgrade。

### 9.3 Migration 策略

现有 `datazen.sqlite` 的 `SCHEMA_VERSION` 升级为增量 migration；Web MySQL 从第一版开始使用独立的 Web control-plane migration。公共资源 migration 使用对齐的逻辑 ID，Web 专属的 users/workspaces/memberships/jobs/audits migration 不应用到 Desktop。两个 adapter 各自维护方言脚本：

    packages/persistence-sqlite/migrations/0001_*.sql
    packages/persistence-mysql/migrations/0001_*.sql

每个 migration：

- 公共资源具有对齐的逻辑编号和能力清单；Web 专属 migration 只存在于 MySQL。每个实际脚本单独记录 schema checksum，方言 SQL 可不同。
- 在对应数据库支持的事务边界内执行，并记录 `applied_at`、binary version 和 checksum。
- 提供只读 dry-run/inspect 命令；启动发现版本超前、checksum 不符或 migration 失败时 fail closed。
- Desktop migration 前创建 SQLite + WAL/SHM 一致性备份。
- Web migration 前要求完成 MySQL 逻辑/物理备份检查；Server 不把 MySQL 数据文件复制到本地 volume。
- 破坏性变更采用 expand/contract，至少跨一个 minor 版本，不依赖自动 down migration。

### 9.4 Web Control Plane 基础表

以下为 Web control plane 的逻辑 schema 示例，不是可直接执行的跨方言 DDL。`STRING/BYTES/BOOL/TIMESTAMP_UTC/JSON` 由 MySQL adapter 映射，并通过 schema capability tests：

    instance_meta(
      key STRING PRIMARY KEY,
      value_json JSON NOT NULL,
      updated_at TIMESTAMP_UTC NOT NULL
    )

    users(
      id STRING PRIMARY KEY,
      normalized_username STRING NOT NULL UNIQUE,
      username STRING NOT NULL,
      display_name STRING NOT NULL,
      password_hash STRING NOT NULL,
      is_instance_admin BOOL NOT NULL,
      status STRING NOT NULL,
      created_at TIMESTAMP_UTC NOT NULL,
      updated_at TIMESTAMP_UTC NOT NULL
    )

    auth_sessions(
      id STRING PRIMARY KEY,
      token_hash BYTES NOT NULL UNIQUE,
      user_id STRING NOT NULL REFERENCES users(id),
      csrf_hash BYTES NOT NULL,
      created_at TIMESTAMP_UTC NOT NULL,
      last_seen_at TIMESTAMP_UTC NOT NULL,
      idle_expires_at TIMESTAMP_UTC NOT NULL,
      absolute_expires_at TIMESTAMP_UTC NOT NULL,
      revoked_at TIMESTAMP_UTC
    )

    workspaces(
      id STRING PRIMARY KEY,
      name STRING NOT NULL,
      slug STRING NOT NULL UNIQUE,
      status STRING NOT NULL,
      created_at TIMESTAMP_UTC NOT NULL,
      updated_at TIMESTAMP_UTC NOT NULL
    )

    workspace_memberships(
      workspace_id STRING NOT NULL REFERENCES workspaces(id),
      user_id STRING NOT NULL REFERENCES users(id),
      role STRING NOT NULL,
      created_at TIMESTAMP_UTC NOT NULL,
      updated_at TIMESTAMP_UTC NOT NULL,
      PRIMARY KEY(workspace_id, user_id)
    )

### 9.5 业务表归属

以下 Web control-plane 表增加 workspace_id：

- connections
- workflows
- workflow_runs
- dashboards
- widgets
- widget_runs
- favorite_queries
- query_history
- audit_events
- jobs
- 后续 knowledge collections/documents/chunks
- 后续 alert states/deliveries

唯一约束改为 workspace-scoped，例如：

    UNIQUE(workspace_id, workflow_id)
    UNIQUE(workspace_id, dashboard_id)

外键优先使用复合约束，避免跨 workspace 引用。

### 9.6 Desktop SQLite 映射

Desktop SQLite 继续使用单用户、本地数据模型，不创建 `local` workspace，不增加 users、memberships 或 Web Session 表。`SqlitePersistence` 通过 `ResourcePersistence` 暴露连接、Workflow、Dashboard、History 等公共资源；Web 的 workspace、用户、Job、Audit 表仅由 `MySqlPersistence` 实现。这样 Core Runtime 和 Desktop 都不会被 Web 多租户概念污染。

### 9.7 Web Connection 与 Secret

拆表：

    connections(
      id,
      workspace_id,
      name,
      driver_type,
      config_json_without_secrets,
      secret_id,
      enabled,
      created_by,
      created_at,
      updated_at,
      version
    )

    encrypted_secrets(
      id,
      workspace_id,
      resource_type,
      resource_id,
      cipher_version,
      key_id,
      nonce,
      ciphertext,
      created_at,
      updated_at
    )

Web Secret 的 AAD：

    datazen:v1:{workspaceId}:{resourceType}:{resourceId}:{secretId}

更新连接时：

- Secret 字段缺失：保持原值。
- Secret 字段显式 clear：删除对应字段。
- Secret 字段新值：重新加密。
- API 返回 secretConfigured 布尔值，不返回 ciphertext。Desktop Secret 继续使用现有本地 AAD 规则，不强制增加 workspaceId。

## 10. dbSession 与 ConnectionManager

### 10.1 Core Session Registry

Core 的 `ConnectionManager` 只管理数据库连接和运行时句柄：

    connect(connection_config) -> DbSessionHandle
    disconnect(db_session_id)
    get_session(db_session_id) -> DriverSession
    cleanup_idle(now)

Core session metadata 只包含 `db_session_id`、目标连接、创建时间、最近使用时间和连接状态，不包含 user、workspace、principal 或 source。

### 10.2 Web SessionBroker

Web Application Service 在通过认证和资源归属检查后，调用 Core 创建 session，并在自己的 SessionBroker 中保存绑定关系：

    web_session_bindings(
      db_session_id,
      user_id,
      workspace_id,
      connection_id,
      created_at,
      last_used_at,
      expires_at
    )

每个 Web 请求先由 SessionBroker 校验 user/workspace/connection owner，再把已经解析的 `DbSessionHandle` 交给 Core。用户退出、成员移除、连接禁用和 workspace 删除时，由 Web 层关闭对应 Core session；Core 不需要理解这些业务事件。

### 10.3 Desktop 生命周期

Desktop 直接调用 Core `connect/disconnect/get_session`，继续使用现有本地连接生命周期和 idle cleanup，不创建 SessionBroker，也不持久化 dbSessionId。页面刷新、窗口关闭等行为由 Desktop adapter 按现有产品规则处理。

## 11. Query Job 与 SSE

### 11.1 Core ExecutionHandle 与 Web JobManager

Core Runtime 只需要管理执行句柄、取消、超时和结果流，不保存 owner、workspace、principal 或审计信息。Web Application Service 在 Core 之上维护可查询的 JobRecord，负责 user/workspace 权限、重连、历史和审计：

    WebJobRecord {
      id,
      user_id,
      workspace_id,
      source,
      kind,
      state,
      request_id,
      created_at,
      started_at,
      finished_at,
      summary,
      error_code
    }

    JobState = Queued | Running | Succeeded | Failed | Cancelled | Interrupted

Core 使用 CancellationToken，不再只用 AtomicBool；Web JobManager 负责把取消请求映射到对应的 Core ExecutionHandle。

### 11.2 Query API

    POST /workspaces/{ws}/query-jobs

请求：

    {
      "dbSessionId": "...",
      "database": "...",
      "schema": "...",
      "sql": "...",
      "resultLimit": 5000
    }

响应：

    202
    {
      "jobId": "...",
      "requestId": "...",
      "eventsUrl": "/api/v1/workspaces/{ws}/jobs/{id}/events"
    }

查询：

    GET  /workspaces/{ws}/jobs/{id}
    POST /workspaces/{ws}/jobs/{id}/cancel
    GET  /workspaces/{ws}/jobs/{id}/events

### 11.3 两类 SSE

1. Workspace SSE：资源变更、Widget run、告警、成员权限变更等小事件。
2. Job SSE：Query rows、AI chunks、Workflow progress 等高流量、单任务事件。

不把 Query row chunks 广播到 Workspace SSE。

### 11.4 JobEventHub

每个 Job：

- tokio broadcast channel。
- 有界 replay ring：最多 256 个事件或 16 MiB，保留 5 分钟。
- 事件有单调 event sequence。
- 支持 Last-Event-ID。
- 慢客户端落后时发送 resync_required。
- rows 不持久化到控制面 Persistence。
- Web terminal summary 持久化，Server 重启把 Web Running 标记 Interrupted；Core 不恢复业务 Job。

### 11.5 Query chunk

从现有 QueryStreamEvent 转为 API DTO：

    statement_start
    rows
    statement_end
    done
    error

每个 rows event 同时限制：

- 最大 100 行。
- 序列化后最大 256 KiB。

若 Driver callback 一次返回过大批次，Core 分片后投递。

### 11.6 Cancel

取消链：

    HTTP cancel
      → Web JobManager CancellationToken
      → Core SqlRuntime
      → driver.cancel_query(handle)
      → Core ExecutionReport
      → Web 层 terminal Cancelled/history/audit
      → SSE cancelled

重复 cancel 返回相同 terminal state，不重复调用 Driver。

## 12. Event 模型

### 12.1 DomainEvent

Core Runtime 只产生执行层事件，例如：

    QueryStarted
    QueryChunk
    QueryCompleted
    QueryCancelled
    DbSessionClosed

`ConnectionCreated`、`WorkflowRunUpdated`、`DashboardUpdated`、`AlertStateChanged` 和 `MembershipChanged` 属于 Web/Desktop Application 层事件；它们可以消费 Core 事件，但不应反向把身份或 workspace 概念塞进 Core。

### 12.2 ServerEvent Envelope

    {
      "eventId": "...",
      "sequence": 123,
      "type": "widget.run.updated",
      "requestId": "...",
      "workspaceId": "...",
      "occurredAt": "...",
      "payload": {}
    }

### 12.3 Adapter

- DesktopEventSink：映射到 Tauri emit/Channel。
- ServerEventSink：映射到 workspace EventHub 或 job EventHub。
- TestEventSink：收集事件用于断言。

事件名在共享 DTO 中定义，前端不得散落手写字符串。

## 13. HTTP DTO 与类型生成

### 13.1 DTO 分层

- Core domain type 不直接等于 HTTP DTO。
- packages/core/dto 放跨 transport 的稳定 DTO。
- packages/server/http/dto 只放 Cookie、pagination、link 等 HTTP 专属类型。
- Tauri 与 HTTP 尽可能使用同一 Core DTO。

### 13.2 TypeScript 生成

建议用 Rust schema 生成 TypeScript：

    packages/core/src/dto
      → schema/openapi generation
      → src/platform/generated/api.ts

生成文件：

- 明确标注 generated。
- CI 运行 generation 后检查 git diff。
- 禁止手工修改。
- API breaking change 需要更新 compatibility test。

### 13.3 API Version

- URL major：/api/v1。
- v0.2.x 只增加 optional field/route/event。
- 删除或改变语义必须进入 v2。
- 前端启动先读取 /api/v1/meta，检查 serverVersion、apiVersion、capabilities。

## 14. 前端 PlatformClient

### 14.1 接口拆分

    interface PlatformClient {
      auth: AuthApi
      workspace: WorkspaceApi
      connection: ConnectionApi
      query: QueryApi
      schema: SchemaApi
      workflow: WorkflowApi
      ai: AiApi
      dashboard: DashboardApi
      settings: SettingsApi
      events: EventApi
      capabilities: PlatformCapabilities
    }

避免一个包含数百方法的扁平接口。

### 14.2 Runtime 选择

    createPlatformClient():
      if Tauri runtime:
        TauriPlatformClient
      else:
        HttpPlatformClient

选择只发生一次。业务 Store/Component 不再判断 __TAURI_INTERNALS__。

### 14.3 现有 commands 迁移

为了控制改动面：

    src/commands/connection.ts
      → return getPlatformClient().connection.*

    src/commands/query.ts
      → return getPlatformClient().query.*

所有现有调用方先保持不变。待各领域稳定后，可选择直接依赖领域 client。

### 14.4 Streaming API

统一：

    startQuery(request) -> JobHandle<QueryEvent>

    type JobHandle<T> = {
      jobId: string
      requestId: string
      subscribe(handler): Unsubscribe
      cancel(): Promise<JobStatus>
      getStatus(): Promise<JobStatus>
    }

Tauri 实现用 Channel/listen；HTTP 实现用 fetch + EventSource。Store 只消费 QueryEvent。

### 14.5 Capability

    PlatformCapabilities {
      nativeWindows
      nativeDialogs
      localFilesystem
      systemTray
      updater
      runtimeExtensions
      scheduledWorkers
      webAuth
      serverManagedSecrets
    }

增加 CI 守护：

    rg @tauri-apps src

只允许命中 src/platform/tauri、desktop-only 与测试 mock 白名单。

## 15. Workflow 与 AI 迁移

### 15.1 Workflow

迁移内容：

- Web Application Service 使用 WorkspaceRepository 加载并校验 WorkflowDefinition；Desktop 使用本地存储加载定义。
- WorkflowDefinition 保留 connectionId 持久化语义。
- Executor 只接收已经解析的 WorkflowDefinition、Core session handle、变量和取消信号，不接收 RequestContext。
- Command Step 调用 Core Runtime；Web 上层在调用前后负责授权、Audit 和历史。
- Web Scheduler 扫描 workspace-scoped schedules；Scheduler 的身份只存在 Web Application 层。
- Workflow history 的 workspace、actor、source 由上层写入。

### 15.2 AI

迁移内容：

- AI Provider 配置按 workspace。
- API Key 进入 SecretStore。
- PromptResolver 支持 instance default + workspace override + driver override。
- StreamCallback 改为 EventSink/JobSink。
- DB Tool 由 Web Application 层完成资源检查和 SQL Audit，再调用 Core Runtime；Desktop 仅执行本地资源检查并写 Query History。
- AskQuestion 为 Job 中间事件；用户回答通过 job continuation API。

AI Job：

    POST /ai-jobs
    GET  /jobs/{id}/events
    POST /ai-jobs/{id}/answers
    POST /jobs/{id}/cancel

回答必须由 Web Application 层校验 job owner、workspace 和 awaiting_input 状态；Core 只处理继续执行所需的输入。

## 16. Dashboard 与 Worker 迁移

### 16.1 MonitorRuntime

MonitorRuntime 不使用浏览器 Session。Web Monitor Worker 在 Application 层读取并校验 workspace 内的 Dashboard、Workflow 和 connection，然后把已经解析的执行对象交给 Core；Desktop Monitor 直接使用本地对象。Worker 身份、workspace 归属和审计字段均由对应上层维护。

### 16.2 调度

单节点实现：

- Server 启动后扫描 enabled interval widgets。
- 使用 DelayQueue 或等价结构维护 next_run_at。
- 加入随机 jitter，避免整分钟同时执行。
- 全局 semaphore + 每 connection mutex。
- timeout 与 retry policy 独立。
- 所有 next state 持久化，不依赖前端。

### 16.3 Desktop

Desktop 继续启动同一 MonitorRuntime，但使用本地对象和 Desktop notification dispatcher，不创建 local workspace/principal 伪装。

### 16.4 Server 通知

Server dispatcher：

- InApp：写 notification/event。
- Webhook：写 outbox，异步投递。
- Desktop channel 在 Server 不可用。
- Email 暂不实现。

## 17. Secret 加密与 Key 管理

### 17.1 Server KeyProvider

    trait KeyProvider {
      load_active_key() -> KeyMaterial
      resolve_key(key_id) -> KeyMaterial
    }

v0.2.0 实现：

- EnvKeyProvider。
- SecretFileKeyProvider。

Desktop 保留 Keychain/File 实现。

### 17.2 启动 fail closed

- 有 encrypted_secrets 且 key 缺失：退出。
- key 解密 canary 失败：退出。
- 空数据库且 key 缺失：生产模式退出，开发模式可显式允许生成。
- 不允许自动生成新 key 使旧数据永久不可读。

### 17.3 Key rotation

v0.2.0 数据模型保留 key_id，但不提供在线 rotation UI。提供离线管理命令：

    datazen-server key rotate --new-key-file ...

流程：

1. 获取独占锁。
2. 事务外逐条解密验证。
3. 事务内写新 ciphertext/key_id。
4. 更新 canary。
5. 输出必须保留旧 key 的回滚提示。

## 18. SQL Prepare、改写与审计

### 18.1 Core 是否重写 SQL

Core 不做数据库方言改写。目标限定、结果行数限制、方言适配等必须由对应 Driver 在 `prepare_sql` 阶段完成；Core 只负责 prepare/execute 时序和执行生命周期，权限、审计和历史由上层负责。

现状中 `qualify_sql_target`、结果限制和部分参数替换可能发生在 Driver 的执行函数内部，导致 Host 记录的 SQL 与数据库实际收到的 SQL 不一致。v0.2.0 将 Driver Command API 拆成显式两阶段：

    #[async_trait]
    pub trait DatabaseDriver {
        async fn prepare_sql(
            &self,
            request: PrepareSqlRequest,
        ) -> Result<PreparedSql, DriverError>;

        async fn execute_prepared_sql(
            &self,
            session: &DbSession,
            prepared: PreparedSql,
            sink: &dyn ResultSink,
        ) -> Result<ExecutionSummary, DriverError>;
    }

`PreparedSql` 是不可变执行计划，至少包含：

    submission_id
    submitted_sql
    statements[] {
      index
      effective_sql_template
      bound_parameters[]     # 仅内存态，供上层按策略审计；不自动持久化
      statement_type
      effective_hash         # hash(template + canonical bound-parameter encoding)
    }
    rewrites[] {
      kind
      actor                 # driver/core-policy
      statement_index
      before_hash
      after_hash
      metadata
    }
    parameter_metadata[]     # name/index/type/null；不包含明文值
    target
    driver_id
    driver_protocol_version

`Core.prepare_sql` 必须在访问目标数据库前把这份 `PreparedSql` 返回给调用层；调用层可以据此做审计、二次确认或直接拒绝。执行完成后，`ExecutionReport.prepared_sql` 回带同一份 prepared 内容，保证上层展示/记录的 SQL 与实际执行计划一致。Core 不提供“执行但隐藏最终 SQL”的接口。

允许的 `RewriteKind` 包括 `QualifyTarget`、`ApplyResultLimit`、`DialectAdaptation`。参数绑定单独记录为 parameter metadata，不把参数明文当作普通 rewrite metadata。`execute_prepared_sql` 必须逐字执行 `effective_sql_template`（配合原生 bind 参数），不得再次追加 LIMIT、限定 schema 或改写语句。无法遵守该约束的 Driver 在 v0.2.0 不得声明支持受审计 SQL 执行。

此改动属于 Driver API 协议变更：提升 `PROTOCOL_VERSION`，`ReuseDriver` 转发新方法，所有 path/Git Driver 同步适配；方言行为与测试仍放在对应 Driver crate。

### 18.2 上层决定审计与执行时序

Query Editor、Workflow、Dashboard、AI Tool、MCP 和内部 Worker 都调用同一个 Core `SqlRuntime`，但不要求 Core 知道调用来源。Router/Tauri Command 不得直调 Driver，应由各自的 Application Service 调用 Core：

    Web Application Service:
      Auth + resource ownership
        → Core.prepare_sql
        → AuditPolicy.evaluate(submitted + effective + rewrites)
        → append Prepared/Started audit event（持久化成功）
        → Core.execute_prepared_sql
        → append Succeeded/Failed/Cancelled audit event
        → QueryHistory summary

    Desktop Application Adapter:
      Tauri command
        → Core.prepare_sql
        → Desktop 写本地 Query History（不写合规级 SQL Audit）
        → Core.execute_prepared_sql
        → 返回 ExecutionReport

审计策略同时分析原始 SQL 和最终 SQL。若 Web AuditPolicy 认为改写导致 statement type、风险等级或目标对象发生非预期变化，可以 Block；Core 不做这个策略判断。Warn 必须由上层在执行事件中保留 finding 与策略版本。

Web 对所有 SQL 执行采用 audit fail-closed：`Prepared/Started` 事件写入 MySQL 失败则不访问用户数据库。Desktop v0.2.0 不启用 SQL Audit，也不存在由 SQL Audit 导致的 fail-closed；危险 SQL 仍由 Safe Mode 和确认策略控制，Core 不强制任何审计策略。

### 18.3 “实际 SQL”的定义与边界

审计中的实际执行表示是 `effective_sql_template + bound_parameters`：前者是 DataZen 通过数据库协议交给目标数据库的最终 SQL 模板，后者是同一次协议执行绑定的实际参数。`effective_hash` 对这两部分的规范化表示计算，而不是只 hash 模板。若 Driver 使用字面量 SQL，则 `bound_parameters` 为空，最终字面量已经包含在 `effective_sql_template` 中。Core 返回这份执行表示，但不负责决定是否保存参数值。

原生 prepared protocol 在数据库侧本来就不存在一条“参数插值后的 SQL 字符串”；因此上层若要展示一条可读 SQL，可以基于该执行表示生成脱敏 preview，但这只是展示/审计视图，不能冒充数据库收到的 wire representation。

`PreparedSql` 和其中的 `bound_parameters` 只在服务端/桌面进程内存中流转，不直接序列化到浏览器、SSE 或普通日志。上层如需持久化，必须先按自身策略做 hash、脱敏或加密。

数据库服务端优化器改写、触发器、存储过程内部语句和代理层改写不在 DataZen 可观测边界内；需要证明这些行为时，应同时开启目标数据库原生审计日志，并使用 request/execution correlation tag 关联。

过渡期若某 Driver 仍采用 `apply_params` 把参数替换成字面量：

- 生成后的 SQL 仍必须在执行前进入 `PreparedSql`，禁止边执行边替换。
- 不得写入 tracing、普通 Query History 或明文 metadata。
- 仅保存 hash、脱敏 preview；按策略保存的全文进入独立加密 payload。
- 对密码、token、二进制和超长值强制掩码。

### 18.4 多语句、限流和重试

- 多语句拆分后按 `statement_index` 记录每条 submitted/effective hash、决策和结果。
- 为探测截断而追加的 `LIMIT n+1` 属于 `ApplyResultLimit`，必须出现在 rewrite chain 和 effective SQL 中。
- 审计后禁止透明重试非幂等写语句；只读重试必须生成新的 `attempt_id`，但沿用同一 `execution_id`。
- Safe Mode 与 SQL Audit 都检查 submitted/effective 两份表示，不能只检查改写前文本。

### 18.5 现有代码迁移映射

首个 Query 纵切按以下方式改造，避免出现“新审计旁路”和“旧执行路径”并存：

- `packages/driver-api/src/traits.rs` 的 `qualify_sql_target` 以及 `execute_standard_sql_command` 内部的结果限制/方言处理，统一收拢到 Driver 的 `prepare_sql`，返回 `PreparedSql.rewrites`。
- `src-tauri/src/commands/driver_command.rs` 的 streaming 与 non-streaming 路径都调用同一个 Core `SqlRuntime`；禁止 streaming 路径自己先 qualify、non-streaming 路径再由 Driver 隐式 qualify。
- 当前 Query History 记录的 `sql` 不再被当作合规审计证据：History 记录 `submitted_sql_preview` 与摘要，Audit 记录 submitted/effective 的 hash、改写链和 outcome。
- Driver 为结果限制生成的最终语句必须通过 `PreparedSql` 回传；Core 不在审计之后再追加 `LIMIT` 或替换参数。
- `ReuseDriver` 必须复用 prepare/execute 协议；MCP DB Tool、Workflow、Dashboard 等上层分别决定自己的授权和审计，再调用 Core `SqlRuntime`。

落地顺序是先为一个主力 Driver 实现 prepare/execute contract，再逐个迁移其他 Driver。未完成迁移的 Driver 只能走兼容模式：明确标记 `audit_accuracy=degraded`，Web 默认禁止执行，避免把不完整日志宣称为完整审计。

## 19. Audit 与日志

### 19.1 Append-only AuditEvent

审计使用 append-only 生命周期事件，不原地覆盖已开始记录。公共字段：

    event_id
    execution_id
    attempt_id
    statement_index
    workspace_id
    actor_type
    actor_id
    source                    # query/workflow/dashboard/ai/mcp/system
    action
    resource_type
    resource_id
    connection_id
    driver_id
    request_id
    event_type                # prepared/started/succeeded/failed/cancelled/outcome_unknown
    decision
    policy_version
    submitted_sql_hash
    submitted_sql_preview
    effective_sql_hash
    effective_sql_preview
    rewrite_chain_json
    parameter_metadata_json
    rows_affected
    duration_ms
    error_code
    occurred_at

默认不保存 Query result rows，也不在普通审计表保存 SQL/参数明文。若 workspace policy 要求保存全文，使用独立 `audit_sql_payloads` 表，以 `execution_id + statement_index` 关联并由 `SecretStore` 加密；读取受专门角色、理由记录和 retention policy 控制。

`query_history` 是方便用户复用的产品数据，可删除且可只保留脱敏摘要；`audit_events` 是合规证据，二者不得共用删除语义。

建议索引：

    INDEX audit_workspace_time(workspace_id, occurred_at, event_id)
    INDEX audit_execution(execution_id, statement_index, occurred_at)
    INDEX audit_connection(connection_id, occurred_at)

`audit_events` 采用按时间分区/归档可选的 schema 设计，但 v0.2.0 不依赖 MySQL 分区特性，以便 SQLite 保持相同语义。清理任务只能按 retention policy 归档或删除已过期合规记录；每次清理本身追加 security audit，不能通过 UI 直接修改或删除单条事件。

### 19.2 崩溃与不确定结果

`Started` 必须在访问目标数据库前持久化，terminal event 在执行返回后追加。进程重启时，Recovery Worker 扫描没有 terminal event 的 `Started`：

- 统一追加 `outcome_unknown`，不得误记为 failed。
- 不自动重放写语句。
- 若 Driver/目标数据库支持可查询的 operation ID，可异步对账后追加 reconciliation event，但不修改历史事件。

`append_execution_started` 与业务元数据写入使用 Persistence `UnitOfWork`；目标数据库事务与控制面 MySQL/SQLite 事务无法原子提交，因此 `outcome_unknown` 是必须保留的真实状态，而不是实现缺陷。

### 19.3 安全事件

必须记录：

- bootstrap。
- 登录成功/失败、退出、Session 撤销。
- 用户启用/禁用、密码重置。
- membership/role 变更。
- connection/AI/Webhook Secret 变更。
- 跨 workspace/forbidden 尝试。
- high-risk command decision。

### 19.4 tracing

每个请求 span：

    request_id
    route
    method
    actor_id
    workspace_id
    status
    latency_ms

禁止记录 Cookie、CSRF、password、API key、connection secret、SQL/参数明文和 Query rows；只允许输出 execution_id、hash、statement type、decision 与稳定错误码。

## 20. Web 安全实现

### 20.1 Security Headers

至少：

- Content-Security-Policy。
- X-Content-Type-Options: nosniff。
- Referrer-Policy。
- Permissions-Policy。
- frame-ancestors 通过 CSP 默认 none。
- HTTPS 部署文档建议 HSTS 由反向代理设置。

### 20.2 CORS

- 默认同源，不添加通配 CORS。
- 若配置独立前端 origin，仅允许显式 allowlist 与 credentials。
- 拒绝 Origin 与 PUBLIC_URL 不一致的 mutation。

### 20.3 SSRF

对 Custom AI endpoint、Webhook、未来远程 MCP：

1. 只允许 http/https。
2. 解析 DNS，检查所有 A/AAAA。
3. 默认拒绝 loopback、link-local、unspecified、multicast、metadata ranges。
4. 每次 redirect 重新校验。
5. 限制 redirect 次数、response size 和 timeout。
6. 允许实例管理员配置 CIDR allowlist。

数据库连接目标不套用同样默认阻断，否则自托管内网数据库不可用；数据库连接由 workspaceAdmin 显式配置，并记录审计。

### 20.4 上传

- 流式写临时文件，不一次性读入内存。
- 总大小、单文件大小和并发限制。
- MIME 与扩展名双校验。
- ZIP 检查条目数、展开大小、压缩比和 traversal。
- 完成校验后原子移动到 workspace 管理目录。
- 用户永远看不到 Server 绝对路径。

## 21. Docker 与部署

### 21.1 镜像

多阶段构建：

    Stage 1: pnpm build Web SPA
    Stage 2: cargo build datazen-server
    Stage 3: Debian slim runtime

Runtime：

- 非 root 用户。
- 只读 root filesystem。
- /var/lib/datazen 可写 volume。
- /tmp 使用 tmpfs。
- 内置 CA certificates。
- 端口 8080。
- basic drivers 作为首发默认；all drivers 使用独立 tag 或后续评估。

### 21.2 Server 静态资源

Server 可以：

1. 编译时 embed dist，得到单 binary 镜像。
2. 运行时从固定只读目录提供 dist。

建议 v0.2.0 使用 embed，减少版本错配。开发模式由 Vite proxy 到 Server。

### 21.3 Compose

至少提供：

- datazen-server。
- MySQL 8.0+，独立 named volume、healthcheck 和初始化字符集配置。
- datazen-server 通过 Secret 读取 `DATAZEN_DATABASE_URL`，等待 MySQL ready 后执行 migration。
- 日志、上传暂存和加密 payload 所需 named volume。
- secret file 示例。
- healthcheck。
- restart policy。
- 可选 Caddy/Nginx HTTPS 示例。

默认 Compose 不加入消息队列。MySQL 是 Web v0.2.0 必需的控制面元数据库；用户创建的目标数据库连接与它相互独立。

### 21.4 优雅关闭

收到 SIGTERM：

1. ready=false。
2. 停止接收新 Job。
3. SSE 发 server.shutdown。
4. 取消或在超时内等待 Query/AI Job。
5. 刷新 Audit/Outbox。
6. 关闭 DB sessions。
7. 释放 Worker 实例租约并关闭 MySQL pool。
8. 退出。

## 22. Frontend 构建与路由

### 22.1 Vite

现有 base 为相对路径以适配 Tauri。Web build 需要显式 mode：

    vite build --mode desktop
    vite build --mode web

差异：

- Desktop 保留相对 asset base。
- Web 根据 DATAZEN_PUBLIC_PATH 或 / 构建。
- Web 只输出主 SPA；不依赖 window.html。
- Desktop 继续输出 main/window entry。

### 22.2 Router

建议增加轻量路由层，至少：

    /login
    /initialize
    /w/{workspaceSlug}/connections
    /w/{workspaceSlug}/workflows
    /w/{workspaceSlug}/dashboards
    /w/{workspaceSlug}/audit
    /settings/*

如果不引入第三方 Router，也必须有统一 route state；禁止继续用 popup 承载 Web 核心页面。

### 22.3 Draft 持久化

- SQL editor draft 可存 IndexedDB/localStorage，key 必须包含 workspaceId、connectionId、panelId。
- 不持久化 dbSessionId、Query rows、Secret 或 AI Key。
- logout/workspace removal 清理该用户的敏感页面缓存。

## 23. 测试实现

### 23.1 Core

- Core 不包含 Authz、Repository、workspace 或身份测试。
- prepare 返回的 effective SQL 与 execute 收到的 SQL 完全一致。
- execute 阶段不会发生隐式改写。
- Query ExecutionHandle 的 cancel、timeout、backpressure 和结果流。
- `ExecutionReport` 始终回带本次执行使用的 PreparedSql。

### 23.2 Server Integration

使用 Router oneshot，不监听真实端口：

- bootstrap 只能一次。
- login Cookie 属性与 CSRF。
- Session idle/absolute expiry。
- membership 修改即时生效。
- workspace IDOR。
- Web AuditPolicy 的 allow/warn/block 与 audit fail-closed。
- append-only lifecycle 与崩溃恢复 `outcome_unknown`。
- Secret write-only。
- SSE replay、lagged、resync、terminal state。
- migration 与 key fail closed。
- MySQL unavailable/pool exhaustion 时 readiness 与稳定错误映射。

### 23.3 Contract Test

同一套领域 contract 分别跑：

    TauriTestAdapter
    HttpTestAdapter

首批 contract：

- Connection CRUD/Test。
- Connect/Disconnect。
- Query stream/cancel/error/history/audit。
- submitted/effective SQL、rewrite chain 与多语句 statement index。
- Schema list。

公共 Persistence contract 以同一组 fixture 跑 `SqlitePersistence` 和 `MySqlPersistence`；WebControlPlane contract 只跑 MySQL。Driver contract 在每个 Driver crate 验证 `prepare_sql` 的 effective SQL 与 `execute_prepared_sql` 收到的内容完全一致，且执行阶段不再改写。

### 23.4 Frontend

- PlatformClient runtime 选择。
- 每个领域 facade 转发参数。
- Tauri/HTTP QueryEvent 等价。
- capability-driven 菜单。
- workspace 切换清理 Store 与 subscription。

### 23.5 E2E

沿用 WebdriverIO：

- 新增 Web browser config 与 Server lifecycle helper。
- Host 通用 Journey 尽量复用 page objects。
- Web 认证/RBAC/部署 Journey 放 e2e/web/ 或等价 Host 目录。
- Driver 专属测试仍在 driver crate。

## 24. CI/CD

新增 jobs：

    core-test
    server-test
    api-schema-check
    frontend-web-typecheck
    web-e2e
    docker-build-amd64
    docker-build-arm64
    container-smoke
    migration-test
    persistence-contract-test
    sql-audit-contract-test
    security-boundary-test

container-smoke：

1. 启动 MySQL 与 datazen-server image。
2. 等待 MySQL 和 Server ready。
3. bootstrap。
4. login。
5. 创建一个“用户目标数据库”的 SQLite 连接（仅用于验证 Driver，不是 Web 元数据库）。
6. SELECT 1。
7. shutdown/restart。
8. 再次 login，验证数据存在。

Release：

- Desktop artifacts 与 Server image 使用同一版本号。
- API version 单独返回。
- 镜像 tag：版本、major.minor、latest。
- 发布说明明确 Desktop/Web capability 差异。

## 25. 分阶段 PR 拆分

### Track A：共享基础

#### PR A1：Core types 与边界

- 新增 packages/core。
- Core SQL request、PreparedSql、ExecutionReport、CoreError、DTO。
- prepare/execute contract 与 fake driver。
- 无业务迁移。

验收：Core 不依赖 tauri/axum/persistence/身份模型；单测通过。

#### PR A2：Frontend PlatformClient skeleton

- runtime/capabilities。
- TauriPlatformClient 包装现有 invoke/listen。
- connection/query facade 切换但行为不变。
- CI 禁止新增散落 Tauri import。

验收：Desktop 全量前端单测与核心 E2E 不回归。

#### PR A3：Persistence API 与 SQLite migration framework

- persistence-api Repository/UnitOfWork 契约。
- SqlitePersistence 与 datazen.sqlite 增量 migration。
- Desktop 单用户资源映射，不创建 local workspace。
- 共享 adapter contract test harness。

验收：v0.1 fixture 升级、失败回滚与备份测试。

### Track B：Server 基础

#### PR B1：Server bootstrap

- packages/server。
- config、health、static SPA、requestId、logging。
- MySqlPersistence、MySQL migration 和 adapter contract tests。
- 含 MySQL 的 Docker dev compose。

#### PR B2：Auth/Workspace

- users/sessions/workspaces/memberships。
- bootstrap/login/logout/session/CSRF。
- RBAC middleware 与安全审计。

#### PR B3：SecretStore

- Env/SecretFile key。
- encrypted_secrets + AAD。
- connection Secret write-only DTO。

### Track C：首个纵切

#### PR C1：ConnectionService

- Web 连接 Repository 与 Web Application Service。
- Core ConnectionManager 只负责连接/句柄生命周期。
- Tauri adapter 切换。
- HTTP routes。

#### PR C2：SessionService + Schema

- Web SessionBroker 与 user/workspace/connection 绑定。
- Web workspace/resource guard。
- Core 只提供无归属的 DbSessionHandle。
- connect/disconnect/schema routes。

#### PR C3：Query Job + SSE

- JobManager。
- Web Query Application Service + Core SqlRuntime。
- Driver `prepare_sql/execute_prepared_sql` 协议与 ReuseDriver 转发。
- Web 上层 submitted/effective SQL 双审计、rewrite chain、append-only outcome。
- Tauri Channel adapter。
- HTTP Job SSE。
- cancel/history/audit。

退出门槛：Web Connection/Query Journey 可 dogfood。

### Track D：领域迁移

#### PR D1：Workflow

- Web workspace repository 与 Application Service。
- Executor 只接收已解析的 WorkflowDefinition 和 Core execution handle。
- run/history/schedule API。

#### PR D2：AI

- workspace provider Secret。
- AI Job SSE。
- AskQuestion continuation。
- Web Application Service 完成 DB Tool 授权和审计，再调用 Core。

#### PR D3：Dashboard

- workspace dashboard store。
- Web/Desktop Worker 分别组装 MonitorRuntime。
- Server Worker、Widget events、通知 outbox。

### Track E：Hardening

#### PR E1：Settings/Capabilities/UI

- 三层设置。
- Web 导航与不支持能力隐藏。
- 用户/成员管理。

#### PR E2：Security

- SSRF、CSP、CORS、rate limit、upload hardening。
- Secret/log redaction。
- 安全测试。

#### PR E3：Deploy/Recovery

- multi-arch image。
- reverse proxy 示例。
- migration backup/restore。
- graceful shutdown。

#### PR E4：RC

- Web + Desktop E2E。
- 性能、兼容、无障碍。
- 文档与 release gates。

## 26. 并行开发规则

可并行：

- A1 Core types 与 B1 Server skeleton。
- A2 Frontend skeleton 与 A3 Store migration。
- D1 Workflow、D2 AI、D3 Dashboard，在 C3 contract 稳定后。
- E2 Security 与 E3 Deploy。

不可并行或必须等待：

- Connection/Query Service 接口未定前，不批量迁移其他 commands。
- Workspace schema 未定前，不迁移 Workflow/Dashboard 表。
- Job/SSE envelope 未定前，不分别实现 AI/Query 两套流协议。
- Authz operation 未定前，不写各 Router 的临时角色判断。

冲突高发文件：

- Cargo.toml
- src-tauri/src/lib.rs
- src-tauri/src/commands/mod.rs
- src/commands/*
- src/stores/*
- SQLite/MySQL migration 与 persistence contract fixtures
- src/locales/en.ts

需要按开发 playbook 分轨或串行合并。

## 27. 性能预算

### 27.1 Server

- 非外部 API p95 小于 300ms。
- SSE publish 到浏览器 p95 小于 1 秒。
- 单实例 20 活跃用户。
- 50 活跃 dbSession。
- 50 interval Widget。
- Job event memory 总预算可配置，默认不超过 256 MiB。

### 27.2 Query

- 默认显示 5,000 行。
- hard limit 50,000 行或 32 MiB。
- rows event 100 行或 256 KiB。
- SSE 慢客户端不能阻塞 Driver callback；达到队列上限后中断并返回 client_too_slow。

### 27.3 Desktop SQLite

- WAL。
- busy timeout。
- 短写事务。
- 外部调用不持有 SQLite transaction。
- 定期 checkpoint。
- Monitor/Audit 高频写入批量化但不丢 terminal state。

### 27.4 Web MySQL

- 独立 control-plane pool；建议初始 `max_connections=20`，按活跃用户和 Worker 压测调整。
- 外部数据库、LLM、Webhook 调用期间不得持有 MySQL transaction。
- 事务重试只处理明确的 deadlock/serialization failure，并要求操作具备 idempotency key。
- `audit_events(execution_id, occurred_at)`、workspace 归属和 Worker claim 建立针对性索引。
- 审计批量写入不得跨越 `Started` 的执行前持久化屏障，也不得丢 terminal event。

## 28. 可观测性

### 28.1 Metrics

    http_requests_total
    http_request_duration
    auth_login_failures
    active_auth_sessions
    active_db_sessions
    db_pool_wait_duration
    jobs_by_state
    job_queue_depth
    sse_clients
    sse_lagged_clients
    workflow_schedule_lag
    dashboard_monitor_lag
    webhook_delivery_failures
    sqlite_busy_errors
    mysql_pool_connections
    mysql_pool_wait_duration
    mysql_transaction_retries
    audit_outcome_unknown_total

### 28.2 Health

- live：事件循环可响应。
- ready：对应 adapter migration 完成、Persistence 可读写、Core/Worker 初始化完成；Web 还要求 MySQL pool 可用。
- health 不主动连接用户数据库或 LLM。

### 28.3 Admin Diagnostics

实例管理员可查看：

- 版本/API version。
- DB schema version。
- active sessions/jobs/workers 数量。
- 最近 migration。
- persistence kind、pool 状态；Desktop 显示数据目录空间，Web 显示 MySQL schema/连接状态。
- Worker last tick。

不展示 Secret、SQL rows 和敏感配置。

## 29. 回滚与恢复

### 29.1 应用回滚

- Server image 可回滚到上一个 patch。
- 只允许回滚到支持当前 schema version 的 binary。
- destructive migration 延后至少一个 minor，不在 v0.2.0 直接删除旧列。

### 29.2 Migration 回滚

- Desktop：migration 前自动创建 SQLite/WAL 一致备份；失败退出并保留 backup 路径。
- Web：升级前由部署流程验证 MySQL 备份/恢复点；migration 失败时 Server 不 ready，并输出逻辑 migration ID，不尝试复制 MySQL 数据文件。
- 恢复需停止对应应用实例、校验 master key，并按 adapter runbook 恢复；MySQL 优先使用备份恢复或前向修复 migration。

### 29.3 Job 恢复

- Query/AI running job 在重启后标记 Interrupted，不自动重放。
- Workflow/Monitor 根据持久化 schedule 重新计算 next run。
- Webhook outbox 可继续重试，使用 delivery id 幂等。

## 30. 技术验收清单

### Core

- [ ] packages/core 不依赖 tauri/axum。
- [ ] Core SqlRuntime 不接收 User/Principal/source/workspace。
- [ ] `prepare_sql` 返回 effective SQL，`execute_prepared_sql` 不再隐式改写。
- [ ] `ExecutionReport` 回带本次执行使用的 PreparedSql。

### Server

- [ ] Bootstrap/Auth/Workspace/RBAC 完成。
- [ ] Cookie/CSRF/CORS/SSRF/Rate Limit 完成。
- [ ] Secret write-only + fail closed。
- [ ] REST/SSE Job 支持 cancel/replay/terminal state。
- [ ] Worker 重启恢复。

### Frontend

- [ ] 业务 Store 不判断 Tauri runtime。
- [ ] commands facade 委托 PlatformClient。
- [ ] Web 不显示 Desktop-only 死入口。
- [ ] workspace 切换清理 session/cache/subscription。

### Data

- [ ] Persistence API 不泄漏 rusqlite/sqlx/MySQL/SQLite 类型。
- [ ] 公共资源契约同时通过 SQLitePersistence/MySqlPersistence；Web Control Plane 契约通过 MySqlPersistence。
- [ ] 公共 migration 逻辑 ID 对齐，Web 专属 migration 不下发到 Desktop，方言脚本分别维护。
- [ ] v0.1 fixture 可升级。
- [ ] Web workspace 复合约束；Desktop 保持单用户本地模型。
- [ ] migration backup/restore 演练。
- [ ] SQL 审计保存 submitted/effective hash、rewrite chain 与参数 metadata。
- [ ] `Started` 先于目标 DB 执行持久化；崩溃恢复生成 `outcome_unknown`。

### Test/Deploy

- [ ] Tauri/HTTP contract test。
- [ ] Web P0 E2E。
- [ ] Desktop 回归。
- [ ] multi-arch container smoke。
- [ ] Release Gates 全通过。

## 31. 建议的首个开发迭代

第一个两周迭代只做以下内容：

1. packages/core skeleton。
2. Core SQL request、PreparedSql、ExecutionReport、CoreError。
3. PlatformClient skeleton + TauriPlatformClient。
4. 现有 connection/query command facade 改为委托 PlatformClient，但仍走原 Tauri IPC。
5. 增量 migration framework 与 Desktop SQLite/Web MySQL migration 测试设计。
6. Persistence API、SQLite/MySQL adapter skeleton 与共享 contract test harness。
7. packages/server skeleton，只提供 health/meta、MySQL readiness 和静态页。
8. Query vertical slice RFC 的 `PreparedSql`、审计事件、DTO 和 contract test fixture。

第一迭代明确不做真实登录页面、不迁移 Workflow/AI/Dashboard、不大规模移动 Rust 文件。其成功标准是依赖方向与迁移路径被代码验证，而不是展示一个只有静态页面的“Web Demo”。
