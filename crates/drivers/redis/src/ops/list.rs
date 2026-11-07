//! List commands: `LPUSH` / `LSET` / `LPOP` / `LINDEX` / `LREM` / `LRANGE`.

use super::parse::parse_string_array;
use redis::AsyncCommands;

pub async fn list_push<C>(
    conn: &mut C,
    key: &str,
    side: &str,
    values: &[String],
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if values.is_empty() {
        return Ok(());
    }
    match side {
        "left" => conn
            .lpush::<_, _, i64>(key, values)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "right" => conn
            .rpush::<_, _, i64>(key, values)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        _ => Err(format!(
            "invalid side: {side} (expected \"left\" or \"right\")"
        )),
    }
}

pub async fn list_set<C>(conn: &mut C, key: &str, index: i64, value: &str) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("LSET")
        .arg(key)
        .arg(index)
        .arg(value)
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_pop<C>(conn: &mut C, key: &str, side: &str) -> Result<Option<String>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let raw: redis::Value = match side {
        "left" => redis::cmd("LPOP").arg(key).query_async(conn).await,
        "right" => redis::cmd("RPOP").arg(key).query_async(conn).await,
        _ => {
            return Err(format!(
                "invalid side: {side} (expected \"left\" or \"right\")"
            ))
        }
    }
    .map_err(|e| e.to_string())?;
    Ok(match raw {
        redis::Value::Nil => None,
        redis::Value::BulkString(b) => Some(String::from_utf8_lossy(&b).into()),
        redis::Value::SimpleString(s) => Some(s),
        other => Some(format!("{other:?}")),
    })
}

/// LINDEX: get the element at `index` in the list stored at `key`.
pub async fn list_index<C>(conn: &mut C, key: &str, index: i64) -> Result<Option<String>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let raw: redis::Value = redis::cmd("LINDEX")
        .arg(key)
        .arg(index)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    Ok(match raw {
        redis::Value::Nil => None,
        redis::Value::BulkString(b) => Some(String::from_utf8_lossy(&b).into()),
        redis::Value::SimpleString(s) => Some(s),
        other => Some(format!("{other:?}")),
    })
}

/// LREM: remove `count` occurrences of `value` from the list stored at `key`.
/// count > 0: remove first `count` occurrences; count < 0: remove last `count`; count = 0: remove all.
pub async fn list_rem<C>(conn: &mut C, key: &str, count: i64, value: &str) -> Result<i64, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("LREM")
        .arg(key)
        .arg(count)
        .arg(value)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())
}

/// LRANGE wrapper: returns elements from start to stop (inclusive).
pub async fn list_range<C>(
    conn: &mut C,
    key: &str,
    start: i64,
    stop: i64,
) -> Result<Vec<String>, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let raw: redis::Value = redis::cmd("LRANGE")
        .arg(key)
        .arg(start)
        .arg(stop)
        .query_async(conn)
        .await
        .map_err(|e| e.to_string())?;
    parse_string_array(&raw)
}
