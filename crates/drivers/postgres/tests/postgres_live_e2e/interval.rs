use super::{Fixture, string};
use datazen_driver_api::{DatabaseDriver, QueryStreamEvent};
use datazen_driver_postgres::PostgresDriver;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn binary_postgres_intervals_preserve_fractional_seconds_and_mixed_signs() {
    let mut fixture = Fixture::open().await;
    // IntervalStyle is a formatting option, not part of the binary wire value.
    sqlx::query("SET IntervalStyle TO postgres")
        .execute(&mut fixture.oracle)
        .await
        .expect("independent PostgreSQL oracle uses canonical interval style");
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("driver connection");
    let values = "
        FROM (VALUES
            (1, make_interval(years => 1, months => 1, days => 1, secs => 6.123456)),
            (2, make_interval(days => -1, secs => 0.25)),
            (3, make_interval(years => -1, months => -1, days => 1, secs => -0.25)),
            (4, make_interval(months => 1, days => -1, secs => 0.25)),
            (5, make_interval(secs => -3661.125)),
            (6, make_interval(hours => 42)),
            (7, make_interval()),
            (8, make_interval(years => -1, days => 2, secs => 0.000001))
        ) AS durations(sort_order, duration)
        ORDER BY sort_order";
    let expected = sqlx::query_scalar::<_, String>(&format!("SELECT duration::text {values}"))
        .fetch_all(&mut fixture.oracle)
        .await
        .expect("independent PostgreSQL text representations")
        .into_iter()
        .map(|value| vec![string(value)])
        .collect::<Vec<_>>();
    let query = format!("SELECT duration {values}");
    let queried = driver
        .query(&handle, &query)
        .await
        .expect("binary interval query");
    assert_eq!(
        queried.rows, expected,
        "binary values match PostgreSQL text"
    );
    let rows = Arc::new(Mutex::new(Vec::new()));
    driver
        .query_stream(&handle, &query, None, {
            let rows = Arc::clone(&rows);
            Arc::new(move |event| {
                if let QueryStreamEvent::Rows { rows: batch, .. } = event {
                    rows.lock().expect("row lock").extend(batch);
                }
            })
        })
        .await
        .expect("streamed binary interval query");
    assert_eq!(*rows.lock().expect("row lock"), expected);
    driver.disconnect(handle).await.expect("driver disconnect");
}
