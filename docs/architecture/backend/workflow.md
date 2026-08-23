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
- Query / Command / Ai / Condition / ForEach
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

- IPC：`commands/ai.rs` → `workflow_*` / `workflow_history_*`；以及 Driver Command IPC
- GUI：`WorkflowPanel` / `WorkflowPage`
- MCP：`list_workflows` / `run_workflow`、`datazen://workflows`（实现位于 `mcp/server.rs`）
- Driver API：`packages/driver-api/src/command.rs`、`traits.rs`

**用户手册（YAML 语法与用法）：** [Workflow 使用手册](../../features/workflow-guide.zh-CN.md)
