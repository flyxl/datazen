/**
 * DB-02: the pure keyboard logic of the grid.
 *
 * Every case here is a decision the browser is not needed for: the key → intent
 * mapping, the intent → command mapping, and the cursor helpers. The React
 * layer only dispatches the command (02 §3.3).
 */
import { describe, expect, it } from 'vitest';
import {
  EMPTY_ROW_MODE_SELECTION,
  clampGridCursor,
  gridCursorEquals,
  isImeComposingKey,
  readGridKeyIntent,
  resolveEntryCursor,
  resolveGridKeyCommand,
  resolveKeyboardCursor,
  resolvePageDelta,
  toGridKeyEventLike,
  type GridKeyEventLike,
  type GridKeyboardContext,
} from '../gridKeyboard';
import { EMPTY_GRID_SELECTION, type GridSelection } from '../../stores/tableData/gridSelection';

/**
 * The columns in **render order**. `name` comes before `id` on purpose: a helper
 * that sorted the names lexicographically would answer `id` first and every
 * edge case would look right (01 §4.1's sorting trap).
 */
const COLUMNS = ['name', 'id'] as const;

function keyEvent(overrides: Partial<GridKeyEventLike> = {}): GridKeyEventLike {
  return {
    key: '',
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    isComposing: false,
    ...overrides,
  };
}

function context(overrides: Partial<GridKeyboardContext> = {}): GridKeyboardContext {
  return {
    columnOrder: COLUMNS,
    rowCount: 10,
    mode: 'cell',
    cursor: { rowIndex: 2, columnName: 'id' },
    viewportRowCount: 20,
    editing: false,
    ...overrides,
  };
}

/** A cell selection with one region and (optionally) extra ranges. */
function cellSelection(
  anchor: { rowIndex: number; columnName: string } | null = { rowIndex: 2, columnName: 'id' },
  focus: { rowIndex: number; columnName: string } | null = { rowIndex: 2, columnName: 'id' },
  extraRanges: GridSelection['extraRanges'] = [],
): GridSelection {
  return { anchor, focus, extraRanges, mode: 'cell' };
}

describe('gridKeyboard · the key event projection', () => {
  it('normalises the key to lower case and keeps the modifiers', () => {
    const projection = toGridKeyEventLike({
      key: 'ArrowDown',
      metaKey: true,
      ctrlKey: false,
      shiftKey: true,
      altKey: false,
      nativeEvent: {},
    });
    expect(projection).toEqual({
      key: 'arrowdown',
      metaKey: true,
      ctrlKey: false,
      shiftKey: true,
      altKey: false,
      isComposing: false,
    });
  });

  it('reads the composition from the native event and from the legacy key code', () => {
    const composing = toGridKeyEventLike({
      key: 'Enter',
      metaKey: false,
      ctrlKey: false,
      shiftKey: false,
      altKey: false,
      nativeEvent: { isComposing: true },
    });
    expect(composing.isComposing).toBe(true);
    const legacy = toGridKeyEventLike({
      key: 'Enter',
      metaKey: false,
      ctrlKey: false,
      shiftKey: false,
      altKey: false,
      nativeEvent: { keyCode: 229 },
    });
    expect(legacy.isComposing).toBe(true);
  });

  it('treats the Chromium combination key as a composition', () => {
    expect(isImeComposingKey(keyEvent({ key: 'process' }))).toBe(true);
    expect(isImeComposingKey(keyEvent({ key: 'enter', isComposing: true }))).toBe(true);
    expect(isImeComposingKey(keyEvent({ key: 'enter' }))).toBe(false);
  });
});

describe('gridKeyboard · readGridKeyIntent', () => {
  it('maps the four arrows to a move', () => {
    expect(readGridKeyIntent(keyEvent({ key: 'arrowup' }), context())).toEqual({
      kind: 'move',
      deltaRow: -1,
      deltaColumn: 0,
    });
    expect(readGridKeyIntent(keyEvent({ key: 'arrowdown' }), context())).toEqual({
      kind: 'move',
      deltaRow: 1,
      deltaColumn: 0,
    });
    expect(readGridKeyIntent(keyEvent({ key: 'arrowleft' }), context())).toEqual({
      kind: 'move',
      deltaRow: 0,
      deltaColumn: -1,
    });
    expect(readGridKeyIntent(keyEvent({ key: 'arrowright' }), context())).toEqual({
      kind: 'move',
      deltaRow: 0,
      deltaColumn: 1,
    });
  });

  it('matches the modifiers exactly', () => {
    // Shift does not change the intent: the command layer decides to extend.
    expect(readGridKeyIntent(keyEvent({ key: 'arrowdown', shiftKey: true }), context())).toEqual({
      kind: 'move',
      deltaRow: 1,
      deltaColumn: 0,
    });
    expect(readGridKeyIntent(keyEvent({ key: 'arrowdown', altKey: true }), context())).toEqual({
      kind: 'pass',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'c', metaKey: true }), context())).toEqual({
      kind: 'pass',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'z', metaKey: true }), context())).toEqual({
      kind: 'pass',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'a', metaKey: true }), context())).toEqual({
      kind: 'select-all-rows',
    });
    // `Ctrl` is the `Cmd` of the other platform.
    expect(readGridKeyIntent(keyEvent({ key: 'a', ctrlKey: true }), context())).toEqual({
      kind: 'select-all-rows',
    });
    // A plain letter is not the grid's key.
    expect(readGridKeyIntent(keyEvent({ key: 'a' }), context())).toEqual({ kind: 'pass' });
  });

  it('leaves Escape, Delete and the clipboard keys to their owners', () => {
    for (const key of ['escape', 'delete', 'backspace', ' ']) {
      expect(readGridKeyIntent(keyEvent({ key }), context())).toEqual({ kind: 'pass' });
    }
    for (const modifier of [{ metaKey: true }, { ctrlKey: true }]) {
      for (const key of ['c', 'x', 'v', 'z']) {
        expect(readGridKeyIntent(keyEvent({ key, ...modifier }), context())).toEqual({
          kind: 'pass',
        });
      }
      expect(
        readGridKeyIntent(keyEvent({ key: 'z', shiftKey: true, ...modifier }), context()),
      ).toEqual({ kind: 'pass' });
    }
  });

  it('passes every key while an editor is open', () => {
    const editing = context({ editing: true });
    for (const key of [
      'arrowup',
      'arrowdown',
      'arrowleft',
      'arrowright',
      'enter',
      'f2',
      'tab',
      'home',
      'end',
      'pageup',
      'pagedown',
    ]) {
      expect(readGridKeyIntent(keyEvent({ key }), editing)).toEqual({ kind: 'pass' });
    }
  });

  it('keeps the composition above the editing state', () => {
    const composing = keyEvent({ key: 'enter', isComposing: true });
    expect(readGridKeyIntent(composing, context({ editing: false }))).toEqual({ kind: 'pass' });
    expect(readGridKeyIntent(composing, context({ editing: true }))).toEqual({ kind: 'pass' });
    expect(readGridKeyIntent(keyEvent({ key: 'process' }), context())).toEqual({ kind: 'pass' });
  });

  it('claims Tab and Shift+Tab as the leave-grid intent', () => {
    expect(readGridKeyIntent(keyEvent({ key: 'tab' }), context())).toEqual({
      kind: 'leave-grid',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'tab', shiftKey: true }), context())).toEqual({
      kind: 'leave-grid',
    });
  });

  it('maps the edges, the pages and the editing keys', () => {
    expect(readGridKeyIntent(keyEvent({ key: 'home' }), context())).toEqual({
      kind: 'move-edge',
      edge: 'row-start',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'end' }), context())).toEqual({
      kind: 'move-edge',
      edge: 'row-end',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'home', metaKey: true }), context())).toEqual({
      kind: 'move-edge',
      edge: 'grid-start',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'end', metaKey: true }), context())).toEqual({
      kind: 'move-edge',
      edge: 'grid-end',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'arrowleft', metaKey: true }), context())).toEqual({
      kind: 'move-edge',
      edge: 'row-start',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'arrowright', metaKey: true }), context())).toEqual({
      kind: 'move-edge',
      edge: 'row-end',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'pageup' }), context())).toEqual({
      kind: 'move-page',
      direction: 'up',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'pagedown' }), context())).toEqual({
      kind: 'move-page',
      direction: 'down',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'enter' }), context())).toEqual({
      kind: 'begin-edit',
    });
    expect(readGridKeyIntent(keyEvent({ key: 'f2' }), context())).toEqual({
      kind: 'begin-edit',
    });
    // Without a selection there is no cell to edit.
    expect(readGridKeyIntent(keyEvent({ key: 'enter' }), context({ mode: 'none' }))).toEqual({
      kind: 'ignored',
    });
  });
});

describe('gridKeyboard · resolveGridKeyCommand', () => {
  it('enters the cell mode from none with the remembered cursor', () => {
    const cmd = resolveGridKeyCommand(
      { kind: 'move', deltaRow: 1, deltaColumn: 0 },
      context({ mode: 'none', cursor: { rowIndex: 7, columnName: 'name' } }),
      EMPTY_GRID_SELECTION,
    );
    expect(cmd).toEqual({
      kind: 'set-selection',
      selection: {
        anchor: { rowIndex: 7, columnName: 'name' },
        focus: { rowIndex: 7, columnName: 'name' },
        extraRanges: [],
        mode: 'cell',
      },
    });
  });

  it('falls back to the first cell when the remembered cursor is unusable', () => {
    const cmd = resolveGridKeyCommand(
      { kind: 'move', deltaRow: 1, deltaColumn: 0 },
      context({ mode: 'none', cursor: { rowIndex: 0, columnName: 'gone' } }),
      EMPTY_GRID_SELECTION,
    );
    expect(cmd).toEqual({
      kind: 'set-selection',
      selection: {
        anchor: { rowIndex: 0, columnName: 'name' },
        focus: { rowIndex: 0, columnName: 'name' },
        extraRanges: [],
        mode: 'cell',
      },
    });
  });

  it('does not collapse the selection at the border', () => {
    const selection = cellSelection(
      { rowIndex: 0, columnName: 'name' },
      { rowIndex: 0, columnName: 'id' },
    );
    const atBorder = context({ cursor: { rowIndex: 0, columnName: 'name' } });
    expect(
      resolveGridKeyCommand({ kind: 'move', deltaRow: -1, deltaColumn: 0 }, atBorder, selection),
    ).toEqual({ kind: 'consume' });
    expect(
      resolveGridKeyCommand({ kind: 'move', deltaRow: 0, deltaColumn: -1 }, atBorder, selection),
    ).toEqual({ kind: 'consume' });
  });

  it('collapses the main region and drops the extra ranges on an unmodified move', () => {
    const selection = cellSelection(
      { rowIndex: 2, columnName: 'name' },
      { rowIndex: 4, columnName: 'id' },
      [
        {
          start: { rowIndex: 6, columnName: 'name' },
          end: { rowIndex: 6, columnName: 'name' },
        },
      ],
    );
    const cmd = resolveGridKeyCommand(
      { kind: 'move', deltaRow: 1, deltaColumn: 0 },
      context({ cursor: { rowIndex: 4, columnName: 'id' } }),
      selection,
    );
    expect(cmd).toEqual({
      kind: 'set-selection',
      selection: {
        anchor: { rowIndex: 5, columnName: 'id' },
        focus: { rowIndex: 5, columnName: 'id' },
        extraRanges: [],
        mode: 'cell',
      },
    });
  });

  it('keeps the anchor and the extra ranges on an extending move', () => {
    const extra = [
      {
        start: { rowIndex: 6, columnName: 'name' },
        end: { rowIndex: 6, columnName: 'name' },
      },
    ];
    const selection = cellSelection(
      { rowIndex: 2, columnName: 'name' },
      { rowIndex: 4, columnName: 'id' },
      extra,
    );
    const cmd = resolveGridKeyCommand(
      { kind: 'move', deltaRow: 1, deltaColumn: 0 },
      context({ cursor: { rowIndex: 4, columnName: 'id' }, extend: true }),
      selection,
    );
    expect(cmd).toEqual({
      kind: 'set-selection',
      selection: {
        anchor: { rowIndex: 2, columnName: 'name' },
        focus: { rowIndex: 5, columnName: 'id' },
        extraRanges: extra,
        mode: 'cell',
      },
    });
  });

  it('builds the anchor when the extending move meets an incomplete state', () => {
    const selection = cellSelection(null, { rowIndex: 2, columnName: 'id' });
    const cmd = resolveGridKeyCommand(
      { kind: 'move', deltaRow: 1, deltaColumn: 0 },
      context({ cursor: { rowIndex: 2, columnName: 'id' }, extend: true }),
      selection,
    );
    expect(cmd).toEqual({
      kind: 'set-selection',
      selection: {
        anchor: { rowIndex: 3, columnName: 'id' },
        focus: { rowIndex: 3, columnName: 'id' },
        extraRanges: [],
        mode: 'cell',
      },
    });
  });

  it('moves the row selection in row mode, and the range follows the shift key', () => {
    const rowContext = context({ mode: 'row', cursor: { rowIndex: 3, columnName: 'name' } });
    expect(
      resolveGridKeyCommand({ kind: 'move', deltaRow: -1, deltaColumn: 0 }, rowContext, EMPTY_GRID_SELECTION),
    ).toEqual({ kind: 'row-select', rowIndex: 2, opts: { multi: false, range: false } });
    expect(
      resolveGridKeyCommand(
        { kind: 'move', deltaRow: -1, deltaColumn: 0 },
        context({ mode: 'row', cursor: { rowIndex: 3, columnName: 'name' }, extend: true }),
        EMPTY_GRID_SELECTION,
      ),
    ).toEqual({ kind: 'row-select', rowIndex: 2, opts: { multi: false, range: true } });
  });

  it('sends no command when a row movement stays on the same row', () => {
    const rowContext = context({ mode: 'row', cursor: { rowIndex: 0, columnName: 'name' } });
    expect(
      resolveGridKeyCommand({ kind: 'move', deltaRow: -1, deltaColumn: 0 }, rowContext, EMPTY_GRID_SELECTION),
    ).toEqual({ kind: 'consume' });
  });

  it('steps a page by one screen minus one row', () => {
    expect(resolvePageDelta(1)).toBe(1);
    expect(resolvePageDelta(20)).toBe(19);
    const cmd = resolveGridKeyCommand(
      { kind: 'move-page', direction: 'down' },
      context({ rowCount: 100, cursor: { rowIndex: 5, columnName: 'id' }, viewportRowCount: 20 }),
      cellSelection(),
    );
    expect(cmd).toEqual({
      kind: 'set-selection',
      selection: {
        anchor: { rowIndex: 24, columnName: 'id' },
        focus: { rowIndex: 24, columnName: 'id' },
        extraRanges: [],
        mode: 'cell',
      },
    });
  });

  it('clamps the row mode page move into the page', () => {
    const cmd = resolveGridKeyCommand(
      { kind: 'move-page', direction: 'down' },
      context({
        mode: 'row',
        rowCount: 10,
        cursor: { rowIndex: 8, columnName: 'name' },
        viewportRowCount: 20,
      }),
      EMPTY_GRID_SELECTION,
    );
    expect(cmd).toEqual({ kind: 'row-select', rowIndex: 9, opts: { multi: false, range: false } });
  });

  it('selects every row in every mode', () => {
    for (const mode of ['none', 'cell', 'row'] as const) {
      expect(
        resolveGridKeyCommand(
          { kind: 'select-all-rows' },
          context({ mode }),
          EMPTY_GRID_SELECTION,
        ),
      ).toEqual({ kind: 'select-all-rows' });
    }
  });

  it('edits the focused cell, the selected row or nothing at all', () => {
    expect(
      resolveGridKeyCommand(
        { kind: 'begin-edit' },
        context({ cursor: { rowIndex: 2, columnName: 'id' } }),
        cellSelection({ rowIndex: 2, columnName: 'id' }, { rowIndex: 2, columnName: 'id' }),
      ),
    ).toEqual({ kind: 'begin-edit', coord: { rowIndex: 2, columnName: 'id' } });
    // Row mode: the row anchor and the first column of the render order.
    expect(
      resolveGridKeyCommand(
        { kind: 'begin-edit' },
        context({ mode: 'row', cursor: { rowIndex: 4, columnName: 'name' } }),
        EMPTY_GRID_SELECTION,
      ),
    ).toEqual({ kind: 'begin-edit', coord: { rowIndex: 4, columnName: 'name' } });
    // No selection at all: the key is claimed, but nothing happens.
    expect(
      resolveGridKeyCommand(
        { kind: 'begin-edit' },
        context({ mode: 'none' }),
        EMPTY_GRID_SELECTION,
      ),
    ).toEqual({ kind: 'ignored' });
    expect(
      resolveGridKeyCommand(
        { kind: 'begin-edit' },
        context({ mode: 'row', cursor: null }),
        EMPTY_GRID_SELECTION,
      ),
    ).toEqual({ kind: 'ignored' });
  });

  it('moves to the four edges in the render order of the columns', () => {
    const at = (rowIndex: number, columnName: string) =>
      context({ cursor: { rowIndex, columnName } });
    expect(
      resolveGridKeyCommand({ kind: 'move-edge', edge: 'row-start' }, at(2, 'id'), cellSelection()),
    ).toEqual({
      kind: 'set-selection',
      selection: {
        anchor: { rowIndex: 2, columnName: 'name' },
        focus: { rowIndex: 2, columnName: 'name' },
        extraRanges: [],
        mode: 'cell',
      },
    });
    expect(
      resolveGridKeyCommand({ kind: 'move-edge', edge: 'row-end' }, at(2, 'name'), cellSelection()),
    ).toEqual({
      kind: 'set-selection',
      selection: {
        anchor: { rowIndex: 2, columnName: 'id' },
        focus: { rowIndex: 2, columnName: 'id' },
        extraRanges: [],
        mode: 'cell',
      },
    });
    expect(
      resolveGridKeyCommand({ kind: 'move-edge', edge: 'grid-start' }, at(2, 'id'), cellSelection()),
    ).toEqual({
      kind: 'set-selection',
      selection: {
        anchor: { rowIndex: 0, columnName: 'name' },
        focus: { rowIndex: 0, columnName: 'name' },
        extraRanges: [],
        mode: 'cell',
      },
    });
    expect(
      resolveGridKeyCommand({ kind: 'move-edge', edge: 'grid-end' }, at(2, 'name'), cellSelection()),
    ).toEqual({
      kind: 'set-selection',
      selection: {
        anchor: { rowIndex: 9, columnName: 'id' },
        focus: { rowIndex: 9, columnName: 'id' },
        extraRanges: [],
        mode: 'cell',
      },
    });
  });

  it('claims Tab without touching the state', () => {
    expect(
      resolveGridKeyCommand(
        { kind: 'leave-grid' },
        context(),
        cellSelection(),
      ),
    ).toEqual({ kind: 'consume' });
  });
});

describe('gridKeyboard · the cursor helpers', () => {
  it('reads the cursor of the current selection', () => {
    expect(
      resolveKeyboardCursor(
        cellSelection({ rowIndex: 1, columnName: 'name' }, { rowIndex: 1, columnName: 'name' }),
        5,
        COLUMNS,
      ),
    ).toEqual({ rowIndex: 1, columnName: 'name' });
    // Row mode: the row anchor and the first column of the render order.
    expect(resolveKeyboardCursor(EMPTY_GRID_SELECTION, 5, COLUMNS)).toBeNull();
    expect(
      resolveKeyboardCursor({ anchor: null, focus: null, extraRanges: [], mode: 'row' }, 5, COLUMNS),
    ).toEqual({ rowIndex: 5, columnName: 'name' });
    expect(
      resolveKeyboardCursor({ anchor: null, focus: null, extraRanges: [], mode: 'row' }, null, COLUMNS),
    ).toBeNull();
  });

  it('lands the entry cursor on the first cell when nothing is remembered', () => {
    expect(resolveEntryCursor(null, 10, COLUMNS)).toEqual({ rowIndex: 0, columnName: 'name' });
    expect(resolveEntryCursor({ rowIndex: 3, columnName: 'id' }, 10, COLUMNS)).toEqual({
      rowIndex: 3,
      columnName: 'id',
    });
    // A hidden column has no place to land: the first cell it is.
    expect(resolveEntryCursor({ rowIndex: 3, columnName: 'gone' }, 10, COLUMNS)).toEqual({
      rowIndex: 0,
      columnName: 'name',
    });
    // An empty grid has no cell at all.
    expect(resolveEntryCursor(null, 0, COLUMNS)).toBeNull();
  });

  it('converges a cursor whose column was hidden', () => {
    expect(clampGridCursor(null, 10, COLUMNS)).toBeNull();
    expect(clampGridCursor({ rowIndex: 1, columnName: 'gone' }, 10, COLUMNS)).toBeNull();
    // A row beyond the page is clamped into it (01's `clampCellCoord`).
    expect(clampGridCursor({ rowIndex: 99, columnName: 'id' }, 10, COLUMNS)).toEqual({
      rowIndex: 9,
      columnName: 'id',
    });
  });

  it('compares two cursors with the semantics of 01', () => {
    expect(gridCursorEquals(null, null)).toBe(true);
    expect(gridCursorEquals({ rowIndex: 1, columnName: 'id' }, null)).toBe(false);
    expect(gridCursorEquals({ rowIndex: 1, columnName: 'id' }, { rowIndex: 1, columnName: 'id' })).toBe(
      true,
    );
    expect(gridCursorEquals({ rowIndex: 1, columnName: 'id' }, { rowIndex: 1, columnName: 'name' })).toBe(
      false,
    );
  });

  it('keeps the row mode free of geometry', () => {
    expect(EMPTY_ROW_MODE_SELECTION.anchor).toBeNull();
    expect(EMPTY_ROW_MODE_SELECTION.focus).toBeNull();
    expect(EMPTY_ROW_MODE_SELECTION.extraRanges).toHaveLength(0);
    expect(EMPTY_ROW_MODE_SELECTION.mode).toBe('row');
  });
});
