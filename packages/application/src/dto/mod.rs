//! 用例侧 DTO：**整体 re-export 契约层词汇 + 承载用例请求视图**。
//!
//! 契约形状（`SessionView`、`ExecutionView`、`ArtifactChunk`、`JobView`、事件信封……）由
//! [`datazen_platform_api::dto`] 单点定义并在两侧保持逐字段一致；本模块**不重新定义**任何
//! 已有形状，只补两类 platform-api 不该拥有的东西：
//!
//! * [`requests`]：连接 §4 的**请求** DTO（`OpenSessionRequest` 等）及其纯字段校验。
//!   响应形状进契约层（端口签名要用），请求形状是应用侧入口，两侧不需要共享类型实例。
//! * [`execution`]：可落盘的 `DurableExecutionRecord`（连接 §4.4），与实时 `ExecutionView`
//!   结构上分离，仓储只能接收前者。
//! * [`accept`]：幂等接受记录（连接 §13.1）的白名单投影，不含 `SessionHandle`。
//!
//! 依据：[概要 §6.2](../../../docs/architecture/platform/system-overview.md)、
//! [连接 §4 / §4.4 / §13.1](../../../docs/architecture/platform/connection-management.md)。

pub use datazen_platform_api::dto::*;

pub mod accept;
pub mod execution;
pub mod requests;
