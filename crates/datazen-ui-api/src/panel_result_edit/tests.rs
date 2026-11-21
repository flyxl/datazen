use super::*;

fn identity(column: &str, source: &str) -> IdentityColumn {
    IdentityColumn { column: column.to_owned(), source_column: source.to_owned() }
}

fn writable() -> ServerEditability {
    ServerEditability {
        writable: true,
        identity_columns: vec![identity("id", "id")],
        editable_columns: vec![identity("name", "name")],
        table: Some(ResultTableRef {
            catalog: None,
            schema: Some("public".to_owned()),
            name: "users".to_owned(),
            aliased: false,
        }),
        reason: None,
    }
}

fn read_only(reason: Option<GridReadOnlyReason>) -> ServerEditability {
    ServerEditability { writable: false, reason, ..writable() }
}

fn request() -> ResultOriginRequest {
    ResultOriginRequest {
        db_session_id: "s1".to_owned(),
        database: Some("db".to_owned()),
        schema: None,
        sql: "select * from users".to_owned(),
        result_columns: vec!["id".to_owned(), "name".to_owned()],
        table: Some("users".to_owned()),
    }
}

fn options(key: Option<&str>) -> ResultEditabilityOptions {
    ResultEditabilityOptions {
        key: key.map(str::to_owned),
        db_session_id: "s1".to_owned(),
        database: Some("db".to_owned()),
        schema: None,
        sql: "select * from users".to_owned(),
        result_columns: vec!["id".to_owned(), "name".to_owned()],
        table: Some("users".to_owned()),
    }
}

#[test]
fn key_format_message_key_and_gap_are_pinned() {
    assert_eq!(result_edit_key("s1", 0), "s1#0");
    assert_eq!(result_edit_key("s1", 12), "s1#12");
    assert_eq!(grid_read_only_message_key(GridReadOnlyReason::UnknownOrigin), "query.gridReadOnly.unknownOrigin");
    assert_eq!(grid_read_only_message_key(GridReadOnlyReason::MultiTable), "query.gridReadOnly.multiTable");
    assert_eq!(grid_read_only_message_key(GridReadOnlyReason::SetOperation), "query.gridReadOnly.setOperation");
    assert_eq!(grid_read_only_message_key(GridReadOnlyReason::KeyValueNull), "query.gridReadOnly.keyValueNull");
    assert_eq!(grid_read_only_message_key(GridReadOnlyReason::DuplicateIdentity), "query.gridReadOnly.duplicateIdentity");
    assert_eq!(grid_read_only_message_key(GridReadOnlyReason::RowNotFound), "query.gridReadOnly.rowNotFound");
    assert_eq!(GridReadOnlyReason::SessionReadOnly.as_str(), "sessionReadOnly");
    assert_eq!(PANEL_RESULT_EDIT_GAP.len(), 4);
    assert!(PANEL_RESULT_EDIT_GAP.contains(&"queryCommands.resolveResultOrigin IPC"));
}

#[test]
fn successful_resolve_publishes_the_verdict_and_clears_resolving() {
    let mut store = PanelResultEditStore::new();
    let ticket = store.begin_resolve("s1#0", request()).expect("a fresh key starts a resolve");
    assert!(store.resolving("s1#0"));
    assert_eq!(store.generation("s1#0"), 1);
    assert_eq!(ticket.generation, 1);
    assert_eq!(ticket.request.db_session_id, "s1");

    store.complete_resolve(ticket, Ok(writable()));
    assert!(!store.resolving("s1#0"));
    assert!(store.verdict("s1#0").is_some_and(|verdict| verdict.writable));
    assert_eq!(store.error("s1#0"), None);
    assert_eq!(store.generation("s1#0"), 1);
}

#[test]
fn failed_resolve_keeps_the_result_read_only_and_records_the_error() {
    let mut store = PanelResultEditStore::new();
    let ticket = store.begin_resolve("s1#0", request()).expect("a fresh key starts a resolve");
    store.complete_resolve(ticket, Err("offline".to_owned()));
    assert!(!store.resolving("s1#0"));
    assert_eq!(store.verdict("s1#0"), None, "a failed verdict is null, never writable");
    assert_eq!(store.error("s1#0"), Some("offline"));
}

#[test]
fn a_second_resolve_is_deduplicated_while_one_is_in_flight() {
    let mut store = PanelResultEditStore::new();
    let first = store.begin_resolve("s1#0", request());
    let second = store.begin_resolve("s1#0", request());
    assert!(first.is_some());
    assert!(second.is_none(), "the second surface waits for the first");
    assert_eq!(store.generation("s1#0"), 1, "dedup does not bump the generation");
}

#[test]
fn an_answer_after_clear_is_discarded() {
    let mut store = PanelResultEditStore::new();
    let ticket = store.begin_resolve("s1#0", request()).expect("first resolve");
    store.clear_for_result("s1#0");
    assert_eq!(store.generation("s1#0"), 2, "clear makes the in-flight answer stale");
    assert!(!store.resolving("s1#0"));
    store.complete_resolve(ticket, Ok(writable()));
    assert_eq!(store.verdict("s1#0"), None, "a stale answer must not write a verdict");
    assert_eq!(store.generation("s1#0"), 2);
}

#[test]
fn a_stale_answer_does_not_clear_a_newer_request() {
    let mut store = PanelResultEditStore::new();
    let stale = store.begin_resolve("s1#0", request()).expect("first resolve");
    store.clear_for_result("s1#0");
    let fresh = store.begin_resolve("s1#0", request()).expect("a new key starts a new resolve");
    assert_eq!(fresh.generation, 3);
    assert!(store.resolving("s1#0"));

    store.complete_resolve(stale, Ok(writable()));
    assert!(store.resolving("s1#0"), "the stale answer leaves the newer request resolving");
    assert_eq!(store.verdict("s1#0"), None);
    assert_eq!(store.generation("s1#0"), 3);

    store.complete_resolve(fresh, Ok(writable()));
    assert!(!store.resolving("s1#0"));
    assert!(store.verdict("s1#0").is_some());
}

#[test]
fn clear_for_result_and_clear_all_reset_the_right_scope() {
    let mut store = PanelResultEditStore::new();
    let first = store.begin_resolve("s1#0", request()).expect("first key");
    store.complete_resolve(first, Ok(writable()));
    let second = store.begin_resolve("s1#1", request()).expect("second key");
    store.complete_resolve(second, Err("boom".to_owned()));
    assert!(store.verdict("s1#0").is_some());
    assert_eq!(store.error("s1#1"), Some("boom"));

    store.clear_for_result("s1#0");
    assert_eq!(store.verdict("s1#0"), None);
    assert_eq!(store.generation("s1#0"), 2);
    assert!(store.verdict("s1#1").is_none(), "the error verdict stayed null");
    assert_eq!(store.error("s1#1"), Some("boom"), "the other key is untouched");

    store.clear_all();
    assert_eq!(store.generation("s1#1"), 0);
    assert_eq!(store.error("s1#1"), None);
    assert!(!store.resolving("s1#1"));
}

#[test]
fn verdict_view_reads_without_fetching_and_handles_a_null_key() {
    let mut store = PanelResultEditStore::new();
    let empty = verdict_view(&store, Some("s1#0"));
    assert_eq!(empty, VerdictView { verdict: None, resolving: false });
    assert_eq!(verdict_view(&store, None), empty);

    let ticket = store.begin_resolve("s1#0", request()).expect("resolve");
    assert!(verdict_view(&store, Some("s1#0")).resolving);
    store.complete_resolve(ticket, Ok(writable()));
    let view = verdict_view(&store, Some("s1#0"));
    assert!(view.verdict.is_some_and(|verdict| verdict.writable));
    assert!(!view.resolving);
}

#[test]
fn columns_roundtrip_and_the_effect_dependency_uses_content() {
    let cols = vec!["id".to_owned(), "name".to_owned()];
    let key = columns_key(&cols);
    assert_eq!(key, r#"["id","name"]"#);
    assert_eq!(columns_from_key(&key), cols);
    assert_eq!(columns_from_key("not json"), Vec::<String>::new());

    let a = options(Some("s1#0"));
    let mut b = options(Some("s1#0"));
    b.result_columns = vec!["id".to_owned(), "name".to_owned()];
    assert_eq!(effect_dependency_key(&a), effect_dependency_key(&b), "equal content, different array");
    b.sql = "select 1".to_owned();
    assert_ne!(effect_dependency_key(&a), effect_dependency_key(&b));
}

#[test]
fn effect_request_requires_a_key_and_roundtrips_columns() {
    assert!(effect_request(&options(None)).is_none());
    let (key, req) = effect_request(&options(Some("s1#0"))).expect("a key produces a request");
    assert_eq!(key, "s1#0");
    assert_eq!(req.db_session_id, "s1");
    assert_eq!(req.database.as_deref(), Some("db"));
    assert_eq!(req.schema, None);
    assert_eq!(req.sql, "select * from users");
    assert_eq!(req.result_columns, vec!["id".to_owned(), "name".to_owned()]);
    assert_eq!(req.table.as_deref(), Some("users"));
}

#[test]
fn start_effect_begins_a_resolve_and_deduplicates() {
    let mut store = PanelResultEditStore::new();
    let first = start_effect(&mut store, &options(Some("s1#0")));
    assert!(first.is_some());
    assert_eq!(store.generation("s1#0"), 1);
    assert!(store.resolving("s1#0"));
    assert!(start_effect(&mut store, &options(Some("s1#0"))).is_none());
    assert!(start_effect(&mut store, &options(None)).is_none());
}

#[test]
fn writable_guard_only_closes_the_origin_channel() {
    let mut store = PanelResultEditStore::new();
    assert!(assert_result_writable(&store, "s1", 0).is_ok(), "an unjudged result is not refused");

    let mut ticket = store.begin_resolve("s1#0", request()).expect("resolve");
    store.complete_resolve(ticket, Ok(writable()));
    assert!(assert_result_writable(&store, "s1", 0).is_ok(), "a writable verdict passes");

    ticket = store.begin_resolve("s1#0", request()).expect("re-resolve");
    store.complete_resolve(ticket, Ok(read_only(Some(GridReadOnlyReason::View))));
    let err = assert_result_writable(&store, "s1", 0).unwrap_err();
    assert_eq!(err.message_key(), "query.gridReadOnly.view");

    ticket = store.begin_resolve("s1#0", request()).expect("re-resolve");
    store.complete_resolve(ticket, Ok(read_only(None)));
    assert_eq!(assert_result_writable(&store, "s1", 0).unwrap_err().message_key(), "query.gridReadOnly.unknownOrigin");
    assert!(assert_result_writable(&store, "other", 0).is_ok());
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::result_edit_key;
    let _ = crate::grid_read_only_message_key;
    let _ = crate::verdict_view;
    let _ = crate::columns_key;
    let _ = crate::columns_from_key;
    let _ = crate::effect_dependency_key;
    let _ = crate::effect_request;
    let _ = crate::start_effect;
    let _ = crate::assert_result_writable;
    assert_eq!(crate::PANEL_RESULT_EDIT_GAP.len(), 4);
    let _ = crate::PanelResultEditStore::new();
    let _: Option<crate::GridReadOnlyReason> = None;
    let _: Option<crate::IdentityColumn> = None;
    let _: Option<crate::ResultTableRef> = None;
    let _: Option<crate::ServerEditability> = None;
    let _: Option<crate::ResultOriginRequest> = None;
    let _: Option<crate::ResultEditabilityOptions> = None;
    let _: Option<crate::VerdictView> = None;
    let _: Option<crate::ResolveTicket> = None;
    let _: Option<crate::ResultWritableError> = None;
}
