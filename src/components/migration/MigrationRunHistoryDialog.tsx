import { useCallback, useEffect, useState } from 'react';
import { Clock3 } from 'lucide-react';
import {
  historyCommands,
  type MigrationOperation,
  type MigrationRunRecord,
} from '../../commands/history';
import { useI18n } from '../../hooks/useI18n';
import { Button } from '../ui/Button';
import { Dialog } from '../ui/Dialog';
import { ErrorBanner } from '../ui/ErrorBanner';
import { Spinner } from '../ui/Spinner';

export function MigrationRunHistoryDialog({
  operation,
  onReconcile,
}: Readonly<{
  operation: MigrationOperation;
  onReconcile?: (run: MigrationRunRecord) => Promise<boolean> | boolean;
}>) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [items, setItems] = useState<MigrationRunRecord[]>([]);
  const [selected, setSelected] = useState<MigrationRunRecord | null>(null);
  const [error, setError] = useState('');

  const rollbackDisplay = (run: MigrationRunRecord) => {
    if (run.operation !== 'dataTransfer') {
      return { label: run.rollbackOutcome, className: '' };
    }
    const knownOutcomes = [
      'notRequired',
      'notStarted',
      'rolledBack',
      'unknown',
      'partiallyApplied',
    ] as const;
    if (!knownOutcomes.includes(run.rollbackOutcome as (typeof knownOutcomes)[number])) {
      return { label: run.rollbackOutcome, className: '' };
    }
    const isUncertain =
      run.rollbackOutcome === 'unknown' || run.rollbackOutcome === 'partiallyApplied';
    return {
      label: t(`transfer.historyOutcome.${run.rollbackOutcome}`),
      className: isUncertain ? 'font-medium text-amber-700 dark:text-amber-400' : '',
    };
  };

  const reconcileSelected = async () => {
    if (!selected || !onReconcile) return;
    setError('');
    try {
      if (await onReconcile(selected)) setOpen(false);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const load = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      setItems((await historyCommands.listMigrationRuns({ operation })).items);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  }, [operation]);

  useEffect(() => {
    if (open) void load();
  }, [load, open]);

  return (
    <>
      <Button
        variant="secondary"
        className="h-6 px-2 text-xs"
        data-testid="migration-history-open"
        onClick={() => setOpen(true)}
      >
        <Clock3 className="mr-1 h-3 w-3" />
        {t('migrationHistory.open')}
      </Button>
      <Dialog open={open} onClose={() => setOpen(false)} title={t('migrationHistory.title')}>
        <div className="max-h-[65vh] overflow-auto text-xs" data-testid="migration-run-history">
          {loading && (
            <div className="flex items-center gap-2 p-4 text-fg-muted">
              <Spinner size="lg" />
              {t('common.loading')}
            </div>
          )}
          {error && (
            <ErrorBanner as="p" className="p-4">
              {error}
            </ErrorBanner>
          )}
          {!loading && !error && items.length === 0 && (
            <p className="p-4 text-fg-muted">{t('migrationHistory.empty')}</p>
          )}
          {items.map((run) => (
            <button
              key={run.id}
              type="button"
              data-testid={`migration-run-${run.id}`}
              className="grid w-full grid-cols-[1fr_auto] gap-3 border-b border-edge p-3 text-left hover:bg-surface-hover"
              onClick={() => setSelected(run)}
            >
              <span>
                <strong>{run.status}</strong> · {run.phase}
                <br />
                <span className="text-fg-muted">{new Date(run.startedAt).toLocaleString()}</span>
              </span>
              <span>
                {run.committedCount}/{run.selectedCount}
              </span>
            </button>
          ))}
          {selected && (
            <div
              className="m-3 rounded border border-edge bg-surface-secondary p-3"
              data-testid="migration-run-detail"
            >
              <div>
                {t('migrationHistory.outcome')}: {selected.outcome}
              </div>
              <div>
                {t('migrationHistory.counts')}: {selected.committedCount} / {selected.failedCount} /{' '}
                {selected.conflictCount}
              </div>
              <div>
                {t('migrationHistory.rollback')}:{' '}
                <span
                  data-testid="migration-history-rollback-outcome"
                  data-outcome={selected.rollbackOutcome}
                  className={rollbackDisplay(selected).className}
                >
                  {rollbackDisplay(selected).label}
                </span>
              </div>
              {selected.profileId && (
                <div>
                  {t('migrationHistory.profile')}: {selected.profileId} @ {selected.profileRevision}
                </div>
              )}
              {selected.errorSummary && (
                <ErrorBanner className="mt-2">{selected.errorSummary}</ErrorBanner>
              )}
              {operation === 'dataSync' && selected.outcome === 'unknown' && onReconcile && (
                <div className="mt-3 border-t border-edge pt-3">
                  <p className="mb-2 text-fg-secondary">{t('migrationHistory.syncUnknownHint')}</p>
                  <Button
                    variant="primary"
                    size="sm"
                    data-testid="migration-run-reconcile"
                    onClick={() => void reconcileSelected()}
                  >
                    {t('migrationHistory.syncReconcile')}
                  </Button>
                </div>
              )}
            </div>
          )}
        </div>
      </Dialog>
    </>
  );
}
