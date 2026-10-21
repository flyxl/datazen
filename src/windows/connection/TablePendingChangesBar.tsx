import { useCallback, useEffect, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { tableSessionController } from '../../lib/session/tableSession';
import type { TableChangeContext } from '../../lib/tableChanges';
import { extractErrorMessage } from '../../stores/tableData/connectionState';

export interface TableTransactionControlsProps {
  panelId: string;
  context: TableChangeContext;
  /** Manual transactions are an editing affordance; read-only panels never show them. */
  isEditable: boolean;
  /** A page load disables the buttons, exactly like the inline toolbar did. */
  loading: boolean;
  /** Re-fetch the current page (rollback and session recovery both need it). */
  onReload: () => void;
}

const TX_ERROR_CLASS =
  'border-b border-red-500/30 bg-red-500/10 px-3 py-1 text-xs text-red-400';
const TX_CONTROLS_CLASS =
  'flex shrink-0 items-center gap-2 border-b border-edge px-3 py-1 text-xs text-fg-muted';
const TX_ACTION_CLASS = 'text-accent hover:underline disabled:opacity-50';

/**
 * Manual transaction toolbar for one table panel: begin / commit / rollback plus the session
 * loss recovery entry. Extracted verbatim from `TableView` (DB-01 Step 10) to make room for
 * the grid selection wiring; every `data-testid` here is consumed by
 * `e2e/specs/table-session-loss-recovery.ts` and `TableView.test.tsx`, so it must not change.
 *
 * The whole lifecycle lives here (not just the buttons) because the component is always
 * mounted: the initial `refreshTxStatus()` runs even for read-only panels, matching the
 * inline implementation it replaces.
 */
export function TableTransactionControls({
  panelId,
  context,
  isEditable,
  loading,
  onReload,
}: TableTransactionControlsProps) {
  const { t } = useI18n();
  const [inTx, setInTx] = useState(false);
  const [txBusy, setTxBusy] = useState(false);
  const [txLost, setTxLost] = useState(false);
  const [txError, setTxError] = useState<string | null>(null);

  const refreshTxStatus = useCallback(async () => {
    try {
      const controller = await tableSessionController(panelId, context);
      const snapshot = await controller.refresh();
      setTxLost(snapshot?.state === 'lost' || snapshot?.state === 'closed');
      const state = snapshot?.observedContext.transactionState;
      setInTx(state === 'active' || state === 'aborted' || state === 'unknown');
    } catch (error) {
      const controller = await tableSessionController(panelId, context);
      setTxLost(controller.snapshot?.state === 'lost' || controller.snapshot?.state === 'closed');
      setTxError(extractErrorMessage(error, t('query.session.disconnected')));
    }
  }, [panelId, context, t]);

  useEffect(() => {
    void refreshTxStatus();
  }, [refreshTxStatus]);

  const executeTableTransaction = useCallback(
    async (operation: 'begin' | 'commit' | 'rollback') => {
      const controller = await tableSessionController(panelId, context);
      try {
        const receipt = await controller.execute({
          command: `platform.${operation}Transaction`,
          input: { defersCommit: true },
        });
        for (;;) {
          const execution = await controller.getExecution(receipt.executionId);
          if (execution.state === 'succeeded') break;
          if (execution.state === 'failed' || execution.state === 'cancelled') {
            throw new Error(
              `${execution.errorCode ?? 'Transaction outcome unavailable'}; effect: ${execution.effectOutcome}`,
            );
          }
          await new Promise<void>((resolve) => setTimeout(resolve, 50));
        }
        await controller.refresh();
      } catch (error) {
        if (!controller.acceptSessionError(error) && controller.snapshot?.state !== 'lost') {
          // Observe resource loss after a terminal execution failure, while preserving the
          // original transaction error and its effect outcome.
          await controller.refresh().catch(() => undefined);
        }
        setTxLost(controller.snapshot?.state === 'lost' || controller.snapshot?.state === 'closed');
        throw error;
      }
    },
    [panelId, context],
  );

  const runTransaction = useCallback(
    async (operation: 'begin' | 'commit' | 'rollback', failureKey: string) => {
      setTxBusy(true);
      setTxError(null);
      try {
        await executeTableTransaction(operation);
        await refreshTxStatus();
        if (operation === 'rollback') onReload();
      } catch (error) {
        setTxError(extractErrorMessage(error, t(failureKey)));
      } finally {
        setTxBusy(false);
      }
    },
    [executeTableTransaction, onReload, refreshTxStatus, t],
  );

  const handleBeginTx = useCallback(() => {
    void runTransaction('begin', 'tableData.txBeginFailed');
  }, [runTransaction]);
  const handleCommitTx = useCallback(() => {
    void runTransaction('commit', 'tableData.txCommitFailed');
  }, [runTransaction]);
  const handleRollbackTx = useCallback(() => {
    void runTransaction('rollback', 'tableData.txRollbackFailed');
  }, [runTransaction]);

  const handleOpenNewSession = useCallback(() => {
    void tableSessionController(panelId, context)
      .then((controller) => controller.openNewSession())
      .then(() => {
        setTxLost(false);
        setInTx(false);
        setTxError(null);
        onReload();
      })
      .catch((error: unknown) => {
        setTxError(extractErrorMessage(error, t('query.session.disconnected')));
      });
  }, [context, onReload, panelId, t]);

  return (
    <>
      {txError && (
        <div data-testid="table-transaction-error" className={TX_ERROR_CLASS}>
          {txError}
          {txLost && (
            <button
              className="ml-3 underline"
              data-testid="table-session-open-new"
              onClick={handleOpenNewSession}
            >
              {t('query.session.openNew')}
            </button>
          )}
        </div>
      )}
      {isEditable && (
        <div className={TX_CONTROLS_CLASS} data-testid="table-tx-controls">
          <span className={cn(inTx ? 'text-accent' : 'text-fg-muted')}>
            {inTx ? t('tableData.inManualTx') : t('tableData.shortTxDefault')}
          </span>
          {!inTx ? (
            <button
              type="button"
              className={TX_ACTION_CLASS}
              onClick={handleBeginTx}
              disabled={txBusy || loading}
              data-testid="table-tx-begin"
            >
              {t('tableData.beginTx')}
            </button>
          ) : (
            <>
              <button
                type="button"
                className={TX_ACTION_CLASS}
                onClick={handleCommitTx}
                disabled={txBusy || loading}
                data-testid="table-tx-commit"
              >
                {t('tableData.commitTx')}
              </button>
              <button
                type="button"
                className={TX_ACTION_CLASS}
                onClick={handleRollbackTx}
                disabled={txBusy || loading}
                data-testid="table-tx-rollback"
              >
                {t('tableData.rollbackTx')}
              </button>
            </>
          )}
          {inTx && <span className="ml-auto text-warning">{t('tableData.txManualHint')}</span>}
        </div>
      )}
    </>
  );
}

export interface TablePendingChangesBarProps {
  /** Total staged changes; the bar renders only when there is at least one. */
  pendingCount: number;
  updateCount: number;
  deleteCount: number;
  /** Staged new rows; shown as its own segment (04 §4.3). */
  insertCount?: number;
  /** Loading or previewing/committing: every action is disabled while true. */
  busy: boolean;
  onPreview: () => void;
  onCommit: () => void;
  onRollback: () => void;
}

const PENDING_BAR_CLASS =
  'flex shrink-0 items-center gap-2 border-b border-amber-500/30 bg-amber-500/10 px-3 py-1.5 text-xs';

/** Staged-change summary and its preview/commit/rollback actions (DB-01 Step 10). */
export function TablePendingChangesBar({
  pendingCount,
  updateCount,
  deleteCount,
  insertCount = 0,
  busy,
  onPreview,
  onCommit,
  onRollback,
}: TablePendingChangesBarProps) {
  const { t } = useI18n();
  if (pendingCount === 0) return null;
  return (
    <div className={PENDING_BAR_CLASS} aria-busy={busy} data-testid="pending-changes-bar">
      <span className="font-medium text-amber-300">
        {t('tableData.pendingChanges', { count: pendingCount })}
      </span>
      {insertCount > 0 && (
        <span className="text-fg-muted">{t('tableData.pendingInserts', { count: insertCount })}</span>
      )}
      {updateCount > 0 && (
        <span className="text-fg-muted">
          {t('tableData.pendingUpdates', { count: updateCount })}
        </span>
      )}
      {deleteCount > 0 && (
        <span className="text-fg-muted">
          {t('tableData.pendingDeletes', { count: deleteCount })}
        </span>
      )}
      <div className="ml-auto flex items-center gap-1">
        <Button
          size="sm"
          variant="ghost"
          disabled={busy}
          onClick={onPreview}
          data-testid="pending-preview"
        >
          {t('tableData.preview')}
        </Button>
        <Button
          size="sm"
          variant="primary"
          disabled={busy}
          onClick={onCommit}
          data-testid="pending-commit"
        >
          {t('tableData.commit')}
        </Button>
        <Button
          size="sm"
          variant="ghost"
          disabled={busy}
          onClick={onRollback}
          data-testid="pending-rollback"
        >
          {t('tableData.rollback')}
        </Button>
      </div>
    </div>
  );
}
