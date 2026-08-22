import { describe, expect, it } from 'vitest';
import { parseSettingsSection, SETTINGS_SECTIONS } from '../settingsSections';

describe('settingsSections (F7 registration)', () => {
  it('registers appearance as the second top-level settings menu item', () => {
    expect(SETTINGS_SECTIONS.map((s) => s.id)).toContain('appearance');
    expect(SETTINGS_SECTIONS[1]?.id).toBe('appearance');
    expect(SETTINGS_SECTIONS.find((s) => s.id === 'appearance')?.labelKey).toBe(
      'settings.appearance',
    );
  });

  it('does not keep a legacy theme-pack section reachable', () => {
    expect(SETTINGS_SECTIONS.map((s) => s.id)).not.toContain('themePack');
    expect(SETTINGS_SECTIONS.map((s) => s.id)).not.toContain('theme-pack');
  });

  it('parseSettingsSection: deep-links to appearance work; unknown/legacy ids fall back to general', () => {
    expect(parseSettingsSection('appearance')).toBe('appearance');
    expect(parseSettingsSection('theme-pack')).toBe('general');
    expect(parseSettingsSection('themePack')).toBe('general');
    expect(parseSettingsSection('nope')).toBe('general');
    expect(parseSettingsSection(null)).toBe('general');
    expect(parseSettingsSection(undefined)).toBe('general');
  });
});
