//! Metadata layer (ported from `packages/frontend/src/lib/metadata/`).
//!
//! `identity.rs` carries the cache/batch isolation keys (React
//! `lib/metadata/identity.ts`, tasklist T080). `lib/metadata/batch.ts`
//! (tasklist T031) lands beside it as `batch.rs` and consumes the same
//! derivations instead of forking them. `lib/metadata/temporaryResults.ts`
//! (tasklist T118) lands as `temporary_results.rs` and reuses `identity.rs`'s
//! target mirrors while adding the `SessionHandle`/`ResultProvenance`
//! mirrors of its own.

mod batch;
mod identity;
mod temporary_results;

pub use batch::{BatchFlush, MetadataBatcher, MetadataBatcherOptions, MetadataClock};
pub use identity::{
    ExecutionTarget, MetadataIdentityInput, MetadataSessionIdentity, MetadataTarget,
    NamespaceTarget, ObjectTarget, metadata_batch_bucket, metadata_cache_key,
    metadata_config_bucket, metadata_session_key, METADATA_IDENTITY_SURFACE,
};
pub use temporary_results::{
    CurrentCallContext, CurrentProvenanceContext, InvalidationDecision, InvalidationReason,
    ResultProvenance, SessionHandle, TemporaryResult, check_temporary_result,
    is_provenance_current, read_temporary_result,
};
