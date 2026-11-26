//! Heterogeneous transfer preflight against a real PostgreSQL source and MySQL target.
//! The coverage gate is scoped to the MySQL implementation.
use datazen_driver_api::{
    ColumnSchema, ConnectionConfig, DatabaseDriver, IRColumn, IRType, SyncAdapterFactory, Value,
    inventory,
};
use datazen_driver_mysql::MysqlDriver;
use serde_json::{Value as Json, json};
use sqlx::{
    Connection, MySqlConnection,
    mysql::{MySqlConnectOptions, MySqlSslMode},
};
use std::{env, process::Command};

fn var(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("missing live fixture field {name}"))
}

fn pg(sql: &str) -> String {
    let host = var("TEST_PG_HOST");
    let database = var("TEST_PG_DATABASE");
    let path = std::path::PathBuf::from(var("DATAZEN_MYSQL_E2E_PSQL"));
    assert_eq!(host, "127.0.0.1");
    assert_eq!(database, "dz_fixture_pg_a");
    assert!(path.is_absolute());
    let output = Command::new(path)
        .args([
            "-X",
            "--no-password",
            "-q",
            "-A",
            "-t",
            "-v",
            "ON_ERROR_STOP=1",
            "-h",
            &host,
            "-p",
            &var("TEST_PG_PORT"),
            "-U",
            &var("TEST_PG_USER"),
            "-d",
            &database,
            "-c",
            sql,
        ])
        .env("PGPASSWORD", var("TEST_PG_PASSWORD"))
        .output()
        .expect("run native psql tool");
    assert!(
        output.status.success(),
        "native Pg fixture command must succeed"
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[tokio::test]
#[ignore = "the cross-transfer coverage mode provisions both native fixtures"]
async fn postgres_catalog_types_preflight_actual_mysql_target_and_preserve_supported_rows() {
    assert_eq!(
        pg("SELECT marker FROM public.rune_e2e_fixture"),
        "datazen-rune-disposable"
    );
    let label = uuid::Uuid::new_v4().simple().to_string();
    let table = format!("dz_cross_transfer_{label}");
    let color = format!("dz_cross_color_{label}");
    pg(&format!("CREATE TYPE {color} AS ENUM ('green','blue');
      CREATE TABLE {table}(unbounded numeric, negative_scale numeric(10,-2), overwide numeric(70,7),
        exact_value numeric(40,12), occurred timestamp with time zone, event_clock time with time zone,
        native_only {color}, note text);
      INSERT INTO {table} VALUES (13,12345,10,12345678901234567890123456.000000000015,
        '2030-01-02 03:04:05+00:00','04:05:06+02:00','green',repeat('雪',24000))"));
    let metadata: Json = serde_json::from_str(&pg(&format!(
        "SELECT jsonb_object_agg(attname,format_type(atttypid,atttypmod)) FROM pg_attribute
          WHERE attrelid='{table}'::regclass AND attnum>0 AND NOT attisdropped"
    )))
    .unwrap();
    let values: Json = serde_json::from_str(&pg(&format!(
        "SELECT jsonb_build_object('exact',exact_value::text,'size',octet_length(note),
         'note',note,'negative',negative_scale::text,'native',native_only::text) FROM {table}"
    )))
    .unwrap();
    assert_eq!(values["negative"], "12300");
    assert_eq!(values["size"], 72000);
    assert_eq!(values["native"], "green");

    let host = var("TEST_MYSQL_HOST");
    let port: u16 = var("TEST_MYSQL_PORT").parse().unwrap();
    let catalog = var("TEST_MYSQL_DATABASE");
    let user = var("TEST_MYSQL_USER");
    let password = var("TEST_MYSQL_PASSWORD");
    assert_eq!(host, "127.0.0.1");
    assert!(catalog.starts_with("dz_fixture_mysql_"));
    let mut native = MySqlConnection::connect_with(
        &MySqlConnectOptions::new()
            .host(&host)
            .port(port)
            .database(&catalog)
            .username(&user)
            .password(&password)
            .ssl_mode(MySqlSslMode::Disabled),
    )
    .await
    .expect("native MySQL target");
    let proof: (String, String) =
        sqlx::query_as("SELECT @@version_comment,marker FROM rune_e2e_fixture")
            .fetch_one(&mut native)
            .await
            .unwrap();
    assert!(proof.0.contains("MySQL") && !proof.0.contains("MariaDB"));
    assert_eq!(proof.1, var("DATAZEN_MYSQL_E2E_MARKER"));
    let config:ConnectionConfig=serde_json::from_value(json!({
        "id":label, "name":"isolated native transfer target","databaseType":"mysql","host":host,"port":port,
        "username":user,"password":password,"database":catalog,"sslMode":"disable","connectionTimeout":5,"maxPoolSize":3
    })).unwrap();
    let target = MysqlDriver::new(false);
    let handle = target.connect(&config).await.unwrap();
    sqlx::query(&format!(
        "CREATE TABLE {table}(exact_value NUMERIC(40,12), note LONGTEXT) CHARACTER SET utf8mb4"
    ))
    .execute(&mut native)
    .await
    .unwrap();
    let columns = target
        .get_table_schema(&handle, &table, &catalog, None)
        .await
        .unwrap()
        .columns;
    let registered = inventory::iter::<SyncAdapterFactory>
        .into_iter()
        .find(|factory| factory.db_types.contains(&"mysql"))
        .unwrap();
    let adapter = (registered.create)().target;
    let specs = [
        (
            "unbounded",
            IRType::Decimal {
                precision: 0,
                scale: 0,
            },
            "unbounded",
        ),
        (
            "negative_scale",
            IRType::Decimal {
                precision: 10,
                scale: 0,
            },
            "negative scale",
        ),
        (
            "overwide",
            IRType::Decimal {
                precision: 70,
                scale: 7,
            },
            "exceeds MySQL DECIMAL capacity",
        ),
        (
            "occurred",
            IRType::Timestamp {
                with_timezone: true,
            },
            "no timezone-preserving",
        ),
        (
            "event_clock",
            IRType::Time {
                with_timezone: true,
            },
            "no timezone-preserving",
        ),
        (
            "native_only",
            IRType::Other(metadata["native_only"].as_str().unwrap().into()),
            "native name",
        ),
    ];
    let adapted = |name: &str, ty: IRType| {
        let column = ColumnSchema {
            name: name.into(),
            data_type: metadata[name].as_str().unwrap().into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        };
        let ir = IRColumn {
            name: name.into(),
            ir_type: ty,
            nullable: true,
            default_expr: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        };
        (column, ir)
    };
    for (name, ty, reason) in specs {
        let native_only = matches!(&ty, IRType::Other(_));
        let (column, ir) = adapted(name, ty);
        let error = adapter
            .validate_transfer_column_type(&column, &ir, None, false, native_only, None, true)
            .unwrap_err();
        assert!(error.contains(reason), "{name}: {error}");
    }
    let before: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&mut native)
        .await
        .unwrap();
    assert_eq!(before, 0);

    let (decimal, ir) = adapted(
        "exact_value",
        IRType::Decimal {
            precision: 40,
            scale: 12,
        },
    );
    assert!(
        adapter
            .validate_transfer_column_type(
                &decimal,
                &ir,
                None,
                false,
                false,
                Some(&columns[0].data_type),
                false
            )
            .is_ok()
    );
    assert!(
        adapter
            .validate_transfer_column_type(
                &decimal,
                &ir,
                None,
                false,
                false,
                Some("NUMERIC(40,12)"),
                false
            )
            .is_ok()
    );
    let (text, text_ir) = adapted("note", IRType::Text);
    let observed = values["size"].as_u64().unwrap();
    assert!(
        adapter
            .validate_transfer_column_type(
                &text,
                &text_ir,
                Some(observed),
                false,
                false,
                Some("TEXT"),
                false
            )
            .unwrap_err()
            .contains("holds at most")
    );
    assert!(
        adapter
            .validate_transfer_column_type(
                &text,
                &text_ir,
                Some(observed),
                false,
                false,
                Some(&columns[1].data_type),
                false
            )
            .is_ok()
    );
    let literals = [
        (values["exact"].as_str().unwrap(), &ir),
        (values["note"].as_str().unwrap(), &text_ir),
    ]
    .into_iter()
    .map(|(value, ir)| adapter.format_literal(&Some(Value::String(value.to_owned())), &ir.ir_type))
    .collect::<Vec<_>>()
    .join(",");
    target
        .execute(&handle, &format!("INSERT INTO {table} VALUES ({literals})"))
        .await
        .unwrap();
    let actual: (String, u64) = sqlx::query_as(&format!(
        "SELECT CAST(exact_value AS CHAR),CAST(OCTET_LENGTH(note) AS UNSIGNED) FROM {table}"
    ))
    .fetch_one(&mut native)
    .await
    .unwrap();
    assert_eq!(actual, (values["exact"].as_str().unwrap().into(), observed));
    target.disconnect(handle).await.unwrap();
}
