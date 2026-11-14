//! Ports the Rune UI is allowed to call.
//!
//! Hosts implement [`WorkspaceApi`]. The memory workspace is only a stand-in so the
//! interface can run before the desktop and HTTP adapters exist.

mod tabs;

pub use tabs::{Tab, TabKind, Tabs};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TablePage {
    pub table: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub page: u32,
    pub page_size: u32,
    /// Rows in the whole table, not only this page. Footers page over it.
    pub total_rows: u32,
}

pub trait WorkspaceApi {
    fn connections(&self) -> &[ConnectionProfile];
    fn session(&self) -> Option<&Session>;
    fn connect(&mut self, id: &str) -> Result<(), String>;
    fn disconnect(&mut self);
    fn open_table(&mut self, table: &str, page: u32, page_size: u32) -> Result<TablePage, String>;
    fn stage_cell(&mut self, row: u32, column: &str, value: String) -> Result<(), String>;
}

#[derive(Clone, Debug)]
struct StoredTable {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
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
        Self {
            connections: vec![ConnectionProfile {
                id: "local".into(),
                name: "Local Postgres".into(),
                engine: "postgres".into(),
            }],
            session: None,
            tables: vec![
                ("customers".into(), StoredTable { columns: columns.into_iter().map(str::to_owned).collect(), rows }),
                (
                    "orders".into(),
                    StoredTable {
                        columns: vec!["id".into(), "name".into()],
                        rows: vec![vec!["1".into(), "order-0".into()]],
                    },
                ),
            ],
            open: None,
        }
    }
}

impl WorkspaceApi for MemoryWorkspace {
    fn connections(&self) -> &[ConnectionProfile] {
        &self.connections
    }

    fn session(&self) -> Option<&Session> {
        self.session.as_ref()
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
    }

    fn open_table(&mut self, table: &str, page: u32, page_size: u32) -> Result<TablePage, String> {
        if self.session.is_none() {
            return Err("not connected".into());
        }
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        let total = stored.1.rows.len() as u32;
        let page_size = page_size.max(1);
        let page = page.min(total / page_size);
        let rows = stored.1.rows
            [(page * page_size) as usize..(((page + 1) * page_size) as usize).min(total as usize)]
            .to_vec();
        self.open = Some(OpenTable { name: table.to_owned(), page, page_size });
        Ok(TablePage {
            table: table.to_owned(),
            columns: stored.1.columns.clone(),
            rows,
            page,
            page_size,
            total_rows: total,
        })
    }

    fn stage_cell(&mut self, row: u32, column: &str, value: String) -> Result<(), String> {
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
        let record = stored
            .1
            .rows
            .get_mut(stored_row as usize)
            .ok_or_else(|| format!("row {row} is outside the page"))?;
        record[index] = value;
        Ok(())
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
        let page = workspace.open_table("customers", 0, 25).unwrap();
        assert_eq!(page.columns[1], "name");
        assert_eq!(page.rows[0][1], "customer-0");
        workspace.stage_cell(0, "name", "alice".into()).unwrap();
        let again = workspace.open_table("customers", 0, 25).unwrap();
        assert_eq!(again.rows[0][1], "alice");
    }

    #[test]
    fn the_second_page_is_not_the_first_page() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        let first = workspace.open_table("customers", 0, 25).unwrap();
        let second = workspace.open_table("customers", 1, 25).unwrap();
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
        let page = workspace.open_table("customers", 99, 25).unwrap();
        assert_eq!(page.page, 1);
        assert_eq!(page.rows.len(), 15);
    }

    #[test]
    fn staging_addresses_the_row_behind_the_open_page() {
        let mut workspace = MemoryWorkspace::new();
        workspace.connect("local").unwrap();
        workspace.open_table("customers", 1, 25).unwrap();
        workspace.stage_cell(0, "name", "staged".into()).unwrap();
        let again = workspace.open_table("customers", 1, 25).unwrap();
        assert_eq!(again.rows[0][1], "staged", "page 2 row 0 is stored row 25");
        let first = workspace.open_table("customers", 0, 25).unwrap();
        assert_eq!(first.rows[0][1], "customer-0");
        assert_eq!(first.rows[24][1], "customer-24");
    }
}
