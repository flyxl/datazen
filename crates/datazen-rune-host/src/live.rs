//! The live adapter: the shared UI's seam over real use cases (N3).
//!
//! Real IO never runs on the UI frame loop (plan invariant 3): every job
//! goes to one worker thread that owns a tokio runtime, and the answers
//! come back through [`DesktopAdapter::poll_jobs`]. N3 wires opening a
//! connection — resolve the driver through `datazen`'s registry (never a
//! second registration), connect, read the catalog through the driver's
//! catalog contract — and the connection list still answers through the
//! file store's use case, whose futures are always ready.

use datazen::connection::application::ConnectionService;
use datazen::connection::domain::{ConnectionRepository, ConnectionScope};
use datazen::dashboard::application::create::create_widget_from_sql;
use datazen::dashboard::domain::repository::DashboardRepositoryError;
use datazen::dashboard::domain::repository as dashboard_rules;
use datazen::execution::application::orchestration::RuntimeConnectionUseCases;
use datazen::execution::application::ConnectionUseCases as _;
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::execution::target::ExecutionTarget;
use datazen::schema::CachedColumns;
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::{ConnectionId, Counter};
use datazen::table_browsing::api::{
    read_table_page, TargetBinding, TargetCommandRunner, TableReadRequest,
};
use datazen::table_editing::change_set::{
    CommitPendingChangesRequest, PendingRowChange, RowChangeTableContext,
};
use datazen::table_editing::ports::ArtifactChunkReader;
use datazen::table_editing::{commit_changes, preview_changes, ConnectionFacts, TableColumnSource};
use datazen::workflow::application::registry::WorkflowRegistry;
use datazen::workflow::domain::model::{WorkflowDefinition, WorkflowStep, WorkflowVisibility};
use datazen::dashboard::domain::types::ViewMode;
use datazen_driver_api::filters::{FilterCondition, FilterOperator, SortCondition};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, Value as Cell};
use datazen_ui_api::{
    AdapterDelivery, AdapterJob, Catalog, ColumnFilter, ColumnSort, ConnectionProfile,
    DashboardWidget, DatabaseNode, DesktopAdapter, ProgressSnapshot, SchemaNode, Snippet,
    TablePage, ValueSearchHit, Workflow, WorkflowStepResult, DESKTOP_NOT_IMPLEMENTED,
};
use datazen_ui_api::{FavoriteSpec, InboundRef};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

/// The host's registry, in its static form: every factory the host links
/// is admitted at construction — the registry's own constructor, not a
/// second registration — so a type is either here or a hard error, and no
/// test-binary link can drop an inventory registration the lazy form
/// would scan for. N3 links sqlite; other drivers join this list on the
/// same one-line seam.
pub(crate) fn host_registry() -> DriverRegistry {
    DriverRegistry::with_factories([datazen_driver_sqlite::factory()])
        .expect("the sqlite factory admits at construction")
}

/// A job on the worker's channel, with the generation the UI expects back.
enum HostJob {
    OpenConnection { generation: u32, id: String },
    ValueSearch { generation: u32, value: String, mode: String },
    ReadTable {
        generation: u32,
        table: String,
        page: u32,
        page_size: u32,
        sort: Option<ColumnSort>,
        filter: Option<ColumnFilter>,
    },
    CommitCells {
        generation: u32,
        batch: datazen_ui_api::CellEditBatch,
    },
    RunQuery {
        generation: u32,
        sql: String,
    },
}

/// A finished job on the answer channel.
enum HostAnswer {
    Connection { generation: u32, id: String, catalog: Result<Catalog, String> },
    ValueSearched { generation: u32, hits: Result<Vec<ValueSearchHit>, String> },
    TablePage { generation: u32, page: Result<TablePage, String> },
    CommitCount { generation: u32, count: Result<u32, String> },
    QueryPage { generation: u32, page: Result<TablePage, String> },
}

/// The open live session, shared between the adapter and the worker. N4's
/// reads and N6's commits run over the same session.
#[derive(Default)]
struct LiveSession {
    driver: Option<Arc<dyn DatabaseDriver>>,
    handle: Option<ConnectionHandle>,
    database: Option<String>,
    /// The connection id the session was opened for (N4's read context).
    connection: Option<String>,
    /// The connection's read-only flag, as opened (N6's commit facts).
    read_only: bool,
}

impl LiveSession {
    fn is_open(&self) -> bool {
        self.driver.is_some() && self.handle.is_some()
    }
}

pub struct LiveAdapter {
    /// Jobs to the worker.
    jobs: Mutex<std::sync::mpsc::Sender<HostJob>>,
    /// Answers from the worker, drained on the poll path.
    answers: Mutex<std::sync::mpsc::Receiver<HostAnswer>>,
    /// The connection use case over the host's file store. `list` touches
    /// only the loaded store (its futures are always ready); the plaintext
    /// `get` the worker calls runs on the worker's runtime.
    service: ConnectionService,
    /// The shared live session.
    session: Arc<Mutex<LiveSession>>,
    /// The host's execution runtime (N5): the use cases the commit, query
    /// and sync paths run through. Exposed for the job flows that follow.
    runtime: Arc<RuntimeConnectionUseCases>,
    /// The settings use case over the settings file (N8), with the document
    /// cached in memory — reads are memory-speed, writes go through.
    settings: datazen::settings::application::SettingsService,
    settings_document: Arc<Mutex<serde_json::Value>>,
    /// The query history use case over the history files (N9): every executed
    /// statement lands in it, success or failure.
    history: datazen::history::HistoryService,
    /// The live session's table schemas, keyed by table (N10): the structure
    /// reads answer from here, warmed on first ask. A local sqlite schema
    /// read is the same IO class as the settings file write the adapter
    /// already performs synchronously; a remote driver will need the job
    /// channel — honestly noted for the day one is wired.
    schemas: Arc<Mutex<std::collections::HashMap<String, datazen_driver_api::TableSchema>>>,
    /// A dedicated runtime for the adapter's synchronous driver calls (N10):
    /// sqlx requires a Tokio context, and these reads (a local sqlite schema
    /// read, a DDL render) are the bounded local-IO class the adapter already
    /// performs synchronously. The worker's runtime stays the one real IO
    /// runs on; this one only serves the cache misses.
    sync_runtime: tokio::runtime::Runtime,
    /// The static driver registry, for the sync session's driver resolution.
    registry: Arc<DriverRegistry>,
    /// The export/backup jobs (N11), behind the UI's job vocabulary.
    host_jobs: crate::jobs::HostJobs,
    /// The data directory exports and backups land under (N11).
    data_dir: PathBuf,
    /// The sync reads' own connection: the live session's pool is single and
    /// belongs to the worker's runtime, so the sync runtime opens its own —
    /// created lazily per connection id and reused while the id holds.
    sync_handle: Mutex<Option<SyncSession>>,
    /// The workflow registry use case over the workflow file (N13).
    workflows: Arc<WorkflowRegistry>,
    /// The snippets' file store (T3): reads answer from the loaded file,
    /// writes persist atomically.
    snippets: Arc<crate::snippets::FileSnippets>,
    /// The favorites' file store (T6): the favorites root under the data
    /// directory, the desktop's `.sql` file format.
    favorites: Arc<crate::favorites::FileFavorites>,
    /// The dashboard rows' file store (N13); the widget use case runs over
    /// it and the registry.
    dashboard_store: Arc<crate::dashboards::FileDashboardStore>,
    /// The profiles handle the runtime was assembled with (N13): the
    /// workflow and widget query paths carry the same config revision the
    /// runtime's sessions were opened with.
    profiles: Arc<crate::backend::HostProfiles>,
}

/// The sync reads' own session over the live connection.
struct SyncSession {
    connection_id: String,
    driver: Arc<dyn DatabaseDriver>,
    handle: ConnectionHandle,
    database: String,
}

impl LiveAdapter {
    /// The host's execution runtime (N5): open sessions, execute, read
    /// results back through the use cases over the three host parts.
    pub fn runtime(&self) -> &Arc<RuntimeConnectionUseCases> {
        &self.runtime
    }

    /// The live host's adapter: connections from the file store, sessions
    /// through the drivers the host's registry assembles — the sqlite
    /// factory, linked in by this crate, admitted at construction.
    pub fn new(repository: Arc<dyn ConnectionRepository>) -> Self {
        let data_dir = crate::default_connections_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| Path::new(".").to_path_buf());
        Self::with_data_dir(repository, &data_dir)
    }

    /// The same adapter with an explicit data directory: the runtime's
    /// result artifacts land under `data_dir/result-artifacts`.
    pub fn with_data_dir(
        repository: Arc<dyn ConnectionRepository>,
        data_dir: &Path,
    ) -> Self {
        let (job_tx, job_rx) = std::sync::mpsc::channel::<HostJob>();
        let (answer_tx, answer_rx) = std::sync::mpsc::channel::<HostAnswer>();
        let registry = Arc::new(host_registry());
        // N5: the execution runtime over the three host parts, sharing the
        // one repository with the adapter's connection use case.
        let (runtime, _backend, profiles) = crate::backend::assemble_runtime(
            Arc::clone(&registry),
            Arc::clone(&repository),
            data_dir,
        );
        let adapter_registry = Arc::clone(&registry);
        let jobs = crate::jobs::HostJobs::new();
        let adapter_data_dir = data_dir.to_path_buf();
        // The use case is stateless over the repository: the adapter and the
        // worker each hold their own service over the one store.
        let service = ConnectionService::new(repository.clone());
        // N8: the settings use case over the settings file, with the
        // document cached for memory-speed reads.
        let settings_repository =
            Arc::new(crate::settings::FileSettingsRepository::open(&data_dir.join("settings.json")));
        let settings = datazen::settings::application::SettingsService::new(settings_repository);
        let settings_document = Arc::new(Mutex::new(
            futures_executor::block_on(settings.read_app_settings())
                .unwrap_or_else(|_| serde_json::json!({})),
        ));
        // N9: the history use case over the two history files.
        let history = datazen::history::HistoryService::new(
            Arc::new(
                crate::history::FileQueryHistory::open(&data_dir.join("query-history.json"))
                    .expect("the query history store opens"),
            ),
            Arc::new(
                crate::history::FileMigrationRuns::open(&data_dir.join("migration-runs.json"))
                    .expect("the migration run store opens"),
            ),
        );
        let worker_history = history.clone();
        // N13: the workflow registry over the workflow file, and the
        // dashboard rows' file store — no builtin workflows ship with the
        // native host; an empty registry is the honest start.
        let workflow_store = Arc::new(
            crate::dashboards::FileWorkflowStore::open(&data_dir.join("workflows.json"))
                .expect("the workflow store opens"),
        );
        let workflows = Arc::new(WorkflowRegistry::new(workflow_store, Vec::new()));
        let dashboard_store = Arc::new(
            crate::dashboards::FileDashboardStore::open(&data_dir.join("dashboards.json"))
                .expect("the dashboard store opens"),
        );
        // T3: the snippets' file store.
        let snippets = Arc::new(
            crate::snippets::FileSnippets::open(&data_dir.join("snippets.json"))
                .expect("the snippets store opens"),
        );
        // T6: the favorites' file store — a favorites root directory of
        // self-describing .sql files.
        let favorites = Arc::new(
            crate::favorites::FileFavorites::open(&data_dir.join("favorites"))
                .expect("the favorites store opens"),
        );
        let sync_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the adapter's sync runtime starts");
        let schemas = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let worker_schemas = Arc::clone(&schemas);
        let worker_service = ConnectionService::new(repository);
        let session = Arc::new(Mutex::new(LiveSession::default()));
        let worker_session = Arc::clone(&session);
        let worker_runtime = Arc::clone(&runtime);
        let worker_profiles = Arc::clone(&profiles);
        let worker_registry = Arc::clone(&registry);
        thread::Builder::new()
            .name("datazen-live-jobs".into())
            .spawn(move || {
                // One runtime for the worker's whole life; every job's async
                // use case runs inside it. Nothing here touches the UI
                // thread.
                let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        // Without a runtime no job can run; say so per job
                        // instead of dying silently.
                        while let Ok(job) = job_rx.recv() {
                            let answer = match job {
                                HostJob::OpenConnection { generation, id } => HostAnswer::Connection {
                                    generation,
                                    id,
                                    catalog: Err(format!("the job runtime did not start: {error}")),
                                },
                                HostJob::ReadTable { generation, .. } => HostAnswer::TablePage {
                                    generation,
                                    page: Err(format!("the job runtime did not start: {error}")),
                                },
                                HostJob::CommitCells { generation, .. } => HostAnswer::CommitCount {
                                    generation,
                                    count: Err(format!("the job runtime did not start: {error}")),
                                },
                                HostJob::RunQuery { generation, .. } => HostAnswer::QueryPage {
                                    generation,
                                    page: Err(format!("the job runtime did not start: {error}")),
                                },
                                HostJob::ValueSearch { generation, .. } => {
                                    HostAnswer::ValueSearched {
                                        generation,
                                        hits: Err(format!(
                                            "the job runtime did not start: {error}"
                                        )),
                                    }
                                }
                            };
                            if answer_tx.send(answer).is_err() {
                                break;
                            }
                        }
                        return;
                    }
                };
                while let Ok(job) = job_rx.recv() {
                    let answer = match job {
                        HostJob::OpenConnection { generation, id } => {
                            let id_for_answer = id.clone();
                            let catalog = runtime.block_on(open_connection(
                                &worker_service,
                                &registry,
                                &worker_session,
                                &worker_schemas,
                                &id,
                            ));
                            HostAnswer::Connection { generation, id: id_for_answer, catalog }
                        }
                        HostJob::ValueSearch { generation, value, mode } => {
                            let hits = runtime.block_on(value_search_scan(
                                &worker_session,
                                &value,
                                &mode,
                            ));
                            HostAnswer::ValueSearched { generation, hits }
                        }
                        HostJob::ReadTable { generation, table, page, page_size, sort, filter } => {
                            let page_result = runtime.block_on(read_table(
                                &worker_session,
                                &worker_schemas,
                                &table,
                                page,
                                page_size,
                                sort,
                                filter,
                            ));
                            HostAnswer::TablePage { generation, page: page_result }
                        }
                        HostJob::CommitCells { generation, batch } => {
                            let count = runtime.block_on(commit_edits(
                                &worker_runtime,
                                &worker_registry,
                                &worker_profiles,
                                &worker_session,
                                batch,
                            ));
                            HostAnswer::CommitCount { generation, count }
                        }
                        HostJob::RunQuery { generation, sql } => {
                            let started = std::time::Instant::now();
                            let page = runtime.block_on(run_query(
                                &worker_runtime,
                                &worker_profiles,
                                &worker_session,
                                &sql,
                            ));
                            // N9: the executed statement lands in the history
                            // — success and failure alike, with its own
                            // timing. A failed history write never costs the
                            // query its answer (the use case logs it).
                            let (connection, database) = {
                                let session =
                                    worker_session.lock().expect("the live session lock");
                                (
                                    session.connection.clone().unwrap_or_default(),
                                    session.database.clone().unwrap_or_default(),
                                )
                            };
                            runtime.block_on(worker_history.add_query_history(
                                datazen::history::QueryHistoryEntry {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    connection_id: connection,
                                    database,
                                    schema: None,
                                    sql: sql.clone(),
                                    executed_at: chrono::Utc::now(),
                                    execution_time_ms: started
                                        .elapsed()
                                        .as_millis()
                                        .min(u64::MAX as u128) as u64,
                                    rows_affected: None,
                                    success: page.is_ok(),
                                    error_message: page.as_ref().err().map(|error| error.clone()),
                                },
                            ));
                            HostAnswer::QueryPage { generation, page }
                        }
                    };
                    if answer_tx.send(answer).is_err() {
                        break;
                    }
                }
            })
            .expect("the live job worker starts");
        Self {
            jobs: Mutex::new(job_tx),
            answers: Mutex::new(answer_rx),
            service,
            session,
            runtime,
            settings,
            settings_document,
            history,
            schemas,
            sync_runtime,
            registry: adapter_registry,
            sync_handle: Mutex::new(None),
            host_jobs: jobs,
            data_dir: adapter_data_dir,
            workflows,
            snippets,
            favorites,
            dashboard_store,
            profiles,
        }
    }

    /// The live session, for the hosts that follow (N4 reads, N5 commits).
    pub(crate) fn session(&self) -> &Arc<Mutex<LiveSession>> {
        &self.session
    }

    /// Copy one table's rows from one connection to another (N12): the
    /// source read goes through the read use case, the target writes go
    /// through the shared insert builder and the execution use case — no
    /// host-side SQL of its own. The target table must already exist (the
    /// same contract the desktop's sync asks the user to confirm).
    pub async fn sync_table(
        &self,
        runtime: &Arc<RuntimeConnectionUseCases>,
        source_connection: &str,
        table: &str,
        target_connection: &str,
    ) -> Result<u64, String> {
        let source_config = self.plain_config(source_connection)?;
        let target_config = self.plain_config(target_connection)?;
        let source_driver = self
            .registry
            .get(&source_config.database_type)
            .await
            .ok_or_else(|| format!("no driver for '{}'", source_config.database_type))?;
        let target_driver = self
            .registry
            .get(&target_config.database_type)
            .await
            .ok_or_else(|| format!("no driver for '{}'", target_config.database_type))?;

        // The source read: one session, the read use case over it.
        let source_session = Arc::new(Mutex::new(LiveSession {
            driver: Some(Arc::clone(&source_driver)),
            handle: Some(
                source_driver
                    .connect(&source_config)
                    .await
                    .map_err(|error| error.to_string())?,
            ),
            database: source_config.database.clone(),
            connection: Some(source_connection.to_owned()),
            read_only: source_config.read_only,
        }));
        let runner = SessionRunner { session: Arc::clone(&source_session) };
        let context = host_request_context();
        let binding = TargetBinding {
            target: ExecutionTarget::new(
                ConnectionId::new(source_connection.to_owned()),
                datazen::connection::domain::NamespaceTarget::new(
                    source_config.database.clone(),
                    None,
                    None,
                    Vec::new(),
                ),
            ),
            revision: Counter::new(1),
            manual_handle: None,
        };
        let mut landed = 0u64;
        let mut page = 0u32;
        loop {
            let request = TableReadRequest {
                manual_handle: None,
                context: RowChangeTableContext {
                    connection_id: source_connection.to_owned(),
                    db_session_id: "datazen-native".into(),
                    driver_type: source_driver.driver_type(),
                    database: source_config.database.clone(),
                    schema: None,
                    table: table.to_owned(),
                },
                page,
                page_size: 500,
                filters: None,
                sorts: None,
                skip_count: None,
                filter_logic: None,
                raw_where: None,
                count_strategy: None,
                default_order_columns: None,
                after_key: None,
                require_keyset: None,
                cursor_depth: None,
            };
            let read =
                read_table_page(source_driver.as_ref(), request, Counter::new(1), context.clone(), &runner)
                    .await
                    .map_err(|error| error.to_string())?;
            if read.rows.is_empty() {
                break;
            }
            let column_names: Vec<String> =
                read.columns.iter().map(|column| column.name.clone()).collect();
            let target_binding = TargetBinding {
                target: ExecutionTarget::new(
                    ConnectionId::new(target_connection.to_owned()),
                    datazen::connection::domain::NamespaceTarget::new(
                        target_config.database.clone(),
                        None,
                        None,
                        Vec::new(),
                    ),
                ),
                revision: Counter::new(1),
                manual_handle: None,
            };
            let dialect = datazen_driver_api::LegacySqlDialect::new(target_driver.as_ref());
            for row in &read.rows {
                let cells: Vec<(&str, Option<Cell>)> = column_names
                    .iter()
                    .zip(row.iter())
                    .map(|(name, cell)| (name.as_str(), cell.clone()))
                    .collect();
                let statement =
                    datazen_driver_api::sql_builder::build_insert_statement(
                        &dialect,
                        target_driver.max_bound_parameters(),
                        table,
                        &cells,
                    )
                    .map_err(|error| error.to_string())?;
                let token = runtime
                    .issue_submission_token(
                        &context,
                        datazen::execution::IdempotentOperation::ExecuteAtTarget,
                        None,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                let receipt = runtime
                    .execute_at_target(
                        &context,
                        datazen::execution::application::requests::ExecuteAtTargetRequest {
                            target: target_binding.target.clone(),
                            expected_config_revision: Counter::new(1),
                            call: datazen::execution::application::requests::CommandCall {
                                command: "execute".into(),
                                input: serde_json::json!({
                                    "sql": statement.sql,
                                    "boundParameters": statement.parameters,
                                }),
                            },
                            idempotency_key: token.idempotency_key,
                        },
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                let view = loop {
                    let view = runtime
                        .get_execution(&context, receipt.execution_id.clone())
                        .await
                        .map_err(|error| error.to_string())?;
                    if view.state.is_terminal() {
                        break view;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                };
                if view.state != datazen::execution::ExecutionState::Succeeded {
                    return Err(match view.error_code {
                        Some(code) => format!("the insert failed: {code:?}"),
                        None => "the insert failed".to_string(),
                    });
                }
                landed += 1;
            }
            if read.rows.len() < 500 {
                break;
            }
            page += 1;
        }
        Ok(landed)
    }

    /// Compare one table's schema between two connections (N12): the schema
    /// diff use case over both endpoints, in its raw-comparison form (the
    /// cross-dialect adapters are not assembled in this host; sqlite-to-sqlite
    /// needs none). The missing-on-target columns are the sync's ADD list.
    pub async fn compare_table(
        &self,
        source_connection: &str,
        source_table: &str,
        target_connection: &str,
        target_table: &str,
    ) -> Result<datazen::migration::schema_diff::api::TableSchemaComparison, String> {
        let source_config = self.plain_config(source_connection)?;
        let target_config = self.plain_config(target_connection)?;
        let source_driver = self
            .registry
            .get(&source_config.database_type)
            .await
            .ok_or_else(|| format!("no driver for '{}'", source_config.database_type))?;
        let target_driver = self
            .registry
            .get(&target_config.database_type)
            .await
            .ok_or_else(|| format!("no driver for '{}'", target_config.database_type))?;
        let source = datazen::migration::schema_diff::api::SchemaDiffEndpoint {
            driver: source_driver.clone(),
            handle: source_driver
                .connect(&source_config)
                .await
                .map_err(|error| error.to_string())?,
            config: source_config,
        };
        let target = datazen::migration::schema_diff::api::SchemaDiffEndpoint {
            driver: target_driver.clone(),
            handle: target_driver
                .connect(&target_config)
                .await
                .map_err(|error| error.to_string())?,
            config: target_config,
        };
        let comparison = datazen::migration::schema_diff::api::compare_table_schemas(
            &source,
            &target,
            source_table,
            target_table,
            None,
            None,
            &NoAdapters,
            &NoPreview,
        )
        .await
        .map_err(|error| error.to_string())?;
        Ok(comparison)
    }

    /// The connection's plaintext config from the file store.
    fn plain_config(&self, connection: &str) -> Result<datazen_driver_api::ConnectionConfig, String> {
        let scope = ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION);
        futures_executor::block_on(self.service.get(&scope, connection))
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("no connection '{connection}' in the store"))
    }

    /// The sync reads' session over the live connection: the live session's
    /// single-connection pool belongs to the worker's runtime, so the sync
    /// runtime opens its own connection, lazily, reused while the connection
    /// id holds.
    fn sync_session(&self) -> Result<SyncSession, String> {
        let (connection, database) = {
            let session = self.session.lock().expect("the live session lock");
            if !session.is_open() {
                return Err("the live session is not open".into());
            }
            (
                session.connection.clone().unwrap_or_default(),
                session.database.clone().unwrap_or_default(),
            )
        };
        if let Some(existing) = self.sync_handle.lock().expect("the sync lock").as_ref() {
            if existing.connection_id == connection {
                return Ok(SyncSession {
                    connection_id: existing.connection_id.clone(),
                    driver: Arc::clone(&existing.driver),
                    handle: existing.handle.clone(),
                    database: existing.database.clone(),
                });
            }
        }
        let config = self.plain_config(&connection)?;
        let driver = self
            .sync_runtime
            .block_on(self.registry.get(&config.database_type))
            .ok_or_else(|| format!("no driver for '{}'", config.database_type))?;
        let handle = self
            .sync_runtime
            .block_on(driver.connect(&config))
            .map_err(|error| error.to_string())?;
        let session = SyncSession {
            connection_id: connection,
            driver,
            handle,
            database,
        };
        *self.sync_handle.lock().expect("the sync lock") = Some(SyncSession {
            connection_id: session.connection_id.clone(),
            driver: Arc::clone(&session.driver),
            handle: session.handle.clone(),
            database: session.database.clone(),
        });
        Ok(session)
    }

    /// The table's schema, from the cache, fetched on a miss (N10). A local
    /// sqlite schema read is the same IO class as the settings file write;
    /// the cache keeps repeats free.
    fn cached_schema(
        &self,
        table: &str,
    ) -> Result<datazen_driver_api::TableSchema, String> {
        if let Some(schema) = self
            .schemas
            .lock()
            .expect("the schema lock")
            .get(table)
            .cloned()
        {
            return Ok(schema);
        }
        let session = self.sync_session()?;
        let schema = self
            .sync_runtime
            .block_on(session.driver.get_table_schema(&session.handle, table, &session.database, None))
            .map_err(|error| error.to_string())?;
        if schema.columns.is_empty() {
            // A schema with no columns is a table the catalog does not have —
            // the driver's honest answer for an unknown name.
            return Err(format!("the catalog has no table '{table}'"));
        }
        self.schemas
            .lock()
            .expect("the schema lock")
            .insert(table.to_owned(), schema.clone());
        Ok(schema)
    }
}

/// Open a connection and read its catalog: resolve the driver through the
/// registry, connect with the store's plaintext config, read databases and
/// tables through the driver's catalog contract, and keep the session for
/// the reads and commits that follow. The schema layer follows the
/// registry's own rule (metadata's derivation): the driver's schema names
/// when it has them, else one node named for the database — sqlite has no
/// schema level, so its tables sit under one.
async fn open_connection(
    service: &ConnectionService,
    registry: &DriverRegistry,
    session: &Arc<Mutex<LiveSession>>,
    schemas: &Arc<Mutex<std::collections::HashMap<String, datazen_driver_api::TableSchema>>>,
    id: &str,
) -> Result<Catalog, String> {
    let scope = ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION);
    let config = service
        .get(&scope, id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("no connection '{id}' in the store"))?;
    let driver = registry
        .get(&config.database_type)
        .await
        .ok_or_else(|| format!("no driver for '{}'", config.database_type))?;
    let handle = driver.connect(&config).await.map_err(|error| error.to_string())?;
    let databases = driver.get_databases(&handle).await.map_err(|error| error.to_string())?;

    let mut nodes = Vec::new();
    for database in &databases {
        let tables = driver
            .get_tables(&handle, database, None)
            .await
            .map_err(|error| error.to_string())?;
        let mut schemas: Vec<String> = driver
            .get_schemas(&handle, database)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                let mut names: Vec<String> =
                    tables.iter().filter_map(|table| table.schema.clone()).collect();
                names.sort();
                names.dedup();
                names
            });
        if schemas.is_empty() {
            schemas.push(database.clone());
        }
        let single = schemas.len() == 1;
        let schema_nodes = schemas
            .into_iter()
            .map(|schema| {
                let tables_of: Vec<String> = tables
                    .iter()
                    .filter(|table| match &table.schema {
                        Some(name) => *name == schema,
                        // A schema-less driver: every table sits under the
                        // one node.
                        None => single,
                    })
                    .map(|table| table.name.clone())
                    .collect();
                SchemaNode { name: schema, tables: tables_of }
            })
            .collect();
        nodes.push(DatabaseNode { name: database.clone(), schemas: schema_nodes });
    }

    // A new session invalidates the old one's cached schemas.
    schemas.lock().expect("the schema lock").clear();
    // The session is the adapter's for the reads and commits that follow:
    // swap it in whole, replacing any earlier one.
    *session.lock().expect("the live session lock") = LiveSession {
        driver: Some(driver),
        handle: Some(handle),
        database: databases.first().cloned(),
        connection: Some(id.to_owned()),
        read_only: config.read_only,
    };
    Ok(Catalog { databases: nodes })
}

/// The schema-diff adapter host in its raw form (N12): no cross-dialect
/// adapters are assembled, so the comparison falls back to the raw snapshot
/// diff — exactly the sqlite-to-sqlite case, which needs none.
struct NoAdapters;

impl datazen::migration::schema_diff::api::SchemaDiffAdapterHost for NoAdapters {
    fn resolve_plan_pair(
        &self,
        source_type: &str,
        target_type: &str,
    ) -> Result<datazen::migration::schema_diff::api::PlanSyncAdapters, String> {
        Err(format!("no cross-dialect plan adapters in the native host: {source_type} -> {target_type}"))
    }

    fn comparison_adapters(
        &self,
        _source_type: &str,
        _target_type: &str,
    ) -> Option<datazen::migration::schema_diff::api::ComparisonSyncAdapters> {
        None
    }
}

/// The preview host's raw form: no full-type reads, no DDL previews — the
/// raw comparison path never asks.
struct NoPreview;

#[async_trait::async_trait]
impl datazen::migration::schema_diff::api::ComparisonPreviewHost for NoPreview {
    async fn full_column_types(
        &self,
        _adapter: &dyn datazen_driver_api::sync::SyncSourceAdapter,
        _driver: &dyn DatabaseDriver,
        _handle: &ConnectionHandle,
        _table: &str,
    ) -> Option<std::collections::HashMap<String, String>> {
        None
    }

    fn create_table_ddl(
        &self,
        _ir_table: &datazen_driver_api::sync::IRTable,
        _target: &dyn datazen_driver_api::sync::SyncTargetAdapter,
    ) -> String {
        String::new()
    }
}

/// The single-session host's form of the read port's runner: the command
/// runs inline on the session the host already holds — no submission tokens,
/// no polling, no artifact chunks, because there is no runtime facade in
/// front of the driver to go through. The output wraps the driver's command
/// result in the one `commandResult` chunk the read algorithm reads
/// (`data.results` for queries, `data.schema` for schema fetches), the same
/// shape the desktop and server runners publish from their runtimes.
struct SessionRunner {
    session: Arc<Mutex<LiveSession>>,
}

#[async_trait::async_trait]
impl TargetCommandRunner for SessionRunner {
    async fn run(
        &self,
        _context: &RequestContext,
        _binding: &TargetBinding,
        command: &str,
        input: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, ApiError> {
        let (driver, handle) = {
            let session = self.session.lock().expect("the live session lock");
            (
                session.driver.clone().ok_or_else(session_closed)?,
                session.handle.clone().ok_or_else(session_closed)?,
            )
        };
        let result = driver
            .execute_command(&handle, command, input)
            .await
            .map_err(|error| ApiError::new(ApiErrorCode::ServiceUnavailable, error.to_string()))?;
        Ok(vec![serde_json::json!({
            "kind": "commandResult",
            "data": result.data,
        })])
    }
}

fn session_closed() -> ApiError {
    ApiError::new(
        ApiErrorCode::ServiceUnavailable,
        "the live session is not open".to_string(),
    )
}

/// The host's request context: the single-tenant desktop organization and a
/// synthetic principal for the host process — the read's authorization
/// vocabulary, filled the only way a local single-user host can.
fn host_request_context() -> RequestContext {
    RequestContext::new(
        datazen::shared::id::OrganizationId::new(datazen_ui_api::DESKTOP_ORGANIZATION),
        datazen::shared::id::PrincipalId::new("datazen-native"),
        None,
        datazen::shared::id::ClientInstanceId::new("datazen-native"),
        datazen::shared::id::RequestId::new("datazen-native"),
        None,
    )
}

/// One page of a table through the existing read use case, over the live
/// session (N4). The paging, sorting and filtering are served by the SQL the
/// use case assembles — nothing here re-implements a read. The `Contains`
/// filter and the single sort arrive in the UI's vocabulary and map to the
/// use case's own conditions.
#[allow(clippy::too_many_arguments)]
async fn read_table(
    session: &Arc<Mutex<LiveSession>>,
    schemas: &Arc<Mutex<std::collections::HashMap<String, datazen_driver_api::TableSchema>>>,
    table: &str,
    page: u32,
    page_size: u32,
    sort: Option<ColumnSort>,
    filter: Option<ColumnFilter>,
) -> Result<TablePage, String> {
    let (driver, handle, database, connection) = {
        let session = session.lock().expect("the live session lock");
        if !session.is_open() {
            return Err("the live session is not open".into());
        }
        (
            session.driver.clone().expect("checked"),
            session.handle.clone().expect("checked"),
            session.database.clone(),
            session.connection.clone().unwrap_or_default(),
        )
    };
    let driver_type = driver.driver_type();
    let runner = SessionRunner { session: Arc::clone(session) };
    let context = host_request_context();
    let binding = TargetBinding {
        target: ExecutionTarget::new(
            ConnectionId::new(connection),
            datazen::connection::domain::NamespaceTarget::new(
                database.clone(),
                None,
                None,
                Vec::new(),
            ),
        ),
        // The host opened the session itself and holds it; no second writer
        // can race the revision, so one stable value serves every read.
        revision: Counter::new(1),
        manual_handle: None,
    };
    let sorts = sort.clone().map(|sort| {
        vec![SortCondition { column: sort.column, descending: sort.descending }]
    });
    let filters = filter.clone().map(|filter| {
        vec![FilterCondition {
            column: filter.column,
            operator: FilterOperator::Contains,
            value: Cell::String(filter.contains),
            connector: None,
        }]
    });
    let request = TableReadRequest {
        manual_handle: None,
        context: RowChangeTableContext {
            connection_id: binding.target.connection_id.to_string(),
            db_session_id: "datazen-native".into(),
            driver_type,
            database: database.clone(),
            schema: None,
            table: table.to_owned(),
        },
        page,
        page_size,
        filters,
        sorts,
        skip_count: None,
        filter_logic: None,
        raw_where: None,
        count_strategy: None,
        default_order_columns: None,
        after_key: None,
        require_keyset: None,
        cursor_depth: None,
    };
    let read = read_table_page(driver.as_ref(), request, binding.revision, context, &runner)
        .await
        .map_err(|error| error.to_string())?;
    // N10: warm the structure cache — one schema read beside the page read,
    // both local-file speed, so the structure pages answer from memory.
    let schema_database = database.clone().unwrap_or_else(|| "main".into());
    if let Ok(schema) = driver
        .get_table_schema(&handle, table, &schema_database, None)
        .await
    {
        schemas
            .lock()
            .expect("the schema lock")
            .insert(table.to_owned(), schema);
    }
    // The count the driver could not give is the absence of a fact: fall
    // back to the best lower bound (the page's own end), never to zero.
    let known = read.total_rows.unwrap_or(page as i64 * page_size as i64 + read.rows.len() as i64);
    // The driver's primary-key answer (N6): the commit batch addresses rows
    // by these names.
    let primary_key: Vec<String> = read
        .columns
        .iter()
        .filter(|column| column.is_primary_key)
        .map(|column| column.name.clone())
        .collect();
    Ok(TablePage {
        table: table.to_owned(),
        columns: read.columns.into_iter().map(|column| column.name).collect(),
        rows: read
            .rows
            .into_iter()
            .map(|row| row.into_iter().map(cell_to_text).collect())
            .collect(),
        page: read.page,
        page_size: read.page_size,
        total_rows: known.max(0) as u32,
        sort,
        filter,
        primary_key,
        read_only: None,
    })
}

/// One driver cell as the grid's text: null is the empty string, bytes land
/// as their lossy text, everything else its own display form.
fn cell_to_text(cell: Option<Cell>) -> String {
    match cell {
        None | Some(Cell::Null) => String::new(),
        Some(Cell::Bool(value)) => value.to_string(),
        Some(Cell::Integer(value)) => value.to_string(),
        Some(Cell::Float(value)) => value.to_string(),
        Some(Cell::String(value)) => value,
        Some(Cell::Bytes(bytes)) => String::from_utf8_lossy(&bytes).into_owned(),
        Some(Cell::Timestamp(value)) => value,
        Some(Cell::Json(value)) => value.to_string(),
    }
}

/// The host's column source for the commit use case: the live session's
/// driver answers `get_columns` — the same catalog the read path used, so
/// the plan is built against the schema the page was read through.
struct SessionColumns {
    session: Arc<Mutex<LiveSession>>,
}

#[async_trait::async_trait]
impl TableColumnSource for SessionColumns {
    async fn table_columns(&self, table: &RowChangeTableContext) -> Result<CachedColumns, datazen::table_editing::TableEditingError> {
        let (driver, handle) = {
            let session = self.session.lock().expect("the live session lock");
            if !session.is_open() {
                return Err(datazen::table_editing::TableEditingError::Validation(
                    "the live session is not open".into(),
                ));
            }
            (
                session.driver.clone().expect("checked"),
                session.handle.clone().expect("checked"),
            )
        };
        let database = table
            .database
            .clone()
            .ok_or_else(|| datazen::table_editing::TableEditingError::Validation(
                "table database required".into(),
            ))?;
        let (columns, primary_keys) = driver
            .get_columns(&handle, &table.table, &database, table.schema.as_deref())
            .await
            .map_err(|error| datazen::table_editing::TableEditingError::Validation(error.to_string()))?;
        Ok(CachedColumns {
            table_name: table.table.clone(),
            columns,
            primary_keys,
            cached_at: std::time::Instant::now(),
        })
    }
}

/// The commit path's artifact reader: the runtime's own `read_artifact_chunk`,
/// projected onto the port the commit use case takes.
struct RuntimeChunks {
    runtime: Arc<RuntimeConnectionUseCases>,
}

#[async_trait::async_trait]
impl ArtifactChunkReader for RuntimeChunks {
    async fn read_chunk(
        &self,
        ctx: &RequestContext,
        artifact: &datazen::shared::id::ArtifactId,
        index: Counter,
    ) -> Result<datazen::execution::ResultChunk, ApiError> {
        self.runtime.read_artifact_chunk(ctx, artifact.clone(), index).await
    }
}

/// Apply one commit batch through the existing edit use cases (N6):
/// `preview_changes` builds the plan against the live schema — the identity
/// checks and the fingerprint are the use case's, not a host re-implementation
/// — and `commit_changes` executes it over the runtime in owned-session mode.
/// A failure keeps the grid's dirty marks (the UI's side of the contract); the
/// count that comes back is the rows that landed.
async fn commit_edits(
    runtime: &Arc<RuntimeConnectionUseCases>,
    registry: &Arc<DriverRegistry>,
    profiles: &Arc<crate::backend::HostProfiles>,
    session: &Arc<Mutex<LiveSession>>,
    batch: datazen_ui_api::CellEditBatch,
) -> Result<u32, String> {
    let (driver, connection, database, read_only) = {
        let session = session.lock().expect("the live session lock");
        if !session.is_open() {
            return Err("the live session is not open".into());
        }
        (
            session.driver.clone().expect("checked"),
            session.connection.clone().unwrap_or_default(),
            session.database.clone(),
            session.read_only,
        )
    };
    let _ = registry; // the live session's driver is the one the page was read through
    let driver_type = driver.driver_type();
    let context = RowChangeTableContext {
        connection_id: connection.clone(),
        db_session_id: "datazen-native".into(),
        driver_type,
        database,
        schema: None,
        table: batch.table.clone(),
    };
    let facts = ConnectionFacts {
        database_type: driver.driver_type(),
        read_only,
        database: context.database.clone(),
        schema: None,
        config_revision: profiles.revision_of(&connection),
    };
    let columns = SessionColumns { session: Arc::clone(session) };
    let changes: Vec<PendingRowChange> = batch
        .rows
        .iter()
        .map(|row| PendingRowChange {
            row_identity: row
                .identity
                .iter()
                .map(|(column, value)| (column.clone(), Some(Cell::String(value.clone()))))
                .collect(),
            original_values: row
                .changes
                .iter()
                .map(|(column, original, _)| (column.clone(), Some(Cell::String(original.clone()))))
                .collect(),
            current_values: row
                .changes
                .iter()
                .map(|(column, _, value)| (column.clone(), Some(Cell::String(value.clone()))))
                .collect(),
            changed_columns: row.changes.iter().map(|(column, _, _)| column.clone()).collect(),
            delete_marked: false,
            insert_marked: false,
            expression_values: Default::default(),
        })
        .collect();
    let plan = preview_changes(
        driver.as_ref(),
        context.clone(),
        &changes,
        &columns,
        facts.config_revision,
    )
    .await
    .map_err(|error| error.to_string())?;
    let request = CommitPendingChangesRequest {
        db_session_id: "datazen-native".into(),
        fingerprint: plan.fingerprint.clone(),
        plan,
    };
    let artifacts = Arc::new(RuntimeChunks { runtime: Arc::clone(runtime) });
    let use_cases = Arc::clone(runtime) as Arc<dyn datazen::execution::application::ConnectionUseCases>;
    let ctx = host_request_context();
    let response = commit_changes(
        driver.as_ref(),
        request,
        &facts,
        &columns,
        None,
        use_cases,
        artifacts,
        ctx,
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(response.affected_rows as u32)
}

/// Run one SQL statement through the runtime (N7): `execute_at_target` in
/// owned-session mode — the runtime opens a session at the live connection's
/// target, executes the statement, and the artifacts carry the driver's rows.
/// The first statement's result becomes the grid's page; a failed execution is
/// its error sentence. Cancellation is the UI's generation semantics (Q5/Q6):
/// a stale delivery is dropped on arrival, and the runtime's idle policy reaps
/// the session behind it.
/// The value search's scan (T4): every table of the live session's
/// database, every column of the table, one `search_table_values` per
/// column — the hit list is the columns with matching rows, the same
/// per-column answers the React dialog's scan gives the front end.
async fn value_search_scan(
    session: &Arc<Mutex<LiveSession>>,
    value: &str,
    mode: &str,
) -> Result<Vec<ValueSearchHit>, String> {
    use datazen::services::value_search::{
        search_table_values, SearchMode, SearchTableValuesRequest,
    };
    let (driver, handle, database) = {
        let session = session.lock().expect("the live session lock");
        if !session.is_open() {
            return Err("the live session is not open".into());
        }
        let database = session.database.clone().unwrap_or_else(|| "main".into());
        (
            session.driver.clone().expect("the session driver"),
            session.handle.clone().expect("the session handle"),
            database,
        )
    };
    let mode = match mode {
        "contains" => SearchMode::Contains,
        "notContains" => SearchMode::NotContains,
        "exact" => SearchMode::Exact,
        "prefix" => SearchMode::Prefix,
        "suffix" => SearchMode::Suffix,
        other => return Err(format!("unknown value search mode '{other}'")),
    };
    let tables = driver
        .get_tables(&handle, &database, None)
        .await
        .map_err(|error| error.to_string())?;
    let mut hits = Vec::new();
    for table in &tables {
        let schema = driver
            .get_table_schema(&handle, &table.name, &database, None)
            .await
            .map_err(|error| error.to_string())?;
        for column in &schema.columns {
            let request = SearchTableValuesRequest {
                db_session_id: String::new(),
                table: table.name.clone(),
                columns: vec![column.name.clone()],
                value: value.to_owned(),
                mode: mode.clone(),
                database: Some(database.clone()),
                schema: table.schema.clone(),
            };
            let result = search_table_values(
                driver.as_ref(),
                &handle,
                &database,
                table.schema.as_deref(),
                &request,
            )
            .await
            .map_err(|error| error.to_string())?;
            if result.matches > 0 {
                hits.push(ValueSearchHit {
                    table: table.name.clone(),
                    column: column.name.clone(),
                    matches: result.matches,
                });
            }
        }
    }
    Ok(hits)
}

async fn run_query(
    runtime: &Arc<RuntimeConnectionUseCases>,
    profiles: &Arc<crate::backend::HostProfiles>,
    session: &Arc<Mutex<LiveSession>>,
    sql: &str,
) -> Result<TablePage, String> {
    let (connection, database) = {
        let session = session.lock().expect("the live session lock");
        if !session.is_open() {
            return Err("the live session is not open".into());
        }
        (
            session.connection.clone().unwrap_or_default(),
            session.database.clone(),
        )
    };
    let ctx = host_request_context();
    let token = runtime
        .issue_submission_token(
            &ctx,
            datazen::execution::IdempotentOperation::ExecuteAtTarget,
            None,
        )
        .await
        .map_err(|error| error.to_string())?;
    let receipt = runtime
        .execute_at_target(
            &ctx,
            datazen::execution::application::requests::ExecuteAtTargetRequest {
                target: ExecutionTarget::new(
                    ConnectionId::new(connection.clone()),
                    datazen::connection::domain::NamespaceTarget::new(
                        database.clone(),
                        None,
                        None,
                        Vec::new(),
                    ),
                ),
                expected_config_revision: Counter::new(profiles.revision_of(&connection)),
                call: datazen::execution::application::requests::CommandCall {
                    command: "query".into(),
                    input: serde_json::json!({ "sql": sql }),
                },
                idempotency_key: token.idempotency_key,
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    let view = loop {
        let view = runtime
            .get_execution(&ctx, receipt.execution_id.clone())
            .await
            .map_err(|error| error.to_string())?;
        if view.state.is_terminal() {
            break view;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    };
    if view.state != datazen::execution::ExecutionState::Succeeded {
        return Err(match view.error_code {
            Some(code) => format!("the statement failed: {code:?}"),
            None => "the statement failed".to_string(),
        });
    }
    // Collect the artifacts: the statement metadata names the columns, the
    // row chunks carry the rows.
    let mut columns: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for artifact in &view.artifact_ids {
        let first = runtime
            .read_artifact_chunk(&ctx, artifact.clone(), Counter::ZERO)
            .await
            .map_err(|error| error.to_string())?;
        for index in 0..first.published_chunk_count.get() {
            let chunk = if index == 0 {
                first.clone()
            } else {
                runtime
                    .read_artifact_chunk(&ctx, artifact.clone(), Counter::new(index))
                    .await
                    .map_err(|error| error.to_string())?
            };
            let Some(output) = chunk.output else { continue };
            if output.get("kind").and_then(|kind| kind.as_str()) == Some("statementMetadata") {
                if let Some(names) = output.get("columns").and_then(|cols| cols.as_array()) {
                    if columns.is_empty() {
                        columns = names
                            .iter()
                            .filter_map(|column| column.get("name").and_then(|n| n.as_str()))
                            .map(str::to_owned)
                            .collect();
                    }
                }
                continue;
            }
            // A row chunk: the provider's ResultChunk shape (the same type
            // the sink published, `dr::ResultChunk`).
            let Ok(chunk) = serde_json::from_value::<datazen_driver_api::resource::ResultChunk>(
                output.clone(),
            ) else {
                continue;
            };
            if chunk.statement_index != 0 {
                continue; // the grid shows the first statement's page
            }
            for row in chunk.rows {
                rows.push(row.into_iter().map(cell_to_text).collect());
            }
        }
    }
    let total = rows.len() as u32;
    Ok(TablePage {
        table: "result".into(),
        columns,
        rows,
        page: 0,
        page_size: total.max(1),
        total_rows: total,
        sort: None,
        filter: None,
        primary_key: Vec::new(),
        read_only: Some("query result".into()),
    })
}

impl DesktopAdapter for LiveAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        // The file store's list path touches only the loaded store; its
        // future is always ready, so the blocking poll is memory-speed.
        let scope = ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION);
        let configs =
            futures_executor::block_on(self.service.list_redacted(&scope)).map_err(|e| e.to_string())?;
        Ok(configs
            .into_iter()
            .map(|config| ConnectionProfile {
                id: config.id,
                name: config.name,
                engine: config.database_type,
            })
            .collect())
    }

    fn read_table_page(
        &mut self,
        _: &str,
        _: u32,
        _: u32,
        _: Option<&datazen_ui_api::ColumnSort>,
        _: Option<&datazen_ui_api::ColumnFilter>,
    ) -> Result<TablePage, String> {
        // N4 wires the read through the session; until then the fixed
        // error keeps the port's fallback honest.
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn begin_job(&mut self, job: AdapterJob) -> bool {
        let job = match job {
            AdapterJob::OpenConnection { generation, id } => HostJob::OpenConnection { generation, id },
            AdapterJob::ReadTable { generation, table, page, page_size, sort, filter } => {
                HostJob::ReadTable { generation, table, page, page_size, sort, filter }
            }
            AdapterJob::CommitCells { generation, batch } => {
                HostJob::CommitCells { generation, batch }
            }
            AdapterJob::RunQuery { generation, sql } => HostJob::RunQuery { generation, sql },
            AdapterJob::ValueSearch { generation, value, mode } => {
                HostJob::ValueSearch { generation, value, mode }
            }
        };
        self.jobs.lock().expect("the job channel").send(job).is_ok()
    }

    fn poll_jobs(&mut self) -> Vec<AdapterDelivery> {
        let answers = self.answers.lock().expect("the answer channel");
        let mut deliveries = Vec::new();
        while let Ok(answer) = answers.try_recv() {
            match answer {
                HostAnswer::Connection { generation, id, catalog } => {
                    deliveries.push(AdapterDelivery::ConnectionOpened { generation, id, catalog });
                }
                HostAnswer::ValueSearched { generation, hits } => {
                    deliveries.push(AdapterDelivery::ValueSearched { generation, hits });
                }
                HostAnswer::TablePage { generation, page } => {
                    deliveries.push(AdapterDelivery::TablePage { generation, page });
                }
                HostAnswer::CommitCount { generation, count } => {
                    deliveries.push(AdapterDelivery::CommitCount { generation, count });
                }
                HostAnswer::QueryPage { generation, page } => {
                    deliveries.push(AdapterDelivery::QueryPage { generation, page });
                }
            }
        }
        deliveries
    }

    fn read_ui_settings(&self) -> Result<Vec<datazen_ui_api::Setting>, String> {
        // The cached document answers; the "ui" object holds the three the
        // UI knows. Absent keys are simply absent settings.
        let document = self.settings_document.lock().expect("the settings lock");
        let ui = document
            .get("ui")
            .and_then(|ui| ui.as_object())
            .ok_or_else(|| "no ui settings yet".to_string())?;
        let mut settings = Vec::new();
        if let Some(dark) = ui.get("themeDark").and_then(|value| value.as_bool()) {
            settings.push(datazen_ui_api::Setting::Theme { dark });
        }
        if let Some(rows) = ui.get("tablePageSize").and_then(|value| value.as_u64()) {
            settings.push(datazen_ui_api::Setting::TablePageSize { rows: rows as u32 });
        }
        if let Some(rows) = ui.get("queryMaxRows").and_then(|value| value.as_u64()) {
            settings.push(datazen_ui_api::Setting::QueryMaxRows { rows: rows as u32 });
        }
        if settings.is_empty() {
            // An empty answer would replace the memory port's list with
            // "nothing set"; the honest fresh install reports nothing and
            // lets the UI fall back only on error. An empty ui object is a
            // real answer: nothing configured.
            return Ok(Vec::new());
        }
        Ok(settings)
    }

    fn write_ui_setting(&mut self, setting: datazen_ui_api::Setting) -> Result<(), String> {
        // The same validation the memory port runs: the rules are the
        // product's, not the store's.
        match &setting {
            datazen_ui_api::Setting::Theme { .. } => {}
            datazen_ui_api::Setting::TablePageSize { rows } => {
                if *rows == 0 || *rows > 500 {
                    return Err(format!("table page size must be 1..=500, got {rows}"));
                }
            }
            datazen_ui_api::Setting::QueryMaxRows { rows } => {
                if *rows == 0 || *rows > 10_000 {
                    return Err(format!("query max rows must be 1..=10000, got {rows}"));
                }
            }
        }
        let (key, value) = match &setting {
            datazen_ui_api::Setting::Theme { dark } => ("themeDark", serde_json::json!(dark)),
            datazen_ui_api::Setting::TablePageSize { rows } => {
                ("tablePageSize", serde_json::json!(rows))
            }
            datazen_ui_api::Setting::QueryMaxRows { rows } => {
                ("queryMaxRows", serde_json::json!(rows))
            }
        };
        let mut document = self.settings_document.lock().expect("the settings lock").clone();
        let ui = document
            .as_object_mut()
            .ok_or_else(|| "the settings document is not an object".to_string())?
            .entry("ui")
            .or_insert_with(|| serde_json::json!({}));
        if let Some(object) = ui.as_object_mut() {
            object.insert(key.into(), value);
        }
        // Through the use case: the repository writes atomically, the
        // service round-trips the stored snapshot. The file write is small
        // local IO — the same class as the connection store's reads.
        let stored = futures_executor::block_on(self.settings.save_app_settings(document.clone()))
            .map_err(|error| error.to_string())?;
        *self.settings_document.lock().expect("the settings lock") = stored;
        Ok(())
    }

    fn table_ddl(&self, table: &str) -> Result<String, String> {
        let session = self.sync_session()?;
        // The driver's own DDL command renders the object's definition.
        let result = self
            .sync_runtime
            .block_on(session.driver.execute_command(
                &session.handle,
                "get_object_ddl",
                serde_json::json!({
                    "kind": "table",
                    "name": table,
                    "database": session.database,
                }),
            ))
            .map_err(|error| error.to_string())?;
        result
            .data
            .get("ddl")
            .and_then(|ddl| ddl.as_str())
            .map(str::to_owned)
            .ok_or_else(|| "the driver returned no DDL".to_string())
    }

    fn table_columns(&self, table: &str) -> Result<Vec<datazen_ui_api::ColumnSpec>, String> {
        let schema = self.cached_schema(table)?;
        Ok(schema
            .columns
            .iter()
            .map(|column| datazen_ui_api::ColumnSpec {
                name: column.name.clone(),
                sql_type: column.data_type.clone(),
                nullable: column.nullable,
            })
            .collect())
    }

    fn table_indexes(&self, table: &str) -> Result<Vec<datazen_ui_api::IndexSpec>, String> {
        let schema = self.cached_schema(table)?;
        Ok(schema
            .indexes
            .iter()
            .map(|index| datazen_ui_api::IndexSpec {
                name: index.name.clone(),
                columns: index.columns.clone(),
                unique: index.is_unique,
            })
            .collect())
    }

    fn table_foreign_keys(&self, table: &str) -> Result<Vec<datazen_ui_api::ForeignKeySpec>, String> {
        let schema = self.cached_schema(table)?;
        Ok(schema
            .foreign_keys
            .iter()
            .map(|key| datazen_ui_api::ForeignKeySpec {
                name: key.name.clone(),
                columns: key.columns.clone(),
                target_table: key.referenced_table.clone(),
                target_columns: key.referenced_columns.clone(),
            })
            .collect())
    }

    fn start_export(
        &mut self,
        table: &str,
        format: &str,
        range: &str,
        path: Option<&str>,
    ) -> Result<u64, String> {
        let connection = {
            let session = self.session.lock().expect("the live session lock");
            if !session.is_open() {
                return Err("the live session is not open".into());
            }
            session.connection.clone().unwrap_or_default()
        };
        let config = self.plain_config(&connection)?;
        Ok(self.host_jobs.spawn_export(
            Arc::clone(&self.registry),
            config,
            table.to_owned(),
            format.to_owned(),
            range.to_owned(),
            self.data_dir.join("exports"),
            path.map(std::path::PathBuf::from),
        ))
    }

    fn start_backup(&mut self, path: Option<&str>) -> Result<u64, String> {
        let connection = {
            let session = self.session.lock().expect("the live session lock");
            if !session.is_open() {
                return Err("the live session is not open".into());
            }
            session.connection.clone().unwrap_or_default()
        };
        let config = self.plain_config(&connection)?;
        Ok(self.host_jobs.spawn_backup(
            Arc::clone(&self.registry),
            config,
            self.data_dir.join("backups"),
            path.map(std::path::PathBuf::from),
        ))
    }

    fn host_job_progress(&self, job: u64) -> Option<ProgressSnapshot> {
        self.host_jobs.progress(job)
    }

    fn stop_host_job(&self, job: u64) -> Result<(), String> {
        self.host_jobs.stop(job)
    }

    fn dashboard_widgets(&self) -> Result<Vec<DashboardWidget>, String> {
        // N13: the dashboard rows through the domain rules; a fresh install
        // has no dashboard row yet and answers no widgets.
        let scope = crate::dashboards::host_scope();
        match dashboard_rules::get_dashboard(
            &scope,
            self.dashboard_store.as_ref(),
            crate::dashboards::DEFAULT_DASHBOARD_ID,
        ) {
            Ok(dashboard) => Ok(dashboard
                .widgets
                .iter()
                .map(|widget| DashboardWidget {
                    id: crate::dashboards::widget_ui_id(&widget.id),
                    title: widget.title.clone(),
                    // The UI's kind names the chart the widget renders.
                    kind: match widget.view_mode {
                        ViewMode::Chart => "bar",
                        ViewMode::Table => "table",
                    }
                    .into(),
                })
                .collect()),
            Err(DashboardRepositoryError::NotFound(_)) => Ok(Vec::new()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn save_dashboard_widget(&mut self, title: &str, query: &str) -> Result<u64, String> {
        if title.trim().is_empty() {
            return Err("a widget needs a title".into());
        }
        // The widget's hidden workflow carries its query's connection: the
        // live session names it. Without an open session there is nothing
        // honest to bind.
        let connection = {
            let session = self.session.lock().expect("the live session lock");
            if !session.is_open() {
                return Err(
                    "connect a database first — a widget carries its query's connection".into(),
                );
            }
            session.connection.clone().unwrap_or_default()
        };
        let host = crate::dashboards::HostWidgets::new(
            Arc::clone(&self.workflows),
            Arc::clone(&self.dashboard_store),
        );
        // The widget use case decides everything: the hidden workflow, the
        // layout, the refresh policy; the two files land through it.
        let result = futures_executor::block_on(create_widget_from_sql(
            &crate::dashboards::host_scope(),
            &host,
            crate::dashboards::DEFAULT_DASHBOARD_ID,
            &connection,
            query,
            Some(title.trim().to_owned()),
            crate::dashboards::form_view_mode(),
            None,
        ))
        .map_err(|error| error.to_string())?;
        Ok(crate::dashboards::widget_ui_id(&result.widget.id))
    }

    fn widget_bars(&self, id: u64) -> Result<Vec<(String, u32)>, String> {
        // N13: the widget's query runs live through the execution use case
        // — the same path the editor's statements take — and the first two
        // columns are the chart's label and value. A local sqlite query is
        // the bounded local-IO class the adapter already serves
        // synchronously (N10's precedent); a remote driver will need the
        // job channel.
        let scope = crate::dashboards::host_scope();
        let widget = dashboard_rules::get_dashboard(
            &scope,
            self.dashboard_store.as_ref(),
            crate::dashboards::DEFAULT_DASHBOARD_ID,
        )
        .map_err(|error| error.to_string())?
        .widgets
        .into_iter()
        .find(|widget| crate::dashboards::widget_ui_id(&widget.id) == id)
        .ok_or_else(|| format!("no widget {id}"))?;
        let definition = futures_executor::block_on(self.workflows.get(&scope, &widget.workflow_id))
            .ok_or("the widget's workflow is missing")?;
        let sql = definition
            .steps
            .iter()
            .find_map(|step| match step {
                WorkflowStep::Query { sql, .. } => Some(sql.clone()),
                _ => None,
            })
            .ok_or("the widget's workflow has no query step")?;
        let page = self.sync_runtime.block_on(run_query(
            &self.runtime,
            &self.profiles,
            &self.session,
            &sql,
        ))?;
        if page.columns.len() < 2 {
            return Err("the widget's query needs two columns: label and value".into());
        }
        Ok(page
            .rows
            .iter()
            .filter_map(|row| {
                let label = row.first()?.clone();
                let value = row
                    .get(1)
                    .and_then(|text| text.trim().parse().ok())
                    .unwrap_or(0);
                Some((label, value))
            })
            .collect())
    }

    fn table_inbound_references(&self, table: &str) -> Result<Vec<InboundRef>, String> {
        // T5: the shared scan over the sync session — its own connection on
        // the adapter's runtime, the same one the N10 structure reads use
        // (the live session's pool belongs to the worker's runtime).
        struct LiveNavigationSource {
            driver: Arc<dyn DatabaseDriver>,
            handle: ConnectionHandle,
            database: String,
        }

        #[async_trait::async_trait]
        impl datazen::services::ports::ForeignKeyNavigationSource for LiveNavigationSource {
            async fn bind_session(
                &self,
                _: &str,
            ) -> Result<
                datazen::services::ports::FkSessionBinding,
                datazen::services::ports::FkSessionError,
            > {
                Ok(datazen::services::ports::FkSessionBinding {
                    driver: Arc::clone(&self.driver),
                    handle: self.handle.clone(),
                    database: Some(self.database.clone()),
                    schema: None,
                    // The sync session lives in the adapter for as long as
                    // the connection holds; nothing to lease.
                    guard: Box::new(()),
                })
            }
        }

        let sync = self.sync_session()?;
        let source = LiveNavigationSource {
            driver: Arc::clone(&sync.driver),
            handle: sync.handle.clone(),
            database: sync.database.clone(),
        };
        let request = datazen::services::referenced_by::ListInboundForeignKeysRequest {
            // The envelope requires a session label; the host's one session
            // is the live one.
            db_session_id: "live".into(),
            database: None,
            schema: None,
            table: table.to_owned(),
            qualified_table: None,
        };
        let catalog = self
            .sync_runtime
            .block_on(datazen::services::referenced_by::list_inbound_foreign_keys(
                &source,
                &request,
            ))
            .map_err(|error| error.to_string())?;
        // Flatten the pairings: one row per (referencing column, referenced
        // column), the shape the UI's list and filters speak.
        let mut refs = Vec::new();
        for reference in &catalog.references {
            for (column, referenced) in reference.columns.iter().zip(&reference.referenced_columns) {
                refs.push(InboundRef {
                    table: reference.table.clone(),
                    column: column.clone(),
                    referenced_column: referenced.clone(),
                    constraint: reference.constraint_name.clone(),
                });
            }
        }
        Ok(refs)
    }

    fn snippets(&self) -> Result<Vec<Snippet>, String> {
        // T3: the snippets' file store — the same vocabulary the memory
        // port serves in the demo process.
        Ok(self.snippets.list())
    }

    fn save_snippet(&mut self, name: &str, sql: &str) -> Result<(), String> {
        self.snippets.save(name, sql)
    }

    fn update_snippet(&mut self, name: &str, sql: &str) -> Result<(), String> {
        self.snippets.update(name, sql)
    }

    fn delete_snippet(&mut self, name: &str) -> Result<(), String> {
        self.snippets.delete(name)
    }

    fn favorites(&self) -> Result<Vec<FavoriteSpec>, String> {
        // T6: the favorites root is the store.
        Ok(self.favorites.list())
    }

    fn save_favorite(&mut self, connection: &str, title: &str, sql: &str) -> Result<String, String> {
        self.favorites.add(connection, title, sql)
    }

    fn rename_favorite(&mut self, id: &str, title: &str) -> Result<(), String> {
        self.favorites.rename(id, title)
    }

    fn delete_favorite(&mut self, id: &str) -> Result<(), String> {
        self.favorites.delete(id)
    }

    fn workflows(&self) -> Result<Vec<Workflow>, String> {
        // N13: the registry use case's user-visible definitions — the
        // dashboard's hidden workflows stay out of the user list.
        let scope = crate::dashboards::host_scope();
        let mut definitions =
            futures_executor::block_on(self.workflows.list_definitions(&scope));
        definitions.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(definitions
            .into_iter()
            .map(|definition| Workflow {
                name: definition.name,
                steps: definition.steps.iter().map(crate::dashboards::step_text).collect(),
            })
            .collect())
    }

    fn save_workflow(&mut self, name: &str, steps: &[String]) -> Result<(), String> {
        // N13: the registry use case; the UI's steps are SQL statements,
        // each one a query step of the definition.
        let name = name.trim();
        if name.is_empty() {
            return Err("a workflow needs a name".into());
        }
        let scope = crate::dashboards::host_scope();
        let existing = futures_executor::block_on(self.workflows.list_definitions(&scope));
        let id = match existing.iter().find(|definition| definition.name == name) {
            Some(definition) => definition.id.clone(),
            None => crate::dashboards::workflow_id_for(
                name,
                &existing
                    .iter()
                    .map(|definition| definition.id.clone())
                    .collect::<Vec<_>>(),
            ),
        };
        let definition = WorkflowDefinition {
            database: None,
            id,
            name: name.to_owned(),
            description: "Saved in the native UI".into(),
            version: None,
            author: None,
            variables: Vec::new(),
            connection: None,
            steps: steps
                .iter()
                .enumerate()
                .map(|(index, sql)| WorkflowStep::Query {
                    id: format!("s{}", index + 1),
                    sql: sql.clone(),
                    connection: None,
                    database: None,
                    timeout_secs: Some(60),
                    on_error: None,
                })
                .collect(),
            blocks: Vec::new(),
            output: None,
            timeout_secs: Some(60),
            error_handling: None,
            schedule: None,
            visibility: WorkflowVisibility::User,
        };
        futures_executor::block_on(self.workflows.save(&scope, &definition))
    }

    fn run_workflow(&mut self, name: &str) -> Result<Vec<WorkflowStepResult>, String> {
        // N13: every query step runs through the execution use case — the
        // same path the editor's statements take. A step of another kind
        // reports itself as not wired; a step's failure does not stop the
        // steps that follow (the results list is the report).
        let scope = crate::dashboards::host_scope();
        let definition = futures_executor::block_on(self.workflows.list_definitions(&scope))
            .into_iter()
            .find(|definition| definition.name == name)
            .ok_or_else(|| format!("no workflow {name}"))?;
        let mut results = Vec::new();
        for step in &definition.steps {
            match step {
                WorkflowStep::Query { sql, .. } => {
                    let result = self.sync_runtime.block_on(run_query(
                        &self.runtime,
                        &self.profiles,
                        &self.session,
                        sql,
                    ));
                    results.push(match result {
                        Ok(page) => WorkflowStepResult {
                            step: sql.clone(),
                            ok: true,
                            summary: format!("ok: {} rows", page.total_rows),
                        },
                        Err(error) => WorkflowStepResult {
                            step: sql.clone(),
                            ok: false,
                            summary: error,
                        },
                    });
                }
                other => results.push(WorkflowStepResult {
                    step: crate::dashboards::step_text(other),
                    ok: false,
                    summary: "this step kind is not wired in the native host yet".into(),
                }),
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_support::sqlite_config;
    use crate::store::FileConnectionRepository;
    use datazen_ui_api::DesktopAdapter;
    use std::path::PathBuf;
    use std::time::Duration;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("datazen-rune-live-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Build a real sqlite database with two tables, through the driver
    /// itself. The empty file is created first — the driver's
    /// `create_if_missing` is false, so a missing file is a connect error,
    /// not a fresh database (the same shape as the repo's other real-sqlite
    /// fixtures). The fixture runs inside its own current-thread tokio
    /// runtime, as the worker's jobs do.
    fn create_sqlite_file(path: &std::path::Path) {
        std::fs::File::create(path).expect("the empty database file is created");
        let registry = host_registry();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the fixture runtime starts");
        runtime.block_on(async {
            let driver = registry
                .get(&"sqlite".into())
                .await
                .expect("the sqlite driver is linked into the host");
            let config = sqlite_config("fixture", Some(path.to_string_lossy().into_owned()));
            let handle = driver.connect(&config).await.expect("the fixture database opens");
            driver
                .execute(&handle, "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
                .await
                .expect("the users table is created");
            driver
                .execute(&handle, "CREATE TABLE events (id INTEGER PRIMARY KEY, label TEXT)")
                .await
                .expect("the events table is created");
            for index in 0..60 {
                driver
                    .execute(&handle, &format!(
                        "INSERT INTO users (id, name) VALUES ({index}, 'user-{index:02}')"
                    ))
                    .await
                    .expect("the fixture row is inserted");
            }
            driver
                .execute(&handle, "CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id), note TEXT)")
                .await
                .expect("the orders table with a foreign key is created");
            driver
                .execute(&handle, "CREATE UNIQUE INDEX idx_orders_note ON orders(note)")
                .await
                .expect("the unique index is created");
        });
    }

    /// Save a connection over the file store, through the use case.
    fn save_connection(store_path: &std::path::Path, config: datazen_driver_api::ConnectionConfig) {
        let service =
            ConnectionService::new(Arc::new(FileConnectionRepository::open(store_path).unwrap()));
        let scope = ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION);
        futures_executor::block_on(service.save(&scope, config)).unwrap();
    }

    /// Poll the adapter until its open job answers (the worker thread runs
    /// concurrently; a short retry window is plenty for a local file).
    fn poll_open(adapter: &mut LiveAdapter) -> Result<Catalog, String> {
        for _ in 0..300 {
            for delivery in adapter.poll_jobs() {
                if let AdapterDelivery::ConnectionOpened { catalog, .. } = delivery {
                    return catalog;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the open job never answered");
    }

    #[test]
    fn the_live_adapter_opens_a_real_sqlite_session_and_lists_its_tables() {
        let dir = temp_dir("open");
        let database = dir.join("live.db");
        create_sqlite_file(&database);
        let store_path = dir.join("connections.json");
        save_connection(
            &store_path,
            sqlite_config("live", Some(database.to_string_lossy().into_owned())),
        );

        let mut adapter = LiveAdapter::new(Arc::new(
            FileConnectionRepository::open(&store_path).unwrap(),
        ));
        // The connection list still answers synchronously from the file.
        let listed = adapter.list_connections().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "live");
        assert_eq!(listed[0].engine, "sqlite");

        assert!(
            adapter.begin_job(AdapterJob::OpenConnection { generation: 1, id: "live".into() }),
            "the live host takes the open job"
        );
        let catalog = poll_open(&mut adapter).expect("the session opens");
        // sqlite: one database "main", one schema node named for the database
        // (the driver has no schema level), both tables in sqlite_master's
        // name order.
        assert_eq!(catalog.databases.len(), 1);
        assert_eq!(catalog.databases[0].name, "main");
        assert_eq!(catalog.databases[0].schemas.len(), 1);
        let schema = &catalog.databases[0].schemas[0];
        assert_eq!(schema.name, "main");
        assert_eq!(
            schema.tables,
            vec!["events".to_string(), "orders".to_string(), "users".to_string()]
        );

        // The session stays for the reads and commits that follow (N4/N5).
        assert!(adapter.session().lock().unwrap().is_open(), "the live session is kept");
    }

    #[test]
    fn a_bad_database_file_answers_an_error_and_the_adapter_survives() {
        let dir = temp_dir("bad");
        let bad = dir.join("bad.db");
        std::fs::write(&bad, b"this is not a database").unwrap();
        let good = dir.join("good.db");
        create_sqlite_file(&good);
        let store_path = dir.join("connections.json");
        save_connection(&store_path, sqlite_config("bad", Some(bad.to_string_lossy().into_owned())));
        save_connection(
            &store_path,
            sqlite_config("good", Some(good.to_string_lossy().into_owned())),
        );

        let mut adapter = LiveAdapter::new(Arc::new(
            FileConnectionRepository::open(&store_path).unwrap(),
        ));
        assert!(adapter.begin_job(AdapterJob::OpenConnection { generation: 1, id: "bad".into() }));
        let error = poll_open(&mut adapter).expect_err("the bad file does not open");
        assert!(!error.is_empty(), "the failure carries a sentence: {error}");

        // The adapter is not poisoned: the good connection opens after the
        // failure, and the session replaces the never-opened one.
        assert!(adapter.begin_job(AdapterJob::OpenConnection { generation: 2, id: "good".into() }));
        let catalog = poll_open(&mut adapter).expect("the good connection opens after the failure");
        assert_eq!(catalog.databases[0].schemas[0].tables.len(), 3);
    }

    #[test]
    fn an_unknown_connection_answers_a_sentence() {
        let dir = temp_dir("unknown");
        let store_path = dir.join("connections.json");
        let mut adapter = LiveAdapter::new(Arc::new(
            FileConnectionRepository::open(&store_path).unwrap(),
        ));
        assert!(adapter.begin_job(AdapterJob::OpenConnection { generation: 1, id: "nope".into() }));
        let error = poll_open(&mut adapter).expect_err("an unknown id is a failure");
        assert!(error.contains("no connection"), "the sentence names it: {error}");
    }

    /// Poll the adapter until its read job answers.
    fn poll_read(adapter: &mut LiveAdapter) -> Result<TablePage, String> {
        for _ in 0..300 {
            for delivery in adapter.poll_jobs() {
                if let AdapterDelivery::TablePage { page, .. } = delivery {
                    return page;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the read job never answered");
    }

    /// Open the fixture connection through the adapter (the N3 job path) and
    /// return the adapter, ready for read jobs.
    fn opened_adapter(dir: &std::path::Path) -> LiveAdapter {
        let database = dir.join("live.db");
        create_sqlite_file(&database);
        let store_path = dir.join("connections.json");
        save_connection(
            &store_path,
            sqlite_config("live", Some(database.to_string_lossy().into_owned())),
        );
        let mut adapter = LiveAdapter::with_data_dir(
            Arc::new(FileConnectionRepository::open(&store_path).unwrap()),
            &dir,
        );
        assert!(
            adapter.begin_job(AdapterJob::OpenConnection { generation: 1, id: "live".into() })
        );
        poll_open(&mut adapter).expect("the session opens");
        adapter
    }

    #[test]
    fn read_jobs_serve_real_pages_sorts_and_filters_from_sqlite() {
        let dir = temp_dir("read");
        let mut adapter = opened_adapter(&dir);

        // Page 2 of 25: the second half of the 60 fixture rows, in id order
        // (the read's default order).
        assert!(adapter.begin_job(AdapterJob::ReadTable {
            generation: 2,
            table: "users".into(),
            page: 1,
            page_size: 25,
            sort: None,
            filter: None,
        }));
        let page = poll_read(&mut adapter).expect("the read answers");
        assert_eq!(page.table, "users");
        assert_eq!(page.columns, vec!["id".to_string(), "name".to_string()]);
        assert_eq!(page.rows.len(), 25, "the second page is full");
        assert_eq!(page.rows[0][1], "user-25", "page 2 starts at row 25");
        assert_eq!(page.rows[24][1], "user-49", "page 2 ends at row 49");
        assert_eq!(page.total_rows, 60, "the count is exact");

        // A descending sort is served by SQL: the last page's first row is
        // the highest id.
        assert!(adapter.begin_job(AdapterJob::ReadTable {
            generation: 3,
            table: "users".into(),
            page: 1,
            page_size: 25,
            sort: Some(ColumnSort { column: "id".into(), descending: true }),
            filter: None,
        }));
        let page = poll_read(&mut adapter).expect("the sorted read answers");
        assert_eq!(page.rows[0][1], "user-34", "descending id: page 2 starts at 34");
        assert_eq!(page.rows[24][1], "user-10", "descending id: page 2 ends at 10");

        // A contains filter is served by SQL too: only the matching rows.
        assert!(adapter.begin_job(AdapterJob::ReadTable {
            generation: 4,
            table: "users".into(),
            page: 0,
            page_size: 25,
            sort: None,
            filter: Some(ColumnFilter { column: "name".into(), contains: "user-5".into() }),
        }));
        let page = poll_read(&mut adapter).expect("the filtered read answers");
        let names: Vec<&str> = page.rows.iter().map(|row| row[1].as_str()).collect();
        assert_eq!(
            names,
            vec!["user-50", "user-51", "user-52", "user-53", "user-54", "user-55",
                "user-56", "user-57", "user-58", "user-59"],
            "the fixture's user-5x decade (user-05 does not contain 'user-5'), in id order"
        );
        assert_eq!(page.total_rows, 10, "the count follows the filter");

        // An unknown table is a failure, not an empty page.
        assert!(adapter.begin_job(AdapterJob::ReadTable {
            generation: 5,
            table: "nope".into(),
            page: 0,
            page_size: 25,
            sort: None,
            filter: None,
        }));
        let error = poll_read(&mut adapter).expect_err("an unknown table fails");
        assert!(!error.is_empty(), "the failure carries a sentence: {error}");
    }

    #[test]
    fn a_read_without_a_session_answers_a_sentence() {
        let dir = temp_dir("read-closed");
        let store_path = dir.join("connections.json");
        let mut adapter =
            LiveAdapter::new(Arc::new(FileConnectionRepository::open(&store_path).unwrap()));
        assert!(adapter.begin_job(AdapterJob::ReadTable {
            generation: 1,
            table: "users".into(),
            page: 0,
            page_size: 25,
            sort: None,
            filter: None,
        }));
        let error = poll_read(&mut adapter).expect_err("no session, no read");
        assert!(error.contains("session"), "the sentence names the state: {error}");
    }


    /// Poll the adapter until its commit job answers.
    fn poll_commit(adapter: &mut LiveAdapter) -> Result<u32, String> {
        for _ in 0..300 {
            for delivery in adapter.poll_jobs() {
                if let AdapterDelivery::CommitCount { count, .. } = delivery {
                    return count;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the commit job never answered");
    }

    #[test]
    fn commit_jobs_write_through_the_use_case_and_read_back() {
        let dir = temp_dir("commit");
        let mut adapter = opened_adapter(&dir);

        // The page the grid would have loaded: the driver's primary key rides
        // along (N6).
        assert!(adapter.begin_job(AdapterJob::ReadTable {
            generation: 2,
            table: "users".into(),
            page: 0,
            page_size: 25,
            sort: None,
            filter: None,
        }));
        let page = poll_read(&mut adapter).expect("the read answers");
        assert_eq!(page.primary_key, vec!["id".to_string()], "the driver's PK rides the page");

        // One edited row, as the grid's commit batch would build it: the PK
        // value the page showed, the original the page loaded, the new value.
        let batch = datazen_ui_api::CellEditBatch {
            table: "users".into(),
            page: 0,
            page_size: 25,
            sort: None,
            filter: None,
            rows: vec![datazen_ui_api::RowEdit {
                index: 0,
                identity: vec![("id".into(), page.rows[0][0].clone())],
                changes: vec![("name".into(), page.rows[0][1].clone(), "renamed".into())],
            }],
        };
        assert!(adapter.begin_job(AdapterJob::CommitCells { generation: 3, batch }));
        let landed = poll_commit(&mut adapter).expect("the commit lands");
        assert_eq!(landed, 1, "one row landed");

        // The next read shows the stored value: a real UPDATE went through.
        assert!(adapter.begin_job(AdapterJob::ReadTable {
            generation: 4,
            table: "users".into(),
            page: 0,
            page_size: 25,
            sort: None,
            filter: None,
        }));
        let page = poll_read(&mut adapter).expect("the read-back answers");
        assert_eq!(page.rows[0][1], "renamed", "the committed value reads back");
        assert_eq!(page.rows[1][1], "user-01", "the other rows are untouched");

        // A change to a column the table does not have is a failure sentence,
        // not a silent no-op.
        let bad = datazen_ui_api::CellEditBatch {
            table: "users".into(),
            page: 0,
            page_size: 25,
            sort: None,
            filter: None,
            rows: vec![datazen_ui_api::RowEdit {
                index: 1,
                identity: vec![("id".into(), "1".into())],
                changes: vec![("no_such_column".into(), String::new(), "x".into())],
            }],
        };
        assert!(adapter.begin_job(AdapterJob::CommitCells { generation: 5, batch: bad }));
        let error = poll_commit(&mut adapter).expect_err("an unknown column fails");
        assert!(!error.is_empty(), "the failure carries a sentence: {error}");
    }

    #[test]
    fn a_commit_without_identity_is_refused() {
        let dir = temp_dir("commit-no-identity");
        let mut adapter = opened_adapter(&dir);
        // The PK column missing from the identity: the use case's own gate
        // refuses it (grid.cell.noRowIdentity), not a host re-implementation.
        let batch = datazen_ui_api::CellEditBatch {
            table: "users".into(),
            page: 0,
            page_size: 25,
            sort: None,
            filter: None,
            rows: vec![datazen_ui_api::RowEdit {
                index: 0,
                identity: Vec::new(),
                changes: vec![("name".into(), "user-00".into(), "x".into())],
            }],
        };
        assert!(adapter.begin_job(AdapterJob::CommitCells { generation: 2, batch }));
        let error = poll_commit(&mut adapter).expect_err("a keyless change cannot commit");
        assert!(
            error.contains("noRowIdentity") || error.contains("primary key"),
            "the refusal names the missing identity: {error}"
        );
    }


    /// Poll the adapter until its query job answers.
    fn poll_query(adapter: &mut LiveAdapter) -> Result<TablePage, String> {
        for _ in 0..300 {
            for delivery in adapter.poll_jobs() {
                if let AdapterDelivery::QueryPage { page, .. } = delivery {
                    return page;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the query job never answered");
    }

    #[test]
    fn query_jobs_run_real_sql_and_answer_real_rows() {
        let dir = temp_dir("query");
        let mut adapter = opened_adapter(&dir);

        // A real SELECT through the runtime: the columns and the rows are the
        // driver's answer.
        assert!(adapter.begin_job(AdapterJob::RunQuery {
            generation: 2,
            sql: "SELECT id, name FROM users WHERE id < 3 ORDER BY id".into(),
        }));
        let page = poll_query(&mut adapter).expect("the query answers");
        assert_eq!(page.columns, vec!["id".to_string(), "name".to_string()]);
        assert_eq!(page.rows.len(), 3, "three rows match the WHERE");
        assert_eq!(page.rows[0], vec!["0".to_string(), "user-00".to_string()]);
        assert_eq!(page.rows[2], vec!["2".to_string(), "user-02".to_string()]);
        assert_eq!(page.total_rows, 3);
        assert!(page.read_only.is_some(), "the result page is read-only");

        // A write statement works through the same path: the rows affected
        // come back as the driver's report (the page is empty, the write
        // happened - the next query sees it).
        assert!(adapter.begin_job(AdapterJob::RunQuery {
            generation: 3,
            sql: "UPDATE users SET name = 'touched' WHERE id = 1".into(),
        }));
        let page = poll_query(&mut adapter).expect("the write answers");
        assert!(page.rows.is_empty(), "an UPDATE answers no rows");

        assert!(adapter.begin_job(AdapterJob::RunQuery {
            generation: 4,
            sql: "SELECT name FROM users WHERE id = 1".into(),
        }));
        let page = poll_query(&mut adapter).expect("the read-back answers");
        assert_eq!(page.rows[0][0], "touched", "the write really landed");

        // A broken statement is a failure sentence, not an empty page.
        assert!(adapter.begin_job(AdapterJob::RunQuery {
            generation: 5,
            sql: "SELECT no_such_column FROM users".into(),
        }));
        let error = poll_query(&mut adapter).expect_err("a broken statement fails");
        assert!(!error.is_empty(), "the failure carries a sentence: {error}");
    }

    #[test]
    fn a_query_without_a_session_answers_a_sentence() {
        let dir = temp_dir("query-closed");
        let store_path = dir.join("connections.json");
        let mut adapter =
            LiveAdapter::new(Arc::new(FileConnectionRepository::open(&store_path).unwrap()));
        assert!(adapter.begin_job(AdapterJob::RunQuery {
            generation: 1,
            sql: "SELECT 1".into(),
        }));
        let error = poll_query(&mut adapter).expect_err("no session, no query");
        assert!(error.contains("session"), "the sentence names the state: {error}");
    }

    #[test]
    fn ui_settings_persist_through_the_file_and_survive_a_reopen() {
        let dir = temp_dir("settings");
        let store_path = dir.join("connections.json");
        let repository = Arc::new(FileConnectionRepository::open(&store_path).unwrap());

        // A fresh adapter: no ui settings yet (the memory port's defaults
        // answer the UI), a write lands in the file, a read answers it.
        let mut adapter = LiveAdapter::with_data_dir(repository.clone(), &dir);
        assert!(adapter.read_ui_settings().is_err(), "a fresh install has no ui settings");
        adapter
            .write_ui_setting(datazen_ui_api::Setting::TablePageSize { rows: 50 })
            .unwrap();
        adapter.write_ui_setting(datazen_ui_api::Setting::Theme { dark: false }).unwrap();
        let settings = adapter.read_ui_settings().unwrap();
        assert!(settings.contains(&datazen_ui_api::Setting::TablePageSize { rows: 50 }));
        assert!(settings.contains(&datazen_ui_api::Setting::Theme { dark: false }));
        assert!(dir.join("settings.json").exists(), "the settings file exists");

        // A fresh adapter over the same data dir sees them: persistence.
        let mut reopened = LiveAdapter::with_data_dir(repository, &dir);
        let settings = reopened.read_ui_settings().unwrap();
        assert!(settings.contains(&datazen_ui_api::Setting::TablePageSize { rows: 50 }));

        // The validation rules are the product's, not the store's: an
        // out-of-range page size is refused and nothing is written.
        let before = dir.join("settings.json");
        let before = std::fs::read_to_string(&before).unwrap();
        let error = reopened
            .write_ui_setting(datazen_ui_api::Setting::TablePageSize { rows: 999 })
            .unwrap_err();
        assert!(error.contains("1..=500"), "the refusal names the rule: {error}");
        let after = std::fs::read_to_string(&dir.join("settings.json")).unwrap();
        assert_eq!(before, after, "a refused write changes nothing on disk");
    }

    #[test]
    fn executed_statements_land_in_the_history_file() {
        let dir = temp_dir("history");
        let mut adapter = opened_adapter(&dir);

        // One successful query and one failed one — both belong in history.
        assert!(adapter.begin_job(AdapterJob::RunQuery {
            generation: 2,
            sql: "SELECT id FROM users WHERE id = 0".into(),
        }));
        poll_query(&mut adapter).expect("the good query answers");
        assert!(adapter.begin_job(AdapterJob::RunQuery {
            generation: 3,
            sql: "SELECT no_such_column FROM users".into(),
        }));
        poll_query(&mut adapter).expect_err("the broken statement fails");

        // The history file holds both, with their verdicts and timing.
        let path = dir.join("query-history.json");
        let raw = std::fs::read_to_string(&path).expect("the history file exists");
        let entries: Vec<datazen::history::QueryHistoryEntry> =
            serde_json::from_str(&raw).expect("the history parses");
        assert_eq!(entries.len(), 2, "both statements are recorded");
        assert!(entries.iter().all(|entry| entry.connection_id == "live"));
        let good = entries.iter().find(|entry| entry.sql.contains("id = 0")).unwrap();
        assert!(good.success, "the successful query is marked");
        let bad = entries.iter().find(|entry| entry.sql.contains("no_such_column")).unwrap();
        assert!(!bad.success, "the failed query is marked");
        assert!(bad.error_message.is_some(), "the failure carries its sentence");
        assert!(good.execution_time_ms > 0 || bad.execution_time_ms > 0, "the timing was measured");
    }

    #[test]
    fn structure_reads_answer_the_real_catalog() {
        let dir = temp_dir("structure");
        let mut adapter = opened_adapter(&dir);

        // Warm the cache through a read (the page the grid would load).
        assert!(adapter.begin_job(AdapterJob::ReadTable {
            generation: 2,
            table: "orders".into(),
            page: 0,
            page_size: 25,
            sort: None,
            filter: None,
        }));
        poll_read(&mut adapter).expect("the read answers");

        // The columns page: the driver's names, types, nullability.
        let columns = adapter.table_columns("orders").expect("the columns answer");
        assert_eq!(columns.len(), 3);
        assert_eq!(columns[0].name, "id");
        assert_eq!(columns[1].name, "user_id");
        assert!(!columns[1].nullable, "user_id is NOT NULL");
        assert!(columns[2].nullable, "note is nullable");

        // The index list keeps the compound-free but real index, with its
        // uniqueness.
        let indexes = adapter.table_indexes("orders").expect("the indexes answer");
        let unique = indexes
            .iter()
            .find(|index| index.name == "idx_orders_note")
            .expect("the created index is listed");
        assert!(unique.unique, "the index was created UNIQUE");
        assert_eq!(unique.columns, vec!["note".to_string()]);

        // The foreign key list keeps the reference whole.
        let keys = adapter.table_foreign_keys("orders").expect("the keys answer");
        let fk = keys.iter().find(|key| key.columns == vec!["user_id".to_string()])
            .expect("the FK is listed");
        assert_eq!(fk.target_table, "users");
        assert_eq!(fk.target_columns, vec!["id".to_string()]);

        // The DDL is the driver's own rendering of the definition.
        let ddl = adapter.table_ddl("orders").expect("the DDL answers");
        assert!(ddl.to_lowercase().contains("create table"), "the DDL names the object: {ddl}");
        assert!(ddl.contains("orders"), "the DDL is about this table: {ddl}");

        // A table the catalog does not know is a failure sentence.
        let error = adapter.table_columns("nope").unwrap_err();
        assert!(!error.is_empty(), "the failure carries a sentence: {error}");
    }

    /// Wait for a value search job's delivery (T4).
    fn poll_value_search(adapter: &mut LiveAdapter, generation: u32) -> Option<Vec<ValueSearchHit>> {
        for _ in 0..300 {
            for delivery in adapter.poll_jobs() {
                if let AdapterDelivery::ValueSearched { generation: g, hits } = delivery {
                    if g == generation {
                        return Some(hits.expect("the scan succeeded"));
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    /// Wait for a host job to reach a terminal status.
    fn wait_job(adapter: &LiveAdapter, job: u64) -> datazen_ui_api::ProgressSnapshot {
        for _ in 0..300 {
            if let Some(snapshot) = adapter.host_job_progress(job) {
                if snapshot.status != "running" {
                    return snapshot;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the job never finished");
    }

    #[test]
    fn export_jobs_write_real_csv_and_json() {
        let dir = temp_dir("export");
        let mut adapter = opened_adapter(&dir);

        // CSV: the whole table, header and every row, quoted where needed.
        let job = adapter.start_export("users", "csv", "", None).expect("the export starts");
        let snapshot = wait_job(&adapter, job);
        assert_eq!(snapshot.status, "completed", "log: {:?}", snapshot.log);
        let exports = std::fs::read_dir(dir.join("exports")).expect("the exports dir exists");
        let csv = exports
            .filter_map(|entry| entry.ok())
            .find(|entry| entry.path().extension().is_some_and(|ext| ext == "csv"))
            .expect("a csv file was written");
        let body = std::fs::read_to_string(csv.path()).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines[0], "id,name", "the header names the columns");
        assert_eq!(lines.len(), 61, "60 rows plus the header");
        assert_eq!(lines[1], "0,user-00", "the first row");
        assert_eq!(lines[60], "59,user-59", "the last row");

        // A range the host cannot serve is refused honestly.
        let job = adapter.start_export("users", "csv", "rows 1-5", None).expect("the job starts");
        let snapshot = wait_job(&adapter, job);
        assert_eq!(snapshot.status, "failed", "an unsupported range fails: {:?}", snapshot.log);

        // A format the host has not wired is refused honestly.
        let job = adapter.start_export("users", "sql", "", None).expect("the job starts");
        let snapshot = wait_job(&adapter, job);
        assert_eq!(snapshot.status, "failed", "sql export is not wired: {:?}", snapshot.log);

        // T2: a picked save path wins over the default directory naming.
        let picked = dir.join("picked-exports").join("users.csv");
        let job = adapter
            .start_export("users", "csv", "", Some(picked.to_str().unwrap()))
            .expect("the export starts");
        let snapshot = wait_job(&adapter, job);
        assert_eq!(snapshot.status, "completed", "log: {:?}", snapshot.log);
        let body = std::fs::read_to_string(&picked).expect("the picked file was written");
        assert_eq!(body.lines().count(), 61, "the same whole-table export lands at the picked path");
    }

    #[test]
    fn backup_jobs_write_the_dump() {
        let dir = temp_dir("backup");
        let mut adapter = opened_adapter(&dir);

        let job = adapter.start_backup(None).expect("the backup starts");
        let snapshot = wait_job(&adapter, job);
        assert_eq!(snapshot.status, "completed", "log: {:?}", snapshot.log);
        let backups = std::fs::read_dir(dir.join("backups")).expect("the backups dir exists");
        let dump = backups
            .filter_map(|entry| entry.ok())
            .find(|entry| entry.path().extension().is_some_and(|ext| ext == "sql"))
            .expect("a backup file was written");
        let body = std::fs::read_to_string(dump.path()).unwrap();
        assert!(!body.is_empty(), "the backup is not empty");
        assert!(
            body.to_lowercase().contains("create table"),
            "the dump carries the schema: {} bytes",
            body.len()
        );
        assert!(body.contains("users"), "the dump names the table");

        // T2: a picked save path wins over the default directory naming.
        let picked = dir.join("picked-backup.sql");
        let job = adapter
            .start_backup(Some(picked.to_str().unwrap()))
            .expect("the backup starts");
        let snapshot = wait_job(&adapter, job);
        assert_eq!(snapshot.status, "completed", "log: {:?}", snapshot.log);
        let picked_body = std::fs::read_to_string(&picked).expect("the picked file was written");
        assert!(!picked_body.is_empty(), "the dump lands at the picked path");
    }

    #[test]
    fn inbound_references_answer_the_scan_over_the_live_session() {
        let dir = temp_dir("inbound");
        let adapter = opened_adapter(&dir);

        // The fixture: orders.user_id REFERENCES users(id).
        let refs = adapter.table_inbound_references("users").expect("the scan answers");
        assert_eq!(refs.len(), 1, "one referencing column: {refs:?}");
        assert_eq!(refs[0].table, "orders");
        assert_eq!(refs[0].column, "user_id");
        assert_eq!(refs[0].referenced_column, "id");

        // A table nothing references: the honest empty list.
        let refs = adapter.table_inbound_references("events").expect("the scan answers");
        assert!(refs.is_empty(), "no table references events: {refs:?}");
    }

    #[test]
    fn a_value_search_finds_the_columns_that_match() {
        let dir = temp_dir("value-search");
        let mut adapter = opened_adapter(&dir);

        // contains "user-4": the users.name column has ten matching rows
        // (user-40..user-49); events.label has none.
        assert!(adapter.begin_job(AdapterJob::ValueSearch {
            generation: 1,
            value: "user-4".into(),
            mode: "contains".into(),
        }));
        let hits = poll_value_search(&mut adapter, 1).expect("the scan answers");
        let users_name = hits
            .iter()
            .find(|hit| hit.table == "users" && hit.column == "name")
            .expect("users.name is a hit");
        assert_eq!(users_name.matches, 10, "user-40 through user-49: {:?}", hits);
        assert!(
            !hits.iter().any(|hit| hit.table == "events"),
            "events carries no matching value: {:?}",
            hits
        );

        // suffix "2": the names ending in 2 — user-02, 12, …, 52.
        assert!(adapter.begin_job(AdapterJob::ValueSearch {
            generation: 2,
            value: "2".into(),
            mode: "suffix".into(),
        }));
        let hits = poll_value_search(&mut adapter, 2).expect("the scan answers");
        let users_name = hits
            .iter()
            .find(|hit| hit.table == "users" && hit.column == "name")
            .expect("users.name is a hit");
        assert_eq!(users_name.matches, 6, "six names end in 2: {:?}", hits);

        // exact: one row.
        assert!(adapter.begin_job(AdapterJob::ValueSearch {
            generation: 3,
            value: "user-42".into(),
            mode: "exact".into(),
        }));
        let hits = poll_value_search(&mut adapter, 3).expect("the scan answers");
        assert_eq!(hits.len(), 1, "one column matches exactly: {:?}", hits);
        assert_eq!(hits[0].matches, 1);

        // A value nothing holds: the honest empty list.
        assert!(adapter.begin_job(AdapterJob::ValueSearch {
            generation: 4,
            value: "no such value".into(),
            mode: "contains".into(),
        }));
        let hits = poll_value_search(&mut adapter, 4).expect("the scan answers");
        assert!(hits.is_empty(), "no column matches: {:?}", hits);
    }

    #[test]
    fn snippets_persist_through_the_adapter_and_the_file() {
        let dir = temp_dir("snippets");
        let mut adapter = opened_adapter(&dir);

        // Create, refuse a duplicate, update, delete — through the adapter.
        adapter.save_snippet("top customers", "SELECT * FROM users").unwrap();
        assert!(adapter.save_snippet("top customers", "SELECT 1").is_err());
        adapter.save_snippet("by name", "SELECT * FROM users ORDER BY name").unwrap();
        adapter.update_snippet("by name", "SELECT 1").unwrap();
        let listed = adapter.snippets().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[1].sql, "SELECT 1", "the update landed");
        adapter.delete_snippet("top customers").unwrap();
        assert_eq!(adapter.snippets().unwrap().len(), 1);

        // A fresh store over the same file reads the same snippets back:
        // the file is the store (the live session keeps the sqlite file
        // locked, so the reopen goes through the store, not the adapter).
        let store = crate::snippets::FileSnippets::open(&dir.join("snippets.json")).unwrap();
        let listed = store.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "by name");
        let on_disk = std::fs::read_to_string(dir.join("snippets.json")).unwrap();
        assert!(on_disk.contains("by name"), "the file carries the snippet: {on_disk}");
    }

    #[test]
    fn sync_copies_a_table_and_compare_finds_the_missing_column() {
        let dir = temp_dir("sync");
        let store_path = dir.join("connections.json");
        // Source: the fixture database (users with 60 rows, orders with an
        // FK, events).
        let source_db = dir.join("live.db");
        create_sqlite_file(&source_db);
        let source_path = source_db.to_string_lossy().into_owned();
        // Target: a second real database. Its users table has the source's
        // shape (the sync's landing zone); its orders table is an older
        // shape — no note column — for the compare to find.
        let target_db = dir.join("target.db");
        std::fs::File::create(&target_db).expect("the empty target file");
        let target_path = target_db.to_string_lossy().into_owned();
        {
            let driver = crate::live::test_support::sqlite_driver();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("the fixture runtime starts");
            let value = target_path.clone();
            runtime.block_on(async move {
                let config = sqlite_config("target", Some(value.clone()));
                let handle = driver.connect(&config).await.expect("the target opens");
                driver
                    .execute(&handle, "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
                    .await
                    .expect("the target users table is created");
                driver
                    .execute(&handle, "CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER)")
                    .await
                    .expect("the target orders table is created without the note column");
            });
        }

        // Both connections in the store; the adapter over them.
        let repository = Arc::new(FileConnectionRepository::open(&store_path).unwrap());
        let service = ConnectionService::new(repository.clone());
        futures_executor::block_on(service.save(
            &ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION),
            sqlite_config("source", Some(source_path.clone())),
        ))
        .unwrap();
        futures_executor::block_on(service.save(
            &ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION),
            sqlite_config("target", Some(target_path.clone())),
        ))
        .unwrap();
        let adapter = LiveAdapter::with_data_dir(repository, &dir);

        let host_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the host runtime starts");

        // The compare: the note column is missing on the target.
        let comparison = host_runtime
            .block_on(async {
                adapter
                    .compare_table("source", "orders", "target", "orders")
                    .await
            })
            .expect("the compare answers");
        assert!(
            comparison
                .missing_on_target
                .iter()
                .any(|column| column.name == "note"),
            "the missing column is found: {:?}",
            comparison.missing_on_target
        );

        // The sync: 60 rows land in the same-shape target table through the
        // read and execution use cases.
        let landed = host_runtime
            .block_on(async {
                let (runtime, _, _) = crate::backend::assemble_runtime(
                    Arc::new(host_registry()),
                    Arc::new(FileConnectionRepository::open(&store_path).unwrap()),
                    &dir,
                );
                adapter.sync_table(&runtime, "source", "users", "target").await
            })
            .expect("the sync lands");
        assert_eq!(landed, 60, "every source row landed");

        // The target really has the rows.
        {
            let driver = crate::live::test_support::sqlite_driver();
            let value = target_path.clone();
            host_runtime.block_on(async move {
                let config = sqlite_config("target", Some(value.clone()));
                let handle = driver.connect(&config).await.expect("the target reopens");
                let result = driver
                    .query(&handle, "SELECT COUNT(*) FROM users")
                    .await
                    .expect("the count answers");
                let count = result
                    .rows
                    .first()
                    .and_then(|row| row.first())
                    .and_then(|cell| cell.as_ref())
                    .map(|cell| match cell {
                        Cell::Integer(value) => *value,
                        _ => 0,
                    })
                    .unwrap_or(0);
                assert_eq!(count, 60, "the target has the data");
            });
        }
    }

    #[test]
    fn a_saved_workflow_round_trips_and_survives_a_restart() {
        let dir = temp_dir("workflow-persist");
        let mut adapter = LiveAdapter::with_data_dir(
            Arc::new(FileConnectionRepository::open(&dir.join("connections.json")).unwrap()),
            &dir,
        );
        // A fresh install lists no workflows.
        assert!(adapter.workflows().unwrap().is_empty());
        adapter
            .save_workflow("nightly etl", &["SELECT 1".into(), "SELECT 2".into()])
            .unwrap();
        let listed = adapter.workflows().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "nightly etl");
        assert_eq!(
            listed[0].steps,
            vec!["SELECT 1".to_owned(), "SELECT 2".to_owned()],
            "the UI's steps are the definition's query steps"
        );

        // A restart — a fresh adapter over the same data directory — still
        // lists it (the completion standard of N13).
        let mut adapter = LiveAdapter::with_data_dir(
            Arc::new(FileConnectionRepository::open(&dir.join("connections.json")).unwrap()),
            &dir,
        );
        let listed = adapter.workflows().unwrap();
        assert_eq!(listed.len(), 1, "the workflow survived the restart");
        assert_eq!(
            listed[0].steps,
            vec!["SELECT 1".to_owned(), "SELECT 2".to_owned()]
        );

        // An edit replaces the stored steps; the list stays one workflow.
        adapter.save_workflow("nightly etl", &["SELECT 3".into()]).unwrap();
        let listed = adapter.workflows().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].steps,
            vec!["SELECT 3".to_owned()],
            "the edit replaced the steps"
        );

        // The port's contract: an empty name is refused.
        assert!(adapter.save_workflow("   ", &[]).is_err());
    }

    #[test]
    fn a_widget_survives_a_restart_and_its_bars_read_the_live_database() {
        let dir = temp_dir("widget-persist");
        let database = dir.join("live.db");
        create_sqlite_file(&database);
        let store_path = dir.join("connections.json");
        save_connection(
            &store_path,
            sqlite_config("live", Some(database.to_string_lossy().into_owned())),
        );

        let mut adapter = LiveAdapter::with_data_dir(
            Arc::new(FileConnectionRepository::open(&store_path).unwrap()),
            &dir,
        );
        adapter.begin_job(AdapterJob::OpenConnection { generation: 1, id: "live".into() });
        poll_open(&mut adapter).expect("the session opens");

        // A fresh install answers no widgets; the port's title contract
        // holds on the host path too.
        assert!(adapter.dashboard_widgets().unwrap().is_empty());
        assert!(adapter.save_dashboard_widget("  ", "SELECT 1").is_err());

        let id = adapter
            .save_dashboard_widget("First four", "SELECT name, id FROM users ORDER BY id LIMIT 4")
            .unwrap();
        let widgets = adapter.dashboard_widgets().unwrap();
        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].id, id, "the saved widget lists under the returned id");
        assert_eq!(widgets[0].title, "First four");
        assert_eq!(widgets[0].kind, "bar");

        // The bars are the live query's rows: the first column is the
        // label, the second the value.
        let bars = adapter.widget_bars(id).unwrap();
        assert_eq!(
            bars,
            vec![
                ("user-00".to_owned(), 0),
                ("user-01".to_owned(), 1),
                ("user-02".to_owned(), 2),
                ("user-03".to_owned(), 3),
            ],
            "the chart reads the live database"
        );

        // A restart keeps the widget and its chart answers again.
        let mut adapter = LiveAdapter::with_data_dir(
            Arc::new(FileConnectionRepository::open(&store_path).unwrap()),
            &dir,
        );
        adapter.begin_job(AdapterJob::OpenConnection { generation: 1, id: "live".into() });
        poll_open(&mut adapter).expect("the session opens again");
        let widgets = adapter.dashboard_widgets().unwrap();
        assert_eq!(widgets.len(), 1, "the widget survived the restart");
        assert_eq!(widgets[0].title, "First four");
        let bars = adapter.widget_bars(widgets[0].id).unwrap();
        assert_eq!(bars.len(), 4, "the chart answers after the restart");
        assert_eq!(bars.first().map(|(label, _)| label.clone()), Some("user-00".into()));
    }

    #[test]
    fn a_workflow_runs_its_steps_through_the_live_session() {
        let dir = temp_dir("workflow-run");
        let database = dir.join("live.db");
        create_sqlite_file(&database);
        let store_path = dir.join("connections.json");
        save_connection(
            &store_path,
            sqlite_config("live", Some(database.to_string_lossy().into_owned())),
        );
        let mut adapter = LiveAdapter::with_data_dir(
            Arc::new(FileConnectionRepository::open(&store_path).unwrap()),
            &dir,
        );
        adapter.begin_job(AdapterJob::OpenConnection { generation: 1, id: "live".into() });
        poll_open(&mut adapter).expect("the session opens");

        adapter
            .save_workflow(
                "counts",
                &[
                    "SELECT count(*) FROM users".into(),
                    "SELECT no_such_column FROM users".into(),
                ],
            )
            .unwrap();
        let results = adapter.run_workflow("counts").unwrap();
        assert_eq!(results.len(), 2, "every step reports");
        assert!(results[0].ok, "the count ran: {}", results[0].summary);
        assert!(
            results[0].summary.contains("1 rows"),
            "count answers one row: {}",
            results[0].summary
        );
        assert!(!results[1].ok, "the broken statement failed");
        assert!(!results[1].summary.is_empty(), "the failure carries a sentence");

        // The port's contract: an unknown workflow is an error.
        assert!(adapter.run_workflow("no-such").is_err());
    }
}

/// Test-only helpers shared with the backend module's tests.
#[cfg(test)]
pub(crate) mod test_support {
    /// The sqlite driver, resolved through the host's static registry.
    pub(crate) fn sqlite_driver() -> std::sync::Arc<dyn datazen_driver_api::DatabaseDriver> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the helper runtime starts");
        runtime.block_on(async {
            super::host_registry()
                .get(&"sqlite".into())
                .await
                .expect("the sqlite driver is linked into the host")
        })
    }
}