use super::Fixture;
use datazen_driver_api::{
    CheckConstraint, DatabaseDriver, ForeignKeyDeferrability, ForeignKeyInfo, IndexInfo,
    MigrationColumn, MigrationOperation as Op, StructureChangeMode, StructureChangeRequest,
    StructureColumnDraft, StructureIndexDraft,
};
use serde_json::json;

fn draft(value: serde_json::Value) -> StructureColumnDraft {
    serde_json::from_value(value).expect("test column draft")
}

fn index(value: serde_json::Value) -> StructureIndexDraft {
    serde_json::from_value(value).expect("test index draft")
}

#[tokio::test]
async fn mysql_structure_editor_plans_native_create_and_alter_then_reads_catalog() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let table = format!("dz_structure_{}", uuid::Uuid::new_v4().simple());
    let id = draft(
        json!({"id":"key","name":"id","dataType":"INT","nullable":false,"isPrimaryKey":true,"isAutoIncrement":true}),
    );
    let amount = draft(
        json!({"id":"amount","name":"amount","dataType":"INT","nullable":false,"defaultValue":"2"}),
    );
    let note = draft(
        json!({"id":"note","name":"note","dataType":"VARCHAR(32)","nullable":true,"comment":"note's comment"}),
    );
    let old = index(
        json!({"id":"old","name":"by_amount","columns":["amount"],"isUnique":false,"indexType":"BTREE"}),
    );
    let caps = driver
        .structure_capabilities(&handle)
        .await
        .expect("live capabilities");
    assert!(caps.create_table && caps.rename_column && caps.reorder_column);
    let base = StructureChangeRequest {
        mode: StructureChangeMode::Create,
        schema: Some(f.database.clone()),
        table: table.clone(),
        original_columns: vec![],
        current_columns: vec![id.clone(), amount.clone(), note.clone()],
        original_indexes: vec![],
        current_indexes: vec![old.clone()],
    };
    let plan = driver
        .plan_structure_changes(&handle, &base)
        .await
        .expect("create native plan");
    let present = async |oracle: &mut sqlx::MySqlConnection| {
        sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM information_schema.TABLES WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=?")
          .bind(&table).fetch_one(oracle).await.expect("independent table oracle")
    };
    assert_eq!(present(&mut f.oracle).await, 0);
    for statement in plan.statements {
        driver
            .execute(&handle, &statement.sql)
            .await
            .expect("apply reviewed MySQL CREATE");
    }
    assert_eq!(present(&mut f.oracle).await, 1);
    sqlx::query(&format!("INSERT INTO {table} (note) VALUES ('original')"))
        .execute(&mut f.oracle)
        .await
        .expect("table created with native defaults");

    let cost = draft(
        json!({"id":"amount","name":"cost","dataType":"BIGINT","nullable":false,"defaultValue":"7","comment":"cost's description"}),
    );
    let label = draft(
        json!({"id":"label","name":"label","dataType":"VARCHAR(40)","nullable":true,"defaultValue":"'new'"}),
    );
    let by_cost = index(
        json!({"id":"new","name":"by_cost","columns":["cost"],"isUnique":true,"indexType":"BTREE","comment":"one cost"}),
    );
    let alter = StructureChangeRequest {
        mode: StructureChangeMode::Alter,
        schema: Some(f.database.clone()),
        table: table.clone(),
        original_columns: vec![id.clone(), amount, note],
        current_columns: vec![id.clone(), label, cost],
        original_indexes: vec![old],
        current_indexes: vec![by_cost],
    };
    let plan = driver
        .plan_structure_changes(&handle, &alter)
        .await
        .expect("native MySQL ALTER");
    for statement in plan.statements {
        driver
            .execute(&handle, &statement.sql)
            .await
            .expect("apply reviewed MySQL ALTER");
    }
    let columns:Vec<(String,String,Option<String>,String)>=sqlx::query_as("SELECT CAST(COLUMN_NAME AS CHAR),CAST(DATA_TYPE AS CHAR),CAST(COLUMN_DEFAULT AS CHAR),CAST(COLUMN_COMMENT AS CHAR) FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=? ORDER BY ORDINAL_POSITION")
        .bind(&table).fetch_all(&mut f.oracle).await.expect("independent column catalog");
    assert_eq!(
        columns.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(),
        ["id", "label", "cost"]
    );
    assert_eq!(
        (
            columns[2].1.as_str(),
            columns[2].2.as_deref(),
            columns[2].3.as_str()
        ),
        ("bigint", Some("7"), "cost's description")
    );
    let idx:Vec<String>=sqlx::query_scalar("SELECT DISTINCT CAST(INDEX_NAME AS CHAR) FROM information_schema.STATISTICS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=? ORDER BY 1")
        .bind(&table).fetch_all(&mut f.oracle).await.expect("actual indexes");
    assert_eq!(idx, ["by_cost", "PRIMARY"]);
    let row: (i64, String) = sqlx::query_as(&format!("SELECT cost,label FROM {table} WHERE id=1"))
        .fetch_one(&mut f.oracle)
        .await
        .expect("original row after native ALTER");
    assert_eq!(row, (2, "new".into()));
    driver
        .disconnect(handle)
        .await
        .expect("release real driver");
}

fn col(name: &str, data_type: &str, nullable: bool) -> MigrationColumn {
    MigrationColumn {
        name: name.into(),
        data_type: data_type.into(),
        nullable,
        default_value: None,
        comment: None,
        is_auto_increment: false,
    }
}

#[tokio::test]
async fn mysql_migration_replays_native_foreign_check_index_defaults_and_rollback_fragments() {
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let k = uuid::Uuid::new_v4().simple().to_string();
    let parent = format!("dz_parent_{k}");
    let child = format!("dz_child_{k}");
    let migration = driver
        .migration_renderer()
        .expect("native migration dialect");
    let capabilities = driver
        .migration_capabilities()
        .expect("migration capabilities");
    let foreign = ForeignKeyInfo {
        name: format!("fk_{k}"),
        columns: vec!["parent_id".into()],
        referenced_table: format!("{}.{parent}", f.database),
        referenced_columns: vec!["id".into()],
        on_delete: "SET NULL".into(),
        on_update: "CASCADE".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    };
    let check = CheckConstraint {
        name: format!("ck_{k}"),
        expression: "amount >= 0".into(),
    };
    let using = IndexInfo {
        name: "amount_idx".into(),
        columns: vec!["amount".into()],
        is_primary: false,
        is_unique: false,
        index_type: "BTREE".into(),
    };
    let mut id = col("id", "INT", false);
    id.is_auto_increment = true;
    let mut amount = col("amount", "DECIMAL(20,4)", false);
    amount.default_value = Some("2".into());
    let mut rollbacks = Vec::new();
    for operation in [
        Op::CreateTable {
            table: parent.clone(),
            columns: vec![col("id", "INT", false)],
            primary_keys: vec!["id".into()],
        },
        Op::CreateTable {
            table: child.clone(),
            columns: vec![id, col("parent_id", "INT", true), amount],
            primary_keys: vec!["id".into()],
        },
        Op::CreateIndex {
            table: child.clone(),
            index: using,
        },
        Op::AddForeignKey {
            table: child.clone(),
            foreign_key: foreign,
        },
        Op::AddCheckConstraint {
            table: child.clone(),
            constraint: check,
        },
        Op::AddColumn {
            table: child.clone(),
            column: col("label", "VARCHAR(40)", true),
        },
    ] {
        assert!(capabilities.supports(&operation), "{operation:?}");
        let stmt = migration
            .render(&operation)
            .expect("reviewed real migration fragment");
        driver
            .execute(&handle, &stmt.sql)
            .await
            .unwrap_or_else(|e| panic!("native migration fragment rejected: {e}; {}", stmt.sql));
        if let Some(sql) = stmt.rollback_sql {
            rollbacks.push(sql);
        }
    }
    let query = format!(
        "INSERT INTO {parent} VALUES (7); INSERT INTO {child}(parent_id,label) VALUES (7,'live'); UPDATE {parent} SET id=9 WHERE id=7;"
    );
    sqlx::raw_sql(&query)
        .execute(&mut f.oracle)
        .await
        .expect("foreign key cascade runs in MySQL");
    let row: (i32, i32, String) = sqlx::query_as(&format!(
        "SELECT id,parent_id,CAST(amount AS CHAR) FROM {child}"
    ))
    .fetch_one(&mut f.oracle)
    .await
    .expect("native auto ID, default and updated FK");
    assert_eq!(row, (1, 9, "2.0000".into()));
    let bad_fk = sqlx::query(&format!("INSERT INTO {child}(parent_id) VALUES (99)"))
        .execute(&mut f.oracle)
        .await
        .expect_err("FK enforced by actual engine");
    assert!(
        bad_fk
            .as_database_error()
            .is_some_and(|e| e.message().contains("foreign key"))
    );
    let bad_check = sqlx::query(&format!("INSERT INTO {child}(amount) VALUES (-1)"))
        .execute(&mut f.oracle)
        .await
        .expect_err("CHECK enforced by actual engine");
    assert!(
        bad_check
            .as_database_error()
            .is_some_and(|e| e.message().contains("Check constraint"))
    );
    sqlx::query(&format!("DELETE FROM {parent} WHERE id=9"))
        .execute(&mut f.oracle)
        .await
        .expect("real ON DELETE SET NULL");
    let after: Option<i32> = sqlx::query_scalar(&format!("SELECT parent_id FROM {child}"))
        .fetch_one(&mut f.oracle)
        .await
        .expect("actual NULL");
    assert_eq!(after, None);
    for sql in rollbacks.into_iter().rev() {
        driver
            .execute(&handle, &sql)
            .await
            .unwrap_or_else(|e| panic!("native rollback fragment rejected: {e}; {sql}"));
    }
    let remaining:i64=sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.TABLES WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME IN (?, ?)")
        .bind(parent).bind(child).fetch_one(&mut f.oracle).await.expect("native catalog after rollback DDL");
    assert_eq!(remaining, 0);
    driver
        .disconnect(handle)
        .await
        .expect("release actual driver");
}

#[tokio::test]
async fn mysql_structure_editor_rejects_unrepresentable_edits_without_modifying_live_table() {
    use datazen_driver_api::DriverError;
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let table = format!("dz_structure_review_{}", uuid::Uuid::new_v4().simple());
    let id = draft(
        json!({"id":"key","name":"id","dataType":"INT","nullable":false,"isPrimaryKey":true,"isAutoIncrement":true}),
    );
    let code = draft(
        json!({"id":"code","name":"code","dataType":"VARCHAR(16)","nullable":true,"defaultValue":"NULL","isUnique":true}),
    );
    let counter = draft(
        json!({"id":"count","name":"points","dataType":"INT","nullable":false,"defaultValue":"0"}),
    );
    let pk = index(
        json!({"id":"pk","name":"PRIMARY","columns":["id"],"isUnique":true,"isPrimary":true,"indexType":"BTREE"}),
    );
    let lookup = index(
        json!({"id":"lookup","name":"by_points","columns":["points"],"isUnique":false,"indexType":"","comment":"live index's note"}),
    );
    let base = StructureChangeRequest {
        mode: StructureChangeMode::Create,
        schema: None,
        table: table.clone(),
        original_columns: vec![],
        current_columns: vec![id.clone(), code.clone(), counter.clone()],
        original_indexes: vec![],
        current_indexes: vec![pk.clone(), lookup.clone()],
    };
    let plan = driver.plan_structure_changes(&handle, &base).await.unwrap();
    for statement in plan.statements {
        driver
            .execute(&handle, &statement.sql)
            .await
            .expect("apply reviewed unqualified native create");
    }
    sqlx::raw_sql(&format!(
        "INSERT INTO {table}(code,points) VALUES ('actual',4);"
    ))
    .execute(&mut f.oracle)
    .await
    .expect("independent seed after applying plan");
    let show = async |oracle: &mut sqlx::MySqlConnection| {
        sqlx::query_as::<_, (String, String)>(&format!("SHOW CREATE TABLE {table}"))
            .fetch_one(oracle)
            .await
            .expect("native table unchanged for refused edits")
            .1
    };
    let unchanged = show(&mut f.oracle).await;
    assert!(unchanged.contains("UNIQUE KEY `code`"));
    let comment: String = sqlx::query_scalar("SELECT CAST(INDEX_COMMENT AS CHAR) FROM information_schema.STATISTICS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=? AND INDEX_NAME='by_points'")
        .bind(&table).fetch_one(&mut f.oracle).await.unwrap();
    assert_eq!(comment, "live index's note");
    let stable = StructureChangeRequest {
        mode: StructureChangeMode::Alter,
        schema: Some(f.database.clone()),
        table: table.clone(),
        original_columns: vec![id.clone(), code.clone(), counter.clone()],
        current_columns: vec![id.clone(), code.clone(), counter.clone()],
        original_indexes: vec![pk.clone(), lookup.clone()],
        current_indexes: vec![pk.clone(), lookup.clone()],
    };
    let mut cases = vec![(stable.clone(), "no structure changes")];
    let mut attempt = stable.clone();
    attempt.current_indexes.remove(0);
    cases.push((attempt, "dropping the primary key"));
    let mut attempt = stable.clone();
    attempt.original_indexes.remove(0);
    cases.push((attempt, "adding a primary key index"));
    let mut attempt = stable.clone();
    attempt.current_columns[0].is_primary_key = false;
    cases.push((attempt, "alter_primary_key"));
    let mut attempt = stable.clone();
    attempt.current_columns[0].is_auto_increment = false;
    cases.push((attempt, "changing AUTO_INCREMENT"));
    let mut attempt = stable.clone();
    attempt.current_indexes[1].is_unique = true;
    cases.push((attempt, "drop and recreate"));
    let mut attempt = stable.clone();
    let mut candidate = lookup.clone();
    candidate.id = "new".into();
    candidate.name = "by_new_points".into();
    candidate.include_columns = vec!["code".into()];
    attempt.current_indexes.push(candidate);
    cases.push((attempt, "index_include"));
    let mut attempt = stable.clone();
    let mut candidate = lookup.clone();
    candidate.id = "new".into();
    candidate.name = "by_new_points".into();
    candidate.filter = Some("points > 0".into());
    attempt.current_indexes.push(candidate);
    cases.push((attempt, "index_filter"));
    let mut attempt = stable.clone();
    let mut candidate = lookup.clone();
    candidate.id = "new".into();
    candidate.name = "by_new_points".into();
    candidate.index_type = "GIN".into();
    attempt.current_indexes.push(candidate);
    cases.push((attempt, "index method"));
    let mut attempt = base.clone();
    attempt.current_columns.clear();
    cases.push((attempt, "requires at least one column"));
    for (request, wanted) in cases {
        let error = driver
            .plan_structure_changes(&handle, &request)
            .await
            .expect_err(wanted);
        assert!(
            matches!(&error, DriverError::Unsupported(detail) if detail.contains(wanted)),
            "invalid plan for {wanted}: {error}"
        );
        assert_eq!(show(&mut f.oracle).await, unchanged);
    }

    let mut forward = stable;
    forward.current_columns.swap(0, 1);
    let plan = driver
        .plan_structure_changes(&handle, &forward)
        .await
        .expect("reorder a real column to first");
    for statement in plan.statements {
        driver
            .execute(&handle, &statement.sql)
            .await
            .expect("apply reviewed FIRST");
    }
    let native:Vec<String>=sqlx::query_scalar("SELECT CAST(COLUMN_NAME AS CHAR) FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=? ORDER BY ORDINAL_POSITION")
       .bind(&table).fetch_all(&mut f.oracle).await.expect("native order after apply");
    assert_eq!(native, ["code", "id", "points"]);
    let state: (Option<String>, i32, i32) =
        sqlx::query_as(&format!("SELECT code,id,points FROM {table}"))
            .fetch_one(&mut f.oracle)
            .await
            .unwrap();
    assert_eq!(state, (Some("actual".into()), 1, 4));
    driver.disconnect(handle).await.unwrap();
}

#[tokio::test]
async fn mysql_migration_applies_and_reverts_native_table_options_and_default() {
    use datazen_driver_api::{MigrationRisk, TableOptions};
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let table = format!("dz_options_{}", uuid::Uuid::new_v4().simple());
    let full_name = format!("{}.{table}", f.database);
    let renderer = driver.migration_renderer().expect("MySQL renderer");
    let caps = driver
        .migration_capabilities()
        .expect("MySQL migration capabilities");
    let start = TableOptions {
        engine: Some("InnoDB".into()),
        charset: Some("utf8mb4".into()),
        collation: Some("utf8mb4_0900_ai_ci".into()),
        comment: Some("column owner's original".into()),
        ..Default::default()
    };
    let target = TableOptions {
        engine: Some("MyISAM".into()),
        charset: Some("latin1".into()),
        collation: Some("latin1_swedish_ci".into()),
        comment: None,
        ..Default::default()
    };
    let mut amount = col("amount", "INT", true);
    amount.default_value = Some("13".into());
    amount.comment = Some("amount's note".into());
    let create = Op::CreateTable {
        table: full_name.clone(),
        columns: vec![col("id", "INT", false), amount],
        primary_keys: vec![],
    };
    let plan = renderer
        .render_create_table_with_options(&create, &start)
        .unwrap();
    driver
        .execute(&handle, &plan.sql)
        .await
        .expect("native CREATE with selected table collation");
    let get = async |oracle: &mut sqlx::MySqlConnection| {
        sqlx::query_as::<_,(String,String,String)>("SELECT CAST(ENGINE AS CHAR),CAST(TABLE_COLLATION AS CHAR),CAST(TABLE_COMMENT AS CHAR) FROM information_schema.TABLES WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=?")
        .bind(&table).fetch_one(oracle).await.expect("independent MySQL information_schema options")
    };
    let original = get(&mut f.oracle).await;
    assert_eq!(
        original,
        (
            "InnoDB".into(),
            "utf8mb4_0900_ai_ci".into(),
            "column owner's original".into()
        )
    );
    let catalog = driver
        .get_table_schema(&handle, &table, &f.database, None)
        .await
        .unwrap();
    assert_eq!(
        catalog
            .columns
            .iter()
            .find(|c| c.name == "amount")
            .unwrap()
            .comment
            .as_deref(),
        Some("amount's note")
    );

    let alter = Op::SetTableOptions {
        table: full_name.clone(),
        from: start.clone(),
        to: target.clone(),
    };
    assert!(caps.supports(&alter) && caps.requires_table_rebuild(&alter));
    let change = renderer
        .render(&alter)
        .expect("render native change with exact rollback");
    assert_eq!(change.risk, MigrationRisk::Rewrite);
    driver
        .execute(&handle, &change.sql)
        .await
        .expect("apply engine, charset and remove comment");
    assert_eq!(
        get(&mut f.oracle).await,
        ("MyISAM".into(), "latin1_swedish_ci".into(), String::new())
    );
    driver
        .execute(&handle, change.rollback_sql.as_deref().unwrap())
        .await
        .expect("restore native prior table options");
    assert_eq!(get(&mut f.oracle).await, original);

    let default = Op::SetDefault {
        table: full_name.clone(),
        column: "amount".into(),
        from: Some("13".into()),
        to: None,
    };
    let change = renderer.render(&default).unwrap();
    driver.execute(&handle, &change.sql).await.unwrap();
    let default_value: Option<String> = sqlx::query_scalar("SELECT CAST(COLUMN_DEFAULT AS CHAR) FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME=? AND COLUMN_NAME='amount'")
        .bind(&table).fetch_one(&mut f.oracle).await.unwrap();
    assert!(default_value.is_none(), "drop the actual native default");
    sqlx::query(&format!("INSERT INTO {table}(id,amount) VALUES (1,NULL)"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
    driver
        .execute(&handle, change.rollback_sql.as_deref().unwrap())
        .await
        .unwrap();
    sqlx::query(&format!("INSERT INTO {table}(id) VALUES (2)"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
    let states: Vec<Option<i32>> =
        sqlx::query_scalar(&format!("SELECT amount FROM {table} ORDER BY id"))
            .fetch_all(&mut f.oracle)
            .await
            .unwrap();
    assert_eq!(states, [None, Some(13)]);
    for (from, to, reason) in [
        (start.clone(), start.clone(), "unchanged"),
        (
            start.clone(),
            TableOptions {
                engine: None,
                ..start.clone()
            },
            "engine removal",
        ),
        (
            start.clone(),
            TableOptions {
                charset: None,
                ..start.clone()
            },
            "charset removal",
        ),
        (
            TableOptions {
                engine: None,
                ..start.clone()
            },
            target.clone(),
            "engine rollback",
        ),
        (
            TableOptions {
                charset: None,
                ..start.clone()
            },
            TableOptions {
                engine: start.engine.clone(),
                ..target.clone()
            },
            "charset rollback",
        ),
    ] {
        let bad = Op::SetTableOptions {
            table: full_name.clone(),
            from,
            to,
        };
        assert!(
            renderer
                .render(&bad)
                .expect_err("unrepresentable native change")
                .contains(reason)
        );
        assert_eq!(get(&mut f.oracle).await, original);
    }
    driver.disconnect(handle).await.unwrap();
}

#[tokio::test]
async fn mysql_catalog_roundtrips_native_constraint_names_with_escaped_punctuation() {
    use datazen_driver_api::{DriverError, MigrationRisk};
    let mut f = Fixture::open().await;
    let (driver, handle) = f.connect().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let parent = format!("dz_quoted_parent_{tag}");
    let child = format!("dz_quoted_child_{tag}");
    let fk = format!("fk`wire_{tag}");
    let check = format!("ck`wire_{tag}");
    let parent_fields = ["left,key", "right)key"];
    let child_fields = ["ref,key", "ref)key"];
    let quote = |name: &str| driver.quote_ident(name);
    let p = parent_fields.map(quote);
    let c = child_fields.map(quote);
    let fk_ident = quote(&fk);
    let check_ident = quote(&check);
    let sql = format!(
        r#"CREATE TABLE {parent} ({} INT NOT NULL,{} INT NOT NULL,PRIMARY KEY ({},{}));
      CREATE TABLE {child} (id INT PRIMARY KEY,{} INT NOT NULL,{} INT NOT NULL, note VARCHAR(64),
        CONSTRAINT {fk_ident} FOREIGN KEY ({},{}) REFERENCES {parent} ({},{}) ON UPDATE CASCADE ON DELETE NO ACTION,
        CONSTRAINT {check_ident} CHECK ((CHAR_LENGTH(note)>2 AND note <> 'marker CHECK(\\)') OR note IS NULL))
        COMMENT='not a CHECK(x) declaration';
      INSERT INTO {parent} VALUES (5,6); INSERT INTO {child} VALUES(1,5,6,'okay')"#,
        p[0], p[1], p[0], p[1], c[0], c[1], c[0], c[1], p[0], p[1]
    );
    sqlx::raw_sql(&sql)
        .execute(&mut f.oracle)
        .await
        .expect("real quoted MySQL constraints");

    let schema = driver
        .get_table_schema(&handle, &child, &f.database, None)
        .await
        .unwrap();
    assert_eq!(schema.foreign_keys.len(), 1);
    let edge = schema.foreign_keys[0].clone();
    assert_eq!(edge.name, fk);
    assert_eq!(edge.columns, child_fields);
    assert_eq!(edge.referenced_columns, parent_fields);
    assert_eq!(edge.referenced_table, parent);
    assert_eq!(edge.on_update, "CASCADE");
    assert!(matches!(edge.on_delete.as_str(), "RESTRICT" | "NO ACTION"));
    assert_eq!(schema.check_constraints.len(), 1);
    assert_eq!(schema.check_constraints[0].name, check);
    assert!(
        schema.check_constraints[0]
            .expression
            .contains("marker CHECK(")
    );
    let control = sqlx::query_scalar::<_, String>(
        "SELECT CAST(CONSTRAINT_NAME AS CHAR) FROM information_schema.REFERENTIAL_CONSTRAINTS
       WHERE CONSTRAINT_SCHEMA=DATABASE() AND TABLE_NAME=?",
    )
    .bind(&child)
    .fetch_one(&mut f.oracle)
    .await
    .unwrap();
    assert_eq!(control, fk);
    let renderer = driver.migration_renderer().unwrap();
    let drop = renderer
        .render(&Op::DropForeignKey {
            table: child.clone(),
            foreign_key: edge.clone(),
        })
        .unwrap();
    assert_eq!(drop.risk, MigrationRisk::Destructive);
    driver
        .execute(&handle, &drop.sql)
        .await
        .expect("native drop targets exact escaped name");
    let create = renderer
        .render(&Op::AddForeignKey {
            table: child.clone(),
            foreign_key: ForeignKeyInfo {
                on_delete: "NO ACTION".into(),
                ..edge
            },
        })
        .unwrap();
    driver
        .execute(&handle, &create.sql)
        .await
        .expect("same exact catalog FK can be replayed");
    driver
        .execute(
            &handle,
            &format!("UPDATE {parent} SET {}=16 WHERE {}=6", p[1], p[1]),
        )
        .await
        .unwrap();
    let state: (i32, i32) =
        sqlx::query_as(&format!("SELECT {},{} FROM {child} WHERE id=1", c[0], c[1]))
            .fetch_one(&mut f.oracle)
            .await
            .unwrap();
    assert_eq!(state, (5, 16));
    assert!(
        matches!(driver.execute(&handle,&format!("UPDATE {child} SET note='a' WHERE id=1")).await,
       Err(DriverError::QueryFailed(detail)) if detail.contains("Check constraint"))
    );
    driver.disconnect(handle).await.unwrap();
}

#[tokio::test]
async fn mysql_review_refuses_schema_edits_that_cannot_preserve_live_constraints() {
    let mut f = Fixture::open().await;
    let (driver, h) = f.connect().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let parent = format!("dz_review_parent_{tag}");
    let child = format!("dz_review_child_{tag}");
    let foreign = format!("fk_review_{tag}");
    sqlx::raw_sql(&format!("CREATE TABLE {parent}(id INT PRIMARY KEY);
      CREATE TABLE {child}(id INT NOT NULL AUTO_INCREMENT PRIMARY KEY, parent_id INT, body TEXT,
        FULLTEXT INDEX native_words(body),
        CONSTRAINT {foreign} FOREIGN KEY(parent_id) REFERENCES {parent}(id) ON DELETE CASCADE);
      INSERT INTO {parent} VALUES(1); INSERT INTO {child}(parent_id,body) VALUES(1,'verified actual row')"))
      .execute(&mut f.oracle).await.unwrap();
    let schema = driver
        .get_table_schema(&h, &child, &f.database, None)
        .await
        .unwrap();
    let mut index = schema
        .indexes
        .iter()
        .find(|i| i.name == "native_words")
        .unwrap()
        .clone();
    assert_eq!(index.index_type, "FULLTEXT");
    let edge = schema
        .foreign_keys
        .iter()
        .find(|v| v.name == foreign)
        .unwrap()
        .clone();
    let renderer = driver.migration_renderer().unwrap();
    let caps = driver.migration_capabilities().unwrap();
    assert!(!caps.supports(&Op::SetNullable {
        table: child.clone(),
        column: "parent_id".into(),
        nullable: false
    }));
    let nullable = Op::SetNullable {
        table: child.clone(),
        column: "parent_id".into(),
        nullable: false,
    };
    let auto = Op::SetAutoIncrement {
        table: child.clone(),
        column: "id".into(),
        from: true,
        to: false,
    };
    assert!(!caps.supports(&auto));
    assert!(caps.requires_table_rebuild(&auto));
    index.is_unique = true;
    let create_unique = Op::CreateIndex {
        table: child.clone(),
        index: index.clone(),
    };
    index.is_unique = false;
    index.index_type.clear();
    let missing_method = Op::CreateIndex {
        table: child.clone(),
        index: index.clone(),
    };
    index.index_type = "GIN".into();
    let unsupported_method = Op::CreateIndex {
        table: child.clone(),
        index,
    };
    let deferrable = Op::AddForeignKey {
        table: child.clone(),
        foreign_key: ForeignKeyInfo {
            deferrability: ForeignKeyDeferrability::DeferrableInitiallyDeferred,
            ..edge.clone()
        },
    };
    let mismatch = Op::AddForeignKey {
        table: child.clone(),
        foreign_key: ForeignKeyInfo {
            columns: vec!["id".into(), "parent_id".into()],
            ..edge.clone()
        },
    };
    let nonnative_action = Op::AddForeignKey {
        table: child.clone(),
        foreign_key: ForeignKeyInfo {
            on_delete: "INVALID ACTION".into(),
            ..edge
        },
    };
    let original: (String, String) = sqlx::query_as(&format!("SHOW CREATE TABLE {child}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    for (request, reason) in [
        (nullable, "complete original column"),
        (auto, "removal requires original column"),
        (create_unique, "cannot create a unique FULLTEXT"),
        (missing_method, "index type is missing"),
        (unsupported_method, "cannot be safely recreated"),
        (deferrable, "deferrable foreign key semantics"),
        (mismatch, "matching column lists"),
        (nonnative_action, "cannot represent foreign-key action"),
    ] {
        let refused = renderer.render(&request).unwrap_err();
        assert!(refused.contains(reason), "{refused}");
        let unchanged: (String, String) = sqlx::query_as(&format!("SHOW CREATE TABLE {child}"))
            .fetch_one(&mut f.oracle)
            .await
            .unwrap();
        assert_eq!(unchanged, original);
    }
    sqlx::query(&format!("DELETE FROM {parent} WHERE id=1"))
        .execute(&mut f.oracle)
        .await
        .unwrap();
    let actual: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {child}"))
        .fetch_one(&mut f.oracle)
        .await
        .unwrap();
    assert_eq!(actual, 0, "real original foreign key is still applied");
    driver.disconnect(h).await.unwrap();
}
