use serde_json::Value;

use crate::{BatchExportDataFormat, ExportProgress, FetchColumn, FetchOutcome};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProvidedFormatPlan {
    Text,
    Binary,
}

pub fn validation_error(
    table_name: &str,
    columns: &[FetchColumn],
    rows: &[Vec<Value>],
) -> Option<String> {
    if columns.is_empty() {
        return Some(format!(
            "provided data source '{table_name}' has no columns (empty schema)"
        ));
    }
    for (index, column) in columns.iter().enumerate() {
        if columns[..index].iter().any(|earlier| earlier.name == column.name) {
            return Some(format!(
                "provided data source '{table_name}' has a duplicate column name '{}'",
                column.name
            ));
        }
    }
    for (index, row) in rows.iter().enumerate() {
        if row.len() != columns.len() {
            return Some(format!(
                "provided data source '{table_name}' row {index} has {} values but the schema has {} columns",
                row.len(),
                columns.len()
            ));
        }
    }
    None
}

pub fn format_plan(format: BatchExportDataFormat) -> Result<ProvidedFormatPlan, String> {
    match format {
        BatchExportDataFormat::Xlsx => Ok(ProvidedFormatPlan::Binary),
        BatchExportDataFormat::Parquet => {
            Err("provided-data export cannot format 'parquet'".to_owned())
        }
        _ => Ok(ProvidedFormatPlan::Text),
    }
}

pub fn start_progress(table: &str, rows_total: u64) -> ExportProgress {
    ExportProgress {
        table: table.to_owned(),
        rows_written: 0,
        rows_total: Some(rows_total),
    }
}

pub fn finish(saved: bool, table: &str, rows_total: u64) -> (FetchOutcome, Option<ExportProgress>) {
    if saved {
        (
            FetchOutcome::Saved,
            Some(ExportProgress {
                table: table.to_owned(),
                rows_written: rows_total,
                rows_total: Some(rows_total),
            }),
        )
    } else {
        (FetchOutcome::Cancelled, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn columns(names: &[&str]) -> Vec<FetchColumn> {
        names.iter().map(|name| FetchColumn { name: (*name).to_owned() }).collect()
    }

    #[test]
    fn a_valid_provided_source_has_no_validation_error() {
        let rows = vec![vec![json!(1), json!("a")], vec![json!(2), json!("b")]];
        assert_eq!(validation_error("users", &columns(&["id", "name"]), &rows), None);
        assert_eq!(validation_error("users", &columns(&["id"]), &[]), None);
    }

    #[test]
    fn an_empty_schema_is_rejected() {
        assert_eq!(
            validation_error("users", &[], &[]).as_deref(),
            Some("provided data source 'users' has no columns (empty schema)")
        );
    }

    #[test]
    fn a_duplicate_column_name_is_rejected_with_the_name() {
        assert_eq!(
            validation_error("users", &columns(&["id", "id"]), &[]).as_deref(),
            Some("provided data source 'users' has a duplicate column name 'id'")
        );
    }

    #[test]
    fn a_row_width_mismatch_is_rejected_with_index_and_both_lengths() {
        let short_first = vec![vec![json!(1)], vec![json!(2), json!(3)]];
        assert_eq!(
            validation_error("users", &columns(&["a", "b"]), &short_first).as_deref(),
            Some("provided data source 'users' row 0 has 1 values but the schema has 2 columns")
        );
        let short_second = vec![vec![json!(1), json!(2)], vec![json!(3)]];
        assert_eq!(
            validation_error("users", &columns(&["a", "b"]), &short_second).as_deref(),
            Some("provided data source 'users' row 1 has 1 values but the schema has 2 columns")
        );
        let wide = vec![vec![json!(1), json!(2)]];
        assert_eq!(
            validation_error("users", &columns(&["a"]), &wide).as_deref(),
            Some("provided data source 'users' row 0 has 2 values but the schema has 1 columns")
        );
    }

    #[test]
    fn an_empty_first_row_is_rejected_before_later_rows() {
        let rows = vec![Vec::new(), vec![json!(1)]];
        assert_eq!(
            validation_error("users", &columns(&["a"]), &rows).as_deref(),
            Some("provided data source 'users' row 0 has 0 values but the schema has 1 columns")
        );
    }

    #[test]
    fn only_xlsx_takes_the_binary_branch_and_parquet_is_rejected() {
        assert_eq!(format_plan(BatchExportDataFormat::Xlsx), Ok(ProvidedFormatPlan::Binary));
        assert_eq!(format_plan(BatchExportDataFormat::Csv), Ok(ProvidedFormatPlan::Text));
        assert_eq!(format_plan(BatchExportDataFormat::SqlUpdate), Ok(ProvidedFormatPlan::Text));
        assert_eq!(
            format_plan(BatchExportDataFormat::Parquet),
            Err("provided-data export cannot format 'parquet'".to_owned())
        );
    }

    #[test]
    fn progress_is_a_start_end_pair_with_known_total() {
        let start = start_progress("users", 3);
        assert_eq!(start.table, "users");
        assert_eq!(start.rows_written, 0);
        assert_eq!(start.rows_total, Some(3));
        let (outcome, end) = finish(true, "users", 3);
        assert_eq!(outcome, FetchOutcome::Saved);
        assert_eq!(end, Some(ExportProgress { table: "users".to_owned(), rows_written: 3, rows_total: Some(3) }));
    }

    #[test]
    fn a_cancelled_save_emits_no_end_event() {
        let (outcome, end) = finish(false, "users", 3);
        assert_eq!(outcome, FetchOutcome::Cancelled);
        assert_eq!(end, None);
    }

    #[test]
    fn a_zero_row_export_still_emits_both_events() {
        let start = start_progress("users", 0);
        assert_eq!(start.rows_total, Some(0));
        let (_, end) = finish(true, "users", 0);
        assert_eq!(end.map(|event| event.rows_written), Some(0));
    }
}
