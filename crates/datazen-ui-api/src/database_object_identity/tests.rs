use super::*;
use crate::DatabaseObjectKind;

fn object() -> DatabaseObject {
    DatabaseObject {
        kind: DatabaseObjectKind::Function,
        schema: None,
        name: "f".to_owned(),
        signature: None,
        target_schema: None,
        target_name: None,
    }
}

#[test]
fn the_key_is_the_six_leg_json_array() {
    assert_eq!(
        database_object_identity_key(&object()),
        "[\"function\",null,\"f\",null,null,null]"
    );
}

#[test]
fn every_leg_changes_the_identity() {
    let base = database_object_identity_key(&object());

    let mut schema = object();
    schema.schema = Some("public".to_owned());
    assert_eq!(
        database_object_identity_key(&schema),
        "[\"function\",\"public\",\"f\",null,null,null]"
    );
    assert_ne!(database_object_identity_key(&schema), base);

    let mut name = object();
    name.name = "g".to_owned();
    assert_ne!(database_object_identity_key(&name), base);

    let mut signature = object();
    signature.signature = Some("(int)".to_owned());
    assert_eq!(
        database_object_identity_key(&signature),
        "[\"function\",null,\"f\",\"(int)\",null,null]"
    );

    let mut target = object();
    target.target_schema = Some("public".to_owned());
    target.target_name = Some("t".to_owned());
    assert_eq!(
        database_object_identity_key(&target),
        "[\"function\",null,\"f\",null,\"public\",\"t\"]"
    );

    let mut kind = object();
    kind.kind = DatabaseObjectKind::Procedure;
    assert_eq!(
        database_object_identity_key(&kind),
        "[\"procedure\",null,\"f\",null,null,null]"
    );
}

#[test]
fn absent_and_null_optional_legs_collapse() {
    let mut explicit_nulls = object();
    explicit_nulls.schema = None;
    explicit_nulls.signature = None;
    explicit_nulls.target_schema = None;
    explicit_nulls.target_name = None;
    assert_eq!(
        database_object_identity_key(&explicit_nulls),
        database_object_identity_key(&object())
    );
}

#[test]
fn crate_root_reexports_the_identity_key() {
    assert_eq!(
        crate::database_object_identity_key(&object()),
        "[\"function\",null,\"f\",null,null,null]"
    );
}
