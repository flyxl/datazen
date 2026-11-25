//! SQL-editor relation metadata facade — the React
//! `packages/frontend/src/components/sql-editor/metadata/` directory port
//! (T242): `findRelation.ts`, `metadataCache.ts`, `types.ts`.
//!
//! One Rust module per React file, at the same path, plus this `mod.rs`
//! standing in for the directory:
//!
//! * [`find_relation`] — the robust snapshot resolver with the schema-tree
//!   fallback (the directory's only real logic).
//! * [`types`] / [`metadata_cache`] are **re-export shims**, exactly like
//!   their React counterparts: the contracts moved to
//!   `src/lib/relationMetadata` when the Visual Query Builder started
//!   reading the same cache ("a shared service must not live inside one of
//!   its consumers"), so those two React files are pure `export … from`
//!   re-exports and the landed Rust home is
//!   [`crate::relation_metadata`]. The shims keep the editor-side import
//!   path working and self-documenting.
//!
//! Pure logic: no Rune tree, no `oklch`, no i18n, no IPC.

pub mod find_relation;
pub mod metadata_cache;
pub mod types;

pub use find_relation::{extract_columns_from_schema, find_relation_metadata};
