/**
 * The grid's clipboard orchestration (DB-03 §5 Step 7).
 *
 * Everything that needs both worlds lives here: the selection model's coordinates, the
 * panel's `ColumnSchema`, the store's batch staging and the confirmation dialog. `DataTable`
 * only reports gestures (it owns no schema), `TableView` only wires it up.
 *
 * Order matters and is enforced below: **plan → confirm → stage**. Staging first and asking
 * afterwards would be a paste the user never agreed to.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { classifyDataType } from '../lib/dataTypeColors';
import {
  GRID_PASTE_CHUNK_CELLS,
  GRID_PASTE_CONFIRM_CELLS,
  GRID_PASTE_MAX_CELLS,
  buildRowsMatrix,
  buildSelectionMatrix,
  parseClipboardMatrix,
  serializeCells,
  type ClipboardFormat,
  type GridClipboardAnchor,
} from '../lib/gridClipboard';
import {
  peekClipboardSource,
  sourceForClipboardText,
  pendingCellKey,
  planPaste,
  recordClipboardSource,
  tableChangeKey,
  type PasteOverflow,
  type PastePlan,
} from '../lib/gridPastePlan';
import type { PasteProblem } from '../lib/gridErrors';
import type {
  CellPasteWrite,
  PendingRowChange,
  TableChangeContext,
} from '../lib/tableChanges';
import { requirePlatformServices } from '@datazen/backend-client';
import {
  cellCoordsInRange,
  normalizeSelectionRange,
  rowsCoveredBySelection,
  type GridSelection,
} from '../stores/tableData/gridSelection';
import type { StagePasteWritesResult } from '../stores/tableData/pendingEditActions';
import type { ColumnSchema } from '../types';
import type {
  GridClipboardHandlers,
  GridCopyTarget,
} from '../components/DataTable/gridClipboardBridge';
import { useConfirmDialog } from './useConfirmDialog';
import { useClipboardCopyFeedback } from './useClipboardCopyFeedback';
import { useI18n } from './useI18n';

/** How long "copied" stays visible; the grid's own window length. */
const COPY_FEEDBACK_MS = 1200;
/** Stored problems are capped: a 5 000-row block can produce thousands of identical ones. */
const MAX_REPORTED_PROBLEMS = 100;

export interface PasteSummary {
  /** Cells the plan wanted to write. */
  cells: number;
  /** Cells that really entered `pendingChanges`. */
  staged: number;
  /** Cells that were skipped, for any reason. */
  skipped: number;
  /** Staged changes that were overwritten. */
  overwrittenPending: number;
  rows: number;
  columns: number;
  cancelled: boolean;
}

export interface UseGridClipboardOptions {
  panelId: string;
  isEditable: boolean;
  columns: readonly ColumnSchema[];
  /** Displayed column names in render order. */
  columnOrder: readonly string[];
  /** The panel's row records, by column name. */
  rows: readonly Record<string, unknown>[];
  pendingChanges: Map<string, PendingRowChange>;
  /** Row index → staged identity key; used only to count overwritten staged changes. */
  rowIdentityAnchors: Map<number, string>;
  selectedRows: ReadonlySet<number>;
  gridSelection: GridSelection;
  context: TableChangeContext | null;
  /** Human-readable name of the target table, for the cross-table confirmation. */
  tableLabel: string;
  overflow?: PasteOverflow;
  /** Stage the block; supplied by the panel's action table. */
  stagePasteWrites: (writes: readonly CellPasteWrite[]) => StagePasteWritesResult;
  /** Report one problem (the owner renders it; `null` cells are highlighted by the grid). */
  onProblem?: (problem: PasteProblem) => void;
  /** "Read-only" refusal, shared with the cell editor's tip. */
  showReadOnlyTip?: () => void;
}

export interface UseGridClipboardResult {
  handlers: GridClipboardHandlers;
  /** "Copy Selected Cells As …" — the asynchronous, fallback-chain copy. */
  copySelectedCellsAs: (
    format: ClipboardFormat,
    options?: { includeHeader?: boolean },
  ) => Promise<boolean>;
  copyFeedback: { copied: boolean };
  /** Cells the last paste reported a problem for, as `${rowIndex}:${columnName}`. */
  cellErrors: ReadonlySet<string>;
  problems: readonly PasteProblem[];
  lastPaste: PasteSummary | null;
  clearPasteResult: () => void;
  /** The paste flow itself, awaitable: the native `paste` handler starts it and returns `true`. */
  pasteText: (text: string, anchor: GridClipboardAnchor) => Promise<boolean>;
  /** The confirmation dialog element; render it once in the panel. */
  confirmElement: React.ReactNode;
}

/** Render one structured paste problem as text (shared by the banner and the tests). */
export function describePasteProblem(
  problem: PasteProblem,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  switch (problem.code) {
    case 'grid.paste.empty':
      return t('dataTable.pasteEmpty');
    case 'grid.paste.tooLarge':
      return t('dataTable.pasteTooLarge', {
        cells: problem.detail ?? '0',
        // From the constant the parser enforces. A literal here drifts silently the day the
        // limit moves, and the user would be told a number the code does not obey.
        maxCells: String(GRID_PASTE_MAX_CELLS),
      });
    case 'grid.paste.raggedRow':
      return problem.detail ?? t('dataTable.pasteRaggedRow', { row: '', count: '', expected: '' });
    case 'grid.paste.unclosedQuote':
      return t('dataTable.pasteUnclosedQuote');
    case 'grid.paste.outOfBounds':
      return t('dataTable.pasteClippedToPage', { count: problem.detail ?? '0' });
    case 'grid.paste.noWritableCell':
      return t('dataTable.pasteNoWritableColumn');
    case 'grid.paste.notNullEmpty':
      return t('dataTable.pasteNotAllowedEmpty', {
        row: (problem.row ?? 0) + 1,
        column: problem.column ?? '',
      });
    case 'grid.paste.identityChanged':
      return t('dataTable.pasteIdentityChanged', {
        row: (problem.row ?? 0) + 1,
        column: problem.column ?? '',
      });
    case 'grid.paste.columnMismatch':
      return t('dataTable.pasteDroppedColumns', {
        count: problem.detail ? problem.detail.split(', ').length : 0,
        names: problem.detail ?? '',
      });
    case 'grid.paste.crossTable':
      // The status line's wording, not the dialog's title: after the user confirmed, a line
      // reading "Paste into another table?" would look like an unanswered question.
      return t('dataTable.pasteCrossTableNotice', { table: problem.detail ?? '' });
    case 'grid.cell.readOnly':
      // No column ⇒ the whole panel is read-only (the paste never reached a column); with a
      // column ⇒ that one column cannot be written and the rest still lands.
      //
      // The **column name** is in `column`; `detail` carries the *reason* (`auto-increment`),
      // not a name list. Reading `detail` as names told the user "cannot be written: for
      // auto-increment columns" — a sentence that names no column at all.
      return problem.column
        ? t('dataTable.pasteReadOnlyColumn', { count: 1, columns: problem.column })
        : t('dataTable.readOnlyPasteDisabled');
    case 'grid.cell.noRowIdentity':
      return t('dataTable.pasteNoRowIdentity');
    case 'grid.clipboard.writeFailed':
      return t('dataTable.copyFailed');
    case 'grid.clipboard.readFailed':
      return t('dataTable.pasteReadFailed', { shortcut: '⌘V' });
    default:
      // An unknown code must be shown, not swallowed (00-contracts §8.1): the raw detail is
      // the only thing the user has.
      return problem.detail ?? problem.code;
  }
}

function nextFrame(): Promise<void> {
  return new Promise<void>((resolve) => {
    if (typeof window === 'undefined') {
      resolve();
      return;
    }
    let settled = false;
    const done = (): void => {
      if (settled) return;
      settled = true;
      resolve();
    };
    if (typeof window.requestAnimationFrame === 'function') {
      window.requestAnimationFrame(() => done());
      // A hidden/background frame never fires; the timer keeps the batch from stalling.
      window.setTimeout(done, 32);
      return;
    }
    window.setTimeout(done, 0);
  });
}

export function useGridClipboard(options: UseGridClipboardOptions): UseGridClipboardResult {
  const {
    panelId: _panelId,
    isEditable,
    columns,
    columnOrder,
    rows,
    pendingChanges,
    rowIdentityAnchors,
    selectedRows,
    gridSelection,
    context,
    tableLabel,
    overflow = 'expand',
    stagePasteWrites,
    onProblem,
    showReadOnlyTip,
  } = options;
  const { t } = useI18n();
  const copyFeedback = useClipboardCopyFeedback(COPY_FEEDBACK_MS);
  const [confirm, confirmElement] = useConfirmDialog();
  const [cellErrors, setCellErrors] = useState<ReadonlySet<string>>(() => new Set<string>());
  const [problems, setProblems] = useState<readonly PasteProblem[]>([]);
  const [lastPaste, setLastPaste] = useState<PasteSummary | null>(null);
  /**
   * Feedback for the **native** copy path. That path writes the clipboard itself (inside the
   * gesture, which is the only way `setData` is allowed), so it needs the timer but not the
   * write; `copyFeedback.copy` is only used by the asynchronous menu/shortcut path.
   */
  const [nativeCopied, setNativeCopied] = useState(false);
  const nativeTimerRef = useRef<number | null>(null);
  useEffect(
    () => () => {
      if (nativeTimerRef.current !== null) {
        window.clearTimeout(nativeTimerRef.current);
        nativeTimerRef.current = null;
      }
    },
    [],
  );

  const typeByColumn = useMemo(() => {
    const map = new Map<string, string>();
    for (const column of columns) map.set(column.name, column.dataType);
    return map;
  }, [columns]);

  /** The normalized main region of the selection, if there is one. */
  const selectionRange = useMemo(
    () =>
      gridSelection.mode === 'cell'
        ? normalizeSelectionRange(gridSelection, columnOrder)
        : null,
    [columnOrder, gridSelection],
  );
  /** Rows the row-level actions act on — derived, never written back into `selectedRows`. */
  const coveredRows = useMemo(
    () => rowsCoveredBySelection({ ...gridSelection, selectedRows }),
    [gridSelection, selectedRows],
  );

  const report = useCallback(
    (problem: PasteProblem) => {
      if (problem.row !== null && problem.column !== null) {
        const key = pendingCellKey(problem.row, problem.column);
        setCellErrors((previous) => {
          const next = new Set(previous);
          next.add(key);
          return next;
        });
      }
      setProblems((previous) =>
        previous.length >= MAX_REPORTED_PROBLEMS ? previous : [...previous, problem],
      );
      onProblem?.(problem);
    },
    [onProblem],
  );

  const rememberSource = useCallback(
    (columnNames: readonly string[], text: string) => {
      recordClipboardSource(
        {
          tableKey: tableChangeKey(context),
          columnNames: [...columnNames],
          // DB-05 will supply the finer `CellType`; the family is what exists in commit order.
          columnTypes: columnNames.map((name) => classifyDataType(typeByColumn.get(name))),
        },
        // The text is stored with the source so a paste can tell whether the clipboard still
        // holds it. Without it, an Excel copy would leave a source that still claims "from T".
        text,
      );
    },
    [context, typeByColumn],
  );

  const buildText = useCallback(
    (
      target: GridCopyTarget,
      format: ClipboardFormat,
      includeHeader?: boolean,
    ): { text: string; target: GridCopyTarget } | null => {
      if (target.mode === 'cell') {
        const { range } = target;
        const left = columnOrder.indexOf(range.leftColumn);
        const right = columnOrder.indexOf(range.rightColumn);
        if (left < 0 || right < 0 || right < left) return null;
        const columnNames = columnOrder.slice(left, right + 1);
        const coords = cellCoordsInRange(
          {
            start: { rowIndex: range.topRow, columnName: range.leftColumn },
            end: { rowIndex: range.bottomRow, columnName: range.rightColumn },
          },
          columnOrder,
        );
        if (coords.length === 0) return null;
        const matrix = buildSelectionMatrix(rows, coords);
        const text = serializeCells(matrix, { format, columnNames, includeHeader });
        if (text !== '') rememberSource(columnNames, text);
        return { text, target };
      }
      const matrix = buildRowsMatrix(rows, columnOrder, target.rowIndices);
      if (matrix.length === 0) return null;
      const text = serializeCells(matrix, { format, columnNames: columnOrder, includeHeader });
      if (text !== '') rememberSource(columnOrder, text);
      return { text, target };
    },
    [columnOrder, rememberSource, rows],
  );

  const buildCopyText = useCallback(
    (target: GridCopyTarget): string | null => buildText(target, 'tsv')?.text ?? null,
    [buildText],
  );

  const onCopied = useCallback((_target: GridCopyTarget, ok: boolean) => {
    setNativeCopied(ok);
    if (nativeTimerRef.current !== null) window.clearTimeout(nativeTimerRef.current);
    if (!ok) {
      nativeTimerRef.current = null;
      return;
    }
    nativeTimerRef.current = window.setTimeout(() => {
      nativeTimerRef.current = null;
      setNativeCopied(false);
    }, COPY_FEEDBACK_MS);
  }, []);

  const copySelectedCellsAs = useCallback(
    async (
      format: ClipboardFormat,
      copyOptions?: { includeHeader?: boolean },
    ): Promise<boolean> => {
      const target: GridCopyTarget | null = selectionRange
        ? {
            mode: 'cell',
            range: {
              topRow: selectionRange.start.rowIndex,
              bottomRow: selectionRange.end.rowIndex,
              leftColumn: selectionRange.start.columnName,
              rightColumn: selectionRange.end.columnName,
            },
          }
        : coveredRows.size > 0
          ? { mode: 'row', rowIndices: [...coveredRows].sort((left, right) => left - right) }
          : null;
      if (!target) return false;
      // The name-alignment bookkeeping has to see the same columns the text does.
      const built = buildText(target, format, copyOptions?.includeHeader);
      if (!built) return false;
      const ok = await copyFeedback.copy(built.text);
      if (!ok) report({ code: 'grid.clipboard.writeFailed', row: null, column: null });
      return ok;
    },
    [buildText, copyFeedback, coveredRows, report, selectionRange],
  );

  const pendingKeys = useMemo(() => {
    const keys = new Set<string>();
    for (const [rowIndex, identityKey] of rowIdentityAnchors) {
      const change = pendingChanges.get(identityKey);
      if (!change) continue;
      for (const column of change.changedColumns) {
        keys.add(pendingCellKey(rowIndex, column));
      }
    }
    return keys;
  }, [pendingChanges, rowIdentityAnchors]);

  const confirmOptionsFor = useCallback(
    (plan: PastePlan) => {
      const rowsWritten = plan.bounds ? plan.bounds.bottomRow - plan.bounds.topRow + 1 : 0;
      const columnsWritten = plan.droppedColumns.length > 0
        ? plan.matchedColumns
        : plan.totalColumns;
      const source = peekClipboardSource();
      const body =
        plan.crossTable === null
          ? t('dataTable.pasteUnknownSourceBody', {
              total: columnsWritten,
              target: tableLabel,
            })
          : t('dataTable.pasteCrossTableBody', {
              source: source?.tableKey ?? '',
              target: tableLabel,
              matched: plan.matchedColumns,
              total: plan.totalColumns,
            });
      const dropped =
        plan.droppedColumns.length > 0
          ? ` ${t('dataTable.pasteDroppedColumns', {
              count: plan.droppedColumns.length,
              names: plan.droppedColumns.slice(0, 5).join(', '),
            })}`
          : '';
      return {
        title:
          plan.writes.length > GRID_PASTE_CONFIRM_CELLS
            ? t('dataTable.pasteConfirmTitle', { cells: plan.writes.length })
            : t('dataTable.pasteCrossTableTitle'),
        message:
          plan.writes.length > GRID_PASTE_CONFIRM_CELLS
            ? t('dataTable.pasteConfirmMessage', {
                rows: rowsWritten,
                cols: columnsWritten,
                overwritten: plan.overwrittenPending,
              })
            : body,
        description: dropped.trim() || undefined,
        // The expensive mistake is a wrong write, so the safe answer is the default.
        confirmLabel: t('dataTable.pasteToSelectedCells'),
        cancelLabel: t('common.cancel'),
        kind: 'warning' as const,
      };
    },
    [t, tableLabel],
  );

  const pasteText = useCallback(
    async (text: string, anchor: GridClipboardAnchor): Promise<boolean> => {
      setProblems([]);
      setCellErrors(new Set<string>());
      if (!isEditable) {
        // Consume the event (so the browser does not paste into the page) but write nothing.
        showReadOnlyTip?.();
        report({ code: 'grid.cell.readOnly', row: null, column: null, detail: 'readOnly' });
        return true;
      }

      const parsed = parseClipboardMatrix(text);
      if (!parsed.ok) {
        if (parsed.code === 'grid.paste.raggedRow') {
          report({
            code: 'grid.paste.raggedRow',
            row: parsed.row ?? null,
            column: null,
            detail: t('dataTable.pasteRaggedRow', {
              row: (parsed.row ?? 0) + 1,
              count: parsed.actual ?? 0,
              expected: parsed.expected ?? 0,
            }),
          });
        } else {
          report({
            code: parsed.code,
            row: null,
            column: null,
            detail: String(parsed.cells),
          });
        }
        return true;
      }
      for (const problem of parsed.problems) report(problem);

      const plan = planPaste({
        matrix: parsed.matrix,
        anchor,
        columnOrder,
        columns,
        region: selectionRange
          ? {
              topRow: selectionRange.start.rowIndex,
              bottomRow: selectionRange.end.rowIndex,
              leftColumn: selectionRange.start.columnName,
              rightColumn: selectionRange.end.columnName,
            }
          : null,
        pageRowCount: rows.length,
        overflow,
        pendingCellKeys: pendingKeys,
        // Only a source the clipboard still holds may steer alignment or waive the
        // confirmation; a stale one is treated exactly like an external paste.
        source: sourceForClipboardText(text),
        context,
      });

      // `plan.problems` is the complete diagnostic list: the planner aggregates dropped
      // columns into one `columnMismatch` (with the names) and reports each unwritable column
      // with its name. Reporting them here again duplicated the messages — and the copy built
      // from `plan.droppedColumns` had no `detail`, so it rendered as "0 … : ''".
      //
      // Reported **after** the user agrees, because these lines describe what happened. The
      // cross-table notice in particular reads "Pasted from another table into X", which is a
      // lie once the user answers "no" — and the dialog already said it before the decision.
      const reportPlanProblems = () => {
        if (plan.crossTable !== false) {
          report({ code: 'grid.paste.crossTable', row: null, column: null, detail: tableLabel });
        }
        for (const problem of plan.problems) report(problem);
      };
      if (plan.writes.length === 0) {
        reportPlanProblems();
        setLastPaste({
          cells: 0,
          staged: 0,
          skipped: plan.skipped,
          overwrittenPending: 0,
          rows: 0,
          columns: 0,
          cancelled: false,
        });
        return true;
      }

      if (plan.requiresConfirmation || plan.writes.length > GRID_PASTE_CONFIRM_CELLS) {
        const confirmed = await confirm(confirmOptionsFor(plan));
        if (!confirmed) {
          setLastPaste({
            cells: plan.writes.length,
            staged: 0,
            skipped: 0,
            overwrittenPending: plan.overwrittenPending,
            rows: 0,
            columns: 0,
            cancelled: true,
          });
          return true;
        }
      }

      reportPlanProblems();
      let staged = 0;
      let skipped = plan.skipped;
      for (let start = 0; start < plan.writes.length; start += GRID_PASTE_CHUNK_CELLS) {
        const chunk = plan.writes.slice(start, start + GRID_PASTE_CHUNK_CELLS);
        const result = stagePasteWrites(chunk);
        staged += result.staged;
        skipped += result.skipped;
        for (const problem of result.problems) {
          report({
            code: problem.code,
            row: problem.rowIndex,
            column: problem.columnName,
            detail: problem.detail,
          });
        }
        if (start + GRID_PASTE_CHUNK_CELLS < plan.writes.length) await nextFrame();
      }
      setLastPaste({
        cells: plan.writes.length,
        staged,
        skipped,
        overwrittenPending: plan.overwrittenPending,
        rows: plan.bounds ? plan.bounds.bottomRow - plan.bounds.topRow + 1 : 0,
        columns: plan.bounds
          ? columnOrder.indexOf(plan.bounds.rightColumn) -
            columnOrder.indexOf(plan.bounds.leftColumn) +
            1
          : 0,
        cancelled: false,
      });
      return true;
    },
    [
      columnOrder,
      columns,
      confirm,
      confirmOptionsFor,
      context,
      isEditable,
      overflow,
      pendingKeys,
      report,
      rows.length,
      selectionRange,
      showReadOnlyTip,
      stagePasteWrites,
      t,
      tableLabel,
    ],
  );

  const handlers = useMemo<GridClipboardHandlers>(
    () => ({
      buildCopyText,
      onCopied,
      onPasteText: (text, anchor) => {
        void pasteText(text, anchor);
        return true;
      },
      onPasteFromClipboard: (anchor) => {
        void (async () => {
          try {
            const text = await requirePlatformServices().readClipboard();
            await pasteText(text ?? '', anchor);
          } catch {
            // A missing platform binding or a denied permission is reported, never guessed
            // around (B23): there is no "fall back to something on the page".
            report({ code: 'grid.clipboard.readFailed', row: null, column: null });
          }
        })();
      },
      canCopy: selectionRange !== null || coveredRows.size > 0,
      canPaste: isEditable && selectionRange !== null,
    }),
    [buildCopyText, coveredRows.size, isEditable, onCopied, pasteText, report, selectionRange],
  );

  const clearPasteResult = useCallback(() => {
    setProblems([]);
    setCellErrors(new Set<string>());
    setLastPaste(null);
  }, []);

  return {
    handlers,
    copySelectedCellsAs,
    copyFeedback: { copied: nativeCopied || copyFeedback.copied },
    cellErrors,
    problems,
    lastPaste,
    clearPasteResult,
    pasteText,
    confirmElement,
  };
}
