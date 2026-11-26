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
    assert!(
        actual.rows[1]
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 0 && *i != 6)
            .all(|(_, v)| v.is_none())
    );
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

#[tokio::test]
async fn mysql_decodes_native_unsigned_widths_and_wide_bits_without_rounding() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let table = format!("dz_unsigned_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        r#"CREATE TABLE {table} (
       id INT PRIMARY KEY, i8u TINYINT UNSIGNED, i16u SMALLINT UNSIGNED,
       i24u MEDIUMINT UNSIGNED, i32u INT UNSIGNED, i64u BIGINT UNSIGNED, i64s BIGINT,
       bool_bias BIT(1), wide BIT(64), yr YEAR, fraction FLOAT, blend DOUBLE,
       today DATE, when_present DATETIME(6), elapsed TIME(6));
       INSERT INTO {table} VALUES (1,255,65535,16777215,4294967295,42,-9223372036854775808,
            b'1', CAST(18446744073709551615 AS UNSIGNED),2024,1.125,-3.25,
            '2024-02-29','2024-02-29 01:02:03.654321','01:02:03.000002'),
            (2,0,0,0,0,0,-1,b'0',0,2000,0,0,'2000-01-01','2000-01-01 00:00:00','00:00:00')"#
    ))
    .execute(&mut f.oracle)
    .await
    .expect("actual MySQL extremes and ordinary zero");
    let check = driver
        .query(&handle, &format!("SELECT * FROM {table} ORDER BY id"))
        .await
        .expect("real typed wire result");
    let actual = &check.rows[0];
    assert_eq!(
        &actual[1..6],
        [
            Some(Value::Integer(255)),
            Some(Value::Integer(65535)),
            Some(Value::Integer(16777215)),
            Some(Value::Integer(4294967295)),
            Some(Value::Integer(42)),
        ]
    );
    let extremes: (String, String) = sqlx::query_as(&format!(
        "SELECT CAST(i64s AS CHAR),CAST(CAST(wide AS UNSIGNED) AS CHAR) FROM {table} WHERE id=1"
    ))
    .fetch_one(&mut f.oracle)
    .await
    .expect("independent exact large integer and wide bit oracle");
    assert_eq!(actual[6], Some(Value::String(extremes.0)));
    assert_eq!(actual[7], Some(Value::Integer(1)));
    assert_eq!(actual[8], Some(Value::String(extremes.1)));
    assert_eq!(actual[9], Some(Value::Integer(2024)));
    assert_eq!(actual[10], Some(Value::Float(1.125)));
    assert_eq!(actual[11], Some(Value::Float(-3.25)));
    assert_eq!(actual[12], Some(Value::String("2024-02-29".into())));
    assert_eq!(
        actual[13],
        Some(Value::String("2024-02-29 01:02:03.654321".into()))
    );
    assert_eq!(actual[14], Some(Value::String("01:02:03.000002".into())));
    assert_eq!(check.rows[1][7], Some(Value::Integer(0)));
    assert_eq!(check.rows[1][8], Some(Value::Integer(0)));
    assert_eq!(check.rows[1][14], Some(Value::String("00:00:00".into())));
    driver
        .disconnect(handle)
        .await
        .expect("typed fixture cleanup");
}

#[tokio::test]
async fn mysql_prepared_parameters_roundtrip_native_scalar_null_json_and_temporal_values() {
    let mut f = Fixture::open().await;
    let (driver, pooled) = f.connect().await;
    let fixed = driver.connect_fixed_session(&f.cfg).await.unwrap();
    let table = format!("dz_bound_wire_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!(
        "CREATE TABLE {table}(id INT PRIMARY KEY,enabled BOOLEAN, narrow TINYINT,
      small SMALLINT, medium MEDIUMINT, real_value FLOAT, wide_value DOUBLE, occurred DATETIME(6),
      nullable_note VARCHAR(40), doc JSON, raw_value VARBINARY(12))"
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    let payload = serde_json::json!({"flag":true,"label":"雪\\folder","empty":null});
    let occurred = "2030-02-28 04:11:12.123456";
    for (h, id, enabled) in [(&pooled, 1, true), (&fixed, 2, false)] {
        let affected = driver
            .execute_with_params(
                h,
                &format!("INSERT INTO {table} VALUES(?,?,?,?,?,?,?,?,?,?,?)"),
                &[
                    Value::Integer(id),
                    Value::Bool(enabled),
                    Value::Integer(-120),
                    Value::Integer(-31_000),
                    Value::Integer(-8_000_000),
                    Value::Float(-0.625),
                    Value::Float(0.3125),
                    Value::Timestamp(occurred.into()),
                    Value::Null,
                    Value::Json(payload.clone()),
                    Value::Bytes(vec![0, 255, 92, 39]),
                ],
            )
            .await
            .unwrap();
        assert_eq!(affected, 1);
        let result=driver.query_with_params(h,&format!("SELECT narrow,small,medium,real_value,wide_value,occurred,nullable_note,doc,raw_value FROM {table} WHERE id=? AND enabled=?"),
         &[Value::Integer(id),Value::Bool(enabled)]).await.unwrap();
        assert_eq!(
            result.rows,
            [vec![
                Some(Value::Integer(-120)),
                Some(Value::Integer(-31_000)),
                Some(Value::Integer(-8_000_000)),
                Some(Value::Float(-0.625)),
                Some(Value::Float(0.3125)),
                Some(Value::String(occurred.into())),
                None,
                Some(Value::Json(payload.clone())),
                Some(Value::Bytes(vec![0, 255, 92, 39]))
            ]]
        );
    }
    type Native = (
        i32,
        i8,
        Option<String>,
        sqlx::types::Json<serde_json::Value>,
        Vec<u8>,
        String,
    );
    let stored:Vec<Native>=sqlx::query_as(&format!("SELECT id,enabled,nullable_note,doc,raw_value,CAST(occurred AS CHAR) FROM {table} ORDER BY id"))
       .fetch_all(&mut f.oracle).await.unwrap();
    assert_eq!(
        stored,
        [
            (
                1,
                1,
                None,
                sqlx::types::Json(payload.clone()),
                vec![0, 255, 92, 39],
                occurred.into()
            ),
            (
                2,
                0,
                None,
                sqlx::types::Json(payload),
                vec![0, 255, 92, 39],
                occurred.into()
            )
        ]
    );
    driver.disconnect(fixed).await.unwrap();
    driver.disconnect(pooled).await.unwrap();
}
