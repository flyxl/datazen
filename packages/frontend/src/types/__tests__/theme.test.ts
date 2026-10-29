import { describe, expect, it } from 'vitest';
import { normalizeThemePreference, DEFAULT_THEME_PREFERENCE } from '../theme';

describe('normalizeThemePreference', () => {
  it('migrates legacy string', () => {
    expect(normalizeThemePreference('dark')).toEqual({ mode: 'dark', packId: null });
    expect(normalizeThemePreference('light')).toEqual({ mode: 'light', packId: null });
    expect(normalizeThemePreference('system')).toEqual({ mode: 'system', packId: null });
  });

  it('keeps object shape', () => {
    expect(normalizeThemePreference({ mode: 'light', packId: 'community.slate-blue' })).toEqual({
      mode: 'light',
      packId: 'community.slate-blue',
    });
  });

  it('falls back on garbage', () => {
    expect(normalizeThemePreference(null)).toEqual(DEFAULT_THEME_PREFERENCE);
    expect(normalizeThemePreference({ mode: 'neon' })).toEqual(DEFAULT_THEME_PREFERENCE);
  });
});
