//! Port of `packages/frontend/src/windows/settings/McpSettingsSection.tsx`
//! (T348, split) — Settings → AI → MCP Server.
//!
//! The MCP host service's administration surface: the stdio server's
//! enable toggle with its live status dot, the Streamable HTTP
//! listener block (enable, the port field with its apply, the running
//! status + bound port, the once-only token with regenerate/copy),
//! the three permission modes, the connection allowlist, the
//! database/namespace scope textareas, the Cursor/Claude agent config
//! snippet with copy, the exposed-tools list with per-tool switches
//! and enable/disable-all, and the audit log table.
//!
//! ## What maps where
//!
//! - `useLocaleDomains(['mcp'])` is elided, the T018 ruling (the
//!   zh-CN `mcp.ts` pack is compiled in by `build.rs`).
//! - The settings store and `onSettingsChange` draft-override mode:
//!   the projection lands in
//!   [`McpSettingsSectionState::settings`] (kept current by the host);
//!   [`McpSettingsSectionState::draft_override`] is React's
//!   `onSettingsChange` present — the two save buttons hide and every
//!   write routes to the host slot instead.
//! - Every `aiCommands.*` call is a host action slot (the
//!   `language_packs_card` convention); the mount effect's request
//!   quartet and the 3s poll are
//!   [`McpSettingsSectionState::on_mount`] + [`MCP_POLL_INTERVAL`]
//!   (the host wakes and re-delivers).
//! - The five `[settings.<key>]` resync effects are
//!   [`McpSettingsSectionState::resync_from_settings`] (the host calls
//!   it on every settings delivery).
//! - `handleToggleEnabled` / `handleToggleHttp` / `handleApplyHttpPort`
//!   / `handleRegenerateHttpToken` are the `begin_*` plan methods +
//!   `note_*` answers in the state module; `reloadMcpIfRunning`'s
//!   predicate is [`McpSettingsSectionState::reload_if_running`].
//! - `parseScope` / `scopeDirty` / `toolsDirty` / `parseHttpPort` are
//!   the state module's pure rules.
//! - `useCopyFeedback(2000)` is the `copied_until` deadline
//!   ([`MCP_COPIED_WINDOW`]); the clipboard write is the host's
//!   `on_copy_*` slot.
//! - `lib/mcpAgentConfig.ts` (segment-2's T557) is carried in the
//!   state module ([`build_mcp_agent_snippet`] and friends, the
//!   `lib_helpers` pattern); `useAppExecutablePath` is the host seam
//!   (the snippet takes the resolved path as a build argument).
//! - `SectionTitle` / `SettingRow` / `ToggleRow` are the
//!   `settingsUi.tsx` port; the hand-drawn switches (the HTTP enable
//!   checkbox, the per-tool switches) are the kit [`Switch`]; the
//!   permission radios and the allowlist checkboxes are the kit
//!   [`Checkbox`] (the kit has no radio atom — a square where React
//!   drew a circle, the visible delta); the audit table is
//!   [`crate::ui::table::DataTable`].
//! - React `Button` variants map per T022: `secondary` → `Outline`,
//!   `primary` → `Submit`.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: `mcp-http-toggle`,
//! `mcp-http-port`, `mcp-http-apply-port`, `mcp-http-regenerate`,
//! `mcp-http-token`, `mcp-http-copy`. Everything else derives from the
//! section key base: the stdio toggle `mcp-stdio-toggle`, the status
//! line `mcp-stdio-status`, the HTTP block `mcp-http-block` (status
//! line `mcp-http-status`, actual-port `mcp-http-actual-port`, token
//! card `mcp-http-token-card`, token-once line `mcp-http-token-once`),
//! one permission card `mcp-permission-{value}`, the allowlist
//! `mcp-allowlist` (one row `mcp-allowlist-{id}`), the scope block
//! `mcp-scope` (databases `mcp-scope-databases`, namespaces
//! `mcp-scope-namespaces`, save `mcp-scope-save`, saved line
//! `mcp-scope-saved`), the snippet card `mcp-agent-snippet` (tab
//! `mcp-agent-tab-{target}`, copy `mcp-agent-copy`, path hint
//! `mcp-agent-path`, the JSON `mcp-agent-json`), the tools block
//! `mcp-tools` (heading `mcp-tools-title`, enable-all
//! `mcp-tools-enable-all`, disable-all `mcp-tools-disable-all`, one
//! row `mcp-tool-{name}`, save `mcp-tools-save`, saved line
//! `mcp-tools-saved`), and the audit block `mcp-audit` (refresh
//! `mcp-audit-refresh`, table `mcp-audit-table`).
//!
//! Known deltas (documented, not silent): the two scope textareas and
//! the port field render as the kit's single-line [`TextField`]
//! (values and parses keep the newline form); the permission radios
//! render as checkboxes; `Number()`'s hex coercion reads `-1` here
//! (decimal only — see the state module's `parse_http_port`); the
//! audit table's column widths are the atom's defaults (React's
//! `text-xs` table has no widths either).

// No production consumer yet (the settings view wiring track mounts
// the section); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, State};

use super::mcp_settings_section_state::{
    McpAgentTarget, McpAuditRecordVm, McpPermissionMode, McpSettingsSectionState,
    build_mcp_agent_snippet,
};
use crate::hooks::use_i18n::UseI18n;
use crate::i18n::t;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::table::{CellValue, Column, DataTable, Row};
use crate::ui::toggle::{Checkbox, Switch};
use crate::views::settings::setting_hint::SettingHint;
use crate::views::settings::settings_ui::{section_title, setting_row, toggle_row, ToggleRow as ToggleRowSpec};

/// The status dot's size.
const DOT: f32 = 8.0;
/// React `text-xs` / `text-sm` / `text-[11px]`… the settingsUi sizes.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);
const TEXT_SM: TextStyle = TextStyle::new(14.0, 400.0);
const TEXT_SM_MEDIUM: TextStyle = TextStyle::new(14.0, 500.0);

/// `McpSettingsSection()`: the section over the state and the host's
/// action slots.
pub(crate) struct McpSettingsSection<'a, S: 'static> {
    state: &'a McpSettingsSectionState,
    dark: bool,
    /// The host clock the saved/copied windows read (the host passes
    /// `tx.clock()` at build time).
    now: f64,
    /// The resolved executable path (`useAppExecutablePath` — the host
    /// seam; `datazen` until it answers).
    app_executable_path: String,
    // the three text inputs' lenses
    database_scope_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    namespace_scope_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    http_port_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    // host actions
    on_toggle_enabled: Option<Rc<dyn Fn(&mut S, bool)>>,
    on_toggle_http: Option<Rc<dyn Fn(&mut S, bool)>>,
    on_apply_port: Option<Rc<dyn Fn(&mut S)>>,
    on_regenerate_token: Option<Rc<dyn Fn(&mut S)>>,
    on_permission_change: Option<Rc<dyn Fn(&mut S, McpPermissionMode)>>,
    on_toggle_tool: Option<Rc<dyn Fn(&mut S, String)>>,
    on_toggle_allowed: Option<Rc<dyn Fn(&mut S, String)>>,
    on_enable_all: Option<Rc<dyn Fn(&mut S)>>,
    on_disable_all: Option<Rc<dyn Fn(&mut S)>>,
    on_save_tools: Option<Rc<dyn Fn(&mut S)>>,
    on_save_scope: Option<Rc<dyn Fn(&mut S)>>,
    on_copy_snippet: Option<Rc<dyn Fn(&mut S)>>,
    on_copy_token: Option<Rc<dyn Fn(&mut S)>>,
    on_set_agent_target: Option<Rc<dyn Fn(&mut S, McpAgentTarget)>>,
    on_refresh_audit: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> McpSettingsSection<'a, S> {
    /// The section over the state, the clock, the executable path and
    /// the three text-input lenses.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        state: &'a McpSettingsSectionState,
        dark: bool,
        now: f64,
        app_executable_path: impl Into<String>,
        database_scope_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        namespace_scope_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
        http_port_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        Self {
            state,
            dark,
            now,
            app_executable_path: app_executable_path.into(),
            database_scope_edit: Rc::new(database_scope_edit),
            namespace_scope_edit: Rc::new(namespace_scope_edit),
            http_port_edit: Rc::new(http_port_edit),
            on_toggle_enabled: None,
            on_toggle_http: None,
            on_apply_port: None,
            on_regenerate_token: None,
            on_permission_change: None,
            on_toggle_tool: None,
            on_toggle_allowed: None,
            on_enable_all: None,
            on_disable_all: None,
            on_save_tools: None,
            on_save_scope: None,
            on_copy_snippet: None,
            on_copy_token: None,
            on_set_agent_target: None,
            on_refresh_audit: None,
        }
    }

    /// `handleToggleEnabled(next)` — the stdio toggle row.
    pub(crate) fn on_toggle_enabled(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_toggle_enabled = Some(Rc::new(f));
        self
    }
    /// `handleToggleHttp(next)` — the HTTP enable checkbox.
    pub(crate) fn on_toggle_http(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_toggle_http = Some(Rc::new(f));
        self
    }
    /// `handleApplyHttpPort`.
    pub(crate) fn on_apply_port(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_apply_port = Some(Rc::new(f));
        self
    }
    /// `handleRegenerateHttpToken`.
    pub(crate) fn on_regenerate_token(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_regenerate_token = Some(Rc::new(f));
        self
    }
    /// `handlePermissionChange(mode)`.
    pub(crate) fn on_permission_change(
        mut self,
        f: impl Fn(&mut S, McpPermissionMode) + 'static,
    ) -> Self {
        self.on_permission_change = Some(Rc::new(f));
        self
    }
    /// `toggleTool(name)`.
    pub(crate) fn on_toggle_tool(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_tool = Some(Rc::new(f));
        self
    }
    /// `toggleAllowed(id)`.
    pub(crate) fn on_toggle_allowed(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_allowed = Some(Rc::new(f));
        self
    }
    /// `handleEnableAll`.
    pub(crate) fn on_enable_all(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_enable_all = Some(Rc::new(f));
        self
    }
    /// `handleDisableAll`.
    pub(crate) fn on_disable_all(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_disable_all = Some(Rc::new(f));
        self
    }
    /// `handleSaveTools`.
    pub(crate) fn on_save_tools(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_save_tools = Some(Rc::new(f));
        self
    }
    /// `handleSaveScope`.
    pub(crate) fn on_save_scope(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_save_scope = Some(Rc::new(f));
        self
    }
    /// `handleCopySnippet` (the host also writes the clipboard).
    pub(crate) fn on_copy_snippet(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_copy_snippet = Some(Rc::new(f));
        self
    }
    /// The token's copy button.
    pub(crate) fn on_copy_token(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_copy_token = Some(Rc::new(f));
        self
    }
    /// `setAgentTarget(target)`.
    pub(crate) fn on_set_agent_target(
        mut self,
        f: impl Fn(&mut S, McpAgentTarget) + 'static,
    ) -> Self {
        self.on_set_agent_target = Some(Rc::new(f));
        self
    }
    /// `loadAudit` (the refresh link).
    pub(crate) fn on_refresh_audit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_refresh_audit = Some(Rc::new(f));
        self
    }

    /// The section body.
    pub(crate) fn build(self, layer: &mut Vec<El>) -> El {
        let dark = self.dark;
        let k = kit(dark);
        let state = self.state;
        let now = self.now;

        let mut body = col().key("mcp-settings-section").w_full().gap(16.0);

        body = body.child(section_title(
            &k,
            layer,
            "mcp-settings-title".to_owned(),
            t("mcp.title"),
            Some(t("mcp.description")),
        ));

        // The stdio enable toggle (the guard refuses while toggling).
        body = body.child(toggle_row::<S>(
            &k,
            layer,
            ToggleRowSpec {
                key: "mcp-stdio-toggle".to_owned(),
                label: t("mcp.enabled"),
                hint: Some(t("mcp.enabledHint")),
                checked: state.settings.mcp_server_enabled.unwrap_or(false),
                disabled: state.toggling,
            },
            self.on_toggle_enabled.clone().unwrap_or_else(|| Rc::new(|_s: &mut S, _v: bool| {})),
        ));

        // The status line.
        body = body.child(setting_row(
            &k,
            layer,
            "mcp-stdio-status-row".to_owned(),
            t("mcp.status"),
            None,
            status_dot(&k, state.running, if state.running { t("mcp.running") } else { t("mcp.stopped") }, "mcp-stdio-status"),
        ));

        if let Some(message) = &state.toggle_error {
            body = body.child(error_line(&k, "mcp-toggle-error", message));
        }

        // The HTTP listener block.
        body = body.child(self.http_block(&k, layer));

        // The permission modes.
        body = body.child(self.permission_block(&k, layer));

        // The connection allowlist.
        body = body.child(self.allowlist_block(&k, layer));

        // The scope block.
        body = body.child(self.scope_block(&k, layer, dark));

        // The agent snippet card.
        body = body.child(self.snippet_card(&k, layer, now));

        // The tools list (and its no-tools dirty fallback).
        if !state.all_tools.is_empty() {
            body = body.child(self.tools_block(&k, layer, now));
        } else if state.tools_dirty() {
            body = body.child(self.tools_save_row(&k, now));
        }

        // The audit log.
        body = body.child(self.audit_block(&k, layer, dark));

        body
    }

    /// The Streamable HTTP listener's setting row.
    fn http_block(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let dark = self.dark;

        let mut block = col().key("mcp-http-block").w_full().gap(12.0);

        // The enable card: label + hint left, the switch right.
        let mut card = row()
            .key("mcp-http-enable-card")
            .w_full()
            .px(12.0)
            .py(8.0)
            .gap(12.0)
            .center_y()
            .justify_between()
            .radius(k.radius.md)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base);
        card = card.child(
            col().key("mcp-http-enable-meta").min_w(0.0).gap(2.0)
                .child(text(t("mcp.http.enable")).font(TEXT_SM).color(k.c.text_base))
                .child(text(t("mcp.http.enableHint")).font(TEXT_XS).color(k.c.text_muted)),
        );
        let http_checked = state
            .http_status
            .as_ref()
            .map(|s| s.enabled)
            .unwrap_or_else(|| state.settings.mcp_http_enabled.unwrap_or(false));
        let mut switch = Switch::new("mcp-http-toggle", http_checked)
            .label(t("mcp.http.enable"))
            .hide_label();
        if state.http_toggling {
            switch = switch.disabled(true);
        } else if let Some(f) = &self.on_toggle_http {
            let f = Rc::clone(f);
            let next = !http_checked;
            switch = switch.on_toggle(move |s: &mut S| f(s, next));
        }
        card = card.child(switch.build(k));
        block = block.child(card);

        // The port row: label + field + apply + hint.
        let port_lens = Rc::clone(&self.http_port_edit);
        let port_input = TextField::new("mcp-http-port", &state.http_port)
            .width(128.0)
            .placeholder("0")
            .on_edit::<S>(move |s: &mut S| port_lens(s));
        let port_field_col = col().key("mcp-http-port-wrap").gap(4.0)
            .child(text(t("mcp.http.port")).font(TEXT_XS).color(k.c.text_muted))
            .child(port_input.build(dark));
        let mut apply = Button::new(t("mcp.http.applyPort"))
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Regular);
        if state.http_toggling {
            apply = apply.state(State::Disabled).disabled(true);
        } else if let Some(f) = &self.on_apply_port {
            let f = Rc::clone(f);
            apply = apply.on_press(move |s: &mut S| f(s));
        }
        let port_row = row().key("mcp-http-port-row").gap(8.0).items_end()
            .child(port_field_col)
            .child(apply.build(k).key("mcp-http-apply-port"))
            .child(text(t("mcp.http.portHint")).font(TEXT_XS).color(k.c.text_muted));
        block = block.child(port_row);

        // The status line: dot + running/stopped + the URL.
        let http_running = state.http_status.as_ref().is_some_and(|s| s.running);
        let mut status_line = row().key("mcp-http-status").gap(8.0).center_y()
            .child(status_dot(
                k,
                http_running,
                if http_running { t("mcp.http.running") } else { t("mcp.http.stopped") },
                "mcp-http-status-dot",
            ));
        if let Some(url) = state.http_status.as_ref().and_then(|s| s.url.as_deref()) {
            status_line = status_line.child(
                text(url.to_owned()).font(k.ty.mono).color(k.c.text_base),
            );
        }
        block = block.child(status_line);
        if http_running {
            if let Some(port) = state.http_status.as_ref().and_then(|s| s.port) {
                let i18n = UseI18n::new();
                block = block.child(
                    text(i18n.t_with_params("mcp.http.actualPort", &[("port", &port.to_string())]))
                        .key("mcp-http-actual-port")
                        .font(TEXT_XS)
                        .color(k.c.text_muted),
                );
            }
        }

        // The token card.
        let mut token_card = col()
            .key("mcp-http-token-card")
            .w_full()
            .padding(12.0)
            .gap(8.0)
            .radius(k.radius.md)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base);
        let token_head = row().key("mcp-http-token-head").w_full().gap(8.0).center_y().justify_between()
            .child(
                text(
                    state
                        .http_status
                        .as_ref()
                        .and_then(|s| s.token_last4.as_deref())
                        .map(|last4| {
                            let i18n = UseI18n::new();
                            i18n.t_with_params("mcp.http.tokenLast4", &[("last4", last4)])
                        })
                        .unwrap_or_else(|| t("mcp.http.noToken").to_owned()),
                )
                .font(TEXT_XS)
                .color(k.c.text_muted),
            );
        let regenerate_label = if state.http_status.as_ref().is_some_and(|s| s.token_configured) {
            t("mcp.http.regenerateToken")
        } else {
            t("mcp.http.generateToken")
        };
        let mut regenerate = Button::new(regenerate_label)
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Regular);
        if state.http_toggling {
            regenerate = regenerate.state(State::Disabled).disabled(true);
        } else if let Some(f) = &self.on_regenerate_token {
            let f = Rc::clone(f);
            regenerate = regenerate.on_press(move |s: &mut S| f(s));
        }
        token_card = token_card.child(token_head.child(regenerate.build(k).key("mcp-http-regenerate")));

        if let Some(token) = state.http_token.as_deref() {
            token_card = token_card.child(
                text(t("mcp.http.tokenOnce"))
                    .key("mcp-http-token-once")
                    .font(TextStyle { weight: 500.0, ..TEXT_XS })
                    .color(k.c.warning.fg),
            );
            // The read-only token display: a plain TextEdit the field
            // renders unfocusable (React's readOnly input).
            let token_edit = TextEdit::new(token.to_owned());
            let token_field = TextField::new("mcp-http-token", &token_edit).disabled(true);
            let mut copy = Button::new(if state.copied(self.now) { t("mcp.http.copied") } else { t("mcp.http.copyToken") })
                .variant(ButtonVariant::Outline) // React secondary
                .control_size(ControlSize::Regular);
            if let Some(f) = &self.on_copy_token {
                let f = Rc::clone(f);
                copy = copy.on_press(move |s: &mut S| f(s));
            }
            token_card = token_card.child(
                row().key("mcp-http-token-row").gap(8.0)
                    .child(token_field.build(dark).grow(1.0).min_w(0.0))
                    .child(copy.build(k).key("mcp-http-copy")),
            );
        }
        block = block.child(token_card);

        if let Some(message) = &state.http_error {
            block = block.child(error_line(k, "mcp-http-error", message));
        }

        setting_row(
            k,
            layer,
            "mcp-http-row".to_owned(),
            t("mcp.http.title"),
            Some(t("mcp.http.description")),
            block,
        )
    }

    /// The three permission mode cards.
    fn permission_block(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let selected_mode = state.settings.mcp_permission_mode.unwrap_or_default();
        let mut list = col().key("mcp-permission-list").w_full().gap(8.0);
        for mode in McpPermissionMode::ALL {
            let selected = selected_mode == mode;
            let (label_key, hint_key) = mode_labels(mode);
            let mut card = row()
                .key(format!("mcp-permission-{}", mode.as_str()))
                .w_full()
                .px(12.0)
                .py(8.0)
                .gap(12.0)
                .center_y()
                .radius(k.radius.md)
                .border(
                    1.0,
                    if selected { k.c.border_focus } else { k.c.border_base },
                )
                .fill(if selected { k.alpha(k.c.border_focus, 0.05) } else { k.c.surface_base });
            let mut radio = Checkbox::new(format!("mcp-permission-radio-{}", mode.as_str()), selected)
                .label(t(label_key));
            if let Some(f) = &self.on_permission_change {
                let f = Rc::clone(f);
                radio = radio.on_toggle(move |s: &mut S| f(s, mode));
            }
            card = card.child(radio.build(k).grow(1.0).min_w(0.0));
            card = card.child(
                SettingHint::new(
                    format!("mcp-permission-hint-{}", mode.as_str()),
                    t(label_key).to_owned(),
                    t(hint_key).to_owned(),
                )
                .build(k, layer),
            );
            list = list.child(card);
        }
        setting_row(
            k,
            layer,
            "mcp-permission-row".to_owned(),
            t("mcp.permission.title"),
            Some(t("mcp.permission.applyHint")),
            list,
        )
    }

    /// The connection allowlist.
    fn allowlist_block(&self, k: &Kit, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let content = if state.connections.is_empty() {
            text(t("mcp.allowlist.empty")).font(TEXT_XS).color(k.c.text_muted)
        } else {
            let mut list = col().key("mcp-allowlist").w_full().gap(4.0);
            for connection in &state.connections {
                let checked = state.allowed_ids.iter().any(|id| id == &connection.id);
                let mut checkbox =
                    Checkbox::new(format!("mcp-allowlist-{}", connection.id), checked)
                        .label(connection.name.clone());
                if let Some(f) = &self.on_toggle_allowed {
                    let f = Rc::clone(f);
                    let id = connection.id.clone();
                    checkbox = checkbox.on_toggle(move |s: &mut S| f(s, id.clone()));
                }
                let short_id: String = connection.id.chars().take(8).collect();
                list = list.child(
                    row().key(format!("mcp-allowlist-row-{}", connection.id))
                        .w_full()
                        .px(12.0)
                        .py(8.0)
                        .gap(8.0)
                        .center_y()
                        .radius(k.radius.md)
                        .border(1.0, k.c.border_base)
                        .fill(k.c.surface_base)
                        .child(checkbox.build(k).grow(1.0).min_w(0.0))
                        .child(text(short_id).font(TextStyle { size: 12.0, ..k.ty.mono }).color(k.c.text_muted)),
                );
            }
            list
        };
        setting_row(
            k,
            layer,
            "mcp-allowlist-row".to_owned(),
            t("mcp.allowlist.title"),
            Some(&format!("{} {}", t("mcp.allowlist.description"), t("mcp.allowlist.applyHint"))),
            content,
        )
    }

    /// The database/namespace scope block.
    fn scope_block(&self, k: &Kit, layer: &mut Vec<El>, dark: bool) -> El {
        let state = self.state;

        let db_lens = Rc::clone(&self.database_scope_edit);
        let db_input = TextField::new("mcp-scope-databases", &state.database_scope)
            .w_full()
            .placeholder(t("mcp.scope.placeholder"))
            .on_edit::<S>(move |s: &mut S| db_lens(s));
        let ns_lens = Rc::clone(&self.namespace_scope_edit);
        let ns_input = TextField::new("mcp-scope-namespaces", &state.namespace_scope)
            .w_full()
            .placeholder(t("mcp.scope.placeholder"))
            .on_edit::<S>(move |s: &mut S| ns_lens(s));

        let mut footer = row().key("mcp-scope-footer").gap(12.0).center_y();
        if state.scope_saved(self.now) {
            footer = footer.child(
                text(t("mcp.scope.saved")).key("mcp-scope-saved").font(TEXT_XS).color(k.c.success.fg),
            );
        }
        if !state.draft_override {
            let mut save = Button::new(t("mcp.scope.save"))
                .variant(ButtonVariant::Submit) // React primary
                .control_size(ControlSize::Regular);
            if !state.scope_dirty() {
                save = save.state(State::Disabled).disabled(true);
            } else if let Some(f) = &self.on_save_scope {
                let f = Rc::clone(f);
                save = save.on_press(move |s: &mut S| f(s));
            }
            footer = footer.child(save.build(k).key("mcp-scope-save"));
        }

        let content = col().key("mcp-scope").w_full().gap(12.0)
            .child(
                col().key("mcp-scope-databases-wrap").gap(4.0)
                    .child(text(t("mcp.scope.databases")).font(TEXT_XS).color(k.c.text_muted))
                    .child(db_input.build(dark)),
            )
            .child(
                col().key("mcp-scope-namespaces-wrap").gap(4.0)
                    .child(text(t("mcp.scope.namespaces")).font(TEXT_XS).color(k.c.text_muted))
                    .child(ns_input.build(dark)),
            )
            .child(footer);

        setting_row(
            k,
            layer,
            "mcp-scope-row".to_owned(),
            t("mcp.scope.title"),
            Some(&format!("{} {}", t("mcp.scope.description"), t("mcp.scope.hint"))),
            content,
        )
    }

    /// The Cursor/Claude agent config snippet card.
    fn snippet_card(&self, k: &Kit, layer: &mut Vec<El>, now: f64) -> El {
        let state = self.state;
        let snippet = build_mcp_agent_snippet(state.agent_target, &self.app_executable_path);

        let mut card = col()
            .key("mcp-agent-snippet")
            .w_full()
            .padding(12.0)
            .gap(8.0)
            .radius(k.radius.md)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_base);

        let mut head = row().key("mcp-agent-head").gap(8.0).center_y();
        for target in [McpAgentTarget::Cursor, McpAgentTarget::Claude] {
            let selected = state.agent_target == target;
            let label = match target {
                McpAgentTarget::Cursor => t("mcp.config.cursor"),
                McpAgentTarget::Claude => t("mcp.config.claude"),
            };
            let mut tab = Button::new(label)
                .variant(if selected { ButtonVariant::Submit } else { ButtonVariant::Ghost })
                .control_size(ControlSize::Small);
            if !selected {
                if let Some(f) = &self.on_set_agent_target {
                    let f = Rc::clone(f);
                    tab = tab.on_press(move |s: &mut S| f(s, target));
                }
            }
            head = head.child(tab.build(k).key(format!("mcp-agent-tab-{}", target_key(target))));
        }
        let tab_label = match state.agent_target {
            McpAgentTarget::Cursor => t("mcp.config.cursor"),
            McpAgentTarget::Claude => t("mcp.config.claude"),
        };
        head = head.child(
            SettingHint::new(
                format!("mcp-agent-hint-{}", target_key(state.agent_target)),
                tab_label.to_owned(),
                format!("{} {}", t("mcp.usage"), t("mcp.config.commandHint")),
            )
            .build(k, layer),
        );
        let mut copy = Button::new(if state.copied(now) { t("mcp.config.copied") } else { t("mcp.config.copy") })
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small);
        if let Some(f) = &self.on_copy_snippet {
            let f = Rc::clone(f);
            copy = copy.on_press(move |s: &mut S| f(s));
        }
        head = head.child(copy.build(k).key("mcp-agent-copy"));
        card = card.child(head);

        let i18n = UseI18n::new();
        card = card.child(
            text(i18n.t_with_params("mcp.config.pathHint", &[("path", snippet.config_path_hint)]))
                .key("mcp-agent-path")
                .font(TEXT_XS)
                .color(k.c.text_muted),
        );
        card = card.child(
            text(snippet.json.clone())
                .key("mcp-agent-json")
                .font(k.ty.mono)
                .color(k.c.text_muted),
        );
        card
    }

    /// The exposed-tools list.
    fn tools_block(&self, k: &Kit, layer: &mut Vec<El>, now: f64) -> El {
        let state = self.state;

        let mut block = col().key("mcp-tools").w_full().gap(8.0);

        let mut head = row().key("mcp-tools-title").w_full().gap(8.0).center_y().justify_between();
        let title = row().gap(4.0).center_y()
            .child(text(t("mcp.tools")).font(TEXT_SM_MEDIUM).color(k.c.text_base))
            .child(
                SettingHint::new(
                    "mcp-tools-hint".to_owned(),
                    t("mcp.tools").to_owned(),
                    format!("{} {}", t("mcp.tools.description"), t("mcp.tools.applyHint")),
                )
                .build(k, layer),
            );
        let mut actions = row().gap(8.0).center_y();
        let mut enable_all = Button::new(t("mcp.tools.enableAll"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small);
        if let Some(f) = &self.on_enable_all {
            let f = Rc::clone(f);
            enable_all = enable_all.on_press(move |s: &mut S| f(s));
        }
        let mut disable_all = Button::new(t("mcp.tools.disableAll"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small);
        if let Some(f) = &self.on_disable_all {
            let f = Rc::clone(f);
            disable_all = disable_all.on_press(move |s: &mut S| f(s));
        }
        actions = actions
            .child(enable_all.build(k).key("mcp-tools-enable-all"))
            .child(text("|").font(TEXT_XS).color(k.c.text_muted))
            .child(disable_all.build(k).key("mcp-tools-disable-all"));
        head = head.child(title).child(actions);
        block = block.child(head);

        let mut list = col().key("mcp-tools-list").w_full().gap(4.0);
        for tool in &state.all_tools {
            let enabled = !state.disabled_tools.iter().any(|n| n == tool);
            let mut switch = Switch::new(format!("mcp-tool-{tool}"), enabled).label(tool.clone()).hide_label();
            if let Some(f) = &self.on_toggle_tool {
                let f = Rc::clone(f);
                let tool = tool.clone();
                switch = switch.on_toggle(move |s: &mut S| f(s, tool.clone()));
            }
            list = list.child(
                row().key(format!("mcp-tool-row-{tool}"))
                    .w_full()
                    .px(12.0)
                    .py(8.0)
                    .gap(8.0)
                    .center_y()
                    .justify_between()
                    .radius(k.radius.md)
                    .border(1.0, k.c.border_base)
                    .fill(k.c.surface_base)
                    .child(text(tool.clone()).font(TextStyle { size: 14.0, ..k.ty.mono }).color(k.c.text_base))
                    .child(switch.build(k)),
            );
        }
        block = block.child(list);
        block = block.child(self.tools_save_row(k, now));
        block
    }

    /// The tools block's saved line + save button (also the no-tools
    /// dirty fallback, verbatim: no disabled rule there).
    fn tools_save_row(&self, k: &Kit, now: f64) -> El {
        let state = self.state;
        let mut footer = row().key("mcp-tools-footer").gap(12.0).center_y();
        if !state.draft_override && state.tools_saved(now) {
            footer = footer.child(
                text(t("mcp.saved")).key("mcp-tools-saved").font(TEXT_XS).color(k.c.success.fg),
            );
        }
        if !state.draft_override {
            let mut save = Button::new(t("common.save"))
                .variant(ButtonVariant::Submit) // React primary
                .control_size(ControlSize::Regular);
            if !state.all_tools.is_empty() && !state.tools_dirty() {
                save = save.state(State::Disabled).disabled(true);
            } else if let Some(f) = &self.on_save_tools {
                let f = Rc::clone(f);
                save = save.on_press(move |s: &mut S| f(s));
            }
            footer = footer.child(save.build(k).key("mcp-tools-save"));
        }
        footer
    }

    /// The audit log block.
    fn audit_block(&self, k: &Kit, layer: &mut Vec<El>, dark: bool) -> El {
        let state = self.state;
        let _ = k;
        let _ = layer;

        let mut block = col().key("mcp-audit").w_full().gap(8.0);

        let mut head = row().key("mcp-audit-head").w_full().gap(12.0).center_y().justify_between();
        head = head.child(
            col().key("mcp-audit-head-meta").gap(2.0)
                .child(text(t("mcp.audit.title")).font(TEXT_SM_MEDIUM).color(k.c.text_base))
                .child(text(t("mcp.audit.description")).font(TEXT_XS).color(k.c.text_muted)),
        );
        let mut refresh = Button::new(t("mcp.audit.refresh"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small);
        if state.audit_loading {
            refresh = refresh.state(State::Disabled).disabled(true);
        } else if let Some(f) = &self.on_refresh_audit {
            let f = Rc::clone(f);
            refresh = refresh.on_press(move |s: &mut S| f(s));
        }
        head = head.child(refresh.build(k).key("mcp-audit-refresh"));
        block = block.child(head);

        if let Some(message) = &state.audit_error {
            block = block.child(error_line(k, "mcp-audit-error", message));
        } else if state.audit_rows.is_empty() && !state.audit_loading {
            block = block.child(
                text(t("mcp.audit.empty")).key("mcp-audit-empty").font(TEXT_XS).color(k.c.text_muted),
            );
        }

        if !state.audit_rows.is_empty() {
            let columns = [
                Column::new(t("mcp.audit.when").to_owned()),
                Column::new(t("mcp.audit.who").to_owned()),
                Column::new(t("mcp.audit.connection").to_owned()),
                Column::new(t("mcp.audit.tool").to_owned()),
                Column::new(t("mcp.audit.decision").to_owned()),
                Column::new(t("mcp.audit.requestId").to_owned()),
                Column::new(t("mcp.audit.rows").to_owned()),
            ];
            let table = DataTable::new("mcp-audit-table", &columns)
                .rows(state.audit_rows.iter().map(audit_row))
                .empty_text(t("mcp.audit.empty").to_owned())
                .label(t("mcp.audit.title").to_owned());
            block = block.child(table.build(dark));
        }
        block
    }
}

/// One audit row → the table atom's row shape (the same cells React's
/// `<td>`s print, `—` for the nullables).
fn audit_row(record: &McpAuditRecordVm) -> Row {
    let connection = match (&record.connection_id, &record.database_name, &record.namespace) {
        (None, _, _) => "—".to_owned(),
        (Some(id), db, ns) => {
            let mut text = id.clone();
            if let Some(db) = db {
                text.push('/');
                text.push_str(db);
            }
            if let Some(ns) = ns {
                text.push('/');
                text.push_str(ns);
            }
            text
        }
    };
    Row {
        key: Some(record.id.clone()),
        cells: vec![
            CellValue::new(record.created_at.clone()).mono().muted(),
            CellValue::new(record.principal_id.clone()).mono(),
            CellValue::new(connection).mono(),
            CellValue::new(record.tool.clone()).mono(),
            CellValue::new(record.decision.as_str().to_owned()),
            CellValue::new(record.request_id.clone().unwrap_or_else(|| "—".to_owned())).mono().muted(),
            CellValue::new(
                record.row_count.map(|n| n.to_string()).unwrap_or_else(|| "—".to_owned()),
            ),
        ],
    }
}

/// The permission mode's (label, hint) keys.
fn mode_labels(mode: McpPermissionMode) -> (&'static str, &'static str) {
    match mode {
        McpPermissionMode::ReadOnly => ("mcp.permission.readOnly", "mcp.permission.readOnlyHint"),
        McpPermissionMode::SafeWrite => ("mcp.permission.safeWrite", "mcp.permission.safeWriteHint"),
        McpPermissionMode::HighRiskWrite => {
            ("mcp.permission.highRiskWrite", "mcp.permission.highRiskWriteHint")
        }
    }
}

/// The agent target's key suffix.
fn target_key(target: McpAgentTarget) -> &'static str {
    match target {
        McpAgentTarget::Cursor => "cursor",
        McpAgentTarget::Claude => "claude",
    }
}

/// The status line: the green/muted dot + the label.
fn status_dot(k: &Kit, on: bool, label: &str, key: &str) -> El {
    let color = if on { k.c.success.fg } else { k.c.text_muted };
    row().key(key.to_owned()).gap(8.0).center_y().pt(8.0)
        .child(col().key(format!("{key}-dot")).size(DOT, DOT).radius(DOT / 2.0).fill(color))
        .child(text(label.to_owned()).key(format!("{key}-label")).font(TEXT_SM).color(k.c.text_base))
}

/// React's `text-xs text-red-500` error line.
fn error_line(k: &Kit, key: &str, message: &str) -> El {
    text(message.to_owned())
        .key(key.to_owned())
        .font(TEXT_XS)
        .color(k.c.danger.fg)
}
