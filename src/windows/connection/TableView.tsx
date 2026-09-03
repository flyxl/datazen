import { useCallback, useEffect, useRef, useState } from 'react';
import { Filter, Loader2, ShieldAlert } from 'lucide-react';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { NlFilterInput } from '../../components/ai/NlFilterInput';
import { useTableDataStore, type TableState } from '../../stores/tableDataStore';
import { useSettingsStore } from '../../stores/settingsStore';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { cn } from '../../lib/cn';
import { CopyableError } from '../../components/ui/CopyableError';
import { tableChangeContextKey } from '../../lib/tableChanges';
import type { RowChangePlan } from '../../lib/tableChanges';
import {
  filterExpressionToConditions,
  parseFilterForApply,
  type FilterExpression,
} from '../../lib/filterExpression';
import { Button } from '../../components/ui/Button';
import { Dialog } from '../../components/ui/Dialog';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import type { DatabaseType } from '../../types';

interface TableViewProps {
  dbSessionId: string;
  database: string;
  tableName: string;
  connectionId?: string;
  schema?: string | null;
  databaseType?: string;
  /** Data-export capability, threaded to the table-data export dialog. */
  dataExportCapability?: 'none' | 'loaded_only' | 'full_table';
}

export function TableView({
  dbSessionId,
  database,
  tableName,
  connectionId,
  schema = null,
  databaseType,
  dataExportCapability,
}: TableViewProps) {
  const { t } = useI18n();
  // NlFilterInput handles unconfigured state internally
  const tableStates = useTableDataStore((s) => s.tableStates);
  const activeTable = useTableDataStore((s) => s.activeTable);
  const activeTableKey = useTableDataStore((s) => s.activeTableKey);
  const loadTableData = useTableDataStore((s) => s.loadTableData);
  const switchToTable = useTableDataStore((s) => s.switchToTable);
  const setSort = useTableDataStore((s) => s.setSort);
  const removeFilter = useTableDataStore((s) => s.removeFilter);
  const clearFilters = useTableDataStore((s) => s.clearFilters);
  const addFilter = useTableDataStore((s) => s.addFilter);
  const updateFilter = useTableDataStore((s) => s.updateFilter);
  const setFilterLogic = useTableDataStore((s) => s.setFilterLogic);
  const applyFilters = useTableDataStore((s) => s.applyFilters);
  const setFilterPanelOpen = useTableDataStore((s) => s.setFilterPanelOpen);
  const setPage = useTableDataStore((s) => s.setPage);
  const setPageSize = useTableDataStore((s) => s.setPageSize);
  const startEdit = useTableDataStore((s) => s.startEdit);
  const updateCell = useTableDataStore((s) => s.updateCell);
  const cancelEdit = useTableDataStore((s) => s.cancelEdit);
  const selectRow = useTableDataStore((s) => s.selectRow);
  const toggleSelectAll = useTableDataStore((s) => s.toggleSelectAll);
  const deleteRows = useTableDataStore((s) => s.deleteRows);
  const previewPendingChanges = useTableDataStore((s) => s.previewPendingChanges);
  const commitPendingChanges = useTableDataStore((s) => s.commitPendingChanges);
  const rollbackPendingChanges = useTableDataStore((s) => s.rollbackPendingChanges);
  const setDetailRow = useTableDataStore((s) => s.setDetailRow);
  const detailRowIndex = useTableDataStore((s) => s.detailRowIndex);
  const confirmOnDelete = useSettingsStore((s) => s.settings.confirmOnDelete);
  const safeMode = useSettingsStore((s) => s.settings.safeMode);
  /**
   * Drivers that declare `readOnly` in `DB_REGISTRY` (e.g. Kiwi / Superset)
   * never support in-place cell editing — regardless of Safe Mode. Gate here
   * so a read-only driver can never enter edit mode.
   */
  const driverReadOnly = databaseType
    ? DB_REGISTRY[databaseType as DatabaseType]?.readOnly === true
    : false;
  const [confirmDelete, confirmDeleteDialog] = useConfirmDialog();
  const [confirmCommit, confirmCommitDialog] = useConfirmDialog();
  const [previewOpen, setPreviewOpen] = useState(false);
  const [quickFilter, setQuickFilter] = useState('');
  const [quickFilterError, setQuickFilterError] = useState<string | null>(null);
  const [safeModeTipVisible, setSafeModeTipVisible] = useState(false);
  const safeModeTipTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (safeModeTipTimer.current !== null) clearTimeout(safeModeTipTimer.current);
    };
  }, []);

  const showSafeModeTip = useCallback(() => {
    setSafeModeTipVisible(true);
    if (safeModeTipTimer.current !== null) clearTimeout(safeModeTipTimer.current);
    safeModeTipTimer.current = setTimeout(() => {
      setSafeModeTipVisible(false);
      safeModeTipTimer.current = null;
    }, 3000);
  }, []);

  const handleCellDoubleClick = useCallback(
    (row: number, col: string) => {
      // Read-only drivers never edit, regardless of Safe Mode.
      if (driverReadOnly) return;
      if (safeMode) {
        showSafeModeTip();
        return;
      }
      startEdit(row, col);
    },
    [driverReadOnly, safeMode, showSafeModeTip, startEdit],
  );

  const handleCellEdit = useCallback(
    (row: number, col: string, value: unknown) => {
      if (driverReadOnly) {
        cancelEdit();
        return;
      }
      if (safeMode) {
        showSafeModeTip();
        cancelEdit();
        return;
      }
      updateCell(row, col, value);
    },
    [cancelEdit, driverReadOnly, safeMode, showSafeModeTip, updateCell],
  );

  const handleDeleteRows = useCallback(
    async (rowIndices: number[]) => {
      if (rowIndices.length === 0) return;
      if (driverReadOnly) return;
      if (confirmOnDelete) {
        const confirmed = await confirmDelete({
          title: t('dataTable.deleteRow'),
          message: t('dataTable.confirmDeleteRows', { count: rowIndices.length }),
          kind: 'warning',
        });
        if (!confirmed) return;
      }
      await deleteRows(rowIndices);
    },
    [confirmOnDelete, confirmDelete, deleteRows, driverReadOnly, t],
  );

  const tableContext = {
    connectionId: connectionId ?? null,
    dbSessionId,
    driverType: databaseType ?? null,
    database: database || null,
    schema,
    table: tableName,
  } as const;
  const tableKey = tableChangeContextKey(tableContext);
  // The fallback keeps isolated component tests and older embedders readable;
  // the real store always keys table state by the complete context key.
  const ts: TableState | undefined = tableStates.get(tableKey) ?? tableStates.get(tableName);
  const hasData = ts != null && ts.columns.length > 0;

  useEffect(() => {
    if (hasData && (activeTable !== tableName || activeTableKey !== tableKey)) {
      switchToTable(tableName, {
        connectionId: connectionId ?? null,
        driverType: databaseType ?? null,
        database: database || null,
        schema,
      });
    } else if (!hasData) {
      // F1: carry the panel's target database so cross-database tables load
      // correctly even when the session's active database differs.
      void loadTableData({
        dbSessionId,
        table: tableName,
        connectionId: connectionId ?? null,
        driverType: databaseType ?? null,
        database: database || null,
        schema,
      });
    }
  }, [
    dbSessionId,
    tableName,
    hasData,
    activeTable,
    activeTableKey,
    tableKey,
    connectionId,
    databaseType,
    database,
    schema,
    loadTableData,
    switchToTable,
  ]);

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
  const editingCell = ts?.editingCell ?? null;
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
    if ((safeMode || driverReadOnly) && editingCell) cancelEdit();
  }, [cancelEdit, driverReadOnly, editingCell, safeMode]);

  const handlePreviewPendingChanges = useCallback(async () => {
    if (driverReadOnly || pendingBusy || pendingChanges.size === 0) return;
    const plan = await previewPendingChanges();
    if (plan) setPreviewOpen(true);
  }, [driverReadOnly, pendingBusy, pendingChanges.size, previewPendingChanges]);

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
    if (confirmed) await commitPendingChanges();
  }, [
    commitPendingChanges,
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
      clearFilters();
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
    useTableDataStore
      .getState()
      .setFilters(filterExpressionToConditions(parsed.value.expression), [...logic][0] ?? 'and');
  }, [clearFilters, columns, quickFilter, t]);

  const openManualFilter = () => {
    if (filterPanelOpen) {
      setFilterPanelOpen(false);
      return;
    }
    setFilterPanelOpen(true);
    if (draftFilters.length === 0) {
      addFilter({
        column: columns[0]?.name ?? '',
        operator: 'eq',
        value: '',
      });
    }
  };

  if (loading && columns.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
        <Loader2 className="h-5 w-5 animate-spin" />
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
            onClick={() =>
              void loadTableData({
                dbSessionId,
                table: tableName,
                connectionId: connectionId ?? null,
                driverType: databaseType ?? null,
                database: database || null,
                schema,
              })
            }
          >
            {t('common.retry')}
          </button>
        </div>
      </div>
    );
  }

  const columnDefs: ColumnDef[] = columns.map((c) => ({
    id: c.name,
    name: c.name,
    type: c.dataType,
  }));

  const rowArrays: unknown[][] = rows.map((record) =>
    columns.map((col) => record[col.name] ?? null),
  );

  const filterActive = filterPanelOpen || filters.length > 0 || draftFilters.length > 0;

  return (
    <div className="flex flex-1 flex-col overflow-hidden" onKeyDown={handleTableKeyDown}>
      {error && (
        <div className="flex items-start gap-3 border-b border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-400">
          <CopyableError message={error} copyButton className="min-w-0 flex-1" />
          <button
            type="button"
            className="shrink-0 text-xs text-accent hover:underline"
            onClick={() =>
              void loadTableData({
                dbSessionId,
                table: tableName,
                connectionId: connectionId ?? null,
                driverType: databaseType ?? null,
                database: database || null,
                schema,
              })
            }
          >
            {t('common.retry')}
          </button>
        </div>
      )}
      <div className="flex shrink-0 items-start gap-0.5 border-b border-edge px-2 py-0.5">
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
        <NlFilterInput dbSessionId={dbSessionId} database={database} tableName={tableName} />
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
        <div
          className="border-b border-red-500/30 bg-red-500/10 px-3 py-1 text-xs text-red-400"
          role="alert"
        >
          {quickFilterError}
        </div>
      )}
      {safeModeTipVisible && (
        <div
          className="flex shrink-0 items-center gap-1.5 border-b border-warning/30 bg-warning/10 px-3 py-1.5 text-xs text-warning"
          role="status"
          aria-live="polite"
          data-testid="table-safe-mode-tip"
        >
          <ShieldAlert className="h-3.5 w-3.5 shrink-0" />
          {t('tableData.safeModeEditDisabled')}
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
              variant="run"
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
              onClick={() => rollbackPendingChanges()}
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
        onFilterPanelOpenChange={setFilterPanelOpen}
        onAddFilter={addFilter}
        onUpdateFilter={updateFilter}
        onFilterLogicChange={setFilterLogic}
        onApplyFilters={applyFilters}
        editingCell={safeMode || driverReadOnly ? null : editingCell}
        selectedRows={selectedRows}
        loading={loading}
        onSort={setSort}
        onRemoveFilter={removeFilter}
        onClearFilters={clearFilters}
        onPageChange={setPage}
        onPageSizeChange={setPageSize}
        onCellDoubleClick={handleCellDoubleClick}
        onCellEdit={handleCellEdit}
        onCellEditCancel={cancelEdit}
        onRowSelect={selectRow}
        onSelectAll={toggleSelectAll}
        onRowClick={setDetailRow}
        highlightedRow={detailRowIndex}
        exportTableName={tableName}
        databaseType={databaseType}
        dbSessionId={dbSessionId}
        dataExportCapability={dataExportCapability}
        primaryKeyColumns={columns.filter((c) => c.isPrimaryKey).map((c) => c.name)}
        onDeleteRows={driverReadOnly ? undefined : handleDeleteRows}
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
