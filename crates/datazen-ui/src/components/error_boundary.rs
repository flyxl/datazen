//! T063: `components/ErrorBoundary.tsx` ported to Rune.
//!
//! React mounts this around the whole app (`App.tsx:98`): a class
//! component whose `getDerivedStateFromError` flips `hasError` and keeps
//! the error, `componentDidCatch` logs it, and the fallback renders a
//! full-screen card — warning glyph, `common.error` heading, the error
//! message (or `backend.unknownError`), a copy button (stack, else
//! message, else `backend.unknownError`), a `common.close` dismiss
//! button that clears the state and re-renders the children, and a
//! `common.retry` button that reloads the page. It also hides the
//! `#splash` element on the error render.
//!
//! Rune is immediate mode and has no render-error interception, so the
//! boundary cannot catch a child's failure by itself: the host detects
//! the failure (its panic hook / render guard) and reports it with
//! [`ErrorBoundaryState::capture`]. The boundary itself is the state
//! plus the fallback render; the host owns detection, logging and the
//! reload. The production consumer is the app-root wiring (not yet
//! done); until then the component tests are the only callers, so
//! reusable API items carry per-item `#[allow(dead_code)]` (not a
//! module-wide blanket).
//!
//! ## Keys (prefix defaults to `error-boundary`)
//!
//! | part | key |
//! |------|-----|
//! | root (fallback only; children render unkeyed by the boundary) | `<prefix>` |
//! | card | `<prefix>-card` |
//! | warning icon | `<prefix>-icon` |
//! | heading | `<prefix>-heading` |
//! | message box | `<prefix>-message-box` |
//! | message text | `<prefix>-message` |
//! | copy button | `<prefix>-copy` |
//! | actions row | `<prefix>-actions` |
//! | close button | `<prefix>-close` |
//! | retry button | `<prefix>-retry` |
//!
//! ## Deliberate deviations from React, point by point
//!
//! * **No automatic capture.** React's `getDerivedStateFromError` /
//!   `componentDidCatch` fire when a child render throws; Rune has no
//!   equivalent, so nothing in this module observes a failure — the
//!   host must call `capture` when its own error path trips. Until the
//!   app-root wiring exists, a rendering failure will surface exactly
//!   as it does today (through the host), not through this fallback.
//! * **No logging in `capture`.** React's `componentDidCatch` writes
//!   `console.error('[ErrorBoundary]', { name, messageLength,
//!   componentStack })`; `datazen-ui` has no logging sink, so the host
//!   logs at the capture site, where the name and stack are known.
//! * **No splash handling.** React hides `#splash` (and removes it
//!   after 350 ms) on the error render; Rune has no DOM splash to
//!   reach, and the shell owns its own splash outside the boundary.
//! * **No `WindowChromeSlot`.** React renders the chrome slot at the
//!   top of the fallback (desktop `TitleBar`, nothing on web); the
//!   Rune window's own title band is owned by the host above the
//!   boundary, so the fallback fills the remaining area with no chrome
//!   of its own.
//! * **Reload is a host callback.** React's retry calls
//!   `window.location.reload()`; here `on_reload` is the host's
//!   reload, and without it the retry button renders but does nothing
//!   (the same no-callback convention as `QueryArtifactExport`).
//! * **Copy is a host callback.** React writes to
//!   `navigator.clipboard`; here `on_copy` receives [`copy_text`],
//!   and without it the copy row renders but does nothing.
//! * **Warning glyph, not emoji.** React shows a `⚠️` emoji at
//!   `text-4xl`; icons go through the `icons.rs` registry only, so
//!   the fallback shows [`ICON_ALERT_TRIANGLE`] at 36pt in the
//!   warning tone.
//! * **Message fallbacks keep JS `||` semantics.**
//!   [`display_message`] and [`copy_text`] treat an empty message /
//!   stack as absent, exactly as React's `error?.message || …` and
//!   `error?.stack || error?.message || …` do.

use rune::{col, icon, row, text, Align, El, LiveRegion, ScrollSpec, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::ICON_ALERT_TRIANGLE;
use crate::ui::kit::kit;

/// What React keeps in state: the caught `Error`'s message and stack.
/// `stack` is `None` when the host only has a message (React's
/// `error.stack` is always present on a real `Error`, but a host that
/// reports a failure as a plain string has no stack to give).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired at the app root
pub(crate) struct CapturedError {
    pub message: String,
    pub stack: Option<String>,
}

/// React's `State`: whether the fallback shows, and the error behind it.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // reusable component; wired at the app root
pub(crate) struct ErrorBoundaryState {
    pub has_error: bool,
    pub error: Option<CapturedError>,
}

#[allow(dead_code)] // reusable component; wired at the app root
impl ErrorBoundaryState {
    /// The host's report of a failure: React's `getDerivedStateFromError`
    /// entry. Sets `has_error` and keeps the error; a later capture
    /// replaces the previous error, as a second React throw would.
    pub(crate) fn capture(&mut self, message: impl Into<String>, stack: Option<String>) {
        self.has_error = true;
        self.error = Some(CapturedError {
            message: message.into(),
            stack,
        });
    }

    /// React's `handleDismiss`: clears the state so the children render
    /// again. (React re-renders the same children, which may throw
    /// again — the host sees a fresh capture if they do.)
    pub(crate) fn dismiss(&mut self) {
        self.has_error = false;
        self.error = None;
    }

    /// React's `{error?.message || t('backend.unknownError')}`: the
    /// message, or the fallback when there is no error or its message
    /// is empty.
    pub(crate) fn display_message(&self) -> String {
        match self
            .error
            .as_ref()
            .and_then(|error| non_empty(&error.message))
        {
            Some(message) => message.to_owned(),
            None => t("backend.unknownError").to_owned(),
        }
    }

    /// React's copy content: `error?.stack || error?.message ||
    /// t('backend.unknownError')` — the stack when it has one, else the
    /// message, else the fallback. Empty strings count as absent on
    /// both steps, as JS `||` does.
    pub(crate) fn copy_text(&self) -> String {
        let stack = self
            .error
            .as_ref()
            .and_then(|error| error.stack.as_deref())
            .and_then(non_empty);
        if let Some(stack) = stack {
            return stack.to_owned();
        }
        self.display_message()
    }
}

/// JS `||` on a string: empty counts as absent.
fn non_empty(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Reads the boundary state out of the app's `S`.
#[allow(dead_code)] // reusable component; wired at the app root
pub(crate) type ReadBoundaryStateFn<S> = fn(&S) -> &ErrorBoundaryState;

/// React's `handleDismiss`, on the app's `S`.
#[allow(dead_code)] // reusable component; wired at the app root
pub(crate) type DismissBoundaryFn<S> = fn(&mut S);

/// React's `handleReload` (`window.location.reload()`), on the app's `S`.
#[allow(dead_code)] // reusable component; wired at the app root
pub(crate) type ReloadFn<S> = fn(&mut S);

/// The clipboard write for [`ErrorBoundaryState::copy_text`], on the app's
/// `S` (React's `navigator.clipboard.writeText`).
#[allow(dead_code)] // reusable component; wired at the app root
pub(crate) type CopyErrorFn<S> = fn(&mut S, String);

/// The boundary. `build` renders `children` unchanged while there is no
/// error, and the full-screen fallback once the host has captured one.
#[allow(dead_code)] // reusable component; wired at the app root
pub(crate) struct ErrorBoundary<S: 'static> {
    key: String,
    read_state: ReadBoundaryStateFn<S>,
    dismiss: DismissBoundaryFn<S>,
    on_reload: Option<ReloadFn<S>>,
    on_copy: Option<CopyErrorFn<S>>,
}

#[allow(dead_code)] // reusable component; wired at the app root
impl<S: 'static> ErrorBoundary<S> {
    pub(crate) fn new(read_state: ReadBoundaryStateFn<S>, dismiss: DismissBoundaryFn<S>) -> Self {
        Self {
            key: "error-boundary".to_string(),
            read_state,
            dismiss,
            on_reload: None,
            on_copy: None,
        }
    }

    /// Override the key prefix.
    pub(crate) fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// The host's reload for the retry button; without it the button
    /// renders but does nothing.
    pub(crate) fn on_reload(mut self, on_reload: ReloadFn<S>) -> Self {
        self.on_reload = Some(on_reload);
        self
    }

    /// The host's clipboard write for the copy button; without it the
    /// button renders but does nothing.
    pub(crate) fn on_copy(mut self, on_copy: CopyErrorFn<S>) -> Self {
        self.on_copy = Some(on_copy);
        self
    }

    /// Renders the children, or the fallback when the state carries an
    /// error. `children` is the host's app content, already keyed by
    /// its own builder.
    pub(crate) fn build(self, s: &S, children: El, dark: bool) -> El {
        let state = (self.read_state)(s);
        if !state.has_error {
            return children;
        }
        let k = kit(dark);
        self.fallback(state, &k)
    }

    /// React's fallback: `h-screen w-screen flex-col bg-surface`, the
    /// centred `max-w-md space-y-4 text-center` card. The message box
    /// is `max-h-40 overflow-auto rounded bg-surface-alt p-2 font-mono
    /// text-xs text-danger`, the copy row is a 12pt accent text button,
    /// and the actions are a bordered close button and an accent retry
    /// button (`gap-3`, centred).
    fn fallback(&self, state: &ErrorBoundaryState, k: &Kit) -> El {
        let message = state.display_message();
        let copy_content = state.copy_text();

        let mut copy = row()
            .key(format!("{}-copy", self.key))
            .role("button")
            .focusable(true)
            .items_center()
            .gap(4.0)
            .child(
                text(t("common.copy"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_accent),
            );
        if let Some(on_copy) = self.on_copy {
            copy = copy.on_press(move |s: &mut S| on_copy(s, copy_content.clone()));
        }

        let dismiss = self.dismiss;
        let close = Button::new(t("common.close"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .on_press(move |s: &mut S| dismiss(s))
            .build(k)
            .key(format!("{}-close", self.key));

        let mut retry = Button::new(t("common.retry"))
            .variant(ButtonVariant::Submit)
            .control_size(ControlSize::Small);
        if let Some(on_reload) = self.on_reload {
            retry = retry.on_press(move |s: &mut S| on_reload(s));
        }
        let retry = retry.build(k).key(format!("{}-retry", self.key));

        col()
            .key(self.key.clone())
            .w_full()
            .h_full()
            .items_center()
            .justify_center()
            .fill(k.c.surface_base)
            .padding(32.0)
            .child(
                col()
                    .key(format!("{}-card", self.key))
                    .max_w(448.0)
                    .gap(16.0)
                    .items_center()
                    .child(
                        icon(&ICON_ALERT_TRIANGLE, 36.0)
                            .key(format!("{}-icon", self.key))
                            .color(k.c.warning.fg)
                            .a11y_hidden(),
                    )
                    .child(
                        text(t("common.error"))
                            .key(format!("{}-heading", self.key))
                            .font(TextStyle::new(18.0, 600.0))
                            .color(k.c.text_base)
                            .align(Align::Center)
                            .role("heading"),
                    )
                    .child(
                        col()
                            .key(format!("{}-message-box", self.key))
                            .w_full()
                            .max_h(160.0)
                            .scroll(ScrollSpec {
                                stick_bottom: false,
                            })
                            .radius(k.radius.md)
                            .fill(k.c.layer_01)
                            .padding(8.0)
                            .child(
                                text(message)
                                    .key(format!("{}-message", self.key))
                                    .font(k.ty.mono)
                                    .color(k.c.danger.fg)
                                    .align(Align::Center)
                                    .selectable(true)
                                    .role("alert")
                                    .live_region(LiveRegion::Assertive),
                            ),
                    )
                    .child(copy)
                    .child(
                        row()
                            .key(format!("{}-actions", self.key))
                            .justify_center()
                            .gap(12.0)
                            .child(close)
                            .child(retry),
                    ),
            )
    }
}
