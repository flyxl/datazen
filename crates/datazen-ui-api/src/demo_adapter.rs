//! The desktop demo's connection list and editable, paged tables.

use crate::{
    ColumnFilter, ColumnSort, ConnectionProfile, DesktopAdapter, MemoryWorkspace, TablePage,
    WorkspaceApi,
};

/// The in-memory adapter used by the demo launcher and UI tests.
#[derive(Default)]
pub struct DemoAdapter {
    workspace: MemoryWorkspace,
    connection_override: Option<Vec<ConnectionProfile>>,
}

impl DemoAdapter {
    /// Show these profiles while serving the demo workspace's tables.
    pub fn with_connections(connections: Vec<ConnectionProfile>) -> Self {
        Self {
            connection_override: Some(connections),
            ..Self::default()
        }
    }
}

impl DesktopAdapter for DemoAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Ok(self
            .connection_override
            .as_deref()
            .unwrap_or_else(|| self.workspace.connections())
            .to_vec())
    }

    fn read_table_page(
        &mut self,
        table: &str,
        page: u32,
        page_size: u32,
        sort: Option<&ColumnSort>,
        filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        if self.workspace.session().is_none() {
            self.workspace.connect("local")?;
        }
        self.workspace
            .open_table(table, page, page_size, sort, filter)
    }

    fn commit_cells(
        &mut self,
        _table: &str,
        edits: &[(u32, String, String)],
    ) -> Result<u32, String> {
        // Row numbers are relative to the last page read. If staging fails,
        // keep the already staged edits available for the next attempt.
        for (row, column, value) in edits {
            self.workspace.stage_cell(*row, column, value.clone())?;
        }
        self.workspace.commit_staged()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AdapterJob, UnimplementedAdapter};

    #[test]
    fn demo_and_unwired_adapters_decline_background_jobs() {
        let job = AdapterJob::ReadTable {
            generation: 1,
            table: "customers".into(),
            page: 0,
            page_size: 25,
            sort: None,
            filter: None,
        };
        let mut unwired = UnimplementedAdapter;
        assert!(
            !unwired.begin_job(job.clone()),
            "the unwired adapter declines"
        );
        assert!(
            unwired.poll_jobs().is_empty(),
            "the unwired adapter delivers nothing"
        );

        let mut demo = DemoAdapter::with_connections(Vec::new());
        assert!(!demo.begin_job(job), "the demo adapter declines");
        assert!(
            demo.poll_jobs().is_empty(),
            "the demo adapter delivers nothing"
        );
    }

    #[test]
    fn lists_seeded_profiles_in_order() {
        let mut adapter = DemoAdapter::default();
        let listed = adapter
            .list_connections()
            .expect("the demo profiles are listed");
        assert_eq!(
            listed,
            vec![
                ConnectionProfile {
                    id: "local".into(),
                    name: "Local Postgres".into(),
                    engine: "postgres".into(),
                    host: None,
                },
                ConnectionProfile {
                    id: "replica".into(),
                    name: "Read Replica".into(),
                    engine: "postgres".into(),
                    host: None,
                },
            ],
            "the demo preserves its original profiles and order"
        );
    }

    #[test]
    fn explicitly_empty_profile_list_stays_empty() {
        let mut adapter = DemoAdapter::with_connections(Vec::new());
        assert_eq!(
            adapter
                .list_connections()
                .expect("an empty store is not an error"),
            Vec::new()
        );
    }

    #[test]
    fn commits_an_edit_batch_to_the_demo_table() {
        let mut adapter = DemoAdapter::default();
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
        let mut adapter = DemoAdapter::default();
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
    fn reads_successive_pages_of_the_demo_table() {
        let mut adapter = DemoAdapter::default();
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
    fn sorts_and_filters_the_demo_table() {
        let mut adapter = DemoAdapter::default();
        let sort = ColumnSort {
            column: "name".into(),
            descending: true,
        };
        let page = adapter
            .read_table_page("customers", 0, 25, Some(&sort), None)
            .expect("the sorted read answers");
        assert_eq!(
            page.rows[0][1], "customer-9",
            "lexicographic order, descending: 9 beats 39"
        );

        let filter = ColumnFilter {
            column: "name".into(),
            contains: "customer-3".into(),
        };
        let page = adapter
            .read_table_page("customers", 0, 25, None, Some(&filter))
            .expect("the filtered read answers");
        assert!(
            page.rows.iter().all(|row| row[1].contains("customer-3")),
            "the page contains only matching rows"
        );
    }

    #[test]
    fn an_unknown_demo_table_fails_the_read() {
        let mut adapter = DemoAdapter::default();
        assert!(
            adapter.read_table_page("ghost", 0, 25, None, None).is_err(),
            "the demo has no such table"
        );
    }
}
