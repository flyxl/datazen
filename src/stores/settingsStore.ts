import { create } from 'zustand';
import { settingsCommands } from '../commands/settings';
import { emitCrossWindow } from '../lib/crossWindowBus';
import { resolveUiLanguage } from '../lib/resolveUiLanguage';
import type { AppSettings } from '../types';
import { HOST_DEFAULT_EDITOR_FONT } from '../lib/resolveEditorFontFamily';
import {
  DEFAULT_THEME_PREFERENCE,
  normalizeThemePreference,
  type ThemeMode,
} from '../types/theme';
import { DEFAULT_MONITOR_SETTINGS } from '../types/dashboard';
import { applyThemePack, syncWebviewBackgroundFromTokens } from '../lib/themePackApply';

const DEFAULT_SETTINGS: AppSettings = {
  theme: DEFAULT_THEME_PREFERENCE,
  language: 'en',
  limitSelectResults: true,
  queryResultLimit: 5000,
  editorFontSize: 13,
  editorFontFamily: HOST_DEFAULT_EDITOR_FONT,
  confirmOnDelete: true,
  autoCommit: true,
  defaultPageSize: 50,
  connectionPoolSize: 10,
  logLevel: 'info',
  logPath: '',
  mcpServerEnabled: false,
  mcpDisabledTools: [],
  mcpPermissionMode: 'safe_write',
  mcpAllowedConnectionIds: [],
  contextDir: '',
  checkForUpdatesOnStartup: false,
  autoChartOnQuery: true,
  monitor: DEFAULT_MONITOR_SETTINGS,
  pluginSettings: {},
};

const THEME_STORAGE_KEY = 'datazen-theme';

function resolveIsDark(mode: ThemeMode): boolean {
  if (mode === 'system') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  }
  return mode === 'dark';
}

async function applyTheme(mode: ThemeMode, packId: string | null) {
  const isDark = resolveIsDark(mode);
  document.documentElement.classList.toggle('dark', isDark);
  const result = await applyThemePack(packId);
  if (!result.ok) {
    throw new Error(result.error);
  }
  syncWebviewBackgroundFromTokens();
  try {
    localStorage.setItem(THEME_STORAGE_KEY, mode);
  } catch {
    // localStorage might be unavailable
  }
}

export function currentIsDark(): boolean {
  return document.documentElement.classList.contains('dark');
}

let systemThemeCleanup: (() => void) | null = null;

function watchSystemTheme(mode: ThemeMode) {
  if (systemThemeCleanup) {
    systemThemeCleanup();
    systemThemeCleanup = null;
  }

  if (mode !== 'system') return;

  const mq = window.matchMedia('(prefers-color-scheme: dark)');
  const handler = () => {
    const { mode, packId } = useSettingsStore.getState().settings.theme;
    void applyTheme(mode, packId);
  };
  mq.addEventListener('change', handler);
  systemThemeCleanup = () => mq.removeEventListener('change', handler);
}

/**
 * Apply a theme to the current window without persisting to backend.
 * Used by cross-window / menu event listeners.
 */
export async function applyThemeLocally(mode: ThemeMode) {
  const packId = useSettingsStore.getState().settings.theme.packId;
  await applyTheme(mode, packId);
  watchSystemTheme(mode);
  useSettingsStore.setState((state) => ({
    settings: {
      ...state.settings,
      theme: { ...state.settings.theme, mode },
    },
  }));
}

/**
 * Apply all settings from another window without persisting.
 */
export async function applySettingsLocally(incoming: AppSettings) {
  const theme = normalizeThemePreference(incoming.theme);
  await applyTheme(theme.mode, theme.packId);
  watchSystemTheme(theme.mode);
  useSettingsStore.setState({ settings: { ...incoming, theme } });
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
      const raw = await settingsCommands.getSettings();
      const theme = normalizeThemePreference(raw.theme);
      let settings = {
        ...raw,
        theme,
        connectionPoolSize: raw.connectionPoolSize ?? 10,
        monitor: { ...DEFAULT_MONITOR_SETTINGS, ...raw.monitor },
      };
      try {
        await applyTheme(theme.mode, theme.packId);
      } catch {
        if (theme.packId) {
          const recoveredTheme = { ...theme, packId: null };
          settings = { ...settings, theme: recoveredTheme };
          await settingsCommands.saveSettings(settings);
          await applyTheme(recoveredTheme.mode, null);
        } else {
          throw new Error('failed to apply theme');
        }
      }
      watchSystemTheme(theme.mode);
      set({ settings });
    } catch {
      await applyTheme(DEFAULT_SETTINGS.theme.mode, DEFAULT_SETTINGS.theme.packId);
      const language = resolveUiLanguage(navigator.language);
      set({ settings: { ...DEFAULT_SETTINGS, language } });
    }
  },

  updateSettings: async (partial) => {
    const merged = { ...get().settings, ...partial };
    const theme = partial.theme
      ? normalizeThemePreference(partial.theme)
      : merged.theme;
    const next: AppSettings = { ...merged, theme };
    if (partial.theme) {
      await applyTheme(theme.mode, theme.packId);
      watchSystemTheme(theme.mode);
    }
    await settingsCommands.saveSettings(next);
    set({ settings: next });

    if (partial.theme) {
      void emitCrossWindow('datazen:theme-changed', theme.mode);
    }
    if (partial.language) {
      void import('@tauri-apps/api/core').then(({ invoke }) =>
        invoke('rebuild_menu', { language: partial.language }).catch(() => {}),
      );
    }
    void emitCrossWindow('datazen:settings-changed', next);
  },
}));
