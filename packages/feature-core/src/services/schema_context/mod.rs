//! Shared implementation of the [`LogicalSchemaContextSource`](crate::services::ports::LogicalSchemaContextSource) port.
//!
//! D6 item 4 收尾（2026-10-26）：实现的唯一定义迁入
//! `datazen-modules::ai::application::schema_context`（AI schema 上下文的
//! 生产端）。本模块只再导出，旧路径不变；**行为用例留在本垫片层**
//! （`#[cfg(test)] mod tests`）——它们需要一个真实
//! `RuntimeConnectionUseCases`（fakes 的端口实现无法覆盖分页/可见性
//! 语义），持有 datazen-modules dev-dep，与 D5 data_sync 垫片保留集成
//! 测试同一先例；D8 退役 feature-core 时随垫片一并处置。
//!
//! The AI feature needs database metadata (tables, columns, types, schema) for
//! the session the user is working in. That read is **not** a desktop concern:
//! every form that owns a Runtime session needs the same answer. The
//! implementation therefore lives in the shared layer, and a form only supplies
//! the two cross-form dependencies:
//!
//! - [`SessionConnectionLookup`](crate::services::ports::SessionConnectionLookup)
//!   for the session-to-connection mapping (a consumer-defined port the host answers), and
//! - [`ResourceExecutionHost`](crate::workflow::resource_execution::ResourceExecutionHost)
//!   for the profile, the driver and command execution.
//!
//! The source holds no host type: it cannot see a store, a platform entry or
//! a driver registry, which is what keeps the same code usable by the desktop
//! and by a future web form.
//!
//! The port it implements is the *logical* one: the Runtime path addresses
//! sessions logically, so it never owns a live connection handle. It answers
//! the physical contract — [`PhysicalSchemaContextSource`](crate::services::ports::PhysicalSchemaContextSource)
//! — with the refusal that rejects physical-handle exposure.

pub use datazen_modules::ai::application::schema_context::{
    RuntimeSchemaContextSource, UnavailableSchemaContextSource,
};

#[cfg(test)]
mod tests;
