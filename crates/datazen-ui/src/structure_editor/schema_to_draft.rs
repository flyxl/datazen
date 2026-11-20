//! Port of `packages/frontend/src/lib/structureEditor/schemaToDraft.ts`.
//!
//! The source `TableSchema` reuses the host schema VM already in
//! `crate::relation_metadata` (its `columns` / `primary_keys` / `indexes` fields
//! are the camelCase React fields in Rust snake_case); the two output drafts
//! are the existing `structure_editor` VMs. Ids come from T085's `draft_ids`,
//! matching React's `newColumnId` / `newIndexId`.
//!
//! Two details are intentionally exact rather than "tidy":
//! * PK membership is `schema.primaryKeys.includes(c.name)` — a name match, not
//!   `ColumnSchema.is_primary_key`.
//! * a column is unique only when a single-column, non-primary, unique index
//!   names it; a multi-column unique index never marks a column unique.
//! * primary indexes are dropped from the index drafts, and an absent
//!   `indexType` becomes `Some("")` (`idx.indexType ?? ''`), not `None`.

use super::{StructureColumnDraft, StructureIndexDraft};
use crate::relation_metadata::TableSchema;
use crate::structure_editor::draft_ids::{new_column_id, new_index_id};

/// Port of the `{ columns, indexes }` object returned by `schemaToDraft`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SchemaToDraft {
    pub(crate) columns: Vec<StructureColumnDraft>,
    pub(crate) indexes: Vec<StructureIndexDraft>,
}

/// Port of `schemaToDraft(schema)`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
pub(crate) fn schema_to_draft(schema: &TableSchema) -> SchemaToDraft {
    let columns: Vec<StructureColumnDraft> = schema
        .columns
        .iter()
        .map(|column| StructureColumnDraft {
            id: new_column_id(),
            name: column.name.clone(),
            data_type: column.data_type.clone(),
            nullable: column.nullable,
            default_value: column.default_value.clone(),
            comment: column.comment.clone(),
            is_primary_key: schema
                .primary_keys
                .iter()
                .any(|primary_key| primary_key == &column.name),
            is_auto_increment: column.is_auto_increment,
            is_unique: schema.indexes.iter().any(|index| {
                index.is_unique
                    && !index.is_primary
                    && index.columns.len() == 1
                    && index
                        .columns
                        .first()
                        .is_some_and(|name| name == &column.name)
            }),
        })
        .collect();

    let indexes: Vec<StructureIndexDraft> = schema
        .indexes
        .iter()
        .filter(|index| !index.is_primary)
        .map(|index| StructureIndexDraft {
            id: new_index_id(),
            name: index.name.clone(),
            columns: index.columns.clone(),
            is_unique: index.is_unique,
            is_primary: false,
            // React: idx.indexType ?? '' — an empty string, not undefined.
            index_type: Some(index.index_type.clone().unwrap_or_default()),
            include_columns: None,
            filter: None,
            comment: None,
        })
        .collect();

    SchemaToDraft { columns, indexes }
}
