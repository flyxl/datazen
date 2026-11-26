//! Validation and result collection for native SQLite SELECT scripts.

use datazen::execution::domain::sql_guard::tokenize_sql;
use datazen_driver_api::resource::ResultChunk;
use datazen_ui_api::TablePage;
use serde_json::Value;

pub(super) fn select_statement_count(sql: &str) -> Result<usize, String> {
    let ranges = datazen_driver_sqlite::sqlite_statement_ranges(sql)
        .map_err(|_| "The SQLite script is not complete.")?;
    if !(2..=20).contains(&ranges.len()) {
        return Err("Enter 2 to 20 SQLite SELECT statements.".into());
    }
    if ranges.iter().any(|range| {
        !tokenize_sql(&sql[range.clone()])
            .first()
            .is_some_and(|token| token.eq_ignore_ascii_case("select"))
    }) {
        return Err("The SQLite script may contain only SELECT statements.".into());
    }
    Ok(ranges.len())
}

#[derive(Default)]
struct Page {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    has_metadata: bool,
    has_rows: bool,
}

pub(super) struct ResultPages {
    pages: Vec<Page>,
    script: bool,
}

impl ResultPages {
    pub(super) fn new(count: usize, script: bool) -> Self {
        Self {
            pages: (0..count).map(|_| Page::default()).collect(),
            script,
        }
    }

    pub(super) fn push(&mut self, output: Value) -> Result<(), String> {
        if output["kind"] == "statementMetadata" {
            let index = output["statementIndex"]
                .as_u64()
                .and_then(|index| usize::try_from(index).ok());
            if self.script && index.is_none() {
                return Err("Statement metadata has no valid index.".into());
            }
            let Some(page) = self.pages.get_mut(index.unwrap_or(0)) else {
                return if self.script {
                    Err("Unexpected statement metadata index.".into())
                } else {
                    Ok(())
                };
            };
            if self.script && page.has_metadata {
                return Err("Duplicate statement metadata.".into());
            }
            if let Some(columns) = output["columns"].as_array() {
                page.columns = columns
                    .iter()
                    .filter_map(|column| column["name"].as_str())
                    .map(str::to_owned)
                    .collect();
                page.has_metadata = true;
            }
            return Ok(());
        }
        let Ok(chunk) = serde_json::from_value::<ResultChunk>(output) else {
            return Ok(());
        };
        let Some(page) = self.pages.get_mut(chunk.statement_index) else {
            return if self.script {
                Err("Unexpected statement result index.".into())
            } else {
                Ok(())
            };
        };
        page.rows.extend(
            chunk
                .rows
                .into_iter()
                .map(|row| row.into_iter().map(super::cell_to_text).collect()),
        );
        page.has_rows = true;
        Ok(())
    }

    pub(super) fn finish(self) -> Result<Vec<TablePage>, String> {
        if self.script
            && self
                .pages
                .iter()
                .any(|page| !page.has_metadata || !page.has_rows)
        {
            return Err("The SQLite script returned incomplete statement results.".into());
        }
        Ok(self
            .pages
            .into_iter()
            .map(|page| {
                let total = page.rows.len() as u32;
                TablePage {
                    table: "result".into(),
                    columns: page.columns,
                    rows: page.rows,
                    page: 0,
                    page_size: total.max(1),
                    total_rows: total,
                    sort: None,
                    filter: None,
                    primary_key: Vec::new(),
                    read_only: Some("query result".into()),
                }
            })
            .collect())
    }
}
