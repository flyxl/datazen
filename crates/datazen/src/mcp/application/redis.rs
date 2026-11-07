//! Redis read-only command classification and execution scaffolding.
//!
//! The driver already owns the Redis wire protocol; this module owns the MCP
//! security boundary. Unknown commands fail closed, writes/dangerous/admin
//! commands are refused before any executor call, and a caller can narrow the
//! readable key space with a glob `key_pattern`. `SCAN` is the only way to
//! enumerate keys: `KEYS` is classified as dangerous and never reaches the
//! driver.

use serde_json::{json, Value};
use std::time::Duration;

use crate::mcp::application::tools::ToolError;

pub const DEFAULT_LIMIT: u32 = 100;
pub const MAX_LIMIT: u32 = 1000;
pub const DEFAULT_TIMEOUT_MS: u64 = 5_000;
pub const MAX_TIMEOUT_MS: u64 = 30_000;
const MAX_ARGS: usize = 100;
const MAX_KEY_PATTERN_CHARS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedisCommandClass {
    Read,
    Write,
    Dangerous,
    Admin,
    Unsupported,
}

pub fn classify_redis_command(command: &str) -> RedisCommandClass {
    let name = command
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    match name.as_str() {
        "GET" | "MGET" | "SCAN" | "EXISTS" | "TYPE" | "TTL" | "PTTL" | "STRLEN" | "GETRANGE"
        | "BITCOUNT" | "HGET" | "HMGET" | "HGETALL" | "HLEN" | "HEXISTS" | "HKEYS" | "HVALS"
        | "LRANGE" | "LLEN" | "LINDEX" | "SMEMBERS" | "SCARD" | "SISMEMBER" | "ZRANGE"
        | "ZSCORE" | "ZCARD" | "ZCOUNT" | "XRANGE" | "XLEN" | "DBSIZE" | "PING" => {
            RedisCommandClass::Read
        }

        "SET" | "MSET" | "MSETNX" | "SETEX" | "PSETEX" | "SETNX" | "SETXX" | "APPEND" | "INCR"
        | "DECR" | "INCRBY" | "DECRBY" | "INCRBYFLOAT" | "GETSET" | "SETRANGE" | "SETBIT"
        | "GETDEL" | "GETEX" | "DEL" | "UNLINK" | "RENAME" | "RENAMENX" | "EXPIRE" | "PEXPIRE"
        | "EXPIREAT" | "PEXPIREAT" | "PERSIST" | "MOVE" | "COPY" | "LPUSH" | "LPUSHX" | "RPUSH"
        | "RPUSHX" | "LSET" | "LREM" | "LTRIM" | "LINSERT" | "RPOPLPUSH" | "LMOVE" | "LMPOP"
        | "BLMPOP" | "SADD" | "SREM" | "SINTERSTORE" | "SUNIONSTORE" | "SDIFFSTORE" | "ZADD"
        | "ZREM" | "ZINCRBY" | "ZDIFFSTORE" | "ZINTERSTORE" | "ZUNIONSTORE" | "ZREMRANGEBYRANK"
        | "ZREMRANGEBYSCORE" | "HSET" | "HMSET" | "HDEL" | "HINCRBY" | "HINCRBYFLOAT" | "XADD"
        | "XACK" | "XDEL" | "XTRIM" | "XSETID" | "PUBLISH" | "MIGRATE" | "RESTORE" | "LINK"
        | "TOUCH" => RedisCommandClass::Write,

        "FLUSHDB" | "FLUSHALL" | "KEYS" | "EVAL" | "EVALSHA" | "SCRIPT" | "FUNCTION" | "FCALL"
        | "FCALL_RO" | "SORT" | "SWAPDB" | "SUBSCRIBE" | "PSUBSCRIBE" | "UNSUBSCRIBE"
        | "PUNSUBSCRIBE" | "MULTI" | "EXEC" | "DISCARD" | "WATCH" | "UNWATCH" | "RESET" => {
            RedisCommandClass::Dangerous
        }

        "CONFIG" | "ACL" | "CLIENT" | "CLUSTER" | "MODULE" | "REPLICAOF" | "SLAVEOF"
        | "SHUTDOWN" | "DEBUG" | "MONITOR" | "MIGRATE" | "LATENCY" | "SLOWLOG" | "MEMORY"
        | "COMMAND" | "INFO" | "ROLE" | "FAILOVER" => RedisCommandClass::Admin,

        _ => RedisCommandClass::Unsupported,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedisCommandPlan {
    pub command: String,
    pub args: Vec<String>,
    pub key_pattern: String,
    pub limit: u32,
    pub timeout_ms: u64,
}

pub fn plan_redis_command(
    command: &str,
    args: &[String],
    key_pattern: Option<&str>,
    limit: Option<u32>,
    timeout_ms: Option<u32>,
) -> Result<RedisCommandPlan, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("redis_command requires a command name".into());
    }
    if command.split_whitespace().count() != 1 || command.contains(['\n', '\r']) {
        return Err("redis_command takes one command name; pass arguments separately".into());
    }
    let upper = command.to_ascii_uppercase();
    let class = classify_redis_command(&upper);
    if class != RedisCommandClass::Read {
        let reason = match class {
            RedisCommandClass::Write => "write",
            RedisCommandClass::Dangerous => "dangerous",
            RedisCommandClass::Admin => "administrative",
            RedisCommandClass::Unsupported => "unsupported",
            RedisCommandClass::Read => unreachable!(),
        };
        return Err(format!(
            "Redis command '{upper}' is {reason} and is not available to MCP; only read-only commands are allowed"
        ));
    }
    if args.len() > MAX_ARGS {
        return Err(format!(
            "redis_command accepts at most {MAX_ARGS} arguments (received {})",
            args.len()
        ));
    }
    if args
        .iter()
        .any(|arg| arg.contains(['\n', '\r']) || arg.contains('\0'))
    {
        return Err("redis_command arguments must not contain newlines or NUL bytes".into());
    }
    let key_pattern = key_pattern.unwrap_or("*").trim();
    if key_pattern.is_empty() {
        return Err("key_pattern must not be empty".into());
    }
    if key_pattern.chars().count() > MAX_KEY_PATTERN_CHARS {
        return Err(format!(
            "key_pattern must be at most {MAX_KEY_PATTERN_CHARS} characters"
        ));
    }
    let limit = match limit {
        None | Some(0) => DEFAULT_LIMIT,
        Some(value) => value.min(MAX_LIMIT),
    };
    let timeout_ms = match timeout_ms {
        None | Some(0) => DEFAULT_TIMEOUT_MS,
        Some(value) => u64::from(value).min(MAX_TIMEOUT_MS),
    };

    let args = if upper == "SCAN" {
        plan_scan(args, key_pattern, limit)?
    } else {
        validate_key_args(&upper, args, key_pattern, limit)?;
        args.to_vec()
    };

    Ok(RedisCommandPlan {
        command: upper,
        args,
        key_pattern: key_pattern.to_owned(),
        limit,
        timeout_ms,
    })
}

fn validate_key_args(
    command: &str,
    args: &[String],
    key_pattern: &str,
    limit: u32,
) -> Result<(), String> {
    let positions: Vec<usize> = match command {
        // These read commands take a variable number of keys: every argument
        // is a key and every one must stay inside key_pattern. HMGET below is
        // different: only args[0] is a key; the rest are fields.
        "MGET" | "EXISTS" => {
            if args.is_empty() {
                return Err(format!(
                    "Redis command '{command}' requires at least one key"
                ));
            }
            if args.len() > limit as usize {
                return Err(format!(
                    "Redis command '{command}' accepts at most {limit} keys in this call"
                ));
            }
            (0..args.len()).collect()
        }
        "HMGET" => {
            if args.is_empty() {
                return Err("Redis command 'HMGET' requires a key".into());
            }
            vec![0]
        }
        "PING" | "DBSIZE" => Vec::new(),
        _ => {
            if args.is_empty() {
                return Err(format!(
                    "Redis command '{command}' requires at least one key"
                ));
            }
            vec![0]
        }
    };
    for position in positions {
        let Some(key) = args.get(position) else {
            return Err(format!(
                "Redis command '{command}' is missing argument {}",
                position + 1
            ));
        };
        if !wildcard_matches(key_pattern, key) {
            return Err(format!(
                "Redis key '{key}' is outside the configured key pattern '{key_pattern}'"
            ));
        }
    }
    Ok(())
}

fn plan_scan(args: &[String], key_pattern: &str, limit: u32) -> Result<Vec<String>, String> {
    let Some(cursor) = args.first() else {
        return Err("Redis SCAN requires a numeric cursor".into());
    };
    if cursor.parse::<u64>().is_err() {
        return Err("Redis SCAN cursor must be a non-negative integer".into());
    }
    let mut pattern: Option<&str> = None;
    let mut count: Option<u32> = None;
    let mut type_filter: Option<&str> = None;
    let mut index = 1;
    while index < args.len() {
        let option = args[index].to_ascii_uppercase();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("Redis SCAN option '{option}' requires a value"))?;
        match option.as_str() {
            "MATCH" => pattern = Some(value),
            "COUNT" => {
                count = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| "Redis SCAN COUNT must be a positive integer".to_string())?,
                );
            }
            "TYPE" => type_filter = Some(value),
            other => return Err(format!("unsupported Redis SCAN option '{other}'")),
        }
        index += 2;
    }
    if let Some(pattern) = pattern {
        if key_pattern != "*" && pattern != key_pattern {
            return Err(format!(
                "Redis SCAN MATCH '{pattern}' must stay within the configured key pattern '{key_pattern}'"
            ));
        }
    } else if key_pattern != "*" {
        // A restricted key space must never fall back to an unrestricted SCAN.
        pattern = Some(key_pattern);
    }
    let count = match count {
        Some(count) if count > limit => {
            return Err(format!(
                "Redis SCAN COUNT {count} exceeds the configured limit {limit}"
            ));
        }
        Some(count) => count,
        None => limit,
    };
    let mut canonical = vec![cursor.clone()];
    if let Some(pattern) = pattern.or(Some("*")) {
        canonical.push("MATCH".into());
        canonical.push(pattern.to_owned());
    }
    canonical.push("COUNT".into());
    canonical.push(count.to_string());
    if let Some(type_filter) = type_filter {
        canonical.push("TYPE".into());
        canonical.push(type_filter.to_owned());
    }
    Ok(canonical)
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let value: Vec<char> = value.chars().collect();
    let mut dp = vec![vec![false; value.len() + 1]; pattern.len() + 1];
    dp[0][0] = true;
    for i in 1..=pattern.len() {
        if pattern[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=pattern.len() {
        for j in 1..=value.len() {
            dp[i][j] = match pattern[i - 1] {
                '*' => dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i - 1][j - 1],
                literal => dp[i - 1][j - 1] && literal == value[j - 1],
            };
        }
    }
    dp[pattern.len()][value.len()]
}

#[async_trait::async_trait]
pub trait RedisReadExecutor: Send + Sync {
    async fn execute_redis(
        &self,
        connection_id: &str,
        database: Option<&str>,
        command: &str,
        args: &[String],
        timeout_ms: u64,
    ) -> Result<Value, String>;
}

pub async fn execute_redis_read(
    executor: &dyn RedisReadExecutor,
    connection_id: &str,
    database: Option<&str>,
    command: &str,
    args: &[String],
    key_pattern: Option<&str>,
    limit: Option<u32>,
    timeout_ms: Option<u32>,
) -> Result<String, ToolError> {
    let plan = plan_redis_command(command, args, key_pattern, limit, timeout_ms)
        .map_err(ToolError::InvalidParams)?;
    let result = tokio::time::timeout(
        Duration::from_millis(plan.timeout_ms),
        executor.execute_redis(
            connection_id,
            database,
            &plan.command,
            &plan.args,
            plan.timeout_ms,
        ),
    )
    .await
    .map_err(|_| ToolError::Internal("Redis command timed out".into()))?
    .map_err(ToolError::Internal)?;

    if result.get("results").and_then(Value::as_array).is_none() {
        return Err(ToolError::Internal(
            "Redis driver returned an unexpected result shape".into(),
        ));
    }
    let first = result
        .get("results")
        .and_then(Value::as_array)
        .and_then(|results| results.first())
        .ok_or_else(|| ToolError::Internal("Redis driver returned no command result".into()))?;
    if first.get("ok").and_then(Value::as_bool) == Some(false) {
        let message = first
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Redis command failed");
        return Err(ToolError::Internal(message.to_owned()));
    }
    let output = json!({
        "command": plan.command,
        "keyPattern": plan.key_pattern,
        "limit": plan.limit,
        "result": first,
    });
    let text = serde_json::to_string_pretty(&output)
        .map_err(|error| ToolError::Internal(error.to_string()))?;
    let max_chars = (plan.limit as usize).saturating_mul(1024);
    if text.len() > max_chars {
        return Err(ToolError::InvalidParams(format!(
            "Redis result exceeds the configured limit ({max_chars} chars); narrow key_pattern or reduce limit"
        )));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn classification_matrix_blocks_writes_and_dangerous_commands() {
        for command in ["GET", "MGET", "SCAN"] {
            assert_eq!(classify_redis_command(command), RedisCommandClass::Read);
        }
        for command in ["SET", "DEL"] {
            assert_eq!(classify_redis_command(command), RedisCommandClass::Write);
        }
        for command in ["FLUSHALL", "EVAL", "KEYS"] {
            assert_eq!(
                classify_redis_command(command),
                RedisCommandClass::Dangerous
            );
        }
        assert_eq!(classify_redis_command("CONFIG"), RedisCommandClass::Admin);
        assert_eq!(
            classify_redis_command("NOT_A_COMMAND"),
            RedisCommandClass::Unsupported
        );
    }

    #[test]
    fn write_and_dangerous_commands_never_plan() {
        for command in ["SET", "DEL", "FLUSHALL", "CONFIG", "EVAL"] {
            let error = plan_redis_command(command, &strings(&["key", "value"]), None, None, None)
                .unwrap_err();
            assert!(error.contains("only read-only"), "{command}: {error}");
        }
    }

    #[test]
    fn key_pattern_rejects_keys_outside_the_prefix() {
        assert!(
            plan_redis_command("GET", &strings(&["app:user:1"]), Some("app:*"), None, None).is_ok()
        );
        let error = plan_redis_command(
            "GET",
            &strings(&["other:user:1"]),
            Some("app:*"),
            None,
            None,
        )
        .unwrap_err();
        assert!(error.contains("outside the configured key pattern"));
    }

    #[test]
    fn every_key_of_a_multi_key_read_must_match_the_pattern() {
        let exists = plan_redis_command(
            "EXISTS",
            &strings(&["t20:key", "t20:other"]),
            Some("t20:*"),
            Some(100),
            None,
        );
        assert!(exists.is_ok(), "{exists:?}");
        let mixed = plan_redis_command(
            "EXISTS",
            &strings(&["t20:key", "other:key"]),
            Some("t20:*"),
            Some(100),
            None,
        )
        .unwrap_err();
        assert!(mixed.contains("other:key"), "{mixed}");
        let single = plan_redis_command(
            "EXISTS",
            &strings(&["other:key"]),
            Some("t20:*"),
            Some(100),
            None,
        )
        .unwrap_err();
        assert!(single.contains("other:key"), "{single}");
        let mget_mixed = plan_redis_command(
            "MGET",
            &strings(&["t20:key", "other:key"]),
            Some("t20:*"),
            Some(100),
            None,
        )
        .unwrap_err();
        assert!(mget_mixed.contains("other:key"), "{mget_mixed}");
        for command in ["DEL", "UNLINK", "TOUCH"] {
            assert!(plan_redis_command(
                command,
                &strings(&["t20:key", "t20:other"]),
                Some("t20:*"),
                Some(100),
                None,
            )
            .is_err());
        }
    }

    #[test]
    fn mget_and_scan_limits_are_enforced() {
        let error = plan_redis_command("MGET", &strings(&["a", "b", "c"]), None, Some(2), None)
            .unwrap_err();
        assert!(error.contains("at most 2 keys"));
        let scan = plan_redis_command(
            "SCAN",
            &strings(&["0", "MATCH", "app:*", "COUNT", "10"]),
            Some("app:*"),
            Some(10),
            None,
        )
        .unwrap();
        assert_eq!(scan.args, strings(&["0", "MATCH", "app:*", "COUNT", "10"]));
        let too_many = plan_redis_command(
            "SCAN",
            &strings(&["0", "COUNT", "11"]),
            None,
            Some(10),
            None,
        )
        .unwrap_err();
        assert!(too_many.contains("exceeds the configured limit"));
    }

    struct FakeExecutor {
        calls: Mutex<Vec<String>>,
        result: Value,
    }

    #[async_trait::async_trait]
    impl RedisReadExecutor for FakeExecutor {
        async fn execute_redis(
            &self,
            _connection_id: &str,
            _database: Option<&str>,
            command: &str,
            args: &[String],
            _timeout_ms: u64,
        ) -> Result<Value, String> {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("{command} {}", args.join(" ")));
            Ok(self.result.clone())
        }
    }

    #[tokio::test]
    async fn read_command_executes_through_the_injected_executor_and_bounds_output() {
        let executor = FakeExecutor {
            calls: Mutex::new(Vec::new()),
            result: json!({ "results": [{ "command": "GET user:1", "ok": true, "value": "alice", "resultType": "scalar" }] }),
        };
        let output = execute_redis_read(
            &executor,
            "redis-cfg",
            Some("0"),
            "GET",
            &strings(&["user:1"]),
            Some("user:*"),
            Some(10),
            None,
        )
        .await
        .expect("read command");
        assert!(output.contains("alice"));
        assert!(output.contains("\"keyPattern\": \"user:*\""));
        assert_eq!(
            executor.calls.lock().expect("calls").as_slice(),
            ["GET user:1"]
        );
    }

    #[tokio::test]
    async fn multi_key_scope_violation_never_calls_the_executor() {
        for command in ["EXISTS", "MGET"] {
            let executor = FakeExecutor {
                calls: Mutex::new(Vec::new()),
                result: json!({ "results": [] }),
            };
            let error = execute_redis_read(
                &executor,
                "redis-cfg",
                None,
                command,
                &strings(&["t20:key", "other:key"]),
                Some("t20:*"),
                Some(100),
                None,
            )
            .await
            .expect_err("mixed keys must be rejected");
            assert!(error.into_message().contains("other:key"));
            assert!(executor.calls.lock().expect("calls").is_empty());
        }
    }

    #[tokio::test]
    async fn exists_with_all_keys_inside_the_pattern_reaches_the_executor() {
        let executor = FakeExecutor {
            calls: Mutex::new(Vec::new()),
            result: json!({ "results": [{ "result": "2" }] }),
        };
        execute_redis_read(
            &executor,
            "redis-cfg",
            None,
            "EXISTS",
            &strings(&["t20:key", "t20:other"]),
            Some("t20:*"),
            Some(100),
            None,
        )
        .await
        .expect("all keys are in scope");
        assert_eq!(executor.calls.lock().expect("calls").len(), 1);
    }

    #[tokio::test]
    async fn executor_is_never_called_for_a_write() {
        let executor = FakeExecutor {
            calls: Mutex::new(Vec::new()),
            result: json!({ "results": [] }),
        };
        let error = execute_redis_read(
            &executor,
            "redis-cfg",
            None,
            "SET",
            &strings(&["user:1", "x"]),
            None,
            None,
            None,
        )
        .await
        .expect_err("write must be rejected");
        assert!(error.into_message().contains("only read-only"));
        assert!(executor.calls.lock().expect("calls").is_empty());
    }
}
