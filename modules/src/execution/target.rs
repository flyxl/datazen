//! Canonical execution target vocabulary.
//!
//! Namespace shape belongs to `connection`; executable/object target semantics
//! belong to `execution`. This module is the single import surface for callers
//! that need both halves.

pub use crate::connection::domain::{NamespaceTarget, TargetNamespaceLayer};
pub use crate::execution::domain::{
    CanonicalNamespace, CanonicalNamespaceId, CanonicalTarget, ExecutionTarget, LayerRequirement,
    ObjectTarget, TargetCanonicalIdRules, TargetCanonicalizationError, TargetCaseFolding,
    TargetCaseRules, TargetNamespacePathSegment, TargetNamespaceShape, TargetRequirements,
};
