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
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::execution::target::ExecutionTarget;
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::{ConnectionId, Counter};
use datazen::table_browsing::api::{
    read_table_page, TargetBinding, TargetCommandRunner, TableReadRequest,
};
use datazen::table_editing::change_set::RowChangeTableContext;
use datazen_driver_api::filters::{FilterCondition, FilterOperator, SortCondition};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, Value as Cell};
use datazen_ui_api::{
    AdapterDelivery, AdapterJob, Catalog, ColumnFilter, ColumnSort, ConnectionProfile,
    DatabaseNode, DesktopAdapter, SchemaNode, TablePage, DESKTOP_NOT_IMPLEMENTED,
};
use std::sync::{Arc, Mutex};
use std::thread;

/// The host's registry, in its static form: every factory the host links
/// is admitted at construction — the registry's own constructor, not a
/// second registration — so a type is either here or a hard error, and no
/// test-binary link can drop an inventory registration the lazy form
/// would scan for. N3 links sqlite; other drivers join this list on the
/// same one-line seam.
fn host_registry() -> DriverRegistry {
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
}

/// A finished job on the answer channel.
enum HostAnswer {
    Connection { generation: u32, id: String, catalog: Result<Catalog, String> },
    TablePage { generation: u32, page: Result<TablePage, String> },
}

/// The open live session, shared between the adapter and the worker. N4's
/// reads and N5's commits run over the same session.
#[derive(Default)]
struct LiveSession {
    driver: Option<Arc<dyn DatabaseDriver>>,
    handle: Option<ConnectionHandle>,
    database: Option<String>,
    /// The connection id the session was opened for (N4's read context).
    connection: Option<String>,
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
}

impl LiveAdapter {
    /// The live host's adapter: connections from the file store, sessions
    /// through the drivers the host's registry assembles — the sqlite
    /// factory, linked in by this crate, admitted at construction.
    pub fn new(repository: Arc<dyn ConnectionRepository>) -> Self {
        let (job_tx, job_rx) = std::sync::mpsc::channel::<HostJob>();
        let (answer_tx, answer_rx) = std::sync::mpsc::channel::<HostAnswer>();
        let registry = host_registry();
        // The use case is stateless over the repository: the adapter and the
        // worker each hold their own service over the one store.
        let service = ConnectionService::new(Arc::clone(&repository));
        let worker_service = ConnectionService::new(repository);
        let session = Arc::new(Mutex::new(LiveSession::default()));
        let worker_session = Arc::clone(&session);
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
                            let catalog = runtime
                                .block_on(open_connection(&worker_service, &registry, &worker_session, &id));
                            HostAnswer::Connection { generation, id: id_for_answer, catalog }
                        }
                        HostJob::ReadTable { generation, table, page, page_size, sort, filter } => {
                            let page_result = runtime.block_on(read_table(
                                &worker_session,
                                &table,
                                page,
                                page_size,
                                sort,
                                filter,
                            ));
                            HostAnswer::TablePage { generation, page: page_result }
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
        }
    }

    /// The live session, for the hosts that follow (N4 reads, N5 commits).
    pub(crate) fn session(&self) -> &Arc<Mutex<LiveSession>> {
        &self.session
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

    // The session is the adapter's for the reads and commits that follow:
    // swap it in whole, replacing any earlier one.
    *session.lock().expect("the live session lock") = LiveSession {
        driver: Some(driver),
        handle: Some(handle),
        database: databases.first().cloned(),
        connection: Some(id.to_owned()),
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
    table: &str,
    page: u32,
    page_size: u32,
    sort: Option<ColumnSort>,
    filter: Option<ColumnFilter>,
) -> Result<TablePage, String> {
    let (driver, database, connection) = {
        let session = session.lock().expect("the live session lock");
        if !session.is_open() {
            return Err("the live session is not open".into());
        }
        (
            session.driver.clone().expect("checked"),
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
            database,
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
    // The count the driver could not give is the absence of a fact: fall
    // back to the best lower bound (the page's own end), never to zero.
    let known = read.total_rows.unwrap_or(page as i64 * page_size as i64 + read.rows.len() as i64);
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
            // N5-N6 wire the commits and queries; until then decline
            // so the synchronous paths answer.
            _ => return false,
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
            }
        }
        deliveries
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
        assert_eq!(schema.tables, vec!["events".to_string(), "users".to_string()]);

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
        assert_eq!(catalog.databases[0].schemas[0].tables.len(), 2);
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
        let mut adapter = LiveAdapter::new(Arc::new(
            FileConnectionRepository::open(&store_path).unwrap(),
        ));
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
}
