use super::*;
use datazen_driver_api::ColumnSchema;

fn schema() -> TableSchema {
    TableSchema {
        table_name: "users".into(),
        columns: vec![
            ColumnSchema {
                name: "id".into(),
                data_type: "INTEGER".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            },
            ColumnSchema {
                name: "status".into(),
                data_type: "TEXT".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
        ],
        primary_keys: vec!["id".into()],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

fn composite_schema(types: &[(&str, &str)]) -> TableSchema {
    let mut columns = types
        .iter()
        .map(|(name, data_type)| ColumnSchema {
            name: (*name).into(),
            data_type: (*data_type).into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: false,
        })
        .collect::<Vec<_>>();
    columns.push(ColumnSchema {
        name: "status".into(),
        data_type: "TEXT".into(),
        nullable: false,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
    });
    TableSchema {
        table_name: "events".into(),
        columns,
        primary_keys: types.iter().map(|(name, _)| (*name).into()).collect(),
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

fn condition(column: &str, operator: FilterOperator, value: Value) -> FilterCondition {
    FilterCondition {
        column: column.into(),
        operator,
        value,
    }
}

#[test]
fn builds_parameterized_and_filter_without_interpolating_values() {
    let filter = SyncSourceFilter::new(
        vec![
            condition(
                "status",
                FilterOperator::Eq,
                Value::String("active'".into()),
            ),
            condition("id", FilterOperator::Gt, Value::Integer(2)),
        ],
        SyncFilterLogic::And,
    )
    .unwrap();
    filter.validate(&schema()).unwrap();
    let (where_sql, params) = filter
        .build_where('"', 1, |i, _| Ok(format!("${i}")))
        .unwrap();
    assert_eq!(
        where_sql.as_deref(),
        Some("WHERE (\"status\" = $1) AND (\"id\" > $2)")
    );
    assert_eq!(params.len(), 2);
    assert!(matches!(params[0], Value::String(ref value) if value == "active'"));
}

#[test]
fn rejects_unknown_columns_and_empty_values() {
    let unknown = SyncSourceFilter::new(
        vec![condition("secret", FilterOperator::Eq, Value::Integer(1))],
        SyncFilterLogic::And,
    )
    .unwrap();
    assert!(unknown.validate(&schema()).is_err());
    let empty = SyncSourceFilter::new(
        vec![condition(
            "id",
            FilterOperator::Eq,
            Value::String(String::new()),
        )],
        SyncFilterLogic::And,
    )
    .unwrap();
    assert!(empty.validate(&schema()).is_err());
}

#[test]
fn in_values_are_bounded_and_parameterized() {
    let filter = SyncSourceFilter::new(
        vec![condition(
            "id",
            FilterOperator::In,
            Value::Json(serde_json::json!([1, 2, 3])),
        )],
        SyncFilterLogic::Or,
    )
    .unwrap();
    let (where_sql, params) = filter
        .build_where('`', 3, |i, _| Ok(format!("?{i}")))
        .unwrap();
    assert_eq!(where_sql.as_deref(), Some("WHERE (`id` IN (?3, ?4, ?5))"));
    assert_eq!(params.len(), 3);
}

#[test]
fn accepts_frontend_json_arrays_without_untagged_value_coercion() {
    let filter: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "filters": [{"column": "id", "operator": "in", "value": [1, 2, 3]}],
        "logic": "and"
    }))
    .unwrap();
    let (_, params) = filter
        .build_where('"', 1, |i, _| Ok(format!("${i}")))
        .unwrap();
    assert!(matches!(
        params.as_slice(),
        [Value::Integer(1), Value::Integer(2), Value::Integer(3)]
    ));
}

#[test]
fn preserves_binary_filter_values_with_an_explicit_marker() {
    let filter = SyncSourceFilter::new(
        vec![condition(
            "status",
            FilterOperator::Eq,
            Value::Bytes(vec![0, 255]),
        )],
        SyncFilterLogic::And,
    )
    .unwrap();
    let (_, params) = filter
        .build_where('"', 1, |i, _| Ok(format!("${i}")))
        .unwrap();
    assert!(matches!(params.as_slice(), [Value::Bytes(value)] if value == &[0, 255]));
}

#[test]
fn passes_source_type_to_placeholder_formatter() {
    let filter = SyncSourceFilter::new(
        vec![condition(
            "id",
            FilterOperator::Gt,
            Value::String("2".into()),
        )],
        SyncFilterLogic::And,
    )
    .unwrap();
    let (sql, params) = filter
        .build_where_typed(
            '"',
            1,
            |column| (column == "id").then_some("integer".into()),
            |index, data_type| Ok(format!("${index}::{}", data_type.unwrap_or("none"))),
        )
        .unwrap();
    assert_eq!(sql.as_deref(), Some("WHERE (\"id\" > $1::integer)"));
    assert!(matches!(params.as_slice(), [Value::String(value)] if value == "2"));
}

#[test]
fn recordset_only_scope_uses_default_primary_key_and_preserves_limit() {
    let filter: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "filters": [],
        "recordset": {
            "start": {"value": "2"},
            "end": {"value": "5", "inclusive": false},
            "limit": 10
        }
    }))
    .unwrap();
    assert!(!filter.is_empty().unwrap());
    filter.validate(&schema()).unwrap();
    assert_eq!(filter.recordset_limit(&schema()).unwrap(), Some(10));
    let (sql, params) = filter
        .build_where_typed_with_default_order(
            '"',
            1,
            Some("id"),
            |column| (column == "id").then_some("INTEGER".into()),
            |index, data_type| Ok(format!("${index}::{}", data_type.unwrap_or("none"))),
        )
        .unwrap();
    assert_eq!(
        sql.as_deref(),
        Some("WHERE (\"id\" >= $1::INTEGER) AND (\"id\" < $2::INTEGER)")
    );
    assert!(matches!(
        params.as_slice(),
        [Value::Integer(2), Value::Integer(5)]
    ));

    let limit_only: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"limit": 2}
    }))
    .unwrap();
    let (sql, params) = limit_only
        .build_where_typed_with_default_order(
            '"',
            1,
            Some("id"),
            |column| (column == "id").then_some("INTEGER".into()),
            |index, _| Ok(format!("${index}")),
        )
        .unwrap();
    assert!(sql.is_none());
    assert!(params.is_empty());
}

#[test]
fn recordset_rejects_non_primary_key_and_reversed_or_overflow_bounds() {
    let non_key: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"orderBy": "status", "start": {"value": "a"}}
    }))
    .unwrap();
    assert!(non_key.validate(&schema()).is_err());

    let reversed: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {
            "start": {"value": "20"},
            "end": {"value": "10"}
        }
    }))
    .unwrap();
    assert!(reversed.validate(&schema()).is_err());

    let overflow: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"start": {"value": "2147483648"}}
    }))
    .unwrap();
    assert!(overflow.validate(&schema()).is_err());
}

#[test]
fn recordset_keeps_or_filter_grouped_before_range() {
    let filter: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "filters": [{"column": "status", "operator": "eq", "value": "active"}],
        "logic": "or",
        "recordset": {"start": {"value": "2"}}
    }))
    .unwrap();
    let (sql, _) = filter
        .build_where_typed_with_default_order(
            '"',
            1,
            Some("id"),
            |column| Some(if column == "id" { "INTEGER" } else { "TEXT" }.into()),
            |index, _| Ok(format!("${index}")),
        )
        .unwrap();
    assert_eq!(
        sql.as_deref(),
        Some("WHERE ((\"status\" = $1)) AND (\"id\" >= $2)")
    );
}

#[test]
fn tuple_recordset_builds_parameterized_two_column_bounds_in_scan_order() {
    let filter: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "filters": [{"column": "status", "operator": "eq", "value": "active"}],
        "logic": "or",
        "recordset": {
            "tupleRange": {
                "columns": ["tenant_id", "id"],
                "start": {"values": ["5", "10"], "inclusive": false},
                "end": {"values": ["9", "999"], "inclusive": true}
            },
            "limit": 50
        }
    }))
    .unwrap();
    let schema = composite_schema(&[("tenant_id", "INTEGER"), ("id", "BIGINT")]);
    filter.validate(&schema).unwrap();
    let keys = vec!["tenant_id".into(), "id".into()];
    let order = vec!["BINARY `tenant_id`".into(), "BINARY `id`".into()];
    let (sql, params) = filter
        .build_where_typed_with_key_order(
            '`',
            1,
            None,
            Some((&keys, &order)),
            |_, value| Ok(value.clone()),
            |column| {
                Some(
                    if column == "tenant_id" {
                        "INTEGER"
                    } else {
                        "BIGINT"
                    }
                    .into(),
                )
            },
            |index, data_type| Ok(format!("${index}::{}", data_type.unwrap_or("none"))),
        )
        .unwrap();
    assert_eq!(
            sql.as_deref(),
            Some("WHERE ((`status` = $1::BIGINT)) AND ((BINARY `tenant_id`, BINARY `id`) > ($2::INTEGER, $3::BIGINT)) AND ((BINARY `tenant_id`, BINARY `id`) <= ($4::INTEGER, $5::BIGINT))")
        );
    assert!(matches!(
        params.as_slice(),
        [
            Value::String(status),
            Value::String(start_tenant),
            Value::String(start_id),
            Value::String(end_tenant),
            Value::String(end_id)
        ] if status == "active" && start_tenant == "5" && start_id == "10" && end_tenant == "9" && end_id == "999"
    ));
}

#[test]
fn tuple_recordset_builds_three_column_bounds_and_validates_exact_shape() {
    let schema = composite_schema(&[
        ("tenant_id", "INTEGER"),
        ("bucket", "SMALLINT"),
        ("id", "BIGINT"),
    ]);
    let valid: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {
            "tupleRange": {
                "columns": ["tenant_id", "bucket", "id"],
                "start": {"values": ["1", "2", "3"]}
            }
        }
    }))
    .unwrap();
    valid.validate(&schema).unwrap();
    let keys = vec!["tenant_id".into(), "bucket".into(), "id".into()];
    let expressions = vec!["k1".into(), "k2".into(), "k3".into()];
    let (sql, params) = valid
        .build_where_typed_with_key_order(
            '"',
            4,
            None,
            Some((&keys, &expressions)),
            |_, value| Ok(value.clone()),
            |_| Some("INTEGER".into()),
            |index, _| Ok(format!("${index}")),
        )
        .unwrap();
    assert_eq!(sql.as_deref(), Some("WHERE ((k1, k2, k3) >= ($4, $5, $6))"));
    assert!(
        matches!(params.as_slice(), [Value::String(a), Value::String(b), Value::String(c)] if a == "1" && b == "2" && c == "3")
    );

    let incomplete: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {
            "tupleRange": {
                "columns": ["tenant_id", "bucket"],
                "start": {"values": ["1", "2"]}
            }
        }
    }))
    .unwrap();
    assert!(incomplete.validate(&schema).is_err());
}

#[test]
fn tuple_recordset_rejects_mixed_nullable_null_reordered_and_unverified_ranges() {
    let schema = composite_schema(&[("tenant_id", "INTEGER"), ("id", "INTEGER")]);
    let valid = serde_json::json!({
        "recordset": {"tupleRange": {
            "columns": ["tenant_id", "id"],
            "start": {"values": ["1", "2"]}
        }}
    });
    let mixed: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"orderBy": "tenant_id", "tupleRange": valid["recordset"]["tupleRange"]}
    }))
    .unwrap();
    assert!(mixed.validate(&schema).is_err());

    let reordered: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"tupleRange": {
            "columns": ["id", "tenant_id"],
            "start": {"values": ["1", "2"]}
        }}
    }))
    .unwrap();
    assert!(reordered.validate(&schema).is_err());

    let null_component: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"tupleRange": {
            "columns": ["tenant_id", "id"],
            "start": {"values": ["1", null]}
        }}
    }))
    .unwrap();
    assert!(null_component.validate(&schema).is_err());

    let nullable_schema = {
        let mut schema = schema.clone();
        schema.columns[0].nullable = true;
        schema
    };
    let valid_filter: SyncSourceFilter = serde_json::from_value(valid).unwrap();
    assert!(valid_filter.validate(&nullable_schema).is_err());
    assert!(valid_filter
        .validate_tuple_range_order(|_, _| Err("unknown or unsupported type".into()))
        .is_err());
}

#[test]
fn tuple_range_uses_key_contract_order_and_checks_inclusive_empty_ranges() {
    let filter: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"tupleRange": {
            "columns": ["tenant_id", "id"],
            "start": {"values": ["1", "10"]},
            "end": {"values": ["1", "20"]}
        }}
    }))
    .unwrap();
    filter
        .validate_tuple_range_order(|_, value| match value {
            Value::String(value) => value
                .parse::<i128>()
                .map(datazen_driver_api::SyncKeyValue::Integer)
                .map_err(|error| error.to_string()),
            _ => Err("non-integer".into()),
        })
        .unwrap();

    let reversed: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"tupleRange": {
            "columns": ["tenant_id", "id"],
            "start": {"values": ["2", "1"]},
            "end": {"values": ["1", "999"]}
        }}
    }))
    .unwrap();
    assert!(reversed
        .validate_tuple_range_order(|_, value| match value {
            Value::String(value) => value
                .parse::<i128>()
                .map(datazen_driver_api::SyncKeyValue::Integer)
                .map_err(|error| error.to_string()),
            _ => Err("non-integer".into()),
        })
        .is_err());

    let equal_exclusive: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"tupleRange": {
            "columns": ["tenant_id", "id"],
            "start": {"values": ["1", "2"], "inclusive": false},
            "end": {"values": ["1", "2"], "inclusive": true}
        }}
    }))
    .unwrap();
    assert!(equal_exclusive
        .validate_tuple_range_order(|_, value| match value {
            Value::String(value) => value
                .parse::<i128>()
                .map(datazen_driver_api::SyncKeyValue::Integer)
                .map_err(|error| error.to_string()),
            _ => Err("non-integer".into()),
        })
        .is_err());
}

#[test]
fn legacy_scalar_filter_json_remains_unchanged_when_round_tripped() {
    let json = serde_json::json!({
        "filters": [],
        "recordset": {
            "orderBy": "id",
            "start": {"value": "10", "inclusive": false},
            "limit": 20
        }
    });
    let filter: SyncSourceFilter = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(serde_json::to_value(filter).unwrap(), json);
}

#[test]
fn test_tester_filter_value_conversion_covers_scalar_and_invalid_json_shapes() {
    use super::super::filter_values::{in_values, json_to_value, scalar_value, value_to_json};

    assert!(matches!(
        scalar_value(&serde_json::json!(true)).unwrap(),
        Value::Bool(true)
    ));
    assert!(scalar_value(&serde_json::json!([1, 2])).is_err());
    assert!(matches!(
        in_values(&serde_json::json!(" a, ,b ")).unwrap().as_slice(),
        [Value::String(a), Value::String(b)] if a == "a" && b == "b"
    ));
    assert!(in_values(&serde_json::json!(true)).is_err());
    assert!(in_values(&serde_json::json!([{"nested": true}])).is_err());

    for value in [
        Value::Null,
        Value::Bool(false),
        Value::Integer(3),
        Value::Float(1.5),
        Value::String("text".into()),
        Value::Timestamp("2026-09-23T12:00:00Z".into()),
        Value::Bytes(vec![0, 255]),
        Value::Json(serde_json::json!({"key": "value"})),
    ] {
        value_to_json(&value).expect("supported filter value should serialize");
    }
    assert!(value_to_json(&Value::Float(f64::NAN)).is_err());
    assert!(json_to_value(&serde_json::json!({"not": "a scalar"})).is_err());
    assert!(json_to_value(&serde_json::json!({
        "$datazenType": "bytes",
        "encoding": "base64",
        "value": "not base64!"
    }))
    .is_err());
}

#[test]
fn test_tester_recordset_rejects_bad_primary_key_metadata_and_tuple_scalars() {
    let valid = serde_json::json!({
        "recordset": {"tupleRange": {
            "columns": ["tenant_id", "id"],
            "start": {"values": ["1", "2"]}
        }}
    });
    let filter: SyncSourceFilter = serde_json::from_value(valid.clone()).unwrap();

    let mut duplicate_keys = composite_schema(&[("tenant_id", "INTEGER"), ("id", "INTEGER")]);
    duplicate_keys.primary_keys = vec!["tenant_id".into(), "tenant_id".into()];
    let duplicate_filter: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"tupleRange": {
            "columns": ["tenant_id", "tenant_id"],
            "start": {"values": ["1", "2"]}
        }}
    }))
    .unwrap();
    assert!(duplicate_filter.validate(&duplicate_keys).is_err());

    let mut missing_column = composite_schema(&[("tenant_id", "INTEGER"), ("id", "INTEGER")]);
    missing_column.primary_keys = vec!["tenant_id".into(), "missing".into()];
    let missing_filter: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"tupleRange": {
            "columns": ["tenant_id", "missing"],
            "start": {"values": ["1", "2"]}
        }}
    }))
    .unwrap();
    assert!(missing_filter.validate(&missing_column).is_err());

    for component in [
        serde_json::json!(["nested"]),
        serde_json::json!({"nested": true}),
    ] {
        let invalid: SyncSourceFilter = serde_json::from_value(serde_json::json!({
            "recordset": {"tupleRange": {
                "columns": ["tenant_id", "id"],
                "start": {"values": ["1", component]}
            }}
        }))
        .unwrap();
        assert!(invalid
            .validate(&composite_schema(&[
                ("tenant_id", "INTEGER"),
                ("id", "INTEGER")
            ]))
            .is_err());
    }

    let no_keys = {
        let mut schema = composite_schema(&[("tenant_id", "INTEGER"), ("id", "INTEGER")]);
        schema.primary_keys.clear();
        schema
            .columns
            .iter_mut()
            .for_each(|column| column.is_primary_key = false);
        schema
    };
    let scalar_without_order: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"start": {"value": "1"}}
    }))
    .unwrap();
    assert!(scalar_without_order.validate(&no_keys).is_err());
    assert!(filter
        .validate(&composite_schema(&[
            ("tenant_id", "INTEGER"),
            ("id", "INTEGER")
        ]))
        .is_ok());
}

#[test]
fn test_tester_recordset_builder_rejects_unverified_columns_order_and_binding() {
    let tuple: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"tupleRange": {
            "columns": ["tenant_id", "id"],
            "start": {"values": ["1", "2"]}
        }}
    }))
    .unwrap();
    assert!(tuple
        .build_where_typed_with_key_order(
            '"',
            1,
            None,
            None,
            |_, value| Ok(value.clone()),
            |_| Some("INTEGER".into()),
            |index, _| Ok(format!("${index}")),
        )
        .is_err());
    assert!(tuple
        .build_where_typed_with_key_order(
            '"',
            1,
            None,
            Some((&["tenant_id".into(), "id".into()], &["tenant".into()])),
            |_, value| Ok(value.clone()),
            |_| Some("INTEGER".into()),
            |index, _| Ok(format!("${index}")),
        )
        .is_err());

    let no_default: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"start": {"value": "1"}}
    }))
    .unwrap();
    assert!(no_default
        .build_where_typed_with_default_order(
            '"',
            1,
            None,
            |_| None,
            |index, _| { Ok(format!("${index}")) }
        )
        .is_err());
    assert!(no_default
        .build_where_typed_with_default_order(
            '"',
            1,
            Some("missing"),
            |_| None,
            |index, _| { Ok(format!("${index}")) }
        )
        .is_err());

    let valid_keys = vec!["tenant_id".to_string(), "id".to_string()];
    let expression_count_mismatch = tuple.build_where_typed_with_key_order(
        '"',
        1,
        None,
        Some((&valid_keys, &["tenant_expr".into()])),
        |_, value| Ok(value.clone()),
        |_| Some("INTEGER".into()),
        |index, _| Ok(format!("${index}")),
    );
    assert!(expression_count_mismatch.is_err());

    let placeholder_error = tuple.build_where_typed_with_key_order(
        '"',
        1,
        None,
        Some((&valid_keys, &["tenant_expr".into(), "id_expr".into()])),
        |_, value| Ok(value.clone()),
        |_| Some("INTEGER".into()),
        |_, _| Err(crate::DataSyncError::validation("placeholder failed")),
    );
    assert!(placeholder_error.is_err());

    let normalization_error = tuple.build_where_typed_with_key_order(
        '"',
        1,
        None,
        Some((&valid_keys, &["tenant_expr".into(), "id_expr".into()])),
        |_, _| Err(crate::DataSyncError::validation("unknown key")),
        |_| Some("INTEGER".into()),
        |index, _| Ok(format!("${index}")),
    );
    assert!(normalization_error.is_err());
}
