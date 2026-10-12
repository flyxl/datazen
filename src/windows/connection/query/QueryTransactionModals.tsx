import { QueryArtifactExport } from '../../../components/query/QueryArtifactExport';
import type { QueryResultArtifacts } from '../../../lib/queryResultArtifacts';
import {
  editorOwnerForPane,
  getQueryPanelSessionRegistry,
  queryPanelExecutionTarget,
} from '../../../lib/session/queryPanelSessionRoot';
import { useCallback, useEffect, useState } from 'react';
import { Download, Gauge, Pin } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { Dialog } from '../../../components/ui/Dialog';
import { CopyableError } from '../../../components/ui/CopyableError';
import { QueryErrorPanel } from '../../../components/query/QueryErrorPanel';
import { DiagnosisPanel } from '../../../components/ai/DiagnosisPanel';
import { ExplainPanel } from '../../../components/ai/ExplainPanel';
import { ResultWorkspace } from '../result-workspace';
import { AddToDashboardDialog } from '../../dashboard/AddToDashboardDialog';
import { DataExportDialog } from '../../../components/DataTable/DataExportDialog';
import { isAbortedTransactionError } from '../../../lib/sqlTransactionGuard';
import { useI18n } from '../../../hooks/useI18n';
import { cn } from '../../../lib/cn';
import { tid } from '../../../lib/tid';
import type { ExplainResult, StatementResult } from '../../../types';
import type { ChartConfig } from '../../../types/chart';
import type { DataExportCapability } from '../../../lib/exportCapability';
import type { QueryActionBuildResult, QueryDiagnosisContext } from '../../../lib/aiQueryActions';
import { Spinner } from '../../../components/ui/Spinner';

export interface UseQueryTransactionOptions {
  dbSessionId: string;
  paneKey?: string;
  connectionId?: string;
  database?: string | null;
  schema?: string | null;
}

export function useQueryTransaction({
  dbSessionId: _dbSessionId,
  paneKey,
  connectionId,
  database,
  schema,
}: UseQueryTransactionOptions) {
  const [inTransaction, setInTransaction] = useState(false);
  const [txBusy, setTxBusy] = useState(false);
  const [txAbortedOpen, setTxAbortedOpen] = useState(false);
  const [txAbortedDetail, setTxAbortedDetail] = useState<string | null>(null);

  const ownedController = useCallback(async () => {
    if (!paneKey || !connectionId) return null;
    return getQueryPanelSessionRegistry().get(
      paneKey,
      queryPanelExecutionTarget(connectionId, database, schema),
      await editorOwnerForPane(paneKey),
    );
  }, [paneKey, connectionId, database, schema]);

  const readTxStatus = useCallback(async () => {
    const controller = await ownedController();
    if (!controller) throw new Error('Query transaction requires a persistent connection owner');
    const session = await controller.refresh();
    const state = session?.observedContext?.transactionState;
    return state === 'active' || state === 'aborted' || state === 'unknown';
  }, [ownedController]);

  const transaction = useCallback(
    async (operation: 'begin' | 'commit' | 'rollback') => {
      const controller = await ownedController();
      if (!controller) throw new Error('Query transaction requires a persistent connection owner');
      const receipt = await controller.execute({
        command: `platform.${operation}Transaction`,
        input: { defersCommit: true },
      });
      for (;;) {
        const execution = await controller.client.getExecution(receipt.executionId);
        if (execution.state === 'succeeded') break;
        if (execution.state === 'failed' || execution.state === 'cancelled') {
          throw new Error(execution.errorCode ?? 'Transaction outcome unavailable');
        }
        await new Promise<void>((resolve) => setTimeout(resolve, 50));
      }
      await controller.refresh();
    },
    [ownedController],
  );

  const refreshTxStatus = useCallback(async () => {
    try {
      setInTransaction(await readTxStatus());
    } catch (error) {
      // Preserve the last observed state: a transport error does not prove rollback.
      setTxAbortedDetail(error instanceof Error ? error.message : String(error));
    }
  }, [readTxStatus]);

  useEffect(() => {
    void refreshTxStatus();
  }, [refreshTxStatus]);

  const handleBeginTx = useCallback(async () => {
    setTxBusy(true);
    try {
      await transaction('begin');
      await refreshTxStatus();
    } catch (e) {
      setTxAbortedDetail(e instanceof Error ? e.message : String(e));
      setTxAbortedOpen(true);
    } finally {
      setTxBusy(false);
    }
  }, [transaction, refreshTxStatus]);

  const handleCommitTx = useCallback(async () => {
    setTxBusy(true);
    try {
      await transaction('commit');
      await refreshTxStatus();
    } catch (e) {
      setTxAbortedDetail(e instanceof Error ? e.message : String(e));
      setTxAbortedOpen(true);
    } finally {
      setTxBusy(false);
    }
  }, [transaction, refreshTxStatus]);

  const handleRollbackTx = useCallback(async () => {
    setTxBusy(true);
    try {
      await transaction('rollback');
      await refreshTxStatus();
    } catch (e) {
      setTxAbortedDetail(e instanceof Error ? e.message : String(e));
      setTxAbortedOpen(true);
    } finally {
      setTxBusy(false);
    }
  }, [transaction, refreshTxStatus]);

  const maybeOfferAbortedDialog = useCallback(
    async (error: string | null | undefined) => {
      await refreshTxStatus();
      const stillInTx = await readTxStatus().catch(() => true);
      if (stillInTx || isAbortedTransactionError(error)) {
        setTxAbortedDetail(error ?? null);
        setTxAbortedOpen(true);
      }
    },
    [readTxStatus, refreshTxStatus],
  );

  const handleAbortedRollback = useCallback(async () => {
    setTxBusy(true);
    try {
      await transaction('rollback');
      await refreshTxStatus();
      setTxAbortedOpen(false);
      setTxAbortedDetail(null);
    } catch (e) {
      setTxAbortedDetail(e instanceof Error ? e.message : String(e));
    } finally {
      setTxBusy(false);
    }
  }, [transaction, refreshTxStatus]);

  const handleAbortedSkip = useCallback(() => {
    setTxAbortedOpen(false);
    setTxAbortedDetail(null);
  }, []);

  return {
    inTransaction,
    setInTransaction,
    txBusy,
    txAbortedOpen,
    txAbortedDetail,
    refreshTxStatus,
    handleBeginTx,
    handleCommitTx,
    handleRollbackTx,
    maybeOfferAbortedDialog,
    handleAbortedRollback,
    handleAbortedSkip,
  };
}

export interface QueryTransactionModalsProps {
  txUnclosedOpen: boolean;
  txAbortedOpen: boolean;
  txAbortedDetail: string | null;
  txBusy: boolean;
  onConfirmUnclosedTx: () => void;
  onCancelUnclosedTx: () => void;
  onAbortedRollback: () => void;
  onAbortedSkip: () => void;
}

export function QueryTransactionModals({
  txUnclosedOpen,
  txAbortedOpen,
  txAbortedDetail,
  txBusy,
  onConfirmUnclosedTx,
  onCancelUnclosedTx,
  onAbortedRollback,
  onAbortedSkip,
}: QueryTransactionModalsProps) {
  const { t } = useI18n();

  return (
    <>
      <Dialog
        open={txUnclosedOpen}
        title={t('query.txUnclosedTitle')}
        description={t('query.txUnclosedBody')}
        onClose={onCancelUnclosedTx}
        footer={
          <>
            <Button variant="ghost" onClick={onCancelUnclosedTx}>
              {t('query.txUnclosedCancel')}
            </Button>
            <Button onClick={onConfirmUnclosedTx}>{t('query.txUnclosedConfirm')}</Button>
          </>
        }
      >
        <p className="text-xs text-fg-muted">{t('query.inTransaction')}</p>
      </Dialog>

      <Dialog
        open={txAbortedOpen}
        title={t('query.txAbortedTitle')}
        description={t('query.txAbortedBody')}
        onClose={onAbortedSkip}
        footer={
          <>
            <Button variant="ghost" onClick={onAbortedSkip} disabled={txBusy}>
              {t('query.txAbortedSkip')}
            </Button>
            <Button variant="danger" onClick={() => void onAbortedRollback()} disabled={txBusy}>
              {t('query.txAbortedRollback')}
            </Button>
          </>
        }
      >
        {txAbortedDetail ? (
          <pre className="copyable max-h-40 overflow-auto whitespace-pre-wrap break-words rounded border border-edge bg-surface p-2 font-mono text-[11px] text-danger">
            {txAbortedDetail}
          </pre>
        ) : null}
      </Dialog>
    </>
  );
}

export interface FavoriteNameDialogProps {
  open: boolean;
  favoriteName: string;
  favoriteDialogSql: string;
  onFavoriteNameChange: (name: string) => void;
  onClose: () => void;
  onSave: () => void;
}

export function FavoriteNameDialog({
  open,
  favoriteName,
  favoriteDialogSql,
  onFavoriteNameChange,
  onClose,
  onSave,
}: FavoriteNameDialogProps) {
  const { t } = useI18n();

  return (
    <Dialog
      open={open}
      title={t('common.addToFavorites')}
      onClose={onClose}
      className="max-w-md"
      footer={
        <>
          <Button variant="ghost" className="h-7 px-3 text-xs" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button
            variant="primary"
            className="h-7 px-3 text-xs"
            disabled={!favoriteName.trim()}
            onClick={onSave}
            data-testid="query-favorite-save"
          >
            {t('common.save')}
          </Button>
        </>
      }
    >
      <div className="mb-2">
        <label className="mb-1 block text-xs text-fg-muted">{t('query.favoriteTitle')}</label>
        <input
          type="text"
          data-testid="query-favorite-title-input"
          value={favoriteName}
          onChange={(e) => onFavoriteNameChange(e.target.value)}
          placeholder={t('query.favoriteTitlePlaceholder')}
          className="h-8 w-full rounded border border-edge bg-surface-alt px-2 text-sm text-fg focus:border-accent focus:outline-none"
          onKeyDown={(e) => {
            if (e.key === 'Enter' && favoriteName.trim()) onSave();
          }}
        />
      </div>
      <div>
        <label className="mb-1 block text-xs text-fg-muted">SQL</label>
        <div className="max-h-[120px] overflow-auto rounded border border-edge bg-surface-alt p-2 font-mono text-xs text-fg-secondary">
          {favoriteDialogSql}
        </div>
      </div>
    </Dialog>
  );
}

export interface QueryResultsPaneProps {
  publishedArtifacts?: QueryResultArtifacts;
  onCellEdit?: (row: number, column: string, value: unknown) => Promise<void>;
  dbSessionId: string;
  databaseType?: string;
  sql: string;
  running: boolean;
  error: string | null | undefined;
  results: StatementResult[];
  activeResultIdx: number;
  activeResult: StatementResult | undefined;
  resultViewMode: 'table' | 'chart';
  chartConfig: ChartConfig | undefined;
  resultDetailRowIndex: number | null | undefined;
  queryResultExportCapability: DataExportCapability;
  selectedDatabase?: string | null;
  showExplain: boolean;
  explainLoading: boolean;
  explainError: string | null;
  explainResult: ExplainResult | null;
  diagnosisVisible: boolean;
  diagnosisContext: QueryActionBuildResult<QueryDiagnosisContext>;
  onExplainError?: () => void;
  retryActionEnabled: boolean;
  addToDashboardOpen: boolean;
  onApplyAiSql: (sql: string) => void;
  onApplyFixSql: (sql: string) => void;
  onRetry: () => void;
  onSetActiveResult: (idx: number) => void;
  onTogglePinResult?: (idx: number) => void;
  onSetResultViewMode: (mode: 'table' | 'chart') => void;
  onChartConfigChange: (cfg: ChartConfig) => void;
  onRowDetail: (rowIndex: number | null) => void;
  onShowExplain: (show: boolean) => void;
  onDiagnosisVisible: (visible: boolean) => void;
  onAddToDashboardOpen: (open: boolean) => void;
  onAddToDashboardConfirm: (dashboardId: string, newName?: string) => void;
  /** S6-A: Send sanitized error context to AI Chat as a draft. */
  onAskInChat?: () => void;
}

export function QueryResultsPane({
  publishedArtifacts,
  onCellEdit,
  dbSessionId,
  databaseType,
  sql,
  running,
  error,
  results,
  activeResultIdx,
  activeResult,
  resultViewMode,
  chartConfig,
  resultDetailRowIndex,
  queryResultExportCapability,
  selectedDatabase,
  showExplain,
  explainLoading,
  explainError,
  explainResult,
  diagnosisVisible,
  diagnosisContext,
  onExplainError,
  retryActionEnabled,
  addToDashboardOpen,
  onApplyAiSql,
  onApplyFixSql,
  onRetry,
  onSetActiveResult,
  onTogglePinResult,
  onSetResultViewMode,
  onChartConfigChange,
  onRowDetail,
  onShowExplain,
  onDiagnosisVisible,
  onAddToDashboardOpen,
  onAddToDashboardConfirm,
  onAskInChat,
}: QueryResultsPaneProps) {
  const { t } = useI18n();
  const [exportOpen, setExportOpen] = useState(false);

  const exportEnabled =
    activeResult != null && activeResult.rows.length > 0 && queryResultExportCapability !== 'none';

  const exportButton = exportEnabled ? (
    <button
      type="button"
      {...tid('data-table-export')}
      className="flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-fg-secondary hover:bg-surface-raised hover:text-fg"
      onClick={() => setExportOpen(true)}
      title={t('export.export')}
    >
      <Download className="h-3 w-3" />
      {t('export.export')}
    </button>
  ) : null;

  return (
    <>
      <div className="flex min-h-0 flex-1 flex-col">
        {publishedArtifacts && <QueryArtifactExport key={publishedArtifacts.executionId} artifacts={publishedArtifacts} />}
        {showExplain && !running && (
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="flex shrink-0 items-center gap-0 border-b border-edge bg-surface-alt px-1">
              {results.length > 0 && (
                <button
                  type="button"
                  className="relative px-3 py-1.5 text-xs text-fg-muted hover:text-fg-secondary transition-colors"
                  onClick={() => onShowExplain(false)}
                >
                  {t('query.result')}
                </button>
              )}
              <button
                type="button"
                className="relative px-3 py-1.5 text-xs text-fg font-medium transition-colors"
              >
                {t('explain.title')}
                <span
                  className={cn('absolute bottom-0 left-0 right-0 h-0.5 bg-accent opacity-100')}
                />
              </button>
            </div>
            {explainLoading && (
              <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
                <Spinner size="xl" />
                {t('explain.loading')}
              </div>
            )}
            {!explainLoading && explainError && (
              <div className="p-4">
                <CopyableError
                  message={explainError}
                  copyButton
                  className="rounded-md border border-danger/20 bg-danger/10 px-4 py-3 text-sm text-danger"
                />
              </div>
            )}
            {!explainLoading && explainResult && (
              <ExplainPanel
                dbSessionId={dbSessionId}
                sql={sql}
                explainOutput={explainResult.planText}
                planJson={explainResult.planJson}
                planTree={explainResult.planTree}
                onApplySql={onApplyAiSql}
              />
            )}
          </div>
        )}

        {!showExplain && running && results.length === 0 && (
          <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
            <Spinner size="xl" />
            {t('query.executing')}
          </div>
        )}

        {!showExplain && error && !running && (
          <div className="flex-1 overflow-auto">
            <div className="p-4">
              <QueryErrorPanel
                message={error}
                onExplain={onExplainError}
                onFixSql={diagnosisContext.ok ? () => onDiagnosisVisible(true) : undefined}
                onRetry={retryActionEnabled ? () => void onRetry() : undefined}
                onAskInChat={onAskInChat}
              />
            </div>
            {diagnosisVisible && selectedDatabase && (
              <DiagnosisPanel
                diagnosisContext={diagnosisContext}
                onApplySql={onApplyFixSql}
                onClose={() => onDiagnosisVisible(false)}
              />
            )}
          </div>
        )}

        {!showExplain && !error && results.length > 0 && (
          <>
            {(results.length > 1 || explainResult) && (
              <div className="scrollbar-hide flex shrink-0 items-center gap-0 overflow-x-auto border-b border-edge bg-surface-alt px-1">
                {results.map((r, idx) => (
                  <button
                    key={idx}
                    type="button"
                    data-testid={`query-result-tab-${idx + 1}`}
                    className={cn(
                      'group relative flex items-center gap-1.5 px-3 py-1.5 text-xs transition-colors',
                      idx === activeResultIdx
                        ? 'text-fg font-medium'
                        : 'text-fg-muted hover:text-fg-secondary',
                    )}
                    onClick={() => onSetActiveResult(idx)}
                  >
                    <span>
                      {t('query.result')} {idx + 1}
                    </span>
                    <span className="text-[10px] text-fg-muted">
                      ({r.rows.length} {t('common.rows')}
                      {running ? '' : `, ${r.executionTimeMs}ms`})
                    </span>
                    {onTogglePinResult && (
                      <span
                        role="button"
                        tabIndex={0}
                        className={cn(
                          'ml-1 rounded p-0.5 transition hover:bg-surface-raised',
                          r.pinned
                            ? 'text-accent opacity-100'
                            : 'text-fg-muted opacity-0 group-hover:opacity-100',
                        )}
                        title={r.pinned ? t('query.unpinTab') : t('query.pinTab')}
                        onClick={(e) => {
                          e.stopPropagation();
                          onTogglePinResult(idx);
                        }}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter' || e.key === ' ') {
                            e.stopPropagation();
                            onTogglePinResult(idx);
                          }
                        }}
                      >
                        <Pin className={cn('h-3 w-3', r.pinned && 'fill-accent')} />
                      </span>
                    )}
                    <span
                      className={cn(
                        'absolute bottom-0 left-0 right-0 h-0.5 bg-accent transition-opacity duration-300',
                        idx === activeResultIdx ? 'opacity-100' : 'opacity-0',
                      )}
                    />
                  </button>
                ))}
                {explainResult && (
                  <button
                    type="button"
                    className="relative px-3 py-1.5 text-xs text-fg-muted hover:text-fg-secondary transition-colors"
                    onClick={() => onShowExplain(true)}
                  >
                    {t('explain.title')}
                  </button>
                )}
              </div>
            )}

            {activeResult && (
              <>
                {running && (
                  <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-muted">
                    <Spinner size="sm" />
                    {t('query.streamingRows', { n: String(activeResult.rows.length) })}
                  </div>
                )}
                <ResultWorkspace
                  result={activeResult}
                  onCellEdit={running ? undefined : onCellEdit}
                  view={running ? 'table' : resultViewMode}
                  chartConfig={chartConfig}
                  rowDetailIndex={resultDetailRowIndex}
                  dataExportCapability={queryResultExportCapability}
                  headerActions={
                    <>
                      {resultViewMode === 'chart' && chartConfig && (
                        <button
                          type="button"
                          className="flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-fg-secondary hover:bg-surface-raised hover:text-fg"
                          data-testid="query-add-to-dashboard"
                          disabled={!activeResult.rows.length}
                          onClick={() => onAddToDashboardOpen(true)}
                          title={t('query.addToDashboard')}
                        >
                          <Gauge className="h-3 w-3" />
                          {t('query.addToDashboard')}
                        </button>
                      )}
                      {resultViewMode !== 'chart' && exportButton}
                    </>
                  }
                  onViewChange={onSetResultViewMode}
                  onChartConfigChange={onChartConfigChange}
                  onRowDetail={onRowDetail}
                />
              </>
            )}
          </>
        )}
      </div>

      <AddToDashboardDialog
        open={addToDashboardOpen}
        onClose={() => onAddToDashboardOpen(false)}
        onConfirm={onAddToDashboardConfirm}
      />

      {activeResult && (
        <DataExportDialog
          open={exportOpen}
          onClose={() => setExportOpen(false)}
          columns={activeResult.columns.map((c) => ({
            id: c.name,
            name: c.name,
            type: c.dataType,
          }))}
          rows={activeResult.rows}
          tableName="query_result"
          databaseType={databaseType}
          dbSessionId={dbSessionId}
          totalRows={activeResult.rows.length}
          dataExportCapability={queryResultExportCapability}
        />
      )}
    </>
  );
}
