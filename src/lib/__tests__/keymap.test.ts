import { describe, expect, it } from 'vitest';
import {
  formatShortcutForDisplay,
  getActionShortcut,
  KEYMAP_PRESETS,
  normalizeShortcutInput,
  toCodeMirrorKeyFormat,
  toShortcutHookFormat,
} from '../keymap';

describe('keymap presets and resolution', () => {
  it('resolves default preset shortcuts correctly', () => {
    expect(getActionShortcut('execute', 'default')).toBe('Mod-Enter');
    expect(getActionShortcut('executeAll', 'default')).toBe('Mod-Shift-Enter');
    expect(getActionShortcut('newQuery', 'default')).toBe('Mod-n');
    expect(getActionShortcut('closeTab', 'default')).toBe('Mod-w');
  });

  it('resolves dbeaver preset shortcuts correctly', () => {
    expect(getActionShortcut('execute', 'dbeaver')).toBe('Mod-Enter');
    expect(getActionShortcut('executeAll', 'dbeaver')).toBe('Alt-x');
    expect(getActionShortcut('newQuery', 'dbeaver')).toBe('Ctrl-]');
    expect(getActionShortcut('closeTab', 'dbeaver')).toBe('Mod-w');
    expect(getActionShortcut('saveQuery', 'dbeaver')).toBe('Mod-s');
    expect(getActionShortcut('formatSql', 'dbeaver')).toBe('Ctrl-Shift-f');
  });

  it('resolves navicat preset shortcuts correctly', () => {
    expect(getActionShortcut('execute', 'navicat')).toBe('Mod-r');
    expect(getActionShortcut('executeAll', 'navicat')).toBe('Mod-Shift-r');
    expect(getActionShortcut('newQuery', 'navicat')).toBe('Mod-q');
  });

  it('custom overrides take precedence over preset', () => {
    const custom = { execute: 'F5', newQuery: 'Mod-t' };
    expect(getActionShortcut('execute', 'default', custom)).toBe('F5');
    expect(getActionShortcut('newQuery', 'dbeaver', custom)).toBe('Mod-t');
    // Non-overridden fallback to preset
    expect(getActionShortcut('executeAll', 'dbeaver', custom)).toBe('Alt-x');
  });
});

describe('toShortcutHookFormat', () => {
  it('converts CodeMirror hyphenated keys to lowercased plus format', () => {
    expect(toShortcutHookFormat('Mod-Enter')).toBe('mod+enter');
    expect(toShortcutHookFormat('Ctrl-F3')).toBe('ctrl+f3');
    expect(toShortcutHookFormat('Mod-Shift-Enter')).toBe('mod+shift+enter');
  });
});

describe('toCodeMirrorKeyFormat', () => {
  it('normalizes plus and hyphen keys to CodeMirror capitalized format', () => {
    expect(toCodeMirrorKeyFormat('mod+enter')).toBe('Mod-Enter');
    expect(toCodeMirrorKeyFormat('ctrl-enter')).toBe('Ctrl-Enter');
    expect(toCodeMirrorKeyFormat('ctrl+f3')).toBe('Ctrl-F3');
    expect(toCodeMirrorKeyFormat('alt+x')).toBe('Alt-x');
  });
});

describe('formatShortcutForDisplay', () => {
  it('uses readable platform-specific modifier names', () => {
    const originalPlatform = navigator.platform;
    const originalUserAgent = navigator.userAgent;
    Object.defineProperty(navigator, 'platform', { configurable: true, value: 'MacIntel' });
    Object.defineProperty(navigator, 'userAgent', { configurable: true, value: 'Macintosh' });
    expect(formatShortcutForDisplay('Mod-Shift-f')).toBe('Cmd+Shift+F');
    Object.defineProperty(navigator, 'platform', { configurable: true, value: originalPlatform });
    Object.defineProperty(navigator, 'userAgent', { configurable: true, value: originalUserAgent });
    expect(formatShortcutForDisplay('Mod-Shift-f')).toBe('Ctrl+Shift+F');
  });

  it('normalizes platform labels before persistence', () => {
    expect(normalizeShortcutInput('Cmd+Shift+F')).toBe('Mod-Shift-F');
    expect(normalizeShortcutInput('Ctrl+Shift+F')).toBe('Ctrl-Shift-F');
  });
});
