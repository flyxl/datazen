//! Redis value parsing and preview helpers shared by the driver and query UI.

use redis::FromRedisValue;

/// Safely extract a string from a redis::Value, handling non-UTF-8 bytes via lossy conversion.
pub(crate) fn value_to_string(v: &redis::Value) -> String {
    match v {
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::VerbatimString { text, .. } => text.clone(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::Okay => "OK".into(),
        _ => format!("{:?}", v),
    }
}

/// Parse a SCAN result (Array of [cursor, Array of keys]) into (next_cursor, Vec<String>).
/// Tolerates non-UTF-8 keys by using lossy conversion.
pub(crate) fn parse_scan_result(v: &redis::Value) -> (u64, Vec<String>) {
    match v {
        redis::Value::Array(items) if items.len() >= 2 => {
            let next_cursor: u64 = match &items[0] {
                redis::Value::BulkString(b) => String::from_utf8_lossy(b).parse().unwrap_or(0),
                redis::Value::Int(i) => *i as u64,
                _ => 0,
            };
            let keys = match &items[1] {
                redis::Value::Array(arr) => arr.iter().map(|v| value_to_string(v)).collect(),
                _ => vec![],
            };
            (next_cursor, keys)
        }
        _ => (0, vec![]),
    }
}

pub(crate) fn preview_value_to_string(v: &redis::Value, key_type: &str) -> String {
    if key_type == "zset" {
        if let Ok(parts) = Vec::<String>::from_redis_value(v) {
            if !parts.is_empty() {
                let member = &parts[0];
                return if parts.len() >= 2 {
                    format!("{member} (score: {})", parts[1])
                } else {
                    member.clone()
                };
            }
        }
    }
    if key_type == "stream" {
        if let Some(s) = stream_preview_from_xrev(v) {
            return s;
        }
    }
    match v {
        redis::Value::Nil => String::new(),
        redis::Value::Array(a) if a.is_empty() => String::new(),
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
        redis::Value::VerbatimString { text, .. } => text.clone(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::Map(pairs) if key_type == "hash" => {
            let n = 2.min(pairs.len());
            let mut s = "(".to_string();
            for (i, (fk, fv)) in pairs.iter().take(n).enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&value_field_for_preview(fk));
                s.push_str(": ");
                s.push_str(&value_field_for_preview(fv));
            }
            s.push(')');
            if pairs.len() > 2 {
                s.push_str(" …");
            }
            s
        }
        redis::Value::Array(items) if key_type == "hash" && items.len() == 2 => {
            let fields = match &items[1] {
                redis::Value::Array(inner) => inner,
                _ => return format!("{v:?}"),
            };
            if fields.is_empty() {
                return String::new();
            }
            let n = 4.min(fields.len());
            let mut s = "(".to_string();
            for i in (0..n).step_by(2) {
                if i + 1 < fields.len() {
                    let f = value_field_for_preview(&fields[i]);
                    let val = value_field_for_preview(&fields[i + 1]);
                    s.push_str(&f);
                    s.push_str(": ");
                    s.push_str(&val);
                    if i + 2 < n {
                        s.push_str(", ");
                    }
                }
            }
            s.push(')');
            if fields.len() > 4 {
                s.push_str(" …");
            }
            s
        }
        redis::Value::Array(items) if key_type == "hash" && !items.is_empty() => {
            let n = 4.min(items.len());
            let mut s = "(".to_string();
            for i in (0..n).step_by(2) {
                if i + 1 < items.len() {
                    let f = value_field_for_preview(&items[i]);
                    let val = value_field_for_preview(&items[i + 1]);
                    s.push_str(&f);
                    s.push_str(": ");
                    s.push_str(&val);
                    if i + 2 < n {
                        s.push_str(", ");
                    }
                }
            }
            s.push(')');
            if items.len() > 4 {
                s.push_str(" …");
            }
            s
        }
        redis::Value::Array(items) if !items.is_empty() => format!("{items:?}"),
        redis::Value::SimpleString(s) if s == "PONG" => String::new(),
        redis::Value::Okay => String::new(),
        _ => format!("{v:?}"),
    }
}

pub(crate) fn value_field_for_preview(v: &redis::Value) -> String {
    match v {
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
        redis::Value::VerbatimString { text, .. } => text.clone(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::Okay => "OK".into(),
        _ => format!("{v:?}"),
    }
}

/// `XREVRANGE` with COUNT 1: `[[id, [field, val, ...]]]`
pub(crate) fn stream_preview_from_xrev(v: &redis::Value) -> Option<String> {
    let a = match v {
        redis::Value::Array(x) => x,
        _ => return None,
    };
    if a.is_empty() {
        return Some(String::new());
    }
    let id = value_field_for_preview(&a[0]);
    if a.len() < 2 {
        return Some(id);
    }
    let rest = match &a[1] {
        redis::Value::Array(fields) if !fields.is_empty() => {
            let mut s = id + ": ";
            for f in fields.iter().take(2) {
                s.push_str(&value_field_for_preview(f));
            }
            s
        }
        other => format!("{id}: {other:?}"),
    };
    Some(rest)
}

pub(crate) fn stream_entry_to_json(v: &redis::Value) -> Result<serde_json::Value, ()> {
    let a = match v {
        redis::Value::Array(x) if !x.is_empty() => x,
        _ => return Err(()),
    };
    let id = value_field_for_preview(&a[0]);
    if a.len() < 2 {
        return Ok(serde_json::json!({ "id": id, "fields": {} }));
    }
    let mut map = serde_json::Map::new();
    if let redis::Value::Array(fields) = &a[1] {
        for pair in fields.chunks(2) {
            if pair.len() == 2 {
                let k = value_field_for_preview(&pair[0]);
                let val = value_field_for_preview(&pair[1]);
                map.insert(k, serde_json::Value::String(val));
            }
        }
    }
    Ok(serde_json::json!({ "id": id, "fields": serde_json::Value::Object(map) }))
}

pub(crate) fn truncate_preview(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}
