use datazen_ui_api::{
    AdapterDelivery, AdapterJob, Catalog, ColumnFilter, ColumnSort, ConnectionProfile,
    DashboardWidget, DatabaseNode, DesktopAdapter, SchemaNode, TablePage, DESKTOP_NOT_IMPLEMENTED,
};
use std::cell::RefCell;
use std::rc::Rc;

/// A host adapter that owns snippet storage (T3).
pub(crate) struct SnippetHostAdapter {
    pub snippets: Vec<datazen_ui_api::Snippet>,
}

impl DesktopAdapter for SnippetHostAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Ok(Vec::new())
    }

    fn read_table_page(
        &mut self,
        _: &str,
        _: u32,
        _: u32,
        _: Option<&ColumnSort>,
        _: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn snippets(&self) -> Result<Vec<datazen_ui_api::Snippet>, String> {
        Ok(self.snippets.clone())
    }

    fn save_snippet(&mut self, name: &str, sql: &str) -> Result<(), String> {
        self.snippets
            .push(datazen_ui_api::Snippet { name: name.to_owned(), sql: sql.to_owned() });
        Ok(())
    }

    fn update_snippet(&mut self, name: &str, sql: &str) -> Result<(), String> {
        let snippet = self
            .snippets
            .iter_mut()
            .find(|snippet| snippet.name == name)
            .ok_or_else(|| format!("no snippet named {name}"))?;
        snippet.sql = sql.to_owned();
        Ok(())
    }

    fn delete_snippet(&mut self, name: &str) -> Result<(), String> {
        self.snippets.retain(|snippet| snippet.name != name);
        Ok(())
    }
}

/// A host adapter that records export and backup requests (T2) and owns their job snapshots.
pub(crate) struct PathAskingAdapter {
    pub exports: Rc<RefCell<Vec<(String, String, String, Option<String>)>>>,
    pub backups: Rc<RefCell<Vec<Option<String>>>>,
}

impl PathAskingAdapter {
    pub fn new() -> (
        Self,
        Rc<RefCell<Vec<(String, String, String, Option<String>)>>>,
        Rc<RefCell<Vec<Option<String>>>>,
    ) {
        let exports = Rc::new(RefCell::new(Vec::new()));
        let backups = Rc::new(RefCell::new(Vec::new()));
        (
            Self { exports: Rc::clone(&exports), backups: Rc::clone(&backups) },
            exports,
            backups,
        )
    }
}

impl DesktopAdapter for PathAskingAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Ok(Vec::new())
    }

    fn read_table_page(
        &mut self,
        _: &str,
        _: u32,
        _: u32,
        _: Option<&ColumnSort>,
        _: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn start_export(
        &mut self,
        table: &str,
        format: &str,
        range: &str,
        path: Option<&str>,
    ) -> Result<u64, String> {
        self.exports.borrow_mut().push((
            table.to_owned(),
            format.to_owned(),
            range.to_owned(),
            path.map(str::to_owned),
        ));
        Ok(7)
    }

    fn start_backup(&mut self, path: Option<&str>) -> Result<u64, String> {
        self.backups.borrow_mut().push(path.map(str::to_owned));
        Ok(8)
    }

    fn host_job_progress(&self, job: u64) -> Option<datazen_ui_api::ProgressSnapshot> {
        Some(datazen_ui_api::ProgressSnapshot {
            job,
            status: "running".into(),
            percent: 40,
            log: Vec::new(),
        })
    }
}

/// A host adapter that answers the inbound references (T5).
pub(crate) struct InboundAdapter;

impl DesktopAdapter for InboundAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Ok(Vec::new())
    }

    fn read_table_page(
        &mut self,
        _: &str,
        _: u32,
        _: u32,
        _: Option<&ColumnSort>,
        _: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn table_inbound_references(&self, table: &str) -> Result<Vec<datazen_ui_api::InboundRef>, String> {
        if table == "customers" {
            Ok(vec![datazen_ui_api::InboundRef {
                table: "orders".into(),
                column: "id".into(),
                referenced_column: "id".into(),
                constraint: "orders_customer_fk".into(),
            }])
        } else {
            Ok(Vec::new())
        }
    }
}

/// A host adapter that owns the favorites store (T6).
pub(crate) struct FavoritesHostAdapter {
    pub favorites: Vec<datazen_ui_api::FavoriteSpec>,
}

impl DesktopAdapter for FavoritesHostAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Ok(Vec::new())
    }

    fn read_table_page(
        &mut self,
        _: &str,
        _: u32,
        _: u32,
        _: Option<&ColumnSort>,
        _: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn favorites(&self) -> Result<Vec<datazen_ui_api::FavoriteSpec>, String> {
        Ok(self.favorites.clone())
    }

    fn save_favorite(&mut self, connection: &str, title: &str, sql: &str) -> Result<String, String> {
        let id = format!("id-{}", self.favorites.len() + 1);
        self.favorites.push(datazen_ui_api::FavoriteSpec {
            id: id.clone(),
            connection: connection.to_owned(),
            title: title.to_owned(),
            sql: sql.to_owned(),
        });
        Ok(id)
    }

    fn rename_favorite(&mut self, id: &str, title: &str) -> Result<(), String> {
        let favorite = self
            .favorites
            .iter_mut()
            .find(|favorite| favorite.id == id)
            .ok_or_else(|| format!("no favorite {id}"))?;
        favorite.title = title.to_owned();
        Ok(())
    }

    fn delete_favorite(&mut self, id: &str) -> Result<(), String> {
        self.favorites.retain(|favorite| favorite.id != id);
        Ok(())
    }
}

/// A host adapter that parses and confirms imports (T7).
pub(crate) struct ImportHostAdapter {
    pub parsed_paths: Rc<RefCell<Vec<String>>>,
    pub confirmed: Rc<RefCell<Vec<u64>>>,
}

impl ImportHostAdapter {
    pub fn new() -> (Self, Rc<RefCell<Vec<String>>>, Rc<RefCell<Vec<u64>>>) {
        let parsed_paths = Rc::new(RefCell::new(Vec::new()));
        let confirmed = Rc::new(RefCell::new(Vec::new()));
        (
            Self { parsed_paths: Rc::clone(&parsed_paths), confirmed: Rc::clone(&confirmed) },
            parsed_paths,
            confirmed,
        )
    }
}

impl DesktopAdapter for ImportHostAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Ok(Vec::new())
    }

    fn read_table_page(
        &mut self,
        _: &str,
        _: u32,
        _: u32,
        _: Option<&ColumnSort>,
        _: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn parse_connections_import(
        &mut self,
        path: &str,
        _password: &str,
    ) -> Result<datazen_ui_api::ImportPreview, String> {
        self.parsed_paths.borrow_mut().push(path.to_owned());
        Ok(datazen_ui_api::ImportPreview {
            handle: 42,
            format: "TablePlus".into(),
            connections: vec![
                datazen_ui_api::ImportedConnectionSpec {
                    name: "warehouse".into(),
                    engine: "postgres".into(),
                    address: "db.internal:5432".into(),
                },
                datazen_ui_api::ImportedConnectionSpec {
                    name: "local".into(),
                    engine: "sqlite".into(),
                    address: "/tmp/local.db".into(),
                },
            ],
            skipped: Vec::new(),
        })
    }

    fn confirm_import(&mut self, handle: u64) -> Result<u32, String> {
        self.confirmed.borrow_mut().push(handle);
        Ok(2)
    }
}

/// N13: an adapter whose registry and dashboard answer.
#[derive(Default)]
pub(crate) struct AdapterServingWorkflows {
    pub saved_widget: Rc<RefCell<Option<(String, String)>>>,
    pub saved_workflow: Rc<RefCell<Option<(String, Vec<String>)>>>,
    pub ran: Rc<RefCell<Option<String>>>,
}

impl DesktopAdapter for AdapterServingWorkflows {
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

    fn commit_cells(&mut self, _table: &str, _edits: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn dashboard_widgets(&self) -> Result<Vec<DashboardWidget>, String> {
        Ok(vec![DashboardWidget { id: 7, title: "Host Widget".into(), kind: "bar".into() }])
    }

    fn save_dashboard_widget(&mut self, title: &str, query: &str) -> Result<u64, String> {
        if title.trim().is_empty() {
            return Err("a widget needs a title".into());
        }
        *self.saved_widget.borrow_mut() = Some((title.to_owned(), query.to_owned()));
        Ok(7)
    }

    fn widget_bars(&self, id: u64) -> Result<Vec<(String, u32)>, String> {
        if id != 7 {
            return Err(format!("no widget {id}"));
        }
        Ok(vec![(String::from("one"), 2), (String::from("two"), 5)])
    }

    fn workflows(&self) -> Result<Vec<datazen_ui_api::Workflow>, String> {
        Ok(vec![datazen_ui_api::Workflow {
            name: "host-etl".into(),
            steps: vec!["SELECT count(*) FROM users".into()],
        }])
    }

    fn save_workflow(&mut self, name: &str, steps: &[String]) -> Result<(), String> {
        *self.saved_workflow.borrow_mut() = Some((name.to_owned(), steps.to_vec()));
        Ok(())
    }

    fn run_workflow(
        &mut self,
        name: &str,
    ) -> Result<Vec<datazen_ui_api::WorkflowStepResult>, String> {
        *self.ran.borrow_mut() = Some(name.to_owned());
        Ok(vec![datazen_ui_api::WorkflowStepResult {
            step: "SELECT count(*) FROM users".into(),
            ok: true,
            summary: "ok: 1 rows".into(),
        }])
    }
}

/// An adapter whose read use case serves one distinctive row.
pub(crate) struct AdapterServingOneRow;

impl DesktopAdapter for AdapterServingOneRow {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn read_table_page(
        &mut self,
        table: &str,
        page: u32,
        _page_size: u32,
        _sort: Option<&ColumnSort>,
        _filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Ok(TablePage {
            table: table.into(),
            columns: vec![String::from("adapter")],
            rows: vec![vec![String::from("from-adapter")]],
            page,
            page_size: 1,
            total_rows: 1,
            sort: None,
            filter: None,
            primary_key: Vec::new(),
            read_only: None,
        })
    }

    fn commit_cells(&mut self, _table: &str, _edits: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }
}

/// An adapter that serves a writable page but rejects every commit.
pub(crate) struct AdapterFailingCommits;

impl DesktopAdapter for AdapterFailingCommits {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn read_table_page(
        &mut self,
        table: &str,
        page: u32,
        _page_size: u32,
        _sort: Option<&ColumnSort>,
        _filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Ok(TablePage {
            table: table.into(),
            columns: vec![String::from("id"), String::from("name")],
            rows: vec![
                vec![String::from("0"), String::from("customer-0")],
                vec![String::from("1"), String::from("customer-1")],
            ],
            page,
            page_size: 2,
            total_rows: 2,
            sort: None,
            filter: None,
            primary_key: Vec::new(),
            read_only: None,
        })
    }

    fn commit_cells(&mut self, _table: &str, _edits: &[(u32, String, String)]) -> Result<u32, String> {
        Err("the desktop rejected the commit".into())
    }
}

/// N1's test host: takes every job and answers each on the poll after the next one.
pub(crate) struct JobAdapter {
    pub accepted: Vec<AdapterJob>,
    pub polls: u32,
    pub fail_commit: bool,
    pub fail_open: bool,
}

impl JobAdapter {
    pub fn new() -> Self {
        Self { accepted: Vec::new(), polls: 0, fail_commit: false, fail_open: false }
    }

    pub fn failing_commit() -> Self {
        Self { fail_commit: true, ..Self::new() }
    }

    pub fn failing_open() -> Self {
        Self { fail_open: true, ..Self::new() }
    }
}

impl DesktopAdapter for JobAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Ok(Vec::new())
    }

    fn read_table_page(
        &mut self,
        _: &str,
        _: u32,
        _: u32,
        _: Option<&ColumnSort>,
        _: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn begin_job(&mut self, job: AdapterJob) -> bool {
        self.accepted.push(job);
        true
    }

    fn poll_jobs(&mut self) -> Vec<AdapterDelivery> {
        self.polls += 1;
        if self.polls < 2 {
            return Vec::new();
        }
        self.polls = 0;
        let fail_commit = self.fail_commit;
        let fail_open = self.fail_open;
        std::mem::take(&mut self.accepted)
            .into_iter()
            .map(|job| match job {
                AdapterJob::RunWorkflow { generation, .. } => AdapterDelivery::WorkflowRun {
                    generation,
                    results: Ok(vec![datazen_ui_api::WorkflowStepResult {
                        step: "SELECT 1".into(),
                        ok: true,
                        summary: "1 rows".into(),
                    }]),
                },
                AdapterJob::ValueSearch { generation, .. } => AdapterDelivery::ValueSearched {
                    generation,
                    hits: Ok(vec![
                        datazen_ui_api::ValueSearchHit {
                            table: "events".into(),
                            column: "label".into(),
                            matches: 2,
                        },
                        datazen_ui_api::ValueSearchHit {
                            table: "users".into(),
                            column: "name".into(),
                            matches: 1,
                        },
                    ]),
                },
                AdapterJob::OpenConnection { generation, id } => AdapterDelivery::ConnectionOpened {
                    generation,
                    id: id.clone(),
                    catalog: if fail_open {
                        Err(format!("cannot open '{id}'"))
                    } else {
                        Ok(Catalog {
                            databases: vec![DatabaseNode {
                                name: "main".into(),
                                schemas: vec![SchemaNode {
                                    name: "main".into(),
                                    tables: vec!["events".into(), "users".into()],
                                }],
                            }],
                        })
                    },
                },
                AdapterJob::ReadTable { generation, table, page, page_size, .. } => {
                    AdapterDelivery::TablePage {
                        generation,
                        page: Ok(TablePage {
                            table,
                            columns: vec!["job".into()],
                            rows: vec![vec![format!("read-{generation}")]],
                            page,
                            page_size,
                            total_rows: 1,
                            sort: None,
                            filter: None,
                            primary_key: Vec::new(),
                            read_only: None,
                        }),
                    }
                }
                AdapterJob::ReadStructure { generation, table } => {
                    let ddl = format!("CREATE TABLE {table} (job text);");
                    AdapterDelivery::StructureRead {
                        generation,
                        table,
                        structure: Ok(datazen_ui_api::TableStructure {
                            ddl,
                            columns: vec![datazen_ui_api::ColumnSpec {
                                name: "job".into(),
                                sql_type: "text".into(),
                                nullable: true,
                            }],
                            indexes: vec![datazen_ui_api::IndexSpec {
                                name: "idx_job".into(),
                                columns: vec!["job".into()],
                                unique: true,
                            }],
                            foreign_keys: Vec::new(),
                        }),
                    }
                }
                AdapterJob::WidgetBars { generation, id } => AdapterDelivery::WidgetBarsRead {
                    generation,
                    id,
                    bars: Ok(vec![("mon".into(), 1)]),
                },
                AdapterJob::InboundRefs { generation, table } => AdapterDelivery::InboundRefsRead {
                    generation,
                    table,
                    refs: Ok(vec![datazen_ui_api::InboundRef {
                        table: "orders".into(),
                        column: "user_id".into(),
                        referenced_column: "id".into(),
                        constraint: "fk".into(),
                    }]),
                },
                AdapterJob::CommitCells { generation, .. } => AdapterDelivery::CommitCount {
                    generation,
                    count: if fail_commit { Err("rejected by the host".into()) } else { Ok(1) },
                },
                AdapterJob::RunQuery { generation, .. } => AdapterDelivery::QueryPage {
                    generation,
                    page: Ok(TablePage {
                        table: "result".into(),
                        columns: vec!["job".into()],
                        rows: vec![vec![format!("query-{generation}")]],
                        page: 0,
                        page_size: 1,
                        total_rows: 1,
                        sort: None,
                        filter: None,
                        primary_key: Vec::new(),
                        read_only: None,
                    }),
                },
            })
            .collect()
    }
}

/// E1: host that panics on synchronous structure reads.
pub(crate) struct StructureStrictHost {
    pub accepted: Vec<AdapterJob>,
}

impl StructureStrictHost {
    pub fn new() -> Self {
        Self { accepted: Vec::new() }
    }
}

impl DesktopAdapter for StructureStrictHost {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn read_table_page(
        &mut self,
        _: &str,
        _: u32,
        _: u32,
        _: Option<&ColumnSort>,
        _: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn table_ddl(&self, _: &str) -> Result<String, String> {
        panic!("the DDL read reached the adapter's synchronous path")
    }

    fn table_columns(&self, _: &str) -> Result<Vec<datazen_ui_api::ColumnSpec>, String> {
        panic!("the columns read reached the adapter's synchronous path")
    }

    fn table_indexes(&self, _: &str) -> Result<Vec<datazen_ui_api::IndexSpec>, String> {
        panic!("the indexes read reached the adapter's synchronous path")
    }

    fn table_foreign_keys(&self, _: &str) -> Result<Vec<datazen_ui_api::ForeignKeySpec>, String> {
        panic!("the foreign keys read reached the adapter's synchronous path")
    }

    fn begin_job(&mut self, job: AdapterJob) -> bool {
        self.accepted.push(job);
        true
    }

    fn poll_jobs(&mut self) -> Vec<AdapterDelivery> {
        self.accepted
            .drain(..)
            .map(|job| match job {
                AdapterJob::ReadStructure { generation, table } => {
                    let ddl = format!("CREATE TABLE {table} (id integer);");
                    AdapterDelivery::StructureRead {
                        generation,
                        table,
                        structure: Ok(datazen_ui_api::TableStructure {
                            ddl,
                            columns: vec![datazen_ui_api::ColumnSpec {
                                name: "id".into(),
                                sql_type: "integer".into(),
                                nullable: false,
                            }],
                            indexes: Vec::new(),
                            foreign_keys: Vec::new(),
                        }),
                    }
                }
                AdapterJob::ReadTable { generation, table, page, page_size, .. } => {
                    AdapterDelivery::TablePage {
                        generation,
                        page: Ok(TablePage {
                            table,
                            columns: vec!["id".into()],
                            rows: vec![vec!["row".into()]],
                            page,
                            page_size,
                            total_rows: 1,
                            sort: None,
                            filter: None,
                            primary_key: Vec::new(),
                            read_only: None,
                        }),
                    }
                }
                _ => unreachable!("the test only sends structure and read jobs"),
            })
            .collect()
    }
}

/// E2: host with one widget that takes bars jobs.
pub(crate) struct DashboardsJobHost {
    pub accepted: Vec<AdapterJob>,
    pub polls: u32,
}

impl DashboardsJobHost {
    pub fn new() -> Self {
        Self { accepted: Vec::new(), polls: 0 }
    }
}

impl DesktopAdapter for DashboardsJobHost {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn read_table_page(
        &mut self,
        _: &str,
        _: u32,
        _: u32,
        _: Option<&ColumnSort>,
        _: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
        Err(DESKTOP_NOT_IMPLEMENTED.into())
    }

    fn dashboard_widgets(&self) -> Result<Vec<DashboardWidget>, String> {
        Ok(vec![DashboardWidget { id: 7, title: "Counts".into(), kind: "bar".into() }])
    }

    fn widget_bars(&self, _: u64) -> Result<Vec<(String, u32)>, String> {
        panic!("the bars read reached the adapter's synchronous path")
    }

    fn begin_job(&mut self, job: AdapterJob) -> bool {
        self.accepted.push(job);
        true
    }

    fn poll_jobs(&mut self) -> Vec<AdapterDelivery> {
        self.polls += 1;
        if self.polls < 2 {
            return Vec::new();
        }
        self.polls = 0;
        self.accepted
            .drain(..)
            .map(|job| match job {
                AdapterJob::WidgetBars { generation, id } => AdapterDelivery::WidgetBarsRead {
                    generation,
                    id,
                    bars: Ok(vec![("mon".into(), 3)]),
                },
                _ => unreachable!("the test only sends bars jobs"),
            })
            .collect()
    }
}
