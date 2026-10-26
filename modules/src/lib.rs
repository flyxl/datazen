//! `datazen-modules` owns feature-oriented domain vocabulary and application use cases.
//!
//! The crate intentionally has no dependency on a host, runtime, kernel,
//! `platform-api`, or Tauri. Hosts provide the repository implementations
//! declared by the enabled feature modules. The `ai` feature additionally
//! carries the AI provider adapters (merged 2026-10-26, D1.5): their outbound
//! HTTP client (`reqwest`) and the bottom-layer `datazen-infra` utilities are
//! deliberate, user-decided exceptions recorded in the refactor plan.

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

/// D5：迁移三件套共享纯算法（pairing/recordset_bounds）。
#[cfg(feature = "migration_common")]
pub mod migration_common;

/// D5：Schema Diff 引擎（比较/审查/计划/部署 + job handler）。
#[cfg(feature = "schema_diff")]
pub mod schema_diff;

/// D5：Data Sync 引擎（比较/审查/变更集/SQL 预览/执行 + job handler）。
#[cfg(feature = "data_sync")]
pub mod data_sync;

/// D5：Data Transfer 引擎（单向复制/迁移：映射/预览/断点续传 + job pipeline）。
#[cfg(feature = "data_transfer")]
pub mod data_transfer;

/// D6 item 1：查询历史与迁移运行历史（词汇/仓储端口/用例门面）。
#[cfg(feature = "history")]
pub mod history;

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

/// 跨模块基础词汇（ID/上下文/错误信封）。常驻编译，不设 feature 门
/// （§5.1：形成归属契约的最小 ID/引用值在 shared 唯一定义）。
pub mod shared;

#[cfg(test)]
mod tests {
    #[test]
    fn the_modules_crate_has_a_real_root() {
        assert!(module_path!().ends_with("tests"));
    }
}
