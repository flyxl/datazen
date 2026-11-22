//! Pure logic of `windows/connection/useConnectionWorkspaceMeta.ts` (T612).
//!
//! The hook is a pure relocation of ContentView's derived state: the same
//! store snapshots always yield the same `ConnectionWorkspaceMeta`. This port
//! keeps that derivation as functions over host-owned VM inputs. There is no
//! store subscription, IPC, timer or clipboard here — the host reads its own
//! snapshots, calls [`derive_workspace_meta`] and applies the returned VM.
//!
//! Two single-source-of-truth rules are preserved:
//!
//! - database behaviour comes only from the host-supplied driver-meta map
//!   (React `DB_REGISTRY`); no driver name or dialect is spelled out here;
//! - `canOpenStructureEditor` is **reused** from `crate::structure_editor`
//!   through [`DatabaseTypeMetaVm::to_structure_meta`], so the structure-editor
//!   capability has exactly one implementation. The `readOnly` guard is the
//!   hook's own second conjunct, not part of that predicate.
//!
//! The two "!== false" guards keep React's default-true semantics by modelling
//! the optional driver fields as `Option<bool>`: `None` and `Some(true)` both
//! show the item, only `Some(false)` hides it.
//!
//! ## Consumer / removal stage
//!
//! No production connection host mounts this derivation yet. Every reusable
//! item carries a per-item `#[allow(dead_code)]` with this stage: drop each
//! item when the connection host owns that derived field.

use std::collections::BTreeMap;

use datazen_ui_api::{resolve_connection_context, ContentViewActiveConnectionEntry, SavedConnection};

use crate::structure_editor::{
    can_open_structure_editor::can_open_structure_editor, ConnectionView, DatabaseTypeMeta,
    StructureEditorUiConfig,
};

/// Host-owned projection of one panel (`stores/panelStore.Panel`).
#[allow(dead_code)] // VM mirror; supplied by the panel store.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspacePanelVm {
    pub(crate) id: String,
    pub(crate) connection_id: String,
    pub(crate) db_session_id: String,
    pub(crate) connection_name: String,
    pub(crate) database_type: String,
    /// Redis-style panels own their target database (`dbName` in React).
    pub(crate) db_name: Option<String>,
}

/// React `ConnectionContext` (`stores/panelStore`).
#[allow(dead_code)] // VM mirror; returned to the host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceConnectionContextVm {
    pub(crate) connection_id: String,
    pub(crate) db_session_id: String,
    pub(crate) connection_name: String,
    pub(crate) database_type: String,
}

/// One active-connection entry, including the status the connecting guard reads.
#[allow(dead_code)] // VM mirror; supplied by the active-connection store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceActiveEntryVm {
    pub(crate) connection_id: String,
    /// Empty while connecting or errored (React `dbSessionId: string`).
    pub(crate) db_session_id: Option<String>,
    pub(crate) status: String,
}

/// React `DatabaseTypeMeta.connectionView`.
#[allow(dead_code)] // VM mirror; supplied by the driver-meta map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MetaConnectionViewVm {
    Sql,
    Keyvalue,
    Document,
}

/// React `DataExportCapability` from `lib/exportCapability.ts`.
#[allow(dead_code)] // VM mirror; supplied by the driver-meta map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DataExportCapabilityVm {
    None,
    LoadedOnly,
    #[default]
    FullTable,
}

impl DataExportCapabilityVm {
    /// Exact React string values of the capability union.
    #[allow(dead_code)] // pure helper; drop with the host wiring.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::LoadedOnly => "loaded_only",
            Self::FullTable => "full_table",
        }
    }
}

/// The subset of `DatabaseTypeMeta` this hook reads. Optional booleans keep
/// React's `!== false` / `=== true` / `=== undefined` distinctions intact.
#[allow(dead_code)] // VM mirror; supplied by the host's DB_REGISTRY projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DatabaseTypeMetaVm {
    pub(crate) is_key_value: bool,
    pub(crate) connection_view: MetaConnectionViewVm,
    pub(crate) structure_editor_enabled: Option<bool>,
    pub(crate) supports_sql: Option<bool>,
    pub(crate) is_read_only: bool,
    pub(crate) supports_er_diagram: Option<bool>,
    pub(crate) export_scope: DataExportCapabilityVm,
}

impl DatabaseTypeMetaVm {
    /// Project onto the existing structure-editor contract so the capability
    /// predicate is reused branch-for-branch rather than duplicated. Absent
    /// `supportsSQL` is falsy for `canOpenStructureEditor` (`if (!meta.supportsSQL)`)
    /// even though `showNewQuery` treats it as default-true (`!== false`).
    fn to_structure_meta(&self) -> DatabaseTypeMeta {
        DatabaseTypeMeta {
            is_key_value: self.is_key_value,
            connection_view: match self.connection_view {
                MetaConnectionViewVm::Sql => ConnectionView::Sql,
                MetaConnectionViewVm::Keyvalue => ConnectionView::Keyvalue,
                MetaConnectionViewVm::Document => ConnectionView::Document,
            },
            structure_editor: Some(StructureEditorUiConfig {
                enabled: self.structure_editor_enabled,
                // The predicate never reads the default column type.
                default_column_type: String::new(),
            }),
            supports_sql: self.supports_sql.unwrap_or(false),
        }
    }
}

/// `canOpenStructureEditor(dbMeta)` — the shared predicate, not a copy.
#[allow(dead_code)] // pure capability gate; drop when the host wires the menu.
pub(crate) fn can_open(meta: Option<&DatabaseTypeMetaVm>) -> bool {
    match meta {
        None => false,
        Some(meta) => can_open_structure_editor(Some(&meta.to_structure_meta())),
    }
}

/// `canOpenStructureEditor(dbMeta) && dbMeta?.readOnly !== true`.
#[allow(dead_code)] // pure capability gate; drop when the host wires the menu.
pub(crate) fn show_structure_editor(meta: Option<&DatabaseTypeMetaVm>) -> bool {
    match meta {
        None => false,
        Some(meta) => can_open(Some(meta)) && !meta.is_read_only,
    }
}

/// `resolveExportScope(dbMeta)`: absent metadata defaults to `full_table`.
#[allow(dead_code)] // pure capability gate; drop when the host wires export.
pub(crate) fn resolve_export_scope(meta: Option<&DatabaseTypeMetaVm>) -> DataExportCapabilityVm {
    meta.map_or(DataExportCapabilityVm::FullTable, |meta| meta.export_scope)
}

/// `supportsFullTableExport(capability)` — the whole-table export is the only
/// capability that permits a batch export.
#[allow(dead_code)] // pure capability gate; drop when the host wires export.
pub(crate) fn supports_full_table_export(scope: DataExportCapabilityVm) -> bool {
    scope == DataExportCapabilityVm::FullTable
}

/// `toolbarDbMeta?.isKeyValue === true` (note: toolbar meta, not panel meta).
#[allow(dead_code)] // pure capability gate; drop when the host wires the toolbar.
pub(crate) fn is_kv_panel(meta: Option<&DatabaseTypeMetaVm>) -> bool {
    meta.is_some_and(|meta| meta.is_key_value)
}

/// `!isKvPanel && toolbarDbMeta?.supportsSQL !== false && !!toolbarDbType`.
#[allow(dead_code)] // pure toolbar guard; drop when the host wires the toolbar.
pub(crate) fn show_new_query(meta: Option<&DatabaseTypeMetaVm>, has_toolbar_type: bool) -> bool {
    !is_kv_panel(meta)
        && meta.map_or(true, |meta| meta.supports_sql != Some(false))
        && has_toolbar_type
}

/// `!isKvPanel && canOpenStructureEditor(toolbarDbMeta) && readOnly !== true && !!toolbarDbType`.
#[allow(dead_code)] // pure toolbar guard; drop when the host wires the toolbar.
pub(crate) fn show_new_table(meta: Option<&DatabaseTypeMetaVm>, has_toolbar_type: bool) -> bool {
    !is_kv_panel(meta)
        && can_open(meta)
        && meta.map_or(true, |meta| !meta.is_read_only)
        && has_toolbar_type
}

/// `!isKvPanel && toolbarDbMeta?.supportsErDiagram !== false && !!toolbarDbType`.
#[allow(dead_code)] // pure toolbar guard; drop when the host wires the toolbar.
pub(crate) fn show_er_diagram_toolbar(
    meta: Option<&DatabaseTypeMetaVm>,
    has_toolbar_type: bool,
) -> bool {
    !is_kv_panel(meta)
        && meta.map_or(true, |meta| meta.supports_er_diagram != Some(false))
        && has_toolbar_type
}

/// `!isKvPanel && toolbarDbMeta?.readOnly !== true && !!toolbarDbType`.
#[allow(dead_code)] // pure toolbar guard; drop when the host wires the toolbar.
pub(crate) fn show_objects_toolbar(
    meta: Option<&DatabaseTypeMetaVm>,
    has_toolbar_type: bool,
) -> bool {
    !is_kv_panel(meta)
        && meta.map_or(true, |meta| !meta.is_read_only)
        && has_toolbar_type
}

/// React `connectingEntry`'s trimmed shape.
#[allow(dead_code)] // VM mirror; returned to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConnectingEntryVm {
    pub(crate) status: String,
    pub(crate) connection_id: String,
    pub(crate) db_session_id: Option<String>,
}

/// The hook's full host-owned state (one zustand snapshot per store).
#[allow(dead_code)] // VM surface; owned by the connection host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceMetaInputsVm {
    pub(crate) active_panel: Option<WorkspacePanelVm>,
    pub(crate) saved_connections: Vec<SavedConnection>,
    pub(crate) active_connections: Vec<WorkspaceActiveEntryVm>,
    /// `useSchemaStore.activeDbSessionId`.
    pub(crate) store_active_db_session_id: Option<String>,
    /// `useConnectionSchemaField(panel?.dbSessionId ?? '', 'currentDatabase')`.
    pub(crate) current_database: Option<String>,
    pub(crate) all_panels: Vec<WorkspacePanelVm>,
    /// Host projection of `DB_REGISTRY`, keyed by database type.
    pub(crate) db_meta: BTreeMap<String, DatabaseTypeMetaVm>,
}

/// The exact field set of React `ConnectionWorkspaceMeta` (snake_case here).
#[allow(dead_code)] // VM surface; consumed by ContentView's host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceMetaVm {
    pub(crate) conn_ctx: Option<WorkspaceConnectionContextVm>,
    pub(crate) sidebar_conn_ctx: Option<WorkspaceConnectionContextVm>,
    pub(crate) initial_database: Option<String>,
    pub(crate) database_type: Option<String>,
    pub(crate) db_session_id: String,
    pub(crate) connection_id: String,
    pub(crate) connection_name: String,
    pub(crate) has_saved_connections: bool,
    pub(crate) toolbar_db_type: Option<String>,
    pub(crate) show_structure_editor: bool,
    pub(crate) export_scope: DataExportCapabilityVm,
    pub(crate) batch_export_supported: bool,
    pub(crate) is_kv_panel: bool,
    pub(crate) show_new_query: bool,
    pub(crate) show_new_table: bool,
    pub(crate) show_er_diagram_toolbar: bool,
    pub(crate) show_objects_toolbar: bool,
    pub(crate) connecting_entry: Option<ConnectingEntryVm>,
    pub(crate) connecting_name: Option<String>,
    pub(crate) connecting_db_type: Option<String>,
    pub(crate) recent_panels: Vec<WorkspacePanelVm>,
    pub(crate) status_database: Option<String>,
}

/// React's `connCtx`: null without an active panel, otherwise a straight copy.
#[allow(dead_code)] // pure derivation; drop when the host wires the contexts.
pub(crate) fn derive_conn_ctx(
    active_panel: Option<&WorkspacePanelVm>,
) -> Option<WorkspaceConnectionContextVm> {
    active_panel.map(|panel| WorkspaceConnectionContextVm {
        connection_id: panel.connection_id.clone(),
        db_session_id: panel.db_session_id.clone(),
        connection_name: panel.connection_name.clone(),
        database_type: panel.database_type.clone(),
    })
}

/// React's `sidebarConnCtx`: the schema store's active session wins, resolved
/// through the shared `resolveConnectionContext`; a miss (or no session id)
/// falls back to `connCtx`.
#[allow(dead_code)] // pure derivation; drop when the host wires the contexts.
pub(crate) fn derive_sidebar_conn_ctx(
    inputs: &WorkspaceMetaInputsVm,
    conn_ctx: Option<&WorkspaceConnectionContextVm>,
) -> Option<WorkspaceConnectionContextVm> {
    let Some(session) = inputs.store_active_db_session_id.as_deref() else {
        return conn_ctx.cloned();
    };
    if session.is_empty() {
        return conn_ctx.cloned();
    }
    let active: BTreeMap<String, ContentViewActiveConnectionEntry> = inputs
        .active_connections
        .iter()
        .map(|entry| {
            (
                entry.connection_id.clone(),
                ContentViewActiveConnectionEntry {
                    connection_id: entry.connection_id.clone(),
                    db_session_id: entry.db_session_id.clone().unwrap_or_default(),
                },
            )
        })
        .collect();
    match resolve_connection_context(session, &active, &inputs.saved_connections) {
        Some(context) => Some(WorkspaceConnectionContextVm {
            connection_id: context.connection_id,
            db_session_id: context.db_session_id,
            connection_name: context.connection_name,
            database_type: context.database_type,
        }),
        None => conn_ctx.cloned(),
    }
}

/// React's `initialDatabase`: sidebar connection id first, then the panel's;
/// no id at all means `undefined`.
#[allow(dead_code)] // pure derivation; drop when the host wires the dialogs.
pub(crate) fn resolve_initial_database(
    inputs: &WorkspaceMetaInputsVm,
    sidebar_ctx: Option<&WorkspaceConnectionContextVm>,
) -> Option<String> {
    let ctx_connection_id = sidebar_ctx
        .map(|ctx| ctx.connection_id.as_str())
        .filter(|id| !id.is_empty())
        .or_else(|| {
            inputs
                .active_panel
                .as_ref()
                .map(|panel| panel.connection_id.as_str())
                .filter(|id| !id.is_empty())
        })?;
    inputs
        .saved_connections
        .iter()
        .find(|saved| saved.id == ctx_connection_id)
        .and_then(|saved| saved.database.clone())
}

/// React's `toolbarDbType`: the panel's own type beats the sidebar context's.
#[allow(dead_code)] // pure derivation; drop when the host wires the toolbar.
pub(crate) fn resolve_toolbar_db_type(
    inputs: &WorkspaceMetaInputsVm,
    sidebar_ctx: Option<&WorkspaceConnectionContextVm>,
) -> Option<String> {
    inputs
        .active_panel
        .as_ref()
        .map(|panel| panel.database_type.clone())
        .or_else(|| sidebar_ctx.map(|ctx| ctx.database_type.clone()))
}

/// React's `connectingEntry`: only while no panel is active; the first active
/// entry with status `connecting` in store order.
#[allow(dead_code)] // pure derivation; drop when the host wires the landing view.
pub(crate) fn resolve_connecting_entry(
    inputs: &WorkspaceMetaInputsVm,
) -> Option<ConnectingEntryVm> {
    if inputs.active_panel.is_some() {
        return None;
    }
    inputs
        .active_connections
        .iter()
        .find(|entry| entry.status == "connecting")
        .map(|entry| ConnectingEntryVm {
            status: entry.status.clone(),
            connection_id: entry.connection_id.clone(),
            db_session_id: entry.db_session_id.clone(),
        })
}

/// React's `recentPanels`: panels of the sidebar connection, last six first.
#[allow(dead_code)] // pure derivation; drop when the host wires the tabs.
pub(crate) fn recent_panels(
    inputs: &WorkspaceMetaInputsVm,
    sidebar_ctx: Option<&WorkspaceConnectionContextVm>,
) -> Vec<WorkspacePanelVm> {
    let Some(ctx) = sidebar_ctx else {
        return Vec::new();
    };
    let matched: Vec<WorkspacePanelVm> = inputs
        .all_panels
        .iter()
        .filter(|panel| panel.connection_id == ctx.connection_id)
        .cloned()
        .collect();
    let start = matched.len().saturating_sub(6);
    matched[start..].iter().rev().cloned().collect()
}

/// React's `statusDatabase`: a KV panel owns its target database even when the
/// schema store still points somewhere else; `dbName ?? currentDatabase`.
#[allow(dead_code)] // pure derivation; drop when the host wires the status bar.
pub(crate) fn resolve_status_database(
    inputs: &WorkspaceMetaInputsVm,
    panel_meta: Option<&DatabaseTypeMetaVm>,
) -> Option<String> {
    if panel_meta.is_some_and(|meta| meta.is_key_value) {
        if let Some(panel) = inputs.active_panel.as_ref() {
            return panel.db_name.clone().or_else(|| inputs.current_database.clone());
        }
    }
    inputs.current_database.clone()
}

/// The hook's single derivation. Every field comes from the inputs above;
/// calling it twice with the same inputs gives an equal VM.
#[allow(dead_code)] // pure derivation; drop when the connection host wires it.
pub(crate) fn derive_workspace_meta(inputs: &WorkspaceMetaInputsVm) -> WorkspaceMetaVm {
    let database_type = inputs.active_panel.as_ref().map(|panel| panel.database_type.clone());
    let conn_ctx = derive_conn_ctx(inputs.active_panel.as_ref());
    let sidebar_conn_ctx = derive_sidebar_conn_ctx(inputs, conn_ctx.as_ref());
    let initial_database = resolve_initial_database(inputs, sidebar_conn_ctx.as_ref());
    let toolbar_db_type = resolve_toolbar_db_type(inputs, sidebar_conn_ctx.as_ref());
    let panel_meta = database_type.as_ref().and_then(|kind| inputs.db_meta.get(kind));
    let toolbar_meta = toolbar_db_type.as_ref().and_then(|kind| inputs.db_meta.get(kind));
    let toolbar_export_scope = resolve_export_scope(toolbar_meta);
    let has_toolbar_type = toolbar_db_type.is_some();
    let connecting_entry = resolve_connecting_entry(inputs);
    let (connecting_name, connecting_db_type) = match connecting_entry.as_ref() {
        None => (None, None),
        Some(entry) => {
            let saved = inputs
                .saved_connections
                .iter()
                .find(|saved| saved.id == entry.connection_id);
            (
                saved.map(|saved| saved.name.clone()),
                saved.map(|saved| saved.database_type.clone()),
            )
        }
    };

    WorkspaceMetaVm {
        db_session_id: inputs
            .active_panel
            .as_ref()
            .map(|panel| panel.db_session_id.clone())
            .unwrap_or_default(),
        connection_id: inputs
            .active_panel
            .as_ref()
            .map(|panel| panel.connection_id.clone())
            .unwrap_or_default(),
        connection_name: inputs
            .active_panel
            .as_ref()
            .map(|panel| panel.connection_name.clone())
            .unwrap_or_default(),
        has_saved_connections: !inputs.saved_connections.is_empty(),
        export_scope: resolve_export_scope(panel_meta),
        batch_export_supported: supports_full_table_export(toolbar_export_scope),
        show_structure_editor: show_structure_editor(panel_meta),
        is_kv_panel: is_kv_panel(toolbar_meta),
        show_new_query: show_new_query(toolbar_meta, has_toolbar_type),
        show_new_table: show_new_table(toolbar_meta, has_toolbar_type),
        show_er_diagram_toolbar: show_er_diagram_toolbar(toolbar_meta, has_toolbar_type),
        show_objects_toolbar: show_objects_toolbar(toolbar_meta, has_toolbar_type),
        recent_panels: recent_panels(inputs, sidebar_conn_ctx.as_ref()),
        status_database: resolve_status_database(inputs, panel_meta),
        conn_ctx,
        sidebar_conn_ctx,
        initial_database,
        database_type,
        toolbar_db_type,
        connecting_entry,
        connecting_name,
        connecting_db_type,
    }
}
