#[cfg(test)]
mod tests;

use crate::{ColumnInfo, StatementResult, WidgetRun};

pub fn widget_run_to_statement_result(run: &WidgetRun) -> StatementResult {
    StatementResult {
        source: None,
        runtime_binding: None,
        provenance: None,
        sql: String::new(),
        columns: run
            .columns
            .iter()
            .map(|name| ColumnInfo {
                name: name.clone(),
                data_type: "unknown".to_owned(),
                nullable: true,
            })
            .collect(),
        rows: run
            .rows
            .iter()
            .map(|row| row.iter().map(|cell| Some(cell.clone())).collect())
            .collect(),
        rows_affected: None,
        execution_time_ms: 0,
        truncated: None,
        pinned: None,
    }
}
