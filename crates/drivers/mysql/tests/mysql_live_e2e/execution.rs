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
