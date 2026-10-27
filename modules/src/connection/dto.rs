//! Connection-facing projections and receipt aliases.

pub use crate::connection::application::ProfileView;
pub use crate::connection::domain::{ProfileDraft, ProfileDraftPatch, ProfilePatch};
pub use crate::execution::application::{ConnectionEvent, ResultChunkEvent};
pub use crate::execution::target::ExecutionTarget;
pub use crate::shared::id::{DbSessionId, ExecutionId, StageId, StreamId, Timestamp};

pub type ProfileOpenSessionReceipt = crate::execution::OpenSessionReceipt;
pub type ProfileContextChangeReceipt = crate::execution::ContextChangeReceipt;
pub type ReceiptDbSessionId = DbSessionId;
pub type ReceiptExecutionId = ExecutionId;
pub type ReceiptStageId = StageId;
pub type ReceiptStreamId = StreamId;
pub type ReceiptTimestamp = Timestamp;
pub type ProfileInitialTarget = ExecutionTarget;
pub type ProfileCancelReceipt = crate::execution::CancelReceipt;
pub type ProfileCloseReceipt = crate::execution::CloseReceipt;
