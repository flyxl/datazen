# DataZen 后端技术方案

> **本文档已拆分为模块化子文档。** 请参阅 [架构文档总览](architecture/README.md)。

## 子文档索引

| 文档 | 内容 |
|------|------|
| [数据库驱动层](architecture/backend/drivers.md) | DatabaseDriver trait、驱动注册表、插件扩展机制 |
| [Schema 缓存](architecture/backend/cache.md) | 多级缓存架构、缓存失效策略、查询执行优化 |
| [服务层](architecture/backend/services.md) | ConnectionManager、资源安全、连接泄露防护 |
| [持久化存储](architecture/backend/store.md) | 本地文件存储、AES-256-GCM 加密 |
| [IPC 命令层](architecture/backend/commands.md) | Tauri Commands、AppState、CommandError |
| [AI 模块](architecture/backend/ai.md) | AiProvider trait、Provider 实现、Prompt 管理 |
| [MCP 模块](architecture/backend/mcp.md) | MCP Server/Client |
| [Workflow 模块](architecture/backend/workflow.md) | YAML Workflow 引擎 |
| [安全措施](architecture/security.md) | 加密、CSP、密码派生 |
| [窗口管理](architecture/windows.md) | 多窗口架构、Rust 端窗口创建 |
| [测试策略](architecture/testing.md) | 单元测试、集成测试、E2E |
