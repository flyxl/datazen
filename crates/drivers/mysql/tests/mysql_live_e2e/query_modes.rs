use super::Fixture;
use datazen_driver_api::{DatabaseDriver, QueryExecutionId, QueryStreamEvent, Value};
use datazen_driver_mysql::MysqlDriver;
use std::sync::{Arc, Mutex};

fn collect() -> (
    Arc<Mutex<Vec<QueryStreamEvent>>>,
    datazen_driver_api::QueryStreamCallback,
) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let copy = events.clone();
    (
        events,
        Arc::new(move |event| copy.lock().unwrap().push(event)),
    )
}

#[tokio::test]
async fn mysql_streamed_use_rebases_warmed_statement_on_same_physical_connection() {
    let mut f = Fixture::open().await;
    let driver = MysqlDriver::new(false);
    let h = driver
        .connect_fixed_session(&f.cfg)
        .await
        .expect("actual fixed MySQL connection");
    let foreign = std::env::var("TEST_MYSQL_DATABASE_B").unwrap();
    let table = format!("dz_stream_context_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE TABLE {table}(v int);INSERT INTO {table} VALUES(19);
         CREATE TABLE {foreign}.{table}(v int);INSERT INTO {foreign}.{table} VALUES(81)"
    ))
    .execute(&mut f.oracle)
    .await
    .expect("disjoint real data under same relation name");
    let same = format!("SELECT v FROM {table}");
    let first = driver
        .query(&h, &same)
        .await
        .expect("warm MySQL prepared statement in A");
    assert_eq!(first.rows, [vec![Some(Value::Integer(19))]]);
    let (events, callback) = collect();
    driver
        .query_stream(&h, &format!("USE `{foreign}`; {same}"), None, callback)
        .await
        .expect("real stream session context change");
    assert!(events.lock().unwrap().iter().any(|event| matches!(event,
        QueryStreamEvent::Rows{index:1,rows} if rows==&vec![vec![Some(Value::Integer(81))]])));
    let second = driver
        .query(&h, &same)
        .await
        .expect("same prepared SQL now returns B");
    assert_eq!(second.rows, [vec![Some(Value::Integer(81))]]);
    let id = QueryExecutionId::new("native-switch-back");
    let (events, callback) = collect();
    driver
        .prepare_query_execution(&h, &id)
        .await
        .expect("actual registered script");
    driver
        .query_stream_with_execution(
            &h,
            &id,
            &format!("USE `{}`; {same}", f.database),
            None,
            callback,
        )
        .await
        .expect("registered retained stream changes back");
    assert!(events.lock().unwrap().iter().any(|event| matches!(event,
        QueryStreamEvent::Rows{index:1,rows} if rows==&vec![vec![Some(Value::Integer(19))]])));
    let native: (i32, i32) = sqlx::query_as(&format!(
        "SELECT (SELECT v FROM {table}),(SELECT v FROM {foreign}.{table})"
    ))
    .fetch_one(&mut f.oracle)
    .await
    .expect("independent ownership oracle");
    assert_eq!(native, (19, 81));
    driver
        .disconnect(h)
        .await
        .expect("release same fixture backend");
}

#[tokio::test]
async fn mysql_query_shapes_preserve_empty_headers_and_limit_with_real_sql_comments() {
    let f = Fixture::open().await;
    let driver = MysqlDriver::new(false);
    for h in [
        driver
            .connect(&f.cfg)
            .await
            .expect("real pooled connection"),
        driver
            .connect_fixed_session(&f.cfg)
            .await
            .expect("real retained connection"),
    ] {
        let empty = "/* query editor comment */ SELECT CAST(1 AS SIGNED) AS field WHERE FALSE";
        let direct = driver
            .query(&h, empty)
            .await
            .expect("empty real SQL still has column shape");
        assert_eq!(
            direct
                .columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["field"]
        );
        let script = format!(
            "{empty}; -- statement label\nSELECT 1 AS n UNION ALL SELECT 2 UNION ALL SELECT 3;"
        );
        let multi = driver
            .query_multi(&h, &script, Some(2))
            .await
            .expect("MySQL native multi-result query and limit");
        assert_eq!(multi.results.len(), 2);
        assert_eq!(multi.results[0].columns[0].name, "field");
        assert!(multi.results[0].rows.is_empty());
        assert_eq!(multi.results[1].rows.len(), 2);
        assert!(multi.results[1].truncated);
        let (events, callback) = collect();
        driver
            .query_stream(&h, empty, None, callback)
            .await
            .expect("actual empty stream");
        assert!(events.lock().unwrap().iter().any(|event|matches!(event,QueryStreamEvent::StatementStart{columns,..} if columns.first().is_some_and(|c|c.name=="field"))));
        let high = driver
            .query_multi(&h, "SELECT 42", Some(u32::MAX))
            .await
            .expect("largest advertised limit does not overflow");
        assert_eq!(high.results[0].rows.len(), 1);
        let arithmetic = driver
            .query_multi(
                &h,
                "SELECT 3--2 AS n; # mysql comment\nSELECT 9 AS n;",
                None,
            )
            .await
            .expect("arithmetic dashes are not a MySQL comment without whitespace");
        let first: i64 = match arithmetic.results[0].rows[0][0] {
            Some(Value::Integer(v)) => v,
            _ => panic!("actual arithmetic integer"),
        };
        assert_eq!(first, 5);
        assert_eq!(arithmetic.results[1].rows, [vec![Some(Value::Integer(9))]]);
        driver.disconnect(h).await.expect("native teardown");
    }
}

#[tokio::test]
async fn mysql_native_failed_prepared_and_text_statements_do_not_poison_checkout_or_run_batch_tail()
{
    use datazen_driver_api::DriverError;
    let mut f = Fixture::open().await;
    let driver = MysqlDriver::new(false);
    let n = uuid::Uuid::new_v4().simple().to_string();
    let actual = format!("dz_stmt_{n}");
    let absent = format!("dz_missing_{n}");
    sqlx::raw_sql(&format!(
        "CREATE TABLE {actual}(id INT PRIMARY KEY,amount INT); INSERT INTO {actual} VALUES(1,7);"
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    fn native<T: std::fmt::Debug>(result: Result<T, DriverError>, message: &str) {
        assert!(
            matches!(&result,Err(DriverError::QueryFailed(text)) if text.contains(message)),
            "expected actual MySQL {message}: {result:?}"
        );
    }
    let pool = driver.connect(&f.cfg).await.unwrap();
    let pinned = driver.connect_fixed_session(&f.cfg).await.unwrap();
    for h in [&pool, &pinned] {
        let early = format!("SELECT id FROM {absent}");
        let early_params = format!("SELECT id FROM {absent} WHERE id=?");
        let duplicate = format!("INSERT INTO {actual} VALUES(1,99)");
        let duplicate_params = format!("INSERT INTO {actual} VALUES(?,99)");
        let want = "doesn't exist";
        native(driver.query(h, &early).await, want);
        native(
            driver
                .query_with_params(h, &early_params, &[Value::Integer(1)])
                .await,
            want,
        );
        native(driver.execute(h, &duplicate).await, "Duplicate entry");
        native(
            driver
                .execute_with_params(h, &duplicate_params, &[Value::Integer(1)])
                .await,
            "Duplicate entry",
        );
        for (bad, detail) in [(&early, want), (&duplicate, "Duplicate entry")] {
            let follow = format!("{bad}; INSERT INTO {actual} VALUES (7,91)");
            native(driver.query_multi(h, &follow, None).await, detail);
            let (events, callback) = collect();
            native(
                driver.query_stream(h, &follow, None, callback).await,
                detail,
            );
            assert!(
                !events
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|event| matches!(event, QueryStreamEvent::Done { .. }))
            );
            let id = QueryExecutionId::new(format!("native-registered-{n}-{bad}"));
            let (_, callback) = collect();
            driver.prepare_query_execution(h, &id).await.unwrap();
            native(
                driver
                    .query_stream_with_execution(h, &id, &follow, None, callback)
                    .await,
                detail,
            );
        }
        let healthy = driver
            .query_with_params(
                h,
                &format!("SELECT amount FROM {actual} WHERE id=?"),
                &[Value::Integer(1)],
            )
            .await
            .unwrap();
        assert_eq!(healthy.rows, [vec![Some(Value::Integer(7))]]);
    }
    let stored: Vec<(i32, i32)> =
        sqlx::query_as(&format!("SELECT id,amount FROM {actual} ORDER BY id"))
            .fetch_all(&mut f.oracle)
            .await
            .unwrap();
    assert_eq!(
        stored,
        [(1, 7)],
        "real statement errors must not permit the later write"
    );
    driver.disconnect(pinned).await.unwrap();
    driver.disconnect(pool).await.unwrap();
}

#[tokio::test]
async fn mysql_native_batch_handles_comments_without_suppressing_server_execution_or_row_budget() {
    let mut f = Fixture::open().await;
    let driver = MysqlDriver::new(false);
    let h = driver.connect_fixed_session(&f.cfg).await.unwrap();
    let table = format!("dz_comment_budget_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE TABLE {table}(id INT PRIMARY KEY); INSERT INTO {table} VALUES (1),(2),(3)"
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    driver.execute(&h, "SET @dz_touched=0").await.unwrap();
    let results=driver.query_multi(&h,&format!("SELECT (@dz_touched:=@dz_touched+1) AS seen FROM {table} ORDER BY id -- do not swallow server LIMIT"),Some(1))
        .await.expect("row limit survives trailing MySQL line comment");
    assert_eq!(results.results[0].rows, [vec![Some(Value::Integer(1))]]);
    assert!(results.results[0].truncated);
    let read = driver
        .query(&h, "SELECT CAST(@dz_touched AS SIGNED)")
        .await
        .unwrap();
    assert_eq!(
        read.rows,
        [vec![Some(Value::Integer(2))]],
        "native server evaluates budget plus one, not every row"
    );
    let formatted = driver
        .query_multi(
            &h,
            r#"/*!80000 SET @dz_seen=77 */; SELECT CAST(@dz_seen AS SIGNED) AS `native;``label`,
         "semicolon; quoted ""string""" AS literal; # ordinary tail is not a database statement
         /* comment only ; */"#,
            None,
        )
        .await
        .expect("skip comment-only fragments and preserve native versioned statements");
    assert_eq!(formatted.results.len(), 2);
    assert_eq!(formatted.results[1].columns[0].name, "native;`label");
    assert_eq!(
        formatted.results[1].rows,
        [vec![
            Some(Value::Integer(77)),
            Some(Value::String("semicolon; quoted \"string\"".into()))
        ]]
    );
    let blank = driver
        .query_multi(&h, "-- no SQL\n /* only commentary */;", None)
        .await
        .unwrap();
    assert!(blank.results.is_empty());
    let oracle: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(oracle, 3, "source data intact");
    driver.disconnect(h).await.unwrap();
}
