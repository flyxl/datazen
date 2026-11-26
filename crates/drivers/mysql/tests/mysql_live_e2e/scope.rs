use super::Fixture;
use datazen_driver_api::{
    DatabaseDriver, DatabaseObject, MigrationOperation, MigrationView, ObjectKind,
    SchemaObjectScopeDependency,
};
use serde_json::json;

#[tokio::test]
async fn mysql_rendered_view_scope_switches_real_local_relations_but_keeps_external_ones() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let source = f.database.clone();
    let target = std::env::var("TEST_MYSQL_DATABASE_B").unwrap();
    let external = std::env::var("MYSQL_MIGRATION_TEST_DATABASE").unwrap();
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let local = format!("dz_scope_local_{tag}");
    let mapped = format!("dz_scope_other_name_{tag}");
    let dependency = format!("dz_scope_external_{tag}");
    let view = format!("dz_scope_view_{tag}");
    sqlx::raw_sql(&format!(
        "CREATE TABLE {source}.{local}(v INT); INSERT INTO {source}.{local} VALUES(11);
         CREATE TABLE {target}.{mapped}(v INT); INSERT INTO {target}.{mapped} VALUES(73);
         CREATE TABLE {external}.{dependency}(v INT); INSERT INTO {external}.{dependency} VALUES(1000);"
    )).execute(&mut f.oracle).await.expect("three distinct real databases and decoy values");
    let definition = format!(
        "WITH selected AS (SELECT v FROM `{source}`.`{local}`)\nSELECT selected.v+extra.v AS val, '雨' AS label FROM selected CROSS JOIN `{external}`.`{dependency}` extra"
    );
    sqlx::raw_sql(&format!("CREATE VIEW {source}.{view} AS {definition}"))
        .execute(&mut f.oracle)
        .await
        .expect("actual source view with complete relations");
    let response = driver
        .execute_command(
            &handle,
            "get_object_dependencies",
            json!({"kind":"view","schema":source,"name":view}),
        )
        .await
        .expect("real dependency discovery");
    assert_eq!(response.data["complete"], true);
    let refs: Vec<DatabaseObject> = serde_json::from_value(response.data["dependencies"].clone())
        .expect("actual dependency catalog");
    assert_eq!(refs.len(), 2);
    let deps: Vec<_> = refs
        .into_iter()
        .map(|original| {
            let mut dest = original.clone();
            if original.schema.as_deref() == Some(&source) {
                assert_eq!(original.name, local);
                dest.schema = Some(target.clone());
                dest.name = mapped.clone();
            } else {
                assert_eq!(original.schema.as_deref(), Some(external.as_str()));
                assert_eq!(original.name, dependency);
            }
            SchemaObjectScopeDependency {
                source: original,
                target: dest,
            }
        })
        .collect();
    let renderer = driver.migration_renderer().expect("MySQL scope mapper");
    let mapping = renderer
        .map_schema_object_scope(ObjectKind::View, &source, &target, &definition, &deps)
        .expect("proved exact mapping")
        .expect("native view mapping supported");
    assert!(
        mapping
            .definition
            .contains(&format!("`{target}`.`{mapped}`"))
    );
    assert!(
        mapping
            .definition
            .contains(&format!("`{external}`.`{dependency}`"))
    );
    assert_eq!(mapping.dependencies.len(), 2);
    let statement = renderer
        .render(&MigrationOperation::CreateView {
            view: MigrationView {
                schema: Some(target.clone()),
                name: view.clone(),
                definition: mapping.definition,
            },
        })
        .expect("native render target");
    driver
        .execute(&handle, &statement.sql)
        .await
        .expect("execute proved scope rewrite");
    let source_v: (i64, String) = sqlx::query_as(&format!("SELECT val,label FROM {source}.{view}"))
        .fetch_one(&mut f.oracle)
        .await
        .expect("actual source view");
    let target_v: (i64, String) = sqlx::query_as(&format!("SELECT val,label FROM {target}.{view}"))
        .fetch_one(&mut f.oracle)
        .await
        .expect("actual destination and external data");
    assert_eq!(source_v, (1011, "雨".into()));
    assert_eq!(target_v, (1073, "雨".into()));
    assert!(
        renderer
            .map_schema_object_scope(ObjectKind::View, &source, &target, &definition, &deps[..1])
            .is_err()
    );
    let alternate = format!("SELECT {local}.v FROM {local}");
    let mapped_unqualified = renderer
        .map_schema_object_scope(ObjectKind::View, &source, &target, &alternate, &deps)
        .expect("unqualified real local relation")
        .expect("definition")
        .definition;
    let replacement = renderer
        .render(&MigrationOperation::ReplaceView {
            current: MigrationView {
                schema: Some(target.clone()),
                name: view.clone(),
                definition: format!("SELECT v FROM `{target}`.`{mapped}`"),
            },
            desired: MigrationView {
                schema: Some(target.clone()),
                name: view.clone(),
                definition: mapped_unqualified,
            },
        })
        .expect("documented reusable mapping");
    driver
        .execute(&handle, &replacement.sql)
        .await
        .expect("target database resolves unqualified relation name");
    let actual: i32 = sqlx::query_scalar(&format!("SELECT v FROM {target}.{view}"))
        .fetch_one(&mut f.oracle)
        .await
        .expect("actual renamed relation");
    assert_eq!(actual, 73);
    for (number, body) in [
        (
            1,
            format!("SELECT alias_name.v FROM `{source}`.`{local}` AS alias_name"),
        ),
        (
            2,
            format!("SELECT `{source}`.`{local}`.v FROM `{source}`.`{local}`"),
        ),
    ] {
        let definition = renderer
            .map_schema_object_scope(ObjectKind::View, &source, &target, &body, &deps)
            .expect("independently prove real source identifiers")
            .unwrap()
            .definition;
        let variant = format!("{view}_{number}");
        let ddl = renderer
            .render(&MigrationOperation::CreateView {
                view: MigrationView {
                    schema: Some(target.clone()),
                    name: variant.clone(),
                    definition,
                },
            })
            .expect("native target view shape");
        driver
            .execute(&handle, &ddl.sql)
            .await
            .expect("retain valid explicit or implicit correlation");
        let actual: i32 = sqlx::query_scalar(&format!("SELECT v FROM {target}.{variant}"))
            .fetch_one(&mut f.oracle)
            .await
            .expect("real view uses renamed native base table");
        assert_eq!(actual, 73);
    }
    driver
        .disconnect(handle)
        .await
        .expect("close actual scoped driver");
}

#[tokio::test]
async fn mysql_view_scope_preserves_proven_native_names_and_refuses_cte_ambiguity() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let source = f.database.clone();
    let target = std::env::var("TEST_MYSQL_DATABASE_B").unwrap();
    let third = std::env::var("MYSQL_MIGRATION_TEST_DATABASE").unwrap();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let base = format!("dz_member_{id}");
    let view = format!("dz_member_view_{id}");
    let collision = format!("dz_duplicate_{id}");
    let procedure = format!("dz_not_view_{id}");
    sqlx::raw_sql(&format!(
        "CREATE TABLE {source}.{base}(v INT);
        CREATE TABLE {target}.{base}(v INT);
        CREATE TABLE {third}.{base}(v INT);
        INSERT INTO {source}.{base} VALUES (13);
        INSERT INTO {target}.{base} VALUES (83);
        INSERT INTO {third}.{base} VALUES (10);
        CREATE PROCEDURE {source}.{procedure}() SELECT 1"
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    let exact = format!(
        "SELECT\n `{source}`.`{base}`.v + ext.v AS value, '雪' AS label\nFROM `{source}`.`{base}` JOIN `{third}`.`{base}` AS ext"
    );
    let teeter = format!(
        "WITH `{base}` AS (SELECT 99 AS v FROM DUAL) SELECT `{base}`.v + actual.v AS value
      FROM `{base}` JOIN `{source}`.`{base}` actual"
    );
    sqlx::raw_sql(&format!(
        "CREATE VIEW {source}.{view} AS {exact}; CREATE VIEW {source}.{collision} AS {teeter}"
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    let dependencies = async |name: &str| {
        let lookup = driver
            .execute_command(
                &handle,
                "get_object_dependencies",
                json!({"kind":"view","schema":source,"name":name}),
            )
            .await
            .unwrap();
        assert_eq!(lookup.data["complete"], true);
        serde_json::from_value::<Vec<DatabaseObject>>(lookup.data["dependencies"].clone())
            .unwrap()
            .into_iter()
            .map(|object| {
                let mut moved = object.clone();
                if moved.schema.as_deref() == Some(source.as_str()) {
                    moved.schema = Some(target.clone());
                }
                SchemaObjectScopeDependency {
                    source: object,
                    target: moved,
                }
            })
            .collect::<Vec<_>>()
    };
    let deps = dependencies(&view).await;
    assert_eq!(deps.len(), 2);
    let renderer = driver.migration_renderer().unwrap();
    let rendered = renderer
        .map_schema_object_scope(ObjectKind::View, &source, &target, &exact, &deps)
        .unwrap()
        .unwrap();
    driver
        .execute(
            &handle,
            &renderer
                .render(&MigrationOperation::CreateView {
                    view: MigrationView {
                        schema: Some(target.clone()),
                        name: view.clone(),
                        definition: rendered.definition,
                    },
                })
                .unwrap()
                .sql,
        )
        .await
        .unwrap();
    let native = (
        sqlx::query_as::<_, (i64, String)>(&format!("SELECT value,label FROM {source}.{view}"))
            .fetch_one(&mut f.oracle)
            .await
            .unwrap(),
        sqlx::query_as::<_, (i64, String)>(&format!("SELECT value,label FROM {target}.{view}"))
            .fetch_one(&mut f.oracle)
            .await
            .unwrap(),
    );
    assert_eq!(native, ((23, "雪".into()), (93, "雪".into())));

    let catalog = dependencies(&collision).await;
    let refusal = renderer
        .map_schema_object_scope(ObjectKind::View, &source, &target, &teeter, &catalog)
        .expect_err("CTE shadows a proven physical relation");
    assert!(
        refusal.contains("CTE") && refusal.contains("ambiguous"),
        "{refusal}"
    );
    let original: i64 = sqlx::query_scalar(&format!("SELECT value FROM {source}.{collision}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(original, 112);
    let denied: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.VIEWS WHERE TABLE_SCHEMA=? AND TABLE_NAME=?",
    )
    .bind(&target)
    .bind(&collision)
    .fetch_one(&mut f.oracle)
    .await
    .unwrap();
    assert_eq!(denied, 0);
    let local = deps
        .iter()
        .position(|refd| refd.source.schema.as_deref() == Some(source.as_str()))
        .unwrap();
    let external = 1 - local;
    let mut misses = deps.clone();
    misses[local].target.schema = Some(third.clone());
    assert!(
        renderer
            .map_schema_object_scope(ObjectKind::View, &source, &target, &exact, &misses)
            .unwrap_err()
            .contains("Local MySQL")
    );
    let mut misses = deps.clone();
    misses[external].target.schema = Some(target.clone());
    assert!(
        renderer
            .map_schema_object_scope(ObjectKind::View, &source, &target, &exact, &misses)
            .unwrap_err()
            .contains("External MySQL")
    );
    let mut misses = deps.clone();
    misses[external].source.schema = None;
    assert!(
        renderer
            .map_schema_object_scope(ObjectKind::View, &source, &target, &exact, &misses)
            .unwrap_err()
            .contains("omitted an exact")
    );
    assert!(
        renderer
            .map_schema_object_scope(ObjectKind::View, &source, &source, &exact, &deps)
            .unwrap_err()
            .contains("distinct")
    );
    assert!(
        renderer
            .map_schema_object_scope(ObjectKind::View, &source, "", &exact, &deps)
            .unwrap_err()
            .contains("non-empty")
    );
    let concatenated = format!("{exact}; SELECT 1");
    assert!(
        renderer
            .map_schema_object_scope(ObjectKind::View, &source, &target, &concatenated, &deps)
            .unwrap_err()
            .contains("exactly one")
    );
    let damaged = format!("{exact} /*");
    assert!(
        renderer
            .map_schema_object_scope(ObjectKind::View, &source, &target, &damaged, &deps)
            .is_err()
    );
    let ambiguous_view = format!("dz_ambiguous_{id}");
    let ambiguous = format!(
        "SELECT baseline.v+other.v AS val FROM {base} AS baseline JOIN {third}.{base} AS other"
    );
    sqlx::raw_sql(&format!(
        "CREATE VIEW {source}.{ambiguous_view} AS {ambiguous}"
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    let both = dependencies(&ambiguous_view).await;
    assert_eq!(both.len(), 2);
    let mapped = renderer
        .map_schema_object_scope(ObjectKind::View, &source, &target, &ambiguous, &both)
        .expect("the actual local database has priority over the same-named external table")
        .unwrap();
    let applied = renderer
        .render(&MigrationOperation::CreateView {
            view: MigrationView {
                schema: Some(target.clone()),
                name: ambiguous_view.clone(),
                definition: mapped.definition,
            },
        })
        .unwrap();
    driver.execute(&handle, &applied.sql).await.unwrap();
    let native: i64 = sqlx::query_scalar(&format!("SELECT val FROM {source}.{ambiguous_view}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    let migrated: i64 = sqlx::query_scalar(&format!("SELECT val FROM {target}.{ambiguous_view}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!((native, migrated), (23, 93));
    let routine = format!("dz_view_dep_{id}");
    let routine_view = format!("dz_from_routine_{id}");
    let routine_body = format!("SELECT {source}.{routine}(v) AS result FROM {source}.{base}");
    sqlx::raw_sql(&format!(
        "CREATE FUNCTION {source}.{routine}(v INT) RETURNS INT DETERMINISTIC RETURN v+2;
        CREATE VIEW {source}.{routine_view} AS {routine_body}"
    ))
    .execute(&mut f.oracle)
    .await
    .unwrap();
    let arg = dependencies(&routine_view).await;
    assert!(
        arg.iter()
            .any(|dependency| dependency.source.kind == "function")
    );
    assert!(
        renderer
            .map_schema_object_scope(ObjectKind::View, &source, &target, &routine_body, &arg)
            .unwrap_err()
            .contains("only supports exact table/view")
    );
    let result: i32 = sqlx::query_scalar(&format!("SELECT result FROM {source}.{routine_view}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(result, 15);
    let real_proc = driver.dump_routines(&handle, &source).await.unwrap();
    assert!(real_proc.contains(&procedure));
    assert!(
        renderer
            .map_schema_object_scope(ObjectKind::Procedure, &source, &target, &real_proc, &[])
            .unwrap()
            .is_none()
    );
    driver.disconnect(handle).await.unwrap();
}
