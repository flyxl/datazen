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
    let boundary = driver
        .query_with_params(&handle, &at_limit, &params)
        .await
        .unwrap();
    assert!(matches!(boundary.rows[0][0], Some(Value::Integer(7))));
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
    for (cell, expected) in rows[0]
        .iter()
        .zip([total as i64, total as i64, 0, total as i64 - 1])
    {
        assert!(matches!(cell, Some(Value::Integer(actual)) if *actual == expected));
    }
    let last = driver
        .query(
            &handle,
            "SELECT payload FROM batches ORDER BY id DESC LIMIT 1",
        )
        .await
        .unwrap()
        .rows;
    assert!(
        matches!(&last[0][0], Some(Value::String(actual)) if actual == &format!("row-{}", total - 1))
    );
    driver.disconnect(handle).await.unwrap();
}

#[tokio::test]
async fn actual_transfer_splits_a_wide_500_row_page_at_the_linked_runtime_limit() {
    use datazen_data_transfer::{
        execute_transfer_data, TableInspectResult, TransferJob, ValueFormatter,
    };
    use serde_json::json;
    use std::collections::HashMap;
    let source = SqliteDriver::new();
    let target = SqliteDriver::new();
    let source_handle = source.connect(&config(":memory:")).await.unwrap();
    let target_handle = target.connect(&config(":memory:")).await.unwrap();
    let columns: Vec<String> = (0..150).map(|n| format!("c{n}")).collect();
    let definitions: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(n, column)| {
            format!(
                "{column} INTEGER{}",
                if n == 0 { " PRIMARY KEY" } else { "" }
            )
        })
        .collect();
    let ddl = format!("CREATE TABLE wide ({})", definitions.join(","));
    source.execute(&source_handle, &ddl).await.unwrap();
    target.execute(&target_handle, &ddl).await.unwrap();
    let values: Vec<String> = (0..150).map(|n| format!("id+{n}")).collect();
    source.execute(&source_handle, &format!("WITH RECURSIVE n(id) AS (SELECT 0 UNION ALL SELECT id+1 FROM n WHERE id<499) INSERT INTO wide SELECT {} FROM n", values.join(","))).await.unwrap();
    let schema = source
        .get_table_schema(&source_handle, "wide", "main", None)
        .await
        .unwrap();
    let schema_map = HashMap::from([("wide".into(), schema)]);
    let job: TransferJob = serde_json::from_value(json!({
        "source": { "dbSessionId": "source", "database": "main", "schema": null },
        "target": { "dbSessionId": "target", "database": "main", "schema": null },
        "mode": "data", "writeMode": "insert", "tables": [],
        "options": { "batchSize": 500, "stopOnError": true }
    }))
    .unwrap();
    let mappings: Vec<serde_json::Value> = columns
        .iter()
        .map(|column| json!({"sourceColumn": column, "targetColumn": column}))
        .collect();
    let table: TableInspectResult = serde_json::from_value(json!({
        "sourceTable": "wide", "targetTable": "wide", "status": "MATCHED",
        "createNew": false, "enabled": true, "columnMappings": mappings,
        "sourceColumns": columns, "targetColumns": columns, "sourcePrimaryKeys": ["c0"],
        "incompatibleReason": null, "sourceRowCount": 500
    }))
    .unwrap();
    let result = execute_transfer_data(
        &source,
        &source_handle,
        &target,
        &target_handle,
        &job,
        &[table],
        &schema_map,
        &ValueFormatter::SameFamily,
        None,
        false,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(result.rows_inserted, 500, "{:?}", result.tables);
    assert!(!result.partial);
    let result_rows = target
        .query(
            &target_handle,
            "SELECT COUNT(*), SUM(c0), SUM(c149) FROM wide",
        )
        .await
        .unwrap()
        .rows;
    for (cell, expected) in result_rows[0].iter().zip([500, 124750, 199250]) {
        assert!(matches!(cell, Some(Value::Integer(actual)) if *actual == expected));
    }
    source.disconnect(source_handle).await.unwrap();
    target.disconnect(target_handle).await.unwrap();
}
