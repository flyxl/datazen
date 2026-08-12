import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent, screen, within } from '@testing-library/react';
import { SettingsWindow } from '../SettingsWindow';
import type { AppSettings, PromptScenario } from '../../../types';

const closeWindowMock = vi.fn().mockResolvedValue(undefined);

const {
  loadSettingsMock,
  updateSettingsMock,
  loadAiConfigMock,
  loadProvidersMock,
  validateConfigMock,
  saveConfigMock,
  deleteConfigMock,
  fetchRemoteModelsMock,
  clearErrorMock,
  loadMcpServersMock,
  connectMcpServerMock,
  disconnectMcpServerMock,
  clearMcpErrorMock,
  getLogPathMock,
  getDefaultLogDirMock,
  openLogDirMock,
  openContextDirMock,
  getContextDirMock,
  promptListMock,
  promptSetOverrideMock,
  promptRemoveOverrideMock,
  mcpListAllToolsMock,
  mcpGetStatusMock,
  mcpStartStdioMock,
  mcpStopMock,
  getUrlParamMock,
  currentSettings,
  aiState,
} = vi.hoisted(() => {
  const currentSettings: AppSettings = {
    theme: { mode: 'dark', packId: null },
    language: 'zh-CN',
    limitSelectResults: true,
    queryResultLimit: 5000,
    editorFontSize: 13,
    editorFontFamily: 'Menlo',
    confirmOnDelete: true,
    autoCommit: true,
    safeMode: true,
    defaultPageSize: 50,
    connectionPoolSize: 10,
    checkForUpdatesOnStartup: true,
    logLevel: 'info',
    logPath: '',
    mcpServerEnabled: false,
    mcpDisabledTools: [],
    mcpPermissionMode: 'read_only',
    contextDir: '/tmp/context',
    pluginSettings: {},
    monitor: {
      enabled: false,
      pollIntervalSecs: 60,
      retentionDays: 7,
      trayEnabled: false,
      alertsEnabled: false,
    },
  };

  const aiState = {
    loadConfig: vi.fn().mockResolvedValue(undefined),
    loadProviders: vi.fn().mockResolvedValue(undefined),
    config: null as AppSettings extends never
      ? never
      : import('../../../types').AiProviderConfig | null,
    isConfigured: false,
    providers: [
      {
        providerType: 'open_ai' as const,
        displayName: 'OpenAI',
        defaultEndpoint: 'https://api.openai.com/v1',
        defaultProtocol: 'open_ai_compatible',
      },
      {
        providerType: 'custom' as const,
        displayName: 'Custom',
        defaultEndpoint: 'https://custom.example.com',
        defaultProtocol: 'open_ai_compatible',
      },
    ],
    configError: null as string | null,
    validating: false,
    saving: false,
    remoteModels: [] as { id: string; displayName: string }[],
    fetchingRemoteModels: false,
    validateConfig: vi.fn().mockResolvedValue(true),
    saveConfig: vi.fn().mockResolvedValue(true),
    deleteConfig: vi.fn().mockResolvedValue(undefined),
    fetchRemoteModels: vi.fn().mockResolvedValue([]),
    clearError: vi.fn(),
    setupEventListeners: vi.fn().mockResolvedValue(() => {}),
    mcpServers: [] as { serverId: string; serverName: string; toolsCount: number }[],
    mcpConnecting: false,
    mcpError: null as string | null,
    connectMcpServer: vi.fn().mockResolvedValue(undefined),
    disconnectMcpServer: vi.fn().mockResolvedValue(undefined),
    loadMcpServers: vi.fn().mockResolvedValue(undefined),
    clearMcpError: vi.fn(),
  };

  return {
    loadSettingsMock: vi.fn().mockImplementation(async () => {
      /* sync draft after load */
    }),
    updateSettingsMock: vi.fn().mockImplementation(async (partial: Partial<AppSettings>) => {
      if (partial.theme) {
        currentSettings.theme = { ...currentSettings.theme, ...partial.theme };
      }
      if (partial.monitor) {
        currentSettings.monitor = { ...(currentSettings.monitor ?? {}), ...partial.monitor };
      }
      Object.assign(currentSettings, partial);
    }),
    loadAiConfigMock: aiState.loadConfig,
    loadProvidersMock: aiState.loadProviders,
    validateConfigMock: aiState.validateConfig,
    saveConfigMock: aiState.saveConfig,
    deleteConfigMock: aiState.deleteConfig,
    fetchRemoteModelsMock: aiState.fetchRemoteModels,
    clearErrorMock: aiState.clearError,
    loadMcpServersMock: aiState.loadMcpServers,
    connectMcpServerMock: aiState.connectMcpServer,
    disconnectMcpServerMock: aiState.disconnectMcpServer,
    clearMcpErrorMock: aiState.clearMcpError,
    getLogPathMock: vi.fn().mockResolvedValue('/tmp/logs'),
    getDefaultLogDirMock: vi.fn().mockResolvedValue('/tmp/logs'),
    openLogDirMock: vi.fn().mockResolvedValue(undefined),
    openContextDirMock: vi.fn().mockResolvedValue(undefined),
    getContextDirMock: vi.fn().mockResolvedValue('/default/context'),
    promptListMock: vi.fn().mockResolvedValue([]),
    promptSetOverrideMock: vi.fn().mockResolvedValue(undefined),
    promptRemoveOverrideMock: vi.fn().mockResolvedValue(undefined),
    mcpListAllToolsMock: vi.fn().mockResolvedValue(['query_db', 'list_tables']),
    mcpGetStatusMock: vi.fn().mockResolvedValue({ running: false, transport: 'stdio' }),
    mcpStartStdioMock: vi.fn().mockResolvedValue(undefined),
    mcpStopMock: vi.fn().mockResolvedValue(undefined),
    getUrlParamMock: vi.fn().mockReturnValue(null),
    currentSettings,
    aiState,
  };
});

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useThemeListener', () => ({
  useThemeListener: () => {},
}));

vi.mock('../../../lib/windowKind', () => ({
  getUrlParam: (...args: unknown[]) => getUrlParamMock(...args),
}));

vi.mock('../../../stores/settingsStore', () => {
  const useSettingsStore = Object.assign(
    (
      sel: (s: {
        settings: AppSettings;
        loadSettings: typeof loadSettingsMock;
        updateSettings: typeof updateSettingsMock;
      }) => unknown,
    ) =>
      sel({
        settings: currentSettings,
        loadSettings: loadSettingsMock,
        updateSettings: updateSettingsMock,
      }),
    {
      getState: () => ({
        settings: currentSettings,
        loadSettings: loadSettingsMock,
        updateSettings: updateSettingsMock,
      }),
    },
  );
  return { useSettingsStore };
});

vi.mock('../../../stores/aiStore', () => {
  const useAiStore = Object.assign(
    (sel?: (s: typeof aiState) => unknown) => {
      if (typeof sel === 'function') return sel(aiState);
      return aiState;
    },
    { getState: () => aiState },
  );
  return { useAiStore };
});

vi.mock('../../../commands/settings', () => ({
  settingsCommands: {
    getLogPath: (...a: unknown[]) => getLogPathMock(...a),
    getDefaultLogDir: (...a: unknown[]) => getDefaultLogDirMock(...a),
    openLogDir: (...a: unknown[]) => openLogDirMock(...a),
    openContextDir: (...a: unknown[]) => openContextDirMock(...a),
  },
}));

vi.mock('../../../commands/context', () => ({
  contextCommands: {
    getDir: (...a: unknown[]) => getContextDirMock(...a),
  },
}));

vi.mock('../../../commands/ai', () => ({
  aiCommands: {
    promptList: (...a: unknown[]) => promptListMock(...a),
    promptSetOverride: (...a: unknown[]) => promptSetOverrideMock(...a),
    promptRemoveOverride: (...a: unknown[]) => promptRemoveOverrideMock(...a),
    mcpListAllTools: (...a: unknown[]) => mcpListAllToolsMock(...a),
    mcpGetStatus: (...a: unknown[]) => mcpGetStatusMock(...a),
    mcpStartStdio: (...a: unknown[]) => mcpStartStdioMock(...a),
    mcpStop: (...a: unknown[]) => mcpStopMock(...a),
  },
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ close: closeWindowMock }),
}));

vi.mock('../../../components/ui/PathInput', () => ({
  PathInput: ({
    value,
    onChange,
    placeholder,
    className,
  }: {
    value: string;
    onChange: (v: string) => void;
    placeholder?: string;
    className?: string;
  }) => (
    <input
      data-testid="path-input"
      type="text"
      value={value}
      placeholder={placeholder}
      className={className}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({ title }: { title: string }) => <div data-testid="title-bar">{title}</div>,
}));

vi.mock('../../../components/ThemedIcon', () => ({
  ThemedIcon: () => <span data-testid="themed-icon" />,
}));

vi.mock('../ThemePackSection', () => ({
  ThemePackSection: () => <div data-testid="theme-pack" />,
}));

vi.mock('../UpdateSection', () => ({
  UpdateSection: ({
    checkOnStartup,
    onCheckOnStartupChange,
  }: {
    checkOnStartup: boolean;
    onCheckOnStartupChange: (v: boolean) => void;
  }) => (
    <label data-testid="update-section">
      <input
        type="checkbox"
        checked={checkOnStartup}
        onChange={(e) => onCheckOnStartupChange(e.target.checked)}
      />
      update-check
    </label>
  ),
}));

vi.mock('../PluginSettingsSection', () => ({
  PluginSettingsSection: () => <div data-testid="plugin-settings" />,
}));

async function waitForSettingsLoad() {
  await waitFor(() => expect(loadSettingsMock).toHaveBeenCalled());
}

function goToSection(label: string) {
  fireEvent.click(screen.getByText(label));
}

function getSaveButton() {
  return (
    screen.getAllByText('common.save').find((el) => el.closest('footer')) ??
    screen.getByText('common.save')
  );
}

/** Open a Select combobox and pick an option by visible label. */
function pickSelectOption(comboboxIndex: number, optionLabel: string) {
  const triggers = screen
    .getAllByRole('button', { hidden: false })
    .filter((b) => b.getAttribute('aria-haspopup') === 'listbox');
  fireEvent.click(triggers[comboboxIndex]);
  fireEvent.mouseDown(screen.getByText(optionLabel));
}

const samplePrompts = [
  {
    scenario: 'nl2sql' as PromptScenario,
    label: 'NL to SQL',
    source: 'default' as const,
    systemZh: '中文'.repeat(80),
    systemEn: 'English prompt body',
    defaultZh: '默认中文',
    defaultEn: 'Default EN',
  },
  {
    scenario: 'chat' as PromptScenario,
    label: 'Chat',
    source: 'user' as const,
    systemZh: '用户自定义',
    systemEn: 'User custom',
    defaultZh: '默认',
    defaultEn: 'Default',
  },
];

beforeEach(() => {
  vi.clearAllMocks();
  Element.prototype.scrollIntoView = vi.fn();
  getUrlParamMock.mockReturnValue(null);
  loadSettingsMock.mockResolvedValue(undefined);
  getLogPathMock.mockResolvedValue('/tmp/logs');
  getContextDirMock.mockResolvedValue('/default/context');
  promptListMock.mockResolvedValue(samplePrompts);
  mcpListAllToolsMock.mockResolvedValue(['query_db', 'list_tables']);
  mcpGetStatusMock.mockResolvedValue({ running: false, transport: 'stdio' });
  mcpStartStdioMock.mockResolvedValue(undefined);
  mcpStopMock.mockResolvedValue(undefined);

  Object.assign(currentSettings, {
    theme: { mode: 'dark', packId: null },
    language: 'zh-CN',
    limitSelectResults: true,
    queryResultLimit: 5000,
    editorFontSize: 13,
    editorFontFamily: 'Menlo',
    confirmOnDelete: true,
    autoCommit: true,
    safeMode: true,
    defaultPageSize: 50,
    connectionPoolSize: 10,
    checkForUpdatesOnStartup: true,
    logLevel: 'info',
    logPath: '',
    mcpServerEnabled: false,
    mcpDisabledTools: [],
    mcpPermissionMode: 'read_only',
    contextDir: '/tmp/context',
    pluginSettings: {},
    monitor: {
      enabled: false,
      pollIntervalSecs: 60,
      retentionDays: 7,
      trayEnabled: false,
      alertsEnabled: false,
    },
  });

  Object.assign(aiState, {
    config: null,
    isConfigured: false,
    configError: null,
    validating: false,
    saving: false,
    remoteModels: [],
    fetchingRemoteModels: false,
    mcpServers: [],
    mcpConnecting: false,
    mcpError: null,
  });
  aiState.validateConfig.mockResolvedValue(true);
  aiState.saveConfig.mockResolvedValue(true);
  aiState.fetchRemoteModels.mockResolvedValue([]);
  aiState.connectMcpServer.mockResolvedValue(undefined);
  aiState.disconnectMcpServer.mockResolvedValue(undefined);

  delete (globalThis as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

afterEach(cleanup);

describe('SettingsWindow', () => {
  it('renders general section by default', async () => {
    render(<SettingsWindow />);
    await waitForSettingsLoad();
    expect(screen.getByText('settings.language')).toBeInTheDocument();
    expect(screen.getByTestId('theme-pack')).toBeInTheDocument();
    expect(screen.getByTestId('update-section')).toBeInTheDocument();
  });

  it('opens section from URL deep-link param', async () => {
    getUrlParamMock.mockReturnValue('ai');
    render(<SettingsWindow />);
    await waitForSettingsLoad();
    expect(screen.getByText('settings.ai.description')).toBeInTheDocument();
    expect(loadProvidersMock).toHaveBeenCalled();
  });

  it('edits general settings and saves draft', async () => {
    render(<SettingsWindow />);
    await waitForSettingsLoad();

    pickSelectOption(0, 'English');
    fireEvent.click(within(screen.getByTestId('update-section')).getByRole('checkbox'));

    const saveBtn = getSaveButton();
    expect(saveBtn).not.toBeDisabled();
    fireEvent.click(saveBtn);

    await waitFor(() => expect(updateSettingsMock).toHaveBeenCalled());
    expect(currentSettings.language).toBe('en');
    expect(currentSettings.checkForUpdatesOnStartup).toBe(false);
    await waitFor(() => expect(screen.getByText('settings.saved')).toBeInTheDocument());
  });

  it('closes window when Tauri is available', async () => {
    (globalThis as { __TAURI_INTERNALS__?: object }).__TAURI_INTERNALS__ = {};
    render(<SettingsWindow />);
    await waitForSettingsLoad();
    fireEvent.click(screen.getByText('common.close'));
    await waitFor(() => expect(closeWindowMock).toHaveBeenCalled());
  });

  it('covers data browsing section toggles and limits', async () => {
    render(<SettingsWindow />);
    await waitForSettingsLoad();
    goToSection('settings.dataBrowsing');

    expect(screen.getByText('settings.maxRows')).toBeInTheDocument();
    const limitSwitch = screen.getAllByRole('switch')[0];
    fireEvent.click(limitSwitch);
    expect(screen.queryByText('settings.maxRows')).not.toBeInTheDocument();

    fireEvent.click(limitSwitch);
    pickSelectOption(0, '100 common.rows');

    fireEvent.click(getSaveButton());
    await waitFor(() => expect(updateSettingsMock).toHaveBeenCalled());
    const saved = updateSettingsMock.mock.calls.at(-1)?.[0] as AppSettings;
    expect(saved.defaultPageSize).toBe(100);
    expect(saved.limitSelectResults).toBe(true);
  });

  it('covers editor and behavior sections', async () => {
    render(<SettingsWindow />);
    await waitForSettingsLoad();

    goToSection('settings.editor');
    const range = document.querySelector('input[type="range"]') as HTMLInputElement;
    fireEvent.change(range, { target: { value: '16' } });
    expect(screen.getByText('16px')).toBeInTheDocument();
    const fontInput = document.querySelector('input[type="text"]') as HTMLInputElement;
    fireEvent.change(fontInput, { target: { value: 'JetBrains Mono' } });

    goToSection('settings.behavior');
    const switches = screen.getAllByRole('switch');
    fireEvent.click(switches[0]);
    fireEvent.click(switches[1]);

    fireEvent.click(getSaveButton());
    await waitFor(() => expect(updateSettingsMock).toHaveBeenCalled());
    const saved = updateSettingsMock.mock.calls.at(-1)?.[0] as AppSettings;
    expect(saved.editorFontSize).toBe(16);
    expect(saved.editorFontFamily).toBe('JetBrains Mono');
  });

  it('covers logging section actions', async () => {
    render(<SettingsWindow />);
    await waitForSettingsLoad();
    goToSection('settings.logging');

    pickSelectOption(0, 'Debug');
    const logPathInput = document.querySelector('input[type="text"]') as HTMLInputElement;
    fireEvent.change(logPathInput, { target: { value: '/var/log/datazen' } });
    fireEvent.click(screen.getByText('settings.viewLogs'));
    expect(openLogDirMock).toHaveBeenCalled();

    fireEvent.click(getSaveButton());
    await waitFor(() => expect(updateSettingsMock).toHaveBeenCalled());
    const saved = updateSettingsMock.mock.calls.at(-1)?.[0] as AppSettings;
    expect(saved.logLevel).toBe('debug');
    expect(saved.logPath).toBe('/var/log/datazen');
  });

  it('removes monitor section from settings navigation', async () => {
    render(<SettingsWindow />);
    await waitForSettingsLoad();
    expect(screen.queryByText('settings.monitor')).not.toBeInTheDocument();
  });

  it('covers AI settings validate, save, delete, and fetch models', async () => {
    aiState.config = {
      providerType: 'open_ai',
      apiKey: 'sk-test',
      endpoint: 'https://api.openai.com/v1',
      model: 'gpt-4o',
      maxTokens: 8000,
    };
    aiState.isConfigured = true;
    aiState.remoteModels = [{ id: 'gpt-4o', displayName: 'GPT-4o' }];

    render(<SettingsWindow />);
    await waitForSettingsLoad();
    goToSection('settings.ai');

    expect(screen.getByText('settings.ai.configured')).toBeInTheDocument();

    const apiKey = document.querySelector('input[type="password"]') as HTMLInputElement;
    fireEvent.change(apiKey, { target: { value: 'sk-new' } });
    fireEvent.click(screen.getByText('settings.ai.fetchModels'));
    await waitFor(() => expect(fetchRemoteModelsMock).toHaveBeenCalled());

    fireEvent.click(screen.getByText('settings.ai.validate'));
    await waitFor(() => expect(validateConfigMock).toHaveBeenCalled());

    fireEvent.click(screen.getByText('settings.ai.save'));
    await waitFor(() => expect(saveConfigMock).toHaveBeenCalled());

    fireEvent.click(screen.getByText('settings.ai.delete'));
    await waitFor(() => expect(deleteConfigMock).toHaveBeenCalled());
  });

  it('covers custom AI provider protocol and manual model input', async () => {
    aiState.remoteModels = [{ id: 'claude-3', displayName: 'Claude 3' }];
    render(<SettingsWindow />);
    await waitForSettingsLoad();
    goToSection('settings.ai');

    pickSelectOption(0, 'Custom');
    expect(screen.getByText('settings.ai.customHint')).toBeInTheDocument();
    pickSelectOption(1, 'settings.ai.protocolAnthropic');

    const textInputs = document.querySelectorAll('input[type="text"]');
    fireEvent.change(textInputs[0], { target: { value: 'https://anthropic.example.com' } });
    fireEvent.change(document.querySelector('input[type="password"]') as HTMLInputElement, {
      target: { value: 'key' },
    });

    fireEvent.click(screen.getByText('settings.ai.fetchModels'));
    await waitFor(() => expect(fetchRemoteModelsMock).toHaveBeenCalled());

    const manualCheckbox = screen.getByRole('checkbox');
    fireEvent.click(manualCheckbox);
    fireEvent.change(textInputs[0], { target: { value: 'my-model' } });
  });

  it('covers context directory setting in AI section', async () => {
    render(<SettingsWindow />);
    await waitForSettingsLoad();
    goToSection('settings.ai');

    await waitFor(() => expect(getContextDirMock).toHaveBeenCalled());
    const pathInput = screen.getByTestId('path-input');
    fireEvent.change(pathInput, { target: { value: '/new/context' } });
    const saveButtons = screen.getAllByText('common.save');
    fireEvent.click(saveButtons[saveButtons.length - 1]);
    await waitFor(() =>
      expect(updateSettingsMock).toHaveBeenCalledWith(
        expect.objectContaining({ contextDir: '/new/context' }),
      ),
    );
    fireEvent.click(screen.getByText('context.openDir'));
    expect(openContextDirMock).toHaveBeenCalled();
  });

  it('covers prompts edit, save, and reset flows', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);

    render(<SettingsWindow />);
    await waitForSettingsLoad();
    goToSection('settings.prompts');
    await waitFor(() => expect(promptListMock).toHaveBeenCalled());

    expect(screen.getByText('NL to SQL')).toBeInTheDocument();
    expect(screen.getByText(/settings\.prompts\.variables/)).toBeInTheDocument();

    fireEvent.click(screen.getAllByText('settings.prompts.edit')[0]);
    const textareas = document.querySelectorAll('textarea');
    fireEvent.change(textareas[0], { target: { value: '新中文' } });
    fireEvent.change(textareas[1], { target: { value: 'New EN' } });
    fireEvent.click(screen.getByText('common.save'));
    await waitFor(() => expect(promptSetOverrideMock).toHaveBeenCalled());

    fireEvent.click(screen.getByText('settings.prompts.reset'));
    await waitFor(() => expect(promptRemoveOverrideMock).toHaveBeenCalled());

    fireEvent.click(screen.getAllByText('settings.prompts.edit')[0]);
    fireEvent.click(screen.getByText('common.cancel'));
  });

  it('covers MCP server enable, tools, and permission modes', async () => {
    mcpGetStatusMock.mockResolvedValue({ running: true, transport: 'stdio' });

    render(<SettingsWindow />);
    await waitForSettingsLoad();
    goToSection('mcp.title');

    await waitFor(() => expect(mcpListAllToolsMock).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByText('mcp.running')).toBeInTheDocument());

    const enabledSwitch = screen.getAllByRole('switch')[0];
    fireEvent.click(enabledSwitch);
    await waitFor(() => expect(mcpStartStdioMock).toHaveBeenCalled());

    fireEvent.click(document.querySelector('input[value="safe_write"]') as HTMLInputElement);
    expect(updateSettingsMock).toHaveBeenCalledWith(
      expect.objectContaining({ mcpPermissionMode: 'safe_write' }),
    );

    fireEvent.click(screen.getByText('mcp.tools.disableAll'));
    fireEvent.click(screen.getByText('common.save'));
    await waitFor(() =>
      expect(updateSettingsMock).toHaveBeenCalledWith(
        expect.objectContaining({
          mcpDisabledTools: ['query_db', 'list_tables'],
        }),
      ),
    );

    fireEvent.click(screen.getByText('mcp.tools.enableAll'));
    fireEvent.click(enabledSwitch);
    await waitFor(() => expect(mcpStopMock).toHaveBeenCalled());
  });

  it('reverts MCP server enable when start fails', async () => {
    mcpStartStdioMock.mockRejectedValueOnce(new Error('start failed'));

    render(<SettingsWindow />);
    await waitForSettingsLoad();
    goToSection('mcp.title');
    await waitFor(() => expect(mcpListAllToolsMock).toHaveBeenCalled());

    fireEvent.click(screen.getAllByRole('switch')[0]);
    await waitFor(() => expect(screen.getByText('start failed')).toBeInTheDocument());
  });

  it('covers MCP client connect and disconnect', async () => {
    aiState.mcpServers = [{ serverId: 'srv1', serverName: 'Test MCP', toolsCount: 3 }];
    aiState.mcpError = 'connect failed';

    render(<SettingsWindow />);
    await waitForSettingsLoad();
    goToSection('mcpClient.title');

    await waitFor(() => expect(loadMcpServersMock).toHaveBeenCalled());
    expect(screen.getByText('connect failed')).toBeInTheDocument();
    const errorBanner = screen.getByText('connect failed').closest('div')!;
    fireEvent.click(within(errorBanner).getByText('common.close'));
    expect(clearMcpErrorMock).toHaveBeenCalled();

    fireEvent.click(screen.getByText('mcpClient.disconnect'));
    await waitFor(() => expect(disconnectMcpServerMock).toHaveBeenCalledWith('srv1'));

    aiState.mcpServers = [];
    aiState.mcpError = null;
    cleanup();
    render(<SettingsWindow />);
    await waitForSettingsLoad();
    goToSection('mcpClient.title');

    fireEvent.click(screen.getByText('mcpClient.addServer'));
    fireEvent.change(screen.getByPlaceholderText('my-mcp-server'), {
      target: { value: 'my-server' },
    });
    fireEvent.change(screen.getByPlaceholderText('My Server'), {
      target: { value: 'My Server' },
    });
    fireEvent.change(screen.getByTestId('path-input'), {
      target: { value: '/usr/bin/mcp' },
    });
    fireEvent.change(document.querySelector('textarea') as HTMLTextAreaElement, {
      target: { value: '--stdio\n--verbose' },
    });
    fireEvent.click(screen.getByText('mcpClient.connect'));
    await waitFor(() => expect(connectMcpServerMock).toHaveBeenCalled());
  });

  it('shows extensions section', async () => {
    render(<SettingsWindow />);
    await waitForSettingsLoad();
    goToSection('settings.extensions.title');
    expect(screen.getByTestId('plugin-settings')).toBeInTheDocument();
  });

  it('shows config error in AI section', async () => {
    aiState.configError = 'Invalid API key';
    render(<SettingsWindow />);
    await waitForSettingsLoad();
    goToSection('settings.ai');
    expect(screen.getByText('Invalid API key')).toBeInTheDocument();
  });
});
