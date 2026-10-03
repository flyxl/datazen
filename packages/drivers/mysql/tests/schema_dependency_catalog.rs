//! Live regression for MySQL structured view dependencies.
//!
//! Run against an existing test database with unique fixtures:
//! MYSQL_MIGRATION_TEST_DATABASE=datazen_test MYSQL_MIGRATION_TEST_USER=root MYSQL_MIGRATION_TEST_PASSWORD= cargo test -p datazen-driver-mysql --test schema_dependency_catalog
//!
//! The prefix is `MYSQL_MIGRATION_TEST_`, not the bare `MIGRATION_TEST_` the
//! PostgreSQL suites use. One `cargo test -p <pg> -p <mysql>` puts every test
//! binary in a single environment, so a shared name would point these suites at
//! the PostgreSQL server. See `http-support/tests/support/migration_gate.rs`.
//!
//! This suite is no longer `#[ignore]`d. With `MYSQL_MIGRATION_TEST_DATABASE` unset it reports
//! `view-dependency-catalog` unverified and skips; with
//! `DATAZEN_CONTRACT_REQUIRE_LIVE=1` that report is a failure.

use datazen_driver_api::{ConnectionHandle, DatabaseDriver, QueryResult, Value as DriverValue};
use datazen_driver_mysql::MysqlDriver;
use serde_json::{json, Value};
#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;

#[tokio::test]
async fn view_dependency_catalog_returns_exact_table_and_routine_edges_when_visible() {
    // `None` means the gate already reported this dimension unverified — or, under
    // `DATAZEN_CONTRACT_REQUIRE_LIVE=1`, already failed it.
    let Some(config) = migration_gate::require_config(
        "mysql",
        "view-dependency-catalog",
        "mysql",
        "migration-view-dep",
        &["datazen_test"],
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
    let table_name = format!("dz_mig_dep_base_{suffix}");
    let view_name = format!("dz_mig_dep_view_{suffix}");
    let routine_name = format!("dz_mig_dep_fn_{suffix}");

    let driver = MysqlDriver::new(false);
    let handle = driver
        .connect(&config)
        .await
        .expect("connect to isolated MySQL test database");

    let lookup = async {
        driver
            .execute(
                &handle,
                &format!("CREATE TABLE {database}.{table_name} (id INT NOT NULL)"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE FUNCTION {database}.{routine_name}(x INT) RETURNS INT DETERMINISTIC RETURN x"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE VIEW {database}.{view_name} AS SELECT {database}.{routine_name}(id) AS id FROM {database}.{table_name}"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        let sql = datazen_driver_api::schema_dependencies::view_dependencies_sql(
            "mysql",
            &view_name,
            Some(&database),
        )
        .expect("MySQL view catalog query");
        let raw_result = driver
            .query(&handle, &sql)
            .await
            .map_err(|error| format!("raw MySQL dependency catalog query failed: {error}"))?;
        let raw_rows = raw_result.rows;
        let result = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"view","schema":database,"name":view_name}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let missing_view = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"view","schema":database,"name":format!("{view_name}_missing")}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let routine = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"function","schema":database,"name":routine_name}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        Ok::<_, String>((result, raw_rows, missing_view, routine))
    }
    .await;

    // Only these generated, unique objects are owned by this test.
    for sql in [
        format!("DROP VIEW IF EXISTS {database}.{view_name}"),
        format!("DROP FUNCTION IF EXISTS {database}.{routine_name}"),
        format!("DROP TABLE IF EXISTS {database}.{table_name}"),
    ] {
        driver
            .execute(&handle, &sql)
            .await
            .unwrap_or_else(|error| panic!("fixture cleanup failed for owned object: {error}"));
    }
    driver.disconnect(handle).await.expect("disconnect");
    let (result, raw_rows, missing_view, routine) =
        lookup.expect("create fixtures and query dependency catalog");

    assert_eq!(
        result["complete"], true,
        "view catalog is complete: {result}"
    );
    assert_eq!(
        raw_rows.len(),
        2,
        "raw query returns table and routine edges"
    );
    assert_eq!(
        result["dependencies"].as_array().map(Vec::len),
        Some(2),
        "view catalog contains exactly the expected direct edges: {result}"
    );
    assert_eq!(
        missing_view["complete"], false,
        "missing view identity must fail closed: {missing_view}"
    );
    assert_eq!(
        routine["complete"], false,
        "opaque routine dependencies must fail closed: {routine}"
    );
    assert_has_dependency(&result, "table", &database, &table_name);
    assert_has_dependency(&result, "function", &database, &routine_name);
}

#[tokio::test]
async fn table_dependency_catalog_proves_empty_and_exact_composite_fk_edges() {
    // `None` means the gate already reported this dimension unverified — or, under
    // `DATAZEN_CONTRACT_REQUIRE_LIVE=1`, already failed it.
    let Some(config) = migration_gate::require_config(
        "mysql",
        "view-dependency-catalog",
        "mysql",
        "migration-view-dep",
        &["datazen_test"],
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
    let no_fk = format!("dz_mig_no_fk_{suffix}");
    let parent = format!("dz_mig_parent_{suffix}");
    let child = format!("dz_mig_child_{suffix}");
    let cycle_a = format!("dz_mig_cycle_a_{suffix}");
    let cycle_b = format!("dz_mig_cycle_b_{suffix}");
    let cross_schema = format!("dz_mig_fk_scope_{suffix}");
    let cross_parent = format!("dz_mig_cross_parent_{suffix}");
    let cross_child = format!("dz_mig_cross_child_{suffix}");

    let driver = MysqlDriver::new(false);
    let handle = driver
        .connect(&config)
        .await
        .expect("connect to isolated MySQL test database");
    let mut cross_schema_created = false;
    let mut cross_schema_fk_supported = false;
    let mut cycle_a_fk_created = false;
    let mut cycle_b_fk_created = false;

    let lookup = async {
        for sql in [
            format!("CREATE TABLE `{database}`.`{no_fk}` (id INT NOT NULL PRIMARY KEY) ENGINE=InnoDB"),
            format!(
                "CREATE TABLE `{database}`.`{parent}` (tenant_id INT NOT NULL, record_id INT NOT NULL, PRIMARY KEY (tenant_id, record_id)) ENGINE=InnoDB"
            ),
            format!(
                "CREATE TABLE `{database}`.`{child}` (id INT NOT NULL PRIMARY KEY, tenant_id INT NOT NULL, record_id INT NOT NULL, manager_id INT NULL, CONSTRAINT `fk_comp_{suffix}` FOREIGN KEY (tenant_id, record_id) REFERENCES `{database}`.`{parent}` (tenant_id, record_id), CONSTRAINT `fk_self_{suffix}` FOREIGN KEY (manager_id) REFERENCES `{database}`.`{child}` (id)) ENGINE=InnoDB"
            ),
            format!(
                "CREATE TABLE `{database}`.`{cycle_a}` (id INT NOT NULL PRIMARY KEY, peer_id INT NULL, KEY peer_idx (peer_id)) ENGINE=InnoDB"
            ),
            format!(
                "CREATE TABLE `{database}`.`{cycle_b}` (id INT NOT NULL PRIMARY KEY, peer_id INT NULL, KEY peer_idx (peer_id)) ENGINE=InnoDB"
            ),
        ] {
            driver
                .execute(&handle, &sql)
                .await
                .map_err(|error| format!("create owned table fixture failed: {error}"))?;
        }
        driver
            .execute(
                &handle,
                &format!(
                    "ALTER TABLE `{database}`.`{cycle_a}` ADD CONSTRAINT `fk_cycle_ab_{suffix}` FOREIGN KEY (peer_id) REFERENCES `{database}`.`{cycle_b}` (id)"
                ),
            )
            .await
            .map_err(|error| format!("create first owned cycle edge failed: {error}"))?;
        cycle_a_fk_created = true;
        driver
            .execute(
                &handle,
                &format!(
                    "ALTER TABLE `{database}`.`{cycle_b}` ADD CONSTRAINT `fk_cycle_ba_{suffix}` FOREIGN KEY (peer_id) REFERENCES `{database}`.`{cycle_a}` (id)"
                ),
            )
            .await
            .map_err(|error| format!("create second owned cycle edge failed: {error}"))?;
        cycle_b_fk_created = true;

        // MySQL/MariaDB configurations may deny CREATE DATABASE or may not
        // support cross-schema foreign keys. Exercise this edge when the
        // isolated test account/server permits it; same-schema coverage is
        // required in all cases.
        match driver
            .execute(&handle, &format!("CREATE DATABASE `{cross_schema}`"))
            .await
        {
            Ok(_) => {
            cross_schema_created = true;
            let setup_cross_schema = async {
                driver
                    .execute(
                        &handle,
                        &format!(
                            "CREATE TABLE `{cross_schema}`.`{cross_parent}` (id INT NOT NULL PRIMARY KEY) ENGINE=InnoDB"
                        ),
                    )
                    .await?;
                driver
                    .execute(
                        &handle,
                        &format!(
                            "CREATE TABLE `{database}`.`{cross_child}` (remote_id INT NOT NULL, KEY remote_idx (remote_id), CONSTRAINT `fk_cross_{suffix}` FOREIGN KEY (remote_id) REFERENCES `{cross_schema}`.`{cross_parent}` (id)) ENGINE=InnoDB"
                        ),
                    )
                    .await?;
                Ok::<_, datazen_driver_api::DriverError>(())
            }
            .await;
            cross_schema_fk_supported = setup_cross_schema.is_ok();
                if let Err(error) = setup_cross_schema {
                    eprintln!("optional cross-schema FK fixture unavailable: {error}");
                }
            }
            Err(error) => eprintln!("optional cross-schema FK fixture unavailable: {error}"),
        }

        let plain = table_dependencies(&driver, &handle, &database, &no_fk).await?;
        let parent_dependencies = table_dependencies(&driver, &handle, &database, &parent).await?;
        let child_dependencies = table_dependencies(&driver, &handle, &database, &child).await?;
        let cycle_a_dependencies = table_dependencies(&driver, &handle, &database, &cycle_a).await?;
        let cycle_b_dependencies = table_dependencies(&driver, &handle, &database, &cycle_b).await?;
        let missing = table_dependencies(
            &driver,
            &handle,
            &database,
            &format!("{no_fk}_missing"),
        )
        .await?;
        let cross = if cross_schema_fk_supported {
            Some(table_dependencies(&driver, &handle, &database, &cross_child).await?)
        } else {
            None
        };
        Ok::<_, String>((
            plain,
            parent_dependencies,
            child_dependencies,
            cycle_a_dependencies,
            cycle_b_dependencies,
            missing,
            cross,
        ))
    }
    .await;

    let mut cleanup_errors = Vec::new();
    for (table, constraint, created) in [
        (
            &cycle_a,
            format!("fk_cycle_ab_{suffix}"),
            cycle_a_fk_created,
        ),
        (
            &cycle_b,
            format!("fk_cycle_ba_{suffix}"),
            cycle_b_fk_created,
        ),
    ] {
        if created {
            if let Err(error) = driver
                .execute(
                    &handle,
                    &format!("ALTER TABLE `{database}`.`{table}` DROP FOREIGN KEY `{constraint}`"),
                )
                .await
            {
                cleanup_errors.push(format!("drop owned foreign key {constraint}: {error}"));
            }
        }
    }
    for table in [&cross_child, &child, &cycle_a, &cycle_b, &parent, &no_fk] {
        if let Err(error) = driver
            .execute(
                &handle,
                &format!("DROP TABLE IF EXISTS `{database}`.`{table}`"),
            )
            .await
        {
            cleanup_errors.push(format!("drop owned table {table}: {error}"));
        }
    }
    if cross_schema_created {
        if let Err(error) = driver
            .execute(
                &handle,
                &format!("DROP DATABASE IF EXISTS `{cross_schema}`"),
            )
            .await
        {
            cleanup_errors.push(format!("drop owned schema {cross_schema}: {error}"));
        }
    }
    let source_remaining = driver
        .query(
            &handle,
            &format!(
                "SELECT COUNT(*) AS remaining FROM information_schema.TABLES WHERE TABLE_SCHEMA = '{database}' AND TABLE_NAME IN ('{no_fk}', '{parent}', '{child}', '{cycle_a}', '{cycle_b}', '{cross_child}')"
            ),
        )
        .await;
    match source_remaining {
        Ok(result) => match single_integer(&result) {
            Ok(0) => {}
            Ok(remaining) => cleanup_errors.push(format!("{remaining} owned source tables remain")),
            Err(error) => cleanup_errors.push(format!("verify source fixture cleanup: {error}")),
        },
        Err(error) => cleanup_errors.push(format!("verify source fixture cleanup: {error}")),
    }
    if cross_schema_created {
        let cross_remaining = driver
            .query(
                &handle,
                &format!(
                    "SELECT COUNT(*) AS remaining FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = '{cross_schema}'"
                ),
            )
            .await;
        match cross_remaining {
            Ok(result) => match single_integer(&result) {
                Ok(0) => {}
                Ok(remaining) => {
                    cleanup_errors.push(format!("{remaining} owned cross schema remains"))
                }
                Err(error) => cleanup_errors.push(format!("verify cross-schema cleanup: {error}")),
            },
            Err(error) => cleanup_errors.push(format!("verify cross-schema cleanup: {error}")),
        }
    }
    if let Err(error) = driver.disconnect(handle).await {
        cleanup_errors.push(format!("disconnect after fixture cleanup: {error}"));
    }
    assert!(
        cleanup_errors.is_empty(),
        "fixture cleanup errors: {cleanup_errors:?}"
    );
    let (
        plain,
        parent_dependencies,
        child_dependencies,
        cycle_a_dependencies,
        cycle_b_dependencies,
        missing,
        cross,
    ) = lookup.expect("create fixtures and query table dependency catalog");

    assert_complete_dependencies(&plain, &[]);
    assert_complete_dependencies(&parent_dependencies, &[]);
    assert_complete_dependencies(
        &child_dependencies,
        &[("table", database.as_str(), parent.as_str())],
    );
    assert_complete_dependencies(
        &cycle_a_dependencies,
        &[("table", database.as_str(), cycle_b.as_str())],
    );
    assert_complete_dependencies(
        &cycle_b_dependencies,
        &[("table", database.as_str(), cycle_a.as_str())],
    );
    assert_eq!(
        missing["complete"], false,
        "missing table must fail closed: {missing}"
    );
    assert_eq!(missing["dependencies"], serde_json::json!([]));
    if let Some(cross) = cross {
        assert_complete_dependencies(
            &cross,
            &[("table", cross_schema.as_str(), cross_parent.as_str())],
        );
    }
}

fn single_integer(result: &QueryResult) -> Result<i64, String> {
    let index = result
        .columns
        .iter()
        .position(|column| column.name.eq_ignore_ascii_case("remaining"))
        .ok_or_else(|| "cleanup query missing remaining column".to_string())?;
    let value = result
        .rows
        .first()
        .and_then(|row| row.get(index))
        .and_then(Option::as_ref)
        .ok_or_else(|| "cleanup query returned no count".to_string())?;
    match value {
        DriverValue::Integer(count) => Ok(*count),
        DriverValue::String(count) => count.parse().map_err(|_| "cleanup count is invalid".into()),
        _ => Err("cleanup query returned a non-integer count".into()),
    }
}

async fn table_dependencies(
    driver: &MysqlDriver,
    handle: &ConnectionHandle,
    schema: &str,
    name: &str,
) -> Result<Value, String> {
    driver
        .execute_command(
            handle,
            "get_object_dependencies",
            json!({"kind":"table","schema":schema,"name":name}),
        )
        .await
        .map(|result| result.data)
        .map_err(|error| error.to_string())
}

fn assert_complete_dependencies(data: &Value, expected: &[(&str, &str, &str)]) {
    assert_eq!(data["complete"], true, "table catalog incomplete: {data}");
    let dependencies = data["dependencies"]
        .as_array()
        .expect("dependencies array in driver command result");
    let actual = dependencies
        .iter()
        .map(|dependency| {
            (
                dependency["kind"].as_str().unwrap_or_default(),
                dependency["schema"].as_str().unwrap_or_default(),
                dependency["name"].as_str().unwrap_or_default(),
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    let expected = expected
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        actual, expected,
        "unexpected exact dependency identities: {data}"
    );
    assert_eq!(
        dependencies.len(),
        expected.len(),
        "composite FK edges must dedupe: {data}"
    );
}

fn assert_has_dependency(data: &Value, kind: &str, schema: &str, name: &str) {
    let dependencies = data["dependencies"]
        .as_array()
        .expect("dependencies array in driver command result");
    assert!(
        dependencies.iter().any(|dependency| {
            dependency["kind"] == kind
                && dependency["schema"] == schema
                && dependency["name"] == name
        }),
        "missing exact dependency {kind} {schema}.{name} in {data}"
    );
}
