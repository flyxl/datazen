//! Metadata data contracts, ported from React
//! `packages/frontend/src/lib/relationMetadata/types.ts` and `types/connection.ts`
//! (`TableSchema`, `ColumnSchema`, `IndexInfo`, `ForeignKeyInfo`).
//!
//! Empty vectors stand for React's optional/`?? []` arrays; `Option<String>`
//! covers both `null` and `undefined`. All types derive `Eq` so snapshots can be
//! compared by value — the Rust stand-in for React's frozen referential equality.

use super::identity::{EditorRelationKey, EditorRelationKind, QualifiedRelationId};

/// Immutable metadata entry for one loaded relation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct EditorRelationMetadata {
    pub key: EditorRelationKey,
    pub identity: QualifiedRelationId,
    pub kind: EditorRelationKind,
    pub columns: Vec<ColumnSchema>,
    pub primary_key: Vec<String>,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
    pub comment: Option<String>,
    pub loaded_at: i64,
}

/// Immutable snapshot read synchronously by completion / hover / inlay.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct EditorMetadataSnapshot {
    pub db_session_id: String,
    /// Tab-bound database the relations were resolved against.
    pub database: String,
    /// Stable schema context the relations were resolved against (if any).
    pub schema: Option<String>,
    /// Monotonic generation counter; bumps on any relations/context change.
    pub epoch: u64,
    pub(crate) relations: std::collections::BTreeMap<EditorRelationKey, EditorRelationMetadata>,
}

#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
impl EditorMetadataSnapshot {
    /// Snapshot entries (the React `ReadonlyMap`).
    pub fn relations(&self) -> &std::collections::BTreeMap<EditorRelationKey, EditorRelationMetadata> {
        &self.relations
    }

    pub fn relation(&self, key: &str) -> Option<&EditorRelationMetadata> {
        self.relations.get(key)
    }
}

/// Context needed to resolve/normalize a relation for a session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct EditorMetadataContext {
    /// Tab/panel-bound database — every relation lookup runs against it.
    pub database: String,
    pub schema: Option<String>,
    /// Dialect id (e.g. `postgresql`, `mysql`) — drives identifier folding.
    pub dialect_id: String,
}

/// A queued request from the semantic model.
#[derive(Clone, Debug)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct EditorRelationRequest {
    pub identity: QualifiedRelationId,
    /// `table`/`view` load; `cte`/`subquery` are skipped. `None` defaults to table.
    pub kind: Option<super::identity::SqlRelationSourceKind>,
}

#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
impl EditorRelationRequest {
    pub fn new(identity: QualifiedRelationId, kind: super::identity::SqlRelationSourceKind) -> Self {
        Self {
            identity,
            kind: Some(kind),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct ColumnSchema {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub index_type: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) enum Deferrability {
    Unknown,
    NotDeferrable,
    DeferrableInitiallyImmediate,
    DeferrableInitiallyDeferred,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct ForeignKeyInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    pub on_update: Option<String>,
    pub on_delete: Option<String>,
    pub deferrability: Option<Deferrability>,
}

/// The schema returned by the host loader. `comment` is read by
/// `schema_to_metadata` through a cast in React; here it is a first-class
/// optional field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) struct TableSchema {
    pub table_name: String,
    pub columns: Vec<ColumnSchema>,
    pub primary_keys: Vec<String>,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
    pub comment: Option<String>,
}

/// Build immutable metadata from a fetched schema.
#[allow(dead_code)] // T032 reusable API: remove once a SQL editor / VQB consumer lands.
pub(crate) fn schema_to_metadata(
    key: EditorRelationKey,
    identity: &QualifiedRelationId,
    kind: EditorRelationKind,
    schema: &TableSchema,
    loaded_at: i64,
) -> EditorRelationMetadata {
    EditorRelationMetadata {
        key,
        identity: identity.clone(),
        kind,
        columns: schema.columns.clone(),
        primary_key: schema.primary_keys.clone(),
        indexes: schema.indexes.clone(),
        foreign_keys: schema.foreign_keys.clone(),
        comment: schema.comment.clone(),
        loaded_at,
    }
}
