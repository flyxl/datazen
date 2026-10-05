use super::*;
use datazen_driver_api::ColumnSchema;

fn schema(primary_keys: &[&str]) -> TableSchema {
    TableSchema {
        table_name: "users".into(),
        columns: vec![
            ColumnSchema {
                name: "id".into(),
                data_type: "INTEGER".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: primary_keys.contains(&"id"),
                is_auto_increment: false,
            },
            ColumnSchema {
                name: "name".into(),
                data_type: "TEXT".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: primary_keys.contains(&"name"),
                is_auto_increment: false,
            },
        ],
        primary_keys: primary_keys.iter().map(|v| (*v).into()).collect(),
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

fn rs(order_by: Option<&str>) -> TransferRecordset {
    TransferRecordset {
        order_by: order_by.map(str::to_string),
        start: Some(TransferRecordsetBound {
            value: serde_json::json!(2),
            inclusive: true,
        }),
        end: Some(TransferRecordsetBound {
            value: serde_json::json!(5),
            inclusive: false,
        }),
        tuple_range: None,
        limit: Some(10),
    }
}

fn tuple_recordset(
    columns: &[&str],
    start: Option<(Vec<serde_json::Value>, bool)>,
    end: Option<(Vec<serde_json::Value>, bool)>,
) -> TransferRecordset {
    TransferRecordset {
        order_by: None,
        start: None,
        end: None,
        tuple_range: Some(TransferRecordsetTupleRange {
            columns: columns.iter().map(|column| (*column).to_string()).collect(),
            start: start
                .map(|(values, inclusive)| TransferRecordsetTupleBound { values, inclusive }),
            end: end.map(|(values, inclusive)| TransferRecordsetTupleBound { values, inclusive }),
        }),
        limit: None,
    }
}

fn three_column_schema() -> TableSchema {
    let mut schema = schema(&["name", "id"]);
    schema.table_name = "events".into();
    schema.columns[0].data_type = "BIGINT".into();
    schema.columns.push(ColumnSchema {
        name: "revision".into(),
        data_type: "NUMERIC(12,2)".into(),
        nullable: false,
        default_value: None,
        comment: None,
        is_primary_key: true,
        is_auto_increment: false,
    });
    schema.primary_keys = vec!["name".into(), "id".into(), "revision".into()];
    schema
}

#[test]
fn legacy_scalar_recordset_round_trips_without_new_wire_fields() {
    let legacy = serde_json::json!({
        "orderBy": "id",
        "start": { "value": "2", "inclusive": false },
        "end": { "value": "9", "inclusive": true },
        "limit": 4
    });
    let decoded: TransferRecordset = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), legacy);
    let omitted_inclusive: TransferRecordset = serde_json::from_value(serde_json::json!({
        "orderBy": "id",
        "end": { "value": "9" }
    }))
    .unwrap();
    assert!(omitted_inclusive.end.unwrap().inclusive);
}

#[test]
fn two_column_tuple_uses_native_row_predicates_and_declared_scan_order() {
    let schema = schema(&["name", "id"]);
    let recordset = tuple_recordset(
        &["name", "id"],
        Some((
            vec![serde_json::json!("東京"), serde_json::json!("-8")],
            true,
        )),
        Some((
            vec![serde_json::json!("東京"), serde_json::json!("2147483647")],
            false,
        )),
    );
    let scope = build_source_scope(
        &schema,
        None,
        Some(&recordset),
        '"',
        "postgresql",
        |index, data_type| Ok(format!("${index}::{}", data_type.unwrap_or("none"))),
        |column| {
            schema
                .columns
                .iter()
                .find(|candidate| candidate.name == column)
                .map(|candidate| candidate.data_type.clone())
        },
    )
    .unwrap();
    assert_eq!(
        scope.where_sql.as_deref(),
        Some(
            r#"WHERE ("name", "id") >= ($1::TEXT, $2::INTEGER) AND ("name", "id") < ($3::TEXT, $4::INTEGER)"#
        )
    );
    assert_eq!(
        scope.recordset_sql.as_deref(),
        Some(r#"ORDER BY "name" ASC, "id" ASC"#)
    );
    assert!(matches!(
        scope.params.as_slice(),
        [Value::String(start_name), Value::Integer(-8), Value::String(end_name), Value::Integer(2147483647)]
            if start_name == "東京" && end_name == "東京"
    ));
}

#[test]
fn three_column_tuple_binds_mixed_key_types_in_start_then_end_order() {
    let schema = three_column_schema();
    let recordset = tuple_recordset(
        &["name", "id", "revision"],
        Some((
            vec![
                serde_json::json!("租户雪"),
                serde_json::json!("-9223372036854775808"),
                serde_json::json!("-1.25"),
            ],
            false,
        )),
        Some((
            vec![
                serde_json::json!("租户雪"),
                serde_json::json!("9223372036854775807"),
                serde_json::json!("10.50"),
            ],
            true,
        )),
    );
    let scope = build_source_scope(
        &schema,
        None,
        Some(&recordset),
        '`',
        "mysql",
        |_, _| Ok("?".to_string()),
        |column| {
            schema
                .columns
                .iter()
                .find(|candidate| candidate.name == column)
                .map(|candidate| candidate.data_type.clone())
        },
    )
    .unwrap();
    assert_eq!(
        scope.where_sql.as_deref(),
        Some(
            "WHERE (`name`, `id`, `revision`) > (?, ?, ?) AND (`name`, `id`, `revision`) <= (?, ?, ?)"
        )
    );
    assert_eq!(
        scope.recordset_sql.as_deref(),
        Some("ORDER BY `name` ASC, `id` ASC, `revision` ASC")
    );
    assert!(matches!(
        scope.params.as_slice(),
        [
            Value::String(name1), Value::Integer(i64::MIN), Value::String(decimal1),
            Value::String(name2), Value::Integer(i64::MAX), Value::String(decimal2)
        ] if name1 == "租户雪" && name2 == "租户雪" && decimal1 == "-1.25" && decimal2 == "10.50"
    ));
}

#[test]
fn tuple_ranges_reject_partial_reordered_null_unknown_nullable_and_mixed_bounds() {
    let schema = schema(&["name", "id"]);
    let valid = tuple_recordset(
        &["name", "id"],
        Some((vec![serde_json::json!("x"), serde_json::json!(1)], true)),
        None,
    );
    let mut invalid = valid.clone();
    invalid.tuple_range.as_mut().unwrap().columns = vec!["id".into()];
    assert!(resolve_recordset(&invalid, &schema).is_err());
    invalid.tuple_range.as_mut().unwrap().columns = vec!["id".into(), "name".into()];
    assert!(resolve_recordset(&invalid, &schema).is_err());
    invalid = valid.clone();
    invalid
        .tuple_range
        .as_mut()
        .unwrap()
        .start
        .as_mut()
        .unwrap()
        .values
        .pop();
    assert!(resolve_recordset(&invalid, &schema).is_err());
    invalid = valid.clone();
    invalid
        .tuple_range
        .as_mut()
        .unwrap()
        .start
        .as_mut()
        .unwrap()
        .values[0] = serde_json::Value::Null;
    assert!(resolve_recordset(&invalid, &schema).is_err());
    invalid = valid.clone();
    invalid
        .tuple_range
        .as_mut()
        .unwrap()
        .start
        .as_mut()
        .unwrap()
        .values[0] = serde_json::json!(7);
    assert!(resolve_recordset(&invalid, &schema).is_err());
    invalid = valid.clone();
    invalid.order_by = Some("id".into());
    assert!(resolve_recordset(&invalid, &schema).is_err());

    let mut nullable_schema = schema.clone();
    nullable_schema.columns[1].nullable = true;
    assert!(resolve_recordset(&valid, &nullable_schema).is_err());
    let mut unknown_type_schema = schema;
    unknown_type_schema.columns[1].data_type = "custom_sortable".into();
    assert!(resolve_recordset(&valid, &unknown_type_schema).is_err());
}

#[test]
fn tuple_ranges_reject_reversed_empty_and_unverified_driver_ordering() {
    let schema = three_column_schema();
    let mut recordset = tuple_recordset(
        &["name", "id", "revision"],
        Some((
            vec![
                serde_json::json!("同名"),
                serde_json::json!(10),
                serde_json::json!("1"),
            ],
            true,
        )),
        Some((
            vec![
                serde_json::json!("同名"),
                serde_json::json!(9),
                serde_json::json!("2"),
            ],
            true,
        )),
    );
    assert!(resolve_recordset(&recordset, &schema).is_err());
    recordset
        .tuple_range
        .as_mut()
        .unwrap()
        .end
        .as_mut()
        .unwrap()
        .values[1] = serde_json::json!(10);
    recordset
        .tuple_range
        .as_mut()
        .unwrap()
        .end
        .as_mut()
        .unwrap()
        .values[2] = serde_json::json!("1.00");
    recordset
        .tuple_range
        .as_mut()
        .unwrap()
        .end
        .as_mut()
        .unwrap()
        .inclusive = false;
    assert!(resolve_recordset(&recordset, &schema).is_err());
    recordset
        .tuple_range
        .as_mut()
        .unwrap()
        .end
        .as_mut()
        .unwrap()
        .inclusive = true;
    assert!(resolve_recordset(&recordset, &schema).is_ok());
    let unsupported = build_source_scope(
        &schema,
        None,
        Some(&recordset),
        '"',
        "sqlite",
        |_, _| Ok("?".to_string()),
        |column| {
            schema
                .columns
                .iter()
                .find(|candidate| candidate.name == column)
                .map(|c| c.data_type.clone())
        },
    );
    assert!(unsupported
        .unwrap_err()
        .to_string()
        .contains("cannot guarantee"));
}

#[test]
fn defaults_to_single_effective_primary_key_and_parameterizes_scope() {
    let schema = schema(&["id"]);
    let scope = build_source_scope(
        &schema,
        None,
        Some(&rs(None)),
        '"',
        "postgresql",
        |index, data_type| Ok(format!("${index}::{}", data_type.unwrap_or("none"))),
        |column| {
            Some(
                schema
                    .columns
                    .iter()
                    .find(|c| c.name == column)?
                    .data_type
                    .clone(),
            )
        },
    )
    .unwrap();
    assert_eq!(
        scope.where_sql.as_deref(),
        Some(r#"WHERE ("id" >= $1::INTEGER) AND ("id" < $2::INTEGER)"#)
    );
    assert_eq!(
        scope.recordset_sql.as_deref(),
        Some(r#"ORDER BY "id" ASC LIMIT $3::none"#)
    );
    assert!(matches!(
        scope.params.as_slice(),
        [Value::Integer(2), Value::Integer(5), Value::Integer(10)]
    ));
}

#[test]
fn rejects_missing_primary_key_without_explicit_order_and_composite_default() {
    let no_pk = schema(&[]);
    assert!(resolve_recordset(&rs(None), &no_pk)
        .unwrap_err()
        .to_string()
        .contains("no primary key"));
    let composite = schema(&["id", "name"]);
    assert!(resolve_recordset(&rs(None), &composite)
        .unwrap_err()
        .to_string()
        .contains("composite"));
}

#[test]
fn rejects_invalid_bounds_limits_and_identifiers() {
    let schema = schema(&["id"]);
    let mut invalid = rs(Some("missing"));
    assert!(resolve_recordset(&invalid, &schema).is_err());
    invalid.order_by = Some("id".into());
    invalid.start = Some(TransferRecordsetBound {
        value: serde_json::Value::Null,
        inclusive: true,
    });
    assert!(resolve_recordset(&invalid, &schema).is_err());
    invalid.start = None;
    invalid.limit = Some(0);
    assert!(resolve_recordset(&invalid, &schema).is_err());
    invalid.limit = Some(i64::MAX as u64 + 1);
    assert!(resolve_recordset(&invalid, &schema).is_err());
}

#[test]
fn strict_recordset_serde_rejects_unknown_fields() {
    let result = serde_json::from_value::<TransferRecordset>(serde_json::json!({
        "orderBy": "id",
        "unknown": true
    }));
    assert!(result.is_err());
}

#[test]
fn test_tester_rejects_reversed_bounds_before_query() {
    let schema = schema(&["id"]);
    let mut recordset = rs(Some("id"));
    recordset.start.as_mut().unwrap().value = serde_json::json!("20");
    recordset.end.as_mut().unwrap().value = serde_json::json!("10");

    let result = build_source_scope(
        &schema,
        None,
        Some(&recordset),
        '"',
        "postgresql",
        |_, _| Ok("?".to_string()),
        |column| {
            schema
                .columns
                .iter()
                .find(|candidate| candidate.name == column)
                .map(|candidate| candidate.data_type.clone())
        },
    );
    assert!(result.is_err(), "start > end must fail closed before query");
}

#[test]
fn test_tester_rejects_integer_bound_overflow_from_frontend_text() {
    let schema = schema(&["id"]);
    let recordset = TransferRecordset {
        order_by: Some("id".into()),
        start: Some(TransferRecordsetBound {
            value: serde_json::json!("2147483648"),
            inclusive: true,
        }),
        end: None,
        tuple_range: None,
        limit: None,
    };

    let result = build_source_scope(
        &schema,
        None,
        Some(&recordset),
        '"',
        "postgresql",
        |_, _| Ok("?".to_string()),
        |column| {
            schema
                .columns
                .iter()
                .find(|candidate| candidate.name == column)
                .map(|candidate| candidate.data_type.clone())
        },
    );
    assert!(result.is_err(), "integer bound overflow must fail closed");
}

#[test]
fn equal_bounds_require_both_endpoints_to_be_inclusive() {
    let schema = schema(&["id"]);
    let mut recordset = rs(Some("id"));
    recordset.start.as_mut().unwrap().value = serde_json::json!("10");
    recordset.end.as_mut().unwrap().value = serde_json::json!("10");
    recordset.end.as_mut().unwrap().inclusive = true;
    assert!(resolve_recordset(&recordset, &schema).is_ok());

    recordset.end.as_mut().unwrap().inclusive = false;
    assert!(resolve_recordset(&recordset, &schema).is_err());
}

#[test]
fn typed_bounds_cover_signed_unsigned_and_malformed_integer_text() {
    let mut schema = schema(&["id"]);
    schema.columns[0].data_type = "BIGINT UNSIGNED".into();
    let mut recordset = rs(Some("id"));
    recordset.start.as_mut().unwrap().value = serde_json::json!("18446744073709551615");
    recordset.end = None;
    assert!(resolve_recordset(&recordset, &schema).is_ok());

    recordset.start.as_mut().unwrap().value = serde_json::json!("-1");
    assert!(resolve_recordset(&recordset, &schema).is_err());
    recordset.start.as_mut().unwrap().value = serde_json::json!("not-an-integer");
    assert!(resolve_recordset(&recordset, &schema).is_err());

    schema.columns[0].data_type = "SMALLINT".into();
    recordset.start.as_mut().unwrap().value = serde_json::json!("32768");
    assert!(resolve_recordset(&recordset, &schema).is_err());
}

#[test]
fn decimal_and_float_bounds_are_validated_before_comparison() {
    let mut schema = schema(&["id"]);
    schema.columns[0].data_type = "NUMERIC(12,4)".into();
    let mut recordset = rs(Some("id"));
    recordset.start.as_mut().unwrap().value = serde_json::json!("1.20");
    recordset.end.as_mut().unwrap().value = serde_json::json!("1.2");
    recordset.end.as_mut().unwrap().inclusive = true;
    assert!(resolve_recordset(&recordset, &schema).is_ok());
    recordset.end.as_mut().unwrap().value = serde_json::json!("1.19");
    assert!(resolve_recordset(&recordset, &schema).is_err());
    recordset.start.as_mut().unwrap().value = serde_json::json!("not-a-decimal");
    assert!(resolve_recordset(&recordset, &schema).is_err());

    schema.columns[0].data_type = "DOUBLE PRECISION".into();
    recordset.start.as_mut().unwrap().value = serde_json::json!("1.0");
    recordset.end.as_mut().unwrap().value = serde_json::json!("2.0");
    assert!(resolve_recordset(&recordset, &schema).is_ok());
    recordset.end.as_mut().unwrap().value = serde_json::json!("NaN");
    assert!(resolve_recordset(&recordset, &schema).is_err());
}

#[test]
fn text_bounds_remain_strings_and_fail_closed_when_collation_order_is_unknown() {
    let mut schema = schema(&["id"]);
    schema.columns[0].data_type = "VARCHAR(32)".into();
    let mut recordset = rs(Some("id"));
    recordset.start.as_mut().unwrap().value = serde_json::json!("b");
    recordset.end.as_mut().unwrap().value = serde_json::json!("a");
    assert!(resolve_recordset(&recordset, &schema).is_err());
    recordset.end.as_mut().unwrap().value = serde_json::json!("z");
    assert!(resolve_recordset(&recordset, &schema).is_err());
    recordset.end = None;
    let resolved = resolve_recordset(&recordset, &schema).unwrap();
    assert!(matches!(
        resolved.start.unwrap().values.as_slice(),
        [Value::String(value)] if value == "b"
    ));
}

#[test]
fn boolean_and_non_finite_bounds_fail_closed() {
    let mut schema = schema(&["id"]);
    schema.columns[0].data_type = "BOOLEAN".into();
    let mut recordset = rs(Some("id"));
    recordset.start.as_mut().unwrap().value = serde_json::json!("true");
    recordset.end.as_mut().unwrap().value = serde_json::json!("false");
    assert!(resolve_recordset(&recordset, &schema).is_err());

    recordset.start.as_mut().unwrap().value = serde_json::json!("false");
    recordset.end.as_mut().unwrap().value = serde_json::json!("true");
    let resolved = resolve_recordset(&recordset, &schema).unwrap();
    assert!(matches!(
        resolved.start.unwrap().values.as_slice(),
        [Value::Bool(false)]
    ));

    schema.columns[0].data_type = "DOUBLE PRECISION".into();
    recordset.start.as_mut().unwrap().value = serde_json::json!("Infinity");
    recordset.end.as_mut().unwrap().value = serde_json::json!("Infinity");
    assert!(resolve_recordset(&recordset, &schema).is_err());
    recordset.start.as_mut().unwrap().value = serde_json::Value::Null;
    assert!(resolve_recordset(&recordset, &schema).is_err());
}

#[test]
fn legacy_table_mapping_without_recordset_deserializes() {
    let mapping: super::super::model::TableMapping = serde_json::from_value(serde_json::json!({
        "sourceTable": "users",
        "targetTable": "users",
        "enabled": true,
        "columnMappings": []
    }))
    .unwrap();
    assert!(mapping.recordset.is_none());
}
