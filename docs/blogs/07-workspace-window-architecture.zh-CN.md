# DataZen 架构设计（七）：从多窗口到统一工作区

> 桌面数据库工具很容易把每个功能都做成独立窗口，最后用户得到的是一组互相失去上下文的窗口。DataZen 把连接、设置、Workflow 和 Dashboard 收进主工作区，只为少数长流程保留原生子窗口。

## 主窗口承载上下文

主窗口的路由壳是 `MainPage`：没有连接时显示 `WelcomePage`，有连接时显示 `ConnectionPage`。ConnectionPage 统一承载连接树、SQL 面板、Schema 浏览、Workflow 和 Dashboard 导航。

Settings 也以内嵌 `SettingsPage` 呈现。Docs 则直接在系统浏览器打开官网手册，不再创建一个“文档子窗口”。

这样做的核心理由是上下文连续：当前连接、数据库、Tab、查询历史和主题状态都留在同一个工作区中，不需要在多个 OS 窗口之间复制同步。

## 哪些功能仍保留子窗口

Backup、Data Sync、Schema Diff 和 Data Transfer 是单一目的、步骤较长或需要独立工作区的功能。它们使用单例原生子窗口：

- `backup-singleton`：备份与恢复；
- `data-sync-singleton`：同族数据同步 Diff；
- `schema-diff`：结构比较与 DDL Deploy；
- `data-transfer`：异构数据传输向导。

这些窗口拥有清晰的进入和退出边界，不会把主连接页面拆成多个互相竞争的导航树。

## WindowKind 与兼容路由

`src/lib/windowKind.ts` 根据 URL 参数 `window` 解析窗口类型。旧的 `connection`、`workflow`、`dashboard`、`settings`、`docs` 和 `new-connection` 别名会落到 `main`，保证历史深链仍能打开统一壳。

Rust 的 `create_sub_window` 负责真正创建原生窗口，设置尺寸、透明度、装饰和 macOS 的 `accept_first_mouse`。窗口创建策略与 React 页面路由分离，避免组件直接操作平台窗口 API。

## windowManager 的降级策略

`src/lib/windowManager.ts` 提供统一入口。Tauri 环境调用 Rust 的 `create_sub_window`；浏览器开发模式则降级到 `window.open()`。上层组件只表达“打开哪个功能”，不判断当前是否桌面环境。

主工作区导航通过事件（如 `menu:workflow`、`menu:dashboard`）和状态 Store 协作；打开连接则通过 `localStorage` 与 `datazen:open-connection` 投递 payload。这些事件必须可重复处理，因为窗口切换和 WebView 恢复可能导致接收时机变化。

## 故障隔离

全局 `ErrorBoundary` 防止一个未处理 React 错误让整个窗口白屏。独立窗口还可以在自己的边界内显示 Reload/Dismiss，而不会影响主工作区的连接和查询。

隔离的另一面是资源归属：子窗口若打开专用数据库会话，应在关闭时释放；主窗口的共享会话则由 active connection Store 和 ConnectionManager 共同管理。窗口模型不能替代会话生命周期模型。

## 取舍

统一工作区减少了窗口数量和跨窗口同步，却要求主窗口更重视 Tab、导航历史和布局恢复。原生子窗口提供了更强的专注感和独立尺寸，但需要处理单例、关闭清理以及平台差异。

因此 DataZen 的规则不是“永远单窗口”，而是：有共享上下文的能力留在主工作区；有独立生命周期和长流程的能力才创建子窗口。

## 结语

窗口架构最终服务的是状态边界。主工作区负责连续的数据库工作流，少量子窗口负责隔离的专用任务，WindowKind 和 windowManager 则把平台差异集中起来。下一篇将深入主工作区内部：当连接、面板、Schema 和 AI 状态不断变化时，Zustand 如何避免一个全局 Store 失控。

相关资料：[窗口管理](../architecture/windows.md) · [Extension 页面壳](../architecture/backend/extensions.md)
