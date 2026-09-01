import { useCallback, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { historyCommands, type HistoryPurgeScope } from '../../commands/history';
import { SectionTitle, SettingRow } from './settingsUi';

const RETENTION_PRESETS = [7, 30, 90] as const;

type RetentionMode = 'preset' | 'custom' | 'clearAll';

export function DataCleanupSection() {
  const { t } = useI18n();
  const [confirmCleanup, confirmCleanupDialog] = useConfirmDialog();
  const [scopeQuery, setScopeQuery] = useState(true);
  const [scopeWorkflow, setScopeWorkflow] = useState(true);
  const [retentionMode, setRetentionMode] = useState<RetentionMode>('preset');
  const [presetDays, setPresetDays] = useState<number>(30);
  const [customDays, setCustomDays] = useState(30);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ kind: 'success' | 'error'; text: string } | null>(null);

  const resolveScope = useCallback((): HistoryPurgeScope | null => {
    if (scopeQuery && scopeWorkflow) return 'all';
    if (scopeQuery) return 'query';
    if (scopeWorkflow) return 'workflow';
    return null;
  }, [scopeQuery, scopeWorkflow]);

  const handleRun = useCallback(async () => {
    const scope = resolveScope();
    if (!scope) {
      setMessage({ kind: 'error', text: t('settings.dataCleanup.noScope') });
      return;
    }

    const retainDays =
      retentionMode === 'clearAll'
        ? null
        : retentionMode === 'custom'
          ? Math.min(365, Math.max(1, customDays))
          : presetDays;

    const confirmMessage =
      retentionMode === 'clearAll'
        ? t('settings.dataCleanup.confirmClearAll')
        : t('settings.dataCleanup.confirmMessage', { days: retainDays ?? 0 });

    const ok = await confirmCleanup({
      title: t('settings.dataCleanup.confirmTitle'),
      message: confirmMessage,
      kind: 'warning',
    });
    if (!ok) return;

    setBusy(true);
    setMessage(null);
    try {
      const count = await historyCommands.purgeHistory({ scope, retainDays });
      setMessage({
        kind: 'success',
        text: t('settings.dataCleanup.success', { count }),
      });
    } catch {
      setMessage({ kind: 'error', text: t('settings.dataCleanup.error') });
    } finally {
      setBusy(false);
    }
  }, [confirmCleanup, customDays, presetDays, resolveScope, retentionMode, t]);

  return (
    <div className="space-y-4 rounded-md border border-edge bg-surface-alt p-4">
      <div>
        <h3 className="text-sm font-medium text-fg">{t('settings.dataCleanup.title')}</h3>
        <p className="mt-1 text-xs text-fg-muted">{t('settings.dataCleanup.description')}</p>
      </div>

      <SettingRow label={t('settings.dataCleanup.scope')}>
        <div className="flex flex-col gap-2 text-sm text-fg-secondary">
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={scopeQuery}
              onChange={(e) => setScopeQuery(e.target.checked)}
              data-testid="data-cleanup-scope-query"
            />
            {t('settings.dataCleanup.scopeQuery')}
          </label>
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={scopeWorkflow}
              onChange={(e) => setScopeWorkflow(e.target.checked)}
              data-testid="data-cleanup-scope-workflow"
            />
            {t('settings.dataCleanup.scopeWorkflow')}
          </label>
        </div>
      </SettingRow>

      <SectionTitle>{t('settings.dataCleanup.retention')}</SectionTitle>

      <div className="flex flex-wrap gap-2">
        {RETENTION_PRESETS.map((days) => (
          <button
            key={days}
            type="button"
            onClick={() => {
              setRetentionMode('preset');
              setPresetDays(days);
            }}
            className={`rounded-md border px-3 py-1.5 text-xs transition-colors ${
              retentionMode === 'preset' && presetDays === days
                ? 'border-accent bg-accent/15 text-accent'
                : 'border-edge text-fg-secondary hover:bg-surface-raised'
            }`}
            data-testid={`data-cleanup-preset-${days}`}
          >
            {t(`settings.dataCleanup.preset${days}` as 'settings.dataCleanup.preset7')}
          </button>
        ))}
      </div>

      <SettingRow
        label={t('settings.dataCleanup.customDays')}
        hint={t('settings.dataCleanup.customDaysHint')}
      >
        <div className="flex items-center gap-2">
          <input
            type="number"
            min={1}
            max={365}
            value={customDays}
            onFocus={() => setRetentionMode('custom')}
            onChange={(e) => {
              setRetentionMode('custom');
              setCustomDays(Number(e.target.value) || 1);
            }}
            className="h-9 w-24 rounded-md border border-edge bg-surface px-3 text-sm tabular-nums text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/25"
            data-testid="data-cleanup-custom-days"
          />
        </div>
      </SettingRow>

      <label className="flex items-center gap-2 text-sm text-fg-secondary">
        <input
          type="radio"
          name="retention-mode"
          checked={retentionMode === 'clearAll'}
          onChange={() => setRetentionMode('clearAll')}
          data-testid="data-cleanup-clear-all"
        />
        {t('settings.dataCleanup.clearAll')}
      </label>

      <div className="flex items-center gap-3 pt-1">
        <Button
          variant="run"
          disabled={busy}
          onClick={() => void handleRun()}
          data-testid="data-cleanup-run"
        >
          {t('settings.dataCleanup.run')}
        </Button>
        {message && (
          <span
            className={`text-xs ${message.kind === 'success' ? 'text-success' : 'text-danger'}`}
          >
            {message.text}
          </span>
        )}
      </div>
      {confirmCleanupDialog}
    </div>
  );
}
