# DataZen 系统架构文章系列

> 系列名称：《DataZen 架构设计：一个现代桌面数据库工具是如何构建的》

本系列不按照源码目录逐一介绍模块，而是沿着“全景认知 → 核心抽象 → 前端工作区 → 智能与自动化 → 高级数据库能力 → 工程保障”的路径，解释 DataZen 在真实开发过程中遇到的问题、采用的设计以及相应的取舍。

每篇文章可以独立阅读，按顺序阅读则可以逐步建立完整的 DataZen 架构地图。

## 配套架构图

- [DataZen 系统架构全景](diagrams/datazen-system-architecture.html)
- [DataZen Driver 运行时架构](diagrams/datazen-driver-runtime-architecture.html)
- [DataZen Driver 选择与编译流程](diagrams/datazen-driver-build-workflow.html)

三份图均为独立 HTML，可切换明暗主题、缩放、搜索、聚焦关系并导出 PNG、JPEG、WebP 或 SVG。可维护的 Archify JSON 源文件位于 [`diagrams/src/`](diagrams/src/)。

## 第一部分：建立全局认知

### 第 1 篇：DataZen 架构全景

**核心问题：DataZen 是什么，它由哪些系统组成？**

- 为什么选择 Tauri v2、Rust 与 React
- GUI 桌面应用与无头 MCP Server 双运行模式
- 前端、IPC、服务层、驱动层和数据库之间的关系
- AI、Workflow、插件系统在整体架构中的位置
- 一条 SQL 从界面到数据库再返回表格的完整链路

已完成：[DataZen 架构设计（一）：一个现代桌面数据库工具是如何构建的](01-datazen-architecture-overview.zh-CN.md)

### 第 2 篇：Tauri 桌面应用的前后端边界

**核心问题：哪些逻辑放 React，哪些逻辑放 Rust？**

- React 前端的职责：交互、状态、编辑器和结果展示
- Rust 后端的职责：连接、执行、安全和持久化
- Tauri IPC 的调用模型
- camelCase 与 snake_case 参数映射
- `CommandError` 结构化错误处理
- 查询结果流式传输与普通 IPC 的区别
- 为什么不让前端直接接触数据库

已完成：[Tauri 桌面应用的前后端边界](02-tauri-frontend-backend-boundary.zh-CN.md)

### 第 3 篇：连接配置与数据库会话的生命周期

**核心问题：为什么同时存在 `connectionId` 和 `dbSessionId`？**

- 持久化连接配置与运行时连接会话的区别
- `ConnectionManager` 如何管理连接池
- 连接建立、复用、断开和异常清理
- 前端状态与后端资源的对应关系
- Workflow、MCP 为什么只接受 `connectionId`
- 历史双模 ID 带来的复杂性
- 用生命周期建模避免资源泄漏

## 第二部分：核心扩展架构

### 第 4 篇：可插拔数据库驱动架构

**核心问题：一个数据库工具如何支持不断增加的数据库类型？**

- `DatabaseDriver` 抽象
- `driver-api` 与宿主之间的依赖边界
- Path Driver 与 Git Driver
- Cargo feature 编译时选型
- `inventory` 链接时自动注册
- `drivers-registry.json` 与代码生成
- `ReuseDriver` 的作用
- 新增数据库驱动需要实现哪些能力
- 为什么驱动测试必须留在驱动 crate 内

### 第 5 篇：从 Driver Trait 到 Driver Command API

**核心问题：如何避免宿主到处判断数据库类型？**

- 传统大型 Driver Trait 的局限
- `command_definitions()` 与 `execute_command()`
- Command Definition 的名称、描述和输入 Schema
- `query`、`execute` 的默认实现
- SQL、Redis KV 和管理操作如何统一为 Command
- 前端如何动态发现能力
- Schema-driven UI 如何减少硬编码
- Workflow、IPC、插件如何复用同一个 Command Runtime
- Redis 深度能力为何仍能保持宿主无感知

### 第 6 篇：编译时 Driver 与运行时 Extension 为什么并存

**核心问题：DataZen 为什么有两种插件机制？**

- 编译时数据库 Driver 解决什么问题
- 运行时 Extension 解决什么问题
- Extension Manifest v2
- 工作区页面与主题贡献
- `datazen://` 自定义协议
- 沙箱 iframe 与 `postMessage` 桥
- 权限声明与 deny-by-default
- 插件安装、校验、备份和卸载
- 为什么运行时 Extension 暂不承载数据库 Driver

## 第三部分：前端工作区设计

### 第 7 篇：从多窗口到统一工作区

**核心问题：桌面数据库工具应该如何组织复杂功能？**

- 主工作区 Page 与独立子窗口的边界
- Connection、Settings、Workflow 等为何内嵌主窗口
- Backup、Data Sync、Schema Diff 为何保留子窗口
- Workspace Tab 的生命周期
- 窗口路由与 `windowKind`
- 跨窗口通信
- ErrorBoundary 与窗口级故障隔离
- macOS、Windows WebView 差异

### 第 8 篇：Zustand 状态管理与事件流

**核心问题：复杂工作区状态如何拆分而不失控？**

- Store 按领域拆分的原则
- Connection、Panel、Schema、Settings、AI、Dashboard 等 Store
- 持久状态、会话状态和纯 UI 状态的边界
- 跨 Store 协作
- 跨窗口状态同步
- 快捷键、单元格编辑、筛选排序的事件流
- 如何避免组件直接编排业务流程

### 第 9 篇：大数据量结果集的前端性能设计

**核心问题：数据库查询返回大量数据时，界面如何保持流畅？**

- 查询结果流式传输
- DataTable 虚拟滚动
- 单元格渲染与数据类型颜色
- 分页、Offset 能力与驱动元数据
- 大对象和复杂字段的延迟展示
- 筛选、排序、编辑的性能权衡
- Schema 缓存与前端体验的关系
- 性能指标与测试方法

## 第四部分：通用执行与智能能力

### 第 10 篇：YAML Workflow 执行引擎

**核心问题：如何把数据库操作组织成可复用工作流？**

- Workflow Definition 与 Executor
- Command、Condition、ForEach 和 AI Step
- Workflow 默认连接与 Step 覆盖
- 模板变量解析
- Command Discovery 与输入校验
- 旧版 Query Step 的兼容策略
- GUI、IPC、MCP 共用同一 Runtime
- 执行历史、失败处理与可观测性
- Schema-driven Workflow 编辑器

### 第 11 篇：多 Provider AI 架构与 NL2SQL

**核心问题：如何让 AI 能力独立于具体模型厂商？**

- `AiProvider` trait 与 `ai-api`
- OpenAI、Anthropic、DeepSeek 与 Custom Provider
- Provider 协议层与流式响应
- PromptResolver 的多级覆盖策略
- SchemaContextBuilder
- 大量数据库表下的上下文压缩
- `.ctx.yaml` 表组机制
- `@` 上下文引用
- Driver Prompt Override
- API Key 的安全存储与日志隔离

### 第 12 篇：让数据库能力进入 AI 生态——MCP 架构

**核心问题：DataZen 如何同时成为 MCP Server 和 MCP Client？**

- GUI 模式与 `--mcp-stdio` 无头模式
- MCP Server 暴露的 Tools、Resources 和 Prompts
- 持久化 `connection_id` 的设计
- MCP 调用如何复用 DbTools、Workflow 和 Command Runtime
- MCP Client 如何接入外部工具
- AI Chat 如何调用 MCP
- Token、权限与敏感数据边界
- 为什么协议入口不应该复制一套业务逻辑

## 第五部分：数据库高级能力

### 第 13 篇：Schema Diff、Data Sync 与 Transfer 的边界

**核心问题：三个看起来相似的功能为什么必须分开设计？**

- Structure Sync、Data Synchronization 与 Transfer 的概念区别
- Data Sync 的同族数据库硬门闸
- 表结构与主键一致性检查
- 流式数据比较
- ChangeSet 建模
- 参数化 DML 执行
- Schema Diff 到 DDL Plan 再到 Deploy
- 异构数据迁移为什么需要独立 IR
- 如何防止产品概念混乱渗透到代码架构

### 第 14 篇：缓存、持久化与数据安全

**核心问题：桌面数据库工具如何安全保存状态并控制资源消耗？**

- SchemaCache 的两级 TTL 缓存
- 缓存命中、失效和刷新
- 应用配置、查询历史与敏感信息的存储差异
- AES-256-GCM 加密
- 系统 Keychain 与 `.key` 后备方案
- CSP、路径遍历防护与扩展名白名单
- Extension 资源访问隔离
- 日志脱敏
- AI Key 和数据库密码的安全边界

## 第六部分：工程质量与未来演进

### 第 15 篇：可插拔系统的测试策略

**核心问题：如何证明宿主与大量 Driver 能够独立演进？**

- Rust 单元测试与集成测试
- Vitest 前端组件和 Store 测试
- Host 测试与 Driver 测试的边界
- 为什么驱动方言测试不能写进 Host
- WebdriverIO Host E2E
- Driver Contract Matrix
- 插件安装与权限守护测试
- 手工黑盒测试
- PR 合并前的质量门槛

### 第 16 篇：从桌面应用走向共享 Core 与 Web 平台

**核心问题：现有架构如何支撑未来的平台化？**

- 哪些能力属于可共享 Core
- Desktop Adapter 与 Web Adapter
- SQLite 与 MySQL 持久化后端
- Tauri IPC 与 HTTP API 的边界
- SQL Prepare、Audit 与服务端安全
- 哪些桌面假设必须被移除
- 渐进式迁移而不是重写
- 当前架构中已经为平台化准备好的抽象

这一篇应明确标注为“演进方向或 RFC”，避免把规划误认为当前实现。

## 单篇文章结构约定

为了保持系列风格统一，每篇文章采用下面的基本结构：

1. 从一个真实问题或失败模式开始；
2. 给出模块在全局架构中的位置；
3. 解释核心设计和关键抽象；
4. 走一遍完整调用链；
5. 展示少量关键接口或数据结构；
6. 说明主要设计取舍；
7. 总结当前限制与后续演进；
8. 附相关源码和架构文档入口。

首批优先完成架构全景、Tauri 前后端边界、数据库 Driver、Driver Command API、Workflow、AI 与 MCP 六篇，以便先建立 DataZen 最具辨识度的架构主线。
