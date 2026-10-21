# DataZen 架构设计（三）：连接配置与数据库会话的生命周期

> `connectionId` 和 `dbSessionId` 看起来都像“连接 ID”，但它们属于完全不同的生命周期。本文沿着一次连接从保存、建立、复用、空闲驱逐到断开的过程，解释为什么 DataZen 必须把持久化身份与运行时资源分开。

## 两种连接状态

DataZen 保存的是 `ConnectionConfig`，运行时持有的是 Runtime 与 Driver 共同建立的物理资源。前者可以在应用重启后继续存在，后者只在进程中有效。

| 标识 | 含义 | 存储 | 主要使用者 |
| --- | --- | --- | --- |
| `connectionId` | 持久化连接配置 ID | 加密 Store | 设置、历史、Workflow、MCP |
| `dbSessionId` | Runtime 数据库会话 ID | Runtime 内存态 | SQL、Schema、取消查询 |

GUI 的标准路径是 `connect(connectionId)`，后端返回 `dbSessionId`；之后所有面向已建立会话的查询和 Schema IPC 都使用后者。MCP 和 AI 数据库工具通常只接收 `connectionId`，由后端经 Runtime execution port 按需创建或借用资源。

## 为什么不能只保留一个 ID

如果把配置 ID 当作会话 ID，应用重启后就无法区分“配置存在”和“连接已经建立”。如果把会话 ID 持久化，连接池、TLS 状态和驱动句柄就会被错误地当成可恢复文件。

两种 ID 分开还解决了三个实际问题：

1. 同一份配置可以同时打开多个编辑器 Tab 或任务；
2. 修改连接名称、密码不会改变历史记录归属；
3. 运行时会话可以独立失效，而配置仍然稳定可引用。

## 建立连接的链路

桌面 `connect` 经 `PlatformAdapter` 进入 application 的 `ConnectionUseCases`，大致执行以下步骤：

1. Runtime 建立 `SessionRegistry` 会话记录并生成 `dbSessionId`；
2. `DesktopSessionBackend` 从 Store 读取配置并解密密码，按需建立 Tunnel；
3. 从 `DriverRegistry` 找到数据库 Driver；
4. 调用 Driver 的资源接口获取物理资源，并把资源绑定到该 session；
5. 记录 owner、预算、隧道和资源生命周期；
6. 将 `dbSessionId` 返回给前端。

前端的 `activeConnectionStore` 以 `connectionId` 为 key 保存配置与状态，并把返回值写入对应 entry。SQL 面板则同时保存两个字段：执行用 `dbSessionId`，历史和收藏过滤用 `connectionId`。

## 复用、引用与专用会话

编辑器按 pane 管理独立 session，避免一个 Tab 的 database selection、事务或取消影响另一个 Tab。Data Sync、Data Transfer、Schema Diff 等长任务使用各自专用 Runtime session；Workflow、MCP、AI database tools 以 `connectionId` 为入口，经共享 Runtime execution port 借用或创建资源。

Runtime 负责物理资源预算、lease 和归池。它不会因为多个调用方引用同一条配置就共享可变会话状态。

## 空闲驱逐导致显式失效

为了控制桌面应用长期运行的资源占用，后台清理任务会扫描长时间未使用的会话。默认业务空闲期限为 30 分钟，桌面 adapter 每 15 秒扫描一次；detach 后收紧到 60 秒 grace。

到期时 Runtime 会在原资源上 rollback-and-close，撤销活连接借用并清理会话登记。**失效的 `dbSessionId` 不会用同一个 ID 透明重建**：后续请求返回 `SessionLost`，调用方必须显式打开新 session。这是资源所有权与事务状态能够被证明干净的前提。

物理资源关闭后不会通过 `connectionId` 映射复活旧 `dbSessionId`。如果关闭结果不确定，Runtime 保留隔离与预算状态，而不是谎称资源已释放。

## 断开与异常清理

断开流程不仅是关闭 socket。它还需要停止活动查询、释放 Runtime 资源、清理 Tunnel、移除会话登记，并通知前端清除面板上的运行时状态。连接失败时则必须回滚已经创建的中间资源，避免留下孤儿隧道或半初始化会话。

前端不应该把“请求断开”当作“资源已经销毁”。只有收到 IPC 成功响应后，Store 才应清除 `dbSessionId`；失败时保留状态并提示用户重试或重新连接。

## Workflow 与 MCP 为什么使用 connectionId

Workflow 定义和 MCP 配置需要跨进程、跨重启保存引用，因此使用稳定的 `connectionId`。执行时由共享 Runtime execution port 解析目标并创建/借用独立 session；它不依赖某个 GUI 窗口是否打开，也不会把内存态 `dbSessionId` 写进 YAML 或 MCP 请求。

相应地，GUI 的 SQL IPC 必须使用 `dbSessionId`，不再依赖历史的双模回退。

## 生命周期测试

测试重点不是 ID 字符串本身，而是不变式：

- 配置编辑不改变 `connectionId`；
- `connect` 返回的 `dbSessionId` 能被查询使用；
- 空闲驱逐后旧 ID 被拒绝，必须显式建立新会话；
- 显式断开后旧会话 ID 被拒绝；
- 一个会话的 `executionId` 不能被另一个会话取消；
- 物理资源关闭前，会话级事务/游标句柄已注销。

## 结语

`connectionId` 是用户世界里的“这条配置”，`dbSessionId` 是进程世界里的“这次会话”。把两者分开，Store、Runtime、前端状态、Workflow 和 MCP 才能各自拥有清晰的生命周期。下一篇将继续向下拆解：当会话找到 Driver 后，一个可插拔数据库驱动究竟如何被选中和注册。

相关资料：[ID 术语规范](../architecture/naming.md) · [服务层](../architecture/backend/services.md) · [连接命令](../../src/commands/connection.ts)
