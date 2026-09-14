//! Redis command dispatch (`execute_redis_command`).

use datazen_driver_api::{
    execute_standard_sql_command, try_execute_schema_catalog_command, CommandResult,
    ConnectionHandle, DriverError,
};
use serde_json::Value as JsonValue;

use crate::ops::ZsetMember;
use crate::ops_io::RestoreKeyEntry;
use crate::RedisDriver;

fn req_str<'a>(input: &'a JsonValue, field: &str) -> Result<&'a str, DriverError> {
    input
        .get(field)
        .and_then(JsonValue::as_str)
        .ok_or_else(|| DriverError::InvalidConfig(format!("command input requires '{field}'")))
}

fn opt_str<'a>(input: &'a JsonValue, field: &str) -> Option<&'a str> {
    input
        .get(field)
        .and_then(JsonValue::as_str)
        .filter(|s| !s.is_empty())
}

fn req_i64(input: &JsonValue, field: &str) -> Result<i64, DriverError> {
    input.get(field).and_then(JsonValue::as_i64).ok_or_else(|| {
        DriverError::InvalidConfig(format!("command input requires integer '{field}'"))
    })
}

fn req_bool(input: &JsonValue, field: &str) -> Result<bool, DriverError> {
    input
        .get(field)
        .and_then(JsonValue::as_bool)
        .ok_or_else(|| {
            DriverError::InvalidConfig(format!("command input requires boolean '{field}'"))
        })
}

fn db_index(input: &JsonValue) -> u32 {
    input
        .get("dbIndex")
        .or_else(|| input.get("db_index"))
        .and_then(JsonValue::as_u64)
        .unwrap_or(0) as u32
}

fn opt_string_vec(input: &JsonValue, field: &str) -> Vec<String> {
    input
        .get(field)
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn string_vec(input: &JsonValue, field: &str) -> Result<Vec<String>, DriverError> {
    input
        .get(field)
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_string)
                .collect()
        })
        .ok_or_else(|| {
            DriverError::InvalidConfig(format!("command input requires string array '{field}'"))
        })
}

fn json_ok<T: serde::Serialize>(value: T) -> Result<CommandResult, DriverError> {
    serde_json::to_value(value)
        .map(CommandResult::new)
        .map_err(|e| DriverError::QueryFailed(e.to_string()))
}

fn ok() -> CommandResult {
    CommandResult::new(serde_json::json!({ "ok": true }))
}

pub async fn execute_redis_command(
    driver: &RedisDriver,
    handle: &ConnectionHandle,
    command: &str,
    input: JsonValue,
) -> Result<CommandResult, DriverError> {
    match execute_standard_sql_command(driver, handle, command, input.clone()).await {
        Err(DriverError::Unsupported(_)) => {}
        other => return other,
    }
    if let Some(result) =
        try_execute_schema_catalog_command(driver, handle, command, input.clone()).await?
    {
        return Ok(result);
    }

    let id = handle.pool_id.as_str();
    let db = db_index(&input);

    // Full `match command { ... }` expression — include! cannot expand in pattern position.
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/commands_exec_dispatch.rs"
    ))
}
