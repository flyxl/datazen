//! Live Data Transfer sequence regression. The fixture is isolated in one
//! uniquely named schema and is removed by the test after each run.

use datazen_driver_api::{ConnectionHandle, DatabaseDriver, Value};
use datazen_driver_postgres::PostgresDriver;
#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;

fn quote_ident(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

async fn import_ids_and_sync(
    driver: &PostgresDriver,
    handle: &ConnectionHandle,
    schema: &str,
    table: &str,
    column: &str,
    via_sql_file_statement: bool,
) -> Result<(), String> {
    let relation = format!("{}.{}", quote_ident(schema), quote_ident(table));
    let quoted_column = quote_ident(column);
    let transaction = driver
        .begin_transaction(handle)
        .await
        .map_err(|error| error.to_string())?;
    let work = async {
        let marker = "/*DATAZEN_TEST_IDENTITY_OVERRIDE*/";
        let insert = format!("INSERT INTO {relation} ({quoted_column}) {marker} VALUES (31), (41)");
        let insert = if via_sql_file_statement {
            driver
                .render_transfer_sql_file_insert(&insert, marker)
                .map_err(|error| error.to_string())?
        } else if let Some(clause) = driver.transfer_explicit_identity_insert_clause() {
            insert.replacen(marker, clause, 1)
        } else {
            insert.replacen(marker, "", 1)
        };
        driver
            .execute(handle, &insert)
            .await
            .map_err(|error| error.to_string())?;
        if via_sql_file_statement {
            let statements = driver
                .render_transfer_identity_sequence_sync_sql(
                    Some(schema),
                    table,
                    &[column.to_string()],
                )
                .map_err(|error| error.to_string())?;
            if statements.len() != 1 {
                return Err(format!(
                    "expected one SQL-file sequence statement, got {}",
                    statements.len()
                ));
            }
            driver
                .execute(handle, &statements[0])
                .await
                .map_err(|error| error.to_string())?;
        } else {
            driver
                .advance_transfer_identity_sequences(
                    handle,
                    Some(schema),
                    table,
                    &[column.to_string()],
                )
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok::<(), String>(())
    }
    .await;
    if let Err(error) = work {
        let rollback_error = driver.rollback(transaction).await.err();
        return Err(match rollback_error {
            Some(rollback) => format!("{error}; rollback failed: {rollback}"),
            None => error,
        });
    }
    driver
        .commit(transaction)
        .await
        .map_err(|error| error.to_string())
}

async fn insert_default_and_read(
    driver: &PostgresDriver,
    handle: &ConnectionHandle,
    schema: &str,
    table: &str,
    column: &str,
) -> Result<i64, String> {
    let relation = format!("{}.{}", quote_ident(schema), quote_ident(table));
    let result = driver
        .query(
            handle,
            &format!(
                "INSERT INTO {relation} DEFAULT VALUES RETURNING {}",
                quote_ident(column)
            ),
        )
        .await
        .map_err(|error| error.to_string())?;
    match result
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(Option::as_ref)
    {
        Some(Value::Integer(value)) => Ok(*value),
        other => Err(format!("unexpected generated value: {other:?}")),
    }
}

#[tokio::test]
async fn explicit_id_import_advances_identity_and_owned_serial_sequences_safely() {
    // `None` means the gate already reported this dimension unverified — or, under
    // `DATAZEN_CONTRACT_REQUIRE_LIVE=1`, already failed it.
    let Some(config) = migration_gate::require_config(
        "postgresql",
        "identity-sequence-import",
        "postgresql",
        "migration-identity",
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

    let schema = format!("dz_transfer_seq_{}", uuid::Uuid::new_v4().simple());
    let high_table = "identity.high\"water";
    let fresh_table = "identity_fresh";
    let always_table = "identity_always";
    let serial_table = "serial.table\"quoted";
    let file_table = "sql_file_identity";
    let ordinary_table = "ordinary_override_target";
    let cached_table = "identity_cache_unsupported";
    let cycle_table = "identity_cycle_unsupported";
    let id_column = "id.\"quoted";
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&config)
        .await
        .expect("connect to isolated PostgreSQL database");
    let q_schema = quote_ident(&schema);
    let high_relation = format!("{q_schema}.{}", quote_ident(high_table));
    let setup = async {
        driver
            .execute(&handle, &format!("CREATE SCHEMA {q_schema}"))
            .await
            .map_err(|error| error.to_string())?;
        for table in [
            high_table,
            fresh_table,
            always_table,
            serial_table,
            file_table,
            ordinary_table,
            cached_table,
            cycle_table,
        ] {
            let q_table = format!("{q_schema}.{}", quote_ident(table));
            let column_ddl = if table == serial_table {
                format!("{} BIGSERIAL PRIMARY KEY", quote_ident(id_column))
            } else if table == ordinary_table {
                format!("{} BIGINT PRIMARY KEY", quote_ident(id_column))
            } else if table == always_table {
                format!(
                    "{} BIGINT GENERATED ALWAYS AS IDENTITY (CACHE 1) PRIMARY KEY",
                    quote_ident(id_column)
                )
            } else if table == cached_table {
                format!(
                    "{} BIGINT GENERATED BY DEFAULT AS IDENTITY (CACHE 2) PRIMARY KEY",
                    quote_ident(id_column)
                )
            } else if table == cycle_table {
                format!(
                    "{} BIGINT GENERATED BY DEFAULT AS IDENTITY (CACHE 1 CYCLE) PRIMARY KEY",
                    quote_ident(id_column)
                )
            } else {
                format!(
                    "{} BIGINT GENERATED BY DEFAULT AS IDENTITY (CACHE 1) PRIMARY KEY",
                    quote_ident(id_column)
                )
            };
            driver
                .execute(&handle, &format!("CREATE TABLE {q_table} ({column_ddl})"))
                .await
                .map_err(|error| error.to_string())?;
        }
        let sequence_name = format!(
            "pg_catalog.pg_get_serial_sequence({}, {})",
            quote_literal(&high_relation),
            quote_literal(id_column)
        );
        driver
            .execute(
                &handle,
                &format!("SELECT pg_catalog.setval({sequence_name}, 80, true)"),
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok::<(), String>(())
    }
    .await;

    let result = if let Err(error) = setup {
        Err(error)
    } else {
        async {
            import_ids_and_sync(&driver, &handle, &schema, high_table, id_column, false).await?;
            import_ids_and_sync(&driver, &handle, &schema, fresh_table, id_column, false).await?;
            import_ids_and_sync(&driver, &handle, &schema, always_table, id_column, false).await?;
            import_ids_and_sync(&driver, &handle, &schema, serial_table, id_column, false).await?;
            import_ids_and_sync(&driver, &handle, &schema, file_table, id_column, true).await?;

            let ordinary_relation = format!("{q_schema}.{}", quote_ident(ordinary_table));
            let marker = "/*DATAZEN_TEST_IDENTITY_OVERRIDE*/";
            let ordinary_insert = driver
                .render_transfer_sql_file_insert(
                    &format!(
                        "INSERT INTO {ordinary_relation} ({}) {marker} VALUES (31), (41)",
                        quote_ident(id_column),
                    ),
                    marker,
                )
                .map_err(|error| error.to_string())?;
            driver
                .execute(&handle, &ordinary_insert)
                .await
                .map_err(|error| error.to_string())?;
            let ordinary_count = driver
                .query(
                    &handle,
                    &format!("SELECT COUNT(*) FROM {ordinary_relation}"),
                )
                .await
                .map_err(|error| error.to_string())?;
            if !matches!(
                ordinary_count
                    .rows
                    .first()
                    .and_then(|row| row.first())
                    .and_then(Option::as_ref),
                Some(Value::Integer(2))
            ) {
                return Err(
                    "OVERRIDING SYSTEM VALUE did not preserve ordinary-column inserts".into(),
                );
            }

            let cache_error =
                import_ids_and_sync(&driver, &handle, &schema, cached_table, id_column, false)
                    .await
                    .expect_err("CACHE 2 must fail closed");
            if !cache_error.contains("CACHE 2") {
                return Err(format!("unexpected cache error: {cache_error}"));
            }
            let cycle_error =
                import_ids_and_sync(&driver, &handle, &schema, cycle_table, id_column, false)
                    .await
                    .expect_err("CYCLE must fail closed");
            if !cycle_error.contains("CYCLE") {
                return Err(format!("unexpected cycle error: {cycle_error}"));
            }
            for table in [cached_table, cycle_table] {
                let result = driver
                    .query(
                        &handle,
                        &format!("SELECT COUNT(*) FROM {q_schema}.{}", quote_ident(table)),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                if !matches!(
                    result
                        .rows
                        .first()
                        .and_then(|row| row.first())
                        .and_then(Option::as_ref),
                    Some(Value::Integer(0))
                ) {
                    return Err(format!("failed sequence preflight left rows in {table}"));
                }
            }

            Ok::<Vec<i64>, String>(vec![
                insert_default_and_read(&driver, &handle, &schema, high_table, id_column).await?,
                insert_default_and_read(&driver, &handle, &schema, fresh_table, id_column).await?,
                insert_default_and_read(&driver, &handle, &schema, always_table, id_column).await?,
                insert_default_and_read(&driver, &handle, &schema, serial_table, id_column).await?,
                insert_default_and_read(&driver, &handle, &schema, file_table, id_column).await?,
            ])
        }
        .await
    };

    let cleanup = driver
        .execute(
            &handle,
            &format!("DROP SCHEMA IF EXISTS {q_schema} CASCADE"),
        )
        .await;
    let disconnect = driver.disconnect(handle).await;
    cleanup.expect("remove only this test's unique schema");
    disconnect.expect("disconnect test session");
    let generated = result.expect("identity/serial imports and synchronization should succeed");
    assert!(
        generated[0] > 80,
        "higher sequence high-water regressed: {generated:?}"
    );
    assert!(
        generated[1] > 41,
        "BY DEFAULT identity was not advanced: {generated:?}"
    );
    assert!(
        generated[2] > 41,
        "ALWAYS identity explicit-value import was not accepted/reseeded: {generated:?}"
    );
    assert!(
        generated[3] > 41,
        "owned serial sequence was not advanced: {generated:?}"
    );
    assert!(
        generated[4] > 41,
        "SQL-file sync block did not advance identity: {generated:?}"
    );
}
