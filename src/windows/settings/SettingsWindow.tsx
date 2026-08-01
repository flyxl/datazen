import { useCallback, useEffect, useState } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useI18n } from '../../hooks/useI18n';
import type { AppSettings, AiProviderConfig, AiProviderType } from '../../types';
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

export function SettingsWindow() {
  useThemeListener();
  const { t } = useI18n();

  const settings = useSettingsStore((s) => s.settings);
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const [draft, setDraft] = useState<AppSettings>(settings);
  const [saved, setSaved] = useState(false);

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
      {/* Title bar */}
      <TitleBar title={t('win.settings')} />

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-8 py-6">
        <h1 className="mb-6 text-lg font-semibold text-fg">{t('settings.title')}</h1>

        <div className="mx-auto max-w-lg space-y-6">
          {/* Language */}
          <SettingRow label={t('settings.language')}>
            <Select
              value={draft.language}
              options={LANGUAGE_OPTIONS}
              onChange={(v) => updateField('language', v)}
            />
          </SettingRow>

          {/* Theme */}
          <SettingRow label={t('settings.theme')}>
            <Select
              value={draft.theme}
              options={themeOptions}
              onChange={(v) => updateField('theme', v as AppSettings['theme'])}
            />
          </SettingRow>

          <Divider />

          {/* Data browsing section */}
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

          <Divider />

          {/* Editor section */}
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

          <Divider />

          {/* Behavior section */}
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

          <Divider />

          {/* AI section */}
          <AiSettingsSection />

          <Divider />

          {/* MCP Server section */}
          <McpSettingsSection />
        </div>
      </div>

      {/* Footer */}
      <footer className="flex shrink-0 items-center justify-end gap-3 border-t border-edge px-8 py-3">
        {saved && <span className="text-xs text-green-500">{t('settings.saved')}</span>}
        <Button variant="secondary" onClick={() => void handleClose()}>{t('common.close')}</Button>
        <Button variant="primary" disabled={!isDirty} onClick={() => void handleSave()}>
          {t('common.save')}
        </Button>
      </footer>
    </div>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return <h2 className="text-[13px] font-semibold uppercase tracking-wider text-fg-muted">{children}</h2>;
}

function Divider() {
  return <div className="h-px bg-edge" />;
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

function AiSettingsSection() {
  const { t } = useI18n();
  const {
    config,
    isConfigured,
    providers,
    configError,
    validating,
    saving,
    loadConfig,
    loadProviders,
    validateConfig,
    saveConfig,
    deleteConfig,
    clearError,
  } = useAiStore();

  const [aiDraft, setAiDraft] = useState<AiProviderConfig>({
    providerType: 'open_ai',
    apiKey: '',
    endpoint: '',
    model: '',
  });
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
      });
    }
  }, [config]);

  const selectedProvider = providers.find(
    (p) => p.providerType === aiDraft.providerType,
  );
  const isOllama = aiDraft.providerType === 'ollama';

  const handleProviderChange = (val: string) => {
    const providerType = val as AiProviderType;
    const provider = providers.find((p) => p.providerType === providerType);
    setAiDraft({
      providerType,
      apiKey: '',
      endpoint: '',
      model: provider?.defaultModel ?? '',
    });
    setValidateOk(false);
    setSaveOk(false);
    clearError();
  };

  const handleValidate = async () => {
    setValidateOk(false);
    const ok = await validateConfig(aiDraft);
    if (ok) {
      setValidateOk(true);
      setTimeout(() => setValidateOk(false), 3000);
    }
  };

  const handleSave = async () => {
    setSaveOk(false);
    const ok = await saveConfig(aiDraft);
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
    setSaveOk(false);
    setValidateOk(false);
  };

  const providerOptions = providers.map((p) => ({
    value: p.providerType,
    label: p.displayName,
  }));

  const modelOptions = (selectedProvider?.models ?? []).map((m) => ({
    value: m.id,
    label: m.displayName,
  }));

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

      {!isOllama && (
        <SettingRow label={t('settings.ai.apiKey')}>
          <input
            type="password"
            value={aiDraft.apiKey ?? ''}
            onChange={(e) =>
              setAiDraft((d) => ({ ...d, apiKey: e.target.value }))
            }
            placeholder={t('settings.ai.apiKeyPlaceholder')}
            className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25"
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
          placeholder={t('settings.ai.endpointPlaceholder')}
          className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25"
        />
      </SettingRow>

      <SettingRow label={t('settings.ai.model')}>
        <Select
          value={aiDraft.model}
          options={modelOptions}
          onChange={(v) => setAiDraft((d) => ({ ...d, model: v }))}
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
    </>
  );
}
