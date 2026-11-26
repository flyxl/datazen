use super::Fixture;
use datazen_driver_api::{BackupDumpOptions, DatabaseDriver, DumpPhase};
use datazen_driver_postgres::PostgresDriver;
use sqlx::PgConnection;

#[tokio::test]
async fn catalog_backup_ddl_restores_owned_sequence_state_schema_and_views() {
    let mut fixture = Fixture::open().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let from = format!("dz_fixture_backup_{tag}");
    let to = format!("dz_fixture_restore_{tag}");
    for name in [&from, &to] {
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&mut fixture.oracle)
            .await
            .expect("isolated writable backup round-trip database");
    }
    let mut origin = fixture.connect_to(&from).await;
    let mut restored = fixture.connect_to(&to).await;
    let schema = "backup_demo";
    let relation = format!("{schema}.orders");
    sqlx::raw_sql(&format!(r#"
       CREATE SCHEMA {schema};
       CREATE SEQUENCE {schema}.ticket_no AS bigint START 31 INCREMENT 3 CACHE 1;
       CREATE SEQUENCE {schema}.unused_no AS integer START -3 INCREMENT -2 MINVALUE -32 MAXVALUE -1 CACHE 1 CYCLE;
       CREATE TABLE {relation} (
          tenant text NOT NULL, ticket bigint NOT NULL DEFAULT nextval('{schema}.ticket_no'),
          spare integer NOT NULL DEFAULT nextval('{schema}.unused_no'),
          payload varchar(50) DEFAULT 'paid',
          PRIMARY KEY (tenant, ticket)
       );
       ALTER SEQUENCE {schema}.ticket_no OWNED BY {relation}.ticket;
       ALTER SEQUENCE {schema}.unused_no OWNED BY {relation}.spare;
       SELECT nextval('{schema}.ticket_no'), nextval('{schema}.ticket_no');
       CREATE VIEW {schema}.visible_orders AS SELECT tenant, ticket, payload FROM {relation} WHERE ticket > 0;
       CREATE FUNCTION {schema}.uppercase_payload() RETURNS trigger LANGUAGE plpgsql
       AS $$ BEGIN NEW.payload := upper(NEW.payload); RETURN NEW; END; $$;
       CREATE TRIGGER clean_payload BEFORE INSERT ON {relation}
       FOR EACH ROW EXECUTE FUNCTION {schema}.uppercase_payload();
    "#))
    .execute(&mut origin)
    .await.expect("seed PostgreSQL schema and usable DDL objects");

    let seq_state = async |conn: &mut PgConnection| -> ((i64, bool), (i64, bool)) {
        let state = sqlx::query_as("SELECT last_value, is_called FROM backup_demo.ticket_no")
            .fetch_one(&mut *conn)
            .await
            .expect("called PG sequence oracle");
        let fresh = sqlx::query_as("SELECT last_value, is_called FROM backup_demo.unused_no")
            .fetch_one(&mut *conn)
            .await
            .expect("uncalled PG sequence oracle");
        (state, fresh)
    };
    assert_eq!(seq_state(&mut origin).await, ((34, true), (-3, false)));
    // Fixed checkout with search_path produces unqualified pg_get_expr regclass
    // literals. The backup must restore those when the target uses its default.
    let driver = PostgresDriver::new();
    let mut read_config = fixture.config.clone();
    read_config.database = Some(from.clone());
    read_config.read_only = true;
    let handle = driver
        .connect_fixed_session(&read_config)
        .await
        .expect("read-only retained driver");
    driver
        .execute(&handle, &format!("SET search_path TO {schema}, pg_catalog"))
        .await
        .expect("source uses schema-local names only");
    driver
        .execute(&handle, "BEGIN READ ONLY")
        .await
        .expect("explicit retained read-only transaction");
    let table_sql = driver
        .dump_table_ddl(&handle, "orders", &from, Some(schema))
        .await
        .expect("live schema DDL");
    assert!(table_sql.contains("START WITH 37") && table_sql.contains("START WITH -3"));
    assert!(table_sql.contains("OWNED BY") && table_sql.contains("CYCLE"));
    let before_path = driver
        .query(&handle, "SHOW search_path")
        .await
        .expect("original source path")
        .rows;
    let view_sql = driver
        .dump_view_ddl(&handle, "visible_orders", &from, Some(schema))
        .await
        .expect("live view definition");
    assert_eq!(
        driver
            .query(&handle, "SHOW search_path")
            .await
            .expect("source path after rendering")
            .rows,
        before_path,
        "view backup must not mutate the retained search_path"
    );
    let routines = driver
        .dump_routines(&handle, &from)
        .await
        .expect("live routines output");
    let triggers = driver
        .dump_triggers(&handle, &from)
        .await
        .expect("live triggers output");
    assert_eq!(
        driver
            .query(&handle, "SHOW search_path")
            .await
            .expect("source path after trigger rendering")
            .rows,
        before_path,
        "trigger backup must not mutate the retained search_path"
    );
    assert!(routines.contains("uppercase_payload") && triggers.contains("clean_payload"));
    let mut progress = Vec::new();
    let full = driver
        .dump_database_with_progress(
            &handle,
            &from,
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
        .expect("schema-only database dump with live progress");
    assert!(full.starts_with(&format!("CREATE DATABASE \"{from}\";")));
    assert!(full.contains("CREATE OR REPLACE VIEW") && full.contains("CREATE TRIGGER"));
    assert!(!full.contains("INSERT INTO"));
    assert!(progress
        .iter()
        .any(|event| event.phase == DumpPhase::Object && event.object_name.contains("orders")));
    assert_eq!(
        driver
            .query(&handle, "SHOW search_path")
            .await
            .expect("path after complete dump")
            .rows,
        before_path
    );
    driver
        .execute(&handle, "ROLLBACK")
        .await
        .expect("finish retained read-only transaction");
    assert_eq!(
        seq_state(&mut origin).await,
        ((34, true), (-3, false)),
        "read-only backup must not advance source sequences"
    );

    sqlx::query(&format!("CREATE SCHEMA {schema};"))
        .execute(&mut restored)
        .await
        .expect("prepare separate target namespace");
    for script in [&table_sql, &view_sql, &routines, &triggers] {
        sqlx::raw_sql(script)
            .execute(&mut restored)
            .await
            .unwrap_or_else(|e| {
                panic!("generated script did not restore correctly: {e}: {script}")
            });
    }
    let actual = sqlx::query_as::<_, (i64, i32, String)>(&format!(
        "INSERT INTO {relation} (tenant) VALUES ('customer') RETURNING ticket,spare,payload"
    ))
    .fetch_one(&mut restored)
    .await
    .expect("restore SQL row behavior verified independently");
    assert_eq!(actual, (37, -3, "PAID".into()));
    let labels =
        sqlx::query_scalar::<_, String>(&format!("SELECT payload FROM {schema}.visible_orders"))
            .fetch_all(&mut restored)
            .await
            .expect("restored live view");
    assert_eq!(labels, ["PAID"]);
    let pk: Vec<String> = sqlx::query_scalar(
        "SELECT a.attname::text FROM pg_constraint c JOIN LATERAL unnest(c.conkey) WITH ORDINALITY k(attnum,pos) ON TRUE JOIN pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=k.attnum WHERE c.conrelid=to_regclass($1) AND c.contype='p' ORDER BY k.pos"
    ).bind(&relation).fetch_all(&mut restored).await.expect("physical composite primary key");
    assert_eq!(pk, ["tenant", "ticket"]);
    let owner: Option<String> = sqlx::query_scalar("SELECT pg_get_serial_sequence($1,'ticket')")
        .bind(&relation)
        .fetch_one(&mut restored)
        .await
        .expect("physical sequence ownership");
    assert_eq!(owner, Some(format!("{schema}.ticket_no")));
    assert_eq!(seq_state(&mut origin).await, ((34, true), (-3, false)));
    driver
        .disconnect(handle)
        .await
        .expect("release source session");
    drop(origin);
    drop(restored);
    for name in [&from, &to] {
        sqlx::query(&format!("DROP DATABASE {name}"))
            .execute(&mut fixture.oracle)
            .await
            .expect("remove isolated backup round-trip database");
    }
}

#[path = "../../../http-support/tests/support/backup_runtime.rs"]
mod runtime_host;

#[tokio::test]
async fn retained_backup_runtime_streams_and_restores_views_routine_and_trigger() {
    use datazen::{
        backup_gateway::backup_to_path, execution::target::*, jobs::CancelToken, shared::id::*,
        RequestContext,
    };
    use datazen_driver_postgres::PostgresResourceProvider;
    use std::sync::{atomic::Ordering, Arc};

    let mut fixture = Fixture::open().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    let origin_name = format!("dz_fixture_backup_live_{tag}");
    let target_name = format!("dz_fixture_backup_sink_{tag}");
    for name in [&origin_name, &target_name] {
        sqlx::query(&format!("CREATE DATABASE {name}"))
            .execute(&mut fixture.oracle)
            .await
            .expect("disposable backup source and restore target");
    }
    let mut source = fixture.connect_to(&origin_name).await;
    let mut target = fixture.connect_to(&target_name).await;
    let schema = "retained_demo";
    sqlx::raw_sql(
        r#"
        CREATE SCHEMA retained_demo;
        CREATE TABLE retained_demo.entries (id integer PRIMARY KEY, value text);
        INSERT INTO retained_demo.entries VALUES(1,'original');
        CREATE FUNCTION retained_demo.clean_entry() RETURNS trigger LANGUAGE plpgsql
            AS $$ BEGIN NEW.value := upper(NEW.value); RETURN NEW; END $$;
        CREATE TRIGGER uppercase_data BEFORE INSERT ON retained_demo.entries
            FOR EACH ROW EXECUTE FUNCTION retained_demo.clean_entry();
        CREATE VIEW retained_demo.live_view AS SELECT * FROM retained_demo.entries;
        CREATE MATERIALIZED VIEW retained_demo.saved_view AS SELECT * FROM retained_demo.entries;
    "#,
    )
    .execute(&mut source)
    .await
    .expect("independent source seed before retained artifact");

    let mut config = fixture.config.clone();
    config.database = Some(origin_name.clone());
    config.schema = Some(schema.into());
    let lifecycle = Arc::new(runtime_host::Lifecycle::default());
    let driver = Arc::new(PostgresDriver::new());
    let runtime = runtime_host::runtime(
        config,
        Arc::new(PostgresResourceProvider::new(driver)),
        lifecycle.clone(),
    );
    let ctx = RequestContext::new(
        OrganizationId::new("local"),
        PrincipalId::new("backup-owner"),
        None,
        ClientInstanceId::new("pg-e2e"),
        RequestId::new("pg-retained-restore"),
        None,
    );
    let endpoint = ExecutionTarget::new(
        ConnectionId::new("p"),
        NamespaceTarget::new(Some(origin_name.clone()), None, Some(schema.into()), vec![]),
    );
    let dir = tempfile::tempdir().expect("isolated SQL artifact");
    let path = dir.path().join("provider.sql");
    let result = backup_to_path(
        runtime.clone(),
        runtime_host::backup_binder(runtime.clone()),
        &ctx,
        endpoint.clone(),
        BackupDumpOptions {
            single_transaction: true,
            routines: true,
            triggers: true,
            create_database: true,
            ..Default::default()
        },
        &path,
        false,
        None,
        &CancelToken::new(),
    )
    .await;
    assert!(
        result.is_ok(),
        "real Runtime retained/PostgreSQL streaming backup"
    );
    let dump = std::fs::read_to_string(path).expect("retained filesystem artifact");
    let prefix = format!("CREATE DATABASE \"{origin_name}\";\n\n");
    let contents = dump
        .strip_prefix(&prefix)
        .expect("driver emits only selected actual database preamble");
    assert!(contents.contains("CREATE MATERIALIZED VIEW") && contents.contains("CREATE TRIGGER"));
    assert!(!contents.contains("statementMetadata"));
    assert!(contents.contains("CREATE SCHEMA IF NOT EXISTS \"retained_demo\""));
    sqlx::raw_sql(contents)
        .execute(&mut target)
        .await
        .unwrap_or_else(|error| {
            panic!("streamed backup should restore with unrelated search_path: {error}")
        });
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT value FROM retained_demo.saved_view WHERE id=1")
            .fetch_one(&mut target)
            .await
            .expect("materialized relation restored after source data"),
        "original"
    );
    let new: String =
        sqlx::query_scalar("INSERT INTO retained_demo.entries VALUES(2,'fixture') RETURNING value")
            .fetch_one(&mut target)
            .await
            .expect("restored trigger invokes the target routine");
    assert_eq!(new, "FIXTURE");
    let actual: Vec<(i32, String)> =
        sqlx::query_as("SELECT id,value FROM retained_demo.live_view ORDER BY id")
            .fetch_all(&mut target)
            .await
            .expect("real view references its surviving target table");
    assert_eq!(actual, [(1, "original".into()), (2, "FIXTURE".into())]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM retained_demo.entries")
            .fetch_one(&mut source)
            .await
            .expect("source remains unchanged"),
        1
    );
    assert_eq!(lifecycle.opened.load(Ordering::SeqCst), 1);
    assert_eq!(lifecycle.closed.load(Ordering::SeqCst), 1);
    struct CancelOnObject(CancelToken);
    impl datazen::backup::BackupEvents for CancelOnObject {
        fn backup_progress(&self, _: datazen_driver_api::DumpProgress) {
            self.0.cancel();
        }
        fn restore_progress(&self, _: datazen_driver_api::DumpProgress) {}
    }
    let token = CancelToken::new();
    let cancel_events = CancelOnObject(token.clone());
    let partial = dir.path().join("cancelled-snapshot.sql");
    assert!(backup_to_path(
        runtime.clone(),
        runtime_host::backup_binder(runtime.clone()),
        &ctx,
        endpoint.clone(),
        BackupDumpOptions {
            single_transaction: true,
            ..Default::default()
        },
        &partial,
        false,
        Some(Arc::new(cancel_events)),
        &token,
    )
    .await
    .is_err());
    assert!(
        !partial.exists(),
        "aborted real backup must not publish a partial artifact"
    );
    let conflicting = dir.path().join("conflicting-options.sql");
    assert!(backup_to_path(
        runtime.clone(),
        runtime_host::backup_binder(runtime.clone()),
        &ctx,
        endpoint,
        BackupDumpOptions {
            schema_only: true,
            data_only: true,
            ..Default::default()
        },
        &conflicting,
        false,
        None,
        &CancelToken::new(),
    )
    .await
    .is_err());
    assert!(!conflicting.exists());
    assert_eq!(
        lifecycle.opened.load(Ordering::SeqCst),
        lifecycle.closed.load(Ordering::SeqCst)
    );
    assert_eq!(sqlx::query_scalar::<_,i64>(
        "SELECT count(*) FROM pg_stat_activity WHERE datname=$1 AND state LIKE 'idle in transaction%'"
    ).bind(&origin_name).fetch_one(&mut fixture.oracle).await.expect("independent snapshot cleanup"),0);
    drop(runtime);
    drop(source);
    drop(target);
    for name in [&origin_name, &target_name] {
        sqlx::query(&format!("DROP DATABASE {name}"))
            .execute(&mut fixture.oracle)
            .await
            .expect("remove isolated backup DB");
    }
}
