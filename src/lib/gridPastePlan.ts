/**
 * Paste planning: "where does this block land?" (DB-03 §3.2b).
 *
 * Split out of `gridClipboard.ts` when that module crossed its size ceiling: parsing/serializing
 * a block and deciding where to write it are two different responsibilities, and the planner is
 * the half that has to be read against the cross-table rules (L1/L2/L3), so it gets its own file
 * and its own test (`gridClipboard.crossTable.test.ts`).
 *
 * Pure functions only: no Zustand, no `t()`, no DOM.
 */
import type { DataTypeFamily } from './dataTypeColors';
import type { ColumnSchema, Value } from '../types';
import { columnWritability, type CellPasteWrite, type TableChangeContext } from './tableChanges';
import type { PasteProblem } from './gridErrors';
import {
  GRID_PASTE_MAX_BYTES,
  GRID_PASTE_MAX_CELLS,
  type ClipboardMatrix,
  type GridClipboardAnchor,
  type GridClipboardRange,
} from './gridClipboard';

// ── Copy source tracking (cross-table guard, L1) ─────────────────────────────────────

/**
 * Where the last in-app copy came from. `columnNames[index]` belongs to matrix column
 * `index`; `null` on either side means "unknown".
 */
export interface ClipboardSource {
  /** Table identity, from {@link tableChangeKey}. */
  tableKey: string;
  /** Source column names, in matrix order. A TSV copy carries no header, but this does. */
  columnNames: readonly string[];
  /** Source column type families (`DataTypeFamily`; DB-05 will offer the finer `CellType`). */
  columnTypes: readonly DataTypeFamily[];
}

/**
 * The last copy this application made.
 *
 * **Module state on purpose**: it describes the system clipboard, not the table state of a
 * session — and it must be readable from a different panel.
 *
 * It is **not** bound to what is on the clipboard right now. Copying in Excel invalidates
 * it, and another DataZen window can overwrite the clipboard without touching this value.
 * It may therefore only **raise** the gate (prove "same table" to skip a confirmation); it
 * must never be used to auto-approve a write or to rewrite the alignment on its own.
 */
let lastCopySource: ClipboardSource | null = null;

export function recordClipboardSource(source: ClipboardSource): void {
  lastCopySource = source;
}

/** Read-only probe; tests may also use it. */
export function peekClipboardSource(): ClipboardSource | null {
  return lastCopySource;
}

export function clearClipboardSource(): void {
  lastCopySource = null;
}

/**
 * Stable identity of "the table" for cross-table detection.
 *
 * Three traps, all of them load-bearing:
 *
 * 1. **Never `dbSessionId`** — it changes on every reconnect, so the same table would look
 *    like another table and every paste would ask for confirmation.
 * 2. **Lower-case everything** — schemas are case-sensitive in PostgreSQL and not in MySQL;
 *    without folding, MySQL would report "another table" constantly.
 * 3. **`null` and `''` are the same table** — `database` is `null` for SQLite.
 */
export function tableChangeKey(context: TableChangeContext | null): string {
  if (!context) return '';
  const normalize = (value: string | null | undefined): string =>
    (value ?? '').trim().toLowerCase();
  return JSON.stringify([
    normalize(context.connectionId),
    normalize(context.database),
    normalize(context.schema),
    normalize(context.table),
  ]);
}

// ── Paste planning ───────────────────────────────────────────────────────────────────

export type PasteOverflow = 'expand' | 'clip';

export interface PastePlanOptions {
  matrix: ClipboardMatrix;
  /** Landing cell — always the **top-left** of the normalized region, never the raw anchor. */
  anchor: GridClipboardAnchor;
  /** Displayed columns, in render order. */
  columnOrder: readonly string[];
  /** The page's column metadata (`TableState.columns`). */
  columns: readonly ColumnSchema[];
  /** The original region when pasting into a region; `null` for a single cell. */
  region: GridClipboardRange | null;
  /** Rows on the current page: both the expansion bound and the truncation bound. */
  pageRowCount: number;
  /** Default `'expand'` (the PRD behaviour). */
  overflow?: PasteOverflow;
  /** Case-insensitive NULL literal. Default `'NULL'`; `\N` is always recognized as well. */
  nullLiteral?: string;
  /** `(rowIndex, columnName)` keys already staged, to count what the paste will overwrite. */
  pendingCellKeys?: ReadonlySet<string>;
  /** Where the clipboard content came from, when this application copied it (L1). */
  source?: ClipboardSource | null;
  /** The target table's context, used only for the same-table test (L1). */
  context?: TableChangeContext | null;
}

export interface PastePlan {
  /** **Only** the cells to write; "do not write" cells are simply absent from this list. */
  writes: CellPasteWrite[];
  /** Area actually written; `null` when nothing is written. */
  bounds: GridClipboardRange | null;
  problems: PasteProblem[];
  /** Cells skipped (read-only column, NOT NULL conflict, out of bounds, off-grid). */
  skipped: number;
  /** Staged changes the paste will overwrite. */
  overwrittenPending: number;
  /** Columns skipped wholesale because they are not writable (deduplicated, in order). */
  skippedColumns: string[];
  /** `null` = the source is unknown (it cannot be judged), which is **not** the same as `false`. */
  crossTable: boolean | null;
  /** Actual alignment. `unknown` = the block never reached planning (empty/oversized). */
  alignment: 'byName' | 'byPosition' | 'unknown';
  /** Source columns dropped by name alignment. **Not one of their cells is written.** */
  droppedColumns: string[];
  /** The caller **must** confirm before staging. Always `true` unless the source proves same-table. */
  requiresConfirmation: boolean;
  /** Source columns that found a same-named target column (0 when aligning by position). */
  matchedColumns: number;
  /** Matrix width, for the confirmation text. */
  totalColumns: number;
}

/** `(rowIndex, columnName)` → stable key, shared with `pendingCellKeys` and the tests. */
export function pendingCellKey(rowIndex: number, columnName: string): string {
  return `${rowIndex}:${columnName}`;
}

/** No matrix reached planning: nothing is aligned, nothing is written. */
function emptyPlan(width: number): PastePlan {
  return {
    writes: [],
    bounds: null,
    problems: [],
    skipped: 0,
    overwrittenPending: 0,
    skippedColumns: [],
    crossTable: null,
    alignment: 'unknown',
    droppedColumns: [],
    requiresConfirmation: true,
    matchedColumns: 0,
    totalColumns: width,
  };
}

function isNullText(text: string, nullLiteral: string): boolean {
  return text === '\\N' || text.toLowerCase() === nullLiteral.toLowerCase();
}

/**
 * Plan one paste: which cells to write, with which values, and what to tell the user.
 *
 * The three-state rule is honoured by **omission**: a cell the block does not cover, a
 * column that cannot be written, and a cell beyond the page are not present in `writes` at
 * all. `value: null` always means "write SQL NULL" and nothing else.
 *
 * Aligning by **column name** whenever the source is known (L2) is what keeps a paste from
 * silently writing `email` into the `amount` column when the two tables order their columns
 * differently; falling back to positions plus a mandatory confirmation (L3) is the last
 * line of defence when the source is unknown (an unknown source is treated as "possibly
 * another table", never as "probably the same one").
 */
export function planPaste(options: PastePlanOptions): PastePlan {
  const { matrix, anchor, columnOrder, columns, region, pageRowCount } = options;
  const overflow: PasteOverflow = options.overflow ?? 'expand';
  const nullLiteral = options.nullLiteral ?? 'NULL';
  const height = matrix.cells.length;
  const width = matrix.cells[0]?.length ?? 0;
  const plan = emptyPlan(width);

  if (height === 0 || width === 0) {
    plan.problems = [{ code: 'grid.paste.empty', row: null, column: null }];
    return plan;
  }
  const cellCount = height * width;
  if (cellCount > GRID_PASTE_MAX_CELLS || matrix.byteLength > GRID_PASTE_MAX_BYTES) {
    plan.problems = [
      { code: 'grid.paste.tooLarge', row: null, column: null, detail: String(cellCount) },
    ];
    return plan;
  }

  // ── L2: column-name alignment when the source is known ──────────────────────────────
  const sourceNames = options.source?.columnNames ?? null;
  let alignment: 'byName' | 'byPosition' = 'byPosition';
  let mapping: (string | null)[] = [];
  let droppedColumns: string[] = [];
  let matchedColumns = 0;
  if (sourceNames && sourceNames.length === width) {
    const targetNames = new Set(columnOrder);
    const byName = sourceNames.map((name) => (targetNames.has(name) ? name : null));
    const matched = byName.filter((name) => name !== null).length;
    if (matched > 0) {
      alignment = 'byName';
      mapping = byName;
      matchedColumns = matched;
      const dropped = new Set<string>();
      for (const name of sourceNames) {
        if (!targetNames.has(name)) dropped.add(name);
      }
      droppedColumns = [...dropped];
    }
  }
  if (alignment === 'byPosition') {
    const landing = region ? region.leftColumn : anchor.columnName;
    const leftIndex = columnOrder.indexOf(landing);
    if (leftIndex < 0) {
      plan.problems = [{ code: 'grid.paste.noWritableCell', row: null, column: null }];
      return plan;
    }
    mapping = [];
    for (let column = 0; column < width; column += 1) {
      mapping.push(columnOrder[leftIndex + column] ?? null);
    }
    matchedColumns = 0;
    droppedColumns = [];
  }

  // ── L1/L3: same-table proof and the confirmation gate ───────────────────────────────
  const sourceKey = options.source?.tableKey ?? null;
  const targetKey = options.context ? tableChangeKey(options.context) : null;
  const crossTable = sourceKey === null || targetKey === null ? null : sourceKey !== targetKey;
  const requiresConfirmation = !(sourceKey !== null && targetKey !== null && sourceKey === targetKey);

  plan.alignment = alignment;
  plan.crossTable = crossTable;
  plan.requiresConfirmation = requiresConfirmation;
  plan.matchedColumns = matchedColumns;
  plan.totalColumns = width;
  plan.droppedColumns = droppedColumns;

  const rowStart = anchor.rowIndex;
  if (!Number.isInteger(rowStart) || rowStart < 0 || pageRowCount <= 0) {
    plan.problems = [{ code: 'grid.paste.outOfBounds', row: null, column: null }];
    return plan;
  }

  const columnByName = new Map(columns.map((column) => [column.name, column]));
  const rightEdge =
    overflow === 'clip' && region ? columnOrder.indexOf(region.rightColumn) : columnOrder.length - 1;
  const bottomEdge =
    overflow === 'clip' && region ? Math.min(region.bottomRow, pageRowCount - 1) : pageRowCount - 1;

  const writes: CellPasteWrite[] = [];
  const problems: PasteProblem[] = [];
  const unwritableColumns = new Set<string>();
  const skippedColumnOrder: string[] = [];
  const reportedColumns = new Set<string>();
  let skipped = 0;
  let clipped = 0;
  let overwrittenPending = 0;
  let minRow = Number.POSITIVE_INFINITY;
  let maxRow = Number.NEGATIVE_INFINITY;
  let minColumn = Number.POSITIVE_INFINITY;
  let maxColumn = Number.NEGATIVE_INFINITY;

  for (let matrixRow = 0; matrixRow < height; matrixRow += 1) {
    const rowIndex = rowStart + matrixRow;
    if (rowIndex > bottomEdge) {
      clipped += width;
      continue;
    }
    for (let matrixColumn = 0; matrixColumn < width; matrixColumn += 1) {
      const columnName = mapping[matrixColumn] ?? null;
      if (columnName === null) {
        skipped += 1;
        continue;
      }
      const columnPosition = columnOrder.indexOf(columnName);
      // `clip` mode keeps a name-aligned paste inside its rectangle; a position-aligned one
      // simply runs out of displayed columns.
      if (
        columnPosition < 0 ||
        (alignment === 'byPosition' && overflow === 'clip' && columnPosition > rightEdge)
      ) {
        skipped += 1;
        continue;
      }
      const column = columnByName.get(columnName);
      const writability = columnWritability(column);
      if (!writability.writable) {
        skipped += 1;
        unwritableColumns.add(columnName);
        if (writability.reason === 'auto-increment' && !skippedColumnOrder.includes(columnName)) {
          skippedColumnOrder.push(columnName);
        }
        if (!reportedColumns.has(columnName)) {
          reportedColumns.add(columnName);
          problems.push({
            code: 'grid.cell.readOnly',
            row: null,
            column: columnName,
            detail: writability.reason ?? undefined,
          });
        }
        continue;
      }
      const text = matrix.cells[matrixRow]?.[matrixColumn] ?? '';
      const quoted = matrix.quoted[matrixRow]?.[matrixColumn] === true;
      let value: Value | null;
      if (quoted) {
        // A quoted field is an explicit string, even when it is empty or literally "NULL".
        value = text;
      } else if (text === '' || isNullText(text, nullLiteral)) {
        if (column?.nullable === false) {
          skipped += 1;
          problems.push({
            code: 'grid.paste.notNullEmpty',
            row: matrixRow,
            column: columnName,
          });
          continue;
        }
        value = null;
      } else {
        value = text;
      }
      if (options.pendingCellKeys?.has(pendingCellKey(rowIndex, columnName))) {
        overwrittenPending += 1;
      }
      writes.push({ rowIndex, columnName, value });
      minRow = Math.min(minRow, rowIndex);
      maxRow = Math.max(maxRow, rowIndex);
      minColumn = Math.min(minColumn, columnPosition);
      maxColumn = Math.max(maxColumn, columnPosition);
    }
  }

  if (alignment === 'byName' && droppedColumns.length > 0) {
    problems.push({
      code: 'grid.paste.columnMismatch',
      row: null,
      column: null,
      detail: droppedColumns.join(', '),
    });
  }
  if (clipped > 0) {
    problems.push({
      code: 'grid.paste.outOfBounds',
      row: null,
      column: null,
      detail: String(clipped),
    });
  }
  if (writes.length === 0 && unwritableColumns.size > 0) {
    const targetColumns = new Set(mapping.filter((name): name is string => name !== null));
    if (targetColumns.size > 0 && unwritableColumns.size >= targetColumns.size) {
      problems.push({ code: 'grid.paste.noWritableCell', row: null, column: null });
    }
  }

  plan.writes = writes;
  plan.problems = problems;
  plan.skipped = skipped + clipped;
  plan.overwrittenPending = overwrittenPending;
  plan.skippedColumns = skippedColumnOrder;
  plan.bounds =
    writes.length === 0
      ? null
      : {
          topRow: minRow,
          bottomRow: maxRow,
          leftColumn: columnOrder[minColumn] ?? anchor.columnName,
          rightColumn: columnOrder[maxColumn] ?? anchor.columnName,
        };
  return plan;
}
