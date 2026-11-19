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

pub(crate) mod build_structure_change_request;
pub(crate) mod can_open_structure_editor;
pub(crate) mod control_hints;
// T085: the remaining pure `.ts` helpers (draft IDs/defaults + DDL warnings).
pub(crate) mod ddl_apply_warnings;
pub(crate) mod draft_defaults;
pub(crate) mod draft_ids;

/// Mirror of `StructureChangeMode` (`'create' | 'alter'`).
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructureChangeMode {
    Create,
    Alter,
}

/// Mirror of `StructureColumnDraft`. Fields are public to the crate so the
/// ported transformation and its tests can construct drafts verbatim.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StructureColumnDraft {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) data_type: String,
    pub(crate) nullable: bool,
    pub(crate) default_value: Option<String>,
    pub(crate) comment: Option<String>,
    pub(crate) is_primary_key: bool,
    pub(crate) is_auto_increment: bool,
    pub(crate) is_unique: bool,
}

/// Mirror of `StructureIndexDraft`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StructureIndexDraft {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) columns: Vec<String>,
    pub(crate) is_unique: bool,
    pub(crate) is_primary: bool,
    pub(crate) index_type: Option<String>,
    pub(crate) include_columns: Option<Vec<String>>,
    pub(crate) filter: Option<String>,
    pub(crate) comment: Option<String>,
}

/// Mirror of the value returned by `buildStructureChangeRequest`. Both
/// `original*` arrays are always present — create mode stores empty vectors,
/// which is what the React object literal does.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StructureChangeRequest {
    pub(crate) mode: StructureChangeMode,
    pub(crate) table: String,
    pub(crate) schema: Option<String>,
    pub(crate) original_columns: Vec<StructureColumnDraft>,
    pub(crate) current_columns: Vec<StructureColumnDraft>,
    pub(crate) original_indexes: Vec<StructureIndexDraft>,
    pub(crate) current_indexes: Vec<StructureIndexDraft>,
}

/// The three possible values of `DatabaseTypeMeta.connectionView`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionView {
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
pub(crate) struct StructureEditorUiConfig {
    pub(crate) enabled: Option<bool>,
    pub(crate) default_column_type: String,
}

/// Mirror of `StatementRisk` (`'additive' | 'destructive' | 'rewrite'`).
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatementRisk {
    Additive,
    Destructive,
    Rewrite,
}

impl StatementRisk {
    /// The exact React string literal for this risk, used when the DDL warning
    /// joins the de-duplicated risky statement labels.
    // consumers: the future structure-editor view; delete this allow when it lands.
    #[allow(dead_code)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            StatementRisk::Additive => "additive",
            StatementRisk::Destructive => "destructive",
            StatementRisk::Rewrite => "rewrite",
        }
    }

    /// The two risks `ddlApplyWarnings` treats as requiring confirmation.
    // consumers: the future structure-editor view; delete this allow when it lands.
    #[allow(dead_code)]
    pub(crate) fn is_risky(self) -> bool {
        matches!(self, StatementRisk::Destructive | StatementRisk::Rewrite)
    }
}

/// Mirror of `PlanStatement` (`{ sql, summary, risk }`).
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanStatement {
    pub(crate) sql: String,
    pub(crate) summary: String,
    pub(crate) risk: StatementRisk,
}

/// Mirror of `StructureChangePlan` (`{ statements, warnings? }`).
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StructureChangePlan {
    pub(crate) statements: Vec<PlanStatement>,
    pub(crate) warnings: Option<Vec<String>>,
}

/// The subset of `DatabaseTypeMeta` read by `canOpenStructureEditor`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DatabaseTypeMeta {
    pub(crate) is_key_value: bool,
    pub(crate) connection_view: ConnectionView,
    pub(crate) structure_editor: Option<StructureEditorUiConfig>,
    pub(crate) supports_sql: bool,
}

/// Mirror of `AlterStrategy` from `structureEditor/types.ts`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum AlterStrategy {
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
pub(crate) struct StructureCapabilities {
    pub(crate) create_table: bool,
    pub(crate) add_column: bool,
    pub(crate) drop_column: bool,
    pub(crate) rename_column: bool,
    pub(crate) alter_type: bool,
    pub(crate) alter_nullability: bool,
    pub(crate) alter_default: bool,
    pub(crate) alter_primary_key: bool,
    pub(crate) reorder_column: bool,
    pub(crate) comment: bool,
    pub(crate) create_index: bool,
    pub(crate) drop_index: bool,
    pub(crate) rebuild_index: bool,
    pub(crate) index_type: bool,
    pub(crate) index_include: bool,
    pub(crate) index_filter: bool,
    pub(crate) index_comment: bool,
    pub(crate) alter_strategy: AlterStrategy,
    pub(crate) dialect_id: String,
    pub(crate) index_methods: Vec<String>,
}

/// Mirror of `StructureCapabilityFlag`: only the boolean-valued capability
/// keys, matching the mapped type in `structureEditor/types.ts`.
// consumers: the future structure-editor view; delete this allow when it lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructureCapabilityFlag {
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
