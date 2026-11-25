//! Independent audit for T612: pure `useConnectionWorkspaceMeta` derivation.
//!
//! Pin the fallback edges, sub-six recent panels, first-connecting choice and
//! default-true capability semantics the coder does not drive.

use std::collections::BTreeMap;

use datazen_ui_api::SavedConnection;

use datazen_ui::views::{
    can_open, derive_conn_ctx, derive_sidebar_conn_ctx, derive_workspace_meta,
    resolve_initial_database, show_new_query, show_structure_editor, DataExportCapabilityVm,
    DatabaseTypeMetaVm, MetaConnectionViewVm, WorkspaceActiveEntryVm, WorkspaceMetaInputsVm,
    WorkspacePanelVm,
};

fn saved(id: &str, db_type: &str, database: Option<&str>) -> SavedConnection {
    SavedConnection {
        id: id.to_owned(),
        name: format!("name-{id}"),
        database_type: db_type.to_owned(),
        database: database.map(str::to_owned),
    }
}

fn panel(id: &str, conn: &str, db_type: &str) -> WorkspacePanelVm {
    WorkspacePanelVm {
        id: id.to_owned(),
        connection_id: conn.to_owned(),
        db_session_id: format!("s-{id}"),
        connection_name: format!("name-{conn}"),
        database_type: db_type.to_owned(),
        db_name: None,
    }
}

fn meta(export: DataExportCapabilityVm) -> DatabaseTypeMetaVm {
    DatabaseTypeMetaVm {
        is_key_value: false,
        connection_view: MetaConnectionViewVm::Sql,
        structure_editor_enabled: None,
        supports_sql: Some(true),
        is_read_only: false,
        supports_er_diagram: Some(true),
        export_scope: export,
    }
}

#[test]
fn empty_and_missing_ids_leave_initial_database_undefined() {
    let mut inputs = WorkspaceMetaInputsVm {
        active_panel: Some(panel("p", "c1", "postgresql")),
        saved_connections: vec![saved("c1", "postgresql", Some("db"))],
        ..Default::default()
    };
    assert_eq!(resolve_initial_database(&inputs, None).as_deref(), Some("db"), "audit: panel id supplies the lookup");
    inputs.active_panel.as_mut().expect("panel").connection_id.clear();
    assert_eq!(resolve_initial_database(&inputs, None), None, "audit: an empty panel id is no lookup");
    assert_eq!(
        resolve_initial_database(&WorkspaceMetaInputsVm::default(), None),
        None,
        "audit: no panel and no sidebar context is undefined"
    );
}

#[test]
fn sidebar_context_falls_back_when_the_saved_connection_is_missing() {
    let mut inputs = WorkspaceMetaInputsVm::default();
    inputs.store_active_db_session_id = Some("sx".to_owned());
    inputs.active_connections.push(WorkspaceActiveEntryVm {
        connection_id: "ghost".to_owned(),
        db_session_id: Some("sx".to_owned()),
        status: "connected".to_owned(),
    });
    let ctx = derive_conn_ctx(inputs.active_panel.as_ref());
    assert_eq!(derive_sidebar_conn_ctx(&inputs, ctx.as_ref()), ctx, "audit: a resolved active entry with no saved row falls back");
}

#[test]
fn recent_panels_under_six_keep_all_in_reverse_order() {
    let mut inputs = WorkspaceMetaInputsVm {
        all_panels: vec![panel("p1", "c1", "postgresql"), panel("p2", "c1", "postgresql"), panel("p3", "c1", "postgresql")],
        ..Default::default()
    };
    inputs.store_active_db_session_id = Some("s".to_owned());
    inputs.active_connections.push(WorkspaceActiveEntryVm {
        connection_id: "c1".to_owned(),
        db_session_id: Some("s".to_owned()),
        status: "connected".to_owned(),
    });
    inputs.saved_connections.push(saved("c1", "postgresql", None));
    let result = derive_workspace_meta(&inputs);
    assert_eq!(
        result.recent_panels.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        vec!["p3", "p2", "p1"],
        "audit: fewer than six matching panels are all kept, most recent first"
    );
}

#[test]
fn export_scope_follows_the_panel_meta_not_the_toolbar_meta() {
    let mut inputs = WorkspaceMetaInputsVm::default();
    inputs.store_active_db_session_id = Some("s".to_owned());
    inputs.active_connections.push(WorkspaceActiveEntryVm {
        connection_id: "limited".to_owned(),
        db_session_id: Some("s".to_owned()),
        status: "connected".to_owned(),
    });
    inputs.saved_connections.push(saved("limited", "limited", None));
    inputs.db_meta.insert("limited".to_owned(), meta(DataExportCapabilityVm::LoadedOnly));
    let result = derive_workspace_meta(&inputs);
    assert_eq!(result.toolbar_db_type.as_deref(), Some("limited"));
    assert_eq!(
        result.export_scope,
        DataExportCapabilityVm::FullTable,
        "audit: export scope follows the panel meta (absent here), not the toolbar meta"
    );
    assert!(!result.batch_export_supported, "audit: batch follows the toolbar meta");
}

#[test]
fn absent_driver_defaults_and_structure_read_only_guard() {
    assert_eq!(DataExportCapabilityVm::FullTable.as_str(), "full_table");
    assert!(show_new_query(None, true), "audit: absent toolbar meta keeps supportsSQL default-true");
    assert!(!show_structure_editor(None), "audit: absent panel meta cannot open the structure editor");
    let no_sql = DatabaseTypeMetaVm { supports_sql: None, ..meta(DataExportCapabilityVm::FullTable) };
    assert!(!can_open(Some(&no_sql)), "audit: absent supportsSQL is falsy for the structure predicate");
}
