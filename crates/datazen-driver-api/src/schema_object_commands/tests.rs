use super::*;
use crate::schema_dependencies::TypeDependencyUsageKind;
use crate::types::ColumnInfo;

mod dependency_route;
mod view_check_option;
mod view_scope_mapping;

fn col(name: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: "text".into(),
        nullable: true,
    }
}

#[test]
fn command_definitions_include_schema_object_commands() {
    let defs = schema_object_command_definitions();
    let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
    assert!(ids.contains(&"list_objects"));
    assert!(ids.contains(&"get_object_ddl"));
    assert!(ids.contains(&"get_object_dependencies"));
    assert!(ids.contains(&"list_privileges"));
    assert!(is_schema_object_command("get_object_dependencies"));
    let dependency_command = defs
        .iter()
        .find(|definition| definition.id == "get_object_dependencies")
        .unwrap();
    let schema = dependency_command.output_schema.as_ref().unwrap();
    assert_eq!(
        schema["properties"]["sequenceDependencyUsages"]["type"],
        "array"
    );
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .all(|field| field != "sequenceDependencyUsages"));
}

fn sequence_dependency_result(
    kind: &str,
    dependency_name: &str,
    sequence_schema: Option<&str>,
    sequence_name: Option<&str>,
    owner_schema: Option<&str>,
    owner_table: Option<&str>,
    owner_column: Option<&str>,
    usage: Option<&str>,
) -> QueryResult {
    QueryResult {
        columns: vec![
            col("selected_count"),
            col("unsupported_count"),
            col("kind"),
            col("dependency_schema"),
            col("name"),
            col("signature"),
            col("sequence_schema"),
            col("sequence_name"),
            col("owner_table_schema"),
            col("owner_table_name"),
            col("owner_column_name"),
            col("sequence_usage"),
        ],
        rows: vec![vec![
            Some(Value::Integer(1)),
            Some(Value::Integer(0)),
            Some(Value::String(kind.into())),
            Some(Value::String("public".into())),
            Some(Value::String(dependency_name.into())),
            None,
            sequence_schema.map(|value| Value::String(value.into())),
            sequence_name.map(|value| Value::String(value.into())),
            owner_schema.map(|value| Value::String(value.into())),
            owner_table.map(|value| Value::String(value.into())),
            owner_column.map(|value| Value::String(value.into())),
            usage.map(|value| Value::String(value.into())),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    }
}

#[test]
fn parse_sequence_default_and_owned_by_edges_preserve_exact_identities() {
    let default = sequence_dependency_result(
        "sequence",
        "orders_id_seq",
        Some("public"),
        Some("orders_id_seq"),
        Some("public"),
        Some("orders"),
        Some("id"),
        Some("column_default"),
    );
    let (_, _, dependencies, _, _, usages, complete) = parse_object_dependency_catalog(
        &default,
        false,
        Some(SequenceDependencyUsageKind::ColumnDefault),
    )
    .unwrap();
    assert!(complete);
    assert_eq!(dependencies[0].kind, "sequence");
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].sequence.schema.as_deref(), Some("public"));
    assert_eq!(usages[0].sequence.name, "orders_id_seq");
    assert_eq!(usages[0].owner_table.schema.as_deref(), Some("public"));
    assert_eq!(usages[0].owner_table.name, "orders");
    assert_eq!(usages[0].column_name, "id");
    assert_eq!(usages[0].usage, SequenceDependencyUsageKind::ColumnDefault);

    let owned = sequence_dependency_result(
        "table",
        "orders",
        Some("public"),
        Some("orders_id_seq"),
        Some("public"),
        Some("orders"),
        Some("id"),
        Some("owned_by"),
    );
    let (_, _, dependencies, _, _, usages, complete) =
        parse_object_dependency_catalog(&owned, false, Some(SequenceDependencyUsageKind::OwnedBy))
            .unwrap();
    assert!(complete);
    assert_eq!(dependencies[0].kind, "table");
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].sequence.name, "orders_id_seq");
    assert_eq!(usages[0].owner_table.name, "orders");
    assert_eq!(usages[0].column_name, "id");
    assert_eq!(usages[0].usage, SequenceDependencyUsageKind::OwnedBy);
}

#[test]
fn parse_sequence_usage_fails_closed_for_wrong_missing_or_ambiguous_metadata() {
    let wrong_owner = sequence_dependency_result(
        "sequence",
        "orders_id_seq",
        Some("public"),
        Some("some_other_sequence"),
        Some("public"),
        Some("other_table"),
        Some("id"),
        Some("column_default"),
    );
    let (_, _, _, _, _, _, complete) = parse_object_dependency_catalog(
        &wrong_owner,
        false,
        Some(SequenceDependencyUsageKind::ColumnDefault),
    )
    .unwrap();
    assert!(
        !complete,
        "metadata for a different dependency must fail closed"
    );

    let missing_column = sequence_dependency_result(
        "sequence",
        "orders_id_seq",
        Some("public"),
        Some("orders_id_seq"),
        Some("public"),
        Some("orders"),
        None,
        Some("column_default"),
    );
    let (_, _, _, _, _, _, complete) = parse_object_dependency_catalog(
        &missing_column,
        false,
        Some(SequenceDependencyUsageKind::ColumnDefault),
    )
    .unwrap();
    assert!(!complete, "unresolved owner column must fail closed");

    let missing_schema = sequence_dependency_result(
        "sequence",
        "orders_id_seq",
        None,
        Some("orders_id_seq"),
        Some("public"),
        Some("orders"),
        Some("id"),
        Some("column_default"),
    );
    let (_, _, _, _, _, _, complete) = parse_object_dependency_catalog(
        &missing_schema,
        false,
        Some(SequenceDependencyUsageKind::ColumnDefault),
    )
    .unwrap();
    assert!(!complete, "unqualified sequence identity must fail closed");

    let mut ambiguous = sequence_dependency_result(
        "table",
        "orders",
        Some("public"),
        Some("orders_id_seq"),
        Some("public"),
        Some("orders"),
        Some("id"),
        Some("owned_by"),
    );
    ambiguous.rows.push(ambiguous.rows[0].clone());
    ambiguous.rows[1][6] = Some(Value::String("other_schema".into()));
    ambiguous.rows[1][7] = Some(Value::String("orders_id_seq".into()));
    let (_, _, _, _, _, usages, complete) = parse_object_dependency_catalog(
        &ambiguous,
        false,
        Some(SequenceDependencyUsageKind::OwnedBy),
    )
    .unwrap();
    assert_eq!(usages.len(), 2);
    assert!(
        !complete,
        "multiple owner edges for one sequence are ambiguous"
    );
}

#[test]
fn parse_object_list_maps_name_and_schema() {
    let result = QueryResult {
        columns: vec![col("schema"), col("name")],
        rows: vec![vec![
            Some(Value::String("public".into())),
            Some(Value::String("fn_ok".into())),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    let objects = parse_object_list(&result, "function").unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].name, "fn_ok");
    assert_eq!(objects[0].schema.as_deref(), Some("public"));
}

#[test]
fn parse_object_list_preserves_routine_and_trigger_identity() {
    let result = QueryResult {
        columns: vec![
            col("schema"),
            col("name"),
            col("signature"),
            col("target_schema"),
            col("target_name"),
        ],
        rows: vec![vec![
            Some(Value::String("public".into())),
            Some(Value::String("lookup".into())),
            Some(Value::String("integer".into())),
            Some(Value::String("public".into())),
            Some(Value::String("orders".into())),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    let objects = parse_object_list(&result, "function").unwrap();
    assert_eq!(objects[0].signature.as_deref(), Some("integer"));
    assert_eq!(objects[0].target_schema.as_deref(), Some("public"));
    assert_eq!(objects[0].target_name.as_deref(), Some("orders"));
}

#[test]
fn parse_object_list_preserves_empty_zero_argument_signature() {
    let result = QueryResult {
        columns: vec![col("name"), col("signature")],
        rows: vec![vec![
            Some(Value::String("zero_arg".into())),
            Some(Value::String(String::new())),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    let objects = parse_object_list(&result, "function").unwrap();
    assert_eq!(objects[0].signature.as_deref(), Some(""));
}

#[test]
fn parse_object_list_empty_when_no_columns() {
    let result = QueryResult {
        columns: vec![],
        rows: vec![],
        rows_affected: None,
        execution_time_ms: 0,
    };
    assert!(parse_object_list(&result, "function").unwrap().is_empty());
}

#[test]
fn parse_dependency_catalog_preserves_qualified_overloaded_identities() {
    let result = QueryResult {
        columns: vec![
            col("selected_count"),
            col("unsupported_count"),
            col("kind"),
            col("dependency_schema"),
            col("name"),
            col("signature"),
        ],
        rows: vec![vec![
            Some(Value::Integer(1)),
            Some(Value::Integer(0)),
            Some(Value::String("function".into())),
            Some(Value::String("sales".into())),
            Some(Value::String("normalize".into())),
            Some(Value::String("text, integer".into())),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    let (
        selected,
        unsupported,
        dependencies,
        type_usages,
        type_usage_complete,
        sequence_usages,
        sequence_usage_complete,
    ) = parse_object_dependency_catalog(&result, false, None).unwrap();
    assert_eq!(selected, Some(1));
    assert_eq!(unsupported, Some(0));
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].kind, "function");
    assert_eq!(dependencies[0].schema.as_deref(), Some("sales"));
    assert_eq!(dependencies[0].name, "normalize");
    assert_eq!(dependencies[0].signature.as_deref(), Some("text, integer"));
    assert!(type_usages.is_empty());
    assert!(type_usage_complete);
    assert!(sequence_usages.is_empty());
    assert!(sequence_usage_complete);
}

#[test]
fn parse_dependency_catalog_exposes_declared_column_type_usage() {
    let result = QueryResult {
        columns: vec![
            col("selected_count"),
            col("unsupported_count"),
            col("kind"),
            col("dependency_schema"),
            col("name"),
            col("signature"),
            col("type_usage"),
            col("column_name"),
        ],
        rows: vec![vec![
            Some(Value::Integer(1)),
            Some(Value::Integer(0)),
            Some(Value::String("type".into())),
            Some(Value::String("public".into())),
            Some(Value::String("account_status".into())),
            None,
            Some(Value::String("column_type".into())),
            Some(Value::String("status".into())),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    let (_, _, dependencies, usages, complete, sequence_usages, sequence_complete) =
        parse_object_dependency_catalog(&result, true, None).unwrap();
    assert!(complete);
    assert_eq!(dependencies.len(), 1);
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].dependency.name, "account_status");
    assert_eq!(usages[0].usage, TypeDependencyUsageKind::ColumnType);
    assert_eq!(usages[0].column_name.as_deref(), Some("status"));
    assert!(sequence_usages.is_empty());
    assert!(sequence_complete);
}

#[test]
fn parse_dependency_catalog_marks_unattributed_type_use_incomplete() {
    let result = QueryResult {
        columns: vec![
            col("selected_count"),
            col("unsupported_count"),
            col("kind"),
            col("dependency_schema"),
            col("name"),
            col("signature"),
            col("type_usage"),
            col("column_name"),
        ],
        rows: vec![vec![
            Some(Value::Integer(1)),
            Some(Value::Integer(0)),
            Some(Value::String("type".into())),
            Some(Value::String("public".into())),
            Some(Value::String("account_status".into())),
            None,
            None,
            None,
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    let (_, _, dependencies, usages, complete, _, sequence_complete) =
        parse_object_dependency_catalog(&result, true, None).unwrap();
    assert_eq!(dependencies.len(), 1);
    assert!(usages.is_empty());
    assert!(!complete);
    assert!(sequence_complete);
}

#[test]
fn dependency_catalog_rejects_unqualified_known_edges() {
    let result = QueryResult {
        columns: vec![
            col("selected_count"),
            col("unsupported_count"),
            col("kind"),
            col("dependency_schema"),
            col("name"),
            col("signature"),
        ],
        rows: vec![vec![
            Some(Value::Integer(1)),
            Some(Value::Integer(0)),
            Some(Value::String("table".into())),
            None,
            Some(Value::String("customers".into())),
            None,
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    assert!(parse_object_dependency_catalog(&result, true, None).is_err());
}

#[test]
fn extract_object_ddl_prefers_named_column() {
    let result = QueryResult {
        columns: vec![col("Function"), col("Create Function")],
        rows: vec![vec![
            Some(Value::String("fn_ok".into())),
            Some(Value::String("CREATE FUNCTION fn_ok() ...".into())),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    let ddl = extract_object_ddl(&result);
    assert!(ddl.contains("CREATE FUNCTION"));
}

#[test]
fn checked_ddl_rejects_missing_and_ambiguous_results() {
    let empty = QueryResult {
        columns: vec![col("ddl")],
        rows: vec![],
        rows_affected: None,
        execution_time_ms: 0,
    };
    assert!(extract_object_ddl_checked(&empty).is_err());

    let ambiguous = QueryResult {
        columns: vec![col("ddl")],
        rows: vec![
            vec![Some(Value::String("CREATE FUNCTION a()".into()))],
            vec![Some(Value::String("CREATE FUNCTION a(integer)".into()))],
        ],
        rows_affected: None,
        execution_time_ms: 0,
    };
    assert!(extract_object_ddl_checked(&ambiguous).is_err());
}

#[test]
fn checked_ddl_decodes_only_valid_utf8_bytes() {
    let ddl = format!(
        "CREATE VIEW `fixture_view` AS SELECT '{}';",
        "雪".repeat(300)
    );
    let encoded = QueryResult {
        columns: vec![col("ddl")],
        rows: vec![vec![Some(Value::Bytes(ddl.as_bytes().to_vec()))]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    assert_eq!(extract_object_ddl_checked(&encoded).unwrap(), ddl);

    let invalid_utf8 = QueryResult {
        columns: vec![col("ddl")],
        rows: vec![vec![Some(Value::Bytes(vec![0x43, 0x52, 0xff, 0x54]))]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    assert!(extract_object_ddl_checked(&invalid_utf8).is_err());
}

#[test]
fn mysql_view_metadata_is_extracted_as_required_creation_semantics() {
    let result = mysql_view_catalog_result(
        "SELECT 1",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let show_create = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 1",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );

    let metadata = extract_mysql_view_metadata(&result, &show_create).unwrap();
    assert_eq!(metadata.algorithm, "UNDEFINED");
    assert!(!metadata.has_explicit_column_list);
    assert_eq!(metadata.definer, "migrator@localhost");
    assert_eq!(metadata.security_type, "DEFINER");
    assert_eq!(metadata.check_option, "NONE");
    assert_eq!(metadata.character_set_client, "utf8mb4");
    assert_eq!(metadata.collation_connection, "utf8mb4_0900_ai_ci");

    let non_default_show_create = mysql_show_view_result(
        "CREATE ALGORITHM=MERGE DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` (`label`) AS SELECT 1",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let non_default = extract_mysql_view_metadata(&result, &non_default_show_create).unwrap();
    assert_eq!(non_default.algorithm, "MERGE");
    assert!(non_default.has_explicit_column_list);
}

#[test]
fn mysql_view_metadata_rejects_mixed_catalog_snapshots() {
    let catalog = mysql_view_catalog_result(
        "SELECT 1",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let show = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 1",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );

    for (field, changed) in [
        ("view_definer", "other@localhost"),
        ("view_security_type", "INVOKER"),
        ("view_check_option", "CASCADED"),
        ("view_character_set_client", "latin1"),
        ("view_collation_connection", "latin1_swedish_ci"),
    ] {
        let mut mismatched = catalog.clone();
        let index = column_index(&mismatched.columns, &[field]).unwrap();
        mismatched.rows[0][index] = Some(Value::String(changed.into()));
        assert!(
            extract_mysql_view_metadata(&mismatched, &show).is_err(),
            "mixed snapshot must be rejected when {field} differs"
        );
    }

    let changed_body = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 2",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let error = extract_mysql_view_metadata(&catalog, &changed_body).unwrap_err();
    assert!(error.to_string().contains("different query bodies"));
}

#[test]
fn mysql_view_metadata_rejects_show_create_semantics_not_in_catalog_snapshot() {
    let catalog = mysql_view_catalog_result(
        "SELECT 1",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    for (ddl, character_set, collation) in [
        (
            "CREATE ALGORITHM=UNDEFINED DEFINER=`other`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 1",
            "utf8mb4",
            "utf8mb4_0900_ai_ci",
        ),
        (
            "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY INVOKER VIEW `source_db`.`item_view` AS SELECT 1",
            "utf8mb4",
            "utf8mb4_0900_ai_ci",
        ),
        (
            "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 1 /*!50013 WITH CASCADED CHECK OPTION */",
            "utf8mb4",
            "utf8mb4_0900_ai_ci",
        ),
        (
            "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 1",
            "latin1",
            "utf8mb4_0900_ai_ci",
        ),
        (
            "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 1",
            "utf8mb4",
            "latin1_swedish_ci",
        ),
    ] {
        let show = mysql_show_view_result(ddl, character_set, collation);
        assert!(
            extract_mysql_view_metadata(&catalog, &show).is_err(),
            "SHOW CREATE VIEW metadata must agree with INFORMATION_SCHEMA"
        );
    }
}

#[test]
fn mysql_view_metadata_reads_check_option_from_show_create_version_comment() {
    let catalog = mysql_view_catalog_result(
        "SELECT 1",
        "migrator@localhost",
        "DEFINER",
        "CASCADED",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let show = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 1 /*!50013 WITH CASCADED CHECK OPTION */",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let metadata = extract_mysql_view_metadata(&catalog, &show).unwrap();
    assert_eq!(metadata.check_option, "CASCADED");

    let body_with_phrase = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT 'WITH CASCADED CHECK OPTION' /* WITH LOCAL CHECK OPTION */",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let catalog_none = mysql_view_catalog_result(
        "SELECT 'WITH CASCADED CHECK OPTION'",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let metadata = extract_mysql_view_metadata(&catalog_none, &body_with_phrase).unwrap();
    assert_eq!(metadata.check_option, "NONE");
}

#[test]
fn mysql_view_check_option_lexer_ignores_quoted_and_line_comment_phrases() {
    let cases = [
        r#"SELECT 'prefix ''WITH LOCAL CHECK OPTION'' suffix'"#,
        r#"SELECT 'escaped \' WITH CASCADED CHECK OPTION'"#,
        "SELECT 1 -- WITH CASCADED CHECK OPTION\n -- WITH LOCAL CHECK OPTION",
    ];

    for sql in cases {
        assert_eq!(
            mysql_show_create_view_check_option(sql).unwrap(),
            "NONE",
            "quoted or commented text must not become view metadata: {sql}"
        );
    }
}

#[test]
fn mysql_view_metadata_normalizes_only_its_own_database_qualifiers() {
    let local_catalog = mysql_view_catalog_result(
        "SELECT c.id FROM `source_db`.`child` AS c",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let local_show = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT c.id FROM `child` AS c",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    assert!(extract_mysql_view_metadata(&local_catalog, &local_show).is_ok());

    let unquoted_catalog = mysql_view_catalog_result(
        "SELECT c.id FROM source_db.child AS c",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let unquoted_show = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT c.id FROM child AS c",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    assert!(extract_mysql_view_metadata(&unquoted_catalog, &unquoted_show).is_ok());

    let differently_cased_local = mysql_view_catalog_result(
        "SELECT c.id FROM `SOURCE_DB`.`child` AS c",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    assert!(extract_mysql_view_metadata(&differently_cased_local, &local_show).is_err());

    let external_catalog = mysql_view_catalog_result(
        "SELECT c.id FROM `archive_db`.`child` AS c",
        "migrator@localhost",
        "DEFINER",
        "NONE",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    let same_external_show = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT c.id FROM `archive_db`.`child` AS c",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    assert!(extract_mysql_view_metadata(&external_catalog, &same_external_show).is_ok());

    let external_changed_to_local = mysql_show_view_result(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`migrator`@`localhost` SQL SECURITY DEFINER VIEW `source_db`.`item_view` AS SELECT c.id FROM `child` AS c",
        "utf8mb4",
        "utf8mb4_0900_ai_ci",
    );
    assert!(extract_mysql_view_metadata(&external_catalog, &external_changed_to_local).is_err());
}

fn mysql_view_catalog_result(
    body: &str,
    definer: &str,
    security: &str,
    check_option: &str,
    character_set: &str,
    collation: &str,
) -> QueryResult {
    QueryResult {
        columns: vec![
            col("view_schema"),
            col("ddl"),
            col("view_definer"),
            col("view_security_type"),
            col("view_check_option"),
            col("view_character_set_client"),
            col("view_collation_connection"),
        ],
        rows: vec![vec![
            Some(Value::String("source_db".into())),
            Some(Value::String(body.into())),
            Some(Value::String(definer.into())),
            Some(Value::String(security.into())),
            Some(Value::String(check_option.into())),
            Some(Value::String(character_set.into())),
            Some(Value::String(collation.into())),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    }
}

fn mysql_show_view_result(ddl: &str, character_set: &str, collation: &str) -> QueryResult {
    QueryResult {
        columns: vec![
            col("Create View"),
            col("character_set_client"),
            col("collation_connection"),
        ],
        rows: vec![vec![
            Some(Value::String(ddl.into())),
            Some(Value::String(character_set.into())),
            Some(Value::String(collation.into())),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    }
}

#[test]
fn parse_privilege_list_skips_incomplete_rows() {
    let result = QueryResult {
        columns: vec![col("grantee"), col("schema"), col("name"), col("privilege")],
        rows: vec![
            vec![
                Some(Value::String("alice".into())),
                Some(Value::String("public".into())),
                Some(Value::String("users".into())),
                Some(Value::String("SELECT".into())),
            ],
            vec![None, None, None, None],
        ],
        rows_affected: None,
        execution_time_ms: 0,
    };
    let grants = parse_privilege_list(&result).unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].grantee, "alice");
}
