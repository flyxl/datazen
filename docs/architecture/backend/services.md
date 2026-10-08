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

### 2.1 连接 ID 约定

DataZen 明确区分两种 ID，二者不可互换，也不存在双模回退：

| ID | 含义 | 落盘 | 语义 |
|---|---|---|---|
| `connectionId` | 持久化连接配置 ID（原 `configId`） | 写入 `connections.json` / 任务 / profile | 配置、归属、调度 |
| `dbSessionId` | 运行时数据库 session ID | **永不落盘** | 操作已建立的会话 |

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

从持久化 ID 到运行时会话只有三个入口（`connection_manager/sessions.rs`、`connection_manager/connections.rs`）：

| 入口 | 行为 | 语义 |
|---|---|---|
| `get_or_connect_session(connection_id)` | 在 `session_owner_map` 中查找 owner 等于该 `connectionId` 的**任意**现存会话；命中即复用并 +1 引用计数，否则新建物理连接 | 复用优先 |
| `connect_dedicated(connection_id, database)` | 绕过复用扫描，始终新建物理连接，随后 +1 引用计数 | 需要独立会话或独立 database |
| `resolve_session_for_connection(connection_id)` | 前两者的组合：先 `get_or_connect_session`，再 `get_session` | 「给我这个 `connectionId` 对应的会话」 |

`get_or_connect_session` 对同一 `connectionId` 的所有调用方返回**同一个** `dbSessionId`；调用方之间没有会话隔离，除非显式走 `connect_dedicated`。`resolve_session_for_connection` 返回的运行时会话 ID 会被 Workflow、MCP / AI db tools、Data Sync 的共享路径直接丢弃，**这些路径不调用 `release`**——引用计数只增不减。

### 2.2 引用计数、释放与 idle eviction

`release(db_session_id)` 递减计数，仅在计数归零时断开；`disconnect(db_session_id)` 立即清除计数、owner 映射并调用 `driver.disconnect`。

后台 `cleanup_idle_connections` 每 60 秒执行一次，只挑「引用计数为 0 且 `last_used` 超过 `idle_timeout`」的会话，`idle_timeout` 默认 1800 秒。该清理**故意不经过 `disconnect()`**：保留 `session_owner_map` 条目才能让 `reconnect()` 以同一个 `dbSessionId` 重建会话。因此被清理的物理连接消失了，`dbSessionId` 在映射上仍然「存在」；后续 `get_session` 未命中会直接 `reconnect`，并把新句柄的 ID 重写为同一个 `dbSessionId`。代价见下一节。

### 2.3 事务句柄不随 idle eviction 回收（已实现的现状）

`AppState.session_transactions` 保存「`dbSessionId` → 事务句柄」映射，它位于 `AppState` 而不在 `ConnectionManager` 内：

- `begin_session_transaction_impl` 用 `get_session` 开启事务并写入该表，**不取引用计数**；
- `release_connection_impl` 只递减引用，不回滚事务；后台 idle eviction 也只丢物理连接，两者都不碰这张表；
- 只有 `disconnect_impl` 会先移除并回滚事务句柄再断开。

因此存在一条可达路径：`connect` → `begin_session_transaction` → `release_connection`（计数归零、事务句柄残留）→ 空闲超过 1800 秒被清理 → 对同一 `dbSessionId` 提交 → `get_session` 触发 `reconnect` → 提交落在一条**从未开启该事务**的物理连接上，且提交按成功返回。这条连续旅程目前没有测试覆盖；本节只记录已实现的现状，不预判修复形态。

### 2.4 隧道

连接建立时先解析隧道再创建 driver：配置若带 `tunnel_id`，`resolve_tunnel_ref` 会从 `tunnels.json` 还原为内联的 `tunnel_kind` + 各类隧道参数；随后 `maybe_start_tunnel` 在本机监听回环端口并把 driver 看到的 host/port 改写为该回环地址。四种隧道类型（直连 / SSH / HTTP CONNECT / WebSocket）的差异对 driver 不可见。详见 [tunnel.md](tunnel.md)。

## 3. 消费方的会话获取与目标路径

下表是**已实现的现状**：每个消费方在「创建 / 复用 / 关闭 / 取消 / 配置与目标路径」五个维度上各自做了什么。文档描述的是当前代码行为，不是目标形态。

| 消费方 | 创建 | 复用 | 关闭 | 取消 | 配置与目标路径 |
|---|---|---|---|---|---|
| **Query** | 不创建。`execute_query` / `execute_query_stream` / `get_explain` 一律收 `db_session_id` | 与同一 `connectionId` 的所有消费方共享同一条会话 | 不关闭；会话由连接面板的 `release_connection` / `disconnect` 结束 | `QueryExecutionId` + 归属校验（校验 `dbSessionId` 与执行记录一致后才允许取消） | database 随命令 envelope 传递，`schema` 固定为 `None`；不做切库，手写 SQL 落到连接默认库 |
| **Table / DataTable / 导出** | 不创建。三个入口都只收 `db_session_id` | DataTable 写操作**复用同一 `dbSessionId` 上已存在的事务**，没有才自开一个 | 不关闭。导出同样只消费既有会话 | **导出没有中途取消**：结果里的「已取消」只代表保存对话框被放弃 | DataTable 提交前把 `dbSessionId` 映射回 `connectionId`，与表格记录上的 `connectionId` 不一致即拒绝 |
| **Schema Diff** | 后端不创建；专用会话由前端 `useSchemaDiffEndpoints` 调 `connectDedicated` 建立并在结束时 `releaseDedicatedSession` | 不与主工作区共享 | 前端在比较结束 / 卸载时释放 | 部署通过旧进程级 job registry 按 `jobId` 取消（`cancel_schema_diff_deploy`）；比较阶段无取消通道，尚未接入 P5 DesktopJobHost | IPC 收成对的 `sourceDbSessionId` / `targetDbSessionId`；profile 落盘的是 `sourceConnectionId` / `targetConnectionId`，部署记录也只写 `connectionId` |
| **Data Sync** | `resolve_task_endpoint`：任务保存了**非空目标库**时走 `connect_dedicated`（含恰好等于连接默认库的情形），未指定时才走 `resolve_session_for_connection` | 共享路径下与 GUI 同一条会话 | 仅 dedicated 路径释放；共享路径不释放 | 独立的进程内 `InMemoryJobRepository` 记录 cancel intent；未受理前由窗口 job registry 暂存 | task 文件保存 `sourceConnectionId` / `targetConnectionId` 与四个 database/schema 字段；Job runtime/artifact 仍是进程内状态，尚未接入 AppDb DesktopJobHost |
| **Data Transfer** | 兼容执行路径仍由前端 `DataTransferWindow` 持有源 / 目标会话；P5 prepare/apply 从 live endpoint 组装 handler | 不与主工作区共享 | 运行时 session 由当前 handler 使用；窗口卸载不取消已受理 P5 apply | P5 命令通过 `AppState.desktop_job_host` 持久 cancel intent；未知旧 job ID 回退到进程级 registry | 运行时 endpoint 不落盘；P5 Job 只保存安全 plan projection / 结果引用，plan body 与 session 仍为内存态 |
| **Workflow** | `resolve_session_for_connection`（首次执行时建立） | 与 GUI 同一条会话 | **不调用 `release`**：命令运行时与执行器都不释放，引用计数常驻 ≥1，因此这类会话不会被 idle eviction 回收 | 无独立取消通道，失败按 step 的错误策略 abort / skip / fallback | step 显式目标 → block 目标 → workflow 默认目标 → profile 初始目标；目标随命令输入走，**不做切库** |
| **MCP** | DB tools 收**持久化 `connection_id`**，经 `resolve_connection_with_id` → `resolve_session_for_connection` 建立 | 与 GUI 共享同一个 `ConnectionManager` 实例，MCP 的一次 `query` 可能命中的正是 GUI 查询编辑器正在用的那条会话 | 不释放引用 | 无独立取消通道 | database / schema 全部来自调用参数 → 连接配置回退 → 驱动约定，**从不读会话当前库**；白名单为空时按拒绝一切处理 |
| **AI 诊断** | 不创建。诊断、NL2SQL、EXPLAIN 分析、Schema 文档都只收 `db_session_id` | AI Chat 的 schema 上下文分支用既有会话；取不到时**静默跳过 DDL 上下文，db tools 仍然注入**（见 [ai.md](./ai.md)） | 不关闭 | AI 侧独立的取消注册表 | 唯一反向的入口是查询历史分析：它把 `dbSessionId` 映射回 `connectionId`，再按连接过滤历史 |

要点：

- **会话创建集中在少数入口**。生产代码里真正新建会话的地方只有连接 IPC（共享 / 专用两种）、Data Sync 的端点解析、Workflow 的命令运行时与执行器、以及 db_tools（MCP 与 AI 共用）这几处；Query、DataTable、导出、Schema Diff、Data Transfer、AI 诊断都只消费已存在的 `dbSessionId`。
- **「三件套」的专用会话不是同一套机制**。Schema Diff 与 Data Transfer 的专用会话由**前端**建立并释放；Data Sync 的专用会话由**后端**按任务是否保存了非空目标库自行决定要不要开（判据是字符串是否为空，不是「目标库是否等于连接默认库」，见 `commands/sync/tasks.rs:77-108`）。三者都避免与主工作区共享事务状态。
- **共享即钉住**。Workflow 与 MCP 取得会话后不释放引用，使这些会话不再满足 idle eviction 的「计数为 0」条件。
- **不切库是全局约束**。驱动契约里已经没有 `use_database` 方法；源码中残留的同名符号只有 Mock 驱动测试中恒空的 `use_database_calls()` 回归绊线，以及 MySQL 驱动在**建连时**内部拼 `USE` 语句的细节——两者都不构成命令级切库通道。因此所有消费方的目标都通过命令 envelope 传递。

### 取消维度：五种独立的取消机制

「取消」在宿主里不是一个全局机制。查询、旧任务、Data Sync、P5 Data Transfer 与 AI 各有不同 owner/state source；key、归属校验和重启行为不同——**拿一种机制的 key 去查另一种机制会落空**：

| 取消表 | key | 粒度 | 归属校验 | 生命周期 | 位置 |
|---|---|---|---|---|---|
| 查询流执行注册表 | `QueryExecutionId` | 单次流式查询执行 | **有**：发起会话与执行记录不一致直接拒绝（`belongs to a different db session`） | 进程内；条目在每个流的终态路径上被移除，未知或已过期的 ID 报 `unknown or stale` | `AppState.query_executions`（`commands/mod.rs:88-142`） |
| Legacy job registry | `jobId` | Schema Diff 部署及 Data Transfer 兼容路径 | 只有 job ID，不按 `connectionId` / `dbSessionId` 归类 | 进程内静态表；`cancel_job` 先置位再返回，支持先取消后开始；重启失效 | `services/job_registry.rs`（全局 `static JOBS`） |
| Data Sync Job state | `jobId` | Data Sync prepare/apply | Job 上下文 + Data Sync 进程内仓储 | `InMemoryJobRepository`、ChangeSet 与窗口登记表均为进程内；没有跨重启恢复 | `commands/sync/host/state.rs`、`commands/sync/jobs.rs` |
| P5 Data Transfer Job state | `jobId` | `dataTransferPrepare` / `dataTransferApply` | 由本机 DesktopJobHost 的 RequestContext / 仓储校验 | cancel intent 和 Job 结果保存在 `{appData}/datazen.sqlite`；关闭窗口后继续当前进程里的 handler。重启只标记恢复候选，不复用旧 worker/session 或自动重放 | `AppState.desktop_job_host`、`store/app_db/jobs/` |
| AI 调用取消注册表 | AI 调用 ID（`request_id`） | 单次 AI 调用（含流式） | **无**：只按调用 ID 查找 | 进程内；开始时登记、结束时注销，取消一个从未登记或已注销的调用返回 `false` 且无副作用 | `ai/cancel.rs` |

要点：

- Query 校验会话归属；Data Sync 与 P5 DesktopJobHost 通过其 Job record 的作用域校验；旧 registry 仍只以 jobId 寻址。
- Legacy registry 支持先取消再开始；Data Sync 和 P5 host 则把 cancel intent 写入其 Job record。P5 host 的 cancel 与 Job metadata 重启后可读，但不会因此恢复旧运行时资源。
- Data Sync / Legacy registry / AI cancel state 仍只存在内存；P5 Data Transfer 是当前唯一接入 AppDb 持久 host 的迁移 consumer。

## 4. Query execution

普通 Query 由 command 层进入 QueryExecutor / Driver。Services 不根据 PostgreSQL/MySQL 等类型拼接方言 SQL；数据库特定能力由 Driver 提供。

Query 是最纯粹的会话消费方，也是这套约定的基准形状：

- **创建**：不创建。执行、EXPLAIN、流式查询的 IPC 形参都是 `dbSessionId`，命令体内只做会话查找。
- **复用**：无独立策略——用的是调用方给的那条会话；会话若已被空闲回收，会话管理器会用同一个 ID 重建物理连接（见 [2.2](#22-引用计数释放与-idle-eviction)）。
- **关闭**：不在 Query 层关闭。会话由连接 IPC 释放或断开。
- **取消**：按**执行**而不是按会话取消。流式执行分配一个执行 ID，取消时校验发起者与会话一致，不一致直接拒绝——避免一个面板取消另一个面板的查询。
- **配置目标**：database 随命令 envelope 传递，schema 恒为 `None`（不切库）。目标名的解析交给驱动：能内联限定名的驱动自己改写，不能的驱动由用户写全限定名。**读**命令另有 `closeDatabase`：它关掉当前打开的库并作废 schema 缓存，不是把会话切到别的库。

事务：显式事务按会话登记句柄。开启事务**不取引用**，因此它的存亡不受引用计数保护（见 [2.3](#23-事务句柄不随-idle-eviction-回收已实现的现状)）。

## 5. 表与数据导出

DataTable 的行编辑与数据导出是两条不同的写路径，不要按同一套语义理解：

**行编辑（DataTable 暂存改动的提交）**

- **创建 / 复用**：都只在调用方已建的会话上执行。会话找不到时走同一条透明重建路径。
- **事务**：预览待提交改动**不**开事务，只读；真正执行改动计划时，若该会话已有显式事务就复用，否则自己开一个。语义不明确的写入（例如无法判定影响行数）一律拒绝，不做「尽力而为」。
- **关闭**：不涉及。
- **取消**：无独立的取消句柄——粒度是「提交前 / 提交后」，不是执行中途。
- **配置目标**：提交前复核四件事：改动计划里的表必须属于这条会话的**所属连接**、驱动类型与 database/schema 必须与当前配置一致、连接不得只读、以及结构指纹是否仍然匹配。任一不符即中止，避免把过期界面上的改动写到已经变化的表上。表所属连接是从会话反查出来的（见 [2.1](#21-连接-id-约定)），该反查依赖 owner 映射在空闲回收后仍然保留。

**数据导出**

- **创建 / 复用**：只消费调用方的 `dbSessionId`，不创建；导出前先过 SQL 安全门闸，再以流式回调逐批输出。
- **关闭 / 取消**：**导出没有中途取消机制**——没有取消令牌也没有取消标志位，与 Data Sync / Data Transfer 的 job 标志是两种模型。导出结果里的「已取消」只表示用户在保存对话框里放弃了选文件，也就是导出根本没开始；一旦开始流式写盘，就只能由查询本身失败而结束。
- **配置目标**：database / schema 随导出请求传递，并从会话配置补齐缺省项，不读会话当前状态。

## 6. 长任务

Schema Diff、Data Sync 与 Data Transfer 的长任务仍由各自 IPC 暴露显式取消，关闭 UI 不作为取消信号。Schema Diff 部署仍走旧进程级 registry；Data Sync 使用独立内存 JobRepository；P5 Data Transfer 使用本机持久化 `DesktopJobHost`，经 `cancel_data_transfer` 写入 cancel intent。Transfer apply 在当前进程中由后台任务继续执行，AppDb 保存 Job state 与 receipt；进程重启不会恢复 handler，而将 Queued/Running 任务分别标记为 NotExecuted/PendingVerification。横向差异见[取消机制表](#取消维度五种独立的取消机制)。

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
