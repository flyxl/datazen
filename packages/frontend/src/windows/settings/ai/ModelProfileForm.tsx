import { AlertTriangle, CheckCircle2, Layers, Settings2, Shield } from 'lucide-react';
import { Dialog } from '../../../components/ui/Dialog';
import { Button } from '../../../components/ui/Button';
import { Select } from '../../../components/ui/Select';
import { useI18n } from '../../../hooks/useI18n';
import type { AiDataEgressLevel, AiToolPermissionPolicy } from '../../../types';
import { cn } from '../../../lib/cn';
import { SettingRow, ToggleRow } from '../settingsUi';
import type { ModelProfileDraft } from './useModelProfileDraft';

interface ModelProfileFormProps {
  draft: ModelProfileDraft;
  /**
   * `dialog` renders the Cancel/Save action row (settings dialog);
   * `inline` leaves the primary action to the host (first-run journey footer).
   */
  variant?: 'dialog' | 'inline';
  onCancel?: () => void;
}

const inputClass =
  'w-full rounded-md border border-edge bg-surface px-3 py-1.5 text-sm text-fg placeholder:text-fg-muted focus:border-accent focus:outline-none';

/**
 * The AI model profile form (basic + safety gate tabs) used by the settings
 * `ModelProfileDialog` and the first-run journey's "Set up AI assistance" step.
 * One implementation, so the journey can never drift from Settings → AI.
 */
export function ModelProfileForm({
  draft: d,
  variant = 'dialog',
  onCancel,
}: Readonly<ModelProfileFormProps>) {
  const { t } = useI18n();

  return (
    <div
      className="flex flex-col gap-4"
      data-testid={variant === 'inline' ? 'onboarding-ai-form' : 'ai-profile-form'}
    >
      {/* Tab switcher */}
      <div className="flex border-b border-edge">
        <button
          type="button"
          onClick={() => d.setTab('basic')}
          className={cn(
            'flex items-center gap-2 border-b-2 px-4 py-2 text-sm font-medium transition-colors',
            d.tab === 'basic'
              ? 'border-accent text-accent'
              : 'border-transparent text-fg-muted hover:text-fg',
          )}
        >
          <Settings2 className="h-4 w-4" />
          {t('settings.ai.tabBasic')}
        </button>
        <button
          type="button"
          onClick={() => d.setTab('safety')}
          className={cn(
            'flex items-center gap-2 border-b-2 px-4 py-2 text-sm font-medium transition-colors',
            d.tab === 'safety'
              ? 'border-accent text-accent'
              : 'border-transparent text-fg-muted hover:text-fg',
          )}
        >
          <Shield className="h-4 w-4" />
          {t('settings.ai.tabSafety')}
        </button>
      </div>

      {/* Tab 1: Basic configuration */}
      {d.tab === 'basic' && (
        <div className="flex flex-col gap-3 py-1">
          <SettingRow label={t('settings.ai.modelName')}>
            <input
              type="text"
              value={d.draft.name}
              onChange={(e) => d.updateDraft({ name: e.target.value })}
              placeholder={t('settings.ai.modelNamePlaceholder')}
              className={inputClass}
            />
          </SettingRow>

          <SettingRow label={t('settings.ai.provider')}>
            <Select
              value={d.draft.providerType}
              options={d.providerOptions}
              onChange={d.handleProviderChange}
            />
          </SettingRow>

          {d.isCustom && (
            <SettingRow label={t('settings.ai.protocol')}>
              <Select
                value={d.customProtocol}
                options={d.protocolOptions.map((o) => ({
                  value: o.value,
                  label: t(o.labelKey),
                }))}
                onChange={d.handleProtocolChange}
              />
            </SettingRow>
          )}

          <SettingRow label={t('settings.ai.apiKey')}>
            <input
              type="password"
              value={d.draft.apiKey ?? ''}
              onChange={(e) => d.updateDraft({ apiKey: e.target.value })}
              placeholder={t('settings.ai.apiKeyPlaceholder')}
              className={inputClass}
            />
          </SettingRow>

          <SettingRow label={t('settings.ai.endpoint')}>
            <input
              type="text"
              value={d.draft.endpoint ?? ''}
              onChange={(e) => d.updateDraft({ endpoint: e.target.value })}
              placeholder={t('settings.ai.endpointPlaceholder')}
              className={inputClass}
            />
          </SettingRow>

          <div className="flex items-center gap-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void d.handleFetchModels()}
              disabled={d.fetchingRemoteModels}
            >
              {d.fetchingRemoteModels
                ? t('settings.ai.fetchingModels')
                : t('settings.ai.fetchModels')}
            </Button>
            {d.modelOptions.length > 0 && (
              <label className="flex items-center gap-1.5 text-xs text-fg-muted">
                <input
                  type="checkbox"
                  checked={d.manualModelInput}
                  onChange={(e) => d.setManualModelInput(e.target.checked)}
                  className="rounded"
                />
                {t('settings.ai.modelManual')}
              </label>
            )}
          </div>

          <SettingRow label={t('settings.ai.model')}>
            {d.manualModelInput || d.modelOptions.length === 0 ? (
              <input
                type="text"
                value={d.draft.model}
                onChange={(e) => d.updateDraft({ model: e.target.value })}
                placeholder="e.g. gpt-4o, claude-3-7-sonnet, qwen2.5-coder"
                className={inputClass}
              />
            ) : (
              <Select
                value={d.draft.model}
                options={d.modelOptions}
                onChange={(v) => d.updateDraft({ model: v })}
              />
            )}
          </SettingRow>

          <SettingRow label={t('settings.ai.maxTokens')}>
            <input
              type="number"
              value={d.draft.maxTokens ?? 200000}
              onChange={(e) => d.updateDraft({ maxTokens: parseInt(e.target.value, 10) || 200000 })}
              min={1000}
              className={inputClass}
            />
          </SettingRow>

          <ToggleRow
            label={t('settings.ai.setDefault')}
            hint="Use this profile as the default for AI features"
            checked={d.draft.isDefault ?? false}
            onChange={(isDefault) => d.updateDraft({ isDefault })}
          />

          {d.configError && <p className="select-text text-xs text-red-500">{d.configError}</p>}
        </div>
      )}

      {/* Tab 2: Safety gate */}
      {d.tab === 'safety' && (
        <div className="flex flex-col gap-4 py-1">
          <div>
            <label className="mb-2 block text-xs font-semibold text-fg-muted">
              {t('settings.ai.safetyPreset')}
            </label>
            <div className="grid grid-cols-3 gap-2">
              <button
                type="button"
                onClick={() => d.applyPreset('cloud_strict')}
                className={cn(
                  'flex flex-col items-center gap-1 rounded-lg border p-2.5 text-xs font-medium transition-all',
                  d.currentPreset === 'cloud_strict'
                    ? 'border-blue-500 bg-blue-500/10 text-blue-400'
                    : 'border-edge bg-surface text-fg-muted hover:border-edge-focus',
                )}
              >
                <Shield className="h-4 w-4" />
                {t('settings.ai.presetCloudStrict')}
              </button>
              <button
                type="button"
                onClick={() => d.applyPreset('enterprise_balanced')}
                className={cn(
                  'flex flex-col items-center gap-1 rounded-lg border p-2.5 text-xs font-medium transition-all',
                  d.currentPreset === 'enterprise_balanced'
                    ? 'border-amber-500 bg-amber-500/10 text-amber-400'
                    : 'border-edge bg-surface text-fg-muted hover:border-edge-focus',
                )}
              >
                <Layers className="h-4 w-4" />
                {t('settings.ai.presetEnterpriseBalanced')}
              </button>
              <button
                type="button"
                onClick={() => d.applyPreset('local_trust')}
                className={cn(
                  'flex flex-col items-center gap-1 rounded-lg border p-2.5 text-xs font-medium transition-all',
                  d.currentPreset === 'local_trust'
                    ? 'border-emerald-500 bg-emerald-500/10 text-emerald-400'
                    : 'border-edge bg-surface text-fg-muted hover:border-edge-focus',
                )}
              >
                <Shield className="h-4 w-4" />
                {t('settings.ai.presetLocalTrust')}
              </button>
            </div>
          </div>

          <SettingRow label={t('settings.ai.dataEgressLevel')}>
            <Select
              value={d.draft.safetyGate.dataEgressLevel}
              options={[
                { value: 'strict', label: t('settings.ai.egressStrict') },
                { value: 'sample_masked', label: t('settings.ai.egressSampleMasked') },
                { value: 'unrestricted', label: t('settings.ai.egressUnrestricted') },
              ]}
              onChange={(v) => d.updateGate('dataEgressLevel', v as AiDataEgressLevel)}
            />
          </SettingRow>

          {d.draft.safetyGate.dataEgressLevel === 'sample_masked' && (
            <SettingRow label={`${t('settings.ai.maxSampleRows')} (1-10)`}>
              <div className="flex items-center gap-3">
                <input
                  type="range"
                  min={1}
                  max={10}
                  value={d.draft.safetyGate.maxSampleRows}
                  onChange={(e) => d.updateGate('maxSampleRows', parseInt(e.target.value, 10) || 3)}
                  className="flex-1"
                />
                <span className="w-6 text-center font-mono text-xs text-fg">
                  {d.draft.safetyGate.maxSampleRows}
                </span>
              </div>
            </SettingRow>
          )}

          <SettingRow label={t('settings.ai.dbToolPolicy')}>
            <Select
              value={d.draft.safetyGate.dbToolPolicy}
              options={[
                { value: 'disabled', label: t('settings.ai.policyDisabled') },
                { value: 'read_only', label: t('settings.ai.policyReadOnly') },
                { value: 'require_confirm', label: t('settings.ai.policyRequireConfirm') },
                { value: 'unrestricted', label: t('settings.ai.policyUnrestricted') },
              ]}
              onChange={(v) => d.updateGate('dbToolPolicy', v as AiToolPermissionPolicy)}
            />
          </SettingRow>

          <SettingRow label={t('settings.ai.mcpToolPolicy')}>
            <Select
              value={d.draft.safetyGate.mcpToolPolicy}
              options={[
                { value: 'disabled', label: t('settings.ai.policyDisabled') },
                { value: 'read_only', label: t('settings.ai.policyReadOnly') },
                { value: 'require_confirm', label: t('settings.ai.policyRequireConfirm') },
                { value: 'unrestricted', label: t('settings.ai.policyUnrestricted') },
              ]}
              onChange={(v) => d.updateGate('mcpToolPolicy', v as AiToolPermissionPolicy)}
            />
          </SettingRow>

          <ToggleRow
            label={t('settings.ai.requireSqlConfirm')}
            hint="Always show a confirmation prompt before executing AI-generated SQL"
            checked={d.draft.safetyGate.requireSqlConfirm}
            onChange={(val) => d.updateGate('requireSqlConfirm', val)}
          />
        </div>
      )}

      {/* Validate + actions */}
      <div
        className={cn(
          'mt-4 flex items-center gap-3 border-t border-edge pt-4',
          variant === 'dialog' && 'justify-between',
        )}
      >
        <Button
          variant="secondary"
          size="sm"
          onClick={() => void d.handleValidate()}
          disabled={d.validating}
          data-testid="ai-profile-validate"
        >
          {d.validating ? (
            t('settings.ai.validating')
          ) : d.validateOk ? (
            <span className="flex items-center gap-1 text-emerald-400">
              <CheckCircle2 className="h-3.5 w-3.5" />
              {t('settings.ai.validated')}
            </span>
          ) : (
            t('settings.ai.validate')
          )}
        </Button>

        {variant === 'dialog' && (
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" onClick={onCancel} data-testid="ai-profile-cancel">
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={() => void d.requestSave()}
              disabled={d.savingState}
              data-testid="ai-profile-save"
            >
              {d.savingState ? t('settings.ai.saving') : t('common.save')}
            </Button>
          </div>
        )}
      </div>

      {/* High-risk unrestricted warning confirmation */}
      {d.showHighRiskModal && (
        <Dialog
          open={d.showHighRiskModal}
          onClose={() => d.setShowHighRiskModal(false)}
          title={t('settings.ai.unrestrictedWarningTitle')}
          className="max-w-md"
        >
          <div className="flex flex-col gap-4">
            <div className="flex items-start gap-3 rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-amber-400">
              <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0" />
              <div className="text-xs leading-relaxed">
                {t('settings.ai.unrestrictedWarningMessage')}
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <Button variant="secondary" size="sm" onClick={() => d.setShowHighRiskModal(false)}>
                {t('common.cancel')}
              </Button>
              <Button
                variant="primary"
                size="sm"
                className="bg-red-600 text-white hover:bg-red-700"
                onClick={() => void d.confirmHighRisk()}
                data-testid="ai-profile-confirm-risk"
              >
                {t('common.confirm')}
              </Button>
            </div>
          </div>
        </Dialog>
      )}
    </div>
  );
}
