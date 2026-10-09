//! 连接用例的契约 DTO：会话视图、回执、结果来源、任务、产物、事件、提交令牌。
//!
//! **为什么这些 DTO 在 platform-api 而不是 application**：端口 trait 的签名（§4.3–§4.6）
//! 直接引用了其中的 `SessionHandle`、`ExecutionView`、`JobState`、`ArtifactChunk`、
//! `IdempotentOperation`、`SubmissionToken`、`EventEnvelope` 等类型，而 F-04 规定
//! platform-api **不得依赖** application。因此凡被端口签名命名、或必须两侧一致的词汇，
//! 只能落在 platform-api。`application::dto` 承载的是**用例请求/响应 DTO**
//! （`OpenSessionRequest`、`ExecuteInSessionRequest` 等）并整体 `pub use` 本模块。
//!
//! 字段与 [连接管理 §4](../../../docs/architecture/platform/connection-management.md)
//! 的 TypeScript 声明逐字段对齐，JSON 键为 camelCase。

pub mod artifact;
pub mod event;
pub mod execution;
pub mod idempotency;
pub mod job;
pub mod profile;
pub mod session;
