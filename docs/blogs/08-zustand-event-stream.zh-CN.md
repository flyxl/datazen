# DataZen 架构设计（八）：Zustand 状态管理与事件流

> 复杂桌面工作区的难点不是“有没有状态管理库”，而是哪些状态应该持久化、哪些只在会话中存在，以及一次查询事件如何穿过 Store 而不把业务流程塞回组件。

## 按领域拆 Store

DataZen 按领域拆分 Zustand Store：连接、面板、Schema、设置、AI、Dashboard、Workflow 和插件各自拥有明确的状态与动作。组件订阅自己需要的切片，避免一个输入框变化导致整个工作区重渲染。

拆分的标准不是目录结构，而是生命周期：连接 Store 关心配置和会话映射，Panel Store 关心 Tab 与编辑器，Schema Store 关心缓存结果，Settings Store 关心可持久化偏好。

## 三类状态

**持久状态**通过 IPC 读写 Rust Store，例如连接配置、语言、主题、Workflow 和历史。**会话状态**只在当前进程有效，例如 `dbSessionId`、活动 `executionId` 和流式行批次。**纯 UI 状态**包括折叠、焦点、选中行和弹窗，不应污染后端模型。

如果把三者混在一个对象里，重启恢复会尝试序列化运行时句柄，组件卸载又可能误删持久配置。

## 事件驱动的查询状态

查询不是一个简单的 `loading: boolean`。一次流式执行至少包含：准备、开始、列信息、行批次、语句结束、全部完成、失败和取消。

前端 `queryCommands.executeQueryStream` 注册 `Channel<QueryStreamEvent>`，Store 根据事件更新状态：

```text
idle → starting → streaming → completed
                 ├──────────→ failed
                 └──────────→ cancelled
```

`executionStarted` 产生 `executionId` 后，取消按钮才能调用 `cancel_query`。行批次按 statement index 追加，DataTable 只订阅当前面板的结果。

## 跨 Store 协作

连接成功后，Connection Store 写入 `dbSessionId` 并广播 `datazen:connection-ready`；Panel Store 收到后绑定面板上下文；Schema Store 以连接和数据库为 key 读取或失效缓存。每个 Store 只发布领域事件，不直接修改另一个 Store 的内部对象。

这种协作方式比在组件中串联十几个 `setState` 更容易测试，也能在窗口恢复时重复消费事件。

## 持久化与节流

自动保存设置和布局时，需要区分“用户正在输入”和“应该写盘”。Store 可以用 debounce 合并连续变更，但最终仍通过明确的 `save_settings` 或相关 IPC 持久化。查询结果、编辑器草稿和运行时事件不应无条件写盘，否则会放大 I/O。

## 事件流中的竞态

同一个面板可能先后发起两次查询。Store 必须用面板上下文或执行 ID 判断事件归属，旧查询迟到的 `done` 不能关闭新查询的 loading。窗口关闭时也要取消或忽略仍在飞行中的事件。

前端不能只相信事件顺序，还要在收到终止事件后清理订阅和执行 ID；后端则负责保证每个流都有终止路径。

## 结语

Zustand 的价值在于把状态按生命周期和领域分开，把 IPC 事件转成可观察的状态变化。组件负责渲染和触发意图，Store 负责编排状态，Rust 负责资源与执行。下一篇继续关注结果侧：当查询返回几十万行时，DataTable 如何在不阻塞 WebView 的情况下保持可用。

相关资料：[查询命令](../../src/commands/query.ts) · [前端状态目录](../../src/stores/) · [查询结果类型](../../src/types/index.ts)
