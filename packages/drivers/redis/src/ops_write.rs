//! Binary-safe write helpers (kept out of the size-capped `ops.rs`).

use redis::AsyncCommands;

/// SET a string key from raw bytes (binary-safe). When `keep_ttl` is true, uses
/// `SET … KEEPTTL` so an existing expiry survives (Redis ≥ 6.0). Unlike
/// [`crate::ops::set_string_with_options`] this accepts arbitrary `Vec<u8>`
/// payloads (NUL bytes, invalid UTF-8, …) for the base64 write-back path (R9).
pub async fn set_string_bytes<C>(
    conn: &mut C,
    key: &str,
    bytes: &[u8],
    keep_ttl: bool,
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let mut cmd = redis::cmd("SET");
    cmd.arg(key).arg(bytes);
    if keep_ttl {
        cmd.arg("KEEPTTL");
    }
    cmd.query_async::<()>(conn).await.map_err(|e| e.to_string())
}
