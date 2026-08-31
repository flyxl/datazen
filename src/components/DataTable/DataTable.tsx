import { useCallback, useMemo, useState, type KeyboardEvent } from 'react';
import { tid } from '../../lib/tid';
import { Download, Trash2 } from 'lucide-react';
import type { FilterCondition, SortCondition } from '../../types';
import type { CellEdit } from '../../stores/tableDataStore';
import { useI18n } from '../../hooks/useI18n';
import { useColumnResize, adjustWidthsForSort } from '../../hooks/useColumnResize';
import {
  buildDataTableContextMenuItems,
  formatRowAsSqlInsert,
  formatRowAsSqlUpdate,
  resolveDataTableCellFromEvent,
  rowToNamedRecord,
  serializeDataTableRowsAsCsv,
  serializeDataTableRowsAsTsv,
} from '../../lib/dataTableContextMenu';
import { showNativeContextMenu } from '../../lib/nativeContextMenu';
import { FilterBar } from '../FilterBar';
import { FilterEditor } from '../FilterEditor';
import { Pagination } from './Pagination';
import { TableHeader, type ColumnDef } from './TableHeader';
import { VirtualBody } from './VirtualBody';
import { DataExportDialog } from './DataExportDialog';

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
  rowHeight = 40,
  exportTableName,
  databaseType,
  dbSessionId,
  dataExportCapability,
  primaryKeyColumns,
  onDeleteRows,
  getContextCellText,
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
      onRowSelect?.(index, opts);
    },
    [onRowClick, onRowSelect],
  );

  const exportEnabled = !!exportTableName && rows.length > 0 && dataExportCapability !== 'none';
  const columnNames = useMemo(() => columns.map((c) => c.name), [columns]);

  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();

      const hit = resolveDataTableCellFromEvent(e.target);
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
      const canFilterByValue = !loading && hasCellContext && !!onAddFilter && !!hit;
      const setNullAllowed = enableSetNull ?? !!onCellEdit;
      const canSetNull = hasCellContext && setNullAllowed && !!onCellEdit && !!hit;
      const deleteIndices = hasSelectedRows ? selectedIndices : hit != null ? [hit.rowIndex] : [];
      const canDelete =
        !!onDeleteRows && (primaryKeyColumns?.length ?? 0) > 0 && deleteIndices.length > 0;

      const csvRows =
        hasSelectedRows && !hasCellContext
          ? selectedDataRows
          : hasCellContext && hitRow
            ? [hitRow]
            : selectedDataRows;
      const canCopyCsv = csvRows.length > 0;

      const copyText = (text: string) => {
        void navigator.clipboard.writeText(text);
      };

      void showNativeContextMenu(
        buildDataTableContextMenuItems({
          labels: {
            copy: t('common.copy'),
            copyRow: t('dataTable.copyRow'),
            copyAsJson: t('dataTable.copyAsJson'),
            copyAsSqlInsert: t('dataTable.copyAsSqlInsert'),
            copyAsUpdate: t('dataTable.copyAsUpdate'),
            copyAsCsv: t('dataTable.copyAsCsv'),
            copyColumnName: t('dataTable.copyColumnName'),
            setNull: t('dataTable.setNull'),
            filterByValue: t('dataTable.filterByValue'),
            copySelectedRows: `${t('common.copy')} ${t('export.selectedRows')}`,
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
            onCopy: hasCellContext
              ? () => {
                  copyText(cellTextForCopy);
                }
              : undefined,
            onCopyRow:
              hasCellContext && hitRow
                ? () => {
                    copyText(serializeDataTableRowsAsTsv([hitRow]));
                  }
                : undefined,
            onCopyAsJson:
              hasCellContext && hitRow
                ? () => {
                    copyText(JSON.stringify(rowToNamedRecord(columnNames, hitRow), null, 2));
                  }
                : undefined,
            onCopyAsSqlInsert:
              hasCellContext && hitRow
                ? () => {
                    copyText(formatRowAsSqlInsert(exportTableName || 'table', columnNames, hitRow));
                  }
                : undefined,
            onCopyAsUpdate:
              hasCellContext && hitRow
                ? () => {
                    copyText(
                      formatRowAsSqlUpdate(
                        exportTableName || 'table',
                        columnNames,
                        hitRow,
                        primaryKeyColumns,
                      ),
                    );
                  }
                : undefined,
            onCopyAsCsv: canCopyCsv
              ? () => {
                  copyText(serializeDataTableRowsAsCsv(columnNames, csvRows));
                }
              : undefined,
            onCopyColumnName:
              hasCellContext && hit
                ? () => {
                    copyText(hit.columnName);
                  }
                : undefined,
            onSetNull:
              canSetNull && hit
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
          hasSelectedRows,
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
  const hasSelection = onSelectAll != null && onRowSelect != null;
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
      if (selectedRows.size === 0) return;
      e.preventDefault();
      onDeleteRows(Array.from(selectedRows).sort((a, b) => a - b));
    },
    [editingCell, onDeleteRows, primaryKeyColumns, selectedRows],
  );

  return (
    <div
      className="selectable flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-md border border-edge bg-surface"
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

      {hasSelection && (
        <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface px-2 py-1.5">
          <label className="flex cursor-pointer items-center gap-1.5 text-xs text-fg-secondary hover:text-fg">
            <input
              type="checkbox"
              className="accent-blue-500"
              checked={rows.length > 0 && selectedRows.size === rows.length}
              ref={(el) => {
                if (el) el.indeterminate = selectedRows.size > 0 && selectedRows.size < rows.length;
              }}
              onChange={onSelectAll}
            />
            {t('common.selectAll')}
          </label>
          {selectedRows.size > 0 && (
            <span className="text-xs text-fg-muted">
              {t('dataTable.selected')} {selectedRows.size} / {rows.length} {t('common.rows')}
            </span>
          )}
          <div className="ml-auto flex items-center gap-1">
            {onDeleteRows && selectedRows.size > 0 && (primaryKeyColumns?.length ?? 0) > 0 && (
              <button
                type="button"
                data-testid="data-table-delete-rows"
                className="flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-red-400 hover:bg-surface-raised hover:text-red-300"
                onClick={() => onDeleteRows(Array.from(selectedRows).sort((a, b) => a - b))}
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
        onContextMenu={handleContextMenu}
      >
        <TableHeader
          columns={columns}
          sorts={sorts}
          onSort={onSort ?? NOOP}
          columnWidths={columnWidths}
          onResizeStart={onResizeStart}
          sortable={onSort != null}
          onFilterColumn={
            onAddFilter ? (column) => onAddFilter({ column, operator: 'eq', value: '' }) : undefined
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
          onCellDoubleClick={onCellDoubleClick ?? NOOP}
          onCellEdit={onCellEdit ?? NOOP}
          onCellEditCancel={onCellEditCancel ?? NOOP}
          onRowSelect={handleRowClick}
        />
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

      {exportEnabled && !hasSelection && (
        <div className="flex shrink-0 items-center justify-end border-t border-edge bg-surface-alt px-2 py-1">
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
        </div>
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
