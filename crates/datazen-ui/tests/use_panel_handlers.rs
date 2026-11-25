//! T622: `usePanelHandlers` logic — the React test cases as pure decisions:
//! query database binding (incl. path hierarchy), ER schema/database rules,
//! DBR-03 table identity, DB-object identity, reuse-by-kind, per-connection
//! panels, close/menu/subTab/refresh transitions and pending-intent guards.

use std::collections::BTreeMap;

use datazen_ui::views::use_panel_handlers::{
    close_all_panels, close_other_panels, close_panel, close_panels_side, edit_table_structure,
    new_query, open_by_kind, open_connection_panel, open_db_object, open_er_diagram,
    open_structure, pending_matches, refresh, resolve_open_target, select_table, set_sub_tab,
    tab_context_menu_items, table_action_route, ConnectionContextVm, NamespaceEnsure, OpenTargetVm,
    PanelEffect, PanelKind, PanelPatch, PanelVm, PathPin, SubTab, TabMenuItem, TableActionRoute,
};

fn ctx() -> ConnectionContextVm {
    ConnectionContextVm {
        connection_id: "conn-1".to_owned(),
        db_session_id: "sess-1".to_owned(),
        connection_name: "MyConn".to_owned(),
        database_type: "postgresql".to_owned(),
    }
}

fn panel(id: &str, kind: PanelKind) -> PanelVm {
    PanelVm { id: id.to_owned(), kind: Some(kind), connection_id: "conn-1".to_owned(), ..PanelVm::default() }
}

fn added(effect: &PanelEffect) -> &PanelVm {
    match effect {
        PanelEffect::Add { panel } | PanelEffect::AddSql { panel, .. } => panel,
        other => panic!("expected Add, got {other:?}"),
    }
}

// ── new query binding ────────────────────────────────────────────────────────

#[test]
fn new_query_binds_store_current_database() {
    let effect = new_query(Some(&ctx()), None, None, None, None, Some("db_b"), None, None, NamespaceEnsure::DefaultSql, None);
    assert_eq!(added(&effect).database.as_deref(), Some("db_b"));
    assert_eq!(added(&effect).title, "MyConn@db_b");
}

#[test]
fn new_query_prefers_explicit_target_and_keeps_title_override() {
    let effect = new_query(Some(&ctx()), None, Some("db_a"), Some("public"), Some("  My Title  "), Some("db_b"), None, None, NamespaceEnsure::DefaultSql, None);
    let panel = added(&effect);
    assert_eq!(panel.database.as_deref(), Some("db_a"));
    assert_eq!(panel.schema.as_deref(), Some("public"));
    assert_eq!(panel.title, "My Title");
}

#[test]
fn new_query_falls_back_to_session_pointer_then_initial() {
    let effect = new_query(Some(&ctx()), None, None, None, None, None, Some("db_s"), Some("db_i"), NamespaceEnsure::DefaultSql, None);
    assert_eq!(added(&effect).database.as_deref(), Some("db_s"));
    let effect = new_query(Some(&ctx()), None, None, None, None, None, None, Some("db_i"), NamespaceEnsure::DefaultSql, None);
    // initialDatabase only feeds the title; the bound database stays '' (React's
    // `boundDatabase ?? ''`), so a later session pointer is not shadowed.
    assert_eq!(added(&effect).database.as_deref(), Some(""));
    assert_eq!(added(&effect).title, "MyConn@db_i");
}

#[test]
fn new_query_binds_whole_path_hierarchy() {
    let pin = PathPin { root: "558:hive".to_owned(), namespace_path: vec!["snap".to_owned()] };
    let effect = new_query(Some(&ctx()), None, None, None, None, Some("558:hive/snap"), None, None, NamespaceEnsure::PathHierarchy, Some(pin));
    let panel = added(&effect);
    assert_eq!(panel.database.as_deref(), Some("558:hive"));
    assert_eq!(panel.namespace_path, ["558:hive", "snap"]);
    assert_eq!(panel.title, "MyConn@558:hive/snap", "title uses the raw pin");
}

#[test]
fn new_query_without_context_or_with_sql() {
    assert_eq!(new_query(None, None, None, None, None, None, None, None, NamespaceEnsure::DefaultSql, None), PanelEffect::None);
    let effect = new_query(Some(&ctx()), Some("SELECT 1"), None, None, None, Some("db"), None, None, NamespaceEnsure::DefaultSql, None);
    assert_eq!(effect, PanelEffect::AddSql { panel: added(&effect).clone(), sql: "SELECT 1".to_owned() });
}

// ── ER diagram ───────────────────────────────────────────────────────────────

#[test]
fn er_inherits_active_panel_schema_and_binds_database() {
    let mut active = panel("tbl-1", PanelKind::Table);
    active.table_schema = Some("reporting".to_owned());
    let effect = open_er_diagram(Some(&ctx()), &[active], Some("tbl-1"), None, None, Some("db_a"), None, None, &[]);
    let panel = added(&effect);
    assert_eq!(panel.schema.as_deref(), Some("reporting"));
    assert_eq!(panel.database.as_deref(), Some("db_a"));
}

#[test]
fn er_focus_schema_beats_active_panel() {
    let mut active = panel("tbl-1", PanelKind::Table);
    active.schema = Some("reporting".to_owned());
    let resolver = |table: &str| (table == "orders").then(|| "sales".to_owned());
    let effect = open_er_diagram(Some(&ctx()), &[active], Some("tbl-1"), Some("orders"), None, Some("db_a"), None, Some(&resolver), &[]);
    let panel = added(&effect);
    assert_eq!(panel.schema.as_deref(), Some("sales"));
    assert_eq!(panel.focus_table.as_deref(), Some("orders"));
}

#[test]
fn er_reuse_rebinds_only_for_explicit_target_or_focusless_call() {
    let existing = panel("er-1", PanelKind::ErDiagram);
    // Focus-only invocation keeps the tab's own database, but still updates schema/focus.
    let effect = open_er_diagram(Some(&ctx()), &[existing.clone()], None, Some("orders"), None, Some("app"), None, None, &[]);
    match effect {
        PanelEffect::ActivatePatch { patch, .. } => {
            assert_eq!(patch.database, None, "focus-only keeps database");
            assert_eq!(patch.focus_table, Some(Some("orders".to_owned())));
        }
        other => panic!("expected ActivatePatch, got {other:?}"),
    }
    // Explicit target rebinds the database.
    let effect = open_er_diagram(Some(&ctx()), &[existing], None, None, Some("analytics"), Some("app"), None, None, &[]);
    match effect {
        PanelEffect::ActivatePatch { patch, .. } => assert_eq!(patch.database, Some(Some("analytics".to_owned()))),
        other => panic!("expected ActivatePatch, got {other:?}"),
    }
}

// ── table identity / structure ───────────────────────────────────────────────

#[test]
fn select_table_keys_by_schema_and_reuses_matching_schema() {
    let first = select_table(Some(&ctx()), &[], &[], "orders", Some("sales"), "db_a", None, None, false);
    let sales = panel("tbl-1", PanelKind::Table);
    let mut sales = sales;
    sales.table_name = "orders".to_owned();
    sales.table_schema = Some("sales".to_owned());
    sales.database = Some("db_a".to_owned());
    let second = select_table(Some(&ctx()), &[sales], &[], "orders", Some("archive"), "db_a", None, None, false);
    assert!(matches!(first, PanelEffect::Add { .. }));
    assert!(matches!(second, PanelEffect::Add { .. }), "different schema = second tab");
    let third = select_table(Some(&ctx()), &[panel_with_table("tbl-1", "orders", Some("sales"), "db_a")], &[], "orders", Some("sales"), "db_a", None, None, false);
    assert!(matches!(third, PanelEffect::Activate { .. }), "same schema reuses tab");
}

#[test]
fn select_table_null_schema_keeps_legacy_name_match() {
    let existing = panel_with_table("tbl-1", "orders", Some("sales"), "db_a");
    let effect = select_table(Some(&ctx()), &[existing], &[], "orders", None, "db_a", None, None, false);
    assert!(matches!(effect, PanelEffect::Activate { .. }));
}

fn panel_with_table(id: &str, table: &str, schema: Option<&str>, db: &str) -> PanelVm {
    let mut panel = panel(id, PanelKind::Table);
    panel.table_name = table.to_owned();
    panel.table_schema = schema.map(str::to_owned);
    panel.database = Some(db.to_owned());
    panel
}

#[test]
fn structure_editor_gate_and_subtab_transitions() {
    let effect = open_structure(Some(&ctx()), &[], "orders", true, ("db_a", ""));
    assert!(added(&effect).structure_editing, "showStructureEditor routes to edit");
    let effect = open_structure(Some(&ctx()), &[], "orders", false, ("db_a", ""));
    assert!(!added(&effect).structure_editing, "read-only structure has no editing");

    let table = panel_with_table("tbl-1", "orders", None, "db_a");
    let effect = set_sub_tab(Some(&table), SubTab::Data);
    match effect {
        PanelEffect::Patch { patch, .. } => assert_eq!(patch.structure_editing, Some(false)),
        other => panic!("expected Patch, got {other:?}"),
    }
    let effect = set_sub_tab(Some(&panel("v-1", PanelKind::View)), SubTab::Structure);
    match effect {
        PanelEffect::Patch { patch, .. } => assert_eq!(patch.structure_editing, None, "views have no editing flag"),
        other => panic!("expected Patch, got {other:?}"),
    }
    assert_eq!(set_sub_tab(None, SubTab::Data), PanelEffect::None);
    assert_eq!(set_sub_tab(Some(&panel("q-1", PanelKind::Query)), SubTab::Data), PanelEffect::None);
}

#[test]
fn edit_structure_reuses_and_forces_editing() {
    let table = panel_with_table("tbl-1", "orders", None, "db_a");
    let effect = edit_table_structure(Some(&ctx()), &[table], "orders", ("db_a", ""));
    match effect {
        PanelEffect::ActivatePatch { patch, .. } => {
            assert_eq!(patch.sub_tab, Some(SubTab::Structure));
            assert_eq!(patch.structure_editing, Some(true));
        }
        other => panic!("expected ActivatePatch, got {other:?}"),
    }
}

// ── db object identity, reuse, connection panels ─────────────────────────────

#[test]
fn db_object_identity_includes_signature_target_and_database() {
    let base = open_db_object(Some(&ctx()), &[], "function", "lookup", Some("public"), Some("integer"), None, None, None, Some("db_a"), None);
    let mut obj = added(&base).clone();
    obj.id = "dbobj-1".to_owned();
    let same = open_db_object(Some(&ctx()), &[obj.clone()], "function", "lookup", Some("public"), Some("integer"), None, None, None, Some("db_a"), None);
    assert!(matches!(same, PanelEffect::Activate { .. }), "identical tuple reuses");
    let other_sig = open_db_object(Some(&ctx()), &[obj.clone()], "function", "lookup", Some("public"), Some("text"), None, None, None, Some("db_a"), None);
    assert!(matches!(other_sig, PanelEffect::Add { .. }), "different overload opens a tab");
    let other_db = open_db_object(Some(&ctx()), &[obj], "function", "lookup", Some("public"), Some("integer"), None, None, Some("db_b"), Some("db_a"), None);
    assert!(matches!(other_db, PanelEffect::Add { .. }), "different database opens a tab");
}

#[test]
fn open_by_kind_reuses_then_adds() {
    let existing = panel("obj-1", PanelKind::Objects);
    assert!(matches!(open_by_kind(Some(&ctx()), &[existing], PanelKind::Objects, "obj"), PanelEffect::Activate { .. }));
    assert!(matches!(open_by_kind(Some(&ctx()), &[], PanelKind::Privileges, "priv"), PanelEffect::Add { .. }));
    assert_eq!(open_by_kind(None, &[], PanelKind::Objects, "obj"), PanelEffect::None);
}

#[test]
fn connection_panels_are_unique_per_connection() {
    let mut other_conn = panel("status-2", PanelKind::ServerStatus);
    other_conn.connection_id = "conn-2".to_owned();
    assert!(matches!(open_connection_panel(Some(&ctx()), &[other_conn], PanelKind::ServerStatus, "status"), PanelEffect::Add { .. }));
    let mine = panel("status-1", PanelKind::ServerStatus);
    assert!(matches!(open_connection_panel(Some(&ctx()), &[mine], PanelKind::ServerStatus, "status"), PanelEffect::Activate { .. }));
}

// ── open target, pending intents, table action routing ───────────────────────

#[test]
fn explicit_target_uses_live_session_id() {
    let target = OpenTargetVm { connection_id: "conn-1".to_owned(), db_session_id: "stale".to_owned(), connection_name: "MyConn".to_owned(), database_type: "postgresql".to_owned() };
    let live = BTreeMap::from([("conn-1".to_owned(), "fresh".to_owned())]);
    let resolved = resolve_open_target(Some(&target), Some(&ctx()), &live).unwrap();
    assert_eq!(resolved.db_session_id, "fresh");
    let no_live = resolve_open_target(Some(&target), Some(&ctx()), &BTreeMap::new()).unwrap();
    assert_eq!(no_live.db_session_id, "stale", "falls back to the target id");
    assert_eq!(resolve_open_target(None, Some(&ctx()), &live), Some(ctx()));
}

#[test]
fn pending_intents_match_only_the_live_connection() {
    assert!(pending_matches(Some(&ctx()), Some("conn-1")));
    assert!(!pending_matches(Some(&ctx()), Some("conn-2")));
    assert!(!pending_matches(None, Some("conn-1")));
    assert!(!pending_matches(Some(&ctx()), None));
}

#[test]
fn table_action_routes_by_connection_and_open_data() {
    assert_eq!(table_action_route(Some(&ctx()), "conn-1", false), TableActionRoute::OpenQuery);
    assert_eq!(table_action_route(Some(&ctx()), "conn-1", true), TableActionRoute::OpenData);
    assert_eq!(table_action_route(Some(&ctx()), "conn-2", false), TableActionRoute::Ignore);
    assert_eq!(table_action_route(None, "conn-1", false), TableActionRoute::Ignore);
}

// ── close / menu / refresh transitions ───────────────────────────────────────

#[test]
fn close_transitions() {
    let panels: Vec<PanelVm> = ["a", "b", "c", "d"].iter().map(|id| panel(id, PanelKind::Query)).collect();
    assert_eq!(close_panel("b"), PanelEffect::Close { panel_ids: vec!["b".to_owned()] });
    assert_eq!(close_other_panels(&panels, "b"), PanelEffect::Close { panel_ids: vec!["a".to_owned(), "c".to_owned(), "d".to_owned()] });
    assert_eq!(close_all_panels(&panels), PanelEffect::Close { panel_ids: vec!["a".to_owned(), "b".to_owned(), "c".to_owned(), "d".to_owned()] });
    assert_eq!(close_panels_side(&panels, "b", true), PanelEffect::Close { panel_ids: vec!["c".to_owned(), "d".to_owned()] });
    assert_eq!(close_panels_side(&panels, "c", false), PanelEffect::Close { panel_ids: vec!["a".to_owned(), "b".to_owned()] });
    assert_eq!(close_panels_side(&panels, "missing", true), PanelEffect::None);
}

#[test]
fn context_menu_flags_follow_tab_position() {
    let panels: Vec<PanelVm> = ["a", "b", "c"].iter().map(|id| panel(id, PanelKind::Query)).collect();
    assert_eq!(tab_context_menu_items(&panels, "a"), vec![TabMenuItem::Close, TabMenuItem::CloseOthers, TabMenuItem::CloseToRight, TabMenuItem::CloseAll]);
    assert_eq!(tab_context_menu_items(&panels, "b"), vec![TabMenuItem::Close, TabMenuItem::CloseOthers, TabMenuItem::CloseToRight, TabMenuItem::CloseToLeft, TabMenuItem::CloseAll]);
    assert_eq!(tab_context_menu_items(&panels, "c"), vec![TabMenuItem::Close, TabMenuItem::CloseOthers, TabMenuItem::CloseToLeft, TabMenuItem::CloseAll]);
    assert_eq!(tab_context_menu_items(&[], "a"), vec![TabMenuItem::Close, TabMenuItem::CloseOthers, TabMenuItem::CloseAll]);
}

#[test]
fn refresh_routes_by_database_presence_and_session() {
    assert_eq!(refresh(Some(&ctx()), Some("db_a")), PanelEffect::Refresh { db_session_id: "sess-1".to_owned(), database: Some("db_a".to_owned()), database_type: "postgresql".to_owned() });
    assert_eq!(refresh(Some(&ctx()), None), PanelEffect::Refresh { db_session_id: "sess-1".to_owned(), database: None, database_type: "postgresql".to_owned() });
    let mut empty = ctx();
    empty.db_session_id.clear();
    assert_eq!(refresh(Some(&empty), Some("db_a")), PanelEffect::None);
    assert_eq!(refresh(None, Some("db_a")), PanelEffect::None);
}

#[test]
fn patch_applies_clearing_values() {
    let mut panel = panel_with_table("tbl-1", "orders", Some("sales"), "db_a");
    let patch = PanelPatch { schema: Some(None), structure_editing: Some(false), ..PanelPatch::default() };
    patch.apply(&mut panel);
    assert_eq!(panel.schema, None);
    assert!(!panel.structure_editing);
}

// ── T622 independent audit (appended; reuses this module's helpers) ──────

/// React `panelSchema` (panelTypes.ts:214-235) returns `viewSchema` for a view
/// panel. The port's `active_schema` only reads `schema` then `table_schema`,
/// so opening an ER diagram while a VIEW tab is active loses that namespace.
#[test]
fn audit_er_inherits_active_view_panel_schema() {
    let mut active = panel("v-1", PanelKind::View);
    active.view_schema = Some("reporting".to_owned());
    let effect = open_er_diagram(
        Some(&ctx()), &[active], Some("v-1"), None, None,
        Some("db_a"), None, None, &[],
    );
    assert_eq!(
        added(&effect).schema.as_deref(),
        Some("reporting"),
        "React panelSchema(view) inherits the view's schema"
    );
}

/// React `panelSchema` returns `objectSchema` for a db-object panel. The port
/// never reads `object_schema`, so the same namespace loss affects that case.
#[test]
fn audit_er_inherits_active_db_object_panel_schema() {
    let mut active = panel("obj-1", PanelKind::DbObject);
    active.object_schema = Some("reporting".to_owned());
    let effect = open_er_diagram(
        Some(&ctx()), &[active], Some("obj-1"), None, None,
        Some("db_a"), None, None, &[],
    );
    assert_eq!(
        added(&effect).schema.as_deref(),
        Some("reporting"),
        "React panelSchema(db-object) inherits the object's schema"
    );
}

/// React `buildConnectionTabContextMenuItems` (connectionTabContextMenu.ts:60-66)
/// pushes items in the order close, close-others, close-to-the-right,
/// close-to-the-left, **close-all last**. The port puts CloseAll third.
#[test]
fn audit_context_menu_order_matches_react_push_order() {
    let panels: Vec<PanelVm> = ["a", "b", "c"].iter().map(|id| panel(id, PanelKind::Query)).collect();
    assert_eq!(
        tab_context_menu_items(&panels, "b"),
        vec![
            TabMenuItem::Close,
            TabMenuItem::CloseOthers,
            TabMenuItem::CloseToRight,
            TabMenuItem::CloseToLeft,
            TabMenuItem::CloseAll,
        ],
        "React pushes close-all after the to-the-right/to-the-left entries"
    );
}
