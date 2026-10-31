//! Execution application: service-boundary DTOs — views, receipts, requests,
//! events, and idempotency tokens (§5.1, D3).
//!
//! 原属 platform-api `dto/{session,execution,requests,event,idempotency}.rs`
//! 的应用侧，D3（2026-10-26）起唯一定义在 `datazen::execution`；
//! platform-api 只再导出。编排服务（`RuntimeConnectionUseCases` 的拆分）在
//! D3 后续切片迁入。

pub mod budget;
pub mod command_policy;
pub mod event_sink;
pub mod events;
pub mod idempotency;
pub mod identity;
pub mod master_key;
pub mod network;
/// `RuntimeConnectionUseCases` 编排（原 runtime::application，D3-3c 归位）。
pub mod orchestration;
pub mod policy;
pub mod requests;
pub mod secret;
pub mod settings_reader;
pub mod submission;
pub mod target_resolver;
pub mod use_cases;
pub mod views;

pub use event_sink::{EventSink, EventSubscription};
pub use events::{
    ArchivedEnvelope, ConnectionEvent, ConnectionEventEnvelope, EventEnvelope, EventSequence,
    ResultChunkEvent,
};
pub use idempotency::{IdempotentOperation, SubmissionToken};
pub use identity::{ExecutionIdentity, IdentityResolver};
pub use network::{endpoint_scheme, NetworkProvider, RoutePlan, TunnelBinding, TunnelSpec};
pub use orchestration::{
    BackupOutputBinding, IdlePolicy, IdleReclaimer, LiveConnection, LiveConnectionError,
    LiveConnectionLease, LiveConnectionLedger, LiveConnectionRequest, LiveConnectionSource,
    LivePurpose, LiveRelease, RuntimeArtifactChunk, RuntimeConnectionUseCases, RuntimeResultSink,
    SessionTransactionOperation, WaitOutcome,
};
pub use policy::{
    AuthorizationAction, AuthorizationDecision, AuthorizationSubject, DelegationGrant,
    PolicyService, PolicyVersionChange,
};
pub use requests::*;
pub use secret::{ResolvedCredential, SecretProvider, SecretPurpose};
pub use settings_reader::{ExecutionPolicy, ExecutionSettingsReader};
pub use submission::{SubmissionPresentation, SubmissionTokenIssuer, VerifiedSubmission};
pub use target_resolver::{ExecutionTargetResolver, TargetResolution};
pub use use_cases::ConnectionUseCases;
pub use views::{
    BackupOutput, CancelReceipt, CloseReceipt, ContextChangeReceipt, ExecutionReceipt,
    ExecutionView, OpenSessionReceipt, ResultChunk, RuntimeResultBinding, SessionView,
};
