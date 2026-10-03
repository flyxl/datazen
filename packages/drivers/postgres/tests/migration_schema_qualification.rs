//! Regression probe for schema-qualified migration metadata.

use datazen_driver_api::*;
use datazen_driver_postgres::{PgSyncAdapter, PostgresDriver};
#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;

#[tokio::test]
async fn test_transfer_qualified_metadata_isolates_selected_schema() {
    // `None` means the gate already reported this dimension unverified — or, under
    // `DATAZEN_CONTRACT_REQUIRE_LIVE=1`, already failed it.
    let Some(mut config) = migration_gate::require_config(
        "postgresql",
        "transfer-qualified-metadata",
        "postgresql",
        "migration-qualification",
        &[],
    ) else {
        return;
    };
    // Kept as a local so the assertions below stay byte-identical to the version
    // that used to run only under `--ignored`.
    let database = config
        .database
        .clone()
        .expect("the gate always sets the database it just validated");

    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let selected_schema = format!("DtSelected_{suffix}");
    // The suite pins metadata to one schema; the gate leaves `schema` unset.
    config.schema = Some(selected_schema.clone());
    let other_schema = format!("dt_other_{suffix}");
    // Deliberately dot-free. `resolve_pg_table_schema` splits a table reference
    // on the first `.` and treats what precedes it as the schema, so a name that
    // genuinely contains a dot is not addressable through `get_table_schema` —
    // the suffix used to end in `.part`, which made this test unrunnable. The
    // point under test is that ONE name exists in TWO schemas and the selected
    // one wins; a unique plain name carries that just as well.
    let table = format!("same_name_{suffix}");
    let driver = PostgresDriver::new();
    let handle = match driver.connect(&config).await {
        Ok(handle) => handle,
        Err(e) => {
            migration_gate::unverified(
                "postgresql",
                "transfer-qualified-metadata",
                &format!("the configured migration database did not accept a connection: {e}"),
            );
            return;
        }
    };

    let selected_sql = driver.quote_ident(&selected_schema);
    let other_sql = driver.quote_ident(&other_schema);
    let table_sql = driver.quote_ident(&table);
    for sql in [
        format!("CREATE SCHEMA {selected_sql}"),
        format!("CREATE SCHEMA {other_sql}"),
        format!("CREATE TABLE {selected_sql}.{table_sql} (selected_id integer)"),
        format!("CREATE TABLE {other_sql}.{table_sql} (other_payload text)"),
    ] {
        driver.execute(&handle, &sql).await.unwrap();
    }

    let schema = driver
        .get_table_schema(&handle, &table, &database, Some(&selected_schema))
        .await
        .unwrap();
    let full_sql = PgSyncAdapter
        .full_column_types_query(&format!("{selected_schema}.{table}"))
        .unwrap();
    let full_types = driver.query(&handle, &full_sql).await.unwrap();
    assert_eq!(full_types.rows.len(), 1);
    assert!(matches!(&full_types.rows[0][0], Some(Value::String(name)) if name == "selected_id"));
    driver
        .execute(&handle, &format!("DROP SCHEMA {selected_sql} CASCADE"))
        .await
        .unwrap();
    driver
        .execute(&handle, &format!("DROP SCHEMA {other_sql} CASCADE"))
        .await
        .unwrap();
    driver.disconnect(handle).await.unwrap();

    let names = schema
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["selected_id"]);
}
