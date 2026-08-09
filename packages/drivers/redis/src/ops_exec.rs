//! Multi-line Redis console command execution for the `exec` plugin command.

use redis::aio::MultiplexedConnection;
use serde::Serialize;

use crate::redis_driver::parse_redis_command_args;

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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResponse {
    pub results: Vec<ExecResult>,
}

pub async fn exec_commands(
    conn: &mut MultiplexedConnection,
    commands: &str,
) -> Result<ExecResponse, String> {
    let lines = split_redis_commands(commands);
    let mut results = Vec::with_capacity(lines.len());
    for line in lines {
        results.push(exec_one(conn, &line).await);
    }
    Ok(ExecResponse { results })
}

async fn exec_one(conn: &mut MultiplexedConnection, line: &str) -> ExecResult {
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
                },
                Err(error) => ExecResult {
                    command: line.to_string(),
                    ok: false,
                    value: None,
                    error: Some(error.to_string()),
                },
            }
        }
        Err(error) => ExecResult {
            command: line.to_string(),
            ok: false,
            value: None,
            error: Some(error.to_string()),
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
                .map(|(key, val)| format!("{} => {}", format_redis_value(key), format_redis_value(val)))
                .collect();
            pairs.sort();
            format!("{{{}}}", pairs.join(", "))
        }
        redis::Value::Double(d) => d.to_string(),
        redis::Value::Boolean(b) => b.to_string(),
        redis::Value::BigNumber(n) => n.to_string(),
        redis::Value::Attribute { data, attributes } => {
            format!(
                "{} (attrs: {:?})",
                format_redis_value(data),
                attributes
            )
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
}
