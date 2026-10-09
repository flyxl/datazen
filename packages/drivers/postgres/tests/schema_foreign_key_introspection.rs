//! Driver-level regression for PostgreSQL FK metadata used by Schema Diff.
//!
//! Run against a disposable migration database, or the dedicated DataZen sync
//! test database using only unique `dz_mig_fk_*` fixture names:
//! `MIGRATION_TEST_DATABASE=<database> cargo test -p datazen-driver-postgres --test schema_foreign_key_introspection`
//!
//! This suite is no longer `#[ignore]`d. With `MIGRATION_TEST_DATABASE` unset it reports
//! `foreign-key-introspection` unverified and skips; with
//! `DATAZEN_CONTRACT_REQUIRE_LIVE=1` that report is a failure.

use datazen_driver_api::DatabaseDriver;
use datazen_driver_postgres::PostgresDriver;
#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;

#[tokio::test]
async fn selected_table_schema_preserves_foreign_key_metadata() {
    // `None` means the gate already reported this dimension unverified — or, under
    // `DATAZEN_CONTRACT_REQUIRE_LIVE=1`, already failed it.
    let Some(config) = migration_gate::require_config(
        "postgresql",
        "foreign-key-introspection",
        "postgresql",
        "migration-fk",
        &["datazen_sync_src"],
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
    let parent = format!("dz_mig_fk_parent_{suffix}");
    let source_child = format!("dz_mig_fk_source_child_{suffix}");
    let target_child = format!("dz_mig_fk_target_child_{suffix}");
    let constraint = format!("dz_mig_fk_constraint_{suffix}");
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&config)
        .await
        .expect("connect to isolated PostgreSQL database");

    let schemas = async {
        driver
            .execute(
                &handle,
                &format!("CREATE TABLE public.{parent} (id INTEGER PRIMARY KEY)"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE TABLE public.{source_child} (id INTEGER PRIMARY KEY, parent_id INTEGER NOT NULL, CONSTRAINT {constraint} FOREIGN KEY (parent_id) REFERENCES public.{parent}(id))"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE TABLE public.{target_child} (id INTEGER PRIMARY KEY, parent_id INTEGER NOT NULL)"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        let source_schema = driver
            .get_table_schema(&handle, &source_child, &database, Some("public"))
            .await
            .map_err(|error| error.to_string())?;
        let target_schema = driver
            .get_table_schema(&handle, &target_child, &database, Some("public"))
            .await
            .map_err(|error| error.to_string())?;
        Ok::<_, String>((source_schema, target_schema))
    }
    .await;

    for table in [&source_child, &target_child, &parent] {
        driver
            .execute(&handle, &format!("DROP TABLE IF EXISTS public.{table}"))
            .await
            .expect("remove unique-prefix fixture table");
    }
    driver.disconnect(handle).await.expect("disconnect");
    let (source_schema, target_schema) = schemas.expect("create and read FK fixtures");

    assert_eq!(source_schema.foreign_keys.len(), 1);
    assert_eq!(source_schema.foreign_keys[0].name, constraint);
    assert_eq!(
        source_schema.foreign_keys[0].columns,
        vec!["parent_id".to_string()]
    );
    assert_eq!(
        source_schema.foreign_keys[0].referenced_table,
        format!("public.{parent}")
    );
    assert_eq!(
        source_schema.foreign_keys[0].referenced_columns,
        vec!["id".to_string()]
    );
    assert!(target_schema.foreign_keys.is_empty());
}
