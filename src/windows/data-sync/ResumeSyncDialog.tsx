import { AlertTriangle, Play, RefreshCcw } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { useI18n } from '../../hooks/useI18n';
import type { SyncTask } from '../../commands/sync';
import type { ConflictInfo } from './utils';

interface ResumeSyncDialogProps {
  open: boolean;
  resumeTask: SyncTask | null;
  onClose: () => void;
  onConfirm: (restartFromZero: boolean) => void;
}

export function ResumeSyncDialog({ open, resumeTask, onClose, onConfirm }: ResumeSyncDialogProps) {
  const { t } = useI18n();

  return (
    <Dialog
      open={open}
      title={t('sync.resumeTitle')}
      description={resumeTask ? t('sync.resumeDesc', { done: resumeTask.completedTables.length, total: resumeTask.tables.length }) : ''}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t('common.cancel')}</Button>
          <Button variant="ghost" onClick={() => void onConfirm(true)}>
            <RefreshCcw className="h-3.5 w-3.5" /> {t('sync.resumeRestart')}
          </Button>
          <Button variant="primary" onClick={() => void onConfirm(false)}>
            <Play className="h-3.5 w-3.5" /> {t('sync.resumeContinue')}
          </Button>
        </>
      }
    >
      <div className="space-y-3 text-sm text-fg-secondary">
        <p>{t('sync.resumeExplain')}</p>
        <div className="rounded-lg border border-edge bg-surface p-3 text-xs">
          <div><span className="font-semibold text-fg">{t('sync.resumeContinue')}</span>：{t('sync.resumeContinueDesc', { count: resumeTask?.completedTables.length ?? 0 })}</div>
          <div className="mt-2"><span className="font-semibold text-fg">{t('sync.resumeRestart')}</span>：{t('sync.resumeRestartDesc', { count: resumeTask?.tables.length ?? 0 })}</div>
        </div>
      </div>
    </Dialog>
  );
}

interface ConflictSyncDialogProps {
  open: boolean;
  conflicts: ConflictInfo[];
  onClose: () => void;
  onConfirm: (restartFromZero: boolean) => void;
}

export function ConflictSyncDialog({ open, conflicts, onClose, onConfirm }: ConflictSyncDialogProps) {
  const { t } = useI18n();

  return (
    <Dialog
      open={open}
      title={t('sync.conflictTitle')}
      description={t('sync.conflictDesc')}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>{t('common.cancel')}</Button>
          <Button variant="ghost" onClick={() => void onConfirm(true)}>
            <RefreshCcw className="h-3.5 w-3.5" /> {t('sync.resumeRestart')}
          </Button>
          <Button variant="primary" onClick={() => void onConfirm(false)}>
            <Play className="h-3.5 w-3.5" /> {t('sync.conflictContinue')}
          </Button>
        </>
      }
    >
      <div className="space-y-3 text-sm">
        <div className="flex items-start gap-2 rounded-lg border border-amber-300/30 bg-amber-500/10 p-3 text-xs text-amber-700 dark:text-amber-300">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <span>{t('sync.conflictWarning')}</span>
        </div>

        <div className="overflow-hidden rounded-lg border border-edge">
          <div className="flex items-center gap-3 bg-surface-alt px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
            <div className="min-w-0 flex-1">{t('sync.tableName')}</div>
            <div className="w-24 text-right">{t('sync.originalRows')}</div>
            <div className="w-24 text-right">{t('sync.currentRows')}</div>
          </div>
          {conflicts.map((c) => (
            <div key={c.table} className="flex items-center gap-3 border-t border-edge px-3 py-1.5 text-xs">
              <div className="min-w-0 flex-1 truncate font-mono text-fg">{c.table}</div>
              <div className="w-24 text-right tabular-nums text-fg-secondary">{c.originalRows.toLocaleString()}</div>
              <div className="w-24 text-right tabular-nums text-amber-600 dark:text-amber-400">{c.currentRows.toLocaleString()}</div>
            </div>
          ))}
        </div>

        <div className="rounded-lg border border-edge bg-surface p-3 text-xs text-fg-muted">
          <div><span className="font-semibold text-fg">{t('sync.conflictContinue')}</span>：{t('sync.conflictContinueDesc')}</div>
          <div className="mt-1"><span className="font-semibold text-fg">{t('sync.resumeRestart')}</span>：{t('sync.conflictRestartDesc')}</div>
        </div>
      </div>
    </Dialog>
  );
}
