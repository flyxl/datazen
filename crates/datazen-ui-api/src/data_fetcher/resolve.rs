use crate::data_fetcher::{DataFetcherKind, ExportContext};

pub fn resolve_fetcher(ctx: &ExportContext) -> Result<DataFetcherKind, String> {
    if ctx.scope.is_provided() {
        if ctx.rows.is_none() {
            return Err(format!(
                "provided export for table '{}' has no loaded rows (scope '{}'): current-page / selected exports need rows in the frontend",
                ctx.table_name,
                ctx.scope.as_str()
            ));
        }
        return Ok(DataFetcherKind::Provided);
    }
    let has_session = ctx
        .db_session_id
        .as_deref()
        .map(|id| !id.is_empty())
        .unwrap_or(false);
    if !has_session {
        return Err(format!(
            "cannot resolve an exporter for table '{}' (scope '{}'): entire-table and filtered exports need a database session",
            ctx.table_name,
            ctx.scope.as_str()
        ));
    }
    Ok(DataFetcherKind::Sql)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BatchExportDataFormat, ExportScope};

    fn ctx() -> ExportContext {
        ExportContext {
            table_name: "orders".to_owned(),
            scope: ExportScope::CurrentPage,
            columns: Vec::new(),
            column_names: None,
            rows: None,
            db_session_id: None,
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
    fn a_provided_scope_with_rows_never_needs_a_session() {
        for scope in [ExportScope::CurrentPage, ExportScope::Selected] {
            let mut request = ctx();
            request.scope = scope;
            request.rows = Some(Vec::new());
            assert_eq!(resolve_fetcher(&request), Ok(DataFetcherKind::Provided));
            request.db_session_id = Some("session-1".to_owned());
            assert_eq!(resolve_fetcher(&request), Ok(DataFetcherKind::Provided));
        }
    }

    #[test]
    fn a_provided_scope_without_rows_is_a_defensive_error() {
        for (scope, literal) in [
            (ExportScope::CurrentPage, "current_page"),
            (ExportScope::Selected, "selected"),
        ] {
            let mut request = ctx();
            request.scope = scope;
            let message = format!(
                "provided export for table 'orders' has no loaded rows (scope '{literal}'): current-page / selected exports need rows in the frontend"
            );
            assert_eq!(resolve_fetcher(&request).err().as_deref(), Some(message.as_str()));
        }
    }

    #[test]
    fn a_table_scope_needs_a_live_session() {
        for scope in [ExportScope::EntireTable, ExportScope::FilteredTable] {
            let mut request = ctx();
            request.scope = scope;
            request.rows = Some(Vec::new());
            let message = format!(
                "cannot resolve an exporter for table 'orders' (scope '{}'): entire-table and filtered exports need a database session",
                scope.as_str()
            );
            assert_eq!(resolve_fetcher(&request).err().as_deref(), Some(message.as_str()));
            request.db_session_id = Some(String::new());
            assert!(resolve_fetcher(&request).is_err());
            request.db_session_id = Some("session-1".to_owned());
            assert_eq!(resolve_fetcher(&request), Ok(DataFetcherKind::Sql));
        }
    }

    #[test]
    fn a_sql_scope_with_rows_still_uses_the_sql_fetcher() {
        let mut request = ctx();
        request.scope = ExportScope::FilteredTable;
        request.rows = Some(Vec::new());
        request.sql = Some("SELECT 1".to_owned());
        request.db_session_id = Some("session-1".to_owned());
        assert_eq!(resolve_fetcher(&request), Ok(DataFetcherKind::Sql));
    }
}
