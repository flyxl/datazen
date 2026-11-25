//! Rune-side value models for React `packages/frontend/src/lib/structureEditor/`.
//!
//! T036 ports three pure `.ts` helpers — `buildStructureChangeRequest`,
//! `canOpenStructureEditor` and `controlHints` — with no Rune tree, no state and
//! no IPC. The React types have no Rust counterpart in `datazen-ui-api` yet, so
//! they are mirrored here as view models (VM). `DatabaseTypeMeta` deliberately
//! carries only the four fields the predicate reads; the full driver meta stays
//! in the extension-points registry.
//!
//! Every item below is `#[allow(dead_code)]` with an explicit delete phase: the
//! structure-editor view that consumes these helpers does not exist yet. Remove
//! the per-item allow (not the whole module) when that view lands.

pub mod build_structure_change_request;
pub mod can_open_structure_editor;
pub mod control_hints;
// T085: the remaining pure `.ts` helpers (draft IDs/defaults + DDL warnings).
pub mod ddl_apply_warnings;
pub mod draft_defaults;
pub mod draft_ids;
// T124: the capability/schema helpers from `lib/structureEditor`.
pub mod is_control_enabled;
pub mod resolve_create_table_schema;
pub mod schema_to_draft;
// T156: contract verification + gap-fill for React lib/structureEditor/types.ts.
pub mod types;

/// Mirror of `StructureChangeMode` (`'create' | 'alter'`).
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureChangeMode {
    Create,
    Alter,
}

/// Mirror of `StructureColumnDraft`. Fields are public to the crate so the
/// ported transformation and its tests can construct drafts verbatim.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureColumnDraft {
    pub id: String,
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub comment: Option<String>,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
    pub is_unique: bool,
}

/// Mirror of `StructureIndexDraft`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureIndexDraft {
    pub id: String,
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub index_type: Option<String>,
    pub include_columns: Option<Vec<String>>,
    pub filter: Option<String>,
    pub comment: Option<String>,
}

/// Mirror of the value returned by `buildStructureChangeRequest`. Both
/// `original*` arrays are always present — create mode stores empty vectors,
/// which is what the React object literal does.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureChangeRequest {
    pub mode: StructureChangeMode,
    pub table: String,
    pub schema: Option<String>,
    pub original_columns: Vec<StructureColumnDraft>,
    pub current_columns: Vec<StructureColumnDraft>,
    pub original_indexes: Vec<StructureIndexDraft>,
    pub current_indexes: Vec<StructureIndexDraft>,
}

/// The three possible values of `DatabaseTypeMeta.connectionView`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionView {
    Sql,
    Keyvalue,
    Document,
}

/// The subset of `StructureEditorUiConfig` the ported helpers inspect.
/// `enabled` is optional in React (`enabled?: boolean`); absent stays `None`.
/// `default_column_type` is required in React (`defaultColumnType: string`) and
/// is what `draftDefaults.defaultCreateColumns` seeds both drafts with.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructureEditorUiConfig {
    pub enabled: Option<bool>,
    pub default_column_type: String,
}

/// Mirror of `StatementRisk` (`'additive' | 'destructive' | 'rewrite'`).
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementRisk {
    Additive,
    Destructive,
    Rewrite,
}

impl StatementRisk {
    /// The exact React string literal for this risk, used when the DDL warning
    /// joins the de-duplicated risky statement labels.
    // consumers: the future structure-editor view; delete this allow when it lands.
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            StatementRisk::Additive => "additive",
            StatementRisk::Destructive => "destructive",
            StatementRisk::Rewrite => "rewrite",
        }
    }

    /// The two risks `ddlApplyWarnings` treats as requiring confirmation.
    // consumers: the future structure-editor view; delete this allow when it lands.
    #[allow(dead_code)]
    pub fn is_risky(self) -> bool {
        matches!(self, StatementRisk::Destructive | StatementRisk::Rewrite)
    }
}

/// Mirror of `PlanStatement` (`{ sql, summary, risk }`).
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStatement {
    pub sql: String,
    pub summary: String,
    pub risk: StatementRisk,
}

/// Mirror of `StructureChangePlan` (`{ statements, warnings? }`).
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureChangePlan {
    pub statements: Vec<PlanStatement>,
    pub warnings: Option<Vec<String>>,
}

/// The subset of `DatabaseTypeMeta` read by `canOpenStructureEditor`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseTypeMeta {
    pub is_key_value: bool,
    pub connection_view: ConnectionView,
    pub structure_editor: Option<StructureEditorUiConfig>,
    pub supports_sql: bool,
}

/// Mirror of `AlterStrategy` from `structureEditor/types.ts`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlterStrategy {
    #[default]
    None,
    Direct,
    SqliteRebuild,
}

/// Mirror of `StructureCapabilities`. `StructureCapabilityFlag` in React is the
/// subset of keys whose value is `boolean`; the three non-boolean fields are
/// kept here so the VM is complete but never treated as a control flag.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StructureCapabilities {
    pub create_table: bool,
    pub add_column: bool,
    pub drop_column: bool,
    pub rename_column: bool,
    pub alter_type: bool,
    pub alter_nullability: bool,
    pub alter_default: bool,
    pub alter_primary_key: bool,
    pub reorder_column: bool,
    pub comment: bool,
    pub create_index: bool,
    pub drop_index: bool,
    pub rebuild_index: bool,
    pub index_type: bool,
    pub index_include: bool,
    pub index_filter: bool,
    pub index_comment: bool,
    pub alter_strategy: AlterStrategy,
    pub dialect_id: String,
    pub index_methods: Vec<String>,
}

/// Mirror of `StructureCapabilityFlag`: only the boolean-valued capability
/// keys, matching the mapped type in `structureEditor/types.ts`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureCapabilityFlag {
    CreateTable,
    AddColumn,
    DropColumn,
    RenameColumn,
    AlterType,
    AlterNullability,
    AlterDefault,
    AlterPrimaryKey,
    ReorderColumn,
    Comment,
    CreateIndex,
    DropIndex,
    RebuildIndex,
    IndexType,
    IndexInclude,
    IndexFilter,
    IndexComment,
}
