//! Redis value → table rows and command-line parsing.

use datazen_driver_api::{ColumnInfo, DriverError, Value};

use crate::redis_value_preview::value_to_string;

/// Split a Redis command line, respecting double-quoted arguments (spaces inside quotes).
pub(crate) fn parse_redis_command_args(s: &str) -> Result<Vec<String>, DriverError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(DriverError::QueryFailed("Empty command".into()));
    }
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == b'"' {
            i += 1;
            let mut cur = String::new();
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 1;
                    cur.push(bytes[i] as char);
                    i += 1;
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    cur.push(bytes[i] as char);
                    i += 1;
                }
            }
            out.push(cur);
        } else {
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            out.push(s[start..i].to_string());
        }
    }
    if out.is_empty() {
        return Err(DriverError::QueryFailed("Empty command".into()));
    }
    Ok(out)
}

pub(crate) fn redis_value_to_rows(value: &redis::Value) -> (Vec<ColumnInfo>, Vec<Vec<Option<Value>>>) {
    match value {
        redis::Value::Nil => (
            vec![ColumnInfo {
                name: "result".into(),
                data_type: "string".into(),
                nullable: true,
            }],
            vec![vec![Some(Value::Null)]],
        ),
        redis::Value::Int(n) => (
            vec![ColumnInfo {
                name: "result".into(),
                data_type: "integer".into(),
                nullable: false,
            }],
            vec![vec![Some(Value::Integer(*n))]],
        ),
        redis::Value::BulkString(bytes) => {
            let s = String::from_utf8_lossy(bytes).to_string();
            (
                vec![ColumnInfo {
                    name: "result".into(),
                    data_type: "string".into(),
                    nullable: false,
                }],
                vec![vec![Some(Value::String(s))]],
            )
        }
        redis::Value::VerbatimString { text, .. } => (
            vec![ColumnInfo {
                name: "result".into(),
                data_type: "string".into(),
                nullable: false,
            }],
            vec![vec![Some(Value::String(text.clone()))]],
        ),
        redis::Value::Array(items) => {
            if items.len() >= 2 && items.len() % 2 == 0 && looks_like_hash(items) {
                let columns = vec![
                    ColumnInfo {
                        name: "field".into(),
                        data_type: "string".into(),
                        nullable: false,
                    },
                    ColumnInfo {
                        name: "value".into(),
                        data_type: "string".into(),
                        nullable: true,
                    },
                ];
                let rows: Vec<Vec<Option<Value>>> = items
                    .chunks(2)
                    .map(|pair| {
                        vec![
                            Some(redis_to_value(&pair[0])),
                            Some(redis_to_value(&pair[1])),
                        ]
                    })
                    .collect();
                (columns, rows)
            } else {
                let columns = vec![
                    ColumnInfo {
                        name: "index".into(),
                        data_type: "integer".into(),
                        nullable: false,
                    },
                    ColumnInfo {
                        name: "value".into(),
                        data_type: "string".into(),
                        nullable: true,
                    },
                ];
                let rows: Vec<Vec<Option<Value>>> = items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| vec![Some(Value::Integer(i as i64)), Some(redis_to_value(v))])
                    .collect();
                (columns, rows)
            }
        }
        redis::Value::SimpleString(s) => (
            vec![ColumnInfo {
                name: "result".into(),
                data_type: "string".into(),
                nullable: false,
            }],
            vec![vec![Some(Value::String(s.clone()))]],
        ),
        #[allow(deprecated)]
        redis::Value::Okay => (
            vec![ColumnInfo {
                name: "result".into(),
                data_type: "string".into(),
                nullable: false,
            }],
            vec![vec![Some(Value::String("OK".into()))]],
        ),
        _ => (
            vec![ColumnInfo {
                name: "result".into(),
                data_type: "string".into(),
                nullable: false,
            }],
            vec![vec![Some(Value::String(format!("{:?}", value)))]],
        ),
    }
}

pub(crate) fn redis_to_value(v: &redis::Value) -> Value {
    match v {
        redis::Value::Nil => Value::Null,
        redis::Value::Int(n) => Value::Integer(*n),
        redis::Value::BulkString(bytes) => {
            Value::String(String::from_utf8_lossy(bytes).to_string())
        }
        redis::Value::VerbatimString { text, .. } => Value::String(text.clone()),
        redis::Value::SimpleString(s) => Value::String(s.clone()),
        #[allow(deprecated)]
        redis::Value::Okay => Value::String("OK".into()),
        redis::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(|i| format!("{i:?}")).collect();
            Value::String(format!("[{}]", parts.join(", ")))
        }
        _ => Value::String(format!("{v:?}")),
    }
}

pub(crate) fn looks_like_hash(items: &[redis::Value]) -> bool {
    items.chunks(2).all(|pair| {
        matches!(
            &pair[0],
            redis::Value::BulkString(_) | redis::Value::SimpleString(_)
        )
    })
}
