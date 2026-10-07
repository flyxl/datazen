//! Multi-line Redis console command execution for the `exec` plugin command.

use redis::AsyncCommands;
use serde::Serialize;

use crate::driver::parse_redis_command_args;

/// Classify a Redis command by its danger level.
///
/// Returns one of: `"safe"`, `"write"`, `"danger"`, `"ultra-danger"`.
pub fn danger_classify(command: &str) -> &'static str {
    let name = command
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    match name.as_str() {
        // Ultra-danger — destructive / global / admin
        "FLUSHDB" | "FLUSHALL" | "SHUTDOWN" | "DEBUG" | "KEYS" | "CONFIG" | "ACL"
        | "MODULE" | "CLUSTER" | "REPLICAOF" | "SLAVEOF" => "ultra-danger",

        // Danger — delete / expire / dangerous mutations
        "DEL" | "UNLINK" | "RENAME" | "RENAMENX" | "EXPIRE" | "PEXPIRE" | "EXPIREAT"
        | "PEXPIREAT" | "PERSIST" | "MOVE" | "SORT" | "OBJECT" | "CLIENT" | "WAIT"
        | "SWAPDB" | "SUBSCRIBE" | "PSUBSCRIBE" | "UNSUBSCRIBE" | "PUNSUBSCRIBE"
        | "DISCARD" | "RESET" => "danger",

        // Write — normal mutations (data-modifying)
        "SET" | "MSET" | "MSETNX" | "SETEX" | "PSETEX" | "SETNX" | "SETXX"
        | "APPEND" | "INCR" | "DECR" | "INCRBY" | "DECRBY" | "INCRBYFLOAT"
        | "GETSET" | "SETRANGE" | "SETBIT" | "GETDEL" | "GETEX"
        // List
        | "LPUSH" | "LPUSHX" | "RPUSH" | "RPUSHX" | "LSET" | "LREM" | "LTRIM"
        | "LINSERT" | "RPOPLPUSH" | "LMOVE" | "LMPOP" | "BLMPOP"
        // Set
        | "SADD" | "SREM" | "SINTERSTORE" | "SUNIONSTORE" | "SDIFFSTORE" | "SMISMEMBER"
        // ZSet
        | "ZADD" | "ZREM" | "ZINCRBY" | "ZDIFFSTORE" | "ZINTERSTORE" | "ZUNIONSTORE"
        | "ZRANGEBYSCORE" | "ZREMRANGEBYRANK" | "ZREMRANGEBYSCORE"
        // Hash
        | "HSET" | "HMSET" | "HDEL" | "HINCRBY" | "HINCRBYFLOAT"
        // Stream
        | "XADD" | "XACK" | "XDEL" | "XTRIM" | "XSETID"
        // PubSub / transactions
        | "PUBLISH" | "EXEC" | "MULTI"
        // Generic key ops
        | "COPY" | "MIGRATE" | "RESTORE" | "LINK" => "write",

        // Safe — read-only / info / meta
        _ => "safe",
    }
}

/// Determine the result type string from a Redis value.
pub fn result_type_of(value: &redis::Value) -> &'static str {
    match value {
        redis::Value::Nil => "nil",
        redis::Value::Okay | redis::Value::SimpleString(_) => "ok",
        redis::Value::Int(_) | redis::Value::Double(_) | redis::Value::BigNumber(_) => "scalar",
        redis::Value::BulkString(_) | redis::Value::VerbatimString { .. } => "scalar",
        redis::Value::Array(_) => "array",
        redis::Value::Map(_) => "map",
        _ => "ok",
    }
}

/// Split a multi-line script into non-empty trimmed command lines.
pub fn split_redis_commands(commands: &str) -> Vec<String> {
    commands
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    pub command: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Structured result type hint for the UI.
    /// "scalar" | "array" | "map" | "ok" | "nil" | "error"
    pub result_type: String,
    /// Danger level classification: "safe" | "write" | "danger" | "ultra-danger"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub danger_level: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResponse {
    pub results: Vec<ExecResult>,
}

pub async fn exec_commands<C>(conn: &mut C, commands: &str) -> Result<ExecResponse, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let lines = split_redis_commands(commands);
    let mut results = Vec::with_capacity(lines.len());
    for line in lines {
        results.push(exec_one(conn, &line).await);
    }
    Ok(ExecResponse { results })
}

async fn exec_one<C>(conn: &mut C, line: &str) -> ExecResult
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let danger = Some(danger_classify(line).to_string());
    match parse_redis_command_args(line) {
        Ok(parts) => {
            let mut cmd = redis::cmd(&parts[0]);
            for part in &parts[1..] {
                cmd.arg(part.as_str());
            }
            match cmd.query_async::<redis::Value>(conn).await {
                Ok(value) => ExecResult {
                    command: line.to_string(),
                    ok: true,
                    value: Some(format_redis_value(&value)),
                    error: None,
                    result_type: result_type_of(&value).to_string(),
                    danger_level: danger,
                },
                Err(error) => ExecResult {
                    command: line.to_string(),
                    ok: false,
                    value: None,
                    error: Some(error.to_string()),
                    result_type: "error".to_string(),
                    danger_level: danger,
                },
            }
        }
        Err(error) => ExecResult {
            command: line.to_string(),
            ok: false,
            value: None,
            error: Some(error.to_string()),
            result_type: "error".to_string(),
            danger_level: danger,
        },
    }
}

fn format_redis_value(value: &redis::Value) -> String {
    match value {
        redis::Value::Nil => "(nil)".into(),
        redis::Value::Int(n) => n.to_string(),
        redis::Value::Okay => "OK".into(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::BulkString(bytes) => String::from_utf8_lossy(bytes).to_string(),
        redis::Value::VerbatimString { text, .. } => text.clone(),
        redis::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(format_redis_value).collect();
            format!("[{}]", parts.join(", "))
        }
        redis::Value::Map(map) => {
            let mut pairs: Vec<String> = map
                .iter()
                .map(|(key, val)| {
                    format!("{} => {}", format_redis_value(key), format_redis_value(val))
                })
                .collect();
            pairs.sort();
            format!("{{{}}}", pairs.join(", "))
        }
        redis::Value::Double(d) => d.to_string(),
        redis::Value::Boolean(b) => b.to_string(),
        redis::Value::BigNumber(n) => n.to_string(),
        redis::Value::Attribute { data, attributes } => {
            format!("{} (attrs: {:?})", format_redis_value(data), attributes)
        }
        redis::Value::Set(items) => {
            let mut parts: Vec<String> = items.iter().map(format_redis_value).collect();
            parts.sort();
            format!("{{{}}}", parts.join(", "))
        }
        redis::Value::Push { .. } | redis::Value::ServerError(_) => format!("{value:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_redis_commands_basic() {
        let lines = split_redis_commands("GET a\n\nSET b 1\n");
        assert_eq!(lines, vec!["GET a", "SET b 1"]);
    }

    #[test]
    fn danger_classify_safe_commands() {
        assert_eq!(danger_classify("GET foo"), "safe");
        assert_eq!(danger_classify("INFO"), "safe");
        assert_eq!(danger_classify("PING"), "safe");
        assert_eq!(danger_classify("TTL key"), "safe");
        assert_eq!(danger_classify("TYPE key"), "safe");
        assert_eq!(danger_classify("EXISTS key"), "safe");
        assert_eq!(danger_classify("SCAN 0"), "safe");
        assert_eq!(danger_classify("DBSIZE"), "safe");
    }

    #[test]
    fn danger_classify_write_commands() {
        assert_eq!(danger_classify("SET foo bar"), "write");
        assert_eq!(danger_classify("LPUSH list a"), "write");
        assert_eq!(danger_classify("HSET h f v"), "write");
        assert_eq!(danger_classify("ZADD z 1 m"), "write");
        assert_eq!(danger_classify("SADD s a"), "write");
        assert_eq!(danger_classify("XADD stream * k v"), "write");
        assert_eq!(danger_classify("INCR counter"), "write");
    }

    #[test]
    fn danger_classify_danger_commands() {
        assert_eq!(danger_classify("DEL key"), "danger");
        assert_eq!(danger_classify("EXPIRE key 100"), "danger");
        assert_eq!(danger_classify("RENAME a b"), "danger");
        assert_eq!(danger_classify("PERSIST key"), "danger");
        assert_eq!(danger_classify("SUBSCRIBE ch"), "danger");
        assert_eq!(danger_classify("CLIENT LIST"), "danger");
    }

    #[test]
    fn danger_classify_ultra_danger_commands() {
        assert_eq!(danger_classify("FLUSHDB"), "ultra-danger");
        assert_eq!(danger_classify("FLUSHALL"), "ultra-danger");
        assert_eq!(danger_classify("KEYS *"), "ultra-danger");
        assert_eq!(danger_classify("CONFIG SET"), "ultra-danger");
        assert_eq!(danger_classify("SHUTDOWN NOSAVE"), "ultra-danger");
    }

    #[test]
    fn danger_classify_case_insensitive() {
        assert_eq!(danger_classify("get foo"), "safe");
        assert_eq!(danger_classify("Set foo bar"), "write");
        assert_eq!(danger_classify("del key"), "danger");
        assert_eq!(danger_classify("flushdb"), "ultra-danger");
    }

    #[test]
    fn danger_classify_empty_command() {
        assert_eq!(danger_classify(""), "safe");
    }

    #[test]
    fn result_type_of_values() {
        // Each row pins one variant to the category `result_type_of` gives it.
        // The function matches on the variant and binds every payload to `_`, so
        // the payloads below are filler that exists only to make the variant
        // under test constructible: no number can move a row between categories.
        // The two `Double` rows therefore carry different numbers and still
        // expect the same category — if a payload ever started to matter, one of
        // them would stop matching while the other kept matching.
        let shapes: &[(redis::Value, &str)] = &[
            (redis::Value::Nil, "nil"),
            (redis::Value::Okay, "ok"),
            (redis::Value::Int(42), "scalar"),
            (redis::Value::Double(10.5), "scalar"),
            (redis::Value::Double(-0.25), "scalar"),
            (redis::Value::BulkString(b"hello".to_vec()), "scalar"),
            (redis::Value::Array(vec![redis::Value::Int(1)]), "array"),
            (
                redis::Value::Map(vec![(
                    redis::Value::BulkString(b"k".to_vec()),
                    redis::Value::Int(1),
                )]),
                "map",
            ),
        ];
        for (shape, category) in shapes {
            assert_eq!(result_type_of(shape), *category);
        }
    }
}
