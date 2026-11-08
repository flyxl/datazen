use datazen::ai::AiProviderConfig;
use datazen_driver_api::{ConnectionConfig, SavedTunnel};

use super::settings::AppSettings;

// D6 第一切片：查询历史条目的唯一定义在 `datazen::history`
// （domain::query），本模块只再导出，序列化形状（camelCase）钉在模块单测。
pub use datazen::history::QueryHistoryEntry;

// W8 刀3a：查询收藏的唯一定义同样归位 `datazen::history`
// （domain::favorite；桌面文件形与服务端表形共用一条词汇），本模块只再
// 导出——serde camelCase 形状钉在共享模块单测，桌面消费点零改动。
pub use datazen::history::FavoriteQuery;

/* FavoriteQuery 的定义本体已上移共享层；下方注释保留原文档要点：
///
/// A saved SQL favorite, backed by one `.sql` file under the favorites root.
///
/// The `id` is the file's stem — a ULID the app mints, never a title-derived
/// name, so renaming a favorite never rewrites a file (see plan §2.6.2). */

// （FavoriteQuery 定义本体已上移共享层 `datazen::history::domain::favorite`，
// 见文件头再导出；桌面文件形与服务端表形共用一条词汇。）

// W8 刀3e-2：同步任务（断点/续传状态）的定义本体上移共享层
// `datazen::migration::data_sync::task_checkpoint`（StoredSyncTask——与域内
// SyncTask 任务定义区分；桌面文件形与服务端 v15 表形共用一条词汇），本
// 模块只再导出，序列化形状（camelCase + 遗留缺省）钉在共享模块单测。
pub use datazen::migration::data_sync::task_checkpoint::StoredSyncTask as SyncTask;

#[derive(Default)]
pub(crate) struct StoreCache {
    pub(super) connections: Vec<ConnectionConfig>,
    pub(super) platform_profiles:
        std::collections::BTreeMap<String, super::platform_profiles::ProfileMetadata>,
    /// Independently stored tunnel definitions (`tunnels.json`).
    pub(super) tunnels: Vec<SavedTunnel>,
    pub(super) groups: Vec<String>,
    pub(super) settings: AppSettings,
    /// Lazy: loaded on first sync / AI access.
    pub(super) sync_tasks: Vec<SyncTask>,
    pub(super) sync_tasks_loaded: bool,
    /// Lazy: loaded on first Data Sync profile access.
    pub(super) sync_profiles: Vec<crate::data_sync::SyncProfile>,
    pub(super) sync_profiles_loaded: bool,
    /// Lazy: loaded on first Data Transfer profile access.
    pub(super) transfer_profiles: Vec<crate::data_transfer::TransferProfile>,
    pub(super) transfer_profiles_loaded: bool,
    /// Lazy: loaded on first Schema Diff profile access.
    pub(super) schema_diff_profiles: Vec<crate::schema_diff::SchemaDiffProfile>,
    pub(super) schema_diff_profiles_loaded: bool,
    /// Lazy: loaded on first named-view access (`named_views.json`).
    pub(super) named_views: Vec<datazen::named_views::NamedView>,
    pub(super) named_views_loaded: bool,
    pub(super) ai_config: Option<AiProviderConfig>,
    pub(super) ai_settings_config: Option<datazen::ai::AiSettingsConfig>,
    pub(super) ai_config_loaded: bool,
}
