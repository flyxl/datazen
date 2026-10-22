import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Columns3, ShieldAlert } from 'lucide-react';
import { DataTable } from '../../components/DataTable/DataTable';
import { DATA_TABLE_ROW_HEIGHT } from '../../components/DataTable/rowHeight';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { TableColumnFilter } from './TableColumnFilter';
import { useTableDataStore } from '../../stores/tableDataStore';
import { EMPTY_GRID_SELECTION } from '../../stores/tableData/gridSelection';
import { hasPendingWork } from '../../stores/tableData/pendingChanges';
import { selectDirtyRow } from '../../stores/tableData/dirtyIndex';
import { useSettingsStore } from '../../stores/settingsStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { CopyableError } from '../../components/ui/CopyableError';
import { buildRowIdentity, rowIdentityKey, tableChangeContextKey } from '../../lib/tableChanges';
import type { TableChangeContext } from '../../lib/tableChanges';
import { Button } from '../../components/ui/Button';
import {
  TablePendingChangesBar,
  TableTransactionControls,
} from './TablePendingChangesBar';
import { TableStatusBanners } from './TableStatusBanners';
import { TableToolbar } from './TableToolbar';
import { TableFkBreadcrumb } from './TableFkBreadcrumb';
import { TableFkErrorBanner } from './TableFkErrorBanner';
import { useTableActions } from './useTableActions';
import { useTableForeignKeyNavigation } from './useTableForeignKeyNavigation';
import { useDirtyIndex } from './useDirtyIndex';
import { PendingPlanDialog } from './PendingPlanDialog';
import { useTableDraftRows } from './useTableDraftRows';
import { useNamedViewMenu } from './useNamedViewMenu';
import { isComposingEvent, isEditableTarget } from '../../lib/gridEventGuards';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import {
  likeCaseInsensitiveForMeta,
  supportedOperatorsForMeta,
} from '../../lib/filterOperators';
import type { ColumnSchema, DatabaseType } from '../../types';
import { Spinner } from '../../components/ui/Spinner';

/** Stable empty fallbacks: a fresh array per render would defeat the memos below. */
const EMPTY_ROWS: Record<string, unknown>[] = [];
const EMPTY_COLUMNS: ColumnSchema[] = [];

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
  /**
   * The filter operators this database serves. A driver that declares none
   * gets the shared set; the dialect specific operators only show up when the
   * driver names them (the back end stays the authority).
   *
   * The rule ("nothing declared ⇒ the shared set") is the one in
   * `supportedOperatorsForMeta`, so the shipped path and the tested path
   * cannot drift apart.
   */
  const filterOperators = useMemo(
    () =>
      supportedOperatorsForMeta(
        databaseType ? DB_REGISTRY[databaseType as DatabaseType] : undefined,
      ),
    [databaseType],
  );
  /** Copy only: whether this database ignores case in `LIKE`. */
  const filterLikeCaseInsensitive = useMemo(
    () =>
      likeCaseInsensitiveForMeta(
        databaseType ? DB_REGISTRY[databaseType as DatabaseType] : undefined,
      ),
    [databaseType],
  );
  const isConnectionReadOnly = Boolean(
    readOnlyProp ?? (savedConnection?.readOnly || driverReadOnly),
  );
  const isEditable = !isConnectionReadOnly;

  // NlFilterInput handles unconfigured state internally
  const ts = useTableDataStore((s) => s.byPanel.get(panelId));
  /** New-row drafts: the staged rows plus the interaction state around them. */
  const drafts = useTableDraftRows(panelId, ts, isEditable);
  /**
   * The panel pins its own target database, so a cross-database table loads
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
  /**
   * Neutral notice shown when an insert could not be located after the commit.
   *
   * It describes **one commit of one page**, so it expires with the page
   * state: a reload, another page, new filters, a new sort or another table
   * all make "the new row could not be located" stale, and a notice that
   * outlived its commit would contradict the highlight the user sees.
   */
  const [insertNotice, setInsertNotice] = useState<string | null>(null);
  /**
   * Every action that moves the page invalidates the notice: the user is
   * looking at a different set of rows than the one the notice talks about.
   *
   * It is stable on purpose — the action table is memoised on it.
   */
  const invalidateNotice = useCallback(() => setInsertNotice(null), []);
  /** Panel-scoped action binders: every store call is namespaced by this tab's id. */
  const actions = useTableActions({ panelId, context, invalidateNotice });

  /**
   * The dirty index of the panel (DB-06): the **single** derivation of "what is
   * staged but not committed". The grid paints the cells and the bar shows the
   * counts, and both read this one index — that is what keeps the highlight and
   * the counter from drifting apart.
   */
  const dirtyIndex = useDirtyIndex(ts);

  const confirmOnDelete = useSettingsStore((s) => s.settings.confirmOnDelete);
  const [confirmDelete, confirmDeleteDialog] = useConfirmDialog();
  const [confirmCommit, confirmCommitDialog] = useConfirmDialog();
  const [previewOpen, setPreviewOpen] = useState(false);
  const [readOnlyTipVisible, setReadOnlyTipVisible] = useState(false);
  const readOnlyTipTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Manual transaction state, controls and pending-change actions live in
  // `TablePendingChangesBar`; this component keeps only the ChangeSet handlers.

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
      // A row that is marked for deletion must not be edited: it is on its way
      // out, and the row-level state says so (06 §4.2).
      if (selectDirtyRow(dirtyIndex, row)?.state === 'deleted') return;
      actions.startEdit(row, col);
    },
    [actions, dirtyIndex, isEditable, showReadOnlyTip],
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
  /**
   * Another table (or another database / schema) means another set of rows:
   * the notice of the previous table says nothing about this one.
   */
  useEffect(() => {
    setInsertNotice(null);
  }, [contextKey]);
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

  const columns = ts?.columns ?? EMPTY_COLUMNS;
  const rows = ts?.rows ?? EMPTY_ROWS;
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

  /** The saved-views menu, or nothing when this backend cannot serve it. */
  const namedView = useNamedViewMenu({
    connectionId,
    database,
    schema,
    tableName,
    filters,
    filterLogic,
    sorts,
    draftFilters,
    draftFilterLogic,
    visibleColumns,
    actions,
  });

  const displayedColumns = useMemo(() => {
    if (visibleColumns === null) return columns;
    const set = new Set(visibleColumns);
    return columns.filter((c) => set.has(c.name));
  }, [columns, visibleColumns]);

  /** The foreign-key navigation of this tab (DB-07): arrows, jump, breadcrumb, keyboard, viewport. */
  const fkNavigation = useTableForeignKeyNavigation({
    panelId,
    dbSessionId,
    database,
    tableName,
    schema,
    connectionId: connectionId ?? null,
    databaseType: databaseType ?? null,
    rows,
    displayedColumns,
    rowHeight: DATA_TABLE_ROW_HEIGHT,
    activeCell: ts?.gridSelection?.focus ?? null,
  });

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
  const previewPlan = ts?.previewPlan ?? null;
  const pendingStatus = ts?.pendingStatus ?? 'idle';
  const dirtyCounts = dirtyIndex.counts;
  const pendingUpdateCount = dirtyCounts.updateRows;
  const pendingDeleteCount = dirtyCounts.deleteRows;
  /** Staged new rows; they are pending work just like the staged edits. */
  const pendingInsertCount = ts?.draftInserts.size ?? 0;
  /**
   * Everything that has to be committed: staged edits **and** staged new rows.
   * A panel with only drafts still has work, so its commit button is live.
   */
  const hasWork = ts ? hasPendingWork(ts) : false;
  const pendingBusy = loading || pendingStatus !== 'idle';

  /**
   * The row the "new row" highlight points at.
   *
   * The store keeps the **identity key** of the located row; the grid needs an
   * index. When the row cannot be located (the regular path in this batch:
   * the driver returns no primary key), there is no highlight at all.
   */
  const highlightedNewRowIndex = useMemo(() => {
    const key = ts?.highlightedNewRowKey;
    if (!key || !ts) return null;
    const pkColumns = ts.columns.filter((column) => column.isPrimaryKey);
    if (pkColumns.length === 0) return null;
    const index = ts.rows.findIndex((row) => {
      const identity = buildRowIdentity(row, pkColumns);
      if (!identity) return false;
      try {
        return rowIdentityKey(identity) === key;
      } catch {
        return false;
      }
    });
    return index >= 0 ? index : null;
  }, [ts]);

  // The highlight expires on its own (04 §4.1: the new row is highlighted for
  // a moment, not forever).
  useEffect(() => {
    if (!ts?.highlightedNewRowKey) return;
    const timer = setTimeout(() => {
      useTableDataStore.getState().clearHighlightedNewRow(panelId);
    }, 1500);
    return () => clearTimeout(timer);
  }, [panelId, ts?.highlightedNewRowKey]);

  useEffect(() => {
    if (!isEditable && editingCell) actions.cancelEdit();
  }, [actions, editingCell, isEditable]);

  const handlePreviewPendingChanges = useCallback(async () => {
    if (driverReadOnly || pendingBusy || !hasWork) return;
    const plan = await actions.previewPendingChanges();
    if (plan) setPreviewOpen(true);
  }, [actions, driverReadOnly, pendingBusy, hasWork]);

  const handleCommitPendingChanges = useCallback(async () => {
    if (driverReadOnly || pendingBusy || !hasWork) return;
    const confirmed = await confirmCommit({
      title: t('tableData.commit'),
      message:
        pendingInsertCount > 0
          ? t('tableData.confirmCommitWithInserts', {
              inserts: pendingInsertCount,
              updates: pendingUpdateCount,
              deletes: pendingDeleteCount,
            })
          : t('tableData.confirmCommit', {
              updates: pendingUpdateCount,
              deletes: pendingDeleteCount,
            }),
      confirmLabel: t('tableData.commit'),
      kind: 'warning',
    });
    if (!confirmed) return;
    const result = await actions.commitPendingChanges();
    // The insert rows have no returned primary key in this batch, so the new
    // row is often not locatable. The interface says so instead of claiming a
    // highlight it cannot deliver (04 §4.1) — and it says **nothing** when
    // the row *was* located: a notice left over from an earlier commit would
    // contradict the highlight the user is looking at.
    setInsertNotice(
      result.insertLocation === 'unavailable' ? t('tableData.insertReturningUnavailable') : null,
    );
  }, [
    actions,
    confirmCommit,
    driverReadOnly,
    pendingBusy,
    pendingDeleteCount,
    pendingInsertCount,
    pendingUpdateCount,
    hasWork,
    t,
  ]);

  const handleTableKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      // The foreign-key navigation (DB-07) comes first: it is the only
      // shortcut that is about the tab, not about the grid.
      fkNavigation.onKeyDown(event);
      if (event.defaultPrevented) return;
      // An IME composition owns every key, and so does a text field: the grid
      // shortcuts never fire while the user is typing (01-selection §4.5).
      if (isComposingEvent(event)) return;
      const mod = event.metaKey || event.ctrlKey;
      if (mod && event.key === 'Enter') {
        if (!hasWork || pendingBusy) return;
        event.preventDefault();
        void handleCommitPendingChanges();
        return;
      }
      if (mod && (event.key === 'i' || event.key === 'I')) {
        if (!isEditable || pendingBusy) return;
        if (isEditableTarget(event.target)) return;
        event.preventDefault();
        drafts.addDraftRow();
        return;
      }
      if (mod && (event.key === 'd' || event.key === 'D')) {
        if (!isEditable || pendingBusy) return;
        if (isEditableTarget(event.target)) return;
        // The focus is on a real row: duplicating a row is not this track's
        // business (02 owns that), so the key is left alone.
        if (!drafts.activeDraftId) return;
        event.preventDefault();
        drafts.duplicateDraftRow(drafts.activeDraftId);
        return;
      }
      if (event.key === 'Escape') {
        // The editor owns Escape while it is open; the grid clears its own
        // selection. What is left for the panel is the drafts: the **empty**
        // ones go, a filled one stays (04 §4.1).
        if (editingCell || drafts.editingDraft) return;
        if (isEditableTarget(event.target)) return;
        drafts.removeEmptyDraftRows();
        return;
      }
      if (event.key === 'Delete' || event.key === 'Backspace') {
        // The grid handles Delete for the selected real rows; when it did, the
        // event is already consumed and the draft must survive.
        if (event.defaultPrevented) return;
        if (!isEditable || pendingBusy) return;
        if (isEditableTarget(event.target)) return;
        // A cell editor owns the key while it is open.
        if (editingCell || drafts.editingDraft) return;
        if (!drafts.activeDraftId) return;
        event.preventDefault();
        drafts.removeDraftRow(drafts.activeDraftId);
      }
    },
    [
      drafts,
      editingCell,
      fkNavigation,
      handleCommitPendingChanges,
      isEditable,
      pendingBusy,
      hasWork,
    ],
  );

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

  return (
    <div className="flex flex-1 flex-col overflow-hidden" onKeyDown={handleTableKeyDown}>
      {fkNavigation.breadcrumb && (
        <TableFkBreadcrumb
          origin={fkNavigation.breadcrumb.origin}
          truncated={fkNavigation.breadcrumb.truncated}
          canGoBack={fkNavigation.breadcrumb.canGoBack}
          canGoForward={fkNavigation.breadcrumb.canGoForward}
          onBack={fkNavigation.onBack}
          onForward={fkNavigation.onForward}
        />
      )}
      <TableStatusBanners
        error={error}
        hasWork={hasWork}
        insertNotice={insertNotice}
        onRetry={actions.load}
      />
      {/* A refused jump stays in the tab it started from: no tab is opened,
          and the reason is shown here. */}
      <TableFkErrorBanner error={fkNavigation.error} onDismiss={fkNavigation.dismissError} />
      <TableToolbar
        panelId={panelId}
        dbSessionId={dbSessionId}
        database={database}
        tableName={tableName}
        columns={columns}
        loading={loading}
        pendingBusy={pendingBusy}
        isEditable={isEditable}
        filterPanelOpen={filterPanelOpen}
        appliedFilterCount={filters.length}
        draftFilterCount={draftFilters.length}
        onRefresh={handleRefresh}
        onSetFilterPanelOpen={actions.setFilterPanelOpen}
        onAddFilter={actions.addFilter}
        onSetFilters={actions.setFilters}
        onClearFilters={actions.clearFilters}
        onAddDraftRow={() => drafts.addDraftRow()}
        namedView={namedView}
      />
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
      <TableTransactionControls
        panelId={panelId}
        context={context}
        isEditable={isEditable}
        loading={loading}
        onReload={actions.reload}
      />
      <TablePendingChangesBar
        pendingCount={dirtyCounts.totalRows + pendingInsertCount}
        updateCount={pendingUpdateCount}
        deleteCount={pendingDeleteCount}
        insertCount={pendingInsertCount}
        counts={dirtyCounts}
        busy={pendingBusy}
        onPreview={() => void handlePreviewPendingChanges()}
        onCommit={() => void handleCommitPendingChanges()}
        onRollback={actions.rollbackPendingChanges}
      />
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
        filterOperators={filterOperators}
        filterLikeCaseInsensitive={filterLikeCaseInsensitive}
        onAddFilter={actions.addFilter}
        onUpdateFilter={actions.updateFilter}
        onFilterLogicChange={actions.setFilterLogic}
        onApplyFilters={actions.applyFilters}
        editingCell={!isEditable ? null : editingCell}
        selectedRows={selectedRows}
        gridSelection={ts?.gridSelection ?? EMPTY_GRID_SELECTION}
        onGridSelectionChange={actions.setGridSelection}
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
        highlightedRow={detailRowIndex ?? highlightedNewRowIndex}
        dirtyByRow={dirtyIndex.byRowIndex}
        onRestoreRow={actions.unstageRowDelete}
        // The foreign-key jump (DB-07): the arrows and the callback.
        foreignKeys={fkNavigation.foreignKeys}
        rowHeight={DATA_TABLE_ROW_HEIGHT}
        onScrollElementChange={fkNavigation.onScrollElementChange}
        // The marker of the inserted row is its own channel: the background
        // highlight above is shared with the detail panel, so only a marker
        // with a word on it tells the two apart.
        newRowIndex={highlightedNewRowIndex}
        draftRows={drafts.draftRows}
        editingDraft={drafts.editingDraft}
        onDraftCellEdit={drafts.setDraftValue}
        onDraftCellEditStart={drafts.startDraftEdit}
        onDraftCellEditCancel={drafts.cancelDraftEdit}
        onDraftRowRemove={drafts.removeDraftRow}
        onDraftRowActivate={drafts.activateDraftRow}
        onAddDraftRow={isEditable ? () => drafts.addDraftRow() : undefined}
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
