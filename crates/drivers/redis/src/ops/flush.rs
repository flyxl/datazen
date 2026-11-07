//! `FLUSHDB` / `FLUSHALL` and the opt-in gate that guards them.

use redis::AsyncCommands;
use std::sync::atomic::{AtomicBool, Ordering};

pub async fn flush_db<C>(conn: &mut C) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("FLUSHDB")
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

pub async fn flush_all<C>(conn: &mut C) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    redis::cmd("FLUSHALL")
        .query_async::<()>(conn)
        .await
        .map_err(|e| e.to_string())
}

/// Host-synced mirror of `AppSettings.pluginSettings.redis.allowFlush`.
static SETTINGS_ALLOW_FLUSH: AtomicBool = AtomicBool::new(false);

/// Called by the host when settings are loaded or saved (feature `driver-redis`).
pub fn set_settings_allow_flush(allow: bool) {
    SETTINGS_ALLOW_FLUSH.store(allow, Ordering::Relaxed);
}

/// Whether settings currently allow flush (for tests / diagnostics).
pub fn settings_allow_flush() -> bool {
    SETTINGS_ALLOW_FLUSH.load(Ordering::Relaxed)
}

/// Reject destructive flush unless **settings** allow it.
/// The IPC `allow_flush` flag is treated as an additional UI confirmation and
/// cannot bypass a disabled settings toggle.
pub fn ensure_flush_allowed(client_allow_flush: bool) -> Result<(), String> {
    if !SETTINGS_ALLOW_FLUSH.load(Ordering::Relaxed) {
        return Err("Flush is disabled in Redis extension settings".into());
    }
    if !client_allow_flush {
        return Err("Flush is disabled in Redis extension settings".into());
    }
    Ok(())
}
