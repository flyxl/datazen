# DataZen 架构设计（十）：YAML Workflow 执行引擎

> 当一次数据库操作需要查询、判断、循环和 AI 协作时，按钮已经不够用了。DataZen 用 YAML 描述步骤，用统一 Command Runtime 执行步骤，让 GUI、Tauri IPC 和 MCP 共享同一个 Workflow 核心。

## Workflow 不是宏录制

Workflow Definition 是可保存、可检查和可重复运行的模型。它描述默认连接、步骤、输入、变量和错误策略，不保存运行时连接池或窗口状态。

```yaml
name: daily-check
connection: mysql-prod
steps:
  - type: command
    command: query
    input:
      sql: SELECT count(*) AS total FROM orders
  - type: condition
    expression: "steps[0].rows[0].total > 0"
    then:
      - type: ai
        prompt: "分析今天订单数量"
```

旧版 `type: query` 在加载时规范化为 `Command("query")`，执行器不再维护两套语义。

## 模块分层

`src-tauri/src/workflow/` 将注册、上下文、条件、执行和历史拆开：

- `registry.rs`：加载、保存、删除 YAML；
- `model.rs` / `command.rs`：Definition 与 Step 类型；
- `context.rs`：模板变量、路径和循环上下文；
- `conditions.rs`：Condition 表达式；
- `executor.rs`：步骤编排、超时和错误策略；
- `command_runtime.rs`：连接解析、Command discovery 和 Driver 执行；
- `history.rs`：执行历史持久化。

`workflows.rs` 只是兼容 facade，新代码应依赖拆分后的模块。

## 连接继承

Workflow 可以在顶层声明默认 `connection`，Step 未指定时继承；Step 显式指定时覆盖。解析发生在执行时，因此 YAML 只保存稳定的 `connectionId`，不保存 `dbSessionId`。

跨数据库 Workflow 仍然可以工作，但每个 Command 是否存在、输入 Schema 是否兼容，必须由目标 Driver 定义决定。执行器不会假设所有连接都支持相同命令。

## Command Step 的执行链

```text
Step
  → resolve effective connection
  → resolve command definition
  → resolve template variables
  → validate input schema
  → check risk / permission
  → Driver::execute_command()
```

模板解析发生在输入校验之前，避免把字符串模板当作已经验证过的值。命令执行结果进入运行上下文，后续 Step 可以通过路径引用，但大型结果不应无限制地复制到每个步骤。

## Condition、ForEach 与 AI

Condition 只决定分支，不直接执行数据库副作用；ForEach 为每个元素建立子上下文，变量作用域在循环结束后可控；AI Step 通过 AiProvider 生成文本或结构化结果，若要访问数据库则仍然走 DbTools 或 Command Runtime。

这三类 Step 是编排能力，不是第三套数据库 API。它们可以组合 Command，却不能绕过连接、权限、取消和历史机制。

## 超时、失败与历史

执行器需要为单步和整个 Workflow 设置超时，并在失败时保留步骤索引、错误和已完成输出。默认策略应让失败可见，而不是继续执行潜在依赖失败结果的步骤；需要继续时由 Definition 明确声明。

历史记录保存执行时间、状态和摘要，不应把数据库密码或完整敏感输入写入日志。取消信号要沿着 Executor → Command Runtime → Driver 传播。

## GUI、IPC 与 MCP 共用 Runtime

GUI 点击运行、Tauri `run_workflow` 命令和 MCP Server 的 `run_workflow` Tool 都调用同一个 Executor。协议入口只负责认证、参数转换和权限模式，不能复制 YAML 解析和步骤执行。

这让 Workflow 在不同入口下拥有一致的命令发现、连接继承、错误和审计行为，也方便未来增加后台调度器。

## 结语

Workflow 的核心价值是把“多步意图”建模为数据，把执行约束留在 Runtime。YAML 负责描述，Executor 负责编排，Command Runtime 负责验证和调用 Driver。下一篇将把同样的 Provider 思路应用到 AI：如何在不绑定单一模型厂商的前提下支持 NL2SQL 和流式对话。

相关资料：[Workflow 架构](../architecture/backend/workflow.md) · [Workflow 用户指南](../features/workflow-guide.zh-CN.md)
