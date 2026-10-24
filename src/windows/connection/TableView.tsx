import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Columns3 } from 'lucide-react';
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
import { buildRowIdentity, rowIdentityKey, tableChangeContextKey } from '../../lib/tableChanges';
import type { TableChangeContext } from '../../lib/tableChanges';
import { Button } from '../../components/ui/Button';
import { TablePendingChangesBar, TableTransactionControls } from './TablePendingChangesBar';
import { TableClipboardStatus } from './TableClipboardStatus';
import { TableNoPrimaryKeyTip } from './TableNoPrimaryKeyTip';
import { TableLoadErrorState } from './TableLoadErrorState';
import { useTableViewClipboard } from './useTableViewClipboard';
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
import { useTableEditing } from './tableEditing';
import { TableReadOnlyTip } from './TableReadOnlyTip';
import type { CellReadOnlyReason } from '../../lib/cellEditorKind';
import { isComposingEvent, isEditableTarget } from '../../lib/gridEventGuards';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { likeCaseInsensitiveForMeta, supportedOperatorsForMeta } from '../../lib/filterOperators';
import type { ColumnSchema, DatabaseType } from '../../types';
import { resolveDefaultTableSort } from '../../lib/defaultSort';
import { DEFAULT_ROW_COUNT_MODE, DEFAULT_TABLE_SORT } from '../../types/settings';
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
  /** Why the panel is read-only, when the mount point knows better (DB-05). */
  readOnlyReason?: CellReadOnlyReason;
  /** Data-export capability, threaded to the table-data export dialog. */
  dataExportCapability?: 'none' | 'loaded_only' | 'full_table';
  /**
   * Escape hatch of a table without a primary key (DB-16): opens a query tab
   * with a `SELECT … WHERE` draft. Absent — e.g. on a view panel, or wherever
   * the mount point cannot open a query tab — the banner keeps the shape it
   * shipped before DB-16.
   */
  onEditInSql?: () => void;
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
  readOnlyReason: readOnlyReasonProp,
  dataExportCapability,
  onEditInSql,
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
  /** The global default order; the panel's own override beats it. */
  const defaultTableSortSetting = useSettingsStore(
    (s) => s.settings.defaultTableSort ?? DEFAULT_TABLE_SORT,
  );
  /** How the user wants the row count obtained. */
  const rowCountMode = useSettingsStore((s) => s.settings.rowCountMode ?? DEFAULT_ROW_COUNT_MODE);
  const [confirmDelete, confirmDeleteDialog] = useConfirmDialog();
  const [confirmCommit, confirmCommitDialog] = useConfirmDialog();
  const [previewOpen, setPreviewOpen] = useState(false);
  /** The reason the last refusal showed (DB-05); `null` = no notice. */
  const [readOnlyTipReason, setReadOnlyTipReason] = useState<CellReadOnlyReason | null>(null);
  const readOnlyTipTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Manual transaction state, controls and pending-change actions live in
  // `TablePendingChangesBar`; this component keeps only the ChangeSet handlers.

  useEffect(() => {
    return () => {
      if (readOnlyTipTimer.current !== null) clearTimeout(readOnlyTipTimer.current);
    };
  }, []);

  const showReadOnlyTip = useCallback((reason: CellReadOnlyReason) => {
    setReadOnlyTipReason(reason);
    if (readOnlyTipTimer.current !== null) clearTimeout(readOnlyTipTimer.current);
    readOnlyTipTimer.current = setTimeout(() => {
      setReadOnlyTipReason(null);
      readOnlyTipTimer.current = null;
    }, 3000);
  }, []);

  const handleCellEdit = useCallback(
    (row: number, col: string, value: unknown) => {
      if (!isEditable) {
        showReadOnlyTip('connectionReadOnly');
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
  /**
   * The editing wiring of the panel (DB-05): the editor facts of every
   * column, the write path and the "Set Value" entries.
   */
  const editing = useTableEditing({
    panelId,
    dbSessionId,
    database,
    schema,
    tableName,
    databaseType,
    columns,
    connectionReadOnly: isConnectionReadOnly,
    mountReason: readOnlyReasonProp,
  });

  const handleCellDoubleClick = useCallback(
    (row: number, col: string) => {
      // The reason comes first: a cell that cannot be edited says **why**
      // (DB-05), and "read-only" alone leaves the user guessing. A read-only
      // connection is one of the reasons.
      const reason = editing.resolveReadOnlyReason(row, col);
      if (reason) {
        showReadOnlyTip(reason);
        return;
      }
      // A row that is marked for deletion must not be edited: it is on its way
      // out, and the row-level state says so (06 §4.2).
      if (selectDirtyRow(dirtyIndex, row)?.state === 'deleted') return;
      actions.startEdit(row, col);
    },
    [actions, dirtyIndex, editing, showReadOnlyTip],
  );
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
  /**
   * How much the row count may be trusted. The grid renders this state and
   * nothing else: an estimate is shown as "about N", and a database that
   * cannot count says so instead of showing zero rows.
   */
  const totalRowsKind = ts?.totalRowsKind ?? 'exact';
  /**
   * The user asked for an estimate and this database cannot give one.
   *
   * The host falls back to counting when the driver has no estimate, and says
   * so in the answer. The front end knows that on its own, so the grid says
   * it directly — there is no error to classify here, and nothing failed.
   */
  const estimateUnavailable =
    rowCountMode === 'estimated' && (ts?.columns.length ?? 0) > 0 && totalRowsKind !== 'estimated';
  /** The panel's temporary override of the global default order. */
  const defaultOrderOverride = ts?.defaultOrderOverride ?? 'inherit';
  /** The order this read actually uses. */
  const effectiveOrder = resolveDefaultTableSort(defaultOrderOverride, defaultTableSortSetting);
  /**
   * Without an order the rows of two pages may repeat or be skipped. The grid
   * has to say so — an explicit sort the user made is stable again.
   */
  const unstablePaging = effectiveOrder === 'none' && sorts.length === 0;
  /**
   * What the back end said about the paging of this page.
   *
   * A degradation is not an error: the page was read by offset and the
   * reason says why. The hint is **not** hidden when the row count is
   * unknown — that is exactly when the user cannot tell that something
   * degraded. What is hidden is the case where the database simply cannot
   * seek at all; see `showPagingReason`.
   */
  const pagingHint = useMemo(() => {
    const keyset = ts?.keyset;
    if (!keyset) return undefined;
    return {
      reason: keyset.unavailable,
      driftSuspected: keyset.driftSuspected,
      mode: keyset.mode,
    };
  }, [ts?.keyset]);

  /**
   * A table without a primary key cannot be ordered by one. The grid still
   * opens it — that has always worked — but the implicit order (the first
   * column) is worth saying out loud.
   */
  const noPrimaryKey =
    columns.length > 0 &&
    columns.every((column) => !column.isPrimaryKey) &&
    effectiveOrder !== 'none';

  /**
   * A table whose grid cannot address rows, on an otherwise editable
   * connection (DB-16). This outranks the order hint above: "you cannot edit
   * this here, but here is the way out" is actionable, "ordered by the first
   * column" is a footnote — and the footnote stays reachable from the pager's
   * sort dropdown.
   */
  const noPkEditBlocked =
    onEditInSql != null &&
    isEditable &&
    columns.length > 0 &&
    columns.every((column) => !column.isPrimaryKey);

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
        // The editor needs the nullability: a grid that has to guess it
        // would let a `NOT NULL` column accept an empty input as a `NULL`.
        nullable: c.nullable,
      })),
    [displayedColumns],
  );

  const rowArrays: unknown[][] = useMemo(
    () => rows.map((record) => displayedColumns.map((col) => record[col.name] ?? null)),
    [rows, displayedColumns],
  );

  /**
   * The clipboard bridge (DB-03): ⌘C / ⌘V on the grid, the "Copy Selected Cells As …"
   * menu, and the paste confirmation. `displayedColumns` is the render order, which is the
   * order a paste has to align against.
   */
  const clipboard = useTableViewClipboard({
    panelId,
    ts,
    actions,
    isEditable,
    columnOrder: displayedColumns.map((column) => column.name),
    tableLabel: tableName,
    onReadOnlyPaste: () => showReadOnlyTip('connectionReadOnly'),
  });
  const editingCell = ts?.editingCell ?? null;
  const detailRowIndex = ts?.detailRowIndex ?? null;
  const selectedRows = ts?.selectedRows ?? new Set<number>();
  const loading = ts?.loading ?? false;
  /**
   * An exact count is being fetched.
   *
   * The estimate is not wrong, it is just not the truth yet; the button that
   * asks for the truth says so while it waits, so the user knows the number
   * on screen is about to change.
   */
  const [counting, setCounting] = useState(false);
  useEffect(() => {
    if (!loading) setCounting(false);
  }, [loading]);
  const handleCountExact = useCallback(() => {
    setCounting(true);
    actions.countExact();
  }, [actions]);
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
      <TableLoadErrorState error={error} retryLabel={t('common.retry')} onRetry={actions.load} />
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
      {readOnlyTipReason ? <TableReadOnlyTip reason={readOnlyTipReason} /> : null}
      {noPkEditBlocked ? (
        <TableNoPrimaryKeyTip
          message={t('tableNoPk.editBanner')}
          action={{ label: t('tableNoPk.editInSql'), onClick: onEditInSql }}
        />
      ) : (
        noPrimaryKey && <TableNoPrimaryKeyTip message={t('defaultSort.noPrimaryKey')} />
      )}
      <TableClipboardStatus bridge={clipboard} />
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
        {...clipboard.dataTableProps}
        columns={columnDefs}
        rows={rowArrays}
        totalRows={totalRows}
        totalRowsKind={totalRowsKind}
        onCountExact={handleCountExact}
        counting={counting}
        unstablePaging={unstablePaging}
        estimateUnavailable={estimateUnavailable}
        pagingHint={pagingHint}
        defaultOrderOverride={defaultOrderOverride}
        onDefaultOrderOverrideChange={actions.setDefaultOrderOverride}
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
        // DB-02: the keyboard navigation of the grid (arrows, `Cmd+A`, `Enter`).
        keyboardNavigation
        lastSelectedIndex={ts?.lastSelectedIndex ?? null}
        loading={loading}
        onSort={actions.setSort}
        // DB-11: the multi-column sort actions of the header context menu.
        onAddSort={actions.addSort}
        onRemoveSort={actions.removeSort}
        onClearSorts={actions.clearSorts}
        onRemoveFilter={actions.removeFilter}
        onClearFilters={actions.clearFilters}
        onPageChange={actions.setPage}
        onPageSizeChange={actions.setPageSize}
        onCellDoubleClick={handleCellDoubleClick}
        onCellEdit={isEditable ? handleCellEdit : undefined}
        onCellEditCancel={actions.cancelEdit}
        cellEditing={editing.cellEditing}
        cellTypeHints={editing.cellTypeHints}
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
