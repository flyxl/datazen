use super::Fixture;
use datazen_driver_api::{
    DatabaseDriver, DriverError, QueryExecutionId, QueryStreamCallback, QueryStreamEvent,
    SqlTarget, Value,
};
use datazen_driver_postgres::PostgresDriver;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn record() -> (Arc<Mutex<Vec<QueryStreamEvent>>>, QueryStreamCallback) {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let shared = Arc::clone(&buffer);
    (
        buffer,
        Arc::new(move |event| shared.lock().expect("events").push(event)),
    )
}

#[tokio::test]
async fn explicit_target_parameterized_transactions_and_row_streams_use_the_real_catalog() {
    let mut fixture = Fixture::open().await;
    let database = fixture
        .config
        .database
        .as_deref()
        .expect("fixture database");
    let schema = format!("dz_execution_{}", uuid::Uuid::new_v4().simple());
    let relation = format!("{schema}.records");
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema}; CREATE TABLE {relation}(id bigint PRIMARY KEY, value text);
         INSERT INTO {relation} SELECT n, 'row-'||n FROM generate_series(1,505) n"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("independent fixture");
    let target = SqlTarget::new(Some(database), Some(&schema));
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("driver connection");

    let parameter = [Value::Integer(3)];
    let query = "SELECT value FROM records WHERE id = $1::bigint";
    assert_eq!(
        driver
            .query_with_params_at(&handle, query, &parameter, target)
            .await
            .expect("target SQL")
            .rows,
        vec![vec![super::string("row-3")]]
    );
    let empty = driver
        .query_with_params_at(&handle, query, &[Value::Integer(0)], target)
        .await
        .expect("typed empty projection");
    assert!(empty.rows.is_empty());
    assert_eq!(empty.columns[0].name, "value");

    let tx = driver
        .begin_transaction(&handle)
        .await
        .expect("write transaction");
    assert!(
        driver.begin_transaction(&handle).await.is_err(),
        "nested transactions are refused"
    );
    assert_eq!(
        driver
            .execute_at(
                &handle,
                "UPDATE records SET value='uncommitted' WHERE id=3",
                target
            )
            .await
            .expect("scoped write"),
        1
    );
    assert_eq!(
        driver
            .query_with_params_at(&handle, query, &parameter, target)
            .await
            .expect("read pending contents")
            .rows,
        vec![vec![super::string("uncommitted")]]
    );
    let outside: String = sqlx::query_scalar(&format!("SELECT value FROM {relation} WHERE id=3"))
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("independent isolation oracle");
    assert_eq!(outside, "row-3");
    let multi = driver.query_multi_at(&handle, "SELECT id FROM records ORDER BY id; SELECT value FROM records WHERE id=0; UPDATE records SET value='row-3' WHERE id=3;", Some(2), target)
        .await.expect("typed retained multi-script");
    assert_eq!(multi.results.len(), 3);
    assert!(multi.results[0].truncated);
    assert_eq!(
        multi.results[0].rows,
        vec![vec![Some(Value::Integer(1))], vec![Some(Value::Integer(2))]]
    );
    assert!(multi.results[1].rows.is_empty());
    assert_eq!(multi.results[1].columns[0].name, "value");
    assert_eq!(multi.results[2].rows_affected, Some(1));
    let pinned_empty = driver
        .query_with_params_at(&handle, query, &[Value::Integer(0)], target)
        .await
        .expect("empty pinned typed projection");
    assert_eq!(pinned_empty.columns[0].name, "value");
    let id = QueryExecutionId::new("pinned-stream");
    driver
        .prepare_query_execution(&handle, &id)
        .await
        .expect("prepare while pinned");
    let (events, callback) = record();
    driver.query_stream_with_execution_at(&handle,&id,
         "SELECT value FROM records WHERE id = 3; UPDATE records SET value='registered' WHERE id = 3; SELECT id FROM records WHERE id = 0",
         Some(2),target,callback)
        .await.expect("registered stream while pinned");
    {
        let events = events.lock().expect("events");
        assert!(events.iter().any(|e|matches!(e,QueryStreamEvent::Rows{index:0,rows} if rows == &vec![vec![super::string("row-3")]])));
        assert!(events.iter().any(|e| matches!(
            e,
            QueryStreamEvent::StatementEnd {
                index: 1,
                rows_affected: Some(1),
                ..
            }
        )));
        assert!(events.iter().any(|e|matches!(e,QueryStreamEvent::StatementStart{index:2,columns,..} if columns.first().is_some_and(|c|c.name=="id"))));
    }
    driver.rollback(tx).await.expect("rollback pinned changes");
    let outside: String = sqlx::query_scalar(&format!("SELECT value FROM {relation} WHERE id=3"))
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("independent rollback oracle");
    assert_eq!(outside, "row-3");

    let unpinned = driver
        .query_multi_at(
            &handle,
            "SELECT value FROM records WHERE FALSE",
            None,
            target,
        )
        .await
        .expect("empty unpinned projection");
    assert_eq!(unpinned.results[0].columns[0].name, "value");
    let (events, cb) = record();
    driver
        .query_stream_at(
            &handle,
            "SELECT id FROM records ORDER BY id",
            Some(503),
            target,
            cb,
        )
        .await
        .expect("chunked nontransactional target stream");
    {
        let events = events.lock().expect("events");
        assert_eq!(
            events
                .iter()
                .filter_map(|e| match e {
                    QueryStreamEvent::Rows { rows, .. } => Some(rows.len()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [500, 3]
        );
        assert!(events.iter().any(|e| matches!(
            e,
            QueryStreamEvent::StatementEnd {
                index: 0,
                truncated: true,
                ..
            }
        )));
    }
    let plan = driver
        .explain(&handle, &format!("SELECT value FROM {relation} WHERE id=3"))
        .await
        .expect("actual PostgreSQL EXPLAIN JSON");
    assert!(
        plan.plan_json.is_some() && plan.plan_tree.is_some() && plan.estimated_rows == Some(1),
        "{plan:?}"
    );
    driver.disconnect(handle).await.expect("driver cleanup");
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove execution fixtures");
}

#[tokio::test]
async fn registered_execution_cancels_only_its_real_postgres_statement() {
    let mut fixture = Fixture::open().await;
    let driver = Arc::new(PostgresDriver::new());
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("worker connection");
    let mut other_config = fixture.config.clone();
    other_config.id = uuid::Uuid::new_v4().to_string();
    let control = driver
        .connect(&other_config)
        .await
        .expect("another app connection");
    let pre_cancel = QueryExecutionId::new("pre-cancel");
    driver
        .prepare_query_execution(&handle, &pre_cancel)
        .await
        .expect("pending registration");
    assert!(driver
        .prepare_query_execution(&handle, &pre_cancel)
        .await
        .is_err());
    assert!(matches!(
        driver
            .cancel_query_with_execution(&control, &pre_cancel)
            .await,
        Err(DriverError::QueryExecutionSessionMismatch)
    ));
    driver
        .cancel_query_with_execution(&handle, &pre_cancel)
        .await
        .expect("accept same-session prestart cancellation");
    let (events, callback) = record();
    assert!(matches!(
        driver
            .query_stream_with_execution(&handle, &pre_cancel, "SELECT 1", None, callback)
            .await,
        Err(DriverError::QueryCancelled)
    ));
    assert!(
        events.lock().expect("event lock").is_empty(),
        "pre-cancelled query never started"
    );
    assert!(matches!(
        driver
            .cancel_query_with_execution(&handle, &pre_cancel)
            .await,
        Err(DriverError::QueryExecutionNotFound(_))
    ));

    let id = QueryExecutionId::new("active-cancel");
    driver
        .prepare_query_execution(&handle, &id)
        .await
        .expect("active execution registration");
    let marker = format!("live_query_{}", uuid::Uuid::new_v4().simple());
    let (_, callback) = record();
    let worker = {
        let driver = Arc::clone(&driver);
        let handle = handle.clone();
        let id = id.clone();
        let sql = format!("SELECT pg_sleep(30) /*{marker}*/");
        tokio::spawn(async move {
            driver
                .query_stream_with_execution(&handle, &id, &sql, None, callback)
                .await
        })
    };
    tokio::time::timeout(Duration::from_secs(5),async {
        loop {
            let ready = sqlx::query_scalar::<_,bool>(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE state='active' AND wait_event='PgSleep' AND strpos(query,$1)>0)"
            ).bind(&marker).fetch_one(&mut fixture.oracle).await.expect("independent live wait check");
            if ready { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("registered backend really running");
    assert!(matches!(
        driver.cancel_query_with_execution(&control, &id).await,
        Err(DriverError::QueryExecutionSessionMismatch)
    ));
    driver
        .cancel_query_with_execution(&handle, &id)
        .await
        .expect("whole runtime targets its own postgres backend");
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .expect("prompt cancel")
            .expect("worker task"),
        Err(DriverError::QueryCancelled)
    ));
    assert!(matches!(
        driver.cancel_query_with_execution(&handle, &id).await,
        Err(DriverError::QueryExecutionNotFound(_))
    ));
    for session in [&handle, &control] {
        let query = driver
            .query(session, "SELECT 41 + 1")
            .await
            .expect("session remains usable");
        assert_eq!(query.rows, vec![vec![Some(Value::Integer(42))]]);
    }
    let blank = QueryExecutionId::new("empty-script");
    let (events, callback) = record();
    driver
        .prepare_query_execution(&handle, &blank)
        .await
        .expect("prepare empty script");
    driver
        .query_stream_with_execution(&handle, &blank, "   ", None, callback)
        .await
        .expect("empty script completion");
    assert!(matches!(
        events.lock().expect("events").as_slice(),
        [QueryStreamEvent::Done { .. }]
    ));
    driver.disconnect(control).await.expect("control cleanup");
    driver.disconnect(handle).await.expect("worker cleanup");
}

#[tokio::test]
async fn explicit_target_queries_and_writes_do_not_switch_or_reuse_a_retained_foreign_database() {
    let mut fixture = Fixture::open().await;
    let database = fixture
        .config
        .database
        .as_deref()
        .expect("fixture database");
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let other = format!("dz_fixture_route_{tag}");
    let table = format!("route_records_{tag}");
    sqlx::query(&format!("CREATE DATABASE {other}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("disposable explicit target");
    let mut other_oracle = fixture.connect_to(&other).await;
    for (oracle, label) in [
        (&mut fixture.oracle, "source"),
        (&mut other_oracle, "foreign"),
    ] {
        sqlx::raw_sql(&format!(
            "CREATE TABLE public.{table}(id integer PRIMARY KEY,label text); INSERT INTO public.{table} VALUES (1,'{label}')"
        )).execute(oracle).await.expect("same relation name but different databases");
    }
    let driver = Arc::new(PostgresDriver::new());
    let handle = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("retained source session");
    let initial = driver
        .observe_fixed_session(&handle)
        .await
        .expect("exact retained initial state");
    let foreign = SqlTarget::new(Some(&other), Some("public"));
    let read_sql = format!("SELECT current_database()::text,label FROM {table} WHERE id=1");
    let initial_cross = driver
        .query_at(&handle, &read_sql, foreign)
        .await
        .expect("explicit cross target query");
    let count = driver
        .execute_at(
            &handle,
            &format!("UPDATE {table} SET label='foreign edited' WHERE id=1"),
            foreign,
        )
        .await
        .expect("explicit foreign write matches one row");
    let current_label = async |oracle: &mut sqlx::PgConnection| {
        sqlx::query_scalar::<_, String>(&format!("SELECT label FROM public.{table}"))
            .fetch_one(oracle)
            .await
            .expect("independent persisted database value")
    };
    let before_cross =
        initial_cross.rows == vec![vec![super::string(&other), super::string("foreign")]];
    let outside_source = current_label(&mut fixture.oracle).await;
    let edited_foreign = current_label(&mut other_oracle).await;
    // Gather red evidence for both paths before failing; the fixture data is
    // isolated regardless of which database the driver accidentally used.
    assert!(before_cross && outside_source=="source" && edited_foreign=="foreign edited",
        "cross-target must read and write its explicit database: read={:?}, source={outside_source:?}, target={edited_foreign:?}",
        initial_cross.rows);
    assert_eq!(count, 1);
    let expected = vec![vec![super::string(&other), super::string("foreign edited")]];
    let bound = driver
        .query_with_params_at(
            &handle,
            &format!("SELECT current_database()::text,label FROM {table} WHERE id=$1::int"),
            &[Value::Integer(1)],
            foreign,
        )
        .await
        .expect("foreign prepared query");
    assert_eq!(bound.rows, expected);
    let script = driver
        .query_multi_at(
            &handle,
            &format!("{read_sql}; SELECT current_database()::text"),
            None,
            foreign,
        )
        .await
        .expect("foreign materialized script");
    assert_eq!(script.results[0].rows, expected);
    assert_eq!(script.results[1].rows, vec![vec![super::string(&other)]]);
    let (events, callback) = record();
    driver
        .query_stream_at(&handle, &read_sql, None, foreign, callback)
        .await
        .expect("foreign wire stream");
    assert!(events
        .lock()
        .expect("actual output events")
        .iter()
        .any(|event| matches!(event,
            QueryStreamEvent::Rows{rows,..} if rows == &expected)));
    let exec = QueryExecutionId::new("retained-cross-target");
    driver
        .prepare_query_execution(&handle, &exec)
        .await
        .expect("register exact route");
    let (events, callback) = record();
    driver
        .query_stream_with_execution_at(&handle, &exec, &read_sql, None, foreign, callback)
        .await
        .expect("registered foreign wire stream");
    assert!(events
        .lock()
        .expect("typed foreign output")
        .iter()
        .any(|event| matches!(event,
            QueryStreamEvent::Rows{rows,..} if rows == &expected)));

    // Start the foreign registered statement first so its explicit route is
    // established; then run an independent query on the retained primary.
    // Cancelling by execution id must not hit the latter backend.
    let foreign_id = QueryExecutionId::new("retained-cross-target-cancel");
    driver
        .prepare_query_execution(&handle, &foreign_id)
        .await
        .expect("foreign cancellation registration");
    let foreign_marker = format!("foreign_guard_{tag}");
    let primary_marker = format!("primary_guard_{tag}");
    let wait_for_sql = async |oracle: &mut sqlx::PgConnection, database: &str, marker: &str| {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let running: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=$1 AND wait_event='PgSleep' AND strpos(query,$2)>0)"
                ).bind(database).bind(marker).fetch_one(&mut *oracle).await.expect("independent backend route oracle");
                if running {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.expect("fixture backend is actively executing its own target query");
    };
    let foreign_query = {
        let driver = Arc::clone(&driver);
        let handle = handle.clone();
        let destination = other.clone();
        let id = foreign_id.clone();
        let statement = format!("SELECT 99 FROM pg_sleep(30) /* {foreign_marker} */");
        let (_, callback) = record();
        tokio::spawn(async move {
            driver
                .query_stream_with_execution_at(
                    &handle,
                    &id,
                    &statement,
                    None,
                    SqlTarget::new(Some(&destination), None),
                    callback,
                )
                .await
        })
    };
    wait_for_sql(&mut fixture.oracle, &other, &foreign_marker).await;
    let primary_query = {
        let driver = Arc::clone(&driver);
        let handle = handle.clone();
        let statement = format!("SELECT 17 FROM pg_sleep(1) /* {primary_marker} */");
        tokio::spawn(async move { driver.query(&handle, &statement).await })
    };
    wait_for_sql(&mut fixture.oracle, database, &primary_marker).await;
    driver
        .cancel_query_with_execution(&handle, &foreign_id)
        .await
        .expect("cancel exact foreign PID");
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), foreign_query)
            .await
            .expect("foreign target stops promptly")
            .expect("stopped task"),
        Err(DriverError::QueryCancelled)
    ));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), primary_query)
            .await
            .expect("retained primary completes")
            .expect("original task")
            .expect("foreign cancel never hit primary")
            .rows,
        vec![vec![Some(Value::Integer(17))]]
    );
    assert!(matches!(
        driver
            .cancel_query_with_execution(&handle, &foreign_id)
            .await,
        Err(DriverError::QueryExecutionNotFound(_))
    ));

    // The retained session is still physically bound to its original
    // database. Check a real snapshot and auth identity before and after an
    // unannounced transaction, with no pre-observation to refresh any cache.
    let snapshot = driver
        .begin_read_snapshot(&handle)
        .await
        .expect("real source snapshot");
    let local = driver
        .query(&handle, &read_sql)
        .await
        .expect("snapshot source read");
    assert_eq!(
        local.rows,
        vec![vec![super::string(database), super::string("source")]]
    );
    sqlx::query(&format!(
        "UPDATE public.{table} SET label='external source edit'"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("separate writer commits");
    assert_eq!(
        driver
            .query(&handle, &read_sql)
            .await
            .expect("same source snapshot")
            .rows,
        local.rows
    );
    assert!(matches!(
        driver.query_at(&handle, &read_sql, foreign).await,
        Err(DriverError::TransactionError(_))
    ));
    assert!(matches!(
        driver
            .execute_at(
                &handle,
                &format!("UPDATE {table} SET label='must not apply'"),
                foreign
            )
            .await,
        Err(DriverError::TransactionError(_))
    ));
    assert_eq!(current_label(&mut other_oracle).await, "foreign edited");
    driver
        .rollback(snapshot)
        .await
        .expect("end source snapshot");
    assert_eq!(
        driver
            .query_at(&handle, &read_sql, foreign)
            .await
            .expect("foreign read after rollback")
            .rows,
        expected
    );
    assert_eq!(
        driver
            .query(&handle, &read_sql)
            .await
            .expect("source sees external commit")
            .rows,
        vec![vec![
            super::string(database),
            super::string("external source edit")
        ]]
    );
    let now = driver
        .observe_fixed_session(&handle)
        .await
        .expect("original session unchanged");
    assert_eq!(now.namespace, initial.namespace);
    assert_eq!(now.effective_identity, initial.effective_identity);
    assert_eq!(now.search_path, initial.search_path);
    driver
        .disconnect(handle)
        .await
        .expect("release both retained and shadow pools");
    drop(other_oracle);
    sqlx::raw_sql(&format!("DROP TABLE public.{table}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove source fixture");
    sqlx::query(&format!("DROP DATABASE {other}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove foreign fixture");
}
