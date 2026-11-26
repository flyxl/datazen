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

#[tokio::test]
async fn reviewed_schema_diff_deploy_rolls_back_all_ddl_then_retries_on_real_postgres() {
    use datazen::migration::schema_diff::{
        build_column_plan, execute_schema_diff_deploy, DeployOptions, DeployStatus,
    };

    let mut fixture = Fixture::open().await;
    let database = fixture
        .config
        .database
        .as_deref()
        .expect("fixture database");
    let schema = format!("dz_reviewed_{}", uuid::Uuid::new_v4().simple());
    let desired = format!("{schema}.desired");
    let target = format!("{schema}.current");
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema};
         CREATE TABLE {desired}(id integer PRIMARY KEY, required text NOT NULL DEFAULT 'future', reviewed_note text);
         CREATE TABLE {target}(id integer PRIMARY KEY, required text);
         INSERT INTO {target} VALUES(11,NULL),(22,'keep')"
    )).execute(&mut fixture.oracle).await.expect("genuinely distinct source and target schemas");
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("live migration handle");
    let source = driver
        .get_table_schema(&handle, "desired", database, Some(&schema))
        .await
        .expect("actual desired catalog snapshot");
    let current = driver
        .get_table_schema(&handle, "current", database, Some(&schema))
        .await
        .expect("actual current catalog snapshot");
    let plan = build_column_plan(&target, &source, &current, "postgresql")
        .expect("build reviewed PostgreSQL migration SQL");
    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert!(plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("ADD COLUMN")));
    assert!(plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("NOT NULL")));
    let rollback =
        execute_schema_diff_deploy(&driver, &handle, &plan, DeployOptions::default(), None).await;
    assert_eq!(
        rollback.status,
        DeployStatus::RolledBack,
        "live finished deploy: {:?}",
        rollback.errors
    );
    assert!(rollback
        .errors
        .iter()
        .any(|error| error.contains("contains null values")));
    assert!(!sqlx::query_scalar::<_,bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass($1) AND attname='reviewed_note' AND NOT attisdropped)"
    ).bind(&target).fetch_one(&mut fixture.oracle).await.expect("independent catalog after rollback"),
    "the earlier additive DDL actually rolled back");
    let original: Vec<(i32, Option<String>)> =
        sqlx::query_as(&format!("SELECT id,required FROM {target} ORDER BY id"))
            .fetch_all(&mut fixture.oracle)
            .await
            .expect("committed rows after failed deploy");
    assert_eq!(original, [(11, None), (22, Some("keep".into()))]);

    sqlx::query(&format!(
        "UPDATE {target} SET required='cleaned beforehand' WHERE id=11"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("scope fixture remediation to its missing value");
    let success =
        execute_schema_diff_deploy(&driver, &handle, &plan, DeployOptions::default(), None).await;
    assert_eq!(
        success.status,
        DeployStatus::Committed,
        "live deploy after remediation: {:?}",
        success.errors
    );
    assert_eq!(success.executed_count, plan.statements.len());
    let applied: Vec<(i32, String, Option<String>)> = sqlx::query_as(&format!(
        "SELECT id,required,reviewed_note FROM {target} ORDER BY id"
    ))
    .fetch_all(&mut fixture.oracle)
    .await
    .expect("independent real DDL and row result");
    assert_eq!(
        applied,
        [
            (11, "cleaned beforehand".into(), None),
            (22, "keep".into(), None)
        ]
    );
    let default_value: String = sqlx::query_scalar(&format!(
        "INSERT INTO {target}(id) VALUES(33) RETURNING required"
    ))
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("new target defaults really persist");
    assert_eq!(default_value, "future");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {desired}"))
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("desired source not mutated"),
        0
    );
    driver
        .disconnect(handle)
        .await
        .expect("migration cleanup session");
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("clean real migration scope");
}

#[tokio::test]
async fn rendered_postgres_primary_key_uses_qualified_relation_but_bare_constraint_identity() {
    let mut fixture = Fixture::open().await;
    let schema = format!("dz_constraint_{}", uuid::Uuid::new_v4().simple());
    let table = format!("{schema}.accounts");
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema};CREATE TABLE {table}(id integer, active boolean);INSERT INTO {table} VALUES(1,true)"
    )).execute(&mut fixture.oracle).await.expect("actual target relation with no existing primary key");
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("real migrated connection");
    let renderer = driver
        .migration_renderer()
        .expect("PostgreSQL migration dialect");
    let add = apply(
        &driver,
        &handle,
        renderer.as_ref(),
        Op::AddPrimaryKey {
            table: table.clone(),
            columns: vec!["id".into()],
        },
    )
    .await;
    let key: String = sqlx::query_scalar(
        "SELECT conname::text FROM pg_constraint WHERE conrelid=to_regclass($1) AND contype='p'",
    )
    .bind(&table)
    .fetch_one(&mut fixture.oracle)
    .await
    .expect("exact physical constraint identity");
    assert_eq!(key, "accounts_pkey");
    let duplicate = sqlx::query(&format!("INSERT INTO {table} VALUES(1,false)"))
        .execute(&mut fixture.oracle)
        .await
        .expect_err("new actual key rejects duplicate rows");
    assert_eq!(
        duplicate
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23505")
    );
    run(
        &driver,
        &handle,
        add.rollback_sql.as_deref().expect("inverse key rollback"),
    )
    .await;
    let count = sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("live key rollback did not touch rows");
    assert_eq!(count, 1);
    apply(
        &driver,
        &handle,
        renderer.as_ref(),
        Op::AddPrimaryKey {
            table: table.clone(),
            columns: vec!["id".into()],
        },
    )
    .await;
    apply(
        &driver,
        &handle,
        renderer.as_ref(),
        Op::DropPrimaryKey {
            table: table.clone(),
            columns: vec!["id".into()],
        },
    )
    .await;
    assert_eq!(
        sqlx::query(&format!("INSERT INTO {table} VALUES(1,false)"))
            .execute(&mut fixture.oracle)
            .await
            .expect("reviewed drop targets exact bare key")
            .rows_affected(),
        1
    );
    driver
        .disconnect(handle)
        .await
        .expect("remove test connection");
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("dispose actual constraint scope");
}

#[tokio::test]
async fn rendered_postgres_trigger_drop_targets_only_its_catalog_schema_and_relation() {
    use datazen_driver_api::MigrationTrigger;
    use serde_json::json;

    let mut fixture = Fixture::open().await;
    let schema = format!("dz_triggers_{}", uuid::Uuid::new_v4().simple());
    let first = format!("{schema}.events");
    let other = format!("{schema}.other_events");
    sqlx::raw_sql(&format!(
        r#"
        CREATE SCHEMA {schema};
        CREATE TABLE {first}(id integer);
        CREATE TABLE {other}(id integer);
        CREATE FUNCTION {schema}.bump() RETURNS trigger LANGUAGE plpgsql AS
          $$ BEGIN NEW.id := NEW.id+1; RETURN NEW; END $$;
        CREATE TRIGGER stamp BEFORE INSERT ON {first} FOR EACH ROW EXECUTE FUNCTION {schema}.bump();
        CREATE TRIGGER stamp BEFORE INSERT ON {other} FOR EACH ROW EXECUTE FUNCTION {schema}.bump();
    "#
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("two exact live trigger targets sharing name");
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("real driver for catalog diff");
    let renderer = driver.migration_renderer().expect("Postgres renderer");
    let object=driver.execute_command(&handle,"get_object_ddl",json!({
        "kind":"trigger","name":"stamp","schema":schema,"targetSchema":schema,"targetName":"events"
    })).await.expect("actual source trigger catalog DDL");
    let definition = object.data["ddl"]
        .as_str()
        .expect("live DDL text")
        .to_owned();
    let trigger = MigrationTrigger {
        name: "stamp".into(),
        schema: Some(schema.clone()),
        target_schema: Some(schema.clone()),
        target_name: "events".into(),
        definition,
    };
    let dropped = apply(
        &driver,
        &handle,
        renderer.as_ref(),
        Op::DropTrigger {
            trigger: trigger.clone(),
        },
    )
    .await;
    for (table, expected) in [(&first, 1), (&other, 2)] {
        let observed: i32 =
            sqlx::query_scalar(&format!("INSERT INTO {table} VALUES(1) RETURNING id"))
                .fetch_one(&mut fixture.oracle)
                .await
                .expect("independent node's trigger behavior");
        assert_eq!(observed, expected);
    }
    run(
        &driver,
        &handle,
        dropped
            .rollback_sql
            .as_deref()
            .expect("captured live trigger definition"),
    )
    .await;
    assert_eq!(
        sqlx::query_scalar::<_, i32>(&format!("INSERT INTO {first} VALUES (3) RETURNING id"))
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("source target restored"),
        4
    );
    let mut wrong = trigger;
    wrong.target_schema = Some("public".into());
    assert!(
        renderer
            .render(&Op::DropTrigger { trigger: wrong })
            .is_err(),
        "conflicting trigger/target schemas are not safe to project"
    );
    driver.disconnect(handle).await.expect("end test session");
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("cleanup actual catalog scope");
}

#[tokio::test]
async fn rendered_postgres_index_rollback_uses_the_reviewed_physical_namespace() {
    let mut fixture = Fixture::open().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let intended = format!("dz_index_target_{tag}");
    let neutral = format!("dz_index_decoy_{tag}");
    let table = format!("{intended}.accounts");
    let name = "literal.history";
    sqlx::raw_sql(&format!(
        r#"
        CREATE SCHEMA {intended};CREATE SCHEMA {neutral};
        CREATE TABLE {table}(id integer, "account.code" text);
        CREATE TABLE {neutral}.accounts(id integer, "account.code" text);
        CREATE INDEX "literal.history" ON {neutral}.accounts("account.code");
        INSERT INTO {table} VALUES(1,'first');
    "#
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("two actual namespaces with shared index name");
    let driver = PostgresDriver::new();
    let h = driver
        .connect_fixed_session(&fixture.config)
        .await
        .expect("retained admin scope");
    run(
        &driver,
        &h,
        &format!("SET search_path TO {neutral}, public"),
    )
    .await;
    let renderer = driver.migration_renderer().expect("PostgreSQL renderer");
    let index = IndexInfo {
        name: name.into(),
        columns: vec!["account.code".into()],
        is_unique: true,
        is_primary: false,
        index_type: "btree".into(),
    };
    let names = async |oracle: &mut sqlx::PgConnection| {
        sqlx::query_scalar::<_,String>(
            "SELECT schemaname::text FROM pg_indexes WHERE indexname=$1 AND schemaname=ANY($2) ORDER BY schemaname"
        ).bind(name).bind(vec![intended.clone(),neutral.clone()]).fetch_all(oracle).await.expect("actual index ownership")
    };
    let first = apply(
        &driver,
        &h,
        renderer.as_ref(),
        Op::CreateIndex {
            table: table.clone(),
            index: index.clone(),
        },
    )
    .await;
    assert_eq!(names(&mut fixture.oracle).await.len(), 2);
    let duplicate = sqlx::query(&format!("INSERT INTO {table} VALUES(2,'first')"))
        .execute(&mut fixture.oracle)
        .await
        .expect_err("actual unique index guards duplicate input");
    assert_eq!(
        duplicate
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23505")
    );
    run(
        &driver,
        &h,
        first.rollback_sql.as_deref().expect("reviewed inverse SQL"),
    )
    .await;
    assert_eq!(names(&mut fixture.oracle).await, [neutral.clone()]);
    apply(
        &driver,
        &h,
        renderer.as_ref(),
        Op::CreateIndex {
            table: table.clone(),
            index: index.clone(),
        },
    )
    .await;
    apply(
        &driver,
        &h,
        renderer.as_ref(),
        Op::DropIndex {
            table: table.clone(),
            index,
        },
    )
    .await;
    assert_eq!(names(&mut fixture.oracle).await, [neutral.clone()]);
    assert_eq!(
        sqlx::query(&format!("INSERT INTO {table} VALUES(2,'first')"))
            .execute(&mut fixture.oracle)
            .await
            .expect("only intended constraint removed")
            .rows_affected(),
        1
    );
    driver
        .disconnect(h)
        .await
        .expect("release retained index fixture");
    sqlx::raw_sql(&format!(
        "DROP SCHEMA {intended} CASCADE; DROP SCHEMA {neutral} CASCADE"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("remove genuine index fixtures");
}

#[tokio::test]
async fn migrated_postgres_foreign_key_modes_apply_real_enforcement_timing() {
    let mut fixture = Fixture::open().await;
    let schema = format!("dz_fk_modes_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema};
         CREATE TABLE {schema}.parent(id int PRIMARY KEY);
         CREATE TABLE {schema}.child(id int PRIMARY KEY, mandatory int, can_defer int);"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("independent fixture");
    let driver = PostgresDriver::new();
    let handle = driver.connect_fixed_session(&fixture.config).await.unwrap();
    driver
        .execute(&handle, &format!("SET search_path TO {schema}, pg_catalog"))
        .await
        .unwrap();
    let renderer = driver.migration_renderer().unwrap();
    for (name, column, deferrability) in [
        (
            "per_row",
            "mandatory",
            ForeignKeyDeferrability::NotDeferrable,
        ),
        (
            "initially_verified",
            "can_defer",
            ForeignKeyDeferrability::DeferrableInitiallyImmediate,
        ),
    ] {
        apply(
            &driver,
            &handle,
            renderer.as_ref(),
            Op::AddForeignKey {
                table: format!("{schema}.child"),
                foreign_key: ForeignKeyInfo {
                    name: name.into(),
                    columns: vec![column.into()],
                    referenced_table: format!("{schema}.parent"),
                    referenced_columns: vec!["id".into()],
                    on_update: "NO ACTION".into(),
                    on_delete: "NO ACTION".into(),
                    deferrability,
                },
            },
        )
        .await;
    }
    let child = format!("{schema}.child");
    let options: Vec<(String, bool, bool)> = sqlx::query_as(
        "SELECT conname::text, condeferrable, condeferred FROM pg_constraint
         WHERE conrelid=to_regclass($1) AND contype='f' ORDER BY conname",
    )
    .bind(&child)
    .fetch_all(&mut fixture.oracle)
    .await
    .unwrap();
    assert_eq!(
        options,
        [
            ("initially_verified".into(), true, false),
            ("per_row".into(), false, false)
        ]
    );
    let learned = driver
        .get_table_schema(
            &handle,
            "child",
            fixture.config.database.as_deref().unwrap(),
            Some(&schema),
        )
        .await
        .expect("re-read applied constraint metadata");
    assert_eq!(
        learned
            .foreign_keys
            .iter()
            .map(|fk| (fk.name.as_str(), fk.deferrability))
            .collect::<std::collections::BTreeMap<_, _>>(),
        [
            (
                "initially_verified",
                ForeignKeyDeferrability::DeferrableInitiallyImmediate
            ),
            ("per_row", ForeignKeyDeferrability::NotDeferrable)
        ]
        .into_iter()
        .collect()
    );
    let violation = sqlx::query(&format!("INSERT INTO {child} VALUES (1,NULL,23)"))
        .execute(&mut fixture.oracle)
        .await
        .expect_err("initial mode really checks at statement level");
    assert_eq!(
        violation.as_database_error().unwrap().code().as_deref(),
        Some("23503")
    );
    let tx = driver.begin_transaction(&handle).await.unwrap();
    let no_defer = driver
        .execute(&handle, "SET CONSTRAINTS per_row DEFERRED")
        .await
        .expect_err("not-deferrable can never switch timing");
    assert!(no_defer.to_string().contains("not deferrable"));
    driver.rollback(tx).await.unwrap();
    let tx = driver.begin_transaction(&handle).await.unwrap();
    run(
        &driver,
        &handle,
        "SET CONSTRAINTS initially_verified DEFERRED;
        INSERT INTO child VALUES (2,NULL,55); INSERT INTO parent VALUES (55);",
    )
    .await;
    driver.commit(tx).await.unwrap();
    let persisted: Vec<(i32, Option<i32>, Option<i32>)> =
        sqlx::query_as(&format!("SELECT id,mandatory,can_defer FROM {child}"))
            .fetch_all(&mut fixture.oracle)
            .await
            .unwrap();
    assert_eq!(persisted, [(2, None, Some(55))]);
    driver.disconnect(handle).await.unwrap();
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .unwrap();
}
