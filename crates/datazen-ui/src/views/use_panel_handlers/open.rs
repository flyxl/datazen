//! Open/reuse decisions for `usePanelHandlers`. Every handler answers with a
//! [`PanelEffect`] over host-owned panel snapshots; no store, IPC or timer.

use std::collections::BTreeMap;

use super::model::{
    ConnectionContextVm, OpenTargetVm, PanelEffect, PanelKind, PanelPatch, PanelVm, SubTab,
};

/// Which namespace strategy a driver uses for `database`.
#[allow(dead_code)] // VM mirror; supplied by the driver registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NamespaceEnsure {
    DefaultSql,
    Postgresql,
    PathHierarchy,
}

/// A path-hierarchy database pin split into root + namespace path.
#[allow(dead_code)] // VM mirror; supplied by `splitPathHierarchyDatabasePin`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PathPin {
    pub root: String,
    pub namespace_path: Vec<String>,
}

/// React `resolveOpenTarget`: an explicit target wins, but the live
/// `dbSessionId` is looked up from the active-connection table — never the
/// possibly stale id the target carries. Without a target, the sidebar context
/// is used verbatim.
#[allow(dead_code)] // pure rule; consumed by the status/process openers and tests.
pub(crate) fn resolve_open_target(
    target: Option<&OpenTargetVm>,
    sidebar: Option<&ConnectionContextVm>,
    live_sessions: &BTreeMap<String, String>,
) -> Option<ConnectionContextVm> {
    match target {
        Some(target) if !target.connection_id.is_empty() => Some(ConnectionContextVm {
            connection_id: target.connection_id.clone(),
            db_session_id: live_sessions
                .get(&target.connection_id)
                .filter(|id| !id.is_empty())
                .cloned()
                .unwrap_or_else(|| target.db_session_id.clone()),
            connection_name: target.connection_name.clone(),
            database_type: target.database_type.clone(),
        }),
        _ => sidebar.cloned(),
    }
}

/// React `handleSelectTable` (DBR-03): a view/table is reused by name+database,
/// and an explicit schema never lands on another schema's tab. A caller without
/// schema knowledge (null) keeps the legacy name+database match. The arrow's
/// subTab/targetColumn are applied only when a subTab was supplied.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn select_table(
    ctx: Option<&ConnectionContextVm>,
    panels: &[PanelVm],
    schema_views: &[super::model::PanelVm],
    table: &str,
    schema: Option<&str>,
    database: &str,
    sub_tab: Option<SubTab>,
    target_column: Option<&str>,
    path_hierarchy: bool,
) -> PanelEffect {
    let Some(ctx) = ctx else {
        return PanelEffect::None;
    };
    let normalized = if path_hierarchy { None } else { schema.map(str::to_owned) };
    let is_view = schema_views.iter().any(|view| {
        view.kind == Some(PanelKind::View)
            && view.view_name == table
            && (normalized.is_none() || view.view_schema == normalized)
    });

    if is_view {
        if let Some(existing) = panels.iter().find(|panel| {
            panel.kind == Some(PanelKind::View)
                && panel.view_name == table
                && panel.database.as_deref() == Some(database)
                && (normalized.is_none() || panel.view_schema == normalized)
        }) {
            let mut patch = PanelPatch::default();
            if let Some(sub_tab) = sub_tab {
                patch.sub_tab = Some(sub_tab);
            }
            return if patch.sub_tab.is_some() {
                PanelEffect::ActivatePatch { panel_id: existing.id.clone(), patch }
            } else {
                PanelEffect::Activate { panel_id: existing.id.clone() }
            };
        }
        let mut panel = base_panel(ctx, PanelKind::View, "view");
        panel.view_name = table.to_owned();
        panel.view_schema = normalized;
        panel.database = Some(database.to_owned());
        panel.sub_tab = sub_tab.unwrap_or(SubTab::Data);
        return PanelEffect::Add { panel };
    }

    if let Some(existing) = panels.iter().find(|panel| {
        panel.kind == Some(PanelKind::Table)
            && panel.table_name == table
            && panel.database.as_deref() == Some(database)
            && (normalized.is_none() || panel.table_schema == normalized)
    }) {
        let mut patch = PanelPatch::default();
        if let Some(sub_tab) = sub_tab {
            patch.sub_tab = Some(sub_tab);
            patch.target_column = Some(target_column.map(str::to_owned));
        }
        return if patch.sub_tab.is_some() {
            PanelEffect::ActivatePatch { panel_id: existing.id.clone(), patch }
        } else {
            PanelEffect::Activate { panel_id: existing.id.clone() }
        };
    }
    let mut panel = base_panel(ctx, PanelKind::Table, "tbl");
    panel.table_name = table.to_owned();
    panel.table_schema = normalized;
    panel.database = Some(database.to_owned());
    panel.sub_tab = sub_tab.unwrap_or(SubTab::Data);
    panel.target_column = target_column.map(str::to_owned);
    PanelEffect::Add { panel }
}

/// Shared panel identity from a connection context + id prefix. React's
/// `nextPanelId` is host-owned; `prefix` is the wire spelling it would use.
fn base_panel(ctx: &ConnectionContextVm, kind: PanelKind, prefix: &str) -> PanelVm {
    PanelVm {
        id: format!("{prefix}-1"),
        kind: Some(kind),
        connection_id: ctx.connection_id.clone(),
        db_session_id: ctx.db_session_id.clone(),
        connection_name: ctx.connection_name.clone(),
        database_type: ctx.database_type.clone(),
        ..PanelVm::default()
    }
}

/// React's generic "reuse by kind, else add" handler (objects / privileges /
/// create-table). `id_prefix` mirrors `nextPanelId`'s argument.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn open_by_kind(
    ctx: Option<&ConnectionContextVm>,
    panels: &[PanelVm],
    kind: PanelKind,
    id_prefix: &str,
) -> PanelEffect {
    let Some(ctx) = ctx else {
        return PanelEffect::None;
    };
    if let Some(existing) = panels.iter().find(|panel| panel.kind == Some(kind)) {
        return PanelEffect::Activate { panel_id: existing.id.clone() };
    }
    PanelEffect::Add { panel: base_panel(ctx, kind, id_prefix) }
}

/// React `handleEditTableStructure`: reuse the table panel and force
/// `subTab: structure, structureEditing: true`, else add one so configured.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn edit_table_structure(
    ctx: Option<&ConnectionContextVm>,
    panels: &[PanelVm],
    name: &str,
    fallbacks: (&str, &str),
) -> PanelEffect {
    let Some(ctx) = ctx else {
        return PanelEffect::None;
    };
    if let Some(existing) = panels.iter().find(|panel| {
        panel.kind == Some(PanelKind::Table) && panel.table_name == name
    }) {
        return PanelEffect::ActivatePatch {
            panel_id: existing.id.clone(),
            patch: PanelPatch {
                sub_tab: Some(SubTab::Structure),
                structure_editing: Some(true),
                ..PanelPatch::default()
            },
        };
    }
    let mut panel = base_panel(ctx, PanelKind::Table, "tbl");
    panel.table_name = name.to_owned();
    panel.database = Some(fallbacks.0.to_owned());
    panel.sub_tab = SubTab::Structure;
    panel.structure_editing = true;
    // fallbacks.1 would be lastTableSchema; this handler hardcodes null.
    let _ = fallbacks.1;
    PanelEffect::Add { panel }
}

/// React `handleOpenStructure`: with the structure editor enabled this becomes
/// the edit handler; otherwise it opens the read-only structure subTab.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn open_structure(
    ctx: Option<&ConnectionContextVm>,
    panels: &[PanelVm],
    name: &str,
    show_structure_editor: bool,
    fallbacks: (&str, &str),
) -> PanelEffect {
    if show_structure_editor {
        return edit_table_structure(ctx, panels, name, fallbacks);
    }
    let Some(ctx) = ctx else {
        return PanelEffect::None;
    };
    if let Some(existing) = panels.iter().find(|panel| {
        panel.kind == Some(PanelKind::Table) && panel.table_name == name
    }) {
        return PanelEffect::ActivatePatch {
            panel_id: existing.id.clone(),
            patch: PanelPatch { sub_tab: Some(SubTab::Structure), ..PanelPatch::default() },
        };
    }
    let mut panel = base_panel(ctx, PanelKind::Table, "tbl");
    panel.table_name = name.to_owned();
    panel.database = Some(fallbacks.0.to_owned());
    panel.sub_tab = SubTab::Structure;
    PanelEffect::Add { panel }
}

/// React `handleOpenErDiagram`: the schema comes from the focused relation (if
/// resolvable) else the active panel; the database is the explicit target, else
/// the current/initial selection. A reused diagram is re-bound to a new
/// database only for an explicit target or a focus-less (toolbar) invocation.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn open_er_diagram(
    ctx: Option<&ConnectionContextVm>,
    panels: &[PanelVm],
    active_panel_id: Option<&str>,
    focus: Option<&str>,
    target_database: Option<&str>,
    current_database: Option<&str>,
    initial_database: Option<&str>,
    resolve_focus_schema: Option<&dyn Fn(&str) -> Option<String>>,
    view_schemas: &[(String, Option<String>)],
) -> PanelEffect {
    let Some(ctx) = ctx else {
        return PanelEffect::None;
    };
    let er_database = target_database
        .or(current_database)
        .or(initial_database)
        .map(str::to_owned);
    let active_schema = active_panel_id
        .and_then(|id| panels.iter().find(|panel| panel.id == id))
        .and_then(super::model::panel_schema);
    let focused_schema = focus.and_then(|table| {
        resolve_focus_schema
            .and_then(|resolve| resolve(table))
            .or_else(|| {
                view_schemas
                    .iter()
                    .find(|(name, _)| name == table)
                    .and_then(|(_, schema)| schema.clone())
            })
    });
    let schema = focused_schema.or(active_schema);

    let mut patch = PanelPatch { schema: Some(schema.clone()), ..PanelPatch::default() };
    if let Some(focus) = focus {
        patch.focus_table = Some(Some(focus.to_owned()));
    }
    let rebind = (target_database.is_some() || focus.is_none()) && er_database.is_some();
    if rebind {
        patch.database = Some(er_database.clone());
    }
    if let Some(existing) = panels.iter().find(|panel| panel.kind == Some(PanelKind::ErDiagram)) {
        return PanelEffect::ActivatePatch { panel_id: existing.id.clone(), patch };
    }
    let mut panel = base_panel(ctx, PanelKind::ErDiagram, "er");
    panel.database = er_database;
    panel.schema = schema;
    panel.focus_table = focus.map(str::to_owned);
    PanelEffect::Add { panel }
}

/// React `handleOpenDbObject`: the panel identity is the full tuple (kind,
/// name, schema, signature, target schema/name, database), so overloads and
/// trigger targets never share a tab.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn open_db_object(
    ctx: Option<&ConnectionContextVm>,
    panels: &[PanelVm],
    kind: &str,
    name: &str,
    schema: Option<&str>,
    signature: Option<&str>,
    target_schema: Option<&str>,
    target_name: Option<&str>,
    database: Option<&str>,
    current_database: Option<&str>,
    initial_database: Option<&str>,
) -> PanelEffect {
    let Some(ctx) = ctx else {
        return PanelEffect::None;
    };
    let target_database = database.or(current_database).or(initial_database).unwrap_or("");
    if let Some(existing) = panels.iter().find(|panel| {
        panel.kind == Some(PanelKind::DbObject)
            && panel.object_name == name
            && panel.object_kind == kind
            && panel.object_schema.as_deref() == schema
            && panel.object_signature.as_deref() == signature
            && panel.object_target_schema.as_deref() == target_schema
            && panel.object_target_name.as_deref() == target_name
            && panel.object_database.as_deref() == Some(target_database)
    }) {
        return PanelEffect::Activate { panel_id: existing.id.clone() };
    }
    let mut panel = base_panel(ctx, PanelKind::DbObject, "dbobj");
    panel.object_kind = kind.to_owned();
    panel.object_name = name.to_owned();
    panel.object_schema = schema.map(str::to_owned);
    panel.object_database = Some(target_database.to_owned());
    panel.object_signature = signature.map(str::to_owned);
    panel.object_target_schema = target_schema.map(str::to_owned);
    panel.object_target_name = target_name.map(str::to_owned);
    PanelEffect::Add { panel }
}

/// React `handleOpenServerStatus` / `handleOpenProcessList`: unique per
/// connection across ALL panels (not just this sidebar's).
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn open_connection_panel(
    ctx: Option<&ConnectionContextVm>,
    all_panels: &[PanelVm],
    kind: PanelKind,
    id_prefix: &str,
) -> PanelEffect {
    let Some(ctx) = ctx else {
        return PanelEffect::None;
    };
    if let Some(existing) = all_panels.iter().find(|panel| {
        panel.kind == Some(kind) && panel.connection_id == ctx.connection_id
    }) {
        return PanelEffect::Activate { panel_id: existing.id.clone() };
    }
    PanelEffect::Add { panel: base_panel(ctx, kind, id_prefix) }
}

/// React `handleNewQuery` (database binding at creation time). The explicit
/// target wins over the store pointer, then the session pointer, then initial.
/// A path-hierarchy driver splits the pin into root (bound database) plus the
/// whole root-first namespace path. A title is derived when none is supplied.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn new_query(
    ctx: Option<&ConnectionContextVm>,
    initial_sql: Option<&str>,
    target_database: Option<&str>,
    target_schema: Option<&str>,
    title: Option<&str>,
    current_database: Option<&str>,
    session_current_database: Option<&str>,
    initial_database: Option<&str>,
    namespace: NamespaceEnsure,
    split: Option<PathPin>,
) -> PanelEffect {
    let Some(ctx) = ctx else {
        return PanelEffect::None; // React returns false
    };
    let trimmed_target = target_database.map(str::trim).filter(|s| !s.is_empty());
    let trimmed_current = current_database.map(str::trim).filter(|s| !s.is_empty());
    let trimmed_session = session_current_database.map(str::trim).filter(|s| !s.is_empty());
    let raw = trimmed_target.or(trimmed_current).or(trimmed_session);

    let (bound, namespace_path) = if namespace == NamespaceEnsure::PathHierarchy && raw.is_some() {
        let split = split.unwrap_or_default();
        // React binds the root as the panel database but keeps the WHOLE
        // root-first path on the tab (root + catalog/schema) for restoration.
        let path: Vec<String> = std::iter::once(split.root.clone())
            .chain(split.namespace_path)
            .filter(|segment| !segment.is_empty())
            .collect();
        let root = (!split.root.is_empty()).then_some(split.root);
        (root, path)
    } else {
        (raw.map(str::to_owned), Vec::new())
    };

    let db_for_title = raw.or(initial_database).unwrap_or("");
    let derived_title = if db_for_title.is_empty() {
        ctx.connection_name.clone()
    } else {
        format!("{}@{}", ctx.connection_name, db_for_title)
    };
    let mut panel = base_panel(ctx, PanelKind::Query, "qry");
    panel.title = title.map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&derived_title).to_owned();
    panel.database = Some(bound.unwrap_or_default());
    panel.schema = target_schema.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    panel.namespace_path = namespace_path;

    match initial_sql {
        Some(sql) => PanelEffect::AddSql { panel, sql: sql.to_owned() },
        None => PanelEffect::Add { panel },
    }
}

/// React `handleOpenTableAction`'s routing decision. A different connection is
/// ignored; `openData` re-enters `selectTable`; every other action opens a query
/// tab carrying the spec's initial SQL.
#[allow(dead_code)] // pure rule; remove with the host wiring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableActionRoute {
    Ignore,
    OpenData,
    OpenQuery,
}

#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn table_action_route(
    ctx: Option<&ConnectionContextVm>,
    input_connection_id: &str,
    open_data: bool,
) -> TableActionRoute {
    match ctx {
        None => TableActionRoute::Ignore,
        Some(ctx) if input_connection_id != ctx.connection_id => TableActionRoute::Ignore,
        Some(_) if open_data => TableActionRoute::OpenData,
        Some(_) => TableActionRoute::OpenQuery,
    }
}

/// React's pending-intent guards: a pending request is consumed only when its
/// connection id matches the live sidebar context.
#[allow(dead_code)] // pure guard; remove with the host wiring.
pub(crate) fn pending_matches(ctx: Option<&ConnectionContextVm>, pending_connection_id: Option<&str>) -> bool {
    match (ctx, pending_connection_id) {
        (Some(ctx), Some(pending)) => !pending.is_empty() && ctx.connection_id == pending,
        _ => false,
    }
}
