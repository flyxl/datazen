//! RedisJSON helpers (`JSON.GET` / `JSON.SET` / `JSON.DEL`) and module probe.

use redis::aio::ConnectionLike;
use redis::AsyncCommands;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonGetResult {
    pub value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonDelResult {
    pub deleted: u64,
}

/// Returns true when `MODULE LIST` includes ReJSON / RedisJSON.
pub fn has_redis_json(modules: &[String]) -> bool {
    modules.iter().any(|name| {
        let lower = name.to_ascii_lowercase();
        lower == "rejson" || lower == "redisjson"
    })
}

fn normalize_key(key: &str) -> Result<&str, String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("key is required".into());
    }
    Ok(trimmed)
}

fn normalize_path(path: &str) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("path is required".into());
    }
    Ok(trimmed.to_string())
}

fn value_to_string(v: &redis::Value) -> String {
    match v {
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).into(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::VerbatimString { text, .. } => text.clone(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::Okay => "OK".into(),
        redis::Value::Nil => String::new(),
        other => format!("{other:?}"),
    }
}

fn parse_json_reply(raw: &str) -> Result<Option<serde_json::Value>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    serde_json::from_str(trimmed)
        .map(Some)
        .map_err(|e| format!("invalid JSON reply: {e}"))
}

/// `JSON.GET key $` returns `[doc]` — unwrap single-element arrays for the root path.
pub fn unwrap_json_get_root(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) if items.len() == 1 => {
            items.into_iter().next().unwrap_or(serde_json::Value::Null)
        }
        other => other,
    }
}

pub async fn json_get<C>(conn: &mut C, key: &str, path: &str) -> Result<JsonGetResult, String>
where
    C: AsyncCommands + ConnectionLike + Send,
{
    let key = normalize_key(key)?;
    let path = normalize_path(path)?;

    let raw: redis::Value = redis::cmd("JSON.GET")
        .arg(key)
        .arg(&path)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;

    if matches!(raw, redis::Value::Nil) {
        return Ok(JsonGetResult { value: None });
    }

    let text = value_to_string(&raw);
    let parsed = parse_json_reply(&text)?;
    let value = parsed.map(|v| {
        if path == "$" {
            unwrap_json_get_root(v)
        } else {
            v
        }
    });
    Ok(JsonGetResult { value })
}

pub async fn json_set<C>(
    conn: &mut C,
    key: &str,
    path: &str,
    value: &str,
) -> Result<(), String>
where
    C: AsyncCommands + ConnectionLike + Send,
{
    let key = normalize_key(key)?;
    let path = normalize_path(path)?;
    let value = value.trim();
    if value.is_empty() {
        return Err("value is required".into());
    }
    serde_json::from_str::<serde_json::Value>(value)
        .map_err(|e| format!("invalid JSON value: {e}"))?;

    redis::cmd("JSON.SET")
        .arg(key)
        .arg(&path)
        .arg(value)
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

pub async fn json_del<C>(conn: &mut C, key: &str, path: &str) -> Result<JsonDelResult, String>
where
    C: AsyncCommands + ConnectionLike + Send,
{
    let key = normalize_key(key)?;
    let path = normalize_path(path)?;

    let deleted: i64 = redis::cmd("JSON.DEL")
        .arg(key)
        .arg(&path)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;

    Ok(JsonDelResult {
        deleted: deleted.max(0) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_redis_json_detects_rejson_and_redisjson() {
        assert!(has_redis_json(&["ReJSON".into()]));
        assert!(has_redis_json(&["redisjson".into()]));
        assert!(has_redis_json(&["RedisJSON".into(), "other".into()]));
    }

    #[test]
    fn has_redis_json_false_when_absent() {
        assert!(!has_redis_json(&[]));
        assert!(!has_redis_json(&["search".into(), "timeseries".into()]));
    }

    #[test]
    fn unwrap_json_get_root_unwraps_single_element_array() {
        let wrapped = serde_json::json!([{"a": 1}]);
        let unwrapped = unwrap_json_get_root(wrapped);
        assert_eq!(unwrapped, serde_json::json!({"a": 1}));
    }

    #[test]
    fn unwrap_json_get_root_leaves_other_values() {
        let scalar = serde_json::json!("hello");
        assert_eq!(unwrap_json_get_root(scalar.clone()), scalar);
        let multi = serde_json::json!([1, 2]);
        assert_eq!(unwrap_json_get_root(multi.clone()), multi);
    }

    #[test]
    fn normalize_path_rejects_empty() {
        assert!(normalize_path("").is_err());
        assert!(normalize_path("  ").is_err());
        assert_eq!(normalize_path(" $.a ").unwrap(), "$.a");
    }

    #[test]
    fn normalize_key_rejects_empty() {
        assert!(normalize_key("").is_err());
        assert_eq!(normalize_key(" mykey ").unwrap(), "mykey");
    }

    #[test]
    fn parse_json_reply_handles_null_and_objects() {
        assert_eq!(
            parse_json_reply("null").unwrap(),
            Some(serde_json::Value::Null)
        );
        assert_eq!(
            parse_json_reply(r#"{"x":1}"#).unwrap(),
            Some(serde_json::json!({"x": 1}))
        );
        assert_eq!(parse_json_reply("").unwrap(), None);
    }
}
