//! Port of `packages/frontend/src/hooks/useThemeSync.ts` (T430) —
//! the reusable theme sync for any window: the native menu's
//! theme-change and the other windows' theme/theme-pack toggles.
//!
//! The hook is three `listenCrossWindow` subscriptions over one
//! `cancelled` guard; every delivery is a DECISION the host executes
//! — the theme APPLY itself (`applyThemeLocally` / `applyThemePack`)
//! is the document's DOM/CSS machinery (the classList toggle, the
//! pack's style element, the webview background sync, the
//! `datazen-theme` localStorage echo), all of it the host's side of
//! the seam. This port is the decision machine: which channel
//! fired, whether the delivery is live, and which action the host
//! runs.
//!
//! ## What maps where
//!
//! - The three `subscribe(...)` calls are the three channel slots of
//!   [`UseThemeSyncState`] (`menu:theme-change`,
//!   `datazen:theme-changed`, `datazen:theme-pack-changed`), each
//!   with the T413 triple: in-flight → listening / unavailable, the
//!   late-resolve rule (`if (cancelled) fn()`), and the unlisten on
//!   unmount.
//! - `menu:theme-change` is [`Self::note_menu_theme_change`]: the
//!   `mode === store's mode` no-op guard, then the host runs
//!   `applyThemeLocally(mode)` AND persists — `saveSettings` with
//!   the store's state AFTER the apply (the menu is this window's
//!   own authority: it both applies and saves).
//! - `datazen:theme-changed` is [`Self::note_theme_changed`]: the
//!   same no-op guard, then the host applies locally ONLY — the
//!   originating window already saved (this is the other-window
//!   mirror of the T11 rule: the notification carries the mode, but
//!   the persistence is not repeated here).
//! - `datazen:theme-pack-changed` is
//!   [`Self::note_theme_pack_changed`]: NO equality guard (React
//!   applies unconditionally); the host runs
//!   `applyThemePack(packId, { broadcast: false })` — the
//!   re-broadcast is off because the originating window already
//!   emitted.
//! - The `cancelled` flag + the `cleanups` array are the state's
//!   unmount arm: [`Self::unmount`] drops every late delivery and
//!   unlistens every resolved handle.
//!
//! Known deltas: none behavioural — the apply/save executions are
//! the host's, the decisions are verbatim.

// No production consumer yet (the windows' bootstrap track mounts
// it through `hooks/use_settings.rs`); this crate's tests are the
// callers meanwhile.
#![allow(dead_code)]

use crate::views::appearance_section::ThemeMode;

/// `menu:theme-change` — the native menu's theme toggle.
pub(crate) const MENU_THEME_CHANGE_CHANNEL: &str = "menu:theme-change";
/// `datazen:theme-changed` — another window's theme toggle.
pub(crate) const THEME_CHANGED_CHANNEL: &str = "datazen:theme-changed";
/// `datazen:theme-pack-changed` — another window's theme-pack toggle.
pub(crate) const THEME_PACK_CHANGED_CHANNEL: &str = "datazen:theme-pack-changed";

/// One subscription slot's lifecycle (the T413 triple).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ThemeSyncSlot {
    /// `None` while the subscribe is in flight, `Some(true)` once
    /// listening, `Some(false)` when the bus is unavailable.
    subscribed: Option<bool>,
    /// The resolved unlisten has run.
    unlistened: bool,
}

impl ThemeSyncSlot {
    /// The subscribe resolved: the window is listening on this
    /// channel — unless the unmount already began, in which case the
    /// late handle unlistens immediately (`if (cancelled) fn()`).
    pub(crate) fn note_subscribed(&mut self, cancelled: bool) {
        if cancelled {
            self.unlistened = true;
            self.subscribed = Some(false);
            return;
        }
        self.subscribed = Some(true);
    }

    /// The bus is unavailable on this channel (tests / non-Tauri):
    /// the `.catch` arm — nothing to unlisten.
    pub(crate) fn note_unavailable(&mut self) {
        self.subscribed = Some(false);
    }

    /// Whether this channel is listening.
    pub(crate) fn subscribed(&self) -> bool {
        self.subscribed == Some(true)
    }

    /// Whether the resolved unlisten has run (the late-resolve or
    /// the unmount arm).
    pub(crate) fn unlistened(&self) -> bool {
        self.unlistened
    }
}

/// `useThemeSync()`'s state machine: the three channel slots over
/// the one `cancelled` guard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct UseThemeSyncState {
    /// `menu:theme-change`.
    pub(crate) menu: ThemeSyncSlot,
    /// `datazen:theme-changed`.
    pub(crate) theme_changed: ThemeSyncSlot,
    /// `datazen:theme-pack-changed`.
    pub(crate) pack_changed: ThemeSyncSlot,
    /// The effect's `cancelled` flag (set by the cleanup).
    cancelled: bool,
}

impl UseThemeSyncState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The effect's cleanup: `cancelled = true` + every resolved
    /// unlisten.
    pub(crate) fn unmount(&mut self) {
        self.cancelled = true;
        for slot in [&mut self.menu, &mut self.theme_changed, &mut self.pack_changed] {
            if slot.subscribed() {
                slot.unlistened = true;
                slot.subscribed = Some(false);
            }
        }
    }

    /// Whether a delivery on any channel is live (the `cancelled`
    /// guard).
    pub(crate) fn live(&self) -> bool {
        !self.cancelled
    }

    /// `menu:theme-change`: answers whether the host should run the
    /// apply AND the save (`applyThemeLocally(mode)` then
    /// `saveSettings` with the post-apply store state — the menu is
    /// this window's own authority). A mode equal to the store's is
    /// the React no-op (`return`).
    pub(crate) fn note_menu_theme_change(&mut self, mode: ThemeMode, current_mode: ThemeMode) -> bool {
        self.live() && mode != current_mode
    }

    /// `datazen:theme-changed`: answers whether the host should run
    /// the local apply ONLY (the originating window saved — this
    /// side never repeats the persistence). The same no-op guard.
    pub(crate) fn note_theme_changed(&mut self, mode: ThemeMode, current_mode: ThemeMode) -> bool {
        self.live() && mode != current_mode
    }

    /// `datazen:theme-pack-changed`: answers whether the host should
    /// run `applyThemePack(packId, { broadcast: false })`. No
    /// equality guard — React applies every payload (a null packId
    /// is the built-in default).
    pub(crate) fn note_theme_pack_changed(&mut self, _pack_id: Option<&str>) -> bool {
        self.live()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mounted() -> UseThemeSyncState {
        // The effect ran: the three subscribes are in flight; the
        // host resolves them through the bus seam.
        let mut state = UseThemeSyncState::new();
        state.menu.note_subscribed(state.cancelled);
        state.theme_changed.note_subscribed(state.cancelled);
        state.pack_changed.note_subscribed(state.cancelled);
        state
    }

    #[test]
    fn the_channel_literals_are_the_react_ones() {
        assert_eq!(MENU_THEME_CHANGE_CHANNEL, "menu:theme-change");
        assert_eq!(THEME_CHANGED_CHANNEL, "datazen:theme-changed");
        assert_eq!(THEME_PACK_CHANGED_CHANNEL, "datazen:theme-pack-changed");
    }

    #[test]
    fn the_three_subscriptions_resolve_to_listening() {
        let state = mounted();
        assert!(state.menu.subscribed());
        assert!(state.theme_changed.subscribed());
        assert!(state.pack_changed.subscribed());
        // An unavailable bus on one channel is not an error (the
        // `.catch` arm); the others are unaffected.
        let mut state = UseThemeSyncState::new();
        state.menu.note_unavailable();
        assert!(!state.menu.subscribed());
        assert!(!state.menu.unlistened(), "nothing to unlisten");
        assert!(!state.theme_changed.subscribed(), "still in flight");
    }

    #[test]
    fn the_menu_change_applies_and_saves_only_on_a_real_change() {
        let mut state = mounted();
        // The no-op guard: the same mode is a no-op.
        assert!(!state.note_menu_theme_change(ThemeMode::Dark, ThemeMode::Dark));
        // A real change answers the apply+save plan.
        assert!(state.note_menu_theme_change(ThemeMode::Light, ThemeMode::Dark));
        assert!(state.note_menu_theme_change(ThemeMode::System, ThemeMode::Dark));
    }

    #[test]
    fn the_other_window_change_applies_without_saving() {
        let mut state = mounted();
        assert!(!state.note_theme_changed(ThemeMode::Light, ThemeMode::Light));
        assert!(state.note_theme_changed(ThemeMode::Light, ThemeMode::Dark));
    }

    #[test]
    fn the_pack_change_has_no_equality_guard() {
        let mut state = mounted();
        assert!(state.note_theme_pack_changed(None), "a null pack applies");
        assert!(state.note_theme_pack_changed(Some("wapp:acme.night:midnight")));
    }

    #[test]
    fn the_unmount_guards_late_deliveries_and_unlistens() {
        let mut state = mounted();
        state.unmount();
        assert!(!state.live());
        assert!(!state.menu.subscribed());
        assert!(!state.theme_changed.subscribed());
        assert!(!state.pack_changed.subscribed());
        // Every late delivery is dropped.
        assert!(!state.note_menu_theme_change(ThemeMode::Light, ThemeMode::Dark));
        assert!(!state.note_theme_changed(ThemeMode::Light, ThemeMode::Dark));
        assert!(!state.note_theme_pack_changed(None));
    }

    #[test]
    fn a_subscribe_resolving_after_the_unmount_unlistens_once() {
        let mut state = UseThemeSyncState::new();
        state.unmount();
        // All three subscribe answers arrive late: `if (cancelled)
        // fn()` — each handle unlistens immediately.
        let cancelled = !state.live();
        state.menu.note_subscribed(cancelled);
        state.theme_changed.note_subscribed(cancelled);
        state.pack_changed.note_subscribed(cancelled);
        assert!(!state.menu.subscribed());
        assert!(state.menu.unlistened(), "the late handle unlistened");
        assert!(state.theme_changed.unlistened());
        assert!(state.pack_changed.unlistened());
    }
}
