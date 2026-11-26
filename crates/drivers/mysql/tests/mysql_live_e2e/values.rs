use super::Fixture;
use datazen_driver_api::{DatabaseDriver, QueryStreamEvent, Value};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn mysql_wire_materialized_and_streaming_preserve_exact_native_values() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let table = format!("dz_values_{}", uuid::Uuid::new_v4().simple());
    let setup = format!(
        r#"CREATE TABLE {table} (
        id bigint PRIMARY KEY, u bigint unsigned, amount decimal(55,15), fixed binary(3),
        bytes varbinary(60), payload json, label varchar(80), day date,
        mark timestamp(6) NULL, clock time(6), flag bit(8), real_value double
    );
    INSERT INTO {table} VALUES
      (1,18446744073709551615,123456789012345678901234567890.001234567890123,X'00FF27',X'8027FF',JSON_OBJECT('é',3),'雪','2024-02-29','2024-02-29 01:02:03.456789','-27:12:13.123456',B'11001001',1.25),
      (2,NULL,NULL,NULL,NULL,NULL,'',NULL,NULL,NULL,NULL,NULL)"#
    );
    sqlx::raw_sql(&setup)
        .execute(&mut f.oracle)
        .await
        .expect("independent native value fixture");
    let expected: (String, String, Vec<u8>, String) = sqlx::query_as(&format!(
        "SELECT CAST(u AS CHAR),CAST(amount AS CHAR),bytes,label FROM {table} WHERE id=1"
    ))
    .fetch_one(&mut f.oracle)
    .await
    .expect("native exact high precision oracle");
    let sql = format!("SELECT * FROM {table} ORDER BY id");
    let actual = driver
        .query(&handle, &sql)
        .await
        .expect("wire materialized");
    assert_eq!(actual.rows.len(), 2);
    assert_eq!(actual.rows[0][1], Some(Value::String(expected.0)));
    assert_eq!(actual.rows[0][2], Some(Value::String(expected.1)));
    assert_eq!(actual.rows[0][4], Some(Value::Bytes(expected.2)));
    assert_eq!(actual.rows[0][6], Some(Value::String(expected.3)));
    let independent: (String, u64) = sqlx::query_as(&format!(
        "SELECT CAST(clock AS CHAR), CAST(flag AS UNSIGNED) FROM {table} WHERE id=1"
    ))
    .fetch_one(&mut f.oracle)
    .await
    .expect("native signed TIME and multi-bit oracle");
    assert_eq!(
        actual.rows[0][9],
        Some(Value::String(independent.0)),
        "signed MySQL TIME is not a clock-only value"
    );
    assert_eq!(
        actual.rows[0][10],
        Some(Value::Integer(independent.1 as i64)),
        "BIT(8) is more than a boolean"
    );
    assert_eq!(actual.rows[1][6], Some(Value::String(String::new())));
    assert!(actual.rows[1]
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 0 && *i != 6)
        .all(|(_, v)| v.is_none()));
    let events = Arc::new(Mutex::new(Vec::new()));
    driver
        .query_stream(&handle, &sql, None, {
            let events = events.clone();
            Arc::new(move |event| events.lock().unwrap().push(event))
        })
        .await
        .expect("wire streaming");
    let streamed = events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            QueryStreamEvent::Rows { rows, .. } => Some(rows.clone()),
            _ => None,
        })
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(streamed, actual.rows);
    let empty = driver
        .query_with_params(
            &handle,
            &format!("SELECT id,amount,bytes FROM {table} WHERE id > ?"),
            &[Value::Integer(999)],
        )
        .await
        .expect("typed prepared metadata when no rows match");
    assert!(empty.rows.is_empty());
    assert_eq!(
        empty
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "amount", "bytes"]
    );
    driver
        .disconnect(handle)
        .await
        .expect("close native connection");
}
