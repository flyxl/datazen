//! ConfirmDialog base — port of `packages/ui/src/ConfirmDialog.tsx`.
//!
//! A `ui/dialog.rs` modal with a message (optionally badged and described),
//! a warning glyph for `kind: 'warning'`, an optional truncated SQL preview
//! with a copy affordance, and ghost/danger confirm-cancel footer buttons.
//! This is the base the `hooks/useConfirmDialog` port renders; the React
//! side reaches it through the `components/ui/ConfirmDialog.tsx` re-export
//! shell, which the migration classifies as `X` (vanishes).
//!
//! ## What maps where
//!
//! - `useCopyFeedback(2000)` is the host's copied-flag projection
//!   ([`ConfirmDialog::copied`]), the same seam `ui/copyable.rs`'s
//!   `CopyButton` uses (whose feedback the T029 fix round pinned at
//!   1500ms — this one is deliberately longer, as its React comment says:
//!   more time to notice what was copied inside a destructive confirm).
//! - `truncateCodePreview` (12 lines / 2000 chars) is pure and pinned
//!   byte-for-byte by the tests.
//! - `kind === 'warning'`'s `text-amber-500` glyph and the badge's amber
//!   tint map to the kit's `warning` role tokens (`c.warning.fg` /
//!   `c.warning.bg`); `variant={kind === 'warning' ? 'danger' : 'primary'}`
//!   maps to the kit `Danger` / `Submit` button variants.
//! - The preview `<pre>` renders as a mono text block; inside a dialog body
//!   the body's own scroll carries overflow when the window is short
//!   (React `max-h-40 overflow-auto` localises it — a documented delta).
//!
//! Keys: `confirm-dialog-cancel` / `confirm-dialog-ok` /
//! `confirm-dialog-copy-sql` (React data-testids, verbatim) plus the
//! `ui/dialog.rs` shell keys from the passed key prefix. The truncation
//! note and the copy label read the same i18n keys React does.

use std::rc::Rc;

use rune::{col, icon, row, text, El, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::ICON_ALERT_TRIANGLE;
use crate::ui::copyable::CopyButton;
use crate::ui::dialog::Dialog;

/// React `kind?: 'warning' | 'info'` (default `warning`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[allow(dead_code)] // ui base; the hooks/ and view tracks reach it
pub(crate) enum ConfirmKind {
    #[default]
    Warning,
    Info,
}

/// React `CODE_PREVIEW_MAX_LINES`.
#[allow(dead_code)] // ui base; the hooks/ and view tracks reach it
pub(crate) const CODE_PREVIEW_MAX_LINES: usize = 12;
/// React `CODE_PREVIEW_MAX_CHARS`.
#[allow(dead_code)] // ui base; the hooks/ and view tracks reach it
pub(crate) const CODE_PREVIEW_MAX_CHARS: usize = 2000;

/// React `truncateCodePreview`: within both limits verbatim; otherwise the
/// first 12 lines and the truncated flag (the caller appends the ellipsis).
#[allow(dead_code)] // ui base; the hooks/ and view tracks reach it
pub(crate) fn truncate_code_preview(raw: &str) -> (String, bool) {
    let lines = raw.split('\n').count();
    if lines <= CODE_PREVIEW_MAX_LINES && raw.len() <= CODE_PREVIEW_MAX_CHARS {
        return (raw.to_owned(), false);
    }
    let truncated = raw.split('\n').take(CODE_PREVIEW_MAX_LINES).collect::<Vec<&str>>().join("\n");
    (truncated, true)
}

type ConfirmFn<S> = Rc<dyn Fn(&mut S) + 'static>;

/// The confirm dialog builder, generic over the owning state `S` (the
/// `ui/dialog.rs` composition pattern).
#[allow(dead_code)] // ui base; the hooks/ and view tracks reach it
pub(crate) struct ConfirmDialog<'a, S: 'static> {
    key: &'a str,
    title: &'a str,
    message: &'a str,
    kind: ConfirmKind,
    badge: Option<&'a str>,
    code_preview: Option<&'a str>,
    description: Option<&'a str>,
    confirm_label: Option<&'a str>,
    cancel_label: Option<&'a str>,
    copied: bool,
    on_confirm: Option<ConfirmFn<S>>,
    on_cancel: Option<ConfirmFn<S>>,
}

#[allow(dead_code)] // ui base; the hooks/ and view tracks reach it
impl<'a, S: 'static> ConfirmDialog<'a, S> {
    /// React's required props (`open` is the caller's: nothing paints while
    /// the host has no pending request).
    pub(crate) fn new(key: &'a str, title: &'a str, message: &'a str) -> Self {
        Self {
            key,
            title,
            message,
            kind: ConfirmKind::Warning,
            badge: None,
            code_preview: None,
            description: None,
            confirm_label: None,
            cancel_label: None,
            copied: false,
            on_confirm: None,
            on_cancel: None,
        }
    }

    /// React `kind` (default warning).
    pub(crate) fn kind(mut self, kind: ConfirmKind) -> Self {
        self.kind = kind;
        self
    }

    /// React `badge`.
    pub(crate) fn badge(mut self, badge: &'a str) -> Self {
        self.badge = Some(badge);
        self
    }

    /// React `codePreview`.
    pub(crate) fn code_preview(mut self, preview: &'a str) -> Self {
        self.code_preview = Some(preview);
        self
    }

    /// React `description`.
    pub(crate) fn description(mut self, description: &'a str) -> Self {
        self.description = Some(description);
        self
    }

    /// React `confirmLabel` (defaults to `t('common.confirm')`).
    pub(crate) fn confirm_label(mut self, label: &'a str) -> Self {
        self.confirm_label = Some(label);
        self
    }

    /// React `cancelLabel` (defaults to `t('common.cancel')`).
    pub(crate) fn cancel_label(mut self, label: &'a str) -> Self {
        self.cancel_label = Some(label);
        self
    }

    /// The copied-flag projection (React `useCopyFeedback`).
    pub(crate) fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    /// The confirm action (footer OK).
    pub(crate) fn on_confirm(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_confirm = Some(Rc::new(f));
        self
    }

    /// The cancel action (footer, header close, Esc, scrim).
    pub(crate) fn on_cancel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }

    /// Paint the modal into `layer` (React `max-w-lg` = 512px = 384pt).
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) {
        let k = crate::ui::kit::kit(dark);
        let mut dialog = Dialog::new(self.key, self.title)
            .open(true)
            .width(384.0) // React max-w-lg
            .fit()
            .body_pad((15.0, 12.0)); // the panel's px-5 py-4

        // Footer: ghost cancel + danger/submit confirm (React h-8 px-3 text-xs).
        let mut cancel = Button::new(self.cancel_label.unwrap_or_else(|| t("common.cancel")))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Large);
        if let Some(on_cancel) = &self.on_cancel {
            let on_cancel = Rc::clone(on_cancel);
            cancel = cancel.on_press(move |s: &mut S| on_cancel(s));
        }
        dialog = dialog.footer(cancel.build(k).key("confirm-dialog-cancel"));

        let mut confirm = Button::new(self.confirm_label.unwrap_or_else(|| t("common.confirm")))
            .variant(match self.kind {
                ConfirmKind::Warning => ButtonVariant::Danger,
                ConfirmKind::Info => ButtonVariant::Submit,
            })
            .control_size(ControlSize::Large);
        if let Some(on_confirm) = &self.on_confirm {
            let on_confirm = Rc::clone(on_confirm);
            confirm = confirm.on_press(move |s: &mut S| on_confirm(s));
        }
        dialog = dialog.footer(confirm.build(k).key("confirm-dialog-ok"));

        dialog = dialog.child(self.body(k, dark));

        let on_cancel = self.on_cancel.clone();
        dialog = dialog.on_close(move |s: &mut S| {
            if let Some(on_cancel) = &on_cancel {
                on_cancel(s);
            }
        });
        dialog.build(k, layer);
    }

    /// The message block and the optional code preview (React space-y-3).
    fn body(&self, k: &Kit, dark: bool) -> El {
        let mut body = col().key(format!("{}-body", self.key)).w_full().gap(12.0);

        // Message row: warning glyph + message (+ badge); description below.
        let mut text_col = col().key(format!("{}-message", self.key)).gap(4.0).w_full();
        let mut message_row = row().key(format!("{}-message-row", self.key)).gap(8.0).center_y();
        if self.kind == ConfirmKind::Warning {
            message_row = message_row.child(
                icon(&ICON_ALERT_TRIANGLE, 15.0) // React h-5 w-5
                    .color(k.c.warning.fg),
            );
        }
        message_row = message_row.child(
            text(self.message.to_owned()).font(type_sm()).color(k.c.text_muted),
        );
        if let Some(badge) = self.badge {
            message_row = message_row.child(
                text(badge.to_owned())
                    .key(format!("{}-badge", self.key))
                    .font(type_10_medium())
                    .color(k.c.warning.fg)
                    .px(6.0)
                    .py(1.5)
                    .radius(9.0) // React rounded-full
                    .fill(k.c.warning.bg),
            );
        }
        text_col = text_col.child(message_row);
        if let Some(description) = self.description {
            text_col = text_col.child(
                text(description.to_owned()).font(type_xs()).color(k.c.text_muted),
            );
        }
        body = body.child(
            row()
                .key(format!("{}-message-wrap", self.key))
                .gap(12.0)
                .items_start()
                .child(text_col),
        );

        // Code preview: bordered box, header (truncation note + copy), pre.
        if let Some(preview) = self.code_preview {
            let (text_out, truncated) = truncate_code_preview(preview);
            body = body.child(self.preview_box(k, dark, text_out, truncated));
        }
        body
    }

    fn preview_box(&self, k: &Kit, dark: bool, text_out: String, truncated: bool) -> El {
        let mut header = row()
            .key(format!("{}-preview-head", self.key))
            .w_full()
            .justify_between()
            .center_y()
            .py(4.5) // React py-1.5
            .px(9.0);
        if truncated {
            header = header.child(
                text(t("query.editor.executionConfirm.previewTruncated")
                    .replace("{lines}", &CODE_PREVIEW_MAX_LINES.to_string()))
                    .font(type_10())
                    .color(k.c.text_muted),
            );
        }
        let copy = CopyButton::new("confirm-dialog-copy-sql", &text_out)
            .label(t("query.editor.executionConfirm.copySql"))
            .copied(self.copied);
        header = header.child(copy.build::<S>(dark));

        col()
            .key(format!("{}-preview", self.key))
            .w_full()
            .radius(4.5) // React rounded-md
            .border(0.75, k.c.border_base)
            .fill(k.c.layer_01)
            .child(header)
            .child(
                col()
                    .key(format!("{}-preview-pre", self.key))
                    .w_full()
                    .padding(9.0) // React p-3
                    .child(
                        text(if truncated { format!("{text_out}\n…") } else { text_out })
                            .key(format!("{}-preview-text", self.key))
                            .font(type_xs().mono()) // React font-mono text-xs
                            .color(k.c.text_muted),
                    ),
            )
    }
}

/// React `text-sm` (14px → 10.5pt).
fn type_sm() -> TextStyle {
    TextStyle::new(10.5, 400.0)
}

/// React `text-xs` (12px → 9pt).
fn type_xs() -> TextStyle {
    TextStyle::new(9.0, 400.0)
}

/// React `text-[10px]` default weight.
fn type_10() -> TextStyle {
    TextStyle::new(7.5, 400.0)
}

/// React `text-[10px] font-medium` (the badge).
fn type_10_medium() -> TextStyle {
    TextStyle::new(7.5, 500.0)
}
