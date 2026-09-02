# DataZen 架构设计（三）：连接配置与数据库会话的生命周期

> `connectionId` 和 `dbSessionId` 看起来都像“连接 ID”，但它们属于完全不同的生命周期。本文沿着一次连接从保存、建立、复用、空闲驱逐到断开的过程，解释为什么 DataZen 必须把持久化身份与运行时资源分开。

## 两种连接状态

DataZen 保存的是 `ConnectionConfig`，运行时持有的是 Driver 创建的 `ConnectionHandle`。前者可以在应用重启后继续存在，后者只在进程中有效。

| 标识 | 含义 | 存储 | 主要使用者 |
| --- | --- | --- | --- |
| `connectionId` | 持久化连接配置 ID | 加密 Store | 设置、历史、Workflow、MCP |
| `dbSessionId` | 运行时数据库会话 ID | 内存会话池 | SQL、Schema、取消查询 |

GUI 的标准路径是 `connect(connectionId)`，后端返回 `dbSessionId`；之后所有面向已建立会话的查询和 Schema IPC 都使用后者。MCP 和 AI 数据库工具通常只接收 `connectionId`，由后端按需解析或建立会话。

## 为什么不能只保留一个 ID

如果把配置 ID 当作会话 ID，应用重启后就无法区分“配置存在”和“连接已经建立”。如果把会话 ID 持久化，连接池、TLS 状态和驱动句柄就会被错误地当成可恢复文件。

两种 ID 分开还解决了三个实际问题：

1. 同一份配置可以同时打开多个窗口或任务；
2. 修改连接名称、密码不会改变历史记录归属；
3. 运行时会话可以被驱逐后重新连接，而配置仍然稳定可引用。

## 建立连接的链路

`ConnectionManager::connect(connection_id)` 大致执行以下步骤：

1. 从 Store 读取配置；
2. 解密密码，按需建立 SSH Tunnel；
3. 从 `DriverRegistry` 找到数据库 Driver；
4. 调用 `driver.connect(&effective_config)` 获取 `ConnectionHandle`；
5. 把会话放入活动池，并记录 `dbSessionId → connectionId` 的归属映射；
6. 将 `dbSessionId` 返回给前端。

前端的 `activeConnectionStore` 以 `connectionId` 为 key 保存配置与状态，并把返回值写入对应 entry。SQL 面板则同时保存两个字段：执行用 `dbSessionId`，历史和收藏过滤用 `connectionId`。

## 复用、引用与专用会话

主工作区通常复用同一个会话，避免每次打开表都重新认证。子窗口或长任务可以通过 `connect_dedicated` 获取专用会话，以隔离事务和执行状态。

连接管理器内部需要处理并发连接请求。多个 Tab 同时点击连接时，`connect_locks` 等去重锁确保不会为同一配置创建重复的底层连接；引用计数归零后，`release` 可以拆除不再使用的会话。

## 空闲驱逐不是断开

为了控制桌面应用长期运行的资源占用，后台清理任务会扫描长时间未使用的会话并移出活动池。驱逐时保留归属映射，因此调用方持有的 `dbSessionId` 仍然可以被恢复。

下一次 `get_session(dbSessionId)` 未命中活动池时，管理器会：

1. 根据归属映射找到 `connectionId`；
2. 从 Store 读取最新配置；
3. 重新建立隧道和 Driver 连接；
4. 用原来的 `dbSessionId` 重新注册会话。

“驱逐不换 ID”是一个重要不变式。只有显式 `disconnect`、引用释放到零或应用关闭，才会移除归属映射并让会话真正失效。

## 断开与异常清理

断开流程不仅是关闭 socket。它还需要停止活动查询、释放 Driver 资源、清理 SSH Tunnel、移除会话映射，并通知前端清除面板上的运行时状态。连接失败时则必须回滚已经创建的中间资源，避免留下孤儿隧道或半初始化会话。

前端不应该把“请求断开”当作“资源已经销毁”。只有收到 IPC 成功响应后，Store 才应清除 `dbSessionId`；失败时保留状态并提示用户重试或重新连接。

## Workflow 与 MCP 为什么使用 connectionId

Workflow 定义和 MCP 配置需要跨进程、跨重启保存引用，因此使用稳定的 `connectionId`。执行时由 `resolve_connection` 通过 `ConnectionManager::get_or_connect_session` 解析到运行时会话。

这让外部 Agent 不必知道某个 GUI 窗口何时打开，也避免把内存态 `dbSessionId` 写进 YAML 或 MCP 请求。相应地，GUI 的 SQL IPC 则必须使用 `dbSessionId`，不再依赖历史的双模回退。

## 生命周期测试

测试重点不是 ID 字符串本身，而是不变式：

- 配置编辑不改变 `connectionId`；
- `connect` 返回的 `dbSessionId` 能被查询使用；
- 空闲驱逐后同一 ID 可以自动重连；
- 显式断开后旧会话 ID 被拒绝；
- 一个会话的 `executionId` 不能被另一个会话取消。

## 结语

`connectionId` 是用户世界里的“这条配置”，`dbSessionId` 是进程世界里的“这次会话”。把两者分开，Store、ConnectionManager、前端状态、Workflow 和 MCP 才能各自拥有清晰的生命周期。下一篇将继续向下拆解：当会话找到 Driver 后，一个可插拔数据库驱动究竟如何被选中和注册。

相关资料：[ID 术语规范](../architecture/naming.md) · [服务层](../architecture/backend/services.md) · [连接命令](../../src/commands/connection.ts)
