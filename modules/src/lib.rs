//! `datazen` owns feature-oriented domain vocabulary and application use cases.
//!
//! The crate intentionally has no dependency on a host, runtime, kernel,
//! `platform-api`, or Tauri. Hosts provide the repository implementations
//! declared by the enabled feature modules. The `ai` feature additionally
//! carries the AI provider adapters (merged 2026-10-26, D1.5): their outbound
//! HTTP client (`reqwest`) and the bottom-layer `datazen-infra` utilities are
//! deliberate, user-decided exceptions recorded in the refactor plan.

#[cfg(feature = "backup")]
pub mod backup;

#[cfg(feature = "connection_import")]
pub mod connection_import;

#[cfg(feature = "feature_error")]
pub mod feature_error;
#[cfg(feature = "feature_error")]
pub use feature_error as error;

#[cfg(feature = "services")]
pub mod services;

#[cfg(feature = "wapps")]
pub mod wapps;
#[cfg(feature = "wapps")]
pub use wapps as workspace_apps;

/// D6 item 5：隧道领域端口、known-hosts 规则与形态中立网络机制。
#[cfg(feature = "tunnel")]
pub mod tunnel;

#[cfg(feature = "export")]
pub mod export;
#[cfg(feature = "export")]
pub use export::file_job;

#[cfg(feature = "table-editing")]
pub use table_editing as data_browsing;

#[cfg(feature = "backup")]
pub use backup::{backup_gateway, restore_gateway, restore_job};

#[cfg(feature = "ai")]
pub mod ai;

#[cfg(feature = "connection")]
pub mod connection;

/// D3：执行域词汇（状态机/目标/产物）与应用边界 DTO；机制与编排后续切片迁入。
#[cfg(feature = "execution")]
pub mod execution;

/// D5：Job 运行时（domain/application/engine 三分；仓储 trait 在 domain，
/// 实现在宿主——§4.1 第 5 条）。
#[cfg(feature = "jobs")]
pub mod jobs;

#[cfg(feature = "named_views")]
pub mod named_views;

/// D6 item 4 切片一：MCP 业务规则（权限模式/连接白名单/表描述格式化）。
#[cfg(feature = "mcp")]
pub mod mcp;

/// D6 Monitor: grants, fixed targets, scheduling and notification rules.
#[cfg(feature = "monitor")]
pub mod monitor;

/// D5：数据迁移 package。三件套与共享算法在同一目录下独立 feature-gated。
#[cfg(any(
    feature = "common",
    feature = "schema_diff",
    feature = "data_sync",
    feature = "data_transfer"
))]
pub mod migration;


/// D6 item 1：查询历史与迁移运行历史（词汇/仓储端口/用例门面）。
#[cfg(feature = "history")]
pub mod history;

/// D6 item 3 第一切片 a：workflow 模型词汇（step/block/输出/错误策略/执行结果）。
#[cfg(feature = "workflow")]
pub mod workflow;

/// D6 item 2：Dashboard（仪表盘/组件/运行历史：词汇/端口/用例规则）。
#[cfg(feature = "dashboard")]
pub mod dashboard;

/// D4-d：表编辑写链路（ChangeSet/insert plan/行身份/写执行）。
#[cfg(feature = "table-editing")]
pub mod table_editing;

/// D4-c：表浏览读链路（浏览/分页/执行器）。
#[cfg(feature = "table-browsing")]
pub mod table_browsing;

/// D4：Schema 元数据（缓存机制已迁入；应用服务后续切片）。
#[cfg(feature = "schema")]
pub mod schema;

#[cfg(feature = "settings")]
pub mod settings;

/// D8：原 `datazen::application` 用例编排已归入 modules，宿主只负责装配。
#[cfg(feature = "application")]
pub mod application;

/// 跨模块基础词汇（ID/上下文/错误信封）。常驻编译，不设 feature 门
/// （§5.1：形成归属契约的最小 ID/引用值在 shared 唯一定义）。
pub mod shared;

// Stable crate-level conveniences for host adapters. Definitions remain in
// their owning vertical slices; these exports avoid recreating a horizontal
// platform-api namespace.
#[cfg(feature = "execution")]
pub use execution::*;
#[cfg(feature = "jobs")]
pub use jobs::JobRepository;
pub use shared::id::*;
pub use shared::{OwnerRef, PortError, RequestContext};

#[cfg(test)]
mod tests {
    #[test]
    fn the_modules_crate_has_a_real_root() {
        assert!(module_path!().ends_with("tests"));
    }
}
