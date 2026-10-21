import { useCallback, useEffect, useId, useMemo, useState, type KeyboardEvent } from 'react';
import { tid } from '../../lib/tid';
import { Download, Trash2 } from 'lucide-react';
import type { FilterCondition, SortCondition, Value } from '../../types';
import { useI18n } from '../../hooks/useI18n';
import { useColumnResize, adjustWidthsForSort } from '../../hooks/useColumnResize';
import { useGridSelection } from '../../hooks/useGridSelection';
import {
  EMPTY_GRID_SELECTION,
  columnsCoveredBySelection,
  gridSelectionCellCount,
  rowsCoveredBySelection,
  type GridSelection,
} from '../../stores/tableData/gridSelection';
import { ROW_MODE_GRID_SELECTION } from '../../stores/tableData/selectionActions';
import type { DirtyRowEntry } from '../../stores/tableData/dirtyIndex';
import type { FkGroupAffordance } from '../../lib/fkNavigation';
import { FilterBar } from '../FilterBar';
import { FilterEditor } from '../FilterEditor';
import { Pagination } from './Pagination';
import { TableHeader, type ColumnDef } from './TableHeader';
import { VirtualBody, type DraftRowViewModel } from './VirtualBody';
import { useDataTableContextMenu } from './useDataTableContextMenu';
import { DataExportDialog } from './DataExportDialog';
import { cn } from '../../lib/cn';
import { Spinner } from '../ui/Spinner';

export interface DataTableProps {
  columns: ColumnDef[];
  rows: unknown[][];

  totalRows?: number;
  page?: number;
  pageSize?: number;
  onPageChange?: (page: number) => void;
  onPageSizeChange?: (size: number) => void;

  sorts?: SortCondition[];
  onSort?: (sort: SortCondition) => void;

  filters?: FilterCondition[];
  filterLogic?: 'and' | 'or';
  draftFilters?: FilterCondition[];
  draftFilterLogic?: 'and' | 'or';
  filterPanelOpen?: boolean;
  onFilterPanelOpenChange?: (open: boolean) => void;
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
   * The foreign keys of the table, by column name (DB-07).
   *
   * Omitted = the table has no foreign keys (or the metadata could not be
   * read): the grid renders exactly what it rendered before.
   */
  fkIndex?: ReadonlyMap<string, FkGroupAffordance>;
  /**
   * The displayed column names, in render order.
   *
   * The jump arrow lands on the group's **first visible** member, so hiding a
   * column moves the arrow to the next member instead of losing it.
   */
  fkVisibleColumns?: readonly string[];
  /** The cells whose resolution is in flight, keyed `${rowIndex}:${columnName}`. */
  fkResolving?: ReadonlySet<string>;
  /** Follow the foreign key of a cell (DB-07). */
  onFkJump?: (rowIndex: number, columnName: string) => void;

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

/**
 * The height of one grid row, in pixels.
 *
 * It is stated here and handed to the grid by its owners, so the viewport
 * anchor of the foreign-key navigation (a row index) and the grid agree on
 * what a row is (DB-07).
 */
export const DATA_TABLE_ROW_HEIGHT = 28;

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
  editingCell,
  selectedRows = EMPTY_SET,
  loading,
  onSort,
  onRemoveFilter,
  onClearFilters,
  onAddFilter,
  onUpdateFilter,
  onFilterLogicChange,
  onApplyFilters,
  onPageChange,
  onPageSizeChange,
  onCellDoubleClick,
  onCellEdit,
  onCellEditCancel,
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
  fkIndex,
  fkVisibleColumns,
  fkResolving,
  onFkJump,
  statusBar,
  rowHeight = DATA_TABLE_ROW_HEIGHT,
  exportTableName,
  databaseType,
  dbSessionId,
  dataExportCapability,
  primaryKeyColumns,
  onDeleteRows,
  headerActions,
  emptyPlaceholder,
  className,
  getContextCellText,
  gridSelection,
  onGridSelectionChange,
  onScrollElementChange,
}: DataTableProps) {
  const { t } = useI18n();
  const [scrollEl, setScrollEl] = useState<HTMLDivElement | null>(null);
  const [exportOpen, setExportOpen] = useState(false);

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
  const { surfaceProps, isCellSelected, activeDescendantId } = useGridSelection({
    columnOrder: columnNames,
    rowCount: rows.length,
    selection,
    onChange: onGridSelectionChange ?? NOOP,
    scrollElement: scrollEl,
    enabled: hasCellSelection,
    editingCell: editingCell ?? null,
    idPrefix: gridIdPrefix,
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
    onExport: handleExport,
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
        <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface px-2 py-1.5">
          {hasSelection && (
            <label className="flex cursor-pointer items-center gap-1.5 text-xs text-fg-secondary hover:text-fg">
              <input
                type="checkbox"
                className="accent-accent"
                checked={rows.length > 0 && selectedRows.size === rows.length}
                ref={(el) => {
                  if (el) el.indeterminate = selectedRows.size > 0 && selectedRows.size < rows.length;
                }}
                onChange={onSelectAll}
              />
              {t('common.selectAll')}
            </label>
          )}
          {hasSelection && selectedRows.size > 0 && (
            <span className="text-xs text-fg-muted">
              {t('dataTable.selected')} {selectedRows.size} / {rows.length} {t('common.rows')}
            </span>
          )}
          {hasCellSelection && selection.mode === 'cell' && (
            <span
              {...tid('data-table-selection-summary')}
              data-dt-selection-rows={String(coveredRows?.length ?? 0)}
              data-dt-selection-cols={String(coveredColumns)}
              data-dt-selection-cells={String(coveredCells)}
              className="text-xs text-fg-muted"
            >
              {t('dataTable.selectionSummary', {
                rows: coveredRows?.length ?? 0,
                cols: coveredColumns,
              })}
            </span>
          )}
          <div className="ml-auto flex items-center gap-1.5">
            {headerActions}
            {onDeleteRows && deleteRowCount > 0 && (primaryKeyColumns?.length ?? 0) > 0 && (
              <button
                type="button"
                data-testid="data-table-delete-rows"
                className="flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-danger hover:bg-surface-raised hover:text-danger"
                onClick={() => onDeleteRows(deleteRowIndices)}
                title={t('dataTable.deleteRow')}
              >
                <Trash2 className="h-3 w-3" />
                {t('dataTable.deleteRow')}
              </button>
            )}
            {exportEnabled && (
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
            )}
          </div>
          {loading ? <span className="text-xs text-fg-muted">{t('common.loading')}</span> : null}
        </div>
      )}

      {statusBar}

      <div
        ref={setScrollEl}
        className="min-h-0 flex-1 overflow-auto"
        aria-busy={loading}
        aria-label={t('dataTable.tableLabel')}
        onContextMenu={onContextMenu}
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
              onFilterColumn={
                onAddFilter
                  ? (column) => onAddFilter({ column, operator: 'eq', value: '' })
                  : undefined
              }
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
              onCellDoubleClick={handleCellDoubleClick}
              onCellEdit={handleCellEdit}
              onCellEditCancel={handleCellEditCancel}
              onRowSelect={handleRowClick}
              onRowGutterSelect={handleGutterSelect}
              onRowClick={onRowClick}
              cellSelectionEnabled={hasCellSelection}
              isCellSelected={isCellSelected}
              activeCell={selection.mode === 'cell' ? selection.focus : null}
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
              fkIndex={fkIndex}
              fkVisibleColumns={fkVisibleColumns}
              fkResolving={fkResolving}
              onFkJump={onFkJump}
            />
            {loading ? (
              gridRow(
                <div
                  role="status"
                  aria-live="polite"
                  className="flex min-h-28 items-center justify-center gap-2 text-xs text-fg-muted"
                >
                  <Spinner size="lg" />
                  {t('dataTable.loading')}
                </div>,
              )
            ) : rows.length === 0 ? (
              gridRow(
                <div
                  role="status"
                  aria-live="polite"
                  className="flex min-h-28 flex-col items-center justify-center gap-1 px-4 text-center text-xs text-fg-muted"
                >
                  <span>{t('dataTable.empty')}</span>
                  <span className="text-fg-muted/80">{t('dataTable.emptyHint')}</span>
                </div>,
              )
            ) : null}
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
          dataExportCapability={dataExportCapability}
        />
      )}
    </div>
  );
}

export type { ColumnDef };
