//! T156: contract verification + gap-fill for React `lib/structureEditor/types.ts`.
//!
//! Most exports already have mirrors in [`super`] from T036/T085/T124. This
//! module adds only what those ports did not need:
//!
//! - exact React string round-trips for [`AlterStrategy`],
//!   [`StructureChangeMode`], [`StatementRisk`] and [`StructureCapabilityFlag`];
//! - an enumeration of the 17 boolean capability keys (the mapped-type
//!   semantics of `StructureCapabilityFlag`), with the three non-boolean
//!   capability fields deliberately absent;
//! - a complete mirror of `StructureEditorUiConfig`, which React merely
//!   re-exports from `@datazen/extension-points`. The legacy two-field mirror in
//!   [`super::StructureEditorUiConfig`] stays untouched because T036/T085 helper
//!   callers construct it verbatim; [`ExtensionStructureEditorUiConfig::to_legacy`]
//!   bridges the two.
//!
//! `datazen-driver-api` has canonical `StructureCapabilities` / `AlterStrategy`
//! definitions, but `datazen-ui` does not depend on that crate (and adding a
//! dependency would be a rewrite), so the existing local mirrors remain the
//! contract surface here. Their field lists match the driver-api definitions.
//!
//! ## Removal stage
//!
//! Every item is `#[allow(dead_code)]` with the same consumer: the structure
//! editor view, not landed yet. Remove the per-item allow (not the module) when
//! that view calls these APIs. The full config mirror can be dropped when the
//! T036/T085 helpers migrate to it and the two-field legacy struct is retired.

use super::{
    AlterStrategy, StatementRisk, StructureCapabilityFlag, StructureChangeMode,
    StructureEditorUiConfig,
};

#[allow(dead_code)] // structure-editor view not landed; remove its allow when wired
impl AlterStrategy {
    /// Every React `'none' | 'direct' | 'sqliteRebuild'` value, in source order.
    pub const ALL: [Self; 3] = [Self::None, Self::Direct, Self::SqliteRebuild];

    /// The exact React string literal for this strategy.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Direct => "direct",
            Self::SqliteRebuild => "sqliteRebuild",
        }
    }

    /// Parse a React string literal back into the variant.
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "direct" => Some(Self::Direct),
            "sqliteRebuild" => Some(Self::SqliteRebuild),
            _ => None,
        }
    }
}

#[allow(dead_code)] // structure-editor view not landed; remove its allow when wired
impl StructureChangeMode {
    /// Every React `'create' | 'alter'` value, in source order.
    pub const ALL: [Self; 2] = [Self::Create, Self::Alter];

    /// The exact React string literal for this mode (driver-api uses lowercase).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Alter => "alter",
        }
    }

    /// Parse a React string literal back into the variant.
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "create" => Some(Self::Create),
            "alter" => Some(Self::Alter),
            _ => None,
        }
    }
}

#[allow(dead_code)] // structure-editor view not landed; remove its allow when wired
impl StatementRisk {
    /// Parse a React string literal back into the variant (`as_str` lives with
    /// the T036 mirror in `mod.rs`).
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "additive" => Some(Self::Additive),
            "destructive" => Some(Self::Destructive),
            "rewrite" => Some(Self::Rewrite),
            _ => None,
        }
    }
}

#[allow(dead_code)] // structure-editor view not landed; remove its allow when wired
impl StructureCapabilityFlag {
    /// The 17 boolean-valued keys of `StructureCapabilities`, in React source
    /// order. `alterStrategy`, `dialectId` and `indexMethods` are non-boolean
    /// and therefore not flags (mapped-type semantics of the React export).
    pub const ALL: [Self; 17] = [
        Self::CreateTable,
        Self::AddColumn,
        Self::DropColumn,
        Self::RenameColumn,
        Self::AlterType,
        Self::AlterNullability,
        Self::AlterDefault,
        Self::AlterPrimaryKey,
        Self::ReorderColumn,
        Self::Comment,
        Self::CreateIndex,
        Self::DropIndex,
        Self::RebuildIndex,
        Self::IndexType,
        Self::IndexInclude,
        Self::IndexFilter,
        Self::IndexComment,
    ];

    /// The exact camelCase property name of the boolean capability field.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreateTable => "createTable",
            Self::AddColumn => "addColumn",
            Self::DropColumn => "dropColumn",
            Self::RenameColumn => "renameColumn",
            Self::AlterType => "alterType",
            Self::AlterNullability => "alterNullability",
            Self::AlterDefault => "alterDefault",
            Self::AlterPrimaryKey => "alterPrimaryKey",
            Self::ReorderColumn => "reorderColumn",
            Self::Comment => "comment",
            Self::CreateIndex => "createIndex",
            Self::DropIndex => "dropIndex",
            Self::RebuildIndex => "rebuildIndex",
            Self::IndexType => "indexType",
            Self::IndexInclude => "indexInclude",
            Self::IndexFilter => "indexFilter",
            Self::IndexComment => "indexComment",
        }
    }

    /// Parse a camelCase boolean capability key back into the variant. The
    /// three non-boolean keys are not accepted.
    pub fn from_str(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|flag| flag.as_str() == value)
    }
}

/// One `{ value, label }` entry of the extension-points `columnTypes` array.
#[allow(dead_code)] // structure-editor view not landed; remove its allow when wired
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructureEditorColumnTypeOption {
    pub value: String,
    pub label: String,
}

/// The optional boolean `fields` map of the extension-points config.
#[allow(dead_code)] // structure-editor view not landed; remove its allow when wired
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructureEditorFields {
    pub comment: Option<bool>,
    pub charset: Option<bool>,
    pub collation: Option<bool>,
    pub unsigned: Option<bool>,
    pub length: Option<bool>,
}

/// Complete mirror of the `StructureEditorUiConfig` that React re-exports from
/// `@datazen/extension-points` (`types.ts:1`).
#[allow(dead_code)] // structure-editor view not landed; remove its allow when wired
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtensionStructureEditorUiConfig {
    pub enabled: Option<bool>,
    pub column_types: Vec<StructureEditorColumnTypeOption>,
    pub default_column_type: String,
    pub fields: StructureEditorFields,
    pub index_methods: Vec<String>,
}

#[allow(dead_code)] // structure-editor view not landed; remove its allow when wired
impl ExtensionStructureEditorUiConfig {
    /// Project onto the two-field subset the T036/T085 helpers consume.
    pub fn to_legacy(&self) -> StructureEditorUiConfig {
        StructureEditorUiConfig {
            enabled: self.enabled,
            default_column_type: self.default_column_type.clone(),
        }
    }
}
