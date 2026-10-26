//! Execution application: service-boundary DTOs — views, receipts, requests,
//! events, and idempotency tokens (§5.1, D3).
//!
//! 原属 platform-api `dto/{session,execution,requests,event,idempotency}.rs`
//! 的应用侧，D3（2026-10-26）起唯一定义在 `datazen-modules::execution`；
//! platform-api 只再导出。编排服务（`RuntimeConnectionUseCases` 的拆分）在
//! D3 后续切片迁入。

pub mod events;
pub mod idempotency;
pub mod requests;
pub mod views;

pub use events::{ArchivedEnvelope, ConnectionEvent, ConnectionEventEnvelope, EventEnvelope, EventSequence, ResultChunkEvent};
pub use idempotency::{IdempotentOperation, SubmissionToken};
pub use requests::*;
pub use views::{
    BackupOutput, CancelReceipt, CloseReceipt, ContextChangeReceipt, ExecutionReceipt,
    ExecutionView, OpenSessionReceipt, ResultChunk, RuntimeResultBinding, SessionView,
};
