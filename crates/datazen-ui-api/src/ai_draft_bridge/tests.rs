use super::*;
use std::cell::RefCell;

fn request() -> AiChatDraftRequest {
    AiChatDraftRequest::new(
        "req-1".to_owned(),
        AiChatDraftSource::QueryError,
        "panel-1".to_owned(),
        "conn-1".to_owned(),
        "sess-1".to_owned(),
        Some("app".to_owned()),
        Some("public".to_owned()),
        "fp-1".to_owned(),
        "fix the query".to_owned(),
    )
}

fn table_input() -> TableContextInput {
    TableContextInput {
        connection_id: "conn-1".to_owned(),
        db_session_id: "sess-1".to_owned(),
        database_type: "postgresql".to_owned(),
        database: "app".to_owned(),
        schema: None,
        table_name: Some("users".to_owned()),
        table_schema: Some("sales".to_owned()),
        view_name: None,
        view_schema: None,
        object_type: None,
        name: None,
    }
}

#[test]
fn surface_gap_and_literals_are_pinned() {
    assert_eq!(AI_DRAFT_BRIDGE_SURFACE.len(), 8);
    assert_eq!(AI_DRAFT_BRIDGE_GAP.len(), 4);
    assert_eq!(AiChatDraftSource::QueryError.as_str(), "query-error");
    for (tab, literal) in [
        (OpenRelationSubTab::Data, "data"),
        (OpenRelationSubTab::Structure, "structure"),
        (OpenRelationSubTab::Ddl, "ddl"),
    ] {
        assert_eq!(tab.as_str(), literal);
    }
    for (kind, literal) in [
        (TableSqlActionKind::OpenData, "openData"),
        (TableSqlActionKind::Select, "select"),
        (TableSqlActionKind::Insert, "insert"),
        (TableSqlActionKind::Update, "update"),
        (TableSqlActionKind::Ddl, "ddl"),
    ] {
        assert_eq!(kind.as_str(), literal);
    }
    assert_eq!(QueryOpenSource::TableAction.as_str(), "table-action");
    assert_eq!(QueryOpenSource::ObjectSearch.as_str(), "object-search");
    assert_eq!(QueryOpenSource::AiAction.as_str(), "ai-action");
    assert_eq!(QueryOpenFocus::Editor.as_str(), "editor");
    assert_eq!(QueryOpenFocus::Result.as_str(), "result");
}

#[test]
fn every_draft_carries_the_focus_signal() {
    let draft = request();
    assert!(AI_CHAT_DRAFT_FOCUS);
    assert!(draft.focus);
    assert_eq!(draft.request_id, "req-1");
    assert_eq!(draft.source, AiChatDraftSource::QueryError);
    assert_eq!(draft.panel_id, "panel-1");
    assert_eq!(draft.connection_id, "conn-1");
    assert_eq!(draft.db_session_id, "sess-1");
    assert_eq!(draft.database.as_deref(), Some("app"));
    assert_eq!(draft.schema.as_deref(), Some("public"));
    assert_eq!(draft.context_fingerprint, "fp-1");
    assert_eq!(draft.content, "fix the query");

    let bare = AiChatDraftRequest::new(
        "r".to_owned(),
        AiChatDraftSource::QueryError,
        "p".to_owned(),
        "c".to_owned(),
        "s".to_owned(),
        None,
        None,
        "f".to_owned(),
        String::new(),
    );
    assert!(bare.focus);
    assert_eq!(bare.database, None);
    assert_eq!(bare.schema, None);
}

#[test]
fn callbacks_route_relation_draft_and_optional_sql_editor() {
    let seen = RefCell::new(Vec::new());
    let open_relation = |name: &str,
                         schema: Option<&str>,
                         database: &str,
                         sub_tab: Option<OpenRelationSubTab>,
                         column: Option<&str>| {
        seen.borrow_mut().push(format!(
            "relation:{name}:{:?}:{database}:{:?}:{column:?}",
            schema, sub_tab
        ));
    };
    let open_draft = |request: &AiChatDraftRequest| {
        seen.borrow_mut().push(format!("draft:{}", request.request_id));
    };
    let open_sql = |input: &TableContextInput, spec: &TableSqlActionSpec| {
        seen.borrow_mut().push(format!(
            "sql:{}:{}",
            input.table_name.as_deref().unwrap_or(""),
            spec.kind.as_str()
        ));
    };
    let callbacks = ContentViewCallbacks {
        open_relation: &open_relation,
        open_ai_chat_draft: &open_draft,
        open_sql_editor_for_table: Some(&open_sql),
    };

    (callbacks.open_relation)(
        "users",
        Some("sales"),
        "app",
        Some(OpenRelationSubTab::Structure),
        Some("id"),
    );
    (callbacks.open_ai_chat_draft)(&request());
    if let Some(handler) = callbacks.open_sql_editor_for_table {
        handler(
            &table_input(),
            &TableSqlActionSpec {
                kind: TableSqlActionKind::Select,
                source: Some(QueryOpenSource::TableAction),
                initial_sql: None,
                focus: Some(QueryOpenFocus::Editor),
            },
        );
    }
    assert_eq!(
        seen.borrow().clone(),
        vec![
            "relation:users:Some(\"sales\"):app:Some(Structure):Some(\"id\")",
            "draft:req-1",
            "sql:users:select",
        ]
    );

    let without_sql = ContentViewCallbacks {
        open_relation: &open_relation,
        open_ai_chat_draft: &open_draft,
        open_sql_editor_for_table: None,
    };
    assert!(without_sql.open_sql_editor_for_table.is_none());
}

#[test]
fn table_context_input_keeps_identity_and_legacy_schema_legs() {
    let input = table_input();
    assert_eq!(input.connection_id, "conn-1");
    assert_eq!(input.db_session_id, "sess-1");
    assert_eq!(input.database_type, "postgresql");
    assert_eq!(input.database, "app");
    assert_eq!(input.schema, None);
    assert_eq!(input.table_name.as_deref(), Some("users"));
    assert_eq!(input.table_schema.as_deref(), Some("sales"));
    assert_eq!(input.view_name, None);
    assert_eq!(input.view_schema, None);
    assert_eq!(input.object_type, None);
    assert_eq!(input.name, None);
}

#[test]
fn crate_root_reexports_the_bridge_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::AiChatDraftRequest>();
    resolves::<crate::AiChatDraftSource>();
    resolves::<crate::ContentViewCallbacks>();
    resolves::<crate::TableContextInput>();
    resolves::<crate::TableSqlActionSpec>();
    assert_eq!(crate::AI_DRAFT_BRIDGE_SURFACE.len(), 8);
    assert_eq!(crate::AiChatDraftSource::QueryError.as_str(), "query-error");
}
