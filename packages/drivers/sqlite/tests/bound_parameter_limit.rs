//! Real linked-runtime boundary and multi-batch write journey.
use datazen_driver_api::*;
use datazen_driver_sqlite::SqliteDriver;

fn config(path: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: format!("tester-transfer-{}", uuid::Uuid::new_v4()),
        name: "tester transfer".into(),
        database_type: "sqlite".into(),
        host: None,
        port: None,
        database: Some(path.into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 1,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

#[tokio::test]
async fn runtime_limit_rejects_oversized_statement_and_splits_large_writes() {
    let driver = SqliteDriver::new();
    let handle = driver.connect(&config(":memory:")).await.unwrap();
    let limit = driver.max_bound_parameters();
    assert!(limit > 0);
    driver
        .execute(
            &handle,
            "CREATE TABLE batches (id INTEGER PRIMARY KEY, payload TEXT)",
        )
        .await
        .unwrap();

    // The advertised boundary must be executable, and the next parameter must
    // fail in the actual engine (not merely in a helper that checks a constant).
    let at_limit = format!("SELECT ?{limit}");
    let params = vec![Value::Integer(7); limit];
    assert_eq!(
        driver
            .query_with_params(&handle, &at_limit, &params)
            .await
            .unwrap()
            .rows[0][0],
        Some(Value::Integer(7))
    );
    let over_limit = format!("SELECT ?{}", limit + 1);
    assert!(driver
        .query_with_params(&handle, &over_limit, &vec![Value::Integer(7); limit + 1])
        .await
        .is_err());

    // A total above the inherited 60000 ceiling exercises repeated full
    // batches plus a remainder, with two independently bound fields per row.
    let total = 31_001usize;
    let per_batch = limit / 2;
    assert!(
        per_batch > 0,
        "linked runtime cannot bind one two-column row"
    );
    let mut batches = 0;
    for first in (0..total).step_by(per_batch) {
        let count = per_batch.min(total - first);
        let sql = format!(
            "INSERT INTO batches VALUES {}",
            vec!["(?, ?)"; count].join(",")
        );
        let params: Vec<Value> = (first..first + count)
            .flat_map(|id| {
                [
                    Value::Integer(id as i64),
                    Value::String(format!("row-{id}")),
                ]
            })
            .collect();
        assert!(params.len() <= limit);
        let transaction = driver.begin_transaction(&handle).await.unwrap();
        assert_eq!(
            driver
                .execute_with_params(&handle, &sql, &params)
                .await
                .unwrap(),
            count as u64
        );
        driver.commit(transaction).await.unwrap();
        batches += 1;
    }
    assert_eq!(batches, total.div_ceil(per_batch));
    let rows = driver
        .query(
            &handle,
            "SELECT COUNT(*), COUNT(DISTINCT id), MIN(id), MAX(id) FROM batches",
        )
        .await
        .unwrap()
        .rows;
    assert_eq!(
        rows[0],
        vec![
            Some(Value::Integer(total as i64)),
            Some(Value::Integer(total as i64)),
            Some(Value::Integer(0)),
            Some(Value::Integer(total as i64 - 1))
        ]
    );
    let last = driver
        .query(
            &handle,
            "SELECT payload FROM batches ORDER BY id DESC LIMIT 1",
        )
        .await
        .unwrap()
        .rows;
    assert_eq!(
        last[0][0],
        Some(Value::String(format!("row-{}", total - 1)))
    );
    driver.disconnect(handle).await.unwrap();
}
