//! `apply_ttl_command` — the one place a resolved `TtlCommand` reaches the wire.

use super::types::TtlCommand;
use redis::AsyncCommands;

pub(crate) async fn apply_ttl_command<C>(
    conn: &mut C,
    key: &str,
    cmd: TtlCommand,
) -> Result<(), String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    match cmd {
        TtlCommand::Persist => conn
            .persist::<_, i64>(key)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        TtlCommand::Expire(secs) => conn
            .expire::<_, i64>(key, secs as i64)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
        TtlCommand::ExpireAt(ts) => redis::cmd("EXPIREAT")
            .arg(key)
            .arg(ts)
            .query_async::<i64>(conn)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string()),
    }
}
