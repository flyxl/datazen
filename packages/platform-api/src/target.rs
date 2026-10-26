//! 目标词汇迁移垫片（D2/D3，2026-10-26）。
//!
//! 全部目标词汇的唯一定义自 D2/D3 起在 `datazen-modules`：
//! - `NamespaceTarget` / `TargetNamespaceLayer` → `connection::domain`
//! - `ExecutionTarget` / `ObjectTarget` / `CanonicalTarget` 家族 /
//!   `TargetNamespaceShape` / `TargetRequirements` 等驱动能力元数据 →
//!   `execution::domain`
//!
//! 本模块只再导出，旧 import 路径 `target::*` 保持不变；D8 退役本 crate 时
//! 一并删除。`ObjectTarget` 按 §5.1 最终归 schema（D4 落位）。

pub use datazen_modules::connection::domain::{NamespaceTarget, TargetNamespaceLayer};
pub use datazen_modules::execution::domain::{
    CanonicalNamespace, CanonicalNamespaceId, CanonicalTarget, ExecutionTarget, LayerRequirement,
    ObjectTarget, TargetCanonicalIdRules, TargetCanonicalizationError, TargetCaseFolding,
    TargetCaseRules, TargetNamespacePathSegment, TargetNamespaceShape, TargetRequirements,
};
