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
//! multi-file React family, one Rust entry per React file, so later siblings
//! land beside their sources instead of at the crate root. Landed so far:
//!
//! - `normalize.rs` — the identifier/type folding the engine compares with
//!   (tasklist T033);
//! - `from_table_schema.rs` — the ER diagram's schema adapter (tasklist
//!   T033);
//! - `predict_relations/` — the engine (React `predictRelations.ts`,
//!   tasklist T082; this segment's tracker assigned the port as T081 — the
//!   tasklist's own T081 is the `relationMetadata` family): the reverse-index
//!   search, the composite fold and the ambiguity abstention, plus the
//!   output half of `types.ts` (evidence/candidates/options). The suite is
//!   split into `predict_relations/tests.rs` to respect the single-file
//!   size discipline.
//! - `related_tables.rs` — the completion boost hints: declared foreign-key
//!   targets plus the engine's unambiguous candidates, collapsed and ranked
//!   (React `relatedTables.ts`, tasklist T121; this segment's tracker
//!   assigns the port as T119).
//!
//! Still to land: `fromRelationMetadata.ts` (the metadata adapter, with
//! `relationMetadata/sessionConfig.ts` + `types.ts`). `relatedTables.ts`
//! itself has landed as `related_tables.rs` (T119), and with it the whole
//! of `types.ts` is carried — input half in `from_table_schema.rs` (T033),
//! output half in `predict_relations/types.rs` (tasklist T082), audited
//! export by export with no gap (T119). React's consumers
//! (`windows/connection/ErDiagramView.tsx:174`,
//! `windows/connection/query/QueryBuilderHostAdapter.tsx:126`) both call
//! `predictRelations(toPredictionTablesFromSchemas(...))`, so the view ports
//! wire through here now that the engine has landed.

mod from_table_schema;
mod normalize;
mod predict_relations;
mod related_tables;

pub use from_table_schema::{
    ColumnSchema, DeclaredForeignKey, ForeignKeyInfo, IndexInfo, PredictionColumn, PredictionTable,
    TableSchema, to_prediction_table_from_schema, to_prediction_tables_from_schemas,
};
pub use normalize::{
    TypeFamily, is_generic_key_name, normalize_data_type, normalize_identifier, singularize,
    split_identifier_words, type_family,
};
pub use predict_relations::{
    ColumnPair, EvidenceWeights, PredictionEvidence, PredictionEvidenceCode, PredictionOptions,
    PredictionTier, RelationCandidate, predict_relations, DEFAULT_AMBIGUITY_MARGIN,
    DEFAULT_HIGH_THRESHOLD, DEFAULT_MEDIUM_THRESHOLD, EVIDENCE_WEIGHTS,
};
pub use related_tables::{
    RelatedTableHint, RelatedTableOrigin, related_table_boost_map, related_table_hints,
    DECLARED_RELATION_BOOST, PREDICTED_RELATION_BOOST,
};
