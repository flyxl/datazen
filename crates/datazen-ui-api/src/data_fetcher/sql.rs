use crate::{
    BatchExportMode, BatchExportOutputMode, ExportContext, ExportProgress, FetchOutcome,
    FetchRequest, FetchResult, FetchTableInput, FetchTableSource,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CancelState {
    pub requested: bool,
    pub tokens: Vec<String>,
}

impl CancelState {
    pub fn request(&mut self, token: &str) {
        if self.requested {
            return;
        }
        self.requested = true;
        self.tokens.push(token.to_owned());
    }
}

pub fn build_request(ctx: &ExportContext, cancel_token: &str) -> Result<FetchRequest, String> {
    let db_session_id = ctx
        .db_session_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "Missing connection".to_owned())?;
    let columns = ctx.column_names.clone().unwrap_or_else(|| {
        ctx.columns.iter().map(|column| column.name.clone()).collect::<Vec<_>>()
    });
    let source = match ctx.sql.as_deref().filter(|sql| !sql.is_empty()) {
        Some(sql) => FetchTableSource::Sql { sql: sql.to_owned() },
        None => FetchTableSource::Table,
    };
    Ok(FetchRequest {
        db_session_id: db_session_id.to_owned(),
        cancel_token: cancel_token.to_owned(),
        database_type: ctx.database_type.clone(),
        database: ctx.database.clone(),
        schema: ctx.schema.clone(),
        mode: ctx.mode.unwrap_or(BatchExportMode::DataOnly),
        data_format: ctx.data_format,
        output_mode: ctx.output_mode.unwrap_or(BatchExportOutputMode::Single),
        tables: vec![FetchTableInput {
            table_name: ctx.table_name.clone(),
            schema: ctx.schema.clone(),
            columns,
            source,
            primary_key: ctx.primary_key.clone(),
            ddl: ctx.ddl.clone(),
        }],
    })
}

pub fn outcome(
    result: Option<FetchResult>,
    aborted_partial_deleted: &mut Option<bool>,
) -> FetchOutcome {
    match result {
        Some(FetchResult::Aborted { partial_deleted }) => {
            *aborted_partial_deleted = Some(partial_deleted);
            FetchOutcome::Aborted
        }
        Some(FetchResult::Saved(_)) => FetchOutcome::Saved,
        Some(FetchResult::Cancelled) | None => FetchOutcome::Cancelled,
    }
}

pub fn forward_progress(table: &str, rows_written: u64) -> ExportProgress {
    ExportProgress {
        table: table.to_owned(),
        rows_written,
        rows_total: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BatchExportDataFormat, ExportScope, FetchColumn};

    fn ctx() -> ExportContext {
        ExportContext {
            table_name: "orders".to_owned(),
            scope: ExportScope::EntireTable,
            columns: Vec::new(),
            column_names: None,
            rows: None,
            db_session_id: Some("session-1".to_owned()),
            database: None,
            schema: None,
            database_type: None,
            sql: None,
            mode: None,
            ddl: None,
            data_format: BatchExportDataFormat::Csv,
            output_mode: None,
            primary_key: None,
        }
    }

    #[test]
    fn a_missing_or_empty_session_cannot_build_a_request() {
        let mut request = ctx();
        request.db_session_id = None;
        assert_eq!(build_request(&request, "t").err().as_deref(), Some("Missing connection"));
        request.db_session_id = Some(String::new());
        assert_eq!(build_request(&request, "t").err().as_deref(), Some("Missing connection"));
    }

    #[test]
    fn defaults_are_data_only_single_and_table_source() {
        let built = build_request(&ctx(), "token-1").unwrap_or_else(|_| panic!("unreachable"));
        assert_eq!(built.db_session_id, "session-1");
        assert_eq!(built.cancel_token, "token-1");
        assert_eq!(built.mode, BatchExportMode::DataOnly);
        assert_eq!(built.output_mode, BatchExportOutputMode::Single);
        assert_eq!(built.tables.len(), 1);
        assert_eq!(built.tables[0].source, FetchTableSource::Table);
        assert_eq!(built.tables[0].table_name, "orders");
        assert!(built.tables[0].ddl.is_none());
        assert!(built.tables[0].primary_key.is_none());
    }

    #[test]
    fn column_names_take_precedence_over_columns() {
        let mut request = ctx();
        request.columns = vec![FetchColumn { name: "from_columns".to_owned() }];
        request.column_names = Some(vec!["from_names".to_owned()]);
        let built = build_request(&request, "t").unwrap_or_else(|_| panic!("unreachable"));
        assert_eq!(built.tables[0].columns, vec!["from_names".to_owned()]);
        request.column_names = None;
        let built = build_request(&request, "t").unwrap_or_else(|_| panic!("unreachable"));
        assert_eq!(built.tables[0].columns, vec!["from_columns".to_owned()]);
        let built = build_request(&ctx(), "t").unwrap_or_else(|_| panic!("unreachable"));
        assert!(built.tables[0].columns.is_empty());
    }

    #[test]
    fn an_empty_sql_string_is_still_a_table_source() {
        let mut request = ctx();
        request.sql = Some(String::new());
        let built = build_request(&request, "t").unwrap_or_else(|_| panic!("unreachable"));
        assert_eq!(built.tables[0].source, FetchTableSource::Table);
        request.sql = Some("SELECT 1".to_owned());
        let built = build_request(&request, "t").unwrap_or_else(|_| panic!("unreachable"));
        assert_eq!(built.tables[0].source, FetchTableSource::Sql { sql: "SELECT 1".to_owned() });
    }

    #[test]
    fn ddl_primary_key_and_scope_fields_are_carried_verbatim() {
        let mut request = ctx();
        request.ddl = Some("CREATE TABLE orders (id INT)".to_owned());
        request.primary_key = Some(String::new());
        request.schema = Some("public".to_owned());
        request.database = Some("shop".to_owned());
        request.database_type = Some("postgresql".to_owned());
        request.mode = Some(BatchExportMode::DataAndStructure);
        request.output_mode = Some(BatchExportOutputMode::Zip);
        let built = build_request(&request, "t").unwrap_or_else(|_| panic!("unreachable"));
        assert_eq!(built.tables[0].ddl.as_deref(), Some("CREATE TABLE orders (id INT)"));
        assert_eq!(built.tables[0].primary_key.as_deref(), Some(""));
        assert_eq!(built.tables[0].schema.as_deref(), Some("public"));
        assert_eq!(built.schema.as_deref(), Some("public"));
        assert_eq!(built.database.as_deref(), Some("shop"));
        assert_eq!(built.database_type.as_deref(), Some("postgresql"));
        assert_eq!(built.mode, BatchExportMode::DataAndStructure);
        assert_eq!(built.output_mode, BatchExportOutputMode::Zip);
    }

    #[test]
    fn results_map_to_the_three_honest_outcomes() {
        let mut deleted = None;
        assert_eq!(outcome(None, &mut deleted), FetchOutcome::Cancelled);
        assert_eq!(outcome(Some(FetchResult::Cancelled), &mut deleted), FetchOutcome::Cancelled);
        assert_eq!(outcome(Some(FetchResult::Saved(0)), &mut deleted), FetchOutcome::Saved);
        assert_eq!(deleted, None);
        assert_eq!(
            outcome(Some(FetchResult::Aborted { partial_deleted: true }), &mut deleted),
            FetchOutcome::Aborted
        );
        assert_eq!(deleted, Some(true));
        assert_eq!(outcome(Some(FetchResult::Saved(1)), &mut deleted), FetchOutcome::Saved);
        assert_eq!(deleted, Some(true));
    }

    #[test]
    fn abort_records_the_partial_file_flag() {
        let mut deleted = None;
        assert_eq!(
            outcome(Some(FetchResult::Aborted { partial_deleted: false }), &mut deleted),
            FetchOutcome::Aborted
        );
        assert_eq!(deleted, Some(false));
    }

    #[test]
    fn cancel_is_idempotent_and_usable_before_fetch() {
        let mut state = CancelState::default();
        state.request("token-1");
        state.request("token-1");
        assert!(state.requested);
        assert_eq!(state.tokens, vec!["token-1".to_owned()]);
    }

    #[test]
    fn streamed_progress_never_carries_a_total() {
        let progress = forward_progress("orders", 12);
        assert_eq!(progress.table, "orders");
        assert_eq!(progress.rows_written, 12);
        assert_eq!(progress.rows_total, None);
    }
}
