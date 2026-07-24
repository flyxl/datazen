import { create } from 'zustand';
import { settingsCommands } from '../commands/settings';
import { emitCrossWindow } from '../lib/crossWindowBus';
import type { AppSettings } from '../types';

const DEFAULT_SETTINGS: AppSettings = {
  theme: 'dark',
  language: 'en',
  limitSelectResults: true,
  queryResultLimit: 5000,
  editorFontSize: 13,
  editorFontFamily: 'Menlo, Monaco, Consolas, "Courier New", monospace',
  confirmOnDelete: true,
  autoCommit: true,
  defaultPageSize: 50,
};

const THEME_STORAGE_KEY = 'datazen-theme';

function resolveIsDark(theme: AppSettings['theme']): boolean {
  if (theme === 'system') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  }
  return theme === 'dark';
}

function applyTheme(theme: AppSettings['theme']) {
  const isDark = resolveIsDark(theme);
  document.documentElement.classList.toggle('dark', isDark);
  const isMacRounded = document.documentElement.classList.contains('macos-rounded');
  if (isMacRounded) {
    document.documentElement.style.backgroundColor = 'transparent';
  } else {
    document.documentElement.style.backgroundColor = isDark ? '#0f172a' : '#ffffff';
  }
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // localStorage might be unavailable
  }
}

function applyLanguage(language: AppSettings['language']) {
  document.documentElement.lang = language || 'en';
}

export function currentIsDark(): boolean {
  return document.documentElement.classList.contains('dark');
}

let systemThemeCleanup: (() => void) | null = null;

function watchSystemTheme(theme: AppSettings['theme']) {
  if (systemThemeCleanup) {
    systemThemeCleanup();
    systemThemeCleanup = null;
  }

  if (theme !== 'system') return;

  const mq = window.matchMedia('(prefers-color-scheme: dark)');
  const handler = () => applyTheme('system');
  mq.addEventListener('change', handler);
  systemThemeCleanup = () => mq.removeEventListener('change', handler);
}

/**
 * Apply a theme to the current window without persisting to backend.
 * Used by cross-window / menu event listeners.
 */
export function applyThemeLocally(theme: AppSettings['theme']) {
  applyTheme(theme);
  watchSystemTheme(theme);
  useSettingsStore.setState((state) => ({
    settings: { ...state.settings, theme },
  }));
}

/**
 * Apply all settings from another window without persisting.
 */
export function applySettingsLocally(incoming: AppSettings) {
  applyTheme(incoming.theme);
  applyLanguage(incoming.language);
  watchSystemTheme(incoming.theme);
  useSettingsStore.setState({ settings: incoming });
}

interface SettingsStore {
  settings: AppSettings;
  loadSettings: () => Promise<void>;
  updateSettings: (partial: Partial<AppSettings>) => Promise<void>;
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
  settings: DEFAULT_SETTINGS,

  loadSettings: async () => {
    try {
      const settings = await settingsCommands.getSettings();
      applyTheme(settings.theme);
      applyLanguage(settings.language);
      watchSystemTheme(settings.theme);
      set({ settings });
    } catch {
      applyTheme(DEFAULT_SETTINGS.theme);
      applyLanguage(DEFAULT_SETTINGS.language);
    }
  },

  updateSettings: async (partial) => {
    const next = { ...get().settings, ...partial };
    await settingsCommands.saveSettings(next);
    applyTheme(next.theme);
    applyLanguage(next.language);
    watchSystemTheme(next.theme);
    set({ settings: next });

    if (partial.theme) {
      void emitCrossWindow('datazen:theme-changed', partial.theme);
    }
    if (partial.language) {
      void import('@tauri-apps/api/core').then(({ invoke }) =>
        invoke('rebuild_menu', { language: partial.language }).catch(() => {}),
      );
    }
    void emitCrossWindow('datazen:settings-changed', next);
  },
}));
