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
}

pub trait WorkspaceApi {
    fn connections(&self) -> &[ConnectionProfile];
    fn session(&self) -> Option<&Session>;
    fn connect(&mut self, id: &str) -> Result<(), String>;
    fn open_table(&mut self, table: &str) -> Result<TablePage, String>;
    fn stage_cell(&mut self, row: u32, column: &str, value: String) -> Result<(), String>;
}

#[derive(Clone, Debug)]
struct StoredTable {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct MemoryWorkspace {
    connections: Vec<ConnectionProfile>,
    session: Option<Session>,
    tables: Vec<(String, StoredTable)>,
    open: Option<String>,
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
            tables: vec![("customers".into(), StoredTable { columns: columns.into_iter().map(str::to_owned).collect(), rows })],
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

    fn open_table(&mut self, table: &str) -> Result<TablePage, String> {
        if self.session.is_none() {
            return Err("not connected".into());
        }
        let stored = self
            .tables
            .iter()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        self.open = Some(table.to_owned());
        Ok(TablePage {
            table: table.to_owned(),
            columns: stored.1.columns.clone(),
            rows: stored.1.rows.clone(),
            page: 0,
            page_size: stored.1.rows.len() as u32,
        })
    }

    fn stage_cell(&mut self, row: u32, column: &str, value: String) -> Result<(), String> {
        let table = self.open.as_deref().ok_or("no open table")?;
        let stored = self
            .tables
            .iter_mut()
            .find(|(name, _)| name == table)
            .ok_or_else(|| format!("unknown table {table}"))?;
        let index = stored
            .1
            .columns
            .iter()
            .position(|name| name == column)
            .ok_or_else(|| format!("unknown column {column}"))?;
        let record = stored
            .1
            .rows
            .get_mut(row as usize)
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
        let page = workspace.open_table("customers").unwrap();
        assert_eq!(page.columns[1], "name");
        assert_eq!(page.rows[0][1], "customer-0");
        workspace.stage_cell(0, "name", "alice".into()).unwrap();
        let again = workspace.open_table("customers").unwrap();
        assert_eq!(again.rows[0][1], "alice");
    }
}
