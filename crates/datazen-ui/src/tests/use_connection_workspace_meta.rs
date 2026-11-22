//! T612: `useConnectionWorkspaceMeta` as a pure derivation — context resolution
//! and fallback, driver-capability guards, toolbar meta vs panel meta, the
//! connecting entry, recent-panel ordering and the KV status override.

use std::collections::BTreeMap;

use datazen_ui_api::SavedConnection;

use crate::views::{
    can_open, derive_conn_ctx, derive_sidebar_conn_ctx, derive_workspace_meta, is_kv_panel,
    resolve_export_scope, resolve_initial_database, resolve_status_database, resolve_toolbar_db_type,
    show_er_diagram_toolbar, show_new_query, show_new_table, show_objects_toolbar,
    show_structure_editor, supports_full_table_export, DataExportCapabilityVm,
    DatabaseTypeMetaVm, MetaConnectionViewVm, WorkspaceActiveEntryVm, WorkspaceMetaInputsVm,
    WorkspacePanelVm,
};

fn saved(id: &str, name: &str, db_type: &str, database: Option<&str>) -> SavedConnection {
    SavedConnection {
        id: id.to_owned(),
        name: name.to_owned(),
        database_type: db_type.to_owned(),
        database: database.map(str::to_owned),
    }
}

fn panel(id: &str, conn: &str, session: &str, db_type: &str) -> WorkspacePanelVm {
    WorkspacePanelVm {
        id: id.to_owned(),
        connection_id: conn.to_owned(),
        db_session_id: session.to_owned(),
        connection_name: name_of(conn),
        database_type: db_type.to_owned(),
        db_name: None,
    }
}

fn name_of(conn: &str) -> String {
    match conn {
        "c1" => "Conn One".to_owned(),
        "c2" => "Conn Two".to_owned(),
        other => other.to_owned(),
    }
}

fn sql_meta() -> DatabaseTypeMetaVm {
    DatabaseTypeMetaVm {
        is_key_value: false,
        connection_view: MetaConnectionViewVm::Sql,
        structure_editor_enabled: None,
        supports_sql: Some(true),
        is_read_only: false,
        supports_er_diagram: Some(true),
        export_scope: DataExportCapabilityVm::FullTable,
    }
}

fn kv_meta() -> DatabaseTypeMetaVm {
    DatabaseTypeMetaVm {
        is_key_value: true,
        connection_view: MetaConnectionViewVm::Keyvalue,
        export_scope: DataExportCapabilityVm::LoadedOnly,
        ..sql_meta()
    }
}

fn inputs() -> WorkspaceMetaInputsVm {
    let mut db_meta = BTreeMap::new();
    db_meta.insert("postgresql".to_owned(), sql_meta());
    db_meta.insert("redis".to_owned(), kv_meta());
    db_meta.insert(
        "limited".to_owned(),
        DatabaseTypeMetaVm {
            export_scope: DataExportCapabilityVm::LoadedOnly,
            ..sql_meta()
        },
    );
    let mut all_panels: Vec<WorkspacePanelVm> = (1..=8)
        .map(|i| panel(&format!("p{i}"), "c1", "s1", "postgresql"))
        .collect();
    all_panels.push(panel("p9", "c2", "", "redis"));
    all_panels.push(panel("p10", "c2", "", "redis"));
    WorkspaceMetaInputsVm {
        active_panel: Some(panel("p1", "c1", "s1", "postgresql")),
        saved_connections: vec![
            saved("c1", "Conn One", "postgresql", Some("first")),
            saved("c2", "Conn Two", "redis", Some("second")),
        ],
        active_connections: vec![
            WorkspaceActiveEntryVm {
                connection_id: "c1".to_owned(),
                db_session_id: Some("s1".to_owned()),
                status: "connected".to_owned(),
            },
            WorkspaceActiveEntryVm {
                connection_id: "c2".to_owned(),
                db_session_id: Some("s2".to_owned()),
                status: "connecting".to_owned(),
            },
        ],
        store_active_db_session_id: None,
        current_database: Some("db0".to_owned()),
        all_panels,
        db_meta,
    }
}

#[test]
fn no_panel_gives_null_contexts_and_no_connecting_entry() {
    let meta = derive_workspace_meta(&WorkspaceMetaInputsVm::default());
    assert!(meta.conn_ctx.is_none() && meta.sidebar_conn_ctx.is_none());
    assert!(meta.initial_database.is_none() && meta.database_type.is_none());
    assert!(meta.toolbar_db_type.is_none());
    assert!(!meta.has_saved_connections, "an empty saved list is pinned");
    assert!(meta.db_session_id.is_empty() && meta.connection_id.is_empty());
    assert!(meta.connection_name.is_empty());
    assert!(meta.recent_panels.is_empty());
    assert!(meta.connecting_entry.is_none());
    assert!(meta.connecting_name.is_none() && meta.connecting_db_type.is_none());
    assert_eq!(meta.status_database.as_deref(), None);
    assert!(!meta.show_new_query && !meta.show_new_table);
    assert!(!meta.show_er_diagram_toolbar && !meta.show_objects_toolbar);
    assert!(!meta.show_structure_editor && !meta.is_kv_panel);
    assert_eq!(meta.export_scope, DataExportCapabilityVm::FullTable);
    assert!(meta.batch_export_supported, "absent metadata defaults to full_table");
}

#[test]
fn active_panel_builds_conn_ctx_and_identifiers() {
    let inputs = inputs();
    let meta = derive_workspace_meta(&inputs);
    let ctx = meta.conn_ctx.expect("an active panel has a context");
    assert_eq!(ctx.connection_id, "c1");
    assert_eq!(ctx.db_session_id, "s1");
    assert_eq!(ctx.connection_name, "Conn One");
    assert_eq!(ctx.database_type, "postgresql");
    assert_eq!(meta.db_session_id, "s1");
    assert_eq!(meta.connection_id, "c1");
    assert_eq!(meta.connection_name, "Conn One");
    assert_eq!(meta.database_type.as_deref(), Some("postgresql"));
    assert_eq!(meta.toolbar_db_type.as_deref(), Some("postgresql"));
    assert_eq!(meta.initial_database.as_deref(), Some("first"));
    assert!(meta.has_saved_connections);
    assert_eq!(derive_conn_ctx(None), None);
}

#[test]
fn sidebar_context_resolves_by_session_and_falls_back() {
    let mut inputs = inputs();
    inputs.store_active_db_session_id = Some("s2".to_owned());
    let conn_ctx = derive_conn_ctx(inputs.active_panel.as_ref());
    let sidebar = derive_sidebar_conn_ctx(&inputs, conn_ctx.as_ref()).expect("s2 resolves");
    assert_eq!(sidebar.connection_id, "c2", "the schema store's session wins");
    assert_eq!(sidebar.connection_name, "Conn Two");
    assert_eq!(sidebar.database_type, "redis");
    assert_eq!(
        resolve_initial_database(&inputs, Some(&sidebar)).as_deref(),
        Some("second"),
        "the sidebar connection id supplies the initial database"
    );

    let mut miss = inputs.clone();
    miss.store_active_db_session_id = Some("missing".to_owned());
    let fallback = derive_sidebar_conn_ctx(&miss, conn_ctx.as_ref()).expect("fallback is connCtx");
    assert_eq!(&fallback, conn_ctx.as_ref().expect("ctx"));

    let mut empty = inputs;
    empty.store_active_db_session_id = Some(String::new());
    assert_eq!(derive_sidebar_conn_ctx(&empty, conn_ctx.as_ref()), conn_ctx);
}

#[test]
fn toolbar_type_prefers_the_panel_and_comes_from_sidebar_without_one() {
    let inputs = inputs();
    let ctx = derive_conn_ctx(inputs.active_panel.as_ref());
    assert_eq!(resolve_toolbar_db_type(&inputs, ctx.as_ref()).as_deref(), Some("postgresql"));

    let mut no_panel = inputs.clone();
    no_panel.active_panel = None;
    no_panel.store_active_db_session_id = Some("s2".to_owned());
    no_panel.active_connections[1].db_session_id = Some("s2".to_owned());
    let sidebar = derive_sidebar_conn_ctx(&no_panel, None).expect("sidebar from s2");
    assert_eq!(resolve_toolbar_db_type(&no_panel, Some(&sidebar)).as_deref(), Some("redis"));
    let meta = derive_workspace_meta(&no_panel);
    assert!(meta.is_kv_panel, "sidebar meta drives the KV guard with no panel");
    assert_eq!(meta.toolbar_db_type.as_deref(), Some("redis"));
}

#[test]
fn structure_editor_reuses_the_shared_predicate_with_its_own_read_only_guard() {
    let base = sql_meta();
    assert!(can_open(Some(&base)) && show_structure_editor(Some(&base)));
    assert!(!can_open(None) && !show_structure_editor(None));
    let disabled = DatabaseTypeMetaVm {
        structure_editor_enabled: Some(false),
        ..sql_meta()
    };
    assert!(!show_structure_editor(Some(&disabled)));
    let document = DatabaseTypeMetaVm {
        connection_view: MetaConnectionViewVm::Document,
        ..sql_meta()
    };
    assert!(!can_open(Some(&document)), "document views never open the structure editor");
    let no_sql = DatabaseTypeMetaVm { supports_sql: Some(false), ..sql_meta() };
    assert!(!can_open(Some(&no_sql)));
    let read_only = DatabaseTypeMetaVm { is_read_only: true, ..sql_meta() };
    assert!(can_open(Some(&read_only)), "read-only is the hook's guard, not the predicate's");
    assert!(!show_structure_editor(Some(&read_only)));
}

#[test]
fn toolbar_flags_keep_default_true_and_kv_guards() {
    let base = sql_meta();
    assert_eq!(
        (show_new_query(Some(&base), true), show_new_table(Some(&base), true),
         show_er_diagram_toolbar(Some(&base), true), show_objects_toolbar(Some(&base), true)),
        (true, true, true, true)
    );
    let no_sql = DatabaseTypeMetaVm { supports_sql: Some(false), ..sql_meta() };
    assert!(!show_new_query(Some(&no_sql), true) && !show_new_table(Some(&no_sql), true));
    let absent_sql = DatabaseTypeMetaVm { supports_sql: None, ..sql_meta() };
    assert!(show_new_query(Some(&absent_sql), true), "supportsSQL !== false defaults true");
    assert!(!show_new_table(Some(&absent_sql), true), "the structure predicate stays falsy");
    let no_er = DatabaseTypeMetaVm { supports_er_diagram: Some(false), ..sql_meta() };
    assert!(!show_er_diagram_toolbar(Some(&no_er), true));
    let absent_er = DatabaseTypeMetaVm { supports_er_diagram: None, ..sql_meta() };
    assert!(show_er_diagram_toolbar(Some(&absent_er), true));
    let read_only = DatabaseTypeMetaVm { is_read_only: true, ..sql_meta() };
    assert!(show_new_query(Some(&read_only), true));
    assert!(!show_new_table(Some(&read_only), true));
    assert!(!show_objects_toolbar(Some(&read_only), true));
    assert!(show_er_diagram_toolbar(Some(&read_only), true));
    let kv = kv_meta();
    assert!(is_kv_panel(Some(&kv)));
    assert!(!show_new_query(Some(&kv), true) && !show_new_table(Some(&kv), true));
    assert!(!show_er_diagram_toolbar(Some(&kv), true) && !show_objects_toolbar(Some(&kv), true));
    assert!(!is_kv_panel(None));
    assert!(!show_new_query(Some(&base), false), "no toolbar type hides every item");
}

#[test]
fn export_scope_comes_from_panel_meta_but_batch_from_toolbar_meta() {
    assert_eq!(
        resolve_export_scope(None),
        DataExportCapabilityVm::FullTable,
        "absent metadata defaults to full_table"
    );
    assert_eq!(DataExportCapabilityVm::LoadedOnly.as_str(), "loaded_only");
    assert_eq!(DataExportCapabilityVm::None.as_str(), "none");
    assert!(!supports_full_table_export(DataExportCapabilityVm::LoadedOnly));
    assert!(!supports_full_table_export(DataExportCapabilityVm::None));
    assert!(supports_full_table_export(DataExportCapabilityVm::FullTable));

    let mut panel_limited = inputs();
    panel_limited.active_panel = Some(panel("p1", "c1", "s1", "limited"));
    let limited = derive_workspace_meta(&panel_limited);
    assert_eq!(limited.export_scope, DataExportCapabilityVm::LoadedOnly);
    assert!(!limited.batch_export_supported, "toolbar meta is the limited one here");

    // With no panel, export scope falls back to full_table while the sidebar's
    // toolbar meta still limits the batch entry — the two must not be swapped.
    let mut sidebar_limited = inputs();
    sidebar_limited.active_panel = None;
    sidebar_limited.store_active_db_session_id = Some("s2".to_owned());
    sidebar_limited.active_connections[1].db_session_id = Some("s2".to_owned());
    let no_panel = derive_workspace_meta(&sidebar_limited);
    assert_eq!(no_panel.export_scope, DataExportCapabilityVm::FullTable);
    assert_eq!(no_panel.toolbar_db_type.as_deref(), Some("redis"));
    assert!(
        !no_panel.batch_export_supported,
        "the batch entry follows the toolbar meta, not the panel fallback"
    );
}

#[test]
fn connecting_entry_appears_only_without_a_panel() {
    let mut inputs = inputs();
    inputs.active_panel = None;
    let meta = derive_workspace_meta(&inputs);
    let entry = meta.connecting_entry.expect("a connecting entry without a panel");
    assert_eq!(entry.connection_id, "c2");
    assert_eq!(entry.status, "connecting");
    assert_eq!(meta.connecting_name.as_deref(), Some("Conn Two"));
    assert_eq!(meta.connecting_db_type.as_deref(), Some("redis"));

    let with_panel = derive_workspace_meta(&inputs_with_panel());
    assert!(with_panel.connecting_entry.is_none(), "an active panel hides the connecting row");
    assert!(with_panel.connecting_name.is_none() && with_panel.connecting_db_type.is_none());

    let mut unknown = inputs;
    unknown.saved_connections.clear();
    let meta = derive_workspace_meta(&unknown);
    assert!(meta.connecting_entry.is_some());
    assert!(meta.connecting_name.is_none() && meta.connecting_db_type.is_none());
}

fn inputs_with_panel() -> WorkspaceMetaInputsVm {
    inputs()
}

#[test]
fn recent_panels_are_the_last_six_most_recent_first() {
    let mut with_sidebar = inputs();
    with_sidebar.store_active_db_session_id = Some("s1".to_owned());
    let meta = derive_workspace_meta(&with_sidebar);
    assert!(meta.sidebar_conn_ctx.is_some());
    assert_eq!(
        meta.recent_panels.iter().map(|entry| entry.id.as_str()).collect::<Vec<_>>(),
        vec!["p8", "p7", "p6", "p5", "p4", "p3"],
        "slice(-6).reverse() drops the three oldest panels of this connection"
    );
    assert!(!meta.recent_panels.iter().any(|entry| entry.connection_id == "c2"));

    let mut no_ctx_inputs = inputs();
    no_ctx_inputs.active_panel = None;
    let no_ctx = derive_workspace_meta(&no_ctx_inputs);
    assert!(no_ctx.recent_panels.is_empty(), "no panel and no store session means no context");
}

#[test]
fn kv_panel_overrides_the_status_database_only_with_a_panel() {
    let mut kv = inputs();
    kv.active_panel = Some(WorkspacePanelVm {
        db_name: Some("db5".to_owned()),
        ..panel("p1", "c1", "s1", "postgresql")
    });
    let meta = derive_workspace_meta(&kv);
    // The panel type is postgresql here, so its meta decides; the KV override
    // needs a panel whose own type maps to a KV meta.
    assert_eq!(meta.status_database.as_deref(), Some("db0"));
    kv.db_meta.insert("postgresql".to_owned(), kv_meta());
    let meta = derive_workspace_meta(&kv);
    assert_eq!(meta.status_database.as_deref(), Some("db5"), "a KV panel owns its target db");
    kv.active_panel.as_mut().expect("panel").db_name = None;
    let meta = derive_workspace_meta(&kv);
    assert_eq!(meta.status_database.as_deref(), Some("db0"), "dbName null falls back");

    let mut no_panel = kv;
    no_panel.active_panel = None;
    let meta = derive_workspace_meta(&no_panel);
    assert_eq!(meta.status_database.as_deref(), Some("db0"), "no panel means no override");
    assert_eq!(resolve_status_database(&WorkspaceMetaInputsVm::default(), None), None);
}

#[test]
fn the_derivation_is_deterministic() {
    let inputs = inputs();
    assert_eq!(derive_workspace_meta(&inputs), derive_workspace_meta(&inputs));
}
