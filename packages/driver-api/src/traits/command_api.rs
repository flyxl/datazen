//! Driver Command dispatch shared by the SQL drivers.
//!
//! `query` and `execute` are the two commands every SQL driver answers, and the
//! routing rule behind them — rewrite the injected target, then dispatch through
//! the driver — is identical for all of them. Keeping the dispatch here lets the
//! trait only declare that the command exists, while the rule itself is written
//! once and stays readable.

use super::DatabaseDriver;
use crate::sql_target::SqlTarget;
use crate::types::{ConnectionHandle, DriverError};
use crate::{try_execute_schema_catalog_command, CommandResult};

pub(crate) async fn execute_command<D: DatabaseDriver + ?Sized>(
    driver: &D,
    handle: &ConnectionHandle,
    command: &str,
    input: serde_json::Value,
) -> Result<CommandResult, DriverError> {
    match execute_standard_sql_command(driver, handle, command, input.clone()).await {
        Ok(result) => return Ok(result),
        Err(DriverError::Unsupported(_)) => {}
        Err(err) => return Err(err),
    }
    if let Some(result) = try_execute_schema_catalog_command(driver, handle, command, input).await?
    {
        return Ok(result);
    }
    Err(DriverError::Unsupported(format!(
        "unsupported driver command: {command}"
    )))
}

/// Default `query` / `execute` command dispatch shared by SQL drivers.
///
/// F7: the input object may carry optional targeting fields `database` /
/// `schema` (injected by the host from the IPC envelope). When present, the
/// SQL is rewritten through [`DatabaseDriver::qualify_sql_target`] before
/// execution; drivers without the capability execute as-is (logged), keeping
/// the host session pin as fallback.
pub async fn execute_standard_sql_command<D: DatabaseDriver + ?Sized>(
    driver: &D,
    handle: &ConnectionHandle,
    command: &str,
    input: serde_json::Value,
) -> Result<CommandResult, DriverError> {
    match command {
        "query" => {
            let (sql, target) = sql_input_with_target(&input, "query")?;
            let limit = input
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|v| v.min(u32::MAX as u64) as u32);
            let result = driver.query_multi_at(handle, &sql, limit, target).await?;
            let data = serde_json::to_value(result).map_err(|e| {
                DriverError::QueryFailed(format!("failed to serialize query result: {e}"))
            })?;
            Ok(CommandResult::new(data))
        }
        "execute" => {
            let (sql, target) = sql_input_with_target(&input, "execute")?;
            let rows_affected = driver.execute_at(handle, &sql, target).await?;
            Ok(CommandResult::new(serde_json::json!({
                "rowsAffected": rows_affected
            })))
        }
        other => Err(DriverError::Unsupported(format!(
            "unsupported driver command: {other}"
        ))),
    }
}

/// Extract the `sql` input of a standard SQL command together with the target
/// the host injected into the envelope.
///
/// The target is returned rather than applied here: qualification is only half
/// the story, because a driver that keeps per-database resources must also
/// route the statement to the right one. Callers pass this to
/// [`DatabaseDriver::query_multi_at`] / [`DatabaseDriver::execute_at`], which
/// apply the rewrite *and* the routing.
fn sql_input_with_target<'a>(
    input: &'a serde_json::Value,
    command: &str,
) -> Result<(String, SqlTarget<'a>), DriverError> {
    let sql = input
        .get("sql")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            DriverError::InvalidConfig(format!("command '{command}' requires string input 'sql'"))
        })?
        .to_string();

    let database = optional_target_field(input, "database");
    let schema = optional_target_field(input, "schema");
    Ok((sql, SqlTarget { database, schema }))
}

fn optional_target_field<'a>(input: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}
