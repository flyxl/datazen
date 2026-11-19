//! Re-export shim — the React
//! `packages/frontend/src/components/sql-editor/metadata/metadataCache.ts`
//! counterpart (T242).
//!
//! That React file is a pure re-export of `src/lib/relationMetadata/metadataCache`
//! ("the cache is a process-wide singleton shared by the editor and the
//! Visual Query Builder, so it lives in `src/lib/relationMetadata` rather
//! than inside the editor"). The landed Rust home of those contracts is
//! [`crate::relation_metadata`]; this shim re-exports the same surface
//! under the editor-side path so the port keeps React's import topology:
//!
//! * `createMetadataCache` → [`MetadataCache::new`][cache-new] /
//!   [`default_metadata_cache`][default_metadata_cache] (the host-built
//!   singleton — React's process-global is constructed by the host here).
//! * `editorRelationKey` → [`build_editor_relation_key`].
//! * `relationIdentityKey` → [`relation_identity_key`].
//! * `schemaToMetadata` → [`schema_to_metadata`].
//! * `MetadataCache` / `MetadataCacheDeps` → same names.
//!
//! [cache-new]: crate::relation_metadata::metadata_cache::MetadataCache::new
//! [default_metadata_cache]: crate::relation_metadata::metadata_cache::default_metadata_cache

pub(crate) use crate::relation_metadata::identity::{
    build_editor_relation_key, relation_identity_key,
};
pub(crate) use crate::relation_metadata::metadata_cache::{
    default_metadata_cache, MetadataCache, MetadataCacheDeps,
};
pub(crate) use crate::relation_metadata::schema::schema_to_metadata;
