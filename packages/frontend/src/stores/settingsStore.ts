import { create } from 'zustand';
import { bindSettingsStore, HOST_DEFAULT_EDITOR_FONT } from '@datazen/driver-sdk';
import { settingsCommands } from '../commands/settings';
import { emitCrossWindow } from '../lib/crossWindowBus';
import { normalizeSettingsLanguage, resolveUiLanguage } from '../lib/resolveUiLanguage';
import type { AppSettings } from '../types';
import {
  DEFAULT_GRID_ALTERNATING_ROWS,
  DEFAULT_GRID_AUTO_HIDE_SCROLLBAR,
  DEFAULT_GRID_DENSITY,
  DEFAULT_GRID_ROW_NUMBERS,
  DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD,
  DEFAULT_ROW_COUNT_MODE,
  DEFAULT_TABLE_SORT,
} from '../types/settings';
import { DEFAULT_SQL_FORMAT_OPTIONS } from '../lib/sqlFormat';
import {
  DEFAULT_VALUE_PRESENTATION_SETTINGS,
  parseStoredValuePresentationSettings,
  type StoredValuePresentationSettings,
} from '../lib/valuePresentation/settings';
import {
  applyValuePresentationUpdate,
  type ValuePresentationSettingsUpdate,
} from '../lib/valuePresentation/valuePresentationSave';
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
  mcpHttpEnabled: false,
  mcpHttpPort: 0,
  mcpDisabledTools: [],
  mcpPermissionMode: 'safe_write',
  mcpAllowedConnectionIds: [],
  mcpAllowedDatabases: [],
  mcpAllowedNamespaces: [],
  aiPluginToolIds: [],
  aiWorkflowToolIds: [],
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
  // The grid-appearance settings (DB-17). Their defaults are the grids'
  // behaviour before they existed: a zero density delta, striped odd rows,
  // the `#` gutter, always-visible scrollbars.
  gridDensity: DEFAULT_GRID_DENSITY,
  gridAlternatingRows: DEFAULT_GRID_ALTERNATING_ROWS,
  gridRowNumbers: DEFAULT_GRID_ROW_NUMBERS,
  gridAutoHideScrollbar: DEFAULT_GRID_AUTO_HIDE_SCROLLBAR,
  // Value presentation (issue-32 T06): v1 默认关闭一切，历史配置缺字段
  // 即默认关闭（设计 §6）。
  valuePresentation: DEFAULT_VALUE_PRESENTATION_SETTINGS,
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
 *
 * T11 注：跨窗口同步链路已不再走本函数（useSettingsSync 改为收到通知
 * 后重读权威配置，不信任广播 payload，见设计 §6）；本函数保留供
 * 兼容调用方与单测使用，语义不变。
 */
export async function applySettingsLocally(incoming: AppSettings) {
  const theme = normalizeThemePreference(incoming.theme);
  const language = normalizeSettingsLanguage(incoming.language);
  const prev = useSettingsStore.getState().settings;
  // 旧窗口 payload 可能没有 valuePresentation：宽容解析为默认值；
  // JSON 变化时 valuePresentationRevision +1（O-5 revision 不变量）。
  const valuePresentation = parseStoredValuePresentationSettings(incoming.valuePresentation);
  useSettingsStore.setState({
    settings: { ...incoming, language, theme, valuePresentation },
    valuePresentationRevision: nextValuePresentationRevision(
      prev.valuePresentation,
      valuePresentation,
      useSettingsStore.getState().valuePresentationRevision,
    ),
  });
  const themeChanged = prev.theme.mode !== theme.mode || prev.theme.packId !== theme.packId;
  if (!themeChanged) return;
  try {
    await applyTheme(theme.mode, theme.packId);
    watchSystemTheme(theme.mode);
  } catch {
    // Settings cache must still update even if the theme pack fails to apply.
  }
}

/**
 * valuePresentation 的修订计数：JSON 变化时 +1，不变时不动。
 * 快照 hook 与展示计划缓存以此失效（O-5 revision 不变量由该计数器保证，
 * 而不是由对象 identity 保证——load/save 往返可能重建等值对象）。
 */
function nextValuePresentationRevision(
  prev: StoredValuePresentationSettings,
  next: StoredValuePresentationSettings,
  revision: number,
): number {
  return JSON.stringify(prev) === JSON.stringify(next) ? revision : revision + 1;
}

// ---------------------------------------------------------------------------
// T11 跨窗口同步：加载代次与权威重读（设计 §6 加载状态 3/4）。
// ---------------------------------------------------------------------------

/**
 * 设置加载的请求代次：每次 loadSettings 调用 +1；只有最新一代的
 * 响应才能写 store，旧一代即使后到也直接丢弃（状态 4：切换上下文
 * 时旧表响应不得覆盖新表；此处是设置加载的同构问题——启动预加载
 * 与设置页挂载可能重叠，或跨窗口通知连续触发两次重读）。
 */
let settingsLoadGeneration = 0;

/** 是否曾经成功加载过设置（任一代成功即 true，含首次失败回落默认）。 */
let hasLoadedSettings = false;

/**
 * 把权威设置响应归一化为完整 AppSettings（loadSettings 与保存前
 * 重读共用同一归一化：主题、语言、monitor、connectionPoolSize、
 * valuePresentation 宽容解析；旧 payload 缺字段即默认，见 T06）。
 */
function normalizeLoadedSettings(raw: Partial<AppSettings>): AppSettings {
  const theme = normalizeThemePreference(raw.theme);
  return {
    ...DEFAULT_SETTINGS,
    ...raw,
    language: normalizeSettingsLanguage(raw.language ?? DEFAULT_SETTINGS.language),
    theme,
    connectionPoolSize: raw.connectionPoolSize ?? 10,
    monitor: { ...DEFAULT_MONITOR_SETTINGS, ...raw.monitor },
    // 旧后端 payload 可能没有 valuePresentation：宽容解析，缺省为默认。
    valuePresentation: parseStoredValuePresentationSettings(raw.valuePresentation),
  };
}

/**
 * 重读一次权威设置（保存前的基底用）。返回 null 时调用方回落本
 * 地 store 值（重读失败、响应不是对象、或权威响应根本没有
 * valuePresentation 字段——旧后端——时，不拿缺省默认去覆盖本地已
 * 有的 valuePresentation，见 valuePresentationSave.ts 文件头裁决 3）。
 */
async function readAuthoritativeSettings(): Promise<AppSettings | null> {
  // await + try/catch（不用 .catch()）：getSettings 恒返回 Promise，
  // 但同步抛错或返回非 Promise 时也不应把保存链路一起拖垮。
  let raw: unknown;
  try {
    raw = await settingsCommands.getSettings();
  } catch {
    return null;
  }
  if (typeof raw !== 'object' || raw === null) return null;
  if ((raw as Partial<AppSettings>).valuePresentation === undefined) return null;
  return normalizeLoadedSettings(raw as Partial<AppSettings>);
}

interface SettingsStore {
  settings: AppSettings;
  /** valuePresentation 的修订计数：JSON 变化时 +1，不变时不动（O-5）。 */
  valuePresentationRevision: number;
  loadSettings: () => Promise<void>;
  updateSettings: (partial: Partial<AppSettings>) => Promise<void>;
  /**
   * 供 T07/T08/T09 调用：保存 valuePresentation（设计 §6，T11 路径化
   * 更新）。接受整份对象（旧形式：整份替换）或 updater（新形式：
   * 只改拥有路径，其余取保存前重读的权威值——两窗口同时保存不互相
   * 覆盖对方字段，见 valuePresentationSave.ts 文件头裁决）。
   */
  updateValuePresentationSettings: (update: ValuePresentationSettingsUpdate) => Promise<void>;
}

export const useSettingsStore = create<SettingsStore>((set, get) => {
  /**
   * 保存链路共用核心（T11 从 updateSettings 抽取）：调后端保存 →
   * 以权威响应更新本地 store（同窗口从权威响应更新，设计 §6）→
   * valuePresentation JSON 变化时 revision +1 → 发布跨窗口设置
   * 通知（接收端重读权威配置，不信任 payload，见 useSettingsSync）。
   * 主题应用与语言菜单同步留在 updateSettings（只属于通用保存路）。
   */
  const persistSettings = async (next: AppSettings): Promise<void> => {
    const saved = await settingsCommands.saveSettings(next);
    const persistedValues = { ...next, ...(saved ?? {}) };
    const persisted: AppSettings = {
      ...persistedValues,
      language: normalizeSettingsLanguage(persistedValues.language),
      // 后端回值可能缺 valuePresentation（旧后端）：宽容解析兜底为默认。
      valuePresentation: parseStoredValuePresentationSettings(persistedValues.valuePresentation),
    };
    const prevValuePresentation = get().settings.valuePresentation;
    const prevRevision = get().valuePresentationRevision;
    set({
      settings: persisted,
      valuePresentationRevision: nextValuePresentationRevision(
        prevValuePresentation,
        persisted.valuePresentation,
        prevRevision,
      ),
    });
    void emitCrossWindow('datazen:settings-changed', persisted);
  };

  return {
    settings: DEFAULT_SETTINGS,
    valuePresentationRevision: 0,

    loadSettings: async () => {
      // T11（设计 §6 状态 3/4）：代次校验——只有最新一代的响应才能
      // 写 store；旧响应后到直接丢弃。刷新失败保留最后一份有效快照，
      // 只有从未成功加载过时才回落默认快照（状态 1）。
      const generation = ++settingsLoadGeneration;
      const isCurrent = (): boolean => generation === settingsLoadGeneration;
      try {
        const raw = await settingsCommands.getSettings();
        if (!isCurrent()) return;
        let settings = normalizeLoadedSettings(raw);
        try {
          await applyTheme(settings.theme.mode, settings.theme.packId);
        } catch {
          if (!isCurrent()) return;
          if (settings.theme.packId) {
            const recoveredTheme = { ...settings.theme, packId: null };
            settings = { ...settings, theme: recoveredTheme };
            await settingsCommands.saveSettings(settings);
            if (!isCurrent()) return;
            await applyTheme(recoveredTheme.mode, null);
          } else {
            throw new Error('failed to apply theme');
          }
        }
        if (!isCurrent()) return;
        watchSystemTheme(settings.theme.mode);
        hasLoadedSettings = true;
        set({
          settings,
          valuePresentationRevision: nextValuePresentationRevision(
            get().settings.valuePresentation,
            settings.valuePresentation,
            get().valuePresentationRevision,
          ),
        });
      } catch {
        if (!isCurrent()) return;
        // 刷新失败：保留最后一份有效快照（状态 3），不 wipe 回默认。
        if (hasLoadedSettings) return;
        await applyTheme(DEFAULT_SETTINGS.theme.mode, DEFAULT_SETTINGS.theme.packId);
        const language = resolveUiLanguage(navigator.language);
        const settings = { ...DEFAULT_SETTINGS, language };
        hasLoadedSettings = true;
        set({
          settings,
          valuePresentationRevision: nextValuePresentationRevision(
            get().settings.valuePresentation,
            settings.valuePresentation,
            get().valuePresentationRevision,
          ),
        });
      }
    },

    updateSettings: async (partial) => {
      const merged = { ...get().settings, ...partial };
      const theme = partial.theme ? normalizeThemePreference(partial.theme) : merged.theme;
      const next: AppSettings = {
        ...merged,
        language: normalizeSettingsLanguage(merged.language),
        theme,
      };
      if (partial.theme) {
        await applyTheme(theme.mode, theme.packId);
        watchSystemTheme(theme.mode);
      }
      await persistSettings(next);

      if (partial.theme) {
        void emitCrossWindow('datazen:theme-changed', theme.mode);
      }
      if (partial.language) {
        void settingsCommands.syncLanguageMenu?.(get().settings.language).catch(() => {});
      }
    },

    updateValuePresentationSettings: async (update) => {
      // T11（设计 §6）：保存前重读一次权威配置，updater 只改拥有
      // 路径、其余取权威原文——两窗口同时保存时不互相覆盖整个旧快照。
      // 重读失败 / 权威响应无 valuePresentation 字段时回落本地基底
      // （不阻塞保存；残余重读→保存窗口见 valuePresentationSave.ts）。
      const authoritative = await readAuthoritativeSettings();
      const base = authoritative ?? get().settings;
      const freshValuePresentation = parseStoredValuePresentationSettings(base.valuePresentation);
      const nextValuePresentation = applyValuePresentationUpdate(freshValuePresentation, update);
      const next: AppSettings = {
        ...base,
        language: normalizeSettingsLanguage(base.language),
        valuePresentation: nextValuePresentation,
      };
      await persistSettings(next);
    },
  };
});

// Inject the settings store into @datazen/driver-sdk so driver UI can read
// `safeMode` / `driverSettings` / editor font without importing host code
// (schemaStoreBridge pattern).
bindSettingsStore(useSettingsStore);
