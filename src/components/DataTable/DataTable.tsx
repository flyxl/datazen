import { useRef } from 'react';
import type { FilterCondition, SortCondition } from '../../types';
import type { CellEdit } from '../../stores/tableDataStore';
import { useColumnResize } from '../../hooks/useColumnResize';
import { FilterBar } from '../FilterBar';
import { Pagination } from './Pagination';
import { TableHeader, type ColumnDef } from './TableHeader';
import { VirtualBody } from './VirtualBody';

export interface DataTableProps {
  columns: ColumnDef[];
  rows: unknown[][];
  totalRows: number;
  page: number;
  pageSize: number;
  sorts: SortCondition[];
  filters: FilterCondition[];
  editBuffer: Map<string, CellEdit>;
  editingCell: { row: number; col: string } | null;
  selectedRows: Set<number>;
  loading: boolean;
  onSort: (sort: SortCondition) => void;
  onFilter: (filter: FilterCondition) => void;
  onRemoveFilter: (index: number) => void;
  onClearFilters: () => void;
  onPageChange: (page: number) => void;
  onPageSizeChange: (size: number) => void;
  onCellDoubleClick: (row: number, col: string) => void;
  onCellEdit: (row: number, col: string, value: unknown) => void;
  onCellEditCancel: () => void;
  onRowSelect: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  onSelectAll: () => void;
}

export function DataTable({
  columns,
  rows,
  totalRows,
  page,
  pageSize,
  sorts,
  filters,
  editBuffer: _editBuffer,
  editingCell,
  selectedRows,
  loading,
  onSort,
  onFilter: _onFilter,
  onRemoveFilter,
  onClearFilters,
  onPageChange,
  onPageSizeChange,
  onCellDoubleClick,
  onCellEdit,
  onCellEditCancel,
  onRowSelect,
  onSelectAll,
}: DataTableProps) {
  void _onFilter;
  void _editBuffer;

  const scrollRef = useRef<HTMLDivElement>(null);
  const { columnWidths, onResizeStart } = useColumnResize({ count: columns.length });

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-md border border-edge bg-surface">
      <FilterBar filters={filters} onRemove={onRemoveFilter} onClear={onClearFilters} />
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
          全选
        </label>
        {selectedRows.size > 0 && (
          <span className="text-xs text-fg-muted">
            已选 {selectedRows.size} / {rows.length} 行
          </span>
        )}
        {loading ? <span className="text-xs text-fg-muted">加载中…</span> : null}
      </div>
      {/* Single scroll container for both horizontal & vertical scrolling */}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto">
        <TableHeader columns={columns} sorts={sorts} onSort={onSort} columnWidths={columnWidths} onResizeStart={onResizeStart} />
        <VirtualBody
          columns={columns}
          rows={rows}
          rowHeight={40}
          editingCell={editingCell}
          selectedRows={selectedRows}
          scrollRef={scrollRef}
          columnWidths={columnWidths}
          onCellDoubleClick={onCellDoubleClick}
          onCellEdit={onCellEdit}
          onCellEditCancel={onCellEditCancel}
          onRowSelect={onRowSelect}
        />
      </div>
      <Pagination
        page={page}
        pageSize={pageSize}
        totalRows={totalRows}
        onPageChange={onPageChange}
        onPageSizeChange={onPageSizeChange}
      />
    </div>
  );
}

export type { ColumnDef };
