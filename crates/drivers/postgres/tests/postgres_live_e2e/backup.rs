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
