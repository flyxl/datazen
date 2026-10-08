import { describe, expect, it } from 'vitest';
import { parseSettingsSection, SETTINGS_NAV_GROUPS, SETTINGS_SECTIONS } from '../settingsSections';

describe('settingsSections registration', () => {
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

  it('assigns every section to exactly one declared nav group, preserving flat order', () => {
    // Grouping is display-only: ids and order must stay byte-stable so
    // `initialSection` deep-links and nav tests keep working.
    expect(SETTINGS_SECTIONS.map((s) => s.id)).toEqual([
      'general',
      'appearance',
      'dataBrowsing',
      'editor',
      'behavior',
      'logging',
      'ai',
      'prompts',
      'mcpServer',
      'mcpClient',
      'tunnels',
      'extensions',
    ]);
    expect(SETTINGS_NAV_GROUPS.map((g) => g.id)).toEqual(['app', 'integration']);
    for (const section of SETTINGS_SECTIONS) {
      expect(SETTINGS_NAV_GROUPS.filter((g) => g.id === section.group)).toHaveLength(1);
    }
    // Original relative order inside each group.
    expect(SETTINGS_SECTIONS.filter((s) => s.group === 'app').map((s) => s.id)).toEqual([
      'general',
      'appearance',
      'dataBrowsing',
      'editor',
      'behavior',
      'logging',
    ]);
    expect(SETTINGS_SECTIONS.filter((s) => s.group === 'integration').map((s) => s.id)).toEqual([
      'ai',
      'prompts',
      'mcpServer',
      'mcpClient',
      'tunnels',
      'extensions',
    ]);
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
