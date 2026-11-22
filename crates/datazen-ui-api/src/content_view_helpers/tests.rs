use std::collections::BTreeMap;

use crate::SavedConnection;

use super::*;

fn panel(kind: PanelKind) -> ContentViewPanel {
    ContentViewPanel::new(kind, "Local")
}

fn translator(key: &str) -> String {
    format!("t:{key}")
}

#[test]
fn panel_icons_cover_every_kind_and_db_object_subkind() {
    for (kind, id) in [
        (PanelKind::Table, "schema.table"),
        (PanelKind::View, "schema.view"),
        (PanelKind::Query, "common.newQuery"),
        (PanelKind::CreateTable, "common.newTable"),
        (PanelKind::ErDiagram, "common.erDiagram"),
        (PanelKind::Objects, "common.objects"),
        (PanelKind::Privileges, "action.privileges"),
        (PanelKind::ServerStatus, "action.serverStatus"),
        (PanelKind::Processes, "action.processes"),
        (PanelKind::RedisDb, "schema.redisDatabase"),
    ] {
        let icon = get_panel_icon(&panel(kind)).expect("an icon");
        assert_eq!(icon.id(), id);
        assert!(icon.class().starts_with(ICON_CLASS));
    }
    assert_eq!(get_panel_icon(&panel(PanelKind::Unknown)), None);

    let mut db_object = panel(PanelKind::DbObject);
    for (kind, expected) in [
        ("trigger", PanelIcon::Trigger),
        ("procedure", PanelIcon::Procedure),
        ("sequence", PanelIcon::Sequence),
        ("type", PanelIcon::Type),
        ("function", PanelIcon::Function),
        ("other", PanelIcon::Function),
    ] {
        db_object.object_kind = Some(kind.to_owned());
        assert_eq!(get_panel_icon(&db_object), Some(expected));
    }
    assert_eq!(PanelIcon::Function.fallback(), "Braces");
    assert_eq!(PanelIcon::Procedure.fallback(), "Braces");
    assert_eq!(PanelIcon::Table.fallback(), "Table2");
    assert_eq!(PanelIcon::Table.class(), format!("{ICON_CLASS} text-blue-400"));
    assert_eq!(PanelIcon::Query.class(), ICON_CLASS);
}

#[test]
fn short_labels_use_panel_names_and_translation_fallbacks() {
    let mut table = panel(PanelKind::Table);
    table.table_name = Some("customers".to_owned());
    assert_eq!(get_panel_short_label(&table, None), "customers");
    assert_eq!(get_panel_label(&table, None), "customers");

    let mut view = panel(PanelKind::View);
    view.view_name = Some("active_users".to_owned());
    assert_eq!(get_panel_short_label(&view, None), "active_users");

    let mut query = panel(PanelKind::Query);
    query.title = Some("Report".to_owned());
    assert_eq!(get_panel_short_label(&query, None), "Report");

    let mut object = panel(PanelKind::DbObject);
    object.object_name = Some("sp_ping".to_owned());
    assert_eq!(get_panel_short_label(&object, None), "sp_ping");
    let mut redis = panel(PanelKind::RedisDb);
    redis.db_name = Some("db1".to_owned());
    assert_eq!(get_panel_short_label(&redis, None), "db1");

    assert_eq!(get_panel_short_label(&panel(PanelKind::CreateTable), None), "New Table");
    assert_eq!(get_panel_short_label(&panel(PanelKind::CreateTable), Some(&translator)), "t:common.newTable");
    assert_eq!(get_panel_short_label(&panel(PanelKind::ErDiagram), None), "ER Diagram");
    assert_eq!(get_panel_short_label(&panel(PanelKind::Objects), None), "Objects");
    assert_eq!(get_panel_short_label(&panel(PanelKind::Privileges), None), "Privileges");
    assert_eq!(get_panel_short_label(&panel(PanelKind::ServerStatus), None), "Server Dashboard");
    assert_eq!(get_panel_short_label(&panel(PanelKind::Processes), None), "Process List");
    assert_eq!(get_panel_short_label(&panel(PanelKind::Unknown), None), "");
}

#[test]
fn tab_database_prefers_the_panel_database_and_redis_uses_its_db_name() {
    let mut table = panel(PanelKind::Table);
    table.database = Some("shop".to_owned());
    assert_eq!(resolve_panel_tab_database(&table, Some("session")).as_deref(), Some("shop"));
    let bare = panel(PanelKind::Table);
    assert_eq!(resolve_panel_tab_database(&bare, Some("session")).as_deref(), Some("session"));
    let mut redis = panel(PanelKind::RedisDb);
    redis.db_name = Some("cache".to_owned());
    assert_eq!(resolve_panel_tab_database(&redis, Some("session")).as_deref(), Some("cache"));
    let query = panel(PanelKind::Query);
    assert_eq!(resolve_panel_tab_database(&query, None), None);
}

#[test]
fn tab_label_joins_connection_database_and_short_without_empty_parts() {
    let mut table = panel(PanelKind::Table);
    table.database = Some("  shop  ".to_owned());
    table.table_name = Some("customers".to_owned());
    assert_eq!(get_panel_tab_label(&table, None, None), "Local · shop · customers");

    let query = panel(PanelKind::Query);
    assert_eq!(get_panel_tab_label(&query, None, None), "Local");
    assert_eq!(get_panel_tab_label(&query, Some(" db "), None), "Local · db");

    let mut blank_db = panel(PanelKind::Table);
    blank_db.database = Some("   ".to_owned());
    assert_eq!(get_panel_tab_label(&blank_db, None, None), "Local");
}

#[test]
fn sub_tabs_follow_the_read_only_and_view_rules() {
    let writable = get_sub_tabs(&translator, false);
    assert_eq!(writable.len(), 5);
    assert_eq!(writable[0].id, SubTabId::Data);
    assert_eq!(writable[0].label, "t:connWin.data");
    assert_eq!(writable[4].id, SubTabId::Ddl);
    assert_eq!(writable[4].label, "DDL");

    let read_only = get_sub_tabs(&translator, true);
    assert_eq!(read_only.len(), 2);
    assert_eq!(read_only[1].id, SubTabId::Structure);

    let view = get_view_sub_tabs(&translator);
    assert_eq!(view.len(), 3);
    assert_eq!(view[2].id, SubTabId::Ddl);
    assert_eq!(SubTabId::ForeignKeys.as_str(), "foreignKeys");
}

fn fixtures() -> (BTreeMap<String, ActiveConnectionEntry>, Vec<SavedConnection>) {
    let active = BTreeMap::from([
        (
            "conn-a".to_owned(),
            ActiveConnectionEntry { connection_id: "conn-a".to_owned(), db_session_id: "sess-a".to_owned() },
        ),
        (
            "conn-b".to_owned(),
            ActiveConnectionEntry { connection_id: "conn-b".to_owned(), db_session_id: String::new() },
        ),
    ]);
    let saved = vec![
        SavedConnection {
            id: "conn-a".to_owned(),
            name: "Production".to_owned(),
            database_type: "postgres".to_owned(),
            database: None,
        },
        SavedConnection {
            id: "conn-c".to_owned(),
            name: "Unused".to_owned(),
            database_type: String::new(),
            database: None,
        },
    ];
    (active, saved)
}

#[test]
fn connection_context_resolves_by_session_then_by_saved_id() {
    let (active, saved) = fixtures();
    let context = resolve_connection_context("sess-a", &active, &saved).expect("a live context");
    assert_eq!(context.connection_id, "conn-a");
    assert_eq!(context.db_session_id, "sess-a");
    assert_eq!(context.connection_name, "Production");
    assert_eq!(context.database_type, "postgres");

    assert_eq!(resolve_connection_context("unknown", &active, &saved), None);
    assert_eq!(resolve_connection_context("", &active, &saved), None);
    assert_eq!(resolve_connection_context("sess-a", &active, &[]), None, "an unsaved connection is not a context");
    let mut no_session = active.clone();
    no_session.insert(
        "conn-z".to_owned(),
        ActiveConnectionEntry { connection_id: "conn-z".to_owned(), db_session_id: String::new() },
    );
    assert_eq!(resolve_connection_context("", &no_session, &saved), None);
}

#[test]
fn connection_context_by_connection_uses_the_key_and_rejects_an_empty_session() {
    let (active, saved) = fixtures();
    let context = resolve_connection_context_by_connection("conn-a", &active, &saved).expect("a context");
    assert_eq!(context.connection_id, "conn-a");
    assert_eq!(context.connection_name, "Production");
    assert_eq!(resolve_connection_context_by_connection("conn-b", &active, &saved), None);
    assert_eq!(resolve_connection_context_by_connection("missing", &active, &saved), None);
    assert_eq!(resolve_connection_context_by_connection("conn-a", &active, &[]), None);
}

#[test]
fn surface_and_gap_are_pinned_and_t612_facing_exports_are_reachable() {
    assert_eq!(CONTENT_VIEW_HELPERS_SURFACE.len(), 9);
    assert_eq!(CONTENT_VIEW_HELPERS_GAP.len(), 3);
    assert_eq!(crate::CONTENT_VIEW_HELPERS_SURFACE.len(), 9);
    let _ = crate::get_panel_icon;
    let _ = crate::get_panel_short_label;
    let _ = crate::resolve_panel_tab_database;
    let _ = crate::get_panel_tab_label;
    let _ = crate::get_panel_label;
    let _ = crate::get_sub_tabs;
    let _ = crate::get_view_sub_tabs;
    let _ = crate::resolve_connection_context;
    let _ = crate::resolve_connection_context_by_connection;
    let _: Option<crate::ContentViewConnectionContext> = None;
    let _: Option<crate::ContentViewPanel> = None;
}
