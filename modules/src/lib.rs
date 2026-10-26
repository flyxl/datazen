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

#[cfg(feature = "named_views")]
pub mod named_views;

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
