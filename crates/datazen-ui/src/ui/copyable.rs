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
//! - Timed "copied" feedback: the control shows `common.copied` / check glyph
//!   when [`CopyableError::copied`] is true (caller computes from its state,
//!   e.g. `copied_until > now`) and reverts when false. On press the control
//!   copies, schedules `tx.wake_after(1500ms)` so the runtime wakes for a
//!   redraw, and invokes the caller callback (which should record the deadline
//!   in state and re-time on repeated presses). Translation uses
//!   `t("common.copy")` / `t("common.copied")`; the message is owned by the
//!   caller.
//!
//! ## Key convention
//!
//! `CopyableError`:
//!
//! | part | key             |
//! |------|-----------------|
//! | root | `<key>`         |
//! | text | `<key>-text`    |
//! | copy | `<key>-copy`    |
//!
//! `CopyButton` (kit [`Button`] base; the root keeps the caller's key):
//!
//! | part   | key                                        |
//! |--------|--------------------------------------------|
//! | root   | `<key>`                                    |
//! | glyph  | kit-internal `icon` (hidden from AT)       |
//! | label  | kit-internal `label` (hidden from AT)      |
#![allow(dead_code)]

use std::time::Duration;

use rune::{col, icon, row, text, El, Icon, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n;
use crate::icons::{ICON_CHECK, ICON_COPY};

/// Duration the "copied" feedback stays visible after a copy press.
const COPIED_FEEDBACK_MS: u64 = 1500;

/// Height of the kit's small push button, in points.
pub(crate) const BUTTON_SM: f32 = 24.0;

/// A copy control with timed "copied" feedback, built on the kit small button.
///
/// React's history card used a plain action button, then hand-wired `tx.copy` +
/// `wake_after(1500ms)` in two identical branches. This base owns that contract:
/// a press copies [`CopyButton::text`], schedules a
/// [`COPIED_FEEDBACK_MS`] wake so the caller's state can flip back after the
/// deadline, then runs the caller callback. `copied` drives `common.copied` +
/// `ICON_CHECK`; otherwise the caller label (default `common.copy`) +
/// [`ICON_COPY`].
#[derive(Default)]
pub(crate) struct CopyButton<'a> {
    key: String,
    text: &'a str,
    icon: Option<&'static Icon>,
    label: Option<&'a str>,
    height: f32,
    copied: bool,
    help: Option<&'a str>,
    acts: Vec<Box<dyn FnOnce(El) -> El>>,
}

impl<'a> CopyButton<'a> {
    pub(crate) fn new(key: impl Into<String>, text: &'a str) -> Self {
        Self {
            key: key.into(),
            text,
            icon: None,
            label: None,
            height: BUTTON_SM,
            copied: false,
            help: None,
            acts: Vec::new(),
        }
    }

    /// Glyph shown while not copied. Defaults to [`ICON_COPY`].
    pub(crate) fn icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Label shown while not copied. Defaults to `common.copy`.
    pub(crate) fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Button height in points. Defaults to [`BUTTON_SM`].
    pub(crate) fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    /// Show feedback (`common.copied` + check glyph) when true.
    pub(crate) fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    /// Tooltip/description text (the React `title` prop).
    pub(crate) fn help(mut self, help: &'a str) -> Self {
        self.help = Some(help);
        self
    }

    /// Called after the text is copied and the feedback wake is scheduled (record
    /// `copied_until = tx.clock() + 1.5` in state).
    pub(crate) fn on_copy_tx<S: 'static>(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        let text = self.text.to_owned();
        self.acts.push(Box::new(move |e: El| {
            e.on_press_tx(move |s: &mut S, tx: &mut Tx| {
                tx.copy(text.clone());
                tx.wake_after(Duration::from_millis(COPIED_FEEDBACK_MS));
                f(s, tx);
            })
        }));
        self
    }

    /// Build the copy control. Without an `on_copy_tx` handler a press still copies
    /// and schedules the feedback wake.
    pub(crate) fn build<S: 'static>(self, dark: bool) -> El {
        let k = crate::ui::kit::kit(dark);
        let label = if self.copied {
            i18n::t("common.copied")
        } else {
            self.label.unwrap_or_else(|| i18n::t("common.copy"))
        };
        let glyph = if self.copied {
            &ICON_CHECK
        } else {
            self.icon.unwrap_or(&ICON_COPY)
        };
        let mut btn = Button::new(label)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .icon(glyph);
        if let Some(help) = self.help {
            btn = btn.help(help);
        }
        let mut e = btn.build(&k).key(self.key.clone()).h(self.height);
        if self.acts.is_empty() {
            let text = self.text.to_owned();
            e = e.on_press_tx(move |_s: &mut S, tx: &mut Tx| {
                tx.copy(text.clone());
                tx.wake_after(Duration::from_millis(COPIED_FEEDBACK_MS));
            });
        } else {
            for bind in self.acts {
                e = bind(e);
            }
        }
        e
    }
}

type CopyBind = Box<dyn FnOnce(El) -> El>;

/// Selectable, optionally copyable error text.
pub(crate) struct CopyableError {
    key: String,
    message: String,
    copy_button: bool,
    mono: bool,
    copied: bool,
    on_copy: Option<CopyBind>,
}

impl CopyableError {
    pub(crate) fn new(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            message: message.into(),
            copy_button: false,
            mono: false,
            copied: false,
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

    /// Show transient "copied" feedback (`common.copied` + check icon) when true.
    /// Caller computes this from its state, e.g. `copied_until.map_or(false, |u| now < u)`.
    pub(crate) fn copied(mut self, on: bool) -> Self {
        self.copied = on;
        self
    }

    /// Called when the copy control is pressed. The control also writes the
    /// exact message to the clipboard and schedules `tx.wake_after(1500ms)` so
    /// the runtime produces a frame for feedback revert. Handler receives `&mut Tx`
    /// (e.g. to record `copied_until = Some(tx.clock() + 1.5)` in state).
    pub(crate) fn on_copy_tx<S: 'static>(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        let message = self.message.clone();
        self.on_copy = Some(Box::new(move |e: El| {
            e.on_press_tx(move |state: &mut S, tx: &mut Tx| {
                tx.copy(message.clone());
                // Schedule a wake so the runtime produces a frame at/after expiry
                // for timed feedback revert (caller/state decide visibility).
                tx.wake_after(Duration::from_millis(COPIED_FEEDBACK_MS));
                f(state, tx);
            })
        }));
        self
    }

    /// Called when the copy control is pressed, after copying and scheduling wake.
    /// Backward-compatible variant without `Tx` access.
    pub(crate) fn on_copy<S: 'static>(self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_copy_tx(move |state: &mut S, _tx: &mut Tx| f(state))
    }

    /// Builds the copyable error. `S` is the runtime state type: the copy
    /// control always needs a typed press handler so it can write to the
    /// clipboard and schedule the feedback wake.
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
            let (label, glyph) = if self.copied {
                (i18n::t("common.copied"), k.glyph.check)
            } else {
                (i18n::t("common.copy"), k.glyph.copy)
            };
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
                .child(icon(glyph, 12.0).color(k.c.text_muted).a11y_hidden())
                .child(text(label).font(k.ty.small).color(k.c.text_muted));

            if let Some(handle) = self.on_copy {
                btn = handle(btn);
            } else {
                let message = self.message.clone();
                btn = btn.on_press_tx(move |_state: &mut S, tx: &mut Tx| {
                    tx.copy(message.clone());
                    // Schedule a wake so the runtime produces a frame at/after expiry
                    // for timed feedback revert (caller/state decide visibility).
                    tx.wake_after(Duration::from_millis(COPIED_FEEDBACK_MS));
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
