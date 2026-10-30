use crate::commands::error::{CmdExt, CommandError};
use datazen_driver_api::sql_identifiers::qualify_relation_sql;

// ── Helpers ─────────────────────────────────────────────────────────

/// Count rows in a table on a given connection.
pub(super) fn value_as_u64(value: &datazen_driver_api::Value) -> Option<u64> {
    match value {
        datazen_driver_api::Value::Integer(n) if *n >= 0 => Some(*n as u64),
        datazen_driver_api::Value::Float(f) if *f >= 0.0 && f.is_finite() => Some(*f as u64),
        datazen_driver_api::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

pub(crate) async fn count_rows(
    driver: &dyn datazen_driver_api::DatabaseDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    family: &str,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> Result<u64, CommandError> {
    let quote = if family == "mysql" { '`' } else { '"' };
    let qualified = qualify_relation_sql(family, database, schema, table, quote);
    let sql = format!("SELECT COUNT(*) FROM {qualified}");
    let res = driver
        .query_at(
            handle,
            &sql,
            datazen_driver_api::SqlTarget::new(database, schema),
        )
        .await
        .cmd_err("count_rows")?;
    if let Some(row) = res.rows.first() {
        if let Some(Some(v)) = row.first() {
            if let Some(n) = value_as_u64(v) {
                return Ok(n);
            }
        }
    }
    Ok(0)
}
