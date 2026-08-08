export type ThemeMode = 'light' | 'dark' | 'system';

export interface ThemePreference {
  mode: ThemeMode;
  packId: string | null;
}

export const DEFAULT_THEME_PREFERENCE: ThemePreference = {
  mode: 'dark',
  packId: null,
};

const MODES = new Set<ThemeMode>(['light', 'dark', 'system']);

export function normalizeThemePreference(input: unknown): ThemePreference {
  if (typeof input === 'string' && MODES.has(input as ThemeMode)) {
    return { mode: input as ThemeMode, packId: null };
  }
  if (input && typeof input === 'object') {
    const obj = input as { mode?: unknown; packId?: unknown };
    if (typeof obj.mode === 'string' && MODES.has(obj.mode as ThemeMode)) {
      const packId =
        typeof obj.packId === 'string' && obj.packId.length > 0 ? obj.packId : null;
      return { mode: obj.mode as ThemeMode, packId };
    }
  }
  return { ...DEFAULT_THEME_PREFERENCE };
}
