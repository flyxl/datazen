import { useCallback, useEffect, useRef, useState } from 'react';
import { CheckCircle2, Loader2, XCircle } from 'lucide-react';
import { Dialog } from '../../components/ui/Dialog';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { cn } from '../../lib/cn';
import { createProgressLogPump } from '../../lib/backupProgress';
import { runSqlFileExecution } from '../../lib/sqlFileExecution';
import { ProgressLog } from '../backup/ProgressLog';

interface ExecuteSqlFileDialogProps {
  open: boolean;
  onClose: () => void;
  connectionId: string;
  database: string | null;
  connectionName: string;
  onExecuted?: () => void;
}

type StatusKind = 'idle' | 'running' | 'success' | 'error';

export function ExecuteSqlFileDialog({
  open,
  onClose,
  connectionId,
  database,
  connectionName,
  onExecuted,
}: ExecuteSqlFileDialogProps) {
  const { t } = useI18n();
  const [confirmExecute, confirmDialog] = useConfirmDialog();
  const [running, setRunning] = useState(false);
  const [statusMessage, setStatusMessage] = useState('');
  const [statusKind, setStatusKind] = useState<StatusKind>('idle');
  const [progressLog, setProgressLog] = useState<string[]>([]);
  const logPumpRef = useRef(createProgressLogPump(setProgressLog, 80, t));

  useEffect(() => {
    logPumpRef.current = createProgressLogPump(setProgressLog, 80, t);
  }, [t]);

  useEffect(() => {
    if (open) return;
    setRunning(false);
    setStatusMessage('');
    setStatusKind('idle');
    setProgressLog([]);
  }, [open]);

  const handleExecute = useCallback(async () => {
    if (!database) return;
    setRunning(true);
    setStatusKind('running');
    setStatusMessage(t('backup.restoring'));
    logPumpRef.current.reset([t('backup.restoring')]);
    try {
      const executed = await runSqlFileExecution({
        connectionId,
        database,
        t,
        logPump: logPumpRef.current,
        successMessageKey: 'sqlFile.executeSuccess',
        onProgress: (payload, line) => {
          setStatusMessage(line);
          if (payload?.phase === 'done') {
            setStatusKind('success');
          } else if (line) {
            setStatusKind('running');
          }
        },
        onError: (message) => {
          setStatusMessage(message);
          setStatusKind('error');
        },
        confirmBeforeExecute: async () =>
          confirmExecute({
            title: t('sqlFile.dialogTitle'),
            message: t('sqlFile.executeConfirm'),
            kind: 'warning',
          }),
      });
      if (executed) onExecuted?.();
    } catch {
      /* status + log updated via onError / onProgress */
    } finally {
      setRunning(false);
    }
  }, [connectionId, database, confirmExecute, onExecuted, t]);

  const targetLabel = database
    ? t('sqlFile.targetDatabase', { database })
    : t('sqlFile.noDatabase');

  return (
    <>
      <Dialog
        open={open}
        title={t('sqlFile.dialogTitle')}
        description={`${connectionName} · ${targetLabel}`}
        onClose={() => {
          if (!running) onClose();
        }}
        className="max-w-2xl"
        footer={
          <>
            <Button variant="ghost" disabled={running} onClick={onClose}>
              {t('common.close')}
            </Button>
            <Button
              variant="primary"
              disabled={running || !database}
              onClick={() => void handleExecute()}
              data-testid="sql-file-execute"
            >
              {running ? (
                <>
                  <Loader2 className="mr-1 h-3 w-3 animate-spin" />
                  {t('backup.restoring')}
                </>
              ) : (
                t('sqlFile.selectAndRun')
              )}
            </Button>
          </>
        }
      >
        <div className="flex min-h-[280px] flex-col">
          {statusMessage ? (
            <div
              className={cn(
                'mb-3 flex items-start gap-2 rounded px-3 py-2 text-xs',
                statusKind === 'error' && 'border border-red-500/30 bg-red-500/10 text-red-400',
                statusKind === 'success' &&
                  'border border-green-500/30 bg-green-500/10 text-green-400',
                statusKind === 'running' && 'border border-edge bg-surface-alt text-fg-secondary',
                statusKind === 'idle' && 'text-fg-muted',
              )}
              data-testid="sql-file-status"
              data-status-kind={statusKind}
            >
              {statusKind === 'error' ? (
                <XCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden />
              ) : statusKind === 'success' ? (
                <CheckCircle2 className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden />
              ) : statusKind === 'running' ? (
                <Loader2 className="mt-0.5 h-3.5 w-3.5 shrink-0 animate-spin" aria-hidden />
              ) : null}
              <span className="copyable min-w-0 flex-1 break-words">{statusMessage}</span>
            </div>
          ) : null}
          <ProgressLog lines={progressLog} />
        </div>
      </Dialog>
      {confirmDialog}
    </>
  );
}
