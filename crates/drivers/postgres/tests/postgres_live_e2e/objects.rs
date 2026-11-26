use super::Fixture;
use datazen_driver_api::{
    sql_split::split_sql_statements, ConnectionHandle, DatabaseDriver, MigrationOperation as Op,
    MigrationRenderer, MigrationRoutine, MigrationSequence, MigrationTrigger, MigrationType,
    MigrationView, ObjectKind,
};
use datazen_driver_postgres::PostgresDriver;
use serde_json::{json, Value};
use sqlx::PgConnection;

async fn apply(
    driver: &PostgresDriver,
    handle: &ConnectionHandle,
    renderer: &dyn MigrationRenderer,
    op: Op,
) -> Option<String> {
    let statement = renderer.render(&op).expect("reviewed catalog DDL");
    execute(driver, handle, &statement.sql).await;
    statement.rollback_sql
}

async fn execute(driver: &PostgresDriver, handle: &ConnectionHandle, sql: &str) {
    for statement in split_sql_statements(sql) {
        driver
            .execute(handle, &statement)
            .await
            .unwrap_or_else(|e| panic!("{statement}: {e}"));
    }
}

async fn command(
    driver: &PostgresDriver,
    handle: &ConnectionHandle,
    id: &str,
    input: Value,
) -> Value {
    driver
        .execute_command(handle, id, input)
        .await
        .unwrap_or_else(|e| panic!("{id}: {e}"))
        .data
}

async fn integer(oracle: &mut PgConnection, sql: &str) -> i32 {
    sqlx::query_scalar::<_, i32>(sql)
        .fetch_one(oracle)
        .await
        .expect("independent server oracle")
}

#[tokio::test]
async fn catalog_dialect_discovers_and_migrates_real_postgres_objects() {
    let mut fixture = Fixture::open().await;
    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&fixture.config)
        .await
        .expect("driver connection");
    let dialect = driver.migration_renderer().expect("migration dialect");
    let dialect = dialect.as_ref();
    let schema = format!("dz_objects_{}", uuid::Uuid::new_v4().simple());
    let scoped = Some(schema.clone());
    let relation = format!("{schema}.events");
    sqlx::raw_sql(&format!(
        "CREATE SCHEMA {schema};
         CREATE TABLE {relation} (n int);
         CREATE FUNCTION {schema}.bump() RETURNS trigger LANGUAGE plpgsql
         AS $$ BEGIN NEW.n := NEW.n+1; RETURN NEW; END; $$;"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("isolated catalog fixture");

    let function = |factor| {
        MigrationRoutine {
        kind: ObjectKind::Function, schema: scoped.clone(), name: "scaled".into(),
        signature: Some("v integer".into()),
        definition: format!("CREATE OR REPLACE FUNCTION {schema}.scaled(v integer) RETURNS integer LANGUAGE SQL AS $$ SELECT v * {factor} $$"),
    }
    };
    let procedure = MigrationRoutine {
        kind: ObjectKind::Procedure, schema: scoped.clone(), name: "seed".into(),
        signature: Some(String::new()),
        definition: format!("CREATE OR REPLACE PROCEDURE {schema}.seed() LANGUAGE SQL AS $$ INSERT INTO {relation} VALUES (10) $$"),
    };
    let view = |limit| MigrationView {
        schema: scoped.clone(),
        name: "published".into(),
        definition: format!("SELECT n FROM {relation} WHERE n >= {limit}"),
    };
    let sequence = |start| MigrationSequence {
        schema: scoped.clone(),
        name: "ticket".into(),
        definition: format!(
            "CREATE SEQUENCE {schema}.ticket AS integer START WITH {start} INCREMENT BY 2 CACHE 1"
        ),
    };
    let type_definition = |labels| MigrationType {
        schema: scoped.clone(),
        name: "phase".into(),
        definition: format!("CREATE TYPE {schema}.phase AS ENUM ({labels})"),
    };
    let trigger = |when| {
        MigrationTrigger {
        // PostgreSQL trigger name is scoped by its relation, not namespace-
        // qualified independently. The target carries the precise schema.
        schema: None, name: "stamp".into(), target_schema: scoped.clone(), target_name: "events".into(),
        definition: format!("CREATE TRIGGER stamp BEFORE INSERT ON {relation} FOR EACH ROW WHEN ({when}) EXECUTE FUNCTION {schema}.bump()"),
    }
    };

    let first = function(2);
    let original_view = view(5);
    let original_sequence = sequence(101);
    let original_type = type_definition("'queued', 'ready'");
    let original_trigger = trigger("NEW.n > 0");
    for operation in [
        Op::CreateRoutine {
            routine: first.clone(),
        },
        Op::CreateRoutine {
            routine: procedure.clone(),
        },
        Op::CreateView {
            view: original_view.clone(),
        },
        Op::CreateSequence {
            sequence: original_sequence.clone(),
        },
        Op::CreateType {
            type_definition: original_type.clone(),
        },
        Op::CreateTrigger {
            trigger: original_trigger.clone(),
        },
    ] {
        assert!(apply(&driver, &handle, dialect, operation).await.is_some());
    }
    // Retain an overloaded function; exact identity is essential to DDL lookup.
    sqlx::query(&format!(
        "CREATE FUNCTION {schema}.scaled(text) RETURNS text LANGUAGE SQL AS $$ SELECT 'decoy'::text $$"
    ))
    .execute(&mut fixture.oracle)
    .await
    .expect("overloaded catalog decoy");
    execute(&driver, &handle, &format!("CALL {schema}.seed()")).await;
    assert_eq!(
        integer(
            &mut fixture.oracle,
            &format!("SELECT n FROM {schema}.published")
        )
        .await,
        11
    );
    assert_eq!(
        integer(&mut fixture.oracle, &format!("SELECT {schema}.scaled(7)")).await,
        14
    );
    assert_eq!(
        integer(
            &mut fixture.oracle,
            &format!("SELECT nextval('{schema}.ticket')::int")
        )
        .await,
        101
    );

    let mut requests = Vec::new();
    for (kind, name, text) in [
        ("function", "scaled", "v * 2"),
        ("procedure", "seed", "INSERT INTO"),
        ("view", "published", "n >= 5"),
        ("trigger", "stamp", "BEFORE INSERT"),
        ("sequence", "ticket", "START WITH 101"),
        ("type", "phase", "'queued', 'ready'"),
    ] {
        let catalog = command(
            &driver,
            &handle,
            "list_objects",
            json!({"kind": kind, "schema": schema}),
        )
        .await;
        let objects = catalog["objects"].as_array().expect("catalog object list");
        assert!(objects
            .iter()
            .any(|object| object["name"] == name && object["schema"] == schema));
        let mut request = json!({"kind": kind, "name": name, "schema": schema});
        if kind == "function" {
            assert_eq!(objects.iter().filter(|o| o["name"] == name).count(), 2);
            assert!(driver
                .execute_command(&handle, "get_object_ddl", request.clone())
                .await
                .is_err());
            request["signature"] = json!("v integer");
        }
        if kind == "trigger" {
            request["targetName"] = json!("events");
            request["targetSchema"] = json!(&schema);
        }
        let ddl = command(&driver, &handle, "get_object_ddl", request.clone()).await;
        assert!(
            ddl["ddl"].as_str().is_some_and(|sql| sql.contains(text)),
            "wrong {kind} definition: {ddl}"
        );
        requests.push(request);
    }
    let privileges = command(&driver, &handle, "list_privileges", json!({})).await;
    let owner = fixture.config.username.as_deref().expect("fixture role");
    assert!(privileges["grants"]
        .as_array()
        .expect("privileges")
        .iter()
        .any(|row| row["grantee"] == owner && row["privilege"] == "SUPERUSER"));

    let second = function(3);
    let next_view = view(70);
    let next_sequence = sequence(501);
    let next_type = type_definition("'waiting', 'complete'");
    let next_trigger = trigger("NEW.n > 100");
    for operation in [
        Op::ReplaceRoutine {
            current: first.clone(),
            desired: second.clone(),
        },
        Op::ReplaceView {
            current: original_view.clone(),
            desired: next_view.clone(),
        },
        Op::ReplaceSequence {
            current: original_sequence,
            desired: next_sequence.clone(),
        },
        Op::ReplaceType {
            current: original_type,
            desired: next_type.clone(),
        },
        Op::ReplaceTrigger {
            current: original_trigger,
            desired: next_trigger.clone(),
        },
    ] {
        let nonrecoverable = matches!(&operation, Op::ReplaceSequence { .. });
        assert_eq!(
            apply(&driver, &handle, dialect, operation).await.is_none(),
            nonrecoverable
        );
    }
    execute(&driver, &handle, &format!("CALL {schema}.seed()")).await;
    assert_eq!(
        integer(
            &mut fixture.oracle,
            &format!("SELECT min(n) FROM {relation}")
        )
        .await,
        10
    );
    assert_eq!(
        integer(
            &mut fixture.oracle,
            &format!("SELECT count(*)::int FROM {schema}.published")
        )
        .await,
        0
    );
    assert_eq!(
        integer(&mut fixture.oracle, &format!("SELECT {schema}.scaled(7)")).await,
        21
    );
    assert_eq!(
        integer(
            &mut fixture.oracle,
            &format!("SELECT nextval('{schema}.ticket')::int")
        )
        .await,
        501
    );
    let phase: String =
        sqlx::query_scalar(&format!("SELECT enum_range(NULL::{schema}.phase)::text"))
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("actual PostgreSQL enum order");
    assert_eq!(phase, "{waiting,complete}");

    for operation in [
        Op::DropView { view: next_view },
        Op::DropTrigger {
            trigger: next_trigger,
        },
        Op::DropRoutine { routine: second },
        Op::DropRoutine { routine: procedure },
        Op::DropSequence {
            sequence: next_sequence,
        },
        Op::DropType {
            type_definition: next_type,
        },
    ] {
        assert!(apply(&driver, &handle, dialect, operation).await.is_some());
    }
    for request in requests {
        assert!(driver
            .execute_command(&handle, "get_object_ddl", request)
            .await
            .is_err());
    }
    let text: String = sqlx::query_scalar(&format!("SELECT {schema}.scaled('x'::text)"))
        .fetch_one(&mut fixture.oracle)
        .await
        .expect("unselected overload remains");
    assert_eq!(text, "decoy");
    assert_eq!(
        integer(
            &mut fixture.oracle,
            &format!("SELECT count(*)::int FROM {relation}")
        )
        .await,
        2
    );
    driver.disconnect(handle).await.expect("driver cleanup");
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove catalog fixture");
}
