//! Port of `packages/frontend/src/hooks/useSettings.ts` (T404) —
//! keep this window's settings cache in sync with other windows and
//! the menu; mount in every window.
//!
//! The React file is the composition: `useSettingsSync()` (the
//! multi-window settings cache sync — T413) + `useThemeSync()` (the
//! reusable theme sync — T430). The port composes the two mounts the
//! same way: [`UseSettingsState::mount`] runs both, in the file's
//! order, and answers the pair so a window can observe which syncs
//! are live.
//!
//! ## What maps where
//!
//! - `useSettingsSync()` is the settings seam's mount (T413 ports
//!   the sync itself — the broadcast-channel listen, the
//!   menu-rebuild trigger and the settings-cache refresh; until that
//!   port lands the seam's mount flags the window as subscribed).
//! - `useThemeSync()` is the theme seam's mount (T430 ports the
//!   theme-apply loop; the same rule).
//! - Both hooks are mount-only effects with no outputs: the port's
//!   [`UseSettingsState`] answers the two mounted flags and nothing
//!   writes back.
//!
//! Known deltas: the two sub-hooks' sync logic is their own tasks'
//! (T413/T430); this composition wires their mounts and will compose
//! their full states as those land.

// No production consumer yet (the windows' bootstrap track mounts
// it); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

/// `useSettingsSync`'s mount seam (T413 ports the sync itself: the
/// settings-cache refresh the broadcast triggers).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct UseSettingsSyncMount {
    pub(crate) mounted: bool,
}

impl UseSettingsSyncMount {
    /// The effect's mount (`useSettingsSync()`): subscribe the
    /// window to the settings sync.
    pub(crate) fn mount(&mut self) {
        self.mounted = true;
    }
}

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
    settings_sync: UseSettingsSyncMount,
    theme_sync: UseThemeSyncMount,
}

impl UseSettingsState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The hook's mount: both syncs, in the React file's order.
    pub(crate) fn mount(&mut self) {
        self.settings_sync.mount();
        self.theme_sync.mount();
    }

    /// Whether the settings sync is live (`useSettingsSync`'s
    /// effect has run).
    pub(crate) fn settings_sync_mounted(&self) -> bool {
        self.settings_sync.mounted
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
    fn a_fresh_window_mounts_nothing() {
        let state = UseSettingsState::new();
        assert_eq!(state, UseSettingsState::default());
        assert!(!state.settings_sync_mounted());
        assert!(!state.theme_sync_mounted());
    }
}
