use super::Fixture;
use datazen_driver_api::{DatabaseDriver, DriverError};
use datazen_driver_postgres::PostgresDriver;
use std::io::{Read, Seek, SeekFrom, Write};

#[tokio::test]
async fn exported_postgres_identity_statements_replay_on_real_server_and_keep_next_key_safe() {
    let mut fixture = Fixture::open().await;
    let driver = PostgresDriver::new();
    let schema = format!("dz_file_{}'report", uuid::Uuid::new_v4().simple());
    let table = "Original.Rows";
    let quoted_schema = driver.quote_ident(&schema);
    let name = format!("{quoted_schema}.{}", driver.quote_ident(table));
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {quoted_schema};
         CREATE TABLE {name} (id integer GENERATED ALWAYS AS IDENTITY (START WITH 5 INCREMENT BY 3) PRIMARY KEY, value text);"
    )).execute(&mut fixture.oracle).await.expect("disposable native identity table");

    let marker = "/*dz_identity_override*/";
    assert_eq!(
        driver.transfer_explicit_identity_insert_clause(),
        Some("OVERRIDING SYSTEM VALUE")
    );
    assert!(driver.transfer_sql_file_insert_batch_size() >= 2);
    let inserts = driver.render_transfer_sql_file_insert(
        &format!("INSERT INTO {name}(id,value) {marker} VALUES(20, 'imported row'), (11,'older row')"),marker)
        .expect("render actual SQL-file identity insert");
    let reseed = driver
        .render_transfer_identity_sequence_sync_sql(
            Some(&schema),
            table,
            &["id".into(), "value".into()],
        )
        .expect("render actual sequence reconciliation");
    assert_eq!(reseed.len(), 2);
    let mut file = tempfile::tempfile().expect("private export file");
    writeln!(file, "BEGIN;\n{inserts};").unwrap();
    for statement in reseed {
        writeln!(file, "{statement};").unwrap();
    }
    writeln!(file, "COMMIT;").unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut recovered = String::new();
    file.read_to_string(&mut recovered).unwrap();
    let db = driver
        .connect(&fixture.config)
        .await
        .expect("actual server consumes exported SQL");
    driver
        .query_multi(&db, &recovered, None)
        .await
        .expect("PG executes the full generated export");
    let actual: Vec<(i32, String)> =
        sqlx::query_as(&format!("SELECT id,value FROM {name} ORDER BY id"))
            .fetch_all(&mut fixture.oracle)
            .await
            .expect("independent PG inspect imported values");
    assert_eq!(
        actual,
        [(11, "older row".into()), (20, "imported row".into())]
    );
    let next: i32 = sqlx::query_scalar(&format!(
        "INSERT INTO {name}(value) VALUES('new') RETURNING id"
    ))
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("new key generated without reuse");
    assert_eq!(next, 23);
    assert!(matches!(
        driver.render_transfer_sql_file_insert("INSERT INTO t VALUES(1)", marker),
        Err(DriverError::Unsupported(_))
    ));
    driver.disconnect(db).await.expect("close import session");
    sqlx::query(&format!("DROP SCHEMA {quoted_schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .unwrap();
}

#[tokio::test]
async fn live_table_check_metadata_matches_postgres_catalog_and_constraint_behavior() {
    let mut fixture = Fixture::open().await;
    let driver = PostgresDriver::new();
    let name = format!("dz_check_{}", uuid::Uuid::new_v4().simple());
    let relation = format!("public.{name}");
    sqlx::query(&format!(
        "CREATE TABLE {relation}(id int PRIMARY KEY, CONSTRAINT positive CHECK (id > 0))"
    ))
    .execute(&mut fixture.oracle)
    .await
    .unwrap();
    let raw: String = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid = to_regclass($1) AND conname = 'positive'")
        .bind(&relation).fetch_one(&mut fixture.oracle).await.unwrap();
    let expected = raw
        .strip_prefix("CHECK (")
        .and_then(|s| s.strip_suffix(')'))
        .expect("native CHECK");
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("real catalog session");
    let schema = driver
        .get_table_schema(
            &handle,
            &name,
            fixture.config.database.as_deref().unwrap(),
            Some("public"),
        )
        .await
        .expect("real driver CHECK metadata");
    assert_eq!(schema.check_constraints.len(), 1);
    assert_eq!(schema.check_constraints[0].name, "positive");
    assert_eq!(schema.check_constraints[0].expression, expected);
    let error = sqlx::query(&format!("INSERT INTO {relation} VALUES(-1)"))
        .execute(&mut fixture.oracle)
        .await
        .expect_err("server must enforce original CHECK");
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23514")
    );
    assert_eq!(
        driver
            .execute(&handle, &format!("INSERT INTO {relation} VALUES(3)"))
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i32>(&format!("SELECT id FROM {relation}"))
            .fetch_one(&mut fixture.oracle)
            .await
            .unwrap(),
        3
    );
    driver.disconnect(handle).await.unwrap();
    sqlx::query(&format!("DROP TABLE {relation}"))
        .execute(&mut fixture.oracle)
        .await
        .unwrap();
}
