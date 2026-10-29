/**
 * Grid keyboard semantics (DB-02).
 *
 * Pure logic only — no React, no store, no DOM — the same discipline as
 * `stores/tableData/gridSelection.ts` (DB-01). Everything that knows about
 * *geometry* belongs to 01: this module decides what a key **means** and
 * delegates every coordinate to 01's pure functions (`moveCellFocus`, `coordAt`,
 * `clampCellCoord`, `selectionFromAnchorFocus`). It never recomputes a region.
 *
 * Two deliberate deviations from the written contract (02 §3.1): the context
 * carries the Shift flag (`extend`), because the *command* layer turns a move
 * into an extension (02 §4.1); and the command has an `ignored` member next to
 * `consume` and `pass` (02 §3.2), because "the grid claims the key name but
 * does nothing here" and "the key is not the grid's" are different duties.
 */
import {
  cellCoordEquals,
  clampCellCoord,
  coordAt,
  moveCellFocus,
  selectionFromAnchorFocus,
  type CellCoord,
  type CellRange,
  type GridSelection,
} from '../stores/tableData/gridSelection';

/** Structural projection of a key event: only the fields the decision needs. */
export interface GridKeyEventLike {
  /** Normalised to lower case, like `useKeyboardShortcuts` does. */
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  /** True while an IME composition is active. */
  isComposing: boolean;
}

/** Key → intent. The order of the checks in `readGridKeyIntent` is the priority. */
export type GridKeyIntent =
  | { kind: 'move'; deltaRow: number; deltaColumn: number }
  | { kind: 'move-page'; direction: 'up' | 'down' }
  | { kind: 'move-edge'; edge: 'row-start' | 'row-end' | 'grid-start' | 'grid-end' }
  | { kind: 'select-all-rows' }
  | { kind: 'begin-edit' }
  | { kind: 'leave-grid' }
  | { kind: 'ignored' }
  | { kind: 'pass' };

export interface GridKeyboardContext {
  /** Displayed column names in render order; the arrows move between them only. */
  columnOrder: readonly string[];
  rowCount: number;
  mode: GridSelection['mode'];
  /** The current cursor; `null` in `none` mode when there is nothing to resume. */
  cursor: CellCoord | null;
  /** Visible rows of the viewport (≥ 1); the step size of PageUp/PageDown. */
  viewportRowCount: number;
  /** An editor is open (`editingCell !== null`). */
  editing: boolean;
  /** Shift was held: the command layer extends the region instead of moving it. */
  extend?: boolean;
}

/** What the key means for the grid. The React layer only dispatches it. */
export type GridKeyCommand =
  | { kind: 'set-selection'; selection: GridSelection }
  | { kind: 'row-select'; rowIndex: number; opts: { multi: boolean; range: boolean } }
  | { kind: 'select-all-rows' }
  | { kind: 'begin-edit'; coord: CellCoord }
  /** Claimed and prevented, but no state changes (border no-op, `Tab`). */
  | { kind: 'consume' }
  /** Claimed, but nothing to do here; the browser default stays untouched. */
  | { kind: 'ignored' }
  /** Not the grid's key: never prevent, never stop propagation. */
  | { kind: 'pass' };

/**
 * Row mode without any geometry (00-contracts §4, invariant I2). The same value as 01's
 * `ROW_MODE_GRID_SELECTION`, declared here because this module must not reach into the
 * store slice. Frozen, so that a stray `.push` fails loud instead of spreading.
 */
export const EMPTY_ROW_MODE_SELECTION: GridSelection = Object.freeze({
  anchor: null,
  focus: null,
  extraRanges: Object.freeze([]) as unknown as CellRange[],
  mode: 'row',
});

/** Flatten a React/native key event into the projection the decisions work on. */
export function toGridKeyEventLike(e: {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  nativeEvent: { isComposing?: boolean; keyCode?: number };
}): GridKeyEventLike {
  return {
    key: e.key.toLowerCase(),
    metaKey: e.metaKey,
    ctrlKey: e.ctrlKey,
    shiftKey: e.shiftKey,
    altKey: e.altKey,
    // `keyCode === 229` is the legacy IME signal some engines still send (01 §3.10).
    isComposing: e.nativeEvent.isComposing === true || e.nativeEvent.keyCode === 229,
  };
}

/** A composition owns every key; Chromium reports the combination as `process`. */
export function isImeComposingKey(e: GridKeyEventLike): boolean {
  return e.isComposing || e.key === 'process';
}

/** `Cmd` on macOS, `Ctrl` elsewhere. The pure layer cannot ask the platform. */
function isModKey(e: GridKeyEventLike): boolean {
  return e.metaKey || e.ctrlKey;
}

/** The four arrow keys and the cell they move to. */
const ARROW_MOVES: Record<string, { deltaRow: number; deltaColumn: number }> = {
  arrowup: { deltaRow: -1, deltaColumn: 0 },
  arrowdown: { deltaRow: 1, deltaColumn: 0 },
  arrowleft: { deltaRow: 0, deltaColumn: -1 },
  arrowright: { deltaRow: 0, deltaColumn: 1 },
};

/**
 * Key → intent. Pure mapping, no state access beyond the context.
 *
 * The order of the checks **is** the priority table (02 §4.0): ① IME composition
 * ② open editor ③ `Escape` (01's) ④ clipboard/undo (03/06) ⑤ `Alt` ⑥ the key
 * table ⑦ everything else.
 */
export function readGridKeyIntent(e: GridKeyEventLike, ctx: GridKeyboardContext): GridKeyIntent {
  if (isImeComposingKey(e)) return { kind: 'pass' };
  if (ctx.editing) return { kind: 'pass' };
  if (e.key === 'escape') return { kind: 'pass' };
  if (isModKey(e) && (e.key === 'c' || e.key === 'x' || e.key === 'v' || e.key === 'z')) {
    return { kind: 'pass' };
  }
  if (e.altKey) return { kind: 'pass' };

  const mod = isModKey(e);
  // `Cmd/Ctrl+Left/Right` is the row edge: a laptop keyboard has no `Home`.
  if (mod && (e.key === 'arrowleft' || e.key === 'arrowright')) {
    return { kind: 'move-edge', edge: e.key === 'arrowleft' ? 'row-start' : 'row-end' };
  }
  const arrow = ARROW_MOVES[e.key];
  if (arrow !== undefined) return { kind: 'move', ...arrow };
  switch (e.key) {
    case 'home':
      return { kind: 'move-edge', edge: mod ? 'grid-start' : 'row-start' };
    case 'end':
      return { kind: 'move-edge', edge: mod ? 'grid-end' : 'row-end' };
    case 'pageup':
      return { kind: 'move-page', direction: 'up' };
    case 'pagedown':
      return { kind: 'move-page', direction: 'down' };
    case 'tab':
      // `Tab` leaves the grid; the row-number slot must not become the target.
      return { kind: 'leave-grid' };
    case 'enter':
    case 'f2':
      // `Cmd+Enter` executes the query (a global shortcut) — not the grid's.
      if (mod) return { kind: 'pass' };
      // Without a selection there is no cell to edit: the grid claims the key
      // name but does nothing (02 §4.1).
      return ctx.mode === 'none' ? { kind: 'ignored' } : { kind: 'begin-edit' };
    case 'a':
      return mod ? { kind: 'select-all-rows' } : { kind: 'pass' };
    default:
      // Printable characters, `Space`, `Delete`/`Backspace`: not the grid's.
      return { kind: 'pass' };
  }
}

/**
 * The cursor of the current selection: the focus corner in cell mode, the row anchor of the
 * row selection in row mode, nothing in `none` mode.
 */
export function resolveKeyboardCursor(
  selection: GridSelection,
  lastSelectedIndex: number | null,
  columnOrder: readonly string[],
): CellCoord | null {
  if (selection.mode === 'cell') return selection.focus;
  if (selection.mode === 'row') {
    if (lastSelectedIndex === null) return null;
    const first = columnOrder[0];
    return first === undefined ? null : { rowIndex: lastSelectedIndex, columnName: first };
  }
  return null;
}

/**
 * Where the grid is entered from `none` mode: the remembered cursor when it is still
 * displayable, the first cell otherwise.
 */
export function resolveEntryCursor(
  remembered: CellCoord | null,
  rowCount: number,
  columnOrder: readonly string[],
): CellCoord | null {
  if (rowCount <= 0 || columnOrder.length === 0) return null;
  if (remembered !== null) {
    const clamped = clampCellCoord(remembered, rowCount, columnOrder);
    if (clamped !== null) return clamped;
  }
  return coordAt(0, 0, columnOrder);
}

/** The step of PageUp/PageDown: one screen minus one row, so the origin stays visible. */
export function resolvePageDelta(viewportRowCount: number): number {
  return Math.max(1, viewportRowCount - 1);
}

/** Coordinate comparison with 01's semantics (no private copies of it). */
export function gridCursorEquals(a: CellCoord | null, b: CellCoord | null): boolean {
  return cellCoordEquals(a, b);
}

/**
 * Converge a cursor after the rows or the displayed columns changed: a hidden column or an
 * empty grid has no legal coordinate at all, hence `null`.
 */
export function clampGridCursor(
  cursor: CellCoord | null,
  rowCount: number,
  columnOrder: readonly string[],
): CellCoord | null {
  if (cursor === null) return null;
  return clampCellCoord(cursor, rowCount, columnOrder);
}

/** The cursor the command layer works with, in whichever mode the grid is in. */
function currentCursor(ctx: GridKeyboardContext, selection: GridSelection): CellCoord | null {
  return ctx.cursor ?? (ctx.mode === 'cell' ? selection.focus : null);
}

/**
 * Entering the grid from `none` mode: the remembered cursor, or the first cell. The region
 * is a single cell — an arrow key never invents a selection of its own in `none` mode.
 */
function enterCellMode(ctx: GridKeyboardContext): GridKeyCommand {
  const entry = resolveEntryCursor(ctx.cursor, ctx.rowCount, ctx.columnOrder);
  if (entry === null) return { kind: 'consume' };
  return { kind: 'set-selection', selection: selectionFromAnchorFocus(entry, entry, 'cell') };
}

/** The selection a cell mode move produces (01's `selectionFromAnchorFocus`, always). */
function buildCellSelection(
  next: CellCoord,
  ctx: GridKeyboardContext,
  selection: GridSelection,
): GridKeyCommand {
  if (ctx.extend === true) {
    // The anchor stays where it is and the extra ranges survive (02 §4.7). A missing
    // anchor is an incomplete state: the new cursor becomes a single-cell region
    // rather than a lost event (02 §3.1).
    const anchor = selection.anchor ?? next;
    return {
      kind: 'set-selection',
      selection: selectionFromAnchorFocus(anchor, next, 'cell', selection.extraRanges),
    };
  }
  // An unmodified move collapses the main region and drops the extra ranges — the same
  // thing a plain click does (02 §4.7).
  return { kind: 'set-selection', selection: selectionFromAnchorFocus(next, next, 'cell') };
}

/** `Enter`/`F2`: the cell to edit, or nothing when there is none. */
function resolveBeginEdit(ctx: GridKeyboardContext, selection: GridSelection): GridKeyCommand {
  if (ctx.mode === 'none') return { kind: 'ignored' };
  const coord = currentCursor(ctx, selection);
  if (coord === null) return { kind: 'ignored' };
  return { kind: 'begin-edit', coord };
}

/** The coordinate of an edge, in the render order of the columns (01's `coordAt`). */
function edgeCoord(
  edge: 'row-start' | 'row-end' | 'grid-start' | 'grid-end',
  cursor: CellCoord,
  ctx: GridKeyboardContext,
): CellCoord | null {
  switch (edge) {
    case 'row-start':
      return coordAt(cursor.rowIndex, 0, ctx.columnOrder);
    case 'row-end':
      return coordAt(cursor.rowIndex, ctx.columnOrder.length - 1, ctx.columnOrder);
    case 'grid-start':
      return coordAt(0, 0, ctx.columnOrder);
    case 'grid-end':
      return coordAt(ctx.rowCount - 1, ctx.columnOrder.length - 1, ctx.columnOrder);
  }
}

/** A movement of the cursor: an arrow key, a page or an edge. */
type GridMovement = Extract<GridKeyIntent, { kind: 'move' | 'move-page' | 'move-edge' }>;

/** The cell a movement lands on in cell mode (01's `moveCellFocus` and `coordAt`). */
function cellMovementTarget(
  movement: GridMovement,
  ctx: GridKeyboardContext,
  cursor: CellCoord,
): CellCoord | null {
  switch (movement.kind) {
    case 'move':
      return moveCellFocus(
        cursor,
        movement.deltaRow,
        movement.deltaColumn,
        ctx.rowCount,
        ctx.columnOrder,
      );
    case 'move-page': {
      const delta = resolvePageDelta(ctx.viewportRowCount) * (movement.direction === 'down' ? 1 : -1);
      return moveCellFocus(cursor, delta, 0, ctx.rowCount, ctx.columnOrder);
    }
    case 'move-edge':
      return edgeCoord(movement.edge, cursor, ctx);
  }
}

/**
 * The row a movement lands on in row mode. Row mode carries no cell geometry
 * (00-contracts §4, I2), so an edge has no target there.
 *
 * The border of an arrow key is a no-op: `selectRow` on the only selected row
 * *clears* the selection, so one more `↑` at the top row would wipe it
 * (02 §7 E6). A page key clamps into the page instead (02 §7 E11).
 */
function rowMovementTarget(
  movement: GridMovement,
  ctx: GridKeyboardContext,
  cursor: CellCoord,
): number | null {
  if (movement.kind === 'move-edge') return null;
  if (movement.kind === 'move' && movement.deltaColumn !== 0) return null; // a horizontal arrow has no row
  const delta =
    movement.kind === 'move-page'
      ? resolvePageDelta(ctx.viewportRowCount) * (movement.direction === 'down' ? 1 : -1)
      : movement.deltaRow;
  // The row movement goes through 01's `moveCellFocus` like every other
  // movement: it clamps the row into the page and answers `null` when there is
  // no room left. A hand-rolled border check here would silently diverge from
  // the cell mode the day 01 changes its border semantics.
  const next = moveCellFocus(cursor, delta, 0, ctx.rowCount, ctx.columnOrder);
  return next === null ? null : next.rowIndex;
}

/** A movement: a cell in cell mode, a row in row mode, an entry in `none`. */
function resolveMovement(
  movement: GridMovement,
  ctx: GridKeyboardContext,
  selection: GridSelection,
): GridKeyCommand {
  if (ctx.rowCount <= 0 || ctx.columnOrder.length === 0) return { kind: 'consume' };
  if (ctx.mode === 'none') return enterCellMode(ctx);
  const cursor = currentCursor(ctx, selection);
  if (cursor === null) return { kind: 'consume' };
  if (ctx.mode === 'row') {
    const row = rowMovementTarget(movement, ctx, cursor);
    if (row === null) return { kind: 'consume' };
    return { kind: 'row-select', rowIndex: row, opts: { multi: false, range: ctx.extend === true } };
  }
  const target = cellMovementTarget(movement, ctx, cursor);
  // No room left: the selection stays exactly as it is (02 §7 E4/E5) — it must not
  // collapse into the border cell.
  if (target === null || gridCursorEquals(target, cursor)) return { kind: 'consume' };
  return buildCellSelection(target, ctx, selection);
}

/**
 * Intent → command. This is the only semantic centre of the volume: every keyboard
 * behaviour of the grid can be decided without a browser.
 */
export function resolveGridKeyCommand(
  intent: GridKeyIntent,
  ctx: GridKeyboardContext,
  selection: GridSelection,
): GridKeyCommand {
  switch (intent.kind) {
    case 'pass':
      return { kind: 'pass' };
    case 'ignored':
      return { kind: 'ignored' };
    case 'leave-grid':
      // `Tab` leaves the grid: the key is claimed (the browser must not move the focus
      // into the row-number slot) but no state changes.
      return { kind: 'consume' };
    case 'select-all-rows':
      return { kind: 'select-all-rows' };
    case 'begin-edit':
      return resolveBeginEdit(ctx, selection);
    case 'move':
    case 'move-page':
    case 'move-edge':
      return resolveMovement(intent, ctx, selection);
  }
}
