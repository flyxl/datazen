import { useState, useEffect } from 'react';
import { Shield, AlertTriangle, Layers, Settings2, CheckCircle2 } from 'lucide-react';
import { Dialog } from '../../../components/ui/Dialog';
import { Button } from '../../../components/ui/Button';
import { Select } from '../../../components/ui/Select';
import { useI18n } from '../../../hooks/useI18n';
import { useAiStore } from '../../../stores/aiStore';
import {
  AI_SAFETY_PRESETS,
  detectSafetyPreset,
  getDefaultSafetyGate,
  isLocalEndpoint,
  type AiSafetyPresetType,
} from '../../../lib/aiSafetyPresets';
import type {
  AiDataEgressLevel,
  AiModelProfile,
  AiProviderType,
  AiSafetyGateConfig,
  AiToolPermissionPolicy,
} from '../../../types';
import { SettingRow, ToggleRow } from '../settingsUi';

interface ModelProfileDialogProps {
  open: boolean;
  onClose: () => void;
  profile: AiModelProfile | null;
  onSave: (profile: AiModelProfile) => Promise<boolean>;
}

const inputClass =
  'w-full rounded-md border border-edge bg-surface px-3 py-1.5 text-sm text-fg placeholder:text-fg-muted focus:border-accent focus:outline-none';

export function ModelProfileDialog({
  open,
  onClose,
  profile,
  onSave,
}: Readonly<ModelProfileDialogProps>) {
  const { t } = useI18n();
  const {
    providers,
    remoteModels,
    fetchingRemoteModels,
    fetchRemoteModels,
    validateConfig,
    validating,
    configError,
    clearError,
  } = useAiStore();

  const [tab, setTab] = useState<'basic' | 'safety'>('basic');
  const [draft, setDraft] = useState<AiModelProfile>(() => {
    return (
      profile ?? {
        id: crypto.randomUUID(),
        name: '',
        providerType: 'open_ai',
        apiKey: '',
        endpoint: '',
        model: '',
        maxTokens: 200000,
        safetyGate: getDefaultSafetyGate('cloud_strict'),
        isDefault: false,
      }
    );
  });

  const [customProtocol, setCustomProtocol] = useState<string>('open_ai_compatible');
  const [manualModelInput, setManualModelInput] = useState(false);
  const [validateOk, setValidateOk] = useState(false);
  const [showHighRiskModal, setShowHighRiskModal] = useState(false);
  const [savingState, setSavingState] = useState(false);

  useEffect(() => {
    if (profile) {
      setDraft(profile);
      if (profile.providerType === 'custom' && profile.extra?.protocol) {
        setCustomProtocol(profile.extra.protocol as string);
      }
    } else {
      setDraft({
        id: crypto.randomUUID(),
        name: '',
        providerType: 'open_ai',
        apiKey: '',
        endpoint: '',
        model: '',
        maxTokens: 200000,
        safetyGate: getDefaultSafetyGate('cloud_strict'),
        isDefault: false,
      });
    }
    setTab('basic');
    setValidateOk(false);
    clearError();
  }, [profile, open, clearError]);

  const isCustom = draft.providerType === 'custom';
  const isOllama = draft.providerType === 'ollama';
  const selectedProvider = providers.find((p) => p.providerType === draft.providerType);

  const providerOptions = providers.map((p) => ({
    value: p.providerType,
    label: p.displayName,
  }));

  const protocolOptions = [
    { value: 'open_ai_compatible', label: t('settings.ai.protocolOpenAiChat') },
    { value: 'open_ai_responses', label: t('settings.ai.protocolOpenAiResponses') },
    { value: 'anthropic', label: t('settings.ai.protocolAnthropic') },
  ];

  const modelOptions = remoteModels.map((m) => ({
    value: m.id,
    label: m.displayName || m.id,
  }));

  const handleProviderChange = (val: string) => {
    const providerType = val as AiProviderType;
    const provider = providers.find((p) => p.providerType === providerType);
    const isLocal = providerType === 'ollama';
    setDraft((d) => ({
      ...d,
      providerType,
      apiKey: isLocal ? 'ollama' : '',
      endpoint: provider?.defaultEndpoint ?? '',
      model: '',
      safetyGate: getDefaultSafetyGate(isLocal ? 'local_trust' : 'cloud_strict'),
      extra: providerType === 'custom' ? { protocol: customProtocol } : undefined,
    }));
    if (provider) {
      setCustomProtocol(provider.defaultProtocol || 'open_ai_compatible');
    }
    setManualModelInput(false);
    setValidateOk(false);
  };

  const handleFetchModels = async () => {
    const endpoint = draft.endpoint?.trim();
    const apiKey = draft.apiKey?.trim() || (isOllama ? 'ollama' : '');
    if (!endpoint || (!apiKey && !isOllama)) return;
    const protocol = isCustom
      ? customProtocol
      : (selectedProvider?.defaultProtocol ?? 'open_ai_compatible');
    await fetchRemoteModels(protocol, endpoint, apiKey || 'ollama');
  };

  const handleValidate = async () => {
    setValidateOk(false);
    const configToValidate = {
      providerType: draft.providerType,
      apiKey: draft.apiKey,
      endpoint: draft.endpoint,
      model: draft.model,
      maxTokens: draft.maxTokens,
      extra: isCustom ? { protocol: customProtocol } : draft.extra,
    };
    const ok = await validateConfig(configToValidate);
    if (ok) {
      setValidateOk(true);
      setTimeout(() => setValidateOk(false), 3000);
    }
  };

  const applyPreset = (presetKey: AiSafetyPresetType) => {
    setDraft((d) => ({
      ...d,
      safetyGate: { ...AI_SAFETY_PRESETS[presetKey] },
    }));
  };

  const updateGate = <K extends keyof AiSafetyGateConfig>(key: K, val: AiSafetyGateConfig[K]) => {
    setDraft((d) => ({
      ...d,
      safetyGate: {
        ...d.safetyGate,
        [key]: val,
      },
    }));
  };

  const performSave = async () => {
    setSavingState(true);
    const profileToSave: AiModelProfile = {
      ...draft,
      name: draft.name.trim() || draft.model || draft.providerType,
      extra: isCustom ? { protocol: customProtocol } : draft.extra,
    };
    const success = await onSave(profileToSave);
    setSavingState(false);
    if (success) {
      onClose();
    }
  };

  const handleSaveClick = async () => {
    // Check if high-risk: unrestricted egress on non-local endpoint
    const isEgressUnrestricted = draft.safetyGate.dataEgressLevel === 'unrestricted';
    const isRemote = !isLocalEndpoint(draft.endpoint);
    if (isEgressUnrestricted && isRemote) {
      setShowHighRiskModal(true);
      return;
    }
    await performSave();
  };

  const currentPreset = detectSafetyPreset(draft.safetyGate);

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={profile ? t('settings.ai.editModel') : t('settings.ai.addModel')}
      className="max-w-xl"
    >
      <div className="flex flex-col gap-4">
        {/* Tab switcher */}
        <div className="flex border-b border-edge">
          <button
            type="button"
            onClick={() => setTab('basic')}
            className={`flex items-center gap-2 border-b-2 px-4 py-2 text-sm font-medium transition-colors ${
              tab === 'basic'
                ? 'border-accent text-accent'
                : 'border-transparent text-fg-muted hover:text-fg'
            }`}
          >
            <Settings2 className="h-4 w-4" />
            {t('settings.ai.tabBasic')}
          </button>
          <button
            type="button"
            onClick={() => setTab('safety')}
            className={`flex items-center gap-2 border-b-2 px-4 py-2 text-sm font-medium transition-colors ${
              tab === 'safety'
                ? 'border-accent text-accent'
                : 'border-transparent text-fg-muted hover:text-fg'
            }`}
          >
            <Shield className="h-4 w-4" />
            {t('settings.ai.tabSafety')}
          </button>
        </div>

        {/* Tab 1: Basic Configuration */}
        {tab === 'basic' && (
          <div className="flex flex-col gap-3 py-1">
            <SettingRow label={t('settings.ai.modelName')}>
              <input
                type="text"
                value={draft.name}
                onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))}
                placeholder={t('settings.ai.modelNamePlaceholder')}
                className={inputClass}
              />
            </SettingRow>

            <SettingRow label={t('settings.ai.provider')}>
              <Select
                value={draft.providerType}
                options={providerOptions}
                onChange={handleProviderChange}
              />
            </SettingRow>

            {isCustom && (
              <SettingRow label={t('settings.ai.protocol')}>
                <Select
                  value={customProtocol}
                  options={protocolOptions}
                  onChange={(v) => {
                    setCustomProtocol(v);
                    setDraft((d) => ({ ...d, extra: { protocol: v } }));
                  }}
                />
              </SettingRow>
            )}

            <SettingRow label={t('settings.ai.apiKey')}>
              <input
                type="password"
                value={draft.apiKey ?? ''}
                onChange={(e) => setDraft((d) => ({ ...d, apiKey: e.target.value }))}
                placeholder={t('settings.ai.apiKeyPlaceholder')}
                className={inputClass}
              />
            </SettingRow>

            <SettingRow label={t('settings.ai.endpoint')}>
              <input
                type="text"
                value={draft.endpoint ?? ''}
                onChange={(e) => setDraft((d) => ({ ...d, endpoint: e.target.value }))}
                placeholder={t('settings.ai.endpointPlaceholder')}
                className={inputClass}
              />
            </SettingRow>

            <div className="flex items-center gap-2">
              <Button
                variant="secondary"
                size="sm"
                onClick={() => void handleFetchModels()}
                disabled={fetchingRemoteModels}
              >
                {fetchingRemoteModels
                  ? t('settings.ai.fetchingModels')
                  : t('settings.ai.fetchModels')}
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
                  value={draft.model}
                  onChange={(e) => setDraft((d) => ({ ...d, model: e.target.value }))}
                  placeholder="e.g. gpt-4o, claude-3-7-sonnet, qwen2.5-coder"
                  className={inputClass}
                />
              ) : (
                <Select
                  value={draft.model}
                  options={modelOptions}
                  onChange={(v) => setDraft((d) => ({ ...d, model: v }))}
                />
              )}
            </SettingRow>

            <SettingRow label={t('settings.ai.maxTokens')}>
              <input
                type="number"
                value={draft.maxTokens ?? 200000}
                onChange={(e) =>
                  setDraft((d) => ({
                    ...d,
                    maxTokens: parseInt(e.target.value, 10) || 200000,
                  }))
                }
                min={1000}
                className={inputClass}
              />
            </SettingRow>

            <ToggleRow
              label={t('settings.ai.setDefault')}
              hint="Use this profile as the default for AI features"
              checked={draft.isDefault ?? false}
              onChange={(isDefault) => setDraft((d) => ({ ...d, isDefault }))}
            />

            {configError && <p className="text-xs text-red-500">{configError}</p>}
          </div>
        )}

        {/* Tab 2: Safety Gate */}
        {tab === 'safety' && (
          <div className="flex flex-col gap-4 py-1">
            {/* Preset Selector */}
            <div>
              <label className="block text-xs font-semibold text-fg-muted mb-2">
                {t('settings.ai.safetyPreset')}
              </label>
              <div className="grid grid-cols-3 gap-2">
                <button
                  type="button"
                  onClick={() => applyPreset('cloud_strict')}
                  className={`flex flex-col items-center gap-1 rounded-lg border p-2.5 text-xs font-medium transition-all ${
                    currentPreset === 'cloud_strict'
                      ? 'border-blue-500 bg-blue-500/10 text-blue-400'
                      : 'border-edge bg-surface text-fg-muted hover:border-edge-focus'
                  }`}
                >
                  <Shield className="h-4 w-4" />
                  {t('settings.ai.presetCloudStrict')}
                </button>
                <button
                  type="button"
                  onClick={() => applyPreset('enterprise_balanced')}
                  className={`flex flex-col items-center gap-1 rounded-lg border p-2.5 text-xs font-medium transition-all ${
                    currentPreset === 'enterprise_balanced'
                      ? 'border-amber-500 bg-amber-500/10 text-amber-400'
                      : 'border-edge bg-surface text-fg-muted hover:border-edge-focus'
                  }`}
                >
                  <Layers className="h-4 w-4" />
                  {t('settings.ai.presetEnterpriseBalanced')}
                </button>
                <button
                  type="button"
                  onClick={() => applyPreset('local_trust')}
                  className={`flex flex-col items-center gap-1 rounded-lg border p-2.5 text-xs font-medium transition-all ${
                    currentPreset === 'local_trust'
                      ? 'border-emerald-500 bg-emerald-500/10 text-emerald-400'
                      : 'border-edge bg-surface text-fg-muted hover:border-edge-focus'
                  }`}
                >
                  <Shield className="h-4 w-4" />
                  {t('settings.ai.presetLocalTrust')}
                </button>
              </div>
            </div>

            {/* Detailed Controls */}
            <SettingRow label={t('settings.ai.dataEgressLevel')}>
              <Select
                value={draft.safetyGate.dataEgressLevel}
                options={[
                  { value: 'strict', label: t('settings.ai.egressStrict') },
                  { value: 'sample_masked', label: t('settings.ai.egressSampleMasked') },
                  { value: 'unrestricted', label: t('settings.ai.egressUnrestricted') },
                ]}
                onChange={(v) => updateGate('dataEgressLevel', v as AiDataEgressLevel)}
              />
            </SettingRow>

            {draft.safetyGate.dataEgressLevel === 'sample_masked' && (
              <SettingRow label={`${t('settings.ai.maxSampleRows')} (1-10)`}>
                <div className="flex items-center gap-3">
                  <input
                    type="range"
                    min={1}
                    max={10}
                    value={draft.safetyGate.maxSampleRows}
                    onChange={(e) => updateGate('maxSampleRows', parseInt(e.target.value, 10) || 3)}
                    className="flex-1"
                  />
                  <span className="w-6 text-center font-mono text-xs text-fg">
                    {draft.safetyGate.maxSampleRows}
                  </span>
                </div>
              </SettingRow>
            )}

            <SettingRow label={t('settings.ai.dbToolPolicy')}>
              <Select
                value={draft.safetyGate.dbToolPolicy}
                options={[
                  { value: 'disabled', label: t('settings.ai.policyDisabled') },
                  { value: 'read_only', label: t('settings.ai.policyReadOnly') },
                  { value: 'require_confirm', label: t('settings.ai.policyRequireConfirm') },
                  { value: 'unrestricted', label: t('settings.ai.policyUnrestricted') },
                ]}
                onChange={(v) => updateGate('dbToolPolicy', v as AiToolPermissionPolicy)}
              />
            </SettingRow>

            <SettingRow label={t('settings.ai.mcpToolPolicy')}>
              <Select
                value={draft.safetyGate.mcpToolPolicy}
                options={[
                  { value: 'disabled', label: t('settings.ai.policyDisabled') },
                  { value: 'read_only', label: t('settings.ai.policyReadOnly') },
                  { value: 'require_confirm', label: t('settings.ai.policyRequireConfirm') },
                  { value: 'unrestricted', label: t('settings.ai.policyUnrestricted') },
                ]}
                onChange={(v) => updateGate('mcpToolPolicy', v as AiToolPermissionPolicy)}
              />
            </SettingRow>

            <ToggleRow
              label={t('settings.ai.requireSqlConfirm')}
              hint="Always show a confirmation prompt before executing AI-generated SQL"
              checked={draft.safetyGate.requireSqlConfirm}
              onChange={(val) => updateGate('requireSqlConfirm', val)}
            />
          </div>
        )}

        {/* Footer Actions */}
        <div className="mt-4 flex items-center justify-between border-t border-edge pt-4">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void handleValidate()}
            disabled={validating}
          >
            {validating ? (
              t('settings.ai.validating')
            ) : validateOk ? (
              <span className="flex items-center gap-1 text-emerald-400">
                <CheckCircle2 className="h-3.5 w-3.5" />
                {t('settings.ai.validated')}
              </span>
            ) : (
              t('settings.ai.validate')
            )}
          </Button>

          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={onClose}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={() => void handleSaveClick()}
              disabled={savingState}
            >
              {savingState ? t('settings.ai.saving') : t('common.save')}
            </Button>
          </div>
        </div>
      </div>

      {/* High-Risk Unrestricted Warning Confirmation */}
      {showHighRiskModal && (
        <Dialog
          open={showHighRiskModal}
          onClose={() => setShowHighRiskModal(false)}
          title={t('settings.ai.unrestrictedWarningTitle')}
          className="max-w-md"
        >
          <div className="flex flex-col gap-4">
            <div className="flex items-start gap-3 rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-amber-400">
              <AlertTriangle className="h-5 w-5 shrink-0 mt-0.5" />
              <div className="text-xs leading-relaxed">
                {t('settings.ai.unrestrictedWarningMessage')}
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <Button variant="secondary" size="sm" onClick={() => setShowHighRiskModal(false)}>
                {t('common.cancel')}
              </Button>
              <Button
                variant="primary"
                size="sm"
                className="bg-red-600 hover:bg-red-700 text-white"
                onClick={() => {
                  setShowHighRiskModal(false);
                  void performSave();
                }}
              >
                {t('common.confirm')}
              </Button>
            </div>
          </div>
        </Dialog>
      )}
    </Dialog>
  );
}
