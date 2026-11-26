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
