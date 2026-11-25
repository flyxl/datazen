//! The McpSettingsSection state machine and view-models (the state half
//! of `windows/settings/McpSettingsSection.tsx`'s port, T348 — split out
//! of [`super::mcp_settings_section`] for the file-size rule, the
//! `model_profile_form` precedent). The mapping table lives in the
//! section module's header.
//!
//! Carried here until their own tasks absorb them (the `lib_helpers`
//! pattern): `lib/mcpAgentConfig.ts`'s pure half is segment-2's T557 —
//! [`build_mcp_agent_snippet`] / [`format_mcp_cli_command`] /
//! [`McpAgentTarget`] below are its 1:1 rules (the executable-path
//! fetch and its module cache are the host seam).

// No production consumer yet (the settings view wiring track mounts
// the section); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use rune::TextEdit;

use crate::commands::ai::AiRequest;
use crate::commands::ai::mcp::{mcp_list_audit_log_request, mcp_list_all_tools_request, mcp_reload_request, mcp_start_stdio_request, mcp_stop_request, mcp_http_disable_request, mcp_http_enable_request, mcp_http_regenerate_token_request, mcp_get_status_request, mcp_http_status_request};

// ---------------------------------------------------------------------------
// The view-models (React types/index.ts shapes the section reads)
// ---------------------------------------------------------------------------

/// `McpPermissionMode` (`SettingsMcpPermissionMode`, backend-client).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum McpPermissionMode {
    ReadOnly,
    #[default]
    SafeWrite,
    HighRiskWrite,
}

impl McpPermissionMode {
    /// The React literal (`'read_only' | 'safe_write' | 'high_risk_write'`).
    pub fn as_str(self) -> &'static str {
        match self {
            McpPermissionMode::ReadOnly => "read_only",
            McpPermissionMode::SafeWrite => "safe_write",
            McpPermissionMode::HighRiskWrite => "high_risk_write",
        }
    }

    /// `MCP_PERMISSION_MODES`'s table order.
    pub const ALL: [McpPermissionMode; 3] = [
        McpPermissionMode::ReadOnly,
        McpPermissionMode::SafeWrite,
        McpPermissionMode::HighRiskWrite,
    ];
}

/// `McpHttpStatus` (the settings listener's status — the shape
/// `aiCommands.mcpHttpStatus()` answers; the enable/regenerate answers
/// carry the same shape plus the once-only `token`, which lives in the
/// section's [`McpSettingsSectionState::http_token`] instead).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct McpHttpStatusVm {
    pub running: bool,
    pub enabled: bool,
    pub url: Option<String>,
    /// Actual bound port (random when configuredPort is 0).
    pub port: Option<i64>,
    pub configured_port: i64,
    pub token_last4: Option<String>,
    pub token_configured: bool,
}

/// `McpAuditRecord['decision']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum McpAuditDecision {
    Allow,
    Deny,
    Timeout,
    Cancel,
    Read,
}

impl McpAuditDecision {
    /// The persisted literal, rendered verbatim (React prints the raw
    /// string).
    pub fn as_str(self) -> &'static str {
        match self {
            McpAuditDecision::Allow => "allow",
            McpAuditDecision::Deny => "deny",
            McpAuditDecision::Timeout => "timeout",
            McpAuditDecision::Cancel => "cancel",
            McpAuditDecision::Read => "read",
        }
    }
}

/// `McpAuditRecord` — one audit-log row.
#[derive(Clone, Debug, PartialEq)]
pub struct McpAuditRecordVm {
    pub id: String,
    pub principal_id: String,
    pub connection_id: Option<String>,
    pub database_name: Option<String>,
    pub namespace: Option<String>,
    pub tool: String,
    pub decision: McpAuditDecision,
    pub row_count: Option<i64>,
    pub request_id: Option<String>,
    pub created_at: String,
}

/// One connection of the allowlist (the connectionStore projection —
/// the section reads `id` and `name` only).
#[derive(Clone, Debug, PartialEq)]
pub struct McpConnectionVm {
    pub id: String,
    pub name: String,
}

/// The AppSettings fields the section reads (the settings projection,
/// host-delivered and kept current; the draft-override mode replaces
/// the store, not the shape).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct McpSettingsProjection {
    pub mcp_server_enabled: Option<bool>,
    pub mcp_disabled_tools: Option<Vec<String>>,
    pub mcp_allowed_connection_ids: Option<Vec<String>>,
    pub mcp_allowed_databases: Option<Vec<String>>,
    pub mcp_allowed_namespaces: Option<Vec<String>>,
    pub mcp_permission_mode: Option<McpPermissionMode>,
    pub mcp_http_enabled: Option<bool>,
    pub mcp_http_port: Option<i64>,
}

// ---------------------------------------------------------------------------
// lib/mcpAgentConfig.ts (carried until T557 absorbs it)
// ---------------------------------------------------------------------------

/// `McpAgentTarget`: `'cursor' | 'claude'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum McpAgentTarget {
    #[default]
    Cursor,
    Claude,
}

/// `McpAgentSnippet` — the built snippet (target, where it goes, the
/// pretty JSON).
#[derive(Clone, Debug, PartialEq)]
pub struct McpAgentSnippet {
    pub target: McpAgentTarget,
    /// Where users typically place this JSON fragment.
    pub config_path_hint: &'static str,
    /// Pretty-printed JSON for the mcpServers wrapper.
    pub json: String,
}

/// `buildMcpAgentSnippet(target, command)`: the `mcpServers` wrapper
/// pretty-printed with 2-space indent (`JSON.stringify(…, null, 2)`),
/// and the per-target path hint.
pub fn build_mcp_agent_snippet(
    target: McpAgentTarget,
    command: &str,
) -> McpAgentSnippet {
    let wrapper = serde_json::json!({
        "mcpServers": {
            "datazen": {
                "command": command,
                "args": ["--mcp"],
            },
        },
    });
    McpAgentSnippet {
        target,
        config_path_hint: match target {
            McpAgentTarget::Cursor => ".cursor/mcp.json (project) or Cursor Settings → MCP",
            McpAgentTarget::Claude => "claude_desktop_config.json (Claude Desktop app support directory)",
        },
        json: serde_json::to_string_pretty(&wrapper).unwrap_or_default(),
    }
}

/// `formatMcpCliCommand`: the CLI launch command, quoted when the
/// executable path contains spaces and not already quoted, `--mcp`
/// appended; an empty/whitespace command falls back to `datazen`.
pub fn format_mcp_cli_command(command: &str) -> String {
    let trimmed = command.trim();
    let trimmed = if trimmed.is_empty() { "datazen" } else { trimmed };
    let has_quotes = (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''));
    let quoted = if trimmed.contains(' ') && !has_quotes {
        format!("\"{trimmed}\"")
    } else {
        trimmed.to_owned()
    };
    format!("{quoted} --mcp")
}

// ---------------------------------------------------------------------------
// The section state
// ---------------------------------------------------------------------------

/// `COPIED_FEEDBACK_MS` (2000) — both copy buttons' revert window, in
/// seconds.
pub const MCP_COPIED_WINDOW: f64 = 2.0;
/// The `setTimeout(() => setSaved(false), 2000)` windows.
pub const MCP_SAVED_WINDOW: f64 = 2.0;
/// The status poll's `setInterval` (3000ms), in seconds.
pub const MCP_POLL_INTERVAL: f64 = 3.0;
/// The audit log's fixed page (`mcpListAuditLog(50)`).
pub const MCP_AUDIT_LIMIT: i64 = 50;

/// What a flow that changed a live server does next —
/// `reloadMcpIfRunning`: reload both statuses only when something is
/// running, else nothing (the pending edits ride the next start).
#[derive(Clone, Debug, PartialEq)]
pub enum ReloadIfRunning {
    /// `!running && !httpStatus?.running` — skip.
    Skip,
    /// `mcpReload()` then refresh both statuses.
    Reload,
}

/// `handleToggleEnabled`'s walk: the settings write, the start/stop
/// request, the follow-up refresh — or the early refusal.
#[derive(Clone, Debug, PartialEq)]
pub enum StdioTogglePlan {
    /// The row's guard: a toggle already in flight (`if (!toggling)`).
    Refused,
    /// Persist `mcpServerEnabled`, run the request, then refresh.
    Apply {
        enabled: bool,
        request: AiRequest,
    },
}

/// `handleToggleHttp`'s walk.
#[derive(Clone, Debug, PartialEq)]
pub enum HttpTogglePlan {
    /// The row's guard (`httpToggling`).
    Refused,
    /// Enabling with an invalid port: the portInvalid error, no request.
    InvalidPort,
    /// Enabling: `mcpHttpEnable(port)`.
    Enable { port: i64 },
    /// Disabling: `mHttpDisable()`.
    Disable,
}

/// `handleApplyHttpPort`'s walk.
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyPortPlan {
    /// Invalid port: the error, no write.
    InvalidPort,
    /// Persist the port; enable the listener when it (or the setting)
    /// says the HTTP server runs, else just refresh.
    Apply { port: i64, reload: ReloadIfRunning },
}

/// The section's state: the settings projection (host-delivered) plus
/// React's `useState` fields.
#[derive(Clone, Default, PartialEq)]
pub struct McpSettingsSectionState {
    /// The settings the section edits (the store's, or the host's
    /// draft when [`Self::draft_override`] is on).
    pub settings: McpSettingsProjection,
    /// React's `onSettingsChange` present: writes go to the host's
    /// draft and the two save buttons stay hidden.
    pub draft_override: bool,
    // ---- host-delivered projections ----
    /// `allTools` (`mcpListAllTools`, mount).
    pub all_tools: Vec<String>,
    /// `connections` (the connectionStore list, mount + host-refreshed).
    pub connections: Vec<McpConnectionVm>,
    // ---- useState ----
    /// `disabledTools`.
    pub disabled_tools: Vec<String>,
    /// `allowedIds`.
    pub allowed_ids: Vec<String>,
    /// `databaseScope` (the textarea's text).
    pub database_scope: TextEdit,
    /// `namespaceScope`.
    pub namespace_scope: TextEdit,
    /// `scopeSaved` — the window's deadline.
    pub scope_saved_until: Option<f64>,
    /// `auditRows`.
    pub audit_rows: Vec<McpAuditRecordVm>,
    /// `auditLoading`.
    pub audit_loading: bool,
    /// `auditError`.
    pub audit_error: Option<String>,
    /// `saved` (the tools block's window).
    pub saved_until: Option<f64>,
    /// `running` (the stdio status poll's latest answer).
    pub running: bool,
    /// `toggling`.
    pub toggling: bool,
    /// `toggleError`.
    pub toggle_error: Option<String>,
    /// `httpStatus` (the HTTP poll's latest answer).
    pub http_status: Option<McpHttpStatusVm>,
    /// `httpPort` (the port input's text).
    pub http_port: TextEdit,
    /// `httpToggling`.
    pub http_toggling: bool,
    /// `httpError`.
    pub http_error: Option<String>,
    /// `httpToken` (the once-only token the enable/regenerate answer
    /// carried; `null` until then, cleared on disable).
    pub http_token: Option<String>,
    /// `agentTarget`.
    pub agent_target: McpAgentTarget,
    /// `useCopyFeedback`'s window (both copy buttons share it).
    pub copied_until: Option<f64>,
}

impl McpSettingsSectionState {
    pub fn new() -> Self {
        Self::default()
    }

    // ---- the mount effects ----

    /// The mount effect's request pair (tools + audit; the connection
    /// fetch and the status polls are host actions — the host also
    /// re-polls every [`MCP_POLL_INTERVAL`]).
    pub fn on_mount(&self) -> [AiRequest; 4] {
        [
            mcp_list_all_tools_request(),
            mcp_list_audit_log_request(Some(MCP_AUDIT_LIMIT)),
            mcp_get_status_request(),
            mcp_http_status_request(),
        ]
    }

    /// The five resync effects (the `[settings.<key>]` deps): the
    /// settings' values replace the local drafts — a typing user keeps
    /// their draft until the HOST delivers a changed projection.
    pub fn resync_from_settings(&mut self) {
        self.disabled_tools = self.settings.mcp_disabled_tools.clone().unwrap_or_default();
        self.allowed_ids = self.settings.mcp_allowed_connection_ids.clone().unwrap_or_default();
        self.database_scope =
            TextEdit::new(self.settings.mcp_allowed_databases.clone().unwrap_or_default().join(", "));
        self.namespace_scope =
            TextEdit::new(self.settings.mcp_allowed_namespaces.clone().unwrap_or_default().join(", "));
        self.http_port = TextEdit::new(
            self.settings.mcp_http_port.unwrap_or(0).to_string(),
        );
    }

    /// `mcpListAllTools`' answer.
    pub fn deliver_tools(&mut self, tools: Vec<String>) {
        self.all_tools = tools;
    }

    /// `loadAudit`'s begin (loading armed, error cleared).
    pub fn begin_load_audit(&mut self) {
        self.audit_loading = true;
        self.audit_error = None;
    }

    /// `loadAudit`'s answers.
    pub fn deliver_audit(&mut self, rows: Vec<McpAuditRecordVm>) {
        self.audit_rows = rows;
        self.audit_loading = false;
    }

    pub fn deliver_audit_error(&mut self, message: String) {
        self.audit_error = Some(message);
        self.audit_loading = false;
    }

    /// `refreshStatus`' answer (a failed poll answers `false`).
    pub fn deliver_status(&mut self, running: bool) {
        self.running = running;
    }

    /// `refreshHttpStatus`' answer (a failed poll answers `None`).
    pub fn deliver_http_status(&mut self, status: Option<McpHttpStatusVm>) {
        self.http_status = status;
    }

    // ---- the stdio toggle ----

    /// `reloadMcpIfRunning`'s predicate.
    pub fn reload_if_running(&self) -> ReloadIfRunning {
        if !self.running && !self.http_status.as_ref().is_some_and(|s| s.running) {
            ReloadIfRunning::Skip
        } else {
            ReloadIfRunning::Reload
        }
    }

    /// `handleToggleEnabled`: the guard, then the write + request plan
    /// (the host persists `mcpServerEnabled`, runs the request, then
    /// `mcpGetStatus`; the answers land through `deliver_status` /
    /// `note_toggle_error`).
    pub fn begin_toggle_enabled(&mut self, enabled: bool) -> StdioTogglePlan {
        if self.toggling {
            return StdioTogglePlan::Refused;
        }
        self.toggling = true;
        self.toggle_error = None;
        let request = if enabled {
            mcp_start_stdio_request()
        } else {
            mcp_stop_request()
        };
        StdioTogglePlan::Apply { enabled, request }
    }

    /// The toggle landed (the `finally`).
    pub fn note_toggle_done(&mut self) {
        self.toggling = false;
    }

    /// The toggle threw: the message surfaces, and a FAILED ENABLE
    /// reverts `mcpServerEnabled` to false (the host persists the
    /// revert — the React catch's second write).
    pub fn note_toggle_error(&mut self, enabled: bool, message: String) {
        self.toggle_error = Some(message);
        self.toggling = false;
        if enabled {
            self.settings.mcp_server_enabled = Some(false);
        }
    }

    // ---- the HTTP listener ----

    /// `parseHttpPort`: `Number(text)`, integer 0..=65535 or `-1`.
    /// JS's `Number('')` is `0` (the random-port sentinel) and
    /// whitespace trims; a non-decimal parse (hex, floats) is `-1`
    /// here where JS's coercion could read `0x10` as 16 — pathological
    /// input, decimal-only (documented delta).
    pub fn parse_http_port(&self) -> i64 {
        let text = self.http_port.text.trim();
        if text.is_empty() {
            return 0;
        }
        match text.parse::<i64>() {
            Ok(value) if (0..=65535).contains(&value) => value,
            _ => -1,
        }
    }

    /// `handleToggleHttp`: the guard, the port rule, the plan (the
    /// host runs the request and answers with the next status through
    /// `note_http_answer` / `note_http_error`).
    pub fn begin_toggle_http(&mut self, enabled: bool) -> HttpTogglePlan {
        if self.http_toggling {
            return HttpTogglePlan::Refused;
        }
        self.http_toggling = true;
        self.http_error = None;
        if enabled {
            let port = self.parse_http_port();
            if port < 0 {
                self.http_error = Some("mcp.http.portInvalid".to_owned());
                self.http_toggling = false;
                return HttpTogglePlan::InvalidPort;
            }
            HttpTogglePlan::Enable { port }
        } else {
            HttpTogglePlan::Disable
        }
    }

    /// `handleApplyHttpPort`.
    pub fn begin_apply_http_port(&mut self) -> ApplyPortPlan {
        let port = self.parse_http_port();
        if port < 0 {
            self.http_error = Some("mcp.http.portInvalid".to_owned());
            return ApplyPortPlan::InvalidPort;
        }
        self.http_toggling = true;
        self.http_error = None;
        let http_runs = self.http_status.as_ref().is_some_and(|s| s.running)
            || self.settings.mcp_http_enabled.unwrap_or(false);
        ApplyPortPlan::Apply {
            port,
            reload: if http_runs {
                ReloadIfRunning::Reload
            } else {
                ReloadIfRunning::Skip
            },
        }
    }

    /// `handleRegenerateHttpToken`: the plan (the host runs the
    /// request; the answer's `token` becomes the once-only display).
    pub fn begin_regenerate_token(&mut self) {
        self.http_toggling = true;
        self.http_error = None;
        self.http_token = None;
    }

    /// The ENABLE answer landed: persist `mcpHttpEnabled: true` +
    /// `mcpHttpPort: configuredPort`, show the once-only token when
    /// the answer carried one, replace the status.
    pub fn note_http_enabled(&mut self, status: McpHttpStatusVm, token: Option<String>) {
        self.settings.mcp_http_enabled = Some(true);
        self.settings.mcp_http_port = Some(status.configured_port);
        if let Some(token) = token {
            self.http_token = Some(token);
        }
        self.http_status = Some(status);
        self.http_toggling = false;
    }

    /// The APPLY-PORT answer landed: the REQUESTED port persists (the
    /// React write is `{ mcpHttpPort: port }`, not the answer's), and
    /// the status/token land only when the re-enable ran (the
    /// refresh-only path answers through `deliver_http_status`).
    pub fn note_http_port_applied(
        &mut self,
        requested_port: i64,
        status: Option<McpHttpStatusVm>,
        token: Option<String>,
    ) {
        self.settings.mcp_http_port = Some(requested_port);
        if let Some(status) = status {
            self.http_status = Some(status);
            if let Some(token) = token {
                self.http_token = Some(token);
            }
        }
        self.http_toggling = false;
    }

    /// The REGENERATE answer landed: no settings write — the status
    /// replaces and the once-only token shows when the answer carried
    /// one (`next.token ?? null`).
    pub fn note_http_regenerated(&mut self, status: McpHttpStatusVm, token: Option<String>) {
        if let Some(token) = token {
            self.http_token = Some(token);
        }
        self.http_status = Some(status);
        self.http_toggling = false;
    }

    /// The disable answer landed: the token clears and the setting
    /// turns off.
    pub fn note_http_disabled(&mut self, status: McpHttpStatusVm) {
        self.http_token = None;
        self.settings.mcp_http_enabled = Some(false);
        self.http_status = Some(status);
        self.http_toggling = false;
    }

    /// A failed apply/regenerate/toggle (the message surfaces; the
    /// `finally` disarms).
    pub fn note_http_error(&mut self, message: String) {
        self.http_error = Some(message);
        self.http_toggling = false;
    }

    // ---- permission / allowlist / tools / scope ----

    /// `handlePermissionChange`: persist the mode, then reload if
    /// something runs.
    pub fn begin_permission_change(&mut self, mode: McpPermissionMode) -> ReloadIfRunning {
        self.settings.mcp_permission_mode = Some(mode);
        self.reload_if_running()
    }

    /// `toggleTool`: membership flip + the immediate
    /// `mcpDisabledTools` write.
    pub fn toggle_tool(&mut self, tool: &str) -> Vec<String> {
        if self.disabled_tools.iter().any(|n| n == tool) {
            self.disabled_tools.retain(|n| n != tool);
        } else {
            self.disabled_tools.push(tool.to_owned());
        }
        self.disabled_tools.clone()
    }

    /// `toggleAllowed`.
    pub fn toggle_allowed(&mut self, id: &str) -> Vec<String> {
        if self.allowed_ids.iter().any(|x| x == id) {
            self.allowed_ids.retain(|x| x != id);
        } else {
            self.allowed_ids.push(id.to_owned());
        }
        self.allowed_ids.clone()
    }

    /// `handleEnableAll` / `handleDisableAll`.
    pub fn set_all_tools_enabled(&mut self, enabled: bool) -> Vec<String> {
        self.disabled_tools = if enabled { Vec::new() } else { self.all_tools.clone() };
        self.disabled_tools.clone()
    }

    /// `handleSaveTools`: both writes + the reload + the 2s saved
    /// window.
    pub fn begin_save_tools(&mut self, now: f64) -> ReloadIfRunning {
        self.saved_until = Some(now + MCP_SAVED_WINDOW);
        self.reload_if_running()
    }

    /// `parseScope`: split on newlines/commas, trim, drop empties,
    /// dedup keeping FIRST-SEEN order (`Array.from(new Set(...))`).
    pub fn parse_scope(text: &str) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for entry in text.split([',', '\n']) {
            let entry = entry.trim();
            if !entry.is_empty() && !seen.iter().any(|s| s == entry) {
                seen.push(entry.to_owned());
            }
        }
        seen
    }

    /// `scopeDirty`: the parsed scopes vs the settings' lists,
    /// ORDER-INSENSITIVE (both sides sorted — the React
    /// `JSON.stringify(...sort())` comparison).
    pub fn scope_dirty(&self) -> bool {
        let sorted_eq = |a: &[String], b: &[String]| {
            let mut a = a.to_vec();
            let mut b = b.to_vec();
            a.sort();
            b.sort();
            a != b
        };
        sorted_eq(
            &Self::parse_scope(&self.database_scope.text),
            self.settings.mcp_allowed_databases.as_deref().unwrap_or(&[]),
        ) || sorted_eq(
            &Self::parse_scope(&self.namespace_scope.text),
            self.settings.mcp_allowed_namespaces.as_deref().unwrap_or(&[]),
        )
    }

    /// `handleSaveScope`'s writes (the parsed scopes) — the host
    /// persists, reloads per the plan, and the saved window arms.
    pub fn begin_save_scope(&mut self, now: f64) -> (Vec<String>, Vec<String>, ReloadIfRunning) {
        self.scope_saved_until = Some(now + MCP_SAVED_WINDOW);
        (
            Self::parse_scope(&self.database_scope.text),
            Self::parse_scope(&self.namespace_scope.text),
            self.reload_if_running(),
        )
    }

    /// `toolsDirty`: the local lists vs the settings', sorted.
    pub fn tools_dirty(&self) -> bool {
        let sorted_eq = |a: &[String], b: &[String]| {
            let mut a = a.to_vec();
            let mut b = b.to_vec();
            a.sort();
            b.sort();
            a != b
        };
        sorted_eq(&self.disabled_tools, self.settings.mcp_disabled_tools.as_deref().unwrap_or(&[]))
            || sorted_eq(
                &self.allowed_ids,
                self.settings.mcp_allowed_connection_ids.as_deref().unwrap_or(&[]),
            )
    }

    // ---- the copy feedback and the snippet ----

    /// `useCopyFeedback`'s `copy()`: the clipboard write is the host's
    /// (`tx.copy`); the window arms for [`MCP_COPIED_WINDOW`].
    pub fn note_copied(&mut self, now: f64) {
        self.copied_until = Some(now + MCP_COPIED_WINDOW);
    }

    /// `copied` — either copy button's label flips while `now` is
    /// behind the deadline.
    pub fn copied(&self, now: f64) -> bool {
        self.copied_until.is_some_and(|until| now < until)
    }

    /// The scope block's saved line.
    pub fn scope_saved(&self, now: f64) -> bool {
        self.scope_saved_until.is_some_and(|until| now < until)
    }

    /// The tools block's saved line.
    pub fn tools_saved(&self, now: f64) -> bool {
        self.saved_until.is_some_and(|until| now < until)
    }
}

/// The pure reload request for a `Reload` plan (the host runs it, then
/// re-polls both statuses).
pub fn mcp_reload_request_for_plan() -> AiRequest {
    mcp_reload_request()
}

/// The enable request a plan carries.
pub fn mcp_http_enable_request_for(port: i64) -> AiRequest {
    mcp_http_enable_request(Some(port))
}

/// The disable request a plan carries.
pub fn mcp_http_disable_request_for() -> AiRequest {
    mcp_http_disable_request()
}

/// The regenerate request.
pub fn mcp_http_regenerate_token_request_for() -> AiRequest {
    mcp_http_regenerate_token_request()
}
