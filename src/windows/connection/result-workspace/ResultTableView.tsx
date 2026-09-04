import { useCallback, useMemo, useState } from 'react';
import { AlertTriangle } from 'lucide-react';
import { DataTable } from '../../../components/DataTable/DataTable';
import type { ColumnDef } from '../../../components/DataTable/TableHeader';
import { useI18n } from '../../../hooks/useI18n';
import { useSettingsStore } from '../../../stores/settingsStore';
import type { StatementResult } from '../../../types';
import type { DataExportCapability } from '../../../lib/exportCapability';
import { tid } from '../../../lib/tid';

export interface ResultTableViewProps {
  result: StatementResult;
  rowDetailIndex?: number | null;
  onRowDetail?: (rowIndex: number) => void;
  dataExportCapability?: DataExportCapability;
}

/** Read-only DataTable adapter for query results.
 *
 * This intentionally does not use TableView: TableView owns database-backed
 * table loading and would create a second query path for an already available
 * StatementResult.
 */
export function ResultTableView({
  result,
  rowDetailIndex = null,
  onRowDetail,
  dataExportCapability,
}: ResultTableViewProps) {
  const { t } = useI18n();
  const queryResultLimit = useSettingsStore((s) => s.settings.queryResultLimit);
  const safeMode = useSettingsStore((s) => s.settings.safeMode);
  const [editingCell, setEditingCell] = useState<{ row: number; col: string } | null>(null);

  const columnDefs = useMemo<ColumnDef[]>(
    () => result.columns.map((c) => ({ id: c.name, name: c.name, type: c.dataType })),
    [result.columns],
  );

  const statusBar = useMemo(
    () => (
      <div className="flex items-center gap-3 border-b border-edge bg-surface-alt px-3 py-1.5 text-xs text-fg-secondary">
        <span>
          {result.rows.length} {t('common.rows')}
        </span>
        <span className="text-edge">|</span>
        <span>
          {result.columns.length} {t('common.columns')}
        </span>
        <span className="text-edge">|</span>
        <span>{result.executionTimeMs} ms</span>
        {result.sql && (
          <>
            <span className="text-edge">|</span>
            <span className="max-w-[400px] truncate font-mono text-fg-muted" title={result.sql}>
              {result.sql}
            </span>
          </>
        )}
        {result.truncated && (
          <>
            <span className="text-edge">|</span>
            <span className="flex items-center gap-1 text-yellow-400">
              <AlertTriangle className="h-3 w-3" />
              {t('query.resultTruncated', { limit: queryResultLimit })}
            </span>
          </>
        )}
      </div>
    ),
    [queryResultLimit, result, t],
  );

  const handleCellDoubleClick = useCallback(
    (row: number, col: string) => {
      // Safe Mode blocks in-place edit of query results. The row detail panel
      // still opens on double-click so the user can inspect the record.
      onRowDetail?.(row);
      if (safeMode) {
        setEditingCell(null);
        return;
      }
      setEditingCell({ row, col });
    },
    [onRowDetail, safeMode],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col" {...tid('result-workspace-table')}>
      <DataTable
        columns={columnDefs}
        rows={result.rows}
        statusBar={statusBar}
        rowHeight={32}
        editingCell={safeMode ? null : editingCell}
        onCellDoubleClick={handleCellDoubleClick}
        onCellEdit={(_row, _col, _value) => setEditingCell(null)}
        onCellEditCancel={() => setEditingCell(null)}
        enableSetNull={false}
        onRowClick={onRowDetail}
        highlightedRow={rowDetailIndex}
        exportTableName="query_result"
        dataExportCapability={dataExportCapability}
      />
      {result.rows.length === 0 && (
        <div className="flex shrink-0 items-center justify-center border-t border-edge px-3 py-3 text-sm text-fg-muted">
          {t('sqlFile.noResults')}
        </div>
      )}
    </div>
  );
}
