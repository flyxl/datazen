import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Columns3, Filter, RefreshCw, ShieldAlert } from 'lucide-react';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { NlFilterInput } from '../../components/ai/NlFilterInput';
import { TableColumnFilter } from './TableColumnFilter';
import { useTableDataStore } from '../../stores/tableDataStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { cn } from '../../lib/cn';
import { CopyableError } from '../../components/ui/CopyableError';
import { ErrorBanner } from '../../components/ui/ErrorBanner';
import { tableChangeContextKey } from '../../lib/tableChanges';
import type { RowChangePlan, TableChangeContext } from '../../lib/tableChanges';
import {
  filterExpressionToConditions,
  parseFilterForApply,
  type FilterExpression,
} from '../../lib/filterExpression';
import { extractErrorMessage } from '../../stores/tableData/connectionState';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { queryCommands } from '../../commands/query';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { DatabaseType, FilterCondition, SortCondition } from '../../types';
import { Spinner } from '../../components/ui/Spinner';

interface TableViewProps {
  /** Table/view panel this grid belongs to; its data slice is keyed by this id. */
  panelId: string;
  dbSessionId: string;
  database: string;
  tableName: string;
  connectionId?: string;
  schema?: string | null;
  databaseType?: string;
  /** Explicit read-only override; if omitted, checked via connection and driver metadata. */
  readOnly?: boolean;
  /** Data-export capability, threaded to the table-data export dialog. */
  dataExportCapability?: 'none' | 'loaded_only' | 'full_table';
}

export function TableView({
  panelId,
  dbSessionId,
  database,
  tableName,
  connectionId,
  schema = null,
  databaseType,
  readOnly: readOnlyProp,
  dataExportCapability,
}: TableViewProps) {
  const { t } = useI18n();
  const savedConnection = useConnectionStore((s) =>
    connectionId ? s.connections.find((c) => c.id === connectionId) : undefined,
  );
  /**
   * Drivers that declare `readOnly` in `DB_REGISTRY` (e.g. Kiwi / Superset)
   * never support in-place cell editing. Gate here so a read-only driver or connection
   * can never enter edit mode.
   */
  const driverReadOnly = databaseType
    ? DB_REGISTRY[databaseType as DatabaseType]?.readOnly === true
    : false;
  const isConnectionReadOnly = Boolean(
    readOnlyProp ?? (savedConnection?.readOnly || driverReadOnly),
  );
  const isEditable = !isConnectionReadOnly;

  // NlFilterInput handles unconfigured state internally
  const ts = useTableDataStore((s) => s.byPanel.get(panelId));
  /**
   * F1: the panel pins its own target database, so a cross-database table loads
   * correctly even when the session's active database differs.
   */
  const context = useMemo<TableChangeContext>(
    () => ({
      connectionId: connectionId ?? null,
      dbSessionId,
      driverType: databaseType ?? null,
      database: database || null,
      schema,
      table: tableName,
    }),
    [connectionId, database, databaseType, dbSessionId, schema, tableName],
  );
  /** Panel-scoped action binders: every store call is namespaced by this tab's id. */
  const actions = useMemo(() => {
    const store = () => useTableDataStore.getState();
    return {
      load: () => void store().loadTableData({ panelId, ...context }),
      reload: () => store().reloadPanel(panelId),
      setSort: (sort: SortCondition) => store().setSort(panelId, sort),
      removeFilter: (index: number) => store().removeFilter(panelId, index),
      clearFilters: () => store().clearFilters(panelId),
      addFilter: (filter: FilterCondition) => store().addFilter(panelId, filter),
      setFilters: (filters: FilterCondition[], logic?: 'and' | 'or') =>
        store().setFilters(panelId, filters, logic),
      updateFilter: (index: number, filter: FilterCondition) =>
        store().updateFilter(panelId, index, filter),
      setFilterLogic: (logic: 'and' | 'or') => store().setFilterLogic(panelId, logic),
      applyFilters: () => store().applyFilters(panelId),
      setFilterPanelOpen: (open: boolean) => store().setFilterPanelOpen(panelId, open),
      setVisibleColumns: (columns: string[] | null) => store().setVisibleColumns(panelId, columns),
      setPage: (page: number) => store().setPage(panelId, page),
      setPageSize: (size: number) => store().setPageSize(panelId, size),
      startEdit: (row: number, col: string) => store().startEdit(panelId, row, col),
      stageCellChange: (row: number, col: string, value: unknown) =>
        store().stageCellChange(panelId, row, col, value),
      cancelEdit: () => store().cancelEdit(panelId),
      selectRow: (index: number, opts?: { multi?: boolean; range?: boolean }) =>
        store().selectRow(panelId, index, opts),
      toggleSelectAll: () => store().toggleSelectAll(panelId),
      deleteRows: (rowIndices: number[]) => store().deleteRows(panelId, rowIndices),
      previewPendingChanges: () => store().previewPendingChanges(panelId),
      commitPendingChanges: () => store().commitPendingChanges(panelId),
      rollbackPendingChanges: () => store().rollbackPendingChanges(panelId),
      setDetailRow: (index: number | null) => store().setDetailRow(panelId, index),
    };
  }, [context, panelId]);

  const confirmOnDelete = useSettingsStore((s) => s.settings.confirmOnDelete);
  const [confirmDelete, confirmDeleteDialog] = useConfirmDialog();
  const [confirmCommit, confirmCommitDialog] = useConfirmDialog();
  const [previewOpen, setPreviewOpen] = useState(false);
  const [quickFilter, setQuickFilter] = useState('');
  const [quickFilterError, setQuickFilterError] = useState<string | null>(null);
  const [readOnlyTipVisible, setReadOnlyTipVisible] = useState(false);
  const readOnlyTipTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Manual transaction toggle: when on, ChangeSet commits join this open
  // transaction; otherwise each commit runs as its own short transaction.
  const [inTx, setInTx] = useState(false);
  const [txBusy, setTxBusy] = useState(false);
  const [txError, setTxError] = useState<string | null>(null);
  const refreshTxStatus = useCallback(async () => {
    try {
      setInTx(await queryCommands.sessionTransactionStatus(dbSessionId));
    } catch {
      setInTx(false);
    }
  }, [dbSessionId]);
  useEffect(() => {
    void refreshTxStatus();
  }, [refreshTxStatus]);
  const handleBeginTx = useCallback(async () => {
    setTxBusy(true);
    setTxError(null);
    try {
      await queryCommands.beginSessionTransaction(dbSessionId);
      await refreshTxStatus();
    } catch (e) {
      setTxError(extractErrorMessage(e, t('tableData.txBeginFailed')));
    } finally {
      setTxBusy(false);
    }
  }, [dbSessionId, refreshTxStatus, t]);
  const handleCommitTx = useCallback(async () => {
    setTxBusy(true);
    setTxError(null);
    try {
      await queryCommands.commitSessionTransaction(dbSessionId);
      await refreshTxStatus();
    } catch (e) {
      setTxError(extractErrorMessage(e, t('tableData.txCommitFailed')));
    } finally {
      setTxBusy(false);
    }
  }, [dbSessionId, refreshTxStatus, t]);
  const handleRollbackTx = useCallback(async () => {
    setTxBusy(true);
    setTxError(null);
    try {
      await queryCommands.rollbackSessionTransaction(dbSessionId);
      await refreshTxStatus();
      actions.reload();
    } catch (e) {
      setTxError(extractErrorMessage(e, t('tableData.txRollbackFailed')));
    } finally {
      setTxBusy(false);
    }
  }, [actions, dbSessionId, refreshTxStatus, t]);

  useEffect(() => {
    return () => {
      if (readOnlyTipTimer.current !== null) clearTimeout(readOnlyTipTimer.current);
    };
  }, []);

  const showReadOnlyTip = useCallback(() => {
    setReadOnlyTipVisible(true);
    if (readOnlyTipTimer.current !== null) clearTimeout(readOnlyTipTimer.current);
    readOnlyTipTimer.current = setTimeout(() => {
      setReadOnlyTipVisible(false);
      readOnlyTipTimer.current = null;
    }, 3000);
  }, []);

  const handleCellDoubleClick = useCallback(
    (row: number, col: string) => {
      if (!isEditable) {
        showReadOnlyTip();
        return;
      }
      actions.startEdit(row, col);
    },
    [actions, isEditable, showReadOnlyTip],
  );

  const handleCellEdit = useCallback(
    (row: number, col: string, value: unknown) => {
      if (!isEditable) {
        showReadOnlyTip();
        actions.cancelEdit();
        return;
      }
      actions.stageCellChange(row, col, value);
    },
    [actions, isEditable, showReadOnlyTip],
  );

  const handleDeleteRows = useCallback(
    async (rowIndices: number[]) => {
      if (rowIndices.length === 0 || !isEditable) return;
      if (confirmOnDelete) {
        const confirmed = await confirmDelete({
          title: t('dataTable.deleteRow'),
          message: t('dataTable.confirmDeleteRows', { count: rowIndices.length }),
          kind: 'warning',
        });
        if (!confirmed) return;
      }
      actions.deleteRows(rowIndices);
    },
    [actions, confirmOnDelete, confirmDelete, isEditable, t],
  );

  // Re-fetch the current page from the database, keeping page/filters/sorts
  // (they live in the panel slice and the load re-reads them).
  const handleRefresh = useCallback(() => {
    actions.reload();
  }, [actions]);

  const contextKey = tableChangeContextKey(context);
  const hasData = ts != null && ts.columns.length > 0;
  const sliceContextKey = ts?.context ? tableChangeContextKey(ts.context) : null;
  // Nothing fetched yet, the panel was invalidated (rows dropped by a write from
  // elsewhere), or this tab now points at a different table/database.
  const needsLoad = !hasData || sliceContextKey !== contextKey;
  const requestRevision = ts?.requestRevision ?? 0;

  useEffect(() => {
    if (!needsLoad) return;
    actions.load();
  }, [actions, needsLoad, requestRevision]);

  const columns = ts?.columns ?? [];
  const rows = ts?.rows ?? [];
  const totalRows = ts?.totalRows ?? 0;
  const page = ts?.page ?? 0;
  const pageSize = ts?.pageSize ?? 50;
  const sorts = ts?.sorts ?? [];
  const filters = ts?.filters ?? [];
  const filterLogic = ts?.filterLogic ?? 'and';
  const draftFilters = ts?.draftFilters ?? [];
  const draftFilterLogic = ts?.draftFilterLogic ?? 'and';
  const filterPanelOpen = ts?.filterPanelOpen ?? false;
  const visibleColumns = ts?.visibleColumns ?? null;

  const displayedColumns = useMemo(() => {
    if (visibleColumns === null) return columns;
    const set = new Set(visibleColumns);
    return columns.filter((c) => set.has(c.name));
  }, [columns, visibleColumns]);

  const columnDefs: ColumnDef[] = useMemo(
    () =>
      displayedColumns.map((c) => ({
        id: c.name,
        name: c.name,
        type: c.dataType,
      })),
    [displayedColumns],
  );

  const rowArrays: unknown[][] = useMemo(
    () => rows.map((record) => displayedColumns.map((col) => record[col.name] ?? null)),
    [rows, displayedColumns],
  );
  const editingCell = ts?.editingCell ?? null;
  const detailRowIndex = ts?.detailRowIndex ?? null;
  const selectedRows = ts?.selectedRows ?? new Set<number>();
  const loading = ts?.loading ?? false;
  const error = ts?.error ?? null;
  const pendingChanges = ts?.pendingChanges ?? new Map();
  const previewPlan = ts?.previewPlan ?? null;
  const pendingStatus = ts?.pendingStatus ?? 'idle';
  const pendingUpdateCount = [...pendingChanges.values()].filter(
    (change) => !change.deleteMarked && change.changedColumns.length > 0,
  ).length;
  const pendingDeleteCount = [...pendingChanges.values()].filter(
    (change) => change.deleteMarked,
  ).length;
  const pendingBusy = loading || pendingStatus !== 'idle';

  useEffect(() => {
    if (!isEditable && editingCell) actions.cancelEdit();
  }, [actions, editingCell, isEditable]);

  const handlePreviewPendingChanges = useCallback(async () => {
    if (driverReadOnly || pendingBusy || pendingChanges.size === 0) return;
    const plan = await actions.previewPendingChanges();
    if (plan) setPreviewOpen(true);
  }, [actions, driverReadOnly, pendingBusy, pendingChanges.size]);

  const handleCommitPendingChanges = useCallback(async () => {
    if (driverReadOnly || pendingBusy || pendingChanges.size === 0) return;
    const confirmed = await confirmCommit({
      title: t('tableData.commit'),
      message: t('tableData.confirmCommit', {
        updates: pendingUpdateCount,
        deletes: pendingDeleteCount,
      }),
      confirmLabel: t('tableData.commit'),
      kind: 'warning',
    });
    if (confirmed) await actions.commitPendingChanges();
  }, [
    actions,
    confirmCommit,
    driverReadOnly,
    pendingBusy,
    pendingChanges.size,
    pendingDeleteCount,
    pendingUpdateCount,
    t,
  ]);

  const handleTableKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
        if (pendingChanges.size === 0 || pendingBusy) return;
        event.preventDefault();
        void handleCommitPendingChanges();
      }
    },
    [handleCommitPendingChanges, pendingBusy, pendingChanges.size],
  );

  const handleQuickFilter = useCallback(() => {
    const input = quickFilter.trim();
    if (!input) {
      setQuickFilterError(null);
      actions.clearFilters();
      return;
    }
    const parsed = parseFilterForApply(input, columns);
    if (!parsed.ok) {
      setQuickFilterError(t('filter.invalidExpression', { error: parsed.error }));
      return;
    }
    const logic = new Set<'and' | 'or'>();
    const collectLogic = (expression: FilterExpression) => {
      if (expression.type === 'logical') {
        logic.add(expression.operator);
        collectLogic(expression.left);
        collectLogic(expression.right);
      }
    };
    collectLogic(parsed.value.expression);
    if (logic.size > 1) {
      setQuickFilterError(t('filter.mixedLogic'));
      return;
    }
    setQuickFilterError(null);
    actions.setFilters(
      filterExpressionToConditions(parsed.value.expression),
      [...logic][0] ?? 'and',
    );
  }, [actions, columns, quickFilter, t]);

  const openManualFilter = () => {
    if (filterPanelOpen) {
      actions.setFilterPanelOpen(false);
      return;
    }
    actions.setFilterPanelOpen(true);
    if (draftFilters.length === 0) {
      actions.addFilter({
        column: columns[0]?.name ?? '',
        operator: 'eq',
        value: '',
      });
    }
  };

  if (loading && columns.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
        <Spinner size="xl" />
        {t('tableView.loadingData')}
      </div>
    );
  }

  // Keep the table chrome (filters etc.) visible after a failed reload so the
  // user can clear/fix the bad filter instead of being stuck on a blank error page.
  if (error && columns.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <div className="max-w-lg text-center">
          <CopyableError message={error} copyButton className="text-sm text-red-400" />
          <button
            type="button"
            className="mt-2 text-xs text-accent hover:underline"
            onClick={actions.load}
          >
            {t('common.retry')}
          </button>
        </div>
      </div>
    );
  }

  const filterActive = filterPanelOpen || filters.length > 0 || draftFilters.length > 0;

  return (
    <div className="flex flex-1 flex-col overflow-hidden" onKeyDown={handleTableKeyDown}>
      {error && (
        <div className="flex items-start gap-3 border-b border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-400">
          <CopyableError message={error} copyButton className="min-w-0 flex-1" />
          <button
            type="button"
            className="shrink-0 text-xs text-accent hover:underline"
            onClick={actions.load}
          >
            {t('common.retry')}
          </button>
        </div>
      )}
      {error && pendingChanges.size > 0 && (
        <div className="border-b border-warning/30 bg-warning/10 px-3 py-1.5 text-xs text-warning">
          {t('tableData.errorPendingHint')}
        </div>
      )}
      <div className="flex shrink-0 items-start gap-0.5 border-b border-edge px-2 py-0.5">
        <button
          type="button"
          onMouseDown={(e) => e.preventDefault()}
          className="mt-0 flex h-7 w-7 shrink-0 items-center justify-center rounded text-xs text-fg-muted transition-colors hover:bg-surface-alt hover:text-fg disabled:opacity-50"
          onClick={handleRefresh}
          disabled={loading}
          title={t('connWin.refresh')}
          aria-label={t('connWin.refresh')}
          data-testid="table-data-refresh"
        >
          <RefreshCw className={cn('h-3.5 w-3.5', loading && 'animate-spin')} />
        </button>
        <button
          type="button"
          onMouseDown={(e) => e.preventDefault()}
          className={cn(
            'mt-0 flex h-7 w-7 shrink-0 items-center justify-center rounded text-xs transition-colors hover:bg-surface-alt',
            filterActive ? 'text-accent' : 'text-fg-muted hover:text-fg',
          )}
          onClick={openManualFilter}
          disabled={loading}
          title={t('filter.filter')}
          aria-label={t('filter.filter')}
          aria-pressed={filterPanelOpen}
          data-testid="table-filter-toggle"
        >
          <Filter className="h-3.5 w-3.5" />
        </button>
        <NlFilterInput
          panelId={panelId}
          dbSessionId={dbSessionId}
          database={database}
          tableName={tableName}
        />
        <div className="ml-1 flex min-w-0 flex-1 items-center">
          <input
            type="text"
            value={quickFilter}
            onChange={(event) => {
              setQuickFilter(event.target.value);
              if (quickFilterError) setQuickFilterError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
                event.preventDefault();
                handleQuickFilter();
              }
            }}
            onBlur={() => {
              if (quickFilter.trim()) handleQuickFilter();
            }}
            disabled={loading || pendingBusy}
            placeholder={t('filter.quickPlaceholder')}
            className="h-7 min-w-0 flex-1 rounded border border-edge bg-surface px-2 text-xs text-fg placeholder:text-fg-muted focus:border-accent focus:outline-none"
            aria-invalid={quickFilterError ? 'true' : undefined}
            data-testid="table-quick-filter"
          />
        </div>
      </div>
      {quickFilterError && (
        <ErrorBanner variant="strip" className="border-danger/30 py-1">
          {quickFilterError}
        </ErrorBanner>
      )}
      {readOnlyTipVisible && (
        <div
          className="flex shrink-0 items-center gap-1.5 border-b border-warning/30 bg-warning/10 px-3 py-1.5 text-xs text-warning"
          role="status"
          aria-live="polite"
          data-testid="table-read-only-tip"
        >
          <ShieldAlert className="h-3.5 w-3.5 shrink-0" />
          {t('tableData.readOnlyEditDisabled')}
        </div>
      )}
      {txError && (
        <div className="border-b border-red-500/30 bg-red-500/10 px-3 py-1 text-xs text-red-400">
          {txError}
        </div>
      )}
      {isEditable && (
        <div
          className="flex shrink-0 items-center gap-2 border-b border-edge px-3 py-1 text-xs text-fg-muted"
          data-testid="table-tx-controls"
        >
          <span className={cn(inTx ? 'text-accent' : 'text-fg-muted')}>
            {inTx ? t('tableData.inManualTx') : t('tableData.shortTxDefault')}
          </span>
          {!inTx ? (
            <button
              type="button"
              className="text-accent hover:underline disabled:opacity-50"
              onClick={() => void handleBeginTx()}
              disabled={txBusy || loading}
              data-testid="table-tx-begin"
            >
              {t('tableData.beginTx')}
            </button>
          ) : (
            <>
              <button
                type="button"
                className="text-accent hover:underline disabled:opacity-50"
                onClick={() => void handleCommitTx()}
                disabled={txBusy || loading}
                data-testid="table-tx-commit"
              >
                {t('tableData.commitTx')}
              </button>
              <button
                type="button"
                className="text-accent hover:underline disabled:opacity-50"
                onClick={() => void handleRollbackTx()}
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
      {pendingChanges.size > 0 && (
        <div
          className="flex shrink-0 items-center gap-2 border-b border-amber-500/30 bg-amber-500/10 px-3 py-1.5 text-xs"
          aria-busy={pendingBusy}
          data-testid="pending-changes-bar"
        >
          <span className="font-medium text-amber-300">
            {t('tableData.pendingChanges', { count: pendingChanges.size })}
          </span>
          {pendingUpdateCount > 0 && (
            <span className="text-fg-muted">
              {t('tableData.pendingUpdates', { count: pendingUpdateCount })}
            </span>
          )}
          {pendingDeleteCount > 0 && (
            <span className="text-fg-muted">
              {t('tableData.pendingDeletes', { count: pendingDeleteCount })}
            </span>
          )}
          <div className="ml-auto flex items-center gap-1">
            <Button
              size="sm"
              variant="ghost"
              disabled={pendingBusy}
              onClick={() => void handlePreviewPendingChanges()}
              data-testid="pending-preview"
            >
              {t('tableData.preview')}
            </Button>
            <Button
              size="sm"
              variant="primary"
              disabled={pendingBusy}
              onClick={() => void handleCommitPendingChanges()}
              data-testid="pending-commit"
            >
              {t('tableData.commit')}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              disabled={pendingBusy}
              onClick={() => actions.rollbackPendingChanges()}
              data-testid="pending-rollback"
            >
              {t('tableData.rollback')}
            </Button>
          </div>
        </div>
      )}
      <DataTable
        columns={columnDefs}
        rows={rowArrays}
        totalRows={totalRows}
        page={page}
        pageSize={pageSize}
        sorts={sorts}
        filters={filters}
        filterLogic={filterLogic}
        draftFilters={draftFilters}
        draftFilterLogic={draftFilterLogic}
        filterPanelOpen={filterPanelOpen}
        onFilterPanelOpenChange={actions.setFilterPanelOpen}
        onAddFilter={actions.addFilter}
        onUpdateFilter={actions.updateFilter}
        onFilterLogicChange={actions.setFilterLogic}
        onApplyFilters={actions.applyFilters}
        editingCell={!isEditable ? null : editingCell}
        selectedRows={selectedRows}
        loading={loading}
        onSort={actions.setSort}
        onRemoveFilter={actions.removeFilter}
        onClearFilters={actions.clearFilters}
        onPageChange={actions.setPage}
        onPageSizeChange={actions.setPageSize}
        onCellDoubleClick={handleCellDoubleClick}
        onCellEdit={isEditable ? handleCellEdit : undefined}
        onCellEditCancel={actions.cancelEdit}
        enableSetNull={isEditable}
        onRowSelect={actions.selectRow}
        onSelectAll={actions.toggleSelectAll}
        onRowClick={actions.setDetailRow}
        highlightedRow={detailRowIndex}
        exportTableName={tableName}
        databaseType={databaseType}
        dbSessionId={dbSessionId}
        dataExportCapability={dataExportCapability}
        primaryKeyColumns={columns.filter((c) => c.isPrimaryKey).map((c) => c.name)}
        onDeleteRows={isEditable ? handleDeleteRows : undefined}
        headerActions={
          <TableColumnFilter
            columns={columns}
            visibleColumns={visibleColumns}
            onChange={actions.setVisibleColumns}
            disabled={loading || pendingBusy || columns.length === 0}
          />
        }
        emptyPlaceholder={
          columns.length > 0 && displayedColumns.length === 0 ? (
            <div
              data-testid="all-columns-hidden-state"
              className="flex min-h-48 flex-1 flex-col items-center justify-center p-8 text-center text-fg-muted"
            >
              <Columns3 className="mb-2 h-8 w-8 opacity-40" />
              <p className="text-sm font-medium">{t('tableData.allColumnsHidden')}</p>
              <Button
                size="sm"
                variant="ghost"
                className="mt-2 text-xs"
                onClick={() => actions.setVisibleColumns(null)}
              >
                {t('tableData.resetColumns')}
              </Button>
            </div>
          ) : undefined
        }
      />
      {confirmDeleteDialog}
      {confirmCommitDialog}
      <PendingPlanDialog
        plan={previewPlan}
        open={previewOpen && previewPlan != null}
        onClose={() => setPreviewOpen(false)}
      />
    </div>
  );
}

function PendingPlanDialog({
  plan,
  open,
  onClose,
}: {
  plan: RowChangePlan | null;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useI18n();
  if (!plan) return null;
  const statements = [...plan.updates, ...plan.deletes];
  return (
    <Dialog
      open={open}
      title={t('tableData.previewTitle')}
      description={`${plan.table.table} · ${plan.updates.length} ${t('tableData.pendingUpdates', { count: plan.updates.length })} · ${plan.deletes.length} ${t('tableData.pendingDeletes', { count: plan.deletes.length })}`}
      onClose={onClose}
      testId="pending-plan-dialog"
      footer={
        <Button size="sm" variant="secondary" onClick={onClose}>
          {t('common.close')}
        </Button>
      }
    >
      <div className="space-y-3 text-xs">
        <div>
          <div className="font-medium text-fg">{t('tableData.previewFingerprint')}</div>
          <code className="mt-1 block break-all rounded bg-surface px-2 py-1 font-mono text-fg-muted">
            {plan.fingerprint}
          </code>
        </div>
        <div>
          <div className="font-medium text-fg">{t('tableData.previewSql')}</div>
          <div className="mt-1 space-y-2">
            {statements.length === 0 ? (
              <div className="text-fg-muted">{t('tableData.noPendingChanges')}</div>
            ) : (
              statements.map((statement, index) => (
                <div
                  key={`${statement.sqlTemplate}-${index}`}
                  className="rounded border border-edge bg-surface p-2"
                >
                  <code className="block whitespace-pre-wrap break-words font-mono text-fg-secondary">
                    {statement.sqlTemplate}
                  </code>
                  {statement.parameterSummary.length > 0 && (
                    <div className="mt-1 text-fg-muted">
                      {t('tableData.previewParameters')}: {statement.parameterSummary.join(', ')}
                    </div>
                  )}
                </div>
              ))
            )}
          </div>
        </div>
        {plan.warnings.length > 0 && (
          <div className="rounded border border-amber-500/30 bg-amber-500/10 p-2 text-amber-200">
            <div className="font-medium">{t('tableData.previewWarnings')}</div>
            <ul className="mt-1 list-disc pl-4">
              {plan.warnings.map((warning) => (
                <li key={`${warning.code}-${warning.message}`}>{warning.message}</li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </Dialog>
  );
}
