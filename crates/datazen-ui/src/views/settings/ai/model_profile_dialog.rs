//! Port of `packages/frontend/src/windows/settings/ai/ModelProfileDialog.tsx`.
//!
//! The settings dialog shell around the shared
//! [`ModelProfileForm`](super::model_profile_form::ModelProfileForm): the
//! open/close gate, the title picked from the `profile` prop's presence
//! (edit vs add), and the `useModelProfileDraft` configuration React's
//! 43-line dialog contributes — `onSave` and `onSaved: () => onClose()`.
//! All draft/validate/save behaviour lives in the T479
//! [`ModelProfileDraft`](super::use_model_profile_draft::ModelProfileDraft);
//! the form's rendering in the T107 port. This module is only the shell
//! that mounts the form inside the `ui/dialog.rs` base.
//!
//! ## What maps where
//!
//! | React | Rune |
//! |---|---|
//! | `open` / `onClose` / `profile` / `onSave` props | [`ModelProfileDialogSlots`] (`open`, `profile`, `dark`) plus [`ModelProfileDialogHost`]'s `close_model_profile_dialog` (Esc, the header close and `onSaved`'s close all land there) and `save_model_profile` (the `onSave` promise resolved to React's `bool`) |
//! | `useModelProfileDraft({ profile, open, onSave, onSaved })` | the host's `S` holds the T479 draft; the `[profile, open]` reseeding effect stays the host's open-path call to [`ModelProfileDraft::reseed`] and the provider-arrival effect its `fill_provider_defaults` (both the T479 module's documented host duties — a view build cannot mutate `S`), while the save wiring (`performSave` / `requestSave` / `confirmHighRisk`) is this dialog's [`perform_save`] / [`confirm_high_risk`] |
//! | `title={profile ? t('settings.ai.editModel') : t('settings.ai.addModel')}` | the same two `crate::i18n::t` keys off the `profile` slot |
//! | `<Dialog className="max-w-xl">` (the shared panel's `max-w-xl` + `max-h-[70vh]` body) | the base's `.width(576.0).fit().body_max_height(448.0)` — the install-wapp ruling for the same React caps (448pt = 70vh at the 640pt minimum window) |
//! | `<ModelProfileForm draft onCancel={onClose} />` | [`ModelProfileForm::new`] over the host's form slots with FOUR fields the dialog owns (see below), the default `FormVariant::Dialog` |
//!
//! The dialog owns four of the form's slots — React's dialog passes
//! `onCancel={onClose}` and configures the hook's save, so the host's
//! values for `on_cancel`, `on_save`, `on_confirm_high_risk` and
//! `on_cancel_high_risk` are overwritten in [`ModelProfileDialog::build`]
//! and never run:
//!
//! - `on_cancel` → the trait's close (the form's Cancel button);
//! - `on_save` → [`perform_save`]: `request_save`, then React's
//!   `performSave` (`begin_save` → the host's save → `end_save`, closing on
//!   success — `onSaved` → `onClose`);
//! - `on_confirm_high_risk` → [`confirm_high_risk`]: the T479
//!   `confirm_high_risk` (which clears the flag) followed by the same
//!   `performSave`;
//! - `on_cancel_high_risk` → the hook's `setShowHighRiskModal(false)`.
//!
//! The high-risk confirmation the form pushes while a save is pending
//! paints ABOVE this dialog: the form builds into a scratch layer so this
//! dialog's scrim/panel enter the caller's layer first (React's nested
//! portal z-order).
//!
//! ## Keys
//!
//! React's dialog has no `data-testid` of its own; the keys come from the
//! `ui/dialog.rs` base around the `model-profile-dialog` root, and the
//! form's keys stay the T107 table verbatim:
//!
//! | part | key |
//! |------|-----|
//! | panel / header / title / close | `model-profile-dialog`, `model-profile-dialog-header`, `model-profile-dialog-title`, `model-profile-dialog-close` |
//! | body / scrim / layer | `model-profile-dialog-body`, `model-profile-dialog-scrim`, `model-profile-dialog-layer` |
//! | the per-open body wrapper (scroll reset) | `model-profile-dialog-content-<epoch>` |
//! | the form (and everything under it) | `ai-profile-form` + the T107 keys |
//!
//! ## Known deltas (documented, not silent)
//!
//! - The host's save resolves synchronously (the settled promise): the
//!   `saving` flag brackets the call instead of spanning frames. A host
//!   with a genuinely slow save flips `saving` itself around its own
//!   `request_save` — the T479 ruling this port follows.
//! - The `crypto.randomUUID()` for a new profile's id, the store's error
//!   clear on open and the `tx.wake_after(3s)` for the validate badge stay
//!   host-side (T479's documented seams).
//! - Focus return, the scrim and Escape are the base's (T0-05 audited).

// The settings AI section (T172) is the production consumer; until it lands
// this crate's tests are the only callers.
#![allow(dead_code)]

use rune::El;
use datazen_ui_api::{AiModelProfile, ProviderListItem};

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use crate::views::settings::ai::model_profile_form::{ModelProfileForm, ModelProfileFormSlots};
use crate::views::settings::ai::use_model_profile_draft::{ModelProfileDraft, SaveRequest};

/// The dialog's host wiring — the two callbacks React's dialog passes into
/// `useModelProfileDraft` (`{ onSave, onSaved: () => onClose() }`) plus the
/// reaches the save glue reads at press time. The owning state implements
/// it (this port's audits; the settings AI section, T172).
pub(crate) trait ModelProfileDialogHost {
    /// The draft being edited (the hook's state, held in `S`).
    fn model_profile_dialog_draft(&mut self) -> &mut ModelProfileDraft;
    /// The store's raw provider list (the save payload's name fallbacks).
    fn model_profile_dialog_providers(&self) -> Vec<ProviderListItem>;
    /// React's `onSave` prop: persist the profile, answering React's bool.
    fn save_model_profile(&mut self, profile: AiModelProfile) -> bool;
    /// React's `onClose` prop — Escape, the header close, the form's cancel
    /// and a successful save's `onSaved` all land here.
    fn close_model_profile_dialog(&mut self);
}

/// React's `performSave`: the saving flag around the host's save, then the
/// close on success (`onSaved` → `onClose`).
fn perform_saved_payload<S: ModelProfileDialogHost>(s: &mut S, payload: AiModelProfile) {
    s.model_profile_dialog_draft().begin_save();
    let success = s.save_model_profile(payload);
    s.model_profile_dialog_draft().end_save();
    if success {
        s.close_model_profile_dialog();
    }
}

/// The form's Save press — the hook's `requestSave`: unrestricted egress on
/// a remote endpoint only arms the confirmation (the form renders it);
/// anything else saves through [`perform_saved_payload`].
fn perform_save<S: ModelProfileDialogHost>(s: &mut S) {
    let providers = s.model_profile_dialog_providers();
    match s.model_profile_dialog_draft().request_save(&providers) {
        SaveRequest::ConfirmHighRisk => {}
        SaveRequest::Save(payload) => perform_saved_payload(s, payload),
    }
}

/// The high-risk confirmation's confirm — the hook's `confirmHighRisk`:
/// close the confirmation (the T479 method clears the flag) and save.
fn confirm_high_risk<S: ModelProfileDialogHost>(s: &mut S) {
    let providers = s.model_profile_dialog_providers();
    if let SaveRequest::Save(payload) = s.model_profile_dialog_draft().confirm_high_risk(&providers)
    {
        perform_saved_payload(s, payload);
    }
}

/// Where the dialog's state lives in the host's `S` (function pointers, the
/// `workflow_form.rs` slot pattern — the struct is `Copy`).
pub(crate) struct ModelProfileDialogSlots<S: 'static> {
    /// React's `open` prop: while false the dialog mounts nothing at all.
    pub(crate) open: fn(&S) -> bool,
    /// The open generation for the base's scroll reset (the host bumps it
    /// every time the dialog opens, next to the `open` flag).
    pub(crate) open_epoch: fn(&S) -> u64,
    /// React's `profile` prop: `Some` edits that profile (the edit title),
    /// `None` adds (the add title).
    pub(crate) profile: fn(&S) -> Option<&AiModelProfile>,
    /// The kit appearance.
    pub(crate) dark: fn(&S) -> bool,
    /// The form's slots. The dialog owns `on_cancel`, `on_save`,
    /// `on_confirm_high_risk` and `on_cancel_high_risk` (React's dialog
    /// passes `onCancel={onClose}` and configures the hook's save); the
    /// host's values for those four are overwritten in
    /// [`ModelProfileDialog::build`] and never run.
    pub(crate) form: ModelProfileFormSlots<S>,
}

impl<S: 'static> Clone for ModelProfileDialogSlots<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S: 'static> Copy for ModelProfileDialogSlots<S> {}

/// The model profile dialog, built per frame over the host's `S` (React's
/// `<ModelProfileDialog open onClose profile onSave />`).
pub(crate) struct ModelProfileDialog<S: ModelProfileDialogHost + 'static> {
    slots: ModelProfileDialogSlots<S>,
}

impl<S: ModelProfileDialogHost + 'static> ModelProfileDialog<S> {
    pub(crate) fn new(slots: ModelProfileDialogSlots<S>) -> Self {
        Self { slots }
    }

    /// One appearance of the dialog. While closed nothing is pushed (a
    /// closed `Dialog open={false}` renders null); while open the form's
    /// high-risk confirmation lands in `layer` after this dialog's panel,
    /// so it paints above (React's nested portal z-order).
    pub(crate) fn build(&self, s: &S, layer: &mut Vec<El>) {
        if !(self.slots.open)(s) {
            return;
        }
        let dark = (self.slots.dark)(s);
        let k = kit(dark);
        // React: `title={profile ? t('settings.ai.editModel') :
        // t('settings.ai.addModel')}`.
        let title = if (self.slots.profile)(s).is_some() {
            t("settings.ai.editModel")
        } else {
            t("settings.ai.addModel")
        };

        // The dialog's own wiring of the four form slots it owns (see the
        // module docs); a non-capturing closure coerces to the slot's fn
        // pointer.
        let mut form_slots = self.slots.form;
        form_slots.on_cancel = |s: &mut S| s.close_model_profile_dialog();
        form_slots.on_save = perform_save::<S>;
        form_slots.on_confirm_high_risk = confirm_high_risk::<S>;
        form_slots.on_cancel_high_risk =
            |s: &mut S| s.model_profile_dialog_draft().show_high_risk_modal = false;

        // The default variant is the dialog one (React's ModelProfileForm
        // defaults `variant: 'dialog'`).
        let form = ModelProfileForm::new(form_slots);
        let mut scratch = Vec::new();
        let body = form.build(s, dark, &mut scratch);

        let dialog = Dialog::new("model-profile-dialog", title)
            .open(true)
            .open_epoch((self.slots.open_epoch)(s))
            // React: the shared Dialog panel's `max-w-xl` and body
            // `max-h-[70vh]` — 448pt at the 640pt minimum window (the
            // install-wapp ruling for the same caps).
            .width(576.0)
            .fit()
            .body_max_height(448.0)
            .child(body)
            .on_close(|s: &mut S| s.close_model_profile_dialog());
        dialog.build(&k, layer);
        layer.extend(scratch);
    }
}
