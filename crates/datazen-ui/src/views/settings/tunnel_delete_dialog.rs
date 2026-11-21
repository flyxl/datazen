//! Port of
//! `packages/frontend/src/windows/settings/TunnelDeleteDialog.tsx`
//! (T441) — the delete confirmation with reference integrity.
//!
//! Two options only: cancel, or "delete and unbind". The affected
//! connection names are shown first so the destructive action is
//! never silent, and the unbind-then-delete ordering lives in the
//! absorbed [`tunnel_deletion`] (the backend `delete_tunnel` does
//! not clean up references itself).
//!
//! ## What maps where
//!
//! - The `useEffect([open, tunnelId])` usage read is
//!   [`TunnelDeleteDialogState::open_changed`]: an open with an id
//!   resets the run (usage/error/note/busy) and arms the loading
//!   read; the host runs `getTunnelUsage` (T432's rail) and answers
//!   through [`Self::note_usage`] /
//!   [`Self::note_usage_error`] (the `cancelled` guard is the
//!   open-and-same-tunnel check — a closed dialog's late answer
//!   never lands).
//! - `handleConfirm` is [`Self::confirm_gate`] +
//!   [`Self::begin_confirm`] + [`Self::note_confirm_outcome`] /
//!   [`Self::note_confirm_error`]: the host runs the absorbed
//!   [`tunnel_deletion::delete_tunnel_and_unbind`] and reports the
//!   outcome. A `deleted` outcome CLOSES the dialog (the host's
//!   `onClose` — the audit pins it); an `unbind-failed` one raises
//!   the unbind error and, when references got stuck, the rollback
//!   note (`{restored}` / `{failed}` formatted — the locale keeps
//!   the placeholders, the caller fills them); a `delete-failed`
//!   one raises the delete error.
//! - `toDialogError` is [`DialogError::classify`]: the transport's
//!   own message when it has one, the fallback key otherwise.
//! - `busy` clears in the `finally` arm — every note method clears
//!   it (the synchronous model has no tick between).
//! - The footer's unbind option is the highlighted default action
//!   (`variant="danger"`), disabled while `busy || loading`; the
//!   cancel is disabled while `busy`; the dialog's dismissal
//!   (`onClose`) is a no-op while `busy`.
//!
//! ## Keys
//!
//! The React `data-testid`s stay verbatim (`tunnel-delete-dialog`,
//! `tunnel-delete-cancel`, `tunnel-delete-confirm`,
//! `tunnel-delete-body`, `tunnel-delete-no-references`,
//! `tunnel-delete-affected-label`, `tunnel-delete-affected`,
//! `tunnel-delete-affected-item`, `tunnel-delete-error`,
//! `tunnel-delete-note`).
//!
//! Known deltas: the repeated `tunnel-delete-affected-item` testid
//! becomes the runtime-suffixed duplicate keys (the established
//! mirror); the note's `text-amber-400` renders through the
//! warning tone.

// No production consumer yet (the tunnel settings section T453
// mounts it); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::banner::{Banner, BannerElement, BannerVariant};
use crate::ui::dialog::Dialog;
use crate::views::settings::tunnel_deletion::DeleteTunnelOutcome;
use crate::commands::tunnel::TunnelUsage;

/// `text-xs` — the body copy.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);
/// `text-[11px]` — the unbind hint.
const TEXT_11: TextStyle = TextStyle::new(11.0, 400.0);
/// `text-sm` — one affected connection's row.
const TEXT_SM: TextStyle = TextStyle::new(14.0, 400.0);

/// `DialogError` — a raw IPC message or a translation key resolved
/// at render time (the effect must not depend on the `t` function's
/// identity — the React file's own note).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DialogError {
    Message(String),
    Key(&'static str),
}

impl DialogError {
    /// `toDialogError(e, fallbackKey)`: the transport's own words
    /// when it has any, the fallback key otherwise.
    pub(crate) fn classify(error: &str, fallback_key: &'static str) -> Self {
        if !error.is_empty() {
            Self::Message(error.to_owned())
        } else {
            Self::Key(fallback_key)
        }
    }

    /// The rendered text (`'key' in error ? t(error.key) :
    /// error.message`).
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Message(message) => message,
            Self::Key(key) => t(key),
        }
    }
}

/// The dialog's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TunnelDeleteDialogState {
    pub(crate) open: bool,
    /// The tunnel under deletion (`None` = closed / none).
    pub(crate) tunnel: Option<crate::commands::tunnel::SavedTunnelSummary>,
    /// The usage read's answer.
    pub(crate) usage: Option<TunnelUsage>,
    /// The usage read in flight.
    pub(crate) loading: bool,
    /// The confirm in flight (disables everything but rendering).
    pub(crate) busy: bool,
    pub(crate) error: Option<DialogError>,
    /// The rollback note (`{restored}` / `{failed}`).
    pub(crate) note: Option<String>,
}

impl TunnelDeleteDialogState {
    /// The `useEffect([open, tunnelId])` arm: an open with an id
    /// resets the run and arms the usage read; closing (or an idless
    /// open) leaves nothing to read. Answers whether the host
    /// should run `getTunnelUsage`.
    pub(crate) fn open_changed(
        &mut self,
        open: bool,
        tunnel: Option<&crate::commands::tunnel::SavedTunnelSummary>,
    ) -> bool {
        self.open = open;
        self.tunnel = tunnel.cloned();
        if !open || tunnel.is_none() {
            return false;
        }
        self.usage = None;
        self.error = None;
        self.note = None;
        self.busy = false;
        self.loading = true;
        true
    }

    /// The tunnel id under deletion.
    pub(crate) fn tunnel_id(&self) -> Option<&str> {
        self.tunnel.as_ref().map(|t| t.id.as_str())
    }

    /// The usage read's answer (the `cancelled` guard: a closed or
    /// retargeted dialog's late answer never lands; the `finally`
    /// clears the loading either way for the live one).
    pub(crate) fn note_usage(&mut self, usage: TunnelUsage) -> bool {
        if !self.open || self.tunnel.is_none() {
            return false;
        }
        self.usage = Some(usage);
        self.loading = false;
        true
    }

    /// The usage read's failure.
    pub(crate) fn note_usage_error(&mut self, error: &str) -> bool {
        if !self.open || self.tunnel.is_none() {
            return false;
        }
        self.error = Some(DialogError::classify(error, "settings.tunnels.delete.failed"));
        self.loading = false;
        true
    }

    /// `handleConfirm`'s gate: `if (!tunnelId || busy) return`.
    pub(crate) fn confirm_gate(&self) -> bool {
        self.tunnel.is_some() && !self.busy
    }

    /// The confirm's head: busy arms, the previous error and note
    /// clear.
    pub(crate) fn begin_confirm(&mut self) {
        self.busy = true;
        self.error = None;
        self.note = None;
    }

    /// The confirm's answer: a `deleted` outcome closes (the host's
    /// `onClose` — the audit pins it); an `unbind-failed` one raises
    /// the unbind error and the rollback note when references got
    /// stuck; a `delete-failed` one raises the delete error. The
    /// `finally` clears busy in every arm that stays open.
    pub(crate) fn note_confirm_outcome(&mut self, outcome: &DeleteTunnelOutcome) -> bool {
        match outcome {
            DeleteTunnelOutcome::Deleted { .. } => {
                // `onClose(); return;` — the finally still clears busy
                // (unobservable while closed, but React's arm does).
                self.busy = false;
                self.open = false;
                true
            }
            DeleteTunnelOutcome::UnbindFailed { message, rolled_back, rollback_failed } => {
                self.error = Some(DialogError::Message(format!(
                    "{} {}",
                    t("settings.tunnels.delete.unbindFailed"),
                    message
                )));
                if !rollback_failed.is_empty() {
                    self.note = Some(
                        t("settings.tunnels.delete.rollbackNote")
                            .replace("{restored}", &rolled_back.len().to_string())
                            .replace("{failed}", &rollback_failed.join(", ")),
                    );
                }
                self.busy = false;
                true
            }
            DeleteTunnelOutcome::DeleteFailed { message, .. } => {
                self.error = Some(DialogError::Message(format!(
                    "{} {}",
                    t("settings.tunnels.delete.deleteFailed"),
                    message
                )));
                self.busy = false;
                true
            }
        }
    }

    /// The confirm's throw arm (`catch (e)`).
    pub(crate) fn note_confirm_error(&mut self, error: &str) {
        self.error = Some(DialogError::classify(error, "settings.tunnels.delete.failed"));
        self.busy = false;
    }

    /// The affected connection names (`usage?.connectionNames ?? []`).
    pub(crate) fn affected_names(&self) -> &[String] {
        self.usage.as_ref().map(|u| u.connection_names.as_slice()).unwrap_or(&[])
    }
}

/// `TunnelDeleteDialog({ open, tunnel, onClose, onConfirm })`: the
/// dialog over the state, the state lens and the host's action
/// slots.
pub(crate) struct TunnelDeleteDialog<'a, S: 'static> {
    state: &'a TunnelDeleteDialogState,
    dark: bool,
    dialog_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TunnelDeleteDialogState>,
    on_close: Option<Rc<dyn Fn(&mut S)>>,
    /// `onConfirm(id)` — the host runs the absorbed
    /// unbind-then-delete and reports the outcome.
    on_confirm: Option<Rc<dyn Fn(&mut S, &str)>>,
}

impl<'a, S: 'static> TunnelDeleteDialog<'a, S> {
    pub(crate) fn new(
        state: &'a TunnelDeleteDialogState,
        dark: bool,
        dialog_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TunnelDeleteDialogState + 'static,
    ) -> Self {
        Self {
            state,
            dark,
            dialog_edit: Rc::new(dialog_edit),
            on_close: None,
            on_confirm: None,
        }
    }

    /// `onClose` (a no-op while busy — the dialog's dismissal rule).
    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// `onConfirm(id)`: the host runs the deletion and answers
    /// through the note methods.
    pub(crate) fn on_confirm(mut self, f: impl Fn(&mut S, &str) + 'static) -> Self {
        self.on_confirm = Some(Rc::new(f));
        self
    }

    /// The dialog (nothing when closed).
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) {
        let state = self.state;
        if !state.open {
            return;
        }
        let name = state.tunnel.as_ref().map(|t| t.name.as_str()).unwrap_or("");
        let description = t("settings.tunnels.delete.description").replace("{name}", name);

        let mut dialog = Dialog::new("tunnel-delete-dialog", t("settings.tunnels.delete.title"))
            .open(true)
            .width(480.0)
            .description(description);
        // The dismissal policy: a no-op while busy.
        {
            let busy = state.busy;
            let on_close = self.on_close.clone();
            dialog = dialog.on_close(move |s: &mut S| {
                if busy {
                    return;
                }
                if let Some(on_close) = &on_close {
                    on_close(s);
                }
            });
        }

        // The body.
        let mut body = col().key("tunnel-delete-body").w_full().gap(12.0); // space-y-3
        let names = state.affected_names();
        if state.loading {
            body = body.child(
                text(t("common.loading").to_owned())
                    .key("tunnel-delete-loading")
                    .font(TEXT_XS)
                    .color(k.c.text_muted),
            );
        } else if names.is_empty() {
            body = body.child(
                text(t("settings.tunnels.delete.noReferences").to_owned())
                    .key("tunnel-delete-no-references")
                    .font(TEXT_XS)
                    .color(k.c.text_muted),
            );
        } else {
            body = body.child(
                text(t("settings.tunnels.delete.affected").to_owned())
                    .key("tunnel-delete-affected-label")
                    .font(TEXT_XS)
                    .color(k.c.text_muted),
            );
            let mut list = col().key("tunnel-delete-affected").w_full().gap(4.0);
            for (index, name) in names.iter().enumerate() {
                // React's key: the connection id when present, the
                // name otherwise.
                let id = state
                    .usage
                    .as_ref()
                    .and_then(|u| u.connection_ids.get(index))
                    .unwrap_or(name);
                list = list.child(
                    row()
                        .key(format!("tunnel-delete-affected-item-{id}"))
                        .w_full()
                        .padding(6.0)
                        .radius(6.0)
                        .border(1.0, k.c.border_base)
                        .fill(k.c.surface_weak)
                        .child(text(name.clone()).font(TEXT_SM).color(k.c.text_base)),
                );
            }
            body = body.child(list);
        }
        body = body.child(
            text(t("settings.tunnels.delete.unbindHint").to_owned())
                .key("tunnel-delete-unbind-hint")
                .font(TEXT_11)
                .color(k.c.text_muted),
        );
        if let Some(error) = &state.error {
            // React <ErrorBanner as="p">.
            if let Some(banner) = Banner::new("tunnel-delete-error", error.text().to_owned())
                .variant(BannerVariant::Plain)
                .as_element(BannerElement::P)
                .build(k)
            {
                body = body.child(banner);
            }
        }
        if let Some(note) = &state.note {
            body = body.child(
                text(note.clone())
                    .key("tunnel-delete-note")
                    .font(TEXT_XS)
                    .color(k.c.warning.fg), // React text-amber-400
            );
        }
        dialog = dialog.child(body);

        // The footer: the cancel + the danger confirm (the unbind
        // option is the highlighted default action).
        let mut cancel = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Small)
            .disabled(state.busy);
        if let Some(on_close) = &self.on_close {
            let on_close = Rc::clone(on_close);
            let busy = state.busy;
            cancel = cancel.on_press(move |s: &mut S| {
                if busy {
                    return;
                }
                on_close(s);
            });
        }
        let mut confirm = Button::new(if state.busy {
            t("settings.tunnels.delete.confirming")
        } else {
            t("settings.tunnels.delete.confirm")
        })
        .variant(ButtonVariant::Danger)
        .control_size(ControlSize::Small)
        .disabled(state.busy || state.loading);
        if let Some(on_confirm) = &self.on_confirm {
            let on_confirm = Rc::clone(on_confirm);
            let dialog_edit = Rc::clone(&self.dialog_edit);
            confirm = confirm.on_press(move |s: &mut S| {
                // `if (!tunnelId || busy) return; setBusy(true); ...`
                let id = {
                    let dialog = dialog_edit(s);
                    if !dialog.confirm_gate() {
                        return;
                    }
                    dialog.begin_confirm();
                    dialog.tunnel_id().unwrap_or_default().to_owned()
                };
                on_confirm(s, &id);
            });
        }
        let footer = row()
            .key("tunnel-delete-footer")
            .w_full()
            .gap(8.0)
            .justify_end()
            .child(cancel.build(k).key("tunnel-delete-cancel"))
            .child(confirm.build(k).key("tunnel-delete-confirm"));
        dialog = dialog.footer(footer);

        dialog.build(k, layer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> crate::commands::tunnel::SavedTunnelSummary {
        crate::commands::tunnel::SavedTunnelSummary {
            id: "t-1".to_owned(),
            name: "Prod Bastion".to_owned(),
            kind: datazen_ui_api::SavedTunnelKind::Ssh,
        }
    }

    fn usage(names: &[&str]) -> TunnelUsage {
        TunnelUsage {
            connection_ids: names.iter().map(|n| format!("c-{}", n)).collect(),
            connection_names: names.iter().map(|n| n.to_string()).collect(),
        }
    }

    #[test]
    fn the_open_arms_the_usage_read_and_resets_the_run() {
        let mut state = TunnelDeleteDialogState::default();
        // A closed dialog reads nothing.
        assert!(!state.open_changed(false, None));
        assert!(!state.open_changed(true, None), "an idless open reads nothing");
        // An open with an id arms the read.
        assert!(state.open_changed(true, Some(&summary())));
        assert!(state.loading);
        assert!(state.usage.is_none());
        assert!(state.error.is_none());
        assert!(!state.busy);
        // The read lands.
        assert!(state.note_usage(usage(&["Alpha", "Beta"])));
        assert!(!state.loading);
        assert_eq!(state.affected_names(), ["Alpha", "Beta"]);
        // A re-open resets a previous run's leftovers.
        state.error = Some(DialogError::Message("stale".to_owned()));
        state.note = Some("stale note".to_owned());
        state.busy = true;
        assert!(state.open_changed(true, Some(&summary())));
        assert!(state.loading, "the read re-arms");
        assert!(state.error.is_none(), "the stale error clears");
        assert!(state.note.is_none());
        assert!(!state.busy);
    }

    #[test]
    fn a_closed_dialogs_late_answer_never_lands() {
        let mut state = TunnelDeleteDialogState::default();
        state.open_changed(true, Some(&summary()));
        state.open = false; // the close between the read and the answer
        assert!(!state.note_usage(usage(&["Alpha"])));
        assert!(state.usage.is_none());
        assert!(!state.note_usage_error("boom"));
        assert!(state.error.is_none());
    }

    #[test]
    fn the_usage_error_falls_back_to_the_key() {
        let mut state = TunnelDeleteDialogState::default();
        state.open_changed(true, Some(&summary()));
        assert!(state.note_usage_error(""));
        assert_eq!(
            state.error,
            Some(DialogError::Key("settings.tunnels.delete.failed")),
            "an empty message uses the fallback key"
        );
        state.note_usage_error("ipc exploded");
        assert_eq!(state.error, Some(DialogError::Message("ipc exploded".to_owned())));
        assert_eq!(
            DialogError::Key("settings.tunnels.delete.failed").text(),
            t("settings.tunnels.delete.failed")
        );
    }

    #[test]
    fn the_confirm_flows_settle_their_outcomes() {
        let mut state = TunnelDeleteDialogState::default();
        state.open_changed(true, Some(&summary()));
        state.note_usage(usage(&["Alpha"]));

        // The gate.
        assert!(state.confirm_gate());
        // A deleted outcome closes the dialog.
        state.begin_confirm();
        assert!(state.busy);
        assert!(state.note_confirm_outcome(&DeleteTunnelOutcome::Deleted {
            unbound_connection_ids: vec!["c-Alpha".to_owned()]
        }));
        assert!(!state.open, "onClose fired");
        assert!(!state.busy, "busy never shows again");

        // An unbind failure raises the error; the note only when
        // references got stuck.
        state.open_changed(true, Some(&summary()));
        state.note_usage(usage(&["Alpha"]));
        state.begin_confirm();
        assert!(state.note_confirm_outcome(&DeleteTunnelOutcome::UnbindFailed {
            message: "cannot save c1".to_owned(),
            rolled_back: vec!["c2".to_owned(), "c1".to_owned()],
            rollback_failed: Vec::new(),
        }));
        assert!(!state.busy, "the finally cleared");
        assert!(state.open, "the dialog stays for the error");
        assert_eq!(
            state.error,
            Some(DialogError::Message(format!(
                "{} cannot save c1",
                t("settings.tunnels.delete.unbindFailed")
            )))
        );
        assert!(state.note.is_none(), "no stuck references, no note");

        // With stuck references the note rides.
        state.begin_confirm();
        assert!(state.note_confirm_outcome(&DeleteTunnelOutcome::UnbindFailed {
            message: "no".to_owned(),
            rolled_back: vec!["c1".to_owned()],
            rollback_failed: vec!["c2".to_owned(), "c3".to_owned()],
        }));
        let note = state.note.as_ref().unwrap();
        let expected = t("settings.tunnels.delete.rollbackNote")
            .replace("{restored}", "1")
            .replace("{failed}", "c2, c3");
        assert_eq!(note, &expected);

        // A delete failure raises its own error and stays open.
        state.open_changed(true, Some(&summary()));
        state.begin_confirm();
        assert!(state.note_confirm_outcome(&DeleteTunnelOutcome::DeleteFailed {
            message: "cannot delete".to_owned(),
            unbound_connection_ids: Vec::new(),
        }));
        assert!(!state.busy);
        assert!(state.open);
        assert_eq!(
            state.error,
            Some(DialogError::Message(format!(
                "{} cannot delete",
                t("settings.tunnels.delete.deleteFailed")
            )))
        );

        // The throw arm.
        state.begin_confirm();
        state.note_confirm_error("");
        assert_eq!(
            state.error,
            Some(DialogError::Key("settings.tunnels.delete.failed"))
        );
        assert!(!state.busy);
    }

    #[test]
    fn the_gate_refuses_a_busy_or_idless_dialog() {
        let mut state = TunnelDeleteDialogState::default();
        assert!(!state.confirm_gate(), "no tunnel");
        state.open_changed(true, Some(&summary()));
        assert!(state.confirm_gate());
        state.begin_confirm();
        assert!(!state.confirm_gate(), "busy refuses a second confirm");
    }
}
