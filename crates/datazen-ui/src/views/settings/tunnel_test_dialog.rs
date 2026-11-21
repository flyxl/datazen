//! Port of
//! `packages/frontend/src/windows/settings/TunnelTestDialog.tsx`
//! (T459) — the live probe for a saved tunnel.
//!
//! The target host/port is a required input, not a decoration: the
//! probe sends a CONNECT handshake to that target (http proxy) or
//! opens a channel to it (websocket `datazen_v1`). The scope note
//! states honestly what a pass proves — an SSH probe only reaches
//! the jump host, `raw_binary` only reaches the relay.
//!
//! ## What maps where
//!
//! - The `useEffect([open, tunnel?.id])` open-reset is
//!   [`TunnelTestDialogState::open_changed`]: an open always starts
//!   from the clean form (`host` back to `127.0.0.1`, `port` blank,
//!   no run in flight, no answer). A close keeps the stale form
//!   (React's `if (!open) return;`) — invisible until the next open.
//! - `isValidPort` is [`is_valid_port`]: non-blank, integral and in
//!   `1..=65535` (React's `Number.isInteger` over `Number(value)`
//!   is a `f64` parse with a zero fraction).
//! - `handleRun` is [`Self::run_gate`] + [`Self::begin_run`] +
//!   [`Self::run_request`] and the host's `on_run(id, host, port)`
//!   slot: the host runs T432's `test_tunnel_request` rail and
//!   answers through [`Self::note_success`] (the elapsed
//!   milliseconds) or [`Self::note_failure`] (the classified
//!   message; blank falls back to `common.operationFailed` —
//!   React's `messageOf(e) ?? t('common.operationFailed')`).
//! - `scopeKey` is [`Self::scope_key`]: ssh / httpProxy /
//!   websocket (a missing tunnel falls through to websocket, the
//!   React ternary's last arm).
//! - The success line / error banner / target hint are
//!   [`Self::success_line`] / `error` + `t(...).replace("{error}")`
//!   / [`Self::target_hint`] (host first, port second).
//!
//! ## Keys
//!
//! The React `data-testid`s stay verbatim (`tunnel-test-dialog`,
//! `tunnel-test-close`, `tunnel-test-run`, `tunnel-test-host`,
//! `tunnel-test-port`, `tunnel-test-scope`,
//! `tunnel-test-scope-general`, `tunnel-test-success`,
//! `tunnel-test-error`, `tunnel-test-target-hint`).
//!
//! Known deltas: none behavioural — the run is the host's invoke
//! (Rune has no async await here), everything else is the React
//! file's verbatim.

// No production consumer yet (the tunnel settings section's
// test_dialog slot mounts it once a host wires it); this crate's
// tests are the callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use datazen_ui_api::SavedTunnelKind;
use rune::{col, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::commands::tunnel::SavedTunnelSummary;
use crate::i18n::t;
use crate::ui::banner::{Banner, BannerElement, BannerVariant};
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;

/// The `host` input's default (and placeholder).
const DEFAULT_TARGET_HOST: &str = "127.0.0.1";

/// `text-xs` — the success line.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);
/// `text-[11px]` — the scope notes and the target hint.
const TEXT_11: TextStyle = TextStyle::new(11.0, 400.0);
/// `text-xs font-medium` — the field labels.
const TEXT_XS_MEDIUM: TextStyle = TextStyle::new(12.0, 500.0);

/// `isValidPort(value)`: non-blank, an integer, in `1..=65535`.
fn is_valid_port(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    match trimmed.parse::<f64>() {
        Ok(port) => port.fract() == 0.0 && port > 0.0 && port <= 65535.0,
        Err(_) => false,
    }
}

/// The dialog's state.
#[derive(Clone, PartialEq)]
pub(crate) struct TunnelTestDialogState {
    pub(crate) open: bool,
    /// The tunnel under test (`None` = closed / none).
    pub(crate) tunnel: Option<SavedTunnelSummary>,
    /// The target host input (defaults to `127.0.0.1`).
    pub(crate) host: TextEdit,
    /// The target port input (blank until typed).
    pub(crate) port: TextEdit,
    /// The probe in flight.
    pub(crate) running: bool,
    /// The probe's answer: elapsed milliseconds.
    pub(crate) elapsed_ms: Option<u64>,
    /// The probe's failure sentence (already the display text).
    pub(crate) error: Option<String>,
}

impl Default for TunnelTestDialogState {
    fn default() -> Self {
        Self {
            open: false,
            tunnel: None,
            host: TextEdit::new(DEFAULT_TARGET_HOST.to_owned()),
            port: TextEdit::default(),
            running: false,
            elapsed_ms: None,
            error: None,
        }
    }
}

impl TunnelTestDialogState {
    /// The `useEffect([open, tunnel?.id])` arm: an open always
    /// starts from the clean form; a close keeps the stale one
    /// (React's early return).
    pub(crate) fn open_changed(&mut self, open: bool, tunnel: Option<&SavedTunnelSummary>) {
        self.open = open;
        self.tunnel = tunnel.cloned();
        if !open {
            return;
        }
        self.host = TextEdit::new(DEFAULT_TARGET_HOST.to_owned());
        self.port = TextEdit::default();
        self.running = false;
        self.elapsed_ms = None;
        self.error = None;
    }

    /// `hostMissing` — a blank host (after trim).
    pub(crate) fn host_missing(&self) -> bool {
        self.host.text.trim().is_empty()
    }

    /// `portMissing` — not a valid port.
    pub(crate) fn port_missing(&self) -> bool {
        !is_valid_port(&self.port.text)
    }

    /// `invalid = hostMissing || portMissing` — the run button's
    /// disabled gate and the hint's show gate.
    pub(crate) fn invalid(&self) -> bool {
        self.host_missing() || self.port_missing()
    }

    /// The run button's gate: `if (!tunnelId || invalid || running)
    /// return;`.
    pub(crate) fn run_gate(&self) -> bool {
        self.tunnel.is_some() && !self.invalid() && !self.running
    }

    /// `handleRun`'s preamble: mark the probe in flight and clear
    /// the previous answer.
    pub(crate) fn begin_run(&mut self) {
        self.running = true;
        self.elapsed_ms = None;
        self.error = None;
    }

    /// `handleRun`'s ask: the tunnel id, the trimmed target host
    /// and the target port (`Number(port)`). `None` when the gate
    /// refuses.
    pub(crate) fn run_request(&self) -> Option<(String, String, u16)> {
        if !self.run_gate() {
            return None;
        }
        let id = self.tunnel.as_ref()?.id.clone();
        let host = self.host.text.trim().to_owned();
        let port = self.port.text.trim().parse::<f64>().ok()?;
        Some((id, host, port as u16))
    }

    /// The probe's success answer (`setElapsedMs(ms)`), landing in
    /// the `finally`'s shadow.
    pub(crate) fn note_success(&mut self, elapsed_ms: u64) {
        self.running = false;
        self.elapsed_ms = Some(elapsed_ms);
    }

    /// The probe's failure answer: `messageOf(e) ??
    /// t('common.operationFailed')` — a blank message falls back to
    /// the generic sentence.
    pub(crate) fn note_failure(&mut self, message: &str) {
        self.running = false;
        self.error = Some(if message.is_empty() {
            t("common.operationFailed").to_owned()
        } else {
            message.to_owned()
        });
    }

    /// `scopeKey` — which surface a pass actually proves.
    pub(crate) fn scope_key(&self) -> &'static str {
        match self.tunnel.as_ref().map(|t| t.kind) {
            Some(SavedTunnelKind::Ssh) => "settings.tunnels.test.scope.ssh",
            Some(SavedTunnelKind::HttpProxy) => "settings.tunnels.test.scope.httpProxy",
            _ => "settings.tunnels.test.scope.websocket",
        }
    }

    /// The success line (`role="status"`): `t('…success', { ms })`.
    pub(crate) fn success_line(&self) -> Option<String> {
        self.elapsed_ms
            .map(|ms| t("settings.tunnels.test.success").replace("{ms}", &ms.to_string()))
    }

    /// The target hint: the host's sentence first, the port's
    /// second, nothing when both are valid.
    pub(crate) fn target_hint(&self) -> Option<&'static str> {
        if self.host_missing() {
            Some("settings.tunnels.test.hostRequired")
        } else if self.port_missing() {
            Some("settings.tunnels.test.portRequired")
        } else {
            None
        }
    }
}

/// `TunnelTestDialog({ open, tunnel, onClose, onRun })`: the dialog
/// over the state, the state lens and the host's action slots.
pub(crate) struct TunnelTestDialog<'a, S: 'static> {
    state: &'a TunnelTestDialogState,
    dark: bool,
    dialog_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TunnelTestDialogState>,
    on_close: Option<Rc<dyn Fn(&mut S)>>,
    /// `onRun(id, host, port)` — the host runs the probe (T432's
    /// rail) and answers through the note methods.
    on_run: Option<Rc<dyn Fn(&mut S, &str, &str, u16)>>,
}

/// The host lens adapter for the `host` input (a top-level fn: the
/// HRTB needs the named shape).
fn host_edit_of<S: 'static>(
    dialog_edit: &Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TunnelTestDialogState>,
) -> Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit> {
    let dialog_edit = Rc::clone(dialog_edit);
    Rc::new(move |s: &mut S| &mut dialog_edit(s).host)
}

/// The port lens (same shape).
fn port_edit_of<S: 'static>(
    dialog_edit: &Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TunnelTestDialogState>,
) -> Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit> {
    let dialog_edit = Rc::clone(dialog_edit);
    Rc::new(move |s: &mut S| &mut dialog_edit(s).port)
}

impl<'a, S: 'static> TunnelTestDialog<'a, S> {
    pub(crate) fn new(
        state: &'a TunnelTestDialogState,
        dark: bool,
        dialog_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TunnelTestDialogState + 'static,
    ) -> Self {
        let dialog_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TunnelTestDialogState> =
            Rc::new(dialog_edit);
        Self { state, dark, dialog_edit, on_close: None, on_run: None }
    }

    /// `onClose` (a no-op while the probe runs — the dialog's
    /// dismissal rule).
    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// `onRun(id, host, port)`: the host runs the probe and answers
    /// through the note methods.
    pub(crate) fn on_run(mut self, f: impl Fn(&mut S, &str, &str, u16) + 'static) -> Self {
        self.on_run = Some(Rc::new(f));
        self
    }

    /// The dialog (nothing when closed).
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) {
        let state = self.state;
        if !state.open {
            return;
        }
        let name = state.tunnel.as_ref().map(|t| t.name.as_str()).unwrap_or("");
        let description = t("settings.tunnels.test.description").replace("{name}", name);

        let mut dialog = Dialog::new("tunnel-test-dialog", t("settings.tunnels.test.title"))
            .open(true)
            .width(480.0)
            .description(description);
        // The dismissal policy: a no-op while the probe runs.
        {
            let running = state.running;
            let on_close = self.on_close.clone();
            dialog = dialog.on_close(move |s: &mut S| {
                if running {
                    return;
                }
                if let Some(on_close) = &on_close {
                    on_close(s);
                }
            });
        }

        // The body: the target grid, the scope notes, then the
        // answer lines.
        let mut body = col().key("tunnel-test-body").w_full().gap(12.0); // space-y-3

        let mut grid = row().key("tunnel-test-grid").w_full().gap(12.0);
        let mut host_block = col().key("tunnel-test-host-block").w_full().gap(6.0);
        host_block = host_block.child(
            text(format!("{} *", t("settings.tunnels.test.targetHost")))
                .font(TEXT_XS_MEDIUM)
                .color(k.c.text_muted),
        );
        {
            let host_edit = host_edit_of::<S>(&self.dialog_edit);
            let field = TextField::new("tunnel-test-host", &state.host)
                .height(36.0)
                .placeholder(DEFAULT_TARGET_HOST)
                .on_edit::<S>(move |s: &mut S| host_edit(s));
            host_block = host_block.child(field.build(self.dark));
        }
        grid = grid.child(host_block.grow(1.0));

        let mut port_block = col().key("tunnel-test-port-block").w_full().gap(6.0);
        port_block = port_block.child(
            text(format!("{} *", t("settings.tunnels.test.targetPort")))
                .font(TEXT_XS_MEDIUM)
                .color(k.c.text_muted),
        );
        {
            let port_edit = port_edit_of::<S>(&self.dialog_edit);
            let field = TextField::new("tunnel-test-port", &state.port)
                .height(36.0)
                .placeholder("5432")
                .on_edit::<S>(move |s: &mut S| port_edit(s));
            port_block = port_block.child(field.build(self.dark));
        }
        grid = grid.child(port_block.grow(1.0));
        body = body.child(grid);

        body = body.child(
            text(t(state.scope_key()).to_owned())
                .key("tunnel-test-scope")
                .font(TEXT_11)
                .color(k.c.text_muted),
        );
        body = body.child(
            text(t("settings.tunnels.test.scope.general").to_owned())
                .key("tunnel-test-scope-general")
                .font(TEXT_11)
                .color(k.c.text_muted),
        );

        if let Some(line) = state.success_line() {
            // React `role="status"`, `text-green-500`.
            body = body.child(
                text(line).key("tunnel-test-success").font(TEXT_XS).color(k.c.success.fg),
            );
        }
        if let Some(error) = &state.error {
            // React <ErrorBanner as="p">.
            let line = t("settings.tunnels.test.failed").replace("{error}", error);
            if let Some(banner) = Banner::new("tunnel-test-error", line)
                .variant(BannerVariant::Plain)
                .as_element(BannerElement::P)
                .build(k)
            {
                body = body.child(banner);
            }
        }
        if let Some(key) = state.target_hint() {
            body = body.child(
                text(t(key).to_owned())
                    .key("tunnel-test-target-hint")
                    .font(TEXT_11)
                    .color(k.c.text_muted),
            );
        }
        dialog = dialog.child(body);

        // The footer: the close + the run.
        let mut close = Button::new(t("common.close"))
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Small)
            .disabled(state.running);
        if let Some(on_close) = &self.on_close {
            let on_close = Rc::clone(on_close);
            let running = state.running;
            close = close.on_press(move |s: &mut S| {
                if running {
                    return;
                }
                on_close(s);
            });
        }
        let mut run = Button::new(if state.running {
            t("settings.tunnels.test.running")
        } else {
            t("settings.tunnels.test.run")
        })
        .control_size(ControlSize::Small)
        .disabled(state.invalid() || state.running);
        if let Some(on_run) = &self.on_run {
            let on_run = Rc::clone(on_run);
            let dialog_edit = Rc::clone(&self.dialog_edit);
            run = run.on_press(move |s: &mut S| {
                // `if (!tunnelId || invalid || running) return;
                // setRunning(true); …`.
                let request = {
                    let dialog = dialog_edit(s);
                    if !dialog.run_gate() {
                        return;
                    }
                    let request = dialog.run_request();
                    if request.is_some() {
                        dialog.begin_run();
                    }
                    request
                };
                if let Some((id, host, port)) = request {
                    on_run(s, &id, &host, port);
                }
            });
        }
        let footer = row()
            .key("tunnel-test-footer")
            .w_full()
            .gap(8.0)
            .justify_end()
            .child(close.build(k).key("tunnel-test-close"))
            .child(run.build(k).key("tunnel-test-run"));
        dialog = dialog.footer(footer);

        dialog.build(k, layer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> SavedTunnelSummary {
        SavedTunnelSummary {
            id: "t-1".to_owned(),
            name: "Prod Bastion".to_owned(),
            kind: SavedTunnelKind::Ssh,
        }
    }

    fn summary_kind(kind: SavedTunnelKind) -> SavedTunnelSummary {
        SavedTunnelSummary { kind, ..summary() }
    }

    fn opened() -> TunnelTestDialogState {
        let mut state = TunnelTestDialogState::default();
        state.open_changed(true, Some(&summary()));
        state
    }

    #[test]
    fn the_open_resets_the_form() {
        let mut state = opened();
        // A previous run's leftovers.
        state.host.text = "  10.0.0.5 ".to_owned();
        state.port.text = "5432".to_owned();
        state.running = true;
        state.elapsed_ms = Some(42);
        state.error = Some("stale".to_owned());
        state.open_changed(true, Some(&summary()));
        assert_eq!(state.host.text, "127.0.0.1", "host back to the default");
        assert_eq!(state.port.text, "", "port blank again");
        assert!(!state.running);
        assert_eq!(state.elapsed_ms, None);
        assert_eq!(state.error, None);
        // A close keeps the stale form (React's early return).
        state.port.text = "5432".to_owned();
        state.open_changed(false, None);
        assert_eq!(state.port.text, "5432");
        assert!(!state.open);
        // The default state matches the React initial state.
        let fresh = TunnelTestDialogState::default();
        assert_eq!(fresh.host.text, "127.0.0.1");
        assert_eq!(fresh.port.text, "");
    }

    #[test]
    fn the_port_guard() {
        for (value, ok) in [
            ("5432", true),
            (" 5432 ", true),
            ("65535", true),
            ("1e2", true), // Number("1e2") is the integer 100
            ("", false),
            ("   ", false),
            ("0", false),
            ("-1", false),
            ("65536", false),
            ("12.5", false),
            ("abc", false),
        ] {
            assert_eq!(is_valid_port(value), ok, "port {value:?}");
        }
    }

    #[test]
    fn the_target_hint_prefers_the_host() {
        let mut state = opened();
        state.host.text = "".to_owned();
        state.port.text = "5432".to_owned();
        assert_eq!(state.target_hint(), Some("settings.tunnels.test.hostRequired"));
        state.host.text = "db.internal".to_owned();
        state.port.text = "".to_owned();
        assert_eq!(state.target_hint(), Some("settings.tunnels.test.portRequired"));
        state.port.text = "5432".to_owned();
        assert_eq!(state.target_hint(), None);
        // `invalid` is the same pair.
        assert!(!state.invalid());
        state.host.text = " ".to_owned();
        assert!(state.invalid());
    }

    #[test]
    fn the_run_gate_refuses_every_refusal() {
        let mut state = opened();
        state.port.text = "5432".to_owned();
        assert!(state.run_gate(), "a valid form runs");
        // No tunnel.
        state.tunnel = None;
        assert!(!state.run_gate());
        state = opened();
        state.port.text = "5432".to_owned();
        // An invalid form (blank host).
        state.host.text = " ".to_owned();
        assert!(!state.run_gate());
        // A run in flight.
        state = opened();
        state.port.text = "5432".to_owned();
        state.begin_run();
        assert!(state.running);
        assert!(!state.run_gate(), "no double run");
    }

    #[test]
    fn the_run_request_trims_the_host_and_parses_the_port() {
        let mut state = opened();
        state.host.text = "  10.0.0.9  ".to_owned();
        state.port.text = "5432".to_owned();
        assert_eq!(state.run_request().map(|(_, h, p)| (h, p)), Some(("10.0.0.9".to_owned(), 5432)));
        // React's Number(port): exponent forms are integers.
        state.port.text = "1e2".to_owned();
        assert_eq!(state.run_request().map(|(_, _, p)| p), Some(100));
        // The gate refuses reach through.
        state.host.text = "   ".to_owned();
        assert_eq!(state.run_request(), None);
        state.host.text = "db".to_owned();
        state.port.text = "abc".to_owned();
        assert_eq!(state.run_request(), None);
    }

    #[test]
    fn the_answers_settle_the_run() {
        let mut state = opened();
        state.port.text = "5432".to_owned();
        state.begin_run();
        state.note_success(1234);
        assert!(!state.running);
        assert_eq!(state.elapsed_ms, Some(1234));
        assert_eq!(
            state.success_line(),
            Some(t("settings.tunnels.test.success").replace("{ms}", "1234"))
        );
        // A rerun clears the previous success.
        state.begin_run();
        assert_eq!(state.elapsed_ms, None);
        assert_eq!(state.success_line(), None);
        state.note_failure("connect ECONNREFUSED");
        assert!(!state.running);
        assert_eq!(state.error.as_deref(), Some("connect ECONNREFUSED"));
        assert_eq!(state.elapsed_ms, None);
    }

    #[test]
    fn a_blank_failure_falls_back_to_the_generic_sentence() {
        let mut state = opened();
        state.begin_run();
        // `messageOf(e) ?? t('common.operationFailed')`.
        state.note_failure("");
        assert_eq!(state.error.as_deref(), Some(t("common.operationFailed")));
        assert!(!state.running);
    }

    #[test]
    fn the_scope_key_follows_the_kind() {
        let mut state = TunnelTestDialogState::default();
        assert_eq!(state.scope_key(), "settings.tunnels.test.scope.websocket");
        state.open_changed(true, Some(&summary_kind(SavedTunnelKind::Ssh)));
        assert_eq!(state.scope_key(), "settings.tunnels.test.scope.ssh");
        state.open_changed(true, Some(&summary_kind(SavedTunnelKind::HttpProxy)));
        assert_eq!(state.scope_key(), "settings.tunnels.test.scope.httpProxy");
        state.open_changed(true, Some(&summary_kind(SavedTunnelKind::WebSocket)));
        assert_eq!(state.scope_key(), "settings.tunnels.test.scope.websocket");
        // A missing tunnel keeps the last arm.
        state.open_changed(true, None);
        assert_eq!(state.scope_key(), "settings.tunnels.test.scope.websocket");
    }
}
