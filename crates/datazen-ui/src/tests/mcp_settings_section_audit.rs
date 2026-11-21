//! T348 audit: the McpSettingsSection port, verified against the React
//! source re-read line by line
//! (`windows/settings/McpSettingsSection.tsx` — the React
//! `__tests__/McpSettingsSection.test.tsx` and `.copyFeedback.test.tsx`
//! cover the React side; the expectations below are re-derived from
//! its JSX and handlers). Element evidence comes from
//! `Runtime::elements()`, presses go through real pointer events (the
//! `ai_settings_section_audit` harness pattern).

use std::rc::Rc;

use rune::{col, El, Env, Runtime, TextEdit};

use crate::views::mcp_settings_section::McpSettingsSection;
use crate::views::mcp_settings_section_state::{
    build_mcp_agent_snippet, format_mcp_cli_command, ApplyPortPlan, HttpTogglePlan,
    McpAgentTarget, McpAuditDecision, McpAuditRecordVm, McpConnectionVm, McpHttpStatusVm,
    McpPermissionMode, McpSettingsProjection, McpSettingsSectionState, ReloadIfRunning,
    StdioTogglePlan, MCP_AUDIT_LIMIT, MCP_COPIED_WINDOW, MCP_POLL_INTERVAL, MCP_SAVED_WINDOW,
};

fn status(running: bool, enabled: bool, port: i64) -> McpHttpStatusVm {
    McpHttpStatusVm {
        running,
        enabled,
        port: Some(port),
        configured_port: port,
        token_last4: Some("ab12".to_owned()),
        token_configured: true,
        url: Some(format!("http://127.0.0.1:{port}")),
    }
}

fn audit_row(id: &str, decision: McpAuditDecision) -> McpAuditRecordVm {
    McpAuditRecordVm {
        id: id.to_owned(),
        principal_id: "agent".to_owned(),
        connection_id: Some("conn-1".to_owned()),
        database_name: Some("app".to_owned()),
        namespace: None,
        tool: "query".to_owned(),
        decision,
        row_count: Some(3),
        request_id: Some("req-9".to_owned()),
        created_at: "2026-11-21T00:00:00Z".to_owned(),
    }
}

#[derive(Clone)]
struct AuditState {
    section: McpSettingsSectionState,
    dark: bool,
    now: f64,
    // the host's records
    stdio_toggles: Vec<bool>,
    http_toggles: Vec<bool>,
    ports_applied: usize,
    regenerated: usize,
    permissions: Vec<McpPermissionMode>,
    tools_toggled: Vec<String>,
    allowed_toggled: Vec<String>,
    enable_alls: usize,
    disable_alls: usize,
    tools_saved: usize,
    pending_disabled_tools: Vec<String>,
    scope_saved: usize,
    snippet_copies: usize,
    token_copies: usize,
    targets: Vec<McpAgentTarget>,
    audit_refreshes: usize,
}

impl AuditState {
    fn empty() -> Self {
        Self {
            section: McpSettingsSectionState::new(),
            dark: true,
            now: 100.0,
            stdio_toggles: Vec::new(),
            http_toggles: Vec::new(),
            ports_applied: 0,
            regenerated: 0,
            permissions: Vec::new(),
            tools_toggled: Vec::new(),
            allowed_toggled: Vec::new(),
            enable_alls: 0,
            disable_alls: 0,
            tools_saved: 0,
            pending_disabled_tools: Vec::new(),
            scope_saved: 0,
            snippet_copies: 0,
            token_copies: 0,
            targets: Vec::new(),
            audit_refreshes: 0,
        }
    }
}

fn view(s: &AuditState, _cx: &mut rune::Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let section = McpSettingsSection::<AuditState>::new(
        &s.section,
        s.dark,
        s.now,
        "/usr/local/bin/datazen",
        |s: &mut AuditState| &mut s.section.database_scope,
        |s: &mut AuditState| &mut s.section.namespace_scope,
        |s: &mut AuditState| &mut s.section.http_port,
    )
    .on_toggle_enabled(|s, next| {
        s.stdio_toggles.push(next);
        if let StdioTogglePlan::Apply { enabled, .. } = s.section.begin_toggle_enabled(next) {
            s.section.settings.mcp_server_enabled = Some(enabled);
            s.section.note_toggle_done();
        }
    })
    .on_toggle_http(|s, next| {
        s.http_toggles.push(next);
        match s.section.begin_toggle_http(next) {
            HttpTogglePlan::Enable { port } => {
                s.section.note_http_enabled(status(next, next, port), Some("tok-123".to_owned()));
            }
            HttpTogglePlan::Disable => {
                s.section.note_http_disabled(status(false, false, 0));
            }
            _ => {}
        }
    })
    .on_apply_port(|s| {
        s.ports_applied += 1;
        if let ApplyPortPlan::Apply { port, .. } = s.section.begin_apply_http_port() {
            s.section.note_http_port_applied(port, Some(status(true, true, port)), None);
        }
    })
    .on_regenerate_token(|s| {
        s.regenerated += 1;
        s.section.begin_regenerate_token();
        s.section.note_http_regenerated(status(true, true, 8080), Some("tok-456".to_owned()));
    })
    .on_permission_change(|s, mode| {
        s.permissions.push(mode);
        let _ = s.section.begin_permission_change(mode);
    })
    .on_toggle_tool(|s, tool| {
        s.tools_toggled.push(tool.clone());
        // The host's store write is IN FLIGHT (the React dirty state is
        // exactly this lag): the local list moves, the settings land
        // with the save below.
        let next = s.section.toggle_tool(&tool);
        s.pending_disabled_tools = next;
    })
    .on_toggle_allowed(|s, id| {
        s.allowed_toggled.push(id.clone());
        s.section.settings.mcp_allowed_connection_ids = Some(s.section.toggle_allowed(&id));
    })
    .on_enable_all(|s| {
        s.enable_alls += 1;
        s.section.settings.mcp_disabled_tools = Some(s.section.set_all_tools_enabled(true));
    })
    .on_disable_all(|s| {
        s.disable_alls += 1;
        s.section.settings.mcp_disabled_tools = Some(s.section.set_all_tools_enabled(false));
    })
    .on_save_tools(|s| {
        s.tools_saved += 1;
        let _ = s.section.begin_save_tools(s.now);
    })
    .on_save_scope(|s| {
        s.scope_saved += 1;
        let _ = s.section.begin_save_scope(s.now);
    })
    .on_copy_snippet(|s| {
        s.snippet_copies += 1;
        s.section.note_copied(s.now);
    })
    .on_copy_token(|s| {
        s.token_copies += 1;
        s.section.note_copied(s.now);
    })
    .on_set_agent_target(|s, target| {
        s.targets.push(target);
        s.section.agent_target = target;
    })
    .on_refresh_audit(|s| {
        s.audit_refreshes += 1;
        s.section.begin_load_audit();
    })
    .build(&mut layer);
    col().max_w(720.0).h_full().child(section).children(layer)
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    // The section is tall (every block stacked); a tall viewport keeps
    // the lower blocks (tools, audit) on-screen for pointer presses.
    let mut rt = Runtime::new(scene, view, (960.0, 3200.0), Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements().into_iter().any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<AuditState>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    settle(rt);
}

// ---- the pure rules ----------------------------------------------------------

/// `parseScope`: trim, drop empties, dedup keeping first-seen order.
#[test]
fn parse_scope_splits_trims_and_dedups() {
    let state = McpSettingsSectionState::new();
    assert_eq!(
        McpSettingsSectionState::parse_scope("app,  ops\n\napp , analytics\n"),
        ["app", "ops", "analytics"],
        "first-seen order, the duplicate app once"
    );
    assert_eq!(McpSettingsSectionState::parse_scope("  \n , "), Vec::<String>::new());
    assert_eq!(McpSettingsSectionState::parse_scope("single"), ["single"]);
}

/// `parseHttpPort`: the integer 0..=65535 rule (empty = 0, the random
/// port sentinel; garbage/range = -1).
#[test]
fn parse_http_port_follows_the_number_rule() {
    let mut state = McpSettingsSectionState::new();
    state.http_port = TextEdit::new(String::new());
    assert_eq!(state.parse_http_port(), 0, "empty is the random-port sentinel");
    state.http_port = TextEdit::new("  8423 ".to_owned());
    assert_eq!(state.parse_http_port(), 8423, "whitespace trims");
    state.http_port = TextEdit::new("65536".to_owned());
    assert_eq!(state.parse_http_port(), -1, "above the range");
    state.http_port = TextEdit::new("-1".to_owned());
    assert_eq!(state.parse_http_port(), -1, "below the range");
    state.http_port = TextEdit::new("12.5".to_owned());
    assert_eq!(state.parse_http_port(), -1, "a float is not an integer");
    state.http_port = TextEdit::new("abc".to_owned());
    assert_eq!(state.parse_http_port(), -1);
    state.http_port = TextEdit::new("65535".to_owned());
    assert_eq!(state.parse_http_port(), 65535, "the top of the range");
}

/// The stdio toggle's walk and its FAILED-ENABLE revert.
#[test]
fn the_stdio_toggle_reverts_a_failed_enable() {
    let mut state = McpSettingsSectionState::new();
    state.settings.mcp_server_enabled = Some(false);
    let StdioTogglePlan::Apply { enabled: true, .. } = state.begin_toggle_enabled(true) else {
        panic!("the first toggle applies");
    };
    assert!(state.toggling);
    // The start throws: the message surfaces AND the setting reverts.
    state.note_toggle_error(true, "spawn failed".to_owned());
    assert_eq!(state.toggle_error.as_deref(), Some("spawn failed"));
    assert!(!state.toggling);
    assert_eq!(state.settings.mcp_server_enabled, Some(false), "the revert write");
    // A disable failure does NOT flip the setting on.
    let _ = state.begin_toggle_enabled(false);
    state.note_toggle_error(false, "stop failed".to_owned());
    assert_eq!(state.settings.mcp_server_enabled, Some(false));
    // The in-flight guard.
    state.toggling = true;
    assert!(matches!(state.begin_toggle_enabled(true), StdioTogglePlan::Refused));
}

/// The HTTP flows: the invalid-port refusal, the enable/regenerate/
/// disable answers, and the apply-port's requested-port write.
#[test]
fn the_http_flows_walk_the_react_paths() {
    let mut state = McpSettingsSectionState::new();
    state.http_port = TextEdit::new("99999".to_owned());
    assert!(matches!(state.begin_toggle_http(true), HttpTogglePlan::InvalidPort));
    assert_eq!(state.http_error.as_deref(), Some("mcp.http.portInvalid"));

    state.http_port = TextEdit::new("8423".to_owned());
    let HttpTogglePlan::Enable { port } = state.begin_toggle_http(true) else {
        panic!("a valid port enables");
    };
    assert_eq!(port, 8423);
    // The enable answer: the setting persists the CONFIGURED port and
    // the once-only token shows.
    state.note_http_enabled(status(true, true, 8424), Some("tok".to_owned()));
    assert_eq!(state.settings.mcp_http_enabled, Some(true));
    assert_eq!(state.settings.mcp_http_port, Some(8424), "configuredPort, not the request");
    assert_eq!(state.http_token.as_deref(), Some("tok"));
    assert!(!state.http_toggling);

    // The regenerate answer: NO settings write, the new token shows.
    state.note_http_regenerated(status(true, true, 8424), Some("tok2".to_owned()));
    assert_eq!(state.http_token.as_deref(), Some("tok2"));

    // The disable answer: the token clears, the setting turns off.
    state.note_http_disabled(status(false, false, 0));
    assert_eq!(state.http_token, None);
    assert_eq!(state.settings.mcp_http_enabled, Some(false));

    // The apply-port: invalid refused; valid writes the REQUESTED port
    // (the answer's lands only through the re-enable).
    state.http_port = TextEdit::new("nope".to_owned());
    assert!(matches!(state.begin_apply_http_port(), ApplyPortPlan::InvalidPort));
    state.http_port = TextEdit::new("9000".to_owned());
    state.http_status = Some(status(true, true, 8424));
    let ApplyPortPlan::Apply { port, reload } = state.begin_apply_http_port() else {
        panic!("a valid port applies");
    };
    assert_eq!(port, 9000);
    assert_eq!(reload, ReloadIfRunning::Reload, "the running listener re-enables");
    state.note_http_port_applied(9000, None, None);
    assert_eq!(state.settings.mcp_http_port, Some(9000), "the REQUESTED port persists");
}

/// `reloadMcpIfRunning` and the two dirty rules.
#[test]
fn the_reload_predicate_and_the_dirty_rules() {
    let mut state = McpSettingsSectionState::new();
    assert_eq!(state.reload_if_running(), ReloadIfRunning::Skip);
    state.running = true;
    assert_eq!(state.reload_if_running(), ReloadIfRunning::Reload);
    state.running = false;
    state.http_status = Some(status(true, true, 1));
    assert_eq!(state.reload_if_running(), ReloadIfRunning::Reload);

    // scopeDirty: order-insensitive, whitespace/newline-tolerant.
    state.settings.mcp_allowed_databases = Some(vec!["app".to_owned(), "ops".to_owned()]);
    state.database_scope = TextEdit::new("ops, app".to_owned());
    assert!(!state.scope_dirty(), "the same set in another order is clean");
    state.database_scope = TextEdit::new("ops, app, extra".to_owned());
    assert!(state.scope_dirty());
    state.database_scope = TextEdit::new("ops, app".to_owned());
    state.namespace_scope = TextEdit::new("public".to_owned());
    assert!(state.scope_dirty(), "the namespace side counts too");

    // toolsDirty.
    let mut state = McpSettingsSectionState::new();
    assert!(!state.tools_dirty());
    state.disabled_tools = vec!["t1".to_owned()];
    assert!(state.tools_dirty());
    state.settings.mcp_disabled_tools = Some(vec!["t1".to_owned()]);
    assert!(!state.tools_dirty());
    state.allowed_ids = vec!["c1".to_owned()];
    assert!(state.tools_dirty(), "the allowed side counts too");
}

/// The carried `mcpAgentConfig` rules (T557's, 1:1).
#[test]
fn the_carried_agent_config_rules() {
    let cursor = build_mcp_agent_snippet(McpAgentTarget::Cursor, "/opt/datazen app");
    assert_eq!(cursor.target, McpAgentTarget::Cursor);
    assert_eq!(cursor.config_path_hint, ".cursor/mcp.json (project) or Cursor Settings → MCP");
    assert!(cursor.json.contains("\"mcpServers\""));
    assert!(cursor.json.contains("\"datazen\""));
    assert!(cursor.json.contains("\"--mcp\""));
    assert!(cursor.json.contains("\"/opt/datazen app\""));
    // Two-space pretty print.
    assert!(cursor.json.contains("\n  \"mcpServers\""), "JSON.stringify(…, null, 2)");
    let claude = build_mcp_agent_snippet(McpAgentTarget::Claude, "datazen");
    assert_eq!(
        claude.config_path_hint,
        "claude_desktop_config.json (Claude Desktop app support directory)"
    );

    // formatMcpCliCommand.
    assert_eq!(format_mcp_cli_command("datazen"), "datazen --mcp");
    assert_eq!(format_mcp_cli_command(""), "datazen --mcp", "the empty fallback");
    assert_eq!(format_mcp_cli_command("  "), "datazen --mcp");
    assert_eq!(format_mcp_cli_command("/opt/my app"), "\"/opt/my app\" --mcp", "spaces quote");
    assert_eq!(format_mcp_cli_command("\"/opt/my app\""), "\"/opt/my app\" --mcp", "already quoted stays");
    assert_eq!(format_mcp_cli_command("'/x/y'"), "'/x/y' --mcp");
}

/// The three windows and the constants.
#[test]
fn the_windows_and_constants() {
    assert_eq!(MCP_COPIED_WINDOW, 2.0);
    assert_eq!(MCP_SAVED_WINDOW, 2.0);
    assert_eq!(MCP_POLL_INTERVAL, 3.0);
    assert_eq!(MCP_AUDIT_LIMIT, 50);
    let mut state = McpSettingsSectionState::new();
    assert!(!state.copied(100.0));
    state.note_copied(100.0);
    assert!(state.copied(101.9));
    assert!(!state.copied(102.0), "the copy window closes at +2s");
    let _ = state.begin_save_tools(100.0);
    assert!(state.tools_saved(101.9));
    assert!(!state.tools_saved(102.0));
    let _ = state.begin_save_scope(100.0);
    assert!(state.scope_saved(101.9));
    assert!(!state.scope_saved(102.0));
}

/// The resync effect seeds the drafts from the settings.
#[test]
fn resync_seeds_the_drafts() {
    let mut state = McpSettingsSectionState::new();
    state.settings = McpSettingsProjection {
        mcp_server_enabled: Some(true),
        mcp_disabled_tools: Some(vec!["t1".to_owned()]),
        mcp_allowed_connection_ids: Some(vec!["c1".to_owned()]),
        mcp_allowed_databases: Some(vec!["app".to_owned(), "ops".to_owned()]),
        mcp_allowed_namespaces: Some(vec!["public".to_owned()]),
        mcp_permission_mode: Some(McpPermissionMode::ReadOnly),
        mcp_http_enabled: Some(true),
        mcp_http_port: Some(8423),
    };
    state.resync_from_settings();
    assert_eq!(state.disabled_tools, ["t1"]);
    assert_eq!(state.allowed_ids, ["c1"]);
    assert_eq!(state.database_scope.text, "app, ops");
    assert_eq!(state.namespace_scope.text, "public");
    assert_eq!(state.http_port.text, "8423");
}

/// The tools/allowlist membership flips (last-write, immediate write).
#[test]
fn the_membership_flips() {
    let mut state = McpSettingsSectionState::new();
    state.all_tools = vec!["t1".to_owned(), "t2".to_owned()];
    assert_eq!(state.toggle_tool("t1"), ["t1"]);
    assert_eq!(state.toggle_tool("t1"), Vec::<String>::new(), "the second press removes");
    assert_eq!(state.toggle_tool("t2"), ["t2"]);
    assert_eq!(state.set_all_tools_enabled(false), ["t1", "t2"]);
    assert_eq!(state.set_all_tools_enabled(true), Vec::<String>::new());
    assert_eq!(state.toggle_allowed("c1"), ["c1"]);
    assert_eq!(state.toggle_allowed("c1"), Vec::<String>::new());
}

// ---- the element evidence ------------------------------------------------------

/// The mount shape: the rows and headings, no tools list without
/// tools, the permission trio.
#[test]
fn mount_renders_the_rows_and_headings() {
    let rt = rt(AuditState::empty());
    assert!(has(&rt, "mcp-settings-title"));
    assert!(has(&rt, "mcp-stdio-toggle"));
    assert!(has(&rt, "mcp-stdio-status"));
    assert!(has(&rt, "mcp-http-toggle"));
    assert!(has(&rt, "mcp-http-port"));
    assert!(has(&rt, "mcp-http-apply-port"));
    assert!(has(&rt, "mcp-http-regenerate"));
    assert!(has(&rt, "mcp-permission-read_only"));
    assert!(has(&rt, "mcp-permission-safe_write"));
    assert!(has(&rt, "mcp-permission-high_risk_write"));
    assert!(has(&rt, "mcp-allowlist-row"), "the setting row (the list key only exists with connections)");
    assert!(has(&rt, "mcp-scope-databases"));
    assert!(has(&rt, "mcp-scope-namespaces"));
    assert!(has(&rt, "mcp-agent-snippet"));
    assert!(has(&rt, "mcp-agent-json"));
    assert!(has(&rt, "mcp-audit-refresh"));
    assert!(!has(&rt, "mcp-tools"), "no tools list without tools");
    assert!(!has(&rt, "mcp-http-token"), "no token display before a token");
    // The scope save button renders but disabled (nothing dirty).
    assert!(has(&rt, "mcp-scope-save"));
}

/// The status lines follow the poll answers.
#[test]
fn the_status_lines_follow_the_polls() {
    let mut scene = AuditState::empty();
    scene.section.running = false;
    let stopped = rt(scene.clone());
    assert_eq!(text_of(&stopped, "mcp-stdio-status-label"), crate::i18n::t("mcp.stopped"));
    let mut running = rt(scene);
    running.update(|s, _| s.section.deliver_status(true));
    settle(&mut running);
    assert_eq!(text_of(&running, "mcp-stdio-status-label"), crate::i18n::t("mcp.running"));
}

/// The permission cards fire and the HTTP toggle walks the enable
/// flow (the token display appears with its once line).
#[test]
fn the_permission_and_http_flows_render() {
    let mut scene = AuditState::empty();
    scene.section.http_port = TextEdit::new("8423".to_owned());
    let mut rt = rt(scene);
    press(&mut rt, "mcp-permission-radio-read_only");
    assert_eq!(rt.state.permissions, [McpPermissionMode::ReadOnly]);
    assert_eq!(
        rt.state.section.settings.mcp_permission_mode,
        Some(McpPermissionMode::ReadOnly)
    );
    // The HTTP enable: the flow lands the token + the once line.
    press(&mut rt, "mcp-http-toggle");
    assert_eq!(rt.state.http_toggles, [true]);
    assert!(has(&rt, "mcp-http-token"));
    assert!(has(&rt, "mcp-http-token-once"));
    assert!(has(&rt, "mcp-http-copy"));
    // The apply port button fires the requested-port write.
    press(&mut rt, "mcp-http-apply-port");
    assert_eq!(rt.state.ports_applied, 1);
    assert_eq!(rt.state.section.settings.mcp_http_port, Some(8423));
    // The regenerate replaces the token.
    press(&mut rt, "mcp-http-regenerate");
    assert_eq!(rt.state.regenerated, 1);
    assert_eq!(rt.state.section.http_token.as_deref(), Some("tok-456"));
}

/// The tools block: switches fire, the save follows the dirty rule,
/// enable/disable-all fire.
#[test]
fn the_tools_block_renders_and_fires() {
    let mut scene = AuditState::empty();
    scene.section.all_tools = vec!["query".to_owned(), "fetch".to_owned()];
    scene.section.resync_from_settings();
    let mut rt = rt(scene);
    assert!(has(&rt, "mcp-tools"));
    assert!(has(&rt, "mcp-tool-query"));
    assert!(has(&rt, "mcp-tool-fetch"));
    assert!(has(&rt, "mcp-tools-save"));
    assert!(has(&rt, "mcp-tools-enable-all"));
    // A tool switch flips membership and dirties the block.
    press(&mut rt, "mcp-tool-query");
    assert_eq!(rt.state.tools_toggled, ["query".to_owned()]);
    assert_eq!(rt.state.section.disabled_tools, ["query"]);
    assert!(rt.state.section.tools_dirty());
    // The save fires and arms the 2s line.
    press(&mut rt, "mcp-tools-save");
    assert_eq!(rt.state.tools_saved, 1);
    assert!(has(&rt, "mcp-tools-saved"));
    // Disable-all flips every tool off; enable-all clears.
    press(&mut rt, "mcp-tools-disable-all");
    assert_eq!(rt.state.section.disabled_tools.len(), 2);
    press(&mut rt, "mcp-tools-enable-all");
    assert!(rt.state.section.disabled_tools.is_empty());
}

/// The allowlist rows render and fire.
#[test]
fn the_allowlist_renders_and_fires() {
    let mut scene = AuditState::empty();
    scene.section.connections = vec![
        McpConnectionVm { id: "conn-123456789".to_owned(), name: "Prod".to_owned() },
        McpConnectionVm { id: "local-1".to_owned(), name: "Local".to_owned() },
    ];
    let mut rt = rt(scene);
    assert!(has(&rt, "mcp-allowlist-conn-123456789"));
    assert!(has(&rt, "mcp-allowlist-local-1"));
    press(&mut rt, "mcp-allowlist-conn-123456789");
    assert_eq!(rt.state.allowed_toggled, ["conn-123456789".to_owned()]);
    assert_eq!(rt.state.section.allowed_ids, ["conn-123456789"]);
}

/// The audit table renders the rows (the DataTable atom's cells).
#[test]
fn the_audit_table_renders_rows() {
    let mut scene = AuditState::empty();
    scene.section.audit_rows = vec![
        audit_row("a1", McpAuditDecision::Allow),
        audit_row("a2", McpAuditDecision::Deny),
    ];
    let table = rt(scene.clone());
    assert!(has(&table, "mcp-audit-table"));
    assert!(has(&table, "mcp-audit-table-row-0"));
    assert!(has(&table, "mcp-audit-table-row-1"));
    // The refresh link fires the loading walk.
    let mut refresh = rt(scene);
    press(&mut refresh, "mcp-audit-refresh");
    assert_eq!(refresh.state.audit_refreshes, 1);
}

/// The snippet tabs switch the target and the copy feedback flips.
#[test]
fn the_snippet_tabs_and_copy() {
    let mut rt = rt(AuditState::empty());
    let cursor_json = text_of(&rt, "mcp-agent-json");
    press(&mut rt, "mcp-agent-tab-claude");
    assert_eq!(rt.state.targets, [McpAgentTarget::Claude]);
    let claude_json = text_of(&rt, "mcp-agent-json");
    assert_eq!(
        cursor_json, claude_json,
        "same command, same JSON — only the path hint differs"
    );
    // The copy feedback flips the label for the window.
    press(&mut rt, "mcp-agent-copy");
    assert_eq!(rt.state.snippet_copies, 1);
    assert!(rt.state.section.copied(rt.state.now));
}
