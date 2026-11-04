//! Whole-key commands: `DEL` / `RENAME` / `EXPIRE` / `EXPIREAT`.

use super::ttl::apply_ttl_command;
use super::types::resolve_expire_at;
use super::types::resolve_ttl;
use redis::AsyncCommands;

pub async fn delete_keys<C>(conn: &mut C, keys: &[String]) -> Result<u64, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if keys.is_empty() {
        return Ok(0);
    }
    conn.del(keys).await.map_err(|e| e.to_string())
}

pub async fn rename_key<C>(conn: &mut C, key: &str, new_key: &str) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("RENAME")
        .arg(key)
        .arg(new_key)
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

pub async fn set_ttl<C>(conn: &mut C, key: &str, ttl_seconds: i64) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    apply_ttl_command(conn, key, resolve_ttl(ttl_seconds)?).await
}

/// Set absolute expiry via EXPIREAT (unix timestamp seconds).
pub async fn set_expire_at<C>(conn: &mut C, key: &str, expire_at: i64) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    apply_ttl_command(conn, key, resolve_expire_at(expire_at)?).await
}
