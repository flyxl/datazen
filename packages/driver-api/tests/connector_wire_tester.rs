//! The wire spelling of the per-condition connector (00-contracts §5.2.1).
//!
//! The contract says: **缺省不写** — a condition without a per-condition
//! connector must **not** carry the key on the wire. Writing `null` would be
//! a *second spelling* of "no connector" next to the absent key the TypeScript
//! side already handles, and the front end's own check is `!== undefined`, so
//! a `null` would slip through and end up inside a filter object.
//!
//! This file is an out-of-tree conformance test: everything is reached through
//! the crate's public API, so it checks the contract a **consumer** sees, not
//! the internals of the struct.
//!
//! Backwards compatibility has to survive the change: a payload that carries
//! an explicit `null` — written by an older build — must still read as "no
//! connector".
use datazen_driver_api::filters::{FilterCondition, FilterConnector, FilterOperator};
use datazen_driver_api::Value;

fn condition(connector: Option<FilterConnector>) -> FilterCondition {
    FilterCondition {
        column: "id".into(),
        operator: FilterOperator::Eq,
        value: Value::Integer(1),
        connector,
    }
}

#[test]
fn an_absent_connector_is_not_written_at_all() {
    let json = serde_json::to_value(condition(None)).expect("serialized");
    assert!(
        json.get("connector").is_none(),
        "a condition without a connector must not carry the key: {json}"
    );
    assert_eq!(
        json.as_object().map(|o| o.len()),
        Some(3),
        "the record holds exactly column/operator/value: {json}"
    );
}

#[test]
fn a_present_connector_is_written() {
    let json = serde_json::to_value(condition(Some(FilterConnector::Or))).expect("serialized");
    assert_eq!(json["connector"], "or");

    let json = serde_json::to_value(condition(Some(FilterConnector::And))).expect("serialized");
    assert_eq!(json["connector"], "and");
}

#[test]
fn an_explicit_null_still_reads_as_no_connector() {
    // Backwards compatibility: a payload written before the change, or by a
    // build that still spells the absence as `null`, must keep loading.
    let parsed: FilterCondition =
        serde_json::from_str(r#"{"column":"id","operator":"eq","value":1,"connector":null}"#)
            .expect("a null connector still reads");
    assert_eq!(parsed.connector, None);
}

#[test]
fn an_absent_key_still_reads_as_no_connector() {
    let parsed: FilterCondition =
        serde_json::from_str(r#"{"column":"id","operator":"eq","value":1}"#).expect("reads");
    assert_eq!(parsed.connector, None);
}

#[test]
fn a_stored_record_never_carries_a_null_connector() {
    // The stored form is what a saved view holds: a list of conditions, of
    // which only the ones the user gave an explicit connector carry one.
    let list = vec![
        condition(None),
        condition(Some(FilterConnector::Or)),
        condition(None),
    ];
    let json = serde_json::to_value(&list).expect("serialized");
    let conditions = json.as_array().expect("an array");
    assert_eq!(conditions.len(), 3);
    assert!(conditions[0].get("connector").is_none());
    assert_eq!(conditions[1]["connector"], "or");
    assert!(conditions[2].get("connector").is_none());
    // Not one of them spells the absence as `null`.
    assert!(
        !json.to_string().contains("\"connector\":null"),
        "the wire must not carry connector: null: {json}"
    );
}

#[test]
fn a_condition_round_trips_through_the_wire() {
    let original = condition(Some(FilterConnector::Or));
    let json = serde_json::to_string(&original).expect("serialized");
    let parsed: FilterCondition = serde_json::from_str(&json).expect("deserialized");
    assert_eq!(parsed.connector, Some(FilterConnector::Or));
    assert_eq!(parsed.column, original.column);
    assert_eq!(parsed.operator, original.operator);
}
