import { useCallback, useEffect, useMemo, useState } from 'react';
import { AlertTriangle } from 'lucide-react';
import { DataTable } from '../../../components/DataTable/DataTable';
import type { ColumnDef } from '../../../components/DataTable/TableHeader';
import { useI18n } from '../../../hooks/useI18n';
import { useSettingsStore } from '../../../stores/settingsStore';
import type { StatementResult } from '../../../types';
import type { DataExportCapability } from '../../../lib/exportCapability';
import type { CellTypeHints } from '../../../lib/cellTypes';
import { tid } from '../../../lib/tid';
import {
  gridReadOnlyMessageKey,
  isColumnEditable,
  originChannelWritable,
  resolveResultEditability,
  resolveResultWriteBack,
  resolveRowAddressability,
  type GridReadOnlyReason,
  type ServerEditability,
} from '../../../lib/resultEditability';
import { useResultVerdict } from '../../../stores/panelResultEdit';
import { isMutationExecution } from './isMutationExecution';
import { ExecutionSummaryCard } from './ExecutionSummaryCard';

/**
 * The verdict of a result nobody has judged yet.
 *
 * "Not judged" is read-only: a permission that has not been granted is not a
 * permission. The reason is "the origin is unknown", which is what the user
 * is told until the back end answers.
 */
const UNJUDGED: ServerEditability = {
  writable: false,
  identityColumns: [],
  editableColumns: [],
  table: null,
  reason: 'unknownOrigin',
};

export interface ResultTableViewProps {
  result: StatementResult;
  onCellEdit?: (row: number, column: string, value: unknown) => Promise<void>;
  rowDetailIndex?: number | null;
  onRowDetail?: (rowIndex: number) => void;
  dataExportCapability?: DataExportCapability;
  /**
   * The key the write-back verdict of this result is stored under.
   *
   * The grid does not fetch the verdict itself: the row-detail drawer does,
   * and both surfaces share one key so the answer is fetched once. Without the
   * key the grid falls back to the narrow verdict it always had.
   */
  writeBackKey?: string | null;
  /**
   * The driver's own column-type declaration.
   *
   * The **same** declaration the table panel and the row-detail drawer read:
   * `DB_REGISTRY[databaseType]?.cellTypes`. A MySQL `tinyint(1)` column is a
   * boolean in the grid's header and cells, in the drawer and in the table
   * panel — one declaration, one answer, three surfaces.
   */
  cellTypeHints?: CellTypeHints;
}

/** Read-only DataTable adapter for query results.
 *
 * This intentionally does not use TableView: TableView owns database-backed
 * table loading and would create a second query path for an already available
 * StatementResult.
 */
export function ResultTableView({
  result,
  onCellEdit,
  rowDetailIndex = null,
  onRowDetail,
  dataExportCapability,
  writeBackKey = null,
  cellTypeHints,
}: ResultTableViewProps) {
  const { t } = useI18n();
  const queryResultLimit = useSettingsStore((s) => s.settings.queryResultLimit);
  const safeMode = useSettingsStore((s) => s.settings.safeMode);
  const [editError, setEditError] = useState<string | null>(null);
  const [editingCell, setEditingCell] = useState<{ row: number; col: string } | null>(null);
  /**
   * Why the last clicked row refused to open an editor.
   *
   * A row-level refusal is not a silent one: the grid says which row could
   * not be addressed and why (its locating key is NULL, or its identity is
   * not unique on the page).
   */
  const [rowRefusal, setRowRefusal] = useState<GridReadOnlyReason | null>(null);

  const { verdict } = useResultVerdict(writeBackKey);
  const serverVerdict = verdict ?? UNJUDGED;
  // The narrow host-side mapping verdict: it recognises `SELECT * FROM
  // <relation>` and the write path re-verifies it against the live catalog.
  const legacyVerified =
    result.source?.writableMapping === 'verified' && !!result.runtimeBinding;
  // The grid is editable when **either** verdict says so. The origin verdict
  // may only grant write-back while the release channel is open — see
  // `resolveResultWriteBack`.
  // Whether the **result** may be written back at all. Safe Mode is a
  // setting the user chose, not a fact about the result, so it does not
  // decide the reason.
  const resultWritable = resolveResultWriteBack(legacyVerified, serverVerdict);
  const canEdit = !safeMode && !!onCellEdit && resultWritable;
  /**
   * Why the grid cannot be edited, when the **result** cannot be written
   * back.
   *
   * A read-only grid always says why: a grid that is simply grey is the
   * "unresponsive cell" the contract forbids.
   */
  const readOnlyReason = resultWritable ? null : (verdict?.reason ?? 'unknownOrigin');
  // A visible result outlives its physical session. Do not keep an input alive
  // across retirement/replacement, or revive it for a newly queried result.
  useEffect(() => {
    setEditingCell(null);
    setEditError(null);
  }, [
    canEdit,
    result.runtimeBinding?.executionId,
    result.runtimeBinding?.resourceBindingId,
    result.runtimeBinding?.handle.dbSessionId,
    result.runtimeBinding?.handle.runtimeEpoch,
  ]);

  const columnDefs = useMemo<ColumnDef[]>(
    () =>
      result.columns.map((c) => ({
        id: c.name,
        name: c.name,
        type: c.dataType,
        nullable: c.nullable,
      })),
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
      if (!canEdit) {
        setEditingCell(null);
        return;
      }
      // The page-level guards are decided for **the row being clicked**: a
      // NULL key value or a duplicate identity makes that row unaddressable.
      const editability = resolveResultEditability(
        legacyVerified,
        serverVerdict,
        result.rows,
        result.columns,
        row,
      );
      if (!editability.writable) {
        setEditingCell(null);
        setRowRefusal(
          resolveRowAddressability(
            serverVerdict.identityColumns ?? [],
            result.rows,
            result.columns,
            row,
          ),
        );
        return;
      }
      setRowRefusal(null);
      // A column that is not a straight copy of a base column has no source
      // to write back to — the back end refuses it with
      // `grid.result.writeDenied`.
      //
      // §1.3 §5 has **no executor on the narrow channel today**: the origin
      // channel is closed, so no column list exists to check against. The
      // guard is in place and starts to bite the moment the channel opens.
      if (originChannelWritable(serverVerdict) && !isColumnEditable(serverVerdict, col)) {
        setEditingCell(null);
        return;
      }
      setEditingCell({ row, col });
    },
    [onRowDetail, canEdit, legacyVerified, serverVerdict, result],
  );

  if (isMutationExecution(result)) {
    return (
      <div className="flex min-h-0 min-w-0 flex-1 flex-col" {...tid('result-workspace-table')}>
        <ExecutionSummaryCard result={result} statusBar={statusBar} />
      </div>
    );
  }

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col" {...tid('result-workspace-table')}>
      {readOnlyReason && (
        <div
          data-dt-grid-readonly-reason={readOnlyReason}
          className="p-2 text-xs text-fg-muted"
        >
          {result.source?.writableMapping === 'verified' && !result.runtimeBinding
            ? t('query.resultEditRequiresRefresh')
            : t(gridReadOnlyMessageKey(readOnlyReason))}
        </div>
      )}
      {rowRefusal && (
        <div
          data-dt-grid-row-refusal={rowRefusal}
          className="p-2 text-xs text-fg-muted"
        >
          {t(gridReadOnlyMessageKey(rowRefusal))}
        </div>
      )}
      {editError && (
        <div role="alert" className="p-2 text-sm text-red-400">
          {editError}
        </div>
      )}
      <DataTable
        columns={columnDefs}
        rows={result.rows}
        cellTypeHints={cellTypeHints}
        statusBar={statusBar}
        rowHeight={32}
        editingCell={canEdit ? editingCell : null}
        onCellDoubleClick={handleCellDoubleClick}
        onCellEdit={(row, column, value) => {
          if (!canEdit) {
            setEditingCell(null);
            return;
          }
          setEditError(null);
          void onCellEdit?.(row, column, value)
            .then(() => setEditingCell(null))
            .catch((error: unknown) => {
              setEditingCell(null);
              setEditError(error instanceof Error ? error.message : String(error));
            });
        }}
        onCellEditCancel={() => setEditingCell(null)}
        enableSetNull={false}
        onRowClick={onRowDetail}
        highlightedRow={rowDetailIndex}
        exportTableName="query_result"
        dataExportCapability={dataExportCapability}
        className="mr-1 rounded-b-md rounded-t-none border-x border-b border-t-0"
      />
      {result.rows.length === 0 && (
        <div className="flex shrink-0 items-center justify-center border-t border-edge px-3 py-3 text-sm text-fg-muted">
          {t('sqlFile.noResults')}
        </div>
      )}
    </div>
  );
}
