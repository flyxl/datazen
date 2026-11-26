use super::Fixture;
use datazen_driver_api::{BackupDumpOptions, DatabaseDriver, DriverError, Value};
use serde_json::json;
use sqlx::{
    mysql::{MySqlConnectOptions, MySqlSslMode},
    Connection, MySqlConnection,
};

#[tokio::test]
async fn mysql_object_browser_reads_and_replays_server_ddl() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let table = format!("dz_orders_{id}");
    let view = format!("dz_order_view_{id}");
    let function = format!("dz_cents_{id}");
    let procedure = format!("dz_report_{id}");
    let trigger = format!("dz_upper_{id}");
    for sql in [
        format!("CREATE TABLE {table}(id INT PRIMARY KEY AUTO_INCREMENT, payload VARCHAR(60) NOT NULL, price DECIMAL(20,4) NOT NULL DEFAULT 3, CONSTRAINT check_price CHECK (price >= 0), INDEX price_idx (price)) ENGINE=InnoDB COMMENT='order table'"),
        format!("CREATE VIEW {view} AS SELECT id, payload FROM {table}"),
        format!("CREATE FUNCTION {function}(x INT) RETURNS INT DETERMINISTIC RETURN x * 100"),
        format!("CREATE PROCEDURE {procedure}() SELECT count(*) FROM {table}"),
        format!("CREATE TRIGGER {trigger} BEFORE INSERT ON {table} FOR EACH ROW SET NEW.payload=UPPER(NEW.payload)"),
        format!("INSERT INTO {table}(payload) VALUES ('initial')"),
    ] {sqlx::raw_sql(&sql).execute(&mut f.oracle).await.expect("actual source database object");}

    for (kind, name) in [
        ("view", &view),
        ("function", &function),
        ("procedure", &procedure),
        ("trigger", &trigger),
    ] {
        let list = driver
            .execute_command(
                &handle,
                "list_objects",
                json!({"kind":kind,"database":f.database}),
            )
            .await
            .expect("native object browser list");
        assert!(
            list.data.to_string().contains(name),
            "{kind} is present in actual catalog"
        );
        let shown = driver
            .execute_command(
                &handle,
                "get_object_ddl",
                json!({"kind":kind,"database":f.database,"name":name}),
            )
            .await
            .expect("real object DDL through the browser command");
        let ddl = shown.data["ddl"].as_str().expect("native object DDL text");
        if kind == "view" {
            // The schema-object API separates the body from view metadata.
            assert!(ddl.contains(&table) && shown.data["viewMetadata"].is_object());
        } else {
            assert!(
                ddl.contains(name),
                "actual {kind} DDL owns the expected name"
            );
        }
    }
    let table_ddl = driver
        .dump_table_ddl(&handle, &table, &f.database, None)
        .await
        .expect("actual table DDL");
    assert!(
        table_ddl.contains("AUTO_INCREMENT")
            && table_ddl.contains("check_price")
            && table_ddl.contains("price_idx")
    );
    let table_object = driver
        .execute_command(
            &handle,
            "get_object_ddl",
            json!({"kind":"table","name":table,"database":f.database}),
        )
        .await
        .expect("native table object");
    assert!(table_object.data["ddl"]
        .as_str()
        .is_some_and(|ddl| ddl.contains("order table")));
    let view_ddl = driver
        .dump_view_ddl(&handle, &view, &f.database, None)
        .await
        .expect("native view DDL");
    assert!(view_ddl.contains(&view));
    let routines = driver
        .dump_routines(&handle, &f.database)
        .await
        .expect("native routines output");
    let triggers = driver
        .dump_triggers(&handle, &f.database)
        .await
        .expect("native trigger output");
    assert!(routines.contains(&function) && routines.contains(&procedure));
    assert!(triggers.contains(&trigger));
    let mut progress = Vec::new();
    let dump = driver
        .dump_database_with_progress(
            &handle,
            &f.database,
            &BackupDumpOptions {
                schema_only: true,
                create_database: true,
                routines: true,
                triggers: true,
                ..Default::default()
            },
            &mut |event| progress.push(event),
        )
        .await
        .expect("native schema-only database backup");
    assert!(
        dump.contains(&table)
            && dump.contains(&view)
            && dump.contains(&trigger)
            && !dump.contains("INSERT INTO")
    );
    assert!(progress
        .iter()
        .any(|event| event.object_name.contains(&table)));

    let other = std::env::var("TEST_MYSQL_DATABASE_B").expect("fixture has second catalog");
    assert_ne!(other, f.database);
    let mut restore = MySqlConnection::connect_with(
        &MySqlConnectOptions::new()
            .host("127.0.0.1")
            .port(f.cfg.port.expect("port"))
            .database(&other)
            .username(f.cfg.username.as_deref().expect("fixture user"))
            .password(f.cfg.password.as_deref().expect("fixture password"))
            .ssl_mode(MySqlSslMode::Disabled),
    )
    .await
    .expect("independent restore target");
    sqlx::raw_sql(&table_ddl)
        .execute(&mut restore)
        .await
        .expect("replay generated source DDL");
    sqlx::query(&format!("INSERT INTO {table}(payload) VALUES ('restored')"))
        .execute(&mut restore)
        .await
        .expect("restore created native table");
    let actual: (i32, String) =
        sqlx::query_as(&format!("SELECT id,CAST(price AS CHAR) FROM {table}"))
            .fetch_one(&mut restore)
            .await
            .expect("restored schema applies real defaults");
    assert_eq!(actual, (2, "3.0000".into()));
    let untouched: String = sqlx::query_scalar(&format!("SELECT payload FROM {table}"))
        .fetch_one(&mut f.oracle)
        .await
        .expect("independent original unchanged");
    assert_eq!(untouched, "INITIAL");
    driver
        .disconnect(handle)
        .await
        .expect("close native session");
}

#[tokio::test]
async fn mysql_explain_reports_the_real_plan_and_retained_temporary_table() {
    let mut f = Fixture::open().await;
    let driver = datazen_driver_mysql::MysqlDriver::new(false);
    let handle = driver
        .connect_fixed_session(&f.cfg)
        .await
        .expect("retain real MySQL checkout");
    let table = format!("dz_plan_{}", uuid::Uuid::new_v4().simple());
    driver
        .execute(
            &handle,
            &format!("CREATE TEMPORARY TABLE {table} (id INT PRIMARY KEY, tag VARCHAR(10))"),
        )
        .await
        .expect("temporary table only visible on retained connection");
    driver
        .execute(
            &handle,
            &format!("INSERT INTO {table} VALUES (1,'one'),(2,'two')"),
        )
        .await
        .expect("seed real plan rows");
    assert!(sqlx::query(&format!("SELECT id FROM {table}"))
        .fetch_one(&mut f.oracle)
        .await
        .is_err());
    let plan = driver
        .explain(&handle, &format!("SELECT id FROM {table} WHERE id=1"))
        .await
        .expect("explain against same retained checkout");
    assert!(plan.plan_text.contains(&table) && plan.plan_text.contains("PRIMARY"));
    let json = plan.plan_json.expect("MySQL plan JSON adapter");
    assert!(json["columns"]
        .as_array()
        .is_some_and(|columns| columns.iter().any(|v| v == "possible_keys")));
    assert!(
        json["query_block"].is_object(),
        "native engine JSON format is preserved"
    );
    assert!(plan.estimated_rows.is_some_and(|rows| rows <= 2));
    assert!(matches!(
        driver.explain(&handle, "SELECT 1; SELECT 2").await,
        Err(DriverError::InvalidConfig(_))
    ));
    assert_eq!(
        driver
            .query(&handle, &format!("SELECT id FROM {table} ORDER BY id"))
            .await
            .expect("same backend after explain")
            .rows,
        [vec![Some(Value::Integer(1))], vec![Some(Value::Integer(2))]]
    );
    driver
        .disconnect(handle)
        .await
        .expect("release fixed checkout");
}

#[tokio::test]
async fn mysql_retained_metadata_distinguishes_real_relation_names_temp_state_and_composite_constraints(
) {
    let mut f = Fixture::open().await;
    let driver = datazen_driver_mysql::MysqlDriver::new(false);
    let handle = driver
        .connect_fixed_session(&f.cfg)
        .await
        .expect("own retained MySQL relation browsing");
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let parent = format!("dz_exact_parent_{tag}");
    let child = format!("dz_exact_child_{tag}");
    let fk = format!("fk_exact_{tag}");
    let ck = format!("ck_exact_{tag}");
    sqlx::raw_sql(&format!(
        "CREATE TABLE {parent}(x INT NOT NULL, y INT NOT NULL, PRIMARY KEY(x,y));
         CREATE TABLE {child}(id INT PRIMARY KEY, a INT NULL, b INT NULL, note VARCHAR(30) NOT NULL DEFAULT 'left, right',
           CONSTRAINT {fk} FOREIGN KEY(a,b) REFERENCES {parent}(x,y) ON UPDATE CASCADE ON DELETE SET NULL,
           CONSTRAINT {ck} CHECK ((char_length(note)>=1) AND note <> 'forbidden,)'));"
    )).execute(&mut f.oracle).await.expect("independent persisted real table metadata");
    let cfg_name = f.database.clone();
    let schema = driver
        .get_table_schema(&handle, &child, &cfg_name, None)
        .await
        .expect("real SHOW FULL / INDEX / CREATE parse");
    assert_eq!(schema.primary_keys, ["id"]);
    let constraint = schema
        .check_constraints
        .iter()
        .find(|x| x.name == ck)
        .expect("nested MySQL CHECK with quoted comma");
    assert!(constraint.expression.contains("forbidden,)"));
    let foreign = schema
        .foreign_keys
        .iter()
        .find(|x| x.name == fk)
        .expect("composite real FK parsed");
    assert_eq!(foreign.columns, ["a", "b"]);
    assert_eq!(foreign.referenced_columns, ["x", "y"]);
    assert_eq!(foreign.on_update, "CASCADE");
    assert_eq!(foreign.on_delete, "SET NULL");
    assert!(foreign.referenced_table.ends_with(&parent));
    let normalizer = driver
        .type_normalizer()
        .expect("native schema comparison normalizer");
    let id = schema.columns.iter().find(|c| c.name == "id").unwrap();
    assert_eq!(
        normalizer.normalize_type(&id.data_type),
        normalizer.normalize_type("INTEGER")
    );

    let literal = format!("live.dot_{tag}");
    let quoted = driver.quote_ident(&literal);
    let raw=format!("CREATE TEMPORARY TABLE {quoted} (temporary_id BIGINT PRIMARY KEY, note VARCHAR(20) DEFAULT 'kept')");
    driver
        .execute(&handle, &raw)
        .await
        .expect("real temp table with literal dot");
    let result = driver
        .execute_command(&handle, "get_table_schema", json!({"relationSql":quoted}))
        .await
        .expect("resolve actual retained scoped identity");
    assert_eq!(result.data["resolvedTarget"]["table"], literal);
    assert_eq!(result.data["resolvedTarget"]["database"], cfg_name);
    assert_eq!(result.data["resolvedTarget"]["sessionScoped"], true);
    assert_eq!(result.data["schema"]["columns"][0]["name"], "temporary_id");
    assert!(sqlx::query(&format!("SELECT temporary_id FROM {quoted}"))
        .fetch_one(&mut f.oracle)
        .await
        .is_err());

    let persisted = driver
        .execute_command(
            &handle,
            "get_table_schema",
            json!({"relationSql":format!("`{cfg_name}`.`{child}`"),"database":cfg_name}),
        )
        .await
        .expect("exact qualified persisted table");
    assert_eq!(persisted.data["resolvedTarget"]["table"], child);
    assert_eq!(persisted.data["resolvedTarget"]["sessionScoped"], false);
    for wrong in [
        format!("{child} JOIN {parent} ON a=x"),
        format!("(SELECT id FROM {child}) t"),
        format!("{child},{parent}"),
    ] {
        assert!(
            matches!(
                driver
                    .execute_command(
                        &handle,
                        "get_table_schema",
                        json!({"relationSql":wrong,"database":cfg_name})
                    )
                    .await,
                Err(DriverError::InvalidConfig(_))
            ),
            "join/derived/multi-source cannot identify one editable table"
        );
    }
    assert!(driver
        .get_table_schema(&handle, "dz_absent_for_mysql_metadata", &cfg_name, None)
        .await
        .is_err());
    driver
        .disconnect(handle)
        .await
        .expect("close and remove session-only objects");
}
