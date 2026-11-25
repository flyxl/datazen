//! Adapt shared relation metadata into prediction input, ported from React
//! `packages/frontend/src/lib/relationPrediction/fromRelationMetadata.ts` (T081).
//!
//! The engine deliberately knows nothing about the metadata cache; this is the
//! one place that bridges them, so the editor and the Visual Query Builder feed
//! it identical input and cannot disagree about the same schema.
//!
//! Pure logic: no Rune tree, no `oklch`, no i18n, no IPC. Case folding uses
//! `str::to_lowercase`, the Rust counterpart of JS `String.prototype.toLowerCase`.
//!
//! # Map input choice
//!
//! React's `toPredictionTables` spreads a `Map`, so its output order is the
//! map's insertion order. The Rust port therefore takes
//! [`OrderedMap`][super::ordered_map::OrderedMap] (the same insertion-ordered
//! map the metadata cache uses) rather than a `BTreeMap`: sorting the input
//! would silently reorder prediction candidates. Callers that hold a
//! `BTreeMap` must copy it into an `OrderedMap` first.

use std::collections::HashSet;

use super::ordered_map::OrderedMap;
use super::relation_prediction_types::{DeclaredForeignKey, PredictionColumn, PredictionTable};
use super::schema::EditorRelationMetadata;

/// React's `schemaOf?: (id: string) => string | undefined`.
#[allow(dead_code)] // T081 forthcoming consumer: the SQL-editor prediction layer (not landed yet).
pub type SchemaOf = dyn Fn(&str) -> Option<String>;

/// Column names covered by at least one index.
///
/// `IndexInfo.columns` is a list; a column counts as indexed if any index
/// mentions it, single-column or composite. Names are lowercased once, here,
/// so the membership test in [`to_prediction_table`] is case-insensitive.
#[allow(dead_code)] // T081 forthcoming consumer: the SQL-editor prediction layer (not landed yet).
pub fn indexed_column_names(relation: &EditorRelationMetadata) -> HashSet<String> {
    let mut indexed = HashSet::new();
    for index in &relation.indexes {
        for column in &index.columns {
            indexed.insert(column.to_lowercase());
        }
    }
    indexed
}

/// Every column set a relationship may target: the primary key plus each unique
/// index. Non-unique indexes are not keys and are excluded. The primary key is
/// pushed verbatim (no case folding), matching React's `[...relation.primaryKey]`.
#[allow(dead_code)] // T081 forthcoming consumer: the SQL-editor prediction layer (not landed yet).
pub fn unique_column_sets(relation: &EditorRelationMetadata) -> Vec<Vec<String>> {
    let mut sets: Vec<Vec<String>> = Vec::new();
    if !relation.primary_key.is_empty() {
        sets.push(relation.primary_key.clone());
    }
    for index in &relation.indexes {
        if !index.is_unique || index.columns.is_empty() {
            continue;
        }
        sets.push(index.columns.clone());
    }
    sets
}

/// Copy the declared foreign keys into prediction input. Ground truth is never
/// re-derived, so the fields are carried verbatim.
#[allow(dead_code)] // T081 forthcoming consumer: the SQL-editor prediction layer (not landed yet).
pub fn declared_foreign_keys(
    relation: &EditorRelationMetadata,
) -> Vec<DeclaredForeignKey> {
    relation
        .foreign_keys
        .iter()
        .map(|fk| DeclaredForeignKey {
            columns: fk.columns.clone(),
            referenced_table: fk.referenced_table.clone(),
            referenced_columns: fk.referenced_columns.clone(),
        })
        .collect()
}

/// Convert one loaded relation into prediction input.
///
/// `id` is the stable identity used in the output; the caller decides what a
/// table is keyed by (the builder uses its canvas identity). `schema` is the
/// schema context the caller tracks for that identity, if any.
#[allow(dead_code)] // T081 forthcoming consumer: the SQL-editor prediction layer (not landed yet).
pub fn to_prediction_table(
    id: &str,
    relation: &EditorRelationMetadata,
    schema: Option<&str>,
) -> PredictionTable {
    let indexed = indexed_column_names(relation);
    let columns: Vec<PredictionColumn> = relation
        .columns
        .iter()
        .map(|column| PredictionColumn {
            name: column.name.clone(),
            data_type: column.data_type.clone(),
            nullable: Some(column.nullable),
            indexed: Some(indexed.contains(&column.name.to_lowercase())),
        })
        .collect();

    PredictionTable {
        id: id.to_owned(),
        name: relation.identity.name.name.clone(),
        schema: schema.map(str::to_owned),
        columns,
        primary_key: relation.primary_key.clone(),
        unique_column_sets: unique_column_sets(relation),
        declared_foreign_keys: declared_foreign_keys(relation),
    }
}

/// Convert a set of loaded relations into prediction input, preserving the
/// input map's iteration order.
///
/// `schema_of` resolves the schema for each identity; `None` (or a `None`
/// result) leaves that table's schema unset, exactly like React's `schemaOf?.(id)`.
#[allow(dead_code)] // T081 forthcoming consumer: the SQL-editor prediction layer (not landed yet).
pub fn to_prediction_tables(
    relations: &OrderedMap<String, EditorRelationMetadata>,
    schema_of: Option<&SchemaOf>,
) -> Vec<PredictionTable> {
    relations
        .iter()
        .map(|(id, relation)| {
            let schema = schema_of.and_then(|resolve| resolve(id));
            to_prediction_table(id, relation, schema.as_deref())
        })
        .collect()
}
