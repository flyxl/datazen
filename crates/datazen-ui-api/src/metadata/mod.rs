//! Metadata layer (ported from `packages/frontend/src/lib/metadata/`).
//!
//! `identity.rs` carries the cache/batch isolation keys (React
//! `lib/metadata/identity.ts`, tasklist T080). `lib/metadata/batch.ts`
//! (tasklist T031) lands beside it as `batch.rs` and consumes the same
//! derivations instead of forking them.

mod batch;
mod identity;

pub use batch::{BatchFlush, MetadataBatcher, MetadataBatcherOptions, MetadataClock};
pub use identity::{
    ExecutionTarget, MetadataIdentityInput, MetadataSessionIdentity, MetadataTarget,
    NamespaceTarget, ObjectTarget, metadata_batch_bucket, metadata_cache_key,
    metadata_config_bucket, metadata_session_key,
};
