export type KeymapPreset = 'default' | 'dbeaver' | 'navicat';

export type KeymapAction =
  | 'execute'
  | 'executeAll'
  | 'newQuery'
  | 'closeTab'
  | 'saveQuery'
  | 'formatSql';

export const KEYMAP_ACTIONS: { id: KeymapAction; labelKey: string }[] = [
  { id: 'execute', labelKey: 'keymap.action.execute' },
  { id: 'executeAll', labelKey: 'keymap.action.executeAll' },
  { id: 'newQuery', labelKey: 'keymap.action.newQuery' },
  { id: 'closeTab', labelKey: 'keymap.action.closeTab' },
  { id: 'saveQuery', labelKey: 'keymap.action.saveQuery' },
  { id: 'formatSql', labelKey: 'keymap.action.formatSql' },
];

export const KEYMAP_PRESETS: Record<KeymapPreset, Record<KeymapAction, string>> = {
  default: {
    execute: 'Mod-Enter',
    executeAll: 'Mod-Shift-Enter',
    newQuery: 'Mod-n',
    closeTab: 'Mod-w',
    saveQuery: 'Mod-s',
    formatSql: 'Mod-Shift-f',
  },
  dbeaver: {
    execute: 'Mod-Enter',
    executeAll: 'Alt-x',
    newQuery: 'Ctrl-]',
    closeTab: 'Mod-w',
    saveQuery: 'Mod-s',
    formatSql: 'Ctrl-Shift-f',
  },
  navicat: {
    execute: 'Mod-r',
    executeAll: 'Mod-Shift-r',
    newQuery: 'Mod-q',
    closeTab: 'Mod-w',
    saveQuery: 'Mod-s',
    formatSql: 'Mod-Shift-f',
  },
};

/**
 * Resolve the effective shortcut string for a given action.
 */
export function getActionShortcut(
  action: KeymapAction,
  preset: KeymapPreset = 'default',
  custom?: Partial<Record<KeymapAction, string>>,
): string {
  if (custom && custom[action]) {
    return custom[action]!;
  }
  const base = KEYMAP_PRESETS[preset] ?? KEYMAP_PRESETS.default;
  return base[action] ?? KEYMAP_PRESETS.default[action];
}

/**
 * Converts a shortcut string (e.g. "Mod-Enter", "Ctrl-F3") to useKeyboardShortcuts hook format (e.g. "mod+enter", "ctrl+f3").
 */
export function toShortcutHookFormat(key: string): string {
  return key
    .split('-')
    .map((p) => p.trim().toLowerCase())
    .join('+');
}

/**
 * Normalizes a shortcut string to CodeMirror keymap format (e.g. "mod+enter" -> "Mod-Enter").
 */
export function toCodeMirrorKeyFormat(key: string): string {
  const parts = key.includes('+') ? key.split('+') : key.split('-');
  return parts
    .map((part) => {
      const lower = part.trim().toLowerCase();
      if (lower === 'mod') return 'Mod';
      if (lower === 'ctrl') return 'Ctrl';
      if (lower === 'alt') return 'Alt';
      if (lower === 'shift') return 'Shift';
      if (lower === 'enter') return 'Enter';
      if (lower === 'space') return 'Space';
      if (lower === 'escape' || lower === 'esc') return 'Escape';
      if (/^f\d+$/i.test(lower)) return lower.toUpperCase();
      return lower;
    })
    .join('-');
}

/**
 * Formats a key binding for user-friendly display.
 */
export function formatShortcutForDisplay(key: string): string {
  const isMac =
    typeof navigator !== 'undefined' &&
    /mac|iphone|ipad|ipod/i.test(navigator.userAgent || navigator.platform || '');
  return key
    .split(/[-+]/)
    .map((part) => {
      const lower = part.trim().toLowerCase();
      if (lower === 'mod') return isMac ? '⌘' : 'Ctrl';
      if (lower === 'ctrl') return isMac ? '⌃' : 'Ctrl';
      if (lower === 'alt') return isMac ? '⌥' : 'Alt';
      if (lower === 'shift') return isMac ? '⇧' : 'Shift';
      if (lower === 'enter') return '↵';
      if (/^f\d+$/i.test(lower)) return lower.toUpperCase();
      return lower.toUpperCase();
    })
    .join(isMac ? '' : '+');
}
