import type { I18nKey, TranslationKey } from '../../locales';

export type SettingsSection =
  | 'general'
  | 'appearance'
  | 'dataBrowsing'
  | 'editor'
  | 'behavior'
  | 'logging'
  | 'ai'
  | 'prompts'
  | 'mcpServer'
  | 'mcpClient'
  | 'tunnels'
  | 'extensions';

/**
 * Section labels are host translation keys. `TranslationKey` is derived from the
 * zh-CN pack, and the i18n rule for development is "en only" — so a freshly
 * added en key is not part of that union yet and must be listed explicitly.
 * Listing it here still fails type-checking on a typo, unlike widening the whole
 * array to the loose `I18nKey` (`TranslationKey | string`).
 */
export type SettingsSectionLabelKey = TranslationKey | 'settings.tunnels.title';

/**
 * Nav groups only bucket the flat section list for display. Section ids, their
 * order and the `initialSection` deep-link contract stay exactly as they were —
 * a group is never a navigation target of its own.
 */
export type SettingsNavGroup = 'app' | 'integration';

export const SETTINGS_NAV_GROUPS: { id: SettingsNavGroup; labelKey: I18nKey }[] = [
  { id: 'app', labelKey: 'settings.nav.group.app' },
  { id: 'integration', labelKey: 'settings.nav.group.integration' },
];

export const SETTINGS_SECTIONS: {
  id: SettingsSection;
  labelKey: SettingsSectionLabelKey;
  group: SettingsNavGroup;
}[] = [
  { id: 'general', labelKey: 'settings.general', group: 'app' },
  { id: 'appearance', labelKey: 'settings.appearance', group: 'app' },
  { id: 'dataBrowsing', labelKey: 'settings.dataBrowsing', group: 'app' },
  { id: 'editor', labelKey: 'settings.editor', group: 'app' },
  { id: 'behavior', labelKey: 'settings.behavior', group: 'app' },
  { id: 'logging', labelKey: 'settings.logging', group: 'app' },
  { id: 'ai', labelKey: 'common.aiAssistant', group: 'integration' },
  { id: 'prompts', labelKey: 'settings.prompts', group: 'integration' },
  { id: 'mcpServer', labelKey: 'settings.mcp.title', group: 'integration' },
  { id: 'mcpClient', labelKey: 'settings.mcpClient.title', group: 'integration' },
  { id: 'tunnels', labelKey: 'settings.tunnels.title', group: 'integration' },
  { id: 'extensions', labelKey: 'settings.extensions.title', group: 'integration' },
];

export function parseSettingsSection(value: string | null | undefined): SettingsSection {
  if (value && SETTINGS_SECTIONS.some((s) => s.id === value)) {
    return value as SettingsSection;
  }
  return 'general';
}

/** Sections currently supplied by native host services. */
export const DESKTOP_SETTINGS_SECTIONS = new Set<SettingsSection>([
  'logging',
  'ai',
  'prompts',
  'mcpServer',
  'mcpClient',
  'tunnels',
  'extensions',
]);
export function settingsSectionsForBackend(desktop: boolean) {
  return SETTINGS_SECTIONS.filter(
    (section) => desktop || !DESKTOP_SETTINGS_SECTIONS.has(section.id),
  );
}
