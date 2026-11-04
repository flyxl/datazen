//! 用例侧 DTO：**整体 re-export 契约层词汇 + 承载用例请求视图**。
//!
//! 契约形状（`SessionView`、`ExecutionView`、`ArtifactChunk`、`JobView`、事件信封……）由
//! [`crate::execution::ports::dto`] 单点定义并在两侧保持逐字段一致；本模块**不重新定义**任何
//! 已有形状，只补 platform-api 不该拥有的东西：
//!
//! * [`execution`]：可落盘的 `DurableExecutionRecord`（连接 §4.4），与实时 `ExecutionView`
//!   结构上分离，仓储只能接收前者。
//!
//! 请求 DTO（`OpenSessionRequest` 等，连接 §4）**已迁到** `platform_api::dto::requests`：
//! 它们出现在用例端口方法签名上，而实现该端口的 `runtime` 不得依赖本 crate（R2），
//! 端口签名里的类型只能住在更内层。本模块经上面的 glob 再导出，`crate::execution::application::requests::*`
//! 路径逐字不变。
//! * [`accept`]：幂等接受记录（连接 §13.1）的白名单投影，不含 `SessionHandle`。
//!
//! 依据：[概要 §6.2](../../../docs/architecture/platform/system-overview.md)、
//! [连接 §4 / §4.4 / §13.1](../../../docs/architecture/platform/connection-management.md)。

pub use crate::execution::*;

pub mod accept;
pub mod execution;
