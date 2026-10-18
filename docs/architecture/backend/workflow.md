# Workflow 模块

> [返回架构总览](../README.md)

用户自定义 AI / Command 工作流引擎。Workflow 引擎独立于 MCP 协议；GUI、Tauri IPC 与 MCP Server 都是调用方。

## 模块结构

```text
src-tauri/src/workflow/
├── mod.rs               # 模块出口
├── workflows.rs         # 对外兼容 facade / re-export
├── model.rs             # WorkflowDefinition / WorkflowStep 等数据模型
├── registry.rs          # YAML workflow 注册、加载、保存、删除
├── context.rs           # 模板解析、路径解析、循环上下文
├── conditions.rs        # Condition 表达式求值
├── executor.rs          # WorkflowExecutor：步骤编排、错误策略、超时
├── command.rs           # Workflow Command Step 数据结构
├── command_runtime.rs   # Connection 解析、Command discovery、Driver command 执行
└── history.rs           # 执行历史持久化
```

`workflows.rs` 保留为兼容 facade；新的业务代码应直接依赖拆分后的模块，而不要继续向 `workflows.rs` 堆积实现。

## 执行模型

Workflow 的核心执行路径统一为：

```text
WorkflowDefinition
        │
        ▼
WorkflowExecutor
        │
        ├── Condition / ForEach / Ai
        │
        └── Command
              │
              ▼
       Command Runtime
              │
              ├── resolve effective connection
              ├── resolve command definition
              ├── resolve template variables
              ├── validate input against schema
              └── Driver::execute_command()
```

### Connection 继承

Workflow 可以在顶层定义默认 connection：

```yaml
connection: mysql-prod
steps:
  - type: command
    command: query
    input:
      sql: SELECT * FROM users
  - type: command
    connection: postgres-prod
    command: query
    input:
      sql: SELECT * FROM orders
```

Step 没有指定 `connection` 时继承 Workflow 默认 connection；Step 显式指定时覆盖默认值。这样常见的“一个 connection 执行多个 step”不需要每个 step 重复选择 connection。

### 会话：共享、且不释放

Workflow 的 IPC 用**持久化 `connectionId`** 指定连接（不要求连接的命令可以只给 `driverType`，见 [IPC](#ipc)）。每条 step 执行时用 `resolve_session_for_connection(connectionId)` 取得运行时会话，但**拿到的会话 ID 被直接丢弃**（`let (_runtime_id, driver, handle)`）——它只被当作 driver 句柄用完就扔。

由此产生三条已实现的事实：

- **与 GUI 共享同一条会话**。同一 `connectionId` 在 GUI 里已连接时，Workflow 走的是那一条，而不是另开一条。好处是上下文一致，代价是无法与主工作区隔离事务状态。
- **引用只增不减**。该路径不调用 `release`，所以被 Workflow 执行过的会话引用计数常驻 ≥1，永远不满足空闲回收的「计数为 0」条件。会话只会在用户显式断开连接或应用退出时结束。
- **取显示名也会建会话**。执行器为某个 step 计算展示用的连接名时同样调用 `resolve_session_for_connection`，因此一次纯粹的标签解析就能建立并钉住一条会话。

唯一会释放的例外是 Workflow 定义迁移（把 YAML 转成内部定义的路径），它在解析完连接后显式归还引用。

### 目标：不切库

Step 的目标 database / schema **随命令输入一起下发**，宿主不做切库。能内联改写 SQL 的驱动由驱动自己把未限定名改写到 step 的目标库；不做改写的驱动则由用户自己写全限定名。宿主没有生产环境的切库路径：驱动契约里已经没有 `use_database` 方法，源码中残留的同名符号只有 Mock 驱动测试里恒空的 `use_database_calls()` 回归绊线，以及 MySQL 驱动在**建连时**内部拼 `USE` 语句的细节——后者不构成 step 级的切库命令。

## Command 模型

Command 是 Driver 暴露给 Workflow / IPC / UI 的统一能力描述。

```text
Driver
 ├── command_definitions()
 │      ├── name / description / input_schema
 │      └── metadata
 │            ├── category   query | mutate | admin | observe | pubSub | stream | io
 │            ├── risk       read | write | highRisk（可省略，由 permissions 推导）
 │            ├── workflow            是否出现在 Workflow Command 选择器
 │            ├── ui                  是否面向 GUI
 │            ├── requiresConnection  false 时可只凭 driverType 执行
 │            └── deprecated / replacedBy
 │
 └── execute_command(command, input)
```

标准 Command 至少包括：

- `query`：查询并返回行数据
- `execute`：执行非查询语句（只读驱动可以不暴露）

Driver 可以增加自己的特殊 Command，例如 NoSQL / KV / 搜索类驱动不需要伪装成 SQL。`sql` 输入字段可通过 JSON Schema `title` 标明实际语言（JSON command / PromQL / InfluxQL 等）。Workflow Engine 不根据具体 Driver 类型写分支，而是通过 Command Definition + JSON input/output 调用 Driver。`metadata.workflow = false` 的 Command（例如 Redis Pub/Sub subscribe）不会进入 Workflow 选择器，运行时也会拒绝。

## Legacy Query 兼容

旧版 Workflow 使用：

```yaml
type: query
connection: mysql-prod
database: reporting
sql: SELECT * FROM users
```

该格式继续支持反序列化。执行前会规范化为内部 Command：

```text
Legacy Query
    │
    ▼
WorkflowCommandStep::from_legacy_query()
    │
    ▼
command = query
input.sql = sql
input.database = database（如果存在）
    │
    ▼
Command Runtime
    │
    ▼
Driver::execute_command("query", input)
```

因此旧配置和新 Command Step 使用同一条执行路径，同时保留旧版 `database` 语义。

## Command Discovery

UI 在编辑 Command Step 时可以根据当前有效 connection 获取对应 Driver 的 Command Definition：

```text
Workflow / Step connection
        │
        ▼
get_connection_commands
        │
        ▼
Driver::command_definitions()
        │
        ▼
Command selector
        │
        ▼
input_schema → schema-driven input editor
```

Connection 改变后应重新 discovery；没有指定 Step connection 时使用 Workflow 默认 connection。

## IPC

Workflow Command Runtime 同时被 Tauri IPC 使用，主要能力包括：

- 获取指定 connection 支持的 Commands
- 获取 Driver 支持的 Commands（无需 live Connection）
- 执行指定 Driver Command（`connectionId` 或 `driverType`；后者仅允许 `requiresConnection = false`）
- SQL 编辑器通过 `query` Command 执行；兼容 `execute_query` IPC 转发到同一路径
- Workflow 的创建、读取、更新、删除与执行
- Workflow execution history

前端 `src/commands/` 与 Rust `src-tauri/src/commands/` 保持领域对齐。

## 能力概要

- YAML 格式定义
- Query / Command / Ai / Condition / ForEach / Merge / Transform
- Driver-specific Command
- Connection inheritance / per-step override
- Command input schema 校验
- 变量替换：`{{var}}`、`{{steps.id.result}}`、深层路径与通配符
- 跨库执行
- 错误策略：abort / skip / fallback
- 查询结果行数限制
- 路径遍历防护
- GUI / IPC / MCP 共用 Workflow Engine

## 前端架构

Workflow 编辑器不应硬编码每种 Driver 的 Command 列表。Command Step 的编辑流程为：

```text
Effective Connection
        ↓
Command discovery
        ↓
Command selector
        ↓
Selected Command Definition
        ↓
input_schema
        ↓
Schema-driven form
        ↓
WorkflowCommandStep
```

Command 的输入使用 JSON，Driver 通过 manifest / definition 声明 schema。这样新增 Driver-specific Command 时，不需要修改 Workflow UI 的 Driver 类型分支。

## 入口

- IPC：`commands/ai/mod.rs` → `workflow_*` / `workflow_history_*`；以及 Driver Command IPC
- GUI：`WorkflowPanel` / `WorkflowPage`
- MCP：`list_workflows` / `run_workflow`、`datazen://workflows`（实现位于 `mcp/server.rs`）
- Driver API：`packages/driver-api/src/command.rs`、`traits.rs`

**用户手册（YAML 语法与用法）：** [Workflow 使用手册](../../features/workflow-guide.zh-CN.md)


### 2026-10-17 Workflow v2 派发安全边界

共享 `WorkflowExecutor` 在任何步骤派发前执行版本分派和完整 v2 目标预检。v1 保留调用方连接兼容；v2 只接受 step / block / workflow 的完整声明，不从 GUI/MCP 调用参数补连接。显式目标层缺命令/驱动元数据要求的字段、块内跨连接或跨库目标均拒绝，条件和循环内的目标也先检查。database 要求保留命令 target requirements 的 Required / Optional / Forbidden 三态；Forbidden 在派发前拒绝已选目标中的 database，且不被驱动 has_multi_database 的通用 SQL 要求覆盖。Optional 才可按该驱动元数据提升为 Required，单库驱动不因 SQL 命令名被强制要求 database；缺元数据明确 CapabilityUnsupported，不猜测其他驱动能力。目标变量仅取本次运行输入与默认值，不能依赖尚未执行步骤或循环变量来改变资源身份。

桌面用户发起的 `workflow_execute` 对含 block 的 v2 定义经 `workflow::job` 执行：先冻结定义和输入、完成整个定义的目标/冲突/命令元数据预检，再由 `DesktopJobHost` 持久受理 `workflowRun` 并经 JobRuntime claim 派发。普通步骤与 sessionBlock 限定 PostgreSQL/MySQL 的 query/execute/explain 与条件/循环；transactionBlock 另按下述 driver proof 合同限制，当前仅 PostgreSQL 窄子集 opt-in。block 的 schema override 明确拒绝。v1 与无 block 的 v2 独立步骤保持原路径。MCP、旧 Workflow 定时入口及其他默认 host 仍拒绝 blocks，不能借桌面用户身份获得后台授权。

JobRuntime 在记录活动 stage 后生成不可序列化、无公开构造器的 `StageOwnerPermit`。独立步骤资源使用真实 `OwnerRef::Job`，session block 使用真实 `OwnerRef::WorkflowBlock { jobId, blockId }`；所有 owner/身份均由后端生成，普通 session 请求即使携带有效 Job ID 仍拒绝。Runtime 在资源获取、enqueue 与物理 dispatch 前核验活动 claim/阶段/取消与固定 profile revision。session block 步骤串行共用同一显式目标和物理 session，块末 RollbackAndClose；下一块另取资源。获取期间的 guard 覆盖回执丢失/超时，块截止时间触发独立于 GUI attachment/idle policy 的清理，取消信号传入运行中 SQL 的终态等待。关闭或结果未确认保留 Unknown，不自动重放。

持久 Job plan 只含版本和冻结定义摘要，不落盘 SQL、变量、令牌、session handle 或资源；实时 Workflow 结果仍沿原 IPC 返回；`workflow_list_jobs` / `workflow_cancel_job` 以当前桌面身份查询和取消此类 Job。重启不会恢复 handler 或重放 Workflow。失败结果保守记录 Unknown；Job handler v2 已提供有界 runtime phase/ID/state/effect 和已确认事务边界投影，尚无专用 Workflow Job 控制 UI 或跨重启 Workflow receipt；真实驱动临时对象/外部效果旅程仍须独立验收。

`transactionBlock` 当前仅由 PostgreSQL driver 的 `pgLockedPlainHeapLiteralDmlV1` 窄合同显式 opt-in。整块最终 SQL 在 Job admission 前完成严格方言 AST 与固定 namespace/profile/driver proof revision 校验；同一 Job-owned retained transaction 执行 `BEGIN`，先锁定关系并核验 catalog，再执行冻结的 literal DML/受限 SELECT，最后 `COMMIT`；失败走 `ROLLBACK`。只接受无索引、约束、trigger/rule/RLS、继承/分区、default/generated/identity 的普通持久 heap 和已审阅内置标量列；限定输入 relation、函数/过程、join/子查询/CTE、DDL、多语句及非 Abort 步骤策略均拒绝。非零整块 retry 仍拒绝；提交或回滚无法确认时记录 Unknown，不自动重放。MySQL/MariaDB 当前返回 `CapabilityUnsupported`，不能进入该事务执行路径。**PG+MySQL 双驱动 transactionBlock 的 P6 门槛仍未关闭**；完整证明条件与 MySQL 缺口见 [Workflow 当前证明边界](../platform/workflow-resource-model.md#2026-10-18-transactionblock-postgresql-窄证明与效果证据)。


#### 桌面 Monitor 定时授权（已实现，2026-10-17）

Monitor 周期轮询与手动刷新现已使用不同入口。手动刷新沿用当前桌面用户上下文；周期 tick 必须取得后端保存的 widget grant，不能因 `connection_id=None`、工作流可运行或桌面用户仍在线而获得授权。用户在报表编辑器中保存 interval 配置后，显式授权一小时或撤销；授权 IPC 仅接受 dashboard/widget ID 与 60～86400 秒有效期，不接受 principal、delegation token 或用户提供的 grant。

后端从当前桌面 RequestContext 取得批准者，并生成独立 service principal / delegation ID。AppDb schema v3 的 `monitor_schedule_grants` 仅保存批准者、服务身份、dashboard/widget/workflow ID、工作流 YAML SHA-256 revision、固定 canonical connection/database/schema 与配置版本、connect/query 动作及命令范围、readOnly、到期时间、撤销状态与版本。不保存 SQL 副本、session、attachment、bearer token、秘密或凭据。新授权取代同 widget 的旧授权；变更 workflow 定义或 profile 配置必须重新授权。

当前授权范围限 PostgreSQL/MySQL 的静态独立 `query` 步骤，要求保存的 workflow 明确声明 connection/database；不接受变量、循环、条件、AI、migration 或资源块。Monitor 使用独立的只读查询 AST 校验（单查询，拒绝 writable CTE、SELECT INTO、行锁及非查询；函数仅允许明确的内置聚合/数值/文本/空值/时钟函数白名单，未知、带 schema、引号名称及表值函数失败关闭，因此序列推进/状态、命名锁获取/状态/释放和会话配置函数不能借 SELECT 派发），不使用会禁止所有 SQL 工具的 MCP ReadOnly 模式。其他驱动和写动作均失败关闭。

共享 MonitorEngine 在入队前及 semaphore / workflow lock 之后重载 dashboard/widget enabled、pause 与 grant，冻结并比较 workflow revision 和目标范围。桌面 scheduled adapter 使用同一冻结定义与 service RequestContext；Runtime 从 PolicyService 后端解析完整 grant、核对主体/客户端/有效期/动作，在 admission 与物理 dispatch 前重新核对 grant、schedule、workflow hash、目标、配置版本、enabled 和只读 SQL。delegation 不能降级为本地用户；撤销后仍允许 owner 关闭其自有资源，不能因撤权遗留短会话。原同 widget in-flight 合并保持。

撤销/禁用阻止后续派发，不伪造已在途执行的取消或回滚。每个 grant 版本、dashboard/widget、workflow revision 与 refresh slot 对应一个带稳定幂等键的 `monitorOccurrence` Job；Job 受理后才派发原 scheduled workflow，WidgetRun 与有界 JobDomainResult 记录实际结果。启动恢复按持久化 service principal 扫描 occurrence：未派发 Job 标为未启动，运行中 Job 转为待核验，恢复流程不重放查询。撤权后无法继续读取 receipt 时保留 SqlUnconfirmed/Unknown；真实 PG/MySQL 驱动旅程仍须独立验收，P6 未整体关闭。

PostgreSQL 的独立实库验收入口为
`cargo test -p datazen-driver-postgres --test workflow_transaction_evidence`，
要求进程提供指向一次性测试库的 `MIGRATION_TEST_*` 配置，缺少配置默认失败。
该测试在 retained transaction 已执行 DML 后验证独立连接 DDL 得到 `55P03`，
提交后验证相同 DDL 恢复，并验证失败回滚与自有 session 丢失后的 Unknown。
它不经过桌面 Workflow Job。生产组合旅程另位于
`packages/drivers/postgres/e2e/workflow-transaction-journey.ts`，使用真实
`workflow_save` / `workflow_execute` 与独立查询验证提交和失败整块回滚；
已有 webdriver 构建时可通过
`pnpm e2e:skip-build -- --spec packages/drivers/postgres/e2e/workflow-transaction-journey.ts`
单独运行。新增旅程尚待独立 tester 实跑，不构成验收通过。
桌面 Job 内 DML 后精确暂停、commit/rollback acknowledgement loss 与不重放的
真实故障旅程仍未覆盖；终止自有 backend 的 Unknown 证据不能替代 acknowledgement
loss。MySQL/MariaDB 仍为 `CapabilityUnsupported`，P6 整体门槛未关闭。
