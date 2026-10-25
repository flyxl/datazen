//! PostgreSQL restore format and provider opt-in. No server or credentials.
use datazen_driver_api::{iter_driver_factories, require_resource_provider, restore_sql};
use datazen_driver_postgres::PostgresDriver;

#[test]
fn postgres_restore_replaces_a_sequence_before_the_table_that_owns_it() {
    // The dump re-creates the sequence and then the table. In overwrite mode the
    // table preamble is a `DROP TABLE`, and a sequence still owned by that table
    // dies with it — so the sequence has to be dropped *first*, not left standing
    // for the table's `DROP` to take away.
    let driver = PostgresDriver::new();
    let dialect = datazen_driver_api::LegacyBackupDialect::new(&driver);
    let statements = restore_sql::prepare(
        &dialect,
        "CREATE SEQUENCE IF NOT EXISTS public.t_id_seq START WITH 2;\n\
         CREATE TABLE public.t (id integer NOT NULL DEFAULT nextval('public.t_id_seq'::regclass));\n\
         ALTER SEQUENCE public.t_id_seq OWNED BY public.t.id;",
        restore_sql::TRANSACTIONAL,
        true,
        true,
        &datazen_driver_api::namespace::NamespaceTarget {
            database: Some("app".into()),
            catalog: None,
            schema: Some("public".into()),
            path: vec![],
        },
    )
    .unwrap();
    // Every object is dropped up front, tables before sequences: the table's drop
    // is what releases the old sequence, and it must happen before the new
    // sequence is created.
    assert_eq!(
        statements[0], "DROP TABLE IF EXISTS public.t",
        "tables are dropped first: {statements:?}"
    );
    assert_eq!(statements[1], "DROP SEQUENCE IF EXISTS public.t_id_seq");
    assert!(statements[2].starts_with("CREATE SEQUENCE IF NOT EXISTS public.t_id_seq"));
    assert!(statements[3].starts_with("CREATE TABLE public.t"));
}

#[test]
fn postgres_restore_preflights_dollar_quotes_overwrite_and_transaction_policy() {
    let driver = PostgresDriver::new();
    let dialect = datazen_driver_api::LegacyBackupDialect::new(&driver);
    let profile = restore_sql::TRANSACTIONAL;
    let statements = restore_sql::prepare(&dialect,
        "-- fixture\nCREATE TABLE public.restore_probe(id int); INSERT INTO public.restore_probe VALUES (1); CREATE FUNCTION public.restore_text() RETURNS text LANGUAGE sql AS $$ SELECT 'a;b' $$;",
        profile, true, true, &datazen_driver_api::namespace::NamespaceTarget { database: Some("app".into()), catalog: None, schema: Some("public".into()), path: vec![] }).unwrap();
    assert_eq!(statements.len(), 4);
    assert_eq!(statements[0], "DROP TABLE IF EXISTS public.restore_probe");
    assert!(statements.last().unwrap().contains("'a;b'"));
    for unsupported in [
        "COPY public.t FROM stdin;\n1\n\\.",
        "USE other",
        "SET search_path=other",
        "BEGIN; INSERT INTO t VALUES(1); COMMIT",
        "\\connect other",
        "CREATE DATABASE other",
        "CREATE/**/DATABASE other",
    ] {
        let script = format!("CREATE TABLE public.restore_probe(id int); {unsupported};");
        assert!(
            restore_sql::prepare(
                &dialect,
                &script,
                profile,
                true,
                false,
                &datazen_driver_api::namespace::NamespaceTarget {
                    database: Some("app".into()),
                    catalog: None,
                    schema: Some("public".into()),
                    path: vec![]
                }
            )
            .is_err(),
            "unsupported input must reject the entire script"
        );
    }
    let factory = iter_driver_factories()
        .into_iter()
        .find(|factory| factory.driver_id() == "postgresql")
        .unwrap();
    let provider = require_resource_provider(*factory).unwrap();
    assert_eq!(
        provider.capabilities().snapshot.confirmed[restore_sql::CAPABILITY_KEY],
        profile
    );
}

#[derive(Default)]
struct Budget(std::sync::atomic::AtomicU32);
#[async_trait::async_trait]
impl datazen_driver_api::resource::BudgetPort for Budget {
    async fn acquire_control_connection(
        &self,
    ) -> Result<
        datazen_driver_api::resource::BudgetPermit,
        datazen_driver_api::resource::ResourceError,
    > {
        self.acquire_physical_connections(1).await
    }
    async fn acquire_physical_connections(
        &self,
        count: u32,
    ) -> Result<
        datazen_driver_api::resource::BudgetPermit,
        datazen_driver_api::resource::ResourceError,
    > {
        let id = self.0.fetch_add(count, std::sync::atomic::Ordering::SeqCst);
        Ok(datazen_driver_api::resource::BudgetPermit {
            permit_id: format!("restore-{id}"),
            physical_connections: count,
        })
    }
    async fn release_physical_connections(
        &self,
        permit: &datazen_driver_api::resource::BudgetPermit,
    ) -> Result<(), datazen_driver_api::resource::ResourceError> {
        self.0.fetch_sub(
            permit.physical_connections,
            std::sync::atomic::Ordering::SeqCst,
        );
        Ok(())
    }
}
#[derive(Default)]
struct Sink(std::sync::atomic::AtomicUsize);
#[async_trait::async_trait]
impl datazen_driver_api::resource::ResultSink for Sink {
    async fn write(
        &self,
        chunk: datazen_driver_api::resource::ResultChunk,
    ) -> Result<(), datazen_driver_api::resource::ResourceError> {
        self.0
            .fetch_add(chunk.rows.len(), std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    async fn write_output(
        &self,
        _: serde_json::Value,
    ) -> Result<(), datazen_driver_api::resource::ResourceError> {
        Ok(())
    }
    async fn complete(&self) -> Result<(), datazen_driver_api::resource::ResourceError> {
        Ok(())
    }
    async fn fail(&self, _: &str) -> Result<(), datazen_driver_api::resource::ResourceError> {
        Ok(())
    }
}

/// Explicit live journey; reads only process environment, and never prints errors or config.
#[tokio::test]
#[ignore = "requires a disposable TEST_PG_DATABASE and TEST_PG_HOST in process environment"]
async fn retained_resource_executes_restore_and_verifies_rows_before_closing() {
    use datazen_driver_api::resource::*;
    use datazen_driver_api::session::{CompletionStatus, ObservationConfidence};
    use std::sync::Arc;
    let required = |name: &str| {
        std::env::var(format!("TEST_PG_{name}"))
            .unwrap_or_else(|_| panic!("missing live fixture environment"))
    };
    let config: datazen_driver_api::ConnectionConfig = serde_json::from_value(serde_json::json!({
        "id":"restore-live", "name":"restore-live", "databaseType":"postgresql",
        "host":required("HOST"), "port":std::env::var("TEST_PG_PORT").ok().and_then(|p|p.parse::<u16>().ok()).unwrap_or(5432),
        "database":required("DATABASE"), "schema":Some("public"),
        "username":required("USER"), "password":std::env::var("TEST_PG_PASSWORD").ok(),
        "connectionTimeout":5, "maxPoolSize":1, "readOnly":false, "pinned":false
    })).unwrap_or_else(|_| panic!("invalid synthetic fixture configuration"));
    let factory = iter_driver_factories()
        .into_iter()
        .find(|f| f.driver_id() == "postgresql")
        .unwrap();
    let driver = factory.create();
    let provider = require_resource_provider(*factory).unwrap();
    let budget = Arc::new(Budget::default());
    let port: Arc<dyn BudgetPort> = budget.clone();
    let handle = provider
        .acquire_resource(
            &AcquireResourceRequest {
                connection_config: config.clone(),
                identity_scope: IdentityScope::default(),
                target: datazen_driver_api::namespace::NamespaceTarget {
                    database: config.database.clone(),
                    catalog: None,
                    schema: config.schema.clone(),
                    path: vec![],
                },
                scope: ResourceScope {
                    purpose: ResourcePurpose::InteractiveQuery,
                    max_physical_connections: 4,
                    holds_open_transaction: false,
                    pin_for_streaming: false,
                },
                baseline: Baseline::default(),
            },
            &port,
        )
        .await
        .unwrap_or_else(|_| panic!("restore fixture resource acquisition failed"));
    let observation = provider
        .observe_session(&handle)
        .await
        .unwrap_or_else(|_| panic!("restore fixture observation failed"));
    assert_eq!(
        observation.context.confidence,
        ObservationConfidence::Confirmed
    );
    assert_eq!(observation.context.namespace.database, config.database);
    let table = format!("public.restore_probe_{}", uuid::Uuid::new_v4().simple());
    let script = format!("CREATE TABLE {table}(id int PRIMARY KEY); INSERT INTO {table} VALUES(7); INSERT INTO {table} VALUES(8);");
    let profile = provider.capabilities().snapshot.confirmed[restore_sql::CAPABILITY_KEY].clone();
    let dialect = datazen_driver_api::LegacyBackupDialect::new(driver.as_ref());
    let statements = restore_sql::prepare(
        &dialect,
        &script,
        &profile,
        false,
        true,
        &datazen_driver_api::namespace::NamespaceTarget {
            database: Some("app".into()),
            catalog: None,
            schema: Some("public".into()),
            path: vec![],
        },
    )
    .unwrap();
    for (index, sql) in statements.into_iter().enumerate() {
        let result = provider
            .execute_on_resource(
                &handle,
                &datazen_driver_api::QueryExecutionId::new(format!("restore-{index}")),
                &CommandCall::new("execute", serde_json::json!({"sql":sql})),
                &Sink::default(),
            )
            .await;
        assert!(result.is_ok(), "retained restore statement failed");
        assert_eq!(
            result.unwrap().completion_status,
            CompletionStatus::Succeeded
        );
    }
    let verification_sink = Sink::default();
    let verified = provider
        .execute_on_resource(
            &handle,
            &datazen_driver_api::QueryExecutionId::new("restore-verify"),
            &CommandCall::new(
                "query",
                serde_json::json!({"sql":format!("SELECT id FROM {table} ORDER BY id")}),
            ),
            &verification_sink,
        )
        .await
        .unwrap_or_else(|_| panic!("restore verification failed"));
    assert_eq!(verified.completion_status, CompletionStatus::Succeeded);
    assert_eq!(
        verification_sink
            .0
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );
    let cleanup = provider
        .execute_on_resource(
            &handle,
            &datazen_driver_api::QueryExecutionId::new("restore-cleanup"),
            &CommandCall::new(
                "execute",
                serde_json::json!({"sql":format!("DROP TABLE {table}")}),
            ),
            &Sink::default(),
        )
        .await;
    assert!(cleanup.is_ok(), "restore fixture cleanup failed");
    assert!(provider.close_resource(&handle).await.is_ok());
    assert_eq!(budget.0.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn frozen_restore_target_rejects_every_foreign_object_before_returning_any_script() {
    let driver = PostgresDriver::new();
    let dialect = datazen_driver_api::LegacyBackupDialect::new(&driver);
    let target = datazen_driver_api::namespace::NamespaceTarget {
        database: Some("app".into()),
        catalog: None,
        schema: Some("public".into()),
        path: vec![],
    };
    let own = "CREATE TABLE public.probe(id int); INSERT INTO public.probe VALUES(1); DROP TABLE public.probe;";
    assert!(restore_sql::prepare(
        &dialect,
        own,
        restore_sql::TRANSACTIONAL,
        false,
        false,
        &target
    )
    .is_ok());
    for foreign in [
        "CREATE TABLE other.probe(id int)",
        "INSERT INTO other.probe VALUES(1)",
        "INSERT INTO victim.public.probe VALUES(1)",
        "CREATE TABLE public.probe(id int) INHERITS(other.parent)",
        "DROP TABLE public.probe, other.probe",
        "CREATE VIEW other.probe AS SELECT 1",
        "CREATE TABLE \"public.other\".probe(id int)",
        "ALTER TABLE public.probe RENAME TO other.probe",
    ] {
        // A valid first statement must never cause execution before discovering
        // a later foreign target. The entire preflight returns no script.
        let script = format!("CREATE TABLE public.first_probe(id int); {foreign};");
        for overwrite in [false, true] {
            assert!(restore_sql::prepare(
                &dialect,
                &script,
                restore_sql::TRANSACTIONAL,
                false,
                overwrite,
                &target
            )
            .is_err());
        }
    }
}

#[test]
fn materialized_view_overwrite_is_rejected_during_whole_script_preflight() {
    let driver = PostgresDriver::new();
    let dialect = datazen_driver_api::LegacyBackupDialect::new(&driver);
    let target = datazen_driver_api::namespace::NamespaceTarget {
        database: Some("app".into()),
        catalog: None,
        schema: Some("public".into()),
        path: vec![],
    };
    let script = "CREATE TABLE public.first_probe(id int); CREATE MATERIALIZED VIEW public.probe AS SELECT 1 AS id;";
    assert!(restore_sql::prepare(
        &dialect,
        script,
        restore_sql::TRANSACTIONAL,
        false,
        false,
        &target
    )
    .is_ok());
    assert!(restore_sql::prepare(
        &dialect,
        script,
        restore_sql::TRANSACTIONAL,
        false,
        true,
        &target
    )
    .is_err());
    let case_target = datazen_driver_api::namespace::NamespaceTarget {
        schema: Some("CaseSchema".into()),
        ..target
    };
    assert!(restore_sql::prepare(
        &dialect,
        "CREATE TABLE \"CaseSchema\".probe(id int);",
        restore_sql::TRANSACTIONAL,
        false,
        false,
        &case_target
    )
    .is_ok());
    assert!(restore_sql::prepare(
        &dialect,
        "CREATE TABLE CaseSchema.probe(id int);",
        restore_sql::TRANSACTIONAL,
        false,
        false,
        &case_target
    )
    .is_err());
}

#[test]
fn custom_schema_restore_never_uses_public_search_path_fallback() {
    let driver = PostgresDriver::new();
    let dialect = datazen_driver_api::LegacyBackupDialect::new(&driver);
    let target = datazen_driver_api::namespace::NamespaceTarget {
        database: Some("app".into()),
        catalog: None,
        schema: Some("custom".into()),
        path: vec![],
    };
    // Production connection initialization is search_path=custom,public. When
    // only public.probe exists, these lookups would resolve outside the plan.
    for sql in [
        "CREATE TABLE probe(id int)",
        "DROP TABLE IF EXISTS probe",
        "INSERT INTO probe VALUES(1)",
        "UPDATE probe SET id=2",
        "DELETE FROM probe",
    ] {
        for overwrite in [false, true] {
            let script = format!("CREATE TABLE custom.first_probe(id int); {sql};");
            assert!(restore_sql::prepare(
                &dialect,
                &script,
                restore_sql::TRANSACTIONAL,
                false,
                overwrite,
                &target
            )
            .is_err());
        }
    }
    for sql in [
        "CREATE TABLE custom.probe(id int)",
        "DROP TABLE IF EXISTS custom.probe RESTRICT",
        "INSERT INTO custom.probe VALUES(1)",
        "UPDATE custom.probe SET id=2",
        "DELETE FROM custom.probe",
        "CREATE INDEX probe_idx ON custom.probe(id)",
    ] {
        assert!(
            restore_sql::prepare(
                &dialect,
                sql,
                restore_sql::TRANSACTIONAL,
                false,
                false,
                &target
            )
            .is_ok(),
            "qualified target should pass: {sql}"
        );
    }
    let prepared = restore_sql::prepare(
        &dialect,
        "CREATE TABLE custom.probe(id int);",
        restore_sql::TRANSACTIONAL,
        false,
        true,
        &target,
    )
    .unwrap();
    assert_eq!(prepared[0], "DROP TABLE IF EXISTS custom.probe");
}

#[test]
fn cascading_effects_are_rejected_before_any_script_write() {
    let driver = PostgresDriver::new();
    let dialect = datazen_driver_api::LegacyBackupDialect::new(&driver);
    let target = datazen_driver_api::namespace::NamespaceTarget {
        database: Some("app".into()),
        catalog: None,
        schema: Some("custom".into()),
        path: vec![],
    };
    for cascade in [
        "DROP TABLE custom.probe CASCADE",
        "DROP VIEW custom.probe CASCADE",
        "TRUNCATE TABLE custom.probe CASCADE",
        "ALTER TABLE custom.probe DROP COLUMN id CASCADE",
        "ALTER TABLE custom.probe DROP CONSTRAINT probe_fk CASCADE",
    ] {
        let script = format!("CREATE TABLE custom.first_probe(id int); {cascade};");
        assert!(restore_sql::prepare(
            &dialect,
            &script,
            restore_sql::TRANSACTIONAL,
            false,
            false,
            &target
        )
        .is_err());
    }
    for restrict in [
        "DROP TABLE custom.probe RESTRICT",
        "TRUNCATE TABLE custom.probe RESTRICT",
        "ALTER TABLE custom.probe DROP COLUMN id RESTRICT",
    ] {
        assert!(restore_sql::prepare(
            &dialect,
            restrict,
            restore_sql::TRANSACTIONAL,
            false,
            false,
            &target
        )
        .is_ok());
    }
}

#[test]
fn postgres_three_part_overwrite_names_are_rejected_before_drop_generation() {
    let driver = PostgresDriver::new();
    let dialect = datazen_driver_api::LegacyBackupDialect::new(&driver);
    let target = datazen_driver_api::namespace::NamespaceTarget {
        database: Some("app".into()),
        catalog: None,
        schema: Some("custom".into()),
        path: vec![],
    };
    for name in ["app.custom.probe", "\"app\".\"custom\".\"probe\""] {
        let sql = format!("CREATE TABLE custom.first_probe(id int); CREATE TABLE {name}(id int);");
        for overwrite in [true, false] {
            // Returning Err prevents Gateway from receiving any executable
            // script, including the old wrongly truncated DROP app.custom.
            assert!(
                restore_sql::prepare(
                    &dialect,
                    &sql,
                    restore_sql::TRANSACTIONAL,
                    false,
                    overwrite,
                    &target
                )
                .is_err(),
                "three-part names must never generate an overwrite script"
            );
        }
    }
    let qualified = restore_sql::prepare(
        &dialect,
        "CREATE TABLE custom.probe(id int);",
        restore_sql::TRANSACTIONAL,
        false,
        true,
        &target,
    )
    .unwrap();
    assert_eq!(qualified[0], "DROP TABLE IF EXISTS custom.probe");
}

/// The product's own dump defers `OWNED BY` to a separate `ALTER SEQUENCE`
/// statement; the pinned sqlparser cannot parse that form, so the restore
/// preflight verifies it directly. A dump containing a serial column used to be
/// unrestorable because of exactly this statement.
#[test]
fn postgres_restore_accepts_deferred_sequence_ownership_inside_the_frozen_scope() {
    let driver = PostgresDriver::new();
    let dialect = datazen_driver_api::LegacyBackupDialect::new(&driver);
    let target = datazen_driver_api::namespace::NamespaceTarget {
        database: Some("app".into()),
        catalog: None,
        schema: Some("public".into()),
        path: vec![],
    };
    let script = concat!(
        "CREATE TABLE public.orders (id bigint NOT NULL);\n",
        "CREATE SEQUENCE public.orders_id_seq AS bigint START WITH 1;\n",
        "ALTER SEQUENCE public.orders_id_seq OWNED BY public.orders.id;\n",
    );
    let statements = restore_sql::prepare(
        &dialect,
        script,
        restore_sql::TRANSACTIONAL,
        false,
        false,
        &target,
    )
    .expect("deferred sequence ownership is part of the restore format");
    assert_eq!(statements.len(), 3);

    // Quoted identifiers are legal names — the tokenizer folds them into
    // words, so a dump that quotes its names still restores.
    let quoted = concat!(
        "CREATE TABLE public.orders (id bigint NOT NULL);\n",
        "ALTER SEQUENCE \"public\".\"orders_id_seq\" OWNED BY \"public\".\"orders\".id;\n",
    );
    restore_sql::prepare(
        &dialect,
        quoted,
        restore_sql::TRANSACTIONAL,
        false,
        false,
        &target,
    )
    .expect("quoted identifiers within the frozen namespace restore fine");

    for foreign in [
        // Another schema is outside the frozen target.
        "ALTER SEQUENCE other.orders_id_seq OWNED BY public.orders.id",
        "ALTER SEQUENCE public.orders_id_seq OWNED BY other.orders.id",
        // Any other ALTER SEQUENCE form must stay rejected.
        "ALTER SEQUENCE public.orders_id_seq RESTART WITH 1000",
        // A single-quoted string is never an identifier in any engine this
        // gates; the verifier must not be looser than the engines.
        "ALTER SEQUENCE 'public'.orders_id_seq OWNED BY public.orders.id",
        "ALTER SEQUENCE public.orders_id_seq OWNED BY public.'orders'.id",
    ] {
        let script = format!("CREATE TABLE public.orders (id bigint NOT NULL); {foreign};");
        assert!(
            restore_sql::prepare(
                &dialect,
                &script,
                restore_sql::TRANSACTIONAL,
                false,
                false,
                &target
            )
            .is_err(),
            "{foreign} must be rejected"
        );
    }
}

/// The overwrite preamble for a sequence must appear even when a comment line
/// leads the statement: the old name scanner started at the raw text and read
/// `--` as the identifier, silently skipping the `DROP SEQUENCE`.
#[test]
fn postgres_overwrite_drops_a_sequence_whose_statement_leads_with_a_comment() {
    let driver = PostgresDriver::new();
    let dialect = datazen_driver_api::LegacyBackupDialect::new(&driver);
    let statements = restore_sql::prepare(
        &dialect,
        "-- emitted by the dump\nCREATE SEQUENCE public.orders_id_seq START WITH 1;",
        restore_sql::TRANSACTIONAL,
        false,
        true,
        &datazen_driver_api::namespace::NamespaceTarget {
            database: Some("app".into()),
            catalog: None,
            schema: Some("public".into()),
            path: vec![],
        },
    )
    .unwrap();
    assert_eq!(
        statements[0], "DROP SEQUENCE IF EXISTS public.orders_id_seq",
        "the sequence preamble must survive a leading comment: {statements:?}"
    );
}
