//! Namespace shape, target DTOs and the validation order (P2).
//!
//! Source of truth: `docs/architecture/platform/connection-management.md` §4.3.
//!
//! The order matters and is not negotiable: DTO fields present -> non-null value
//! for a non-existent level rejected -> aliases merged with conflicts refused ->
//! driver canonicalizes -> the operation's `targetRequirements` validated ->
//! [`CanonicalTarget`] emitted. Nothing later in the pipeline may fill a target
//! in from the UI or from another session.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::resource::ResourceError;

/// Which namespace levels a database has, outermost first.
///
/// `NamespaceLevelKind` has three values — `Database`, `Catalog`, `Schema` — and
/// is the layer enum of this crate's `NamespaceShape`, which also carries
/// `path_segments`, `case_rules`, `canonical_id_rules` and `aliases`, and
/// canonicalizes via `canonicalize()` / `resolve_alias()`.
///
/// The platform side describes a different question — what one request declares
/// — as `datazen_platform_api::target::TargetNamespaceShape`. Its layer enum is
/// `TargetNamespaceLayer` (`Database` / `Catalog` / `Schema` / `Path`), it carries only
/// the `required` / `optional` sets plus the `declares()` / `is_required()`
/// predicates, and it has no case, alias or canonical-id data. `Path` is
/// meaningful only there — it names a request-supplied path, not a level a
/// database has. `ResourceDescriptor::namespace_shape` is typed with this
/// crate's `NamespaceShape`, not with that one, and no conversion exists
/// between the two.
///
/// `PartialOrd`/`Ord` are deliberately absent: no caller sorts or orders
/// namespace levels today. The "outermost first" contract above is enforced by
/// [`NamespaceShape`] validation and by call sites that walk levels in written
/// order, not by comparing them. Adding them back would be an unused API
/// surface, not a fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NamespaceLevelKind {
    Database,
    Catalog,
    Schema,
}

/// One namespace level as the driver declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceLevel {
    pub kind: NamespaceLevelKind,
    /// False for a level the database simply does not have (SQLite has no
    /// catalog). A target that names a non-existent level is rejected.
    pub exists: bool,
    /// True when the connection cannot be opened without a value here.
    pub required: bool,
}

impl NamespaceLevel {
    pub fn present(kind: NamespaceLevelKind, required: bool) -> Self {
        Self {
            kind,
            exists: true,
            required,
        }
    }

    pub fn absent(kind: NamespaceLevelKind) -> Self {
        Self {
            kind,
            exists: false,
            required: false,
        }
    }
}

/// One segment of a driver-supplied namespace path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespacePathSegment {
    /// Position in the path, 0-based.
    pub position: usize,
    pub kind: NamespaceLevelKind,
    /// Free-form driver documentation of what this segment identifies. Kept as
    /// a string so drivers can spell it their way without an enum per driver.
    pub meaning: String,
}

/// How identifiers fold under case rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaseFolding {
    /// `Foo` and `foo` are the same identifier.
    Lowercases,
    /// `Foo` and `foo` are different identifiers.
    Preserved,
    Unknown,
}

impl Default for CaseFolding {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseRules {
    pub unquoted: CaseFolding,
    pub quoted: CaseFolding,
}

impl Default for CaseRules {
    fn default() -> Self {
        Self {
            unquoted: CaseFolding::Unknown,
            quoted: CaseFolding::Unknown,
        }
    }
}

/// How the driver builds the canonical id for an object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalIdRules {
    pub case_sensitive: bool,
    pub delimiter: String,
}

impl Default for CanonicalIdRules {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            delimiter: ".".to_string(),
        }
    }
}

/// Everything the driver needs to validate and canonicalize a namespace
/// target, registered on every [`ResourceDescriptor`]
/// (`connection-management.md` §4.3).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceShape {
    pub levels: Vec<NamespaceLevel>,
    pub path_segments: Vec<NamespacePathSegment>,
    pub case_rules: CaseRules,
    pub canonical_id_rules: CanonicalIdRules,
    /// raw identifier -> canonical identifier. Values may themselves be keys;
    /// resolution therefore has to reach a fixed point (see
    /// [`NamespaceShape::resolve_alias`]).
    pub aliases: BTreeMap<String, String>,
}

impl NamespaceShape {
    pub fn level(&self, kind: NamespaceLevelKind) -> Option<&NamespaceLevel> {
        self.levels.iter().find(|level| level.kind == kind)
    }

    /// Step 3 of the validation order: merge aliases.
    ///
    /// An alias chain must terminate at a value that is not itself an alias. A
    /// cycle is a conflict and is rejected rather than resolved arbitrarily —
    /// picking either end would make the canonical id depend on iteration order.
    pub fn resolve_alias(&self, raw: &str) -> Result<String, ResourceError> {
        let mut current = raw.to_string();
        for _ in 0..=self.aliases.len() {
            match self.aliases.get(&current) {
                Some(next) if *next != current => current = next.clone(),
                _ => return Ok(current),
            }
        }
        Err(ResourceError::AliasConflict {
            alias: raw.to_string(),
        })
    }

    /// Steps 1-3 of the validation order: DTO fields present, non-null values
    /// for non-existent levels rejected, aliases merged and conflicts refused.
    ///
    /// The same canonical identifier may legitimately appear at two levels —
    /// PostgreSQL's catalog *is* the database — so repetition is not a
    /// conflict. Only an unresolvable alias chain is.
    ///
    /// A level the shape does not declare at all is treated exactly like a
    /// level declared with `exists: false`: supplying a value for it is
    /// rejected rather than dropped. Silently ignoring it would let a driver
    /// that forgot to declare its shape (or declared an empty one) appear to
    /// honour an arbitrary target.
    pub fn canonicalize(&self, target: &NamespaceTarget) -> Result<CanonicalTarget, ResourceError> {
        let mut resolved: Vec<String> = Vec::new();

        for (kind, value) in [
            (NamespaceLevelKind::Database, target.database.as_deref()),
            (NamespaceLevelKind::Catalog, target.catalog.as_deref()),
            (NamespaceLevelKind::Schema, target.schema.as_deref()),
        ] {
            let Some(value) = value else { continue };

            match self.level(kind) {
                None => {
                    return Err(ResourceError::NonexistentNamespaceLevel { kind });
                }
                Some(level) if !level.exists => {
                    return Err(ResourceError::NonexistentNamespaceLevel { kind });
                }
                Some(_) => resolved.push(self.resolve_alias(value)?),
            }
        }

        for level in &self.levels {
            let missing = match level.kind {
                NamespaceLevelKind::Database => target.database.is_none(),
                NamespaceLevelKind::Catalog => target.catalog.is_none(),
                NamespaceLevelKind::Schema => target.schema.is_none(),
            };
            if level.exists && level.required && missing {
                return Err(ResourceError::MissingRequiredNamespaceLevel { kind: level.kind });
            }
        }

        for segment in &target.path {
            resolved.push(self.resolve_alias(segment)?);
        }

        Ok(CanonicalTarget {
            path: resolved,
            requested: target.clone(),
        })
    }

    /// Step 5 of the validation order, applied for one specific operation.
    pub fn validate_requirements(
        &self,
        target: &NamespaceTarget,
        requirements: &TargetRequirements,
    ) -> Result<(), ResourceError> {
        for (kind, requirement) in [
            (NamespaceLevelKind::Database, requirements.database),
            (NamespaceLevelKind::Catalog, requirements.catalog),
            (NamespaceLevelKind::Schema, requirements.schema),
        ] {
            let provided = match kind {
                NamespaceLevelKind::Database => target.database.is_some(),
                NamespaceLevelKind::Catalog => target.catalog.is_some(),
                NamespaceLevelKind::Schema => target.schema.is_some(),
            };
            match requirement {
                TargetLevelRequirement::Required if !provided => {
                    return Err(ResourceError::MissingRequiredNamespaceLevel { kind })
                }
                TargetLevelRequirement::Forbidden if provided => {
                    return Err(ResourceError::ForbiddenNamespaceLevel { kind })
                }
                _ => {}
            }
        }
        if !target.path.is_empty() && requirements.path_forbidden {
            return Err(ResourceError::ForbiddenNamespaceLevel {
                kind: NamespaceLevelKind::Database,
            });
        }
        Ok(())
    }
}

/// A namespace target as requested by a caller.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceTarget {
    pub database: Option<String>,
    pub catalog: Option<String>,
    pub schema: Option<String>,
    /// Driver-specific path segments beyond the three levels above.
    pub path: Vec<String>,
}

impl NamespaceTarget {
    /// A target with nothing filled in. This is what an *unobserved* context
    /// carries — see `SessionContext::unobserved`.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn with_database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into());
        self
    }

    pub fn with_schema(mut self, schema: impl Into<String>) -> Self {
        self.schema = Some(schema.into());
        self
    }
}

/// A target after alias merge and validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalTarget {
    pub path: Vec<String>,
    /// What the caller asked for, kept so a discrepancy stays visible.
    pub requested: NamespaceTarget,
}

/// What one operation accepts at a namespace level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetLevelRequirement {
    Required,
    Optional,
    Forbidden,
}

/// Per-operation target requirements, registered alongside the Command
/// definition rather than guessed by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRequirements {
    pub database: TargetLevelRequirement,
    pub catalog: TargetLevelRequirement,
    pub schema: TargetLevelRequirement,
    pub path_forbidden: bool,
}

impl Default for TargetRequirements {
    fn default() -> Self {
        Self {
            database: TargetLevelRequirement::Optional,
            catalog: TargetLevelRequirement::Optional,
            schema: TargetLevelRequirement::Optional,
            path_forbidden: false,
        }
    }
}

#[cfg(test)]
#[path = "namespace_tests.rs"]
mod tests;
