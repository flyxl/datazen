//! T242: real assertions for the Rust port of
//! `packages/frontend/src/components/sql-editor/metadata/findRelation.ts`.
//!
//! Registered in `tests/mod.rs` as an ordinary test module, so its test
//! names are `tests::sql_editor_metadata::*` and the `sql_editor_metadata`
//! filter really runs it. The re-export shims (`types.ts`,
//! `metadataCache.ts`) are re-exports of already-tested contracts
//! (`tests::metadata_cache`), so only `findRelation` needs behavior here —
//! React pins it through the schemaCompletion suites instead of a
//! dedicated file.

use std::collections::BTreeMap;

use datazen_ui::components::sql_editor::metadata::{
    extract_columns_from_schema, find_relation_metadata,
};
use datazen_ui::components::sql_editor::sql_namespace::SqlNamespace;
use datazen_ui::relation_metadata::*;

/* -------------------------------------------------------------------------- */
/*  Fixtures                                                                  */
/* -------------------------------------------------------------------------- */

fn ident(namespace: &[&str], name: &str) -> QualifiedRelationId {
    QualifiedRelationId::qualified(namespace, name)
}

fn relation_meta(identity: QualifiedRelationId, columns: &[(&str, &str)]) -> EditorRelationMetadata {
    EditorRelationMetadata {
        key: relation_identity_key(&identity, "standard"),
        columns: columns
            .iter()
            .map(|(name, ty)| ColumnSchema {
                name: (*name).to_owned(),
                data_type: (*ty).to_owned(),
                nullable: false,
                ..Default::default()
            })
            .collect(),
        identity,
        kind: EditorRelationKind::Table,
        primary_key: Vec::new(),
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
        comment: None,
        loaded_at: 0,
    }
}

fn snapshot_with(
    db_session_id: &str,
    entries: Vec<(String, EditorRelationMetadata)>,
) -> EditorMetadataSnapshot {
    let mut relations = BTreeMap::new();
    for (key, rel) in entries {
        relations.insert(key, rel);
    }
    EditorMetadataSnapshot {
        db_session_id: db_session_id.to_owned(),
        database: "test".to_owned(),
        schema: None,
        epoch: 1,
        relations,
    }
}

fn users_meta() -> EditorRelationMetadata {
    relation_meta(
        ident(&[], "users"),
        &[("id", "integer"), ("name", "varchar")],
    )
}

/* -------------------------------------------------------------------------- */
/*  Tier 1: direct folded-key match                                           */
/* -------------------------------------------------------------------------- */

#[test]
fn tier1_direct_folded_key_match() {
    let public_users = relation_meta(
        ident(&["public"], "users"),
        &[("id", "integer"), ("email", "varchar")],
    );
    let snapshot = snapshot_with("test-session", vec![(public_users.key.clone(), public_users)]);

    let found = find_relation_metadata(
        Some(&snapshot),
        &ident(&["public"], "users"),
        "standard",
        None,
    );
    assert_eq!(found.as_ref().map(|r| r.columns.len()), Some(2));
    assert_eq!(
        found.unwrap().identity.name.name, "users",
        "tier 1 returns the relation the folded key addresses"
    );
}

/* -------------------------------------------------------------------------- */
/*  Tier 2: dbSessionId::key match                                            */
/* -------------------------------------------------------------------------- */

#[test]
fn tier2_prefixed_session_key_match() {
    let users = users_meta();
    let prefixed_key = format!("test-session::{}", users.key);
    let snapshot = snapshot_with("test-session", vec![(prefixed_key, users)]);

    let found = find_relation_metadata(Some(&snapshot), &ident(&[], "users"), "standard", None);
    assert_eq!(
        found.as_ref().map(|r| r.columns.len()),
        Some(2),
        "the cache's canonical `dbSessionId::key` shape resolves the same relation"
    );
}

/* -------------------------------------------------------------------------- */
/*  Tier 3: case-insensitive raw-key and name matches                         */
/* -------------------------------------------------------------------------- */

#[test]
fn tier3_case_insensitive_raw_key_match() {
    // Standard folds lower, so "USERS" is not the folded key — tier 3 must
    // catch it by lowercasing the raw key.
    let users = users_meta();
    let snapshot = snapshot_with("test-session", vec![("USERS".to_owned(), users)]);

    let found = find_relation_metadata(Some(&snapshot), &ident(&[], "users"), "standard", None);
    assert_eq!(
        found.as_ref().map(|r| r.identity.name.name.as_str()),
        Some("users"),
        "a differently-cased raw key still resolves case-insensitively"
    );
}

#[test]
fn tier3_case_insensitive_table_name_match() {
    // Raw key shares no case with the folded key at all; only the table
    // name matches case-insensitively.
    let users = relation_meta(
        ident(&["core"], "Users"),
        &[("id", "integer")],
    );
    let snapshot = snapshot_with("test-session", vec![("misc.entry".to_owned(), users)]);

    let found = find_relation_metadata(Some(&snapshot), &ident(&[], "USERS"), "standard", None);
    assert!(
        found.is_some(),
        "the table name alone (case-insensitive) is a tier-3 match"
    );
}

#[test]
fn tier3_raw_key_match_without_a_name_match() {
    // The map key IS the lookup's folded key modulo case, while the stored
    // relation's name differs — only the raw-key leg of tier 3 can hit.
    let accounts = relation_meta(
        ident(&[], "accounts"),
        &[("account_id", "integer")],
    );
    let snapshot = snapshot_with("test-session", vec![("USERS".to_owned(), accounts)]);

    let found = find_relation_metadata(Some(&snapshot), &ident(&[], "users"), "standard", None);
    assert_eq!(
        found.as_ref().map(|r| r.identity.name.name.as_str()),
        Some("accounts"),
        "a case-drifted raw key resolves even when the stored name differs"
    );
}

#[test]
fn tier3_raw_key_match_strips_the_session_prefix() {
    // Same case-drifted raw key, now behind a `dbSessionId::` prefix: the
    // raw-key leg must compare the part AFTER the marker (and tier 2's
    // exact form missed on case).
    let accounts = relation_meta(
        ident(&[], "accounts"),
        &[("account_id", "integer")],
    );
    let snapshot = snapshot_with(
        "test-session",
        vec![("test-session::USERS".to_owned(), accounts)],
    );

    let found = find_relation_metadata(Some(&snapshot), &ident(&[], "users"), "standard", None);
    assert_eq!(
        found.as_ref().map(|r| r.identity.name.name.as_str()),
        Some("accounts"),
        "the session prefix is stripped before the case-insensitive compare"
    );
}

/* -------------------------------------------------------------------------- */
/*  Tier 4: schema-tree synthesis                                             */
/* -------------------------------------------------------------------------- */

#[test]
fn tier4_synthesizes_from_the_editor_schema_tree() {
    let tree = SqlNamespace::branch([
        ("main", SqlNamespace::branch([("users", SqlNamespace::table(["id", "name"]))])),
    ]);

    let found = find_relation_metadata(None, &ident(&[], "users"), "standard", Some(&tree));
    let rel = found.expect("no snapshot: the tree fallback synthesizes the relation");
    assert_eq!(rel.kind, EditorRelationKind::Table);
    assert_eq!(rel.key, "users", "the synthesized entry is keyed by the raw relation name");
    assert_eq!(
        rel.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["id", "name"],
        "columns come from the tree leaf in tree order"
    );
    assert!(
        rel.columns.iter().all(|c| c.data_type == "text" && c.nullable),
        "tree synthesis degrades every column to text/nullable"
    );
    assert_eq!(rel.primary_key, vec!["id".to_owned()]);
    assert!(
        rel.columns[0].is_primary_key && !rel.columns[1].is_primary_key,
        "the primary-key guess is the leading `id` column only"
    );
    assert!(rel.loaded_at > 0, "loadedAt is stamped with Date.now()");
}

#[test]
fn tier4_leading_id_is_the_only_primary_key_guess() {
    // `id` present but NOT leading: the guess stays false while the
    // primaryKey list still collects it — both React behaviors.
    let tree = SqlNamespace::branch([(
        "t",
        SqlNamespace::table(["name", "id", "email"]),
    )]);

    let found = find_relation_metadata(None, &ident(&[], "t"), "standard", Some(&tree));
    let rel = found.unwrap();
    assert!(rel.columns.iter().all(|c| !c.is_primary_key));
    assert_eq!(rel.primary_key, vec!["id".to_owned()]);
}

#[test]
fn tier4_empty_leaf_does_not_synthesize() {
    // React: `cols && cols.length > 0` — an empty table leaf must NOT
    // short-circuit as a hit; the resolver keeps returning None.
    let tree = SqlNamespace::branch([("users", SqlNamespace::empty_table())]);
    assert!(
        find_relation_metadata(None, &ident(&[], "users"), "standard", Some(&tree)).is_none(),
        "an empty table leaf is no metadata"
    );
}

#[test]
fn no_match_anywhere_returns_none() {
    let snapshot = snapshot_with("test-session", vec![(users_meta().key, users_meta())]);
    assert!(
        find_relation_metadata(Some(&snapshot), &ident(&[], "ghost"), "standard", None).is_none()
    );
}

#[test]
fn snapshot_relations_empty_skips_to_the_tree() {
    let empty = snapshot_with("test-session", vec![]);
    let tree = SqlNamespace::branch([("orders", SqlNamespace::table(["id"]))]);
    assert!(
        find_relation_metadata(Some(&empty), &ident(&[], "orders"), "standard", Some(&tree))
            .is_some(),
        "an empty snapshot must not block the tree fallback"
    );
}

/* -------------------------------------------------------------------------- */
/*  extractColumnsFromSchema                                                  */
/* -------------------------------------------------------------------------- */

#[test]
fn extract_finds_flat_and_nested_leaves_case_insensitively() {
    let tree = SqlNamespace::branch([
        ("Users", SqlNamespace::table(["id", "name"])),
        (
            "shop",
            SqlNamespace::branch([("buyer", SqlNamespace::table(["buyer_id"]))]),
        ),
    ]);
    assert_eq!(
        extract_columns_from_schema(&tree, "users"),
        Some(["id".to_owned(), "name".to_owned()].as_slice()),
        "flat leaves match case-insensitively"
    );
    assert_eq!(
        extract_columns_from_schema(&tree, "BUYER"),
        Some(["buyer_id".to_owned()].as_slice()),
        "nested leaves match case-insensitively"
    );
}

#[test]
fn extract_returns_none_for_a_namespace_with_that_name() {
    // A branch keyed like the table is NOT a table leaf: the search must
    // keep walking (and here, find nothing).
    let tree = SqlNamespace::branch([(
        "users",
        SqlNamespace::branch([("other", SqlNamespace::table(["x"]))]),
    )]);
    assert_eq!(extract_columns_from_schema(&tree, "users"), None);
}

#[test]
fn extract_on_a_leaf_root_returns_none() {
    let leaf = SqlNamespace::table(["id"]);
    assert_eq!(extract_columns_from_schema(&leaf, "id"), None);
}
