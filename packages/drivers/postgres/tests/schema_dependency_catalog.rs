//! Live regression for structured PostgreSQL schema-object dependencies.
//!
//! Run only against a disposable migration database:
//! MIGRATION_TEST_DATABASE=<database> cargo test -p datazen-driver-postgres --test schema_dependency_catalog
//!
//! This suite is no longer `#[ignore]`d. With `MIGRATION_TEST_DATABASE` unset it reports
//! `view-and-trigger-dependencies` unverified and skips; with
//! `DATAZEN_CONTRACT_REQUIRE_LIVE=1` that report is a failure.

use datazen_driver_api::DatabaseDriver;
use datazen_driver_postgres::PostgresDriver;
use serde_json::{json, Value};
#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;

#[tokio::test]
async fn structured_view_and_trigger_dependencies_are_exact_and_complete() {
    // `None` means the gate already reported this dimension unverified — or, under
    // `DATAZEN_CONTRACT_REQUIRE_LIVE=1`, already failed it.
    let Some(config) = migration_gate::require_config(
        "postgresql",
        "view-and-trigger-dependencies",
        "postgresql",
        "migration-dep",
        &["datazen_sync_src"],
    ) else {
        return;
    };
    // No `database` local: this suite works entirely inside suffixed objects,
    // so it never names the database it was pointed at. The four sibling
    // catalog suites do need it, and each keeps its own.
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let base_table = format!("dz_mig_dep_base_{suffix}");
    let view_name = format!("dz_mig_dep_view_{suffix}");
    let view_function = format!("dz_mig_dep_view_fn_{suffix}");
    let trigger_table = format!("dz_mig_dep_trigger_table_{suffix}");
    let trigger_name = format!("dz_mig_dep_trigger_{suffix}");
    let constraint_trigger_name = format!("dz_mig_dep_constraint_trigger_{suffix}");
    let trigger_function = format!("dz_mig_dep_trigger_fn_{suffix}");
    let custom_type = format!("dz_mig_dep_type_{suffix}");
    let expression_table = format!("dz_mig_dep_expression_table_{suffix}");
    let domain_type = format!("dz_mig_dep_domain_{suffix}");
    let range_type = format!("dz_mig_dep_range_{suffix}");
    let typed_table = format!("dz_mig_dep_typed_table_{suffix}");
    let sequence_table = format!("dz_mig_dep_sequence_table_{suffix}");
    let sequence_name = format!("dz_mig_dep_sequence_{suffix}");
    let standalone_sequence = format!("dzstandalone_{suffix}");
    let ambiguous_sequence = format!("dzambiguous_{suffix}");
    let missing_sequence_name = format!("{standalone_sequence}_missing");
    let fk_parent = format!("dz_mig_dep_fk_parent_{suffix}");
    let fk_child = format!("dz_mig_dep_fk_child_{suffix}");
    let serial_table = format!("dz_serial_table_{suffix}");
    let serial_sequence = format!("{serial_table}_id_seq");
    let bigserial_sequence = format!("{serial_table}_big_id_seq");
    let ambiguous_schema = format!("dz_mig_dep_schema_{suffix}");

    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&config)
        .await
        .expect("connect to isolated PostgreSQL database");

    let lookup = async {
        driver
            .execute(
                &handle,
                &format!("CREATE TABLE public.{base_table} (id integer NOT NULL)"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE FUNCTION public.{view_function}(integer) RETURNS integer LANGUAGE SQL IMMUTABLE AS $$ SELECT $1 $$"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE VIEW public.{view_name} AS SELECT public.{view_function}(id) AS id FROM public.{base_table}"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(&handle, &format!("CREATE SCHEMA {ambiguous_schema}"))
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!("CREATE VIEW {ambiguous_schema}.{view_name} AS SELECT 1 AS id"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE FUNCTION public.{trigger_function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!("CREATE TABLE public.{trigger_table} (id integer NOT NULL)"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE TRIGGER {trigger_name} BEFORE INSERT ON public.{trigger_table} FOR EACH ROW EXECUTE FUNCTION public.{trigger_function}()"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE CONSTRAINT TRIGGER {constraint_trigger_name} AFTER INSERT ON public.{trigger_table} DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.{trigger_function}()"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!("CREATE TYPE public.{custom_type} AS ENUM ('fresh', 'ready')"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE TABLE public.{expression_table} (status text DEFAULT ('ready'::public.{custom_type})::text, CHECK ((status::public.{custom_type})::text <> ''))"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!("CREATE DOMAIN public.{domain_type} AS integer CHECK (VALUE > 0)"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!("CREATE TYPE public.{range_type} AS RANGE (SUBTYPE = integer)"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE TABLE public.{typed_table} (status public.{custom_type} NOT NULL)"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!("CREATE TABLE public.{sequence_table} (id bigint)"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE SEQUENCE public.{sequence_name} OWNED BY public.{sequence_table}.id"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!("CREATE SEQUENCE public.{standalone_sequence}"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!("CREATE SEQUENCE public.{ambiguous_sequence}"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!("CREATE SEQUENCE {ambiguous_schema}.{ambiguous_sequence}"),
            )
            .await
            .map_err(|error| error.to_string())?;
        let type_sql = datazen_driver_api::schema_dependencies::postgres_type_dependencies_sql(
            &custom_type,
            Some("public"),
        );
        driver
            .query(&handle, &type_sql)
            .await
            .map_err(|error| format!("raw type catalog query failed: {error:?}"))?;
        driver
            .execute(
                &handle,
                &format!("CREATE TABLE public.{serial_table} (id SERIAL PRIMARY KEY, big_id BIGSERIAL UNIQUE)"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!("CREATE TABLE public.{fk_parent} (id integer PRIMARY KEY)"),
            )
            .await
            .map_err(|error| error.to_string())?;
        driver
            .execute(
                &handle,
                &format!(
                    "CREATE TABLE public.{fk_child} (parent_id integer REFERENCES public.{fk_parent}(id))"
                ),
            )
            .await
            .map_err(|error| error.to_string())?;

        let view = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"view","schema":"public","name":view_name}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let ambiguous_view = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"view","name":view_name}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let missing_view = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"view","schema":"public","name":format!("{view_name}_missing")}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let routine = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"function","schema":"public","name":view_function,"signature":"integer"}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let trigger = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({
                    "kind":"trigger",
                    "schema":"public",
                    "name":trigger_name,
                    "targetSchema":"public",
                    "targetName":trigger_table
                }),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let constraint_trigger = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({
                    "kind":"trigger",
                    "schema":"public",
                    "name":constraint_trigger_name,
                    "targetSchema":"public",
                    "targetName":trigger_table
                }),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let table = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"table","schema":"public","name":typed_table}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let expression_table_dependencies = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"table","schema":"public","name":expression_table}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let type_dependencies = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"type","schema":"public","name":custom_type}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let domain_dependencies = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"type","schema":"public","name":domain_type}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let range_dependencies = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"type","schema":"public","name":range_type}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let sequence = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"sequence","schema":"public","name":sequence_name}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let standalone_sequence_dependencies = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"sequence","schema":"public","name":standalone_sequence}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let ambiguous_sequence_dependencies = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"sequence","name":ambiguous_sequence}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let missing_sequence_dependencies = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"sequence","schema":"public","name":missing_sequence_name}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let serial_table_dependencies = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"table","schema":"public","name":serial_table}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let serial_sequence_dependencies = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"sequence","schema":"public","name":serial_sequence}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let bigserial_sequence_dependencies = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"sequence","schema":"public","name":bigserial_sequence}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        let fk_child_dependencies = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"table","schema":"public","name":fk_child}),
            )
            .await
            .map_err(|error| error.to_string())?
            .data;
        Ok::<_, String>((
            view,
            ambiguous_view,
            missing_view,
            routine,
            trigger,
            constraint_trigger,
            table,
            expression_table_dependencies,
            type_dependencies,
            domain_dependencies,
            range_dependencies,
            sequence,
            standalone_sequence_dependencies,
            ambiguous_sequence_dependencies,
            missing_sequence_dependencies,
            serial_table_dependencies,
            serial_sequence_dependencies,
            bigserial_sequence_dependencies,
            fk_child_dependencies,
        ))
    }
    .await;

    // Drop only the unique objects created above, even if setup or lookup failed.
    for sql in [
        format!("DROP SCHEMA IF EXISTS {ambiguous_schema} CASCADE"),
        format!("DROP VIEW IF EXISTS public.{view_name}"),
        format!("DROP TABLE IF EXISTS public.{trigger_table} CASCADE"),
        format!("DROP FUNCTION IF EXISTS public.{trigger_function}()"),
        format!("DROP SEQUENCE IF EXISTS public.{sequence_name}"),
        format!("DROP SEQUENCE IF EXISTS public.{standalone_sequence}"),
        format!("DROP SEQUENCE IF EXISTS public.{ambiguous_sequence}"),
        format!("DROP TABLE IF EXISTS public.{serial_table} CASCADE"),
        format!("DROP TABLE IF EXISTS public.{fk_child} CASCADE"),
        format!("DROP TABLE IF EXISTS public.{fk_parent} CASCADE"),
        format!("DROP TABLE IF EXISTS public.{sequence_table} CASCADE"),
        format!("DROP TABLE IF EXISTS public.{typed_table} CASCADE"),
        format!("DROP TABLE IF EXISTS public.{expression_table} CASCADE"),
        format!("DROP TYPE IF EXISTS public.{range_type}"),
        format!("DROP DOMAIN IF EXISTS public.{domain_type}"),
        format!("DROP TYPE IF EXISTS public.{custom_type}"),
        format!("DROP TABLE IF EXISTS public.{base_table} CASCADE"),
        format!("DROP FUNCTION IF EXISTS public.{view_function}(integer)"),
    ] {
        driver
            .execute(&handle, &sql)
            .await
            .unwrap_or_else(|error| panic!("fixture cleanup failed for owned object: {error}"));
    }
    driver.disconnect(handle).await.expect("disconnect");
    let (
        view,
        ambiguous_view,
        missing_view,
        routine,
        trigger,
        constraint_trigger,
        table,
        expression_table_dependencies,
        type_dependencies,
        domain_dependencies,
        range_dependencies,
        sequence,
        standalone_sequence_dependencies,
        ambiguous_sequence_dependencies,
        missing_sequence_dependencies,
        serial_table_dependencies,
        serial_sequence_dependencies,
        bigserial_sequence_dependencies,
        fk_child_dependencies,
    ) = lookup.expect("create fixtures and query dependency catalog");

    assert_eq!(
        ambiguous_view["complete"], false,
        "ambiguous view identity must fail closed: {ambiguous_view}"
    );
    assert_eq!(
        missing_view["complete"], false,
        "missing view identity must fail closed: {missing_view}"
    );
    assert_eq!(
        routine["complete"], false,
        "opaque routine dependencies must fail closed: {routine}"
    );
    assert_dependencies(
        &trigger,
        &[
            ("table", "public", trigger_table.as_str(), None),
            ("function", "public", trigger_function.as_str(), Some("")),
        ],
    );
    assert_eq!(
        constraint_trigger["complete"], false,
        "constraint trigger dependencies must fail closed: {constraint_trigger}"
    );
    assert_dependencies(&table, &[("type", "public", custom_type.as_str(), None)]);
    assert_type_usages(
        &table,
        &[(
            "public",
            custom_type.as_str(),
            "column_type",
            Some("status"),
        )],
    );
    assert_dependencies(
        &expression_table_dependencies,
        &[("type", "public", custom_type.as_str(), None)],
    );
    assert_type_usages(
        &expression_table_dependencies,
        &[
            ("public", custom_type.as_str(), "expression", None),
            ("public", custom_type.as_str(), "constraint", None),
        ],
    );
    assert_dependencies(&type_dependencies, &[]);
    assert_dependencies(&domain_dependencies, &[]);
    assert_dependencies(&range_dependencies, &[]);
    assert_dependencies(
        &sequence,
        &[("table", "public", sequence_table.as_str(), None)],
    );
    assert_sequence_usages(
        &sequence,
        &[(
            "public",
            sequence_name.as_str(),
            "public",
            sequence_table.as_str(),
            "id",
            "owned_by",
        )],
    );
    assert_dependencies(&standalone_sequence_dependencies, &[]);
    assert_sequence_usages(&standalone_sequence_dependencies, &[]);
    assert_eq!(
        ambiguous_sequence_dependencies["complete"], false,
        "same-name sequences across schemas must fail closed: {ambiguous_sequence_dependencies}"
    );
    assert!(
        ambiguous_sequence_dependencies
            .get("sequenceDependencyUsages")
            .is_none(),
        "ambiguous sequence identities must not expose a partial usage proof"
    );
    assert_eq!(
        missing_sequence_dependencies["complete"], false,
        "missing sequence {missing_sequence_name} must fail closed: {missing_sequence_dependencies}"
    );
    assert!(
        missing_sequence_dependencies
            .get("sequenceDependencyUsages")
            .is_none(),
        "missing sequence identity must not expose an empty proof"
    );
    // Serial semantics create both edges: table default -> sequence and
    // sequence OWNED BY -> table. The Host must split/defer OWNED BY to break
    // this valid catalog cycle before deployment.
    assert_dependencies(
        &serial_table_dependencies,
        &[
            ("sequence", "public", serial_sequence.as_str(), None),
            ("sequence", "public", bigserial_sequence.as_str(), None),
        ],
    );
    assert_sequence_usages(
        &serial_table_dependencies,
        &[
            (
                "public",
                serial_sequence.as_str(),
                "public",
                serial_table.as_str(),
                "id",
                "column_default",
            ),
            (
                "public",
                bigserial_sequence.as_str(),
                "public",
                serial_table.as_str(),
                "big_id",
                "column_default",
            ),
        ],
    );
    assert_dependencies(
        &serial_sequence_dependencies,
        &[("table", "public", serial_table.as_str(), None)],
    );
    assert_sequence_usages(
        &serial_sequence_dependencies,
        &[(
            "public",
            serial_sequence.as_str(),
            "public",
            serial_table.as_str(),
            "id",
            "owned_by",
        )],
    );
    assert_dependencies(
        &bigserial_sequence_dependencies,
        &[("table", "public", serial_table.as_str(), None)],
    );
    assert_sequence_usages(
        &bigserial_sequence_dependencies,
        &[(
            "public",
            bigserial_sequence.as_str(),
            "public",
            serial_table.as_str(),
            "big_id",
            "owned_by",
        )],
    );
    assert_dependencies(
        &fk_child_dependencies,
        &[("table", "public", fk_parent.as_str(), None)],
    );
    assert_dependencies(
        &view,
        &[
            ("table", "public", base_table.as_str(), None),
            (
                "function",
                "public",
                view_function.as_str(),
                Some("integer"),
            ),
        ],
    );
}

fn assert_type_usages(data: &Value, expected: &[(&str, &str, &str, Option<&str>)]) {
    assert_eq!(
        data["complete"], true,
        "type usage catalog is incomplete: {data}"
    );
    let usages = data["typeDependencyUsages"]
        .as_array()
        .expect("typeDependencyUsages array in driver command result");
    assert_eq!(
        usages.len(),
        expected.len(),
        "type usage catalog contains missing or unexpected usage edges for {expected:?}: {data}"
    );
    for (schema, name, usage, column_name) in expected {
        assert!(
            usages.iter().any(|entry| {
                entry["dependency"]["kind"] == "type"
                    && entry["dependency"]["schema"] == *schema
                    && entry["dependency"]["name"] == *name
                    && entry["usage"] == *usage
                    && match column_name {
                        Some(column_name) => entry["columnName"] == *column_name,
                        None => entry.get("columnName").is_none(),
                    }
            }),
            "missing exact type usage {usage} for {schema}.{name} in {data}"
        );
    }
}

fn assert_dependencies(data: &Value, expected: &[(&str, &str, &str, Option<&str>)]) {
    assert_eq!(
        data["complete"], true,
        "catalog must prove completeness for {expected:?}: {data}"
    );
    let dependencies = data["dependencies"]
        .as_array()
        .expect("dependencies array in driver command result");
    assert_eq!(
        dependencies.len(),
        expected.len(),
        "dependency catalog contains missing or unexpected edges for {expected:?}: {data}"
    );
    for (kind, schema, name, signature) in expected {
        assert!(
            dependencies.iter().any(|dependency| {
                dependency["kind"] == *kind
                    && dependency["schema"] == *schema
                    && dependency["name"] == *name
                    && match signature {
                        Some(signature) => dependency["signature"] == *signature,
                        None => dependency["signature"].is_null(),
                    }
            }),
            "missing exact dependency {kind} {schema}.{name} {signature:?} in {data}"
        );
    }
}

fn assert_sequence_usages(data: &Value, expected: &[(&str, &str, &str, &str, &str, &str)]) {
    assert_eq!(
        data["complete"], true,
        "sequence usage catalog is incomplete for {expected:?}: {data}"
    );
    let usages = data["sequenceDependencyUsages"]
        .as_array()
        .expect("sequenceDependencyUsages array in driver command result");
    assert_eq!(
        usages.len(),
        expected.len(),
        "sequence usage catalog contains missing or unexpected edges for {expected:?}: {data}"
    );
    for (sequence_schema, sequence_name, owner_schema, owner_table, column_name, usage) in expected
    {
        assert!(
            usages.iter().any(|entry| {
                entry["sequence"]["kind"] == "sequence"
                    && entry["sequence"]["schema"] == *sequence_schema
                    && entry["sequence"]["name"] == *sequence_name
                    && entry["ownerTable"]["kind"] == "table"
                    && entry["ownerTable"]["schema"] == *owner_schema
                    && entry["ownerTable"]["name"] == *owner_table
                    && entry["columnName"] == *column_name
                    && entry["usage"] == *usage
            }),
            "missing exact sequence usage {usage} for {sequence_schema}.{sequence_name} -> {owner_schema}.{owner_table}.{column_name} in {data}"
        );
    }
}
