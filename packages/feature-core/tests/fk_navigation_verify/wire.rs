//! The wire shape of the DTOs.
//!
//! Part of the DB-07 verification harness; the shared fixtures live in
//! the parent module.

use super::*;

// ─────────────────────────────────────────────────────────────────────────────
// D 组 · the wire shape of the DTOs
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_resolution_serializes_to_the_documented_camel_case_shape() {
    let driver = single_column_driver(vec![vec![Some(Value::Integer(7))]]);
    let resolution = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::Integer(7)]),
    )
    .await
    .expect("the probe resolves");

    let json = serde_json::to_value(&resolution).expect("the resolution serializes");
    let target = json["target"].as_object().expect("the target is an object");

    // The key set of the target, field by field.
    let mut keys: Vec<&str> = target.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "candidateLimit",
            "columns",
            "dataTypes",
            "database",
            "primaryKeys",
            "schema",
            "table",
            "unique",
        ]
    );

    let mut top: Vec<&str> = json
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    top.sort_unstable();
    assert_eq!(top, vec!["candidates", "exists", "target", "truncated"]);

    assert_eq!(target["database"], serde_json::json!(DATABASE));
    assert_eq!(target["table"], serde_json::json!("parents"));
    assert_eq!(target["columns"], serde_json::json!(["id"]));
    assert_eq!(target["dataTypes"], serde_json::json!(["integer"]));
    assert_eq!(target["primaryKeys"], serde_json::json!(["id"]));
    assert_eq!(target["unique"], serde_json::json!(true));
    assert_eq!(target["candidateLimit"], serde_json::json!(50));
    assert_eq!(json["exists"], serde_json::json!(true));
    assert_eq!(json["truncated"], serde_json::json!(false));
    assert_eq!(json["candidates"], serde_json::json!([[7]]));

    // The **declaration** order is what a struct serialization puts on the
    // wire (`serde_json::Value` sorts its keys, so the order is read off the
    // serialized text instead).
    let text = serde_json::to_string(&resolution).expect("the resolution serializes");
    let target_start = text.find("\"target\":").expect("the target is present");
    let target_end = text.find("},").expect("the target object closes");
    let target_text = &text[target_start..target_end];
    for key in [
        "\"database\"",
        "\"schema\"",
        "\"table\"",
        "\"columns\"",
        "\"dataTypes\"",
        "\"primaryKeys\"",
        "\"unique\"",
        "\"candidateLimit\"",
    ] {
        assert!(
            target_text.contains(key),
            "the target must carry {key}: {target_text}"
        );
    }
    let mut previous = 0usize;
    for key in [
        "\"database\"",
        "\"schema\"",
        "\"table\"",
        "\"columns\"",
        "\"dataTypes\"",
        "\"primaryKeys\"",
        "\"unique\"",
        "\"candidateLimit\"",
    ] {
        let position = target_text.find(key).unwrap_or_else(|| {
            panic!("the serialized target must carry {key}: {target_text}");
        });
        assert!(
            position >= previous,
            "the target fields must keep their declaration order: {target_text}"
        );
        previous = position;
    }

    // The top level: `target`, `exists`, `candidates`, `truncated`.
    let tail = &text[target_end..];
    let mut previous = 0usize;
    for key in ["\"exists\"", "\"candidates\"", "\"truncated\""] {
        let position = tail.find(key).unwrap_or_else(|| {
            panic!("the serialized resolution must carry {key}: {tail}");
        });
        assert!(
            position >= previous,
            "the top-level fields must keep their declaration order: {tail}"
        );
        previous = position;
    }
}

#[test]
fn the_request_deserializes_from_the_documented_camel_case_shape() {
    let json = serde_json::json!({
        "dbSessionId": "sess-1",
        "database": "app",
        "schema": "public",
        "table": "orders",
        "qualifiedTable": "public.orders",
        "columns": ["customer_id"],
        "constraintName": "orders_customer_fk",
        "values": [7],
        "limit": 10,
    });
    let request: ResolveForeignKeyTargetsRequest =
        serde_json::from_value(json).expect("the request deserializes");
    assert_eq!(request.db_session_id, "sess-1");
    assert_eq!(request.database.as_deref(), Some("app"));
    assert_eq!(request.schema.as_deref(), Some("public"));
    assert_eq!(request.table, "orders");
    assert_eq!(request.qualified_table.as_deref(), Some("public.orders"));
    assert_eq!(request.columns, vec!["customer_id".to_string()]);
    assert_eq!(
        request.constraint_name.as_deref(),
        Some("orders_customer_fk")
    );
    assert_eq!(request.values.len(), 1);
    assert_eq!(request.limit, Some(10));

    // Every optional field may be omitted.
    let minimal = serde_json::json!({
        "dbSessionId": "sess-1",
        "table": "orders",
        "columns": ["customer_id"],
        "values": [7],
    });
    let request: ResolveForeignKeyTargetsRequest =
        serde_json::from_value(minimal).expect("the minimal request deserializes");
    assert!(request.database.is_none());
    assert!(request.schema.is_none());
    assert!(request.qualified_table.is_none());
    assert!(request.constraint_name.is_none());
    assert!(request.limit.is_none());
}

#[test]
fn a_json_null_value_reaches_the_resolver_as_a_null_value() {
    // The wire carries a plain `null`; the untagged `Value` enum must decode
    // it into the `Null` variant, or the NULL guard would never fire.
    let json = serde_json::json!({
        "dbSessionId": "sess-1",
        "table": "orders",
        "columns": ["customer_id"],
        "values": [null],
    });
    let request: ResolveForeignKeyTargetsRequest =
        serde_json::from_value(json).expect("the request deserializes");
    assert!(matches!(request.values[0], Value::Null));
}
