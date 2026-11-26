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

#[tokio::test]
async fn retained_registered_transactions_abort_real_postgres_work_on_error_or_cancel() {
    use datazen_driver_api::session::TransactionState;

    let mut fixture = Fixture::open().await;
    let table = format!("dz_retained_transaction_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE TABLE public.{table}(id int PRIMARY KEY,label text); INSERT INTO public.{table} VALUES (1,'unchanged')"))
        .execute(&mut fixture.oracle).await.expect("persisted live transaction fixture");
    let driver = Arc::new(PostgresDriver::new());
    let retained = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("real retained transaction");
    let oracle_read = async |conn: &mut sqlx::PgConnection| {
        sqlx::query_scalar::<_, String>(&format!("SELECT label FROM public.{table} WHERE id=1"))
            .fetch_one(conn)
            .await
            .expect("independent committed data")
    };

    let tx = driver
        .begin_transaction(&retained)
        .await
        .expect("begin registered write");
    let bad = QueryExecutionId::new("retained-server-error");
    driver
        .prepare_query_execution(&retained, &bad)
        .await
        .expect("register actual transaction");
    let (events, callback) = record();
    let sql=format!(
        "UPDATE public.{table} SET label='before SQL error' WHERE id=1; SELECT 10/(1-1); UPDATE public.{table} SET label='must not run' WHERE id=1"
    );
    let result = driver
        .query_stream_with_execution(&retained, &bad, &sql, None, callback)
        .await;
    assert!(
        matches!(result,Err(DriverError::QueryFailed(reason)) if reason.contains("division by zero"))
    );
    let records = events.lock().expect("events").clone();
    assert!(records.iter().any(|e| matches!(
        e,
        QueryStreamEvent::StatementEnd {
            index: 0,
            rows_affected: Some(1),
            ..
        }
    )));
    assert!(!records.iter().any(|e| matches!(
        e,
        QueryStreamEvent::StatementStart { index: 2, .. } | QueryStreamEvent::Done { .. }
    )));
    let in_error = driver
        .observe_fixed_session(&retained)
        .await
        .expect("PostgreSQL retains actual failed transaction");
    assert_eq!(in_error.transaction_state, TransactionState::Aborted);
    assert_eq!(oracle_read(&mut fixture.oracle).await, "unchanged");
    driver
        .rollback(tx)
        .await
        .expect("explicitly roll back real changes");

    let tx = driver
        .begin_transaction(&retained)
        .await
        .expect("begin cancellable transaction");
    driver
        .execute(
            &retained,
            &format!("UPDATE public.{table} SET label='before cancellation' WHERE id=1"),
        )
        .await
        .expect("one transaction-local update");
    let id = QueryExecutionId::new("retained-transaction-cancel");
    driver
        .prepare_query_execution(&retained, &id)
        .await
        .expect("active retained transaction");
    let marker = format!("sleep_{table}");
    let worker = {
        let driver = Arc::clone(&driver);
        let retained = retained.clone();
        let id = id.clone();
        let sql = format!("SELECT 1 FROM pg_sleep(30) /* {marker} */");
        let (_, callback) = record();
        tokio::spawn(async move {
            driver
                .query_stream_with_execution(&retained, &id, &sql, None, callback)
                .await
        })
    };
    tokio::time::timeout(Duration::from_secs(5),async {
        loop {
            let pending:bool=sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event='PgSleep' AND strpos(query,$1)>0)"
            ).bind(&marker).fetch_one(&mut fixture.oracle).await.expect("actual backend waiting");
            if pending { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("transaction worker is really executing");
    driver
        .cancel_query_with_execution(&retained, &id)
        .await
        .expect("exact out-of-band PostgreSQL cancel");
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .expect("prompt cancellation")
            .expect("worker completes"),
        Err(DriverError::QueryCancelled)
    ));
    assert_eq!(
        driver
            .observe_fixed_session(&retained)
            .await
            .expect("still retained")
            .transaction_state,
        TransactionState::Aborted
    );
    assert_eq!(oracle_read(&mut fixture.oracle).await, "unchanged");
    driver
        .rollback(tx)
        .await
        .expect("recover cancelled retained transaction");
    assert_eq!(
        driver
            .observe_fixed_session(&retained)
            .await
            .expect("reusable PostgreSQL session")
            .transaction_state,
        TransactionState::Idle
    );
    assert_eq!(oracle_read(&mut fixture.oracle).await, "unchanged");
    driver
        .disconnect(retained)
        .await
        .expect("release retained resources");
    sqlx::query(&format!("DROP TABLE public.{table}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove transaction fixture");
}

#[tokio::test]
async fn postgres_external_query_interruption_and_expired_execution_target_remain_distinct() {
    let mut fixture = Fixture::open().await;
    let driver = Arc::new(PostgresDriver::new());
    let pooled = driver
        .connect(&fixture.config)
        .await
        .expect("live query target");
    let target = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("second retained target");
    let id = QueryExecutionId::new("external-worker-cancel");
    driver
        .prepare_query_execution(&pooled, &id)
        .await
        .expect("register expected query");
    let marker = format!("external_cancel_{}", uuid::Uuid::new_v4().simple());
    let (events, callback) = record();
    let task = {
        let driver = Arc::clone(&driver);
        let handle = pooled.clone();
        let id = id.clone();
        let sql = format!("SELECT 42 FROM pg_sleep(30) /* {marker} */");
        tokio::spawn(async move {
            driver
                .query_stream_with_execution(&handle, &id, &sql, None, callback)
                .await
        })
    };
    let started_pid=tokio::time::timeout(Duration::from_secs(5),async{
       loop {
           let pid=sqlx::query_scalar::<_,i32>(
              "SELECT pid FROM pg_stat_activity WHERE wait_event='PgSleep' AND strpos(query,$1)>0"
           ).bind(&marker).fetch_optional(&mut fixture.oracle).await.expect("independent active PostgreSQL PID");
           if let Some(pid)=pid { break pid; }
           tokio::time::sleep(Duration::from_millis(20)).await;
       }
    }).await.expect("the actual external worker was running");
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT pg_cancel_backend($1)")
            .bind(started_pid)
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("fixture server independently interrupts this statement")
    );
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("remote interruption completes")
        .expect("worker result");
    assert!(matches!(result,Err(DriverError::QueryFailed(message)) if message.contains("canceling statement")),
      "without an accepted execution cancel, PostgreSQL interruptions are real errors, not reported as the user's own cancellation");
    assert!(!events
        .lock()
        .expect("result events")
        .iter()
        .any(|e| matches!(e, QueryStreamEvent::Done { .. })));
    assert_eq!(
        driver
            .query(&pooled, "SELECT 11")
            .await
            .expect("healthy independent result after cancellation")
            .rows,
        vec![vec![Some(Value::Integer(11))]]
    );

    let live_pid = match driver
        .query(&target, "SELECT pg_backend_pid()")
        .await
        .expect("retained current PID")
        .rows[0][0]
    {
        Some(Value::Integer(pid)) => i32::try_from(pid).expect("int4 pg backend"),
        _ => panic!("unexpected PostgreSQL PID"),
    };
    let lost = QueryExecutionId::new("retained-cancel-target-lost");
    driver
        .prepare_query_execution(&target, &lost)
        .await
        .expect("register the original retained backend");
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT pg_terminate_backend($1)")
            .bind(live_pid)
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("target-only backend closed externally")
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let present: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1)")
                    .bind(live_pid)
                    .fetch_one(&mut fixture.oracle)
                    .await
                    .expect("independent original PID is retired");
            if !present {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("terminated PostgreSQL target goes away");
    assert!(
        matches!(
            driver.cancel_query_with_execution(&target, &lost).await,
            Err(DriverError::QueryExecutionNotFound(_))
        ),
        "a vanished PID cannot be acknowledged as an accepted cancellation"
    );
    driver
        .cleanup_query_execution(&target, &lost)
        .await
        .expect("discard only its stopped registration");
    assert!(driver.observe_fixed_session(&target).await.is_err());
    driver
        .disconnect(target)
        .await
        .expect("drain retained handle");
    driver
        .disconnect(pooled)
        .await
        .expect("close original pool");
}

#[tokio::test]
async fn fixed_read_snapshot_refuses_nesting_and_protects_repeatable_reads() {
    let mut fixture = Fixture::open().await;
    let table = format!("dz_snapshot_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE TABLE {table}(id int PRIMARY KEY); INSERT INTO {table} VALUES (1)"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("independent snapshot fixture");
    let driver = PostgresDriver::new();
    let handle = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("real retained checkout");
    driver
        .execute(&handle, "BEGIN")
        .await
        .expect("original transaction opened as raw SQL");
    driver
        .execute(&handle, &format!("INSERT INTO {table} VALUES (90)"))
        .await
        .expect("write staged in the original transaction");
    assert!(
        matches!(
            driver.begin_read_snapshot(&handle).await,
            Err(DriverError::TransactionError(_))
        ),
        "a writable transaction must never be presented as a read-only snapshot"
    );
    assert!(matches!(
        driver.begin_transaction(&handle).await,
        Err(DriverError::TransactionError(_))
    ));
    let local_count = format!("SELECT count(*) FROM {table}");
    assert_eq!(
        driver
            .query(&handle, &local_count)
            .await
            .expect("original transaction remains usable")
            .rows,
        vec![vec![Some(Value::Integer(2))]]
    );
    driver
        .execute(&handle, "ROLLBACK")
        .await
        .expect("rollback original write");
    let original: Vec<i32> = sqlx::query_scalar(&format!("SELECT id FROM {table} ORDER BY id"))
        .fetch_all(&mut fixture.oracle)
        .await
        .expect("outside view of rollback");
    assert_eq!(original, [1]);

    let snapshot = driver
        .begin_read_snapshot(&handle)
        .await
        .expect("physical snapshot");
    let settings = driver
        .query(&handle, "SELECT current_setting('transaction_read_only'), current_setting('transaction_isolation')")
        .await.expect("actual retained PostgreSQL transaction settings");
    assert_eq!(
        settings.rows,
        vec![vec![super::string("on"), super::string("repeatable read")]]
    );
    assert_eq!(
        driver
            .query(&handle, &local_count)
            .await
            .expect("establish repeatable transaction snapshot")
            .rows,
        vec![vec![Some(Value::Integer(1))]]
    );
    sqlx::query(&format!("INSERT INTO {table} VALUES (2)"))
        .execute(&mut fixture.oracle)
        .await
        .expect("independent concurrent committed insert");
    let (events, callback) = record();
    driver
        .query_stream(
            &handle,
            &format!("SELECT id FROM {table} ORDER BY id"),
            None,
            callback,
        )
        .await
        .expect("legacy streaming preserves the fixed snapshot");
    {
        let events = events.lock().unwrap();
        assert!(events.iter().any(|event| matches!(event, QueryStreamEvent::Rows{rows,..} if rows == &vec![vec![Some(Value::Integer(1))]])));
        assert!(events
            .iter()
            .any(|event| matches!(event, QueryStreamEvent::Done { .. })));
    }
    let rejected = driver
        .execute_with_params(
            &handle,
            &format!("INSERT INTO {table} VALUES ($1::int)"),
            &[Value::Integer(3)],
        )
        .await
        .expect_err("actual server forbids writing through a read-only snapshot");
    assert!(rejected.to_string().contains("read-only"), "{rejected}");
    assert!(
        matches!(
            driver.begin_read_snapshot(&handle).await,
            Err(DriverError::TransactionError(_))
        ),
        "an aborted retained transaction is not a new snapshot"
    );
    driver
        .rollback(snapshot)
        .await
        .expect("rollback fixed snapshot");
    assert_eq!(
        driver
            .query(&handle, &local_count)
            .await
            .expect("fresh view after rollback")
            .rows,
        vec![vec![Some(Value::Integer(2))]]
    );
    let outside: Vec<i32> = sqlx::query_scalar(&format!("SELECT id FROM {table} ORDER BY id"))
        .fetch_all(&mut fixture.oracle)
        .await
        .expect("independent persisted contents");
    assert_eq!(outside, [1, 2]);
    driver
        .disconnect(handle)
        .await
        .expect("close retained backend");
    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&mut fixture.oracle)
        .await
        .unwrap();
}

#[tokio::test]
async fn postgres_scripts_stop_after_first_server_error_without_claiming_later_writes() {
    let mut fixture = Fixture::open().await;
    let table = format!("dz_script_error_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!(
        "CREATE TABLE {table}(id int PRIMARY KEY, notes text)"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("independent script fixture");
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("pooled handle");
    assert_eq!(
        driver
            .execute_with_params(
                &handle,
                &format!("INSERT INTO {table} VALUES ($1::int, $2::text)"),
                &[Value::Integer(1), Value::String("literal '; 🙂".into()),]
            )
            .await
            .expect("pooled prepared write"),
        1,
    );
    let first_error = format!("INSERT INTO {table} VALUES (2,'pre-error'); SELECT 1/0; INSERT INTO {table} VALUES (3,'must not run')");
    let error = driver
        .query_multi(&handle, &first_error, None)
        .await
        .expect_err("real division by zero stops the script");
    assert!(
        error.to_string().contains("SELECT 1/0") && error.to_string().contains("division by zero"),
        "{error}"
    );
    let read_sql = format!("SELECT id, notes FROM {table} ORDER BY id");
    let outside: Vec<(i32, String)> = sqlx::query_as(&read_sql)
        .fetch_all(&mut fixture.oracle)
        .await
        .unwrap();
    assert_eq!(
        outside,
        [
            (1, "literal '; 🙂".to_string()),
            (2, "pre-error".to_string())
        ]
    );

    let tx = driver
        .begin_transaction(&handle)
        .await
        .expect("physical transaction");
    let duplicate = format!("INSERT INTO {table} VALUES (4,'staged'); INSERT INTO {table} VALUES (1,'duplicate'); INSERT INTO {table} VALUES (5,'must not run')");
    let error = driver
        .query_multi(&handle, &duplicate, None)
        .await
        .expect_err("duplicate key aborts the physical transaction");
    assert!(
        error.to_string().contains("duplicate key") && error.to_string().contains("'duplicate'"),
        "{error}"
    );
    driver
        .rollback(tx)
        .await
        .expect("discard aborted script and staged insert");
    let outside_after: Vec<(i32, String)> = sqlx::query_as(&read_sql)
        .fetch_all(&mut fixture.oracle)
        .await
        .unwrap();
    assert_eq!(outside_after, outside);

    let empty = driver
        .query_multi(&handle, "; ;", None)
        .await
        .expect("empty batch is a successful no-op");
    assert!(empty.results.is_empty());
    assert_eq!(empty.total_time_ms, 0);
    let (events, callback) = record();
    driver
        .query_stream(&handle, " ; ", None, callback)
        .await
        .expect("empty stream completes");
    assert!(matches!(
        events.lock().unwrap().as_slice(),
        [QueryStreamEvent::Done { total_time_ms: 0 }]
    ));
    let (events, callback) = record();
    let error = driver
        .query_stream(
            &handle,
            &format!("INSERT INTO {table} VALUES (1,'duplicate'); SELECT 1"),
            None,
            callback,
        )
        .await
        .expect_err("stream reports real DML error");
    assert!(error.to_string().contains("duplicate key"), "{error}");
    assert!(!events.lock().unwrap().iter().any(|event| matches!(
        event,
        QueryStreamEvent::Done { .. } | QueryStreamEvent::Rows { .. }
    )));
    let explain = driver
        .explain(&handle, "SELECT 1; SELECT 2")
        .await
        .expect_err("explain cannot silently pick one of two statements");
    assert!(matches!(explain, DriverError::InvalidConfig(_)));
    driver
        .disconnect(handle)
        .await
        .expect("close pooled handle");
    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&mut fixture.oracle)
        .await
        .unwrap();
}
