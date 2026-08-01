import { useCallback, useEffect, useState } from 'react';
import {
  Bot,
  Code2,
  Globe,
  MousePointerClick,
  Plug,
  Server,
  Table2,
} from 'lucide-react';
import { TitleBar } from '../../components/TitleBar';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useI18n } from '../../hooks/useI18n';
import { getUrlParam } from '../../lib/windowKind';
import { cn } from '../../lib/cn';
import type { AppSettings, AiProviderConfig, AiProviderType, McpServerConfig } from '../../types';
import type { TranslationKey } from '../../locales';

const PAGE_SIZE_OPTIONS = [25, 50, 100, 200, 500];
const RESULT_LIMIT_OPTIONS = [1000, 2000, 5000, 10000, 50000];

const THEME_KEYS: { value: AppSettings['theme']; key: TranslationKey }[] = [
  { value: 'light', key: 'theme.light' },
  { value: 'dark', key: 'theme.dark' },
  { value: 'system', key: 'theme.system' },
];

const LANGUAGE_OPTIONS = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'en', label: 'English' },
];

type SettingsSection = 'general' | 'dataBrowsing' | 'editor' | 'behavior' | 'ai' | 'mcpServer' | 'mcpClient';

const SECTIONS: { id: SettingsSection; labelKey: TranslationKey; icon: React.ComponentType<{ className?: string }> }[] = [
  { id: 'general', labelKey: 'settings.general', icon: Globe },
  { id: 'dataBrowsing', labelKey: 'settings.dataBrowsing', icon: Table2 },
  { id: 'editor', labelKey: 'settings.editor', icon: Code2 },
  { id: 'behavior', labelKey: 'settings.behavior', icon: MousePointerClick },
  { id: 'ai', labelKey: 'settings.ai', icon: Bot },
  { id: 'mcpServer', labelKey: 'mcp.title', icon: Server },
  { id: 'mcpClient', labelKey: 'mcpClient.title', icon: Plug },
];

export function SettingsWindow() {
  useThemeListener();
  const { t } = useI18n();

  const settings = useSettingsStore((s) => s.settings);
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const [draft, setDraft] = useState<AppSettings>(settings);
  const [saved, setSaved] = useState(false);

  const [activeSection, setActiveSection] = useState<SettingsSection>(() => {
    const fromUrl = getUrlParam('section');
    if (fromUrl && SECTIONS.some((s) => s.id === fromUrl)) {
      return fromUrl as SettingsSection;
    }
    return 'general';
  });

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    setDraft(settings);
  }, [settings]);

  const updateField = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setDraft((prev) => ({ ...prev, [key]: value }));
    setSaved(false);
  };

  const handleSave = useCallback(async () => {
    await updateSettings(draft);
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
            const Icon = sec.icon;
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
                <Icon className="h-4 w-4 shrink-0" />
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
                    value={draft.theme}
                    options={themeOptions}
                    onChange={(v) => updateField('theme', v as AppSettings['theme'])}
                  />
                </SettingRow>
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

            {activeSection === 'ai' && <AiSettingsSection />}
            {activeSection === 'mcpServer' && <McpSettingsSection />}
            {activeSection === 'mcpClient' && <McpClientSection />}
          </div>
        </div>
      </div>

      {/* Footer - only show for general settings that use draft/save */}
      {['general', 'dataBrowsing', 'editor', 'behavior'].includes(activeSection) && (
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

  return (
    <>
      <SectionTitle>{t('mcp.title')}</SectionTitle>
      <p className="text-xs text-fg-muted">{t('mcp.description')}</p>

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
            <input
              type="text"
              value={draft.command ?? ''}
              onChange={(e) => setDraft((d) => ({ ...d, command: e.target.value }))}
              placeholder="/usr/local/bin/my-mcp"
              className={inputClass}
            />
          </SettingRow>
          <SettingRow label={t('mcpClient.args')}>
            <textarea
              value={argsText}
              onChange={(e) => setArgsText(e.target.value)}
              placeholder="--flag1&#10;--flag2"
              rows={3}
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
      setAiDraft({
        providerType: config.providerType,
        apiKey: config.apiKey ?? '',
        endpoint: config.endpoint ?? '',
        model: config.model,
        extra: config.extra,
      });
      if (config.providerType === 'custom' && config.extra?.protocol) {
        setCustomProtocol(config.extra.protocol as string);
      }
    }
  }, [config]);

  const isCustom = aiDraft.providerType === 'custom';
  const isOllama = aiDraft.providerType === 'ollama';

  const selectedProvider = providers.find(
    (p) => p.providerType === aiDraft.providerType,
  );

  const handleProviderChange = (val: string) => {
    const providerType = val as AiProviderType;
    const provider = providers.find((p) => p.providerType === providerType);
    setAiDraft({
      providerType,
      apiKey: '',
      endpoint: '',
      model: provider?.defaultModel ?? '',
      extra: providerType === 'custom' ? { protocol: customProtocol } : undefined,
    });
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
    await fetchRemoteModels(customProtocol, endpoint, apiKey);
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
    setAiDraft({
      providerType: 'open_ai',
      apiKey: '',
      endpoint: '',
      model: providers.find((p) => p.providerType === 'open_ai')?.defaultModel ?? '',
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

  const modelOptions = isCustom
    ? remoteModels.map((m) => ({ value: m.id, label: m.displayName }))
    : (selectedProvider?.models ?? []).map((m) => ({ value: m.id, label: m.displayName }));

  const canFetchModels = isCustom && !!(aiDraft.endpoint?.trim()) && !!(aiDraft.apiKey?.trim());

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

      {!isOllama && (
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
      )}

      {isOllama && (
        <p className="text-xs text-fg-muted">{t('settings.ai.ollamaHint')}</p>
      )}

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

      {isCustom && (
        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            onClick={() => void handleFetchModels()}
            disabled={!canFetchModels || fetchingRemoteModels}
          >
            {fetchingRemoteModels ? t('settings.ai.fetchingModels') : t('settings.ai.fetchModels')}
          </Button>
          <label className="flex items-center gap-1.5 text-xs text-fg-muted">
            <input
              type="checkbox"
              checked={manualModelInput}
              onChange={(e) => setManualModelInput(e.target.checked)}
              className="rounded"
            />
            {t('settings.ai.modelManual')}
          </label>
        </div>
      )}

      <SettingRow label={t('settings.ai.model')}>
        {manualModelInput && isCustom ? (
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
    </>
  );
}
