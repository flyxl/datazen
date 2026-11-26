use super::Fixture;
use datazen_driver_api::{DatabaseDriver, DriverError, QueryExecutionId, QueryStreamEvent};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mysql_execution_id_cannot_run_twice_or_execute_after_between_statement_cancel() {
    let mut f = Fixture::open().await;
    let driver = Arc::new(datazen_driver_mysql::MysqlDriver::new(false));
    let handle = driver
        .connect(&f.cfg)
        .await
        .expect("real execution checkout");
    let table = format!("dz_execution_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE TABLE {table} (id INT PRIMARY KEY)"))
        .execute(&mut f.oracle)
        .await
        .expect("independent disposable table");
    let execution = QueryExecutionId::new("same-mysql-execution");
    driver
        .prepare_query_execution(&handle, &execution)
        .await
        .expect("one registered execution");
    let (noticed_tx, noticed_rx) = tokio::sync::oneshot::channel();
    let notice = Arc::new(Mutex::new(Some(noticed_tx)));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let worker = tokio::spawn({
        let driver = driver.clone();
        let handle = handle.clone();
        let execution = execution.clone();
        let release = release.clone();
        let owned_sql = format!("SELECT 1 AS began; INSERT INTO {table} VALUES(7)");
        async move {
            driver
                .query_stream_with_execution(
                    &handle,
                    &execution,
                    &owned_sql,
                    None,
                    Arc::new(move |event| {
                        if matches!(event, QueryStreamEvent::Rows { index: 0, .. }) {
                            if let Some(tx) = notice.lock().unwrap().take() {
                                let _ = tx.send(());
                                let guard = release.0.lock().unwrap();
                                let _wait = release
                                    .1
                                    .wait_timeout_while(guard, Duration::from_secs(8), |go| !*go)
                                    .unwrap();
                            }
                        }
                    }),
                )
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(4), noticed_rx)
        .await
        .expect("actual first MySQL row emitted")
        .unwrap();
    let duplicate = tokio::time::timeout(
        Duration::from_secs(4),
        driver.query_stream_with_execution(
            &handle,
            &execution,
            &format!("INSERT INTO {table} VALUES(9)"),
            None,
            Arc::new(|_| {}),
        ),
    )
    .await;
    let cancel = driver
        .cancel_query_with_execution(&handle, &execution)
        .await;
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    let terminal = tokio::time::timeout(Duration::from_secs(4), worker)
        .await
        .expect("owner finishes")
        .expect("worker task");
    assert!(
        matches!(duplicate,Ok(Err(DriverError::QueryFailed(message))) if message.contains("already running")),
        "a second call must not steal the active MySQL execution"
    );
    cancel.expect("original remains cancellable");
    assert!(matches!(terminal, Err(DriverError::QueryCancelled)));
    let independent: Vec<i32> = sqlx::query_scalar(&format!("SELECT id FROM {table} ORDER BY id"))
        .fetch_all(&mut f.oracle)
        .await
        .expect("independently observe no duplicate/second write");
    assert!(independent.is_empty());
    driver
        .disconnect(handle)
        .await
        .expect("settled execution connection");
}

#[tokio::test]
async fn mysql_pending_cancel_does_not_kill_the_other_checkout_under_real_pool_contention() {
    let mut f = Fixture::open().await;
    let driver = Arc::new(datazen_driver_mysql::MysqlDriver::new(false));
    let mut cfg = f.cfg.clone();
    cfg.max_pool_size = 1;
    let handle = driver
        .connect(&cfg)
        .await
        .expect("one real data checkout and separate control pool");
    let table = format!("dz_pending_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE TABLE {table}(id INT PRIMARY KEY)"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
    let lock = format!("dz_lock_{}", uuid::Uuid::new_v4().simple());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT GET_LOCK(?,0)")
            .bind(&lock)
            .fetch_one(&mut f.oracle)
            .await
            .unwrap(),
        1
    );
    let owning = QueryExecutionId::new("holds-native-checkout");
    let waiting = QueryExecutionId::new("waiting-native-checkout");
    driver
        .prepare_query_execution(&handle, &owning)
        .await
        .unwrap();
    let actual_rows = Arc::new(Mutex::new(Vec::new()));
    let first = tokio::spawn({
        let (driver, handle, owning, rows) = (
            driver.clone(),
            handle.clone(),
            owning.clone(),
            actual_rows.clone(),
        );
        let sql = format!("SELECT GET_LOCK('{lock}',12) AS acquired");
        async move {
            driver
                .query_stream_with_execution(
                    &handle,
                    &owning,
                    &sql,
                    None,
                    Arc::new(move |event| {
                        if let QueryStreamEvent::Rows { rows: batch, .. } = event {
                            rows.lock().unwrap().extend(batch);
                        }
                    }),
                )
                .await
        }
    });
    let is_waiting = async |conn: &mut sqlx::MySqlConnection| {
        let matches:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE ID<>CONNECTION_ID() AND STATE='User lock' AND LOCATE(?, INFO)>0")
            .bind(&lock).fetch_one(conn).await.unwrap();
        matches == 1
    };
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if is_waiting(&mut f.oracle).await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("first task holds the only data connection at a real native lock");
    driver
        .prepare_query_execution(&handle, &waiting)
        .await
        .unwrap();
    let next = tokio::spawn({
        let (driver, handle, waiting) = (driver.clone(), handle.clone(), waiting.clone());
        let sql = format!("INSERT INTO {table} VALUES(81)");
        async move {
            driver
                .query_stream_with_execution(&handle, &waiting, &sql, None, Arc::new(|_| {}))
                .await
        }
    });
    driver
        .cancel_query_with_execution(&handle, &waiting)
        .await
        .expect("accept pending cancellation without borrowing or killing first connection");
    assert!(
        is_waiting(&mut f.oracle).await,
        "pending cancellation must not interrupt the currently running native query"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT RELEASE_LOCK(?)")
            .bind(&lock)
            .fetch_one(&mut f.oracle)
            .await
            .unwrap(),
        1
    );
    first
        .await
        .unwrap()
        .expect("first actual query not canceled");
    assert_eq!(
        *actual_rows.lock().unwrap(),
        [vec![Some(datazen_driver_api::Value::Integer(1))]]
    );
    assert!(matches!(
        next.await.unwrap(),
        Err(DriverError::QueryCancelled)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&mut f.oracle)
            .await
            .unwrap(),
        0
    );
    let empty = QueryExecutionId::new("empty-registered-batch");
    driver
        .prepare_query_execution(&handle, &empty)
        .await
        .unwrap();
    let ended = Arc::new(Mutex::new(false));
    driver
        .query_stream_with_execution(&handle, &empty, " ;  ; ", None, {
            let ended = ended.clone();
            Arc::new(move |event| {
                if matches!(event, QueryStreamEvent::Done { .. }) {
                    *ended.lock().unwrap() = true;
                }
            })
        })
        .await
        .expect("empty owned execution settles without retargeting native backend");
    assert!(*ended.lock().unwrap());
    assert!(matches!(
        driver.cancel_query_with_execution(&handle, &empty).await,
        Err(DriverError::QueryExecutionNotFound(_))
    ));
    driver
        .disconnect(handle)
        .await
        .expect("close actual original pool and advisory lock");
}
