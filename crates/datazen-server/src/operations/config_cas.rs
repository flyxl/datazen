//! 配置版本 CAS 写（WR-D6）：读方携带 expected version，写方
//! compare-and-swap，失败答版本冲突。
//!
//! ## 落点的已记录取舍
//!
//! persistence-model 的 `settings`（键值面）在服务端 v7 迁移里**没有表**
//! （桌面 settings 归桌面库；`crates/datazen-server/src/storage/migrations.rs` 的 v1..v7
//! 只有 connection_profile / jobs / audit / 认证授权 / tunnels 等管理面）。
//! 服务端当前唯一携带「配置版本」语义的配置面是 [`connection_profile`]：
//!
//! - `config_revision INTEGER NOT NULL CHECK (config_revision > 0)`；
//! - `driver_options_json TEXT NOT NULL DEFAULT '{}'`（配置正文）；
//! - `updated_at`（定宽 RFC3339 UTC 毫秒）。
//!
//! 因此本模块把 CAS 落在 `connection_profile` 的配置面上（v1 单组织下
//! 这正是「服务端配置」的实际载体）；`settings` 服务端表随 WR-G 的配置
//! 切片落地时，[`ProfileConfigCas`] 的方法签名（组织 + 实体 id + expected
//! revision + 新值）可直接复用到新表。
//!
//! ## 语义
//!
//! * [`ProfileConfigCas::compare_and_swap`]：单条
//!   `UPDATE ... WHERE config_revision = <expected>`——受 SQLite 单写者
//!   互斥保护，0 行受影响即「行不存在或版本过期」，回读区分两者；
//! * 冲突映射 [`ConfigCasFailure::RevisionConflict`] →
//!   [`ApiError::config_revision_mismatch`]（`configRevisionMismatch`，
//!   `status_for` 的 409 行）；行不存在 → `notFound`（404，防枚举同口径：
//!   越组织与真不存在不可区分）；
//! * 新值必须是 JSON 对象（驱动选项的既有词表），拒绝数组/标量混入。
//!
//! [`connection_profile`]: crate::storage::migrations

use std::sync::Arc;

use rusqlite::OptionalExtension;

use datazen::shared::error::ApiError;
use datazen::shared::id::Timestamp;

use crate::storage::{AdminStore, StorageError};

/// 一次配置读的快照（CAS 的比较基准）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileConfigSnapshot {
    pub organization_id: String,
    pub connection_id: String,
    pub config_revision: i64,
    pub driver_options_json: String,
    pub updated_at: String,
}

/// CAS 失败（端口层事实；wire 映射见 [`ConfigCasFailure::to_api_error`]）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigCasFailure {
    /// 行不存在（或越组织——不可区分，404）。
    #[error("profile config not found: {organization_id}/{connection_id}")]
    NotFound {
        organization_id: String,
        connection_id: String,
    },
    /// 版本冲突：`current` 是写方失败时读到的最新版本。
    #[error("config revision conflict: expected {expected}, current {current}")]
    RevisionConflict { expected: i64, current: i64 },
    /// 存储故障（503）。
    #[error(transparent)]
    Backend(#[from] StorageError),
}

impl ConfigCasFailure {
    pub fn to_api_error(&self) -> ApiError {
        match self {
            ConfigCasFailure::NotFound {
                organization_id,
                connection_id,
            } => ApiError::new(
                datazen::shared::error::ApiErrorCode::NotFound,
                format!("profile config not found: {organization_id}/{connection_id}"),
            ),
            ConfigCasFailure::RevisionConflict { expected, current } => ApiError::config_revision_mismatch(format!(
                "config was changed concurrently (expected revision {expected}, current {current}); reload and retry"
            )),
            ConfigCasFailure::Backend(source) => super::storage_error_to_api_error(source),
        }
    }
}

/// `connection_profile` 配置面的 CAS 写服务。
#[derive(Clone)]
pub struct ProfileConfigCas {
    store: Arc<AdminStore>,
}

impl ProfileConfigCas {
    pub fn new(store: Arc<AdminStore>) -> Self {
        Self { store }
    }

    /// 读当前配置快照（CAS 的 expected 来源）。
    pub fn read(
        &self,
        organization_id: &str,
        connection_id: &str,
    ) -> Result<Option<ProfileConfigSnapshot>, StorageError> {
        let conn = self.store.lock_for_adapter()?;
        conn.query_row(
            "SELECT config_revision, driver_options_json, updated_at
             FROM connection_profile
             WHERE organization_id = ?1 AND connection_id = ?2",
            [organization_id, connection_id],
            |row| {
                Ok(ProfileConfigSnapshot {
                    organization_id: organization_id.to_string(),
                    connection_id: connection_id.to_string(),
                    config_revision: row.get(0)?,
                    driver_options_json: row.get(1)?,
                    updated_at: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|source| StorageError::Query(format!("profile config read: {source}")))
    }

    /// compare-and-swap：`expected_config_revision` 匹配才写。
    ///
    /// 成功返回**新**快照（revision 已 +1）；0 行受影响时回读区分
    /// `NotFound` 与 `RevisionConflict`（带当前版本，供客户端直接重读）。
    pub fn compare_and_swap(
        &self,
        organization_id: &str,
        connection_id: &str,
        expected_config_revision: i64,
        next_driver_options: &serde_json::Value,
        now: Timestamp,
    ) -> Result<ProfileConfigSnapshot, ConfigCasFailure> {
        if expected_config_revision < 1 {
            return Err(ConfigCasFailure::RevisionConflict {
                expected: expected_config_revision,
                current: 0,
            });
        }
        if !next_driver_options.is_object() {
            return Err(ConfigCasFailure::Backend(StorageError::Query(
                "profile config: driver options must be a JSON object".into(),
            )));
        }
        let next_json = serde_json::to_string(next_driver_options).map_err(|source| {
            ConfigCasFailure::Backend(StorageError::Query(format!(
                "profile config serialize: {source}"
            )))
        })?;
        let now = canonical_now(&now)?;
        let conn = self.store.lock_for_adapter()?;
        let changed = conn
            .execute(
                "UPDATE connection_profile
                 SET driver_options_json = ?3, config_revision = config_revision + 1,
                     updated_at = ?4
                 WHERE organization_id = ?1 AND connection_id = ?2
                   AND config_revision = ?5",
                rusqlite::params![
                    organization_id,
                    connection_id,
                    next_json,
                    now,
                    expected_config_revision
                ],
            )
            .map_err(|source| {
                ConfigCasFailure::Backend(StorageError::Query(format!(
                    "profile config cas: {source}"
                )))
            })?;
        if changed == 0 {
            // 区分「行不存在」与「版本过期」：回读当前 revision。
            let current: Option<i64> = conn
                .query_row(
                    "SELECT config_revision FROM connection_profile
                     WHERE organization_id = ?1 AND connection_id = ?2",
                    [organization_id, connection_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|source| {
                    ConfigCasFailure::Backend(StorageError::Query(format!(
                        "profile config cas probe: {source}"
                    )))
                })?;
            return match current {
                Some(current) => Err(ConfigCasFailure::RevisionConflict {
                    expected: expected_config_revision,
                    current,
                }),
                None => Err(ConfigCasFailure::NotFound {
                    organization_id: organization_id.to_string(),
                    connection_id: connection_id.to_string(),
                }),
            };
        }
        Ok(ProfileConfigSnapshot {
            organization_id: organization_id.to_string(),
            connection_id: connection_id.to_string(),
            config_revision: expected_config_revision + 1,
            driver_options_json: next_json,
            updated_at: now,
        })
    }
}

/// 时刻规范化（`login_sessions::canonical_timestamp` 的同款口径）。
fn canonical_now(now: &Timestamp) -> Result<String, ConfigCasFailure> {
    let parsed = chrono::DateTime::parse_from_rfc3339(now.as_str()).map_err(|source| {
        ConfigCasFailure::Backend(StorageError::Query(format!(
            "profile config: bad timestamp: {source}"
        )))
    })?;
    Ok(parsed
        .with_timezone(&chrono::Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::AdminStore;

    fn store() -> Arc<AdminStore> {
        let store = AdminStore::open_in_memory().expect("store");
        store.migrate().expect("migrate");
        Arc::new(store)
    }

    fn seed_profile(store: &AdminStore, revision: i64) {
        let conn = store.lock_for_adapter().expect("lock");
        conn.execute(
            "INSERT INTO connection_profile (organization_id, connection_id, name, driver_id,
                                             config_revision, credential_revision,
                                             initial_namespace_json, created_at, updated_at)
             VALUES ('org-a', 'conn-1', 'n', 'postgres', ?1, 1, '[]', '2026-01-01T00:00:00.000Z',
                     '2026-01-01T00:00:00.000Z')",
            [revision],
        )
        .expect("seed");
    }

    /// CAS 成功：revision 前进、正文替换、updated_at 规范化。
    #[test]
    fn cas_writes_only_when_the_expected_revision_matches() {
        let store = store();
        seed_profile(&store, 3);
        let cas = ProfileConfigCas::new(store.clone());
        let next = serde_json::json!({"pool": {"max": 4}});
        let snapshot = cas
            .compare_and_swap(
                "org-a",
                "conn-1",
                3,
                &next,
                Timestamp::new("2026-01-02T03:04:05.678Z"),
            )
            .expect("cas");
        assert_eq!(snapshot.config_revision, 4);
        assert_eq!(snapshot.updated_at, "2026-01-02T03:04:05.678Z");
        let reread = cas.read("org-a", "conn-1").expect("read").expect("row");
        assert_eq!(reread.config_revision, 4);
        assert_eq!(reread.driver_options_json, r#"{"pool":{"max":4}}"#);
    }

    /// CAS 冲突：版本不匹配拒绝且不覆盖写，错误携带当前版本。
    #[test]
    fn a_stale_expected_revision_is_rejected_without_overwriting() {
        let store = store();
        seed_profile(&store, 7);
        let cas = ProfileConfigCas::new(store.clone());
        let error = cas
            .compare_and_swap(
                "org-a",
                "conn-1",
                5,
                &serde_json::json!({}),
                Timestamp::new("2026-01-01T00:00:00.000Z"),
            )
            .expect_err("conflict");
        match &error {
            ConfigCasFailure::RevisionConflict { expected, current } => {
                assert_eq!(*expected, 5);
                assert_eq!(*current, 7);
            }
            other => panic!("expected revision conflict, got {other:?}"),
        }
        let wire = error.to_api_error();
        assert_eq!(
            wire.code,
            datazen::shared::error::ApiErrorCode::ConfigRevisionMismatch
        );
        // 未覆盖写：正文与版本都保持原样。
        let reread = cas.read("org-a", "conn-1").expect("read").expect("row");
        assert_eq!(reread.config_revision, 7);
        assert_eq!(reread.driver_options_json, "{}");
    }

    /// 行不存在（含越组织）：`NotFound` 而不是冲突。
    #[test]
    fn an_unknown_row_is_not_found_not_a_conflict() {
        let store = store();
        seed_profile(&store, 1);
        let cas = ProfileConfigCas::new(store);
        let error = cas
            .compare_and_swap(
                "org-b",
                "conn-1",
                1,
                &serde_json::json!({}),
                Timestamp::new("2026-01-01T00:00:00.000Z"),
            )
            .expect_err("not found");
        assert!(matches!(error, ConfigCasFailure::NotFound { .. }));
        assert_eq!(
            error.to_api_error().code,
            datazen::shared::error::ApiErrorCode::NotFound
        );
    }
}
