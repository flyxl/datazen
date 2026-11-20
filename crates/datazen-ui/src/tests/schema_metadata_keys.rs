//! T412: the schema-store metadata identity keys — the four-dimension
//! identity constructor, the session-identity struct, and the relation
//! columns cache key that routes a relation through the canonical
//! `metadataCacheKey` spelling.

use crate::schema_metadata_keys::{
    identity_for_session, relation_columns_cache_key, RelationRef, SessionMetadataIdentity,
};
use datazen_ui_api::{
    metadata_cache_key, ExecutionTarget, MetadataTarget, NamespaceTarget, ObjectTarget,
};

fn session(connection_id: &str, metadata_revision: u64) -> SessionMetadataIdentity {
    SessionMetadataIdentity {
        connection_id: connection_id.to_string(),
        metadata_revision,
    }
}

fn relation(database: &str, schema: Option<&str>, name: &str) -> RelationRef {
    RelationRef {
        database: database.to_string(),
        schema: schema.map(str::to_string),
        name: name.to_string(),
    }
}

#[test]
fn identity_for_session_carries_the_four_dimensions() {
    let target = MetadataTarget::Namespace(NamespaceTarget::new(
        Some("app".to_string()),
        None,
        Some("public".to_string()),
        vec!["a".to_string()],
    ));
    let identity = identity_for_session(
        "conn-1".to_string(),
        7,
        "sess-9".to_string(),
        target.clone(),
    );
    assert_eq!(identity.connection_id, "conn-1");
    assert_eq!(identity.config_revision, 7);
    assert_eq!(identity.db_session_id, "sess-9");
    assert_eq!(identity.target, target);
}

#[test]
fn the_relation_columns_key_pins_the_json_stringify_form() {
    // Literal captured from Node's JSON.stringify on the equivalent React
    // input: session conn-1@7, session sess-9, relation app.public/users.
    // Catalog is null (fixed by the React key), the path is empty, and the
    // object is ["relation", name, null] — signature undetermined.
    let key = relation_columns_cache_key(&session("conn-1", 7), "sess-9", &relation(
        "app", Some("public"), "users",
    ));
    assert_eq!(
        key,
        r#"["conn-1",7,"sess-9","app",null,"public",[],["relation","users",null]]"#
    );
}

#[test]
fn a_null_schema_serializes_as_null_not_the_empty_string() {
    // The SDK allows schema: null; the key must spell JSON null, and the
    // database stays a required string (React's `?? null` never fires).
    let key = relation_columns_cache_key(&session("c", 0), "s", &relation("db", None, "t"));
    assert_eq!(
        key,
        r#"["c",0,"s","db",null,null,[],["relation","t",null]]"#
    );
}

#[test]
fn every_dimension_isolates_the_columns_key() {
    let base = relation_columns_cache_key(&session("conn-1", 7), "sess-9", &relation(
        "app", Some("public"), "users",
    ));

    // identity: which persistent config the session belongs to.
    let key = relation_columns_cache_key(&session("conn-2", 7), "sess-9", &relation(
        "app", Some("public"), "users",
    ));
    assert_ne!(key, base);

    // configuration: which revision of that config the session saw.
    let key = relation_columns_cache_key(&session("conn-1", 8), "sess-9", &relation(
        "app", Some("public"), "users",
    ));
    assert_ne!(key, base);

    // runtime session.
    let key = relation_columns_cache_key(&session("conn-1", 7), "sess-10", &relation(
        "app", Some("public"), "users",
    ));
    assert_ne!(key, base);

    // target: the database layer.
    let key = relation_columns_cache_key(&session("conn-1", 7), "sess-9", &relation(
        "other", Some("public"), "users",
    ));
    assert_ne!(key, base);

    // target: the schema layer — including its null boundary.
    let key = relation_columns_cache_key(&session("conn-1", 7), "sess-9", &relation(
        "app", None, "users",
    ));
    assert_ne!(key, base);

    // target: the relation name itself.
    let key = relation_columns_cache_key(&session("conn-1", 7), "sess-9", &relation(
        "app", Some("public"), "events",
    ));
    assert_ne!(key, base);
}

#[test]
fn the_columns_key_is_the_canonical_metadata_cache_key() {
    // Single-derivation check: the store-side key must equal the plain
    // `metadataCacheKey` of the equivalent hand-built identity, so the
    // schema store cannot fork a second spelling of the same address.
    let session_identity = session("conn-1", 7);
    let relation_ref = relation("app", Some("public"), "users");
    let hand_built = identity_for_session(
        "conn-1".to_string(),
        7,
        "sess-9".to_string(),
        MetadataTarget::Execution(ExecutionTarget {
            connection_id: "conn-1".to_string(),
            namespace: NamespaceTarget::new(
                Some("app".to_string()),
                None,
                Some("public".to_string()),
                Vec::new(),
            ),
            object: Some(ObjectTarget::new("relation", "users")),
        }),
    );
    assert_eq!(
        relation_columns_cache_key(&session_identity, "sess-9", &relation_ref),
        metadata_cache_key(&hand_built)
    );
    // The relation object's undetermined signature is `null`, never a
    // string: the ObjectTarget builder leaves it `None`.
    assert_eq!(hand_built.target, MetadataTarget::Execution(ExecutionTarget {
        connection_id: "conn-1".to_string(),
        namespace: NamespaceTarget::new(
            Some("app".to_string()),
            None,
            Some("public".to_string()),
            Vec::new(),
        ),
        object: Some(ObjectTarget::new("relation", "users")),
    }));
}
