//! Port of `packages/frontend/src/hooks/useSettings.ts` (T404) —
//! keep this window's settings cache in sync with other windows and
//! the menu; mount in every window.
//!
//! The React file is the composition: `useSettingsSync()` (the
//! multi-window settings cache sync — T413) + `useThemeSync()` (the
//! reusable theme sync — T430, now landed). The port composes the
//! two mounts the same way: [`UseSettingsState::mount`] runs both,
//! in the file's order, with the settings sync's machine
//! ([`UseSettingsSyncState`]) and the theme sync's machine
//! ([`UseThemeSyncState`]) riding along — the host drives their
//! subscription lifecycles and the deliveries through them.
//!
//! ## What maps where
//!
//! - `useSettingsSync()` is the settings sync machine's mount: the
//!   effect fires the `listenCrossWindow` subscribe (the state
//!   starts in flight; the host resolves it through
//!   `note_subscribed`/`note_unavailable` and delivers through
//!   `note_settings_changed` — the T11 re-read rule).
//! - `useThemeSync()` is the theme sync machine's mount (T430): the
//!   three channel subscribes start in flight; the host resolves
//!   and delivers through the same slots.
//! - Both hooks are mount-only effects with no outputs of their
//!   own: the composition answers the two mounted flags and the two
//!   machines.
//!
//! Known deltas: none — the theme seam's T404 stand-in is retired
//! by T430's machine.

// No production consumer yet (the windows' bootstrap track mounts
// it); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use super::use_settings_sync::UseSettingsSyncState;
use super::use_theme_sync::UseThemeSyncState;

/// `useSettings()` — the composition mounted in every window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UseSettingsState {
    mounted: bool,
    /// `useSettingsSync`'s machine (T413): the subscribe lifecycle
    /// and the notification deliveries ride it.
    pub settings_sync: UseSettingsSyncState,
    /// `useThemeSync`'s machine (T430): the three theme channels'
    /// subscribe lifecycles and the delivery decisions ride it.
    pub theme_sync: UseThemeSyncState,
}

impl UseSettingsState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The hook's mount: both syncs, in the React file's order. The
    /// settings sync's subscribe is in flight from here (the host
    /// resolves it through the bus seam); the theme sync's three
    /// subscribes are in flight the same way (T430).
    pub fn mount(&mut self) {
        self.mounted = true;
    }

    /// Whether the settings sync's effect has run (the composition's
    /// own flag; the subscription's resolution is
    /// [`Self::settings_sync`]'s).
    pub fn settings_sync_mounted(&self) -> bool {
        self.mounted
    }

    /// Whether the theme sync is live (`useThemeSync`'s effect has
    /// run — the machine's slots ride [`Self::theme_sync`]).
    pub fn theme_sync_mounted(&self) -> bool {
        self.mounted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_runs_both_syncs() {
        let mut state = UseSettingsState::new();
        assert!(!state.settings_sync_mounted());
        assert!(!state.theme_sync_mounted());
        state.mount();
        assert!(state.settings_sync_mounted(), "useSettingsSync ran");
        assert!(state.theme_sync_mounted(), "useThemeSync ran");
    }

    #[test]
    fn the_settings_sync_machine_rides_the_composition() {
        let mut state = UseSettingsState::new();
        state.mount();
        // The host resolves the subscribe and delivers a
        // notification through the same machine the sync's own port
        // carries (the T11 re-read).
        state.settings_sync.note_subscribed();
        assert!(state.settings_sync.subscribed());
        assert!(state.settings_sync.note_settings_changed());
    }

    #[test]
    fn the_theme_sync_machine_rides_the_composition() {
        use crate::views::appearance_section::ThemeMode;

        let mut state = UseSettingsState::new();
        state.mount();
        // The host resolves the three theme subscribes and delivers
        // through the same slots T430's own port carries.
        let cancelled = !state.theme_sync.live();
        state.theme_sync.menu.note_subscribed(cancelled);
        state.theme_sync.theme_changed.note_subscribed(cancelled);
        state.theme_sync.pack_changed.note_subscribed(cancelled);
        assert!(state.theme_sync.menu.subscribed());
        // The menu's toggle decides the apply+save plan.
        assert!(state
            .theme_sync
            .note_menu_theme_change(ThemeMode::Light, ThemeMode::Dark));
    }

    #[test]
    fn a_fresh_window_mounts_nothing() {
        let state = UseSettingsState::new();
        assert_eq!(state, UseSettingsState::default());
        assert!(!state.settings_sync_mounted());
        assert!(!state.theme_sync_mounted());
    }
}
