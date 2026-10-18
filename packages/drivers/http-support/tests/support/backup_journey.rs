//! Real driver slice: production backup gateway/renderer and Runtime execution.
//! Test host ports replace desktop wiring; native picker and FileArtifactJob
//! delivery are not represented as live desktop coverage.
#[path = "backup_runtime.rs"]
mod host;
use datazen_driver_api::*;
use datazen_feature_core::{backup::BackupEvents, backup_gateway::backup_to_path};
use datazen_platform_api::{id::*, target::*, RequestContext};
use datazen_runtime::job::CancelToken;
use std::{
    io::Read,
    sync::{atomic::Ordering, Arc},
};
struct CancelOnObject(CancelToken);
impl BackupEvents for CancelOnObject {
    fn backup_progress(&self, _: DumpProgress) {
        self.0.cancel();
    }
    fn restore_progress(&self, _: DumpProgress) {}
}
pub async fn run(
    mut config: ConnectionConfig,
    driver: Arc<dyn DatabaseDriver>,
    provider: Arc<dyn resource::ResourceProvider>,
) {
    let postgres = config.database_type == "postgresql";
    config.max_pool_size = 1;
    let admin = driver
        .connect(&config)
        .await
        .unwrap_or_else(|_| panic!("fixture unavailable"));
    let database = format!("dz_mig_backup_{}", uuid::Uuid::new_v4().simple());
    assert!(
        driver
            .execute(
                &admin,
                &format!("CREATE DATABASE {}", driver.quote_ident(&database))
            )
            .await
            .is_ok(),
        "fixture create database"
    );
    config.database = Some(database.clone());
    config.schema = if postgres {
        Some("public".into())
    } else {
        None
    };
    let observer = driver
        .connect(&config)
        .await
        .unwrap_or_else(|_| panic!("isolated fixture unavailable"));
    assert!(driver
        .execute(
            &observer,
            "CREATE TABLE backup_probe (id INTEGER PRIMARY KEY, payload TEXT)"
        )
        .await
        .is_ok());
    assert!(driver
        .execute(
            &observer,
            "INSERT INTO backup_probe VALUES (1, 'bounded backup ✓'), (2, 'second row')"
        )
        .await
        .is_ok());
    let lifecycle = Arc::new(host::Lifecycle::default());
    let runtime = host::runtime(config.clone(), provider, lifecycle.clone());
    let ctx = RequestContext::new(
        OrganizationId::new("local"),
        PrincipalId::new("backup-owner"),
        None,
        ClientInstanceId::new("backup-client"),
        RequestId::new("backup-live"),
        None,
    );
    let target = ExecutionTarget::new(
        ConnectionId::new("p"),
        NamespaceTarget::new(Some(database.clone()), None, config.schema.clone(), vec![]),
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("backup.sql.gz");
    let result = backup_to_path(
        runtime.clone(),
        &ctx,
        target.clone(),
        BackupDumpOptions {
            single_transaction: true,
            ..Default::default()
        },
        &path,
        true,
        None,
        &CancelToken::new(),
    )
    .await;
    assert!(result.is_ok(), "Runtime backup succeeds");
    let execution = datazen_application::sessions::ConnectionUseCases::get_execution(
        &*runtime,
        &ctx,
        result.unwrap(),
    )
    .await
    .unwrap();
    assert!(
        execution.artifact_ids.is_empty(),
        "no intermediate Runtime artifact"
    );
    assert_eq!(
        execution.state,
        datazen_platform_api::dto::execution::ExecutionState::Succeeded
    );
    let mut dump = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(&path).unwrap())
        .read_to_string(&mut dump)
        .unwrap();
    assert!(dump.contains("CREATE TABLE"));
    assert!(dump.contains("INSERT INTO"));
    assert!(dump.contains("bounded backup ✓"));
    let statements = sql_dump::split_sql_statements(&dump);
    assert!(statements.iter().any(|sql| sql.contains("INSERT INTO")));
    assert!(driver
        .execute(&observer, "DROP TABLE backup_probe")
        .await
        .is_ok());
    for statement in &statements {
        assert!(
            driver.execute(&observer, statement).await.is_ok(),
            "generated backup restores"
        );
    }
    let read = driver
        .query(&observer, "SELECT id,payload FROM backup_probe ORDER BY id")
        .await
        .unwrap_or_else(|_| panic!("independent readback failed"));
    assert_eq!(read.rows.len(), 2);
    assert_eq!(
        read.rows[0][1]
            .as_ref()
            .and_then(|v| if let Value::String(s) = v {
                Some(s.as_str())
            } else {
                None
            }),
        Some("bounded backup ✓")
    );
    assert_eq!(lifecycle.opened.load(Ordering::SeqCst), 1);
    assert_eq!(lifecycle.closed.load(Ordering::SeqCst), 1);
    // Cancellation occurs after metadata on the real retained resource, before
    // data generation; the synchronous sink must reject subsequent output.
    let cancelled = CancelToken::new();
    let cancelled_path = dir.path().join("cancelled.sql");
    assert!(backup_to_path(
        runtime.clone(),
        &ctx,
        target.clone(),
        Default::default(),
        &cancelled_path,
        false,
        Some(Arc::new(CancelOnObject(cancelled.clone()))),
        &cancelled
    )
    .await
    .is_err());
    assert!(!cancelled_path.exists());
    // An oversized single value is an explicit bounded-render failure. It must
    // not publish a partial success or leave a staging file behind.
    let oversized = "x".repeat(sql_dump::INSERT_BATCH_MAX_BYTES + 1);
    assert!(driver
        .execute(
            &observer,
            &format!(
                "INSERT INTO backup_probe VALUES (3, {})",
                driver.format_sql_literal(&Some(Value::String(oversized)))
            )
        )
        .await
        .is_ok());
    let oversized_path = dir.path().join("oversized.sql.gz");
    assert!(backup_to_path(
        runtime.clone(),
        &ctx,
        target,
        Default::default(),
        &oversized_path,
        true,
        None,
        &CancelToken::new()
    )
    .await
    .is_err());
    assert!(!oversized_path.exists());
    assert_eq!(lifecycle.opened.load(Ordering::SeqCst), 3);
    assert_eq!(lifecycle.closed.load(Ordering::SeqCst), 3);
    assert!(
        driver
            .query(&observer, "SELECT COUNT(*) FROM backup_probe")
            .await
            .is_ok(),
        "source survives failed/cancelled backup"
    );
    driver.disconnect(observer).await.unwrap();
    assert!(driver
        .execute(
            &admin,
            &format!("DROP DATABASE {}", driver.quote_ident(&database))
        )
        .await
        .is_ok());
    driver.disconnect(admin).await.unwrap();
}
