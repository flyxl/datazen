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
    let query=format!("INSERT INTO {parent} VALUES (7); INSERT INTO {child}(parent_id,label) VALUES (7,'live'); UPDATE {parent} SET id=9 WHERE id=7;");
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
    assert!(bad_fk
        .as_database_error()
        .is_some_and(|e| e.message().contains("foreign key")));
    let bad_check = sqlx::query(&format!("INSERT INTO {child}(amount) VALUES (-1)"))
        .execute(&mut f.oracle)
        .await
        .expect_err("CHECK enforced by actual engine");
    assert!(bad_check
        .as_database_error()
        .is_some_and(|e| e.message().contains("Check constraint")));
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
