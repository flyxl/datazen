use super::*;
use std::cell::RefCell;

fn context() -> QueryErrorChatContext {
    QueryErrorChatContext {
        database_type: "postgresql".to_owned(),
        database: "app".to_owned(),
        schema: Some("public".to_owned()),
        safe_sql: "SELECT 1".to_owned(),
        safe_error_message: "boom".to_owned(),
        context_fingerprint: "fp-1".to_owned(),
    }
}

fn input() -> QueryErrorDraftInput {
    QueryErrorDraftInput {
        panel_id: "panel-1".to_owned(),
        connection_id: "conn-1".to_owned(),
        db_session_id: "sess-1".to_owned(),
        database: Some("app".to_owned()),
        schema: Some("public".to_owned()),
    }
}

#[test]
fn constants_surface_and_gap_are_pinned() {
    assert_eq!(MAX_PROMPT_SQL_LENGTH, 4_000);
    assert_eq!(MAX_PROMPT_ERROR_LENGTH, 4_000);
    assert_eq!(QUERY_ERROR_CHAT_PROMPT_SURFACE.len(), 3);
    assert_eq!(QUERY_ERROR_CHAT_PROMPT_GAP.len(), 3);
}

#[test]
fn the_prompt_keeps_the_exact_line_layout() {
    let prompt = build_query_error_chat_prompt(&context());
    assert_eq!(
        prompt,
        [
            "I need help understanding and fixing a database query error.",
            "",
            "Database type: postgresql",
            "Database: app",
            "Schema: public",
            "",
            "SQL:",
            "```sql",
            "SELECT 1",
            "```",
            "",
            "Error:",
            "```",
            "boom",
            "```",
        ]
        .join("\n")
    );
}

#[test]
fn an_absent_schema_drops_only_the_schema_line() {
    let mut context = context();
    context.schema = None;
    let prompt = build_query_error_chat_prompt(&context);
    assert!(!prompt.contains("Schema:"));
    assert!(prompt.contains("Database type: postgresql\nDatabase: app\n\nSQL:"));
}

#[test]
fn sql_and_error_are_capped_independently_by_utf16_units() {
    let mut context = context();
    context.safe_sql = "a".repeat(4_010);
    context.safe_error_message = "e".repeat(4_005);
    let prompt = build_query_error_chat_prompt(&context);
    assert!(prompt.contains(&format!("```sql\n{}\n```", "a".repeat(4_000))));
    assert!(!prompt.contains(&"a".repeat(4_001)));
    assert!(prompt.contains(&format!("```\n{}\n```", "e".repeat(4_000))));
    assert!(!prompt.contains(&"e".repeat(4_001)));
}

#[test]
fn a_missing_context_never_dispatches_a_draft() {
    let seen = RefCell::new(0);
    let open_relation = |_: &str, _: Option<&str>, _: &str, _: Option<crate::OpenRelationSubTab>, _: Option<&str>| {};
    let open_draft = |_: &AiChatDraftRequest| *seen.borrow_mut() += 1;
    let callbacks = ContentViewCallbacks {
        open_relation: &open_relation,
        open_ai_chat_draft: &open_draft,
        open_sql_editor_for_table: None,
    };
    assert_eq!(
        send_query_error_chat_draft(&input(), None, "req-1".to_owned(), Some(&callbacks)),
        None
    );
    assert_eq!(*seen.borrow(), 0);
}

#[test]
fn a_built_request_is_dispatched_once_and_returned() {
    let seen = RefCell::new(Vec::new());
    let open_relation = |_: &str, _: Option<&str>, _: &str, _: Option<crate::OpenRelationSubTab>, _: Option<&str>| {};
    let open_draft = |request: &AiChatDraftRequest| seen.borrow_mut().push(request.clone());
    let callbacks = ContentViewCallbacks {
        open_relation: &open_relation,
        open_ai_chat_draft: &open_draft,
        open_sql_editor_for_table: None,
    };
    let request = match send_query_error_chat_draft(
        &input(),
        Some(&context()),
        "req-1".to_owned(),
        Some(&callbacks),
    ) {
        Some(value) => value,
        None => panic!("a built context must produce a draft"),
    };
    assert_eq!(request.request_id, "req-1");
    assert_eq!(request.source, AiChatDraftSource::QueryError);
    assert_eq!(request.panel_id, "panel-1");
    assert_eq!(request.connection_id, "conn-1");
    assert_eq!(request.db_session_id, "sess-1");
    assert_eq!(request.database.as_deref(), Some("app"));
    assert_eq!(request.schema.as_deref(), Some("public"));
    assert_eq!(request.context_fingerprint, "fp-1");
    assert!(request.focus);
    assert!(request.content.contains("SELECT 1"));
    assert_eq!(seen.borrow().as_slice(), [request.clone()]);
}

#[test]
fn a_request_can_be_built_without_a_callback_bridge() {
    let request = send_query_error_chat_draft(&input(), Some(&context()), "r".to_owned(), None);
    assert!(request.is_some());
}

#[test]
fn crate_root_reexports_the_prompt_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::QueryErrorChatContext>();
    resolves::<crate::QueryErrorDraftInput>();
    assert_eq!(crate::MAX_PROMPT_SQL_LENGTH, 4_000);
    assert!(crate::build_query_error_chat_prompt(&context()).contains("Database type: postgresql"));
}
