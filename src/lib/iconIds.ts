export const UI_ICON_IDS = [
  'nav.settings',
  'nav.connections',
  'query.run',
  'query.stop',
  'ai.chat',
  'action.backup',
  'action.sync',
  'action.refresh',
  'action.newConnection',
  'action.workflow',
  'theme.light',
  'theme.dark',
  'theme.system',
] as const;

export type UiIconId = (typeof UI_ICON_IDS)[number];
