//! Relation prediction (ported from React
//! `packages/frontend/src/lib/relationPrediction/`).
//!
//! Real schemas frequently have no declared foreign keys — ORM-managed,
//! legacy and sharded databases all rely on naming conventions instead of
//! constraints. The family under this directory infers those relationships
//! from metadata alone, governed by "a wrong match is worse than no match".
//!
//! ## Layout
//!
//! Mirrors the `metadata` module's convention: a directory module per
//! multi-file React family, one Rust file per React file, so later siblings
//! land beside their sources instead of at the crate root. Landed so far
//! (tasklist T033):
//!
//! - `normalize.rs` — the identifier/type folding the engine compares with;
//! - `from_table_schema.rs` — the ER diagram's schema adapter.
//!
//! Still to land: `fromRelationMetadata.ts` (T081, with the
//! `relationMetadata/sessionConfig` + `types` pair), `predictRelations.ts`
//! (T082, split), `relatedTables.ts` + `types.ts` (T121). React's consumers
//! (`windows/connection/ErDiagramView.tsx:174`,
//! `windows/connection/query/QueryBuilderHostAdapter.tsx:126`) both call
//! `predictRelations(toPredictionTablesFromSchemas(...))`, so the view ports
//! wire through here once the engine lands.

mod from_table_schema;
mod normalize;

pub use from_table_schema::{
    ColumnSchema, DeclaredForeignKey, ForeignKeyInfo, IndexInfo, PredictionColumn, PredictionTable,
    TableSchema, to_prediction_table_from_schema, to_prediction_tables_from_schemas,
};
pub use normalize::{
    TypeFamily, is_generic_key_name, normalize_data_type, normalize_identifier, singularize,
    split_identifier_words, type_family,
};
