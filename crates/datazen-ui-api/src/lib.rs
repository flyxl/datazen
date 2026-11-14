//! Ports the Rune UI is allowed to call.
//!
//! Hosts implement [`WorkspaceApi`]. The memory workspace is only a stand-in so the
//! interface can run before the desktop and HTTP adapters exist.

mod tabs;

use std::collections::HashMap;

use datazen_driver_api::{ConnectionConfig, SslMode};

pub use tabs::{Tab, TabKind, Tabs};

/// One foreign key: this table's columns (a compound key stays together), the
/// target table and its columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignKeySpec {
    pub name: String,
    pub columns: Vec<String>,
    pub target_table: String,
    pub target_columns: Vec<String>,
}

/// One index of a table: its name, its columns in index order, uniqueness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexSpec {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

/// One pending column change: add a column, or retype one. Drafts only —
/// nothing touches the table until the port applies the SQL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnDraft {
    AddColumn { name: String, sql_type: String },
    ChangeType { column: String, sql_type: String },
}

impl ColumnDraft {
    /// The ALTER TABLE statement this draft stands for. The preview shows it;
    /// nothing executes it but the port's apply.
    pub fn sql(&self, table: &str) -> String {
        match self {
            Self::AddColumn { name, sql_type } => {
                format!("ALTER TABLE {table} ADD COLUMN {name} {sql_type};")
            }
            Self::ChangeType { column, sql_type } => {
                format!("ALTER TABLE {table} ALTER COLUMN {column} TYPE {sql_type};")
            }
        }
    }
}

/// One column of a table: name, type, nullability. For lists, not editors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnSpec {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
}

/// The three name layers the port reports: databases, their schemas, and the
/// tables inside each schema. Orders are the port's and stay stable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalog {
    pub databases: Vec<DatabaseNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseNode {
    pub name: String,
    pub schemas: Vec<SchemaNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaNode {
    pub name: String,
    pub tables: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub engine: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub connection_id: String,
    pub tables: Vec<String>,
}

/// One sort column: ascending first, `descending` flips it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnSort {
    pub column: String,
    pub descending: bool,
}

/// One text filter: rows whose `column` contains `contains` stay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnFilter {
    pub column: String,
    pub contains: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TablePage {
    pub table: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub page: u32,
    pub page_size: u32,
    /// Rows in the whole table, not only this page. Footers page over it.
    pub total_rows: u32,
    /// The sort this page was opened with, echoed back for display.
    pub sort: Option<ColumnSort>,
    /// The filter this page was opened with, echoed back for display.
    pub filter: Option<ColumnFilter>,
    /// Why the page cannot be edited; None means writable.
    pub read_only: Option<String>,
}

/// One job's progress, in one piece. The port hands these out when asked;
/// the UI never streams.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgressSnapshot {
    pub job: u64,
    /// "running", "completed", "failed" or "stopped".
    pub status: String,
    pub percent: u32,
    pub log: Vec<String>,
}

/// One job the port runs (export or backup). The memory port advances a job
/// on every progress read; real drivers advance by themselves.
#[derive(Clone, Debug)]
struct MemoryJob {
    job: u64,
    format: String,
    range: String,
    reads: u32,
    stopped: bool,
    log: Vec<String>,
    /// The error line a failing job reports; None means the job succeeds.
    fail_line: Option<String>,
}

/// One step's run result: the step name, success, a one-line summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowStepResult {
    pub step: String,
    pub ok: bool,
    pub summary: String,
}

/// One workflow: a name and its steps, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workflow {
    pub name: String,
    pub steps: Vec<String>,
}

/// One dashboard widget: its id, its title, its kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DashboardWidget {
    pub id: u64,
    pub title: String,
    pub kind: String,
}

/// One object of a structure comparison: its name and whether the target
/// endpoint has an object of the same name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareObject {
    pub name: String,
    pub present_on_target: bool,
}

/// One saved SQL snippet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snippet {
    pub name: String,
    pub sql: String,
}

/// One typed settings entry. A write validates a single entry; a failure
/// leaves every entry at its old value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setting {
    Theme { dark: bool },
    TablePageSize { rows: u32 },
    QueryMaxRows { rows: u32 },
}

/// Reads and writes settings, one entry at a time.
pub trait SettingsApi {
    fn read_settings(&self) -> Vec<Setting>;
    /// Validate and store one entry. An invalid value is rejected and changes
    /// nothing, including the other entries.
    fn write_setting(&mut self, setting: Setting) -> Result<(), String>;
}

/// Runs SQL against the workspace. The memory implementation answers a few
/// fixed statements; unknown SQL is an error that changes no state.
pub trait QueryApi {
    fn execute(&mut self, sql: &str) -> Result<TablePage, String>;
}

/// The fixed error an unwired desktop adapter answers with. The UI displays
/// this sentence verbatim next to the data it falls back to.
pub const DESKTOP_NOT_IMPLEMENTED: &str = "not implemented in the desktop adapter yet";

/// The desktop host's seam into the shared UI. The desktop process wires it
/// one use case at a time — H3 fills in `list_connections`, H4
/// `read_table_page`, H5 `commit_cells` — and until a method is wired it
/// answers with the fixed error, which the UI displays while the memory port
/// answers instead.
pub trait DesktopAdapter {
    /// The connection list from the desktop use case.
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String>;
    /// One page of a table from the desktop use case.
    fn read_table_page(
        &mut self,
        table: &str,
        page: u32,
        page_size: u32,
        sort: Option<&ColumnSort>,
        filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String>;
    /// Apply one batch of cell edits through the desktop use case and report
    /// how many landed. One edit is (row, column name, new value).
    fn commit_cells(&mut self, table: &str, edits: &[(u32, String, String)]) -> Result<u32, String>;
}

/// The stand-in adapter while no desktop use case is wired: every method
/// answers the same fixed error.
pub struct UnimplementedAdapter;

impl DesktopAdapter for UnimplementedAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn read_table_page(
        &mut self,
        _table: &str,
        _page: u32,
        _page_size: u32,
        _sort: Option<&ColumnSort>,
        _filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn commit_cells(
        &mut self,
        _table: &str,
        _edits: &[(u32, String, String)],
    ) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }
}

/// The one organization a desktop process recognizes (single-tenant).
const DESKTOP_ORGANIZATION: &str = "desktop";

/// The adapter's in-memory connection store, seeded at construction. The
/// desktop host swaps in its real store later; the use case above it is
/// unchanged. Out-of-scope access answers empty, the way the boundary
/// defines it.
struct MemoryConnectionRepository {
    connections: Vec<ConnectionConfig>,
}

#[async_trait::async_trait]
impl datazen::connection::domain::ConnectionRepository for MemoryConnectionRepository {
    async fn list(
        &self,
        scope: &datazen::connection::domain::ConnectionScope,
    ) -> Result<Vec<ConnectionConfig>, datazen::connection::domain::ConnectionRepositoryError>
    {
        if scope.organization_id() != DESKTOP_ORGANIZATION {
            return Ok(Vec::new());
        }
        Ok(self.connections.clone())
    }

    async fn get(
        &self,
        scope: &datazen::connection::domain::ConnectionScope,
        connection_id: &str,
    ) -> Result<Option<ConnectionConfig>, datazen::connection::domain::ConnectionRepositoryError>
    {
        if scope.organization_id() != DESKTOP_ORGANIZATION {
            return Ok(None);
        }
        Ok(self
            .connections
            .iter()
            .find(|config| config.id == connection_id)
            .cloned())
    }

    async fn save(
        &self,
        _scope: &datazen::connection::domain::ConnectionScope,
        _config: ConnectionConfig,
    ) -> Result<(), datazen::connection::domain::ConnectionRepositoryError> {
        Err(datazen::connection::domain::ConnectionRepositoryError::BackendUnavailable(
            "the adapter's demo store is read-only".into(),
        ))
    }

    async fn delete(
        &self,
        _scope: &datazen::connection::domain::ConnectionScope,
        _connection_id: &str,
    ) -> Result<(), datazen::connection::domain::ConnectionRepositoryError> {
        Err(datazen::connection::domain::ConnectionRepositoryError::BackendUnavailable(
            "the adapter's demo store is read-only".into(),
        ))
    }

    async fn reorder(
        &self,
        _scope: &datazen::connection::domain::ConnectionScope,
        _ordered_ids: Vec<String>,
    ) -> Result<(), datazen::connection::domain::ConnectionRepositoryError> {
        Err(datazen::connection::domain::ConnectionRepositoryError::BackendUnavailable(
            "the adapter's demo store is read-only".into(),
        ))
    }

    async fn touch_last_connected_at(
        &self,
        _scope: &datazen::connection::domain::ConnectionScope,
        _connection_id: &str,
    ) -> Result<(), datazen::connection::domain::ConnectionRepositoryError> {
        Ok(())
    }

    async fn groups(
        &self,
        _scope: &datazen::connection::domain::ConnectionScope,
    ) -> Result<Vec<String>, datazen::connection::domain::ConnectionRepositoryError> {
        Ok(Vec::new())
    }

    async fn save_groups(
        &self,
        _scope: &datazen::connection::domain::ConnectionScope,
        _groups: Vec<String>,
    ) -> Result<(), datazen::connection::domain::ConnectionRepositoryError> {
        Err(datazen::connection::domain::ConnectionRepositoryError::BackendUnavailable(
            "the adapter's demo store is read-only".into(),
        ))
    }
}

/// One connection as the form's IPC payload, from the UI vocabulary. The
/// adapter's demo seed carries only what the list shows; the desktop host
/// seeds real configs instead.
fn config_from_profile(profile: &ConnectionProfile) -> ConnectionConfig {
    ConnectionConfig {
        id: profile.id.clone(),
        name: profile.name.clone(),
        database_type: profile.engine.clone(),
        host: None,
        port: None,
        database: None,
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::default(),
        connection_timeout: 30,
        max_pool_size: 10,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

/// H3: the adapter with `list_connections` wired to the existing use case —
/// `ConnectionService::list_redacted` over a connection repository. H4 wired
/// `read_table_page` and H5 `commit_cells` to the existing in-memory use
/// cases (`MemoryWorkspace::open_table`, staging plus `commit_staged`).
pub struct UseCaseAdapter {
    service: datazen::connection::application::ConnectionService,
    /// The existing in-memory world the read use case runs over. The desktop
    /// host swaps in the live-driver use case later; the seam stays.
    workspace: MemoryWorkspace,
}

/// The existing in-memory world with its session open: the read use case
/// needs one, and the demo process has no connect call of its own.
fn demo_workspace() -> MemoryWorkspace {
    let mut workspace = MemoryWorkspace::new();
    workspace.connect("local").expect("the demo connection exists");
    workspace
}

impl UseCaseAdapter {
    /// The real seam: the desktop host provides its connection store.
    pub fn from_repository(
        repository: std::sync::Arc<dyn datazen::connection::domain::ConnectionRepository>,
    ) -> Self {
        Self {
            service: datazen::connection::application::ConnectionService::new(repository),
            workspace: demo_workspace(),
        }
    }

    /// The in-memory store the shared UI's demo process starts from, seeded
    /// with the memory port's demo connections.
    pub fn with_demo_connections() -> Self {
        Self::new(vec![
            ConnectionProfile {
                id: "local".into(),
                name: "Local Postgres".into(),
                engine: "postgres".into(),
            },
            ConnectionProfile {
                id: "replica".into(),
                name: "Read Replica".into(),
                engine: "postgres".into(),
            },
        ])
    }

    /// An in-memory store seeded from the UI vocabulary (tests, demo seeds).
    pub fn new(connections: Vec<ConnectionProfile>) -> Self {
        let configs = connections.iter().map(config_from_profile).collect();
        Self::from_repository(std::sync::Arc::new(MemoryConnectionRepository {
            connections: configs,
        }))
    }
}

impl DesktopAdapter for UseCaseAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        let scope = datazen::connection::domain::ConnectionScope::new(DESKTOP_ORGANIZATION);
        let configs =
            futures_executor::block_on(self.service.list_redacted(&scope))
                .map_err(|error| error.to_string())?;
        Ok(configs
            .iter()
            .map(|config| ConnectionProfile {
                id: config.id.clone(),
                name: config.name.clone(),
                engine: config.database_type.clone(),
            })
            .collect())
    }

    fn read_table_page(
        &mut self,
        table: &str,
        page: u32,
        page_size: u32,
        sort: Option<&ColumnSort>,
        filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        // H4: the existing read use case — the memory workspace's paged,
        // sorted, filtered table read.
        self.workspace
            .open_table(table, page, page_size, sort, filter)
    }

    fn commit_cells(&mut self, table: &str, edits: &[(u32, String, String)]) -> Result<u32, String> {
        // H5: the existing commit use case — the batch is staged on the open
        // page the rows refer to (the UI numbers them like the page it last
        // read, which is the page this workspace has open), then applied as
        // one commit. A failure leaves the staged batch in place.
        let _ = table;
        for (row, column, value) in edits {
            self.workspace.stage_cell(*row, column, value.clone())?;
        }
        self.workspace.commit_staged()
    }
}

pub trait WorkspaceApi {
    fn connections(&self) -> &[ConnectionProfile];
    fn session(&self) -> Option<&Session>;
    fn catalog(&self) -> Catalog;
    /// The table's DDL as one string the page shows verbatim. An empty string
    /// means the port cannot produce DDL; an unknown table is an error.
    fn table_ddl(&self, table: &str) -> Result<String, String>;
    /// The table's columns: one entry per column, in the table's order.
    fn table_columns(&self, table: &str) -> Result<Vec<ColumnSpec>, String>;
    /// The table's indexes; a compound index keeps its column order.
    fn table_indexes(&self, table: &str) -> Result<Vec<IndexSpec>, String>;
    /// The table's foreign keys; a compound key is not split.
    fn table_foreign_keys(&self, table: &str) -> Result<Vec<ForeignKeySpec>, String>;
    /// Save a connection from the form: name, engine, address. It goes into
    /// the port only, never the system keychain. A missing address is refused.
    fn save_connection(&mut self, name: &str, engine: &str, address: &str) -> Result<(), String>;
    /// The saved SQL snippets.
    fn snippets(&self) -> Vec<Snippet>;
    /// Create a snippet; an existing name is refused.
    fn save_snippet(&mut self, name: &str, sql: &str) -> Result<(), String>;
    /// Change a snippet's SQL; an unknown name is refused.
    fn update_snippet(&mut self, name: &str, sql: &str) -> Result<(), String>;
    /// Delete a snippet; an unknown name is refused.
    fn delete_snippet(&mut self, name: &str) -> Result<(), String>;
    /// Start an export job for a format and a range. A missing range gets no
    /// job at all.
    fn start_export(&mut self, format: &str, range: &str) -> Result<u64, String>;
    /// One job's progress snapshot, if the job exists. The memory port
    /// advances the job one step per read.
    fn job_progress(&mut self, job: u64) -> Option<ProgressSnapshot>;
    /// Start a backup of the connected database.
    fn start_backup(&mut self) -> Result<u64, String>;
    /// D1: the two endpoints of a sync. Unknown endpoints, or the same endpoint
    /// twice, are refused.
    fn sync_endpoints(&mut self, source: &str, target: &str) -> Result<(), String>;
    /// The tables a connection can see, without opening a session.
    fn connection_tables(&self, connection: &str) -> Vec<String>;
    /// D4: a read-only preview page of one table, as the sync would copy it.
    fn sync_preview(&mut self, source: &str, table: &str) -> Result<TablePage, String>;
    /// D5: run the sync as a job. No tables selected, no job.
    fn run_sync(&mut self, source: &str, target: &str, tables: &[String]) -> Result<u64, String>;
    /// D6: the source's objects, marked when the target has the same name.
    fn compare_objects(&self, source: &str, target: &str) -> Result<Vec<CompareObject>, String>;
    /// D7: the plan as text — one statement per object the target lacks.
    /// An empty string means no differences.
    fn compare_plan(&self, source: &str, target: &str) -> Result<String, String>;
    /// D8: run the plan as a job. An empty plan has nothing to apply; the
    /// replica cannot create analytics tables, so that plan's job fails and
    /// the difference stays.
    fn run_compare_plan(&mut self, plan: &str) -> Result<u64, String>;
    /// D10: run the transfer as a job. No pairs, no job.
    fn run_transfer(&mut self, pairs: &[(String, String)]) -> Result<u64, String>;
    /// W1: the dashboard's widgets.
    fn dashboard_widgets(&self) -> Vec<DashboardWidget>;
    /// W2: save a widget (a title and its query text); an empty title is
    /// refused. Returns the widget's id.
    fn save_dashboard_widget(&mut self, title: &str, query: &str) -> Result<u64, String>;
    /// W2: the widget's chart data: labeled integers from the port.
    fn widget_bars(&self, id: u64) -> Result<Vec<(String, u32)>, String>;
    /// W4: the workflows, sorted by name.
    fn workflows(&self) -> Vec<Workflow>;
    /// W5: save a workflow by name (an edit replaces the stored steps).
    fn save_workflow(&mut self, name: &str, steps: &[String]) -> Result<(), String>;
    /// W7: run a workflow; every step reports name, success and one summary
    /// line. The memory port fails steps that apply anything.
    fn run_workflow(&mut self, name: &str) -> Result<Vec<WorkflowStepResult>, String>;
    /// Stop a running job. A stopped job reports "stopped" from then on.
    fn stop_job(&mut self, job: u64) -> Result<(), String>;
    /// Apply a DDL script to the table. The port records what it applied; a
    /// failure changes nothing.
    fn apply_ddl(&mut self, table: &str, sql: &str) -> Result<(), String>;
    /// The DDL scripts this port has applied, in order.
    fn applied_ddl(&self) -> Vec<String>;
    fn connect(&mut self, id: &str) -> Result<(), String>;
    fn disconnect(&mut self);
    fn open_table(
        &mut self,
        table: &str,
        page: u32,
        page_size: u32,
        sort: Option<&ColumnSort>,
        filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String>;
    fn stage_cell(&mut self, row: u32, column: &str, value: String) -> Result<(), String>;
    /// Open a read-only query result page. While it is open, staging is rejected.
    fn open_result(&mut self, columns: Vec<String>, rows: Vec<Vec<String>>) -> TablePage;
    /// Apply every staged edit to the stored rows and report how many landed.
    fn commit_staged(&mut self) -> Result<u32, String>;
    /// Drop every staged edit; the stored rows keep their values.
    fn rollback_staged(&mut self);
}

#[derive(Clone, Debug)]
struct StoredTable {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    /// Staged cell edits keyed by (stored row, column index); open_table overlays
    /// them, commit_staged applies them, rollback_staged drops them.
    staged: HashMap<(u32, usize), String>,
    /// Why the table cannot be edited; None means writable.
    read_only: Option<String>,
    /// The schema the table lives in.
    schema: String,
    /// Per-column SQL types, parallel to `columns`; missing entries read "text".
    column_types: Vec<String>,
}

/// The table the workspace last opened, with the page window it asked for.
#[derive(Clone, Debug)]
struct OpenTable {
    name: String,
    page: u32,
    page_size: u32,
}

#[derive(Clone, Debug)]
pub struct MemoryWorkspace {
    connections: Vec<ConnectionProfile>,
    session: Option<Session>,
    tables: Vec<(String, StoredTable)>,
    open: Option<OpenTable>,
    /// A query result is showing: the port rejects staging until a table reopens.
    result_open: bool,
    /// DDL scripts the port has applied, in order.
    applied_ddl: Vec<String>,
    /// The settings store, one entry per kind.
    settings: Vec<Setting>,
    /// The saved SQL snippets.
    snippets: Vec<Snippet>,
    /// Export jobs the memory port runs.
    jobs: Vec<MemoryJob>,
    /// The dashboard's widgets.
    widgets: Vec<DashboardWidget>,
    /// The workflows, stored unordered; reads sort by name.
    workflows: Vec<Workflow>,
}

impl Default for MemoryWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryWorkspace {
    pub fn new() -> Self {
        let columns = vec!["id", "name", "city", "status", "note", "orders", "email", "country"];
        let mut rows = Vec::new();
        for index in 0..40 {
            rows.push(vec![
                index.to_string(),
                format!("customer-{index}"),
                "Hangzhou".to_owned(),
                if index % 2 == 0 { "active".to_owned() } else { "paused".to_owned() },
                "seed".to_owned(),
                (index * 3).to_string(),
                format!("c{index}@example.com"),
                "CN".to_owned(),
            ]);
        }
        // A wide demo table for the analytics schema: 30 columns x 60 rows.
        let wide_columns: Vec<String> = (0..30).map(|i| format!("c{i}")).collect();
        let wide_rows: Vec<Vec<String>> = (0..60)
            .map(|row| wide_columns.iter().map(|col| format!("{col}-{row}")).collect())
            .collect();
        let tables = vec![
            (
                "customers".to_owned(),
                StoredTable {
                    columns: columns.into_iter().map(str::to_owned).collect(),
                    rows,
                    staged: HashMap::new(),
                    read_only: None,
                    schema: "public".into(),
                    column_types: Vec::new(),
                },
            ),
            (
                "orders".to_owned(),
                StoredTable {
                    columns: vec!["id".into(), "name".into()],
                    rows: vec![vec!["1".into(), "order-0".into()]],
                    staged: HashMap::new(),
                    read_only: Some("no primary key".into()),
                    schema: "public".into(),
                    column_types: Vec::new(),
                },
            ),
            (
                "catalog".to_owned(),
                StoredTable {
                    columns: wide_columns,
                    rows: wide_rows,
                    staged: HashMap::new(),
                    read_only: None,
                    schema: "analytics".into(),
                    column_types: Vec::new(),
                },
            ),
        ];
        Self {
            connections: vec![
                ConnectionProfile {
                    id: "local".into(),
                    name: "Local Postgres".into(),
                    engine: "postgres".into(),
                },
                ConnectionProfile {
                    id: "replica".into(),
                    name: "Read Replica".into(),
                    engine: "postgres".into(),
                },
            ],
            session: None,
            tables,
            open: None,
            result_open: false,
            applied_ddl: Vec::new(),
            settings: vec![
                Setting::Theme { dark: true },
                Setting::TablePageSize { rows: 25 },
                Setting::QueryMaxRows { rows: 100 },
            ],
            snippets: Vec::new(),
            jobs: Vec::new(),
            widgets: Vec::new(),
            workflows: vec![
                Workflow {
                    name: "weekly-report".into(),
                    steps: vec!["run query".into(), "export csv".into()],
                },
                Workflow {
                    name: "audit".into(),
                    steps: vec!["compare structures".into(), "apply plan".into()],
                },
                Workflow {
                    name: "nightly-backup".into(),
                    steps: vec!["backup".into(), "verify".into()],
                },
            ],
        }
    }
}

impl SettingsApi for MemoryWorkspace {
    fn read_settings(&self) -> Vec<Setting> {
        self.settings.clone()
    }

    fn write_setting(&mut self, setting: Setting) -> Result<(), String> {
        match &setting {
            Setting::Theme { .. } => {}
            Setting::TablePageSize { rows } => {
                if *rows == 0 || *rows > 500 {
                    return Err(format!("table page size must be 1..=500, got {rows}"));
                }
            }
            Setting::QueryMaxRows { rows } => {
                if *rows == 0 || *rows > 10_000 {
                    return Err(format!("query max rows must be 1..=10000, got {rows}"));
                }
            }
        }
        let kind = setting_kind(&setting);
        self.settings.retain(|existing| setting_kind(existing) != kind);
        self.settings.push(setting);
        Ok(())
    }
}

fn setting_kind(setting: &Setting) -> u8 {
    match setting {
        Setting::Theme { .. } => 0,
        Setting::TablePageSize { .. } => 1,
        Setting::QueryMaxRows { .. } => 2,
    }
}

impl MemoryWorkspace {
    /// A read-only result page. No flags touched: callers decide what shows.
    fn result_page(columns: Vec<String>, rows: Vec<Vec<String>>) -> TablePage {
        let total = rows.len() as u32;
        TablePage {
            table: "result".into(),
            columns,
            rows,
            page: 0,
            page_size: total.max(1),
            total_rows: total,
            sort: None,
            filter: None,
            read_only: Some("query result".into()),
        }
    }
}

impl QueryApi for MemoryWorkspace {
    fn execute(&mut self, sql: &str) -> Result<TablePage, String> {
        if self.session.is_none() {
            return Err("not connected".into());
        }
        let normalized = sql
            .trim()
            .trim_end_matches(';')
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_uppercase();
        match normalized.as_str() {
            "SELECT 1" => Ok(Self::result_page(
                vec!["1".to_owned()],
                vec![vec!["1".to_owned()]],
            )),
            "SELECT * FROM CUSTOMERS" => {
                let Some((_, stored)) = self.tables.iter().find(|(name, _)| name == "customers") else {
                    return Err("unknown table customers".into());
                };
                let max_rows = self
                    .settings
                    .iter()
                    .find_map(|setting| match setting {
                        Setting::QueryMaxRows { rows } => Some(*rows),
                        _ => None,
                    })
                    .unwrap_or(100) as usize;
                let rows = stored.rows.iter().take(max_rows).cloned().collect();
                Ok(Self::result_page(stored.columns.clone(), rows))
            }
            _ => Err(format!("cannot execute: {sql}")),
        }
    }
}

impl WorkspaceApi for MemoryWorkspace {
    fn connections(&self) -> &[ConnectionProfile] {
        &self.connections
    }

    fn catalog(&self) -> Catalog {
        let Some(session) = &self.session else {
            return Catalog { databases: Vec::new() };
        };
        let mut schemas: Vec<(String, Vec<String>)> = Vec::new();
        for (name, stored) in &self.tables {
            if !session.tables.contains(name) {
                continue;
            }
            match schemas.iter_mut().find(|(schema, _)| *schema == stored.schema) {
                Some(existing) => existing.1.push(name.clone()),
                None => schemas.push((stored.schema.clone(), vec![name.clone()])),
            }
        }
        let databases = vec![DatabaseNode {
            name: session.connection_id.clone(),
            schemas: schemas
                .into_iter()
                .map(|(name, tables)| SchemaNode { name, tables })
                .collect(),
        }];
        Catalog { databases }
    }

    fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    fn table_ddl(&self, table: &str) -> Result<String, String> {
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        if stored.1.schema == "analytics" {
            // The memory port cannot introspect the external analytics schema.
            return Ok(String::new());
        }
        // (table_ddl continues below; the analytics branch is shared with columns)
        let lines: Vec<String> =
            stored.1.columns.iter().map(|column| format!("  {column} text")).collect();
        Ok(format!("CREATE TABLE {table} (\n{}\n);", lines.join(",\n")))
    }

    fn table_indexes(&self, table: &str) -> Result<Vec<IndexSpec>, String> {
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        Ok(match table {
            "customers" => vec![
                IndexSpec {
                    name: "customers_pkey".into(),
                    columns: vec!["id".into()],
                    unique: true,
                },
                IndexSpec {
                    name: "customers_name_city_idx".into(),
                    columns: vec!["name".into(), "city".into()],
                    unique: false,
                },
            ],
            "orders" => vec![IndexSpec {
                name: "orders_name_idx".into(),
                columns: vec!["name".into()],
                unique: false,
            }],
            "catalog" => vec![IndexSpec {
                name: "catalog_c0_c1_idx".into(),
                columns: vec!["c0".into(), "c1".into()],
                unique: false,
            }],
            _ => Vec::new(),
        })
    }

    fn table_foreign_keys(&self, table: &str) -> Result<Vec<ForeignKeySpec>, String> {
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        let _ = stored;
        Ok(match table {
            "orders" => vec![ForeignKeySpec {
                name: "orders_customer_fk".into(),
                columns: vec!["id".into(), "name".into()],
                target_table: "customers".into(),
                target_columns: vec!["id".into(), "name".into()],
            }],
            "customers" => vec![ForeignKeySpec {
                name: "customers_orders_fk".into(),
                columns: vec!["orders".into()],
                target_table: "orders".into(),
                target_columns: vec!["id".into()],
            }],
            _ => Vec::new(),
        })
    }

    fn snippets(&self) -> Vec<Snippet> {
        self.snippets.clone()
    }

    fn save_snippet(&mut self, name: &str, sql: &str) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("snippet name is required".into());
        }
        if self.snippets.iter().any(|snippet| snippet.name == name) {
            return Err(format!("a snippet named {name} already exists"));
        }
        self.snippets.push(Snippet { name: name.to_owned(), sql: sql.to_owned() });
        Ok(())
    }

    fn update_snippet(&mut self, name: &str, sql: &str) -> Result<(), String> {
        match self.snippets.iter_mut().find(|snippet| snippet.name == name) {
            Some(snippet) => {
                snippet.sql = sql.to_owned();
                Ok(())
            }
            None => Err(format!("no snippet named {name}")),
        }
    }

    fn delete_snippet(&mut self, name: &str) -> Result<(), String> {
        let before = self.snippets.len();
        self.snippets.retain(|snippet| snippet.name != name);
        if self.snippets.len() == before {
            return Err(format!("no snippet named {name}"));
        }
        Ok(())
    }

    fn start_export(&mut self, format: &str, range: &str) -> Result<u64, String> {
        if range.trim().is_empty() {
            return Err("a range is required before an export can start".into());
        }
        if !matches!(format, "csv" | "json" | "sql" | "broken") {
            return Err(format!("unknown export format {format}"));
        }
        let job = self.jobs.last().map(|job| job.job + 1).unwrap_or(1);
        self.jobs.push(MemoryJob {
            job,
            format: format.to_owned(),
            range: range.to_owned(),
            reads: 0,
            stopped: false,
            log: vec![
                "queued".to_owned(),
                format!("exporting {format} of {range}"),
            ],
            fail_line: None,
        });
        Ok(job)
    }

    fn job_progress(&mut self, job: u64) -> Option<ProgressSnapshot> {
        let entry = self.jobs.iter_mut().find(|running| running.job == job)?;
        let snapshot = |status: &str, percent: u32, entry: &MemoryJob| ProgressSnapshot {
            job,
            status: status.to_owned(),
            percent,
            log: entry.log.clone(),
        };
        if entry.stopped {
            return Some(snapshot("stopped", 100, entry));
        }
        entry.reads += 1;
        match entry.reads {
            1 => Some(snapshot("running", 0, entry)),
            2 => {
                entry.log.push("halfway".into());
                Some(snapshot("running", 50, entry))
            }
            _ => {
                let failed = entry.format == "broken"
                    || entry.range == "no-such-table"
                    || entry.fail_line.is_some();
                if entry.reads == 3 {
                    // The terminal line lands once, on the transition.
                    if entry.format == "broken" {
                        entry.log.push("the port cannot write broken files".into());
                    } else if entry.range == "no-such-table" {
                        entry.log.push("no such table no-such-table".into());
                    } else if let Some(line) = &entry.fail_line {
                        entry.log.push(line.clone());
                    } else {
                        entry.log.push("done".into());
                    }
                }
                Some(snapshot(if failed { "failed" } else { "completed" }, 100, entry))
            }
        }
    }

    fn sync_endpoints(&mut self, source: &str, target: &str) -> Result<(), String> {
        for endpoint in [source, target] {
            if !self.connections.iter().any(|profile| profile.id == endpoint) {
                return Err(format!("unknown connection {endpoint}"));
            }
        }
        if source == target {
            return Err("both ends are the same connection".into());
        }
        Ok(())
    }

    fn sync_preview(&mut self, source: &str, table: &str) -> Result<TablePage, String> {
        if !self.connections.iter().any(|profile| profile.id == source) {
            return Err(format!("unknown connection {source}"));
        }
        if !self.connection_tables(source).iter().any(|name| name == table) {
            return Err(format!("{source} cannot see {table}"));
        }
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        let mut page = Self::result_page(stored.1.columns.clone(), stored.1.rows.clone());
        page.table = format!("sync preview: {table}");
        page.read_only = Some("sync preview".into());
        Ok(page)
    }

    fn run_sync(&mut self, source: &str, target: &str, tables: &[String]) -> Result<u64, String> {
        self.sync_endpoints(source, target)?;
        if tables.is_empty() {
            return Err("no tables selected".into());
        }
        let job = self.jobs.last().map(|job| job.job + 1).unwrap_or(1);
        self.jobs.push(MemoryJob {
            job,
            format: "sync".into(),
            range: format!("{} tables", tables.len()),
            reads: 0,
            stopped: false,
            log: vec![
                "queued".to_owned(),
                format!("syncing {source} into {target}"),
            ],
            fail_line: None,
        });
        Ok(job)
    }

    fn compare_objects(&self, source: &str, target: &str) -> Result<Vec<CompareObject>, String> {
        for endpoint in [source, target] {
            if !self.connections.iter().any(|profile| profile.id == endpoint) {
                return Err(format!("unknown connection {endpoint}"));
            }
        }
        let target_tables = self.connection_tables(target);
        Ok(self
            .connection_tables(source)
            .into_iter()
            .map(|name| CompareObject {
                present_on_target: target_tables.contains(&name),
                name,
            })
            .collect())
    }

    fn run_compare_plan(&mut self, plan: &str) -> Result<u64, String> {
        if plan.trim().is_empty() {
            return Err("nothing to apply".into());
        }
        let job = self.jobs.last().map(|job| job.job + 1).unwrap_or(1);
        let fails = plan.contains("CREATE TABLE catalog");
        self.jobs.push(MemoryJob {
            job,
            format: "compare".into(),
            range: "the compare plan".into(),
            reads: 0,
            stopped: false,
            log: vec!["queued".to_owned(), "applying the plan".into()],
            fail_line: fails.then(|| "the replica cannot create analytics tables".to_owned()),
        });
        Ok(job)
    }

    fn dashboard_widgets(&self) -> Vec<DashboardWidget> {
        self.widgets.clone()
    }

    fn save_dashboard_widget(&mut self, title: &str, query: &str) -> Result<u64, String> {
        if title.trim().is_empty() {
            return Err("a widget needs a title".into());
        }
        let id = self.widgets.last().map(|widget| widget.id + 1).unwrap_or(1);
        self.widgets.push(DashboardWidget {
            id,
            title: title.trim().to_owned(),
            kind: "bar".into(),
        });
        let _ = query;
        Ok(id)
    }

    fn workflows(&self) -> Vec<Workflow> {
        let mut workflows = self.workflows.clone();
        workflows.sort_by(|a, b| a.name.cmp(&b.name));
        workflows
    }

    fn run_workflow(&mut self, name: &str) -> Result<Vec<WorkflowStepResult>, String> {
        let flow = self
            .workflows
            .iter()
            .find(|flow| flow.name == name)
            .ok_or_else(|| format!("no workflow {name}"))?;
        Ok(flow
            .steps
            .iter()
            .map(|step| {
                if step.contains("apply") {
                    WorkflowStepResult {
                        step: step.clone(),
                        ok: false,
                        summary: "the replica refused the plan".into(),
                    }
                } else {
                    WorkflowStepResult {
                        step: step.clone(),
                        ok: true,
                        summary: "ok: 12 rows".into(),
                    }
                }
            })
            .collect())
    }

    fn save_workflow(&mut self, name: &str, steps: &[String]) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("a workflow needs a name".into());
        }
        let name = name.trim();
        match self.workflows.iter_mut().find(|flow| flow.name == name) {
            Some(existing) => existing.steps = steps.to_vec(),
            None => self.workflows.push(Workflow {
                name: name.to_owned(),
                steps: steps.to_vec(),
            }),
        }
        Ok(())
    }

    fn widget_bars(&self, id: u64) -> Result<Vec<(String, u32)>, String> {
        if !self.widgets.iter().any(|widget| widget.id == id) {
            return Err(format!("no widget {id}"));
        }
        // The memory port's demo series: a zero and a maximum, both visible.
        Ok(vec![
            ("mon".into(), 0),
            ("tue".into(), 3),
            ("wed".into(), 7),
            ("thu".into(), 12),
        ])
    }

    fn run_transfer(&mut self, pairs: &[(String, String)]) -> Result<u64, String> {
        if pairs.is_empty() {
            return Err("no column pairs to transfer".into());
        }
        let job = self.jobs.last().map(|job| job.job + 1).unwrap_or(1);
        self.jobs.push(MemoryJob {
            job,
            format: "transfer".into(),
            range: format!("{} pairs", pairs.len()),
            reads: 0,
            stopped: false,
            log: vec![
                "queued".to_owned(),
                format!("transferring {} column pairs", pairs.len()),
            ],
            fail_line: None,
        });
        Ok(job)
    }

    fn compare_plan(&self, source: &str, target: &str) -> Result<String, String> {
        let objects = self.compare_objects(source, target)?;
        let lines: Vec<String> = objects
            .iter()
            .filter(|object| !object.present_on_target)
            .map(|object| format!("CREATE TABLE {} (...); -- missing on {}", object.name, target))
            .collect();
        Ok(lines.join("\n"))
    }

    fn connection_tables(&self, connection: &str) -> Vec<String> {
        self.tables
            .iter()
            .filter(|(_, stored)| !(connection == "replica" && stored.schema == "analytics"))
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn start_backup(&mut self) -> Result<u64, String> {
        if self.session.is_none() {
            return Err("not connected".into());
        }
        let job = self.jobs.last().map(|job| job.job + 1).unwrap_or(1);
        self.jobs.push(MemoryJob {
            job,
            format: "backup".into(),
            range: "the database".into(),
            reads: 0,
            stopped: false,
            log: vec![
                "queued".to_owned(),
                "backing up the database".into(),
            ],
            fail_line: None,
        });
        Ok(job)
    }

    fn stop_job(&mut self, job: u64) -> Result<(), String> {
        let entry = self
            .jobs
            .iter_mut()
            .find(|running| running.job == job)
            .ok_or_else(|| format!("no job {job}"))?;
        if entry.stopped {
            return Err("the job is already stopped".into());
        }
        if entry.reads >= 3 {
            return Err("the job already finished".into());
        }
        entry.stopped = true;
        entry.log.push("stopped by the user".into());
        Ok(())
    }

    fn save_connection(&mut self, name: &str, engine: &str, address: &str) -> Result<(), String> {
        if address.trim().is_empty() {
            return Err("address is required".into());
        }
        let name = if name.trim().is_empty() { "Untitled" } else { name.trim() };
        let engine = if engine.trim().is_empty() { "postgres" } else { engine.trim() };
        let slug = |name: &str| {
            let mut id: String = name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
                .collect();
            while id.contains("--") {
                id = id.replace("--", "-");
            }
            id.trim_matches('-').to_owned()
        };
        let mut id = slug(name);
        if id.is_empty() {
            id = format!("connection-{}", self.connections.len() + 1);
        }
        let mut candidate = id.clone();
        let mut counter = 2;
        while self.connections.iter().any(|profile| profile.id == candidate) {
            candidate = format!("{id}-{counter}");
            counter += 1;
        }
        self.connections.push(ConnectionProfile {
            id: candidate,
            name: name.to_owned(),
            engine: engine.to_owned(),
        });
        Ok(())
    }

    fn apply_ddl(&mut self, table: &str, sql: &str) -> Result<(), String> {
        if self.session.is_none() {
            return Err("not connected".into());
        }
        let stored = self
            .tables
            .iter_mut()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        if let Some(reason) = &stored.1.read_only {
            return Err(format!("{table} is read-only: {reason}"));
        }
        enum Change {
            Add(String, String),
            Type(usize, String),
        }
        let add_prefix = format!("ALTER TABLE {table} ADD COLUMN ");
        let type_prefix = format!("ALTER TABLE {table} ALTER COLUMN ");
        let mut changes = Vec::new();
        for line in sql.lines() {
            let line = line.trim().trim_end_matches(';');
            if let Some(rest) = line.strip_prefix(&add_prefix) {
                let Some((name, sql_type)) = rest.split_once(' ') else {
                    return Err(format!("cannot apply: {line}"));
                };
                changes.push(Change::Add(name.to_owned(), sql_type.to_owned()));
            } else if let Some(rest) = line.strip_prefix(&type_prefix) {
                let Some((column, tail)) = rest.split_once(' ') else {
                    return Err(format!("cannot apply: {line}"));
                };
                let Some(sql_type) = tail.strip_prefix("TYPE ") else {
                    return Err(format!("cannot apply: {line}"));
                };
                let index = stored
                    .1
                    .columns
                    .iter()
                    .position(|name| name == column)
                    .ok_or_else(|| format!("unknown column {column}"))?;
                changes.push(Change::Type(index, sql_type.to_owned()));
            } else {
                return Err(format!("cannot apply: {line}"));
            }
        }
        for change in changes {
            match change {
                Change::Add(name, sql_type) => {
                    while stored.1.column_types.len() < stored.1.columns.len() {
                        stored.1.column_types.push("text".into());
                    }
                    stored.1.columns.push(name);
                    stored.1.column_types.push(sql_type);
                    for row in &mut stored.1.rows {
                        row.push("NULL".into());
                    }
                }
                Change::Type(index, sql_type) => {
                    while stored.1.column_types.len() < stored.1.columns.len() {
                        stored.1.column_types.push("text".into());
                    }
                    stored.1.column_types[index] = sql_type;
                }
            }
        }
        self.applied_ddl.push(sql.to_owned());
        Ok(())
    }

    fn applied_ddl(&self) -> Vec<String> {
        self.applied_ddl.clone()
    }

    fn table_columns(&self, table: &str) -> Result<Vec<ColumnSpec>, String> {
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        Ok(stored
            .1
            .columns
            .iter()
            .enumerate()
            .map(|(index, name)| ColumnSpec {
                name: name.clone(),
                sql_type: stored
                    .1
                    .column_types
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| "text".into()),
                // The demo port keeps the first column a key, the rest nullable.
                nullable: index != 0,
            })
            .collect())
    }

    fn connect(&mut self, id: &str) -> Result<(), String> {
        if !self.connections.iter().any(|profile| profile.id == id) {
            return Err(format!("unknown connection {id}"));
        }
        let tables = self
            .tables
            .iter()
            .filter(|(_, stored)| !(id == "replica" && stored.schema == "analytics"))
            .map(|(name, _)| name.clone())
            .collect();
        self.session = Some(Session {
            connection_id: id.to_owned(),
            tables,
        });
        Ok(())
    }

    fn disconnect(&mut self) {
        self.session = None;
        self.open = None;
        self.result_open = false;
        for (_, stored) in self.tables.iter_mut() {
            stored.staged.clear();
        }
    }

    fn open_table(
        &mut self,
        table: &str,
        page: u32,
        page_size: u32,
        sort: Option<&ColumnSort>,
        filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        if self.session.is_none() {
            return Err("not connected".into());
        }
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        let total = stored.1.rows.len() as u32;
        let mut order: Vec<u32> = (0..total).collect();
        if let Some(filter) = filter {
            let index = stored
                .1
                .columns
                .iter()
                .position(|name| name == &filter.column)
                .ok_or_else(|| format!("unknown column {}", filter.column))?;
            let needle = filter.contains.as_str();
            order.retain(|row| stored.1.rows[*row as usize][index].contains(needle));
        }
        if let Some(sort) = sort {
            let index = stored
                .1
                .columns
                .iter()
                .position(|name| name == &sort.column)
                .ok_or_else(|| format!("unknown column {}", sort.column))?;
            let descending = sort.descending;
            order.sort_by(|a, b| {
                stored.1.rows[*a as usize][index].cmp(&stored.1.rows[*b as usize][index])
            });
            if descending {
                order.reverse();
            }
        }
        let total = order.len() as u32;
        let page_size = page_size.max(1);
        let page = page.min(total / page_size);
        self.result_open = false;
        let window =
            (page * page_size) as usize..(((page + 1) * page_size) as usize).min(total as usize);
        let mut rows: Vec<Vec<String>> = order[window.clone()]
            .iter()
            .map(|index| stored.1.rows[*index as usize].clone())
            .collect();
        for ((row, col), value) in &stored.1.staged {
            if let Some(page_index) = order[window.clone()].iter().position(|index| index == row) {
                if let Some(record) = rows.get_mut(page_index) {
                    if let Some(cell) = record.get_mut(*col) {
                        *cell = value.clone();
                    }
                }
            }
        }
        self.open = Some(OpenTable { name: table.to_owned(), page, page_size });
        Ok(TablePage {
            table: table.to_owned(),
            columns: stored.1.columns.clone(),
            rows,
            page,
            page_size,
            total_rows: total,
            sort: sort.cloned(),
            filter: filter.cloned(),
            read_only: stored.1.read_only.clone(),
        })
    }

    fn open_result(&mut self, columns: Vec<String>, rows: Vec<Vec<String>>) -> TablePage {
        self.result_open = true;
        Self::result_page(columns, rows)
    }

    fn stage_cell(&mut self, row: u32, column: &str, value: String) -> Result<(), String> {
        if self.result_open {
            return Err("a query result is open: results are read-only".into());
        }
        let open = self.open.as_ref().ok_or("no open table")?;
        let stored_row = open.page * open.page_size + row;
        let stored = self
            .tables
            .iter_mut()
            .find(|(name, _)| name == &open.name)
            .ok_or_else(|| format!("unknown table {}", open.name))?;
        let index = stored
            .1
            .columns
            .iter()
            .position(|name| name == column)
            .ok_or_else(|| format!("unknown column {column}"))?;
        if let Some(reason) = &stored.1.read_only {
            return Err(format!("{} is read-only: {reason}", open.name));
        }
        if stored_row as usize >= stored.1.rows.len() {
            return Err(format!("row {row} is outside the page"));
        }
        stored.1.staged.insert((stored_row, index), value);
        Ok(())
    }

    fn commit_staged(&mut self) -> Result<u32, String> {
        if let Some(open) = &self.open {
            if let Some((_, stored)) = self.tables.iter().find(|(name, _)| name == &open.name) {
                if let Some(reason) = &stored.read_only {
                    return Err(format!("{} is read-only: {reason}", open.name));
                }
            }
        }
        let mut applied = 0u32;
        for (_, stored) in self.tables.iter_mut() {
            for ((row, col), value) in stored.staged.drain() {
                if let Some(record) = stored.rows.get_mut(row as usize) {
                    if let Some(cell) = record.get_mut(col) {
                        *cell = value;
                        applied += 1;
                    }
                }
            }
        }
        Ok(applied)
    }

    fn rollback_staged(&mut self) {
        for (_, stored) in self.tables.iter_mut() {
            stored.staged.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_open_and_stage_round_trip() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.connect("missing").is_err());
        workspace.connect("local").unwrap();
        let page = workspace.open_table("customers", 0, 25, None, None).unwrap();
        assert_eq!(page.columns[1], "name");
        assert_eq!(page.rows[0][1], "customer-0");
        workspace.stage_cell(0, "name", "alice".into()).unwrap();
        let again = workspace.open_table("customers", 0, 25, None, None).unwrap();
        assert_eq!(again.rows[0][1], "alice");
    }

    #[test]
    fn the_second_page_is_not_the_first_page() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let first = workspace.open_table("customers", 0, 25, None, None).unwrap();
        let second = workspace.open_table("customers", 1, 25, None, None).unwrap();
        assert_eq!(first.rows.len(), 25);
        assert_eq!(second.rows.len(), 15);
        assert_eq!(first.rows[0][0], "0");
        assert_eq!(second.rows[0][0], "25");
        assert_eq!(first.page, 0);
        assert_eq!(second.page, 1);
        assert_eq!(second.total_rows, 40);
        assert_ne!(first.rows[0], second.rows[0]);
    }

    #[test]
    fn an_out_of_range_page_clamps_to_the_last_page() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let page = workspace.open_table("customers", 99, 25, None, None).unwrap();
        assert_eq!(page.page, 1);
        assert_eq!(page.rows.len(), 15);
    }

    #[test]
    fn sorting_orders_the_page_over_the_whole_table() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let ascending = ColumnSort { column: "name".into(), descending: false };
        let page = workspace.open_table("customers", 0, 25, Some(&ascending), None).unwrap();
        assert_eq!(page.rows[0][1], "customer-0");
        assert_eq!(page.rows[2][1], "customer-10", "lexicographic ascending order");
        assert_eq!(page.sort, Some(ascending.clone()));

        let descending = ColumnSort { column: "name".into(), descending: true };
        let page = workspace.open_table("customers", 0, 25, Some(&descending), None).unwrap();
        assert_eq!(page.rows[0][1], "customer-9", "descending starts at the lexicographic max");
        assert_eq!(page.rows[6][1], "customer-39");
        assert_eq!(page.rows.len(), 25);
    }

    #[test]
    fn sorting_by_an_unknown_column_is_an_error() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let sort = ColumnSort { column: "missing".into(), descending: false };
        assert!(workspace.open_table("customers", 0, 25, Some(&sort), None).is_err());
    }

    #[test]
    fn a_filter_narrows_the_page_and_the_total() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let filter = ColumnFilter { column: "name".into(), contains: "customer-3".into() };
        let page = workspace.open_table("customers", 0, 25, None, Some(&filter)).unwrap();
        assert_eq!(page.rows.len(), 11, "customer-3 itself and customer-30..39");
        assert_eq!(page.total_rows, 11);
        assert!(page.rows.iter().all(|row| row[1].contains("customer-3")));
        assert_eq!(page.filter, Some(filter));
    }

    #[test]
    fn a_filter_with_sort_pages_over_the_matching_rows() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let filter = ColumnFilter { column: "name".into(), contains: "customer-3".into() };
        let sort = ColumnSort { column: "name".into(), descending: true };
        let page = workspace.open_table("customers", 0, 25, Some(&sort), Some(&filter)).unwrap();
        assert_eq!(page.rows.len(), 11);
        assert_eq!(page.rows[0][1], "customer-39", "descending over the filtered set");
        assert_eq!(page.rows[10][1], "customer-3");
    }

    #[test]
    fn filtering_an_unknown_column_is_an_error() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let filter = ColumnFilter { column: "missing".into(), contains: "x".into() };
        assert!(workspace.open_table("customers", 0, 25, None, Some(&filter)).is_err());
    }

    #[test]
    fn rollback_restores_the_original_value() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace.open_table("customers", 0, 25, None, None).unwrap();
        workspace.stage_cell(0, "name", "staged".into()).unwrap();
        let page = workspace.open_table("customers", 0, 25, None, None).unwrap();
        assert_eq!(page.rows[0][1], "staged", "staged values show on the page");
        workspace.rollback_staged();
        let page = workspace.open_table("customers", 0, 25, None, None).unwrap();
        assert_eq!(page.rows[0][1], "customer-0");
    }

    #[test]
    fn commit_keeps_the_new_value_after_reopening() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace.open_table("customers", 0, 25, None, None).unwrap();
        workspace.stage_cell(0, "name", "kept".into()).unwrap();
        assert_eq!(workspace.commit_staged().unwrap(), 1);
        let page = workspace.open_table("customers", 0, 25, None, None).unwrap();
        assert_eq!(page.rows[0][1], "kept");
        workspace.rollback_staged();
        let page = workspace.open_table("customers", 0, 25, None, None).unwrap();
        assert_eq!(page.rows[0][1], "kept", "rollback cannot undo a commit");
    }

    #[test]
    fn staged_values_follow_their_row_through_sorting() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace.open_table("customers", 0, 25, None, None).unwrap();
        workspace.stage_cell(0, "name", "aaa-first".into()).unwrap();
        let sort = ColumnSort { column: "name".into(), descending: false };
        let page = workspace.open_table("customers", 0, 25, Some(&sort), None).unwrap();
        assert_eq!(page.rows[0][1], "aaa-first", "the staged name sorts with its row");
    }

    #[test]
    fn open_result_is_read_only_and_blocks_staging() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace.open_table("customers", 0, 25, None, None).unwrap();
        let page = workspace.open_result(
            vec!["a".into(), "b".into()],
            vec![vec!["1".into(), "2".into()], vec!["3".into(), "4".into()]],
        );
        assert_eq!(page.table, "result");
        assert_eq!(page.total_rows, 2);
        assert_eq!(page.read_only.as_deref(), Some("query result"));
        assert!(
            workspace.stage_cell(0, "name", "x".into()).is_err(),
            "staging while a result is open is rejected"
        );

        workspace.open_table("customers", 0, 25, None, None).unwrap();
        workspace.stage_cell(0, "name", "x".into()).unwrap();
        assert_eq!(
            workspace.open_table("customers", 0, 25, None, None).unwrap().rows[0][1],
            "x",
            "reopening a table releases the result lock"
        );
    }

    #[test]
    fn execute_answers_fixed_sql_with_a_page() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let page = workspace.execute("select 1").unwrap();
        assert_eq!(page.table, "result");
        assert_eq!(page.columns, vec!["1".to_owned()]);
        assert_eq!(page.rows, vec![vec!["1".to_owned()]]);
        assert_eq!(page.read_only.as_deref(), Some("query result"));

        let page = workspace.execute("SELECT  *  FROM customers;").unwrap();
        assert_eq!(page.columns[1], "name");
        assert_eq!(page.rows.len(), 40, "under the 100-row cap the whole table returns");
        assert_eq!(page.rows[0][1], "customer-0");

        workspace.write_setting(Setting::QueryMaxRows { rows: 10 }).unwrap();
        let page = workspace.execute("select * from customers").unwrap();
        assert_eq!(page.rows.len(), 10, "the query cap comes from the setting");
    }

    #[test]
    fn unknown_sql_is_an_error_and_changes_nothing() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace.open_table("customers", 0, 25, None, None).unwrap();
        workspace.stage_cell(0, "name", "staged".into()).unwrap();
        let before = workspace.open_table("customers", 0, 25, None, None).unwrap();

        assert!(workspace.execute("drop table customers").is_err());
        assert!(workspace.execute("").is_err());

        let after = workspace.open_table("customers", 0, 25, None, None).unwrap();
        assert_eq!(after.rows, before.rows, "the table and its staged values are untouched");
        workspace.stage_cell(1, "name", "more".into()).unwrap();
        let page = workspace.open_table("customers", 0, 25, None, None).unwrap();
        assert_eq!(page.rows[1][1], "more", "staging still works after failed executes");
    }

    #[test]
    fn execute_without_a_session_is_an_error() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.execute("select 1").is_err());
    }

    #[test]
    fn the_catalog_reports_three_levels_in_a_stable_order() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.catalog().databases.is_empty(), "no session, no catalog");
        workspace.connect("local").unwrap();
        let catalog = workspace.catalog();
        assert_eq!(catalog.databases.len(), 1);
        assert_eq!(catalog.databases[0].name, "local");
        let schemas = &catalog.databases[0].schemas;
        assert_eq!(schemas.len(), 2, "two schemas");
        assert_eq!(schemas[0].name, "public");
        assert_eq!(schemas[0].tables, vec!["customers", "orders"]);
        assert_eq!(schemas[1].name, "analytics");
        assert_eq!(schemas[1].tables, vec!["catalog"]);
        assert_eq!(workspace.catalog(), catalog, "the order is stable across calls");
    }

    #[test]
    fn column_drafts_are_data_not_state_changes() {
        let add = ColumnDraft::AddColumn { name: "score".into(), sql_type: "int".into() };
        let retype = ColumnDraft::ChangeType { column: "name".into(), sql_type: "varchar(80)".into() };
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let before = workspace.table_columns("customers").unwrap();
        let drafts = vec![add, retype];
        let after = workspace.table_columns("customers").unwrap();
        assert_eq!(before, after, "drafts alone change no columns");
        assert_eq!(drafts.len(), 2);
    }

    #[test]
    fn drafts_preview_as_alter_statements() {
        let add = ColumnDraft::AddColumn { name: "score".into(), sql_type: "int".into() };
        assert_eq!(add.sql("customers"), "ALTER TABLE customers ADD COLUMN score int;");
        let retype = ColumnDraft::ChangeType { column: "name".into(), sql_type: "varchar(80)".into() };
        assert_eq!(
            retype.sql("customers"),
            "ALTER TABLE customers ALTER COLUMN name TYPE varchar(80);",
            "a retype draft shows its ALTER COLUMN ... TYPE SQL"
        );
    }

    #[test]
    fn applying_a_draft_adds_the_column_and_the_port_records_it() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let sql = "ALTER TABLE customers ADD COLUMN score int;";
        workspace.apply_ddl("customers", sql).unwrap();
        let columns = workspace.table_columns("customers").unwrap();
        assert_eq!(columns.len(), 9);
        assert_eq!(columns[8].name, "score");
        assert_eq!(columns[8].sql_type, "int");
        let page = workspace.open_table("customers", 0, 25, None, None).unwrap();
        assert!(page.rows.iter().all(|row| row.len() == 9), "rows grow with the column");
        assert_eq!(page.rows[0][8], "NULL");
        assert_eq!(workspace.applied_ddl(), vec![sql.to_owned()], "the port records what it applied");

        workspace
            .apply_ddl("customers", "ALTER TABLE customers ALTER COLUMN score TYPE bigint;")
            .unwrap();
        assert_eq!(workspace.table_columns("customers").unwrap()[8].sql_type, "bigint");
    }

    #[test]
    fn a_failed_apply_changes_nothing() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        assert!(workspace.apply_ddl("orders", "ALTER TABLE orders ADD COLUMN x int;").is_err());
        assert_eq!(workspace.table_columns("orders").unwrap().len(), 2, "read-only tables refuse");
        assert!(workspace.apply_ddl("customers", "DROP TABLE nope;").is_err());
        assert_eq!(workspace.table_columns("customers").unwrap().len(), 8, "unknown statements refuse");
        assert!(
            workspace
                .apply_ddl("customers", "ALTER TABLE customers ADD COLUMN ok int;\nDROP TABLE nope;")
                .is_err(),
            "one bad line fails the whole script"
        );
        assert_eq!(workspace.table_columns("customers").unwrap().len(), 8, "nothing half-applied");
        assert!(workspace.applied_ddl().is_empty(), "failures are not recorded");
    }

    #[test]
    fn an_invalid_setting_is_rejected_and_the_old_values_stay() {
        let mut workspace = MemoryWorkspace::new();
        let before = workspace.read_settings();
        assert_eq!(before.len(), 3);
        assert!(before.contains(&Setting::TablePageSize { rows: 25 }));

        assert!(workspace.write_setting(Setting::TablePageSize { rows: 0 }).is_err());
        assert!(workspace.write_setting(Setting::TablePageSize { rows: 501 }).is_err());
        assert!(workspace.write_setting(Setting::QueryMaxRows { rows: 0 }).is_err());
        assert_eq!(
            workspace.read_settings(),
            before,
            "failed writes leave every entry at its old value"
        );

        assert!(workspace.write_setting(Setting::TablePageSize { rows: 50 }).is_ok());
        let after = workspace.read_settings();
        assert!(after.contains(&Setting::TablePageSize { rows: 50 }));
        assert!(after.contains(&Setting::Theme { dark: true }), "the other entries survive");
        assert!(after.contains(&Setting::QueryMaxRows { rows: 100 }));
        assert_eq!(after.len(), 3, "one entry per kind, replaced in place");

        assert!(workspace.write_setting(Setting::Theme { dark: false }).is_ok());
        assert!(workspace.read_settings().contains(&Setting::Theme { dark: false }));
    }

    #[test]
    fn a_connection_without_an_address_cannot_be_saved() {
        let mut workspace = MemoryWorkspace::new();
        let before = workspace.connections().len();
        assert!(workspace.save_connection("Remote", "postgres", "").is_err());
        assert!(workspace.save_connection("Remote", "postgres", "   ").is_err());
        assert_eq!(workspace.connections().len(), before, "nothing was saved");

        workspace.save_connection("Remote Postgres", "postgres", "db.example.com:5432").unwrap();
        let saved = workspace.connections().last().unwrap();
        assert_eq!(saved.id, "remote-postgres");
        assert_eq!(saved.name, "Remote Postgres");
        assert_eq!(saved.engine, "postgres");
        assert_eq!(workspace.connections().len(), before + 1);

        // A duplicate name gets its own id; both are in the port, not the keychain.
        workspace.save_connection("Remote Postgres", "mysql", "db2.example.com:3306").unwrap();
        assert_eq!(workspace.connections().last().unwrap().id, "remote-postgres-2");
    }

    #[test]
    fn snippets_add_update_delete_and_refuse_duplicates() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.snippets().is_empty());
        workspace.save_snippet("top", "SELECT * FROM customers LIMIT 10").unwrap();
        assert_eq!(workspace.snippets().len(), 1);

        assert!(
            workspace.save_snippet("top", "SELECT 1").is_err(),
            "a duplicate name is refused"
        );
        assert_eq!(
            workspace.snippets()[0].sql,
            "SELECT * FROM customers LIMIT 10",
            "the refused save changed nothing"
        );

        workspace.update_snippet("top", "SELECT 1").unwrap();
        assert_eq!(workspace.snippets()[0].sql, "SELECT 1", "update changes the sql");
        assert!(workspace.update_snippet("missing", "SELECT 1").is_err());
        assert!(workspace.save_snippet("  ", "SELECT 1").is_err(), "a name is required");

        workspace.delete_snippet("top").unwrap();
        assert!(workspace.snippets().is_empty());
        assert!(workspace.delete_snippet("top").is_err());
    }

    #[test]
    fn an_export_needs_a_range_to_get_a_job() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.start_export("csv", "").is_err(), "no range, no job");
        assert!(workspace.start_export("csv", "   ").is_err());
        assert!(workspace.start_export("exe", "customers").is_err(), "unknown format");
        let job = workspace.start_export("csv", "customers").unwrap();
        assert_eq!(job, 1, "job ids count from one");
        let second = workspace.start_export("json", "orders").unwrap();
        assert_eq!(second, 2);
    }

    #[test]
    fn a_progress_snapshot_advances_and_failures_show_their_error_line() {
        let mut workspace = MemoryWorkspace::new();
        let job = workspace.start_export("csv", "customers").unwrap();
        let first = workspace.job_progress(job).unwrap();
        assert_eq!(first.status, "running");
        assert_eq!(first.percent, 0);
        assert_eq!(first.log.len(), 2);
        let second = workspace.job_progress(job).unwrap();
        assert_eq!(second.percent, 50);
        let third = workspace.job_progress(job).unwrap();
        assert_eq!(third.status, "completed");
        assert_eq!(third.percent, 100);
        assert!(third.log.last() == Some(&"done".to_owned()));
        let settled = workspace.job_progress(job).unwrap();
        assert_eq!(settled, third, "a finished job stays settled");

        let broken = workspace.start_export("broken", "customers").unwrap();
        workspace.job_progress(broken).unwrap();
        workspace.job_progress(broken).unwrap();
        let failed = workspace.job_progress(broken).unwrap();
        assert_eq!(failed.status, "failed");
        assert!(
            failed.log.last() == Some(&"the port cannot write broken files".to_owned()),
            "the failure's error line is in the snapshot"
        );

        let missing = workspace.start_export("csv", "no-such-table").unwrap();
        workspace.job_progress(missing).unwrap();
        workspace.job_progress(missing).unwrap();
        let failed = workspace.job_progress(missing).unwrap();
        assert_eq!(failed.status, "failed");
        assert!(failed.log.last() == Some(&"no such table no-such-table".to_owned()));
        assert!(workspace.job_progress(999).is_none(), "an unknown job has no snapshot");
    }

    #[test]
    fn a_backup_stops_and_reports_stopped() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.start_backup().is_err(), "no session, no backup");
        workspace.connect("local").unwrap();
        let job = workspace.start_backup().unwrap();
        let first = workspace.job_progress(job).unwrap();
        assert_eq!(first.status, "running");
        assert_eq!(first.log[1], "backing up the database");

        workspace.job_progress(job).unwrap();
        workspace.stop_job(job).unwrap();
        let stopped = workspace.job_progress(job).unwrap();
        assert_eq!(stopped.status, "stopped", "a stopped job reports stopped");
        assert!(stopped.log.last() == Some(&"stopped by the user".to_owned()));
        let settled = workspace.job_progress(job).unwrap();
        assert_eq!(settled, stopped, "stopped stays settled");

        assert!(workspace.stop_job(job).is_err(), "stopping twice is refused");
        assert!(workspace.stop_job(999).is_err());
    }

    #[test]
    fn sync_endpoints_refuse_the_same_connection_twice() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.sync_endpoints("local", "local").is_err(), "the same end twice");
        assert!(workspace.sync_endpoints("local", "missing").is_err());
        assert!(workspace.sync_endpoints("missing", "local").is_err());
        workspace.sync_endpoints("local", "replica").unwrap();

        // The replica connection sees no analytics tables.
        workspace.connect("replica").unwrap();
        let session = workspace.session().unwrap();
        assert!(session.tables.contains(&"customers".to_owned()));
        assert!(!session.tables.contains(&"catalog".to_owned()), "the replica lacks analytics");
    }

    #[test]
    fn a_sync_preview_is_a_read_only_page() {
        let mut workspace = MemoryWorkspace::new();
        let page = workspace.sync_preview("local", "customers").unwrap();
        assert_eq!(page.table, "sync preview: customers");
        assert_eq!(page.rows.len(), 40);
        assert_eq!(page.read_only.as_deref(), Some("sync preview"));
        assert!(workspace.sync_preview("replica", "catalog").is_err(), "the replica cannot preview analytics");
        assert!(workspace.sync_preview("local", "missing").is_err());
    }

    #[test]
    fn a_sync_without_selected_tables_gets_no_job() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.run_sync("local", "replica", &[]).is_err(), "no selection, no job");
        assert!(workspace.run_sync("local", "local", &["customers".to_owned()]).is_err());
        let tables = vec!["customers".to_owned(), "orders".to_owned()];
        let job = workspace.run_sync("local", "replica", &tables).unwrap();
        let first = workspace.job_progress(job).unwrap();
        assert_eq!(first.status, "running");
        assert_eq!(first.log[1], "syncing local into replica");
        workspace.job_progress(job).unwrap();
        let done = workspace.job_progress(job).unwrap();
        assert_eq!(done.status, "completed", "a sync job finishes like the others");
    }

    #[test]
    fn the_same_object_name_is_marked_on_both_ends() {
        let workspace = MemoryWorkspace::new();
        let objects = workspace.compare_objects("local", "replica").unwrap();
        let find = |name: &str| objects.iter().find(|object| object.name == name).unwrap();
        assert!(find("customers").present_on_target, "the same name is marked present");
        assert!(find("orders").present_on_target);
        assert!(!find("catalog").present_on_target, "the replica lacks analytics");
        assert_eq!(objects.len(), 3);
        assert!(workspace.compare_objects("local", "missing").is_err());
        // A connection against itself: every name matches.
        let same = workspace.compare_objects("local", "local").unwrap();
        assert!(same.iter().all(|object| object.present_on_target));
    }

    #[test]
    fn a_compare_plan_lists_missing_objects_and_can_be_empty() {
        let workspace = MemoryWorkspace::new();
        let plan = workspace.compare_plan("local", "replica").unwrap();
        assert!(
            plan.contains("CREATE TABLE catalog"),
            "the plan names the object the target lacks: {plan}"
        );
        assert!(plan.contains("-- missing on replica"));
        assert_eq!(
            workspace.compare_plan("local", "local").unwrap(),
            "",
            "the same endpoint twice: no differences, an empty plan"
        );
        assert!(workspace.compare_plan("local", "missing").is_err());
    }

    #[test]
    fn a_failed_plan_apply_keeps_the_plan() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.run_compare_plan("").is_err(), "an empty plan has nothing to apply");
        assert!(workspace.run_compare_plan("   ").is_err());

        let plan = workspace.compare_plan("local", "replica").unwrap();
        let job = workspace.run_compare_plan(&plan).unwrap();
        workspace.job_progress(job).unwrap();
        workspace.job_progress(job).unwrap();
        let failed = workspace.job_progress(job).unwrap();
        assert_eq!(failed.status, "failed");
        assert!(
            failed.log.last() == Some(&"the replica cannot create analytics tables".to_owned()),
            "the failure says why"
        );
        // The difference is still there: the plan survives the failure.
        assert_eq!(workspace.compare_plan("local", "replica").unwrap(), plan);

        // A plan the port can apply finishes.
        let job = workspace.run_compare_plan("CREATE TABLE other (...);").unwrap();
        workspace.job_progress(job).unwrap();
        workspace.job_progress(job).unwrap();
        let done = workspace.job_progress(job).unwrap();
        assert_eq!(done.status, "completed");
    }

    #[test]
    fn a_transfer_without_pairs_gets_no_job() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.run_transfer(&[]).is_err(), "no pairs, no job");
        let pairs = vec![("id".to_owned(), "id".to_owned())];
        let job = workspace.run_transfer(&pairs).unwrap();
        let first = workspace.job_progress(job).unwrap();
        assert_eq!(first.status, "running");
        assert_eq!(first.log[1], "transferring 1 column pairs");
        workspace.job_progress(job).unwrap();
        assert_eq!(workspace.job_progress(job).unwrap().status, "completed");
    }

    #[test]
    fn a_dashboard_starts_with_no_widgets() {
        let workspace = MemoryWorkspace::new();
        assert!(workspace.dashboard_widgets().is_empty(), "the widget list starts empty");
    }

    #[test]
    fn a_bar_widget_gets_its_integers_from_the_port() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.save_dashboard_widget("", "select 1").is_err(), "a title is required");
        let id = workspace.save_dashboard_widget("Weekly", "select 1").unwrap();
        assert_eq!(id, 1);
        let bars = workspace.widget_bars(id).unwrap();
        assert_eq!(bars.first().map(|(label, _)| label.as_str()), Some("mon"));
        assert_eq!(bars[0].1, 0, "the zero bar is in the data");
        assert_eq!(bars.last().unwrap().1, 12, "the maximum is in the data");
        assert!(workspace.widget_bars(999).is_err());
        let second = workspace.save_dashboard_widget("Monthly", "select 2").unwrap();
        assert_eq!(second, 2);
        assert_eq!(workspace.dashboard_widgets().len(), 2);
    }

    #[test]
    fn workflows_read_sorted_by_name() {
        let workspace = MemoryWorkspace::new();
        let workflows = workspace.workflows();
        let names: Vec<&str> = workflows.iter().map(|flow| flow.name.as_str()).collect();
        assert_eq!(names, vec!["audit", "nightly-backup", "weekly-report"], "reads sort by name");
        assert_eq!(workflows[0].steps, vec!["compare structures", "apply plan"]);
    }

    #[test]
    fn saving_a_workflow_replaces_its_steps() {
        let mut workspace = MemoryWorkspace::new();
        assert!(workspace.save_workflow("", &[]).is_err(), "a name is required");
        workspace
            .save_workflow("audit", &["reordered".to_owned(), "compare structures".to_owned()])
            .unwrap();
        let audit = workspace
            .workflows()
            .into_iter()
            .find(|flow| flow.name == "audit")
            .unwrap();
        assert_eq!(audit.steps, vec!["reordered", "compare structures"]);
        let count = workspace.workflows().len();
        workspace.save_workflow("fresh", &["one".to_owned()]).unwrap();
        assert_eq!(workspace.workflows().len(), count + 1, "a new name appends");
    }

    #[test]
    fn workflow_runs_separate_failing_and_successful_steps() {
        let mut workspace = MemoryWorkspace::new();
        let results = workspace.run_workflow("audit").unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].step, "compare structures");
        assert!(results[0].ok);
        assert_eq!(results[0].summary, "ok: 12 rows");
        assert_eq!(results[1].step, "apply plan");
        assert!(!results[1].ok, "the applying step fails");
        assert_eq!(results[1].summary, "the replica refused the plan");
        assert!(workspace.run_workflow("missing").is_err());
        let backup = workspace.run_workflow("nightly-backup").unwrap();
        assert!(backup.iter().all(|step| step.ok), "nothing to apply, all steps pass");
    }

    #[test]
    fn table_ddl_returns_the_create_statement_or_empty() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let ddl = workspace.table_ddl("customers").unwrap();
        assert!(ddl.starts_with("CREATE TABLE customers ("));
        assert!(ddl.contains("  id text"));
        assert!(ddl.contains("  name text"));
        assert!(ddl.ends_with(");"));
        assert_eq!(
            workspace.table_ddl("catalog").unwrap(),
            "",
            "external schema tables have no DDL to show"
        );
        assert!(workspace.table_ddl("missing").is_err());
    }

    #[test]
    fn table_columns_lists_one_entry_per_column() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let columns = workspace.table_columns("customers").unwrap();
        assert_eq!(columns.len(), 8, "one row per column");
        assert_eq!(columns[0].name, "id");
        assert_eq!(columns[0].sql_type, "text");
        assert!(!columns[0].nullable, "the first column is the key");
        assert_eq!(columns[1].name, "name");
        assert!(columns[1].nullable);
        assert!(workspace.table_columns("missing").is_err());
    }

    #[test]
    fn a_compound_index_keeps_its_column_order() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let indexes = workspace.table_indexes("customers").unwrap();
        assert_eq!(indexes.len(), 2);
        assert_eq!(indexes[0].name, "customers_pkey");
        assert_eq!(indexes[0].columns, vec!["id"]);
        assert!(indexes[0].unique);
        assert_eq!(
            indexes[1].columns,
            vec!["name", "city"],
            "the compound index keeps its column order"
        );
        assert!(!indexes[1].unique);
        assert!(workspace.table_indexes("missing").is_err());
    }

    #[test]
    fn a_compound_foreign_key_is_not_split() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let keys = workspace.table_foreign_keys("orders").unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].name, "orders_customer_fk");
        assert_eq!(keys[0].columns, vec!["id", "name"], "this side stays together");
        assert_eq!(keys[0].target_table, "customers");
        assert_eq!(keys[0].target_columns, vec!["id", "name"], "the target side too");
        assert!(workspace.table_foreign_keys("catalog").unwrap().is_empty());
        assert!(workspace.table_foreign_keys("missing").is_err());
    }

    #[test]
    fn a_read_only_table_rejects_staging_and_commit() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let page = workspace.open_table("orders", 0, 25, None, None).unwrap();
        assert_eq!(page.read_only.as_deref(), Some("no primary key"));
        assert!(workspace.stage_cell(0, "name", "x".into()).is_err(), "staging on a read-only page is rejected");
        assert!(workspace.commit_staged().is_err(), "committing while a read-only page is open is rejected");

        workspace.open_table("customers", 0, 25, None, None).unwrap();
        workspace.stage_cell(0, "name", "ok".into()).unwrap();
        assert_eq!(workspace.commit_staged().unwrap(), 1, "a writable table is unaffected");
    }

    #[test]
    fn staging_addresses_the_row_behind_the_open_page() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace.open_table("customers", 1, 25, None, None).unwrap();
        workspace.stage_cell(0, "name", "staged".into()).unwrap();
        let again = workspace.open_table("customers", 1, 25, None, None).unwrap();
        assert_eq!(again.rows[0][1], "staged", "page 2 row 0 is stored row 25");
        let first = workspace.open_table("customers", 0, 25, None, None).unwrap();
        assert_eq!(first.rows[0][1], "customer-0");
        assert_eq!(first.rows[24][1], "customer-24");
    }

    #[test]
    fn the_unwired_adapter_answers_every_method_with_the_fixed_error() {
        let mut adapter = UnimplementedAdapter;
        assert_eq!(
            adapter.list_connections().unwrap_err(),
            DESKTOP_NOT_IMPLEMENTED,
            "list_connections is not wired yet"
        );
        assert_eq!(
            adapter
                .read_table_page("customers", 0, 25, None, None)
                .unwrap_err(),
            DESKTOP_NOT_IMPLEMENTED,
            "read_table_page is not wired yet"
        );
        let edits = vec![(0u32, String::from("name"), String::from("alice"))];
        assert_eq!(
            adapter.commit_cells("customers", &edits).unwrap_err(),
            DESKTOP_NOT_IMPLEMENTED,
            "commit_cells is not wired yet"
        );
    }

    #[test]
    fn the_wired_adapter_lists_the_demo_connections_through_the_use_case() {
        let mut adapter = UseCaseAdapter::with_demo_connections();
        let listed = adapter.list_connections().expect("the use case answers");
        assert_eq!(
            listed,
            vec![
                ConnectionProfile {
                    id: "local".into(),
                    name: "Local Postgres".into(),
                    engine: "postgres".into(),
                },
                ConnectionProfile {
                    id: "replica".into(),
                    name: "Read Replica".into(),
                    engine: "postgres".into(),
                },
            ],
            "the use case's redacted list maps onto the UI vocabulary"
        );
    }

    #[test]
    fn a_wired_adapter_over_an_empty_store_lists_no_connections() {
        let mut adapter = UseCaseAdapter::new(Vec::new());
        assert_eq!(
            adapter.list_connections().expect("an empty store is not an error"),
            Vec::new()
        );
    }

    #[test]
    fn the_wired_adapter_commits_a_batch_through_the_existing_use_case() {
        let mut adapter = UseCaseAdapter::with_demo_connections();
        adapter
            .read_table_page("customers", 0, 25, None, None)
            .expect("the read opens the page the rows refer to");
        let edits = vec![
            (0u32, String::from("name"), String::from("alice")),
            (1u32, String::from("name"), String::from("bob")),
        ];
        assert_eq!(
            adapter
                .commit_cells("customers", &edits)
                .expect("the batch lands"),
            2,
            "both edits applied"
        );
        let page = adapter
            .read_table_page("customers", 0, 25, None, None)
            .expect("the page reads back");
        assert_eq!(page.rows[0][1], "alice");
        assert_eq!(page.rows[1][1], "bob");
    }

    #[test]
    fn a_failed_batch_keeps_the_edits_that_did_stage() {
        let mut adapter = UseCaseAdapter::with_demo_connections();
        adapter
            .read_table_page("customers", 0, 25, None, None)
            .expect("the read opens the page");
        let edits = vec![
            (0u32, String::from("name"), String::from("alice")),
            (1u32, String::from("no-such-column"), String::from("x")),
        ];
        assert!(
            adapter.commit_cells("customers", &edits).is_err(),
            "the batch as a whole fails"
        );
        let page = adapter
            .read_table_page("customers", 0, 25, None, None)
            .expect("the page reads back");
        assert_eq!(
            page.rows[0][1], "alice",
            "the edit that did stage survives the failure for the next attempt"
        );
    }

    #[test]
    fn the_wired_adapter_reads_pages_through_the_existing_use_case() {
        let mut adapter = UseCaseAdapter::with_demo_connections();
        let first = adapter
            .read_table_page("customers", 0, 25, None, None)
            .expect("the read use case answers");
        assert_eq!(first.table, "customers");
        assert_eq!(first.columns[0], "id");
        assert_eq!(first.rows[0][0], "0");
        assert_eq!(first.rows[0][1], "customer-0");
        assert_eq!(first.total_rows, 40, "the whole table is behind the page");

        let second = adapter
            .read_table_page("customers", 1, 25, None, None)
            .expect("the second page reads too");
        assert_eq!(second.rows[0][0], "25", "page 2 starts at stored row 25");
    }

    #[test]
    fn the_wired_adapter_honors_the_sort_and_filter_of_the_read() {
        let mut adapter = UseCaseAdapter::with_demo_connections();
        let sort = ColumnSort {
            column: "name".into(),
            descending: true,
        };
        let page = adapter
            .read_table_page("customers", 0, 25, Some(&sort), None)
            .expect("the sorted read answers");
        assert_eq!(page.rows[0][1], "customer-9", "lexicographic order, descending: 9 beats 39");

        let filter = ColumnFilter {
            column: "name".into(),
            contains: "customer-3".into(),
        };
        let page = adapter
            .read_table_page("customers", 0, 25, None, Some(&filter))
            .expect("the filtered read answers");
        assert!(
            page.rows.iter().all(|row| row[1].contains("customer-3")),
            "the use case's filter reaches the rows"
        );
    }

    #[test]
    fn an_unknown_table_fails_the_wired_read() {
        let mut adapter = UseCaseAdapter::with_demo_connections();
        assert!(
            adapter.read_table_page("ghost", 0, 25, None, None).is_err(),
            "no use case knows this table"
        );
    }
}
