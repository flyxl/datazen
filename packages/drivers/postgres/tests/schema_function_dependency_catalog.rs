//! Live regression for safe PostgreSQL trigger-function dependency proof and
//! exact user-defined table column types.
//!
//! Run only against an isolated migration database with explicit credentials:
//! MIGRATION_TEST_DATABASE=<database> cargo test -p datazen-driver-postgres --test schema_function_dependency_catalog
//!
//! This suite is no longer `#[ignore]`d. With `MIGRATION_TEST_DATABASE` unset it reports
//! `function-dependency-catalog` unverified and skips; with
//! `DATAZEN_CONTRACT_REQUIRE_LIVE=1` that report is a failure.

use datazen_driver_api::{DatabaseDriver, Value};
use datazen_driver_postgres::PostgresDriver;
use serde_json::json;
#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;

#[tokio::test]
async fn function_catalog_proves_only_exact_passthrough_and_table_snapshot_keeps_enum_identity() {
    // `None` means the gate already reported this dimension unverified — or, under
    // `DATAZEN_CONTRACT_REQUIRE_LIVE=1`, already failed it.
    let Some(config) = migration_gate::require_config(
        "postgresql",
        "function-dependency-catalog",
        "postgresql",
        "migration-fn-dep",
        &["datazen_sync_src", "datazen_e2e"],
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
    let enum_type = format!("dz_bug003_enum_{suffix}");
    let table = format!("dz_bug003_table_{suffix}");
    let safe_function = format!("dz_bug003_safe_fn_{suffix}");
    let hidden_function = format!("dz_bug003_hidden_fn_{suffix}");
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&config)
        .await
        .expect("connect to isolated PostgreSQL database");

    let lookup = async {
        for sql in [
            format!("CREATE TYPE public.{enum_type} AS ENUM ('active')"),
            format!("CREATE TABLE public.{table} (status public.{enum_type})"),
            format!(
                "CREATE FUNCTION public.{safe_function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$"
            ),
            format!(
                "CREATE FUNCTION public.{hidden_function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM status FROM public.{table}; RETURN NEW; END $$"
            ),
        ] {
            driver
                .execute(&handle, &sql)
                .await
                .map_err(|error| error.to_string())?;
        }

        let safe = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"function","schema":"public","name":safe_function,"signature":""}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let hidden = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"function","schema":"public","name":hidden_function,"signature":""}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let table_schema = driver
            .get_table_schema(&handle, &table, &database, Some("public"))
            .await
            .map_err(|error| error.to_string())?;
        let column_type = table_schema
            .columns
            .iter()
            .find(|column| column.name == "status")
            .map(|column| column.data_type.clone())
            .ok_or_else(|| "typed table status column was not returned".to_owned())?;
        Ok::<_, String>((safe, hidden, column_type))
    }
    .await;

    for sql in [
        format!("DROP FUNCTION IF EXISTS public.{hidden_function}()"),
        format!("DROP FUNCTION IF EXISTS public.{safe_function}()"),
        format!("DROP TABLE IF EXISTS public.{table} CASCADE"),
        format!("DROP TYPE IF EXISTS public.{enum_type}"),
    ] {
        driver
            .execute(&handle, &sql)
            .await
            .unwrap_or_else(|error| panic!("fixture cleanup failed for owned object: {error}"));
    }
    let cleanup_check = format!(
        "SELECT NOT EXISTS (SELECT 1 FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace WHERE n.nspname = 'public' AND t.typname = '{enum_type}') \
         AND NOT EXISTS (SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'public' AND c.relname = '{table}') \
         AND NOT EXISTS (SELECT 1 FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace WHERE n.nspname = 'public' AND p.proname IN ('{safe_function}', '{hidden_function}')) AS fixture_objects_removed"
    );
    let cleanup_result = driver
        .query(&handle, &cleanup_check)
        .await
        .expect("query exact fixture cleanup state");
    assert!(
        matches!(
            cleanup_result.rows.first().and_then(|row| row.first()),
            Some(Some(Value::Bool(true)))
        ),
        "UUID-scoped fixture objects remain after cleanup: {:?}",
        cleanup_result.rows
    );
    driver.disconnect(handle).await.expect("disconnect");

    let (safe, hidden, column_type) =
        lookup.unwrap_or_else(|error| panic!("catalog lookup: {error}"));
    assert_eq!(safe["complete"], true, "exact passthrough proof: {safe}");
    assert_eq!(safe["dependencies"], json!([]));
    assert_eq!(
        hidden["complete"], false,
        "a body with an untracked table reference must remain incomplete: {hidden}"
    );
    assert_eq!(column_type, format!("public.{enum_type}"));
}
