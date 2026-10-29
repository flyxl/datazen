import { Button } from '../../components/ui/Button';
import { Spinner } from '../../components/ui/Spinner';
import type {
  DataSyncRowChange,
  DataSyncTableResult,
  DataSyncTableSelection,
  SyncOptions,
} from '../../commands/sync';
import { useI18n } from '../../hooks/useI18n';
import { CompareSummary } from './CompareSummary';
import { DiffDetail } from './DiffDetail';
import { TableListPanel } from './TableListPanel';
import type { CompareSummaryStats, TableDiffFilter } from './mappingView';
import { tablePageKey } from './mappingView';
import type { DataSyncPageCursor } from './useDataSyncSelectionState';
import type { SyncState } from './utils';

type RowOperation = Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>;

export interface DataSyncCompareStepProps {
  syncState: SyncState;
  /** The comparison produced a reviewable result. */
  compared: boolean;
  rows: DataSyncTableResult[];
  stats: CompareSummaryStats;
  onCopyReport: () => void;
  onExplainDiff: () => void;
  explainLoading: boolean;
  onCancel: () => void;
  filter: TableDiffFilter;
  search: string;
  selectedTableKey: string | null;
  onFilterChange: (filter: TableDiffFilter) => void;
  onSearchChange: (search: string) => void;
  onSelectTable: (key: string) => void;
  selectedTable: DataSyncTableResult | null;
  options: SyncOptions;
  tableSelections: DataSyncTableSelection[];
  onUpdateRows: (table: DataSyncTableResult, rows: DataSyncRowChange[]) => void;
  onSelectAllOperation: (table: DataSyncTableResult, operation: RowOperation) => void;
  onClearAllOperation: (table: DataSyncTableResult, operation: RowOperation) => void;
  isOperationSelected: (table: DataSyncTableResult, operation: RowOperation) => boolean;
  onPageChange: (table: DataSyncTableResult, direction: 'previous' | 'next') => void;
  pageLoading: boolean;
  pageIndex: Record<string, number>;
  pageCursors: Record<string, DataSyncPageCursor>;
}

/**
 * Fourth step of the Data Sync wizard: the comparison result with the table
 * list and the paged row diffs of the selected table.
 */
export function DataSyncCompareStep({
  syncState,
  compared,
  rows,
  stats,
  onCopyReport,
  onExplainDiff,
  explainLoading,
  onCancel,
  filter,
  search,
  selectedTableKey,
  onFilterChange,
  onSearchChange,
  onSelectTable,
  selectedTable,
  options,
  tableSelections,
  onUpdateRows,
  onSelectAllOperation,
  onClearAllOperation,
  isOperationSelected,
  onPageChange,
  pageLoading,
  pageIndex,
  pageCursors,
}: DataSyncCompareStepProps) {
  const { t } = useI18n();

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      {syncState === 'comparing' ? (
        <div className="flex flex-1 items-center justify-center gap-2 text-sm text-fg-muted">
          <Spinner size="lg" />
          {t('sync.comparing')}
          <Button variant="ghost" size="sm" data-testid="data-sync-cancel" onClick={onCancel}>
            {t('common.cancel')}
          </Button>
        </div>
      ) : compared ? (
        <>
          <CompareSummary
            stats={stats}
            onCopyReport={onCopyReport}
            onExplainDiff={onExplainDiff}
            explainLoading={explainLoading}
          />
          <div className="flex min-h-0 flex-1 rounded-lg border border-edge">
            <TableListPanel
              rows={rows}
              filter={filter}
              search={search}
              selectedTableKey={selectedTableKey}
              onFilterChange={onFilterChange}
              onSearchChange={onSearchChange}
              onSelectTable={onSelectTable}
            />
            <div className="flex min-h-0 min-w-0 flex-[1.4] flex-col">
              {selectedTable ? (
                <DiffDetail
                  table={selectedTable}
                  options={options}
                  onUpdateRows={(rows) => onUpdateRows(selectedTable, rows)}
                  onSelectAllOperation={(operation) => onSelectAllOperation(selectedTable, operation)}
                  onClearAllOperation={(operation) => onClearAllOperation(selectedTable, operation)}
                  isOperationSelected={(operation) => isOperationSelected(selectedTable, operation)}
                  hasTableSelection={tableSelections.some(
                    (scope) =>
                      scope.sourceTable === selectedTable.sourceTable &&
                      scope.targetTable === selectedTable.targetTable,
                  )}
                  pageLoading={pageLoading}
                  pageIndex={pageIndex[tablePageKey(selectedTable)] ?? 0}
                  hasPreviousPage={Boolean(
                    pageCursors[tablePageKey(selectedTable)]?.previous.length,
                  )}
                  hasNextPage={Boolean(pageCursors[tablePageKey(selectedTable)]?.next)}
                  onPageChange={(direction) => onPageChange(selectedTable, direction)}
                />
              ) : (
                <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
                  {t('sync.selectTableForDetail')}
                </div>
              )}
            </div>
          </div>
        </>
      ) : (
        <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
          {t('sync.comparePrompt')}
        </div>
      )}
    </div>
  );
}
