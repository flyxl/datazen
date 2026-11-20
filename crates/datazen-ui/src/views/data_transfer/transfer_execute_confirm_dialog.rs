//! T225: `windows/data-transfer/TransferExecuteConfirmDialog.tsx` ported
//! to Rune.
//!
//! The destructive-transfer confirmation: the intro sentence keyed by
//! the write mode (`dropCreateInsert` drops and recreates, otherwise
//! truncate-before-insert), the affected table pairs as a bullet list,
//! and the ghost cancel / primary proceed footer.
//!
//! ## Keys
//!
//! React `data-testid` becomes the element key, verbatim; the rest are
//! internal:
//!
//! | part | key |
//! |---|---|
//! | dialog (React test id) | `data-transfer-execute-confirm-dialog` |
//! | body (React test id) | `data-transfer-execute-confirm` |
//! | intro paragraph (internal) | `…-intro` |
//! | one plan row (React test id) | `data-transfer-execute-confirm-table-<targetTable>` |
//! | cancel button (React test id) | `data-transfer-execute-confirm-cancel` |
//! | proceed button (React test id) | `data-transfer-execute-confirm-proceed` |
//!
//! ## Documented deviations from React
//!
//! * **`list-disc pl-4` becomes plain rows**: the kit has no bullet
//!   glyph token here, and the dialog's own list rows already carry the
//!   pair text the tests assert; the bullet is decoration the React
//!   source derives from the same list.
//! * **The primary footer button is the kit `Submit` variant** — the
//!   track convention (see T074's `Contrast` note) maps React `primary`
//!   to the kit's high-emphasis button.
//! * **`text-fg-secondary` has no kit token** — the intro renders at the
//!   muted step (the same one the list rows use), one notch above
//!   nothing; the pair text keeps the assertions' readability.
//! * **The dialog is fitted at 576pt** (the `Dialog` default, React's
//!   uncapped `max-w-xl` equivalent used across the transfer dialogs).

// The production consumer is the Data Transfer window (its execute
// step mounts this at the confirm seam); until the wiring lands, the
// component tests are the only callers.
#![allow(dead_code)] // reusable component; wired by the Data Transfer window

use std::rc::Rc;

use rune::{col, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use crate::views::{TransferWritePlanVm, WriteMode};

fn no_op<S>() -> Rc<dyn Fn(&mut S)> {
    Rc::new(|_: &mut S| {})
}

/// The builder. `S` is the host state that owns the execute / close
/// actions.
pub(crate) struct TransferExecuteConfirmDialog<S: 'static> {
    open: bool,
    open_epoch: u64,
    write_mode: WriteMode,
    write_plans: Vec<TransferWritePlanVm>,
    on_close: Option<Rc<dyn Fn(&mut S)>>,
    on_confirm: Option<Rc<dyn Fn(&mut S)>>,
}

impl<S: 'static> TransferExecuteConfirmDialog<S> {
    pub(crate) fn new(
        open: bool,
        write_mode: WriteMode,
        write_plans: Vec<TransferWritePlanVm>,
    ) -> Self {
        Self {
            open,
            open_epoch: 0,
            write_mode,
            write_plans,
            on_close: None,
            on_confirm: None,
        }
    }

    /// The open generation (a reopened dialog restarts at the top);
    /// the window bumps it on open, exactly as the other dialogs do.
    pub(crate) fn open_epoch(mut self, epoch: u64) -> Self {
        self.open_epoch = epoch;
        self
    }

    /// React `onClose`: the cancel button, the header close, and Escape.
    pub(crate) fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// React `onConfirm`: the proceed button.
    pub(crate) fn on_confirm(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm = Some(Rc::new(f));
        self
    }

    /// The dialog on its own kit root. The root itself mounts nothing
    /// visible (React returns the `<Dialog>` only); the dialog lives in
    /// the overlay layer.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let mut layer = Vec::new();
        let root = self.build_inner(&k, &mut layer);
        rune_kit::root(root, layer)
    }

    /// The dialog pushed into the caller's layer; the root element is
    /// returned for the caller to place (an empty column, the fragment
    /// root of the React source).
    pub(crate) fn build_inner(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let close = self.on_close.clone().unwrap_or_else(no_op::<S>);
        let confirm = self.on_confirm.clone().unwrap_or_else(no_op::<S>);
        let intro_key = match self.write_mode {
            WriteMode::DropCreateInsert => "transfer.executeConfirm.introDrop",
            _ => "transfer.executeConfirm.introTruncate",
        };
        let mut body = col()
            .key("data-transfer-execute-confirm")
            .gap(12.0)
            .child(
                text(t(intro_key))
                    .key("data-transfer-execute-confirm-intro")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted)
                    .wrap(),
            );
        for plan in &self.write_plans {
            body = body.child(
                text(format!("{} → {}", plan.source_table, plan.target_table))
                    .key(format!(
                        "data-transfer-execute-confirm-table-{}",
                        plan.target_table
                    ))
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            );
        }
        let close_cancel = close.clone();
        Dialog::new(
            "data-transfer-execute-confirm-dialog",
            t("transfer.executeConfirm.title").to_owned(),
        )
        .open(self.open)
        .open_epoch(self.open_epoch)
        .fit()
        .child(body)
        .footer(
            Button::new(t("common.cancel"))
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .on_press(move |s: &mut S| close_cancel(s))
                .build(k)
                .key("data-transfer-execute-confirm-cancel"),
        )
        .footer(
            Button::new(t("transfer.execute"))
                .variant(ButtonVariant::Submit)
                .control_size(ControlSize::Small)
                .on_press(move |s: &mut S| confirm(s))
                .build(k)
                .key("data-transfer-execute-confirm-proceed"),
        )
        .on_close(move |s: &mut S| close(s))
        .build(k, layer);
        col().key("data-transfer-execute-confirm-root")
    }
}
