//! T504: `QueryBuilderHostAdapter` port — catalog derivation, context key /
//! rebind, mount / unmount, host actions and two-window / two-palette geometry.

use std::collections::BTreeMap;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::query::query_builder_host_adapter::{
    build_catalog, dialect_family, json_quote, json_stringify_array, qb_context_key,
    query_builder_host_adapter, relation_identity, ColumnVm, CommitMode, QueryBuilderAction,
    QueryBuilderHostPort, QueryBuilderVm, RelationColumnsVm, RelationVm, SessionSchemaVm,
};

#[derive(Clone)]
struct Probe {
    vm: QueryBuilderVm,
    dark: bool,
    events: Vec<QueryBuilderAction>,
}

fn relation(name: &str, schema: Option<&str>) -> RelationVm {
    RelationVm {
        name: name.to_owned(),
        schema: schema.map(str::to_owned),
    }
}

fn columns(name: &str, schema: Option<&str>, cols: &[(&str, &str)]) -> RelationColumnsVm {
    RelationColumnsVm {
        database: "app".to_owned(),
        schema: schema.map(str::to_owned),
        name: name.to_owned(),
        columns: cols
            .iter()
            .map(|(n, ty)| ColumnVm {
                name: (*n).to_owned(),
                data_type: (*ty).to_owned(),
            })
            .collect(),
    }
}

fn session() -> SessionSchemaVm {
    let mut relation_columns = BTreeMap::new();
    relation_columns.insert(
        "users".to_owned(),
        columns("users", Some("public"), &[("id", "int"), ("name", "text")]),
    );
    relation_columns.insert(
        "orders".to_owned(),
        columns("orders", Some("public"), &[("id", "int"), ("total", "numeric")]),
    );
    relation_columns.insert(
        "ghost".to_owned(),
        columns("ghost", Some("public"), &[("id", "int")]),
    );
    SessionSchemaVm {
        current_database: "app".to_owned(),
        tables: vec![relation("users", Some("public")), relation("orders", Some("public"))],
        views: vec![relation("active_users", Some("public"))],
        table_catalogs: BTreeMap::new(),
        path_items: BTreeMap::new(),
        relation_columns,
    }
}

fn open_probe() -> Probe {
    let mut vm = QueryBuilderVm {
        panel_id: "query-panel-1".to_owned(),
        connection_id: "connection-1".to_owned(),
        db_session_id: "session-1".to_owned(),
        database_type: "postgresql".to_owned(),
        database: "app".to_owned(),
        schema: Some("public".to_owned()),
        current_sql: "SELECT 1".to_owned(),
        contribution_available: true,
        open_panel_id: Some("query-panel-1".to_owned()),
        session_schema: Some(session()),
        enable_fk_prediction: true,
        sql_format_options: "standard".to_owned(),
        ..QueryBuilderVm::default()
    };
    let key = vm.context_key();
    vm.bound_context_key = Some(key);
    Probe {
        vm,
        dark: true,
        events: Vec::new(),
    }
}

fn adapter_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let vm = s.vm.clone();
    let dark = s.dark;
    let host = QueryBuilderHostPort {
        on_action: Some(Rc::new(move |st: &mut Probe, action| st.events.push(action))),
    };
    query_builder_host_adapter(&vm, dark, &host)
        .unwrap_or_else(|| col().w_full().h_full().a11y_hidden().key("qb-none"))
}

fn runtime(p: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, adapter_view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

#[test]
fn context_key_and_rebind_are_pinned() {
    assert_eq!(
        qb_context_key("connection-1", "session-1", "app", Some("public")),
        r#"["connection-1","session-1","app","public"]"#
    );
    assert_eq!(
        qb_context_key("connection-1", "session-1", "app", None),
        r#"["connection-1","session-1","app",""]"#
    );
    assert_eq!(json_quote("a\"b\\c"), r#""a\"b\\c""#);
    assert_eq!(json_stringify_array(&["a", "b"]), r#"["a","b"]"#);
    assert_eq!(relation_identity(None, "events"), r#"[null,"events"]"#);
    assert_eq!(
        relation_identity(Some("public"), "users"),
        r#"["public","users"]"#
    );

    let mut p = open_probe();
    assert!(!p.vm.needs_rebind(), "bound to the current context");
    assert_eq!(p.vm.rebind_action(), None);
    p.vm.database = "analytics".to_owned();
    p.vm.schema = Some("private".to_owned());
    assert!(p.vm.needs_rebind(), "database/schema change rebinds the open builder");
    assert_eq!(
        p.vm.rebind_action(),
        Some(QueryBuilderAction::Rebind {
            panel_id: "query-panel-1".to_owned(),
            context_key: r#"["connection-1","session-1","analytics","private"]"#.to_owned(),
        })
    );

    // A closed builder never rebinds, even with a stale bound key.
    p.vm.open_panel_id = None;
    assert!(!p.vm.needs_rebind());
}

#[test]
fn catalog_dedupes_filters_and_builds_column_maps() {
    // Two `users` relations in different schemas collide by name: neither is
    // selectable and neither gets a column map. The schema selector removes the
    // collision and restores both.
    let mut schema = session();
    schema.tables.push(relation("users", Some("private")));
    schema.relation_columns.insert(
        "private-users".to_owned(),
        columns("users", Some("private"), &[("uid", "int")]),
    );

    let all = build_catalog(Some(&schema), "app", None);
    assert!(all.tables.iter().all(|r| r.name != "users"), "colliding names are not selectable");
    assert!(all.tables.iter().any(|r| r.name == "orders"), "unique names remain");
    assert!(!all.column_map.contains_key("users"), "colliding names have no column map");
    assert!(all.column_map.contains_key("orders"));
    assert!(!all.typed_column_map.contains_key("ghost"), "absent relation skipped");

    let public = build_catalog(Some(&schema), "app", Some("public"));
    let names: Vec<&str> = public.tables.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["active_users", "orders", "users"], "schema filter keeps views, drops the private collision");
    assert_eq!(public.tables.iter().filter(|r| r.name == "users").count(), 1);
    assert_eq!(public.column_map.get("users"), Some(&vec!["id".to_owned(), "name".to_owned()]));
    assert_eq!(
        public.typed_column_map.get("users").and_then(|m| m.get("name")).map(String::as_str),
        Some("text")
    );

    let wrong_db = build_catalog(Some(&schema), "other", None);
    assert!(wrong_db.tables.is_empty(), "no catalog items for another database");

    // `tableCatalogs[database]` takes precedence over tables + views.
    let mut pinned = session();
    pinned
        .table_catalogs
        .insert("app".to_owned(), vec![relation("pinned", Some("public"))]);
    let pinned = build_catalog(Some(&pinned), "app", None);
    assert_eq!(pinned.tables, vec![relation("pinned", Some("public"))]);

    // Path items contribute only when the session is on this database.
    let mut paths = session();
    paths.tables.clear();
    paths
        .path_items
        .insert("tree".to_owned(), vec![relation("path_relation", Some("public"))]);
    assert!(build_catalog(Some(&paths), "app", None).tables.iter().any(|r| r.name == "path_relation"));
    paths.current_database = "other".to_owned();
    assert!(build_catalog(Some(&paths), "app", None).tables.is_empty());
}

#[test]
fn dialect_family_falls_back_to_database_type() {
    assert_eq!(dialect_family("postgresql", None), "postgresql");
    assert_eq!(dialect_family("postgresql", Some("postgres")), "postgres");
}

#[test]
fn mount_unmount_and_toolbar_toggle() {
    // No contribution: React renders neither the toolbar item nor the adapter.
    let mut p = open_probe();
    p.vm.contribution_available = false;
    let rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "qb-none"));
    assert!(!has(&rt, "editor-visual-builder-button"));
    assert!(!has(&rt, "qb-panel"));

    // Closed: toggle + editor slot, no builder panel.
    let mut p = open_probe();
    p.vm.open_panel_id = None;
    p.vm.bound_context_key = None;
    let rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "editor-visual-builder-button"));
    assert!(has(&rt, "qb-editor-slot"));
    assert!(!has(&rt, "qb-panel"));

    // Open: builder panel replaces the editor slot; toggle keeps the React
    // accessible name.
    let rt = runtime(open_probe(), (1440.0, 816.0));
    assert!(has(&rt, "editor-visual-builder-button"));
    assert!(has(&rt, "qb-panel"));
    assert!(!has(&rt, "qb-editor-slot"));
    assert_eq!(
        element(&rt, "editor-visual-builder-button").label.as_deref(),
        Some(t("query.visualBuilder.title"))
    );
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|text| text == t("query.visualBuilder.title")));
    assert!(rendered.iter().any(|text| text == t("common.ok")));
    assert!(rendered.iter().any(|text| text == t("common.cancel")));

    // Toggle dispatches the open/hide request with the current context key.
    let mut rt = runtime(open_probe(), (1440.0, 816.0));
    click(&mut rt, "editor-visual-builder-button");
    assert_eq!(
        rt.state.events,
        [QueryBuilderAction::Toggle {
            panel_id: "query-panel-1".to_owned(),
            context_key: r#"["connection-1","session-1","app","public"]"#.to_owned(),
            open: true,
        }]
    );
    let mut rt = runtime(open_probe(), (1440.0, 816.0));
    click(&mut rt, "qb-panel-close");
    assert_eq!(rt.state.events, [QueryBuilderAction::Cancel]);
}

#[test]
fn catalog_relay_and_service_actions_are_host_owned() {
    let rt = runtime(open_probe(), (1440.0, 816.0));
    for key in [
        "qb-panel",
        "qb-panel-header",
        "qb-context-relay",
        "qb-context-key",
        "qb-bound-context",
        "qb-catalog-table-users",
        "qb-catalog-table-orders",
        "qb-columns-users",
        "qb-load-schema-users",
        "qb-dialect",
        "qb-fk-prediction",
        "qb-current-sql",
        "qb-service-actions",
        "qb-ensure-columns",
        "qb-predict",
        "qb-format",
        "qb-panel-actions",
        "qb-commit",
        "qb-cancel",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    assert_eq!(
        element(&rt, "qb-context-key").text.as_deref(),
        Some(r#"["connection-1","session-1","app","public"]"#)
    );
    assert_eq!(element(&rt, "qb-dialect").text.as_deref(), Some("postgresql"));
    assert_eq!(element(&rt, "qb-fk-prediction").text.as_deref(), Some("true"));
    assert_eq!(element(&rt, "qb-current-sql").text.as_deref(), Some("SELECT 1"));
    assert!(!has(&rt, "qb-context-stale"), "freshly bound context is not stale");

    let mut rt = runtime(open_probe(), (1440.0, 816.0));
    click(&mut rt, "qb-ensure-columns");
    assert!(rt.state.events.iter().any(|a| matches!(a,
        QueryBuilderAction::EnsureColumns { db_session_id, database, schema, names }
            if db_session_id == "session-1" && database == "app"
                && schema.as_deref() == Some("public") && names.contains(&"id".to_owned())
    )), "ensureColumns keeps the session/database/schema identity");

    let mut rt = runtime(open_probe(), (1440.0, 816.0));
    click(&mut rt, "qb-load-schema-users");
    assert!(rt.state.events.iter().any(|a| matches!(a,
        QueryBuilderAction::LoadTableSchema { name, schema }
            if name == "users" && schema.as_deref() == Some("public")
    )), "loadTableSchema pins the table and schema");

    let mut rt = runtime(open_probe(), (1440.0, 816.0));
    click(&mut rt, "qb-predict");
    assert!(rt.state.events.iter().any(|a| matches!(a,
        QueryBuilderAction::PredictRelations { schemas }
            if schemas.iter().any(|r| r.name == "users")
    )), "predictRelations receives the catalog tables");

    let mut rt = runtime(open_probe(), (1440.0, 816.0));
    click(&mut rt, "qb-format");
    assert!(rt.state.events.iter().any(|a| matches!(a,
        QueryBuilderAction::FormatSql { sql, database_type, options }
            if sql == "SELECT 1" && database_type == "postgresql" && options == "standard"
    )), "formatSql carries the dialect and formatting options");
}

#[test]
fn commit_cancel_toast_and_stale_context() {
    let mut rt = runtime(open_probe(), (1440.0, 816.0));
    click(&mut rt, "qb-commit");
    assert_eq!(
        rt.state.events,
        [QueryBuilderAction::Commit {
            sql: "SELECT 1".to_owned(),
            mode: CommitMode::Replace,
        }]
    );

    let mut rt = runtime(open_probe(), (1440.0, 816.0));
    click(&mut rt, "qb-cancel");
    assert_eq!(rt.state.events, [QueryBuilderAction::Cancel]);

    let mut p = open_probe();
    p.vm.toast = Some(t("query.visualBuilder.appliedToast").to_owned());
    let rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "qb-toast"));
    assert_eq!(
        element(&rt, "qb-toast").text.as_deref(),
        Some(t("query.visualBuilder.appliedToast"))
    );

    let mut p = open_probe();
    p.vm.schema = Some("private".to_owned());
    let rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "qb-context-stale"), "a stale binding is visible to the host");
}

#[test]
fn panel_fits_both_windows_in_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = open_probe();
            p.dark = dark;
            let rt = runtime(p, size);
            let outside: Vec<String> = rt.elements().iter()
                .filter(|e| {
                    let r = e.rect;
                    r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                })
                .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                .collect();
            assert!(outside.is_empty(), "dark={dark} {size:?}: {outside:?}");
        }
    }
}
