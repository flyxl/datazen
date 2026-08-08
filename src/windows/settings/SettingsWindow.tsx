import { useCallback, useEffect, useRef, useState } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { ThemedIcon } from '../../components/ThemedIcon';
import { Button } from '../../components/ui/Button';
import { PathInput } from '../../components/ui/PathInput';
import { Select } from '../../components/ui/Select';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useI18n } from '../../hooks/useI18n';
import { getUrlParam } from '../../lib/windowKind';
import { cn } from '../../lib/cn';
import { aiCommands, type PromptInfo, type PromptOverrideEntry, type PromptScenario, type PromptSource } from '../../commands/ai';
import { settingsCommands } from '../../commands/settings';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { isKnownProviderType } from '../../lib/aiProviders';
import { settingsSectionIconId } from '../../lib/hostLucideMap';
import type { AppSettings, AiProviderConfig, AiProviderType, DatabaseType, McpServerConfig } from '../../types';
import type { ThemeMode } from '../../types/theme';
import type { TranslationKey } from '../../locales';
import { ThemePackSection } from './ThemePackSection';

const PAGE_SIZE_OPTIONS = [25, 50, 100, 200, 500];
const RESULT_LIMIT_OPTIONS = [1000, 2000, 5000, 10000, 50000];

const LOG_LEVEL_OPTIONS: { value: AppSettings['logLevel']; label: string }[] = [
  { value: 'trace', label: 'Trace' },
  { value: 'debug', label: 'Debug' },
  { value: 'info', label: 'Info' },
  { value: 'warn', label: 'Warn' },
  { value: 'error', label: 'Error' },
];

const THEME_KEYS: { value: ThemeMode; key: TranslationKey }[] = [
  { value: 'light', key: 'theme.light' },
  { value: 'dark', key: 'theme.dark' },
  { value: 'system', key: 'theme.system' },
];

const LANGUAGE_OPTIONS = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-TW', label: '繁體中文' },
  { value: 'en', label: 'English' },
  { value: 'es', label: 'Español' },
  { value: 'fr', label: 'Français' },
  { value: 'de', label: 'Deutsch' },
  { value: 'ja', label: '日本語' },
  { value: 'pt-BR', label: 'Português (Brasil)' },
  { value: 'ru', label: 'Русский' },
  { value: 'ko', label: '한국어' },
];

type SettingsSection = 'general' | 'dataBrowsing' | 'editor' | 'behavior' | 'logging' | 'ai' | 'prompts' | 'mcpServer' | 'mcpClient';

const SECTIONS: { id: SettingsSection; labelKey: TranslationKey }[] = [
  { id: 'general', labelKey: 'settings.general' },
  { id: 'dataBrowsing', labelKey: 'settings.dataBrowsing' },
  { id: 'editor', labelKey: 'settings.editor' },
  { id: 'behavior', labelKey: 'settings.behavior' },
  { id: 'logging', labelKey: 'settings.logging' },
  { id: 'ai', labelKey: 'settings.ai' },
  { id: 'prompts', labelKey: 'settings.prompts' },
  { id: 'mcpServer', labelKey: 'mcp.title' },
  { id: 'mcpClient', labelKey: 'mcpClient.title' },
];

export function SettingsWindow() {
  useThemeListener();
  const { t } = useI18n();

  const settings = useSettingsStore((s) => s.settings);
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const [draft, setDraft] = useState<AppSettings>(settings);
  const [saved, setSaved] = useState(false);
  const [defaultLogPath, setDefaultLogPath] = useState('');
  const settingsHydrated = useRef(false);

  const [activeSection, setActiveSection] = useState<SettingsSection>(() => {
    const fromUrl = getUrlParam('section');
    if (fromUrl && SECTIONS.some((s) => s.id === fromUrl)) {
      return fromUrl as SettingsSection;
    }
    return 'general';
  });

  useEffect(() => {
    void loadSettings().then(() => {
      setDraft(useSettingsStore.getState().settings);
      settingsHydrated.current = true;
    });
  }, [loadSettings]);

  useEffect(() => {
    void settingsCommands.getLogPath().then(setDefaultLogPath).catch(() => {});
  }, []);

  // Theme pack applies immediately via updateSettings; merge packId only so other draft edits persist.
  useEffect(() => {
    if (!settingsHydrated.current) return;
    setDraft((prev) => ({
      ...prev,
      theme: { ...prev.theme, packId: settings.theme.packId },
    }));
  }, [settings.theme.packId]);

  const updateField = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setDraft((prev) => ({ ...prev, [key]: value }));
    setSaved(false);
  };

  const handleSave = useCallback(async () => {
    await updateSettings(draft);
    setDraft(useSettingsStore.getState().settings);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  }, [draft, updateSettings]);

  const handleClose = useCallback(async () => {
    if (!('__TAURI_INTERNALS__' in globalThis)) return;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().close();
  }, []);

  const isDirty = JSON.stringify(draft) !== JSON.stringify(settings);

  const themeOptions = THEME_KEYS.map((tk) => ({
    value: tk.value,
    label: t(tk.key),
  }));

  return (
    <div className="flex h-screen flex-col bg-surface text-fg">
      <TitleBar title={t('win.settings')} />

      <div className="flex min-h-0 flex-1">
        {/* Sidebar */}
        <nav className="flex w-[180px] shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-edge bg-surface-alt px-2 py-3">
          {SECTIONS.map((sec) => {
            const isActive = activeSection === sec.id;
            return (
              <button
                key={sec.id}
                type="button"
                onClick={() => setActiveSection(sec.id)}
                className={cn(
                  'flex items-center gap-2.5 rounded-md px-3 py-2 text-left text-[13px] transition-colors',
                  isActive
                    ? 'bg-accent/15 font-medium text-accent'
                    : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                )}
              >
                <ThemedIcon id={settingsSectionIconId(sec.id)} className="h-4 w-4 shrink-0" />
                {t(sec.labelKey)}
              </button>
            );
          })}
        </nav>

        {/* Content */}
        <div className="flex-1 overflow-y-auto px-8 py-6">
          <div className="mx-auto max-w-lg space-y-5">
            {activeSection === 'general' && (
              <>
                <SectionTitle>{t('settings.general')}</SectionTitle>

                <SettingRow label={t('settings.language')}>
                  <Select
                    value={draft.language}
                    options={LANGUAGE_OPTIONS}
                    onChange={(v) => updateField('language', v)}
                  />
                </SettingRow>

                <SettingRow label={t('settings.theme')}>
                  <Select
                    value={draft.theme.mode}
                    options={themeOptions}
                    onChange={(v) =>
                      updateField('theme', { ...draft.theme, mode: v as ThemeMode })
                    }
                  />
                </SettingRow>

                <ThemePackSection />
              </>
            )}

            {activeSection === 'dataBrowsing' && (
              <>
                <SectionTitle>{t('settings.dataBrowsing')}</SectionTitle>

                <SettingRow label={t('settings.defaultPageSize')}>
                  <Select
                    value={draft.defaultPageSize}
                    options={PAGE_SIZE_OPTIONS.map((v) => ({ value: String(v), label: `${v} ${t('common.rows')}` }))}
                    onChange={(v) => updateField('defaultPageSize', Number(v))}
                  />
                </SettingRow>

                <ToggleRow
                  label={t('settings.limitSelect')}
                  checked={draft.limitSelectResults}
                  onChange={(v) => updateField('limitSelectResults', v)}
                />

                {draft.limitSelectResults && (
                  <SettingRow label={t('settings.maxRows')}>
                    <Select
                      value={draft.queryResultLimit}
                      options={RESULT_LIMIT_OPTIONS.map((v) => ({ value: String(v), label: `${v.toLocaleString()} ${t('common.rows')}` }))}
                      onChange={(v) => updateField('queryResultLimit', Number(v))}
                    />
                  </SettingRow>
                )}
              </>
            )}

            {activeSection === 'editor' && (
              <>
                <SectionTitle>{t('settings.editor')}</SectionTitle>

                <SettingRow label={t('settings.fontSize')}>
                  <div className="flex items-center gap-3">
                    <input
                      type="range"
                      min={10}
                      max={24}
                      step={1}
                      value={draft.editorFontSize}
                      onChange={(e) => updateField('editorFontSize', Number(e.target.value))}
                      className="flex-1 accent-accent"
                    />
                    <span className="w-12 text-right text-sm tabular-nums text-fg-secondary">{draft.editorFontSize}px</span>
                  </div>
                </SettingRow>

                <SettingRow label={t('settings.fontFamily')}>
                  <input
                    type="text"
                    value={draft.editorFontFamily}
                    onChange={(e) => updateField('editorFontFamily', e.target.value)}
                    className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25"
                  />
                </SettingRow>
              </>
            )}

            {activeSection === 'behavior' && (
              <>
                <SectionTitle>{t('settings.behavior')}</SectionTitle>

                <ToggleRow
                  label={t('settings.confirmDelete')}
                  checked={draft.confirmOnDelete}
                  onChange={(v) => updateField('confirmOnDelete', v)}
                />

                <ToggleRow
                  label={t('settings.autoCommit')}
                  checked={draft.autoCommit}
                  onChange={(v) => updateField('autoCommit', v)}
                />
              </>
            )}

            {activeSection === 'logging' && (
              <>
                <SectionTitle>{t('settings.logging')}</SectionTitle>

                <SettingRow label={t('settings.logLevel')}>
                  <Select
                    value={draft.logLevel}
                    options={LOG_LEVEL_OPTIONS}
                    onChange={(v) => updateField('logLevel', v as AppSettings['logLevel'])}
                  />
                </SettingRow>

                <SettingRow label={t('settings.logPath')}>
                  <input
                    type="text"
                    value={draft.logPath}
                    onChange={(e) => updateField('logPath', e.target.value)}
                    placeholder={defaultLogPath || t('settings.logPathPlaceholder')}
                    className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25"
                  />
                </SettingRow>

                <div className="flex items-center gap-3">
                  <Button variant="secondary" onClick={() => void settingsCommands.openLogDir()}>
                    {t('settings.viewLogs')}
                  </Button>
                </div>

                <p className="text-xs text-fg-muted">{t('settings.logRestartNote')}</p>
              </>
            )}

            {activeSection === 'ai' && <AiSettingsSection />}
            {activeSection === 'prompts' && <PromptSettingsSection />}
            {activeSection === 'mcpServer' && <McpSettingsSection />}
            {activeSection === 'mcpClient' && <McpClientSection />}
          </div>
        </div>
      </div>

      {/* Footer - only show for general settings that use draft/save */}
      {['general', 'dataBrowsing', 'editor', 'behavior', 'logging'].includes(activeSection) && (
        <footer className="flex shrink-0 items-center justify-end gap-3 border-t border-edge px-8 py-3">
          {saved && <span className="text-xs text-green-500">{t('settings.saved')}</span>}
          <Button variant="secondary" onClick={() => void handleClose()}>{t('common.close')}</Button>
          <Button variant="primary" disabled={!isDirty} onClick={() => void handleSave()}>
            {t('common.save')}
          </Button>
        </footer>
      )}
    </div>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return <h2 className="text-[13px] font-semibold uppercase tracking-wider text-fg-muted">{children}</h2>;
}

function SettingRow({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-start gap-4">
      <div className="w-32 shrink-0 pt-2">
        <div className="text-sm text-fg-secondary">{label}</div>
        {hint && <div className="mt-0.5 text-[11px] text-fg-muted">{hint}</div>}
      </div>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

function ToggleRow({ label, checked, onChange }: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between">
      <div className="text-sm text-fg-secondary">{label}</div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors ${
          checked ? 'bg-accent' : 'bg-edge'
        }`}
      >
        <span
          className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform ${
            checked ? 'translate-x-[18px]' : 'translate-x-[3px]'
          }`}
        />
      </button>
    </div>
  );
}

function McpSettingsSection() {
  const { t } = useI18n();
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const [allTools, setAllTools] = useState<string[]>([]);
  const [disabledTools, setDisabledTools] = useState<string[]>([]);
  const [saved, setSaved] = useState(false);
  const [running, setRunning] = useState(false);
  const [toggling, setToggling] = useState(false);
  const [toggleError, setToggleError] = useState<string | null>(null);

  useEffect(() => {
    void aiCommands.mcpListAllTools().then(setAllTools).catch(() => {});
  }, []);

  useEffect(() => {
    setDisabledTools(settings.mcpDisabledTools ?? []);
  }, [settings.mcpDisabledTools]);

  const refreshStatus = async () => {
    try {
      const status = await aiCommands.mcpGetStatus();
      setRunning(status.running);
    } catch {
      setRunning(false);
    }
  };

  useEffect(() => {
    void refreshStatus();
    const id = window.setInterval(() => void refreshStatus(), 3000);
    return () => window.clearInterval(id);
  }, []);

  const handleToggleEnabled = async (enabled: boolean) => {
    setToggling(true);
    setToggleError(null);
    try {
      await updateSettings({ ...settings, mcpServerEnabled: enabled });
      if (enabled) {
        await aiCommands.mcpStartStdio();
      } else {
        await aiCommands.mcpStop();
      }
      await refreshStatus();
    } catch (e) {
      setToggleError(e instanceof Error ? e.message : t('mcp.toggleError'));
      // Revert preference if start failed after enabling
      if (enabled) {
        await updateSettings({ ...settings, mcpServerEnabled: false }).catch(() => {});
      }
    } finally {
      setToggling(false);
    }
  };

  const toggleTool = (toolName: string) => {
    setDisabledTools((prev) =>
      prev.includes(toolName) ? prev.filter((t) => t !== toolName) : [...prev, toolName],
    );
  };

  const handleSaveTools = async () => {
    await updateSettings({ ...settings, mcpDisabledTools: disabledTools });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const handleEnableAll = () => setDisabledTools([]);
  const handleDisableAll = () => setDisabledTools([...allTools]);

  const toolsDirty = JSON.stringify([...disabledTools].sort()) !== JSON.stringify([...(settings.mcpDisabledTools ?? [])].sort());

  return (
    <>
      <SectionTitle>{t('mcp.title')}</SectionTitle>
      <p className="text-xs text-fg-muted">{t('mcp.description')}</p>

      <ToggleRow
        label={t('mcp.enabled')}
        checked={settings.mcpServerEnabled ?? false}
        onChange={(v) => {
          if (!toggling) void handleToggleEnabled(v);
        }}
      />
      <p className="text-xs text-fg-muted -mt-1">{t('mcp.enabledHint')}</p>

      <SettingRow label={t('mcp.status')}>
        <div className="flex items-center gap-2 pt-2">
          <span
            className={`inline-block h-2 w-2 rounded-full ${running ? 'bg-green-500' : 'bg-fg-muted'}`}
          />
          <span className="text-sm text-fg">
            {running ? t('mcp.running') : t('mcp.stopped')}
          </span>
        </div>
      </SettingRow>

      {toggleError && (
        <p className="text-xs text-red-500">{toggleError}</p>
      )}

      <div className="rounded-md border border-edge bg-surface p-3">
        <p className="text-xs text-fg-muted">{t('mcp.usage')}</p>
        <pre className="mt-2 text-xs font-mono text-fg-secondary whitespace-pre-wrap break-all">
{`{
  "mcpServers": {
    "datazen": {
      "command": "datazen",
      "args": ["--mcp"]
    }
  }
}`}
        </pre>
      </div>

      {allTools.length > 0 && (
        <>
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-fg">{t('mcp.tools')}</h3>
            <div className="flex items-center gap-2">
              <button type="button" onClick={handleEnableAll} className="text-xs text-accent hover:underline">
                {t('mcp.tools.enableAll')}
              </button>
              <span className="text-fg-muted">|</span>
              <button type="button" onClick={handleDisableAll} className="text-xs text-accent hover:underline">
                {t('mcp.tools.disableAll')}
              </button>
            </div>
          </div>
          <p className="text-xs text-fg-muted">{t('mcp.tools.description')}</p>

          <div className="space-y-1">
            {allTools.map((tool) => {
              const enabled = !disabledTools.includes(tool);
              return (
                <div key={tool} className="flex items-center justify-between rounded-md border border-edge bg-surface px-3 py-2">
                  <span className="text-sm font-mono text-fg">{tool}</span>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={enabled}
                    onClick={() => toggleTool(tool)}
                    className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors ${
                      enabled ? 'bg-accent' : 'bg-edge'
                    }`}
                  >
                    <span
                      className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform ${
                        enabled ? 'translate-x-[18px]' : 'translate-x-[3px]'
                      }`}
                    />
                  </button>
                </div>
              );
            })}
          </div>

          <p className="text-xs text-fg-muted">{t('mcp.tools.restartHint')}</p>

          <div className="flex items-center gap-3">
            {saved && <span className="text-xs text-green-500">{t('settings.saved')}</span>}
            <Button variant="primary" disabled={!toolsDirty} onClick={() => void handleSaveTools()}>
              {t('common.save')}
            </Button>
          </div>
        </>
      )}
    </>
  );
}

function McpClientSection() {
  const { t } = useI18n();
  const {
    mcpServers,
    mcpConnecting,
    mcpError,
    connectMcpServer,
    disconnectMcpServer,
    loadMcpServers,
    clearMcpError,
  } = useAiStore();

  const [showForm, setShowForm] = useState(false);
  const [draft, setDraft] = useState<McpServerConfig>({
    id: '',
    name: '',
    transport: 'stdio',
    command: '',
    args: [],
    env: {},
    enabled: true,
  });
  const [argsText, setArgsText] = useState('');

  useEffect(() => {
    void loadMcpServers();
  }, [loadMcpServers]);

  const handleConnect = async () => {
    if (!draft.id.trim() || !draft.command?.trim()) return;
    const config: McpServerConfig = {
      ...draft,
      args: argsText
        .split('\n')
        .map((s) => s.trim())
        .filter(Boolean),
    };
    try {
      await connectMcpServer(config);
      setShowForm(false);
      setDraft({ id: '', name: '', transport: 'stdio', command: '', args: [], env: {}, enabled: true });
      setArgsText('');
    } catch {
      // error is already set in store by connectMcpServer
    }
  };

  const inputClass = 'h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500';

  return (
    <>
      <SectionTitle>{t('mcpClient.title')}</SectionTitle>
      <p className="text-xs text-fg-muted">{t('mcpClient.description')}</p>

      {mcpError && (
        <div className="flex items-center justify-between rounded-md border border-red-500/30 bg-red-500/10 px-3 py-1.5">
          <span className="text-xs text-red-500">{mcpError}</span>
          <button onClick={clearMcpError} className="text-xs text-red-500 underline">
            {t('common.close')}
          </button>
        </div>
      )}

      {mcpServers.length === 0 && !showForm && (
        <p className="text-xs text-fg-muted">{t('mcpClient.noServers')}</p>
      )}

      {mcpServers.length > 0 && (
        <div className="space-y-1">
          {mcpServers.map((s) => (
            <div
              key={s.serverId}
              className="flex items-center justify-between rounded-md border border-edge bg-surface p-2"
            >
              <div>
                <span className="text-sm text-fg">{s.serverName}</span>
                <span className="ml-2 text-xs text-fg-muted">
                  ({s.toolsCount} {t('mcpClient.tools')})
                </span>
              </div>
              <Button
                variant="secondary"
                onClick={() => void disconnectMcpServer(s.serverId)}
              >
                {t('mcpClient.disconnect')}
              </Button>
            </div>
          ))}
        </div>
      )}

      {showForm ? (
        <div className="space-y-3 rounded-md border border-edge bg-surface-alt p-3">
          <SettingRow label="ID">
            <input
              type="text"
              value={draft.id}
              onChange={(e) => setDraft((d) => ({ ...d, id: e.target.value }))}
              placeholder="my-mcp-server"
              className={inputClass}
            />
          </SettingRow>
          <SettingRow label={t('mcpClient.serverName')}>
            <input
              type="text"
              value={draft.name}
              onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))}
              placeholder="My Server"
              className={inputClass}
            />
          </SettingRow>
          <SettingRow label={t('mcpClient.command')}>
            <PathInput
              value={draft.command ?? ''}
              onChange={(command) => setDraft((d) => ({ ...d, command }))}
              placeholder="/usr/local/bin/my-mcp"
            />
          </SettingRow>
          <SettingRow label={t('mcpClient.args')}>
            <textarea
              value={argsText}
              onChange={(e) => setArgsText(e.target.value)}
              placeholder="--flag1&#10;--flag2"
              rows={3}
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
              className="w-full rounded-md border border-edge bg-surface px-3 py-2 text-sm text-fg outline-none focus:border-blue-500"
            />
          </SettingRow>
          <div className="flex gap-2">
            <Button
              variant="primary"
              disabled={mcpConnecting || !draft.id.trim() || !draft.command?.trim()}
              onClick={() => void handleConnect()}
            >
              {mcpConnecting ? t('mcpClient.connecting') : t('mcpClient.connect')}
            </Button>
            <Button variant="secondary" onClick={() => setShowForm(false)}>
              {t('common.cancel')}
            </Button>
          </div>
        </div>
      ) : (
        <Button variant="secondary" onClick={() => setShowForm(true)}>
          {t('mcpClient.addServer')}
        </Button>
      )}
    </>
  );
}

function AiSettingsSection() {
  const { t } = useI18n();
  const {
    config,
    isConfigured,
    providers,
    configError,
    validating,
    saving,
    remoteModels,
    fetchingRemoteModels,
    loadConfig,
    loadProviders,
    validateConfig,
    saveConfig,
    deleteConfig,
    fetchRemoteModels,
    clearError,
  } = useAiStore();

  const [aiDraft, setAiDraft] = useState<AiProviderConfig>({
    providerType: 'open_ai',
    apiKey: '',
    endpoint: '',
    model: '',
    maxTokens: 200000,
  });
  const [customProtocol, setCustomProtocol] = useState<string>('open_ai_compatible');
  const [manualModelInput, setManualModelInput] = useState(false);
  const [validateOk, setValidateOk] = useState(false);
  const [saveOk, setSaveOk] = useState(false);

  useEffect(() => {
    void loadProviders();
    void loadConfig();
  }, [loadProviders, loadConfig]);

  useEffect(() => {
    if (config) {
      const providerType = isKnownProviderType(config.providerType)
        ? config.providerType
        : 'open_ai';
      setAiDraft({
        providerType,
        apiKey: config.apiKey ?? '',
        endpoint: config.endpoint ?? '',
        model: config.model,
        maxTokens: config.maxTokens ?? 200000,
        extra: config.extra,
      });
      if (config.providerType === 'custom' && config.extra?.protocol) {
        setCustomProtocol(config.extra.protocol as string);
      }
    }
  }, [config]);

  const isCustom = aiDraft.providerType === 'custom';

  const selectedProvider = providers.find(
    (p) => p.providerType === aiDraft.providerType,
  );

  const handleProviderChange = (val: string) => {
    const providerType = val as AiProviderType;
    const provider = providers.find((p) => p.providerType === providerType);
    setAiDraft({
      providerType,
      apiKey: '',
      endpoint: provider?.defaultEndpoint ?? '',
      model: '',
      extra: providerType === 'custom' ? { protocol: customProtocol } : undefined,
    });
    if (provider) {
      setCustomProtocol(provider.defaultProtocol || 'open_ai_compatible');
    }
    setManualModelInput(false);
    setValidateOk(false);
    setSaveOk(false);
    clearError();
  };

  const handleProtocolChange = (val: string) => {
    setCustomProtocol(val);
    setAiDraft((d) => ({
      ...d,
      model: '',
      extra: { protocol: val },
    }));
    setManualModelInput(false);
  };

  const handleFetchModels = async () => {
    const endpoint = aiDraft.endpoint?.trim();
    const apiKey = aiDraft.apiKey?.trim();
    if (!endpoint || !apiKey) return;
    const protocol = isCustom
      ? customProtocol
      : (selectedProvider?.defaultProtocol ?? 'open_ai_compatible');
    await fetchRemoteModels(protocol, endpoint, apiKey);
  };

  const handleValidate = async () => {
    setValidateOk(false);
    const configToValidate = isCustom
      ? { ...aiDraft, extra: { protocol: customProtocol } }
      : aiDraft;
    const ok = await validateConfig(configToValidate);
    if (ok) {
      setValidateOk(true);
      setTimeout(() => setValidateOk(false), 3000);
    }
  };

  const handleSave = async () => {
    setSaveOk(false);
    const configToSave = isCustom
      ? { ...aiDraft, extra: { protocol: customProtocol } }
      : aiDraft;
    const ok = await saveConfig(configToSave);
    if (ok) {
      setSaveOk(true);
      setTimeout(() => setSaveOk(false), 3000);
    }
  };

  const handleDelete = async () => {
    await deleteConfig();
    const openAiProvider = providers.find((p) => p.providerType === 'open_ai');
    setAiDraft({
      providerType: 'open_ai',
      apiKey: '',
      endpoint: openAiProvider?.defaultEndpoint ?? '',
      model: '',
    });
    setManualModelInput(false);
    setSaveOk(false);
    setValidateOk(false);
  };

  const providerOptions = providers.map((p) => ({
    value: p.providerType,
    label: p.displayName,
  }));

  const protocolOptions = [
    { value: 'open_ai_compatible', label: t('settings.ai.protocolOpenAiChat') },
    { value: 'open_ai_responses', label: t('settings.ai.protocolOpenAiResponses') },
    { value: 'anthropic_compatible', label: t('settings.ai.protocolAnthropic') },
  ];

  const modelOptions = remoteModels.map((m) => ({ value: m.id, label: m.displayName }));

  const canFetchModels = !!(aiDraft.endpoint?.trim()) && !!(aiDraft.apiKey?.trim());

  const endpointPlaceholder = isCustom
    ? customProtocol === 'anthropic_compatible'
      ? t('settings.ai.endpointHintAnthropic')
      : customProtocol === 'open_ai_responses'
        ? t('settings.ai.endpointHintOpenAiResponses')
        : t('settings.ai.endpointHintOpenAiChat')
    : t('settings.ai.endpointPlaceholder');

  const inputClass = 'h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25';

  return (
    <>
      <SectionTitle>{t('settings.ai')}</SectionTitle>
      <p className="text-xs text-fg-muted">{t('settings.ai.description')}</p>

      {isConfigured && (
        <div className="flex items-center gap-2">
          <span className="inline-block h-2 w-2 rounded-full bg-green-500" />
          <span className="text-xs text-green-500">{t('settings.ai.configured')}</span>
        </div>
      )}

      <SettingRow label={t('settings.ai.provider')}>
        <Select
          value={aiDraft.providerType}
          options={providerOptions}
          onChange={handleProviderChange}
        />
      </SettingRow>

      {isCustom && (
        <SettingRow label={t('settings.ai.protocol')}>
          <Select
            value={customProtocol}
            options={protocolOptions}
            onChange={handleProtocolChange}
          />
        </SettingRow>
      )}

      <SettingRow label={t('settings.ai.apiKey')}>
        <input
          type="password"
          value={aiDraft.apiKey ?? ''}
          onChange={(e) =>
            setAiDraft((d) => ({ ...d, apiKey: e.target.value }))
          }
          placeholder={t('settings.ai.apiKeyPlaceholder')}
          className={inputClass}
        />
      </SettingRow>

      <SettingRow label={t('settings.ai.endpoint')}>
        <input
          type="text"
          value={aiDraft.endpoint ?? ''}
          onChange={(e) =>
            setAiDraft((d) => ({ ...d, endpoint: e.target.value }))
          }
          placeholder={endpointPlaceholder}
          className={inputClass}
        />
      </SettingRow>

      {isCustom && (
        <p className="text-xs text-fg-muted">{t('settings.ai.customHint')}</p>
      )}

      <div className="flex items-center gap-2">
        <Button
          variant="secondary"
          onClick={() => void handleFetchModels()}
          disabled={!canFetchModels || fetchingRemoteModels}
        >
          {fetchingRemoteModels ? t('settings.ai.fetchingModels') : t('settings.ai.fetchModels')}
        </Button>
        {modelOptions.length > 0 && (
          <label className="flex items-center gap-1.5 text-xs text-fg-muted">
            <input
              type="checkbox"
              checked={manualModelInput}
              onChange={(e) => setManualModelInput(e.target.checked)}
              className="rounded"
            />
            {t('settings.ai.modelManual')}
          </label>
        )}
      </div>

      <SettingRow label={t('settings.ai.model')}>
        {manualModelInput || modelOptions.length === 0 ? (
          <input
            type="text"
            value={aiDraft.model}
            onChange={(e) => setAiDraft((d) => ({ ...d, model: e.target.value }))}
            placeholder="e.g. gpt-4o, claude-sonnet-4-20250514"
            className={inputClass}
          />
        ) : (
          <Select
            value={aiDraft.model}
            options={modelOptions}
            onChange={(v) => setAiDraft((d) => ({ ...d, model: v }))}
          />
        )}
      </SettingRow>

      <SettingRow label={t('settings.ai.maxTokens')}>
        <input
          type="number"
          value={aiDraft.maxTokens ?? 200000}
          onChange={(e) =>
            setAiDraft((d) => ({ ...d, maxTokens: parseInt(e.target.value, 10) || 200000 }))
          }
          min={1000}
          className={inputClass}
        />
      </SettingRow>

      {configError && (
        <p className="text-xs text-red-500">{configError}</p>
      )}

      <div className="flex items-center gap-3">
        <Button
          variant="secondary"
          onClick={() => void handleValidate()}
          disabled={validating}
        >
          {validating
            ? t('settings.ai.validating')
            : validateOk
              ? t('settings.ai.validated')
              : t('settings.ai.validate')}
        </Button>

        <Button
          variant="primary"
          onClick={() => void handleSave()}
          disabled={saving}
        >
          {saving ? t('settings.ai.saving') : saveOk ? t('settings.ai.saved') : t('settings.ai.save')}
        </Button>

        {isConfigured && (
          <Button variant="secondary" onClick={() => void handleDelete()}>
            {t('settings.ai.delete')}
          </Button>
        )}
      </div>

      <SectionTitle>{t('context.title')}</SectionTitle>

      <SettingRow label={t('context.dirSetting')}>
        <ContextDirSetting />
      </SettingRow>
      <p className="text-xs text-fg-muted">{t('context.dirSettingDesc')}</p>
    </>
  );
}

function ContextDirSetting() {
  const { t } = useI18n();
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const [defaultDir, setDefaultDir] = useState('');
  const [saved, setSaved] = useState(false);
  const [localDir, setLocalDir] = useState(settings.contextDir);

  useEffect(() => {
    import('../../commands/context').then(({ contextCommands }) => {
      void contextCommands.getDir().then(setDefaultDir).catch(() => {});
    });
  }, []);

  useEffect(() => {
    setLocalDir(settings.contextDir);
  }, [settings.contextDir]);

  const handleSave = async () => {
    await updateSettings({ ...settings, contextDir: localDir });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  return (
    <div className="flex items-center gap-2">
      <PathInput
        value={localDir}
        onChange={setLocalDir}
        placeholder={defaultDir || t('context.dirSettingDesc')}
        dialogOptions={{ directory: true }}
        className="flex-1"
      />
      <Button variant="secondary" className="shrink-0 h-9" onClick={() => void handleSave()}>
        {saved ? t('common.success') : t('common.save')}
      </Button>
      <Button
        variant="ghost"
        className="shrink-0 h-9 text-xs"
        onClick={() => void settingsCommands.openContextDir()}
      >
        {t('context.openDir')}
      </Button>
    </div>
  );
}

const SCENARIO_VARIABLES: Record<PromptScenario, string[]> = {
  nl2sql: ['db_type', 'version', 'schema', 'recent'],
  diagnose: ['db_type', 'schema'],
  nl_filter: ['db_type', 'columns'],
  schema_doc_select_tables: ['db_type', 'table_names'],
  schema_doc: ['db_type', 'schema'],
  connection_diagnose: [],
  query_summary: [],
  explain_analysis: ['db_type'],
  chat: [],
  workflow_generate: ['db_type', 'schema', 'connections'],
};

const SOURCE_BADGE_CLASSES: Record<PromptSource, string> = {
  default: 'bg-edge text-fg-muted',
  driver: 'bg-blue-500/15 text-blue-500',
  user: 'bg-green-500/15 text-green-500',
};

function PromptSettingsSection() {
  const { t } = useI18n();
  const [driverType, setDriverType] = useState<string>('*');
  const [prompts, setPrompts] = useState<PromptInfo[]>([]);
  const [editingIdx, setEditingIdx] = useState<number | null>(null);
  const [editZh, setEditZh] = useState('');
  const [editEn, setEditEn] = useState('');
  const [feedback, setFeedback] = useState('');
  const driverOptions = [
    { value: '*', label: t('settings.prompts.allDrivers') },
    ...(Object.entries(DB_REGISTRY) as [DatabaseType, { label: string }][]).map(
      ([key, meta]) => ({ value: key, label: meta.label }),
    ),
  ];

  const loadPrompts = useCallback(async () => {
    const dt = driverType === '*' ? undefined : driverType;
    const list = await aiCommands.promptList(dt);
    setPrompts(list);
    setEditingIdx(null);
  }, [driverType]);

  useEffect(() => {
    void loadPrompts();
  }, [loadPrompts]);

  const handleEdit = (idx: number) => {
    setEditingIdx(idx);
    setEditZh(prompts[idx].systemZh);
    setEditEn(prompts[idx].systemEn);
  };

  const handleSave = async () => {
    if (editingIdx === null) return;
    const p = prompts[editingIdx];
    const entry: PromptOverrideEntry = {
      driverType,
      scenario: p.scenario,
      systemZh: editZh,
      systemEn: editEn,
    };
    await aiCommands.promptSetOverride(entry);
    setFeedback(t('settings.prompts.saved'));
    setTimeout(() => setFeedback(''), 2000);
    setEditingIdx(null);
    void loadPrompts();
  };

  const handleReset = async (p: PromptInfo) => {
    if (!confirm(t('settings.prompts.resetConfirm'))) return;
    await aiCommands.promptRemoveOverride(driverType, p.scenario);
    setFeedback(t('settings.prompts.resetDone'));
    setTimeout(() => setFeedback(''), 2000);
    void loadPrompts();
  };

  const sourceLabel = (source: PromptSource) => {
    return t(
      source === 'default'
        ? 'settings.prompts.source.default'
        : source === 'driver'
          ? 'settings.prompts.source.driver'
          : 'settings.prompts.source.user',
    );
  };

  const textareaClass =
    'w-full rounded-md border border-edge bg-surface px-3 py-2 text-xs font-mono text-fg outline-none focus:border-blue-500 resize-y min-h-[80px]';

  return (
    <>
      <SectionTitle>{t('settings.prompts')}</SectionTitle>
      <p className="text-xs text-fg-muted">{t('settings.prompts.description')}</p>

      <SettingRow label={t('settings.prompts.driver')}>
        <Select value={driverType} options={driverOptions} onChange={setDriverType} />
      </SettingRow>

      {feedback && <p className="text-xs text-green-500">{feedback}</p>}

      <div className="space-y-2">
        {prompts.map((p, idx) => {
          const isEditing = editingIdx === idx;
          const vars = SCENARIO_VARIABLES[p.scenario] ?? [];
          return (
            <div
              key={p.scenario}
              className="rounded-md border border-edge bg-surface p-3 space-y-2"
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium text-fg">{p.label}</span>
                  <span
                    className={cn(
                      'rounded-full px-2 py-0.5 text-[10px] font-medium',
                      SOURCE_BADGE_CLASSES[p.source],
                    )}
                  >
                    {sourceLabel(p.source)}
                  </span>
                </div>
                <div className="flex items-center gap-1">
                  {!isEditing && (
                    <Button variant="secondary" onClick={() => handleEdit(idx)}>
                      {t('settings.prompts.edit')}
                    </Button>
                  )}
                  {p.source === 'user' && !isEditing && (
                    <Button variant="secondary" onClick={() => void handleReset(p)}>
                      {t('settings.prompts.reset')}
                    </Button>
                  )}
                </div>
              </div>

              {vars.length > 0 && (
                <p className="text-[10px] text-fg-muted">
                  {t('settings.prompts.variables')}:{' '}
                  {vars.map((v) => `{{${v}}}`).join(', ')}
                </p>
              )}

              {isEditing ? (
                <div className="space-y-2">
                  <div>
                    <label className="block text-xs text-fg-secondary mb-1">
                      {t('settings.prompts.zh')}
                    </label>
                    <textarea
                      value={editZh}
                      onChange={(e) => setEditZh(e.target.value)}
                      rows={6}
                      className={textareaClass}
                    />
                  </div>
                  <div>
                    <label className="block text-xs text-fg-secondary mb-1">
                      {t('settings.prompts.en')}
                    </label>
                    <textarea
                      value={editEn}
                      onChange={(e) => setEditEn(e.target.value)}
                      rows={6}
                      className={textareaClass}
                    />
                  </div>
                  <div className="flex gap-2">
                    <Button variant="primary" onClick={() => void handleSave()}>
                      {t('common.save')}
                    </Button>
                    <Button variant="secondary" onClick={() => setEditingIdx(null)}>
                      {t('common.cancel')}
                    </Button>
                  </div>
                </div>
              ) : (
                <pre className="max-h-20 overflow-y-auto whitespace-pre-wrap text-[11px] text-fg-muted bg-surface-alt rounded p-2">
                  {p.systemZh.slice(0, 200)}
                  {p.systemZh.length > 200 ? '…' : ''}
                </pre>
              )}
            </div>
          );
        })}
      </div>
    </>
  );
}
