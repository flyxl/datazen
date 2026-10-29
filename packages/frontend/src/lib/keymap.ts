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
    /mac|iphone|ipad|ipod/i.test(`${navigator.platform} ${navigator.userAgent}`);
  return key
    .split(/[-+]/)
    .map((part) => {
      const lower = part.trim().toLowerCase();
      if (lower === 'mod') return isMac ? 'Cmd' : 'Ctrl';
      if (lower === 'ctrl') return 'Ctrl';
      if (lower === 'alt') return isMac ? 'Option' : 'Alt';
      if (lower === 'shift') return 'Shift';
      if (lower === 'enter') return 'Enter';
      if (/^f\d+$/i.test(lower)) return lower.toUpperCase();
      return lower.toUpperCase();
    })
    .join('+');
}

/** Normalize platform-specific shortcut labels back to the persisted format. */
export function normalizeShortcutInput(key: string): string {
  return key
    .trim()
    .replace(/\b(cmd|command)\b/gi, 'Mod')
    .replace(/\b(option)\b/gi, 'Alt')
    .replace(/\s*([+-])\s*/g, '$1')
    .replace(/\+/g, '-')
    .split('-')
    .filter(Boolean)
    .map((part) => {
      const lower = part.toLowerCase();
      if (lower === 'mod') return 'Mod';
      if (lower === 'ctrl') return 'Ctrl';
      if (lower === 'alt') return 'Alt';
      if (lower === 'shift') return 'Shift';
      if (lower === 'enter') return 'Enter';
      if (lower === 'space') return 'Space';
      if (lower === 'escape' || lower === 'esc') return 'Escape';
      if (/^f\d+$/i.test(lower)) return lower.toUpperCase();
      return part;
    })
    .join('-');
}
