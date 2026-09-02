# DataZen 架构设计（十六）：从桌面应用走向共享 Core 与 Web 平台

> 本篇讨论的是演进方向，而不是当前已发布的 Web 版本。问题是：如果未来需要服务端或浏览器工作台，哪些 DataZen 能力可以共享，哪些桌面假设必须先拆掉？

## 先区分当前实现与规划

今天的 DataZen 是 Tauri 桌面应用：Rust 进程持有数据库连接、系统钥匙串和本地 Store，React 在 WebView 中通过 IPC 调用它们。未来的平台化不应被描述成“把 Tauri 页面直接放到浏览器”，那会误解安全和资源边界。

更现实的目标是共享领域 Core，再为 Desktop 和 Web 提供不同 Adapter：

```text
Shared Core
  ├─ Driver API / Command Runtime
  ├─ Workflow / Schema / AI context
  └─ permission / audit / error model
       ├─ Desktop Adapter: Tauri IPC + local Store
       └─ Web Adapter: HTTP/WebSocket + server Store
```

## 可以共享什么

Driver Command Definition、输入校验、Workflow 模型、Schema Diff 计划、Data Sync ChangeSet、AI PromptResolver 和权限模型都属于领域能力，适合抽取为不依赖 UI 或具体传输协议的 Core。

错误分类、审计事件、取消语义和执行历史也应尽量保持一致，这样桌面与 Web 的行为才可比较。

## 必须重新设计什么

桌面端的 `dbSessionId` 是本地运行时句柄，Web 端不能把它当作可公开的长期 token。服务端需要会话认证、租户隔离、连接池配额、审计和超时；浏览器永远不能直接拥有数据库凭据。

本地 AES Store、系统 Keychain、`datazen://` 资源协议和 Native 文件对话框也不能原样搬到服务器。它们需要 Web 适配器或完全不同的安全实现。

## IPC 与 HTTP 的边界

桌面端 `invoke` 是同机进程间调用，延迟低且信任边界较窄；Web 端 HTTP/WebSocket 面向不可信网络，必须增加认证、CSRF/CORS、速率限制和请求大小控制。

流式 `query_stream` 可以映射为 WebSocket 或 SSE，但事件协议应保持相同的语义：开始、列、行批次、语句结束、完成/失败。取消要有服务端可验证的执行 ID，不能只依赖浏览器关闭连接。

## 逐步迁移路线

1. 先把 Command Definition、错误模型和 Workflow Executor 的纯领域逻辑与 Tauri 分离；
2. 为 Store、连接管理和事件传输定义 trait；
3. 保持 Desktop Adapter 行为不变，增加内存/服务端测试实现；
4. 用受控 HTTP API 暴露只读 Schema 和查询能力；
5. 再逐步加入写入、Workflow 和 AI 工具，并为每项增加认证和审计；
6. 最后评估哪些 UI 可以共享，哪些仍应保持桌面专属。

这条路径避免一次性重写，也不会为了“未来 Web”提前削弱桌面应用的安全边界。

## 当前架构已经准备好的部分

Command Runtime 已经把入口和数据库能力分开；Driver API 已经隔离方言；Workflow、AI 和 MCP 已经通过服务复用核心；`CommandError`、权限级别和流式事件也提供了可迁移的协议形状。

仍需补齐的是持久化抽象、服务端认证、租户模型、远程连接生命周期和跨实例任务调度。这些属于未来 RFC，不应在当前文档中假设已经存在。

## 结语

平台化不是把桌面窗口搬上云，而是保留领域核心、重新定义资源与信任边界。DataZen 当前的分层架构已经为这种演进留下空间：Driver、Command、Workflow、AI 和 MCP 可以共享，Tauri IPC、Keychain、本地 Store 和窗口管理则由 Desktop Adapter 独自承担。

至此，DataZen 架构系列从全景、边界、会话、驱动、工作区、智能能力一直走到演进方向。真正重要的不是记住每个模块名，而是理解每个能力应该由谁拥有、通过什么协议暴露，以及在哪个边界上被验证。

相关资料：[架构总览](../architecture/README.md) · [窗口管理](../architecture/windows.md) · [Tauri 前后端边界](02-tauri-frontend-backend-boundary.zh-CN.md)
