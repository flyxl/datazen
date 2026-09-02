# DataZen 架构设计（十四）：缓存、持久化与数据安全

> 桌面数据库工具一边要记住用户的工作环境，一边又不能把密码、AI Key 和查询内容随意写进文件。DataZen 将 Schema 缓存、普通持久化和敏感数据加密分开设计，并把路径与资源访问限制在 Rust 后端。

## SchemaCache 优化什么

SchemaCache 缓存的是表结构、列类型、主键和相关元数据，不是数据库数据本身。缓存 key 至少包含连接、database 和 schema，避免同名表在不同目标之间串用。

读取流程通常是：

1. 检查内存缓存和 TTL；
2. 命中则直接返回结构；
3. 未命中时通过 Driver 查询系统表；
4. 写入缓存并限制条目数量；
5. 结构变更、切库或显式刷新时失效。

缓存减少了 Schema 树、DataTable 列信息和 AI 上下文构建的重复查询，但不能成为永久真相。DDL 或外部工具改变结构后，调用方需要刷新。

## 普通 Store 与敏感 Store

应用设置、连接列表、Workflow、收藏和查询历史由本地 Store 管理。配置模型使用 serde 的 camelCase 序列化，方便前端恢复。

连接密码和 AI 配置单独加密：AES-256-GCM 提供机密性和完整性，主密钥优先放在系统钥匙串；开发或特定环境可使用 `{appData}/.key` 后备方案。日志、错误和导出摘要不能包含明文凭据。

## 写盘策略

自动保存需要 debounce，避免每次键入都触发文件写入。关键文件应通过临时文件 + 原子替换写入，启动时发现损坏则保留可诊断信息并回退到默认值，而不是静默覆盖用户数据。

查询历史可以帮助回溯，但它也是敏感数据。默认只保存必要字段；向 MCP 暴露 query-history 时还要经过权限模式和连接白名单过滤。

## 路径与文件安全

文件导入、导出和 Extension 资源访问都必须在 Rust 端做路径遍历防护、扩展名白名单、大小限制和符号链接策略。生产 IPC 不接受任意路径覆盖；只有 webdriver/E2E 构建允许专用 override path。

Extension 使用自己的 `{appData}/plugins/{id}/.storage.json`，大小和请求次数受限，不能读取其他插件或宿主配置。

## CSP 与主题/资源

主窗口和 Extension 资源遵循 CSP、nosniff 和 MIME 白名单。`datazen://` 资产服务会先验证插件启用状态，再解析安全相对路径；Windows WebView2 与 macOS WebKit 的 scheme 匹配不同，因此 CSP 需要按双源策略设计。

主题资源、图标和编辑器配置加载失败时只降级对应功能，不应阻塞主应用启动。旧的 `{appData}/themes/` 运行时入口已经由 Extension themes 取代。

## 日志脱敏

`CmdExt` 在记录错误前调用日志脱敏，查询日志记录长度和摘要时也应避免完整 SQL、密码或 Token。Extension 审计只记录插件 ID、命令名和连接 ID，不写命令参数。

## 结语

缓存解决重复读取，Store 解决可恢复状态，加密和路径门控解决敏感边界。它们共同形成“本地优先但不随意暴露”的桌面数据层。下一篇将说明如何测试这些边界，尤其是为什么驱动方言测试必须留在驱动 crate 内。

相关资料：[Schema 缓存](../architecture/backend/cache.md) · [Store](../architecture/backend/store.md) · [Extension 安全](../architecture/backend/extensions.md)
