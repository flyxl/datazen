//! Real driver journey; run only against a disposable migration database.
use datazen_driver_api::*;
use datazen_driver_postgres::*;
#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;
#[tokio::test]
async fn bound_writes_preserve_bytes_decimal_and_transaction_counts() {
    // Reads `MIGRATION_TEST_*` from the process environment and refuses any
    // database that is not a disposable fixture. `None` means it already
    // reported this dimension unverified — or, under
    // `DATAZEN_CONTRACT_REQUIRE_LIVE=1`, already failed it.
    let Some(config) = migration_gate::require_config(
        "postgresql",
        "bound-writes",
        "postgresql",
        "migration-bound",
        &[],
    ) else {
        return;
    };
    let driver = PostgresDriver::new();
    let handle = match driver.connect(&config).await {
        Ok(handle) => handle,
        Err(e) => {
            migration_gate::unverified(
                "postgresql",
                "bound-writes",
                &format!("the configured migration database did not accept a connection: {e}"),
            );
            return;
        }
    };
    let table = driver.quote_ident(&format!("bound_{}", uuid::Uuid::new_v4().simple()));
    driver.execute(&handle, &format!("CREATE TABLE {table} (id INTEGER PRIMARY KEY, payload BYTEA, amount NUMERIC(65,30), label TEXT)")).await.unwrap();
    let placeholders = ["INTEGER", "BYTEA", "NUMERIC(65,30)", "TEXT"]
        .iter()
        .enumerate()
        .map(|(i, ty)| driver.parameter_placeholder(i + 1, Some(ty)).unwrap())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("INSERT INTO {table} VALUES ({placeholders})");
    let exact = "12345678901234567890123456789012345.123456789012345678901234567890";
    let bytes = (0..=255).collect::<Vec<u8>>();
    let label = "quotes' backslash\\ newline\n Unicode雪";
    let params = vec![
        Value::Integer(1),
        Value::Bytes(bytes.clone()),
        Value::String(exact.into()),
        Value::String(label.into()),
    ];
    let tx = driver.begin_transaction(&handle).await.unwrap();
    assert_eq!(
        driver
            .execute_with_params(&handle, &sql, &params)
            .await
            .unwrap(),
        1
    );
    driver.rollback(tx).await.unwrap();
    assert!(driver
        .query(&handle, &format!("SELECT id FROM {table}"))
        .await
        .unwrap()
        .rows
        .is_empty());
    let tx = driver.begin_transaction(&handle).await.unwrap();
    assert_eq!(
        driver
            .execute_with_params(&handle, &sql, &params)
            .await
            .unwrap(),
        1
    );
    driver.commit(tx).await.unwrap();
    let row = driver
        .query(
            &handle,
            &format!("SELECT payload, amount, label FROM {table}"),
        )
        .await
        .unwrap()
        .rows
        .remove(0);
    assert!(
        matches!(&row[0], Some(Value::Bytes(value)) if value == &bytes),
        "binary value changed"
    );
    assert!(
        matches!(&row[1], Some(Value::String(value)) if value == exact),
        "decimal value changed: {:?}",
        row[1]
    );
    assert!(
        matches!(&row[2], Some(Value::String(value)) if value == label),
        "text value changed"
    );
    let ph = driver.parameter_placeholder(1, Some("INTEGER")).unwrap();
    assert_eq!(
        driver
            .execute_with_params(
                &handle,
                &format!("DELETE FROM {table} WHERE id = {ph}"),
                &[Value::Integer(2)]
            )
            .await
            .unwrap(),
        0
    );
    driver
        .execute(&handle, &format!("DROP TABLE {table}"))
        .await
        .unwrap();
    driver.disconnect(handle).await.unwrap();
}
