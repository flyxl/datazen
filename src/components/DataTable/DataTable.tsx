import { useCallback, useMemo, useState } from 'react';
import { Download } from 'lucide-react';
import type { FilterCondition, SortCondition } from '../../types';
import type { CellEdit } from '../../stores/tableDataStore';
import { useI18n } from '../../hooks/useI18n';
import { useColumnResize, adjustWidthsForSort } from '../../hooks/useColumnResize';
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
  onRemoveFilter?: (index: number) => void;
  onClearFilters?: () => void;
  onAddFilter?: (filter: FilterCondition) => void;
  onUpdateFilter?: (index: number, filter: FilterCondition) => void;
  onFilterLogicChange?: (logic: 'and' | 'or') => void;

  editingCell?: { row: number; col: string } | null;
  editBuffer?: Map<string, CellEdit>;
  onCellDoubleClick?: (row: number, col: string) => void;
  onCellEdit?: (row: number, col: string, value: unknown) => void;
  onCellEditCancel?: () => void;

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
  editingCell,
  selectedRows = EMPTY_SET,
  loading,
  onSort,
  onRemoveFilter,
  onClearFilters,
  onAddFilter,
  onUpdateFilter,
  onFilterLogicChange,
  onPageChange,
  onPageSizeChange,
  onCellDoubleClick,
  onCellEdit,
  onCellEditCancel,
  onRowSelect,
  onSelectAll,
  onRowClick,
  highlightedRow,
  statusBar,
  rowHeight = 40,
  exportTableName,
  databaseType,
}: DataTableProps) {
  const { t } = useI18n();
  const [scrollEl, setScrollEl] = useState<HTMLDivElement | null>(null);
  const [exportOpen, setExportOpen] = useState(false);
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null);

  const colMeta = useMemo(
    () => columns.map((c) => ({ name: c.name, type: c.type })),
    [columns],
  );
  const { columnWidths: baseWidths, onResizeStart } = useColumnResize({ count: columns.length, columns: colMeta, rows });
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

  const handleContextMenu = useCallback((e: React.MouseEvent) => {
    if (!exportTableName) return;
    e.preventDefault();
    setCtxMenu({ x: e.clientX, y: e.clientY });
  }, [exportTableName]);

  const exportEnabled = !!exportTableName && rows.length > 0;

  const hasPagination = page != null && pageSize != null && totalRows != null && onPageChange && onPageSizeChange;
  const hasSelection = onSelectAll != null && onRowSelect != null;
  const hasFilters = filters.length > 0 && onRemoveFilter && onClearFilters;
  const hasFilterEditor = onAddFilter && onUpdateFilter && onRemoveFilter && onClearFilters && onFilterLogicChange;

  return (
    <div className="selectable flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-md border border-edge bg-surface">
      {hasFilterEditor ? (
        <FilterEditor
          columns={columns}
          filters={filters}
          logic={filterLogic}
          onLogicChange={onFilterLogicChange}
          onChange={onUpdateFilter}
          onAdd={onAddFilter}
          onRemove={onRemoveFilter}
          onClear={onClearFilters}
        />
      ) : hasFilters ? (
        <FilterBar filters={filters} onRemove={onRemoveFilter} onClear={onClearFilters} />
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
            {t('dataTable.selectAll')}
          </label>
          {selectedRows.size > 0 && (
            <span className="text-xs text-fg-muted">
              {t('dataTable.selected')} {selectedRows.size} / {rows.length} {t('common.rows')}
            </span>
          )}
          {exportEnabled && (
            <button
              type="button"
              className="ml-auto flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-fg-secondary hover:bg-surface-raised hover:text-fg"
              onClick={() => setExportOpen(true)}
              title={t('export.export')}
            >
              <Download className="h-3 w-3" />
              {t('export.export')}
            </button>
          )}
          {loading ? <span className="text-xs text-fg-muted">{t('common.loading')}</span> : null}
        </div>
      )}

      {statusBar}

      <div ref={setScrollEl} className="min-h-0 flex-1 overflow-auto" onContextMenu={handleContextMenu}>
        <TableHeader
          columns={columns}
          sorts={sorts}
          onSort={onSort ?? NOOP}
          columnWidths={columnWidths}
          onResizeStart={onResizeStart}
          sortable={onSort != null}
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
        />
      )}

      {exportEnabled && !hasSelection && (
        <div className="flex shrink-0 items-center justify-end border-t border-edge bg-surface-alt px-2 py-1">
          <button
            type="button"
            className="flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-fg-secondary hover:bg-surface-raised hover:text-fg"
            onClick={() => setExportOpen(true)}
            title={t('export.export')}
          >
            <Download className="h-3 w-3" />
            {t('export.export')}
          </button>
        </div>
      )}

      {ctxMenu && exportEnabled && (
        <>
          {/* Backdrop to close context menu */}
          <div className="fixed inset-0 z-[9998]" onClick={() => setCtxMenu(null)} onContextMenu={(e) => { e.preventDefault(); setCtxMenu(null); }} />
          <div
            className="fixed z-[9999] min-w-[160px] rounded-lg border border-edge bg-surface-alt py-1 shadow-xl"
            style={{ left: ctxMenu.x, top: ctxMenu.y }}
          >
            <button
              type="button"
              className="flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-[13px] text-fg-secondary hover:bg-surface-raised hover:text-fg"
              onClick={() => { setCtxMenu(null); setExportOpen(true); }}
            >
              <Download className="h-3.5 w-3.5" />
              {selectedRows.size > 0
                ? `${t('export.export')} (${t('export.selectedRows')} ${selectedRows.size})`
                : t('export.export')}
            </button>
          </div>
        </>
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
        />
      )}
    </div>
  );
}

export type { ColumnDef };
