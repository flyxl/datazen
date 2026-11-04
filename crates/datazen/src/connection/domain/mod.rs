//! Connection domain: the persisted connection aggregate's vocabulary and
//! repository boundary.
//!
//! Two views of the same aggregate live here:
//!
//! * [`catalog`](self) — the desktop form aggregate whose shape is
//!   [`ConnectionConfig`](datazen_driver_api::ConnectionConfig), the exact IPC
//!   payload of the connection form (re-projecting it would drift into
//!   user-visible data loss).
//! * [`profile`](self) — the server-facing profile vocabulary
//!   (`ProfileRecord` / `ProfileRepository` / `ProfileReader`): revision
//!   tracking, `SecretRef`-based credentials, and CAS updates
//!   (§5.1: Profile 模型与仓储契约归 connection::domain，D2 迁入).
//!
//! Plaintext semantics: credentials flow in and out of the repositories in
//! memory as-is. Encryption at rest is the implementor's business (desktop:
//! AES-256-GCM keyed with the connection password key source). Redaction is a
//! *presentation* concern applied by the application layer for summaries —
//! never inside the repository, or the connect path could not obtain the real
//! password.

/// 连接目录（配置 + 分组）：表单词汇 `ConnectionConfig` 的聚合仓储。
pub mod catalog;
/// Profile 词汇：服务端连接档案、revision 与仓储/只读端口。
pub mod profile;
/// 命名空间目标词汇：`NamespaceTarget` 与层级枚举（§5.1）。
pub mod target;

pub use catalog::{ConnectionRepository, ConnectionRepositoryError, ConnectionScope};
pub use profile::{
    ProfileDraft, ProfileDraftPatch, ProfilePatch, ProfileReader, ProfileRecord, ProfileRepository,
    ProfileScope,
};
pub use target::{NamespaceTarget, TargetNamespaceLayer};

/// 连接侧拥有的 shared ID（§5.1：唯一定义在 `shared::id`，领域再导出同一类型）。
pub use crate::shared::id::{
    ConfigRevision, ConnectionId, CredentialRevision, IdempotencyKey, NetworkRouteRef,
    NetworkRouteRevision, SecretRef, Timestamp,
};
