mod batcher;
mod keys;
mod loader;

use std::collections::BTreeMap;

pub use batcher::{build_ensure_plan, invalidate_plan, BucketPlan, EnsurePlan};
pub use keys::{read_bucket, read_identity, relation_columns_cache_key};
pub use loader::{
    column, load_relation_columns_step, project_relation_columns, relation_batch_chunks,
    resolve_column_relations, ReadColumnsRow,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationRef {
    pub database: String,
    pub schema: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationColumn {
    pub name: String,
    pub data_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationColumns {
    pub relation: RelationRef,
    pub columns: Vec<RelationColumn>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionMetadataIdentity {
    pub connection_id: String,
    pub metadata_revision: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaItem {
    pub name: String,
    pub schema: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectionSchemaState {
    pub connection_id: String,
    pub metadata_revision: u64,
    pub table_catalogs: BTreeMap<String, Vec<SchemaItem>>,
    pub tables: Vec<SchemaItem>,
    pub views: Vec<SchemaItem>,
    pub path_items: BTreeMap<String, Vec<SchemaItem>>,
    pub current_database: Option<String>,
    pub namespace_owned_by_plugin: bool,
    pub path_aliases: BTreeMap<String, String>,
    pub relation_columns: BTreeMap<String, RelationColumns>,
}

pub const DEFAULT_BATCH_DEBOUNCE_MS: u64 = 40;
pub const RELATION_COLUMN_BATCH_SIZE: usize = 256;

pub const SCHEMA_COLUMN_LOADING_SURFACE: [&str; 8] = [
    "createSchemaColumnBatcher",
    "loadRelationColumns",
    "resolveColumnRelations",
    "projectRelationColumns",
    "relationColumnsCacheKey",
    "readIdentity",
    "DEFAULT_BATCH_DEBOUNCE_MS",
    "SCHEMA_COLUMN_LOADING_GAP",
];

pub const SCHEMA_COLUMN_LOADING_GAP: [&str; 5] = [
    "Promise/waiter settlement and the debounce timer host",
    "executeTargetCommand read_relation_columns IPC",
    "driver capability metadata (hasSchemaLevel, relationSchemaFor)",
    "knownTableNames over the namespace tree",
    "the schema store that commits loaded columns and invalidates sessions",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_surfaces_and_gaps_are_pinned() {
        assert_eq!(DEFAULT_BATCH_DEBOUNCE_MS, 40);
        assert_eq!(RELATION_COLUMN_BATCH_SIZE, 256);
        assert_eq!(SCHEMA_COLUMN_LOADING_SURFACE.len(), 8);
        assert_eq!(SCHEMA_COLUMN_LOADING_GAP.len(), 5);
        assert_eq!(crate::SCHEMA_COLUMN_LOADING_SURFACE.len(), 8);
        assert_eq!(crate::SCHEMA_COLUMN_LOADING_GAP.len(), 5);
    }
}
