//! T169: `components/McpApprovalHost.tsx` port — the global host for MCP
//! per-call approvals. The backend pauses a non-read-only / high-risk
//! call and emits `mcp:approval-requested`; the host dialog shows the
//! exact redacted SQL / arguments, `Allow once` resumes the call, and
//! Deny / Stop / the five-minute timeout resolve it without executing
//! the tool. An `mcp:audit-error` event shows a dismissible error toast.
//!
//! | React source | Rune item |
//! |---|---|
//! | `McpApprovalRequest` (`types`, the backend `mcp/domain/approval.rs` wire shape) | [`McpApprovalRequest`] — the ui-side contract mirror, `camelCase` serde exactly as the backend's |
//! | `McpApprovalDecision` (`types`) | [`McpApprovalDecision`] — `snake_case` serde, the four vocabulary values |
//! | the component's `useState` queue / responding / error / auditError / `timedOutRef` | [`McpApprovalHostState`] — caller-owned, one slot in the app state |
//! | `listenCrossWindow('mcp:approval-requested' / 'mcp:audit-error')` | not this port's job: the host process feeds the state (`push_request` / `set_audit_error`) — the same seam every other port leaves to the host wiring |
//! | `aiCommands.mcpRespondApproval` / `mcpCancelApproval` | the `on_allow_once` / `on_deny` / `on_stop` callbacks — the host wiring routes them to the backend port |
//! | `window.setInterval(tick, 1000)` | nothing to port: the caller re-renders every frame and passes `now_ms`; [`McpApprovalHostState::expire_check`] is the tick's decision half (dedup + first-timeout) |
//! | the modal (`fixed inset-0 z-[10000]`) | `ui/dialog.rs`'s `Dialog` base — the fixed scrim / overlay placement is the dialog base's, mounted through `layer` |
//! | the audit toast (`fixed bottom-4 right-4 z-[10001]`) | returned in the build's tree (React's fixed placement is the window layer's job in Rune; the caller mounts it, the same documented seam as `ChatBubble`'s fullscreen tree) |
//!
//! # State ownership
//!
//! The host owns no state: the caller owns one [`McpApprovalHostState`]
//! (in `AppState` once the host wiring lands; in the probe state in
//! tests), passes `&state` and the frame's `now_ms` each frame, and
//! drives the mutations through the state's methods, mirroring React's
//! handlers exactly.
//!
//! # Deliberate boundaries
//!
//! * **`Date.parse` non-finite expiry is an immediate zero.** React
//!   computes `Number.isFinite(expiresAt) ? expiresAt - Date.now() : 0`;
//!   the port mirrors that (an unparseable `expiresAt` is already
//!   expired), with chrono's RFC 3339 parser where the browser has
//!   `Date.parse`.
//! * **`remaining` rounds up to the next second.** React's
//!   `Math.ceil(remainingMs / 1000)`; the port does the same, so the
//!   label flips from "{n} 秒后过期" to "已过期" at the same moment.
//! * **`seconds <= 0` disables Allow (and shows the expired label), but
//!   Deny / Stop stay enabled** while `!responding` — React disables
//!   Allow only.
//! * **The timeout decision is the caller's.** React's interval calls
//!   `publishDecision(requestId, 'timeout')`; here
//!   [`McpApprovalHostState::expire_check`] returns the request id once
//!   (the `timedOutRef` dedup), and the caller routes it through the
//!   same respond callback it wired for the buttons.
//! * **The error string prefers the cause's message.** React keeps
//!   `cause.message` when present and otherwise
//!   `mcp.approval.respondFailed`; `fail_response` takes the optional
//!   message and falls back the same way.
//! * **The toast's `×` button is an icon-only kit button** whose a11y
//!   label is `common.close`, as React's `aria-label` does.

use std::rc::Rc;

use chrono::DateTime;
use rune::{col, row, text, El, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;

/// React `McpApprovalRequest` (`types`) — the wire shape the backend's
/// `mcp/domain/approval.rs` sends: `camelCase`, `arguments` as raw JSON.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpApprovalRequest {
    pub request_id: String,
    pub tool: String,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub namespace: Option<String>,
    /// Exact SQL after credential redaction, never an audit copy.
    pub sql: Option<String>,
    /// Exact JSON arguments after credential redaction.
    pub arguments: Value,
    pub created_at: String,
    pub expires_at: String,
    pub timeout_ms: u64,
}

impl McpApprovalRequest {
    /// A minimal request for tests and host wiring seeds.
    pub(crate) fn new(tool: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            tool: tool.into(),
            connection_id: None,
            database: None,
            namespace: None,
            sql: None,
            arguments: Value::Null,
            created_at: String::new(),
            expires_at: String::new(),
            timeout_ms: 0,
        }
    }

    /// `expires_at` as epoch milliseconds, `None` when the stamp is not a
    /// valid RFC 3339 instant (React's `Date.parse` returning `NaN`).
    pub(crate) fn expires_at_ms(&self) -> Option<i64> {
        DateTime::parse_from_rfc3339(&self.expires_at)
            .ok()
            .map(|stamp| stamp.timestamp_millis())
    }
}

/// React `McpApprovalDecision` — the four decisions the host may send
/// (`allow_once` / `deny` / `cancel` / `timeout` on the wire).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum McpApprovalDecision {
    AllowOnce,
    Deny,
    Timeout,
    Cancel,
}

/// The mutable state behind [`McpApprovalHost`]: the waiting queue, the
/// in-flight response, the last respond error, the audit toast text and
/// the timed-out request ids. The caller owns one and drives it through
/// the methods below, mirroring the React handlers.
#[derive(Clone, Debug, Default)]
pub(crate) struct McpApprovalHostState {
    queue: Vec<McpApprovalRequest>,
    responding: bool,
    error: Option<String>,
    audit_error: Option<String>,
    timed_out: Vec<String>,
}

impl McpApprovalHostState {
    /// React `queue[0]` — the request the dialog shows.
    pub(crate) fn current(&self) -> Option<&McpApprovalRequest> {
        self.queue.first()
    }

    /// How many requests are waiting (the queue guard's observable).
    pub(crate) fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// The `mcp:approval-requested` listener's push: an empty tool or a
    /// known `requestId` is dropped, exactly as the React guard does.
    pub(crate) fn push_request(&mut self, request: McpApprovalRequest) {
        if request.tool.is_empty() {
            return;
        }
        if self.queue.iter().any(|item| item.request_id == request.request_id) {
            return;
        }
        self.queue.push(request);
    }

    /// The `mcp:audit-error` listener's setter: the raw string, or the
    /// generic `mcp.approval.auditFailed` when the payload is not one.
    pub(crate) fn set_audit_error(&mut self, message: Option<String>) {
        self.audit_error = Some(message.unwrap_or_else(|| t("mcp.approval.auditFailed").to_owned()));
    }

    /// The toast's `×`: the audit error clears.
    pub(crate) fn clear_audit_error(&mut self) {
        self.audit_error = None;
    }

    /// The audit toast's text, if any.
    pub(crate) fn audit_error(&self) -> Option<&str> {
        self.audit_error.as_deref()
    }

    /// `publishDecision` / `handleStop`'s start: responding begins and the
    /// previous error clears (`setResponding(true)` + `setError(null)`).
    pub(crate) fn begin_responding(&mut self) {
        self.responding = true;
        self.error = None;
    }

    /// The catch arm: the cause's message when present, otherwise
    /// `mcp.approval.respondFailed`; the responding flag ends.
    pub(crate) fn fail_response(&mut self, message: Option<String>) {
        self.error = Some(message.unwrap_or_else(|| t("mcp.approval.respondFailed").to_owned()));
        self.responding = false;
    }

    /// The success arm: the request leaves the queue and the responding
    /// flag ends (`finally`).
    pub(crate) fn finish_response(&mut self, request_id: &str) {
        self.queue.retain(|item| item.request_id != request_id);
        self.responding = false;
    }

    /// The last respond error, if any.
    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Whether a respond is in flight (the buttons disable).
    pub(crate) fn responding(&self) -> bool {
        self.responding
    }

    /// React's `seconds`: `ceil(remainingMs / 1000)`; no current request
    /// or an unparseable / past expiry is 0 (already expired).
    pub(crate) fn seconds_left(&self, now_ms: i64) -> u64 {
        let Some(current) = self.queue.first() else {
            return 0;
        };
        let remaining = current
            .expires_at_ms()
            .map(|expires_at| expires_at - now_ms)
            .unwrap_or(0);
        // React's Math.max(0, next) then ceil: a negative remaining is 0,
        // a positive one rounds up to the next whole second.
        if remaining <= 0 {
            0
        } else {
            ((remaining + 999) / 1000).max(0) as u64
        }
    }

    /// React's `tick` timeout half: the current request's id when its
    /// expiry has passed and it has not been timed out before (the
    /// `timedOutRef` dedup). The caller routes the returned id through
    /// its respond callback with [`McpApprovalDecision::Timeout`].
    pub(crate) fn expire_check(&mut self, now_ms: i64) -> Option<String> {
        let Some(current) = self.queue.first() else {
            return None;
        };
        let remaining = current
            .expires_at_ms()
            .map(|expires_at| expires_at - now_ms)
            .unwrap_or(0);
        if remaining > 0 {
            return None;
        }
        if self.timed_out.iter().any(|id| id == &current.request_id) {
            return None;
        }
        self.timed_out.push(current.request_id.clone());
        Some(current.request_id.clone())
    }

    /// Forgets a request's timed-out mark (the queue keeping it out is
    /// what matters; this exists so a re-push after a full round can
    /// time out again the way a fresh React mount would).
    pub(crate) fn forget_timed_out(&mut self, request_id: &str) {
        self.timed_out.retain(|id| id != request_id);
    }
}

/// `McpApprovalHost` — the dialog for the queued request plus the audit
/// error toast. The host process feeds the state and the wiring routes
/// the decisions; this builder only renders.
pub(crate) struct McpApprovalHost<S: 'static> {
    on_allow_once: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_deny: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_stop: Option<Rc<dyn Fn(&mut S, &str)>>,
    on_dismiss_audit: Option<Rc<dyn Fn(&mut S)>>,
    key: String,
}

impl<S: 'static> Default for McpApprovalHost<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: 'static> McpApprovalHost<S> {
    pub(crate) fn new() -> Self {
        Self {
            on_allow_once: None,
            on_deny: None,
            on_stop: None,
            on_dismiss_audit: None,
            key: "mcp-approval-host".to_string(),
        }
    }

    /// React `publishDecision(current.requestId, 'allow_once')`.
    pub(crate) fn on_allow_once(
        mut self,
        f: impl Fn(&mut S, &str) + 'static,
    ) -> Self {
        self.on_allow_once = Some(Rc::new(f));
        self
    }

    /// React `publishDecision(current.requestId, 'deny')`.
    pub(crate) fn on_deny(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_deny = Some(Rc::new(f));
        self
    }

    /// React `handleStop` — `mcpCancelApproval(current.requestId)`.
    pub(crate) fn on_stop(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_stop = Some(Rc::new(f));
        self
    }

    /// The audit toast's `×` press (`setAuditError(null)`).
    pub(crate) fn on_dismiss_audit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_dismiss_audit = Some(Rc::new(f));
        self
    }

    /// Override the root key prefix.
    pub(crate) fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// Builds the host: the request dialog plus the audit toast, or an
    /// empty root when neither is showing (React's `return null` — an
    /// empty column, no children). `now_ms` is the frame's epoch
    /// milliseconds; the caller re-renders each frame, which is the
    /// port of React's `setInterval(tick, 1000)` recompute. The dialog
    /// itself mounts through `layer` (the modal base's channel).
    pub(crate) fn build(
        self,
        state: &McpApprovalHostState,
        now_ms: i64,
        dark: bool,
        layer: &mut Vec<El>,
    ) -> El {
        let k = kit(dark);
        let mut root = col().key(self.key.clone()).min_w(0.0).w_full();
        if let Some(current) = state.current() {
            self.request_dialog(&k, state, current, now_ms, layer);
        }
        if let Some(audit_error) = state.audit_error() {
            root = root.child(self.audit_toast(&k, audit_error));
        }
        root
    }

    /// The request dialog: title, description, the `dl` detail rows, the
    /// redacted SQL, the JSON arguments, the respond error, and the
    /// footer (countdown + Stop / Deny / Allow once).
    fn request_dialog(
        &self,
        k: &rune_kit::Kit,
        state: &McpApprovalHostState,
        current: &McpApprovalRequest,
        now_ms: i64,
        layer: &mut Vec<El>,
    ) {
        let seconds = state.seconds_left(now_ms);
        let mut dialog = Dialog::new(format!("{}-approval", self.key), t("mcp.approval.title"))
            .description(t("mcp.approval.description"))
            // React `w-full max-w-2xl` = 672pt.
            .width(672.0)
            .open(true);
        dialog = dialog.child(self.detail_rows(k, current));
        if current.sql.as_deref().is_some_and(|sql| !sql.is_empty()) {
            dialog = dialog.child(self.sql_block(k, current.sql.as_deref().unwrap_or("")));
        }
        dialog = dialog.child(self.arguments_block(k, current));
        if let Some(error) = state.error() {
            dialog = dialog.child(
                text(error.to_owned())
                    .key(format!("{}-error", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .wrap(),
            );
        }
        dialog
            .footer(self.footer(k, state, current, seconds))
            // The dialog base mounts nothing without a close handler; the
            // approval answers only through its buttons, so Escape is a
            // no-op exactly as React's dialog (no onClose) is.
            .on_close(|_s: &mut S| {})
            .build(k, layer)
    }

    /// React's `dl` grid: four `8rem / 1fr` rows of muted label plus mono
    /// value, `—` for the `null` options.
    fn detail_rows(&self, k: &rune_kit::Kit, current: &McpApprovalRequest) -> El {
        let mut rows = col()
            .key(format!("{}-details", self.key))
            .min_w(0.0)
            .w_full()
            .gap(2.0);
        let entries = [
            (t("mcp.approval.tool"), current.tool.clone()),
            (
                t("mcp.approval.connection"),
                current.connection_id.clone().unwrap_or_else(|| "—".to_owned()),
            ),
            (
                t("mcp.approval.database"),
                current.database.clone().unwrap_or_else(|| "—".to_owned()),
            ),
            (
                t("mcp.approval.namespace"),
                current.namespace.clone().unwrap_or_else(|| "—".to_owned()),
            ),
        ];
        for (idx, (label, value)) in entries.into_iter().enumerate() {
            rows = rows.child(
                row()
                    .key(format!("{}-detail-{idx}", self.key))
                    .min_w(0.0)
                    .w_full()
                    .items_start()
                    .gap(6.0)
                    .child(
                        text(label.to_owned())
                            .key(format!("{}-detail-label-{idx}", self.key))
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    )
                    .child(
                        text(value)
                            .key(format!("{}-detail-value-{idx}", self.key))
                            .font(k.ty.mono)
                            .color(k.c.text_base)
                            .wrap(),
                    ),
            );
        }
        rows
    }

    /// The SQL block: the muted label plus the `max-h-64` mono pre.
    fn sql_block(&self, k: &rune_kit::Kit, sql: &str) -> El {
        col()
            .key(format!("{}-sql", self.key))
            .min_w(0.0)
            .w_full()
            .gap(2.0)
            .child(
                text(t("mcp.approval.sql"))
                    .key(format!("{}-sql-label", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(
                text(sql.to_owned())
                    .key(format!("{}-sql-text", self.key))
                    .font(k.ty.mono)
                    .color(k.c.text_base)
                    .wrap()
                    .max_h(256.0),
            )
    }

    /// The arguments block: `JSON.stringify(arguments, null, 2)` in a
    /// `max-h-56` mono pre (React renders it even when null: "null").
    fn arguments_block(&self, k: &rune_kit::Kit, current: &McpApprovalRequest) -> El {
        let rendered = serde_json::to_string_pretty(&current.arguments)
            .unwrap_or_else(|_| "null".to_owned());
        col()
            .key(format!("{}-arguments", self.key))
            .min_w(0.0)
            .w_full()
            .gap(2.0)
            .child(
                text(t("mcp.approval.arguments"))
                    .key(format!("{}-arguments-label", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(
                text(rendered)
                    .key(format!("{}-arguments-text", self.key))
                    .font(k.ty.mono)
                    .color(k.c.text_muted)
                    .wrap()
                    .max_h(224.0),
            )
    }

    /// The footer: the countdown (amber within the last 30 seconds) on
    /// the left, Stop / Deny / Allow once on the right. Allow disables
    /// while responding or expired; Stop and Deny only while responding.
    fn footer(
        &self,
        k: &rune_kit::Kit,
        state: &McpApprovalHostState,
        current: &McpApprovalRequest,
        seconds: u64,
    ) -> El {
        let countdown = if seconds > 0 {
            t("mcp.approval.remaining").replacen("{seconds}", &seconds.to_string(), 1)
        } else {
            t("mcp.approval.expired").to_owned()
        };
        let countdown_color = if seconds <= 30 {
            k.c.warning.fg
        } else {
            k.c.text_muted
        };
        let left = text(countdown)
            .key(format!("{}-countdown", self.key))
            .font(k.ty.text_xsmall)
            .color(countdown_color)
            .wrap();

        let mut buttons = row()
            .key(format!("{}-actions", self.key))
            .items_center()
            .gap(8.0);
        if let Some(on_stop) = self.on_stop.clone() {
            let request_id = current.request_id.clone();
            // React's disabled button never fires; Rune's `El::disabled`
            // is the a11y flag, so the dispatch guards itself on the
            // frame's state.
            let disabled = state.responding();
            let mut stop = Button::new(t("mcp.approval.stop"))
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::Small)
                .disabled(disabled)
                .build(k)
                .key(format!("{}-stop", self.key));
            stop = stop.on_press(move |s: &mut S| {
                if !disabled {
                    on_stop(s, &request_id);
                }
            });
            buttons = buttons.child(stop);
        }
        if let Some(on_deny) = self.on_deny.clone() {
            let request_id = current.request_id.clone();
            let disabled = state.responding();
            let mut deny = Button::new(t("mcp.approval.deny"))
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::Small)
                .disabled(disabled)
                .build(k)
                .key(format!("{}-deny", self.key));
            deny = deny.on_press(move |s: &mut S| {
                if !disabled {
                    on_deny(s, &request_id);
                }
            });
            buttons = buttons.child(deny);
        }
        if let Some(on_allow_once) = self.on_allow_once.clone() {
            let request_id = current.request_id.clone();
            let disabled = state.responding() || seconds <= 0;
            let mut allow = Button::new(t("mcp.approval.allowOnce"))
                .variant(ButtonVariant::Contrast)
                .control_size(ControlSize::Small)
                .disabled(disabled)
                .build(k)
                .key(format!("{}-allow-once", self.key));
            allow = allow.on_press(move |s: &mut S| {
                if !disabled {
                    on_allow_once(s, &request_id);
                }
            });
            buttons = buttons.child(allow);
        }

        row()
            .key(format!("{}-footer", self.key))
            .min_w(0.0)
            .w_full()
            .items_center()
            .justify_between()
            .gap(8.0)
            .child(left)
            .child(buttons)
    }

    /// The audit toast: an alert with the `auditFailed` label, the raw
    /// error text and the `×` dismiss (React's fixed bottom-right
    /// placement is the window layer's mount, see the module docs).
    fn audit_toast(&self, k: &rune_kit::Kit, audit_error: &str) -> El {
        let mut toast = row()
            .key(format!("{}-audit-toast", self.key))
            .min_w(0.0)
            .max_w(448.0)
            .items_start()
            .justify_between()
            .gap(8.0)
            .border(1.0, k.c.danger.border)
            .radius(k.radius.sm)
            .fill(k.c.surface_base)
            .px(8.0)
            .py(6.0)
            .child(
                text(format!("{}: {audit_error}", t("mcp.approval.auditFailed")))
                    .key(format!("{}-audit-toast-text", self.key))
                    .font(k.ty.text_xsmall)
                    .color(k.c.danger.fg)
                    .wrap(),
            );
        if let Some(on_dismiss) = self.on_dismiss_audit.clone() {
            let mut dismiss = Button::new(t("common.close"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .build(k)
                .key(format!("{}-audit-toast-dismiss", self.key));
            dismiss = dismiss.on_press(move |s: &mut S| on_dismiss(s));
            toast = toast.child(dismiss);
        }
        toast
    }
}
