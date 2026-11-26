use super::Fixture;
use datazen_driver_api::{DatabaseDriver, QueryStreamEvent, Value};
use datazen_driver_postgres::PostgresDriver;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn ordinary_pg_table_values_and_returning_keep_real_rows_in_both_query_paths() {
    let mut fixture = Fixture::open().await;
    let table = format!("public.dz_statement_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE TABLE {table} (id int PRIMARY KEY)"))
        .execute(&mut fixture.oracle)
        .await
        .expect("real PostgreSQL fixture");
    sqlx::query(&format!("INSERT INTO {table} VALUES (1)"))
        .execute(&mut fixture.oracle)
        .await
        .expect("real row");
    let driver = PostgresDriver::new();
    let handle = driver.connect(&fixture.config).await.expect("live PG");
    let statements=format!(
        "-- exported report\n SELECT 21::int AS marker; /* table preview */ TABLE {table}; VALUES (31),(32),(33); INSERT INTO {table} VALUES (2) RETURNING id;"
    );
    let output = driver
        .query_multi(&handle, &statements, Some(2))
        .await
        .expect("server-executed statement results");
    assert_eq!(output.results.len(), 4);
    assert_eq!(output.results[0].rows, vec![vec![Some(Value::Integer(21))]]);
    assert_eq!(output.results[1].rows, vec![vec![Some(Value::Integer(1))]]);
    assert_eq!(
        output.results[2].rows,
        vec![
            vec![Some(Value::Integer(31))],
            vec![Some(Value::Integer(32))]
        ]
    );
    assert!(output.results[2].truncated);
    assert_eq!(output.results[3].rows, vec![vec![Some(Value::Integer(2))]]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let record = events.clone();
    driver
        .query_stream(
            &handle,
            &format!("UPDATE {table} SET id=7 WHERE id=2 RETURNING id AS updated"),
            None,
            Arc::new(move |event| record.lock().unwrap().push(event)),
        )
        .await
        .expect("server-executed streaming UPDATE RETURNING");
    let events = events.lock().unwrap();
    assert!(events
        .iter()
        .any(|event| matches!(event,QueryStreamEvent::Rows {rows,..}
        if rows == &vec![vec![Some(Value::Integer(7))]])));
    assert!(matches!(events.last(), Some(QueryStreamEvent::Done { .. })));
    drop(events);
    let independently_stored =
        sqlx::query_scalar::<_, i32>(&format!("SELECT id FROM {table} ORDER BY id"))
            .fetch_all(&mut fixture.oracle)
            .await
            .unwrap();
    assert_eq!(
        independently_stored,
        [1, 7],
        "no duplicate execution or misplaced write"
    );
    driver.disconnect(handle).await.unwrap();
    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&mut fixture.oracle)
        .await
        .unwrap();
}
