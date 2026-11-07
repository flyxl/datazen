/**
 * The bar above the grid: select-all, the selection summary, and the header actions.
 *
 * Extracted from `DataTable.tsx` (which was one clipboard change away from the 800-line
 * ceiling) with the markup moved verbatim — same classes, same conditions, same order. The
 * props are exactly what the block read from its scope, so the rendered DOM is unchanged.
 */
import type { ReactNode } from 'react';
import { Download, Trash2 } from 'lucide-react';
import { tid } from '../../lib/tid';
import type { GridSelection } from '../../stores/tableData/gridSelection';

export interface DataTableSelectionBarProps {
  /** Row selection is a thing for this grid (`selectedRows` is a real set). */
  hasSelection: boolean;
  /** The cell-selection contract is on. */
  hasCellSelection: boolean;
  selection: GridSelection;
  selectedRows: ReadonlySet<number>;
  rowCount: number;
  coveredRows: readonly number[] | null;
  coveredColumns: number;
  coveredCells: number;
  onSelectAll: () => void;
  headerActions?: ReactNode;
  onDeleteRows?: (rowIndices: number[]) => void;
  deleteRowIndices: number[];
  deleteRowCount: number;
  primaryKeyColumns?: unknown[];
  exportEnabled: boolean;
  onExport: () => void;
  loading?: boolean;
  t: (key: string, params?: Record<string, string | number>) => string;
}

export function DataTableSelectionBar({
  hasSelection,
  hasCellSelection,
  selection,
  selectedRows,
  rowCount,
  coveredRows,
  coveredColumns,
  coveredCells,
  onSelectAll,
  headerActions,
  onDeleteRows,
  deleteRowIndices,
  deleteRowCount,
  primaryKeyColumns,
  exportEnabled,
  onExport,
  loading,
  t,
}: DataTableSelectionBarProps) {
  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface px-2 py-1.5">
      {hasSelection && (
        <label className="flex cursor-pointer items-center gap-1.5 text-xs text-fg-secondary hover:text-fg">
          <input
            type="checkbox"
            className="accent-accent"
            checked={rowCount > 0 && selectedRows.size === rowCount}
            ref={(el) => {
              if (el) el.indeterminate = selectedRows.size > 0 && selectedRows.size < rowCount;
            }}
            onChange={onSelectAll}
          />
          {t('common.selectAll')}
        </label>
      )}
      {hasSelection && selectedRows.size > 0 && (
        <span className="text-xs text-fg-muted">
          {t('dataTable.selected')} {selectedRows.size} / {rowCount} {t('common.rows')}
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
            onClick={onExport}
            title={t('export.export')}
          >
            <Download className="h-3 w-3" />
            {t('export.export')}
          </button>
        )}
      </div>
      {loading ? <span className="text-xs text-fg-muted">{t('common.loading')}</span> : null}
    </div>
  );
}
