# Workflow 模块

> [返回架构总览](../README.md)

用户自定义 AI / Command 工作流引擎。Workflow 引擎独立于 MCP 协议；GUI、Tauri IPC 与 MCP Server 都是调用方。

## 模块结构

Workflow 的执行语义与宿主绑定分开：共享执行器位于 `packages/feature-core/src/workflow/`，宿主目录只保留 YAML 仓储、桌面历史、scheduler runtime entry 和 Runtime 接线。

```text
packages/feature-core/src/workflow/
├── mod.rs               # 共享模块出口
├── model.rs             # WorkflowDefinition / WorkflowStep 等数据模型
├── executor.rs          # WorkflowExecutor：步骤编排、错误策略、超时
├── executor_tests.rs    # executor.rs 的纯规则用例（database_is_required 等）
├── context.rs           # 模板解析、路径解析、循环上下文
├── conditions.rs        # Condition 表达式求值
├── data_step.rs         # Merge / Transform 等数据处理步骤
├── command.rs           # Workflow Command Step 数据结构
├── command_runtime.rs   # 共享 Command 执行规则：显式目标校验、connection 解析、命令派发
├── resource_execution.rs# 独立 Runtime 资源执行与授权目标
├── plan_guard.rs        # 计划与目标守卫
├── scheduler.rs         # 到期判定与 in-flight 生命周期
├── registry.rs          # 保存的 workflow 规则；宿主以 Arc<dyn WorkflowDefinitionRepository> 注入存储
└── host.rs              # 宿主端口

src-tauri/src/workflow/
├── mod.rs               # 模块出口
├── registry.rs          # YAML workflow 注册、加载、保存、删除（AppDb 绑定）
├── command_runtime.rs   # 桌面 AppState 绑定的 Command 桥（规则在 feature-core）
├── scheduler.rs         # 桌面 scheduler 适配
├── history.rs           # 执行历史持久化
├── host.rs              # 共享端口的桌面实现（WorkflowHost / ResourceExecutionHost）
├── doc_reference_tests.rs # 长期文档的行号引用新鲜度守卫
└── live_acceptance.rs   # 真实 PostgreSQL/MySQL 跨库验收（ignored）
```

宿主不再保留任何 workflow 的 re-export 门面文件：共享类型的调用点直接引用 `datazen_feature_core::workflow::…`。

宿主侧的 Command 桥只做「同一条链的桌面绑定」：显式 `database`/`schema` 校验、connection 继承解析、命令派发这三条规则都在 `packages/feature-core/src/workflow/command_runtime.rs`，宿主那三个函数只把 `AppState` 传进去。

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

### 连接与会话

Workflow IPC 和 YAML 使用持久化 `connectionId`，不会保存或接收 GUI 的 `dbSessionId`。Workflow v1 的每条 step 经 `feature-core command_runtime → feature-core resource_execution` 进入共享 Runtime；Runtime 按该 step 的 connection profile 建立或借用自己的 session/resource，并在该次执行完成后按 Runtime 生命周期清理。它不复用编辑器会话，也不通过 Host `ConnectionManager` 管理连接。

当前实现保留以下 v1 行为：

- step 未指定 connection 时继承 Workflow 默认 connection；显式指定时覆盖默认值。
- 跨库 step 按各自 connection profile 执行；上一步结果仍可作为下一步输入。
- 连接名展示不建立数据库会话。会话过期后，旧 `dbSessionId` 失效，调用方必须显式创建新会话。

空闲期限只由 Runtime 的实际业务执行活动刷新；状态读取、订阅、attach 等管理操作不延长期限。

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

## 迁移 step（已移除）

Workflow 曾支持一种 `migration` step：在 workflow 里以 step 的形式调用数据迁移三件套
（Schema Diff / Data Sync / Data Transfer）——按持久化的迁移 profile 开会话、生成计划、
执行并写入迁移运行历史。

**该能力已从 workflow 中移除，且不保留任何兼容分支**（包括只为报错而存在的残留结构）。
理由与事实：

- 迁移 step 走的是**宿主专属**的服务：宿主存储里的迁移 profile、宿主开的会话、宿主的
  迁移执行路径。web 形态要支持它，就得把这一整套也做成形态中立的。
- 与其留一个「只有桌面能用」的半截能力，不如先删掉；将来按 **web 与桌面同时可用**的
  形态重做。
- 该能力**从未真正发布**（其代码虽随被撤回的 v0.2.3 一起构建过，但该版本已撤回），
  内置 workflow 模板也从未使用它；因此本次按**直接移除**处理，不保留兼容分支或
  「只为报错而存在」的残留结构。

**三件套本身不受影响**：Schema Diff / Data Sync / Data Transfer 作为独立功能（命令、
存储、端口、UI）全部保留，只是不再能以 workflow step 的形式调用。

已删除的实现：`packages/feature-core/src/workflow/migration.rs`、
`packages/feature-core/src/workflow/migration_outcome.rs`、
`src-tauri/src/workflow/migration.rs`，以及 `WorkflowStep::Migration` 变体、
`WorkflowMigrationOperation` / `UnattendedDestructivePolicy` 枚举、
`WorkflowHost::execute_profile_step` 端口，与前端对应的 step 类型、表单与文案。

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

- IPC：`src-tauri/src/commands/workflow.rs` 的 `workflow_*` / `workflow_history_*`；以及 Driver Command IPC
- GUI：`WorkflowPanel` / `WorkflowPage`
- MCP：`list_workflows` / `run_workflow`、`datazen://workflows`（工具清单在 `mcp/server.rs`，执行在 `mcp/tools.rs`）
- Driver API：`packages/driver-api/src/command.rs`、`traits.rs`

**用户手册（YAML 语法与用法）：** [Workflow 使用手册](../../features/workflow-guide.zh-CN.md)


### P6 Workflow v1 范围

Workflow 保持既有 v1 协议、模型、跨数据库逐步执行及前步 output 供后步模板串联的语义；仅将底层连接管理走 `feature-core command_runtime → feature-core resource_execution → Runtime`。本轮不接入 v2 session/transaction block、专用 Workflow Job owner/dispatch、Workflow Job IPC 或 PostgreSQL/MySQL transaction proof。YAML、version model 与既有 IPC 形状不变。scheduler 保留桌面 runtime-entry 修复，Monitor 固定只读授权/独立 occurrence 和其他消费者的 Runtime 接线继续保留。

#### Workflow v1 真实驱动验收

Host 通用旅程 `src-tauri/src/workflow/live_acceptance.rs` 使用真实 PostgreSQL/MySQL inventory factory、桌面 AppState/WorkflowHost 和 Runtime，不以 sqlx 手工串接模拟执行器。两个方向分别执行继承 workflow connection 的 legacy Query，再以 step connection override 执行 Command；前步数值经 `{{steps.seed.rows[0].value}}` 进入后步 SQL，验证最终计算结果。夹具在两个独立测试库建立同名表并写入不同数值，后步同时读取目标库的数值并加上前步结果，以证明目标覆盖实际到达另一个物理数据库；结束时删除夹具表。只使用两库共有的建表/插入/SELECT/删除表语句，不增加驱动方言能力或事务语义。

运行：`scripts/run-live-contract.sh --crate datazen --lib-test workflow::live_acceptance`。测试默认 ignored，安全 runner 显式执行，并要求两族独立 `dz_fixture_*` 数据库；缺配置、不可连接或执行失败都会失败，不以跳过计作通过。原 `src-tauri/tests/workflow_tests.rs` 的 TC13 直接调用 sqlx，仅验证手工数据流，不作为 Workflow/Runtime 接线证据。

#### 桌面 Monitor 定时授权（已实现，2026-10-17）

Monitor 周期轮询与手动刷新现已使用不同入口。手动刷新沿用当前桌面用户上下文；周期 tick 必须取得后端保存的 widget grant，不能因 `connection_id=None`、工作流可运行或桌面用户仍在线而获得授权。用户在报表编辑器中保存 interval 配置后，显式授权一小时或撤销；授权 IPC 仅接受 dashboard/widget ID 与 60～86400 秒有效期，不接受 principal、delegation token 或用户提供的 grant。

后端从当前桌面 RequestContext 取得批准者，并生成独立 service principal / delegation ID。AppDb schema v3 的 `monitor_schedule_grants` 仅保存批准者、服务身份、dashboard/widget/workflow ID、工作流 YAML SHA-256 revision、固定 canonical connection/database/schema 与配置版本、connect/query 动作及命令范围、readOnly、到期时间、撤销状态与版本。不保存 SQL 副本、session、attachment、bearer token、秘密或凭据。新授权取代同 widget 的旧授权；变更 workflow 定义或 profile 配置必须重新授权。

当前授权范围限 PostgreSQL/MySQL 的静态独立 `query` 步骤，要求保存的 workflow 明确声明 connection/database；不接受变量、循环、条件、AI 或资源块。Monitor 使用独立的只读查询 AST 校验（单查询，拒绝 writable CTE、SELECT INTO、行锁及非查询；函数仅允许明确的内置聚合/数值/文本/空值/时钟函数白名单，未知、带 schema、引号名称及表值函数失败关闭，因此序列推进/状态、命名锁获取/状态/释放和会话配置函数不能借 SELECT 派发），不使用会禁止所有 SQL 工具的 MCP ReadOnly 模式。其他驱动和写动作均失败关闭。

共享 MonitorEngine 在入队前及 semaphore / workflow lock 之后重载 dashboard/widget enabled、pause 与 grant，冻结并比较 workflow revision 和目标范围。桌面 scheduled adapter 使用同一冻结定义与 service RequestContext；Runtime 从 PolicyService 后端解析完整 grant、核对主体/客户端/有效期/动作，在 admission 与物理 dispatch 前重新核对 grant、schedule、workflow hash、目标、配置版本、enabled 和只读 SQL。delegation 不能降级为本地用户；撤销后仍允许 owner 关闭其自有资源，不能因撤权遗留短会话。原同 widget in-flight 合并保持。

撤销/禁用阻止后续派发，不伪造已在途执行的取消或回滚。每个 grant 版本、dashboard/widget、workflow revision 与 refresh slot 对应一个带稳定幂等键的 `monitorOccurrence` Job；Job 受理后才派发原 scheduled workflow，WidgetRun 与有界 JobDomainResult 记录实际结果。启动恢复按持久化 service principal 扫描 occurrence：未派发 Job 标为未启动，运行中 Job 转为待核验，恢复流程不重放查询。撤权后无法继续读取 receipt 时保留 SqlUnconfirmed/Unknown；真实 PG/MySQL 驱动旅程仍须独立验收，P6 未整体关闭。
