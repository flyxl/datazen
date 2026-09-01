import type { UiIconId } from './iconIds';

/** Default Lucide component names for v1 semantic UI icon ids (Host fallback when no theme pack). */
export const HOST_LUCIDE_MAP: Record<UiIconId, string> = {
  'nav.settings': 'Settings',
  'nav.connections': 'Database',
  'nav.workspacePages': 'LayoutGrid',
  'nav.plugins': 'Puzzle',
  'query.run': 'Play',
  'query.stop': 'Square',
  'ai.chat': 'Bot',
  'common.backupDatabase': 'DatabaseBackup',
  'common.newQuery': 'Code2',
  'common.newTable': 'TableProperties',
  'common.erDiagram': 'GitFork',
  'common.objects': 'Code2',
  'action.sync': 'Download',
  'action.refresh': 'RefreshCcw',
  'common.newConnection': 'Plus',
  'action.workflow': 'Workflow',
  'action.dashboard': 'Gauge',
  'action.serverStatus': 'Activity',
  'action.processes': 'Database',
  'action.privileges': 'KeyRound',
  'theme.light': 'Sun',
  'theme.dark': 'Moon',
  'theme.system': 'Monitor',
  'schema.database': 'Database',
  'schema.schema': 'FolderOpen',
  'schema.table': 'Table2',
  'schema.view': 'Eye',
  'schema.systemTable': 'Table2',
  'schema.function': 'Braces',
  'schema.procedure': 'Braces',
  'schema.trigger': 'Zap',
  'schema.sequence': 'Hash',
  'schema.type': 'Shapes',
  'schema.redisDatabase': 'Database',
};

/** Settings sidebar sections — extended ids beyond v1 catalog. */
export const SETTINGS_SECTION_LUCIDE_MAP: Record<string, string> = {
  general: 'Globe',
  appearance: 'Palette',
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
