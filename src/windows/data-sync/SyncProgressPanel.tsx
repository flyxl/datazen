import { CheckCircle2, Clock, Loader2, XCircle } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import {
  formatDuration,
  overallProgressPercent,
  tableProgressPercent,
  type SyncProgress,
} from './utils';

interface SyncProgressPanelProps {
  open: boolean;
  progress: SyncProgress | null;
  elapsed: number;
  onClose: () => void;
}

export function SyncProgressPanel({ open, progress, elapsed, onClose }: SyncProgressPanelProps) {
  const { t } = useI18n();
  const overallProgress = overallProgressPercent(progress);
  const tableProgress = tableProgressPercent(progress);

  return (
    <Dialog
      open={open}
      title={t('sync.progressTitle')}
      onClose={() => {
        if (progress?.phase === 'done' || progress?.phase === 'error') onClose();
      }}
      className="max-w-lg"
      footer={
        (progress?.phase === 'done' || progress?.phase === 'error') ? (
          <Button variant="primary" onClick={onClose}>{t('common.close')}</Button>
        ) : undefined
      }
    >
      <div className="space-y-4">
        <div>
          <div className="mb-1.5 flex items-center justify-between text-xs text-fg-muted">
            <span>{t('sync.overallProgress')}</span>
            <span>{t('sync.tableCount', { done: progress?.completedTables.length ?? 0, total: progress?.totalTables ?? 0 })}</span>
          </div>
          <div className="h-2.5 overflow-hidden rounded-full bg-surface-raised">
            <div
              className={cn(
                'h-full rounded-full transition-all duration-300',
                progress?.phase === 'error' ? 'bg-red-500' : progress?.phase === 'done' ? 'bg-green-500' : 'bg-blue-500',
              )}
              style={{ width: `${Math.min(overallProgress, 100)}%` }}
            />
          </div>
        </div>

        {progress && progress.phase === 'syncing' && (
          <div className="rounded-lg border border-edge bg-surface-alt p-3">
            <div className="mb-1 flex items-center justify-between">
              <span className="truncate font-mono text-sm text-fg">{progress.currentTable}</span>
              <span className="ml-2 shrink-0 text-xs tabular-nums text-fg-muted">
                {t('sync.rowProgress', { synced: progress.syncedRows.toLocaleString(), total: progress.sourceRowCount.toLocaleString() })}
              </span>
            </div>
            <div className="h-1.5 overflow-hidden rounded-full bg-surface-raised">
              <div className="h-full rounded-full bg-blue-400 transition-all duration-200" style={{ width: `${Math.min(tableProgress, 100)}%` }} />
            </div>
          </div>
        )}

        {progress?.phase === 'counting' && (
          <div className="flex items-center gap-2 text-sm text-fg-muted">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('sync.countingRows')}
          </div>
        )}

        {progress?.phase === 'done' && (
          <div className="flex items-center gap-2 text-sm text-green-500">
            <CheckCircle2 className="h-4 w-4" />
            {t('sync.syncDone', { count: progress.completedTables.length })}
          </div>
        )}

        {progress?.phase === 'error' && (
          <div className="flex items-start gap-2 text-sm text-red-500">
            <XCircle className="mt-0.5 h-4 w-4 shrink-0" />
            <span className="break-all">{progress.error}</span>
          </div>
        )}

        <div className="flex items-center gap-1.5 text-xs text-fg-muted">
          <Clock className="h-3.5 w-3.5" />
          {t('sync.elapsed')} {formatDuration(elapsed)}
        </div>

        {progress && progress.completedTables.length > 0 && (
          <details className="text-xs">
            <summary className="cursor-pointer text-fg-muted hover:text-fg">
              {t('sync.completedTables')} ({progress.completedTables.length})
            </summary>
            <div className="mt-1 max-h-32 overflow-auto rounded border border-edge bg-surface p-2 font-mono text-fg-secondary">
              {progress.completedTables.map((tableName) => (
                <div key={tableName} className="flex items-center gap-1">
                  <CheckCircle2 className="h-3 w-3 text-green-500" /> {tableName}
                </div>
              ))}
            </div>
          </details>
        )}
      </div>
    </Dialog>
  );
}
