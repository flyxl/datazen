import { useEffect } from 'react';
import { listenCrossWindow } from '../lib/crossWindowBus';
import { settingsCommands } from '../commands/settings';
import { applySettingsLocally, applyThemeLocally, useSettingsStore } from '../stores/settingsStore';
import type { AppSettings } from '../types';
import type { ThemeMode } from '../types/theme';

/**
 * Listens for theme and settings changes from native menu and other windows.
 * Must be mounted in every window that should react to those changes.
 */
export function useThemeListener() {
  useEffect(() => {
    let cancelled = false;
    const cleanups: (() => void)[] = [];

    void (async () => {
      // Native menu bar → Rust emits "menu:theme-change" to all windows
      try {
        const unlisten = await listenCrossWindow(
          'menu:theme-change',
          (payload) => {
            if (cancelled) return;
            const mode = payload as ThemeMode;
            if (mode === useSettingsStore.getState().settings.theme.mode) return;
            applyThemeLocally(mode);
            void settingsCommands.saveSettings(useSettingsStore.getState().settings);
          },
        );
        if (cancelled) unlisten();
        else cleanups.push(unlisten);
      } catch {
        // Not available
      }

      // Cross-window bus → ThemeToggle in another window
      try {
        const unlisten = await listenCrossWindow(
          'datazen:theme-changed',
          (payload) => {
            if (cancelled) return;
            const mode = payload as ThemeMode;
            if (mode === useSettingsStore.getState().settings.theme.mode) return;
            applyThemeLocally(mode);
          },
        );
        if (cancelled) unlisten();
        else cleanups.push(unlisten);
      } catch {
        // Not available
      }

      // Cross-window bus → full settings sync (font size, font family, etc.)
      try {
        const unlisten = await listenCrossWindow(
          'datazen:settings-changed',
          (payload) => {
            if (cancelled) return;
            const incoming = payload as AppSettings;
            applySettingsLocally(incoming);
          },
        );
        if (cancelled) unlisten();
        else cleanups.push(unlisten);
      } catch {
        // Not available
      }
    })();

    return () => {
      cancelled = true;
      for (const fn of cleanups) fn();
    };
  }, []);
}
