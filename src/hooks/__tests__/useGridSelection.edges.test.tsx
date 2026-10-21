/**
 * Boundary/edge coverage for `useGridSelection` (DB-01 Step 6).
 *
 * The main suite covers the happy paths. This file targets the branches the pointer
 * state machine can reach only from the "incomplete" states the PRD calls out:
 * pointer capture, `pointercancel`, a cancelled drag that must stay cancelled, IME
 * composition, right-click, and row-gutter shapes that carry no `data-dt-row`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useCallback, useState } from 'react';
import {
  EMPTY_GRID_SELECTION,
  type CellCoord,
  type GridSelection,
} from '../../stores/tableData/gridSelection';
import { useGridSelection } from '../useGridSelection';

const COLUMNS = ['name', 'id', 'note'];
const ROW_COUNT = 3;

interface HarnessProps {
  onChange?: (next: GridSelection) => void;
  onRowGutterSelect?: (rowIndex: number, opts: { multi: boolean; range: boolean }) => void;
  /** Lets a test simulate a page with no rows while stale cells are still in the DOM. */
  rowCount?: number;
}

function Harness({ onChange, onRowGutterSelect, rowCount = ROW_COUNT }: HarnessProps) {
  const [selection, setSelection] = useState<GridSelection>(EMPTY_GRID_SELECTION);
  const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(null);
  const handleChange = useCallback(
    (next: GridSelection) => {
      setSelection(next);
      onChange?.(next);
    },
    [onChange],
  );
  const grid = useGridSelection({
    columnOrder: COLUMNS,
    rowCount,
    selection,
    onChange: handleChange,
    scrollElement,
    onRowGutterSelect,
    editingCell: null,
  });

  return (
    <div>
      <div data-testid="surface" ref={setScrollElement} {...grid.surfaceProps}>
        <div data-testid="row-0">
          <div role="rowheader" data-dt-row={0}>
            <button type="button" data-testid="gutter-0">
              1
            </button>
          </div>
          <div data-testid="cell-0-name" data-dt-row={0} data-dt-col="name">
            Alice
          </div>
          <div data-testid="cell-0-id" data-dt-row={0} data-dt-col="id">
            1
          </div>
        </div>
        <div data-testid="row-1">
          <div data-testid="cell-1-name" data-dt-row={1} data-dt-col="name">
            Bob
          </div>
          <div data-testid="cell-1-id" data-dt-row={1} data-dt-col="id">
            2
          </div>
        </div>
        {/* Gutter without its own data-dt-row: the row is resolved from the first cell. */}
        <div data-testid="row-2">
          <div role="rowheader" data-testid="header-2">
            <button type="button" data-testid="gutter-2">
              3
            </button>
          </div>
          <div data-testid="cell-2-id" data-dt-row={2} data-dt-col="id">
            3
          </div>
          <div data-testid="cell-2-note" data-dt-row={2} data-dt-col="note">
            z
          </div>
        </div>
        <div data-testid="inline-editor" contentEditable="true" suppressContentEditableWarning>
          inline editor
        </div>
        <div data-testid="surface-filler" />
      </div>
      <span data-testid="mode">{selection.mode}</span>
      <span data-testid="dragging">{String(grid.dragging)}</span>
    </div>
  );
}

const coord = (rowIndex: number, columnName: string): CellCoord => ({ rowIndex, columnName });

const surface = () => screen.getByTestId('surface');
const byId = (testId: string) => screen.getByTestId(testId);

function clickCell(testId: string, init: Record<string, unknown> = {}) {
  fireEvent.pointerDown(byId(testId), { pointerId: 1, button: 0, ...init });
  fireEvent.pointerUp(byId(testId), { pointerId: 1, button: 0, ...init });
}

beforeEach(() => {
  Object.defineProperty(document, 'elementFromPoint', {
    configurable: true,
    writable: true,
    value: vi.fn(() => null),
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  Reflect.deleteProperty(HTMLElement.prototype, 'setPointerCapture');
});

describe('useGridSelection pointer edge cases', () => {
  it('captures the pointer on pointerdown so a release outside the grid still commits', () => {
    const capture = vi.fn();
    Object.defineProperty(HTMLElement.prototype, 'setPointerCapture', {
      configurable: true,
      writable: true,
      value: capture,
    });
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 42, button: 0 });

    expect(capture).toHaveBeenCalledWith(42);
  });

  it('commits once on pointercancel and ignores the trailing pointerup', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 1, button: 0 });
    vi.mocked(document.elementFromPoint).mockReturnValue(byId('cell-1-id'));
    fireEvent.pointerMove(surface(), { pointerId: 1, clientX: 10, clientY: 10 });
    expect(byId('dragging').textContent).toBe('true');

    fireEvent.pointerCancel(surface(), { pointerId: 1 });
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith({
      anchor: coord(0, 'name'),
      focus: coord(1, 'id'),
      extraRanges: [],
      mode: 'cell',
    });
    expect(byId('dragging').textContent).toBe('false');

    fireEvent.pointerUp(surface(), { pointerId: 1, clientX: 10, clientY: 10 });
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it('Escape during a drag cancels the session and the later pointerup must not resurrect it', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 1, button: 0 });
    vi.mocked(document.elementFromPoint).mockReturnValue(byId('cell-1-id'));
    fireEvent.pointerMove(surface(), { pointerId: 1, clientX: 10, clientY: 10 });

    fireEvent.keyDown(surface(), { key: 'Escape' });
    const callsAfterEscape = onChange.mock.calls.length;
    // 01-selection §9.3 drag journey: the in-flight drag must be dropped, not merely
    // reported as `none`, otherwise the next pointerup re-creates the cancelled region.
    expect(byId('dragging').textContent).toBe('false');

    fireEvent.pointerUp(surface(), { pointerId: 1, clientX: 10, clientY: 10 });
    expect(onChange.mock.calls).toHaveLength(callsAfterEscape);
    expect(byId('mode').textContent).toBe('none');
  });

  it('a composing Escape must not clear the selection (IME guard)', () => {
    // Prove the DOM flag the guard is expected to read actually survives jsdom.
    expect(new KeyboardEvent('keydown', { key: 'Escape', isComposing: true }).isComposing).toBe(
      true,
    );
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    clickCell('cell-0-name');
    const callsAfterClick = onChange.mock.calls.length;

    fireEvent.keyDown(surface(), { key: 'Escape', isComposing: true });

    // 01-selection §4.5: every grid-level key is ignored while composing.
    expect(onChange.mock.calls).toHaveLength(callsAfterClick);
    expect(byId('mode').textContent).toBe('cell');
  });

  it('treats Shift+click with no existing cell selection as a plain click', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(byId('cell-1-id'), { pointerId: 1, button: 0, shiftKey: true });
    fireEvent.pointerUp(byId('cell-1-id'), { pointerId: 1, button: 0, shiftKey: true });

    expect(onChange).toHaveBeenLastCalledWith({
      anchor: coord(1, 'id'),
      focus: coord(1, 'id'),
      extraRanges: [],
      mode: 'cell',
    });
  });

  it('ignores non-primary buttons', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 1, button: 2 });
    fireEvent.pointerUp(byId('cell-0-name'), { pointerId: 1, button: 2 });

    expect(onChange).not.toHaveBeenCalled();
  });

  it('ignores pointerdown inside a contentEditable host', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(byId('inline-editor'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(byId('inline-editor'), { pointerId: 1, button: 0 });

    expect(onChange).not.toHaveBeenCalled();
  });

  it('resolves a rowheader without data-dt-row from the first cell of its row', () => {
    const onRowGutterSelect = vi.fn();
    const onChange = vi.fn();
    render(<Harness onChange={onChange} onRowGutterSelect={onRowGutterSelect} />);

    fireEvent.pointerDown(byId('gutter-2'), {
      pointerId: 1,
      button: 0,
      shiftKey: true,
      ctrlKey: true,
    });

    expect(onRowGutterSelect).toHaveBeenCalledWith(2, { multi: true, range: true });
    expect(onChange).not.toHaveBeenCalled();
  });

  it('does not start a cell selection on an empty page, even over stale DOM', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} rowCount={0} />);

    clickCell('cell-0-name');

    // 01-selection §7 #1: no selectable cell exists, so the click degrades to "blank".
    expect(byId('mode').textContent).toBe('none');
    expect(onChange).toHaveBeenLastCalledWith(EMPTY_GRID_SELECTION);
  });

  it('ignores a pointerup with no session and a move from another pointer id', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerUp(surface(), { pointerId: 5, clientX: 1, clientY: 1 });
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 1, button: 0 });
    vi.mocked(document.elementFromPoint).mockReturnValue(byId('cell-1-id'));
    fireEvent.pointerMove(surface(), { pointerId: 2, clientX: 10, clientY: 10 });
    expect(byId('dragging').textContent).toBe('false');

    fireEvent.pointerUp(surface(), { pointerId: 2, clientX: 10, clientY: 10 });
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.pointerUp(surface(), { pointerId: 1, clientX: 10, clientY: 10 });
    expect(onChange).toHaveBeenLastCalledWith({
      anchor: coord(0, 'name'),
      focus: coord(0, 'name'),
      extraRanges: [],
      mode: 'cell',
    });
  });
});
