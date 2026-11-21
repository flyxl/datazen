//! Port of `packages/frontend/src/hooks/useSettings.ts` (T404) —
//! keep this window's settings cache in sync with other windows and
//! the menu; mount in every window.
//!
//! The React file is the composition: `useSettingsSync()` (the
//! multi-window settings cache sync — T413, now landed) +
//! `useThemeSync()` (the reusable theme sync — T430). The port
//! composes the two mounts the same way: [`UseSettingsState::mount`]
//! runs both, in the file's order — the settings sync's machine
//! ([`UseSettingsSyncState`]) rides along so the host can drive the
//! subscription lifecycle and the notification deliveries, and the
//! theme seam's mount flag stands in until T430 lands.
//!
//! ## What maps where
//!
//! - `useSettingsSync()` is the settings sync machine's mount: the
//!   effect fires the `listenCrossWindow` subscribe (the state
//!   starts in flight; the host resolves it through
//!   `note_subscribed`/`note_unavailable` and delivers through
//!   `note_settings_changed` — the T11 re-read rule).
//! - `useThemeSync()` is the theme seam's mount (T430 ports the
//!   theme-apply loop; the same rule).
//! - Both hooks are mount-only effects with no outputs of their
//!   own: the composition answers the two mounted flags.
//!
//! Known deltas: the theme sync's full machine is T430's; this
//! composition wires its mount and will compose its state as it
//! lands.

// No production consumer yet (the windows' bootstrap track mounts
// it); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use super::use_settings_sync::UseSettingsSyncState;

/// `useThemeSync`'s mount seam (T430 ports the theme-apply loop).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct UseThemeSyncMount {
    pub(crate) mounted: bool,
}

impl UseThemeSyncMount {
    /// The effect's mount (`useThemeSync()`): subscribe the window
    /// to the theme sync.
    pub(crate) fn mount(&mut self) {
        self.mounted = true;
    }
}

/// `useSettings()` — the composition mounted in every window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct UseSettingsState {
    mounted: bool,
    /// `useSettingsSync`'s machine (T413): the subscribe lifecycle
    /// and the notification deliveries ride it.
    pub(crate) settings_sync: UseSettingsSyncState,
    theme_sync: UseThemeSyncMount,
}

impl UseSettingsState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The hook's mount: both syncs, in the React file's order. The
    /// settings sync's subscribe is in flight from here (the host
    /// resolves it through the bus seam).
    pub(crate) fn mount(&mut self) {
        self.mounted = true;
        self.theme_sync.mount();
    }

    /// Whether the settings sync's effect has run (the composition's
    /// own flag; the subscription's resolution is
    /// [`Self::settings_sync`]'s).
    pub(crate) fn settings_sync_mounted(&self) -> bool {
        self.mounted
    }

    /// Whether the theme sync is live (`useThemeSync`'s effect has
    /// run).
    pub(crate) fn theme_sync_mounted(&self) -> bool {
        self.theme_sync.mounted
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
    fn a_fresh_window_mounts_nothing() {
        let state = UseSettingsState::new();
        assert_eq!(state, UseSettingsState::default());
        assert!(!state.settings_sync_mounted());
        assert!(!state.theme_sync_mounted());
    }
}
