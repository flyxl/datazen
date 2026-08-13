import { useEffect } from 'react';
import { listenCrossWindow } from '../lib/crossWindowBus';
import { applySettingsLocally } from '../stores/settingsStore';
import type { AppSettings } from '../types';

/** Keep this window's settingsStore in sync when another window saves settings. */
export function useSettingsSync() {
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void listenCrossWindow('datazen:settings-changed', (payload) => {
      if (cancelled) return;
      void applySettingsLocally(payload as AppSettings);
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Not available (tests / non-Tauri)
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
