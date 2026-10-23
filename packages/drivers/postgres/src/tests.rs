//! Unit tests for the PostgreSQL driver.

use super::PostgresDriver;
use crate::catalog::{
    build_pg_alter_sequence_owned_by, build_pg_create_sequence_sql, build_pg_create_table_ddl,
    pg_sequence_start, qualify_regclass_literals, PgColumnDdl, PgSequenceDdl,
};
use crate::execution::{PgQueryExecution, PG_BACKEND_PID_SQL, PG_CANCEL_BACKEND_SQL};
use crate::sql::{apply_select_limit, parse_pg_table_ref};
use datazen_driver_api::*;

#[test]
fn format_sql_literal_keeps_binary_bytes_lossless() {
    let driver = PostgresDriver::new();
    assert_eq!(
        driver.format_sql_literal(&Some(Value::Bytes(vec![0x00, 0xff, 0xfe]))),
        "'\\x00fffe'"
    );
    assert_eq!(
        driver.format_sql_literal(&Some(Value::String("O'Brien".into()))),
        "'O''Brien'"
    );
}

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
    assert_eq!(PostgresDriver::resolve_connect_database(&cfg), "postgres");

    cfg.database = Some("  ".into());
    assert_eq!(PostgresDriver::resolve_connect_database(&cfg), "postgres");

    cfg.database = Some("  app_db  ".into());
    assert_eq!(PostgresDriver::resolve_connect_database(&cfg), "app_db");
}

#[tokio::test]
async fn pool_for_target_rejects_a_blank_foreign_database() {
    let driver = PostgresDriver::new();
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "missing-pool".into(),
    };

    // A blank name means "the handle's own database", and that pool is missing.
    let err = driver.pool_for_target(&handle, "").await.unwrap_err();
    assert!(
        matches!(err, DriverError::ConnectionFailed(_)),
        "expected ConnectionFailed, got {err:?}"
    );

    // A named foreign database needs a connect template, which is also missing.
    let err = driver.pool_for_target(&handle, "app_db").await.unwrap_err();
    assert!(
        matches!(err, DriverError::ConnectionFailed(_)),
        "expected ConnectionFailed, got {err:?}"
    );
}

#[tokio::test]
async fn pool_for_target_reuses_the_primary_pool_for_the_active_database() {
    let driver = PostgresDriver::new();
    let pool_id = "test-pool".to_string();
    driver
        .active_databases
        .write()
        .await
        .insert(pool_id.clone(), "already".to_string());
    let primary = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://localhost/nonexistent")
        .expect("lazy pool");
    driver.pools.write().await.insert(pool_id.clone(), primary);

    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id,
    };

    // Same database (trimmed) must short-circuit to the handle's own pool rather
    // than opening a second one — this is what keeps `USE`-style switching gone.
    assert!(driver.is_active_database(&handle, "already").await);
    assert!(
        driver
            .is_active_database(&handle, "  already  ".trim())
            .await
    );
    driver
        .pool_for_target(&handle, "already")
        .await
        .expect("active database resolves to the primary pool");
    assert!(driver.database_pools.read().await.is_empty());

    // The handle's own database can never be "closed" as a foreign pool.
    let err = driver
        .close_database_pool(&handle, "already")
        .await
        .unwrap_err();
    assert!(matches!(err, DriverError::InvalidConfig(_)), "got {err:?}");
}

#[tokio::test]
async fn closing_an_unopened_database_pool_is_a_noop() {
    let driver = PostgresDriver::new();
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "missing-pool".into(),
    };
    let closed = driver
        .close_database_pool(&handle, "app_db")
        .await
        .expect("closing a never-opened pool is not an error");
    assert!(!closed);
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
async fn begin_read_snapshot_requires_pool() {
    let driver = PostgresDriver::new();
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
    assert_eq!(
        PG_CANCEL_BACKEND_SQL,
        "SELECT pg_catalog.pg_cancel_backend($1)"
    );
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
            cancel_gate: std::sync::Arc::new(tokio::sync::Mutex::new(())),
            session_id: handle.id.clone(),
            target_pool: None,
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
fn pg_dump_qualifies_the_regclass_default_with_the_table_schema() {
    // The catalog's own text is unqualified: it was written against the dumping
    // session's search_path. A restore session has a different one, where
    // `nextval('t_id_seq'::regclass)` resolves to nothing and the CREATE TABLE is
    // rejected — so the dump has to qualify it.
    assert_eq!(
        qualify_regclass_literals("nextval('t_id_seq'::regclass)", Some("\"public\"")),
        "nextval('\"public\".t_id_seq'::regclass)"
    );
    // Already qualified: left untouched.
    assert_eq!(
        qualify_regclass_literals("nextval('sales.t_id_seq'::regclass)", Some("\"public\"")),
        "nextval('sales.t_id_seq'::regclass)"
    );
    // Catalog names are only quoted when they need it: `public.t` is the
    // ordinary case and must qualify just the same.
    assert_eq!(
        qualify_regclass_literals("nextval('t_id_seq'::regclass)", Some("public")),
        "nextval('public.t_id_seq'::regclass)"
    );
    // No schema known: nothing to add.
    assert_eq!(
        qualify_regclass_literals("nextval('t_id_seq'::regclass)", None),
        "nextval('t_id_seq'::regclass)"
    );
    // Anything that is not a `regclass` literal is untouched.
    assert_eq!(
        qualify_regclass_literals("'now'::text", Some("\"public\"")),
        "'now'::text"
    );
    // A literal followed by more text still gets qualified, and the rest survives.
    assert_eq!(
        qualify_regclass_literals(
            "nextval('a_seq'::regclass) + nextval('b_seq'::regclass)",
            Some("\"app\"")
        ),
        "nextval('\"app\".a_seq'::regclass) + nextval('\"app\".b_seq'::regclass)"
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
    let sql = build_pg_create_table_ddl("\"public\".\"users\"", &columns, &["id".into()], &|n| {
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
    assert!(create.starts_with("CREATE SEQUENCE IF NOT EXISTS \"public\".\"categories_id_seq\""));
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
fn postgres_sequence_catalog_ddl_covers_attributes_and_owned_by() {
    use datazen_driver_api::schema_object_sql::SchemaObjectSql as _;
    let sql = crate::PostgresSchemaObjectSql
        .object_ddl_sql(&datazen_driver_api::schema_object_sql::ObjectDdlRequest {
            kind: ObjectKind::Sequence,
            name: "orders_id_seq",
            schema: Some("public"),
            signature: None,
            target_schema: None,
            target_name: None,
        })
        .expect("PostgreSQL sequences have catalog-backed DDL");
    for fragment in [
        "pg_sequence",
        "s.seqincrement",
        "s.seqmin",
        "s.seqmax",
        "s.seqstart",
        "s.seqcache",
        "s.seqcycle",
        "OWNED BY",
        "pg_depend",
        "ns.nspname = 'public'",
        "c.relname = 'orders_id_seq'",
    ] {
        assert!(
            sql.contains(fragment),
            "catalog SQL missing {fragment}: {sql}"
        );
    }
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

#[test]
fn migration_parameters_have_typed_safe_placeholders() {
    let driver = PostgresDriver::new();
    assert_eq!(
        driver
            .parameter_placeholder(1, Some("numeric(65,30)"))
            .unwrap(),
        "$1::numeric"
    );
    assert_eq!(
        driver.parameter_placeholder(2, Some("uuid")).unwrap(),
        "$2::uuid"
    );
    assert_eq!(
        driver.parameter_placeholder(3, Some("bytea")).unwrap(),
        "$3"
    );
    assert_eq!(
        driver
            .parameter_placeholder(4, Some("text); DROP TABLE x;--"))
            .unwrap(),
        "$4"
    );
}

/// 回归：流式执行必须把面板选中的库解析成对应的连接池。
///
/// 背景：query_stream_with_execution 只拿到会话默认库，于是查询面板里选的
/// database 被静默丢弃，SQL 永远跑在默认库上（报 relation does not exist）。
/// pin_query_execution_pool 是修复的关键一步，这里覆盖它的三条约束。
mod stream_target_pool {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    fn lazy_pool() -> sqlx::PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://127.0.0.1:1/none")
            .expect("connect_lazy never dials")
    }

    async fn registered(
        driver: &PostgresDriver,
        handle: &ConnectionHandle,
        id: &QueryExecutionId,
    ) -> sqlx::PgPool {
        driver.query_executions.lock().await.insert(
            id.clone(),
            PgQueryExecution {
                cancel_gate: std::sync::Arc::new(tokio::sync::Mutex::new(())),
                session_id: handle.id.clone(),
                target_pool: None,
                backend_pid: None,
                cancel_requested: false,
                transactional: false,
            },
        );
        lazy_pool()
    }

    #[tokio::test]
    async fn pin_installs_the_resolved_target_pool() {
        let driver = PostgresDriver::new();
        let handle = ConnectionHandle {
            id: "s".into(),
            pool_id: "p".into(),
        };
        let id = QueryExecutionId::new("exec-pin");
        let pool = registered(&driver, &handle, &id).await;

        driver
            .pin_query_execution_pool(&handle, &id, pool.clone())
            .await
            .unwrap();

        let stored = driver.query_executions.lock().await;
        assert!(
            stored[&id].target_pool.is_some(),
            "target pool must be installed"
        );
    }

    #[tokio::test]
    async fn pin_rejects_a_foreign_session() {
        let driver = PostgresDriver::new();
        let handle = ConnectionHandle {
            id: "s".into(),
            pool_id: "p".into(),
        };
        let id = QueryExecutionId::new("exec-foreign");
        let pool = registered(&driver, &handle, &id).await;

        let other = ConnectionHandle {
            id: "other".into(),
            pool_id: "p".into(),
        };
        let err = driver
            .pin_query_execution_pool(&other, &id, pool)
            .await
            .expect_err("a stream must not repin another session's execution");
        assert!(
            matches!(err, DriverError::QueryExecutionSessionMismatch),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn pin_reports_an_unregistered_execution() {
        let driver = PostgresDriver::new();
        let handle = ConnectionHandle {
            id: "s".into(),
            pool_id: "p".into(),
        };
        let id = QueryExecutionId::new("exec-missing");

        let err = driver
            .pin_query_execution_pool(&handle, &id, lazy_pool())
            .await
            .expect_err("an unregistered execution cannot be pinned");
        assert!(
            matches!(err, DriverError::QueryExecutionNotFound(_)),
            "got {err:?}"
        );
    }
}

/// 回归：不活跃的库要定期归还连接资源。
///
/// 背景：池以前每会话常驻 `min_connections = 2`，且不活跃的库永远不回收——
/// 实测一个窗口空闲时仍占 17 条后端连接（其中同一个库 6 条）。
/// 现在会话自身不再常驻连接，浏览过的库在空闲超过 TTL 后整池回收。
mod inactive_database_reap {
    use super::*;
    use crate::connection::POOL_IDLE_TTL;
    use crate::postgres::DatabasePoolEntry;
    use sqlx::{postgres::PgPoolOptions, PgPool};
    use std::time::{Duration, Instant};

    fn lazy_pool() -> PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://127.0.0.1:1/none")
            .expect("connect_lazy never dials")
    }

    fn entry(idle: Duration) -> DatabasePoolEntry {
        DatabasePoolEntry {
            pool: lazy_pool(),
            last_used: Instant::now() - idle,
        }
    }

    #[tokio::test]
    async fn reap_takes_inactive_databases_and_keeps_recent_ones() {
        let driver = PostgresDriver::new();
        let pool_id = "test-pool".to_string();
        let handle = ConnectionHandle {
            id: "conn".into(),
            pool_id: pool_id.clone(),
        };
        {
            let mut cache = driver.database_pools.write().await;
            cache.insert(
                (pool_id.clone(), "idle_db".into()),
                entry(POOL_IDLE_TTL + Duration::from_secs(1)),
            );
            cache.insert(
                (pool_id.clone(), "fresh_db".into()),
                entry(Duration::from_secs(1)),
            );
            cache.insert(
                ("other-pool".into(), "idle_db".into()),
                entry(POOL_IDLE_TTL + Duration::from_secs(1)),
            );
        }

        driver.reap_inactive_databases(&handle).await;

        let cache = driver.database_pools.read().await;
        assert!(!cache.contains_key(&(pool_id.clone(), "idle_db".into())));
        assert!(cache.contains_key(&(pool_id.clone(), "fresh_db".into())));
        // Another handle's pools are none of this handle's business.
        assert!(cache.contains_key(&("other-pool".into(), "idle_db".into())));
    }
}

/// A retained session checks out its pool's only connection for its lifetime.
/// Catalog reads therefore must run on that pinned connection; borrowing from
/// the max-1 pool waits for a checkout that cannot return before disconnect
/// and fails only after the pool acquire timeout.
///
/// Regression: before the pinned-aware helper, `get_databases`, `get_tables`,
/// `get_schemas`, `get_columns` and `get_all_columns` all borrowed from the
/// pool. With `connectionTimeout: 1` they timed out here.
#[tokio::test]
#[ignore = "requires disposable PostgreSQL: TEST_PG_HOST/PORT/USER"]
async fn fixed_session_catalog_reads_use_the_pinned_connection() {
    let host = std::env::var("TEST_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("TEST_PG_PORT")
        .expect("disposable test port")
        .parse::<u16>()
        .expect("numeric TEST_PG_PORT");
    let user = std::env::var("TEST_PG_USER").expect("disposable test user");
    let password = std::env::var("TEST_PG_PASSWORD").ok();
    let database = std::env::var("TEST_PG_DATABASE").unwrap_or_else(|_| "postgres".to_string());
    let mut config: ConnectionConfig = serde_json::from_value(serde_json::json!({
        "id": "fixed-catalog-live",
        "name": "fixed catalog live",
        "databaseType": "postgresql",
        "host": host,
        "port": port,
        "username": user,
        "database": database,
        "sslMode": "disable",
        "connectionTimeout": 1,
        "maxPoolSize": 1,
    }))
    .expect("live test config");
    config.password = password;

    let driver = PostgresDriver::new();
    let handle = driver
        .connect_fixed_session(&config)
        .await
        .expect("fixed session opens");
    let database = config.database.clone().unwrap_or_else(|| "postgres".into());

    let baseline = pool_counters(&driver, &handle)
        .await
        .expect("fixed data pool exists");
    assert_eq!(
        baseline,
        (1, 0),
        "the session pins the pool's only connection"
    );

    // This is the removed failure mode in one statement: the retained session
    // already holds the pool's only connection, so the **first** extra borrow
    // waits for the acquire timeout. There is no 10-statement threshold here.
    let pool = driver
        .pools
        .read()
        .await
        .get(&handle.pool_id)
        .cloned()
        .expect("fixed pool exists");
    let extra_borrow =
        tokio::time::timeout(std::time::Duration::from_secs(2), pool.acquire()).await;
    let refused = match extra_borrow {
        // Our outer guard fired, or sqlx's own acquire timeout did.
        Err(_) | Ok(Err(_)) => true,
        Ok(Ok(_)) => false,
    };
    assert!(
        refused,
        "a max-1 pool with its only connection pinned cannot lend another checkout"
    );

    for index in 1..=20u32 {
        let call = async {
            match index % 7 {
                0 => driver.get_databases(&handle).await.map(|_| ()),
                1 => driver
                    .get_tables(&handle, &database, Some("public"))
                    .await
                    .map(|_| ()),
                2 => driver.get_schemas(&handle, &database).await.map(|_| ()),
                3 => driver
                    .get_columns(&handle, "pg_class", &database, Some("pg_catalog"))
                    .await
                    .map(|_| ()),
                4 => driver
                    .get_all_columns(&handle, &database, Some("public"))
                    .await
                    .map(|_| ()),
                5 => driver.query(&handle, "SELECT 1").await.map(|_| ()),
                _ => driver
                    .query_multi(&handle, "SELECT 1; SELECT 2", None)
                    .await
                    .map(|_| ()),
            }
        };
        let call = tokio::time::timeout(std::time::Duration::from_secs(3), call)
            .await
            .unwrap_or_else(|_| panic!("fixed-session statement {index} timed out"));
        call.expect("catalog read uses the pinned connection");
        assert_eq!(
            pool_counters(&driver, &handle).await,
            Some(baseline),
            "statement {index} changed the fixed pool's checkout state"
        );
    }

    let databases = driver.get_databases(&handle).await.expect("databases list");
    assert!(!databases.is_empty(), "pg_database is never empty");

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let control = control_counters(&driver, &config)
        .await
        .expect("fixed session has a control pool");
    assert_eq!(
        control.1, control.0,
        "control checkouts returned after 20 statements: {control:?}"
    );

    driver
        .disconnect(handle)
        .await
        .expect("fixed session disconnects");
}

/// Pool counters are the leak判据: a statement that returns its checkout leaves
/// `num_idle() == size()`; a leaked checkout leaves `num_idle() < size()`.
#[cfg(test)]
async fn pool_counters(
    driver: &PostgresDriver,
    handle: &ConnectionHandle,
) -> Option<(usize, usize)> {
    let pools = driver.pools.read().await;
    pools
        .get(&handle.pool_id)
        .map(|pool| (pool.size() as usize, pool.num_idle()))
}

#[cfg(test)]
async fn control_counters(
    driver: &PostgresDriver,
    config: &ConnectionConfig,
) -> Option<(usize, usize)> {
    let pools = driver.control_pools.read().await;
    pools
        .get(&PostgresDriver::control_pool_key(config))
        .and_then(|entry| entry.pool_for_test())
        .map(|pool| (pool.size() as usize, pool.num_idle()))
}

/// Run 20 catalog statements through a **leased** (non-retained) session and
/// prove every checkout returns. Sizes 1 and 10 cover the metadata pool bound
/// and the interactive default: if each statement leaked one connection, the
/// size-10 case would stall on statement 11 while the size-1 case would stall
/// on statement 2.
#[tokio::test]
#[ignore = "requires disposable PostgreSQL: TEST_PG_HOST/PORT/USER"]
async fn leased_catalog_reads_return_every_pool_checkout() {
    let host = std::env::var("TEST_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("TEST_PG_PORT")
        .expect("disposable test port")
        .parse::<u16>()
        .expect("numeric TEST_PG_PORT");
    let user = std::env::var("TEST_PG_USER").expect("disposable test user");
    let password = std::env::var("TEST_PG_PASSWORD").ok();
    let database = std::env::var("TEST_PG_DATABASE").unwrap_or_else(|_| "postgres".to_string());

    for max_pool_size in [1, 10] {
        let mut config: ConnectionConfig = serde_json::from_value(serde_json::json!({
            "id": format!("leased-catalog-{max_pool_size}"),
            "name": "leased catalog live",
            "databaseType": "postgresql",
            "host": host,
            "port": port,
            "username": user,
            "database": database,
            "sslMode": "disable",
            "connectionTimeout": 1,
            "maxPoolSize": max_pool_size,
        }))
        .expect("live test config");
        config.password = password.clone();

        let driver = PostgresDriver::new();
        let handle = driver
            .connect_with_control(&config, &config, None)
            .await
            .expect("leased session opens");
        let database = config.database.clone().unwrap_or_else(|| "postgres".into());

        // Warm the pool once, then the baseline is the size nothing may exceed.
        driver
            .get_tables(&handle, &database, Some("public"))
            .await
            .expect("warm-up listing");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let baseline = pool_counters(&driver, &handle)
            .await
            .expect("data pool exists");
        assert_eq!(baseline.1, baseline.0, "warm-up returns its checkout");

        let mut first_failure = None;
        for index in 1..=20u32 {
            let call = async {
                match index % 5 {
                    0 => driver.get_databases(&handle).await.map(|_| ()),
                    1 => driver
                        .get_tables(&handle, &database, Some("public"))
                        .await
                        .map(|_| ()),
                    2 => driver.get_schemas(&handle, &database).await.map(|_| ()),
                    3 => driver
                        .get_columns(&handle, "pg_class", &database, Some("pg_catalog"))
                        .await
                        .map(|_| ()),
                    _ => driver
                        .get_all_columns(&handle, &database, Some("public"))
                        .await
                        .map(|_| ()),
                }
            };
            let call = tokio::time::timeout(std::time::Duration::from_secs(3), call)
                .await
                .unwrap_or_else(|_| panic!("statement {index} timed out"));
            if let Err(error) = call {
                first_failure = Some((index, error.to_string()));
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let counters = pool_counters(&driver, &handle)
                .await
                .expect("data pool exists");
            assert_eq!(
                counters.1, counters.0,
                "statement {index} kept a pool checkout (size={}, idle={})",
                counters.0, counters.1
            );
            assert!(
                counters.0 <= max_pool_size,
                "pool grew past configured max at statement {index}: {counters:?}"
            );
        }
        assert_eq!(
            first_failure, None,
            "max_pool_size={max_pool_size}: first failing statement {first_failure:?}"
        );

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let control = control_counters(&driver, &config)
            .await
            .expect("control pool exists");
        assert_eq!(control.1, control.0, "control checkout returned");

        driver
            .disconnect(handle)
            .await
            .expect("leased session disconnects");
    }
}
