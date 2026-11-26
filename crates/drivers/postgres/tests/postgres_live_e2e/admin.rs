use super::Fixture;
use datazen_driver_api::{ConnectionHandle, DatabaseDriver};
use datazen_driver_postgres::PostgresDriver;
use serde_json::{json, Value};
use sqlx::{postgres::PgConnectOptions, Connection, PgConnection};
use std::time::Duration;

async fn command(
    driver: &PostgresDriver,
    handle: &ConnectionHandle,
    id: &str,
    input: Value,
) -> Value {
    driver
        .execute_command(handle, id, input)
        .await
        .unwrap_or_else(|_| panic!("live driver command {id} failed"))
        .data
}

fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[tokio::test]
async fn admin_lifecycle_round_trips_roles_grants_schemas_and_an_isolated_database() {
    let mut fixture = Fixture::open().await;
    let driver = PostgresDriver::new();
    let handle = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("fixed driver session");
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let role = format!("dz_e2e_role\"{tag}");
    let schema = format!("dz_e2e_schema\"{tag}");
    let child_database = format!("dz_fixture_{tag}");
    let database = fixture
        .config
        .database
        .as_deref()
        .expect("fixture database");
    let owner = fixture.config.username.as_deref().expect("fixture owner");

    assert_eq!(
        command(&driver, &handle, "create_user", json!({ "username": role })).await["ok"],
        true
    );
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT rolcanlogin FROM pg_catalog.pg_roles WHERE rolname = $1"
    )
    .bind(&role)
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("role oracle"));

    command(
        &driver,
        &handle,
        "create_schema",
        json!({ "name": schema, "owner": role }),
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT pg_get_userbyid(nspowner)::text FROM pg_catalog.pg_namespace WHERE nspname = $1"
        )
        .bind(&schema)
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("schema owner oracle"),
        role
    );
    sqlx::query(&format!(
        "CREATE TABLE {}.tracked (id INTEGER PRIMARY KEY)",
        quoted(&schema)
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("disposable schema fixture");
    assert!(
        driver
            .execute_command(&handle, "drop_schema", json!({"name": schema}))
            .await
            .is_err(),
        "nonempty schema must require CASCADE"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {}.tracked", quoted(&schema)))
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("failed non-cascade drop must preserve data"),
        0
    );

    // The command supports combined database privileges + public tables. Use
    // a fresh unique child DB so no shared fixture objects are affected.
    command(
        &driver,
        &handle,
        "create_database",
        json!({ "name": child_database, "owner": owner, "encoding": "UTF8" }),
    )
    .await;
    assert_eq!(
        sqlx::query_as::<_, (String, String)>(
            "SELECT pg_get_userbyid(datdba)::text, pg_encoding_to_char(encoding)::text FROM pg_catalog.pg_database WHERE datname = $1"
        )
        .bind(&child_database)
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("database catalog oracle"),
        (owner.to_owned(), "UTF8".into())
    );
    let mut oracle_child = fixture.connect_to(&child_database).await;
    sqlx::raw_sql(
        "CREATE TABLE public.fixture_table (k INT); INSERT INTO public.fixture_table VALUES(17);",
    )
    .execute(&mut oracle_child)
    .await
    .expect("child database rows");
    command(
        &driver,
        &handle,
        "grant_privileges",
        json!({
            "username": role, "database": child_database,
            "privileges": ["CREATE", "SELECT"], "grantOption": true
        }),
    )
    .await;
    let grants = async |expected| {
        let mut child = fixture.connect_to(&child_database).await;
        let observed = sqlx::query_as::<_, (bool, bool)>(
            "SELECT has_database_privilege($1, current_database(), 'CREATE WITH GRANT OPTION'), has_table_privilege($1, 'public.fixture_table', 'SELECT WITH GRANT OPTION')"
        )
        .bind(&role)
        .fetch_one(&mut child)
        .await
        .expect("database/table privilege oracle");
        assert_eq!(observed, expected);
    };
    grants((true, true)).await;
    command(
        &driver,
        &handle,
        "revoke_privileges",
        json!({
            "username": role, "database": child_database,
            "privileges": ["CREATE", "SELECT"]
        }),
    )
    .await;
    grants((false, false)).await;
    let child_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut oracle_child)
        .await
        .expect("child connection PID");
    assert_eq!(
        command(
            &driver,
            &handle,
            "drop_database",
            json!({ "name": child_database })
        )
        .await["ok"],
        true
    );
    assert!(sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&mut oracle_child)
        .await
        .is_err());
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_catalog.pg_database WHERE datname = $1) OR EXISTS(SELECT 1 FROM pg_catalog.pg_stat_activity WHERE pid = $2)"
    )
    .bind(&child_database)
    .bind(child_pid)
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("disposable child database and backend should be retired"));
    assert_eq!(
        driver
            .query(&handle, "SELECT current_database()")
            .await
            .expect("original session alive")
            .rows,
        vec![vec![super::string(database)]],
    );

    command(
        &driver,
        &handle,
        "drop_schema",
        json!({ "name": schema, "cascade": true }),
    )
    .await;
    command(&driver, &handle, "drop_user", json!({ "username": role })).await;
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_catalog.pg_roles WHERE rolname = $1) OR EXISTS(SELECT 1 FROM pg_catalog.pg_namespace WHERE nspname = $2)"
    )
    .bind(&role)
    .bind(&schema)
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("drop schema and role oracle"));
    driver.disconnect(handle).await.expect("driver cleanup");
}

#[tokio::test]
async fn admin_status_and_process_actions_match_the_live_server() {
    let mut fixture = Fixture::open().await;
    let database = fixture
        .config
        .database
        .as_deref()
        .expect("fixture database");
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("driver connection");
    let mut worker = fixture.connect_to(database).await;
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut worker)
        .await
        .expect("disposable worker PID");
    let sleeper = tokio::spawn(async move {
        let sleep = sqlx::query("SELECT pg_sleep(30) /* driver E2E cancellable */")
            .execute(&mut worker)
            .await;
        (worker, sleep)
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let pending = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM pg_catalog.pg_stat_activity WHERE pid = $1 AND wait_event = 'PgSleep')",
            )
            .bind(pid)
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("worker wait-event oracle");
            if pending {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("worker really starts its query before cancellation");

    let processes = command(&driver, &handle, "list_processes", json!({})).await;
    let worker_entry = processes["processes"]
        .as_array()
        .expect("process list")
        .iter()
        .find(|entry| entry["pid"] == pid)
        .expect("driver lists independently identified worker");
    assert_eq!(worker_entry["database"], database);
    assert_eq!(worker_entry["state"], "active");
    assert!(worker_entry["query"]
        .as_str()
        .is_some_and(|query| query.contains("driver E2E cancellable")));
    command(&driver, &handle, "kill_process", json!({"pid": pid})).await;
    let (mut worker, error) = tokio::time::timeout(Duration::from_secs(5), sleeper)
        .await
        .expect("cancel completes promptly")
        .expect("worker task");
    let error = error.expect_err("Postgres cancelled only the worker statement");
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("57014")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
            .fetch_one(&mut worker)
            .await
            .expect("cancel keeps worker session usable"),
        pid
    );
    command(
        &driver,
        &handle,
        "kill_process",
        json!({"pid": pid, "force": true}),
    )
    .await;
    assert!(
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&mut worker)
            .await
            .is_err(),
        "forced termination closes the disposable worker connection"
    );

    let status = command(&driver, &handle, "server_status_snapshot", json!({})).await;
    let (version, max): (String, i64) =
        sqlx::query_as("SELECT version(), current_setting('max_connections')::bigint")
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("live status oracle");
    assert_eq!(status["version"], version);
    assert_eq!(status["database"], database);
    assert_eq!(status["maxConnections"], max);
    for gauge in [
        "uptimeSeconds",
        "activeQueries",
        "sessionTotal",
        "sessionIdle",
        "xactCommitCounter",
        "walBytes",
    ] {
        assert!(
            status[gauge].as_i64().is_some_and(|value| value >= 0),
            "invalid live status gauge {gauge}"
        );
    }
    assert!(!status["statusVariables"]
        .as_array()
        .expect("server settings")
        .is_empty());
    assert_eq!(
        status["statementCounters"]["select"],
        status["tupFetchedCounter"]
    );

    let table = format!("dz_e2e_estimate_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE TABLE public.{table} AS SELECT n FROM generate_series(1, 17) n; ANALYZE public.{table}"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("row estimate fixture");
    let estimate = command(
        &driver,
        &handle,
        "estimate_table_rows",
        json!({"table": table}),
    )
    .await;
    assert_eq!(estimate["estimatedRows"], 17);
    let missing = command(
        &driver,
        &handle,
        "estimate_table_rows",
        json!({"table": table, "schema": "dz_absent_schema"}),
    )
    .await;
    assert_eq!(missing["estimatedRows"], 0);
    sqlx::query(&format!("DROP TABLE public.{table}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove the row estimate fixture");
    driver.disconnect(handle).await.expect("driver cleanup");
}

// Open an independent connection without logging fixture credentials.
impl Fixture {
    pub(super) async fn connect_to(&self, database: &str) -> PgConnection {
        let opts = PgConnectOptions::new()
            .host("127.0.0.1")
            .port(self.config.port.expect("checked fixture port"))
            .database(database)
            .username(
                self.config
                    .username
                    .as_deref()
                    .expect("checked fixture user"),
            )
            .password(self.config.password.as_deref().unwrap_or_default())
            .ssl_mode(sqlx::postgres::PgSslMode::Disable);
        tokio::time::timeout(Duration::from_secs(8), PgConnection::connect_with(&opts))
            .await
            .expect("independent fixture connection timed out")
            .unwrap_or_else(|_| panic!("independent fixture connection failed"))
    }
}

#[tokio::test]
async fn live_postgres_login_and_table_grants_change_only_the_selected_role() {
    let mut fixture = Fixture::open().await;
    let database = fixture
        .config
        .database
        .as_deref()
        .expect("fixture database");
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("administrator connection");
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let role = format!("dz_e2e_app_{tag}");
    let relation = format!("public.dz_role_rows_{tag}");
    let secret = format!("fixture's-{tag}");
    sqlx::raw_sql(&format!(
        "CREATE TABLE {relation} (k int); INSERT INTO {relation} VALUES (41)"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("scope the genuine privilege fixture");
    command(
        &driver,
        &handle,
        "create_user",
        json!({"username":role,"password":secret}),
    )
    .await;
    let options = PgConnectOptions::new()
        .host("127.0.0.1")
        .port(fixture.config.port.expect("fixture port"))
        .database(database)
        .username(&role)
        .password(&secret)
        .ssl_mode(sqlx::postgres::PgSslMode::Disable);
    let mut new_role =
        tokio::time::timeout(Duration::from_secs(5), PgConnection::connect_with(&options))
            .await
            .expect("live role login promptly completes")
            .unwrap_or_else(|_| panic!("driver-created role can't log into marked fixture"));

    let current: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&mut new_role)
        .await
        .expect("server verifies newly installed login");
    assert_eq!(current, role);
    let can_select = async |conn: &mut PgConnection| {
        sqlx::query_scalar::<_, i32>(&format!("SELECT k FROM {relation}"))
            .fetch_one(conn)
            .await
    };
    let denied = can_select(&mut new_role)
        .await
        .expect_err("login alone does not grant another user's table");
    assert_eq!(
        denied.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("42501")
    );
    let args = json!({"username":role,"privileges":["SELECT"]});
    command(&driver, &handle, "grant_privileges", args.clone()).await;
    assert_eq!(
        can_select(&mut new_role)
            .await
            .expect("explicit grant applies on actual PG"),
        41
    );

    // Invalid UI requests fail with the database still reachable and the
    // previously granted access unchanged. No invalid SQL is run by the test.
    for (id, blank) in [
        ("grant_privileges", json!({"username":role,"privileges":[]})),
        (
            "revoke_privileges",
            json!({"username":role,"privileges":[]}),
        ),
        ("drop_schema", json!({})),
        ("create_schema", json!({})),
        ("create_database", json!({})),
        ("create_user", json!({})),
    ] {
        assert!(
            matches!(
                driver.execute_command(&handle, id, blank).await,
                Err(datazen_driver_api::DriverError::InvalidConfig(_))
            ),
            "incomplete {id} is typed and refused"
        );
    }
    assert_eq!(
        can_select(&mut new_role)
            .await
            .expect("refused request did not revoke legitimate access"),
        41
    );
    command(&driver, &handle, "revoke_privileges", args).await;
    let denied = can_select(&mut new_role)
        .await
        .expect_err("actual revocation after access was granted");
    assert_eq!(
        denied.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("42501")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>(&format!("SELECT k FROM {relation}"))
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("independent control row"),
        41
    );
    new_role
        .close()
        .await
        .expect("retire newly-created login connection");
    command(&driver, &handle, "drop_user", json!({"username":role})).await;
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=$1)"
    )
    .bind(&role)
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("actual role cleanup"));
    sqlx::query(&format!("DROP TABLE {relation}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("dispose own role fixture table");
    driver
        .disconnect(handle)
        .await
        .expect("retire administrator connection");
}

#[tokio::test]
async fn admin_queries_report_real_postgres_errors_and_remain_usable_after_rollback() {
    use datazen_driver_api::DriverError;

    let mut fixture = Fixture::open().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let table = format!("admin_ledger_{tag}");
    let role = format!("admin_viewer_{tag}");
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role};
         CREATE TABLE public.{table} (id int PRIMARY KEY);
         INSERT INTO public.{table} VALUES(1);
         ANALYZE public.{table};"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("PostgreSQL catalog and analysis fixture");

    let driver = PostgresDriver::new();
    let handle = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("retained admin connection");
    let target_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("independent protected PostgreSQL backend");
    let restricted = driver
        .begin_transaction(&handle)
        .await
        .expect("admin role context");
    driver
        .execute(&handle, &format!("SET LOCAL ROLE {role}"))
        .await
        .expect("actual limited PostgreSQL role");
    assert!(matches!(
        driver.execute_command(&handle, "kill_process", json!({"pid":target_pid,"force":true})).await,
        Err(DriverError::QueryFailed(message)) if message.contains("permission") || message.contains("superuser")
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("the protected backend was not terminated"),
        target_pid
    );
    let lookup = json!({"schema":"public","table":table});
    for (name, input) in [
        ("server_status_snapshot", json!({})),
        ("list_processes", json!({})),
        ("estimate_table_rows", lookup.clone()),
        (
            "grant_privileges",
            json!({"username":role,"privileges":["CREATE"]}),
        ),
    ] {
        assert!(
            matches!(
                driver.execute_command(&handle, name, input).await,
                Err(DriverError::QueryFailed(message)) if message.contains("current transaction is aborted")
            ),
            "{name} cannot report a success or empty snapshot while PostgreSQL rejected the statement"
        );
    }
    driver
        .rollback(restricted)
        .await
        .expect("recover ordinary administration after aborted transaction");
    let expected: i64 = sqlx::query_scalar(
        "SELECT c.reltuples::bigint FROM pg_class c JOIN pg_namespace n ON c.relnamespace=n.oid WHERE n.nspname='public' AND c.relname=$1"
    ).bind(&table).fetch_one(&mut fixture.oracle).await.expect("independent analyzed row estimate");
    let estimate = command(&driver, &handle, "estimate_table_rows", lookup).await;
    assert_eq!(estimate["estimatedRows"], expected);
    let status = command(&driver, &handle, "server_status_snapshot", json!({})).await;
    assert_eq!(
        status["database"],
        fixture.config.database.as_deref().expect("fixture DB")
    );
    assert!(matches!(
        driver.execute_command(&handle, "drop_database", json!({"name":format!("dz_fixture_absent_{tag}")})).await,
        Err(DriverError::QueryFailed(message)) if message.contains("does not exist")
    ));
    driver
        .disconnect(handle)
        .await
        .expect("close admin session");
}
