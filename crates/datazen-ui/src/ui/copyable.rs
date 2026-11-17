//! Copyable error text: a selectable message with an optional copy control.
//!
//! This is a reusable base ported from `packages/ui/src/CopyableError.tsx`. No
//! `views/` caller uses it yet; it exists so later tasks can compose it instead
//! of re-styling copyable error messages by hand.
//!
//! ## What it owns
//!
//! - The message body, selectable and wrapping so long errors stay readable.
//! - A `mono` mode that renders the message in the kit's monospace type (the
//!   React `<pre>` form).
//! - An optional copy control that writes the exact message to the clipboard
//!   (`tx.copy`) when pressed.
//! - `role="alert"` when the message is not mono (a mono/technical block is not
//!   announced the same way as an inline alert in the React source).
//!
//! Everything visual comes from [`Kit`]: text colour from `c.danger.fg`,
//! secondary/hover treatment from danger tokens, type from `k.ty.text_xsmall` /
//! `k.ty.small` / `k.ty.mono`, radii from `k.radius`, and glyphs from
//! `k.glyph.{copy,check}`. No colour literals live here.
//!
//! ## What it leaves out
//!
//! - The 1500 ms "copied" confirmation flip. Implementing it needs a timer and
//!   transient state; `dialog`/`menu` keep no timers, and adding one here would
//!   require app/state machinery beyond a small local field. The copy control
//!   therefore keeps the `common.copy` label and only performs the copy; the
//!   timed label swap is recorded as a gap for a later pass that can wire a
//!   timer through existing runtime patterns.
//! - Translation is not done here except for the copy control's own label,
//!   which reads `t("common.copy")` (and would read `t("common.copied")` once
//!   the timed flip exists). The message is owned by the caller.
//!
//! ## Key convention
//!
//! | part | key             |
//! |------|-----------------|
//! | root | `<key>`         |
//! | text | `<key>-text`    |
//! | copy | `<key>-copy`    |
#![allow(dead_code)]

use rune::{col, icon, row, text, El, Tx};
use rune_kit::Kit;

use crate::i18n;

type CopyBind = Box<dyn FnOnce(El) -> El>;

/// Selectable, optionally copyable error text.
pub(crate) struct CopyableError {
    key: String,
    message: String,
    copy_button: bool,
    mono: bool,
    on_copy: Option<CopyBind>,
}

impl CopyableError {
    pub(crate) fn new(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            message: message.into(),
            copy_button: false,
            mono: false,
            on_copy: None,
        }
    }

    /// Show a copy control beside the message.
    pub(crate) fn copy_button(mut self, on: bool) -> Self {
        self.copy_button = on;
        self
    }

    /// Render in monospace (the React `<pre>` form).
    pub(crate) fn mono(mut self, mono: bool) -> Self {
        self.mono = mono;
        self
    }

    /// Called when the copy control is pressed. The control also writes the
    /// exact message to the clipboard in the same press (via `tx.copy`), so the
    /// handler runs as a press action that reaches the probe/state.
    pub(crate) fn on_copy<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        let message = self.message.clone();
        self.on_copy = Some(Box::new(move |e: El| {
            e.on_press_tx(move |state: &mut S, tx: &mut Tx| {
                tx.copy(message.clone());
                f(state);
            })
        }));
        self
    }

    /// Builds the copyable error. `S` is the runtime state type: the copy
    /// control always needs a typed press handler so it can write to the
    /// clipboard even when no [`CopyableError::on_copy`] callback was supplied.
    pub(crate) fn build<S: 'static>(self, k: &Kit) -> El {
        let text_key = format!("{}-text", self.key);

        let mut msg = text(self.message.clone())
            .key(text_key)
            .color(k.c.danger.fg)
            .selectable(true)
            .wrap_anywhere();

        msg = if self.mono {
            msg.font(k.ty.mono)
        } else {
            msg.font(k.ty.text_xsmall)
        };

        if self.copy_button {
            msg = msg.grow(1.0).min_w(0.0);
        }

        let root = if self.copy_button {
            let label = i18n::t("common.copy");
            let mut btn = row()
                .key(format!("{}-copy", self.key))
                .focusable(true)
                .shrink()
                .center_y()
                .gap(4.0)
                .radius(k.radius.sm)
                .px(8.0)
                .py(2.0)
                .label(label)
                .hover(k.alpha(k.c.danger.fg, 0.1))
                .child(icon(k.glyph.copy, 12.0).color(k.c.text_muted).a11y_hidden())
                .child(text(label).font(k.ty.small).color(k.c.text_muted));

            if let Some(handle) = self.on_copy {
                btn = handle(btn);
            } else {
                let message = self.message.clone();
                btn = btn.on_press_tx(move |_state: &mut S, tx: &mut Tx| {
                    tx.copy(message.clone());
                });
            }

            row()
                .key(self.key.clone())
                .w_full()
                .items_start()
                .gap(8.0)
                .child(msg)
                .child(btn)
        } else {
            col().key(self.key.clone()).w_full().child(msg)
        };

        if self.mono {
            root
        } else {
            root.role("alert")
        }
    }
}
