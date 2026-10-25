import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from 'react';
import type {
  CountStrategy,
  FilterCondition,
  FilterOperator,
  SortCondition,
  Value,
} from '../../types';
import { useI18n } from '../../hooks/useI18n';
import { useColumnResize, adjustWidthsForSort } from '../../hooks/useColumnResize';
import { useGridSelection } from '../../hooks/useGridSelection';
import { useGridKeyboardNav } from '../../hooks/useGridKeyboardNav';
import {
  EMPTY_GRID_SELECTION,
  columnsCoveredBySelection,
  gridSelectionCellCount,
  rowsCoveredBySelection,
  type CellCoord,
  type GridSelection,
} from '../../stores/tableData/gridSelection';
import type { GridClipboardHandlers } from './gridClipboardBridge';
import { useDataTableClipboardDom } from './useDataTableClipboardDom';
import { ROW_MODE_GRID_SELECTION } from '../../stores/tableData/selectionActions';
import type { DirtyRowEntry } from '../../stores/tableData/dirtyIndex';
import type { FkGroupAffordance } from '../../lib/fkNavigation';
import { FilterBar } from '../FilterBar';
import { FilterEditor } from '../FilterEditor';
import { Pagination, type PagingHint } from './Pagination';
import { DefaultOrderSelect } from './DefaultOrderSelect';
import type { DefaultOrderOverride } from '../../lib/defaultSort';
import { TableHeader, type ColumnDef } from './TableHeader';
import { VirtualBody, type DraftRowViewModel } from './VirtualBody';
import { GridKeyboardHints } from './GridKeyboardHints';
import { useDataTableContextMenu } from './useDataTableContextMenu';
import { DataExportDialog } from './DataExportDialog';
import { DataTableSelectionBar } from './DataTableSelectionBar';
import type { CellEditingContext } from '../../lib/cellEditorKind';
import type { CellTypeHints } from '../../lib/cellTypes';
import { cn } from '../../lib/cn';
import { Spinner } from '../ui/Spinner';
import { DATA_TABLE_ROW_HEIGHT } from './rowHeight';

export interface DataTableProps {
  columns: ColumnDef[];
  rows: unknown[][];

  totalRows?: number;
  page?: number;
  pageSize?: number;
  onPageChange?: (page: number) => void;
  onPageSizeChange?: (size: number) => void;
  /**
   * How much the row count may be trusted: exact, estimated or unavailable.
   *
   * The grid renders the state the back end reported, never a guess of its
   * own — an estimate is shown as "about N" and a database that cannot count
   * says so instead of showing zero.
   */
  totalRowsKind?: CountStrategy | null;
  /** Ask for the exact count. Offered in the estimated state only. */
  onCountExact?: () => void;
  /** An exact count is being fetched right now. */
  counting?: boolean;
  /** No order is in force: rows across pages may repeat or be skipped. */
  unstablePaging?: boolean;
  /**
   * The user asked for an estimate and this database cannot give one.
   *
   * The grid knows it on its own — the driver declared it, or the answer came
   * back without an estimate — so it says so directly instead of waiting for
   * an error message that would never arrive.
   */
  estimateUnavailable?: boolean;
  /**
   * What the back end said about the paging of this page.
   *
   * A degradation is not an error: the page was read by offset, and this
   * says why. The grid renders the reason the back end gave.
   */
  pagingHint?: PagingHint;
  /** The panel's temporary override of the global default order. */
  defaultOrderOverride?: DefaultOrderOverride;
  /** Change the panel's temporary override of the global default order. */
  onDefaultOrderOverrideChange?: (value: DefaultOrderOverride) => void;

  sorts?: SortCondition[];
  onSort?: (sort: SortCondition) => void;
  /* ── Multi-column sort (DB-11). Omitted ⇒ the header context menu keeps
     its pre-DB-11 shape — the result grids and driver grids are unaffected. ── */
  /** Add (or re-aim) one column of the sort list, from the header menu. */
  onAddSort?: (sort: SortCondition) => void;
  /** Remove one column from the sort list, from the header menu. */
  onRemoveSort?: (column: string) => void;
  /** Drop the whole sort list, from the header menu. */
  onClearSorts?: () => void;
  /* ── Column tools (DB-12). Omitted ⇒ the grid renders and behaves exactly
     as before — drag, freeze and width persistence all opt in. ── */
  /** Reorder columns by dragging a header onto another one (displayed indexes). */
  onReorderColumns?: (from: number, to: number) => void;
  /** How many leading columns are pinned while the grid scrolls. */
  frozenColumns?: number;
  /* ── Grid appearance (DB-17). Every prop defaults to the grid's behaviour
     before the setting existed, so a host that passes none keeps today's DOM
     byte for byte. ── */
  /** Alternating row backgrounds; default `true` (odd rows tinted, as always). */
  alternatingRows?: boolean;
  /** The row-number gutter; default `true` (the `#` column, as always). */
  showRowNumbers?: boolean;
  /** Scrollbars appear only while the grid is hovered; default `false`. */
  scrollbarAutoHide?: boolean;
  /** Freeze through the right-clicked column (that many leading columns). */
  onFreezeThrough?: (throughIndex: number) => void;
  /** Unfreeze every column. */
  onUnfreezeColumns?: () => void;
  /** Ask for the server-side aggregates of the right-clicked column (DB-12). */
  onColumnStats?: (column: string) => void;
  /**
   * The widths this table was last left with, by column name: they seed the
   * grid so a resized column keeps its width across reorders and restarts.
   */
  persistedColumnWidths?: Record<string, number> | null;
  /** Persist the effective column widths, by name, after a resize drag. */
  onColumnWidthsCommit?: (widths: Record<string, number>) => void;

  filters?: FilterCondition[];
  filterLogic?: 'and' | 'or';
  draftFilters?: FilterCondition[];
  draftFilterLogic?: 'and' | 'or';
  filterPanelOpen?: boolean;
  onFilterPanelOpenChange?: (open: boolean) => void;
  /**
   * The filter operators this database serves. Omit it and the shared set is
   * used; a driver that declares fewer operators only offers those.
   */
  filterOperators?: readonly FilterOperator[];
  /**
   * The driver's own column-type rules (`DatabaseTypeMeta.cellTypes`).
   *
   * The filter editor needs them for the same reason the cell editor does:
   * the operator list follows the column type, and a column the driver
   * calls a boolean must not be filtered with the operators of a number
   * (08 §3.5, "一处声明、两处生效").
   */
  cellTypeHints?: CellTypeHints;
  /**
   * Whether this database ignores case in `LIKE` under its default collation.
   * Copy only — it never changes the query.
   */
  filterLikeCaseInsensitive?: boolean;
  onRemoveFilter?: (index: number) => void;
  onClearFilters?: () => void;
  onAddFilter?: (filter: FilterCondition) => void;
  onUpdateFilter?: (index: number, filter: FilterCondition) => void;
  onFilterLogicChange?: (logic: 'and' | 'or') => void;
  onApplyFilters?: () => void;

  editingCell?: { row: number; col: string } | null;
  onCellDoubleClick?: (row: number, col: string) => void;
  onCellEdit?: (row: number, col: string, value: unknown) => void;
  onCellEditCancel?: () => void;
  /**
   * The editing context of the grid (DB-05).
   *
   * It carries the per-column editor facts, the read-only reason, the write
   * path and the "Set Value" entries — one object, because the grid threads
   * it through three components. Omitted → the grid keeps the behaviour it
   * had before DB-05.
   */
  cellEditing?: CellEditingContext;
  /**
   * Show “Set NULL” in the context menu. Defaults to true when `onCellEdit` is set.
   * Pass false for read-only grids that only use `onCellEdit` to dismiss an editor (e.g. query results).
   */
  enableSetNull?: boolean;

  selectedRows?: Set<number>;
  onRowSelect?: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  onSelectAll?: () => void;

  /**
   * Cell/region selection (controlled). Omit it and the grid keeps today's DOM: no
   * `data-dt-surface`, no ARIA grid roles, no selection attributes (01-selection §3.8).
   */
  gridSelection?: GridSelection;
  /** Selection write-back. Paired with `gridSelection`; without it the grid only renders. */
  onGridSelectionChange?: (next: GridSelection) => void;
  /** Keyboard navigation (DB-02). Omitted ⇒ no key is intercepted at all. */
  keyboardNavigation?: boolean;
  /**
   * The clipboard bridge (DB-03).
   *
   * **Omitted ⇒ the grid binds no `copy` / `paste` listener at all**, which is exactly
   * today's behaviour; every consumer that does not opt in keeps byte-identical behaviour.
   */
  clipboard?: GridClipboardHandlers;
  /**
   * Cells the owner wants marked as rejected, keyed `${rowIndex}:${columnName}`.
   *
   * Rendered as `data-dt-cell-error`; omitted ⇒ no attribute (today's DOM).
   */
  cellErrors?: ReadonlySet<string>;
  /** Scroll the cursor cell into view (volume 10's hook point). */
  onRequestScrollIntoView?: (coord: CellCoord) => void;
  /** The anchor of the row selection (`TableState.lastSelectedIndex`), read-only. */
  lastSelectedIndex?: number | null;

  /** Row clicked (single click) — used by parent to track detail panel row */
  onRowClick?: (index: number) => void;

  /** Highlighted row index (e.g. for detail panel) */
  highlightedRow?: number | null;

  /**
   * New-row drafts, rendered after the real rows. They are derived rows: they
   * are not part of `rows` and never reach the selection model (04 §3.7 R-5).
   */
  draftRows?: DraftRowViewModel[];
  /** Write one cell of a draft. `undefined` clears the column (do not write it). */
  onDraftCellEdit?: (draftId: string, column: string, value: Value | null | undefined) => void;
  /** Start editing a draft cell. */
  onDraftCellEditStart?: (draftId: string, column: string) => void;
  /** Leave the draft cell editor without writing anything. */
  onDraftCellEditCancel?: () => void;
  /** Drop a draft row. */
  onDraftRowRemove?: (draftId: string) => void;
  /** The draft row the pointer focus is on. */
  onDraftRowActivate?: (draftId: string) => void;
  /** Append a new draft row — the "empty area below the last row" entry. */
  onAddDraftRow?: () => void;
  /** The draft cell that is being edited right now. */
  editingDraft?: { draftId: string; column: string } | null;
  /**
   * The row the last insert produced; it gets a marker with a word on it.
   *
   * A separate channel from `highlightedRow` (that one is shared with the
   * detail panel). `null` — the default — renders no marker, so a grid that
   * does not use the feature keeps today's DOM.
   */
  newRowIndex?: number | null;

  /**
   * The dirty state of the current page, keyed by row index (DB-06).
   *
   * Omitted / `null` = no dirty marks at all: the grid renders exactly what
   * it rendered before the dirty state existed (00-contracts R4).
   */
  dirtyByRow?: ReadonlyMap<number, DirtyRowEntry> | null;
  /**
   * "Restore This Row" — undo the delete mark of a row. Omitted = the context
   * menu does not offer it.
   */
  onRestoreRow?: (rowIndex: number) => void;
  /**
   * Open the tabs of the rows that reference the right-clicked row (DB-20).
   * Omitted ⇒ the row menu keeps its pre-DB-20 shape.
   */
  onShowReferencingRows?: (rowIndex: number) => void;

  /**
   * The foreign keys of the table (DB-07).
   *
   * Omitted = the table has no foreign keys (or the metadata could not be
   * read): the grid renders exactly what it rendered before.
   *
   * It is **one object** and not four loose props because the pieces belong
   * together: the arrow of a composite key is decided by the values of
   * **all** of its members — including a member the user has hidden — and
   * only the **full** row records carry those values (07 §7 #14). A grid that
   * got the index but not the records would silently lose the arrow of every
   * composite key with a hidden member, so the type makes that combination
   * impossible to write (FKU-D1).
   */
  foreignKeys?: {
    /** The columns that take part in a foreign key, mapped to their group. */
    index: ReadonlyMap<string, FkGroupAffordance>;
    /** The **full** row records (every column of the table), by row index. */
    rows: readonly Record<string, unknown>[];
    /** The displayed column names, in render order. */
    visibleColumns: readonly string[];
    /** The cells whose resolution is in flight, keyed `${rowIndex}:${columnName}`. */
    resolving?: ReadonlySet<string>;
    /** Follow the foreign key of a cell. */
    onJump?: (rowIndex: number, columnName: string, element: HTMLElement | null) => void;
  };

  loading?: boolean;
  statusBar?: React.ReactNode;
  rowHeight?: number;

  /** Enable data export (button + context menu). Provide a table name for the filename. */
  exportTableName?: string;
  databaseType?: string;
  /** Live DB session — enables entire-table streaming export. */
  dbSessionId?: string;
  /**
   * Data-export capability, threaded to the export dialog. Omit for `full_table`.
   * `none` disables the table-data export button entirely.
   */
  dataExportCapability?: 'none' | 'loaded_only' | 'full_table';
  /** Primary-key column names for Copy as UPDATE / Delete Row; falls back to first column for copy. */
  primaryKeyColumns?: string[];
  /** Delete rows by page-row indices (requires primary keys). */
  onDeleteRows?: (rowIndices: number[]) => void;

  /**
   * Show the statements this read would send (DB-15). Absent — a panel that
   * cannot reach the backend's preview — no `SQL` button appears in the filter
   * panel.
   */
  onPreviewSql?: () => void;
  /**
   * The filter panel's direct exit to the export dialog with "current filter
   * results" preselected (DB-15). Absent — a panel whose filtered export
   * cannot be built — no `Export…` button appears there.
   */
  onExportFiltered?: () => void;
  /**
   * The builder of the filtered full-table export statement (DB-15).
   *
   * Forwarded to the export dialog; when it exists the "current filter
   * results" scope is offered. The statement is the backend's own preview —
   * the export streams what the filters really select.
   */
  onBuildFilteredExportSql?: () => Promise<string>;
  /**
   * Request opening the export dialog from outside (DB-15): a counter, not a
   * boolean — each increment opens the dialog once, with the filtered scope
   * preselected.
   */
  exportOpenSignal?: number;

  /** Extra action buttons shown on the right side of the selection toolbar, next to export. */
  headerActions?: React.ReactNode;
  /** Custom empty content when columns are hidden or table has custom empty state. */
  emptyPlaceholder?: React.ReactNode;

  /** Optional layout classes for embedding the table in a composed surface. */
  className?: string;

  /**
   * Optional cell text for the native context menu “copy” item.
   * When omitted, uses the right-clicked cell value (preferred) or `window.getSelection()`.
   */
  getContextCellText?: () => string | null | undefined;

  /**
   * The scroll container of the grid, handed to its owner (DB-07).
   *
   * The owner needs it to snapshot the viewport when the tab is left and to
   * restore it when the tab is entered again. It is a callback and not a ref
   * because the element only exists after the first render.
   */
  onScrollElementChange?: (element: HTMLDivElement | null) => void;
}

const NOOP = () => {};
const EMPTY_SET = new Set<number>();
const EMPTY_SORTS: SortCondition[] = [];
const EMPTY_FILTERS: FilterCondition[] = [];

export function DataTable({
  columns,
  rows,
  totalRows,
  page,
  pageSize,
  sorts = EMPTY_SORTS,
  filters = EMPTY_FILTERS,
  filterLogic = 'and',
  draftFilters = EMPTY_FILTERS,
  draftFilterLogic = 'and',
  filterPanelOpen = false,
  onFilterPanelOpenChange,
  filterOperators,
  cellTypeHints,
  filterLikeCaseInsensitive,
  editingCell,
  clipboard,
  cellErrors,
  selectedRows = EMPTY_SET,
  loading,
  onSort,
  onAddSort,
  onRemoveSort,
  onClearSorts,
  onReorderColumns,
  frozenColumns = 0,
  alternatingRows = true,
  showRowNumbers = true,
  scrollbarAutoHide = false,
  onFreezeThrough,
  onUnfreezeColumns,
  onColumnStats,
  persistedColumnWidths,
  onColumnWidthsCommit,
  onRemoveFilter,
  onClearFilters,
  onAddFilter,
  onUpdateFilter,
  onFilterLogicChange,
  onApplyFilters,
  onPageChange,
  onPageSizeChange,
  totalRowsKind,
  onCountExact,
  counting,
  unstablePaging,
  estimateUnavailable,
  pagingHint,
  defaultOrderOverride,
  onDefaultOrderOverrideChange,
  onCellDoubleClick,
  onCellEdit,
  onCellEditCancel,
  cellEditing,
  enableSetNull,
  onRowSelect,
  onSelectAll,
  onRowClick,
  highlightedRow,
  draftRows,
  onDraftCellEdit,
  onDraftCellEditStart,
  onDraftCellEditCancel,
  onDraftRowRemove,
  onDraftRowActivate,
  onAddDraftRow,
  editingDraft,
  newRowIndex,
  dirtyByRow,
  onRestoreRow,
  onShowReferencingRows,
  foreignKeys,
  statusBar,
  rowHeight = DATA_TABLE_ROW_HEIGHT,
  exportTableName,
  databaseType,
  dbSessionId,
  dataExportCapability,
  primaryKeyColumns,
  onDeleteRows,
  onPreviewSql,
  onExportFiltered,
  onBuildFilteredExportSql,
  exportOpenSignal,
  headerActions,
  emptyPlaceholder,
  className,
  getContextCellText,
  gridSelection,
  onGridSelectionChange,
  keyboardNavigation,
  onRequestScrollIntoView,
  lastSelectedIndex,
  onScrollElementChange,
}: DataTableProps) {
  const { t } = useI18n();
  const [scrollEl, setScrollEl] = useState<HTMLDivElement | null>(null);
  const [exportOpen, setExportOpen] = useState(false);
  /**
   * The scope the dialog opens with (DB-15): the filter panel's direct exit
   * preselects "current filter results"; the toolbar's own button opens the
   * dialog the way it always did.
   */
  const [exportInitialScope, setExportInitialScope] = useState<
    'current_page' | 'filtered_table' | undefined
  >(undefined);
  const signalSeenRef = useRef(0);

  // The counter is a signal, not state: each increment opens the dialog once,
  // and a replayed old value never re-opens it.
  useEffect(() => {
    if (exportOpenSignal == null || exportOpenSignal <= signalSeenRef.current) return;
    signalSeenRef.current = exportOpenSignal;
    setExportInitialScope(onBuildFilteredExportSql ? 'filtered_table' : undefined);
    setExportOpen(true);
  }, [exportOpenSignal, onBuildFilteredExportSql]);

  // The owner of the grid keeps the viewport (DB-07): it has to know where the
  // user was when the tab was left.
  useEffect(() => {
    onScrollElementChange?.(scrollEl);
  }, [scrollEl, onScrollElementChange]);

  const colMeta = useMemo(() => columns.map((c) => ({ name: c.name, type: c.type })), [columns]);
  const { columnWidths: baseWidths, onResizeStart } = useColumnResize({
    count: columns.length,
    columns: colMeta,
    rows,
    // DB-12: widths belong to columns, so they are seeded by name and
    // reported back by name after a drag.
    persistedWidths: persistedColumnWidths,
    onWidthsCommit: onColumnWidthsCommit,
  });
  const columnWidths = useMemo(
    () => adjustWidthsForSort(baseWidths, columns, sorts),
    [baseWidths, columns, sorts],
  );

  const handleRowClick = useCallback(
    (index: number, opts?: { multi?: boolean; range?: boolean }) => {
      // A click on the row background selects the whole row — and only that. The grid
      // never invents a row-mode selection here: a consumer that owns the row set must
      // keep owning it, or `setGridSelection` would clear the consumer's own rows.
      onRowClick?.(index);
      onRowSelect?.(index, opts);
    },
    [onRowClick, onRowSelect],
  );

  const handleGutterSelect = useCallback(
    (index: number, opts?: { multi?: boolean; range?: boolean }) => {
      onRowClick?.(index);
      if (onRowSelect) {
        // The consumer owns the row-mode transition: the store writes the mode and the row
        // set together (00-contracts §4.2), so the grid must not write a second one.
        onRowSelect(index, opts);
        return;
      }
      // No consumer-side row path: the grid owns the mode, and `row` mode carries no
      // geometry at all (00-contracts §4.2, I2).
      onGridSelectionChange?.(ROW_MODE_GRID_SELECTION);
    },
    [onGridSelectionChange, onRowClick, onRowSelect],
  );

  const handleCellDoubleClick = useCallback(
    (row: number, col: string) => onCellDoubleClick?.(row, col),
    [onCellDoubleClick],
  );
  const handleCellEdit = useCallback(
    (row: number, col: string, value: unknown) => onCellEdit?.(row, col, value),
    [onCellEdit],
  );
  const handleCellEditCancel = useCallback(() => onCellEditCancel?.(), [onCellEditCancel]);

  const exportEnabled = !!exportTableName && rows.length > 0 && dataExportCapability !== 'none';
  const columnNames = useMemo(() => columns.map((c) => c.name), [columns]);
  const hasSelection = onSelectAll != null && onRowSelect != null;
  /** Cell selection is a controlled pair: both props or neither (01-selection §3.8). */
  const hasCellSelection = gridSelection != null && onGridSelectionChange != null;
  const selection = gridSelection ?? EMPTY_GRID_SELECTION;
  /** Prefix of the cell ids the grid's `aria-activedescendant` points at. */
  const gridIdPrefix = useId();
  /** The rows one screen holds; the keyboard hook never measures the viewport (02 §K-10). */
  const viewportRowCount = scrollEl
    ? Math.max(1, Math.floor(scrollEl.clientHeight / rowHeight))
    : 1;
  const keyboardNav = useGridKeyboardNav({
    columnOrder: columnNames,
    rowCount: rows.length,
    selection,
    lastSelectedIndex: lastSelectedIndex ?? null,
    editing: editingCell != null,
    viewportRowCount,
    enabled: keyboardNavigation === true && hasCellSelection,
    onGridSelectionChange: onGridSelectionChange ?? NOOP,
    onCellDoubleClick: handleCellDoubleClick,
    onRowGutterSelect: handleGutterSelect,
    onToggleSelectAll: onSelectAll ?? NOOP,
    onRequestScrollIntoView,
  });
  /** The cell that carries `data-dt-active` (02): the keyboard cursor. */
  const activeCell =
    keyboardNav.activeCoord ?? (selection.mode === 'cell' ? selection.focus : null);
  const { surfaceProps, isCellSelected, activeDescendantId } = useGridSelection({
    columnOrder: columnNames,
    rowCount: rows.length,
    selection,
    onChange: onGridSelectionChange ?? NOOP,
    scrollElement: scrollEl,
    enabled: hasCellSelection,
    editingCell: editingCell ?? null,
    idPrefix: gridIdPrefix,
    onKeyboardNav: keyboardNav.handleKeyDown,
  });
  /**
   * The grid surface. The scroll container is the ARIA grid host and the keyboard focus
   * owner (00-contracts §4.4); the row count includes the header row and the column count
   * includes the row-number column, exactly like the rendered header.
   *
   * Rendered only when the cell-selection contract is on, so a grid without it keeps
   * today's DOM.
   */
  const gridRowCount = emptyPlaceholder
    ? 1
    : rows.length + 1 + (loading || rows.length === 0 ? 1 : 0);
  const surfaceDomProps = hasCellSelection
    ? {
        'data-dt-surface': '',
        role: 'grid' as const,
        tabIndex: 0,
        'aria-rowcount': gridRowCount,
        'aria-colcount': columnNames.length + 1,
        'aria-activedescendant': activeDescendantId ?? undefined,
      }
    : {};
  /**
   * Wrap non-row content (the status overlays, the caller-provided empty placeholder) in a
   * row + gridcell: an ARIA grid may only contain rows, so a bare `role="status"` or a
   * placeholder div would be invalid content. The live-region semantics of the status
   * layer stay untouched inside the cell — a screen reader still announces it.
   */
  const gridRow = (content: React.ReactNode) => (
    <div role={hasCellSelection ? 'row' : undefined}>
      <div role={hasCellSelection ? 'gridcell' : undefined}>{content}</div>
    </div>
  );
  /** The `copy` / `paste` binding; `{}` when the owner passed no bridge (DB-03). */
  const clipboardDomHandlers = useDataTableClipboardDom({
    clipboard,
    editingCell: editingCell ?? null,
    enabled: hasCellSelection,
    columnNames,
    selection,
    selectedRows,
  });
  /**
   * Rows a cell selection covers. Derived, never written back into `selectedRows`
   * (00-contracts §4.2, rule 3): in cell mode the row set is empty by contract.
   */
  const coveredRows = useMemo(() => {
    if (!hasCellSelection || selection.mode !== 'cell') return null;
    return Array.from(rowsCoveredBySelection({ ...selection, selectedRows })).sort((a, b) => a - b);
  }, [hasCellSelection, selectedRows, selection]);
  const coveredColumns = useMemo(
    () => (hasCellSelection ? columnsCoveredBySelection(selection, columnNames) : 0),
    [columnNames, hasCellSelection, selection],
  );
  const coveredCells = useMemo(
    () => (hasCellSelection ? gridSelectionCellCount(selection, columnNames) : 0),
    [columnNames, hasCellSelection, selection],
  );
  /** Row indices a delete action acts on: the covered rows in cell mode, else the row set. */
  const deleteRowIndices = coveredRows ?? Array.from(selectedRows).sort((a, b) => a - b);
  const deleteRowCount = coveredRows?.length ?? selectedRows.size;

  const handleExport = useCallback(() => setExportOpen(true), []);
  const { onContextMenu } = useDataTableContextMenu({
    columnNames,
    rows,
    selectedRows,
    loading,
    exportTableName,
    exportEnabled,
    databaseType,
    primaryKeyColumns,
    enableSetNull,
    getContextCellText,
    onAddFilter,
    onCellEdit,
    onDeleteRows,
    dirtyByRow,
    onRestoreRow,
    onShowReferencingRows,
    onExport: handleExport,
    setValueState: cellEditing?.setValueState,
    onSetValue: cellEditing?.onSetValue,
    onLoadCandidates: cellEditing?.onLoadCandidates,
    // DB-11: the sorts in force and their header-menu actions.
    sorts,
    onAddSort,
    onRemoveSort,
    onClearSorts,
    // DB-12: the freeze entries and the stats entry of the header menu.
    frozenColumns,
    onFreezeThrough,
    onUnfreezeColumns,
    onColumnStats,
    t,
  });

  const hasPagination =
    page != null && pageSize != null && totalRows != null && onPageChange && onPageSizeChange;
  const hasFilters = filters.length > 0 && onRemoveFilter && onClearFilters;
  const hasFilterEditor =
    onAddFilter &&
    onUpdateFilter &&
    onRemoveFilter &&
    onClearFilters &&
    onFilterLogicChange &&
    onApplyFilters &&
    onFilterPanelOpenChange;

  const handleDeleteKey = useCallback(
    (e: KeyboardEvent<HTMLDivElement>) => {
      if (editingCell) return;
      if (e.key !== 'Delete' && e.key !== 'Backspace') return;
      if (!onDeleteRows || (primaryKeyColumns?.length ?? 0) === 0) return;
      // Cell mode keeps `selectedRows` empty on purpose, so the Delete key stays a
      // whole-row action (01-selection §4.3). The toolbar button is the explicit way to
      // delete the rows a cell region covers.
      if (selectedRows.size === 0) return;
      e.preventDefault();
      onDeleteRows(Array.from(selectedRows).sort((a, b) => a - b));
    },
    [editingCell, onDeleteRows, primaryKeyColumns, selectedRows],
  );

  return (
    <div
      className={cn(
        'selectable flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-md border border-edge bg-surface',
        className,
      )}
      onKeyDown={handleDeleteKey}
    >
      {hasFilterEditor ? (
        <FilterEditor
          columns={columns}
          supportedOperators={filterOperators}
          cellTypeHints={cellTypeHints}
          likeCaseInsensitive={filterLikeCaseInsensitive}
          appliedFilters={filters}
          appliedLogic={filterLogic}
          draftFilters={draftFilters}
          draftLogic={draftFilterLogic}
          open={filterPanelOpen}
          onOpenChange={onFilterPanelOpenChange}
          onLogicChange={onFilterLogicChange}
          onChange={onUpdateFilter}
          onAdd={onAddFilter}
          onRemove={onRemoveFilter}
          onApply={onApplyFilters}
          onClear={onClearFilters}
          loading={loading}
          onPreviewSql={onPreviewSql}
          onExportFiltered={onExportFiltered}
        />
      ) : hasFilters ? (
        <FilterBar
          filters={filters}
          onRemove={onRemoveFilter}
          onClear={onClearFilters}
          loading={loading}
        />
      ) : null}

      {(hasSelection || hasCellSelection) && (
        <DataTableSelectionBar
          hasSelection={hasSelection}
          hasCellSelection={hasCellSelection}
          selection={selection}
          selectedRows={selectedRows}
          rowCount={rows.length}
          coveredRows={coveredRows}
          coveredColumns={coveredColumns}
          coveredCells={coveredCells}
          onSelectAll={onSelectAll ?? NOOP}
          headerActions={headerActions}
          onDeleteRows={onDeleteRows}
          deleteRowIndices={deleteRowIndices}
          deleteRowCount={deleteRowCount}
          primaryKeyColumns={primaryKeyColumns}
          exportEnabled={exportEnabled}
          onExport={() => setExportOpen(true)}
          loading={loading}
          t={t}
        />
      )}

      {statusBar}

      {keyboardNavigation && hasCellSelection ? (
        <GridKeyboardHints id={`${gridIdPrefix}-shortcuts`} />
      ) : null}

      <div
        ref={setScrollEl}
        className={cn(
          'min-h-0 flex-1 overflow-auto',
          // DB-17: the bars appear only while the grid is hovered/focused.
          scrollbarAutoHide && 'dt-scrollbar-autohide',
        )}
        aria-busy={loading}
        aria-label={hasCellSelection ? t('tableView.a11y.gridLabel') : t('dataTable.tableLabel')}
        aria-describedby={
          keyboardNavigation && hasCellSelection ? `${gridIdPrefix}-shortcuts` : undefined
        }
        onContextMenu={onContextMenu}
        {...clipboardDomHandlers}
        {...surfaceDomProps}
        {...(hasCellSelection ? surfaceProps : {})}
      >
        {emptyPlaceholder ? (
          gridRow(emptyPlaceholder)
        ) : (
          <>
            <TableHeader
              columns={columns}
              sorts={sorts}
              onSort={onSort ?? NOOP}
              columnWidths={columnWidths}
              onResizeStart={onResizeStart}
              sortable={onSort != null}
              gridSemantics={hasCellSelection}
              // DB-12: drag-to-reorder and the freeze are opt-in.
              onReorderColumns={onReorderColumns}
              frozenColumns={frozenColumns}
              // DB-17: the gutter is the setting's to drop.
              showRowNumbers={showRowNumbers}
              onFilterColumn={
                onAddFilter
                  ? (column) => onAddFilter({ column, operator: 'eq', value: '' })
                  : undefined
              }
              cellTypeHints={cellTypeHints}
            />
            <VirtualBody
              columns={columns}
              rows={rows}
              rowHeight={rowHeight}
              editingCell={editingCell ?? null}
              selectedRows={selectedRows}
              highlightedRow={highlightedRow}
              scrollElement={scrollEl}
              columnWidths={columnWidths}
              frozenColumns={frozenColumns}
              // DB-17: the two row-appearance settings, threaded through.
              alternatingRows={alternatingRows}
              showRowNumbers={showRowNumbers}
              onCellDoubleClick={handleCellDoubleClick}
              onCellEdit={handleCellEdit}
              onCellEditCancel={handleCellEditCancel}
              cellEditing={cellEditing}
              cellTypeHints={cellTypeHints}
              onRowSelect={handleRowClick}
              onRowGutterSelect={handleGutterSelect}
              onRowClick={onRowClick}
              cellSelectionEnabled={hasCellSelection}
              isCellSelected={isCellSelected}
              activeCell={activeCell}
              cellIdPrefix={hasCellSelection ? gridIdPrefix : undefined}
              draftRows={draftRows}
              onDraftCellEdit={onDraftCellEdit}
              onDraftCellEditStart={onDraftCellEditStart}
              onDraftCellEditCancel={onDraftCellEditCancel}
              onDraftRowRemove={onDraftRowRemove}
              onDraftRowActivate={onDraftRowActivate}
              onAddDraftRow={onAddDraftRow}
              editingDraft={editingDraft}
              newRowIndex={newRowIndex ?? null}
              dirtyByRow={dirtyByRow}
              foreignKeys={foreignKeys}
              cellErrors={cellErrors}
            />
            {loading
              ? gridRow(
                  <div
                    role="status"
                    aria-live="polite"
                    className="flex min-h-28 items-center justify-center gap-2 text-xs text-fg-muted"
                  >
                    <Spinner size="lg" />
                    {t('dataTable.loading')}
                  </div>,
                )
              : rows.length === 0
                ? gridRow(
                    <div
                      role="status"
                      aria-live="polite"
                      className="flex min-h-28 flex-col items-center justify-center gap-1 px-4 text-center text-xs text-fg-muted"
                    >
                      <span>{t('dataTable.empty')}</span>
                      <span className="text-fg-muted/80">{t('dataTable.emptyHint')}</span>
                    </div>,
                  )
                : null}
          </>
        )}
      </div>

      {hasPagination && (
        <Pagination
          page={page}
          pageSize={pageSize}
          totalRows={totalRows}
          onPageChange={onPageChange}
          onPageSizeChange={onPageSizeChange}
          loading={loading}
          totalRowsKind={totalRowsKind}
          rowCount={rows.length}
          hasMore={rows.length === pageSize}
          onCountExact={onCountExact}
          counting={counting}
          unstablePaging={unstablePaging}
          estimateUnavailable={estimateUnavailable}
          pagingHint={pagingHint}
          footerExtras={
            onDefaultOrderOverrideChange ? (
              <DefaultOrderSelect
                value={defaultOrderOverride ?? 'inherit'}
                onChange={onDefaultOrderOverrideChange}
                primaryKeyCount={primaryKeyColumns?.length ?? 0}
              />
            ) : undefined
          }
        />
      )}

      {exportOpen && (
        <DataExportDialog
          open={exportOpen}
          onClose={() => setExportOpen(false)}
          columns={columns}
          rows={rows}
          selectedRows={selectedRows}
          tableName={exportTableName}
          databaseType={databaseType}
          dbSessionId={dbSessionId}
          totalRows={totalRows}
          primaryKeyColumns={primaryKeyColumns}
          initialScope={exportInitialScope}
          onBuildFilteredExportSql={onBuildFilteredExportSql}
          dataExportCapability={dataExportCapability}
        />
      )}
    </div>
  );
}

export type { ColumnDef };
export type { GridClipboardHandlers, GridCopyTarget } from './gridClipboardBridge';
