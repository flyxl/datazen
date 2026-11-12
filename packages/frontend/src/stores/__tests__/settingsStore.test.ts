import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { DEFAULT_THEME_PREFERENCE } from '../../types/theme';
import type { StoredValuePresentationSettings } from '../../lib/valuePresentation/settings';
import {
  columnOverrideKey,
  withTimestampRenderer,
  type ValuePresentationRule,
} from '../../lib/valuePresentation/index';

const mockSettingsCommands = {
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
  syncLanguageMenu: vi.fn().mockResolvedValue(undefined),
};

vi.mock('../../commands/settings', () => ({
  settingsCommands: mockSettingsCommands,
}));

vi.mock('../../lib/crossWindowBus', () => ({
  emitCrossWindow: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../lib/themePackApply', () => ({
  applyThemePack: vi.fn().mockResolvedValue({ ok: true }),
  syncWebviewBackgroundFromTokens: vi.fn(),
}));

vi.mock('../../lib/resolveUiLanguage', () => {
  const resolveUiLanguage = (locale: string) => {
    const normalized = locale.trim().toLowerCase().replace(/_/g, '-');
    if (normalized.startsWith('zh')) return 'zh-CN';
    if (normalized.startsWith('en')) return 'en';
    return 'en';
  };
  const normalizeSettingsLanguage = (language: string) => {
    const normalized = language.trim().replace(/_/g, '-');
    if (!normalized) return 'en';
    const lower = normalized.toLowerCase();
    if (lower === 'zh-cn' || lower.startsWith('zh')) return 'zh-CN';
    if (lower === 'en' || lower.startsWith('en')) return 'en';
    return normalized;
  };
  return { resolveUiLanguage, normalizeSettingsLanguage };
});

describe('settingsStore', () => {
  let useSettingsStore: typeof import('../settingsStore').useSettingsStore;
  let applyThemePack: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    document.documentElement.classList.remove('dark');
    localStorage.clear();
    Object.defineProperty(window, 'matchMedia', {
      writable: true,
      value: vi.fn().mockImplementation((query: string) => ({
        matches: query.includes('dark'),
        media: query,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    });

    const themeMod = await import('../../lib/themePackApply');
    applyThemePack = vi.mocked(themeMod.applyThemePack);

    const mod = await import('../settingsStore');
    useSettingsStore = mod.useSettingsStore;
  });

  afterEach(() => {
    document.documentElement.classList.remove('dark');
  });

  it('loadSettings applies theme and stores settings', async () => {
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      limitSelectResults: true,
      queryResultLimit: 5000,
      editorFontSize: 13,
      confirmOnDelete: true,
      autoCommit: true,
      safeMode: true,
      defaultPageSize: 50,
      connectionPoolSize: 10,
      logLevel: 'info',
      logPath: '',
      mcpServerEnabled: false,
      mcpDisabledTools: [],
      mcpPermissionMode: 'safe_write',
      contextDir: '',
      checkForUpdatesOnStartup: false,
      monitor: {},
      driverSettings: {},
    });

    await useSettingsStore.getState().loadSettings();

    expect(useSettingsStore.getState().settings.language).toBe('en');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  it('loadSettings preserves a saved non-builtin language code (e.g. installed language pack)', async () => {
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'light', packId: null },
      language: 'ja',
    });

    await useSettingsStore.getState().loadSettings();

    expect(useSettingsStore.getState().settings.language).toBe('ja');
  });

  it('SS-P1: loadSettings dispatches a persisted plugin:<id>:<theme> packId to applyThemePack verbatim', async () => {
    const persistedPackId = 'plugin:acme.bill-audit:midnight-blue';
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: persistedPackId },
      language: 'en',
    });
    applyThemePack.mockResolvedValue({ ok: true });

    await useSettingsStore.getState().loadSettings();

    expect(applyThemePack).toHaveBeenCalledWith(persistedPackId);
    expect(useSettingsStore.getState().settings.theme.packId).toBe(persistedPackId);
    expect(mockSettingsCommands.saveSettings).not.toHaveBeenCalled();
  });

  it('SS-P2: loadSettings resets a failed wapp theme (e.g. wapp disabled) back to the default pack', async () => {
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: 'plugin:acme.bill-audit:midnight-blue' },
      language: 'en',
    });
    // First call (persisted wapp theme) fails — e.g. wapp disabled/removed;
    // second call (reset to no pack) succeeds.
    applyThemePack
      .mockResolvedValueOnce({
        ok: false,
        error: 'Theme "midnight-blue" not found in plugin "acme.bill-audit"',
      })
      .mockResolvedValueOnce({ ok: true });
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);

    await useSettingsStore.getState().loadSettings();

    expect(applyThemePack).toHaveBeenNthCalledWith(1, 'plugin:acme.bill-audit:midnight-blue');
    expect(applyThemePack).toHaveBeenNthCalledWith(2, null);
    expect(useSettingsStore.getState().settings.theme.packId).toBeNull();
    const saved = mockSettingsCommands.saveSettings.mock.calls.at(-1)?.[0] as {
      theme: { packId: string | null };
    };
    expect(saved.theme.packId).toBeNull();
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  it('loadSettings recovers from invalid theme pack', async () => {
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'light', packId: 'bad-pack' },
      language: 'en',
    });
    applyThemePack
      .mockResolvedValueOnce({ ok: false, error: 'invalid pack' })
      .mockResolvedValueOnce({ ok: true });
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);

    await useSettingsStore.getState().loadSettings();

    expect(useSettingsStore.getState().settings.theme.packId).toBeNull();
    expect(mockSettingsCommands.saveSettings).toHaveBeenCalled();
  });

  it('loadSettings falls back to defaults on total failure', async () => {
    mockSettingsCommands.getSettings.mockRejectedValueOnce(new Error('unavailable'));
    applyThemePack.mockResolvedValue({ ok: true });

    await useSettingsStore.getState().loadSettings();

    expect(useSettingsStore.getState().settings.language).toBe('en');
    expect(useSettingsStore.getState().settings.theme.mode).toBe(DEFAULT_THEME_PREFERENCE.mode);
    expect(useSettingsStore.getState().settings.mcpClientServers).toEqual([]);
    expect(useSettingsStore.getState().settings.sqlExecutionStrategy).toBe('current_statement');
  });

  it('defaults to current statement before hydration and for old settings', async () => {
    expect(useSettingsStore.getState().settings.sqlExecutionStrategy).toBe('current_statement');
    mockSettingsCommands.getSettings.mockResolvedValueOnce({ language: 'en' });
    await useSettingsStore.getState().loadSettings();
    expect(useSettingsStore.getState().settings.sqlExecutionStrategy).toBe('current_statement');
    expect(mockSettingsCommands.saveSettings).not.toHaveBeenCalled();
  });

  it.each(['entire_script', 'current_statement', 'largest_statement', 'ask'] as const)(
    'preserves explicit saved execution strategy %s',
    async (sqlExecutionStrategy) => {
      mockSettingsCommands.getSettings.mockResolvedValueOnce({ sqlExecutionStrategy });
      await useSettingsStore.getState().loadSettings();
      expect(useSettingsStore.getState().settings.sqlExecutionStrategy).toBe(sqlExecutionStrategy);
      await useSettingsStore.getState().updateSettings({ editorFontSize: 15 });
      expect(mockSettingsCommands.saveSettings).toHaveBeenLastCalledWith(
        expect.objectContaining({ sqlExecutionStrategy }),
      );
    },
  );

  it('persists intention opt-in and opt-out through generic extension settings and reload', async () => {
    expect(useSettingsStore.getState().settings.driverSettings).toEqual({});
    for (const intentionActions of [true, false]) {
      const driverSettings = {
        'sql-editor-enhanced': { intentionActions, insertValueHints: true },
      };
      await useSettingsStore.getState().updateSettings({ driverSettings });
      const saved = mockSettingsCommands.saveSettings.mock.lastCall?.[0];
      expect(saved.driverSettings).toEqual(driverSettings);
      mockSettingsCommands.getSettings.mockResolvedValueOnce(saved);
      await useSettingsStore.getState().loadSettings();
      expect(useSettingsStore.getState().settings.driverSettings).toEqual(driverSettings);
    }
  });

  it('updateSettings merges partial and saves', async () => {
    applyThemePack.mockResolvedValue({ ok: true });
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);

    await useSettingsStore.getState().updateSettings({ defaultPageSize: 100 });

    expect(useSettingsStore.getState().settings.defaultPageSize).toBe(100);
    expect(mockSettingsCommands.saveSettings).toHaveBeenCalled();
  });

  it('updateSettings preserves non-builtin language codes before saving and syncing the menu', async () => {
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);
    const { settingsCommands } = await import('../../commands/settings');

    await useSettingsStore.getState().updateSettings({ language: 'ja' });

    expect(useSettingsStore.getState().settings.language).toBe('ja');
    expect(mockSettingsCommands.saveSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ language: 'ja' }),
    );
    expect(settingsCommands.syncLanguageMenu).toHaveBeenCalledWith('ja');
  });

  it('updateSettings applies theme when theme partial provided', async () => {
    applyThemePack.mockResolvedValue({ ok: true });
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);

    await useSettingsStore.getState().updateSettings({
      theme: { mode: 'dark', packId: null },
    });

    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  it('currentIsDark reflects document class', async () => {
    const { currentIsDark } = await import('../settingsStore');
    document.documentElement.classList.add('dark');
    expect(currentIsDark()).toBe(true);
    document.documentElement.classList.remove('dark');
    expect(currentIsDark()).toBe(false);
  });

  it('applyThemeLocally updates mode without backend save', async () => {
    applyThemePack.mockResolvedValue({ ok: true });
    useSettingsStore.setState({
      settings: { ...useSettingsStore.getState().settings, theme: { mode: 'light', packId: null } },
    });

    const { applyThemeLocally } = await import('../settingsStore');
    await applyThemeLocally('dark');

    expect(useSettingsStore.getState().settings.theme.mode).toBe('dark');
    expect(mockSettingsCommands.saveSettings).not.toHaveBeenCalled();
  });

  it('applySettingsLocally applies incoming settings', async () => {
    applyThemePack.mockResolvedValue({ ok: true });
    const { applySettingsLocally } = await import('../settingsStore');

    await applySettingsLocally({
      ...useSettingsStore.getState().settings,
      language: 'ja',
      theme: { mode: 'system', packId: null },
    });

    expect(useSettingsStore.getState().settings.language).toBe('ja');
    expect(useSettingsStore.getState().settings.theme.mode).toBe('system');
  });

  it('applySettingsLocally updates autoChartOnQuery from another window', async () => {
    applyThemePack.mockResolvedValue({ ok: true });
    const { applySettingsLocally } = await import('../settingsStore');
    useSettingsStore.setState({
      settings: { ...useSettingsStore.getState().settings, autoChartOnQuery: true },
    });
    applyThemePack.mockClear();

    await applySettingsLocally({
      ...useSettingsStore.getState().settings,
      autoChartOnQuery: false,
    });

    expect(useSettingsStore.getState().settings.autoChartOnQuery).toBe(false);
    expect(applyThemePack).not.toHaveBeenCalled();
  });

  it('enableFkPrediction defaults to off and can be turned on', async () => {
    // Off by default: inference is a guess, so it is opt-in rather than something
    // an upgrade silently starts doing.
    expect(useSettingsStore.getState().settings.enableFkPrediction).toBe(false);

    applyThemePack.mockResolvedValue({ ok: true });
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);

    await useSettingsStore.getState().updateSettings({ enableFkPrediction: true });

    expect(useSettingsStore.getState().settings.enableFkPrediction).toBe(true);
    expect(mockSettingsCommands.saveSettings).toHaveBeenCalled();
  });

  it('editorCompletionQuotePolicy defaults to unquoted and can be updated', async () => {
    expect(useSettingsStore.getState().settings.editorCompletionQuotePolicy).toBe('unquoted');

    applyThemePack.mockResolvedValue({ ok: true });
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);

    await useSettingsStore.getState().updateSettings({ editorCompletionQuotePolicy: 'both' });

    expect(useSettingsStore.getState().settings.editorCompletionQuotePolicy).toBe('both');
    expect(mockSettingsCommands.saveSettings).toHaveBeenCalled();
  });

  it('new settings default to the legacy behaviour', async () => {
    // The three settings a table read consults must not change what a read
    // does for an existing installation: nobody forces a count strategy, the
    // driver decides; nobody forces an order, the driver decides.
    const settings = useSettingsStore.getState().settings;
    expect(settings.rowCountMode).toBe('auto');
    expect(settings.rowCountEstimateThreshold).toBe(200000);
    expect(settings.defaultTableSort).toBe('primaryKeyAsc');
  });

  it('a settings file without the new keys keeps the legacy behaviour', async () => {
    // An installation written before these settings existed must read back
    // with the same defaults, not with `undefined`.
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
    });

    await useSettingsStore.getState().loadSettings();

    const settings = useSettingsStore.getState().settings;
    expect(settings.rowCountMode).toBe('auto');
    expect(settings.rowCountEstimateThreshold).toBe(200000);
    expect(settings.defaultTableSort).toBe('primaryKeyAsc');
  });

  it('stores the three count settings when they are changed', async () => {
    applyThemePack.mockResolvedValue({ ok: true });
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);

    await useSettingsStore.getState().updateSettings({
      rowCountMode: 'estimated',
      rowCountEstimateThreshold: 1000,
      defaultTableSort: 'none',
    });

    const settings = useSettingsStore.getState().settings;
    expect(settings.rowCountMode).toBe('estimated');
    expect(settings.rowCountEstimateThreshold).toBe(1000);
    expect(settings.defaultTableSort).toBe('none');
    expect(mockSettingsCommands.saveSettings).toHaveBeenCalled();
  });

  // Issue-32 T06: valuePresentation 持久化链路与 revision 计数。
  it('loadSettings parses valuePresentation and bumps the revision when it differs', async () => {
    const stored: StoredValuePresentationSettings = {
      version: 1,
      renderers: {
        'datazen.timestamp': {
          enabled: true,
          options: {
            interpretation: { kind: 'native' },
            format: 'iso',
            displayTimeZone: { kind: 'utc' },
          },
        },
      },
    };
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      valuePresentation: stored,
    });

    expect(useSettingsStore.getState().valuePresentationRevision).toBe(0);
    await useSettingsStore.getState().loadSettings();

    expect(useSettingsStore.getState().settings.valuePresentation).toEqual(stored);
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(1);
  });

  it('loadSettings keeps the revision at 0 when the loaded value equals the default', async () => {
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      valuePresentation: { version: 1, renderers: {} },
    });

    await useSettingsStore.getState().loadSettings();

    expect(useSettingsStore.getState().settings.valuePresentation).toEqual({
      version: 1,
      renderers: {},
    });
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(0);
  });

  it('loadSettings falls back to the default for a malformed valuePresentation (old payload)', async () => {
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      valuePresentation: { version: 'x', renderers: 'nope' },
    });

    await useSettingsStore.getState().loadSettings();

    expect(useSettingsStore.getState().settings.valuePresentation).toEqual({
      version: 1,
      renderers: {},
    });
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(0);
  });

  it('loadSettings round-trips valuePresentation through save and reload', async () => {
    const stored: StoredValuePresentationSettings = {
      version: 1,
      renderers: {
        'datazen.timestamp': {
          enabled: true,
          options: {
            interpretation: { kind: 'unix', unit: 'milliseconds' },
            format: 'iso-tz',
            displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
          },
        },
      },
    };
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      valuePresentation: stored,
    });
    await useSettingsStore.getState().loadSettings();
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(1);

    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);
    await useSettingsStore.getState().updateSettings({ defaultPageSize: 100 });
    const saved = mockSettingsCommands.saveSettings.mock.lastCall?.[0] as {
      valuePresentation: unknown;
    };
    expect(saved.valuePresentation).toEqual(stored);
    // 往返中 JSON 不变，revision 不动。
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(1);
    expect(useSettingsStore.getState().settings.valuePresentation).toEqual(stored);
  });

  it('updateSettings bumps the revision only when valuePresentation JSON changes', async () => {
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);
    const next: StoredValuePresentationSettings = {
      version: 1,
      renderers: {
        'datazen.timestamp': {
          enabled: true,
          options: {
            interpretation: { kind: 'native' },
            format: 'iso',
            displayTimeZone: { kind: 'utc' },
          },
        },
      },
    };

    await useSettingsStore.getState().updateSettings({ valuePresentation: next });
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(1);
    expect(mockSettingsCommands.saveSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ valuePresentation: next }),
    );

    // 同值再写：revision 不动（load/save 往返重建等值对象也不触发重建）。
    await useSettingsStore.getState().updateSettings({
      valuePresentation: JSON.parse(JSON.stringify(next)) as typeof next,
    });
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(1);
  });

  it('updateValuePresentationSettings replaces the stored value through the normal save path', async () => {
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);
    const next: StoredValuePresentationSettings = {
      version: 1,
      renderers: {
        'datazen.timestamp': {
          enabled: false,
          options: {
            interpretation: { kind: 'native' },
            format: 'locale',
            displayTimeZone: { kind: 'local' },
          },
        },
      },
    };

    await useSettingsStore.getState().updateValuePresentationSettings(next);

    expect(useSettingsStore.getState().settings.valuePresentation).toEqual(next);
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(1);
    expect(mockSettingsCommands.saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ valuePresentation: next }),
    );
  });

  it('applySettingsLocally syncs valuePresentation and its revision without saving', async () => {
    applyThemePack.mockResolvedValue({ ok: true });
    const { applySettingsLocally } = await import('../settingsStore');
    const next: StoredValuePresentationSettings = {
      version: 1,
      renderers: {
        'datazen.timestamp': {
          enabled: true,
          options: {
            interpretation: { kind: 'native' },
            format: 'iso',
            displayTimeZone: { kind: 'utc' },
          },
        },
      },
    };

    await applySettingsLocally({
      ...useSettingsStore.getState().settings,
      valuePresentation: next,
    });

    expect(useSettingsStore.getState().settings.valuePresentation).toEqual(next);
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(1);
    expect(mockSettingsCommands.saveSettings).not.toHaveBeenCalled();

    // 同值再同步：revision 不动。
    await applySettingsLocally({
      ...useSettingsStore.getState().settings,
      valuePresentation: JSON.parse(JSON.stringify(next)) as typeof next,
    });
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(1);
  });

  // Issue-32 T11：跨窗口同步——路径化保存（保存前重读权威值）与加载代次。
  const T11_RULE: ValuePresentationRule = {
    name: 'unix-ms',
    enabled: true,
    match: { column: { kind: 'exact', value: 'epoch_ms' } },
    options: {
      interpretation: { kind: 'unix', unit: 'milliseconds' },
      format: 'iso-tz',
      displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
    },
  };

  const T11_ISO_UTC = {
    interpretation: { kind: 'native' },
    format: 'iso',
    displayTimeZone: { kind: 'utc' },
  } as const;

  const T11_ISO_TZ_SHANGHAI = {
    interpretation: { kind: 'native' },
    format: 'iso-tz',
    displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
  } as const;

  it('updateValuePresentationSettings applies an updater to the authoritative value', async () => {
    // 本地 store 是旧基底（无规则、无覆盖）；权威端已有 B 窗保存的规则与覆盖。
    const authoritative: StoredValuePresentationSettings = {
      version: 1,
      renderers: {
        'datazen.timestamp': { enabled: true, options: { ...T11_ISO_UTC } },
      },
      rules: [T11_RULE],
      columnOverrides: {
        [columnOverrideKey({
          connectionId: 'conn-1',
          database: 'shop',
          namespace: 'public',
          table: 'orders',
          column: 'epoch_ms',
        })]: { mode: 'raw' },
      },
    };
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      valuePresentation: authoritative,
    });
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);

    await useSettingsStore
      .getState()
      .updateValuePresentationSettings((fresh) =>
        withTimestampRenderer(fresh, true, { ...T11_ISO_TZ_SHANGHAI }),
      );

    const saved = mockSettingsCommands.saveSettings.mock.lastCall?.[0] as {
      valuePresentation: StoredValuePresentationSettings;
    };
    expect(saved.valuePresentation.renderers['datazen.timestamp']?.options.format).toBe('iso-tz');
    // B 窗同时保存的规则与覆盖原样保留。
    expect(saved.valuePresentation.rules).toEqual([T11_RULE]);
    expect(saved.valuePresentation.columnOverrides).toEqual(authoritative.columnOverrides);
    expect(useSettingsStore.getState().settings.valuePresentation).toEqual(saved.valuePresentation);
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(1);
  });

  it('updateValuePresentationSettings with a whole object takes other fields from the authoritative read', async () => {
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      defaultPageSize: 99,
      valuePresentation: { version: 1, renderers: {} },
    });
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);
    const next: StoredValuePresentationSettings = {
      version: 1,
      renderers: {
        'datazen.timestamp': { enabled: true, options: { ...T11_ISO_UTC } },
      },
    };

    await useSettingsStore.getState().updateValuePresentationSettings(next);

    const saved = mockSettingsCommands.saveSettings.mock.lastCall?.[0] as {
      defaultPageSize: number;
      valuePresentation: StoredValuePresentationSettings;
    };
    // 其他字段取权威读到的值，不是本窗口陈旧的本地值（本地默认 50）。
    expect(saved.defaultPageSize).toBe(99);
    expect(saved.valuePresentation).toEqual(next);
  });

  it('updateValuePresentationSettings falls back to the local base when the authoritative re-read fails', async () => {
    useSettingsStore.setState({
      settings: { ...useSettingsStore.getState().settings, defaultPageSize: 77 },
    });
    mockSettingsCommands.getSettings.mockRejectedValueOnce(new Error('backend down'));
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);

    await useSettingsStore
      .getState()
      .updateValuePresentationSettings((fresh) =>
        withTimestampRenderer(fresh, true, { ...T11_ISO_UTC }),
      );

    const saved = mockSettingsCommands.saveSettings.mock.lastCall?.[0] as {
      defaultPageSize: number;
      valuePresentation: StoredValuePresentationSettings;
    };
    expect(saved.defaultPageSize).toBe(77);
    expect(saved.valuePresentation.renderers['datazen.timestamp']?.options.format).toBe('iso');
  });

  it('updateValuePresentationSettings keeps the local valuePresentation when the authoritative response has none', async () => {
    const local: StoredValuePresentationSettings = {
      version: 1,
      renderers: {
        'datazen.timestamp': { enabled: true, options: { ...T11_ISO_UTC } },
      },
      rules: [T11_RULE],
    };
    useSettingsStore.setState({
      settings: { ...useSettingsStore.getState().settings, valuePresentation: local },
      valuePresentationRevision: 1,
    });
    // 旧后端响应：有其他字段，但无 valuePresentation——整份重读作废，
    // 回落本地基底（不拿缺省默认覆盖本地已有规则）。
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      defaultPageSize: 99,
    });
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);

    await useSettingsStore
      .getState()
      .updateValuePresentationSettings((fresh) =>
        withTimestampRenderer(fresh, true, { ...T11_ISO_TZ_SHANGHAI }),
      );

    const saved = mockSettingsCommands.saveSettings.mock.lastCall?.[0] as {
      valuePresentation: StoredValuePresentationSettings;
    };
    expect(saved.valuePresentation.rules).toEqual([T11_RULE]);
    expect(saved.valuePresentation.renderers['datazen.timestamp']?.options.format).toBe('iso-tz');
  });

  it('loadSettings discards a stale response that resolves after a newer load', async () => {
    let resolveFirst: (value: unknown) => void = () => {};
    let resolveSecond: (value: unknown) => void = () => {};
    const first = new Promise<unknown>((resolve) => {
      resolveFirst = resolve;
    });
    const second = new Promise<unknown>((resolve) => {
      resolveSecond = resolve;
    });
    mockSettingsCommands.getSettings.mockReturnValueOnce(first).mockReturnValueOnce(second);
    const firstLoad = useSettingsStore.getState().loadSettings();
    const secondLoad = useSettingsStore.getState().loadSettings();
    resolveSecond({
      theme: { mode: 'light', packId: null },
      language: 'en',
      valuePresentation: { version: 1, renderers: {} },
    });
    await secondLoad;
    resolveFirst({
      theme: { mode: 'dark', packId: null },
      language: 'ja',
      valuePresentation: {
        version: 1,
        renderers: {
          'datazen.timestamp': { enabled: true, options: { ...T11_ISO_UTC } },
        },
      },
    });
    await firstLoad;

    expect(useSettingsStore.getState().settings.language).toBe('en');
    expect(useSettingsStore.getState().settings.theme.mode).toBe('light');
    expect(useSettingsStore.getState().settings.valuePresentation).toEqual({
      version: 1,
      renderers: {},
    });
  });

  it('loadSettings keeps the last valid snapshot when a refresh fails', async () => {
    const stored: StoredValuePresentationSettings = {
      version: 1,
      renderers: {
        'datazen.timestamp': { enabled: true, options: { ...T11_ISO_UTC } },
      },
    };
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: null },
      language: 'en',
      valuePresentation: stored,
    });
    await useSettingsStore.getState().loadSettings();
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(1);

    mockSettingsCommands.getSettings.mockRejectedValueOnce(new Error('backend down'));
    await useSettingsStore.getState().loadSettings();

    expect(useSettingsStore.getState().settings.valuePresentation).toEqual(stored);
    expect(useSettingsStore.getState().valuePresentationRevision).toBe(1);
    expect(useSettingsStore.getState().settings.language).toBe('en');
  });
});
