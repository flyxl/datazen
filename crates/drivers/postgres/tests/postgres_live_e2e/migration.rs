use super::Fixture;
use datazen_driver_api::{
    sql_split::split_sql_statements, CheckConstraint, ConnectionHandle, DatabaseDriver,
    ForeignKeyDeferrability, ForeignKeyInfo, IndexInfo, MigrationColumn, MigrationOperation as Op,
    MigrationRenderer, MigrationRisk, MigrationStatement, Value,
};
use datazen_driver_postgres::PostgresDriver;

async fn run(driver: &PostgresDriver, handle: &ConnectionHandle, sql: &str) {
    for stmt in split_sql_statements(sql) {
        driver
            .execute(handle, &stmt)
            .await
            .unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

async fn apply(
    driver: &PostgresDriver,
    handle: &ConnectionHandle,
    renderer: &dyn MigrationRenderer,
    operation: Op,
) -> MigrationStatement {
    let statement = renderer
        .render(&operation)
        .expect("reviewed dialect migration fragment");
    run(driver, handle, &statement.sql).await;
    statement
}

fn column(name: &str, data_type: &str, nullable: bool) -> MigrationColumn {
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
async fn rendered_relational_migration_round_trips_constraints_on_the_server() {
    let mut fixture = Fixture::open().await;
    let driver = PostgresDriver::new();
    let handle = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("fixed connection");
    let renderer = driver
        .migration_renderer()
        .expect("Postgres migration dialect");
    let caps = driver
        .migration_capabilities()
        .expect("Postgres migration capabilities");
    let schema = format!("dz_migration_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("isolated migration schema");
    run(
        &driver,
        &handle,
        &format!("SET search_path TO {schema}, pg_catalog"),
    )
    .await;

    let mut amount = column("amount", "integer", false);
    amount.default_value = Some("7".into());
    amount.comment = Some("portable phone's amount".into());
    for (table, columns, pk) in [
        (
            "parents",
            vec![column("id", "integer", false)],
            vec!["id".into()],
        ),
        (
            "children",
            vec![
                column("id", "integer", false),
                column("parent_id", "integer", true),
                amount,
            ],
            vec![],
        ),
    ] {
        let operation = Op::CreateTable {
            table: table.into(),
            columns,
            primary_keys: pk,
        };
        assert!(caps.supports(&operation));
        let statement = apply(&driver, &handle, renderer.as_ref(), operation).await;
        assert_eq!(statement.risk, MigrationRisk::Additive);
    }
    let mut note = column("note", "text", true);
    note.comment = Some("reviewed note".into());
    note.default_value = Some("'passed'".into());
    let added = apply(
        &driver,
        &handle,
        renderer.as_ref(),
        Op::AddColumn {
            table: "children".into(),
            column: note,
        },
    )
    .await;
    let index = IndexInfo {
        name: "portable_by_amount".into(),
        columns: vec!["amount".into(), "id".into()],
        is_primary: false,
        is_unique: true,
        index_type: "BTREE".into(),
    };
    let check = CheckConstraint {
        name: "positive_amount".into(),
        expression: "amount > 0".into(),
    };
    let foreign = ForeignKeyInfo {
        name: "child_parent".into(),
        columns: vec!["parent_id".into()],
        referenced_table: "parents".into(),
        referenced_columns: vec!["id".into()],
        on_update: "CASCADE".into(),
        on_delete: "SET NULL".into(),
        deferrability: ForeignKeyDeferrability::DeferrableInitiallyDeferred,
    };
    for operation in [
        Op::AddPrimaryKey {
            table: "children".into(),
            columns: vec!["id".into()],
        },
        Op::CreateIndex {
            table: "children".into(),
            index: index.clone(),
        },
        Op::AddCheckConstraint {
            table: "children".into(),
            constraint: check.clone(),
        },
        Op::AddForeignKey {
            table: "children".into(),
            foreign_key: foreign.clone(),
        },
    ] {
        assert!(caps.supports(&operation));
        assert!(apply(&driver, &handle, renderer.as_ref(), operation)
            .await
            .rollback_sql
            .is_some());
    }

    // A truly deferred FK permits child-before-parent in this transaction.
    run(&driver, &handle, "BEGIN; INSERT INTO children(id,parent_id) VALUES (1,23); INSERT INTO parents VALUES(23); COMMIT").await;
    let relation = format!("{schema}.children");
    let current = sqlx::query_as::<_, (i32, Option<i32>, i32, String)>(&format!(
        "SELECT id,parent_id,amount,note FROM {relation}"
    ))
    .fetch_all(&mut fixture.oracle)
    .await
    .expect("independent row oracle");
    assert_eq!(current, [(1, Some(23), 7, "passed".into())]);
    let comments: Vec<(String, String)> = sqlx::query_as(
        "SELECT attname::text,col_description(attrelid,attnum) FROM pg_attribute WHERE attrelid=to_regclass($1) AND attname IN ('amount','note') ORDER BY attname"
    ).bind(&relation).fetch_all(&mut fixture.oracle).await.expect("actual catalog comments");
    assert_eq!(
        comments,
        [
            ("amount".into(), "portable phone's amount".into()),
            ("note".into(), "reviewed note".into())
        ]
    );
    let constraint: (String, String, bool, bool) = sqlx::query_as(
        "SELECT confupdtype::text,confdeltype::text,condeferrable,condeferred FROM pg_constraint WHERE conrelid=to_regclass($1) AND conname='child_parent'"
    ).bind(&relation).fetch_one(&mut fixture.oracle).await.expect("actual PostgreSQL FK actions");
    assert_eq!(constraint, ("c".into(), "n".into(), true, true));
    let index_definition: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE schemaname=$1 AND indexname='portable_by_amount'",
    )
    .bind(&schema)
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("actual PostgreSQL index definition");
    assert!(
        index_definition.contains("UNIQUE INDEX")
            && index_definition.contains("USING btree (amount, id)")
    );
    let invalid = sqlx::query(&format!("INSERT INTO {relation}(id,amount) VALUES(2,-1)"))
        .execute(&mut fixture.oracle)
        .await
        .expect_err("real check constraint rejects negative amount");
    assert_eq!(
        invalid
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514")
    );
    run(&driver, &handle, "UPDATE parents SET id=25 WHERE id=23").await;
    assert_eq!(
        sqlx::query_scalar::<_, i32>(&format!("SELECT parent_id FROM {relation}"))
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("cascade update"),
        25
    );
    run(&driver, &handle, "DELETE FROM parents WHERE id=25").await;
    assert_eq!(
        sqlx::query_scalar::<_, Option<i32>>(&format!("SELECT parent_id FROM {relation}"))
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("set null on delete"),
        None
    );

    for operation in [
        Op::SetDefault {
            table: "children".into(),
            column: "amount".into(),
            from: Some("7".into()),
            to: Some("9".into()),
        },
        Op::SetDefault {
            table: "children".into(),
            column: "note".into(),
            from: Some("'passed'".into()),
            to: None,
        },
        Op::AlterColumnType {
            table: "children".into(),
            column: "amount".into(),
            from: "integer".into(),
            to: "bigint".into(),
        },
        Op::SetNullable {
            table: "children".into(),
            column: "amount".into(),
            nullable: true,
        },
        Op::SetNullable {
            table: "children".into(),
            column: "amount".into(),
            nullable: false,
        },
        Op::SetComment {
            table: "children".into(),
            column: "amount".into(),
            from: Some("portable phone's amount".into()),
            to: None,
        },
        Op::DropCheckConstraint {
            table: "children".into(),
            constraint: check.clone(),
        },
        Op::DropForeignKey {
            table: "children".into(),
            foreign_key: foreign,
        },
        Op::DropIndex {
            table: "children".into(),
            index,
        },
        Op::DropPrimaryKey {
            table: "children".into(),
            columns: vec!["id".into()],
        },
    ] {
        apply(&driver, &handle, renderer.as_ref(), operation).await;
    }
    let persisted = sqlx::query_as::<_, (i32, i64, Option<String>)>(&format!(
        "INSERT INTO {relation}(id,parent_id) VALUES (1,999) RETURNING id,amount,note"
    ))
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("dropped constraints and new defaults in real DB");
    assert_eq!(persisted, (1, 9, None));
    let (_, no_comment): (String, Option<String>) = sqlx::query_as(
        "SELECT format_type(atttypid,atttypmod),col_description(attrelid,attnum) FROM pg_attribute WHERE attrelid=to_regclass($1) AND attname='amount'"
    ).bind(&relation).fetch_one(&mut fixture.oracle).await.expect("removed actual column comment");
    assert!(no_comment.is_none());
    run(
        &driver,
        &handle,
        added.rollback_sql.as_deref().expect("add rollback"),
    )
    .await;
    assert_eq!(
        driver
            .query(&handle, "SELECT count(*) FROM children")
            .await
            .expect("rollback kept rows")
            .rows,
        vec![vec![Some(Value::Integer(2))]]
    );
    apply(
        &driver,
        &handle,
        renderer.as_ref(),
        Op::DropColumn {
            table: "children".into(),
            column: column("amount", "bigint", false),
        },
    )
    .await;
    let statement = apply(
        &driver,
        &handle,
        renderer.as_ref(),
        Op::DropTable {
            table: "children".into(),
        },
    )
    .await;
    assert!(
        statement.rollback_sql.is_none(),
        "dropping populated data cannot claim rollback SQL"
    );
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT to_regclass($1) IS NULL")
            .bind(&relation)
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("table was dropped")
    );
    driver.disconnect(handle).await.expect("driver cleanup");
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove isolated migration schema");
}
