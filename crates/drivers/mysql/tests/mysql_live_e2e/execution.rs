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

#[tokio::test]
async fn mysql_registered_batch_native_failure_does_not_steal_session_or_run_remaining_write() {
    use datazen_driver_api::Value;
    let mut f = Fixture::open().await;
    let driver = datazen_driver_mysql::MysqlDriver::new(false);
    let foreign = driver.connect(&f.cfg).await.unwrap();
    let table = format!("dz_batch_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!(
        "CREATE TABLE {table}(id INT PRIMARY KEY,label VARCHAR(20))"
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    let persisted = async |oracle: &mut sqlx::MySqlConnection| {
        sqlx::query_scalar::<_, i32>(&format!("SELECT id FROM {table} ORDER BY id"))
            .fetch_all(oracle)
            .await
            .unwrap()
    };
    for (retained, first, last) in [(false, 1, 2), (true, 3, 4)] {
        let h = if retained {
            driver.connect_fixed_session(&f.cfg).await.unwrap()
        } else {
            driver.connect(&f.cfg).await.unwrap()
        };
        let id = QueryExecutionId::new(format!("native-batch-{}", uuid::Uuid::new_v4()));
        let observation = Arc::new(Mutex::new(Vec::new()));
        let callback: datazen_driver_api::QueryStreamCallback = Arc::new({
            let observation = observation.clone();
            move |event| observation.lock().unwrap().push(event)
        });
        driver.prepare_query_execution(&h, &id).await.unwrap();
        assert!(
            matches!(driver.prepare_query_execution(&foreign,&id).await,Err(DriverError::QueryFailed(m)) if m.contains("already registered"))
        );
        assert!(matches!(
            driver
                .query_stream_with_execution(
                    &foreign,
                    &id,
                    "SELECT CONNECTION_ID()",
                    None,
                    callback.clone()
                )
                .await,
            Err(DriverError::QueryExecutionSessionMismatch)
        ));
        assert!(matches!(
            driver.cancel_query_with_execution(&foreign, &id).await,
            Err(DriverError::QueryExecutionSessionMismatch)
        ));
        let script = format!(
            "INSERT INTO {table} VALUES ({first},'real'); SELECT missing_column FROM {table}; INSERT INTO {table} VALUES ({last}, 'must not run')"
        );
        let failure = driver
            .query_stream_with_execution(&h, &id, &script, None, callback.clone())
            .await
            .unwrap_err();
        assert!(
            matches!(failure,DriverError::QueryFailed(m) if m.contains("Unknown column")&&m.contains("missing_column"))
        );
        assert!(
            !observation
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, QueryStreamEvent::Done { .. }))
        );
        assert!(matches!(
            driver
                .query_stream_with_execution(&h, &id, "SELECT 1", None, callback.clone())
                .await,
            Err(DriverError::QueryExecutionNotFound(_))
        ));
        let state = persisted(&mut f.oracle).await;
        assert!(
            state.contains(&first) && !state.contains(&last),
            "only writes before the real SQL error may persist"
        );
        assert!(
            driver
                .query_multi(&h, "; ;", None)
                .await
                .unwrap()
                .results
                .is_empty()
        );
        driver.prepare_query_execution(&h, &id).await.unwrap();
        driver
            .query_stream_with_execution(&h, &id, " ;; ", None, callback.clone())
            .await
            .unwrap();
        assert!(matches!(
            observation.lock().unwrap().last(),
            Some(QueryStreamEvent::Done { total_time_ms: 0 })
        ));
        if retained {
            driver.execute(&h, "START TRANSACTION").await.unwrap();
            let script = format!(
                "INSERT INTO {table} VALUES (99,'rollback'); SELECT id FROM {table} WHERE id=99;"
            );
            let result = driver.query_multi(&h, &script, None).await.unwrap();
            assert_eq!(result.results[0].rows_affected, Some(1));
            assert_eq!(result.results[1].rows[0][0], Some(Value::Integer(99)));
            assert!(!persisted(&mut f.oracle).await.contains(&99));
            driver.execute(&h, "ROLLBACK").await.unwrap();
            assert!(!persisted(&mut f.oracle).await.contains(&99));
        }
        driver.disconnect(h).await.unwrap();
    }
    assert_eq!(persisted(&mut f.oracle).await, [1, 3]);
    driver.disconnect(foreign).await.unwrap();
}

#[tokio::test]
async fn mysql_pending_fixed_execution_cannot_cancel_a_different_native_query_on_the_same_session()
{
    use datazen_driver_api::Value;
    let mut f = Fixture::open().await;
    let driver = Arc::new(datazen_driver_mysql::MysqlDriver::new(false));
    let retained = driver.connect_fixed_session(&f.cfg).await.unwrap();
    let peer = driver.connect(&f.cfg).await.unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let table = format!("dz_pinned_cancel_{suffix}");
    let lock = format!("dz_pending_fixed_{suffix}");
    sqlx::query(&format!("CREATE TABLE {table}(id INT PRIMARY KEY)"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
    let acquired: i64 = sqlx::query_scalar("SELECT GET_LOCK(?,0)")
        .bind(&lock)
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(acquired, 1);
    let physical = driver
        .query(&retained, "SELECT CONNECTION_ID()")
        .await
        .unwrap();
    let thread = match physical.rows[0][0] {
        Some(Value::Integer(v)) => u64::try_from(v).unwrap(),
        ref other => panic!("{other:?}"),
    };
    let active = QueryExecutionId::new(format!("native-holder-{suffix}"));
    let pending = QueryExecutionId::new(format!("native-pending-{suffix}"));
    let owner_rows = Arc::new(Mutex::new(Vec::new()));
    driver
        .prepare_query_execution(&retained, &active)
        .await
        .unwrap();
    let worker = tokio::spawn({
        let (driver, handle, id, lock, rows) = (
            driver.clone(),
            retained.clone(),
            active.clone(),
            lock.clone(),
            owner_rows.clone(),
        );
        async move {
            driver
                .query_stream_with_execution(
                    &handle,
                    &id,
                    &format!("SELECT GET_LOCK('{lock}',10)"),
                    None,
                    Arc::new(move |event| {
                        if let QueryStreamEvent::Rows { rows: chunk, .. } = event {
                            rows.lock().unwrap().extend(chunk)
                        }
                    }),
                )
                .await
        }
    });
    let observation = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let matches: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE ID=? AND INFO LIKE ?",
            )
            .bind(thread)
            .bind(format!("%{lock}%"))
            .fetch_one(&mut f.oracle)
            .await
            .unwrap();
            if matches > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if observation.is_err() {
        let _: Option<i64> = sqlx::query_scalar("SELECT RELEASE_LOCK(?)")
            .bind(&lock)
            .fetch_one(&mut f.oracle)
            .await
            .unwrap();
        let _ = worker.await;
        panic!("fixture must independently observe the actual native waiting statement");
    }
    driver
        .prepare_query_execution(&retained, &pending)
        .await
        .unwrap();
    assert!(matches!(
        driver.cleanup_query_execution(&peer, &active).await,
        Err(DriverError::QueryExecutionSessionMismatch)
    ));
    let pending_cancel = driver
        .cancel_query_with_execution(&retained, &pending)
        .await;
    let released: i64 = sqlx::query_scalar("SELECT RELEASE_LOCK(?)")
        .bind(&lock)
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(released, 1);
    pending_cancel.expect("pending cancellation should not need any server KILL");
    tokio::time::timeout(Duration::from_secs(5), worker)
        .await
        .unwrap()
        .unwrap()
        .expect("unrelated native owner must finish");
    assert_eq!(
        *owner_rows.lock().unwrap(),
        [vec![Some(Value::Integer(1))]],
        "a pending cancellation cannot kill the active statement"
    );
    assert!(matches!(
        driver
            .query_stream_with_execution(
                &retained,
                &pending,
                &format!("INSERT INTO {table} VALUES(9)"),
                None,
                Arc::new(|_| {})
            )
            .await,
        Err(DriverError::QueryCancelled)
    ));
    let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(matches!(
        driver
            .cancel_query_with_execution(&retained, &pending)
            .await,
        Err(DriverError::QueryExecutionNotFound(_))
    ));
    driver.disconnect(retained).await.unwrap();
    driver.disconnect(peer).await.unwrap();
}

#[tokio::test]
async fn mysql_registered_native_sleep_interrupt_cannot_end_as_a_successful_single_statement() {
    use datazen_driver_api::Value;
    let mut f = Fixture::open().await;
    let driver = Arc::new(datazen_driver_mysql::MysqlDriver::new(false));
    for fixed in [false, true] {
        let h = if fixed {
            driver.connect_fixed_session(&f.cfg).await.unwrap()
        } else {
            driver.connect(&f.cfg).await.unwrap()
        };
        let token = format!("dz_sleep_{}", uuid::Uuid::new_v4().simple());
        let id = QueryExecutionId::new(token.clone());
        driver.prepare_query_execution(&h, &id).await.unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let worker = tokio::spawn({
            let (d, h, id, events, token) = (
                driver.clone(),
                h.clone(),
                id.clone(),
                events.clone(),
                token.clone(),
            );
            async move {
                d.query_stream_with_execution(
                    &h,
                    &id,
                    &format!("SELECT SLEEP(10) AS {token}"),
                    None,
                    Arc::new(move |event| events.lock().unwrap().push(event)),
                )
                .await
            }
        });
        let seen=tokio::time::timeout(Duration::from_secs(5),async {
          loop {
             let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE ID<>CONNECTION_ID() AND INFO LIKE ?")
                 .bind(format!("%{token}%")).fetch_one(&mut f.oracle).await.unwrap();
             if count>0 {break;}
             tokio::time::sleep(Duration::from_millis(20)).await;
          }
       }).await;
        assert!(
            seen.is_ok(),
            "cancel only after MySQL accepted and is running this exact SQL"
        );
        driver.cancel_query_with_execution(&h, &id).await.unwrap();
        let result = tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(result, Err(DriverError::QueryCancelled)),
            "native canceled statement advertised wrong terminal status: {result:?}"
        );
        assert!(
            !events
                .lock()
                .unwrap()
                .iter()
                .any(|event| matches!(event, QueryStreamEvent::Done { .. }))
        );
        let reuse = driver
            .query(&h, "SELECT 24 AS native_after_cancel")
            .await
            .unwrap();
        assert_eq!(reuse.rows, [vec![Some(Value::Integer(24))]]);
        driver.disconnect(h).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mysql_cancel_receipt_does_not_claim_success_when_native_thread_has_disappeared() {
    use datazen_driver_api::Value;
    let mut f = Fixture::open().await;
    let driver = Arc::new(datazen_driver_mysql::MysqlDriver::new(false));
    let h = driver.connect_fixed_session(&f.cfg).await.unwrap();
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let table = format!("dz_stale_cancel_{nonce}");
    sqlx::raw_sql(&format!("CREATE TABLE {table}(id INT PRIMARY KEY)"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
    let identity = driver.query(&h, "SELECT CONNECTION_ID()").await.unwrap();
    let Some(Value::Integer(pid)) = identity.rows[0][0] else {
        panic!("actual MySQL backend")
    };
    let id = QueryExecutionId::new(format!("stale-mysql-{nonce}"));
    driver.prepare_query_execution(&h, &id).await.unwrap();
    let (seen, notice) = tokio::sync::oneshot::channel();
    let seen = Arc::new(Mutex::new(Some(seen)));
    let ready = Arc::new((Mutex::new(false), Condvar::new()));
    let script = format!("SELECT 37 AS real_row; INSERT INTO {table} VALUES (1)");
    let task = tokio::spawn({
        let (driver, h, id, ready) = (driver.clone(), h.clone(), id.clone(), ready.clone());
        async move {
            driver
                .query_stream_with_execution(
                    &h,
                    &id,
                    &script,
                    None,
                    Arc::new(move |event| {
                        if matches!(event, QueryStreamEvent::Rows { index: 0, .. }) {
                            tokio::task::block_in_place(|| {
                                if let Some(ack) = seen.lock().unwrap().take() {
                                    let _ = ack.send(());
                                }
                                let guard = ready.0.lock().unwrap();
                                let _wait = ready
                                    .1
                                    .wait_timeout_while(guard, Duration::from_secs(8), |released| {
                                        !*released
                                    })
                                    .unwrap();
                            });
                        }
                    }),
                )
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(4), notice)
        .await
        .unwrap()
        .unwrap();
    sqlx::raw_sql(&format!("KILL CONNECTION {pid}"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE ID=?",
            )
            .bind(pid)
            .fetch_one(&mut f.oracle)
            .await
            .unwrap();
            if count == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("original real backend is gone before cancellation");
    let receipt = tokio::time::timeout(
        Duration::from_secs(4),
        driver.cancel_query_with_execution(&h, &id),
    )
    .await;
    *ready.0.lock().unwrap() = true;
    ready.1.notify_all();
    let terminal = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(&receipt,Ok(Err(DriverError::QueryFailed(message))) if message.contains("Unknown thread id")),
        "native cancel completion for vanished thread: {receipt:?}; stream: {terminal:?}"
    );
    assert!(
        matches!(terminal, Err(DriverError::QueryFailed(_))),
        "do not turn a lost transport into a successful cancel"
    );
    let observed: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(
        observed, 0,
        "defunct registered execution cannot replay its write on another socket"
    );
    driver.disconnect(h).await.unwrap();
}
