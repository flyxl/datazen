import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Columns3 } from 'lucide-react';
import { DataTable } from '../../components/DataTable/DataTable';
import { QuickLookDialog, isQuickLookableValue } from '../../components/DataTable/QuickLookDialog';
import { DATA_TABLE_ROW_HEIGHT, rowHeightForDensity } from '../../components/DataTable/rowHeight';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { TableColumnFilter } from './TableColumnFilter';
import { useTableDataStore, orderRequest } from '../../stores/tableDataStore';
import { keysetRequestFor } from '../../stores/tableData/keysetActions';
import { resolvePanelCountStrategy } from '../../stores/tableData/countStrategy';
import { tableManualHandle } from '../../lib/session/tableSession';
import { databaseCommands } from '../../commands/database';
import { FilterSqlPreviewDialog } from '../../components/filter/FilterSqlPreviewDialog';
import type { TableReadPreviewSql } from '@datazen/backend-client';
import {
  normalizeSelectionRange,
  EMPTY_GRID_SELECTION,
} from '../../stores/tableData/gridSelection';
import { hasPendingWork } from '../../stores/tableData/pendingChanges';
import { selectDirtyRow } from '../../stores/tableData/dirtyIndex';
import { displayedColumnNames } from '../../stores/tableData/selectionActions';
import {
  readPersistedColumnLayout,
  persistColumnWidths,
} from '../../stores/tableData/tableColumnLayoutPreferences';
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
import { useTableForeignKeys } from '../../hooks/useTableForeignKeys';
import { indexFkCandidateSources } from '../../lib/fkSelector';
import { useDirtyIndex } from './useDirtyIndex';
import { PendingPlanDialog } from './PendingPlanDialog';
import { useTableDraftRows } from './useTableDraftRows';
import { useNamedViewMenu } from './useNamedViewMenu';
import { useTableEditing } from './tableEditing';
import { TableReadOnlyTip } from './TableReadOnlyTip';
import type { CellReadOnlyReason } from '../../lib/cellEditorKind';
import { isComposingEvent, isEditableTarget } from '../../lib/gridEventGuards';
import { DB_REGISTRY } from '../../lib/databaseTypes';
import { useColumnStats } from '../../hooks/useColumnStats';
import { ColumnStatsDialog } from '../../components/DataTable/ColumnStatsDialog';
import { SetValueDateDialog } from './SetValueDateDialog';
import { fileCommands } from '../../commands/file';
import { base64ToBytes } from '../../lib/base64';
import { likeCaseInsensitiveForMeta, supportedOperatorsForMeta } from '../../lib/filterOperators';
import { expandAnyColumnFilter } from '../../lib/anyColumnFilter';
import { valueSearchTypeGroupsFor } from '../../lib/valueSearch';
import type { ColumnSchema, DatabaseType, Value } from '../../types';
import { resolveDefaultTableSort } from '../../lib/defaultSort';
import {
  DEFAULT_GRID_ALTERNATING_ROWS,
  DEFAULT_GRID_AUTO_HIDE_SCROLLBAR,
  DEFAULT_GRID_DENSITY,
  DEFAULT_GRID_ROW_NUMBERS,
  DEFAULT_ROW_COUNT_MODE,
  DEFAULT_TABLE_SORT,
} from '../../types/settings';
import { Spinner } from '../../components/ui/Spinner';
import type { ValuePresentationSnapshot } from '../../lib/valuePresentation/presentationPlan';

/** Stable empty fallbacks: a fresh array per render would defeat the memos below. */
const EMPTY_ROWS: Record<string, unknown>[] = [];
const EMPTY_COLUMNS: ColumnSchema[] = [];
/**
 * The extensions the binary loader offers in the native pick dialog (DBR-05).
 * The host keeps its own allow-list (`commands/file.rs`); this list stays
 * inside it — anything else gets the host's explainable "extension not
 * allowed" error, which is the failure path the acceptance asks to see.
 */
const BINARY_FILE_EXTENSIONS = [
  'png',
  'jpg',
  'jpeg',
  'gif',
  'bmp',
  'webp',
  'svg',
  'ico',
  'pdf',
  'bin',
  'dat',
  'txt',
  'json',
  'xml',
  'md',
  'sql',
  'zip',
  'gz',
];

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
  /**
   * Value-presentation snapshot (issue-32 T05, design §7). Omitted ⇒
   * DataTable falls back to its default snapshot and the grid renders
   * exactly as before (default-off); T06 will supply the real snapshot.
   * The source is derived below, never faked: without a `connectionId`
   * there is no source at all (BUG-004: a missing field must not collide
   * with an empty string).
   */
  presentationSnapshot?: ValuePresentationSnapshot;
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
  presentationSnapshot,
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
  /**
   * Whether the Raw SQL pane is offered at all (DBR-04): the driver's own
   * declaration. A driver that says nothing keeps the two-pane panel it had,
   * and the back end would refuse a raw fragment anyway — the flag hides an
   * entry the database would reject, it never widens what may run.
   */
  const rawWhereSupported =
    databaseType != null &&
    (
      DB_REGISTRY[databaseType as DatabaseType] as
        | { dataGrid?: { rawWhereFilter?: boolean } }
        | undefined
    )?.dataGrid?.rawWhereFilter === true;
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
  /* ── Grid appearance (DB-17), all four with the pre-setting default as the
     fallback: a settings file written before the keys existed keeps the grid
     exactly as it was. ── */
  const gridDensity = useSettingsStore((s) => s.settings.gridDensity ?? DEFAULT_GRID_DENSITY);
  const gridAlternatingRows = useSettingsStore(
    (s) => s.settings.gridAlternatingRows ?? DEFAULT_GRID_ALTERNATING_ROWS,
  );
  const gridRowNumbers = useSettingsStore(
    (s) => s.settings.gridRowNumbers ?? DEFAULT_GRID_ROW_NUMBERS,
  );
  const gridAutoHideScrollbar = useSettingsStore(
    (s) => s.settings.gridAutoHideScrollbar ?? DEFAULT_GRID_AUTO_HIDE_SCROLLBAR,
  );
  /** The table grid's row height under the density setting (DB-17). */
  const gridRowHeight = rowHeightForDensity(DATA_TABLE_ROW_HEIGHT, gridDensity);
  const [confirmDelete, confirmDeleteDialog] = useConfirmDialog();
  const [confirmCommit, confirmCommitDialog] = useConfirmDialog();
  /** The binary "load from file" confirm (DBR-05): its own dialog, because its
   * message is built from the picked file, not from a fixed string. */
  const [confirmBinaryLoad, confirmBinaryLoadDialog] = useConfirmDialog();
  const [previewOpen, setPreviewOpen] = useState(false);
  /**
   * The cell the grid's Quick Look entries (DBR-05) opened — the middle-click
   * on a cell and the `Cmd/Ctrl+Shift+L` shortcut. `null` = closed.
   */
  const [quickLookCell, setQuickLookCell] = useState<{
    rowIndex: number;
    columnName: string;
  } | null>(null);
  /** The date/datetime picker of the "Set Value" menu (DBR-05). `null` = closed. */
  const [datePick, setDatePick] = useState<{
    rowIndex: number;
    columnName: string;
  } | null>(null);
  /**
   * The SQL preview dialog of the filter panel (DB-15): the statements the
   * next read of this panel would send, straight from the backend's own
   * assembly. The request mirrors the exact parameters `loadTableData`
   * sends, so the shown statement is the executed one.
   */
  const [sqlPreview, setSqlPreview] = useState<{
    open: boolean;
    preview: TableReadPreviewSql | null;
    error: string | null;
  }>({ open: false, preview: null, error: null });
  /** The filter panel's "Export…" exit opens the export dialog via a signal (DB-15). */
  const [exportSignal, setExportSignal] = useState(0);
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
   * The foreign keys of the table (DB-20): the in-cell selector needs the
   * single-column keys to route its candidate loads. The navigation hook
   * below reads the same schema — the cache makes the second read free.
   */
  const fkMeta = useTableForeignKeys({
    dbSessionId,
    tableName,
    database,
    schema: schema ?? null,
    connectionId: connectionId ?? null,
  });
  const fkSources = useMemo(() => indexFkCandidateSources(fkMeta.index.entries()), [fkMeta.index]);
  /**
   * The dialog entries of the "Set Value" menu (DBR-05): the date/datetime
   * picker and the binary "load from file".
   *
   * The binary flow lives here in one place — pick the file, show what would
   * be written (name, size, target column), stage only on confirm (PRD §8).
   * A cancelled native dialog or a rejected confirm is a cancelled write: the
   * original value is untouched, and a failed pick leaves no partial state.
   */
  const handleSetValueDialog = useCallback(
    (kind: 'pickDate' | 'loadBinaryFromFile', rowIndex: number, columnName: string) => {
      if (kind === 'pickDate') {
        setDatePick({ rowIndex, columnName });
        return;
      }
      const column = columns.find((c) => c.name === columnName);
      if (!column) return;
      void fileCommands
        .openBase64WithDialog(t('dataTable.binaryFileFilterName'), BINARY_FILE_EXTENSIONS)
        .then(async (file) => {
          // A cancelled native dialog is a cancelled write: no confirm, no
          // staging, the cell keeps its value.
          if (!file) return;
          const bytes = base64ToBytes(file.dataBase64);
          const confirmed = await confirmBinaryLoad({
            title: t('dataTable.loadBinaryFromFile'),
            message: `${file.fileName} · ${bytes.length} ${t('detail.bytes')}`,
            description: t('dataTable.binaryLoadConfirm', { column: column.name }),
            kind: 'info',
          });
          if (!confirmed) return;
          // The staged payload is the same untagged byte array the wire has
          // always carried for binary; the preview/commit path is the shared
          // one, so the write is guarded like every other staged change.
          useTableDataStore
            .getState()
            .stageCellValue(panelId, rowIndex, columnName, Array.from(bytes));
        })
        .catch(() => {
          // The pick itself failed (a rejected extension, an unreadable
          // file): the value stays untouched — no silent partial state.
        });
    },
    [columns, confirmBinaryLoad, panelId, t],
  );

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
    fkSources,
    // The two dialog entries of the "Set Value" menu (DBR-05): this panel
    // is their host.
    onSetValueDialog: handleSetValueDialog,
  });

  /**
   * The request parameters of this panel's next read (DB-15) — mirrored from
   * what `loadTableData` sends, so the previewed statement is the executed
   * one. `unbounded` drops the paging frame (the filtered export streams it).
   */
  const buildPreviewRequest = useCallback(
    (unbounded?: boolean) => {
      if (!ts) return null;
      const order = orderRequest(
        ts.defaultOrderOverride,
        ts.columns,
        ts.sorts,
        defaultTableSortSetting,
      );
      const keyset = keysetRequestFor(ts, ts.page);
      return {
        dbSessionId,
        context,
        manualHandle: tableManualHandle(panelId),
        table: tableName,
        page: ts.page,
        pageSize: ts.pageSize,
        filters: ts.filters,
        sorts: order.sorts,
        filterLogic: ts.filterLogic,
        // The applied raw predicate (DBR-04): the preview assembles through
        // the same builder the read executes, so the statements it shows are
        // byte for byte the ones the read sends — raw predicate included.
        rawWhere: ts.rawWhere,
        countStrategy: resolvePanelCountStrategy(context),
        defaultOrderColumns: order.defaultOrderColumns,
        afterKey: keyset.afterKey,
        database,
        schema,
        ...(unbounded ? { unbounded } : {}),
      };
    },
    [context, database, dbSessionId, defaultTableSortSetting, panelId, schema, tableName, ts],
  );

  /** Show the statements the next read would send (DB-15). */
  const handleFilterPreviewSql = useCallback(() => {
    const request = buildPreviewRequest();
    if (!request) return;
    setSqlPreview({ open: true, preview: null, error: null });
    void databaseCommands
      .previewTableReadSql(request)
      .then((preview) => setSqlPreview({ open: true, preview, error: null }))
      .catch((error: unknown) =>
        setSqlPreview({
          open: true,
          preview: null,
          error: error instanceof Error ? error.message : String(error),
        }),
      );
  }, [buildPreviewRequest]);

  /**
   * The statement the filtered full-table export streams (DB-15): the same
   * assembly the preview shows, without the paging frame.
   */
  const buildFilteredExportSql = useCallback(async () => {
    const request = buildPreviewRequest(true);
    if (!request) throw new Error('Panel state unavailable');
    const preview = await databaseCommands.previewTableReadSql(request);
    return preview.dataSql;
  }, [buildPreviewRequest]);

  /** The filter panel's direct exit to the export dialog (DB-15). */
  const handleFilterExportFiltered = useCallback(() => {
    setExportSignal((value) => value + 1);
  }, []);

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
  /* ── The DBR-04 filter modes: pane, drafts, and the applied raw channel. ── */
  const filterMode = ts?.filterMode ?? 'structured';
  const anyColumnDraft = ts?.anyColumnDraft ?? '';
  const rawWhereDraft = ts?.rawWhereDraft ?? '';
  const rawWhere = ts?.rawWhere ?? null;
  /**
   * The Any Column search scope (DBR-04): the same driver-declared searchable
   * types the full-database value search uses (DBR-03), resolved against this
   * table's columns. The report says what would be searched and what is
   * skipped — a skipped column is named, never silently matched with a cast
   * the database may reject.
   */
  const anyColumnReport = useMemo(() => {
    // The driver's own column-type rules — the same declaration the cell
    // editor and the structured filter read (`tableEditing` resolves the
    // same field), so the Any Column scope can never disagree with them.
    const hints = databaseType
      ? (
          DB_REGISTRY[databaseType as DatabaseType] as
            | { cellTypes?: import('../../lib/cellTypes').CellTypeHints }
            | undefined
        )?.cellTypes
      : undefined;
    return expandAnyColumnFilter(columns, anyColumnDraft, {
      groups: valueSearchTypeGroupsFor(databaseType ?? null),
      hints,
    });
  }, [anyColumnDraft, columns, databaseType]);
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

  /** The primary-key column names, in schema order (the toolbar and the grid both ask). */
  const primaryKeyColumnNames = columns
    .filter((column) => column.isPrimaryKey)
    .map((column) => column.name);

  /**
   * A table whose grid cannot address rows, on an otherwise editable
   * connection (DB-16). This outranks the order hint above: "you cannot edit
   * this here, but here is the way out" is actionable, "ordered by the first
   * column" is a footnote — and the footnote stays reachable from the
   * toolbar's default-order select.
   */
  const noPkEditBlocked =
    onEditInSql != null &&
    isEditable &&
    columns.length > 0 &&
    columns.every((column) => !column.isPrimaryKey);

  /**
   * DB-16 (entrance hardening): a keyless table cannot address existing rows,
   * so every write that targets one — cell edits, row deletes, pastes into
   * loaded rows — is withdrawn at the **entrance** instead of failing at
   * stage time (PRD DB-16: no path may look clickable and then fail).
   * This is a fact about the data, not about the SQL exit: it holds even
   * when no `onEditInSql` handler is threaded. Draft inserts stay enabled —
   * an INSERT needs no row locator.
   */
  const noPkWriteBlocker = columns.length > 0 && columns.every((column) => !column.isPrimaryKey);
  /** In-place writes need both a writable connection and a locatable row identity. */
  const cellWritesEnabled = isEditable && !noPkWriteBlocker;

  /** The saved-views menu, or nothing when this backend cannot serve it. */
  const namedView = useNamedViewMenu({
    connectionId,
    database,
    schema,
    tableName,
    filters,
    filterLogic,
    // The applied raw predicate (DBR-04): a view saved while the Raw SQL
    // filter is in effect carries the predicate, so replaying it asks the
    // same question.
    rawWhere,
    sorts,
    draftFilters,
    draftFilterLogic,
    visibleColumns,
    actions,
  });

  const displayedColumns = useMemo(() => {
    // DB-12: the panel's explicit order (drag, persisted) is a permutation of
    // the *displayed* names — the same resolver the selection uses.
    const names = displayedColumnNames(columns, visibleColumns, ts?.columnOrder ?? null);
    const byName = new Map(columns.map((c) => [c.name, c]));
    return names
      .map((name) => byName.get(name))
      .filter((c): c is (typeof columns)[number] => c != null);
  }, [columns, visibleColumns, ts?.columnOrder]);

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
    rowHeight: gridRowHeight,
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
   * Quick Look of a grid cell (DBR-05): the viewer the middle-click and the
   * `Cmd/Ctrl+Shift+L` shortcut open. A read gesture first — read-only panels
   * keep it, only the dialog's save exit needs the write path.
   *
   * The entry opens for content that is worth a viewer — binary, structured
   * JSON, or a long text — the same verdict the detail drawer's per-field
   * button is built on. An empty cell keeps the browser's middle-click.
   */
  const openCellQuickLook = useCallback(
    (rowIndex: number, columnName: string) => {
      if (rowIndex < 0 || rowIndex >= rows.length) return;
      const column = columns.find((c) => c.name === columnName);
      if (!column) return;
      const value = rows[rowIndex][columnName];
      if (!isQuickLookableValue(value, column.dataType)) return;
      setQuickLookCell({ rowIndex, columnName });
    },
    [rows, columns],
  );

  /* ── Column tools (DB-12): reorder, freeze, width persistence. ── */
  const handleReorderColumns = useCallback(
    (from: number, to: number) => {
      // The drag is over the displayed columns; the stored order is that
      // whole list, reordered, and the store resolver keeps anything the
      // panel did not name in its schema position behind it.
      const names = displayedColumns.map((c) => c.name);
      if (from < 0 || from >= names.length || to < 0 || to >= names.length) return;
      const [moved] = names.splice(from, 1);
      names.splice(to, 0, moved);
      actions.setColumnOrder(names);
    },
    [displayedColumns, actions],
  );

  /** The widths this table was last left with — read once per table identity. */
  const tableIdentityKey = ts?.context ? tableChangeContextKey(ts.context) : null;
  const persistedColumnWidths = useMemo(() => {
    const context = ts?.context ?? null;
    return context ? readPersistedColumnLayout(context).widths : null;
    // The identity key is the stable string of the table; the context object
    // itself is rebuilt by every load.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tableIdentityKey]);
  const handleColumnWidthsCommit = useCallback(
    (widths: Record<string, number>) => {
      if (ts?.context) persistColumnWidths(ts.context, widths);
    },
    [ts?.context],
  );

  const frozenColumnCount = ts?.frozenColumnCount ?? 0;
  const handleFreezeThrough = useCallback(
    (throughIndex: number) => actions.setFrozenColumnCount(throughIndex + 1),
    [actions],
  );
  const handleUnfreezeColumns = useCallback(() => actions.setFrozenColumnCount(0), [actions]);

  /* ── Column statistics (DB-12): server-side aggregates from the header
     menu. The numbers never come from the loaded page. ── */
  const {
    state: columnStatsState,
    open: openColumnStats,
    dismiss: dismissColumnStats,
  } = useColumnStats();
  const handleColumnStats = useCallback(
    (column: string) => {
      openColumnStats(
        {
          dbSessionId,
          database: database || '',
          databaseType: databaseType as DatabaseType,
          schema,
          tableName,
        },
        column,
        displayedColumns.find((c) => c.name === column)?.dataType ?? null,
      );
    },
    [openColumnStats, dbSessionId, database, databaseType, schema, tableName, displayedColumns],
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
    isEditable: cellWritesEnabled,
    columnOrder: displayedColumns.map((column) => column.name),
    tableLabel: tableName,
    // A refused paste has one of two panel-level reasons; the tip must say which.
    onReadOnlyPaste: () =>
      showReadOnlyTip(noPkWriteBlocker ? 'noPrimaryKey' : 'connectionReadOnly'),
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
    if (!cellWritesEnabled && editingCell) actions.cancelEdit();
  }, [actions, editingCell, cellWritesEnabled]);

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

  /**
   * Right-click "Paste to Selected Cells": read the system clipboard and land
   * it at the top-left of the current cell selection — the same anchor the
   * `paste` DOM gesture uses, arrived at from the menu instead.
   */
  const handleMenuPasteSelectedCells = useCallback(() => {
    const gridSelection = ts?.gridSelection;
    if (!gridSelection || gridSelection.mode !== 'cell') return;
    const anchor = normalizeSelectionRange(
      gridSelection,
      displayedColumns.map((c) => c.name),
    );
    if (!anchor) return;
    clipboard.handlers.onPasteFromClipboard?.({
      rowIndex: anchor.start.rowIndex,
      columnName: anchor.start.columnName,
    });
  }, [clipboard.handlers, displayedColumns, ts?.gridSelection]);

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
        // With an active draft, ⌘D keeps its pre-DBR-02 meaning: duplicate
        // that draft. Without one — but with real rows selected — it copies
        // the selected existing rows as new drafts (DBR-02, PRD 5.2: the
        // gesture fires only on a real row selection, never in an editor).
        if (drafts.activeDraftId) {
          event.preventDefault();
          drafts.duplicateDraftRow(drafts.activeDraftId);
          return;
        }
        const gridSelection = ts?.gridSelection;
        const hasRealRowSelection =
          gridSelection != null &&
          (gridSelection.mode === 'row'
            ? selectedRows.size > 0
            : normalizeSelectionRange(
                gridSelection,
                displayedColumns.map((c) => c.name),
              ) != null);
        if (!hasRealRowSelection) return;
        event.preventDefault();
        clipboard.copyRowsAsDrafts();
        return;
      }
      if (mod && event.shiftKey && (event.key === 'l' || event.key === 'L')) {
        // Quick Look of the focused cell (DBR-05). It is a read gesture — a
        // read-only panel keeps it; the dialog's save exit is what needs the
        // write path. A cell must be focused: the shortcut describes *this*
        // cell, and without a focus there is nothing to look at.
        if (isEditableTarget(event.target)) return;
        const focus = ts?.gridSelection?.focus;
        if (!focus) return;
        event.preventDefault();
        openCellQuickLook(focus.rowIndex, focus.columnName);
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
      openCellQuickLook,
      pendingBusy,
      hasWork,
      ts,
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
        // The right-click clipboard entries (DB-03 wiring + DBR-02): the DOM
        // gestures stay on the bridge, the menu actions are explicit props.
        onCopySelectedCells={() => {
          void clipboard.copySelectedCellsAs('tsv');
        }}
        onCopySelectedCellsAs={(format, options) => {
          void clipboard.copySelectedCellsAs(format, options);
        }}
        onPasteSelectedCells={handleMenuPasteSelectedCells}
        onCopyRowsAsNewRows={() => {
          void clipboard.copyRowsAsNewRows();
        }}
        onPasteRows={() => {
          void clipboard.pasteRowsAsDrafts();
        }}
        canPasteRows={clipboard.canUseRowCopy}
        columns={columnDefs}
        rows={rowArrays}
        totalRows={totalRows}
        totalRowsKind={totalRowsKind}
        onCountExact={handleCountExact}
        counting={counting}
        unstablePaging={unstablePaging}
        estimateUnavailable={estimateUnavailable}
        pagingHint={pagingHint}
        page={page}
        pageSize={pageSize}
        sorts={sorts}
        filters={filters}
        filterLogic={filterLogic}
        draftFilters={draftFilters}
        draftFilterLogic={draftFilterLogic}
        filterPanelOpen={filterPanelOpen}
        onFilterPanelOpenChange={actions.setFilterPanelOpen}
        // The DBR-04 filter modes: pane switching is render-only; applying is
        // an explicit act that resets the page and reloads (see the actions).
        filterMode={filterMode}
        onFilterModeChange={actions.setFilterMode}
        filterRawWhereSupported={rawWhereSupported}
        anyColumnValue={anyColumnDraft}
        onAnyColumnValueChange={actions.setAnyColumnDraft}
        onApplyAnyColumn={(value) => {
          const hints = databaseType
            ? (
                DB_REGISTRY[databaseType as DatabaseType] as
                  | { cellTypes?: import('../../lib/cellTypes').CellTypeHints }
                  | undefined
              )?.cellTypes
            : undefined;
          const expansion = expandAnyColumnFilter(columns, value, {
            groups: valueSearchTypeGroupsFor(databaseType ?? null),
            hints,
          });
          actions.applyAnyColumn(expansion);
        }}
        anyColumnSkippedCount={anyColumnReport.skippedColumns.length}
        anyColumnSearchableCount={anyColumnReport.searchableColumns.length}
        rawWhereValue={rawWhereDraft}
        onRawWhereValueChange={actions.setRawWhereDraft}
        onApplyRawWhere={actions.applyRawWhere}
        appliedRawWhere={rawWhere}
        // Where the predicate runs (DBR-04): the panel's own database and
        // schema, so the user can see the target the statement will read.
        filterRawWhereContext={
          rawWhereSupported
            ? [database || null, schema ?? null].filter(Boolean).join(' · ') || null
            : null
        }
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
        // DB-12: reorder, freeze and width persistence.
        onReorderColumns={handleReorderColumns}
        frozenColumns={frozenColumnCount}
        onFreezeThrough={handleFreezeThrough}
        onUnfreezeColumns={handleUnfreezeColumns}
        onColumnStats={handleColumnStats}
        persistedColumnWidths={persistedColumnWidths}
        onColumnWidthsCommit={handleColumnWidthsCommit}
        onRemoveFilter={actions.removeFilter}
        onClearFilters={actions.clearFilters}
        onPageChange={actions.setPage}
        onPageSizeChange={actions.setPageSize}
        onCellDoubleClick={handleCellDoubleClick}
        onCellEdit={cellWritesEnabled ? handleCellEdit : undefined}
        onCellEditCancel={actions.cancelEdit}
        onCellQuickLook={openCellQuickLook}
        cellEditing={editing.cellEditing}
        cellTypeHints={editing.cellTypeHints}
        enableSetNull={cellWritesEnabled}
        // Value presentation (issue-32 T05): the snapshot threads through,
        // the source is this panel's own table identity — and only when a
        // connectionId exists; without one there is no source to fake.
        presentationSnapshot={presentationSnapshot}
        presentationSource={
          connectionId
            ? {
                connectionId,
                database,
                namespace: schema ?? undefined,
                table: tableName,
              }
            : undefined
        }
        onRowSelect={actions.selectRow}
        onSelectAll={actions.toggleSelectAll}
        onRowClick={actions.setDetailRow}
        highlightedRow={detailRowIndex ?? highlightedNewRowIndex}
        dirtyByRow={dirtyIndex.byRowIndex}
        onRestoreRow={actions.unstageRowDelete}
        // The reverse foreign-key jump (DB-20): the row menu's referencing-rows entry.
        onShowReferencingRows={fkNavigation.onShowReferencingRows}
        // The foreign-key jump (DB-07): the arrows and the callback.
        foreignKeys={fkNavigation.foreignKeys}
        rowHeight={gridRowHeight}
        // The grid-appearance settings (DB-17): density above, the rest here.
        alternatingRows={gridAlternatingRows}
        showRowNumbers={gridRowNumbers}
        scrollbarAutoHide={gridAutoHideScrollbar}
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
        primaryKeyColumns={primaryKeyColumnNames}
        onDeleteRows={cellWritesEnabled ? handleDeleteRows : undefined}
        onPreviewSql={handleFilterPreviewSql}
        onExportFiltered={handleFilterExportFiltered}
        onBuildFilteredExportSql={buildFilteredExportSql}
        exportOpenSignal={exportSignal}
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
      {confirmBinaryLoadDialog}
      <PendingPlanDialog
        plan={previewPlan}
        open={previewOpen && previewPlan != null}
        onClose={() => setPreviewOpen(false)}
      />
      {datePick &&
        datePick.rowIndex < rows.length &&
        (() => {
          // The picker of the "Set Value" menu (DBR-05): the same canonical
          // strings the in-place editor writes, staged through the same path.
          // A zoned column never reaches here (resolveSetValueState keeps it
          // on the text path); the kind comes from the column's own cell type.
          const column = columns.find((c) => c.name === datePick.columnName);
          if (!column) return null;
          const info = editing.cellEditing?.columnEditorInfo.get(datePick.columnName);
          const kind: 'date' | 'datetime' = info?.cellType === 'datetime' ? 'datetime' : 'date';
          const raw = rows[datePick.rowIndex][datePick.columnName];
          return (
            <SetValueDateDialog
              open
              columnName={column.name}
              columnType={column.dataType}
              kind={kind}
              value={typeof raw === 'string' ? raw : ''}
              onApply={(next) => {
                // A cleared picker applies a SQL NULL — the same intent as an
                // editor emptied by hand, never an empty string in a date
                // column.
                useTableDataStore
                  .getState()
                  .stageCellValue(
                    panelId,
                    datePick.rowIndex,
                    datePick.columnName,
                    next === '' ? null : next,
                  );
              }}
              onClose={() => setDatePick(null)}
            />
          );
        })()}
      {quickLookCell &&
        quickLookCell.rowIndex < rows.length &&
        (() => {
          // The Quick Look of a grid cell (DBR-05): the column definition and the
          // value come from the same records the grid renders, so the dialog and
          // the cell cannot tell two different stories about one value.
          const column = columns.find((c) => c.name === quickLookCell.columnName);
          if (!column) return null;
          const value = rows[quickLookCell.rowIndex][quickLookCell.columnName];
          // The save exit needs the write path of the very cell: the panel's
          // editability and the column's own reason a cell cannot be edited.
          // Binary stays view-only here — its write entry is the file loader.
          const columnReadOnlyReason =
            editing.cellEditing?.columnEditorInfo.get(quickLookCell.columnName)?.readOnlyReason ??
            null;
          const canCommit = cellWritesEnabled && columnReadOnlyReason === null;
          return (
            <QuickLookDialog
              open
              column={{
                id: column.name,
                name: column.name,
                type: column.dataType,
                nullable: column.nullable,
              }}
              value={value}
              editable={canCommit}
              onCommit={
                canCommit
                  ? (next) => {
                      editing.cellEditing?.onCellValue?.(
                        quickLookCell.rowIndex,
                        quickLookCell.columnName,
                        next as Value | null | undefined,
                      );
                      setQuickLookCell(null);
                    }
                  : undefined
              }
              onClose={() => setQuickLookCell(null)}
            />
          );
        })()}
      <FilterSqlPreviewDialog
        open={sqlPreview.open}
        onClose={() => setSqlPreview((state) => ({ ...state, open: false }))}
        preview={sqlPreview.preview}
        error={sqlPreview.error}
      />
      <ColumnStatsDialog state={columnStatsState} onClose={dismissColumnStats} />
    </div>
  );
}
