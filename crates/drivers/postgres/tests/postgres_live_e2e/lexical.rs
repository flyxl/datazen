use super::Fixture;
use datazen_driver_api::{DatabaseDriver, Value};
use datazen_driver_postgres::PostgresDriver;

#[tokio::test]
async fn postgres_result_cap_handles_unicode_aliases_ordinary_identifiers_and_line_comments() {
    let mut fixture = Fixture::open().await;
    let sequence = format!("public.dz_cap_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SEQUENCE {sequence} START 1"))
        .execute(&mut fixture.oracle)
        .await
        .expect("independent server-side work counter");
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("actual PostgreSQL row cap");
    // The SELECT is read through the same capped query path as editor/grid.
    // A sequence in a disposable fixture provides a server-side execution
    // count: truncating the fetched rows after execution is not sufficient.
    for (alias, trailing) in [
        ("limi雪", ""),
        ("ledger_limit_count", ""),
        ("limited_value", " -- result projection comment"),
    ] {
        sqlx::query(&format!("ALTER SEQUENCE {sequence} RESTART WITH 1"))
            .execute(&mut fixture.oracle)
            .await
            .expect("reset only independent fixture sequence");
        let sql =
            format!("SELECT nextval('{sequence}') AS {alias} FROM generate_series(1,8){trailing}");
        let observed = driver
            .query_multi(&handle, &sql, Some(2))
            .await
            .expect("normal PostgreSQL query and row cap");
        let rows = &observed.results[0];
        assert_eq!(rows.columns[0].name, alias);
        assert!(rows.truncated);
        assert_eq!(
            rows.rows,
            [vec![Some(Value::Integer(1))], vec![Some(Value::Integer(2))]]
        );
        let physical: i64 = sqlx::query_scalar(&format!("SELECT last_value FROM {sequence}"))
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("independent executed row count");
        assert_eq!(
            physical, 3,
            "{alias}: real SQL LIMIT includes only one extra row"
        );
    }
    let maximum = driver
        .query_multi(&handle, "SELECT 1 AS pg_live_limit", Some(u32::MAX))
        .await
        .expect("largest supported host cap has no arithmetic overflow");
    assert_eq!(maximum.results[0].rows, [vec![Some(Value::Integer(1))]]);
    assert!(!maximum.results[0].truncated);
    driver.disconnect(handle).await.expect("driver cleanup");
    sqlx::query(&format!("DROP SEQUENCE {sequence}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("cleanup execution counter");
}
