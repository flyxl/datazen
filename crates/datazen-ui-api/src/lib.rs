//! Ports the Rune UI is allowed to call.
//!
//! Hosts implement [`WorkspaceApi`]. The memory workspace is only a stand-in so the
//! interface can run before the desktop and HTTP adapters exist.

mod tabs;

use std::collections::HashMap;

pub use tabs::{Tab, TabKind, Tabs};

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

/// Runs SQL against the workspace. The memory implementation answers a few
/// fixed statements; unknown SQL is an error that changes no state.
pub trait QueryApi {
    fn execute(&mut self, sql: &str) -> Result<TablePage, String>;
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
                },
            ),
        ];
        Self {
            connections: vec![ConnectionProfile {
                id: "local".into(),
                name: "Local Postgres".into(),
                engine: "postgres".into(),
            }],
            session: None,
            tables,
            open: None,
            result_open: false,
        }
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
                let rows = stored.rows.iter().take(25).cloned().collect();
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
                sql_type: "text".into(),
                // The demo port keeps the first column a key, the rest nullable.
                nullable: index != 0,
            })
            .collect())
    }

    fn connect(&mut self, id: &str) -> Result<(), String> {
        if !self.connections.iter().any(|profile| profile.id == id) {
            return Err(format!("unknown connection {id}"));
        }
        self.session = Some(Session {
            connection_id: id.to_owned(),
            tables: self.tables.iter().map(|(name, _)| name.clone()).collect(),
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
        assert_eq!(page.rows.len(), 25, "one page, not the whole table");
        assert_eq!(page.total_rows, 25, "the query result is exactly that page");
        assert_eq!(page.rows[0][1], "customer-0");
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
}
