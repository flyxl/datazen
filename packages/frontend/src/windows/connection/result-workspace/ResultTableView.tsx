import { useCallback, useEffect, useMemo, useState } from 'react';
import { AlertTriangle, Search } from 'lucide-react';
import { DataTable } from '../../../components/DataTable/DataTable';
import type { ColumnDef } from '../../../components/DataTable/TableHeader';
import { useI18n } from '../../../hooks/useI18n';
import { useValuePresentationSnapshot } from '../../../hooks/useValuePresentationSnapshot';
import { useSettingsStore } from '../../../stores/settingsStore';
import {
  DEFAULT_GRID_ALTERNATING_ROWS,
  DEFAULT_GRID_AUTO_HIDE_SCROLLBAR,
  DEFAULT_GRID_DENSITY,
  DEFAULT_GRID_ROW_NUMBERS,
} from '../../../types/settings';
import { rowHeightForDensity } from '../../../components/DataTable/rowHeight';
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
import { ResultFindBar } from './ResultFindBar';
import {
  EMPTY_RESULT_FIND_QUERY,
  filterResultRows,
  planResultReplacements,
  type ResultFindQuery,
} from '../../../lib/resultFindReplace';
import type { DirtyRowEntry } from '../../../stores/tableData/dirtyIndex';
import { TablePendingChangesBar } from '../TablePendingChangesBar';
import { PendingPlanDialog } from '../PendingPlanDialog';
import { queryCommands } from '../../../commands/query';
import type { ResultCellChangeRequest, RowChangePlan } from '@datazen/backend-client';

const UNJUDGED: ServerEditability = {
  writable: false,
  identityColumns: [],
  editableColumns: [],
  table: null,
  reason: 'unknownOrigin',
};

type PendingEdit = { row: number; column: string; original: unknown; value: unknown };
const editKey = (row: number, column: string) => `${row}\u0000${column}`;

export interface ResultTableViewProps {
  result: StatementResult;
  /** Commit staged cell edits in one explicit action. */
  onCommitResultCells?: (
    edits: Array<{ row: number; column: string; value: unknown }>,
  ) => Promise<void>;
  rowDetailIndex?: number | null;
  onRowDetail?: (rowIndex: number) => void;
  dataExportCapability?: DataExportCapability;
  writeBackKey?: string | null;
  cellTypeHints?: CellTypeHints;
  /** Context for preview/commit planning of result-cell changes. */
  connectionId?: string;
  driverType?: string;
  database?: string | null;
  schema?: string | null;
}

/** Query-result grid with staged (preview/commit/rollback) cell edits. */
export function ResultTableView({
  result,
  onCommitResultCells,
  rowDetailIndex = null,
  onRowDetail,
  dataExportCapability,
  writeBackKey = null,
  cellTypeHints,
  connectionId,
  driverType,
  database,
  schema,
}: ResultTableViewProps) {
  const { t } = useI18n();
  const queryResultLimit = useSettingsStore((s) => s.settings.queryResultLimit);
  const safeMode = useSettingsStore((s) => s.settings.safeMode);
  const presentationSnapshot = useValuePresentationSnapshot();
  const gridDensity = useSettingsStore((s) => s.settings.gridDensity ?? DEFAULT_GRID_DENSITY);
  const gridAlternatingRows = useSettingsStore(
    (s) => s.settings.gridAlternatingRows ?? DEFAULT_GRID_ALTERNATING_ROWS,
  );
  const gridRowNumbers = useSettingsStore(
    (s) => s.settings.gridRowNumbers ?? DEFAULT_GRID_ROW_NUMBERS,
  );
  const gridAutoHideScrollbar = useSettingsStore(
    (s) => s.settings.gridAutoHideScrollbar ?? DEFAULT_GRID_AUTO_HIDE_SCROLLBAR,
  );
  const gridRowHeight = rowHeightForDensity(32, gridDensity);
  const [editError, setEditError] = useState<string | null>(null);
  const [editErrorHint, setEditErrorHint] = useState<string | null>(null);
  const [commitNotice, setCommitNotice] = useState<string | null>(null);
  const [editingCell, setEditingCell] = useState<{ row: number; col: string } | null>(null);
  const [findOpen, setFindOpen] = useState(false);
  const [findQuery, setFindQuery] = useState<ResultFindQuery>(EMPTY_RESULT_FIND_QUERY);
  const [findReplacement, setFindReplacement] = useState('');
  const [rowRefusal, setRowRefusal] = useState<GridReadOnlyReason | null>(null);
  const [pendingEdits, setPendingEdits] = useState<Map<string, PendingEdit>>(new Map());
  const [pendingBusy, setPendingBusy] = useState(false);
  const [previewPlan, setPreviewPlan] = useState<RowChangePlan | null>(null);
  const [previewOpen, setPreviewOpen] = useState(false);

  const { verdict } = useResultVerdict(writeBackKey);
  const serverVerdict = verdict ?? UNJUDGED;
  const legacyVerified = result.source?.writableMapping === 'verified' && !!result.runtimeBinding;
  const resultWritable = resolveResultWriteBack(legacyVerified, serverVerdict);
  const canEdit = !safeMode && !!onCommitResultCells && resultWritable;
  const readOnlyReason = resultWritable ? null : (verdict?.reason ?? 'unknownOrigin');

  useEffect(() => {
    setEditingCell(null);
    setEditError(null);
    setEditErrorHint(null);
    setCommitNotice(null);
    setFindQuery(EMPTY_RESULT_FIND_QUERY);
    setPendingEdits(new Map());
    setPreviewPlan(null);
    setPreviewOpen(false);
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

  const columnNames = useMemo(() => result.columns.map((column) => column.name), [result.columns]);
  const originalIndexes = useMemo(
    () => filterResultRows(result.rows, columnNames, findQuery),
    [columnNames, findQuery, result.rows],
  );
  const findFiltering = findOpen && findQuery.needle !== '';
  const baseRows = result.rows;
  const effectiveRows = useMemo(() => {
    if (pendingEdits.size === 0) return baseRows;
    const out = baseRows.map((r) => [...r]);
    for (const edit of pendingEdits.values()) {
      const colIdx = columnNames.indexOf(edit.column);
      if (colIdx >= 0 && out[edit.row]) out[edit.row][colIdx] = edit.value as never;
    }
    return out;
  }, [baseRows, pendingEdits, columnNames]);

  const viewRows = useMemo(
    () => (findFiltering ? originalIndexes.map((index) => effectiveRows[index]) : effectiveRows),
    [findFiltering, originalIndexes, effectiveRows],
  );
  const toOriginal = useCallback(
    (viewIndex: number) => (findFiltering ? (originalIndexes[viewIndex] ?? viewIndex) : viewIndex),
    [findFiltering, originalIndexes],
  );
  const fromOriginal = useCallback(
    (originalIndex: number | null) => {
      if (originalIndex == null) return null;
      if (!findFiltering) return originalIndex;
      const at = originalIndexes.indexOf(originalIndex);
      return at < 0 ? null : at;
    },
    [findFiltering, originalIndexes],
  );

  const replacePlan = useMemo(
    () =>
      canEdit ? planResultReplacements(effectiveRows, columnNames, findQuery, findReplacement) : [],
    [canEdit, columnNames, findQuery, findReplacement, effectiveRows],
  );

  const dirtyByRow = useMemo(() => {
    if (pendingEdits.size === 0) return null;
    const map = new Map<number, DirtyRowEntry>();
    for (const edit of pendingEdits.values()) {
      const existing = map.get(edit.row);
      const cell = {
        columnName: edit.column,
        original: edit.original as any,
        next: edit.value as any,
      };
      if (existing) {
        existing.changedColumns = [...existing.changedColumns, edit.column];
        existing.cells = [...existing.cells, cell];
      } else {
        map.set(edit.row, {
          identityKey: String(edit.row),
          state: 'dirty',
          changedColumns: [edit.column],
          cells: [cell],
        });
      }
    }
    return map;
  }, [pendingEdits]);

  const pendingCount = pendingEdits.size;

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
        <span className="ml-auto flex items-center gap-1">
          <button
            type="button"
            data-testid="result-find-toggle"
            className="flex items-center gap-1 rounded-md px-1.5 py-0.5 text-fg-muted hover:bg-surface-raised hover:text-fg"
            title={t('resultFind.open')}
            aria-pressed={findOpen}
            onClick={() => setFindOpen((open) => !open)}
          >
            <Search className="h-3 w-3" />
            {t('resultFind.openLabel')}
          </button>
        </span>
      </div>
    ),
    [findOpen, queryResultLimit, result, t],
  );

  const handleCellDoubleClick = useCallback(
    (row: number, col: string) => {
      onRowDetail?.(row);
      if (!canEdit) {
        setEditingCell(null);
        return;
      }
      const originalRow = toOriginal(row);
      const editability = resolveResultEditability(
        legacyVerified,
        serverVerdict,
        baseRows,
        result.columns,
        originalRow,
      );
      if (!editability.writable) {
        setEditingCell(null);
        setRowRefusal(
          resolveRowAddressability(
            serverVerdict.identityColumns ?? [],
            baseRows,
            result.columns,
            originalRow,
          ),
        );
        return;
      }
      setRowRefusal(null);
      if (originChannelWritable(serverVerdict) && !isColumnEditable(serverVerdict, col)) {
        setEditingCell(null);
        return;
      }
      setEditingCell({ row: originalRow, col });
    },
    [onRowDetail, canEdit, legacyVerified, serverVerdict, result, toOriginal, baseRows],
  );

  const stageCellEdit = useCallback(
    (row: number, column: string, value: unknown) => {
      const colIdx = columnNames.indexOf(column);
      if (colIdx < 0) return;
      const original = baseRows[row]?.[colIdx];
      const same = JSON.stringify(original) === JSON.stringify(value);
      setCommitNotice(null);
      setEditError(null);
      setEditErrorHint(null);
      setPendingEdits((prev) => {
        const next = new Map(prev);
        const key = editKey(row, column);
        if (same) next.delete(key);
        else next.set(key, { row, column, original, value });
        return next;
      });
      setEditingCell(null);
    },
    [baseRows, columnNames],
  );

  const buildPreviewRequest = useCallback(
    (edit: PendingEdit): ResultCellChangeRequest | null => {
      const dbSessionId = result.runtimeBinding?.handle.dbSessionId;
      const relation = result.source?.relation;
      const tableName = relation?.object?.name ?? verdict?.table?.name;
      const connId = connectionId ?? relation?.connectionId;
      const dType = driverType;
      if (!dbSessionId || !tableName || !connId || !dType) return null;
      const identityColumns = serverVerdict.identityColumns ?? [];
      if (identityColumns.length === 0) return null;
      const rowValues = baseRows[edit.row];
      if (!rowValues) return null;
      const rowIdentity: Record<string, any> = {};
      for (const idCol of identityColumns) {
        const idx = columnNames.indexOf(idCol.column);
        if (idx < 0) return null;
        const v = rowValues[idx];
        if (v == null) return null;
        rowIdentity[idCol.sourceColumn] = v;
      }
      const editable = (serverVerdict.editableColumns ?? []).find((c) => c.column === edit.column);
      const sourceColumn = editable?.sourceColumn ?? edit.column;
      const db = database ?? relation?.namespace?.database ?? null;
      const sch = schema ?? relation?.namespace?.schema ?? null;
      return {
        connectionId: connId,
        dbSessionId,
        driverType: dType,
        database: db,
        schema: sch,
        table: tableName,
        sql: result.sql,
        resultColumns: columnNames,
        identityColumns,
        rowIdentity,
        column: edit.column,
        sourceColumn,
        originalValue: (edit.original as any) ?? null,
        value: (edit.value as any) ?? null,
        rowCount: 1,
      };
    },
    [
      result,
      verdict,
      serverVerdict,
      baseRows,
      columnNames,
      connectionId,
      driverType,
      database,
      schema,
    ],
  );

  const handlePreview = useCallback(async () => {
    const edits = [...pendingEdits.values()];
    if (edits.length === 0) return;
    setPendingBusy(true);
    setEditError(null);
    setEditErrorHint(null);
    setCommitNotice(null);
    try {
      const plans: RowChangePlan[] = [];
      for (const edit of edits) {
        const req = buildPreviewRequest(edit);
        if (!req) throw new Error(t('resultFind.notWritable'));
        plans.push(await queryCommands.previewResultCellChange(req));
      }
      if (plans.length === 1) {
        setPreviewPlan(plans[0]);
      } else {
        const first = plans[0]!;
        setPreviewPlan({
          ...first,
          planId: first.planId,
          fingerprint: `${first.fingerprint} (+${plans.length - 1})`,
          updates: plans.flatMap((p) => p.updates ?? []),
          deletes: plans.flatMap((p) => p.deletes ?? []),
          inserts: plans.flatMap((p) => p.inserts ?? []),
          warnings: plans.flatMap((p) => p.warnings ?? []),
        } as RowChangePlan);
      }
      setPreviewOpen(true);
    } catch (e) {
      setEditError(e instanceof Error ? e.message : String(e));
      setEditErrorHint(null);
    } finally {
      setPendingBusy(false);
    }
  }, [pendingEdits, buildPreviewRequest, t]);

  const handleCommit = useCallback(async () => {
    if (pendingEdits.size === 0 || !onCommitResultCells) return;
    setPendingBusy(true);
    setEditError(null);
    setEditErrorHint(null);
    setCommitNotice(null);
    try {
      const edits = [...pendingEdits.values()].map((e) => ({
        row: e.row,
        column: e.column,
        value: e.value,
      }));
      await onCommitResultCells(edits);
      setPendingEdits(new Map());
      setPreviewPlan(null);
      setCommitNotice(t('query.resultEditCommitted'));
    } catch (e) {
      setEditError(e instanceof Error ? e.message : String(e));
      setEditErrorHint(t('query.resultEditCommitRetryHint'));
    } finally {
      setPendingBusy(false);
    }
  }, [pendingEdits, onCommitResultCells, t]);

  const handleRollback = useCallback(() => {
    setPendingEdits(new Map());
    setPreviewPlan(null);
    setEditError(null);
    setEditErrorHint(null);
    setCommitNotice(null);
  }, []);

  if (isMutationExecution(result)) {
    return (
      <div className="flex min-h-0 min-w-0 flex-1 flex-col" {...tid('result-workspace-table')}>
        <ExecutionSummaryCard result={result} statusBar={statusBar} />
      </div>
    );
  }

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col" {...tid('result-workspace-table')}>
      {pendingCount > 0 && (
        <TablePendingChangesBar
          pendingCount={pendingCount}
          updateCount={pendingCount}
          deleteCount={0}
          insertCount={0}
          busy={pendingBusy}
          onPreview={() => void handlePreview()}
          onCommit={() => void handleCommit()}
          onRollback={handleRollback}
        />
      )}
      <PendingPlanDialog
        plan={previewPlan}
        open={previewOpen}
        onClose={() => setPreviewOpen(false)}
      />
      {findOpen && (
        <ResultFindBar
          columns={columnNames}
          query={findQuery}
          onQueryChange={setFindQuery}
          replacement={findReplacement}
          onReplacementChange={setFindReplacement}
          matchedRows={originalIndexes.length}
          totalRows={result.rows.length}
          plan={replacePlan}
          onApplyReplacement={(item) => {
            stageCellEdit(item.row, item.column, item.value);
            return Promise.resolve();
          }}
          canReplace={canEdit}
          replaceReason={safeMode ? t('resultFind.safeMode') : t('resultFind.notWritable')}
          onClose={() => {
            setFindOpen(false);
            setFindQuery(EMPTY_RESULT_FIND_QUERY);
          }}
        />
      )}
      <div
        className="flex min-h-0 flex-1 flex-col overflow-y-auto"
        data-testid="result-grid-scroll"
        onKeyDown={(event) => {
          if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f') {
            event.preventDefault();
            setFindOpen(true);
          }
        }}
      >
        {readOnlyReason && (
          <div data-dt-grid-readonly-reason={readOnlyReason} className="p-2 text-xs text-fg-muted">
            {result.source?.writableMapping === 'verified' && !result.runtimeBinding
              ? t('query.resultEditRequiresRefresh')
              : t(gridReadOnlyMessageKey(readOnlyReason))}
          </div>
        )}
        {rowRefusal && (
          <div data-dt-grid-row-refusal={rowRefusal} className="p-2 text-xs text-fg-muted">
            {t(gridReadOnlyMessageKey(rowRefusal))}
          </div>
        )}
        {commitNotice && (
          <div data-testid="result-edit-notice" className="p-2 text-sm text-green-500">
            {commitNotice}
          </div>
        )}
        {editError && (
          <div role="alert" className="p-2 text-sm text-red-400">
            <div>{editError}</div>
            {editErrorHint && <div className="mt-1 text-xs text-fg-muted">{editErrorHint}</div>}
          </div>
        )}
        <DataTable
          columns={columnDefs}
          rows={viewRows}
          dirtyByRow={dirtyByRow as any}
          cellTypeHints={cellTypeHints}
          statusBar={statusBar}
          rowHeight={gridRowHeight}
          alternatingRows={gridAlternatingRows}
          showRowNumbers={gridRowNumbers}
          scrollbarAutoHide={gridAutoHideScrollbar}
          editingCell={
            canEdit && editingCell
              ? { row: fromOriginal(editingCell.row) ?? editingCell.row, col: editingCell.col }
              : null
          }
          onCellDoubleClick={handleCellDoubleClick}
          onCellEdit={(row, column, value) => {
            if (!canEdit) {
              setEditingCell(null);
              return;
            }
            setEditError(null);
            stageCellEdit(toOriginal(row), column, value);
          }}
          onCellEditCancel={() => setEditingCell(null)}
          enableSetNull={false}
          onRowClick={(row) => onRowDetail?.(toOriginal(row))}
          highlightedRow={fromOriginal(rowDetailIndex)}
          exportTableName="query_result"
          dbSessionId={result.runtimeBinding?.handle.dbSessionId}
          dataExportCapability={dataExportCapability}
          presentationSnapshot={presentationSnapshot}
          className="mr-1 min-h-[9rem] rounded-b-md rounded-t-none border-x border-b border-t-0"
        />
        {result.rows.length === 0 && (
          <div className="flex shrink-0 items-center justify-center border-t border-edge px-3 py-3 text-sm text-fg-muted">
            {t('sqlFile.noResults')}
          </div>
        )}
        {findFiltering && viewRows.length === 0 && result.rows.length > 0 && (
          <div
            data-testid="result-find-no-matches"
            className="flex shrink-0 items-center justify-center border-t border-edge px-3 py-3 text-sm text-fg-muted"
          >
            {t('resultFind.noRows')}
          </div>
        )}
      </div>
    </div>
  );
}
