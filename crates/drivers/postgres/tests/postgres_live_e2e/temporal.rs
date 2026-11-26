use super::{string, Fixture};
use datazen_driver_api::{DatabaseDriver, Value};
use datazen_driver_postgres::PostgresDriver;

#[tokio::test]
async fn postgres_wire_times_keep_offset_seconds_fraction_and_end_of_day() {
    let mut fixture = Fixture::open().await;
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("real PG wire connection");
    let expressions = [
        "'12:34:56.1234+05:30:15'::timetz",
        "'24:00:00-03'::timetz",
        "'00:00:00+00'::timetz",
        "'08:09:10-04:30'::timetz",
        "'24:00:00'::time",
        "'09:01:00.1001'::time",
        "NULL::timetz",
        "NULL::time",
    ];
    let sql = format!("SELECT {}", expressions.join(","));
    let text_sql = format!(
        "SELECT ARRAY[{}]",
        expressions
            .iter()
            .map(|expr| format!("({expr})::text"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let expected: Vec<Option<String>> = sqlx::query_scalar(&text_sql)
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("independent server text representations");
    assert_eq!(
        expected,
        [
            Some("12:34:56.1234+05:30:15".into()),
            Some("24:00:00-03".into()),
            Some("00:00:00+00".into()),
            Some("08:09:10-04:30".into()),
            Some("24:00:00".into()),
            Some("09:01:00.1001".into()),
            None,
            None
        ]
    );
    let expected: Vec<Option<Value>> = expected
        .into_iter()
        .map(|item| item.and_then(string))
        .collect();
    assert_eq!(
        driver
            .query(&handle, &sql)
            .await
            .expect("actual binary query")
            .rows,
        vec![expected]
    );
    driver.disconnect(handle).await.expect("driver cleanup");
}
