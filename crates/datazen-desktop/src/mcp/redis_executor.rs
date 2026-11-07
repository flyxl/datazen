//! Desktop adapter for the MCP Redis read-only executor.
//!
//! The shared MCP layer classifies and validates commands; this adapter only
//! resolves the configured Redis profile, applies the selected logical
//! database index, and calls the existing Redis driver's `exec` command. It
//! never returns the connection profile or credentials.

use crate::commands::AppState;
use datazen::mcp::redis::RedisReadExecutor;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct DesktopRedisExecutor {
    state: Arc<AppState>,
}

impl DesktopRedisExecutor {
    pub(crate) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl RedisReadExecutor for DesktopRedisExecutor {
    async fn execute_redis(
        &self,
        connection_id: &str,
        database: Option<&str>,
        command: &str,
        args: &[String],
        _timeout_ms: u64,
    ) -> Result<Value, String> {
        let (config, metadata) = self
            .state
            .store
            .platform_profile(connection_id)
            .await
            .ok_or_else(|| "Connection profile unavailable".to_string())?;
        if !metadata.enabled {
            return Err("Connection profile is disabled".into());
        }
        if !config.database_type.eq_ignore_ascii_case("redis") {
            return Err("redis_command requires a Redis connection".into());
        }
        let db_index = redis_db_index(database.or(config.database.as_deref()))?;
        let mut config = config;
        config.database = Some(db_index.to_string());
        let driver = self
            .state
            .driver_registry
            .get(&config.database_type)
            .await
            .ok_or_else(|| "Redis driver unavailable".to_string())?;
        let handle = driver
            .connect(&config)
            .await
            .map_err(|error| error.to_string())?;
        let line = std::iter::once(command)
            .chain(args.iter().map(String::as_str))
            .map(quote_arg)
            .collect::<Vec<_>>()
            .join(" ");
        let result = driver
            .execute_command(
                &handle,
                "exec",
                json!({ "commands": line, "dbIndex": db_index }),
            )
            .await;
        let _ = driver.disconnect(handle).await;
        result
            .map(|value| value.data)
            .map_err(|error| error.to_string())
    }
}

fn redis_db_index(value: Option<&str>) -> Result<u32, String> {
    let value = value.unwrap_or("0").trim();
    let value = value.strip_prefix("db").unwrap_or(value);
    value
        .parse::<u32>()
        .map_err(|_| "Redis database must be a non-negative integer index".to_string())
}

fn quote_arg(arg: &str) -> String {
    if arg.is_empty()
        || arg
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '"' || ch == '\\')
    {
        let escaped = arg.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    } else {
        arg.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_index_accepts_plain_and_db_prefixed_values() {
        assert_eq!(redis_db_index(Some("0")).expect("index"), 0);
        assert_eq!(redis_db_index(Some("db3")).expect("index"), 3);
        assert!(redis_db_index(Some("not-a-number")).is_err());
    }

    #[test]
    fn args_with_spaces_are_quoted_for_the_driver_parser() {
        assert_eq!(quote_arg("user:1"), "user:1");
        assert_eq!(quote_arg("user 1"), "\"user 1\"");
        assert_eq!(quote_arg("a\"b"), "\"a\\\"b\"");
    }
}
