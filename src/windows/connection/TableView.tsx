import { useEffect } from 'react';
import { Filter, Loader2 } from 'lucide-react';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { NlFilterInput } from '../../components/ai/NlFilterInput';
import { useTableDataStore, type TableState } from '../../stores/tableDataStore';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';

interface TableViewProps {
  connectionId: string;
  database: string;
  tableName: string;
}

export function TableView({ connectionId, database, tableName }: TableViewProps) {
  const { t } = useI18n();
  // NlFilterInput handles unconfigured state internally
  const tableStates = useTableDataStore((s) => s.tableStates);
  const activeTable = useTableDataStore((s) => s.activeTable);
  const loadTableData = useTableDataStore((s) => s.loadTableData);
  const switchToTable = useTableDataStore((s) => s.switchToTable);
  const setSort = useTableDataStore((s) => s.setSort);
  const removeFilter = useTableDataStore((s) => s.removeFilter);
  const clearFilters = useTableDataStore((s) => s.clearFilters);
  const addFilter = useTableDataStore((s) => s.addFilter);
  const updateFilter = useTableDataStore((s) => s.updateFilter);
  const setFilterLogic = useTableDataStore((s) => s.setFilterLogic);
  const applyFilters = useTableDataStore((s) => s.applyFilters);
  const setFilterPanelOpen = useTableDataStore((s) => s.setFilterPanelOpen);
  const setPage = useTableDataStore((s) => s.setPage);
  const setPageSize = useTableDataStore((s) => s.setPageSize);
  const startEdit = useTableDataStore((s) => s.startEdit);
  const updateCell = useTableDataStore((s) => s.updateCell);
  const cancelEdit = useTableDataStore((s) => s.cancelEdit);
  const selectRow = useTableDataStore((s) => s.selectRow);
  const toggleSelectAll = useTableDataStore((s) => s.toggleSelectAll);
  const setDetailRow = useTableDataStore((s) => s.setDetailRow);
  const detailRowIndex = useTableDataStore((s) => s.detailRowIndex);

  const ts: TableState | undefined = tableStates.get(tableName);
  const hasData = ts != null && ts.columns.length > 0;

  useEffect(() => {
    if (hasData && activeTable !== tableName) {
      switchToTable(tableName);
    } else if (!hasData) {
      void loadTableData({ connectionId, table: tableName });
    }
  }, [connectionId, tableName, hasData, activeTable, loadTableData, switchToTable]);

  const columns = ts?.columns ?? [];
  const rows = ts?.rows ?? [];
  const totalRows = ts?.totalRows ?? 0;
  const page = ts?.page ?? 0;
  const pageSize = ts?.pageSize ?? 50;
  const sorts = ts?.sorts ?? [];
  const filters = ts?.filters ?? [];
  const filterLogic = ts?.filterLogic ?? 'and';
  const draftFilters = ts?.draftFilters ?? [];
  const draftFilterLogic = ts?.draftFilterLogic ?? 'and';
  const filterPanelOpen = ts?.filterPanelOpen ?? false;
  const editingCell = ts?.editingCell ?? null;
  const selectedRows = ts?.selectedRows ?? new Set<number>();
  const loading = ts?.loading ?? false;
  const error = ts?.error ?? null;

  const openManualFilter = () => {
    if (filterPanelOpen) {
      setFilterPanelOpen(false);
      return;
    }
    setFilterPanelOpen(true);
    if (draftFilters.length === 0) {
      addFilter({
        column: columns[0]?.name ?? '',
        operator: 'eq',
        value: '',
      });
    }
  };

  if (loading && columns.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted">
        <Loader2 className="h-5 w-5 animate-spin" />
        {t('tableView.loadingData')}
      </div>
    );
  }

  // Keep the table chrome (filters etc.) visible after a failed reload so the
  // user can clear/fix the bad filter instead of being stuck on a blank error page.
  if (error && columns.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <div className="text-center">
          <div className="text-sm text-red-400">{error}</div>
          <button
            type="button"
            className="mt-2 text-xs text-accent hover:underline"
            onClick={() => void loadTableData({ connectionId, table: tableName })}
          >
            {t('common.retry')}
          </button>
        </div>
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

  const filterActive = filterPanelOpen || filters.length > 0 || draftFilters.length > 0;

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {error && (
        <div className="flex items-center gap-3 border-b border-red-500/30 bg-red-500/10 px-3 py-2 text-sm text-red-400">
          <span className="flex-1 truncate">{error}</span>
          <button
            type="button"
            className="shrink-0 text-xs text-accent hover:underline"
            onClick={() => void loadTableData({ connectionId, table: tableName })}
          >
            {t('common.retry')}
          </button>
        </div>
      )}
      <div className="flex shrink-0 items-start gap-0.5 border-b border-edge px-2 py-0.5">
        <button
          type="button"
          onMouseDown={(e) => e.preventDefault()}
          className={cn(
            'mt-0 flex h-7 w-7 shrink-0 items-center justify-center rounded text-xs transition-colors hover:bg-surface-alt',
            filterActive ? 'text-accent' : 'text-fg-muted hover:text-fg',
          )}
          onClick={openManualFilter}
          title={t('filter.filter')}
          aria-label={t('filter.filter')}
          aria-pressed={filterPanelOpen}
          data-testid="table-filter-toggle"
        >
          <Filter className="h-3.5 w-3.5" />
        </button>
        <NlFilterInput connectionId={connectionId} database={database} tableName={tableName} />
      </div>
      <DataTable
        columns={columnDefs}
        rows={rowArrays}
        totalRows={totalRows}
        page={page}
        pageSize={pageSize}
        sorts={sorts}
        filters={filters}
        filterLogic={filterLogic}
        draftFilters={draftFilters}
        draftFilterLogic={draftFilterLogic}
        filterPanelOpen={filterPanelOpen}
        onFilterPanelOpenChange={setFilterPanelOpen}
        onAddFilter={addFilter}
        onUpdateFilter={updateFilter}
        onFilterLogicChange={setFilterLogic}
        onApplyFilters={applyFilters}
        editingCell={editingCell}
        selectedRows={selectedRows}
        loading={loading}
        onSort={setSort}
        onRemoveFilter={removeFilter}
        onClearFilters={clearFilters}
        onPageChange={setPage}
        onPageSizeChange={setPageSize}
        onCellDoubleClick={startEdit}
        onCellEdit={updateCell}
        onCellEditCancel={cancelEdit}
        onRowSelect={selectRow}
        onSelectAll={toggleSelectAll}
        onRowClick={setDetailRow}
        highlightedRow={detailRowIndex}
        exportTableName={tableName}
      />
    </div>
  );
}
