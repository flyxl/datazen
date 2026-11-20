//! Port of `packages/frontend/src/windows/schema-diff/
//! SchemaDiffSavedSetupDialogs.tsx` (T229) — the two saved-setup
//! dialogs.
//!
//! The import-config dialog (paste a shared diff setup's JSON; the
//! confirm is disabled until the text trims non-empty, and any error
//! from the apply is a copyable line under the field) and the
//! profile-save dialog (name the current setup; the confirm is disabled
//! until the name trims non-empty). Closing either dialog clears its
//! text and its error (a stale error must not greet the next open).
//!
//! ## What maps where
//!
//! - `useSchemaDiffSavedSetups` (the state machine both dialogs read
//!   and drive) is T411's hook; this component takes the projection
//!   ([`SavedSetupDialogsVm`]) plus callbacks, the component-store
//!   split every schema-diff port uses.
//! - `Dialog`/`Button`/`CopyableError` map to the engine's
//!   `ui/dialog.rs`, kit buttons and `ui/copyable.rs`.
//! - The textarea/input edits land in host-owned `TextEdit`s (the
//!   slots), and every edit clears a present error (React's onChange
//!   half).
//! - React testids stay verbatim: `schema-diff-import-config-dialog`,
//!   `schema-diff-import-config-text`, `schema-diff-import-config-
//!   confirm`, `schema-diff-import-config-error`,
//!   `schema-diff-profile-dialog`, `schema-diff-profile-name`,
//!   `schema-diff-profile-save-confirm`.
//!
//! Known deltas (documented, not silent): `h-48` (192px) maps the
//! textarea's `max_height(192.0)` with auto-grow; the profile input is
//! the shared `TextField`; `font-mono text-xs` renders as the kit's
//! textarea type without a mono face switch.

use std::rc::Rc;

use rune::{col, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, TextArea};

use crate::i18n::t;
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

/// The dialogs' projection of `useSchemaDiffSavedSetups` (T411's hook).
pub(crate) struct SavedSetupDialogsVm<'a> {
    /// `importConfigOpen`.
    pub import_config_open: bool,
    /// `importConfigText`'s edit (the pasted JSON).
    pub import_config_text: &'a TextEdit,
    /// `importConfigError`.
    pub import_config_error: Option<&'a str>,
    /// `profileDialogOpen`.
    pub profile_dialog_open: bool,
    /// `profileName`'s edit.
    pub profile_name: &'a TextEdit,
    /// `profileError`.
    pub profile_error: Option<&'a str>,
}

/// The two saved-setup dialogs, built per frame over the host's `S`.
pub(crate) struct SchemaDiffSavedSetupDialogs<'a, S: 'static> {
    vm: SavedSetupDialogsVm<'a>,
    // The import dialog's callbacks.
    on_close_import: Option<Act<S>>,
    on_import_text_edit: Option<EditSlot<S>>,
    on_apply_import: Option<Act<S>>,
    // The profile dialog's callbacks.
    on_close_profile: Option<Act<S>>,
    on_profile_name_edit: Option<EditSlot<S>>,
    on_save_profile: Option<Act<S>>,
}

type Act<S> = Rc<dyn Fn(&mut S)>;
type EditSlot<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>;

impl<'a, S: 'static> SchemaDiffSavedSetupDialogs<'a, S> {
    /// Build over the projection (one frame's borrow).
    pub(crate) fn over(vm: SavedSetupDialogsVm<'a>) -> Self {
        Self {
            vm,
            on_close_import: None,
            on_import_text_edit: None,
            on_apply_import: None,
            on_close_profile: None,
            on_profile_name_edit: None,
            on_save_profile: None,
        }
    }

    /// The import dialog's close (cancel / Escape / X): close, clear the
    /// text and the error.
    pub(crate) fn on_close_import(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_import = Some(Rc::new(f));
        self
    }

    /// The pasted-JSON field's edit slot (the host also clears a present
    /// error on every edit).
    pub(crate) fn on_import_text_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_import_text_edit = Some(Rc::new(f));
        self
    }

    /// The confirm press (disabled until the text trims non-empty): the
    /// host applies the pasted config.
    pub(crate) fn on_apply_import(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_apply_import = Some(Rc::new(f));
        self
    }

    /// The profile dialog's close: close and clear the error.
    pub(crate) fn on_close_profile(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_profile = Some(Rc::new(f));
        self
    }

    /// The name field's edit slot.
    pub(crate) fn on_profile_name_edit(
        mut self,
        f: impl for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static,
    ) -> Self {
        self.on_profile_name_edit = Some(Rc::new(f));
        self
    }

    /// The save press (disabled until the name trims non-empty).
    pub(crate) fn on_save_profile(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_save_profile = Some(Rc::new(f));
        self
    }

    /// Paint both dialogs into `layer` (each only while open).
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        let k = kit(dark);
        if self.vm.import_config_open {
            self.build_import_dialog(&k, layer);
        }
        if self.vm.profile_dialog_open {
            self.build_profile_dialog(&k, layer);
        }
    }

    fn build_import_dialog(&self, k: &Kit, layer: &mut Vec<El>) {
        let text = self.vm.import_config_text.text.clone();
        let can_confirm = !text.trim().is_empty();

        let mut dialog = Dialog::new("schema-diff-import-config-dialog", t("schemaDiff.importConfigTitle"))
            .open(true)
            .description(t("schemaDiff.importConfigHint"));

        // The textarea: h-48 tall, mono, with the paste placeholder.
        let mut area = TextArea::new(self.vm.import_config_text)
            .placeholder(t("schemaDiff.importConfigPlaceholder"))
            .width(None)
            .rows(8)
            .max_height(192.0); // React h-48
        if let Some(edit_slot) = &self.on_import_text_edit {
            let edit_slot = Rc::clone(edit_slot);
            area = area.on_edit(move |s: &mut S| edit_slot(s));
        }
        dialog = dialog.child(
            col().key("schema-diff-import-config-text").w_full().gap(8.0).child(area.build(k)),
        );

        // The copyable error line under the field.
        if let Some(error) = self.vm.import_config_error {
            dialog = dialog
                .child(CopyableError::new("schema-diff-import-config-error", error).build::<S>(k));
        }

        // The footer: cancel (ghost) + confirm (disabled until non-blank).
        let mut cancel = Button::new(t("common.cancel")).variant(ButtonVariant::Ghost);
        if let Some(on_close) = &self.on_close_import {
            let on_close = Rc::clone(on_close);
            cancel = cancel.on_press(move |s: &mut S| on_close(s));
        }
        dialog = dialog
            .footer(cancel.build(k).key("schema-diff-import-config-dialog-cancel"));
        let mut confirm = Button::new(t("schemaDiff.importConfigConfirm"))
            .variant(ButtonVariant::Submit) // the primary default
            .disabled(!can_confirm);
        if let Some(on_apply) = &self.on_apply_import {
            let on_apply = Rc::clone(on_apply);
            confirm = confirm.on_press(move |s: &mut S| on_apply(s));
        }
        dialog = dialog.footer(confirm.build(k).key("schema-diff-import-config-confirm"));

        if let Some(on_close) = &self.on_close_import {
            let on_close = Rc::clone(on_close);
            dialog = dialog.on_close(move |s: &mut S| on_close(s));
        }
        dialog.build(k, layer);
    }

    fn build_profile_dialog(&self, k: &Kit, layer: &mut Vec<El>) {
        let name = self.vm.profile_name.text.clone();
        let can_confirm = !name.trim().is_empty();

        let mut dialog = Dialog::new("schema-diff-profile-dialog", t("schemaDiff.profileSaveTitle"))
            .open(true)
            .description(t("schemaDiff.profileSaveHint"));

        // The name input.
        let mut field = TextField::new("schema-diff-profile-name", self.vm.profile_name)
            .height(36.0) // React h-9
            .grow()
            .placeholder(t("schemaDiff.profileNamePlaceholder"));
        if let Some(edit_slot) = &self.on_profile_name_edit {
            let edit_slot = Rc::clone(edit_slot);
            field = field.on_edit(move |s: &mut S| edit_slot(s));
        }
        dialog = dialog.child(
            col().key("schema-diff-profile-name").w_full().gap(8.0).child(field.build(true)),
        );

        // The copyable error line.
        if let Some(error) = self.vm.profile_error {
            dialog =
                dialog.child(CopyableError::new("schema-diff-profile-error", error).build::<S>(k));
        }

        // The footer.
        let mut cancel = Button::new(t("common.cancel")).variant(ButtonVariant::Ghost);
        if let Some(on_close) = &self.on_close_profile {
            let on_close = Rc::clone(on_close);
            cancel = cancel.on_press(move |s: &mut S| on_close(s));
        }
        dialog = dialog.footer(cancel.build(k).key("schema-diff-profile-dialog-cancel"));
        let mut confirm = Button::new(t("schemaDiff.profileSave"))
            .variant(ButtonVariant::Submit)
            .disabled(!can_confirm);
        if let Some(on_save) = &self.on_save_profile {
            let on_save = Rc::clone(on_save);
            confirm = confirm.on_press(move |s: &mut S| on_save(s));
        }
        dialog = dialog.footer(confirm.build(k).key("schema-diff-profile-save-confirm"));

        if let Some(on_close) = &self.on_close_profile {
            let on_close = Rc::clone(on_close);
            dialog = dialog.on_close(move |s: &mut S| on_close(s));
        }
        dialog.build(k, layer);
    }
}
