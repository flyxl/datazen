use super::*;
use crate::{DatabaseTypeMeta, DriverCapabilities};
use std::cell::RefCell;

fn registry() -> DatabaseRegistry {
    let mut registry = DatabaseRegistry::default();
    registry.register_metas(vec![
        (
            "postgresql".to_owned(),
            DatabaseTypeMeta {
                quote_char: "\"".to_owned(),
                ..Default::default()
            },
        ),
        (
            "mysql".to_owned(),
            DatabaseTypeMeta {
                quote_char: "`".to_owned(),
                ..Default::default()
            },
        ),
    ]);
    registry
}

fn capabilities(has_schema_level: bool) -> SessionCapabilitiesEntry {
    SessionCapabilitiesEntry {
        db_session_id: "sess-1".to_owned(),
        capabilities: Some(DriverCapabilities {
            supports_cancel_query: true,
            supports_query_execution_cancel: true,
            supports_explain: true,
            supports_streaming_results: true,
            supports_offset: true,
            has_schema_level,
        }),
    }
}

fn column(name: &str, data_type: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        nullable: false,
        default_value: None,
        is_primary_key: None,
        is_auto_increment: None,
        comment: None,
    }
}

#[test]
fn a_pseudo_schema_has_empty_metadata_and_truly_nullable_columns() {
    let schema = build_pseudo_table_schema("users.v2", &["id".to_owned(), "name".to_owned()]);
    assert_eq!(schema.table_name, "users.v2");
    assert_eq!(schema.columns.len(), 2);
    assert_eq!(schema.columns[0].name, "id");
    assert_eq!(schema.columns[0].data_type, "");
    assert!(schema.columns[0].nullable);
    assert!(schema.columns[0].default_value.is_none());
    assert!(schema.columns[0].is_primary_key.is_none());
    assert_eq!(schema.columns[1].name, "name");
    assert!(schema.primary_keys.is_empty());
    assert!(schema.indexes.is_empty());
    assert!(schema.foreign_keys.is_empty());
}

#[test]
fn a_nonempty_cached_schema_wins_and_keeps_the_exact_name() {
    let calls = RefCell::new(Vec::new());
    let mut cached = build_pseudo_table_schema("original", &["cached".to_owned()]);
    cached.columns[0].data_type = "int".to_owned();
    let input = FetchTableSchemaInput {
        db_session_id: "sess-1",
        connection_id: "cfg-1",
        table_name: "users.v2",
        schema: Some("archive"),
        database: "app",
        capability_entries: &[capabilities(true)],
        column_map: None,
        read_cached_schema: &|session, table, database, schema, connection| {
            calls.borrow_mut().push((
                session.to_owned(),
                table.to_owned(),
                database.to_owned(),
                schema.map(str::to_owned),
                connection.to_owned(),
            ));
            Ok(cached.clone())
        },
        read_relation_columns: &|_connection, _relation| {
            panic!("columns must not be read when the cache has data")
        },
    };
    let result = fetch_table_schema_for_sql_generation(&input);
    let result = match result {
        Some(value) => value,
        None => panic!("cached schema should win"),
    };
    assert_eq!(result.table_name, "users.v2");
    assert_eq!(result.columns[0].name, "cached");
    assert_eq!(
        calls.into_inner(),
        vec![(
            "sess-1".to_owned(),
            "users.v2".to_owned(),
            "app".to_owned(),
            Some("archive".to_owned()),
            "cfg-1".to_owned()
        )]
    );
}

#[test]
fn typed_columns_are_taken_from_the_exact_relation_when_the_cache_fails() {
    let seen_relation = RefCell::new(None);
    let input = FetchTableSchemaInput {
        db_session_id: "sess-1",
        connection_id: "cfg-1",
        table_name: "users",
        schema: Some("mydb"),
        database: "mydb",
        capability_entries: &[capabilities(false)],
        column_map: None,
        read_cached_schema: &|_session, _table, _database, schema, _connection| {
            assert_eq!(schema, None);
            Err("unsupported".to_owned())
        },
        read_relation_columns: &|connection, relation| {
            assert_eq!(connection, "cfg-1");
            *seen_relation.borrow_mut() = Some(relation.clone());
            Ok(vec![ReadColumnsRow {
                status: "ok".to_owned(),
                value: Some(ReadColumnsValue {
                    relation: relation.clone(),
                    columns: vec![column("id", "int")],
                    primary_keys: vec!["id".to_owned()],
                }),
            }])
        },
    };
    let result = fetch_table_schema_for_sql_generation(&input);
    let result = match result {
        Some(value) => value,
        None => panic!("columns should be available"),
    };
    assert_eq!(result.table_name, "users");
    assert_eq!(result.columns, vec![column("id", "int")]);
    assert_eq!(result.primary_keys, vec!["id".to_owned()]);
    assert!(result.indexes.is_empty());
    let relation = seen_relation.borrow();
    assert_eq!(relation.as_ref().map(|r| r.database.as_str()), Some("mydb"));
    assert_eq!(relation.as_ref().map(|r| r.schema.as_deref()), Some(None));
    assert_eq!(relation.as_ref().map(|r| r.name.as_str()), Some("users"));
}

#[test]
fn a_mismatched_or_failed_column_row_does_not_produce_a_schema() {
    let input = FetchTableSchemaInput {
        db_session_id: "sess-1",
        connection_id: "cfg-1",
        table_name: "users",
        schema: Some("public"),
        database: "app",
        capability_entries: &[capabilities(true)],
        column_map: None,
        read_cached_schema: &|_session, _table, _database, _schema, _connection| {
            Err("fail".to_owned())
        },
        read_relation_columns: &|_connection, relation| {
            Ok(vec![ReadColumnsRow {
                status: "error".to_owned(),
                value: Some(ReadColumnsValue {
                    relation: relation.clone(),
                    columns: vec![column("id", "int")],
                    primary_keys: Vec::new(),
                }),
            }])
        },
    };
    assert!(fetch_table_schema_for_sql_generation(&input).is_none());
}

#[test]
fn the_column_map_is_the_last_resort_and_empty_lists_do_not_count() {
    let mut columns = BTreeMap::new();
    columns.insert("users".to_owned(), vec!["id".to_owned(), "email".to_owned()]);
    columns.insert("empty".to_owned(), Vec::new());
    let input = FetchTableSchemaInput {
        db_session_id: "sess-1",
        connection_id: "cfg-1",
        table_name: "users",
        schema: None,
        database: "app",
        capability_entries: &[capabilities(true)],
        column_map: Some(&columns),
        read_cached_schema: &|_session, _table, _database, _schema, _connection| {
            Err("fail".to_owned())
        },
        read_relation_columns: &|_connection, _relation| Err("transport".to_owned()),
    };
    assert_eq!(
        fetch_table_schema_for_sql_generation(&input),
        Some(build_pseudo_table_schema("users", &["id".to_owned(), "email".to_owned()]))
    );

    let empty = FetchTableSchemaInput {
        table_name: "empty",
        column_map: Some(&columns),
        ..input
    };
    assert!(fetch_table_schema_for_sql_generation(&empty).is_none());
}

#[test]
fn generate_with_columns_uses_the_full_generator_and_schema_prefix() {
    let schema = build_pseudo_table_schema("users", &["id".to_owned(), "name".to_owned()]);
    let opts = GenerateFallbackOptions {
        schema_prefix: Some("public"),
        table_name: "users",
        table_ref_label: None,
        registry: &registry(),
    };
    let sql = generate_table_sql_with_fallbacks(
        Some(&schema),
        GeneratedSqlType::Insert,
        "postgresql",
        &opts,
    );
    assert!(sql.contains("\"id\""));
    assert!(sql.contains("\"name\""));
    assert!(sql.contains("INSERT INTO \"public\".\"users\""));
}

#[test]
fn generate_fallbacks_pin_select_and_comment_text() {
    let registry = registry();
    let select = GenerateFallbackOptions {
        schema_prefix: Some("public"),
        table_name: "users",
        table_ref_label: None,
        registry: &registry,
    };
    assert_eq!(
        generate_table_sql_with_fallbacks(None, GeneratedSqlType::Select, "postgresql", &select),
        "SELECT *\nFROM \"public\".\"users\";"
    );

    let insert = GenerateFallbackOptions {
        schema_prefix: None,
        table_name: "users",
        table_ref_label: Some("mydb.users"),
        registry: &registry,
    };
    assert_eq!(
        generate_table_sql_with_fallbacks(None, GeneratedSqlType::Insert, "mysql", &insert),
        "/* No column metadata available for mydb.users */"
    );
    let no_label = GenerateFallbackOptions {
        table_ref_label: None,
        ..insert
    };
    assert_eq!(
        generate_table_sql_with_fallbacks(None, GeneratedSqlType::Update, "mysql", &no_label),
        "/* No column metadata available for users */"
    );
}

#[test]
fn crate_root_reexports_the_schema_for_sql_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::RelationRef>();
    resolves::<crate::ReadColumnsValue>();
    resolves::<crate::ReadColumnsRow>();
    resolves::<crate::FetchTableSchemaInput>();
    resolves::<crate::GenerateFallbackOptions>();
    assert_eq!(
        crate::build_pseudo_table_schema("t", &[]).table_name,
        "t"
    );
}
