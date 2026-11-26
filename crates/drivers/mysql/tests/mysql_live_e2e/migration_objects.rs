use super::Fixture;
use datazen_driver_api::{
    schema_objects::ObjectKind, sql_split::split_sql_statements, ConnectionHandle, DatabaseDriver,
    MigrationOperation as Op, MigrationRenderer, MigrationRoutine, MigrationStatement,
    MigrationTrigger, MigrationView, TableOptions,
};
use datazen_driver_mysql::MysqlDriver;

async fn apply(driver: &MysqlDriver, handle: &ConnectionHandle, sql: &str) {
    for statement in split_sql_statements(sql) {
        driver
            .execute(handle, &statement)
            .await
            .unwrap_or_else(|e| panic!("native MySQL migration SQL failed: {statement}: {e}"));
    }
}
fn render(renderer: &dyn MigrationRenderer, op: Op) -> MigrationStatement {
    renderer
        .render(&op)
        .unwrap_or_else(|e| panic!("render live MySQL operation {op:?}: {e}"))
}

#[tokio::test]
async fn mysql_migration_replaces_and_restores_live_routine_trigger_and_view() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let renderer = driver.migration_renderer().expect("native renderer");
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let (table, view, fun, trg) = (
        format!("dz_ob_t_{tag}"),
        format!("dz_ob_v_{tag}"),
        format!("dz_ob_f_{tag}"),
        format!("dz_ob_g_{tag}"),
    );
    let schema = Some(f.database.clone());
    sqlx::query(&format!(
        "CREATE TABLE {table}(id int primary key, cost int not null)"
    ))
    .execute(&mut f.oracle)
    .await
    .expect("real target relations");
    let v = |scale: i32| MigrationView {
        schema: schema.clone(),
        name: view.clone(),
        definition: format!("SELECT id, cost * {scale} AS displayed FROM {table}"),
    };
    let r = |offset: i32| MigrationRoutine {
        kind: ObjectKind::Function,
        schema: schema.clone(),
        name: fun.clone(),
        signature: None,
        definition: format!(
            "CREATE FUNCTION `{}`.`{fun}`(n INT) RETURNS INT DETERMINISTIC RETURN n + {offset}",
            f.database
        ),
    };
    let t = |offset: i32| {
        MigrationTrigger{schema:schema.clone(),name:trg.clone(),target_schema:schema.clone(),target_name:table.clone(),
        definition:format!("CREATE TRIGGER `{}`.`{trg}` BEFORE INSERT ON `{}`.`{table}` FOR EACH ROW SET NEW.cost = NEW.cost + {offset}",f.database,f.database)}
    };
    for op in [
        Op::CreateView { view: v(2) },
        Op::CreateRoutine { routine: r(4) },
        Op::CreateTrigger { trigger: t(5) },
    ] {
        let stmt = render(renderer.as_ref(), op);
        apply(&driver, &handle, &stmt.sql).await;
    }
    let verify = async |oracle: &mut sqlx::MySqlConnection,
                        id: i32,
                        stored: i32,
                        displayed: i32,
                        routine: i32| {
        sqlx::query(&format!("INSERT INTO {table} VALUES ({id}, 1)"))
            .execute(&mut *oracle)
            .await
            .expect("actual trigger called");
        let observed:(i32,i32,i32)=sqlx::query_as(&format!("SELECT t.cost,v.displayed,{fun}(1) FROM {table} t JOIN {view} v USING(id) WHERE id={id}"))
            .fetch_one(oracle).await.expect("native migrated behavior");
        assert_eq!(observed, (stored, displayed, routine));
    };
    verify(&mut f.oracle, 1, 6, 12, 5).await;
    let replaces = [
        render(
            renderer.as_ref(),
            Op::ReplaceView {
                current: v(2),
                desired: v(3),
            },
        ),
        render(
            renderer.as_ref(),
            Op::ReplaceRoutine {
                current: r(4),
                desired: r(8),
            },
        ),
        render(
            renderer.as_ref(),
            Op::ReplaceTrigger {
                current: t(5),
                desired: t(2),
            },
        ),
    ];
    for stmt in &replaces {
        apply(&driver, &handle, &stmt.sql).await;
    }
    verify(&mut f.oracle, 2, 3, 9, 9).await;
    for stmt in replaces.iter().rev() {
        apply(
            &driver,
            &handle,
            stmt.rollback_sql.as_deref().expect("reviewed reverse DDL"),
        )
        .await;
    }
    verify(&mut f.oracle, 3, 6, 12, 5).await;
    let drops = [
        render(renderer.as_ref(), Op::DropView { view: v(2) }),
        render(renderer.as_ref(), Op::DropRoutine { routine: r(4) }),
        render(renderer.as_ref(), Op::DropTrigger { trigger: t(5) }),
    ];
    for stmt in &drops {
        apply(&driver, &handle, &stmt.sql).await;
    }
    assert!(sqlx::query(&format!("SELECT * FROM {view}"))
        .fetch_one(&mut f.oracle)
        .await
        .is_err());
    assert!(sqlx::query(&format!("SELECT {fun}(1)"))
        .fetch_one(&mut f.oracle)
        .await
        .is_err());
    for stmt in drops.iter().rev() {
        apply(
            &driver,
            &handle,
            stmt.rollback_sql.as_deref().expect("reviewed restore DDL"),
        )
        .await;
    }
    verify(&mut f.oracle, 4, 6, 12, 5).await;
    driver
        .disconnect(handle)
        .await
        .expect("release MySQL connection");
}

#[tokio::test]
async fn mysql_migration_round_trips_native_table_options_and_default_changes() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let renderer = driver.migration_renderer().expect("native renderer");
    let table = format!("dz_options_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!("CREATE TABLE {table}(id INT NULL, amount INT NOT NULL DEFAULT 7) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='old'; INSERT INTO {table} (id) VALUES (1)"))
        .execute(&mut f.oracle).await.expect("real original table options and rows");
    let before = TableOptions {
        engine: Some("InnoDB".into()),
        charset: Some("utf8mb4".into()),
        comment: Some("old".into()),
        ..Default::default()
    };
    let after = TableOptions {
        engine: Some("MyISAM".into()),
        charset: Some("latin1".into()),
        comment: Some("editor's comment".into()),
        ..Default::default()
    };
    let show = async |oracle: &mut sqlx::MySqlConnection| {
        sqlx::query_as::<_,(String,String,String)>("SELECT CAST(ENGINE AS CHAR),CAST(TABLE_COLLATION AS CHAR),CAST(TABLE_COMMENT AS CHAR) FROM information_schema.TABLES WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=?")
         .bind(&table).fetch_one(oracle).await.expect("actual physical table options")
    };
    let sql = render(
        renderer.as_ref(),
        Op::SetTableOptions {
            table: table.clone(),
            from: before.clone(),
            to: after.clone(),
        },
    );
    apply(&driver, &handle, &sql.sql).await;
    let current = show(&mut f.oracle).await;
    assert_eq!(current.0, "MyISAM");
    assert!(current.1.starts_with("latin1_"));
    assert_eq!(current.2, "editor's comment");
    apply(
        &driver,
        &handle,
        sql.rollback_sql
            .as_deref()
            .expect("native options rollback"),
    )
    .await;
    let current = show(&mut f.oracle).await;
    assert_eq!(current.0, "InnoDB");
    assert!(current.1.starts_with("utf8mb4_"));
    assert_eq!(current.2, "old");
    for op in [
        Op::AddPrimaryKey {
            table: table.clone(),
            columns: vec!["id".into()],
        },
        Op::SetAutoIncrement {
            table: table.clone(),
            column: "id".into(),
            from: false,
            to: true,
        },
        Op::SetDefault {
            table: table.clone(),
            column: "amount".into(),
            from: Some("7".into()),
            to: Some("11".into()),
        },
    ] {
        apply(&driver, &handle, &render(renderer.as_ref(), op).sql).await;
    }
    sqlx::query(&format!("INSERT INTO {table} () VALUES ()"))
        .execute(&mut f.oracle)
        .await
        .expect("real increment and default");
    let actual: Vec<(i32, i32)> =
        sqlx::query_as(&format!("SELECT id,amount FROM {table} ORDER BY id"))
            .fetch_all(&mut f.oracle)
            .await
            .expect("native old and new persisted defaults");
    assert_eq!(actual, [(1, 7), (2, 11)]);
    let stmt = render(
        renderer.as_ref(),
        Op::SetDefault {
            table: table.clone(),
            column: "amount".into(),
            from: Some("11".into()),
            to: None,
        },
    );
    apply(&driver, &handle, &stmt.sql).await;
    let current:Option<String>=sqlx::query_scalar("SELECT CAST(COLUMN_DEFAULT AS CHAR) FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=? AND COLUMN_NAME='amount'")
        .bind(&table).fetch_one(&mut f.oracle).await.expect("native removed default");
    assert_eq!(current, None);
    driver
        .disconnect(handle)
        .await
        .expect("release MySQL connection");
}

#[tokio::test]
async fn mysql_migration_replays_prefix_fulltext_spatial_hash_and_removes_owned_objects() {
    use datazen_driver_api::{CheckConstraint, IndexInfo, MigrationColumn};
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let renderer = driver.migration_renderer().expect("mysql DDL renderer");
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let indexed = format!("dz_options_ix_{tag}");
    let memory = format!("dz_options_mem_{tag}");
    let check = CheckConstraint {
        name: format!("ck_ix_{tag}"),
        expression: "amount >= 0".into(),
    };
    sqlx::raw_sql(&format!(
        "CREATE TABLE {indexed}(id INT NOT NULL PRIMARY KEY, body TEXT, tag VARCHAR(240),
           position POINT NOT NULL, amount INT NOT NULL, old_note INT NULL, CONSTRAINT {} CHECK(amount>=0)) ENGINE=InnoDB;
         CREATE TABLE {memory}(score INT NOT NULL PRIMARY KEY) ENGINE=MEMORY;
         INSERT INTO {indexed} VALUES(1,'thunderstone lantern','prefixable',ST_GeomFromText('POINT(2 3)'),1,4);
         INSERT INTO {memory} VALUES(2)",check.name))
        .execute(&mut f.oracle).await.expect("real InnoDB & MEMORY tables");
    let idx = |name: &str, method: &str, column: &str| IndexInfo {
        name: name.into(),
        index_type: method.into(),
        columns: vec![column.into()],
        is_primary: false,
        is_unique: false,
    };
    for (table, index) in [
        (&indexed, idx("prefix_ix", "BTREE", "tag(12)")),
        (&indexed, idx("book_ix", "FULLTEXT", "body")),
        (&indexed, idx("map_ix", "RTREE", "position")),
        (&memory, idx("memory_ix", "HASH", "score")),
    ] {
        apply(
            &driver,
            &handle,
            &render(
                renderer.as_ref(),
                Op::CreateIndex {
                    table: table.clone(),
                    index,
                },
            )
            .sql,
        )
        .await;
    }
    let catalog:Vec<(String,String,Option<u64>)>=sqlx::query_as(
        "SELECT CAST(INDEX_NAME AS CHAR),CAST(INDEX_TYPE AS CHAR),CAST(SUB_PART AS UNSIGNED) FROM information_schema.STATISTICS
         WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=? ORDER BY 1"
    ).bind(&indexed).fetch_all(&mut f.oracle).await.expect("independent MySQL index options");
    assert!(catalog
        .iter()
        .any(|(n, t, s)| n == "prefix_ix" && t == "BTREE" && *s == Some(12)));
    assert!(catalog
        .iter()
        .any(|(n, t, _)| n == "book_ix" && t == "FULLTEXT"));
    let spatial = catalog
        .iter()
        .find(|(n, _, _)| n == "map_ix")
        .expect("actual spatial catalog");
    assert!(["RTREE", "SPATIAL"].contains(&spatial.1.as_str()));
    let hash:String=sqlx::query_scalar(
        "SELECT CAST(INDEX_TYPE AS CHAR) FROM information_schema.STATISTICS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=? AND INDEX_NAME='memory_ix'"
    ).bind(&memory).fetch_one(&mut f.oracle).await.expect("native memory index");
    assert_eq!(hash, "HASH");
    let search: i32 = sqlx::query_scalar(&format!(
        "SELECT id FROM {indexed} WHERE MATCH(body) AGAINST('+thunderstone' IN BOOLEAN MODE)"
    ))
    .fetch_one(&mut f.oracle)
    .await
    .expect("real full-text reading");
    assert_eq!(search, 1);

    let drop_check = render(
        renderer.as_ref(),
        Op::DropCheckConstraint {
            table: indexed.clone(),
            constraint: check,
        },
    );
    apply(&driver, &handle, &drop_check.sql).await;
    sqlx::query(&format!("UPDATE {indexed} SET amount=-1 WHERE id=1"))
        .execute(&mut f.oracle)
        .await
        .expect("engine constraint removed");
    sqlx::query(&format!("UPDATE {indexed} SET amount=10 WHERE id=1"))
        .execute(&mut f.oracle)
        .await
        .expect("make old data valid again");
    apply(
        &driver,
        &handle,
        drop_check
            .rollback_sql
            .as_deref()
            .expect("actual reverse constraint"),
    )
    .await;
    assert!(
        sqlx::query(&format!("UPDATE {indexed} SET amount=-1 WHERE id=1"))
            .execute(&mut f.oracle)
            .await
            .is_err()
    );

    for op in [
        Op::AlterColumnType {
            table: indexed.clone(),
            column: "amount".into(),
            from: "INT NOT NULL".into(),
            to: "BIGINT NOT NULL".into(),
        },
        Op::DropColumn {
            table: indexed.clone(),
            column: MigrationColumn {
                name: "old_note".into(),
                data_type: "INT".into(),
                nullable: true,
                default_value: None,
                comment: None,
                is_auto_increment: false,
            },
        },
        Op::DropIndex {
            table: indexed.clone(),
            index: idx("prefix_ix", "BTREE", "tag(12)"),
        },
        Op::DropPrimaryKey {
            table: indexed.clone(),
            columns: vec!["id".into()],
        },
    ] {
        apply(&driver, &handle, &render(renderer.as_ref(), op).sql).await;
    }
    let remaining = driver
        .get_table_schema(&handle, &indexed, &f.database, None)
        .await
        .expect("native after ALTERs");
    assert!(remaining.primary_keys.is_empty());
    assert!(!remaining.columns.iter().any(|c| c.name == "old_note"));
    assert!(remaining
        .columns
        .iter()
        .any(|c| c.name == "amount" && c.data_type.starts_with("bigint")));
    assert!(!remaining.indexes.iter().any(|i| i.name == "prefix_ix"));
    for table in [indexed, memory] {
        apply(
            &driver,
            &handle,
            &render(
                renderer.as_ref(),
                Op::DropTable {
                    table: table.clone(),
                },
            )
            .sql,
        )
        .await;
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.TABLES WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=?")
            .bind(table).fetch_one(&mut f.oracle).await.expect("actual target destroyed only inside generated fixture");
        assert_eq!(count, 0);
    }
    driver
        .disconnect(handle)
        .await
        .expect("release isolated MySQL connection");
}
