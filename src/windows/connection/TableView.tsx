import { useEffect } from 'react';
import { Loader2 } from 'lucide-react';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { useTableDataStore } from '../../stores/tableDataStore';

interface TableViewProps {
  connectionId: string;
  tableName: string;
}

export function TableView({ connectionId, tableName }: TableViewProps) {
  const columns = useTableDataStore((s) => s.columns);
  const rows = useTableDataStore((s) => s.rows);
  const totalRows = useTableDataStore((s) => s.totalRows);
  const page = useTableDataStore((s) => s.page);
  const pageSize = useTableDataStore((s) => s.pageSize);
  const sorts = useTableDataStore((s) => s.sorts);
  const filters = useTableDataStore((s) => s.filters);
  const editBuffer = useTableDataStore((s) => s.editBuffer);
  const editingCell = useTableDataStore((s) => s.editingCell);
  const selectedRows = useTableDataStore((s) => s.selectedRows);
  const loading = useTableDataStore((s) => s.loading);
  const error = useTableDataStore((s) => s.error);

  const loadTableData = useTableDataStore((s) => s.loadTableData);
  const setSort = useTableDataStore((s) => s.setSort);
  const addFilter = useTableDataStore((s) => s.addFilter);
  const removeFilter = useTableDataStore((s) => s.removeFilter);
  const clearFilters = useTableDataStore((s) => s.clearFilters);
  const setPage = useTableDataStore((s) => s.setPage);
  const setPageSize = useTableDataStore((s) => s.setPageSize);
  const startEdit = useTableDataStore((s) => s.startEdit);
  const updateCell = useTableDataStore((s) => s.updateCell);
  const cancelEdit = useTableDataStore((s) => s.cancelEdit);
  const selectRow = useTableDataStore((s) => s.selectRow);
  const toggleSelectAll = useTableDataStore((s) => s.toggleSelectAll);

  useEffect(() => {
    console.log('[TableView] loading', connectionId, tableName);
    void loadTableData({ connectionId, table: tableName });
  }, [connectionId, tableName, loadTableData]);

  if (error) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <div className="text-center">
          <div className="text-sm text-red-400">{error}</div>
          <button
            type="button"
            className="mt-2 text-xs text-accent hover:underline"
            onClick={() => void loadTableData({ connectionId, table: tableName })}
          >
            重试
          </button>
        </div>
      </div>
    );
  }

  if (loading && columns.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
        <Loader2 className="h-5 w-5 animate-spin" />
        加载表数据…
      </div>
    );
  }

  const columnDefs: ColumnDef[] = columns.map((c) => ({
    id: c.name,
    name: c.name,
    type: c.dataType,
  }));

  const rowArrays: unknown[][] = rows.map((record) =>
    columns.map((col) => record[col.name] ?? null),
  );

  return (
    <DataTable
      columns={columnDefs}
      rows={rowArrays}
      totalRows={totalRows}
      page={page}
      pageSize={pageSize}
      sorts={sorts}
      filters={filters}
      editBuffer={editBuffer}
      editingCell={editingCell}
      selectedRows={selectedRows}
      loading={loading}
      onSort={setSort}
      onFilter={addFilter}
      onRemoveFilter={removeFilter}
      onClearFilters={clearFilters}
      onPageChange={setPage}
      onPageSizeChange={setPageSize}
      onCellDoubleClick={startEdit}
      onCellEdit={updateCell}
      onCellEditCancel={cancelEdit}
      onRowSelect={selectRow}
      onSelectAll={toggleSelectAll}
    />
  );
}
