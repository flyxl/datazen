//! T169 tester audit: an independent read of `McpApprovalHost.tsx`
//! against the port, through the runtime's own channels (element keys /
//! texts / rects) and serde round-trips — never the component's own
//! report of itself.
//!
//! Audit scope: the wire contract round-trips both ways (the backend
//! JSON parses into the mirror and back), the queue guard and FIFO
//! resolution, the countdown's exact boundaries (a whole-second expiry,
//! a hair under a second, exactly at the expiry), the timeout dedup,
//! the mounted dialog's layer channel, and the two-window fit.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::mcp_approval_host::{
    McpApprovalDecision, McpApprovalHost, McpApprovalHostState, McpApprovalRequest,
};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn stamp(offset_ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Utc
        .timestamp_millis_opt(offset_ms.max(0))
        .single()
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

/// The audit host, fed through the same surface the host process would.
#[derive(Default)]
struct Audit {
    dark: bool,
    state: McpApprovalHostState,
    now_ms: i64,
    allowed: Vec<String>,
    denied: Vec<String>,
    stopped: Vec<String>,
}

fn fed_request(tool: &str, request_id: &str, expires_at: String) -> McpApprovalRequest {
    let mut request = McpApprovalRequest::new(tool, request_id);
    request.database = Some("db0".to_owned());
    request.arguments = serde_json::json!({"n": 1});
    request.expires_at = expires_at;
    request
}

fn view(state: &Audit, _cx: &mut Cx<Audit>) -> El {
    let mut layer = Vec::new();
    let page = McpApprovalHost::<Audit>::new()
        .on_allow_once(|s: &mut Audit, id| s.allowed.push(id.to_owned()))
        .on_deny(|s: &mut Audit, id| s.denied.push(id.to_owned()))
        .on_stop(|s: &mut Audit, id| s.stopped.push(id.to_owned()))
        .build(&state.state, state.now_ms, state.dark, &mut layer);
    rune_kit::root(
        col().key("audit-page").w_full().h_full().padding(16.0).child(page),
        layer,
    )
}

fn runtime(audit: Audit, w: f32, h: f32) -> Runtime<Audit> {
    let dark = audit.dark;
    let mut rt = Runtime::new(audit, view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

// --- the wire contract, both ways ---

#[test]
fn the_backend_json_round_trips_through_the_mirror() {
    // The exact camelCase shape the backend's serde emits.
    let wire = r#"{
        "requestId": "req-9",
        "tool": "sql.exec",
        "connectionId": null,
        "database": "db0",
        "namespace": null,
        "sql": "SELECT 1",
        "arguments": {"a": [1, 2]},
        "createdAt": "2026-11-20T00:00:00Z",
        "expiresAt": "2026-11-20T00:05:00Z",
        "timeoutMs": 300000
    }"#;
    let parsed: McpApprovalRequest =
        serde_json::from_str(wire).unwrap_or_else(|_| McpApprovalRequest::new("", ""));
    assert_eq!(parsed.request_id, "req-9");
    assert_eq!(parsed.tool, "sql.exec");
    assert_eq!(parsed.timeout_ms, 300_000);
    assert_eq!(parsed.arguments, serde_json::json!({"a": [1, 2]}));
    let back = serde_json::to_value(&parsed).unwrap_or_else(|_| serde_json::Value::Null);
    assert_eq!(back.get("requestId").and_then(|v| v.as_str()), Some("req-9"));
    assert_eq!(back.get("timeoutMs").and_then(|v| v.as_u64()), Some(300_000));
}

#[test]
fn every_decision_value_survives_a_json_round_trip() {
    for decision in [
        McpApprovalDecision::AllowOnce,
        McpApprovalDecision::Deny,
        McpApprovalDecision::Timeout,
        McpApprovalDecision::Cancel,
    ] {
        let json = serde_json::to_string(&decision).unwrap_or_default();
        let back: McpApprovalDecision = serde_json::from_str(&json).unwrap_or(McpApprovalDecision::Deny);
        assert_eq!(back, decision, "{json}");
    }
}

// --- the queue and its resolution ---

#[test]
fn the_second_request_waits_until_the_first_resolves() {
    let mut state = McpApprovalHostState::default();
    state.push_request(fed_request("a", "req-1", stamp(60_000)));
    // A re-delivery of the same id stays one entry (the listener's
    // at-least-once delivery is the world the React guard lives in).
    state.push_request(fed_request("a", "req-1", stamp(60_000)));
    assert_eq!(state.queue_len(), 1);
    state.push_request(fed_request("b", "req-2", stamp(60_000)));
    // A failing respond keeps the request queued (React leaves it on the
    // error path); only a successful one pops it.
    state.begin_responding();
    state.fail_response(None);
    assert_eq!(state.current().map(|r| r.request_id.clone()), Some("req-1".to_owned()));
    state.begin_responding();
    state.finish_response("req-1");
    assert_eq!(state.current().map(|r| r.request_id.clone()), Some("req-2".to_owned()));
}

#[test]
fn an_empty_tool_never_enters_the_queue_even_after_resolves() {
    let mut state = McpApprovalHostState::default();
    state.push_request(fed_request("a", "req-1", stamp(60_000)));
    state.finish_response("req-1");
    state.push_request(fed_request("", "req-2", stamp(60_000)));
    assert!(state.current().is_none());
}

// --- the countdown's exact boundaries ---

#[test]
fn a_whole_second_remaining_reports_that_second() {
    let mut state = McpApprovalHostState::default();
    state.push_request(fed_request("a", "req-1", stamp(5_000)));
    assert_eq!(state.seconds_left(0), 5);
}

#[test]
fn a_hair_under_a_second_rounds_up_to_one() {
    let mut state = McpApprovalHostState::default();
    state.push_request(fed_request("a", "req-1", stamp(999)));
    assert_eq!(state.seconds_left(0), 1);
}

#[test]
fn exactly_at_the_expiry_is_already_expired() {
    let mut state = McpApprovalHostState::default();
    state.push_request(fed_request("a", "req-1", stamp(5_000)));
    assert_eq!(state.seconds_left(5_000), 0);
    // And the expiry check fires at that exact moment.
    assert_eq!(state.expire_check(5_000), Some("req-1".to_owned()));
}

#[test]
fn one_millisecond_before_the_expiry_still_reports_one_second() {
    let mut state = McpApprovalHostState::default();
    state.push_request(fed_request("a", "req-1", stamp(5_000)));
    assert_eq!(state.seconds_left(4_999), 1);
    assert_eq!(state.expire_check(4_999), None);
}

// --- the mounted dialog through the layer channel ---

#[test]
fn the_dialog_mounts_only_with_a_current_request() {
    let mut audit = Audit::default();
    audit.state.set_audit_error(Some("x".to_owned()));
    audit.now_ms = 0;
    let rt = runtime(audit, 960.0, 640.0);
    // The toast shows, but no dialog: there is no current request.
    assert!(has_key(&rt, "mcp-approval-host-audit-toast"));
    assert!(!has_key(&rt, "mcp-approval-host-approval"));

    let mut audit = Audit::default();
    audit.state.push_request(fed_request("sql.exec", "req-1", stamp(60_000)));
    audit.now_ms = 0;
    let rt = runtime(audit, 960.0, 640.0);
    assert!(has_key(&rt, "mcp-approval-host-approval"));
    assert!(!has_key(&rt, "mcp-approval-host-audit-toast"));
}

#[test]
fn the_dialog_shows_the_exact_sql_and_arguments_json() {
    let mut audit = Audit::default();
    let mut request = fed_request("sql.exec", "req-1", stamp(60_000));
    request.sql = Some("UPDATE t SET x = 'redacted' WHERE id = 1".to_owned());
    audit.state.push_request(request);
    audit.now_ms = 0;
    let rt = runtime(audit, 960.0, 640.0);
    let joined = texts(&rt).join(" ");
    assert!(joined.contains("UPDATE t SET x = 'redacted'"), "{joined}");
    assert!(joined.contains("\"n\": 1"), "the arguments pretty JSON: {joined}");
}

#[test]
fn a_request_without_sql_renders_no_sql_block() {
    let mut audit = Audit::default();
    let request = fed_request("fs.read", "req-1", stamp(60_000));
    audit.state.push_request(request);
    audit.now_ms = 0;
    let rt = runtime(audit, 960.0, 640.0);
    assert!(!has_key(&rt, "mcp-approval-host-sql"));
    // The arguments block always renders (React prints JSON even for
    // null: "null").
    assert!(has_key(&rt, "mcp-approval-host-arguments"));
}

// --- the window fit (the task's DoD #4) ---

#[test]
fn the_host_fits_960_and_1440_windows() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut audit = Audit::default();
        audit.state.push_request(fed_request("sql.exec", "req-1", stamp(60_000)));
        audit.now_ms = 0;
        let rt = runtime(audit, w, h);
        let mut outside = Vec::new();
        for e in rt.elements() {
            if e.exiting {
                continue;
            }
            if e.rect[0] < -0.5 || e.rect[0] + e.rect[2] > w + 0.5 {
                outside.push(e.key.clone().unwrap_or(e.path.clone()));
            }
        }
        assert!(outside.is_empty(), "{w}x{h} overflow: {outside:?}");
    }
}
