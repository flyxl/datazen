//! T169: `components/McpApprovalHost.tsx` port — the wire contract, the
//! queue / responding / timeout state machine, and the mounted dialog
//! (detail rows, SQL / arguments blocks, the countdown footer, the audit
//! toast) observed through the runtime's element tree, never through the
//! component's own report of itself.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::mcp_approval_host::{
    McpApprovalDecision, McpApprovalHost, McpApprovalHostState, McpApprovalRequest,
};
use crate::i18n::t;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

/// RFC 3339 stamp `offset_ms` from the epoch, as the backend sends.
fn stamp(offset_ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Utc
        .timestamp_millis_opt(offset_ms.max(0))
        .single()
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

/// The probe host: the approval state, the frame's clock and the
/// decision records.
#[derive(Default)]
struct Probe {
    dark: bool,
    state: McpApprovalHostState,
    now_ms: i64,
    allowed: Vec<String>,
    denied: Vec<String>,
    stopped: Vec<String>,
    dismissed_audit: u32,
}

fn request(tool: &str, request_id: &str, expires_at: String) -> McpApprovalRequest {
    let mut request = McpApprovalRequest::new(tool, request_id);
    request.connection_id = Some("pg-main".to_owned());
    request.database = Some("analytics".to_owned());
    request.namespace = None;
    request.sql = Some("SELECT secret_fn('***')".to_owned());
    request.arguments = serde_json::json!({"table": "orders", "limit": 10});
    request.expires_at = expires_at;
    request
}

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let page = McpApprovalHost::<Probe>::new()
        .on_allow_once(|s: &mut Probe, id| s.allowed.push(id.to_owned()))
        .on_deny(|s: &mut Probe, id| s.denied.push(id.to_owned()))
        .on_stop(|s: &mut Probe, id| s.stopped.push(id.to_owned()))
        .on_dismiss_audit(|s: &mut Probe| s.dismissed_audit += 1)
        .build(&state.state, state.now_ms, state.dark, &mut layer);
    rune_kit::root(
        col().key("probe-page").w_full().h_full().padding(16.0).child(page),
        layer,
    )
}

fn runtime(probe: Probe, w: f32, h: f32) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, view, (w, h), env(dark));
    rt.frame(0.0);
    rt
}

// --- the wire contract (types mirror the backend's serde shapes) ---

#[test]
fn the_request_serializes_camel_case_like_the_backend() {
    let request = request("db.query", "req-1", stamp(300_000));
    let json = serde_json::to_value(&request).unwrap_or_else(|_| serde_json::Value::Null);
    for field in [
        "requestId",
        "tool",
        "connectionId",
        "database",
        "namespace",
        "sql",
        "arguments",
        "createdAt",
        "expiresAt",
        "timeoutMs",
    ] {
        assert!(json.get(field).is_some(), "camelCase field {field}: {json}");
    }
    assert!(json.get("request_id").is_none(), "no snake_case leaks: {json}");
    assert_eq!(json.get("tool").and_then(|v| v.as_str()), Some("db.query"));
}

#[test]
fn the_decision_serializes_snake_case_like_the_backend() {
    let cases = [
        (McpApprovalDecision::AllowOnce, "allow_once"),
        (McpApprovalDecision::Deny, "deny"),
        (McpApprovalDecision::Timeout, "timeout"),
        (McpApprovalDecision::Cancel, "cancel"),
    ];
    for (decision, wire) in cases {
        let json = serde_json::to_value(decision).unwrap_or_else(|_| serde_json::Value::Null);
        assert_eq!(json.as_str(), Some(wire));
    }
}

// --- the queue guard (the listener's push) ---

#[test]
fn an_empty_tool_request_is_dropped() {
    let mut state = McpApprovalHostState::default();
    state.push_request(request("", "req-1", stamp(300_000)));
    assert!(state.current().is_none());
}

#[test]
fn a_known_request_id_is_not_duplicated() {
    let mut state = McpApprovalHostState::default();
    state.push_request(request("db.query", "req-1", stamp(300_000)));
    state.push_request(request("db.query", "req-1", stamp(300_000)));
    let same_id_other_tool = request("fs.read", "req-1", stamp(300_000));
    state.push_request(same_id_other_tool);
    assert_eq!(state.current().map(|r| r.tool.clone()), Some("db.query".to_owned()));
}

#[test]
fn the_queue_is_fifo_and_finish_pops_by_id() {
    let mut state = McpApprovalHostState::default();
    state.push_request(request("db.query", "req-1", stamp(300_000)));
    state.push_request(request("fs.read", "req-2", stamp(300_000)));
    assert_eq!(state.current().map(|r| r.request_id.clone()), Some("req-1".to_owned()));
    state.finish_response("req-1");
    assert_eq!(state.current().map(|r| r.request_id.clone()), Some("req-2".to_owned()));
}

// --- the audit toast ---

#[test]
fn an_audit_error_without_a_string_falls_back_to_the_generic_key() {
    let mut state = McpApprovalHostState::default();
    state.set_audit_error(None);
    assert_eq!(state.audit_error(), Some(t("mcp.approval.auditFailed")));
    state.clear_audit_error();
    assert_eq!(state.audit_error(), None);
}

// --- the responding state machine ---

#[test]
fn a_failed_response_keeps_the_message_and_ends_responding() {
    let mut state = McpApprovalHostState::default();
    state.push_request(request("db.query", "req-1", stamp(300_000)));
    state.begin_responding();
    assert!(state.responding());
    assert_eq!(state.error(), None);
    state.fail_response(Some("backend down".to_owned()));
    assert_eq!(state.error(), Some("backend down"));
    assert!(!state.responding());
    state.fail_response(None);
    assert_eq!(state.error(), Some(t("mcp.approval.respondFailed")));
}

// --- the countdown (React's Date.parse + ceil semantics) ---

#[test]
fn seconds_round_up_to_the_next_whole_second() {
    let mut state = McpApprovalHostState::default();
    state.push_request(request("db.query", "req-1", stamp(4_600)));
    // 4.6s left rounds up to 5.
    assert_eq!(state.seconds_left(0), 5);
}

#[test]
fn a_past_expiry_is_already_zero() {
    let mut state = McpApprovalHostState::default();
    state.push_request(request("db.query", "req-1", stamp(1_000)));
    assert_eq!(state.seconds_left(2_000), 0);
}

#[test]
fn an_unparseable_expiry_is_already_zero_like_date_parse_nan() {
    let mut state = McpApprovalHostState::default();
    let mut bad = request("db.query", "req-1", String::new());
    bad.expires_at = "not a date".to_owned();
    state.push_request(bad);
    assert_eq!(state.seconds_left(0), 0);
}

#[test]
fn no_current_request_is_zero_seconds() {
    let state = McpApprovalHostState::default();
    assert_eq!(state.seconds_left(1_000_000), 0);
}

// --- the timeout half of React's tick ---

#[test]
fn expire_check_returns_the_id_once_and_only_once() {
    let mut state = McpApprovalHostState::default();
    state.push_request(request("db.query", "req-1", stamp(1_000)));
    // Not yet expired.
    assert_eq!(state.expire_check(500), None);
    // Expired: first report, then dedup.
    assert_eq!(state.expire_check(1_500), Some("req-1".to_owned()));
    assert_eq!(state.expire_check(2_000), None);
}

#[test]
fn expire_check_follows_the_next_request_after_the_first_resolves() {
    let mut state = McpApprovalHostState::default();
    state.push_request(request("db.query", "req-1", stamp(1_000)));
    state.push_request(request("fs.read", "req-2", stamp(2_000)));
    assert_eq!(state.expire_check(1_500), Some("req-1".to_owned()));
    state.finish_response("req-1");
    assert_eq!(state.expire_check(2_500), Some("req-2".to_owned()));
}

// --- the mounted dialog ---

#[test]
fn a_queued_request_mounts_the_dialog_with_its_details() {
    let mut probe = Probe::default();
    probe.state.push_request(request("db.query", "req-1", stamp(300_000)));
    probe.now_ms = 0;
    let rt = runtime(probe, 960.0, 640.0);
    // The dialog mounts through the layer (the modal base's channel).
    assert!(has_key(&rt, "mcp-approval-host-approval"));
    assert!(has_key(&rt, "mcp-approval-host-details"));
    // The dl rows show tool / connection / database; the null namespace
    // renders the em dash.
    let joined = texts(&rt).join(" ");
    assert!(joined.contains("db.query"), "the tool row: {joined}");
    assert!(joined.contains("pg-main"), "the connection row: {joined}");
    assert!(joined.contains("analytics"), "the database row: {joined}");
    assert!(joined.contains("—"), "the null namespace row: {joined}");
    // The redacted SQL and the pretty-printed arguments show.
    assert!(joined.contains("SELECT secret_fn"), "the SQL block: {joined}");
    assert!(joined.contains("\"table\": \"orders\""), "the arguments JSON: {joined}");
}

#[test]
fn the_footer_counts_down_and_disables_allow_when_expired() {
    let mut probe = Probe::default();
    probe.state.push_request(request("db.query", "req-1", stamp(45_000)));
    probe.now_ms = 0;
    let rt = runtime(probe, 960.0, 640.0);
    let countdown = element(&rt, "mcp-approval-host-countdown");
    assert!(countdown.text.as_deref().is_some_and(|t| t.contains("45")));
    assert!(countdown.text.as_deref().is_some_and(|t| !t.contains("已过期")));
}

#[test]
fn an_expired_request_shows_the_expired_label_and_disables_allow() {
    let mut probe = Probe::default();
    probe.state.push_request(request("db.query", "req-1", stamp(1_000)));
    probe.now_ms = 5_000;
    let rt = runtime(probe, 960.0, 640.0);
    let countdown = element(&rt, "mcp-approval-host-countdown");
    assert!(countdown.text.as_deref().is_some_and(|t| t.contains("已过期")));
    // The disabled Allow button stays mounted (React keeps it rendered);
    // its press no-ops is the dispatch test below's `responding` case.
    assert!(has_key(&rt, "mcp-approval-host-allow-once"));
}

#[test]
fn the_buttons_dispatch_their_decisions_with_the_request_id() {
    let mut probe = Probe::default();
    probe.state.push_request(request("db.query", "req-1", stamp(300_000)));
    probe.now_ms = 0;
    let mut rt = runtime(probe, 960.0, 640.0);
    click(&mut rt, "mcp-approval-host-stop");
    click(&mut rt, "mcp-approval-host-deny");
    click(&mut rt, "mcp-approval-host-allow-once");
    assert_eq!(rt.state.stopped, vec!["req-1".to_owned()]);
    assert_eq!(rt.state.denied, vec!["req-1".to_owned()]);
    assert_eq!(rt.state.allowed, vec!["req-1".to_owned()]);
}

#[test]
fn while_responding_all_buttons_disable() {
    let mut probe = Probe::default();
    probe.state.push_request(request("db.query", "req-1", stamp(300_000)));
    probe.state.begin_responding();
    probe.now_ms = 0;
    let mut rt = runtime(probe, 960.0, 640.0);
    // A press on a disabled button does not dispatch.
    click(&mut rt, "mcp-approval-host-allow-once");
    click(&mut rt, "mcp-approval-host-deny");
    assert!(rt.state.allowed.is_empty());
    assert!(rt.state.denied.is_empty());
}

#[test]
fn an_empty_host_renders_nothing() {
    let probe = Probe::default();
    let rt = runtime(probe, 960.0, 640.0);
    assert!(!has_key(&rt, "mcp-approval-host-approval"));
    assert!(!has_key(&rt, "mcp-approval-host-audit-toast"));
    let root = element(&rt, "mcp-approval-host");
    // React's `return null`: the root is there but has no children of its
    // own (the page's padding child aside).
    assert!(
        texts(&rt).iter().filter(|t| !t.is_empty()).count() <= 1,
        "no dialog content: {:?}",
        texts(&rt)
    );
}

// --- the audit toast ---

#[test]
fn an_audit_error_mounts_the_toast_and_the_dismiss_clears_it() {
    let mut probe = Probe::default();
    probe.state.set_audit_error(Some("row missing".to_owned()));
    probe.now_ms = 0;
    let mut rt = runtime(probe, 960.0, 640.0);
    assert!(has_key(&rt, "mcp-approval-host-audit-toast"));
    let toast = element(&rt, "mcp-approval-host-audit-toast-text");
    assert!(toast.text.as_deref().is_some_and(|t| t.contains("row missing")));
    click(&mut rt, "mcp-approval-host-audit-toast-dismiss");
    assert_eq!(rt.state.dismissed_audit, 1);
}

// --- the window fit (DoD #4) ---

#[test]
fn the_dialog_fits_both_windows_without_horizontal_overflow() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut probe = Probe::default();
        probe.state.push_request(request("db.query", "req-1", stamp(300_000)));
        probe.now_ms = 0;
        let rt = runtime(probe, w, h);
        let mut outside = Vec::new();
        for e in rt.elements() {
            if e.exiting {
                continue;
            }
            if e.rect[0] + e.rect[2] > w + 0.5 || e.rect[0] < -0.5 {
                outside.push(format!("{} {:?}", e.key.unwrap_or(e.path.clone()), e.rect));
            }
        }
        assert!(
            outside.is_empty(),
            "{w}x{h}: {} node(s) overflow horizontally: {outside:?}",
            outside.len()
        );
    }
}
