import type { UiIconId } from './iconIds';

/** Default Lucide component names for v1 semantic UI icon ids (Host fallback when no theme pack). */
export const HOST_LUCIDE_MAP: Record<UiIconId, string> = {
  'nav.settings': 'Settings',
  'nav.connections': 'Database',
  'query.run': 'Play',
  'query.stop': 'Square',
  'ai.chat': 'Bot',
  'action.backup': 'DatabaseBackup',
  'action.sync': 'Download',
  'action.refresh': 'RefreshCcw',
  'action.newConnection': 'Plus',
  'action.workflow': 'Workflow',
  'action.dashboard': 'Gauge',
  'theme.light': 'Sun',
  'theme.dark': 'Moon',
  'theme.system': 'Monitor',
};

/** Settings sidebar sections — extended ids beyond v1 catalog. */
export const SETTINGS_SECTION_LUCIDE_MAP: Record<string, string> = {
  general: 'Globe',
  dataBrowsing: 'Table2',
  editor: 'Code2',
  behavior: 'MousePointerClick',
  logging: 'FileText',
  ai: 'Bot',
  prompts: 'MessageSquareText',
  mcpServer: 'Server',
  mcpClient: 'Plug',
  monitor: 'Activity',
  extensions: 'Puzzle',
};

export function settingsSectionIconId(section: keyof typeof SETTINGS_SECTION_LUCIDE_MAP): string {
  return section === 'ai' ? 'ai.chat' : `settings.${section}`;
}

/** Combined map for bootstrap IconResolver lucideById. */
export function buildHostLucideById(): Record<string, string> {
  const map: Record<string, string> = { ...HOST_LUCIDE_MAP };
  for (const [section, name] of Object.entries(SETTINGS_SECTION_LUCIDE_MAP)) {
    const id = settingsSectionIconId(section as keyof typeof SETTINGS_SECTION_LUCIDE_MAP);
    if (!(id in map)) {
      map[id] = name;
    }
  }
  return map;
}
