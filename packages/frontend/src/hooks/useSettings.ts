import { useSettingsSync } from './useSettingsSync';
import { useThemeSync } from './useThemeSync';

/**
 * Keep this window's settings cache in sync with other windows and the menu.
 * Mount in every window. Theme updates are handled by reusable `useThemeSync`.
 */
export function useSettings() {
  useSettingsSync();
  useThemeSync();
}
