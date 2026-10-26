//! Host-generic export lifecycle; SQL fixtures use only common PG/MySQL syntax.
use super::*;
use datazen_application::{dto::requests::*, sessions::ConnectionUseCases};
use datazen_driver_mysql as _;
use datazen_driver_postgres as _;
use datazen_feature_core::file_job::*;
use datazen_modules::export::ExportError as FeatureError;
use datazen_platform_api::{
    context::OwnerRef,
    dto::{
        artifact::ArtifactLifecycle, idempotency::IdempotentOperation, session::ResourceRelease,
    },
    id::*,
    ports::artifact::ArtifactRepository,
    target::*,
};
use datazen_runtime::{
    artifact::FileArtifactRepository,
    job::{JobHandler, StageSpec, StageTerminal},
};
use futures_util::FutureExt;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};

#[derive(Default)]
struct Observed {
    batches: Mutex<Vec<usize>>,
    done: AtomicBool,
    rows: AtomicUsize,
}
struct Source {
    desktop: DesktopExportSource,
    observed: Arc<Observed>,
    cancel_after_batch: bool,
}
#[async_trait::async_trait]
impl ExportSource for Source {
    async fn resolve_source(
        &self,
        id: &str,
    ) -> Result<(Arc<dyn DatabaseDriver>, bool, bool), FeatureError> {
        self.desktop.resolve_source(id).await
    }
    async fn stream_query(
        &self,
        id: &str,
        sql: &str,
        target: datazen_driver_api::SqlTarget<'_>,
        callback: datazen_driver_api::QueryStreamCallback,
    ) -> Result<(), FeatureError> {
        let observed = self.observed.clone();
        let cancel = self.desktop.cancel.clone();
        let cancel_after_batch = self.cancel_after_batch;
        self.desktop
            .stream_query(
                id,
                sql,
                target,
                Arc::new(move |event| {
                    if let datazen_driver_api::QueryStreamEvent::Rows { rows, .. } = &event {
                        assert!(rows.len() <= 256, "unbounded export row batch");
                        assert!(serde_json::to_vec(rows).unwrap().len() <= 2 * 1024 * 1024);
                        observed.batches.lock().unwrap().push(rows.len());
                        observed.rows.fetch_add(rows.len(), Ordering::SeqCst);
                        // Real formatter consumes synchronously on the blocking pool.
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    if matches!(event, datazen_driver_api::QueryStreamEvent::Done { .. }) {
                        observed.done.store(true, Ordering::SeqCst);
                    }
                    let batch = matches!(event, datazen_driver_api::QueryStreamEvent::Rows { .. });
                    callback(event);
                    if batch && cancel_after_batch {
                        cancel.cancel();
                    }
                }),
            )
            .await
    }
}
struct Events;
impl ExportEvents for Events {
    fn progress_callback(&self) -> Arc<dyn Fn(ExportProgressEvent) + Send + Sync> {
        Arc::new(|_| {})
    }
}
struct Operation {
    state: Arc<AppState>,
    request: ExportTablesRequest,
    observed: Arc<Observed>,
    staging: tempfile::TempDir,
    cancel_after_batch: bool,
}
#[async_trait::async_trait]
impl FileJobOperation for Operation {
    async fn run(
        &self,
        cancel: &datazen_runtime::job::CancelToken,
    ) -> Result<FileJobOutput, FeatureError> {
        let source = Source {
            desktop: DesktopExportSource {
                platform: self.state.platform.clone(),
                runtime: crate::services::runtime_command_source::RuntimeCommandSource::new(
                    self.state.driver_registry.clone(),
                    self.state.store.clone(),
                    self.state.platform.clone(),
                ),
                registry: self.state.driver_registry.clone(),
                cancel: cancel.clone(),
            },
            observed: self.observed.clone(),
            cancel_after_batch: self.cancel_after_batch,
        };
        let plans = prepare_export(&self.request)?.0;
        let path = self.staging.path().join("staged.json");
        let rows = run_single_export(&Events, &source, &self.request, &plans, &path).await?;
        Ok(FileJobOutput { path, rows })
    }
}
fn setting(prefix: &str, key: &str) -> String {
    std::env::var(format!("{prefix}{key}"))
        .unwrap_or_else(|_| panic!("live prerequisite missing: {prefix}{key}"))
}
async fn journey(prefix: &str, driver_id: &str) {
    assert_eq!(
        std::env::var("DATAZEN_CONTRACT_REQUIRE_LIVE").as_deref(),
        Ok("1")
    );
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(
        crate::store::DesktopRepository::init_with_path(
            temp.path(),
            crate::store::KeyBackend::File,
        )
        .await
        .unwrap(),
    );
    let mut settings = store.get_settings().await;
    settings.limit_select_results = true;
    settings.query_result_limit = 32;
    store.save_settings(settings).await.unwrap();
    let registry = Arc::new(datazen_runtime::driver_registry::DriverRegistry::new());
    let mut config = crate::testing::app_state::sample_postgres_config("export-live");
    config.database_type = driver_id.into();
    config.host = Some(setting(prefix, "HOST"));
    config.port = Some(setting(prefix, "PORT").parse().expect("numeric live port"));
    let database = setting(prefix, "DATABASE");
    assert!(database.starts_with("dz_fixture_"));
    config.database = Some(database.clone());
    config.schema = (driver_id == "postgresql").then(|| "public".into());
    config.username = Some(setting(prefix, "USER"));
    config.password = Some(std::env::var(format!("{prefix}PASSWORD")).unwrap_or_default());
    config.ssl_mode = datazen_driver_api::SslMode::Disable;
    store.save_connection(config.clone()).await.unwrap();
    let driver = registry
        .get(&config.database_type)
        .await
        .expect("real inventory driver");
    let observer = driver
        .connect(&config)
        .await
        .unwrap_or_else(|_| panic!("fixture unavailable"));
    let table = format!("dz_export_{}", uuid::Uuid::new_v4().simple());
    let state = Arc::new(crate::testing::app_state::build_app_state(store, registry));
    let adapter = state.platform_require().unwrap();
    let runtime = adapter.runtime().unwrap();
    let ctx = adapter.begin_call().context().clone();
    let token = runtime
        .issue_submission_token(&ctx, IdempotentOperation::OpenSession, None)
        .await
        .unwrap();
    let opened = runtime
        .open_session(
            &ctx,
            OpenSessionRequest::new(
                ExecutionTarget::new(
                    ConnectionId::new(&config.id),
                    NamespaceTarget::new(
                        Some(database.clone()),
                        None,
                        config.schema.clone(),
                        vec![],
                    ),
                ),
                OwnerRef::ClientSession {
                    client_instance_id: ctx.client_instance_id.clone(),
                    purpose: "exportLiveFixture".into(),
                },
                token.idempotency_key,
            ),
        )
        .await
        .unwrap();
    let handle = opened.session.handle;
    let result = std::panic::AssertUnwindSafe(async {
        assert!(driver
            .execute(
                &observer,
                &format!("CREATE TABLE {table}(id INTEGER NOT NULL)")
            )
            .await
            .is_ok());
        for start in [0, 500, 1000] {
            let values = (start..(start + 500).min(1025))
                .map(|id| format!("({id})"))
                .collect::<Vec<_>>()
                .join(",");
            assert!(driver
                .execute(&observer, &format!("INSERT INTO {table} VALUES {values}"))
                .await
                .is_ok());
        }
        let interactive = crate::commands::query::execute_query_impl(
            &state,
            handle.db_session_id.as_str().into(),
            format!("SELECT id FROM {table}"),
            Some(database.clone()),
        )
        .await
        .unwrap_or_else(|_| panic!("interactive query failed"));
        assert_eq!(interactive.results[0].rows.len(), 32);
        assert!(interactive.results[0].truncated);
        for cancel_after_batch in [false, true] {
            let observed = Arc::new(Observed::default());
            let staging = tempfile::tempdir().unwrap();
            let staging_path = staging.path().to_owned();
            let request = ExportTablesRequest {
                db_session_id: handle.db_session_id.as_str().into(),
                database_type: Some(driver_id.into()),
                database: Some(database.clone()),
                schema: config.schema.clone(),
                mode: ExportMode::DataOnly,
                data_format: DataFormat::Json,
                output_mode: OutputMode::Single,
                tables: vec![TableExportInput {
                    table_name: table.clone(),
                    schema: config.schema.clone(),
                    columns: vec!["id".into()],
                    ddl: None,
                }],
            };
            let operation = Arc::new(Operation {
                state: state.clone(),
                request,
                observed: observed.clone(),
                staging,
                cancel_after_batch,
            });
            let artifacts = Arc::new(
                FileArtifactRepository::open(temp.path().join(if cancel_after_batch {
                    "cancel-artifacts"
                } else {
                    "success-artifacts"
                }))
                .unwrap(),
            );
            let job = FileArtifactJob::new(
                "tableExport",
                ctx.clone(),
                artifacts.clone(),
                operation.clone(),
            );
            let stage = StageSpec {
                stage_id: StageId::new("produceArtifact"),
                kind: "tableExport".into(),
                depends_on: vec![],
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                job.run_stage(&stage, &datazen_runtime::job::CancelToken::new()),
            )
            .await
            .expect("bounded export completion")
            .unwrap();
            if cancel_after_batch {
                assert_eq!(result.terminal, StageTerminal::Cancelled);
                assert!(result.artifact_ids.is_empty());
                assert!(job.output().is_none());
                assert!(!observed.done.load(Ordering::SeqCst));
                assert!(observed.rows.load(Ordering::SeqCst) < 1025);
            } else {
                assert_eq!(result.terminal, StageTerminal::Succeeded);
                let (id, rows) = job.output().expect("published export");
                assert_eq!(rows, 1025);
                let metadata = artifacts.describe(&ctx, id.clone()).await.unwrap();
                assert_eq!(metadata.lifecycle, ArtifactLifecycle::Complete);
                let mut bytes = vec![];
                for index in 0..metadata.published_chunk_count.get() {
                    bytes.extend(
                        artifacts
                            .read_chunk(&ctx, id.clone(), Counter::new(index))
                            .await
                            .unwrap()
                            .bytes,
                    );
                }
                let rows: Vec<serde_json::Value> =
                    serde_json::from_slice(&bytes).expect("complete JSON file");
                let mut ids: Vec<_> = rows.iter().map(|row| row["id"].as_i64().unwrap()).collect();
                ids.sort_unstable();
                assert_eq!(ids, (0..1025).collect::<Vec<_>>());
                assert!(observed.done.load(Ordering::SeqCst));
                assert!(observed.batches.lock().unwrap().len() > 1);
            }
            // Export-owned sessions have closed; the caller's original session survives.
            assert_eq!(runtime.registered_session_ids().len(), 1);
            drop(job);
            drop(operation);
            assert!(!staging_path.exists(), "staging directory must be removed");
            eprintln!(
                "LIVE_EXPORT driver={driver_id} cancelled={cancel_after_batch} stage=verified"
            );
        }
    })
    .catch_unwind()
    .await;
    let cleanup = driver
        .execute(&observer, &format!("DROP TABLE IF EXISTS {table}"))
        .await;
    let closed = runtime
        .close_session(
            &ctx,
            CloseSessionRequest::new(handle, CloseMode::RollbackAndClose),
        )
        .await;
    let disconnected = driver.disconnect(observer).await;
    assert!(
        cleanup.is_ok() && disconnected.is_ok(),
        "fixture cleanup failed"
    );
    assert!(
        matches!(closed, Ok(ref receipt) if receipt.resource_release == ResourceRelease::Confirmed)
    );
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
#[tokio::test]
#[ignore = "requires strict PostgreSQL/MySQL fixtures"]
async fn postgres_export_streams_past_row_cap_and_cancels_without_publication() {
    journey("TEST_PG_", "postgresql").await;
}
#[tokio::test]
#[ignore = "requires strict PostgreSQL/MySQL fixtures"]
async fn mysql_export_streams_past_row_cap_and_cancels_without_publication() {
    journey("TEST_MYSQL_", "mysql").await;
}
