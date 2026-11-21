use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::{json, Value};

use super::*;
use crate::types::wapp::WappPermission;
use crate::ThemeSnapshot;

mod lifecycle;

fn snapshot(dark: bool) -> ThemeSnapshot {
    let mut tokens = HashMap::new();
    tokens.insert("--c-surface".to_owned(), "#0b0e14".to_owned());
    ThemeSnapshot { v: 2, dark, tokens }
}

struct RecordingHost {
    now: i64,
    nonce_seq: u64,
    dark: bool,
    connections: Vec<ConnectionConfigLike>,
    active: Option<String>,
    storage: HashMap<String, Value>,
    driver_result: Result<Value, String>,
    driver_calls: RefCell<Vec<(String, String)>>,
    audits: RefCell<Vec<(String, String, String)>>,
    notifications: RefCell<Vec<(String, Option<String>)>>,
    strings: HashMap<String, String>,
    next_instance: u64,
    session_ends: RefCell<Vec<String>>,
    session_calls: RefCell<Vec<Value>>,
    session_result: Result<Value, String>,
}

impl Default for RecordingHost {
    fn default() -> Self {
        Self {
            now: 0,
            nonce_seq: 0,
            dark: false,
            connections: Vec::new(),
            active: None,
            storage: HashMap::new(),
            driver_result: Ok(Value::Null),
            driver_calls: RefCell::new(Vec::new()),
            audits: RefCell::new(Vec::new()),
            notifications: RefCell::new(Vec::new()),
            strings: HashMap::new(),
            next_instance: 0,
            session_ends: RefCell::new(Vec::new()),
            session_calls: RefCell::new(Vec::new()),
            session_result: Ok(Value::Null),
        }
    }
}

impl WappBridgeHost for RecordingHost {
    fn now_ms(&mut self) -> i64 {
        self.now
    }

    fn new_nonce(&mut self) -> String {
        self.nonce_seq += 1;
        format!("nonce-{}", self.nonce_seq)
    }

    fn theme_snapshot(&self) -> ThemeSnapshot {
        snapshot(self.dark)
    }

    fn load_connections(&self) -> Vec<ConnectionConfigLike> {
        self.connections.clone()
    }

    fn active_connection_id(&self) -> Option<String> {
        self.active.clone()
    }

    fn storage_get(&self, _wapp_id: &str, key: &str) -> Option<Value> {
        self.storage.get(key).cloned()
    }

    fn storage_set(&mut self, _wapp_id: &str, key: &str, value: Value) {
        self.storage.insert(key.to_owned(), value);
    }

    fn storage_remove(&mut self, _wapp_id: &str, key: &str) {
        self.storage.remove(key);
    }

    fn execute_driver_command(
        &self,
        _wapp_id: &str,
        connection_id: &str,
        command: &str,
        _args: Value,
    ) -> Result<Value, String> {
        self.driver_calls
            .borrow_mut()
            .push((connection_id.to_owned(), command.to_owned()));
        self.driver_result.clone()
    }

    fn audit_log(&self, wapp_id: &str, api: &str, detail: &str) {
        self.audits
            .borrow_mut()
            .push((wapp_id.to_owned(), api.to_owned(), detail.to_owned()));
    }

    fn show_notification(&self, title: &str, body: Option<&str>) -> Result<(), String> {
        self.notifications
            .borrow_mut()
            .push((title.to_owned(), body.map(str::to_owned)));
        Ok(())
    }

    fn resolve_string(&self, _wapp_id: &str, key: &str, _locale: &str) -> Option<String> {
        self.strings.get(key).cloned()
    }

    fn session_register(&mut self, _wapp_id: &str) -> Result<String, String> {
        self.next_instance += 1;
        Ok(format!("instance-{}", self.next_instance))
    }

    fn session_invoke(&self, request: &Value) -> Result<Value, String> {
        self.session_calls.borrow_mut().push(request.clone());
        self.session_result.clone()
    }

    fn session_end(&self, instance_id: &str) {
        self.session_ends.borrow_mut().push(instance_id.to_owned());
    }
}

fn bridge(permissions: &[WappPermission]) -> WappBridge {
    WappBridge::new(
        AttachBridgeOptions {
            wapp_id: "acme.test".to_owned(),
            permissions: permissions.to_vec(),
            ..AttachBridgeOptions::default()
        },
        "nonce-1".to_owned(),
    )
}

fn envelope(kind: &str, req_id: &str, payload: Option<Value>) -> Value {
    let mut value = json!({
        "ch": BRIDGE_CHANNEL,
        "type": kind,
        "target": "host",
        "reqId": req_id,
    });
    if let Some(payload) = payload {
        value["payload"] = payload;
    }
    value
}

fn parse(wire: &Value) -> WappRequestEnvelope {
    parse_request(wire).unwrap_or_else(|| panic!("expected a request envelope"))
}

fn wire(value: Option<Value>) -> Value {
    value.unwrap_or_else(|| panic!("expected a wire value"))
}

fn ok_of<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("expected ok, got {error:?}"),
    }
}

fn err_of<T, E: std::fmt::Debug>(result: Result<T, E>) -> E {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected an error"),
    }
}

fn text(value: Option<&str>) -> String {
    value
        .unwrap_or_else(|| panic!("expected a string"))
        .to_owned()
}

fn posts(effects: &[BridgeEffect]) -> Vec<Value> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            BridgeEffect::PostMessage(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
}

fn last_post(effects: &[BridgeEffect]) -> Value {
    posts(effects).last().cloned().unwrap_or(Value::Null)
}

fn error_code(post: &Value) -> Option<String> {
    post.get("payload")
        .and_then(|value| value.get("code"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn connected_config(id: &str) -> ConnectionConfigLike {
    ConnectionConfigLike {
        id: id.to_owned(),
        name: format!("Name {id}"),
        database_type: "postgres".to_owned(),
    }
}

#[test]
fn protocol_constants_are_the_react_literals() {
    assert_eq!(BRIDGE_CHANNEL, "datazen-wapp");
    assert_eq!(REQUEST_TIMEOUT_MS, 30_000);
    assert_eq!(MAX_INFLIGHT_REQUESTS, 20);
    assert_eq!(NOTIFY_MIN_INTERVAL_MS, 5_000);
    assert_eq!(MAX_REQUEST_ID_CHARS, 128);
    assert_eq!(SESSION_REQUEST_LIMIT, 1024);
    assert_eq!(ERROR_MESSAGE_LIMIT_CHARS, 500);
    assert_eq!(
        BRIDGE_ERROR_CODES,
        [
            "E_PERMISSION",
            "E_NOT_FOUND",
            "E_TIMEOUT",
            "E_RATE_LIMIT",
            "E_WAPP_DISABLED",
            "E_BAD_REQUEST",
            "E_NOT_IMPLEMENTED",
            "E_INTERNAL",
        ]
    );
}

#[test]
fn error_codes_round_trip_through_wire_strings() {
    for code in [
        BridgeErrorCode::Permission,
        BridgeErrorCode::NotFound,
        BridgeErrorCode::Timeout,
        BridgeErrorCode::RateLimit,
        BridgeErrorCode::WappDisabled,
        BridgeErrorCode::BadRequest,
        BridgeErrorCode::NotImplemented,
        BridgeErrorCode::Internal,
    ] {
        assert_eq!(wire(serde_json::to_value(code).ok()), json!(code.as_str()));
        assert_eq!(BridgeErrorCode::from_str(code.as_str()), Some(code));
    }
    assert_eq!(BridgeErrorCode::from_str("E_UNKNOWN"), None);
}

#[test]
fn response_types_carry_the_ok_or_err_suffix() {
    assert_eq!(response_type_of("storage.get", true), "storage.get.ok");
    assert_eq!(
        response_type_of("command.invoke", false),
        "command.invoke.err"
    );
}

#[test]
fn a_request_envelope_decodes_and_reencodes_the_camel_wire() {
    let input = envelope("storage.get", "req-1", Some(json!({"key": "k"})));
    let request = parse(&input);
    assert_eq!(request.kind, "storage.get");
    assert_eq!(request.req_id.as_deref(), Some("req-1"));
    assert_eq!(request.target, BridgeTarget::Host);
    assert_eq!(request.ch, BRIDGE_CHANNEL);
    assert_eq!(wire(serde_json::to_value(&request).ok()), input);
}

#[test]
fn envelope_validation_rejects_wrong_channel_target_and_type() {
    assert!(!is_request_envelope(
        &json!({"ch": "datazen-extension", "type": "x", "target": "host"})
    ));
    assert!(!is_request_envelope(
        &json!({"ch": BRIDGE_CHANNEL, "type": "x", "target": "guest"})
    ));
    assert!(!is_request_envelope(
        &json!({"ch": BRIDGE_CHANNEL, "target": "host"})
    ));
    assert!(!is_request_envelope(
        &json!({"ch": BRIDGE_CHANNEL, "type": 7, "target": "host"})
    ));
    assert!(!is_request_envelope(&Value::Null));
    assert!(parse_request(&json!({"ch": BRIDGE_CHANNEL, "type": "x", "target": "host"})).is_some());
}

#[test]
fn request_id_validation_enforces_nonempty_and_128_chars() {
    assert!(!BridgeState::request_id_valid(&None));
    assert!(!BridgeState::request_id_valid(&Some(String::new())));
    assert!(BridgeState::request_id_valid(&Some("x".repeat(128))));
    assert!(!BridgeState::request_id_valid(&Some("x".repeat(129))));
}

#[test]
fn the_route_table_is_the_deny_by_default_contract() {
    assert_eq!(API_ROUTES.len(), 12);
    let expected = [
        ("context.getConnections", WappPermission::ContextConnections),
        (
            "context.getActiveConnection",
            WappPermission::ContextConnections,
        ),
        ("command.invoke", WappPermission::CommandInvoke),
        ("session.open", WappPermission::SessionStateful),
        ("session.use", WappPermission::SessionStateful),
        ("session.close", WappPermission::SessionStateful),
        ("session.receipt", WappPermission::SessionStateful),
        ("storage.get", WappPermission::StorageLocal),
        ("storage.set", WappPermission::StorageLocal),
        ("storage.remove", WappPermission::StorageLocal),
    ];
    for (kind, permission) in expected {
        assert_eq!(route_for(kind), Some(Some(permission)));
    }
    assert_eq!(route_for("ui.notify"), Some(None));
    assert_eq!(route_for("i18n.getString"), Some(None));
    assert_eq!(route_for("Context.getConnections"), None);
    assert_eq!(route_for("toString"), None);
    assert_eq!(route_for("constructor"), None);
    assert_eq!(route_for("__proto__"), None);
    assert_eq!(route_for("fs.readFile"), None);
}

#[test]
fn the_command_denylist_blocks_only_admin_verbs() {
    assert_eq!(WAPP_COMMAND_DENYLIST.len(), 9);
    for command in WAPP_COMMAND_DENYLIST {
        assert!(!is_wapp_command_allowed(command));
    }
    assert!(is_wapp_command_allowed("list_databases"));
    assert!(is_wapp_command_allowed("Execute"));
}

#[test]
fn the_connection_summary_is_constructively_whitelisted() {
    let summary = to_public_connection(&connected_config("conn_1"));
    let output = wire(serde_json::to_value(&summary).ok());
    assert_eq!(
        output,
        json!({"id": "conn_1", "name": "Name conn_1", "dbType": "postgres"})
    );
    assert!(!output.to_string().contains("password"));
    assert!(!output.to_string().contains("host"));
}

#[test]
fn command_payload_validation_pins_legality_of_args() {
    assert_eq!(
        err_of(parse_command_invoke(None)).code,
        BridgeErrorCode::BadRequest
    );
    assert_eq!(
        err_of(parse_command_invoke(Some(&json!({"connectionId": "c"})))).code,
        BridgeErrorCode::BadRequest
    );
    assert_eq!(
        err_of(parse_command_invoke(Some(
            &json!({"connectionId": 7, "command": "x"})
        )))
        .code,
        BridgeErrorCode::BadRequest
    );
    assert_eq!(
        err_of(parse_command_invoke(Some(
            &json!({"connectionId": "c", "command": "x", "args": 5})
        )))
        .code,
        BridgeErrorCode::BadRequest
    );
    assert_eq!(
        err_of(parse_command_invoke(Some(
            &json!({"connectionId": "c", "command": "x", "args": []})
        )))
        .code,
        BridgeErrorCode::BadRequest
    );
    let null_args = ok_of(parse_command_invoke(Some(
        &json!({"connectionId": "c", "command": "x", "args": null}),
    )));
    assert_eq!(null_args.args, json!({}));
    let object_args = ok_of(parse_command_invoke(Some(
        &json!({"connectionId": "c", "command": "x", "args": {"a": 1}}),
    )));
    assert_eq!(object_args.args, json!({"a": 1}));
}

#[test]
fn storage_notify_and_i18n_payload_validation_is_strict() {
    assert_eq!(
        err_of(parse_storage("storage.get", None)).code,
        BridgeErrorCode::BadRequest
    );
    assert_eq!(
        err_of(parse_storage("storage.get", Some(&json!({"key": ""})))).code,
        BridgeErrorCode::BadRequest
    );
    assert_eq!(
        ok_of(parse_storage("storage.get", Some(&json!({"key": "k"})))).key,
        "k"
    );
    assert_eq!(
        err_of(parse_notify(Some(&json!({"body": "x"})))).code,
        BridgeErrorCode::BadRequest
    );
    assert_eq!(
        err_of(parse_notify(Some(&json!({"title": "t", "body": 7})))).code,
        BridgeErrorCode::BadRequest
    );
    let notify = ok_of(parse_notify(Some(&json!({"title": "t", "body": ""}))));
    assert_eq!(notify.title, "t");
    assert_eq!(notify.body.as_deref(), Some(""));
    assert_eq!(
        err_of(parse_i18n_key(None)).code,
        BridgeErrorCode::BadRequest
    );
    assert_eq!(
        ok_of(parse_i18n_key(Some(&json!({"key": "greet"})))),
        "greet"
    );
}

#[test]
fn not_found_rejection_detection_is_case_insensitive() {
    assert!(looks_like_not_found("Connection not found"));
    assert!(looks_like_not_found("NO SUCH database"));
    assert!(looks_like_not_found("unknown command"));
    assert!(!looks_like_not_found("permission denied"));
}

#[test]
fn attach_registers_both_listeners() {
    let mut host = RecordingHost::default();
    let mut b = bridge(&[]);
    assert_eq!(
        b.attach(),
        vec![
            BridgeEffect::AddFrameLoadListener,
            BridgeEffect::AddMessageListener
        ]
    );
    assert_eq!(
        posts(&b.on_message(&mut host, true, &envelope("wapp.ready", "", None))).len(),
        1
    );
}

#[test]
fn wapp_ready_replies_with_the_handshake_snapshot() {
    let mut host = RecordingHost::default();
    host.dark = true;
    let mut b = bridge(&[]);
    let effects = b.on_message(&mut host, true, &envelope("wapp.ready", "", None));
    let post = last_post(&effects);
    assert_eq!(post["type"], "host.ready");
    assert_eq!(post["target"], "host");
    assert_eq!(post["ch"], BRIDGE_CHANNEL);
    let payload = &post["payload"];
    assert_eq!(payload["apiVersion"], 2);
    assert_eq!(payload["instanceNonce"], "nonce-1");
    assert_eq!(payload["locale"], "en");
    assert_eq!(payload["dark"], true);
    assert_eq!(payload["tokens"]["--c-surface"], "#0b0e14");
}

#[test]
fn a_foreign_source_or_malformed_message_is_ignored() {
    let mut host = RecordingHost::default();
    let mut b = bridge(&[WappPermission::StorageLocal]);
    assert!(b
        .on_message(&mut host, false, &envelope("storage.get", "x", None))
        .is_empty());
    assert!(b
        .on_message(
            &mut host,
            true,
            &json!({"ch": "other", "type": "storage.get", "target": "host", "reqId": "x"})
        )
        .is_empty());
    assert!(b
        .on_message(
            &mut host,
            true,
            &json!({"ch": BRIDGE_CHANNEL, "type": "storage.get", "target": "host"})
        )
        .is_empty());
    assert!(b
        .on_message(
            &mut host,
            true,
            &json!({"ch": BRIDGE_CHANNEL, "type": "storage.get", "target": "host", "reqId": ""})
        )
        .is_empty());
    assert!(b.on_message(&mut host, true, &json!({"ch": BRIDGE_CHANNEL, "type": "storage.get", "target": "host", "reqId": "x".repeat(129)})).is_empty());
    assert!(host.storage.is_empty());
}

#[test]
fn permission_denied_and_unknown_routes_never_reach_the_host() {
    let mut host = RecordingHost::default();
    let mut b = bridge(&[]);
    let effects = b.on_message(
        &mut host,
        true,
        &envelope("storage.get", "deny", Some(json!({"key": "k"}))),
    );
    assert_eq!(
        error_code(&last_post(&effects)).as_deref(),
        Some("E_PERMISSION")
    );
    assert_eq!(last_post(&effects)["type"], "storage.get.err");
    assert_eq!(last_post(&effects)["reqId"], "deny");
    assert!(host.storage.is_empty());

    let mut b = bridge(&[WappPermission::StorageLocal]);
    let effects = b.on_message(&mut host, true, &envelope("fs.readFile", "u", None));
    assert_eq!(
        error_code(&last_post(&effects)).as_deref(),
        Some("E_NOT_FOUND")
    );
    assert_eq!(last_post(&effects)["type"], "fs.readFile.err");
}

#[test]
fn a_successful_dispatch_echoes_reqid_and_releases_the_quota() {
    let mut host = RecordingHost::default();
    let mut b = bridge(&[WappPermission::StorageLocal]);
    let effects = b.on_message(
        &mut host,
        true,
        &envelope("storage.get", "ok-1", Some(json!({"key": "k"}))),
    );
    let post = last_post(&effects);
    assert_eq!(post["type"], "storage.get.ok");
    assert_eq!(post["reqId"], "ok-1");
    assert_eq!(post["payload"]["value"], Value::Null);
    assert_eq!(b.state.inflight, 0);
    assert!(effects.iter().any(|e| matches!(
        e,
        BridgeEffect::SetTimer {
            duration_ms: 30_000,
            ..
        }
    )));
    assert!(effects
        .iter()
        .any(|e| matches!(e, BridgeEffect::ClearTimer { .. })));
}

#[test]
fn rate_limit_rejects_before_the_handler_runs() {
    let mut host = RecordingHost::default();
    host.driver_result = Ok(json!({"ok": true}));
    let mut b = bridge(&[WappPermission::CommandInvoke]);
    b.state.inflight = b.options.max_inflight;
    let effects = b.on_message(
        &mut host,
        true,
        &envelope(
            "command.invoke",
            "flood",
            Some(json!({"connectionId": "c", "command": "list"})),
        ),
    );
    assert_eq!(
        error_code(&last_post(&effects)).as_deref(),
        Some("E_RATE_LIMIT")
    );
    assert!(host.driver_calls.borrow().is_empty());
    assert_eq!(b.state.inflight, b.options.max_inflight);
}

#[test]
fn connections_are_whitelisted_for_both_context_apis() {
    let mut host = RecordingHost::default();
    host.connections = vec![connected_config("conn_1")];
    host.active = Some("conn_1".to_owned());
    let mut b = bridge(&[WappPermission::ContextConnections]);
    let all = last_post(&b.on_message(
        &mut host,
        true,
        &envelope("context.getConnections", "a", None),
    ));
    assert_eq!(
        all["payload"]["connections"],
        json!([{"id": "conn_1", "name": "Name conn_1", "dbType": "postgres"}])
    );
    let active = last_post(&b.on_message(
        &mut host,
        true,
        &envelope("context.getActiveConnection", "b", None),
    ));
    assert_eq!(active["payload"]["connection"]["id"], "conn_1");
    host.active = None;
    let idle = last_post(&b.on_message(
        &mut host,
        true,
        &envelope("context.getActiveConnection", "c", None),
    ));
    assert_eq!(idle["payload"]["connection"], Value::Null);
}

#[test]
fn command_invoke_audits_and_maps_backend_errors() {
    let mut host = RecordingHost::default();
    host.driver_result = Ok(json!({"rows": 1}));
    let mut b = bridge(&[WappPermission::CommandInvoke]);
    let input = envelope(
        "command.invoke",
        "cmd",
        Some(json!({"connectionId": "c", "command": "list"})),
    );
    let post = last_post(&b.on_message(&mut host, true, &input));
    assert_eq!(post["payload"]["result"], json!({"rows": 1}));
    assert_eq!(host.audits.borrow()[0].1, "command.invoke");
    assert_eq!(host.audits.borrow()[0].2, "list via c");

    host.driver_result = Err("table not found".to_owned());
    let post = last_post(&b.on_message(
        &mut host,
        true,
        &envelope(
            "command.invoke",
            "nf",
            Some(json!({"connectionId": "c", "command": "list"})),
        ),
    ));
    assert_eq!(error_code(&post).as_deref(), Some("E_NOT_FOUND"));

    host.driver_result = Err("x".repeat(600));
    let post = last_post(&b.on_message(
        &mut host,
        true,
        &envelope(
            "command.invoke",
            "in",
            Some(json!({"connectionId": "c", "command": "list"})),
        ),
    ));
    assert_eq!(error_code(&post).as_deref(), Some("E_INTERNAL"));
    let message = text(post["payload"]["message"].as_str());
    assert_eq!(message.chars().count(), 501);
    assert!(message.ends_with('…'));
}

#[test]
fn a_denylisted_command_is_refused_even_with_permission() {
    let mut host = RecordingHost::default();
    let mut b = bridge(&[WappPermission::CommandInvoke]);
    let effects = b.on_message(
        &mut host,
        true,
        &envelope(
            "command.invoke",
            "drop",
            Some(json!({"connectionId": "c", "command": "drop_database"})),
        ),
    );
    assert_eq!(
        error_code(&last_post(&effects)).as_deref(),
        Some("E_PERMISSION")
    );
    assert!(host.driver_calls.borrow().is_empty());
}

#[test]
fn storage_apis_route_to_the_host_seam() {
    let mut host = RecordingHost::default();
    let mut b = bridge(&[WappPermission::StorageLocal]);
    b.on_message(
        &mut host,
        true,
        &envelope("storage.set", "s", Some(json!({"key": "k", "value": 7}))),
    );
    assert_eq!(host.storage.get("k"), Some(&json!(7)));
    let post = last_post(&b.on_message(
        &mut host,
        true,
        &envelope("storage.get", "g", Some(json!({"key": "k"}))),
    ));
    assert_eq!(post["payload"]["value"], 7);
    b.on_message(
        &mut host,
        true,
        &envelope("storage.remove", "r", Some(json!({"key": "k"}))),
    );
    assert!(host.storage.is_empty());
}

#[test]
fn ui_notify_is_limited_to_one_shot_per_cooldown() {
    let mut host = RecordingHost::default();
    let mut b = bridge(&[]);
    let first = b.on_message(
        &mut host,
        true,
        &envelope("ui.notify", "n1", Some(json!({"title": "t"}))),
    );
    assert_eq!(last_post(&first)["type"], "ui.notify.ok");
    let second = b.on_message(
        &mut host,
        true,
        &envelope("ui.notify", "n2", Some(json!({"title": "t"}))),
    );
    assert_eq!(
        error_code(&last_post(&second)).as_deref(),
        Some("E_RATE_LIMIT")
    );
    host.now = 5_000;
    let third = b.on_message(
        &mut host,
        true,
        &envelope("ui.notify", "n3", Some(json!({"title": "t"}))),
    );
    assert_eq!(last_post(&third)["type"], "ui.notify.ok");
    assert_eq!(host.notifications.borrow().len(), 2);
}

#[test]
fn i18n_resolution_reports_a_null_value_for_unknown_keys() {
    let mut host = RecordingHost::default();
    host.strings.insert("greet".to_owned(), "你好".to_owned());
    let mut b = bridge(&[]);
    let hit = last_post(&b.on_message(
        &mut host,
        true,
        &envelope("i18n.getString", "i1", Some(json!({"key": "greet"}))),
    ));
    assert_eq!(hit["payload"], json!({"key": "greet", "value": "你好"}));
    let miss = last_post(&b.on_message(
        &mut host,
        true,
        &envelope("i18n.getString", "i2", Some(json!({"key": "absent"}))),
    ));
    assert_eq!(miss["payload"]["value"], Value::Null);
}
