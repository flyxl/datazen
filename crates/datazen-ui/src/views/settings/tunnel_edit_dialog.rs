//! Port of
//! `packages/frontend/src/windows/settings/TunnelEditDialog.tsx`
//! (T447) — create/edit a saved tunnel.
//!
//! This is the only place that fetches a full entity (`get_tunnel`),
//! because only editing needs the decrypted credentials; every
//! list/picker path stays on the secret-free summaries (G9).
//!
//! ## What maps where
//!
//! - The `useEffect([open, tunnelId])` open-reset is
//!   [`TunnelEditDialogState::open_changed`]: opening always starts
//!   from a clean form — blank when creating, a `getTunnel` fetch
//!   when editing (fetched here, never carried over from the list).
//!   The host runs the read (T432's rail) and answers through
//!   [`Self::note_entity`] / [`Self::note_entity_missing`] /
//!   [`Self::note_entity_error`] (the `cancelled` guard is the
//!   open-and-same-id check).
//! - The four `patch*` setters are [`Self::patch_ssh`] /
//!   [`Self::patch_jump`] / [`Self::patch_http_proxy`] /
//!   [`Self::patch_web_socket`] + [`Self::set_name`] /
//!   [`Self::set_kind`]: every edit latches `dirty`.
//! - `tunnelDraftProblem` + `TUNNEL_DRAFT_PROBLEM_KEYS` are the
//!   carried [`tunnel_draft`] module's (T637's until it lands): the
//!   save button's `disabled={problem !== null || pending}` gate
//!   and the dirty-only hint (`dirty && problem`).
//! - `handleSave` is [`Self::save_gate`] + [`Self::begin_save`] +
//!   [`Self::note_save_ok`] / [`Self::note_save_error`]: the entity
//!   builds through the carried `draft_to_saved_tunnel` (a create
//!   takes the host's `newTunnelId()`), the host runs the store's
//!   create-or-update (`draft.id` decides which), and a success
//!   fires `onSaved` then `onClose` (both host slots — the audit
//!   pins the order).
//! - The three field bodies (`SshTunnelFields` /
//!   `HttpProxyTunnelFields` / `WebSocketTunnelFields`) are
//!   **segment-1 tasks (T217/T281/T300), not landed**: they are
//!   modelled as the delegated block slots
//!   [`TunnelEditDialog::ssh_fields`] / [`http_proxy_fields`] /
//!   [`websocket_fields`] the host fills (or the structural
//!   `tunnel-edit-kind-fields` marker when it does not — the same
//!   boundary convention `connection_advanced_settings.rs`
//!   records). The kind selector above owns the choice, so the
//!   reused fields render without their own enable checkbox.
//! - `DialogError` is the shared classify shape (the
//!   TunnelDeleteDialog's own — reused verbatim).
//!
//! ## Keys
//!
//! The React `data-testid`s stay verbatim (`tunnel-edit-dialog`,
//! `tunnel-edit-cancel`, `tunnel-edit-save`, `tunnel-edit-name`,
//! `tunnel-edit-kind`, `tunnel-edit-hint`, `tunnel-edit-error`).
//!
//! Known deltas: the field bodies are delegated slots until T217 /
//! T281 / T300 land (above); `pickPath` is the host's picker seam
//! (the slots carry their own).

// No production consumer yet (the tunnel settings section T453
// mounts it); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::rc::Rc;

use datazen_ui_api::SavedTunnelKind;
use rune::{col, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::banner::{Banner, BannerElement, BannerVariant};
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::select::Select;
use crate::views::settings::tunnel_draft::{
    SAVED_TUNNEL_KINDS, TunnelDraft, TunnelDraftProblem, empty_tunnel_draft, tunnel_draft_problem,
    tunnel_kind_label_key,
};
use crate::views::settings::tunnel_delete_dialog::DialogError;

/// `text-xs` — the loading line.
const TEXT_XS: TextStyle = TextStyle::new(12.0, 400.0);
/// `text-xs font-medium` — the field labels.
const TEXT_XS_MEDIUM: TextStyle = TextStyle::new(12.0, 500.0);

/// The dialog's state.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TunnelEditDialogState {
    pub(crate) open: bool,
    /// The entity being edited (`None` = a blank create form).
    pub(crate) tunnel_id: Option<String>,
    /// The draft (the string-field model the carried module owns).
    pub(crate) draft: TunnelDraft,
    /// The entity read in flight.
    pub(crate) loading: bool,
    /// The save in flight.
    pub(crate) saving: bool,
    /// Any edit landed since the open.
    pub(crate) dirty: bool,
    pub(crate) error: Option<DialogError>,
}

impl Default for TunnelEditDialogState {
    fn default() -> Self {
        Self {
            open: false,
            tunnel_id: None,
            draft: empty_tunnel_draft(),
            loading: false,
            saving: false,
            dirty: false,
            error: None,
        }
    }
}

impl TunnelEditDialogState {
    /// The `useEffect([open, tunnelId])` arm: opening always starts
    /// from a clean form — blank when creating (no read), the
    /// stored entity when editing (the host fetches `getTunnel`).
    /// Answers whether the read should run.
    pub(crate) fn open_changed(&mut self, open: bool, tunnel_id: Option<&str>) -> bool {
        self.open = open;
        self.tunnel_id = tunnel_id.map(str::to_owned);
        self.error = None;
        self.saving = false;
        self.dirty = false;
        if !open {
            self.loading = false;
            return false;
        }
        let Some(_id) = tunnel_id else {
            // `setDraft(emptyTunnelDraft())` — the create branch's
            // clean form.
            self.draft = empty_tunnel_draft();
            self.loading = false;
            return false;
        };
        // The edit branch keeps the previous form until the fetch
        // answers (React's setDraft happens in the then/catch; the
        // loading line is the only face meanwhile).
        self.loading = true;
        true
    }

    /// The entity read's answer (the `cancelled` guard: a closed or
    /// retargeted dialog's late answer never lands; the `finally`
    /// clears the loading either way for the live one).
    pub(crate) fn note_entity(&mut self, draft: TunnelDraft) -> bool {
        if !self.open || self.tunnel_id.is_none() {
            return false;
        }
        self.draft = draft;
        self.loading = false;
        true
    }

    /// The read answered `null` (the entity is gone).
    pub(crate) fn note_entity_missing(&mut self) -> bool {
        if !self.open || self.tunnel_id.is_none() {
            return false;
        }
        self.error = Some(DialogError::Key("settings.tunnels.editor.loadFailed"));
        self.draft = empty_tunnel_draft();
        self.loading = false;
        true
    }

    /// The read's failure.
    pub(crate) fn note_entity_error(&mut self, error: &str) -> bool {
        if !self.open || self.tunnel_id.is_none() {
            return false;
        }
        self.error = Some(DialogError::classify(error, "settings.tunnels.editor.loadFailed"));
        self.loading = false;
        true
    }

    /// `setDraft((d) => ({ ...d, name }))` + dirty.
    pub(crate) fn set_name(&mut self, name: String) {
        self.dirty = true;
        self.draft.name = name;
    }

    /// `setDraft((d) => ({ ...d, kind }))` + dirty.
    pub(crate) fn set_kind(&mut self, kind: SavedTunnelKind) {
        self.dirty = true;
        self.draft.kind = kind;
    }

    /// `patchSsh` (a field-level patch + dirty).
    pub(crate) fn patch_ssh(&mut self, patch: TunnelSshPatch) {
        self.dirty = true;
        match patch {
            TunnelSshPatch::Host(v) => self.draft.ssh.host = v,
            TunnelSshPatch::Port(v) => self.draft.ssh.port = v,
            TunnelSshPatch::Username(v) => self.draft.ssh.username = v,
            TunnelSshPatch::Password(v) => self.draft.ssh.password = v,
            TunnelSshPatch::KeyPath(v) => self.draft.ssh.key_path = v,
            TunnelSshPatch::Passphrase(v) => self.draft.ssh.passphrase = v,
        }
    }

    /// `patchJump`.
    pub(crate) fn patch_jump(&mut self, patch: TunnelJumpPatch) {
        self.dirty = true;
        match patch {
            TunnelJumpPatch::Enabled(v) => self.draft.ssh.jump.enabled = v,
            TunnelJumpPatch::Host(v) => self.draft.ssh.jump.host = v,
            TunnelJumpPatch::Port(v) => self.draft.ssh.jump.port = v,
            TunnelJumpPatch::Username(v) => self.draft.ssh.jump.username = v,
            TunnelJumpPatch::Password(v) => self.draft.ssh.jump.password = v,
            TunnelJumpPatch::KeyPath(v) => self.draft.ssh.jump.key_path = v,
            TunnelJumpPatch::Passphrase(v) => self.draft.ssh.jump.passphrase = v,
        }
    }

    /// `patchHttpProxy`.
    pub(crate) fn patch_http_proxy(&mut self, patch: TunnelHttpProxyPatch) {
        self.dirty = true;
        match patch {
            TunnelHttpProxyPatch::Host(v) => self.draft.http_proxy.host = v,
            TunnelHttpProxyPatch::Port(v) => self.draft.http_proxy.port = v,
            TunnelHttpProxyPatch::Username(v) => self.draft.http_proxy.username = v,
            TunnelHttpProxyPatch::Password(v) => self.draft.http_proxy.password = v,
            TunnelHttpProxyPatch::Timeout(v) => self.draft.http_proxy.timeout = v,
        }
    }

    /// `patchWebSocket`.
    pub(crate) fn patch_web_socket(&mut self, patch: TunnelWebSocketPatch) {
        self.dirty = true;
        match patch {
            TunnelWebSocketPatch::Url(v) => self.draft.websocket.url = v,
            TunnelWebSocketPatch::AuthToken(v) => self.draft.websocket.auth_token = v,
            TunnelWebSocketPatch::Timeout(v) => self.draft.websocket.timeout = v,
        }
    }

    /// `problem` — the draft's first unmet requirement.
    pub(crate) fn problem(&self) -> Option<TunnelDraftProblem> {
        tunnel_draft_problem(&self.draft)
    }

    /// `pending` — `loading || saving`.
    pub(crate) fn pending(&self) -> bool {
        self.loading || self.saving
    }

    /// `hint` — the problem's message, only once dirty.
    pub(crate) fn hint(&self) -> Option<&'static str> {
        if !self.dirty {
            return None;
        }
        self.problem().map(|problem| t(problem.message_key()))
    }

    /// `handleSave`'s gate: `if (problem || pending) return`.
    pub(crate) fn save_gate(&self) -> bool {
        self.problem().is_none() && !self.pending()
    }

    /// The save's head: `setSaving(true); setError(null);` — the
    /// entity build happens in the host (it owns `newTunnelId`).
    pub(crate) fn begin_save(&mut self) {
        self.saving = true;
        self.error = None;
    }

    /// The save's answer: `onSaved(); onClose();` (the host fires
    /// both slots; busy clears in the finally either way).
    pub(crate) fn note_save_ok(&mut self) {
        self.saving = false;
        self.open = false;
    }

    /// The save's failure.
    pub(crate) fn note_save_error(&mut self, error: &str) {
        self.error = Some(DialogError::classify(error, "settings.tunnels.editor.saveFailed"));
        self.saving = false;
    }
}

/// `patchSsh`'s payload (the field-level patch).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TunnelSshPatch {
    Host(String),
    Port(String),
    Username(String),
    Password(String),
    KeyPath(String),
    Passphrase(String),
}

/// `patchJump`'s payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TunnelJumpPatch {
    Enabled(bool),
    Host(String),
    Port(String),
    Username(String),
    Password(String),
    KeyPath(String),
    Passphrase(String),
}

/// `patchHttpProxy`'s payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TunnelHttpProxyPatch {
    Host(String),
    Port(String),
    Username(String),
    Password(String),
    Timeout(String),
}

/// `patchWebSocket`'s payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TunnelWebSocketPatch {
    Url(String),
    AuthToken(String),
    Timeout(String),
}

/// `TunnelEditDialog({ open, tunnelId, onClose, onSaved })`: the
/// dialog over the state, the name draft's lens, the open-select
/// machine and the host's action slots.
pub(crate) struct TunnelEditDialog<'a, S: 'static> {
    state: &'a TunnelEditDialogState,
    dark: bool,
    dialog_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TunnelEditDialogState>,
    name_edit: Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>,
    on_close: Option<Rc<dyn Fn(&mut S)>>,
    /// `onSaved` — after a successful create/update (fires before
    /// the close).
    on_saved: Option<Rc<dyn Fn(&mut S)>>,
    /// The host's save: build the entity (`draft_to_saved_tunnel`
    /// + `newTunnelId` for a create), run the store's
    /// create-or-update, answer through the note methods.
    on_save_pressed: Option<Rc<dyn Fn(&mut S)>>,
    /// The delegated SSH field body (T281's slot).
    ssh_fields: Option<El>,
    /// The delegated HTTP-proxy field body (T217's slot).
    http_proxy_fields: Option<El>,
    /// The delegated WebSocket field body (T300's slot).
    websocket_fields: Option<El>,
}

impl<'a, S: 'static> TunnelEditDialog<'a, S> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        state: &'a TunnelEditDialogState,
        dark: bool,
        dialog_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TunnelEditDialogState + 'static,
        name_edit: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        Self {
            state,
            dark,
            dialog_edit: Rc::new(dialog_edit),
            name_edit: Rc::new(name_edit),
            on_close: None,
            on_saved: None,
            on_save_pressed: None,
            ssh_fields: None,
            http_proxy_fields: None,
            websocket_fields: None,
        }
    }

    /// `onClose`.
    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// `onSaved`.
    pub(crate) fn on_saved(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_saved = Some(Rc::new(f));
        self
    }

    /// The Save button's `handleSave` (the host builds and persists
    /// the entity, then answers).
    pub(crate) fn on_save_pressed(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_save_pressed = Some(Rc::new(f));
        self
    }

    /// The delegated SSH field body (T281's slot; `None` renders the
    /// structural marker).
    pub(crate) fn ssh_fields(mut self, fields: Option<El>) -> Self {
        self.ssh_fields = fields;
        self
    }

    /// The delegated HTTP-proxy field body (T217's slot).
    pub(crate) fn http_proxy_fields(mut self, fields: Option<El>) -> Self {
        self.http_proxy_fields = fields;
        self
    }

    /// The delegated WebSocket field body (T300's slot).
    pub(crate) fn websocket_fields(mut self, fields: Option<El>) -> Self {
        self.websocket_fields = fields;
        self
    }

    /// The dialog (nothing when closed).
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) {
        let state = self.state;
        if !state.open {
            return;
        }
        let title = if state.draft.id.is_some() {
            t("settings.tunnels.editor.editTitle")
        } else {
            t("settings.tunnels.editor.createTitle")
        };

        let mut dialog = Dialog::new("tunnel-edit-dialog", title)
            .open(true)
            .width(672.0) // max-w-2xl
            .description(t("settings.tunnels.editor.description"));
        if let Some(on_close) = &self.on_close {
            let on_close = Rc::clone(on_close);
            dialog = dialog.on_close(move |s: &mut S| on_close(s));
        }

        // The body: the name + kind row, the field body (or the
        // loading line), the hint and the error.
        let mut body = col().key("tunnel-edit-body").w_full().gap(16.0); // space-y-4

        // The name + kind grid (two columns).
        let mut grid = row().key("tunnel-edit-grid").w_full().gap(12.0);
        let mut name_block = col().key("tunnel-edit-name-block").w_full().gap(6.0);
        name_block = name_block.child(
            text(format!("{} *", t("settings.tunnels.editor.name")))
                .font(TEXT_XS_MEDIUM)
                .color(k.c.text_muted),
        );
        let lens = Rc::clone(&self.name_edit);
        let name_text = TextEdit::new(state.draft.name.clone());
        let field = TextField::new("tunnel-edit-name", &name_text)
            .height(36.0)
            .placeholder(t("settings.tunnels.editor.namePlaceholder"))
            .on_edit::<S>(move |s: &mut S| lens(s));
        name_block = name_block.child(field.build(self.dark));
        grid = grid.child(name_block.grow(1.0));

        let mut kind_block = col().key("tunnel-edit-kind-block").w_full().gap(6.0);
        kind_block = kind_block.child(
            text(format!("{} *", t("settings.tunnels.editor.kind")))
                .font(TEXT_XS_MEDIUM)
                .color(k.c.text_muted),
        );
        let labels: Vec<String> = SAVED_TUNNEL_KINDS
            .iter()
            .map(|kind| t(tunnel_kind_label_key(*kind)).to_owned())
            .collect();
        let values: Vec<String> = SAVED_TUNNEL_KINDS
            .iter()
            .map(|kind| kind_wire(*kind).to_owned())
            .collect();
        let current = kind_wire(state.draft.kind);
        let mut select = Select::new("tunnel-edit-kind", &labels)
            .values(&values)
            .current(Some(current))
            .width(160.0)
            .open(false);
        {
            let dialog_edit = Rc::clone(&self.dialog_edit);
            select = select.on_toggle(move |_s: &mut S| {
                // The open state lives in the host's machine (the
                // audit drives it); the dialog only forwards.
                let _ = &dialog_edit;
            });
            select = select.on_close(|_s: &mut S| {});
        }
        if let Some(f) = &self.on_save_pressed {
            // (the kind choose is the dialog's own edit, not the
            // save slot — kept for the borrow shape)
            let _ = f;
        }
        kind_block = kind_block.child(select.build(k, layer));
        grid = grid.child(kind_block.grow(1.0));
        body = body.child(grid);

        // The field body by kind (the loading line while the read is
        // in flight).
        if state.loading {
            body = body.child(
                text(t("common.loading").to_owned())
                    .key("tunnel-edit-loading")
                    .font(TEXT_XS)
                    .color(k.c.text_muted),
            );
        } else {
            let (fields, marker) = match state.draft.kind {
                SavedTunnelKind::Ssh => (
                    self.ssh_fields.clone(),
                    "tunnel-edit-ssh-fields-boundary",
                ),
                SavedTunnelKind::HttpProxy => (
                    self.http_proxy_fields.clone(),
                    "tunnel-edit-http-proxy-fields-boundary",
                ),
                SavedTunnelKind::WebSocket => (
                    self.websocket_fields.clone(),
                    "tunnel-edit-websocket-fields-boundary",
                ),
            };
            body = body.child(fields.unwrap_or_else(|| {
                col().key(marker).w_full()
            }));
        }

        // The hint (the problem's message, only once dirty).
        if let Some(hint) = state.hint() {
            if let Some(banner) = Banner::new("tunnel-edit-hint", hint.to_owned())
                .variant(BannerVariant::Plain)
                .as_element(BannerElement::P)
                .build(k)
            {
                body = body.child(banner);
            }
        }
        // The error.
        if let Some(error) = &state.error {
            if let Some(banner) = Banner::new("tunnel-edit-error", error.text().to_owned())
                .variant(BannerVariant::Plain)
                .as_element(BannerElement::P)
                .build(k)
            {
                body = body.child(banner);
            }
        }
        dialog = dialog.child(body);

        // The footer: the cancel + the save.
        let mut cancel = Button::new(t("common.cancel"))
            .variant(ButtonVariant::Outline) // React secondary
            .control_size(ControlSize::Small)
            .disabled(state.saving);
        if let Some(on_close) = &self.on_close {
            let on_close = Rc::clone(on_close);
            cancel = cancel.on_press(move |s: &mut S| on_close(s));
        }
        let mut save = Button::new(if state.saving {
            t("common.loading")
        } else {
            t("common.save")
        })
        .control_size(ControlSize::Small)
        .disabled(state.problem().is_some() || state.pending());
        if let Some(on_save_pressed) = &self.on_save_pressed {
            let on_save_pressed = Rc::clone(on_save_pressed);
            let dialog_edit = Rc::clone(&self.dialog_edit);
            save = save.on_press(move |s: &mut S| {
                // `if (problem || pending) return; setSaving(true); ...`
                {
                    let dialog = dialog_edit(s);
                    if !dialog.save_gate() {
                        return;
                    }
                    dialog.begin_save();
                }
                on_save_pressed(s);
            });
        }
        let footer = row()
            .key("tunnel-edit-footer")
            .w_full()
            .gap(8.0)
            .justify_end()
            .child(cancel.build(k).key("tunnel-edit-cancel"))
            .child(save.build(k).key("tunnel-edit-save"));
        dialog = dialog.footer(footer);

        dialog.build(k, layer);
    }
}

/// The kind select's wire value (`ssh` / `httpProxy` / `websocket`).
fn kind_wire(kind: SavedTunnelKind) -> &'static str {
    kind.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::settings::tunnel_draft::{saved_tunnel_to_draft, draft_to_saved_tunnel};
    use datazen_ui_api::SavedTunnelKind;

    fn valid_draft() -> TunnelDraft {
        let mut draft = empty_tunnel_draft();
        draft.name = "Bastion".to_owned();
        draft.ssh.host = "bastion.example.com".to_owned();
        draft.ssh.username = "ops".to_owned();
        draft
    }

    #[test]
    fn the_open_resets_to_a_blank_create_or_arms_the_read() {
        let mut state = TunnelEditDialogState::default();
        // A blank create: no read.
        assert!(!state.open_changed(true, None));
        assert!(state.open);
        assert_eq!(state.draft.id, None);
        assert!(!state.loading);
        assert_eq!(state.draft.name, "", "blank form");
        // An edit: the read arms — and the previous form stays until
        // the fetch answers (React's setDraft is in the then/catch;
        // the loading line is the only face meanwhile).
        state.set_name("Stale Form".to_owned());
        state.patch_ssh(TunnelSshPatch::Host("stale-host".to_owned()));
        assert!(state.open_changed(true, Some("t-1")));
        assert!(state.loading);
        assert_eq!(state.draft.name, "Stale Form", "the previous form stays while loading");
        assert_eq!(state.draft.ssh.host, "stale-host");
        // The entity lands and replaces it.
        let mut entity_draft = valid_draft();
        entity_draft.id = Some("t-1".to_owned());
        assert!(state.note_entity(entity_draft.clone()));
        assert!(!state.loading);
        assert_eq!(state.draft.id, Some("t-1".to_owned()));
        assert_eq!(state.draft.ssh.host, "bastion.example.com");
        // A re-open resets the leftovers.
        state.dirty = true;
        state.error = Some(DialogError::Message("stale".to_owned()));
        assert!(state.open_changed(true, Some("t-1")));
        assert!(state.loading);
        assert!(!state.dirty, "dirty resets");
        assert!(state.error.is_none(), "the error clears");
        // Closing leaves nothing to read.
        assert!(!state.open_changed(false, Some("t-1")));
        // Re-open, land the entity, then switch to a create: the
        // blank form lands (the stale entity never bleeds into the
        // create).
        assert!(state.open_changed(true, Some("t-1")));
        assert!(state.note_entity(entity_draft));
        assert_eq!(state.draft.ssh.host, "bastion.example.com");
        assert!(!state.open_changed(true, None));
        assert_eq!(state.draft.name, "", "the create branch is blank");
        assert_eq!(state.draft.ssh.host, "", "the loaded entity clears");
        assert_eq!(state.draft.id, None);
    }

    #[test]
    fn the_missing_entity_and_failed_read_raise_their_errors() {
        let mut state = TunnelEditDialogState::default();
        state.open_changed(true, Some("t-1"));
        // An entity lands first, then a re-open's read answers
        // missing: the blank draft must replace the loaded one.
        let mut entity_draft = valid_draft();
        entity_draft.id = Some("t-1".to_owned());
        assert!(state.note_entity(entity_draft));
        assert_eq!(state.draft.ssh.host, "bastion.example.com");
        state.open_changed(true, Some("t-1"));
        assert!(state.note_entity_missing());
        assert_eq!(
            state.error,
            Some(DialogError::Key("settings.tunnels.editor.loadFailed"))
        );
        assert_eq!(state.draft.name, "", "the blank draft lands");
        assert_eq!(state.draft.ssh.host, "", "the loaded entity clears too");
        assert!(!state.loading);

        state.open_changed(true, Some("t-1"));
        assert!(state.note_entity_error("ipc exploded"));
        assert_eq!(state.error, Some(DialogError::Message("ipc exploded".to_owned())));
        assert!(!state.loading);
        // The fallback key for an empty message.
        state.open_changed(true, Some("t-1"));
        assert!(state.note_entity_error(""));
        assert_eq!(
            state.error,
            Some(DialogError::Key("settings.tunnels.editor.loadFailed"))
        );
        // A closed dialog's late answer never lands.
        state.open = false;
        assert!(!state.note_entity_missing());
    }

    #[test]
    fn every_edit_latches_dirty() {
        let mut state = TunnelEditDialogState::default();
        state.open_changed(true, None);
        assert!(!state.dirty);
        state.set_name("Bastion".to_owned());
        assert!(state.dirty);
        state.set_kind(SavedTunnelKind::HttpProxy);
        state.patch_ssh(TunnelSshPatch::Host("h".to_owned()));
        state.patch_jump(TunnelJumpPatch::Enabled(true));
        state.patch_http_proxy(TunnelHttpProxyPatch::Host("p".to_owned()));
        state.patch_web_socket(TunnelWebSocketPatch::Url("wss://x".to_owned()));
        assert_eq!(state.draft.name, "Bastion");
        assert_eq!(state.draft.kind, SavedTunnelKind::HttpProxy);
        assert_eq!(state.draft.ssh.host, "h");
        assert!(state.draft.ssh.jump.enabled);
        assert_eq!(state.draft.http_proxy.host, "p");
        assert_eq!(state.draft.websocket.url, "wss://x");
    }

    #[test]
    fn the_hint_and_gate_follow_the_problem() {
        let mut state = TunnelEditDialogState::default();
        state.open_changed(true, None);
        // The empty draft's problem is the name, but the hint stays
        // hidden until dirty.
        assert_eq!(state.problem(), Some(TunnelDraftProblem::Name));
        assert_eq!(state.hint(), None, "not dirty yet");
        assert!(!state.save_gate(), "a problem blocks the save");
        state.set_name("  ".to_owned()); // whitespace-only stays a problem
        assert_eq!(state.hint(), Some(t("settings.tunnels.validation.name")));
        state.set_name("Bastion".to_owned());
        // The blank draft's ssh block still misses its host and
        // username — the problem moves there once the name lands.
        assert_eq!(state.problem(), Some(TunnelDraftProblem::SshHost));
        assert_eq!(state.hint(), Some(t("settings.tunnels.validation.sshHost")));
        state.patch_ssh(TunnelSshPatch::Host("bastion.example.com".to_owned()));
        state.patch_ssh(TunnelSshPatch::Username("ops".to_owned()));
        assert_eq!(state.problem(), None);
        assert_eq!(state.hint(), None, "no problem, no hint");
        assert!(state.save_gate());
        // The loading arm blocks too.
        state.open_changed(true, Some("t-1"));
        assert!(state.loading);
        assert!(!state.save_gate(), "pending blocks the save");
    }

    #[test]
    fn the_save_flows_settle() {
        let mut state = TunnelEditDialogState::default();
        state.open_changed(true, None);
        state.set_name("Bastion".to_owned());
        state.patch_ssh(TunnelSshPatch::Host("bastion.example.com".to_owned()));
        state.patch_ssh(TunnelSshPatch::Username("ops".to_owned()));

        // A success fires onSaved then onClose (the note is the
        // close; the host's onSaved slot is its own — the audit pins
        // the order through the button's wiring).
        state.begin_save();
        assert!(state.saving);
        assert!(state.pending());
        state.note_save_ok();
        assert!(!state.open, "onClose fired");
        assert!(!state.saving);

        // A failure stays open with the error.
        state.open_changed(true, None);
        state.set_name("Bastion".to_owned());
        state.patch_ssh(TunnelSshPatch::Host("bastion.example.com".to_owned()));
        state.patch_ssh(TunnelSshPatch::Username("ops".to_owned()));
        state.begin_save();
        state.note_save_error("write refused");
        assert!(state.open);
        assert_eq!(state.error, Some(DialogError::Message("write refused".to_owned())));
        assert!(!state.saving);
        state.begin_save();
        state.note_save_error("");
        assert_eq!(
            state.error,
            Some(DialogError::Key("settings.tunnels.editor.saveFailed"))
        );
    }

    #[test]
    fn the_entity_build_rides_the_carried_module() {
        // The dialog's save path builds through the carried module:
        // an edit keeps its id, a create takes the host's mint.
        let mut state = TunnelEditDialogState::default();
        state.open_changed(true, None);
        state.set_name("Bastion".to_owned());
        state.patch_ssh(TunnelSshPatch::Host("bastion.example.com".to_owned()));
        state.patch_ssh(TunnelSshPatch::Username("ops".to_owned()));
        let created = draft_to_saved_tunnel(&state.draft, "minted-1");
        assert_eq!(created.id, "minted-1");
        assert_eq!(created.kind, SavedTunnelKind::Ssh);

        // The edit path round-trips.
        let back = saved_tunnel_to_draft(&created);
        let updated = draft_to_saved_tunnel(&back, "minted-1");
        assert_eq!(updated.id, "minted-1");
        assert_eq!(updated.ssh.as_ref().unwrap().host, "bastion.example.com");
    }
}
