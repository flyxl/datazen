import { useEffect } from 'react';
import { listenCrossWindow } from '../lib/crossWindowBus';
import { settingsCommands } from '../commands/settings';
import { applyThemeLocally, useSettingsStore } from '../stores/settingsStore';
import type { ThemeMode } from '../types/theme';

/**
 * Reusable theme sync for any window: native menu + other windows' theme toggles.
 */
export function useThemeSync() {
  useEffect(() => {
    let cancelled = false;
    const cleanups: (() => void)[] = [];

    const subscribe = (event: string, onPayload: (payload: unknown) => void) => {
      void listenCrossWindow(event, (payload) => {
        if (!cancelled) onPayload(payload);
      })
        .then((fn) => {
          if (cancelled) fn();
          else cleanups.push(fn);
        })
        .catch(() => {
          // Not available (tests / non-Tauri)
        });
    };

    subscribe('menu:theme-change', (payload) => {
      const mode = payload as ThemeMode;
      if (mode === useSettingsStore.getState().settings.theme.mode) return;
      void (async () => {
        await applyThemeLocally(mode);
        void settingsCommands.saveSettings(useSettingsStore.getState().settings);
      })();
    });

    subscribe('datazen:theme-changed', (payload) => {
      const mode = payload as ThemeMode;
      if (mode === useSettingsStore.getState().settings.theme.mode) return;
      void applyThemeLocally(mode);
    });

    return () => {
      cancelled = true;
      for (const fn of cleanups) fn();
    };
  }, []);
}
