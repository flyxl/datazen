# DataZen 架构设计（五）：从 Driver Trait 到 Driver Command API

> 仅有一个巨大的 Driver Trait，最终会把所有数据库差异、管理操作和 UI 判断堆进宿主。DataZen 用 Command Definition 描述能力，用统一 Runtime 执行能力，让“支持什么”和“如何调用”都可以被发现。

## 大 Trait 的边界问题

传统数据库客户端常见的接口是 `query`、`list_tables`、`create_user`、`redis_get` 等一长串方法。它的优点是类型直接，缺点是每增加一种专属能力，Host、前端和 Workflow 都可能需要新增分支。

当 Redis 需要 KV 操作、PostgreSQL 需要角色管理、MongoDB 需要集合命令时，一个静态 Trait 很快变成“所有驱动都必须实现但大多数为空”的接口。

## Command Definition

DataZen 把能力拆成定义和执行两部分。Driver 暴露 `command_definitions()`，每个定义包含：

- 稳定的 command ID；
- 名称和描述；
- 输入 JSON Schema；
- category（query、mutate、admin、observe、stream 等）；
- risk / permissions；
- 是否允许在 Workflow、Extension 或 MCP 中使用；
- 是否需要连接、是否支持流式传输。

前端 Command Editor、Workflow 选择器和管理对话框都基于这些定义生成，而不是硬编码所有数据库命令。

## 统一执行模型

```text
调用方
  └─ execute_driver_command
       ├─ resolve session / driverType
       ├─ find command definition
       ├─ validate input schema
       ├─ check access level
       └─ driver.execute_command()
```

调用方只提交 `{ command, input, dbSessionId? }`。Runtime 负责会话解析、输入校验、风险门控和结果包装。Driver 负责把规范化输入翻译成自己的协议。

## query 与 execute 的默认能力

`query` 和 `execute` 是内置 Command，Driver API 提供默认实现。普通 SQL 驱动通常只需要实现底层查询接口，就可以获得统一的查询命令；特殊驱动可以覆盖默认实现，表达文档数据库或 KV 存储的原生语义。

`query_stream` 则有单独的流式入口，只允许通过 `execute_driver_command_stream` 执行，避免把 Channel 语义误用到普通命令。

## Schema-driven UI

当命令输入是 JSON Schema，前端可以生成文本框、数字框、枚举、必填校验和风险提示。新增 Driver Command 时，Host 不需要新增一个专用弹窗；只要定义完整，通用 Command Editor 就能展示它。

这并不意味着 UI 失去设计感。高频路径可以提供专用体验，低频或驱动专属能力则回退到通用表单。两者共享同一个执行协议，避免出现“专用 UI 一套语义、通用 UI 另一套语义”。

## Workflow、MCP 与 Extension 的复用

Workflow Command Step 在执行前解析连接、模板变量和输入 Schema，然后进入同一 Runtime。MCP Server 将工具参数转换为 Command 请求，Extension 的 `command.invoke` 也通过受控桥调用它。

复用 Runtime 带来的直接收益是：权限检查、查询历史、错误转换和驱动差异只实现一次。新的入口不需要重新发明“怎样连数据库”。

## Redis 为什么不需要 Host 特判

Redis 的 KV、发布订阅和管理操作显然不同于 SQL，但它们可以用 Command 表达：`redis_get`、`redis_set`、`scan_keys` 等定义自己的输入 Schema 和风险级别。Host 只看到可发现的命令，不写 `pluginId === 'redis'` 的设置分支。

## 兼容与演进

Command ID 一旦进入 Workflow、插件或 MCP 配置，就应保持稳定。输入 Schema 可以增加带默认值的可选字段；删除或改变语义时应提供迁移或新 ID。`ReuseDriver`、Workflow 和 MCP 都必须转发命令发现与执行，不能只转发旧的基础 Trait。

## 结语

Command API 把数据库能力变成可描述、可校验、可授权的协议。Trait 仍然承担底层连接和查询抽象，但面向产品的能力通过 Definition 暴露。这样 Driver 可以持续增加能力，宿主却不必持续增加数据库类型分支。

相关资料：[Driver Command API](../architecture/backend/drivers.md) · [Workflow 模块](../architecture/backend/workflow.md) · [Extension 桥接](../architecture/backend/extensions.md)
