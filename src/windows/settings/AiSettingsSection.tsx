import { useEffect, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { PathInput } from '../../components/ui/PathInput';
import { Select } from '../../components/ui/Select';
import { useSettingsStore } from '../../stores/settingsStore';
import { useAiStore } from '../../stores/aiStore';
import { useI18n } from '../../hooks/useI18n';
import { settingsCommands } from '../../commands/settings';
import { isKnownProviderType } from '../../lib/aiProviders';
import type { AiProviderConfig, AiProviderType } from '../../types';
import { SectionTitle, SettingRow } from './settingsUi';

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

export function AiSettingsSection() {
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
    const apiKey = aiDraft.apiKey?.trim() || (isOllama ? 'ollama' : '');
    if (!endpoint || (!apiKey && !isOllama)) return;
    const protocol = isCustom
      ? customProtocol
      : (selectedProvider?.defaultProtocol ?? 'open_ai_compatible');
    await fetchRemoteModels(protocol, endpoint, apiKey || 'ollama');
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

  const canFetchModels =
    !!(aiDraft.endpoint?.trim()) && (!!aiDraft.apiKey?.trim() || isOllama);

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
