import { create } from 'zustand';
import { bindSettingsStore, HOST_DEFAULT_EDITOR_FONT } from '@datazen/driver-sdk';
import { settingsCommands } from '../commands/settings';
import { emitCrossWindow } from '../lib/crossWindowBus';
import { resolveUiLanguage } from '../lib/resolveUiLanguage';
import type { AppSettings } from '../types';
import {
  DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD,
  DEFAULT_ROW_COUNT_MODE,
  DEFAULT_TABLE_SORT,
} from '../types/settings';
import { DEFAULT_SQL_FORMAT_OPTIONS } from '../lib/sqlFormat';
import { DEFAULT_THEME_PREFERENCE, normalizeThemePreference, type ThemeMode } from '../types/theme';
import { DEFAULT_MONITOR_SETTINGS } from '../types/dashboard';
import { applyThemePack, syncWebviewBackgroundFromTokens } from '../lib/themePackApply';

const DEFAULT_SETTINGS: AppSettings = {
  theme: DEFAULT_THEME_PREFERENCE,
  language: 'en',
  limitSelectResults: false,
  queryResultLimit: 5000,
  editorFontSize: 13,
  editorFontFamily: HOST_DEFAULT_EDITOR_FONT,
  confirmOnDelete: true,
  autoCommit: true,
  safeMode: true,
  confirmDangerousExecution: true,
  defaultPageSize: 50,
  connectionPoolSize: 10,
  logLevel: 'info',
  logPath: '',
  mcpServerEnabled: false,
  mcpDisabledTools: [],
  mcpPermissionMode: 'safe_write',
  mcpAllowedConnectionIds: [],
  contextDir: '',
  // Undefined = the app-data default. The favorites panel shows the resolved
  // path from `getFavoritesRoot`, so the default needs no stored value.
  favoritesRoot: undefined,
  checkForUpdatesOnStartup: false,
  autoChartOnQuery: false,
  monitor: DEFAULT_MONITOR_SETTINGS,
  driverSettings: {},
  wappSettings: {},
  mcpClientServers: [],
  aiStrictEgress: true,
  editorCompletionIncludeTablePrefix: true,
  enableFkPrediction: false,
  editorCompletionQuotePolicy: 'unquoted',
  keymapPreset: 'default',
  customKeymap: {},
  sqlFormatOptions: DEFAULT_SQL_FORMAT_OPTIONS,
  sqlExecutionStrategy: 'current_statement',
  sqlSnippets: [],
  workflowStepResultOrder: 'desc',
  // The three settings a table read consults. Their defaults are the
  // behaviour of every read before they existed: nobody forces a count
  // strategy, the driver decides; nobody forces an order, the driver decides.
  rowCountMode: DEFAULT_ROW_COUNT_MODE,
  rowCountEstimateThreshold: DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD,
  defaultTableSort: DEFAULT_TABLE_SORT,
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
  const prev = useSettingsStore.getState().settings;
  useSettingsStore.setState({ settings: { ...incoming, theme } });
  const themeChanged = prev.theme.mode !== theme.mode || prev.theme.packId !== theme.packId;
  if (!themeChanged) return;
  try {
    await applyTheme(theme.mode, theme.packId);
    watchSystemTheme(theme.mode);
  } catch {
    // Settings cache must still update even if the theme pack fails to apply.
  }
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
        ...DEFAULT_SETTINGS,
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
    const theme = partial.theme ? normalizeThemePreference(partial.theme) : merged.theme;
    const next: AppSettings = { ...merged, theme };
    if (partial.theme) {
      await applyTheme(theme.mode, theme.packId);
      watchSystemTheme(theme.mode);
    }
    const saved = await settingsCommands.saveSettings(next);
    const persisted = { ...next, ...(saved ?? {}) };
    set({ settings: persisted });

    if (partial.theme) {
      void emitCrossWindow('datazen:theme-changed', theme.mode);
    }
    if (partial.language) {
      void settingsCommands.syncLanguageMenu?.(partial.language).catch(() => {});
    }
    void emitCrossWindow('datazen:settings-changed', persisted);
  },
}));

// Inject the settings store into @datazen/driver-sdk so driver UI can read
// `safeMode` / `driverSettings` / editor font without importing host code
// (schemaStoreBridge pattern).
bindSettingsStore(useSettingsStore);
