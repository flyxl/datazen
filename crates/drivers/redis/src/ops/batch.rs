//! Bulk operations over a pattern or key list: delete-by-pattern, set-TTL, rename-prefix.

use super::keys::rename_key;
use super::scan::scan_matching_keys;
use super::types::plan_rename_prefix;
use super::types::resolve_ttl;
use super::types::BatchDeleteResult;
use super::types::BatchRenameResult;
use super::types::BatchSetTtlResult;
use super::types::KeyError;
use super::types::TtlCommand;
use redis::AsyncCommands;

pub async fn batch_delete_pattern<C>(
    conn: &mut C,
    pattern: &str,
) -> Result<BatchDeleteResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let keys = scan_matching_keys(conn, pattern).await?;
    let mut deleted = 0u64;
    let mut errors = Vec::new();
    for key in keys {
        match conn.del::<_, u64>(&key).await {
            Ok(n) => deleted += n,
            Err(e) => errors.push(KeyError {
                key,
                error: e.to_string(),
            }),
        }
    }
    Ok(BatchDeleteResult { deleted, errors })
}

pub async fn batch_set_ttl<C>(
    conn: &mut C,
    keys: &[String],
    ttl_seconds: i64,
) -> Result<BatchSetTtlResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let cmd = resolve_ttl(ttl_seconds)?;
    let mut updated = 0u64;
    let mut errors = Vec::new();
    for key in keys {
        let result = match cmd {
            TtlCommand::Persist => conn.persist::<_, i64>(key).await,
            TtlCommand::Expire(secs) => conn.expire::<_, i64>(key, secs as i64).await,
            TtlCommand::ExpireAt(ts) => {
                redis::cmd("EXPIREAT")
                    .arg(key)
                    .arg(ts)
                    .query_async::<i64>(conn)
                    .await
            }
        };
        match result {
            Ok(1) => updated += 1,
            Ok(_) => errors.push(KeyError {
                key: key.clone(),
                error: "key does not exist".into(),
            }),
            Err(e) => errors.push(KeyError {
                key: key.clone(),
                error: e.to_string(),
            }),
        }
    }
    Ok(BatchSetTtlResult { updated, errors })
}

pub async fn batch_rename_prefix<C>(
    conn: &mut C,
    old_prefix: &str,
    new_prefix: &str,
    keys: Option<Vec<String>>,
) -> Result<BatchRenameResult, String>
where
    C: AsyncCommands + redis::aio::ConnectionLike + Send,
{
    let source_keys = match keys {
        Some(k) => k,
        None => scan_matching_keys(conn, &format!("{old_prefix}*")).await?,
    };
    let planned = plan_rename_prefix(old_prefix, new_prefix, &source_keys);
    let mut renamed = 0u64;
    let mut errors = Vec::new();
    for (old_key, new_key) in planned {
        match rename_key(conn, &old_key, &new_key).await {
            Ok(()) => renamed += 1,
            Err(e) => errors.push(KeyError {
                key: old_key,
                error: e,
            }),
        }
    }
    Ok(BatchRenameResult { renamed, errors })
}
