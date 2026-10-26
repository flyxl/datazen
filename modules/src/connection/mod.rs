//! Connection vertical slice: domain vocabulary, repository boundary, and use cases.
//!
//! D2 第二切片（2026-10-26）：Profile/namespace 服务端词汇归位本模块
//! （domain：`ProfileRecord` / `ProfileRepository` / `ProfileReader` /
//! `NamespaceTarget`；application：`ProfileView` 投影），platform-api 只再导出。

pub mod application;
pub mod domain;

pub use application::{
    merge_missing_secrets, redact_secrets, ConnectionError, ConnectionService, ProfileView,
};
pub use domain::{
    ConnectionRepository, ConnectionRepositoryError, ConnectionScope, NamespaceTarget,
    ProfileDraft, ProfileDraftPatch, ProfilePatch, ProfileReader, ProfileRecord,
    ProfileRepository, ProfileScope, TargetNamespaceLayer,
};
