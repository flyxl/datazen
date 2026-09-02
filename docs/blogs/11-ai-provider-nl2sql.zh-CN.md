# DataZen 架构设计（十一）：多 Provider AI 架构与 NL2SQL

> AI 功能最容易被某一家模型 API 绑架：请求格式、流式事件、工具调用和错误处理都散落在 UI 中。DataZen 把厂商差异收进 `AiProvider` 和协议层，让 NL2SQL、诊断和对话共享同一条安全链路。

## Provider 抽象

`packages/ai-api` 定义 `AiProvider`、配置、消息、流式 Chunk、Tool Definition、Tool Call、ModelInfo 和 `AiError`。Host 只依赖 trait，不依赖 OpenAI 或 Anthropic 的 SDK 类型。

Provider 需要实现配置校验、初始化、非流式完成、流式完成，并声明是否支持 streaming 和 tools：

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn provider_type(&self) -> &str;
    async fn validate_config(&self, config: &AiProviderConfig) -> Result<(), AiError>;
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, AiError>;
    async fn stream_complete(
        &self,
        request: &CompletionRequest,
        tx: Sender<StreamChunk>,
    ) -> Result<(), AiError>;
}
```

`AiProviderRegistry` 通过 inventory 发现实现，内置 OpenAI、Anthropic、DeepSeek 和 Custom Provider。

## 协议层复用

Provider 与底层 HTTP 协议分离：OpenAI Chat Completions、OpenAI Responses、Anthropic Messages 分别由共享模块处理。Custom Provider 可以选择兼容协议，而不用重新实现超时、URL 规范化、SSE 解析和流式取消。

统一协议层还提供一致的日志脱敏、请求超时和错误映射。API Key 只在后端配置和请求生命周期中出现，不进入前端日志。

## NL2SQL 的真实输入

NL2SQL 不是把一句自然语言直接发给模型。`SchemaContextBuilder` 会根据连接和用户选择构建上下文：表名、列、类型、主键和必要的关系信息进入 token 预算；大量表可以通过 `.ctx.yaml` 表组和 `@` 引用缩小范围。

Prompt 由 `PromptResolver` 解析，优先级是用户覆盖 → Driver 覆盖 → 资源文件 → 编译时英文 fallback。这样 PostgreSQL 和 Redis 可以表达不同的查询提示，但 UI 不需要知道具体方言。

## AI 与数据库的边界

AI 生成的 SQL 仍然只是候选文本。执行前应回到 Query/Driver Command Runtime，经过 SQL guard、连接上下文和权限门控。AI Provider 不直接持有连接池，也不能绕过 `dbSessionId` 或 `connectionId` 解析。

诊断错误和 EXPLAIN 分析同样如此：后端先取得结构化错误或计划，再把必要上下文交给模型。避免把完整凭据、无关表或敏感查询历史发送到外部 Provider。

## Tool Calling 与流式对话

支持 tools 的 Provider 返回 Tool Call，Host 将其映射为 DbTools 或已连接 MCP Client 的工具，并把 Tool Result 再放回对话上下文。工具执行仍受 MCP/Host 权限模式控制。

流式响应通过 Tauri Events 分发文本 Chunk、Tool Call、完成和错误。前端只负责追加消息和显示状态，取消请求由 Rust 终止 HTTP 流和 Provider 任务。

## 配置与安全

AI 配置存储在 `ai_config.enc`，使用 AES-256-GCM；日志中只记录 Provider 类型、请求长度和错误类别，不记录 Key。Custom Provider 的远程模型列表获取也必须经过同一套超时和错误处理。

## 测试重点

Provider 单测验证协议解析、流式事件和错误转换；`ai-api` 测试验证 trait 和 factory；Host 测试验证 Prompt 优先级、Schema 上下文预算与权限；不应在测试日志中打印真实 Key 或完整 SQL。

## 结语

多 Provider AI 架构的关键不是“支持更多模型”，而是把模型差异、数据库上下文和执行权限分开。Provider 负责生成，ContextBuilder 负责提供最小必要上下文，Command/MCP Runtime 负责执行和门控。下一篇将沿着这条边界进入 MCP，看看 DataZen 如何同时成为 MCP Server 和 MCP Client。

相关资料：[AI 模块](../architecture/backend/ai.md) · [AI API](../../packages/ai-api/src/lib.rs) · [上下文文件技能](../../.agents/skills/gen-ctx-yaml/SKILL.md)
