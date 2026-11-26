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
    let definition=format!("WITH selected AS (SELECT v FROM `{source}`.`{local}`)\nSELECT selected.v+extra.v AS val, '雨' AS label FROM selected CROSS JOIN `{external}`.`{dependency}` extra");
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
    assert!(mapping
        .definition
        .contains(&format!("`{target}`.`{mapped}`")));
    assert!(mapping
        .definition
        .contains(&format!("`{external}`.`{dependency}`")));
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
    assert!(renderer
        .map_schema_object_scope(ObjectKind::View, &source, &target, &definition, &deps[..1])
        .is_err());
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
