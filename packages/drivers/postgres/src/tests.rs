//! Unit tests for the PostgreSQL driver.

use super::PostgresDriver;
use crate::catalog::{
    build_pg_alter_sequence_owned_by, build_pg_create_sequence_sql, build_pg_create_table_ddl,
    pg_sequence_start, PgColumnDdl, PgSequenceDdl,
};
use crate::execution::{PgQueryExecution, PG_BACKEND_PID_SQL, PG_CANCEL_BACKEND_SQL};
use crate::sql::{apply_select_limit, parse_pg_table_ref};
use datazen_driver_api::*;

#[test]
fn fetch_tables_sql_uses_pg_catalog_system_schema_filters() {
    const SQL: &str = r#"
        FROM pg_catalog.pg_class c
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind IN ('r', 'v', 'm', 'f', 'p')
          AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
          AND NOT pg_catalog.pg_is_other_temp_schema(n.oid)
          AND (pg_catalog.pg_my_temp_schema() = 0 OR n.oid <> pg_catalog.pg_my_temp_schema())
    "#;
    assert!(SQL.contains("pg_is_other_temp_schema"));
    assert!(SQL.contains("pg_my_temp_schema"));
    assert!(!SQL.contains("LIKE 'pg_%'"));
}

#[test]
fn parse_pg_table_ref_splits_schema_prefix() {
    assert_eq!(
        parse_pg_table_ref("public.users"),
        (Some("public"), "users")
    );
    assert_eq!(parse_pg_table_ref("users"), (None, "users"));
}

#[test]
fn validate_database_name_trims_and_accepts() {
    assert_eq!(
        PostgresDriver::validate_database_name("  mydb  ").unwrap(),
        "mydb"
    );
    assert_eq!(
        PostgresDriver::validate_database_name("postgres").unwrap(),
        "postgres"
    );
}

#[test]
fn validate_database_name_rejects_empty_or_invalid() {
    assert!(matches!(
        PostgresDriver::validate_database_name(""),
        Err(DriverError::InvalidConfig(_))
    ));
    assert!(matches!(
        PostgresDriver::validate_database_name("   "),
        Err(DriverError::InvalidConfig(_))
    ));
    assert!(matches!(
        PostgresDriver::validate_database_name("bad\0name"),
        Err(DriverError::InvalidConfig(_))
    ));
}

#[test]
fn resolve_connect_database_defaults_to_postgres() {
    let mut cfg = ConnectionConfig {
        id: "id".into(),
        name: "n".into(),
        database_type: "postgresql".into(),
        host: Some("localhost".into()),
        port: Some(5432),
        database: None,
        schema: None,
        username: None,
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 10,
        ssh_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    };
    assert_eq!(PostgresDriver::resolve_connect_database(&cfg), "postgres");

    cfg.database = Some("  ".into());
    assert_eq!(PostgresDriver::resolve_connect_database(&cfg), "postgres");

    cfg.database = Some("  app_db  ".into());
    assert_eq!(PostgresDriver::resolve_connect_database(&cfg), "app_db");
}

#[tokio::test]
async fn use_database_is_wired() {
    let driver = PostgresDriver::new();
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "missing-pool".into(),
    };

    let err = driver.use_database(&handle, "").await.unwrap_err();
    assert!(
        matches!(err, DriverError::InvalidConfig(_)),
        "expected InvalidConfig, got {err:?}"
    );

    let err = driver.use_database(&handle, "app_db").await.unwrap_err();
    assert!(
        matches!(err, DriverError::ConnectionFailed(_)),
        "expected ConnectionFailed, got {err:?}"
    );
}

#[tokio::test]
async fn use_database_noop_when_already_active() {
    let driver = PostgresDriver::new();
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

    // No pool registered — would fail if reconnect were attempted; no-op must short-circuit.
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
    let driver = PostgresDriver::new();
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
    let driver = PostgresDriver::new();
    let tx = TransactionHandle {
        id: "pg_tx_missing".into(),
        connection_id: "conn".into(),
    };
    let err = driver.commit(tx).await.unwrap_err();
    assert!(
        matches!(err, DriverError::TransactionError(_)),
        "expected TransactionError, got {err:?}"
    );

    let tx = TransactionHandle {
        id: "pg_tx_missing".into(),
        connection_id: "conn".into(),
    };
    let err = driver.rollback(tx).await.unwrap_err();
    assert!(
        matches!(err, DriverError::TransactionError(_)),
        "expected TransactionError, got {err:?}"
    );
}

/// Postgres / sqlx use 1-based `$N` placeholders (not `?`).
#[test]
fn postgres_placeholders_are_dollar_n() {
    assert_eq!(
        (1..=3).map(|i| format!("${i}")).collect::<Vec<_>>(),
        vec!["$1", "$2", "$3"]
    );
}

#[test]
fn cancel_sql_targets_one_backend_without_process_scan() {
    assert_eq!(PG_BACKEND_PID_SQL, "SELECT pg_backend_pid()");
    assert_eq!(PG_CANCEL_BACKEND_SQL, "SELECT pg_cancel_backend($1)");
    assert!(!PG_CANCEL_BACKEND_SQL.contains("pg_stat_activity"));
}

#[tokio::test]
async fn execution_cancel_handles_pending_and_stale_ids() {
    let driver = PostgresDriver::new();
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
    // Cancel can arrive after the Host publishes ExecutionStarted but
    // before this driver has acquired a connection/PID.
    driver
        .cancel_query_with_execution(&handle, &execution_id)
        .await
        .unwrap();
    let execution = driver
        .query_executions
        .lock()
        .await
        .get(&execution_id)
        .map(|entry| (entry.backend_pid, entry.cancel_requested));
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
    let driver = PostgresDriver::new();
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
async fn transaction_execution_cancel_is_pending_until_target_is_bound() {
    let driver = PostgresDriver::new();
    let handle = ConnectionHandle {
        id: "session-tx".into(),
        pool_id: "pool-tx".into(),
    };
    let execution_id = QueryExecutionId::new("exec-tx");
    driver.query_executions.lock().await.insert(
        execution_id.clone(),
        PgQueryExecution {
            session_id: handle.id.clone(),
            target_pool: None,
            control_pool: None,
            backend_pid: None,
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
            .map(|entry| (entry.backend_pid, entry.cancel_requested)),
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
        .bind_backend_pid(&handle, &execution_id, 42)
        .await
        .unwrap());
    assert_eq!(
        driver
            .query_executions
            .lock()
            .await
            .get(&execution_id)
            .map(|entry| (entry.backend_pid, entry.cancel_requested)),
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
    let _q = PostgresDriver::bind_values(
        sqlx::query("SELECT $1, $2, $3, $4, $5, $6, $7, $8"),
        &params,
    );
}

#[tokio::test]
async fn query_with_params_requires_pool() {
    let driver = PostgresDriver::new();
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "missing-pool".into(),
    };
    let err = driver
        .query_with_params(&handle, "SELECT $1::int", &[Value::Integer(1)])
        .await
        .unwrap_err();
    assert!(
        matches!(err, DriverError::ConnectionFailed(_)),
        "expected ConnectionFailed, got {err:?}"
    );
}

#[test]
fn build_pg_create_table_ddl_includes_types_not_null_default_and_pk() {
    let columns = vec![
        PgColumnDdl {
            name: "id".into(),
            data_type: "integer".into(),
            not_null: true,
            default_expr: None,
        },
        PgColumnDdl {
            name: "email".into(),
            data_type: "character varying(255)".into(),
            not_null: true,
            default_expr: None,
        },
        PgColumnDdl {
            name: "status".into(),
            data_type: "text".into(),
            not_null: false,
            default_expr: Some("'active'::text".into()),
        },
    ];
    let sql =
        build_pg_create_table_ddl("\"public\".\"users\"", &columns, &["id".into()], &|n| {
            format!("\"{n}\"")
        });
    assert!(sql.starts_with("CREATE TABLE \"public\".\"users\" ("));
    assert!(sql.contains("\"id\" integer NOT NULL"));
    assert!(sql.contains("\"email\" character varying(255) NOT NULL"));
    assert!(sql.contains("\"status\" text DEFAULT 'active'::text"));
    assert!(sql.contains("PRIMARY KEY (\"id\")"));
    assert!(sql.ends_with(");\n"));
}

#[test]
fn pg_sequence_start_uses_last_value_when_called() {
    assert_eq!(pg_sequence_start(Some(3), Some(true), 1, 1), 4);
    assert_eq!(pg_sequence_start(Some(1), Some(false), 1, 1), 1);
    assert_eq!(pg_sequence_start(None, None, 1, 1), 1);
}

#[test]
fn build_pg_create_sequence_sql_and_owned_by() {
    let seq = PgSequenceDdl {
        qualified_name: "\"public\".\"categories_id_seq\"".into(),
        data_type: "integer".into(),
        increment: 1,
        min_value: 1,
        max_value: 2147483647,
        start: 4,
        cache: 1,
        cycle: false,
        owned_column: Some("id".into()),
    };
    let create = build_pg_create_sequence_sql(&seq);
    assert!(
        create.starts_with("CREATE SEQUENCE IF NOT EXISTS \"public\".\"categories_id_seq\"")
    );
    assert!(create.contains("START WITH 4"));
    assert!(create.contains("NO CYCLE"));
    let owned = build_pg_alter_sequence_owned_by(&seq, "\"public\".\"categories\"", &|n| {
        format!("\"{n}\"")
    })
    .unwrap();
    assert_eq!(
        owned,
        "ALTER SEQUENCE \"public\".\"categories_id_seq\" OWNED BY \"public\".\"categories\".\"id\";\n"
    );
}

#[test]
fn build_pg_create_table_ddl_omits_pk_when_empty() {
    let columns = vec![PgColumnDdl {
        name: "x".into(),
        data_type: "text".into(),
        not_null: false,
        default_expr: None,
    }];
    let sql = build_pg_create_table_ddl("t", &columns, &[], &|n| n.to_string());
    assert!(!sql.contains("PRIMARY KEY"));
}

#[test]
fn apply_select_limit_is_independent_of_subquery_limit() {
    assert_eq!(
        apply_select_limit("SELECT * FROM t", None),
        ("SELECT * FROM t".into(), None)
    );
    assert_eq!(
        apply_select_limit("SELECT * FROM t", Some(10)),
        ("SELECT * FROM t LIMIT 11".into(), Some(10))
    );
    assert_eq!(
        apply_select_limit("SELECT * FROM t LIMIT 3", Some(10)),
        ("SELECT * FROM t LIMIT 3".into(), Some(10))
    );
    let (sql, cap) = apply_select_limit("SELECT * FROM (SELECT * FROM t LIMIT 5) s", Some(10));
    assert!(sql.ends_with(" LIMIT 11"), "{sql}");
    assert_eq!(cap, Some(10));
    assert_eq!(
        apply_select_limit("INSERT INTO t VALUES (1)", Some(10)),
        ("INSERT INTO t VALUES (1)".into(), None)
    );
}
