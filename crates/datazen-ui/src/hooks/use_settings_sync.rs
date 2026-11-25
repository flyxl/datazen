//! Port of `packages/frontend/src/hooks/useSettingsSync.ts` (T413) —
//! keep this window's settingsStore in sync when another window
//! saves settings.
//!
//! T11 (design §6): `datazen:settings-changed` is a NOTIFICATION,
//! never a trusted snapshot — the receiving end does not trust the
//! broadcast payload (it may be the sending window's stale base from
//! before its own save); it re-reads the authoritative config
//! (`loadSettings`: the generation check drops stale answers, a
//! failed refresh keeps the last good snapshot). The same window
//! updates from its own save's authoritative response, never from
//! the broadcast. The notification itself arrives on desktop via a
//! Tauri event (all windows) and on web via BroadcastChannel (all
//! tabs) — `lib/crossWindowBus.ts` (T437) owns that plumbing; this
//! hook adds no broadcast adapter of its own.
//!
//! ## What maps where
//!
//! - `listenCrossWindow('datazen:settings-changed', cb)` is the
//!   subscription seam: the host subscribes and answers through
//!   [`UseSettingsSyncState::note_subscribed`] (`Some(())` = the
//!   unlisten handle exists) or
//!   [`UseSettingsSyncState::note_unavailable`] (the `.catch` —
//!   tests / non-Tauri; Rune is a single window today, so no second
//!   window can signal it — the same ruling `main_page.rs` records).
//! - The callback's `loadSettings()` re-read is
//!   [`UseSettingsSyncState::note_settings_changed`]: the answer is
//!   the re-read PLAN (the host fires the store's load), never a
//!   snapshot — the payload is not a parameter at all (the T11
//!   rule, structurally).
//! - The `cancelled` guard is the state's unmount arm: after
//!   [`UseSettingsSyncState::unmount`], a late delivery is dropped
//!   (`if (cancelled) return;`) and the resolved unlisten fires
//!   once (`if (cancelled) fn()` — the subscribe answered after the
//!   unmount already began).
//!
//! Known deltas: none behavioural — the T11 rule is the machine.

// No production consumer yet (the windows' bootstrap track mounts
// it through `hooks/use_settings.rs`); this crate's tests are the
// callers meanwhile.
#![allow(dead_code)]

/// The cross-window notification's channel (the only literal the
/// hook carries).
pub const SETTINGS_CHANGED_CHANNEL: &str = "datazen:settings-changed";

/// The hook's state machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UseSettingsSyncState {
    /// Whether the subscription resolved: `None` while the subscribe
    /// is in flight, `Some(true)` once listening, `Some(false)` when
    /// the bus is unavailable (the `.catch` arm).
    subscribed: Option<bool>,
    /// The effect's `cancelled` flag (set by the cleanup).
    cancelled: bool,
    /// The resolved unlisten has run (the `unlisten?.()` /
    /// `if (cancelled) fn()` arms).
    unlistened: bool,
}

impl UseSettingsSyncState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The subscribe resolved: the window is listening.
    pub fn note_subscribed(&mut self) {
        if self.cancelled {
            // `if (cancelled) fn(); else unlisten = fn;` — the handle
            // unlistens immediately; nothing to keep.
            self.unlistened = true;
            self.subscribed = Some(false);
            return;
        }
        self.subscribed = Some(true);
    }

    /// The bus is unavailable (tests / non-Tauri / the single-window
    /// Rune build): the `.catch` arm — nothing to unlisten.
    pub fn note_unavailable(&mut self) {
        self.subscribed = Some(false);
    }

    /// Whether the window is listening (the subscription's answer).
    pub fn subscribed(&self) -> bool {
        self.subscribed == Some(true)
    }

    /// The notification arrived: answers whether the host should
    /// re-read the authoritative settings (`loadSettings`). The
    /// payload is deliberately absent — a notification, never a
    /// snapshot (T11: the receiving end re-reads; the broadcast's
    /// payload is not a trusted base).
    pub fn note_settings_changed(&mut self) -> bool {
        !self.cancelled
    }

    /// The effect's cleanup: `cancelled = true` + the unlisten.
    pub fn unmount(&mut self) {
        self.cancelled = true;
        if self.subscribed == Some(true) {
            // `unlisten?.()`
            self.unlistened = true;
            self.subscribed = Some(false);
        }
    }

    /// Whether the unlisten has run.
    pub fn unlistened(&self) -> bool {
        self.unlistened
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subscription_resolves_to_listening() {
        let mut state = UseSettingsSyncState::new();
        assert!(!state.subscribed(), "in flight");
        state.note_subscribed();
        assert!(state.subscribed());
        assert!(!state.unlistened(), "still listening");
    }

    #[test]
    fn an_unavailable_bus_is_not_an_error() {
        let mut state = UseSettingsSyncState::new();
        state.note_unavailable();
        assert!(!state.subscribed());
        assert!(!state.unlistened(), "nothing to unlisten");
    }

    #[test]
    fn the_notification_answers_a_re_read_never_a_snapshot() {
        let mut state = UseSettingsSyncState::new();
        state.note_subscribed();
        assert!(state.note_settings_changed());
        // The API carries no payload — the T11 rule is structural:
        // `note_settings_changed` takes nothing but the fact.
    }

    #[test]
    fn the_unmount_guards_late_deliveries_and_unlistens() {
        let mut state = UseSettingsSyncState::new();
        state.note_subscribed();
        state.unmount();
        assert!(state.unlistened(), "unlisten?.() ran");
        assert!(!state.subscribed());
        // A late delivery is dropped.
        assert!(!state.note_settings_changed(), "the cancelled guard");
    }

    #[test]
    fn a_subscribe_resolving_after_the_unmount_unlistens_once() {
        let mut state = UseSettingsSyncState::new();
        state.unmount();
        // The subscribe answers late: `if (cancelled) fn()`.
        state.note_subscribed();
        assert!(state.unlistened(), "the late handle unlistened");
        assert!(!state.subscribed());
    }

    #[test]
    fn the_channel_literal_is_the_react_one() {
        assert_eq!(SETTINGS_CHANGED_CHANNEL, "datazen:settings-changed");
    }
}
