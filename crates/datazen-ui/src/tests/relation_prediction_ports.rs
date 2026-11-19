//! T081: real assertions for the Rust ports of
//! `packages/frontend/src/lib/relationPrediction/fromRelationMetadata.ts` and
//! the `relationMetadata` contracts (`sessionConfig.ts` / `types.ts`).
//!
//! The React unit tests in
//! `packages/frontend/src/lib/relationPrediction/__tests__/fromRelationMetadata.test.ts`
//! are the spec; the cases below pin the same exact outputs. The last section
//! fills the contract gaps T032 left untested (`EditorRelationSourceKind`,
//! `EditorMetadataContext::default`, snapshot lookup).
//!
//! Registered in `tests/mod.rs` as an ordinary test module, so its test names
//! are `tests::relation_prediction_ports::*`.

use std::collections::BTreeMap;

use crate::relation_metadata::ordered_map::OrderedMap;
use crate::relation_metadata::{
    declared_foreign_keys, editor_session_cache_key, indexed_column_names, metadata_session_key,
    resolve_editor_session_config_with, to_prediction_table,
    to_prediction_tables, unique_column_sets, unbound_session_config, ColumnSchema,
    DeclaredForeignKey, EditorMetadataContext, EditorMetadataSnapshot, EditorRelationKind,
    EditorRelationMetadata, EditorRelationRequest, EditorRelationSourceKind, EditorSessionConfig,
    ForeignKeyInfo, IndexInfo, PredictionColumn, QualifiedRelationId, SqlRelationSourceKind,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn column(name: &str, data_type: &str, nullable: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        nullable,
        ..Default::default()
    }
}

fn index(name: &str, columns: &[&str], is_unique: bool, is_primary: bool) -> IndexInfo {
    IndexInfo {
        name: name.to_owned(),
        columns: columns.iter().map(|c| (*c).to_owned()).collect(),
        is_unique,
        is_primary,
        index_type: None,
    }
}

fn foreign_key(
    name: &str,
    columns: &[&str],
    referenced_table: &str,
    referenced_columns: &[&str],
) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: name.to_owned(),
        columns: columns.iter().map(|c| (*c).to_owned()).collect(),
        referenced_table: referenced_table.to_owned(),
        referenced_columns: referenced_columns.iter().map(|c| (*c).to_owned()).collect(),
        ..Default::default()
    }
}

/// Mirrors the React `relation()` fixture: `public.orders`, one PK, no indexes
/// or foreign keys.
fn relation() -> EditorRelationMetadata {
    EditorRelationMetadata {
        key: "public.orders".to_owned(),
        identity: QualifiedRelationId::qualified(&["public"], "orders"),
        kind: EditorRelationKind::Table,
        columns: vec![
            column("id", "integer", false),
            column("user_id", "integer", false),
        ],
        primary_key: vec!["id".to_owned()],
        indexes: vec![],
        foreign_keys: vec![],
        comment: None,
        loaded_at: 1,
    }
}

// ---------------------------------------------------------------------------
// indexed_column_names
// ---------------------------------------------------------------------------

#[test]
fn indexed_column_names_collects_composite_and_single_columns_case_folded() {
    let mut relation = relation();
    relation.indexes = vec![
        index("idx_pair", &["TenantID", "Code"], false, false),
        index("idx_single", &["USER_ID"], false, false),
    ];
    let names = indexed_column_names(&relation);
    assert_eq!(names.len(), 3, "composite columns all count: {names:?}");
    for expected in ["tenantid", "code", "user_id"] {
        assert!(names.contains(expected), "missing {expected:?} in {names:?}");
    }
    assert!(!names.contains("id"), "an unindexed column must not appear");
}

#[test]
fn indexed_column_names_is_empty_without_indexes() {
    assert!(indexed_column_names(&relation()).is_empty());
}

// ---------------------------------------------------------------------------
// unique_column_sets
// ---------------------------------------------------------------------------

#[test]
fn unique_column_sets_are_the_pk_then_unique_indexes_in_order() {
    let mut relation = relation();
    relation.indexes = vec![
        index("idx_plain", &["total"], false, false),
        index("uq_code", &["tenant_id", "code"], true, false),
        index("uq_region", &["region"], true, false),
        index("uq_empty", &[], true, false),
    ];
    assert_eq!(
        unique_column_sets(&relation),
        vec![
            vec!["id".to_owned()],
            vec!["tenant_id".to_owned(), "code".to_owned()],
            vec!["region".to_owned()],
        ]
    );
}

#[test]
fn unique_column_sets_keeps_the_primary_key_spelling_verbatim() {
    let mut relation = relation();
    relation.primary_key = vec!["ID".to_owned()];
    relation.indexes = vec![];
    assert_eq!(unique_column_sets(&relation), vec![vec!["ID".to_owned()]]);
}

#[test]
fn unique_column_sets_is_empty_without_a_pk_or_unique_index() {
    let mut relation = relation();
    relation.primary_key = vec![];
    relation.indexes = vec![index("idx_plain", &["total"], false, false)];
    assert!(unique_column_sets(&relation).is_empty());
}

// ---------------------------------------------------------------------------
// declared_foreign_keys
// ---------------------------------------------------------------------------

#[test]
fn declared_foreign_keys_map_every_field() {
    let mut relation = relation();
    relation.foreign_keys = vec![foreign_key(
        "fk_orders_user",
        &["user_id"],
        "users",
        &["id"],
    )];
    assert_eq!(
        declared_foreign_keys(&relation),
        vec![DeclaredForeignKey {
            columns: vec!["user_id".to_owned()],
            referenced_table: "users".to_owned(),
            referenced_columns: vec!["id".to_owned()],
        }]
    );
}

// ---------------------------------------------------------------------------
// to_prediction_table
// ---------------------------------------------------------------------------

#[test]
fn to_prediction_table_carries_the_bare_name_and_schema() {
    let table = to_prediction_table("public.orders", &relation(), Some("public"));
    assert_eq!(table.id, "public.orders");
    assert_eq!(table.name, "orders");
    assert_eq!(table.schema.as_deref(), Some("public"));
}

#[test]
fn to_prediction_table_marks_only_indexed_columns_and_folds_case() {
    let mut relation = relation();
    relation.columns = vec![
        column("id", "integer", false),
        column("USER_ID", "integer", false),
        column("Code", "varchar", true),
    ];
    relation.indexes = vec![
        index("idx_user", &["user_id"], false, false),
        index("uq_code", &["CODE"], true, false),
    ];
    let table = to_prediction_table("orders", &relation, None);
    let by_name: std::collections::HashMap<&str, &PredictionColumn> =
        table.columns.iter().map(|c| (c.name.as_str(), c)).collect();
    assert_eq!(by_name["USER_ID"].indexed, Some(true), "column case differs from index case");
    assert_eq!(by_name["Code"].indexed, Some(true));
    assert_eq!(by_name["id"].indexed, Some(false));
    assert_eq!(by_name["id"].nullable, Some(false));
    assert_eq!(by_name["Code"].nullable, Some(true));
    assert_eq!(by_name["USER_ID"].data_type, "integer");
}

#[test]
fn to_prediction_table_collects_pk_unique_sets_and_declared_fks() {
    let mut relation = relation();
    relation.indexes = vec![
        index("idx_plain", &["total"], false, false),
        index("uq_code", &["tenant_id", "code"], true, false),
    ];
    relation.foreign_keys = vec![foreign_key(
        "fk_orders_user",
        &["user_id"],
        "users",
        &["id"],
    )];
    let table = to_prediction_table("orders", &relation, None);
    assert_eq!(table.primary_key, vec!["id".to_owned()]);
    assert_eq!(
        table.unique_column_sets,
        vec![
            vec!["id".to_owned()],
            vec!["tenant_id".to_owned(), "code".to_owned()],
        ]
    );
    assert_eq!(
        table.declared_foreign_keys,
        vec![DeclaredForeignKey {
            columns: vec!["user_id".to_owned()],
            referenced_table: "users".to_owned(),
            referenced_columns: vec!["id".to_owned()],
        }]
    );
}

#[test]
fn to_prediction_table_without_a_pk_has_no_key_set() {
    let mut relation = relation();
    relation.primary_key = vec![];
    let table = to_prediction_table("orders", &relation, None);
    assert!(table.primary_key.is_empty());
    assert!(table.unique_column_sets.is_empty());
}

#[test]
fn to_prediction_table_with_empty_columns_keeps_name_and_schema() {
    let mut relation = relation();
    relation.columns = vec![];
    let table = to_prediction_table("orders", &relation, Some("public"));
    assert!(table.columns.is_empty());
    assert_eq!(table.name, "orders");
    assert_eq!(table.schema.as_deref(), Some("public"));
    assert_eq!(table.primary_key, vec!["id".to_owned()]);
}

#[test]
fn to_prediction_table_copies_instead_of_aliasing_metadata() {
    let source = relation();
    let mut table = to_prediction_table("orders", &source, None);
    table.columns[0].name = "mutated".to_owned();
    table.primary_key.push("extra".to_owned());
    assert_eq!(source.columns[0].name, "id", "the cache's column list is untouched");
    assert_eq!(
        source.primary_key,
        vec!["id".to_owned()],
        "the cache's primary key is untouched"
    );
}

// ---------------------------------------------------------------------------
// to_prediction_tables
// ---------------------------------------------------------------------------

#[test]
fn to_prediction_tables_preserves_insertion_order_and_applies_schema_per_id() {
    let mut relations = OrderedMap::new();
    relations.insert("public.orders".to_owned(), relation());
    let mut users = relation();
    users.identity = QualifiedRelationId::qualified(&["sales"], "users");
    relations.insert("sales.users".to_owned(), users);

    let schema_of = |id: &str| -> Option<String> {
        if id.starts_with("sales.") {
            Some("sales".to_owned())
        } else {
            Some("public".to_owned())
        }
    };
    let tables = to_prediction_tables(&relations, Some(&schema_of));
    let rendered: Vec<String> = tables
        .iter()
        .map(|t| format!("{}@{}", t.id, t.schema.as_deref().unwrap_or("")))
        .collect();
    assert_eq!(rendered, vec!["public.orders@public", "sales.users@sales"]);
}

#[test]
fn to_prediction_tables_applies_schema_of_per_identity_and_allows_none() {
    let mut relations = OrderedMap::new();
    relations.insert("public.orders".to_owned(), relation());
    let mut users = relation();
    users.identity = QualifiedRelationId::qualified(&["sales"], "users");
    relations.insert("sales.users".to_owned(), users);

    let schema_of = |id: &str| -> Option<String> {
        (id == "sales.users").then(|| "sales".to_owned())
    };
    let tables = to_prediction_tables(&relations, Some(&schema_of));
    assert_eq!(tables[0].schema, None, "an identity with no schema stays unset");
    assert_eq!(tables[1].schema.as_deref(), Some("sales"));
}

#[test]
fn to_prediction_tables_without_schema_of_leaves_every_schema_none() {
    let mut relations = OrderedMap::new();
    relations.insert("orders".to_owned(), relation());
    let tables = to_prediction_tables(&relations, None);
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].id, "orders");
    assert!(tables[0].schema.is_none());
}

// ---------------------------------------------------------------------------
// Gap fill: sessionConfig.ts / types.ts contracts T032 left untested
// ---------------------------------------------------------------------------

#[test]
fn unbound_session_config_is_empty_connection_and_zero_revision() {
    assert_eq!(
        unbound_session_config(),
        EditorSessionConfig {
            connection_id: String::new(),
            config_revision: 0,
        }
    );
    assert_eq!(
        editor_session_cache_key("s1", &unbound_session_config()),
        r#"["",0,"s1"]"#
    );
}

#[test]
fn partition_key_uses_the_resolved_config_like_the_react_default_argument() {
    let resolver = |sid: &str| -> Option<EditorSessionConfig> {
        if sid == "s1" {
            Some(EditorSessionConfig {
                connection_id: "conn-9".to_owned(),
                config_revision: 4,
            })
        } else {
            None
        }
    };
    let resolved = resolve_editor_session_config_with(&resolver, "s1");
    assert_eq!(
        resolved,
        EditorSessionConfig {
            connection_id: "conn-9".to_owned(),
            config_revision: 4,
        }
    );
    assert_eq!(
        editor_session_cache_key("s1", &resolved),
        r#"["conn-9",4,"s1"]"#
    );
    assert_eq!(
        editor_session_cache_key("s1", &resolved),
        metadata_session_key(
            &EditorSessionConfig {
                connection_id: "conn-9".to_owned(),
                config_revision: 4,
            },
            "s1",
        ),
        "the partition address and the per-relation session prefix agree"
    );
    // React's `?? UNBOUND_SESSION_CONFIG` fallback, through the injected resolver.
    assert_eq!(
        resolve_editor_session_config_with(&resolver, "unknown"),
        unbound_session_config()
    );
}

#[test]
fn editor_relation_source_kind_is_the_sql_source_kind_contract() {
    let kind: EditorRelationSourceKind = EditorRelationSourceKind::View;
    assert_eq!(kind, SqlRelationSourceKind::View);
    let request = EditorRelationRequest::new(QualifiedRelationId::bare("users"), kind);
    assert_eq!(request.kind, Some(SqlRelationSourceKind::View));
}

#[test]
fn editor_metadata_context_default_is_empty() {
    let ctx = EditorMetadataContext::default();
    assert!(ctx.database.is_empty());
    assert!(ctx.schema.is_none());
    assert!(ctx.dialect_id.is_empty());
}

#[test]
fn editor_metadata_snapshot_looks_relations_up_by_key() {
    let mut relations = BTreeMap::new();
    relations.insert("s1::public.orders".to_owned(), relation());
    let snapshot = EditorMetadataSnapshot {
        db_session_id: "s1".to_owned(),
        database: "demo".to_owned(),
        schema: Some("public".to_owned()),
        epoch: 3,
        relations,
    };
    assert_eq!(snapshot.relations().len(), 1);
    assert!(snapshot.relation("s1::public.orders").is_some());
    assert!(snapshot.relation("s1::public.users").is_none());
}
