import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { DEFAULT_THEME_PREFERENCE } from '../../types/theme';

const mockSettingsCommands = {
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
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

vi.mock('../../lib/resolveUiLanguage', () => ({
  resolveUiLanguage: vi.fn().mockReturnValue('zh-CN'),
}));

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
      pluginSettings: {},
    });

    await useSettingsStore.getState().loadSettings();

    expect(useSettingsStore.getState().settings.language).toBe('en');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
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

  it('SS-P2: loadSettings resets a failed plugin theme (e.g. plugin disabled) back to the default pack', async () => {
    mockSettingsCommands.getSettings.mockResolvedValueOnce({
      theme: { mode: 'dark', packId: 'plugin:acme.bill-audit:midnight-blue' },
      language: 'en',
    });
    // First call (persisted plugin theme) fails — e.g. plugin disabled/removed;
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

    expect(useSettingsStore.getState().settings.language).toBe('zh-CN');
    expect(useSettingsStore.getState().settings.theme.mode).toBe(DEFAULT_THEME_PREFERENCE.mode);
    expect(useSettingsStore.getState().settings.mcpClientServers).toEqual([]);
  });

  it('updateSettings merges partial and saves', async () => {
    applyThemePack.mockResolvedValue({ ok: true });
    mockSettingsCommands.saveSettings.mockResolvedValue(undefined);

    await useSettingsStore.getState().updateSettings({ defaultPageSize: 100 });

    expect(useSettingsStore.getState().settings.defaultPageSize).toBe(100);
    expect(mockSettingsCommands.saveSettings).toHaveBeenCalled();
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
});
