//! Exercise the real transfer preflight against a driver without bound DML.
use datazen_data_transfer::{
    execute_transfer_data, TableInspectResult, TransferJob, ValueFormatter,
};
use datazen_driver_api::*;
use datazen_driver_turso::TursoDriver;
use serde_json::json;
use std::collections::HashMap;

#[tokio::test]
async fn transfer_refuses_before_resolving_any_session_or_writing() {
    let driver = TursoDriver::new();
    assert_eq!(driver.max_bound_parameters(), 0);
    // No sessions exist. Any attempted I/O would report a missing pool instead
    // of the parameter-limit rejection asserted below.
    let source = ConnectionHandle {
        id: "missing-source".into(),
        pool_id: "missing-source".into(),
    };
    let target = ConnectionHandle {
        id: "missing-target".into(),
        pool_id: "missing-target".into(),
    };
    let job: TransferJob = serde_json::from_value(json!({
        "source": { "dbSessionId": "source", "database": "main", "schema": null },
        "target": { "dbSessionId": "target", "database": "main", "schema": null },
        "mode": "data", "writeMode": "insert", "tables": [],
        "options": { "batchSize": 500, "stopOnError": true }
    }))
    .unwrap();
    let table: TableInspectResult = serde_json::from_value(json!({
        "sourceTable": "items", "targetTable": "items", "status": "MATCHED",
        "createNew": false, "enabled": true,
        "columnMappings": [{ "sourceColumn": "id", "targetColumn": "id" }],
        "incompatibleReason": null, "sourceRowCount": 1
    }))
    .unwrap();
    let result = execute_transfer_data(
        &driver,
        &source,
        &driver,
        &target,
        &job,
        &[table],
        &HashMap::new(),
        &ValueFormatter::SameFamily,
        None,
        false,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(result.rows_inserted, 0);
    assert!(result.partial);
    let error = result.tables[0].error.as_deref().unwrap();
    assert!(error.contains("0-parameter statement limit"), "{error}");
    assert!(!error.contains("pool"));
}
