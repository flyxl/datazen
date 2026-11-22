//! Metadata layer (ported from `packages/frontend/src/lib/metadata/`).
//!
//! `identity.rs` carries the cache/batch isolation keys (React
//! `lib/metadata/identity.ts`, tasklist T080). `lib/metadata/batch.ts`
//! (tasklist T031) lands beside it as `batch.rs` and consumes the same
//! derivations instead of forking them. `lib/metadata/temporaryResults.ts`
//! (tasklist T118) lands as `temporary_results.rs` and reuses the canonical
//! `types::query` target, handle and provenance families — the identity/target
//! mirrors were unified there so every name has one definition.

mod batch;
mod identity;
mod temporary_results;

pub use batch::{BatchFlush, MetadataBatcher, MetadataBatcherOptions, MetadataClock};
pub use identity::{
    MetadataIdentityInput, MetadataSessionIdentity, MetadataTarget, metadata_batch_bucket,
    metadata_cache_key, metadata_config_bucket, metadata_session_key,
};
pub use temporary_results::{
    CurrentCallContext, CurrentProvenanceContext, InvalidationDecision, InvalidationReason,
    TemporaryResult, check_temporary_result, is_provenance_current, read_temporary_result,
};
pub use crate::types::{
    ExecutionTarget, NamespaceTarget, ObjectTarget, ResultProvenance, SessionHandle,
};
