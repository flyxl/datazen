//! 执行契约迁移垫片（D3，2026-10-26）。
//!
//! 状态机、取消结局、错误码、能力快照与执行来源的唯一定义在
//! `datazen-modules::execution::domain::execution`；实时视图、回执与结果块在
//! `datazen-modules::execution::application::views`。本模块只再导出，
//! 旧 import 路径 `dto::execution::*` 保持不变；D8 退役本 crate 时一并删除。
//!
//! §4.4 硬规则随定义迁移：`ExecutionView` 是实时响应，仓储不得直接序列化它。

pub use datazen_modules::execution::application::{
    ExecutionReceipt, ExecutionView, ResultChunk, RuntimeResultBinding,
};
pub use datazen_modules::execution::domain::{
    CancelOutcome, CapabilitySnapshot, EffectOutcome, ExecutionErrorCode, ExecutionState,
    ResultCompleteness, ResultProvenance, StatementResultSource, WritableMapping,
};
