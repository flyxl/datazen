//! Pure VM mirrors and effects for `usePanelHandlers`. All state is host-owned;
//! the decisions in `open` / `tabs` answer with [`PanelEffect`]s.

/// React `Panel.type`.
#[allow(dead_code)] // VM mirror; consumed by the reuse rules and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelKind {
    Table,
    View,
    CreateTable,
    ErDiagram,
    Objects,
    Privileges,
    DbObject,
    ServerStatus,
    Processes,
    Query,
}

impl PanelKind {
    #[allow(dead_code)] // pure helper; consumed by diagnostics and tests.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::CreateTable => "create-table",
            Self::ErDiagram => "er-diagram",
            Self::Objects => "objects",
            Self::Privileges => "privileges",
            Self::DbObject => "db-object",
            Self::ServerStatus => "server-status",
            Self::Processes => "processes",
            Self::Query => "query",
        }
    }
}

/// React `SubTabId` (only the values this hook sets).
#[allow(dead_code)] // VM mirror; consumed by the tab decisions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SubTab {
    #[default]
    Data,
    Structure,
    Ddl,
}

/// One panel, reduced to the fields this hook reads or writes. A struct (not a
/// sum type) keeps the host feed simple; `kind` selects which fields matter.
#[allow(dead_code)] // VM mirror; supplied by the panel host.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PanelVm {
    pub id: String,
    pub kind: Option<PanelKind>,
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table_name: String,
    pub table_schema: Option<String>,
    pub view_name: String,
    pub view_schema: Option<String>,
    pub sub_tab: SubTab,
    pub structure_editing: bool,
    pub focus_table: Option<String>,
    pub target_column: Option<String>,
    pub object_kind: String,
    pub object_name: String,
    pub object_schema: Option<String>,
    pub object_signature: Option<String>,
    pub object_target_schema: Option<String>,
    pub object_target_name: Option<String>,
    pub object_database: Option<String>,
    pub namespace_path: Vec<String>,
    pub title: String,
}

/// React `ConnectionContext` (the sidebar context).
#[allow(dead_code)] // VM mirror; supplied by the host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConnectionContextVm {
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
}

/// An explicit open target from a context menu (may carry an old dbSessionId).
#[allow(dead_code)] // VM mirror; supplied by the host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OpenTargetVm {
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
}

/// A sparse panel patch: outer `None` means "leave alone", inner `Option` lets
/// a field be cleared (React `updatePanel(id, { schema: null })`).
#[allow(dead_code)] // VM mirror; consumed by the reuse decisions.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PanelPatch {
    pub sub_tab: Option<SubTab>,
    pub structure_editing: Option<bool>,
    pub target_column: Option<Option<String>>,
    pub database: Option<Option<String>>,
    pub schema: Option<Option<String>>,
    pub focus_table: Option<Option<String>>,
    pub view_schema: Option<Option<String>>,
    pub table_schema: Option<Option<String>>,
}

impl PanelPatch {
    /// Apply this patch to a panel snapshot (pure state transition).
    #[allow(dead_code)] // pure transition; consumed by the host and tests.
    pub(crate) fn apply(&self, panel: &mut PanelVm) {
        if let Some(sub_tab) = self.sub_tab {
            panel.sub_tab = sub_tab;
        }
        if let Some(value) = self.structure_editing {
            panel.structure_editing = value;
        }
        if let Some(value) = &self.target_column {
            panel.target_column = value.clone();
        }
        if let Some(value) = &self.database {
            panel.database = value.clone();
        }
        if let Some(value) = &self.schema {
            panel.schema = value.clone();
        }
        if let Some(value) = &self.focus_table {
            panel.focus_table = value.clone();
        }
        if let Some(value) = &self.view_schema {
            panel.view_schema = value.clone();
        }
        if let Some(value) = &self.table_schema {
            panel.table_schema = value.clone();
        }
    }
}

/// The full host effect of a panel handler decision.
#[allow(dead_code)] // host effect; remove with the host wiring.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PanelEffect {
    /// Nothing to do (missing context, wrong connection, no-op close).
    None,
    /// Activate an existing panel.
    Activate { panel_id: String },
    /// Activate an existing panel and apply a patch to it.
    ActivatePatch { panel_id: String, patch: PanelPatch },
    /// Patch a panel without activating it.
    Patch { panel_id: String, patch: PanelPatch },
    /// Add a new panel.
    Add { panel: PanelVm },
    /// Add a new query panel and seed its SQL in the same turn.
    AddSql { panel: PanelVm, sql: String },
    /// Open the query history for a connection, optionally activating a tab.
    ActivateHistory { panel_id: Option<String>, connection_id: String },
    /// Close the given panel ids (in store order).
    Close { panel_ids: Vec<String> },
    /// Invalidate cached data and reload for a session (tables vs connection).
    Refresh {
        db_session_id: String,
        /// `Some` → `loadTables(database)`, `None` → `loadForConnection`.
        database: Option<String>,
        database_type: String,
    },
    /// Show the tab context menu with only the applicable items.
    ContextMenu {
        items: Vec<TabMenuItem>,
        x: f32,
        y: f32,
    },
}

/// The five tab context-menu entries, with the three enable flags from React.
#[allow(dead_code)] // VM mirror; consumed by the context-menu decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabMenuItem {
    Close,
    CloseOthers,
    CloseAll,
    CloseToRight,
    CloseToLeft,
}

/// React `panelSchema(panel)` (`panelTypes.ts`): the schema a panel contributes
/// to an ER diagram, selected by panel type.
#[allow(dead_code)] // pure helper; remove with the host wiring.
pub(crate) fn panel_schema(panel: &PanelVm) -> Option<String> {
    match panel.kind {
        Some(PanelKind::Table) | Some(PanelKind::CreateTable) => panel.table_schema.clone(),
        Some(PanelKind::View) => panel.view_schema.clone(),
        Some(PanelKind::Query) | Some(PanelKind::ErDiagram) => panel.schema.clone(),
        Some(PanelKind::DbObject) => panel.object_schema.clone(),
        _ => None,
    }
}
