import type { TranslationKey } from '../../locales';

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
  | 'extensions';

export const SETTINGS_SECTIONS: { id: SettingsSection; labelKey: TranslationKey }[] = [
  { id: 'general', labelKey: 'settings.general' },
  { id: 'appearance', labelKey: 'settings.appearance' },
  { id: 'dataBrowsing', labelKey: 'settings.dataBrowsing' },
  { id: 'editor', labelKey: 'settings.editor' },
  { id: 'behavior', labelKey: 'settings.behavior' },
  { id: 'logging', labelKey: 'settings.logging' },
  { id: 'ai', labelKey: 'settings.ai' },
  { id: 'prompts', labelKey: 'settings.prompts' },
  { id: 'mcpServer', labelKey: 'mcp.title' },
  { id: 'mcpClient', labelKey: 'mcpClient.title' },
  { id: 'extensions', labelKey: 'settings.extensions.title' },
];

export function parseSettingsSection(value: string | null | undefined): SettingsSection {
  if (value && SETTINGS_SECTIONS.some((s) => s.id === value)) {
    return value as SettingsSection;
  }
  return 'general';
}
