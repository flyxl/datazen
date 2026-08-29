//! D1 regression anchors: these commands receive a **runtime** db session
//! id over IPC, so their wire parameter must be `db_session_id` (frontend
//! camelCase `dbSessionId`). An earlier revision shipped them as
//! `connection_id` (persisted-configuration semantics) while the bodies
//! called strict runtime-session lookups — "renamed but reversed". The
//! assertions below pin both directions so it cannot silently return.

const GENERATE_SOURCE: &str = include_str!("generate.rs");
const CHAT_SOURCE: &str = include_str!("chat.rs");
const WORKFLOW_SOURCE: &str = include_str!("../workflow.rs");

fn ai_command_sources() -> String {
    format!("{GENERATE_SOURCE}{CHAT_SOURCE}{WORKFLOW_SOURCE}")
}

/// Extracts the parameter list of a `pub async fn <command>(...)`.
fn command_params(command: &str) -> String {
    let source = ai_command_sources();
    let needle = format!("pub async fn {command}(");
    let start = source
        .find(&needle)
        .unwrap_or_else(|| panic!("command `{command}` not found in AI command modules"));
    let rest = &source[start + needle.len()..];
    let end = rest.find(')').expect("unterminated parameter list");
    rest[..end].to_string()
}

#[test]
fn session_semantics_commands_take_db_session_id() {
    for cmd in [
        "ai_generate_sql",
        "ai_diagnose_error",
        "ai_analyze_explain",
        "ai_parse_filter",
        "ai_chat",
        "ai_generate_schema_doc",
        "ai_analyze_queries",
    ] {
        let params = command_params(cmd);
        assert!(
            params.contains("db_session_id"),
            "`{cmd}` must take `db_session_id` (runtime session semantics); got: {params}"
        );
        let without_new = params.replace("db_session_id", "");
        assert!(
            !without_new.contains("connection_id"),
            "`{cmd}` must not take (or also take) `connection_id`; got: {params}"
        );
    }
}

#[test]
fn config_semantics_commands_keep_connection_id() {
    // workflow_execute feeds a persisted id into the executor's dual-mode
    // resolve; ai_diagnose_connection looks up the stored configuration.
    for cmd in ["workflow_execute", "ai_diagnose_connection"] {
        let params = command_params(cmd);
        assert!(
            params.contains("connection_id"),
            "`{cmd}` keeps persisted-configuration semantics and must take `connection_id`; got: {params}"
        );
        assert!(
            !params.contains("db_session_id"),
            "`{cmd}` must not take `db_session_id`; got: {params}"
        );
    }
}

#[test]
fn strict_session_lookups_are_never_fed_a_connection_id_binding() {
    // Body-level guard mirroring the signature guards above: if any of
    // these strings reappear, a strict runtime-session lookup is being fed
    // a variable named after the *persisted* configuration id. The needles
    // are assembled at runtime so this test module never contains them.
    let conn = "connection_";
    let id = "id";
    let conn_id = format!("{conn}{id}");
    let source = ai_command_sources();
    assert!(!source.contains(&format!(".get_session(&{conn_id})")));
    assert!(!source.contains(&format!(".get_session({conn_id})")));
    assert!(!source.contains(&format!("owner_connection_id(&{conn_id})")));
}
