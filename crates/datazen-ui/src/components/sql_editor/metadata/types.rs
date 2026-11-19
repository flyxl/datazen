//! Re-export shim — the React
//! `packages/frontend/src/components/sql-editor/metadata/types.ts`
//! counterpart (T242).
//!
//! That React file is a pure re-export of `src/lib/relationMetadata/types`
//! ("the contracts moved to `src/lib/relationMetadata` when the Visual
//! Query Builder started reading the same cache: a shared service must not
//! live inside one of its consumers. This path is kept so editor code
//! keeps importing from here."). The landed Rust home is
//! [`crate::relation_metadata`]; this shim re-exports the same contracts
//! under the editor-side path, exactly like the React original.

pub(crate) use crate::relation_metadata::identity::{
    EditorRelationKey, EditorRelationKind, SqlRelationSourceKind,
};
pub(crate) use crate::relation_metadata::schema::{
    EditorMetadataContext, EditorMetadataSnapshot, EditorRelationMetadata, EditorRelationRequest,
};
