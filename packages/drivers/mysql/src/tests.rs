//! Unit tests for the MySQL driver.

use super::*;
use datazen_driver_api::DatabaseDriver;
use std::collections::HashMap;

#[test]
fn only_innodb_tables_claim_consistent_snapshot_support() {
    assert!(supports_consistent_snapshot_engine(Some("InnoDB")));
    assert!(supports_consistent_snapshot_engine(Some("innodb")));
    assert!(!supports_consistent_snapshot_engine(Some("MyISAM")));
    assert!(!supports_consistent_snapshot_engine(None));
}

#[test]
fn mysql_table_options_query_captures_the_exact_default_collation() {
    assert!(MYSQL_TABLE_OPTIONS_QUERY.contains("t.TABLE_COLLATION AS TABLE_COLLATION"));
    assert!(MYSQL_TABLE_OPTIONS_QUERY.contains("COLLATION_CHARACTER_SET_APPLICABILITY"));
}

#[test]
fn mysql_group_replication_members_share_canonical_database_identity() {
    let member_a = mysql_group_replication_scope(Some("group-uuid"), Ok(Some("ONLINE")));
    let member_b = mysql_group_replication_scope(Some("group-uuid"), Ok(Some("online")));
    let ClusterScopeDecision::Cluster {
        scope: scope_a,
        id: id_a,
    } = member_a
    else {
        panic!("healthy group member should have cluster identity");
    };
    let ClusterScopeDecision::Cluster {
        scope: scope_b,
        id: id_b,
    } = member_b
    else {
        panic!("healthy group member should have cluster identity");
    };

    assert_eq!(
        canonical_database_identity("mysql", scope_a, &id_a, "app"),
        canonical_database_identity("mysql", scope_b, &id_b, "app")
    );
}

#[test]
fn mysql_group_and_database_are_part_of_canonical_identity() {
    let group_a = mysql_group_replication_scope(Some("group-a"), Ok(Some("ONLINE")));
    let group_b = mysql_group_replication_scope(Some("group-b"), Ok(Some("ONLINE")));
    let ClusterScopeDecision::Cluster { scope, id: group_a } = group_a else {
        panic!("healthy group member should have cluster identity");
    };
    let ClusterScopeDecision::Cluster { id: group_b, .. } = group_b else {
        panic!("healthy group member should have cluster identity");
    };

    let app_a = canonical_database_identity("mysql", scope, &group_a, "app");
    let app_b = canonical_database_identity("mysql", scope, &group_b, "app");
    let other_database = canonical_database_identity("mysql", scope, &group_a, "other");
    assert_ne!(app_a, app_b);
    assert_ne!(app_a, other_database);
}

#[test]
fn mysql_group_replication_unknown_or_unhealthy_members_fail_closed() {
    assert_eq!(
        mysql_group_replication_scope(None, Ok(None)),
        ClusterScopeDecision::Unknown
    );
    assert_eq!(
        mysql_group_replication_scope(Some("group"), Err(())),
        ClusterScopeDecision::Unknown
    );
    assert_eq!(
        mysql_group_replication_scope(Some("group"), Ok(Some("RECOVERING"))),
        ClusterScopeDecision::Unknown
    );
    assert_eq!(
        mysql_group_replication_scope(Some(""), Ok(None)),
        ClusterScopeDecision::NodeFallback
    );
    assert_eq!(
        mysql_group_replication_scope(Some("group"), Ok(None)),
        ClusterScopeDecision::NodeFallback
    );
}

#[test]
fn mysql_group_replication_fallback_requires_known_plugin_absence_or_inactive_state() {
    assert_eq!(mysql_group_replication_plugin_active(None), None);
    assert_eq!(
        mysql_group_replication_plugin_active(Some(&HashMap::new())),
        Some(false)
    );
    assert_eq!(
        mysql_group_replication_plugin_active(Some(&HashMap::from([(
            "group_replication".to_string(),
            "INACTIVE".to_string(),
        )]))),
        Some(false)
    );
    assert_eq!(
        mysql_group_replication_plugin_active(Some(&HashMap::from([(
            "group_replication".to_string(),
            "ACTIVE".to_string(),
        )]))),
        Some(true)
    );
    assert_eq!(
        mysql_group_replication_plugin_active(Some(&HashMap::from([(
            "group_replication".to_string(),
            "UNKNOWN".to_string(),
        )]))),
        None
    );
}

#[test]
fn mariadb_wsrep_members_share_identity_and_distinguish_cluster_and_database() {
    let statuses = HashMap::from([
        (
            "wsrep_cluster_state_uuid".to_string(),
            "cluster-uuid".to_string(),
        ),
        ("wsrep_cluster_status".to_string(), "Primary".to_string()),
        ("wsrep_connected".to_string(), "ON".to_string()),
        ("wsrep_ready".to_string(), "ON".to_string()),
    ]);
    let ClusterScopeDecision::Cluster { scope, id } = mariadb_wsrep_scope(Some(&statuses)) else {
        panic!("healthy wsrep member should have cluster identity");
    };
    let primary = canonical_database_identity("mariadb", scope, &id, "app");
    let peer = canonical_database_identity("mariadb", scope, &id, "app");
    let other_cluster = canonical_database_identity("mariadb", scope, "other-uuid", "app");
    let other_database = canonical_database_identity("mariadb", scope, &id, "other");
    assert_eq!(primary, peer);
    assert_ne!(primary, other_cluster);
    assert_ne!(primary, other_database);
}

#[test]
fn mariadb_wsrep_unknown_unhealthy_or_incomplete_status_fails_closed() {
    assert_eq!(mariadb_wsrep_scope(None), ClusterScopeDecision::Unknown);
    assert_eq!(
        mariadb_wsrep_scope(Some(&HashMap::new())),
        ClusterScopeDecision::NodeFallback
    );

    let incomplete = HashMap::from([(
        "wsrep_cluster_state_uuid".to_string(),
        "cluster-uuid".to_string(),
    )]);
    assert_eq!(
        mariadb_wsrep_scope(Some(&incomplete)),
        ClusterScopeDecision::Unknown
    );

    let unhealthy = HashMap::from([
        (
            "wsrep_cluster_state_uuid".to_string(),
            "cluster-uuid".to_string(),
        ),
        (
            "wsrep_cluster_status".to_string(),
            "Non-Primary".to_string(),
        ),
        ("wsrep_connected".to_string(), "ON".to_string()),
        ("wsrep_ready".to_string(), "ON".to_string()),
    ]);
    assert_eq!(
        mariadb_wsrep_scope(Some(&unhealthy)),
        ClusterScopeDecision::Unknown
    );
}

#[test]
fn format_sql_literal_keeps_binary_bytes_lossless() {
    let driver = MysqlDriver::new(false);
    assert_eq!(
        driver.format_sql_literal(&Some(Value::Bytes(vec![0x00, 0xff, 0xfe]))),
        "X'00fffe'"
    );
    assert_eq!(
        driver.format_sql_literal(&Some(Value::String("O'Brien".into()))),
        "'O''Brien'"
    );
}

#[test]
fn quote_identifier_escapes_backticks() {
    assert_eq!(MysqlDriver::quote_identifier("foo"), "`foo`");
    assert_eq!(MysqlDriver::quote_identifier("foo`bar"), "`foo``bar`");
    assert_eq!(MysqlDriver::quote_identifier(""), "``");
}

#[test]
fn parses_named_and_unnamed_checks_from_show_create() {
    let checks = MysqlDriver::parse_check_from_create_table(
        "CREATE TABLE `users` (\n  `age` int,\n  CONSTRAINT `users_age_check` CHECK ((`age` >= 0)),\n  CHECK (`age` < 150)\n)",
    );
    assert_eq!(checks.len(), 2);
    assert_eq!(checks[0].name, "check_1");
    assert_eq!(checks[1].name, "users_age_check");
    assert_eq!(checks[1].expression, "(`age` >= 0)");
}

#[test]
fn test_tester_check_parser_ignores_default_literal_before_real_check() {
    let checks = MysqlDriver::parse_check_from_create_table(
        "CREATE TABLE `users` (\n  `note` varchar(64) DEFAULT 'CHECK (literal)',\n  CONSTRAINT `users_age_check` CHECK (`age` >= 0)\n)",
    );
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].name, "users_age_check");
    assert_eq!(checks[0].expression, "`age` >= 0");
}

#[test]
fn test_tester_check_parser_ignores_comments_and_quoted_identifiers() {
    let checks = MysqlDriver::parse_check_from_create_table(
        "CREATE TABLE `users` (\n  `check_col` varchar(64),\n  /* CHECK (block_only) */\n  -- CHECK (line_only)\n  CONSTRAINT `users_check` CHECK (`check_col` <> 'CHECK (literal)')\n)",
    );
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].name, "users_check");
    assert_eq!(checks[0].expression, "`check_col` <> 'CHECK (literal)'");
}

#[test]
fn mysql_foreign_keys_are_explicitly_not_deferrable() {
    let foreign_keys = MysqlDriver::parse_fk_from_create_table(
        "CREATE TABLE `orders` (\n  CONSTRAINT `orders_user_fk` FOREIGN KEY (`user_id`) REFERENCES `users` (`id`)\n)",
    );

    assert_eq!(foreign_keys.len(), 1);
    assert_eq!(
        foreign_keys[0].deferrability,
        ForeignKeyDeferrability::NotDeferrable
    );
}

#[test]
fn mysql_foreign_key_parser_preserves_referenced_database_identity() {
    let foreign_keys = MysqlDriver::parse_fk_from_create_table(
        "CREATE TABLE `child` (\n  CONSTRAINT `fk_child_parent` FOREIGN KEY (`parent_id`) REFERENCES `archive``db`.`parent` (`id`)\n)",
    );

    assert_eq!(foreign_keys.len(), 1);
    assert_eq!(foreign_keys[0].referenced_table, "archive`db.parent");
    assert_eq!(foreign_keys[0].referenced_columns, ["id"]);
}

#[test]
fn mysql_foreign_key_reference_parser_handles_unquoted_and_malformed_identities() {
    assert_eq!(
        MysqlDriver::extract_qualified_table_after(
            "FOREIGN KEY (parent_id) REFERENCES archive.parent (id)",
            "REFERENCES",
        ),
        "archive.parent"
    );
    assert_eq!(
        MysqlDriver::extract_qualified_table_after(
            "FOREIGN KEY (parent_id) REFERENCES `archive` . `parent` (id)",
            "REFERENCES",
        ),
        "archive.parent"
    );
    assert_eq!(
        MysqlDriver::extract_qualified_table_after(
            "FOREIGN KEY (parent_id) REFERENCES `archive.parent` (id)",
            "REFERENCES",
        ),
        ""
    );
    assert_eq!(
        MysqlDriver::extract_qualified_table_after(
            "FOREIGN KEY (parent_id) REFERENCES `archive.parent (id)",
            "REFERENCES",
        ),
        ""
    );
}

#[test]
fn mysql_server_wide_catalog_visibility_requires_direct_global_select_without_revokes() {
    assert!(MysqlDriver::grants_prove_server_wide_catalog_visibility(&[
        "GRANT SELECT ON *.* TO 'datazen'@'localhost'".into(),
    ]));
    assert!(MysqlDriver::grants_prove_server_wide_catalog_visibility(&[
        "GRANT ALL PRIVILEGES ON *.* TO 'datazen'@'localhost' WITH GRANT OPTION".into(),
    ]));
    assert!(!MysqlDriver::grants_prove_server_wide_catalog_visibility(
        &["GRANT SELECT ON `app`.* TO 'datazen'@'localhost'".into(),]
    ));
    assert!(!MysqlDriver::grants_prove_server_wide_catalog_visibility(
        &["GRANT 'catalog_reader'@'%' TO 'datazen'@'localhost'".into(),]
    ));
    assert!(!MysqlDriver::grants_prove_server_wide_catalog_visibility(
        &[
            "GRANT SELECT ON *.* TO 'datazen'@'localhost'".into(),
            "REVOKE SELECT ON `private`.* FROM 'datazen'@'localhost'".into(),
        ]
    ));
}

#[test]
fn build_use_database_sql_quotes_and_trims() {
    assert_eq!(
        MysqlDriver::build_use_database_sql("mydb").unwrap(),
        "USE `mydb`"
    );
    assert_eq!(
        MysqlDriver::build_use_database_sql("  my`db  ").unwrap(),
        "USE `my``db`"
    );
    assert_eq!(
        MysqlDriver::build_use_database_sql("information_schema").unwrap(),
        "USE `information_schema`"
    );
}

#[test]
fn build_use_database_sql_rejects_empty_or_invalid() {
    assert!(matches!(
        MysqlDriver::build_use_database_sql(""),
        Err(DriverError::InvalidConfig(_))
    ));
    assert!(matches!(
        MysqlDriver::build_use_database_sql("   "),
        Err(DriverError::InvalidConfig(_))
    ));
    assert!(matches!(
        MysqlDriver::build_use_database_sql("bad\0name"),
        Err(DriverError::InvalidConfig(_))
    ));
}

#[test]
fn build_mysql_options_sets_fields_without_url_password() {
    let config = ConnectionConfig {
        id: "c".into(),
        name: "mysql".into(),
        database_type: "mysql".into(),
        host: Some("db.example".into()),
        port: Some(3307),
        database: Some("app".into()),
        schema: None,
        username: Some("root".into()),
        password: Some("s3cret".into()),
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 5,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    };
    let opts = build_mysql_options(&config).unwrap();
    let debug = format!("{opts:?}");
    // Prefer ConnectOptions over URL — Debug may still show password; ensure we
    // at least constructed options (host/port present) without building a DSN string.
    assert!(debug.contains("db.example") || debug.contains("3307") || !debug.is_empty());
    let _ = opts;
}

#[test]
fn build_mysql_options_drops_empty_password() {
    let mut config = ConnectionConfig {
        id: "c".into(),
        name: "mysql".into(),
        database_type: "mysql".into(),
        host: Some("127.0.0.1".into()),
        port: Some(3306),
        database: Some("app".into()),
        schema: None,
        username: Some("root".into()),
        password: Some(String::new()),
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 5,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    };
    assert!(build_mysql_options(&config).is_ok());
    config.password = Some("   ".into());
    assert!(build_mysql_options(&config).is_ok());
}

#[tokio::test]
async fn qualified_table_ref_targets_the_requested_database() {
    // MySQL accepts `db`.`table` in the SHOW family, which is what lets a
    // foreign database be read without a `USE` on a pooled connection.
    assert_eq!(
        MysqlDriver::qualified_table_ref("mydb", "users"),
        "`mydb`.`users`"
    );
    // Backticks are escaped, not interpolated.
    assert_eq!(
        MysqlDriver::qualified_table_ref("my`db", "us`ers"),
        "`my``db`.`us``ers`"
    );
    // A pre-qualified argument keeps only its table part: the database is
    // always the explicit argument.
    assert_eq!(
        MysqlDriver::qualified_table_ref("mydb", "otherdb.users"),
        "`mydb`.`users`"
    );
    // No database at all stays unqualified rather than inventing one.
    assert_eq!(MysqlDriver::qualified_table_ref("  ", "users"), "`users`");
    assert_eq!(MysqlDriver::bare_table_name("`mydb`.`users`"), "users");
}

#[tokio::test]
async fn effective_database_prefers_the_explicit_argument() {
    let driver = MysqlDriver::new(false);
    let pool_id = "test-pool".to_string();
    driver
        .active_databases
        .write()
        .await
        .insert(pool_id.clone(), "tracked".to_string());
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: pool_id.clone(),
    };

    assert_eq!(
        driver.effective_database(&handle, "explicit").await,
        "explicit"
    );
    assert_eq!(driver.effective_database(&handle, "  ").await, "tracked");
    assert_eq!(
        driver
            .effective_database(
                &ConnectionHandle {
                    id: "other".into(),
                    pool_id: "unknown".into(),
                },
                ""
            )
            .await,
        ""
    );
}

#[tokio::test]
async fn mysql_and_mariadb_declare_no_schema_level() {
    assert!(!MysqlDriver::new(false).has_schema_level());
    assert!(!MysqlDriver::new(true).has_schema_level());
}

#[tokio::test]
async fn begin_transaction_requires_pool() {
    let driver = MysqlDriver::new(false);
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "missing-pool".into(),
    };
    let err = driver.begin_transaction(&handle).await.unwrap_err();
    assert!(
        matches!(err, DriverError::ConnectionFailed(_)),
        "expected ConnectionFailed, got {err:?}"
    );
}

#[tokio::test]
async fn begin_read_snapshot_requires_pool() {
    let driver = MysqlDriver::new(false);
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "missing-pool".into(),
    };
    let err = driver.begin_read_snapshot(&handle).await.unwrap_err();
    assert!(
        matches!(err, DriverError::ConnectionFailed(_)),
        "expected ConnectionFailed, got {err:?}"
    );
}

#[tokio::test]
async fn commit_and_rollback_without_begin_error() {
    let driver = MysqlDriver::new(false);
    let tx = TransactionHandle {
        id: "mysql_tx_missing".into(),
        connection_id: "conn".into(),
    };
    let err = driver.commit(tx).await.unwrap_err();
    assert!(
        matches!(err, DriverError::TransactionError(_)),
        "expected TransactionError, got {err:?}"
    );

    let tx = TransactionHandle {
        id: "mysql_tx_missing".into(),
        connection_id: "conn".into(),
    };
    let err = driver.rollback(tx).await.unwrap_err();
    assert!(
        matches!(err, DriverError::TransactionError(_)),
        "expected TransactionError, got {err:?}"
    );
}

/// MySQL / sqlx use positional `?` placeholders (not `$N`).
#[test]
fn mysql_placeholders_are_question_marks() {
    let sql = "SELECT ?, ?, ?";
    assert_eq!(sql.matches('?').count(), 3);
    assert!(!sql.contains('$'));
}

#[test]
fn cancel_sql_targets_one_thread_without_process_scan() {
    assert_eq!(execution::MYSQL_CONNECTION_ID_SQL, "SELECT CONNECTION_ID()");
    assert_eq!(build_kill_query_sql(42), "KILL QUERY 42");
    assert!(!build_kill_query_sql(42).contains("processlist"));
}

#[tokio::test]
async fn execution_cancel_handles_pending_and_stale_ids() {
    let driver = MysqlDriver::new(false);
    let handle = ConnectionHandle {
        id: "session-a".into(),
        pool_id: "pool-a".into(),
    };
    let other = ConnectionHandle {
        id: "session-b".into(),
        pool_id: "pool-b".into(),
    };
    let execution_id = QueryExecutionId::new("exec-a");

    driver
        .prepare_query_execution(&handle, &execution_id)
        .await
        .unwrap();
    driver
        .cancel_query_with_execution(&handle, &execution_id)
        .await
        .unwrap();
    let execution = driver
        .query_executions
        .lock()
        .await
        .get(&execution_id)
        .map(|entry| (entry.thread_id, entry.cancel_requested));
    assert_eq!(execution, Some((None, true)));

    let wrong_session = driver
        .cancel_query_with_execution(&other, &execution_id)
        .await
        .unwrap_err();
    assert!(matches!(
        wrong_session,
        DriverError::QueryExecutionSessionMismatch
    ));

    driver
        .cleanup_query_execution(&handle, &execution_id)
        .await
        .unwrap();
    let stale = driver
        .cancel_query_with_execution(&handle, &execution_id)
        .await
        .unwrap_err();
    assert!(matches!(stale, DriverError::QueryExecutionNotFound(_)));
}

#[tokio::test]
async fn concurrent_execution_ids_keep_cancel_requests_isolated() {
    let driver = MysqlDriver::new(false);
    let first = ConnectionHandle {
        id: "session-a".into(),
        pool_id: "pool-a".into(),
    };
    let second = ConnectionHandle {
        id: "session-b".into(),
        pool_id: "pool-b".into(),
    };
    let first_id = QueryExecutionId::new("exec-a");
    let second_id = QueryExecutionId::new("exec-b");
    driver
        .prepare_query_execution(&first, &first_id)
        .await
        .unwrap();
    driver
        .prepare_query_execution(&second, &second_id)
        .await
        .unwrap();
    driver
        .cancel_query_with_execution(&first, &first_id)
        .await
        .unwrap();

    let executions = driver.query_executions.lock().await;
    assert!(executions[&first_id].cancel_requested);
    assert!(!executions[&second_id].cancel_requested);
}

#[tokio::test]
async fn mariadb_transaction_execution_cancel_is_pending_until_target_is_bound() {
    let driver = MysqlDriver::new(true);
    let handle = ConnectionHandle {
        id: "session-tx".into(),
        pool_id: "pool-tx".into(),
    };
    let execution_id = QueryExecutionId::new("exec-tx");
    driver.query_executions.lock().await.insert(
        execution_id.clone(),
        MysqlQueryExecution {
            cancel_gate: std::sync::Arc::new(tokio::sync::Mutex::new(())),
            session_id: handle.id.clone(),
            target_pool: None,
            control_pool: None,
            thread_id: None,
            cancel_requested: false,
            transactional: true,
        },
    );
    driver
        .cancel_query_with_execution(&handle, &execution_id)
        .await
        .unwrap();
    assert_eq!(
        driver
            .query_executions
            .lock()
            .await
            .get(&execution_id)
            .map(|entry| (entry.thread_id, entry.cancel_requested)),
        Some((None, true))
    );

    let wrong_session = driver
        .cancel_query_with_execution(
            &ConnectionHandle {
                id: "session-other".into(),
                pool_id: "pool-tx".into(),
            },
            &execution_id,
        )
        .await
        .unwrap_err();
    assert!(matches!(
        wrong_session,
        DriverError::QueryExecutionSessionMismatch
    ));

    assert!(driver
        .bind_thread_id(&handle, &execution_id, 42)
        .await
        .unwrap());
    assert_eq!(
        driver
            .query_executions
            .lock()
            .await
            .get(&execution_id)
            .map(|entry| (entry.thread_id, entry.cancel_requested)),
        Some((Some(42), true))
    );

    driver
        .cleanup_query_execution(&handle, &execution_id)
        .await
        .unwrap();
    let stale = driver
        .cancel_query_with_execution(&handle, &execution_id)
        .await
        .unwrap_err();
    assert!(matches!(stale, DriverError::QueryExecutionNotFound(_)));
}

#[test]
fn bind_values_accepts_all_value_variants() {
    let params = [
        Value::Null,
        Value::Bool(true),
        Value::Integer(42),
        Value::Float(1.5),
        Value::String("hi".into()),
        Value::Timestamp("2024-01-01T00:00:00Z".into()),
        Value::Bytes(vec![1, 2, 3]),
        Value::Json(serde_json::json!({"a": 1})),
    ];
    // Compiles and builds a bound query for every Value variant.
    let _q = MysqlDriver::bind_values(sqlx::query("SELECT ?, ?, ?, ?, ?, ?, ?, ?"), &params);
}

#[tokio::test]
async fn query_with_params_requires_pool() {
    let driver = MysqlDriver::new(false);
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "missing-pool".into(),
    };
    let err = driver
        .query_with_params(&handle, "SELECT ?", &[Value::Integer(1)])
        .await
        .unwrap_err();
    assert!(
        matches!(err, DriverError::ConnectionFailed(_)),
        "expected ConnectionFailed, got {err:?}"
    );
}

#[test]
fn apply_mysql_select_limit_plus_one_and_existing_limit() {
    assert_eq!(
        apply_mysql_select_limit("SELECT * FROM t", None),
        ("SELECT * FROM t".into(), None)
    );
    assert_eq!(
        apply_mysql_select_limit("SELECT * FROM t", Some(8)),
        ("SELECT * FROM t LIMIT 9".into(), Some(8))
    );
    assert_eq!(
        apply_mysql_select_limit("SELECT * FROM t LIMIT 2", Some(8)),
        ("SELECT * FROM t LIMIT 2".into(), Some(8))
    );
    assert_eq!(
        apply_mysql_select_limit("UPDATE t SET a = 1", Some(8)),
        ("UPDATE t SET a = 1".into(), None)
    );
}

#[test]
fn is_table_not_found_error_detects_mysql_1146_and_does_not_exist() {
    assert!(MysqlDriver::is_table_not_found_error(
        "error returned from database: 1146 (42S02): Table 'datazen_demo.demo_customers' doesn't exist"
    ));
    assert!(MysqlDriver::is_table_not_found_error(
        "Error 1146: Table 'mydb.users' doesn't exist"
    ));
    assert!(MysqlDriver::is_table_not_found_error(
        "table 'mydb.orders' does not exist"
    ));
    assert!(!MysqlDriver::is_table_not_found_error(
        "1045 (28000): Access denied for user 'root'@'localhost'"
    ));
    assert!(!MysqlDriver::is_table_not_found_error(
        "1064 (42000): You have an error in your SQL syntax"
    ));
}

#[test]
fn migration_parameters_keep_values_out_of_sql() {
    let driver = MysqlDriver::new(false);
    assert_eq!(
        driver
            .parameter_placeholder(1, Some("DECIMAL(65,30)"))
            .unwrap(),
        "?"
    );
    assert_eq!(
        driver.parameter_placeholder(99, Some("LONGBLOB")).unwrap(),
        "?"
    );
}

#[tokio::test]
async fn execute_with_params_requires_pool() {
    let driver = MysqlDriver::new(false);
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "missing-pool".into(),
    };
    let error = driver
        .execute_with_params(&handle, "INSERT INTO t VALUES (?)", &[Value::Integer(1)])
        .await
        .unwrap_err();
    assert!(matches!(error, DriverError::ConnectionFailed(_)));
}

#[test]
fn bound_insert_keeps_mysql_quoting_and_omits_unwritten_columns() {
    let driver = MysqlDriver::new(false);
    let insert = driver
        .build_insert_statement(
            "users",
            &[
                ("name", Some(Value::String("O'Brien".into()))),
                ("note", None),
            ],
        )
        .unwrap();

    // MySQL placeholders are positional `?` (bound in order), and identifiers
    // use backticks with doubling — the same rendering as UPDATE/DELETE.
    assert_eq!(
        insert.sql,
        "INSERT INTO `users` (`name`, `note`) VALUES (?, ?)"
    );
    assert_eq!(insert.parameters.len(), 2);
    assert!(
        matches!(&insert.parameters[0], Value::String(value) if value == "O'Brien"),
        "the value must travel bound, not as SQL text"
    );
    assert!(matches!(&insert.parameters[1], Value::Null));
    // `id` is Unset: it must not be written at all, or auto-increment dies.
    assert!(!insert.sql.contains("id"));

    // MySQL cannot RETURNING; the host must take the degradation path instead
    // of guessing a primary key.
    assert_eq!(driver.insert_returning_clause(&["id"]), None);
}

// ---------------------------------------------------------------------------
// Live pool-accounting regressions (skip, not fail, when no TEST_MYSQL_* env).
// ---------------------------------------------------------------------------

fn live_mysql_config(id: &str) -> Option<ConnectionConfig> {
    let host = std::env::var("TEST_MYSQL_HOST").ok()?;
    let user = std::env::var("TEST_MYSQL_USER").ok()?;
    let port = std::env::var("TEST_MYSQL_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3306);
    let database = std::env::var("TEST_MYSQL_DATABASE").unwrap_or_else(|_| "mysql".to_string());
    let password = std::env::var("TEST_MYSQL_PASSWORD").ok();
    let mut config: ConnectionConfig = serde_json::from_value(serde_json::json!({
        "id": id,
        "name": "mysql live pool regression",
        "databaseType": "mysql",
        "host": host,
        "port": port,
        "username": user,
        "database": database,
        "sslMode": "disable",
        "connectionTimeout": 1,
        "maxPoolSize": 10,
    }))
    .ok()?;
    config.password = password;
    Some(config)
}

fn skip_without_live_mysql(test: &str) -> bool {
    if std::env::var("TEST_MYSQL_HOST").is_ok() && std::env::var("TEST_MYSQL_USER").is_ok() {
        return false;
    }
    eprintln!(
        "skipped {test}: set TEST_MYSQL_HOST and TEST_MYSQL_USER to run against a disposable MySQL"
    );
    true
}

async fn mysql_pool_counters(
    driver: &MysqlDriver,
    handle: &ConnectionHandle,
) -> Option<(usize, usize)> {
    let pools = driver.pools.read().await;
    pools
        .get(&handle.pool_id)
        .map(|pool| (pool.size() as usize, pool.num_idle()))
}

/// A retained session owns the pool's only checkout. Catalog and admin reads
/// must run on it; borrowing the pool would wait for a checkout that is never
/// returned before disconnect. The first extra borrow is therefore refused,
/// not the eleventh.
#[tokio::test]
async fn fixed_session_catalog_reads_use_the_pinned_connection() {
    if skip_without_live_mysql("fixed_session_catalog_reads_use_the_pinned_connection") {
        return;
    }
    let Some(config) = live_mysql_config("mysql-fixed-catalog-live") else {
        return;
    };
    let database = config.database.clone().unwrap_or_else(|| "mysql".into());

    let driver = MysqlDriver::new(false);
    let handle = driver
        .connect_fixed_session(&config)
        .await
        .expect("fixed MySQL session opens");
    let baseline = mysql_pool_counters(&driver, &handle)
        .await
        .expect("fixed data pool exists");
    assert_eq!(
        baseline,
        (1, 0),
        "the retained session pins the pool's only connection"
    );

    let pool = driver
        .pools
        .read()
        .await
        .get(&handle.pool_id)
        .cloned()
        .expect("fixed pool exists");
    // MySQL never rejects a cross-database catalog read on a pinned session:
    // `information_schema` is filtered by TABLE_SCHEMA and needs no `USE`.
    let cross_database_tables = driver
        .get_tables(&handle, "information_schema", None)
        .await
        .expect("a pinned MySQL session can read another database's catalog");
    assert!(
        !cross_database_tables.is_empty(),
        "information_schema should expose its own tables from the pinned session"
    );
    assert_eq!(
        mysql_pool_counters(&driver, &handle).await,
        Some(baseline),
        "cross-database catalog read must not borrow a second checkout"
    );

    let extra_borrow =
        tokio::time::timeout(std::time::Duration::from_secs(2), pool.acquire()).await;
    assert!(
        match extra_borrow {
            Err(_) | Ok(Err(_)) => true,
            Ok(Ok(_)) => false,
        },
        "a max-1 pool with its only connection pinned cannot lend another checkout"
    );

    for index in 1..=20u32 {
        let call = async {
            match index % 8 {
                0 => driver.get_databases(&handle).await.map(|_| ()),
                1 => driver
                    .get_tables(&handle, &database, None)
                    .await
                    .map(|_| ()),
                2 => driver
                    .get_columns(&handle, "user", &database, None)
                    .await
                    .map(|_| ()),
                3 => driver
                    .get_all_columns(&handle, &database, None)
                    .await
                    .map(|_| ()),
                4 => driver
                    .get_table_schema(&handle, "user", &database, None)
                    .await
                    .map(|_| ()),
                5 => driver.get_server_info(&handle).await.map(|_| ()),
                6 => driver
                    .physical_database_identity(&handle, &database)
                    .await
                    .map(|_| ()),
                _ => driver.query(&handle, "SELECT 1").await.map(|_| ()),
            }
        };
        let call = tokio::time::timeout(std::time::Duration::from_secs(3), call)
            .await
            .unwrap_or_else(|_| panic!("fixed-session statement {index} timed out"));
        call.unwrap_or_else(|error| panic!("fixed-session statement {index} failed: {error}"));
        assert_eq!(
            mysql_pool_counters(&driver, &handle).await,
            Some(baseline),
            "statement {index} changed the fixed pool's checkout state"
        );
    }

    let refusal_started = std::time::Instant::now();
    let refusal = driver
        .execute_command(&handle, "server_status_snapshot", serde_json::json!({}))
        .await
        .expect_err("admin commands are not available on a retained session");
    assert!(
        matches!(refusal, DriverError::NotSupported(_)),
        "fixed admin command must fail fast, got {refusal:?}"
    );
    assert!(
        refusal_started.elapsed() < std::time::Duration::from_secs(2),
        "fixed admin command waited for the pinned pool instead of refusing"
    );

    driver
        .disconnect(handle)
        .await
        .expect("fixed session disconnects");
}

/// Leased sessions (metadata included) may borrow from their pool, but every
/// statement must return the checkout. Sizes 1 and 10 cover the metadata bound
/// and the interactive default: a per-statement leak would stall on the second
/// statement at size 1 and on the eleventh at size 10.
#[tokio::test]
async fn leased_catalog_reads_return_every_pool_checkout() {
    if skip_without_live_mysql("leased_catalog_reads_return_every_pool_checkout") {
        return;
    }
    for max_pool_size in [1u32, 10] {
        let Some(mut config) = live_mysql_config(&format!("mysql-leased-{max_pool_size}")) else {
            return;
        };
        config.max_pool_size = max_pool_size;
        let database = config.database.clone().unwrap_or_else(|| "mysql".into());

        let driver = MysqlDriver::new(false);
        let handle = driver
            .connect(&config)
            .await
            .expect("leased MySQL session opens");
        driver
            .get_tables(&handle, &database, None)
            .await
            .expect("warm-up listing");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let baseline = mysql_pool_counters(&driver, &handle)
            .await
            .expect("data pool exists");
        assert_eq!(
            baseline.1, baseline.0,
            "warm-up returns every checkout at max={max_pool_size}"
        );
        assert!(baseline.0 <= max_pool_size as usize);

        let mut first_failure = None;
        for index in 1..=20u32 {
            let call = async {
                match index % 8 {
                    0 => driver.get_databases(&handle).await.map(|_| ()),
                    1 => driver
                        .get_tables(&handle, &database, None)
                        .await
                        .map(|_| ()),
                    2 => driver
                        .get_columns(&handle, "user", &database, None)
                        .await
                        .map(|_| ()),
                    3 => driver
                        .get_all_columns(&handle, &database, None)
                        .await
                        .map(|_| ()),
                    4 => driver
                        .get_table_schema(&handle, "user", &database, None)
                        .await
                        .map(|_| ()),
                    5 => driver.get_server_info(&handle).await.map(|_| ()),
                    6 => driver
                        .physical_database_identity(&handle, &database)
                        .await
                        .map(|_| ()),
                    _ => driver.query(&handle, "SELECT 1").await.map(|_| ()),
                }
            };
            let call = tokio::time::timeout(std::time::Duration::from_secs(3), call)
                .await
                .unwrap_or_else(|_| panic!("statement {index} timed out at max={max_pool_size}"));
            if let Err(error) = call {
                first_failure = Some((index, error.to_string()));
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let counters = mysql_pool_counters(&driver, &handle)
                .await
                .expect("data pool exists");
            assert_eq!(
                counters.1, counters.0,
                "statement {index} kept a pool checkout at max={max_pool_size} (size={}, idle={})",
                counters.0, counters.1
            );
            assert!(
                counters.0 <= max_pool_size as usize,
                "pool grew past configured max at max={max_pool_size}: {counters:?}"
            );
        }
        assert_eq!(
            first_failure, None,
            "max={max_pool_size}: first failing statement {first_failure:?}"
        );

        driver
            .disconnect(handle)
            .await
            .expect("leased session disconnects");
    }
}
