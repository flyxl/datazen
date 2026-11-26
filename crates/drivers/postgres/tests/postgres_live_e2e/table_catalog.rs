use super::Fixture;
use datazen_driver_api::{DatabaseDriver, DriverError};
use datazen_driver_postgres::PostgresDriver;
use serde_json::{json, Value};

#[tokio::test]
async fn table_ddl_and_catalog_introspection_use_exact_live_postgres_identity() {
    let mut fixture = Fixture::open().await;
    let database = fixture
        .config
        .database
        .as_deref()
        .expect("fixture database");
    let id = uuid::Uuid::new_v4().simple().to_string();
    let desired = format!("a_ddl_{id}");
    let decoy = format!("z_ddl_{id}");
    let chosen = format!("{desired}.same_name");
    let unrelated = format!("{decoy}.same_name");
    let quoted = r#""mixed.case""table""#;
    sqlx::raw_sql(&format!(r#"
      CREATE SCHEMA {desired}; CREATE SCHEMA {decoy};
      CREATE TABLE {desired}.parents (id integer PRIMARY KEY);
      CREATE UNLOGGED TABLE {chosen} (
         id integer NOT NULL, parent integer, note varchar(70) COLLATE "C" DEFAULT 'marked',
         CONSTRAINT selected_pkey PRIMARY KEY (id), CONSTRAINT fk_parent FOREIGN KEY(parent) REFERENCES {desired}.parents(id),
         CONSTRAINT check_id CHECK(id > 0)
      );
      CREATE UNIQUE INDEX visible_note ON {chosen}(note) WHERE parent IS NOT NULL;
      COMMENT ON COLUMN {chosen}.note IS 'selected origin';
      INSERT INTO {desired}.parents VALUES (1); INSERT INTO {chosen} VALUES (2,1,'kept');
      CREATE TABLE {unrelated} (decoy text PRIMARY KEY);
      INSERT INTO {unrelated} VALUES('ignore this row');
      CREATE TABLE {desired}.{quoted} (exact_catalog int DEFAULT 33);
      CREATE TABLE {desired}.identity_table (id integer GENERATED ALWAYS AS IDENTITY);
      CREATE VIEW {desired}.wrong_kind AS SELECT id FROM {chosen};
    "#))
    .execute(&mut fixture.oracle)
    .await
    .expect("isolated marked PostgreSQL fixture");

    let driver = PostgresDriver::new();
    let handle = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("retained driver session");
    driver
        .execute(&handle, &format!("SET search_path TO {decoy}, public"))
        .await
        .expect("favor the same-name decoy in ambient path");
    let req = |schema: &str, name: &str| json!({"kind":"table","name":name,"schema":schema,"database":database});
    let ddl = driver
        .execute_command(&handle, "get_object_ddl", req(&desired, "same_name"))
        .await
        .expect("exact table DDL is readable")
        .data;
    let ddl = ddl["ddl"].as_str().expect("table DDL text");
    assert!(ddl.starts_with(&format!("CREATE UNLOGGED TABLE {chosen}")));
    assert!(ddl.contains("note character varying(70) COLLATE pg_catalog.\"C\" DEFAULT"));
    assert!(ddl.contains("CONSTRAINT selected_pkey PRIMARY KEY (id)"));
    assert!(ddl.contains(&format!(
        "CONSTRAINT fk_parent FOREIGN KEY (parent) REFERENCES {desired}.parents(id)"
    )));
    assert!(
        ddl.contains("CONSTRAINT check_id CHECK")
            && ddl.contains("CREATE UNIQUE INDEX visible_note")
    );
    assert_eq!(ddl.matches("selected_pkey").count(), 1);

    let decoy_ddl = driver
        .execute_command(&handle, "get_object_ddl", req(&decoy, "same_name"))
        .await
        .expect("same-name table only when explicit schema selected")
        .data;
    assert!(decoy_ddl["ddl"]
        .as_str()
        .unwrap_or_default()
        .contains("decoy text"));
    let exact = driver
        .execute_command(
            &handle,
            "get_object_ddl",
            req(&desired, r#"mixed.case"table"#),
        )
        .await
        .expect("literal dot and quote in an exact table identity")
        .data;
    assert!(exact["ddl"]
        .as_str()
        .unwrap_or_default()
        .contains("exact_catalog integer DEFAULT 33"));
    for unstable in ["identity_table", "wrong_kind", "nothing_here"] {
        assert!(matches!(
            driver
                .execute_command(&handle, "get_object_ddl", req(&desired, unstable))
                .await,
            Err(DriverError::Unsupported(_))
        ));
    }
    for malformed in [
        json!({"kind":"table","name":"same_name"}),
        json!({"kind":"table","schema":desired,"name":"  "}),
    ] {
        assert!(matches!(
            driver
                .execute_command(&handle, "get_object_ddl", malformed)
                .await,
            Err(DriverError::InvalidConfig(_))
        ));
    }
    let hidden = driver
        .get_columns(&handle, "same_name", database, Some("public"))
        .await;
    assert!(
        hidden.is_err(),
        "wrong schema must not fall back to any same-name relation"
    );
    let (columns, pk) = driver
        .get_columns(&handle, "same_name", database, Some(&desired))
        .await
        .expect("explicit column metadata");
    assert_eq!(pk, ["id"]);
    assert_eq!(
        columns
            .iter()
            .map(|col| col.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "parent", "note"]
    );
    assert_eq!(columns[2].comment.as_deref(), Some("selected origin"));
    let origin = sqlx::query_as::<_,(String,String)>(
        "SELECT format_type(atttypid,atttypmod),col_description(attrelid,attnum) FROM pg_attribute WHERE attrelid=to_regclass($1) AND attname='note'"
    ).bind(&chosen).fetch_one(&mut fixture.oracle).await.expect("independent type and comment oracle");
    assert_eq!(
        origin,
        ("character varying(70)".into(), "selected origin".into())
    );
    let every = driver
        .get_all_columns(&handle, database, None)
        .await
        .expect("bulk physical catalog");
    assert_eq!(
        every["same_name"]
            .0
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "parent", "note"]
    );
    let second = driver
        .get_all_columns(&handle, database, Some(&decoy))
        .await
        .expect("decoy batch scope");
    assert_eq!(
        second["same_name"]
            .0
            .iter()
            .map(|column| column.name.as_str())
            .collect::<Vec<_>>(),
        ["decoy"]
    );

    driver
        .execute(
            &handle,
            r#"CREATE TEMP TABLE "Temp.WithDot" (local_id integer PRIMARY KEY)"#,
        )
        .await
        .expect("temporary relation in this retained session");
    let exact_session = driver
        .execute_command(
            &handle,
            "get_table_schema",
            json!({"relationSql":r#""Temp.WithDot""#}),
        )
        .await
        .expect("exact raw relation resolved on retained PG backend")
        .data;
    assert_eq!(exact_session["resolvedTarget"]["database"], database);
    assert_eq!(exact_session["resolvedTarget"]["table"], "Temp.WithDot");
    assert_eq!(exact_session["resolvedTarget"]["sessionScoped"], true);
    assert!(exact_session["resolvedTarget"]["schema"]
        .as_str()
        .is_some_and(|schema| schema.starts_with("pg_temp_")));
    let persistent = driver
        .execute_command(
            &handle,
            "get_table_schema",
            json!({
                "relationSql": format!("\"{database}\".\"{desired}\".{quoted}")
            }),
        )
        .await
        .expect("three-part named relation on exact active catalog")
        .data;
    assert_eq!(
        persistent["resolvedTarget"],
        json!({
            "database":database,"catalog":Value::Null,"schema":desired,"table":r#"mixed.case"table"#,
            "path":[],"sessionScoped":false
        })
    );
    let independent_temp: Option<String> =
        sqlx::query_scalar(r#"SELECT to_regclass('"Temp.WithDot"')::text"#)
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("unrelated backend cannot see temp");
    assert!(independent_temp.is_none());
    let source_rows: Vec<(i32, Option<i32>, String)> =
        sqlx::query_as(&format!("SELECT * FROM {chosen}"))
            .fetch_all(&mut fixture.oracle)
            .await
            .expect("real persistent source row");
    assert_eq!(source_rows, [(2, Some(1), "kept".into())]);
    let decoy_rows: Vec<String> = sqlx::query_scalar(&format!("SELECT decoy FROM {unrelated}"))
        .fetch_all(&mut fixture.oracle)
        .await
        .expect("unselected same-name table remains untouched");
    assert_eq!(decoy_rows, ["ignore this row"]);
    driver
        .disconnect(handle)
        .await
        .expect("retained session cleanup");
    sqlx::raw_sql(&format!(
        "DROP SCHEMA {desired} CASCADE; DROP SCHEMA {decoy} CASCADE"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("remove exact catalog fixtures");
}

#[tokio::test]
async fn postgres_result_origin_reads_real_base_view_and_materialized_catalog_kinds() {
    use datazen_driver_api::{ResultObjectKind, ResultOrigin, ResultShape, SqlTarget, Value};
    let mut fixture = Fixture::open().await;
    let database = fixture.config.database.as_deref().expect("fixture");
    let schema = format!("dz_origin_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        r#"
         CREATE SCHEMA {schema};
         CREATE TABLE {schema}.records(id integer PRIMARY KEY,label text);
         INSERT INTO {schema}.records VALUES(1,'durable');
         CREATE VIEW {schema}.projected AS SELECT * FROM {schema}.records;
         CREATE MATERIALIZED VIEW {schema}.cached AS SELECT * FROM {schema}.records;
         UPDATE {schema}.records SET label='live';
    "#
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("real relations with independent materialized snapshot");
    let expected_catalog:Vec<(String,String)>=sqlx::query_as(
        "SELECT c.relname::text,c.relkind::text FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relkind IN ('r','v','m') ORDER BY c.relname"
    ).bind(&schema).fetch_all(&mut fixture.oracle).await.expect("actual exact PG catalog identity");
    assert_eq!(
        expected_catalog,
        [
            ("cached".into(), "m".into()),
            ("projected".into(), "v".into()),
            ("records".into(), "r".into())
        ]
    );
    let driver = PostgresDriver::new();
    let handle = driver.connect(&fixture.config).await.expect("driver query");
    let target = SqlTarget::new(Some(database), Some(&schema));
    for (name, kind, expected) in [
        ("records", ResultObjectKind::BaseTable, "live"),
        ("projected", ResultObjectKind::View, "live"),
        ("cached", ResultObjectKind::MaterializedView, "durable"),
    ] {
        let statement =
            format!("SELECT id AS row_id, label, length(label) AS computed FROM {schema}.{name}");
        let query = driver
            .query_at(&handle, &statement, target)
            .await
            .expect("real executed result rows");
        assert_eq!(query.rows[0][1], Some(Value::String(expected.into())));
        let names = query
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>();
        let described = driver
            .describe_result_origin(&handle, target, &names, &statement)
            .await
            .expect("catalog-based origin");
        let ResultOrigin::Determined(origin) = described else {
            panic!("actual result should have origin")
        };
        assert_eq!(origin.shape, ResultShape::SingleBaseTable);
        assert_eq!(
            origin.object_kind, kind,
            "actual PostgreSQL relation kind {name}"
        );
        assert_eq!(
            origin
                .table
                .as_ref()
                .expect("physical object identity")
                .name,
            name
        );
        assert_eq!(origin.columns[0].source_column.as_deref(), Some("id"));
        assert!(origin.columns[0].passthrough);
        assert!(!origin.columns[2].passthrough);
    }
    for (name, command) in [
        ("cached", "CREATE MATERIALIZED VIEW"),
        ("projected", "CREATE OR REPLACE VIEW"),
    ] {
        let ddl = driver
            .dump_view_ddl(&handle, name, database, Some(&schema))
            .await
            .expect("same catalog preserves original relation kind when exporting DDL");
        assert!(ddl.starts_with(command));
    }
    driver.disconnect(handle).await.expect("release driver");
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("dispose exact catalog");
}
