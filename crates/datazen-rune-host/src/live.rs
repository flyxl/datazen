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
use datazen_driver_api::filters::{FilterCondition, FilterOperator, SortCondition};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, Value as Cell};
use datazen_ui_api::{
    AdapterDelivery, AdapterJob, Catalog, ColumnFilter, ColumnSort, ConnectionProfile,
    DatabaseNode, DesktopAdapter, SchemaNode, TablePage, DESKTOP_NOT_IMPLEMENTED,
};
use std::path::Path;
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
    /// The sync reads' own connection: the live session's pool is single and
    /// belongs to the worker's runtime, so the sync runtime opens its own —
    /// created lazily per connection id and reused while the id holds.
    sync_handle: Mutex<Option<SyncSession>>,
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
        // The use case is stateless over the repository: the adapter and the
        // worker each hold their own service over the one store.
        let service = ConnectionService::new(Arc::clone(&repository));
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
        }
    }

    /// The live session, for the hosts that follow (N4 reads, N5 commits).
    pub(crate) fn session(&self) -> &Arc<Mutex<LiveSession>> {
        &self.session
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
        let scope = ConnectionScope::new(datazen_ui_api::DESKTOP_ORGANIZATION);
        let config = futures_executor::block_on(self.service.get(&scope, &connection))
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("no connection '{connection}' in the store"))?;
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