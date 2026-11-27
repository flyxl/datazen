//! Native SQL script validation and ordered result collection.

use datazen::execution::domain::sql_guard::{check_sql, sql_main_keyword, tokenize_sql};
use datazen_driver_api::resource::{ResultChunk, StatementSummary};
use datazen_ui_api::TablePage;
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) enum ScriptDialect {
    Sqlite,
    Postgres,
}

impl ScriptDialect {
    pub(super) fn linked_driver(name: &str) -> Option<Self> {
        match name {
            "sqlite" => Some(Self::Sqlite),
            "postgresql" => Some(Self::Postgres),
            _ => None,
        }
    }

    pub(super) fn is_single_read_select(self, sql: &str) -> bool {
        let single = match self {
            Self::Sqlite => datazen_driver_sqlite::sqlite_statement_ranges(sql)
                .is_ok_and(|ranges| ranges.len() == 1),
            Self::Postgres => datazen_driver_api::sql_dump::split_sql_statements(sql).len() == 1,
        };
        single
            && sql_main_keyword(sql).as_deref() == Some("SELECT")
            && check_sql(sql, true, false).is_ok()
    }

    pub(super) fn statement_count(self, sql: &str) -> Result<usize, String> {
        match self {
            Self::Sqlite => sqlite_statement_count(sql),
            Self::Postgres => postgres_statement_count(sql),
        }
    }
}

fn postgres_statement_count(sql: &str) -> Result<usize, String> {
    // This is the same splitter as the PostgreSQL resource stream. In
    // particular, ordinary, escaped, and dollar-quoted semicolons do not create tabs.
    let statements = datazen_driver_api::sql_dump::split_sql_statements(sql);
    if !(2..=20).contains(&statements.len()) {
        return Err("Enter 2 to 20 PostgreSQL SELECT statements.".into());
    }
    if statements.iter().any(|statement| {
        !tokenize_sql(statement)
            .first()
            .is_some_and(|token| token.eq_ignore_ascii_case("select"))
            || check_sql(statement, true, false).is_err()
    }) {
        return Err("The PostgreSQL script may contain only non-mutating SELECT statements.".into());
    }
    Ok(statements.len())
}

fn sqlite_statement_count(sql: &str) -> Result<usize, String> {
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
    summary: Option<StatementSummary>,
    truncated: bool,
}

pub(super) struct CollectedPage {
    pub(super) page: TablePage,
    pub(super) affected_rows: Option<u64>,
    pub(super) truncated_to: Option<u32>,
}

pub(super) struct ResultPages {
    pages: Vec<Page>,
    script: bool,
    display_limit: Option<u32>,
}

impl ResultPages {
    pub(super) fn new(count: usize, script: bool, display_limit: Option<u32>) -> Self {
        Self {
            pages: (0..count).map(|_| Page::default()).collect(),
            script,
            display_limit,
        }
    }

    pub(super) fn push(&mut self, output: Value) -> Result<(), String> {
        if output["kind"] == "statementSummary" {
            if let Ok(summary) = serde_json::from_value::<StatementSummary>(output) {
                if let Some(page) = self.pages.get_mut(summary.statement_index) {
                    // The statement's final total is not a row-batch increment.
                    page.summary = Some(summary);
                }
            }
            return Ok(());
        }
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
            if let Some(columns) = output["columns"].as_array() {
                let names: Vec<String> = columns
                    .iter()
                    .filter_map(|column| column["name"].as_str())
                    .map(str::to_owned)
                    .collect();
                if self.script && names.len() != columns.len() {
                    return Err("Statement metadata has an invalid column name.".into());
                }
                // PostgreSQL publishes metadata through the resource stream
                // and the host emits the completed statement metadata again.
                // An identical header is idempotent, but disagreement would
                // make it unsafe to render the rows as belonging to this tab.
                if self.script && page.has_metadata && page.columns != names {
                    return Err("Conflicting statement metadata.".into());
                }
                page.columns = names;
                page.has_metadata = true;
            } else if self.script {
                return Err("Statement metadata has no column list.".into());
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
        // Some result providers return a whole artifact chunk. Keep only
        // the requested prefix regardless of chunk boundaries, but consume
        // terminal metadata and all completion artifacts normally.
        let available = self.display_limit.map_or(usize::MAX, |limit| {
            (limit as usize).saturating_sub(page.rows.len())
        });
        page.truncated |= chunk.rows.len() > available;
        page.rows.extend(
            chunk
                .rows
                .into_iter()
                .take(available)
                .map(|row| row.into_iter().map(super::cell_to_text).collect()),
        );
        page.has_rows = true;
        Ok(())
    }

    pub(super) fn finish(self) -> Result<Vec<CollectedPage>, String> {
        if self.script
            && self
                .pages
                .iter()
                .any(|page| {
                    // PostgreSQL has a typed terminal summary even for an empty
                    // SELECT. Never invent a page without metadata and completion.
                    !page.has_metadata || page.columns.is_empty()
                        || !(page.has_rows || page.summary.is_some())
                })
        {
            return Err("The SELECT script returned incomplete statement results.".into());
        }
        let display_limit = self.display_limit;
        Ok(self
            .pages
            .into_iter()
            .map(|page| {
                let total = page.rows.len() as u32;
                let affected_rows = if page.has_metadata && page.columns.is_empty() && total == 0 {
                    page.summary.and_then(|summary| {
                        let verb = sql_main_keyword(&summary.sql)?;
                        matches!(verb.as_str(), "INSERT" | "UPDATE" | "DELETE" | "MERGE")
                            .then_some(summary.rows_affected).flatten()
                    })
                } else {
                    None
                };
                CollectedPage {
                    page: TablePage {
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
                    },
                    affected_rows,
                    truncated_to: display_limit.filter(|_| page.truncated),
                }
            })
            .collect())
    }
}
