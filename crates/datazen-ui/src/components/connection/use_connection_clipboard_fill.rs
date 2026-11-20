//! Port of `packages/frontend/src/components/connection/useConnectionClipboardFill.ts`.
//!
//! The React hook owns only guard logic: a one-shot `appliedRef`, the "form is
//! still empty" check, the password-field `://` rule, and `preventDefault` on a
//! successful paste. Clipboard reads, URL matching and form mutation belong to
//! the host. Rune has no `useEffect`, DOM events or async clipboard, so this
//! module exposes a pure [`handle_event`] step driven by [`ClipboardFillEvent`]
//! values the host delivers, with every I/O side injected through
//! [`ClipboardFillHost`]. No thread, future or fake IPC is created here.
//!
//! `autoRead` stays an option: hosts that mount the form without a user gesture
//! simply never emit [`ClipboardFillEvent::AutoRead`] (the same reason React
//! defaults it off on macOS); explicit paste events still work.

/// Scalar display-safe duplicate of `ConnectionClipboardFill.options`
/// (`Record<string, unknown>` does not cross into the Rune VM).
#[allow(dead_code)] // contract mirror; remove when the driver-sdk DTO lands in Rune.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConnectionClipboardFill {
    pub host: Option<String>,
    pub port: Option<String>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub schema: Option<String>,
    pub name: Option<String>,
    pub ssl_mode: Option<String>,
    pub options: Vec<(String, String)>,
    pub expand_advanced: bool,
}

/// Mirror of driver-sdk `MatchedConnectionClipboard`.
#[allow(dead_code)] // contract mirror; remove when the driver-sdk DTO lands in Rune.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MatchedConnectionClipboard {
    pub database_type: String,
    pub fill: ConnectionClipboardFill,
}

/// `UseConnectionClipboardFillOptions` (`autoRead` defaults to `true`).
#[allow(dead_code)] // host options; consumed by the connection form host.
#[derive(Clone, Debug)]
pub(crate) struct ClipboardFillOptions {
    pub enabled: bool,
    pub auto_read: bool,
    pub available_types: Option<Vec<String>>,
}

#[allow(dead_code)] // host options; consumed by the connection form host.
impl Default for ClipboardFillOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_read: true,
            available_types: None,
        }
    }
}

#[allow(dead_code)] // host options; consumed by the connection form host.
impl ClipboardFillOptions {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }
}

/// The two ways text reaches the hook: the mount-time clipboard read and an
/// explicit paste (Rune's host maps its paste input into the latter).
#[allow(dead_code)] // host event; consumed by the connection form host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClipboardFillEvent {
    AutoRead(String),
    Paste {
        text: String,
        from_password_field: bool,
    },
}

/// What the host should do with a clip event (React's `preventDefault` is only
/// meaningful for a paste, so auto-read never asks for it).
#[allow(dead_code)] // host outcome; consumed by the connection form host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClipboardFillOutcome {
    Ignored,
    Applied {
        database_type: String,
        prevent_default: bool,
    },
}

/// Everything the step needs from the host. `match_clipboard` is a pure parser;
/// `apply_matched` writes the form; `on_applied` reports the chosen type.
#[allow(dead_code)] // host port; consumed by the connection form host.
pub(crate) trait ClipboardFillHost {
    /// `current.name.trim() || current.password` — never overwrite user input.
    fn form_has_input(&self) -> bool;
    /// React's `appliedRef.current`.
    fn is_applied(&self) -> bool;
    fn match_clipboard(
        &self,
        text: &str,
        available_types: Option<&[String]>,
    ) -> Option<MatchedConnectionClipboard>;
    fn apply_matched(&mut self, matched: &MatchedConnectionClipboard);
    fn mark_applied(&mut self);
    fn on_applied(&mut self, database_type: &str);
}

/// Apply the hook's guard rules to one clip event. Pure with respect to I/O:
/// all reads and writes go through `host`.
#[allow(dead_code)] // host port; consumed by the connection form host.
pub(crate) fn handle_event<H: ClipboardFillHost>(
    host: &mut H,
    options: &ClipboardFillOptions,
    event: ClipboardFillEvent,
) -> ClipboardFillOutcome {
    if !options.enabled || host.is_applied() {
        return ClipboardFillOutcome::Ignored;
    }
    let (text, from_password_field, is_auto) = match event {
        ClipboardFillEvent::AutoRead(text) => (text, false, true),
        ClipboardFillEvent::Paste { text, from_password_field } => (text, from_password_field, false),
    };
    if is_auto && !options.auto_read {
        return ClipboardFillOutcome::Ignored;
    }
    if host.form_has_input() {
        return ClipboardFillOutcome::Ignored;
    }
    let Some(matched) = host.match_clipboard(&text, options.available_types.as_deref()) else {
        return ClipboardFillOutcome::Ignored;
    };
    // A password field may hold a raw connection string; a bare host:port pasted
    // there must not hijack the form.
    if from_password_field && !text.contains("://") {
        return ClipboardFillOutcome::Ignored;
    }
    host.apply_matched(&matched);
    host.mark_applied();
    host.on_applied(&matched.database_type);
    ClipboardFillOutcome::Applied {
        database_type: matched.database_type,
        prevent_default: !is_auto,
    }
}

// Leftover L1 (removal stage): the two DTOs above exist until the driver-sdk
// clipboard types are ported; swap them for the shared DTO and drop the allows.
