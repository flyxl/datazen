//! Data Sync 资产保存的域校验（W8 刀3e-3）。
//!
//! 桌面对照 `commands/sync/mod.rs` 的 `save_sync_profile`：①共享层
//! `SyncProfile::validate`（形状校验）②源/目标连接在场性核对（档案引用
//! 的连接已从目录消失时拒绝——存储行不悬挂）③`updated_at` 推进。存储
//! 半边（v15 `sync_profiles` 行落取）在 [`ServerSyncProfiles`]，本模块
//! 只补存储不做的那半。
//!
//! 任务面（`save_sync_task_direct`）是桌面专属命令：web 任务列表由执行
//! 面自维护（3e-4 的执行/断点续传落账），web 客户端面只有读 + 删（与
//! legacy 命令轨的收录一致）。

use datazen::migration::data_sync::profile::SyncProfile;
use datazen::shared::scope::OrganizationScope;

use crate::storage::schema_diff_profiles::connection_exists;
use crate::storage::sync_assets::{ServerSyncProfiles, SyncAssetStoreError};
use crate::storage::AdminStore;

/// 档案保存的域错误（路由层按桶映射 wire 码）。
#[derive(Debug, thiserror::Error)]
pub enum SyncAssetError {
    /// 形状校验或连接在场性核对失败（→ 400 `invalidArgument`，桌面
    /// `CommandError::Validation` 的服务端对应物）。
    #[error("{0}")]
    Validation(String),
    /// 存储故障（→ 503 `serviceUnavailable`）。
    #[error("{0}")]
    Unavailable(String),
}

fn unavailable(error: SyncAssetStoreError) -> SyncAssetError {
    SyncAssetError::Unavailable(error.to_string())
}

/// `saveSyncProfile`：校验 → 连接在场性 → `updated_at` 推进 → 落行。
/// 保存即覆盖（upsert；桌面同语义），成功应答 204。
pub async fn save_sync_profile(
    store: &std::sync::Arc<AdminStore>,
    profiles: &ServerSyncProfiles,
    scope: &OrganizationScope,
    mut profile: SyncProfile,
) -> Result<(), SyncAssetError> {
    profile
        .validate()
        .map_err(|message| SyncAssetError::Validation(message))?;
    for connection_id in [&profile.source_connection_id, &profile.target_connection_id] {
        let present = connection_exists(store, scope, connection_id)
            .await
            .map_err(|error| SyncAssetError::Unavailable(error.to_string()))?;
        if !present {
            return Err(SyncAssetError::Validation(format!(
                "sync profile {} connection no longer exists",
                if connection_id == &profile.source_connection_id {
                    "source"
                } else {
                    "target"
                }
            )));
        }
    }
    profile.updated_at = chrono::Utc::now();
    profiles.save(scope, profile).await.map_err(unavailable)?;
    Ok(())
}
