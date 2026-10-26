//! Execution application: service-boundary DTOs — views, receipts, requests,
//! events, and idempotency tokens (§5.1, D3).
//!
//! 原属 platform-api `dto/{session,execution,requests,event,idempotency}.rs`
//! 的应用侧，D3（2026-10-26）起唯一定义在 `datazen-modules::execution`；
//! platform-api 只再导出。编排服务（`RuntimeConnectionUseCases` 的拆分）在
//! D3 后续切片迁入。

pub mod budget;
pub mod event_sink;
pub mod events;
pub mod idempotency;
pub mod identity;
pub mod network;
pub mod policy;
pub mod use_cases;
pub mod requests;
pub mod secret;
pub mod settings_reader;
pub mod submission;
pub mod target_resolver;
pub mod views;

pub use event_sink::{EventSink, EventSubscription};
pub use events::{
    ArchivedEnvelope, ConnectionEvent, ConnectionEventEnvelope, EventEnvelope, EventSequence,
    ResultChunkEvent,
};
pub use idempotency::{IdempotentOperation, SubmissionToken};
pub use identity::{ExecutionIdentity, IdentityResolver};
pub use policy::{
    AuthorizationAction, AuthorizationDecision, AuthorizationSubject, DelegationGrant,
    PolicyService, PolicyVersionChange,
};
pub use use_cases::ConnectionUseCases;
pub use network::{endpoint_scheme, NetworkProvider, RoutePlan, TunnelBinding, TunnelSpec};
pub use requests::*;
pub use secret::{ResolvedCredential, SecretPurpose, SecretProvider};
pub use settings_reader::{ExecutionPolicy, ExecutionSettingsReader};
pub use submission::{SubmissionPresentation, SubmissionTokenIssuer, VerifiedSubmission};
pub use target_resolver::{ExecutionTargetResolver, TargetResolution};
pub use views::{
    BackupOutput, CancelReceipt, CloseReceipt, ContextChangeReceipt, ExecutionReceipt,
    ExecutionView, OpenSessionReceipt, ResultChunk, RuntimeResultBinding, SessionView,
};
