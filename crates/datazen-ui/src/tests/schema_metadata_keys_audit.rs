//! T412 audit (tester): independent checks of the schema-store metadata
//! identity keys. The coder's tests pin forward string literals; this audit
//! observes through a different channel — the produced key is parsed back
//! with `serde_json` and its array structure is asserted dimension by
//! dimension, so a spelling mistake on either side of the comparison cannot
//! cancel out. Literals below were re-derived from the React source by the
//! tester, not copied from the coder's file.

use crate::schema_metadata_keys::{
    identity_for_session, relation_columns_cache_key, RelationRef, SessionMetadataIdentity,
};
use datazen_ui_api::{
    metadata_cache_key, ExecutionTarget, MetadataTarget, NamespaceTarget, ObjectTarget,
};
use serde_json::Value;

fn audit_session(connection_id: &str, metadata_revision: u64) -> SessionMetadataIdentity {
    SessionMetadataIdentity {
        connection_id: connection_id.to_string(),
        metadata_revision,
    }
}

fn audit_relation(database: &str, schema: Option<&str>, name: &str) -> RelationRef {
    RelationRef {
        database: database.to_string(),
        schema: schema.map(str::to_string),
        name: name.to_string(),
    }
}

/// Decode the key and return the top-level array; the key must parse as a
/// flat JSON array for these shapes (the `metadataCacheKey` contract).
fn decode(key: &str) -> Vec<Value> {
    match serde_json::from_str::<Value>(key) {
        Ok(Value::Array(parts)) => parts,
        other => panic!("key is not a flat JSON array: {other:?}"),
    }
}

#[test]
fn audit_the_key_structure_dimension_by_dimension() {
    let key = relation_columns_cache_key(
        &audit_session("conn-1", 7),
        "sess-9",
        &audit_relation("app", Some("public"), "users"),
    );
    let parts = decode(&key);
    // [connectionId, configRevision, dbSessionId, database, catalog,
    //  schema, path, [kind, name, signature]] — 8 elements.
    assert_eq!(parts.len(), 8, "the key carries exactly 8 dimensions");
    assert_eq!(parts[0], Value::String("conn-1".to_string()), "identity: connectionId");
    assert_eq!(parts[1], Value::Number(7.into()), "configuration: bare JSON number, not a string");
    assert_eq!(parts[2], Value::String("sess-9".to_string()), "runtime session");
    assert_eq!(parts[3], Value::String("app".to_string()), "target: database");
    assert_eq!(parts[4], Value::Null, "target: catalog is null by construction");
    assert_eq!(parts[5], Value::String("public".to_string()), "target: schema");
    assert_eq!(
        parts[6],
        Value::Array(vec![]),
        "target: the path is an empty array, not null"
    );
    assert_eq!(
        parts[7],
        Value::Array(vec![
            Value::String("relation".to_string()),
            Value::String("users".to_string()),
            Value::Null,
        ]),
        "target: [kind, name, signature=null]"
    );
}

#[test]
fn audit_a_null_schema_is_json_null() {
    let key = relation_columns_cache_key(
        &audit_session("c", 0),
        "s",
        &audit_relation("db", None, "t"),
    );
    let parts = decode(&key);
    assert_eq!(parts[3], Value::String("db".to_string()), "database stays a required string");
    assert_eq!(parts[5], Value::Null, "schema: null, not the empty string");
    assert_eq!(
        parts[7],
        Value::Array(vec![
            Value::String("relation".to_string()),
            Value::String("t".to_string()),
            Value::Null,
        ])
    );
    // The whole-key literal, re-derived by the tester from the React form.
    assert_eq!(key, r#"["c",0,"s","db",null,null,[],["relation","t",null]]"#);
}

#[test]
fn audit_two_relations_in_one_session_never_share_a_key() {
    // The P4 contract in the store's own words: two relations of one
    // session/config differ only in the object triple.
    let first = relation_columns_cache_key(
        &audit_session("conn-1", 7),
        "sess-9",
        &audit_relation("app", Some("public"), "users"),
    );
    let second = relation_columns_cache_key(
        &audit_session("conn-1", 7),
        "sess-9",
        &audit_relation("app", Some("public"), "events"),
    );
    assert_ne!(first, second);
    // …and the two keys agree on every prefix except the object.
    let first_parts = decode(&first);
    let second_parts = decode(&second);
    assert_eq!(first_parts[..7], second_parts[..7]);
    assert_ne!(first_parts[7], second_parts[7]);
}

#[test]
fn audit_the_key_agrees_with_a_hand_built_identity() {
    // Independent cross-derivation: build the identity through the api
    // crate directly (not through the port's constructor) and require the
    // same string — the store port must not fork the key spelling.
    let hand_built = MetadataTarget::Execution(ExecutionTarget {
        connection_id: "ignored-by-the-derivation".to_string(),
        namespace: NamespaceTarget::new(
            Some("app".to_string()),
            None,
            Some("public".to_string()),
            Vec::new(),
        ),
        object: Some(ObjectTarget::new("relation", "users")),
    });
    let via_api = metadata_cache_key(&identity_for_session(
        "conn-1".to_string(),
        7,
        "sess-9".to_string(),
        hand_built.clone(),
    ));
    let via_port = relation_columns_cache_key(
        &audit_session("conn-1", 7),
        "sess-9",
        &audit_relation("app", Some("public"), "users"),
    );
    assert_eq!(via_api, via_port);
    // The target struct itself round-trips through the constructor.
    assert_eq!(
        identity_for_session("c".to_string(), 0, "s".to_string(), hand_built.clone()).target,
        hand_built
    );
}
