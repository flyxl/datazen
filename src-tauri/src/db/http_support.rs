//! Shared helpers for HTTP-based builtin drivers (ClickHouse, Elasticsearch,
//! RQLite, Turso, InfluxDB, VictoriaMetrics, HBase, vector stores).

use super::*;
use std::time::Duration;

pub fn build_http_client(
    timeout_secs: u32,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<reqwest::Client, DriverError> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(timeout_secs as u64))
        .timeout(Duration::from_secs((timeout_secs as u64).max(60)));
    if let (Some(u), Some(p)) = (username, password) {
        if !u.is_empty() {
            use base64::Engine;
            let token = base64::engine::general_purpose::STANDARD
                .encode(format!("{u}:{p}"));
            let mut headers = reqwest::header::HeaderMap::new();
            if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Basic {token}")) {
                headers.insert(reqwest::header::AUTHORIZATION, v);
            }
            builder = builder.default_headers(headers);
        }
    }
    builder
        .build()
        .map_err(|e| DriverError::ConnectionFailed(format!("HTTP client error: {e}")))
}

pub fn base_url(config: &ConnectionConfig) -> Result<String, DriverError> {
    let host = config
        .host
        .clone()
        .ok_or_else(|| DriverError::InvalidConfig("host is required".into()))?;
    let port = config
        .port
        .ok_or_else(|| DriverError::InvalidConfig("port is required".into()))?;
    let scheme = match config.ssl_mode {
        SslMode::Disable | SslMode::Prefer => "http",
        SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull => "https",
    };
    Ok(format!("{scheme}://{host}:{port}"))
}

/// Convert an arbitrary serde_json::Value into DataZen's `Value`.
pub fn json_to_value(v: &serde_json::Value) -> Option<Value> {
    match v {
        serde_json::Value::Null => Some(Value::Null),
        serde_json::Value::Bool(b) => Some(Value::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Some(Value::Float(f))
            } else {
                Some(Value::String(n.to_string()))
            }
        }
        serde_json::Value::String(s) => Some(Value::String(s.clone())),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            Some(Value::Json(v.clone()))
        }
    }
}

/// Map an HTTP error to a driver error.
pub fn http_error(context: &str, e: reqwest::Error) -> DriverError {
    DriverError::QueryFailed(format!("{context}: {e}"))
}

pub fn status_error(context: &str, status: reqwest::StatusCode, body: &str) -> DriverError {
    DriverError::QueryFailed(format!(
        "{context}: HTTP {} {}",
        status.as_u16(),
        body.chars().take(300).collect::<String>()
    ))
}

/// Render a DataZen `Value` for display in table cells.
pub fn value_display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => format!("{other:?}"),
    }
}

/// Format a tabular EXPLAIN result into the shared `ExplainResult` shape.
pub fn explain_result_from_query(result: QueryResult) -> ExplainResult {
    let plan_lines: Vec<String> = result
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .filter_map(|v| {
                    v.as_ref().map(|val| match val {
                        Value::String(s) => s.clone(),
                        Value::Integer(n) => n.to_string(),
                        Value::Float(f) => f.to_string(),
                        Value::Bool(b) => b.to_string(),
                        other => format!("{other:?}"),
                    })
                })
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .collect();
    ExplainResult {
        plan_text: plan_lines.join("\n"),
        plan_json: None,
        total_cost: None,
        estimated_rows: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explain_result_joins_row_cells() {
        let result = QueryResult {
            columns: vec![
                ColumnInfo {
                    name: "a".into(),
                    data_type: "Int".into(),
                    nullable: true,
                },
                ColumnInfo {
                    name: "b".into(),
                    data_type: "String".into(),
                    nullable: true,
                },
            ],
            rows: vec![vec![
                Some(Value::Integer(1)),
                Some(Value::String("scan".into())),
            ]],
            rows_affected: None,
            execution_time_ms: 0,
        };
        let explain = explain_result_from_query(result);
        assert_eq!(explain.plan_text, "1 | scan");
        assert!(explain.plan_json.is_none());
    }
}
