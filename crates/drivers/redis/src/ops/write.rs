//! Binary-safe string write helpers (kept out of the size-capped `ops.rs`).
//!
//! This is the single home of the TTL-preserving save semantics the value
//! viewer relies on (PRD §3.3 bottom bar): saving a string key must not silently
//! turn a temporary cache entry into a permanent one.

use redis::AsyncCommands;

/// What a string write actually did about the key's expiry.
///
/// `keepTtlFallback` is the assertion the UI and the driver tests gate on: it is
/// only true when the server rejected `SET … KEEPTTL` and the `PTTL` + `PX`
/// rescue path ran.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SetStringOutcome {
    pub ok: bool,
    pub keep_ttl: bool,
    pub keep_ttl_fallback: bool,
}

/// Resolve the requested keep-TTL policy from a command input object.
///
/// `None` (absent key, or a non-boolean value) means "caller expressed no
/// preference", which resolves to **keep the TTL**: dropping an expiry is a
/// data-affecting surprise, keeping one is not. An explicit `false` still
/// clears the TTL, which is what the TTL pill editor's "no expiry" state needs.
pub fn keep_ttl_policy(input: &serde_json::Value) -> Option<bool> {
    input
        .get("keepTtl")
        .or_else(|| input.get("keep_ttl"))
        .and_then(serde_json::Value::as_bool)
}

/// `SET` a string key from raw bytes with the driver's TTL policy (W3-C).
///
/// * `keep_ttl == Some(false)` — plain `SET`, which clears any expiry.
/// * `keep_ttl == None | Some(true)` — `SET … KEEPTTL`, i.e. the default for
///   every caller that does not pass the flag (GUI, Workflow steps, MCP).
///
/// When the server rejects `KEEPTTL` (Redis < 6.0, or a proxy that strips the
/// keyword) the write is re-issued as `PTTL` + `SET … PX <remaining>` and the
/// outcome reports `keep_ttl_fallback: true`. A non-positive `PTTL` means
/// "persistent" (`-1`) or "key gone" (`-2`) and must never become an expiry.
/// Every other `SET` failure is thrown verbatim — see
/// [`is_keepttl_keyword_rejection`].
pub async fn set_string_with_ttl_policy<C>(
    conn: &mut C,
    key: &str,
    value: &[u8],
    keep_ttl: Option<bool>,
) -> Result<SetStringOutcome, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    if keep_ttl == Some(false) {
        set_plain(conn, key, value).await?;
        return Ok(SetStringOutcome {
            ok: true,
            keep_ttl: false,
            keep_ttl_fallback: false,
        });
    }

    match set_keeping_ttl(conn, key, value).await {
        Ok(()) => Ok(SetStringOutcome {
            ok: true,
            keep_ttl: true,
            keep_ttl_fallback: false,
        }),
        // Not the keyword being refused (transport, ACL, WRONGTYPE, …): C-3
        // keeps those a generic IPC error — no PTTL probe, no second write,
        // and no claim that this server lacks KEEPTTL.
        Err(rejected) if !is_keepttl_keyword_rejection(&rejected) => Err(rejected),
        Err(rejected) => {
            let remaining = match pttl_millis(conn, key).await {
                Ok(ms) => ms,
                // The rescue path is unavailable too (old server without PTTL,
                // ACL refusal, …): surface the original KEEPTTL rejection, which
                // is the error the user can actually act on.
                Err(_) => return Err(rejected),
            };
            let retried = if remaining > 0 {
                set_with_expiry(conn, key, value, remaining).await
            } else {
                set_plain(conn, key, value).await
            };
            retried.map_err(|retry_error| {
                format!("{rejected} (PTTL+PX fallback also failed: {retry_error})")
            })?;
            Ok(SetStringOutcome {
                ok: true,
                keep_ttl: true,
                keep_ttl_fallback: true,
            })
        }
    }
}

/// Did the server reject the `KEEPTTL` **keyword itself** (C-4)?
///
/// Only that refusal (Redis < 6.0, or a proxy stripping the keyword) may arm
/// the `PTTL` + `PX` rescue. The two accepted shapes are what a server emits
/// when *this* command's argument list is refused: it echoes `KEEPTTL` back,
/// or reports an unknown option for `SET`. Anything else — `WRONGTYPE`,
/// `READONLY`, ACL, transport — is thrown verbatim with no fallback claimed.
fn is_keepttl_keyword_rejection(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("keepttl") || (m.contains("unknown option") && m.contains("'set'"))
}

/// Remaining lifetime in milliseconds; negative means "no expiry to preserve".
async fn pttl_millis<C>(conn: &mut C, key: &str) -> Result<i64, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("PTTL")
        .arg(key)
        .query_async::<i64>(conn)
        .await
        .map_err(|e| e.to_string())
}

async fn set_plain<C>(conn: &mut C, key: &str, value: &[u8]) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("SET")
        .arg(key)
        .arg(value)
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

async fn set_keeping_ttl<C>(conn: &mut C, key: &str, value: &[u8]) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("SET")
        .arg(key)
        .arg(value)
        .arg("KEEPTTL")
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

async fn set_with_expiry<C>(
    conn: &mut C,
    key: &str,
    value: &[u8],
    remaining_ms: i64,
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("SET")
        .arg(key)
        .arg(value)
        .arg("PX")
        .arg(remaining_ms)
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
