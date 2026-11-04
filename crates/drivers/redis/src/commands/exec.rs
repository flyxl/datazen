//! Redis command dispatch (`execute_redis_command`).

use datazen_driver_api::{
    execute_standard_sql_command, try_execute_schema_catalog_command, CommandResult,
    ConnectionHandle, DriverError,
};
use serde_json::Value as JsonValue;

use crate::RedisDriver;

mod exec_dispatch;

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

    exec_dispatch::dispatch(driver, handle, command, input).await
}
