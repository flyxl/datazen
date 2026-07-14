import { useCallback, useMemo, useState } from 'react';
import type { FilterCondition, SortCondition } from '../../types';
import type { CellEdit } from '../../stores/tableDataStore';
import { useI18n } from '../../hooks/useI18n';
import { useColumnResize, adjustWidthsForSort } from '../../hooks/useColumnResize';
import { FilterBar } from '../FilterBar';
import { Pagination } from './Pagination';
import { TableHeader, type ColumnDef } from './TableHeader';
import { VirtualBody } from './VirtualBody';

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
  onRemoveFilter?: (index: number) => void;
  onClearFilters?: () => void;

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
  editingCell,
  selectedRows = EMPTY_SET,
  loading,
  onSort,
  onRemoveFilter,
  onClearFilters,
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
}: DataTableProps) {
  const { t } = useI18n();
  const [scrollEl, setScrollEl] = useState<HTMLDivElement | null>(null);
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

  const hasPagination = page != null && pageSize != null && totalRows != null && onPageChange && onPageSizeChange;
  const hasSelection = onSelectAll != null && onRowSelect != null;
  const hasFilters = filters.length > 0 && onRemoveFilter && onClearFilters;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-md border border-edge bg-surface">
      {hasFilters && (
        <FilterBar filters={filters} onRemove={onRemoveFilter} onClear={onClearFilters} />
      )}

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
          {loading ? <span className="text-xs text-fg-muted">{t('common.loading')}</span> : null}
        </div>
      )}

      {statusBar}

      <div ref={setScrollEl} className="min-h-0 flex-1 overflow-auto">
        <TableHeader
          columns={columns}
          sorts={sorts}
          onSort={onSort ?? NOOP}
          columnWidths={columnWidths}
          onResizeStart={onResizeStart}
          sortable={onSort != null}
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
    </div>
  );
}

export type { ColumnDef };
