# Backend Services

> Source of truth: `packages/feature-core/src/services/`（共享规则）与 `src-tauri/src/services/`（桌面绑定）。

Services 是 DataZen backend 中复用性的运行时服务层，不承担 Tauri IPC 的参数边界，也不承担具体 Driver 方言实现。

## 1. 当前服务

| 服务 | 位置 | 职责 |
|---|---|---|
| Runtime sessions/resources | `packages/runtime/src/application/`, `registry/`, `resource/` | session 状态、owner、空闲期限、物理资源、预算与释放 |
| DesktopSessionBackend | `src-tauri/src/platform/session_backend.rs` | Store / DriverRegistry 的桌面适配，打开和关闭 Runtime 物理资源，持有隧道生命周期 |
| QueryExecutor | `packages/feature-core/src/services/query_executor.rs` | 查询参数、过滤、排序和执行辅助 |
| Schema metadata | `src-tauri/src/services/schema_metadata.rs` | 精确关系身份、列批量读取与缓存刷新 |
| DbTools | `packages/feature-core/src/services/db_tools.rs` | 共享 database tools 及 Runtime 执行端口 |
| JobRegistry | `src-tauri/src/services/job_registry.rs` | 兼容长任务的进程内生命周期与取消注册 |
| Runtime 事务 | `packages/runtime` 的 `session_transaction` / `session_transaction_status` | 会话级事务控制、句柄与状态；Host 不再持有事务表（见 [2.3](#23-事务与隧道边界)） |

## 2. Runtime 连接与会话生命周期

### 2.1 ID 与创建

connectionId 标识 Store 中持久化的配置；dbSessionId 标识一次 Runtime session，永不落盘。桌面 PlatformAdapter 组装 RuntimeConnectionUseCases 与 DesktopSessionBackend：Runtime 建立 SessionRegistry 记录，backend 解析 profile、选择 Driver 并打开物理资源。会话 owner 和 connection profile 经 Runtime 的 session / directory API 关联；不再有 Host ConnectionManager 或同 ID 重连映射。

Store connectionId → Runtime connection use case → SessionRegistry dbSessionId
                                      → DesktopSessionBackend → Driver resource（可选 tunnel）

GUI 已连接会话的 Query、Schema 和 Driver Command 使用 dbSessionId。Workflow、MCP、AI database tools 等以配置为入口的路径使用 connectionId，经各自的共享 Runtime execution port 创建或借用资源。

### 2.2 自动空闲回收

Runtime 默认会话空闲期限为 30 分钟，默认扫描间隔为 3 分钟；桌面 adapter 将扫描间隔设为 15 秒。真实业务活动顺延期限；心跳、状态读取和事件订阅不应形成永久保活。detach 后采用 60 秒 grace。到期扫描先回收已到期的资源池 / 隔离资源，再通过 SessionRegistry 串行裁决会话；会话到期时在原资源上 rollback-and-close、撤销活连接借用并清理 Runtime 会话登记。关闭不确定时保留隔离及预算状态，不伪称资源已释放。失效的 dbSessionId 返回 SessionLost，调用方必须显式建立新会话，不透明重连。

资源池空闲 TTL 是另一条策略：短操作池 TTL 为 60 秒；只有通过 driver 复位能力验证的资源才允许停放复用。当前 PostgreSQL / MySQL driver 声明 ResetForReuse::Unsupported，归还时关闭资源，因而当前不能依赖池化保温。

### 2.3 事务与隧道边界

事务控制已移入 Runtime 的 `session_transaction` / `session_transaction_status`：授权和 owner/epoch 检查之后，同会话控制串行，经 ExecutionGateway 的 `platform.*Transaction` 命令调用既有 ResourceProvider 事务端口，句柄和实际状态归 SessionRegistry actor。Host 不再保存 `AppState.session_transactions` 或 driver TransactionHandle。到期/close 统一走 Runtime 的 rollback-and-close；过期 status 返回 false，旧 commit/rollback 拒绝且不重新派发。Unknown 控制保留 Runtime 围栏，不能重发 begin/commit；调用方取消不放开仍在运行的控制任务。Tauri IPC 的参数、bool/void 返回和 Workflow v1 协议保持不变。

桌面 backend 在建立物理资源前解析保存的隧道配置；隧道 Arc 与 Runtime 物理 Resource 同生共死。池化或 quarantine 会继续持有隧道；仅确认物理资源关闭后才释放该资源及隧道。Driver API 与协议不感知隧道类型，详见 tunnel.md。

## 3. 消费方边界

| 消费方 | 配置 / 会话入口 | 资源与生命周期 |
|---|---|---|
| Query、Schema、编辑器 | 已连接会话的 dbSessionId | Runtime session live lease；过期明确失效，不重连旧 ID |
| DataTable、表导出 | 调用方 session 或专用 export session | Runtime execution / bounded output；session close 按 Runtime 结果确认 |
| Schema Diff、Data Sync、Data Transfer | 持久化 profile 与 job 的明确端点 | Job adapter 使用专用 Runtime session；Job 生命周期独立于窗口 |
| Workflow v1 | 配置 connectionId 与既有 step target | 保留 v1 命令与结果传递；底层执行走 Runtime resource path |
| MCP / AI database tools | 工具参数中的 connectionId | feature-core Runtime execution port；不借用 GUI 的事务 session |
| Wapp | Bridge 的 connectionId / host 解析出的当前会话 | 执行仍经受控 Driver Command 与 Runtime backend |
| Monitor | 已保存 grant / workflow target | 共享 Workflow / Runtime scheduler 路径 |

所有 session 操作都必须使用 dbSessionId，所有配置归属与调度语义都使用 connectionId。关闭窗口不自动取消已受理的 Job；取消仍以具体 Query / Job / AI operation 的 owner 和句柄为准。

### 取消维度：五种独立的取消机制

「取消」在宿主里不是一个全局机制。查询、兼容旧任务、迁移三件套 Job 与 AI 各有不同 owner/state source；key、归属校验和重启行为不同——**拿一种机制的 key 去查另一种机制会落空**：

| 取消表 | key | 粒度 | 归属校验 | 生命周期 | 位置 |
|---|---|---|---|---|---|
| 查询流执行注册表 | `QueryExecutionId` | 单次流式查询执行 | **有**：发起会话与执行记录不一致直接拒绝（`belongs to a different db session`） | 进程内；条目在每个流的终态路径上被移除，未知或已过期的 ID 报 `unknown or stale` | `AppState.query_executions`（`commands/mod.rs:88-142`） |
| Legacy job registry | `jobId` | 仍使用兼容执行路径的旧任务 | 只有 job ID，不按 `connectionId` / `dbSessionId` 归类 | 进程内静态表；`cancel_job` 先置位再返回，支持先取消后开始；重启失效 | `services/job_registry.rs`（全局 `static JOBS`） |
| Migration Job state | `jobId` | Schema Diff、Data Sync、Data Transfer 的 prepare/apply | 本机 DesktopJobHost 的 RequestContext / 仓储校验 | cancel intent 和 Job 结果保存在 `{appData}/datazen.sqlite`；关闭窗口后继续当前进程里的 handler。重启只标记恢复候选，不复用旧 worker/session 或自动重放 | `AppState.desktop_job_host`、`store/app_db/jobs/` |
| AI 调用取消注册表 | AI 调用 ID（`request_id`） | 单次 AI 调用（含流式） | **无**：只按调用 ID 查找 | 进程内；开始时登记、结束时注销，取消一个从未登记或已注销的调用返回 `false` 且无副作用 | `ai/cancel.rs` |

要点：

- Query 通过 Runtime session 进行 owner 校验；Schema Diff、Data Sync、Data Transfer 的长任务由 DesktopJobHost 持有 cancel intent。
- MCP / Workflow 通过各自运行时用例执行，不依赖共享 GUI dbSessionId 或旧 ConnectionManager。
- Host compatibility JobRegistry 与 AI cancel registry 仍是各自独立的进程内状态；它们不是连接生命周期管理器。

## 4. Query execution

普通 Query 由 command 层进入 QueryExecutor / Driver。Services 不根据 PostgreSQL/MySQL 等类型拼接方言 SQL；数据库特定能力由 Driver 提供。

Query 是最纯粹的会话消费方，也是这套约定的基准形状：

- **创建**：不创建。执行、EXPLAIN、流式查询的 IPC 形参都是 `dbSessionId`，命令体内只做会话查找。
- **复用**：使用调用方提供的 Runtime session；会话若已被空闲回收，旧 `dbSessionId` 明确返回 `SessionLost`，调用方必须显式打开新 session。
- **关闭**：不在 Query 层关闭。会话由连接 IPC 释放或断开。
- **取消**：按**执行**而不是按会话取消。流式执行分配一个执行 ID，取消时校验发起者与会话一致，不一致直接拒绝——避免一个面板取消另一个面板的查询。
- **配置目标**：database 随命令 envelope 传递，schema 恒为 `None`（不切库）。目标名的解析交给驱动：能内联限定名的驱动自己改写，不能的驱动由用户写全限定名。**读**命令另有 `closeDatabase`：它关掉当前打开的库并作废 schema 缓存，不是把会话切到别的库。

事务：显式控制和句柄登记归 Runtime（见 [2.3](#23-事务与隧道边界)）；Host 仅适配原 IPC，不另存事务表。

## 5. 表与数据导出

DataTable 的行编辑与数据导出是两条不同的写路径，不要按同一套语义理解：

**行编辑（DataTable 暂存改动的提交）**

- **创建 / 复用**：都在调用方已建的 Runtime session 上执行。会话找不到或已过期时明确失败，不以同一 `dbSessionId` 透明重建。
- **事务**：预览待提交改动**不**开事务，只读；真正执行改动计划时，若该会话已有显式事务就复用，否则自己开一个。语义不明确的写入（例如无法判定影响行数）一律拒绝，不做「尽力而为」。
- **写入参数**：UPDATE / DELETE 的 SQL 只包含驱动占位符；列值与主键值通过驱动绑定 API 单独传递。驱动先构造整批语句，确认占位符与绑定执行均受支持后才开启事务。预览中的 SQL 模板不包含数据值。
- **关闭**：不涉及。
- **取消**：无独立的取消句柄——粒度是「提交前 / 提交后」，不是执行中途。
- **配置目标**：提交前复核四件事：改动计划里的表必须属于这条会话的**所属连接**、驱动类型与 database/schema 必须与当前配置一致、连接不得只读、以及结构指纹是否仍然匹配。任一不符即中止，避免把过期界面上的改动写到已经变化的表上。表所属连接从仍有效的 Runtime session 读取（见 [2.1](#21-连接-id-约定)）；session 到期或丢失时操作应以 `SessionLost` 失败，不保证在回收后继续保留旧 session 的 owner 映射，也不得用同一 `dbSessionId` 透明重建。

**数据导出**

- **创建 / 复用**：只消费调用方的 `dbSessionId`，不创建；导出前先过 SQL 安全门闸，再以流式回调逐批输出。
- **SQL 格式**：查询筛选和 SQL INSERT 导出使用驱动显式声明的字面量方言；不支持该方言或遇到无法表示的值时返回错误，不能退回共享反斜杠替换。CSV / JSON 导出不经过 SQL 字面量格式器。
- **关闭 / 取消**：**导出没有中途取消机制**——没有取消令牌也没有取消标志位，与 Data Sync / Data Transfer 的 job 标志是两种模型。导出结果里的「已取消」只表示用户在保存对话框里放弃了选文件，也就是导出根本没开始；一旦开始流式写盘，就只能由查询本身失败而结束。
- **配置目标**：database / schema 随导出请求传递，并从会话配置补齐缺省项，不读会话当前状态。

## 6. 长任务

Schema Diff、Data Sync 与 Data Transfer 的 prepare/apply 长任务均由共享 `DesktopJobHost` 管理，关闭 UI 不作为取消信号。各自 IPC 按 `jobId` 写入持久 cancel intent；Job handler 在当前进程中由后台任务继续执行，AppDb 保存 Job state、receipt、执行边界和结果。进程重启不会恢复旧 handler，而将未派发 Queued / 运行中任务分别标记为 NotExecuted / PendingVerification。横向差异见[取消机制表](#取消维度五种独立的取消机制)。

## 7. 依赖方向

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

## 8. Schema metadata

宿主在 `execute_driver_command` 网关注册 `list_catalog`、`read_relation_columns`、`read_relation_schema` 和 `refresh_schema_metadata`。会话通过 envelope 的 `dbSessionId` 指定，database/schema/name 全部放在 input 的 `RelationRef` 中；这组命令拒绝重复的 envelope database/schema，避免目标冲突。

`RelationRef` 的 schema 必须显式为字符串或 null。具有 schema 层级的驱动要求精确 schema；驱动契约里已经没有 `use_database`，元数据读取不会改变会话上下文，目标只能由调用方在 `RelationRef` 里写全。目录响应分开返回 schema 名称与关系条目，空 schema 不再伪装成空名称关系。

列读取每次最多接受 256 个目标，按完整身份去重，响应按请求顺序为各关系返回 `ok` 或 `error`。同一 database/schema 的未缓存目标可以使用驱动批量读取；驱动不支持批量读取时使用逐关系读取。完整结构缓存同时填充列缓存；失败信息经过凭据脱敏。

刷新支持 session、database 或精确 relation 范围，返回缓存 revision。缓存写入检查读取开始时的 generation，刷新前启动的读取不能重新填入缓存。旧的六个 Host schema IPC 已删除。查询数据和 ER 图聚合继续使用各自的 Host 命令；AI、Workflow 与驱动扩展通过 Driver Command 访问标准元数据命令。新 metadata commands 与这些入口共用 `SchemaCache`。
