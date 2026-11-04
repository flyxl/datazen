//! 权限版本（`permission_versions`）：每组织单调只增的授权时钟。
//!
//! 规则（team-server §6.3 / persistence-model §3.10）：
//!
//! - 组织持有一个单调递增的 `permission_version`；成员关系、角色、ACL、
//!   委托**任一变更即自增**，且必须与实际授权变更**同事务**提交——仅增加
//!   版本不构成撤权，仅变更不递增版本则缓存分区键（`policyIsolationKey`）
//!   不会失效。
//! - 版本只增不减、**永不删除**；`bump` 用 SQLite 的 UPSERT + RETURNING
//!   原子完成「首建为 2、其后递增」。
//! - 会话行建立时快照该版本（`login_sessions.permission_version`），排队
//!   中的执行派发前比对快照（§6.3 的 CM-39 路径）。
//!
//! helper 是 `pub(crate)`：版本递增只允许出现在 [`super::PolicyService`]
//! 的同事务管理用例里，不暴露给散点调用。

use rusqlite::{params, Transaction};

use crate::storage::StorageError;

/// 读取当前版本；无行时返回初始值 1（§3.10：`DEFAULT 1`）。
pub(crate) fn current_tx(tx: &Transaction<'_>, organization_id: &str) -> Result<i64, StorageError> {
    tx.query_row(
        "SELECT permission_version FROM permission_versions WHERE organization_id = ?1",
        [organization_id],
        |row| row.get(0),
    )
    .map(Some)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
    .map_err(|source| StorageError::Query(format!("permission_versions read: {source}")))
    .map(|value| value.unwrap_or(1))
}

/// 递增版本并返回新值（与授权变更同事务调用——顺序不裁决，事务保证
/// 原子性）。首建行落 2（首版基线 1 + 第一次变更）。
pub(crate) fn bump_tx(
    tx: &Transaction<'_>,
    organization_id: &str,
    now: &str,
) -> Result<i64, StorageError> {
    tx.query_row(
        "INSERT INTO permission_versions (organization_id, permission_version, updated_at)
         VALUES (?1, 2, ?2)
         ON CONFLICT (organization_id)
         DO UPDATE SET permission_version = permission_version + 1, updated_at = excluded.updated_at
         RETURNING permission_version",
        params![organization_id, now],
        |row| row.get(0),
    )
    .map_err(|source| StorageError::Query(format!("permission_versions bump: {source}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::AdminStore;

    /// 单调只增：无行读出 1，bump 序列 2 → 3 → …，跨组织互不干扰。
    #[test]
    fn versions_are_monotonic_per_organization() {
        let store = AdminStore::open_in_memory().expect("store");
        store.migrate().expect("migrate");

        // 事务（与其连接互斥锁）在同一作用域内存活，commit 前外部不可见。
        let mut conn = store.lock_for_adapter().expect("lock");
        let tx = conn.transaction().expect("transaction");
        assert_eq!(current_tx(&tx, "org-a").expect("read"), 1, "无行时为初始 1");
        assert_eq!(
            bump_tx(&tx, "org-a", "2026-01-01T00:00:00.000Z").expect("bump"),
            2
        );
        assert_eq!(
            bump_tx(&tx, "org-a", "2026-01-01T00:01:00.000Z").expect("bump"),
            3
        );
        assert_eq!(current_tx(&tx, "org-a").expect("read"), 3);
        // 跨组织互不干扰（每组织一行）。
        assert_eq!(
            bump_tx(&tx, "org-b", "2026-01-01T00:02:00.000Z").expect("bump"),
            2
        );
        assert_eq!(current_tx(&tx, "org-b").expect("read"), 2);
        assert_eq!(current_tx(&tx, "org-a").expect("read"), 3);
        tx.commit().expect("commit");
    }
}
