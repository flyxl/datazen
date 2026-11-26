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
