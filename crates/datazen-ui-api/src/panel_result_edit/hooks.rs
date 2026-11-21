use super::store::PanelResultEditStore;
use super::types::{
    grid_read_only_message_key, result_edit_key, GridReadOnlyReason, ResolveTicket,
    ResultEditabilityOptions, ResultOriginRequest, ResultWritableError, VerdictView,
};

pub fn verdict_view(store: &PanelResultEditStore, key: Option<&str>) -> VerdictView {
    match key {
        Some(key) => VerdictView { verdict: store.verdict(key).cloned(), resolving: store.resolving(key) },
        None => VerdictView { verdict: None, resolving: false },
    }
}

pub fn columns_key(result_columns: &[String]) -> String {
    serde_json::to_string(result_columns).unwrap_or_default()
}

pub fn columns_from_key(key: &str) -> Vec<String> {
    serde_json::from_str(key).unwrap_or_default()
}

pub fn effect_dependency_key(options: &ResultEditabilityOptions) -> String {
    serde_json::json!([
        options.key,
        options.db_session_id,
        options.database,
        options.schema,
        options.sql,
        columns_key(&options.result_columns),
        options.table,
    ])
    .to_string()
}

pub fn effect_request(options: &ResultEditabilityOptions) -> Option<(String, ResultOriginRequest)> {
    let key = options.key.as_ref()?;
    let columns = columns_from_key(&columns_key(&options.result_columns));
    Some((
        key.clone(),
        ResultOriginRequest {
            db_session_id: options.db_session_id.clone(),
            database: options.database.clone(),
            schema: options.schema.clone(),
            sql: options.sql.clone(),
            result_columns: columns,
            table: options.table.clone(),
        },
    ))
}

pub fn start_effect(
    store: &mut PanelResultEditStore,
    options: &ResultEditabilityOptions,
) -> Option<ResolveTicket> {
    let (key, request) = effect_request(options)?;
    store.begin_resolve(&key, request)
}

pub fn assert_result_writable(
    store: &PanelResultEditStore,
    db_session_id: &str,
    result_idx: usize,
) -> Result<(), ResultWritableError> {
    let key = result_edit_key(db_session_id, result_idx);
    match store.verdict(&key) {
        Some(verdict) if !verdict.writable => {
            let reason = verdict.reason.unwrap_or(GridReadOnlyReason::UnknownOrigin);
            Err(ResultWritableError { message_key: grid_read_only_message_key(reason) })
        }
        _ => Ok(()),
    }
}
