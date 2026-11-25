//! The Data Sync window's dialogs — T074.
//!
//! Ported from `packages/frontend/src/windows/data-sync/DataSyncDialogs.tsx`
//! (116 lines): the four dialogs of the Data Sync window — the error dialog,
//! the delete-enable confirmation, the AI explain-diff dialog, and the
//! execute (delete) confirmation. React renders them as a fragment of four
//! `<Dialog>`s; each mounts only while its `open` flag is set, independently
//! of the others.
//!
//! ## Key convention
//!
//! React `data-testid` becomes the element key, verbatim; the other three
//! dialogs carry no test id in React, so their keys follow the shared
//! `<dialog>-dialog` naming of the [`Dialog`] base (see `ui/dialog.rs` for
//! the `<key>-title` / `<key>-close` / `<key>-body` / `<key>-footer` /
//! `<key>-scrim` / `<key>-layer` parts each dialog gains):
//!
//! | part | key |
//! |---|---|
//! | fragment root | `data-sync-dialogs` (internal) |
//! | error dialog | `data-sync-error-dialog` (internal) |
//! | error message | `data-sync-error` (React test id) |
//! | error message text | `data-sync-error-text` |
//! | error OK button | `data-sync-error-ok` (internal) |
//! | delete-confirm dialog | `data-sync-delete-confirm-dialog` (internal) |
//! | delete-confirm body | `data-sync-delete-confirm-body` (internal) |
//! | delete-confirm cancel | `data-sync-delete-confirm-cancel` (internal) |
//! | delete-confirm enable | `data-sync-delete-confirm-enable` (internal) |
//! | explain dialog | `data-sync-explain-dialog` (internal) |
//! | explain loading row | `data-sync-explain-loading` (internal) |
//! | explain spinner | `data-sync-explain-spinner` |
//! | explain loading text | `data-sync-explain-loading-text` (internal) |
//! | explain result | `data-sync-explain-result` (React test id) |
//! | explain close button | `data-sync-explain-close` (internal) |
//! | execute-confirm dialog | `data-sync-execute-confirm-dialog` (React test id) |
//! | execute-confirm body | `data-sync-execute-confirm-body` (internal) |
//! | execute-confirm cancel | `data-sync-execute-confirm-cancel` (internal) |
//! | execute-confirm execute | `data-sync-confirm-execute` (React test id) |
//!
//! ## Mounting
//!
//! [`DataSyncDialogs::build_inner`] returns the fragment root plus the
//! overlay layer holding the four dialogs, so the Data Sync window (T270)
//! can root the whole wizard once; `build` is `build_inner` plus the kit
//! root, exactly as the probes mount it.
//!
//! ## Documented deviations from React
//!
//! * **Panel width is the [`Dialog`] default (576pt).** React caps the panel
//!   at `max-w-xl` (576px); the base defaults to the same width, so no
//!   explicit width is set.
//! * **The error dialog caps its body at 448pt.** React caps the body at
//!   `70vh`; the 448pt figure is the same one `views/modals.rs` keeps for
//!   the 640pt minimum window, so a long error scrolls instead of pushing
//!   the actions out. The three confirmation dialogs are short and use a
//!   plain fitted panel.
//! * **The error message renders at the shared base's size.**
//!   [`CopyableError`] renders non-mono text at `text_xsmall`; React styles
//!   this call site `text-sm`. The size belongs to the base, not to this
//!   slice, so it is not overridden here. The message itself, its
//!   selectability, its `role="alert"`, and the absent copy button all match
//!   React (no `copyButton`, no `mono`).
//! * **React `primary` / `run` buttons render as kit `Contrast`.** The kit
//!   has no `Primary` or `Run` variant; the track convention (see
//!   `ui/toolbar.rs` and `components/ai/ai_chat_panel_shell.rs`) maps both
//!   React variants to [`ButtonVariant::Contrast`]. That covers the error
//!   dialog's OK, the explain dialog's close, and the execute-confirm
//!   dialog's Execute button; ghost and danger map one to one.
//!   `whitespace-pre-wrap` wraps and keeps the AI text's line breaks; Rune
//!   text breaks paragraphs at `\n` and `.wrap()` wraps at the width, so
//!   the same rendering follows.
//! * **A reopened dialog restarts at the top via its epoch.** Each dialog
//!   carries an `open_epoch` in the view model (default 0); the window
//!   bumps it on open, exactly as [`Dialog::open_epoch`] prescribes. Until
//!   T270 wires them the epochs stay 0, which still gives each dialog a
//!   stable body identity.

// The production consumer is the Data Sync window (T270); until then the
// component tests are the only callers, so the dead-code lint is silenced
// here rather than in a crate-wide allow.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::copyable::CopyableError;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};

/// The no-op a missing callback falls back to: a press with no host
/// handler is swallowed, exactly as the compare step's cancel does.
fn no_op<S: 'static>() -> Rc<dyn Fn(&mut S)> {
    Rc::new(|_: &mut S| {})
}

/// One footer button of a dialog: the kit small button in `variant`,
/// keyed for the probes.
fn footer_button<S: 'static>(
    k: &Kit,
    label: &str,
    variant: ButtonVariant,
    key: &str,
    on_press: Rc<dyn Fn(&mut S)>,
) -> El {
    Button::new(label)
        .variant(variant)
        .control_size(ControlSize::Small)
        .on_press(move |s: &mut S| on_press(s))
        .build(k)
        .key(key)
}

/// The controlled state of the dialogs. Fields mirror
/// `DataSyncDialogsProps` one to one; the `*_epoch` fields are the
/// [`Dialog::open_epoch`] generations the Data Sync window (T270) bumps
/// every time it opens a dialog, so a reopened dialog restarts at the top.
#[derive(Clone, Debug, Default)]
pub struct DataSyncDialogsVm {
    pub error_open: bool,
    pub error_msg: String,
    pub error_epoch: u64,
    pub delete_confirm_open: bool,
    pub delete_confirm_epoch: u64,
    pub explain_open: bool,
    pub explain_loading: bool,
    pub explain_text: String,
    pub explain_epoch: u64,
    pub execute_confirm_open: bool,
    pub execute_confirm_epoch: u64,
}

/// Builder for the Data Sync dialogs. `S` is the host state that owns the
/// close / confirm actions.
pub struct DataSyncDialogs<S: 'static> {
    vm: DataSyncDialogsVm,
    dark: bool,
    on_close_error: Option<Rc<dyn Fn(&mut S)>>,
    on_close_delete_confirm: Option<Rc<dyn Fn(&mut S)>>,
    on_confirm_enable_delete: Option<Rc<dyn Fn(&mut S)>>,
    on_close_explain: Option<Rc<dyn Fn(&mut S)>>,
    on_close_execute_confirm: Option<Rc<dyn Fn(&mut S)>>,
    on_confirm_execute: Option<Rc<dyn Fn(&mut S)>>,
}

impl<S: 'static> DataSyncDialogs<S> {
    pub fn new(vm: DataSyncDialogsVm) -> Self {
        Self {
            vm,
            dark: true,
            on_close_error: None,
            on_close_delete_confirm: None,
            on_confirm_enable_delete: None,
            on_close_explain: None,
            on_close_execute_confirm: None,
            on_confirm_execute: None,
        }
    }

    /// React `onCloseError`: the error dialog's OK button, header close,
    /// and Escape.
    pub fn on_close_error(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_error = Some(Rc::new(f));
        self
    }

    /// React `onCloseDeleteConfirm`: the delete-confirm dialog's cancel
    /// button, header close, and Escape.
    pub fn on_close_delete_confirm(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_delete_confirm = Some(Rc::new(f));
        self
    }

    /// React `onConfirmEnableDelete`: the delete-confirm dialog's danger
    /// button.
    pub fn on_confirm_enable_delete(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm_enable_delete = Some(Rc::new(f));
        self
    }

    /// React `onCloseExplain`: the explain dialog's close button, header
    /// close, and Escape.
    pub fn on_close_explain(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_explain = Some(Rc::new(f));
        self
    }

    /// React `onCloseExecuteConfirm`: the execute-confirm dialog's cancel
    /// button, header close, and Escape.
    pub fn on_close_execute_confirm(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close_execute_confirm = Some(Rc::new(f));
        self
    }

    /// React `onConfirmExecute`: the execute-confirm dialog's run button.
    pub fn on_confirm_execute(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm_execute = Some(Rc::new(f));
        self
    }

    /// The dialogs on their own, exactly as the probes mount them.
    pub fn build(self, dark: bool) -> El {
        let (root, layer) = self.build_inner(dark);
        rune_kit::root(root, layer)
    }

    /// The same dialogs without their kit root: the fragment root plus the
    /// overlay layer holding the four dialogs. The Data Sync window (T270)
    /// calls this once and roots the whole wizard once.
    pub fn build_inner(mut self, dark: bool) -> (El, Vec<El>) {
        self.dark = dark;
        let k = kit(dark);
        // React root: a fragment of four dialogs, so the root mounts
        // nothing visible itself; the dialogs live in the layer.
        let root = col().key("data-sync-dialogs");
        let mut layer = Vec::new();
        self.error_dialog(&k, &mut layer);
        self.delete_confirm_dialog(&k, &mut layer);
        self.explain_dialog(&k, &mut layer);
        self.execute_confirm_dialog(&k, &mut layer);
        (root, layer)
    }

    /// The error dialog: `common.hint` title, the copyable error message,
    /// a primary OK footer.
    fn error_dialog(&self, k: &Kit, layer: &mut Vec<El>) {
        let close = self.on_close_error.clone().unwrap_or_else(no_op::<S>);
        let ok = close.clone();
        Dialog::new("data-sync-error-dialog", t("common.hint"))
            .open(self.vm.error_open)
            .open_epoch(self.vm.error_epoch)
            .fit()
            .body_max_height(448.0)
            .child(CopyableError::new("data-sync-error", self.vm.error_msg.clone()).build::<S>(k))
            .footer(footer_button(
                k,
                t("common.ok"),
                ButtonVariant::Contrast,
                "data-sync-error-ok",
                ok,
            ))
            .on_close(move |s: &mut S| close(s))
            .build(k, layer);
    }

    /// The delete-enable confirmation: `sync.deleteConfirmTitle` title,
    /// the `sync.deleteConfirmBody` paragraph, ghost cancel + danger
    /// enable footers.
    fn delete_confirm_dialog(&self, k: &Kit, layer: &mut Vec<El>) {
        let close = self
            .on_close_delete_confirm
            .clone()
            .unwrap_or_else(no_op::<S>);
        let enable = self
            .on_confirm_enable_delete
            .clone()
            .unwrap_or_else(no_op::<S>);
        Dialog::new(
            "data-sync-delete-confirm-dialog",
            t("sync.deleteConfirmTitle"),
        )
        .open(self.vm.delete_confirm_open)
        .open_epoch(self.vm.delete_confirm_epoch)
        .fit()
        .child(
            text(t("sync.deleteConfirmBody"))
                .key("data-sync-delete-confirm-body")
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .wrap(),
        )
        .footer(footer_button(
            k,
            t("common.cancel"),
            ButtonVariant::Ghost,
            "data-sync-delete-confirm-cancel",
            close.clone(),
        ))
        .footer(footer_button(
            k,
            t("sync.enableDelete"),
            ButtonVariant::Danger,
            "data-sync-delete-confirm-enable",
            enable,
        ))
        .on_close(move |s: &mut S| close(s))
        .build(k, layer);
    }

    /// The explain-diff dialog: `sync.explainDiffTitle` title, a large
    /// spinner + `sync.executing` while loading, otherwise the
    /// pre-wrapped explanation text, and a primary close footer.
    fn explain_dialog(&self, k: &Kit, layer: &mut Vec<El>) {
        let close = self.on_close_explain.clone().unwrap_or_else(no_op::<S>);
        let mut dialog = Dialog::new("data-sync-explain-dialog", t("sync.explainDiffTitle"))
            .open(self.vm.explain_open)
            .open_epoch(self.vm.explain_epoch)
            .fit();
        if self.vm.explain_loading {
            dialog = dialog.child(
                row()
                    .key("data-sync-explain-loading")
                    .w_full()
                    .center_y()
                    .gap(8.0)
                    .child(
                        Spinner::new("data-sync-explain-spinner")
                            .size(SpinnerSize::Lg)
                            .build(self.dark),
                    )
                    .child(
                        text(t("sync.executing"))
                            .key("data-sync-explain-loading-text")
                            .font(k.ty.text_small)
                            .color(k.c.text_muted),
                    ),
            );
        } else {
            dialog = dialog.child(
                text(self.vm.explain_text.clone())
                    .key("data-sync-explain-result")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted)
                    .wrap(),
            );
        }
        dialog
            .footer(footer_button(
                k,
                t("common.close"),
                ButtonVariant::Contrast,
                "data-sync-explain-close",
                close.clone(),
            ))
            .on_close(move |s: &mut S| close(s))
            .build(k, layer);
    }

    /// The execute (delete) confirmation: `sync.executeDeleteTitle` title,
    /// the `sync.executeDeleteBody` paragraph, ghost cancel + run execute
    /// footers.
    fn execute_confirm_dialog(&self, k: &Kit, layer: &mut Vec<El>) {
        let close = self
            .on_close_execute_confirm
            .clone()
            .unwrap_or_else(no_op::<S>);
        let execute = self.on_confirm_execute.clone().unwrap_or_else(no_op::<S>);
        Dialog::new(
            "data-sync-execute-confirm-dialog",
            t("sync.executeDeleteTitle"),
        )
        .open(self.vm.execute_confirm_open)
        .open_epoch(self.vm.execute_confirm_epoch)
        .fit()
        .child(
            text(t("sync.executeDeleteBody"))
                .key("data-sync-execute-confirm-body")
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .wrap(),
        )
        .footer(footer_button(
            k,
            t("common.cancel"),
            ButtonVariant::Ghost,
            "data-sync-execute-confirm-cancel",
            close.clone(),
        ))
        .footer(footer_button(
            k,
            t("sync.execute"),
            ButtonVariant::Contrast,
            "data-sync-confirm-execute",
            execute,
        ))
        .on_close(move |s: &mut S| close(s))
        .build(k, layer);
    }
}
