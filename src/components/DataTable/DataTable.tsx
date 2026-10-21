import { useCallback, useId, useMemo, useState, type KeyboardEvent } from 'react';
import { tid } from '../../lib/tid';
import { Download, Trash2 } from 'lucide-react';
import type { DatabaseType, FilterCondition, SortCondition } from '../../types';
import type { CellEdit } from '../../stores/tableData/types';
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
import {
  buildDataTableContextMenuItems,
  formatRowAsSqlInsert,
  formatRowAsSqlUpdate,
  formatRowsAsSqlInsert,
  formatRowsAsSqlUpdate,
  resolveDataTableCellFromEvent,
  resolveDataTableHeaderColFromEvent,
  rowToNamedRecord,
  serializeDataTableColumnValues,
  serializeDataTableRowsAsCsv,
  serializeDataTableRowsAsJson,
  serializeDataTableRowsAsTsv,
} from '../../lib/dataTableContextMenu';
import { showNativeContextMenu } from '../../lib/nativeContextMenu';
import { FilterBar } from '../FilterBar';
import { FilterEditor } from '../FilterEditor';
import { Pagination } from './Pagination';
import { TableHeader, type ColumnDef } from './TableHeader';
import { VirtualBody } from './VirtualBody';
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
  editBuffer?: Map<string, CellEdit>;
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
  statusBar,
  rowHeight = 28,
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
}: DataTableProps) {
  const { t } = useI18n();
  const [scrollEl, setScrollEl] = useState<HTMLDivElement | null>(null);
  const [exportOpen, setExportOpen] = useState(false);

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
  const surfaceDomProps = hasCellSelection
    ? {
        'data-dt-surface': '',
        role: 'grid' as const,
        tabIndex: 0,
        'aria-rowcount': rows.length + 1,
        'aria-colcount': columnNames.length + 1,
        'aria-activedescendant': activeDescendantId ?? undefined,
      }
    : {};
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

  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const hit = resolveDataTableCellFromEvent(e.target);
      const hitHeaderCol = !hit ? resolveDataTableHeaderColFromEvent(e.target) : null;
      const effectiveColName = hit ? hit.columnName : hitHeaderCol;
      const effectiveColIdx = effectiveColName ? columnNames.indexOf(effectiveColName) : -1;
      const hitRow = hit ? rows[hit.rowIndex] : undefined;
      const hitColIdx = hit ? columnNames.indexOf(hit.columnName) : -1;
      const hitCellValue = hitRow && hitColIdx >= 0 ? (hitRow[hitColIdx] ?? null) : undefined;

      const fromProp = getContextCellText?.() ?? '';
      const fromSelection = window.getSelection()?.toString() ?? '';
      const cellTextForCopy =
        hitCellValue !== undefined
          ? hitCellValue == null
            ? ''
            : String(hitCellValue)
          : fromProp || fromSelection;

      const selectedIndices = Array.from(selectedRows).sort((a, b) => a - b);
      const selectedDataRows = selectedIndices
        .map((i) => rows[i])
        .filter((r): r is unknown[] => Array.isArray(r));
      const hasSelectedRows = selectedDataRows.length > 0;
      const hasCellContext = hit != null && Array.isArray(hitRow);
      /**
       * The right-click landed inside a selection of more than one row: the menu
       * must act on the whole selection, not just the clicked row, so every
       * single-row / single-cell action is dropped below.
       */
      const multiRowSelection =
        hasCellContext && hit != null && selectedRows.size > 1 && selectedRows.has(hit.rowIndex);
      /** Row the cell-scoped actions operate on; null in selection mode. */
      const actionRow = hasCellContext && hitRow && !multiRowSelection ? hitRow : null;
      /** Rows the selection-scoped actions operate on. */
      const selectionRowActions = multiRowSelection || !hasCellContext ? selectedDataRows : [];
      const canFilterByValue = !loading && hasCellContext && !!onAddFilter && !!hit;
      const setNullAllowed = enableSetNull ?? !!onCellEdit;
      const canSetNull = hasCellContext && setNullAllowed && !!onCellEdit && !!hit;
      const deleteIndices = hasSelectedRows ? selectedIndices : hit != null ? [hit.rowIndex] : [];
      const canDelete =
        !!onDeleteRows && (primaryKeyColumns?.length ?? 0) > 0 && deleteIndices.length > 0;

      const csvRows = actionRow ? [actionRow] : selectedDataRows;
      const canCopyCsv = csvRows.length > 0;

      const copyText = (text: string) => {
        void navigator.clipboard.writeText(text);
      };

      void showNativeContextMenu(
        buildDataTableContextMenuItems({
          labels: {
            copy: t('common.copy'),
            copyRow: t('dataTable.copyRow'),
            moreActions: t('dataTable.moreActions'),
            copyAsJson: t('dataTable.copyAsJson'),
            copyAsSqlInsert: t('dataTable.copyAsSqlInsert'),
            copyAsUpdate: t('dataTable.copyAsUpdate'),
            copyAsCsv: t('dataTable.copyAsCsv'),
            copyColumnName: t('dataTable.copyColumnName'),
            copyColumnData: t('dataTable.copyColumnData'),
            setNull: t('dataTable.setNull'),
            filterByValue: t('dataTable.filterByValue'),
            copySelectedRows:
              selectedRows.size > 1
                ? `${t('common.copy')} ${t('export.selectedRows')} (${selectedRows.size})`
                : `${t('common.copy')} ${t('export.selectedRows')}`,
            deleteRow:
              deleteIndices.length > 1
                ? `${t('dataTable.deleteRow')} (${deleteIndices.length})`
                : t('dataTable.deleteRow'),
            export:
              selectedRows.size > 0
                ? `${t('export.export')} (${t('export.selectedRows')} ${selectedRows.size})`
                : t('export.export'),
          },
          handlers: {
            onCopy:
              hasCellContext && !multiRowSelection
                ? () => {
                    copyText(cellTextForCopy);
                  }
                : undefined,
            onCopyRow: actionRow
              ? () => {
                  copyText(serializeDataTableRowsAsTsv([actionRow]));
                }
              : undefined,
            onCopyAsJson: actionRow
              ? () => {
                  copyText(JSON.stringify(rowToNamedRecord(columnNames, actionRow), null, 2));
                }
              : selectionRowActions.length > 0
                ? () => {
                    copyText(serializeDataTableRowsAsJson(columnNames, selectionRowActions));
                  }
                : undefined,
            onCopyAsSqlInsert: actionRow
              ? () => {
                  copyText(
                    formatRowAsSqlInsert(
                      exportTableName || 'table',
                      columnNames,
                      actionRow,
                      databaseType as DatabaseType,
                    ),
                  );
                }
              : selectionRowActions.length > 0
                ? () => {
                    copyText(
                      formatRowsAsSqlInsert(
                        exportTableName || 'table',
                        columnNames,
                        selectionRowActions,
                        databaseType as DatabaseType,
                      ),
                    );
                  }
                : undefined,
            onCopyAsUpdate: actionRow
              ? () => {
                  copyText(
                    formatRowAsSqlUpdate(
                      exportTableName || 'table',
                      columnNames,
                      actionRow,
                      primaryKeyColumns,
                      databaseType as DatabaseType,
                    ),
                  );
                }
              : selectionRowActions.length > 0
                ? () => {
                    copyText(
                      formatRowsAsSqlUpdate(
                        exportTableName || 'table',
                        columnNames,
                        selectionRowActions,
                        primaryKeyColumns,
                        databaseType as DatabaseType,
                      ),
                    );
                  }
                : undefined,
            onCopyAsCsv: canCopyCsv
              ? () => {
                  copyText(serializeDataTableRowsAsCsv(columnNames, csvRows));
                }
              : undefined,
            onCopyColumnName: effectiveColName
              ? () => {
                  copyText(effectiveColName);
                }
              : undefined,
            onCopyColumnData:
              effectiveColIdx >= 0
                ? () => {
                    copyText(serializeDataTableColumnValues(effectiveColIdx, rows));
                  }
                : undefined,
            onSetNull:
              canSetNull && !multiRowSelection && hit
                ? () => {
                    onCellEdit?.(hit.rowIndex, hit.columnName, null);
                  }
                : undefined,
            onFilterByValue:
              canFilterByValue && hit
                ? () => {
                    onAddFilter?.({
                      column: hit.columnName,
                      operator: 'eq',
                      value: hitCellValue == null ? '' : String(hitCellValue),
                    });
                  }
                : undefined,
            onCopySelectedRows: hasSelectedRows
              ? () => {
                  copyText(serializeDataTableRowsAsTsv(selectedDataRows));
                }
              : undefined,
            onDeleteRow: canDelete
              ? () => {
                  onDeleteRows?.(deleteIndices);
                }
              : undefined,
            onExport: exportEnabled
              ? () => {
                  setExportOpen(true);
                }
              : undefined,
          },
          hasCellContext,
          hasHeaderContext: !!hitHeaderCol,
          hasSelectedRows,
          multiRowSelection,
          exportEnabled,
          canFilterByValue,
          canSetNull,
          canDelete,
        }),
        { x: e.clientX, y: e.clientY },
      );
    },
    [
      columnNames,
      exportEnabled,
      exportTableName,
      getContextCellText,
      onAddFilter,
      onCellEdit,
      enableSetNull,
      onDeleteRows,
      primaryKeyColumns,
      rows,
      selectedRows,
      t,
      loading,
    ],
  );

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
        onContextMenu={handleContextMenu}
        {...surfaceDomProps}
        {...(hasCellSelection ? surfaceProps : {})}
      >
        {emptyPlaceholder ? (
          emptyPlaceholder
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
              onRowClick={onRowClick}
              cellSelectionEnabled={hasCellSelection}
              isCellSelected={isCellSelected}
              activeCell={selection.mode === 'cell' ? selection.focus : null}
              cellIdPrefix={hasCellSelection ? gridIdPrefix : undefined}
            />
            {loading ? (
              <div
                role="status"
                aria-live="polite"
                className="flex min-h-28 items-center justify-center gap-2 text-xs text-fg-muted"
              >
                <Spinner size="lg" />
                {t('dataTable.loading')}
              </div>
            ) : rows.length === 0 ? (
              <div
                role="status"
                aria-live="polite"
                className="flex min-h-28 flex-col items-center justify-center gap-1 px-4 text-center text-xs text-fg-muted"
              >
                <span>{t('dataTable.empty')}</span>
                <span className="text-fg-muted/80">{t('dataTable.emptyHint')}</span>
              </div>
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
