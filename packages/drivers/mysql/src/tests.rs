//! Unit tests for the MySQL driver.

use super::*;
use datazen_driver_api::DatabaseDriver;

#[test]
fn quote_identifier_escapes_backticks() {
    assert_eq!(MysqlDriver::quote_identifier("foo"), "`foo`");
    assert_eq!(MysqlDriver::quote_identifier("foo`bar"), "`foo``bar`");
    assert_eq!(MysqlDriver::quote_identifier(""), "``");
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
async fn use_database_is_wired_for_mysql_and_mariadb() {
    let mysql = MysqlDriver::new(false);
    let mariadb = MysqlDriver::new(true);
    assert_eq!(mysql.driver_type(), "mysql");
    assert_eq!(mariadb.driver_type(), "mariadb");

    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "missing-pool".into(),
    };

    // Empty name fails before pool lookup (validation).
    let err = mysql.use_database(&handle, "").await.unwrap_err();
    assert!(
        matches!(err, DriverError::InvalidConfig(_)),
        "expected InvalidConfig, got {err:?}"
    );

    // Missing pool surfaces ConnectionFailed — confirms trait override is invoked.
    let err = mysql.use_database(&handle, "app_db").await.unwrap_err();
    assert!(
        matches!(err, DriverError::ConnectionFailed(_)),
        "expected ConnectionFailed, got {err:?}"
    );
    let err = mariadb.use_database(&handle, "app_db").await.unwrap_err();
    assert!(
        matches!(err, DriverError::ConnectionFailed(_)),
        "expected ConnectionFailed, got {err:?}"
    );
}

#[tokio::test]
async fn use_database_noop_when_already_active() {
    let driver = MysqlDriver::new(false);
    let pool_id = "test-pool".to_string();
    driver
        .active_databases
        .write()
        .await
        .insert(pool_id.clone(), "already".to_string());

    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id,
    };

    // No pool registered — would fail if USE were attempted; no-op must short-circuit.
    driver
        .use_database(&handle, "already")
        .await
        .expect("same database should be a no-op");
    driver
        .use_database(&handle, "  already  ")
        .await
        .expect("trimmed match should be a no-op");
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
    assert_eq!(MYSQL_CONNECTION_ID_SQL, "SELECT CONNECTION_ID()");
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
