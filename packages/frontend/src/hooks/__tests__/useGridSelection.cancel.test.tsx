/**
 * DB-01 §9.3 `escape mid-drag journey` — regression pins for the D1/D2 fix.
 *
 * What the main suite does not pin:
 * - after Escape the in-flight session is *gone*, so neither `pointermove` nor `pointerup`
 *   can emit again (the "pointerup resurrects the cancelled region" defect);
 * - a fresh `pointerdown` immediately after Escape starts a clean session (residual state);
 * - `pointercancel` / `blur` also end the session, not just Escape;
 * - pointer capture taken on pointerdown is handed back when the session ends;
 * - an IME-composing Escape is ignored while a plain Escape still works.
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
}

function Harness({ onChange }: HarnessProps) {
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
    rowCount: ROW_COUNT,
    selection,
    onChange: handleChange,
    scrollElement,
    editingCell: null,
  });

  return (
    <div>
      <div data-testid="surface" ref={setScrollElement} {...grid.surfaceProps}>
        <div data-testid="cell-0-name" data-dt-row={0} data-dt-col="name">
          Alice
        </div>
        <div data-testid="cell-0-id" data-dt-row={0} data-dt-col="id">
          1
        </div>
        <div data-testid="cell-1-id" data-dt-row={1} data-dt-col="id">
          2
        </div>
        <div data-testid="cell-1-note" data-dt-row={1} data-dt-col="note">
          x
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

let pointAt: Element | null = null;

beforeEach(() => {
  pointAt = null;
  Object.defineProperty(document, 'elementFromPoint', {
    configurable: true,
    writable: true,
    value: vi.fn(() => pointAt),
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  for (const name of ['setPointerCapture', 'releasePointerCapture', 'hasPointerCapture']) {
    Reflect.deleteProperty(HTMLElement.prototype, name);
  }
});

describe('drag cancellation', () => {
  it('Escape ends the drag: later pointermove and pointerup emit nothing', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 1, button: 0, clientX: 1, clientY: 1 });
    pointAt = byId('cell-1-id');
    fireEvent.pointerMove(surface(), { pointerId: 1, clientX: 2, clientY: 2 });
    expect(onChange).not.toHaveBeenCalled();
    expect(byId('dragging').textContent).toBe('true');

    fireEvent.keyDown(surface(), { key: 'Escape' });
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith(EMPTY_GRID_SELECTION);
    expect(byId('dragging').textContent).toBe('false');
    expect(byId('mode').textContent).toBe('none');

    // The cancelled drag must not be resurrected by the events that are still in flight.
    fireEvent.pointerMove(surface(), { pointerId: 1, clientX: 3, clientY: 3 });
    fireEvent.pointerUp(surface(), { pointerId: 1, clientX: 3, clientY: 3 });
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it('a fresh pointerdown after Escape starts a clean session and commits once', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 1, button: 0, clientX: 1, clientY: 1 });
    pointAt = byId('cell-1-id');
    fireEvent.pointerMove(surface(), { pointerId: 1, clientX: 2, clientY: 2 });
    fireEvent.keyDown(surface(), { key: 'Escape' });
    expect(onChange).toHaveBeenCalledTimes(1);

    pointAt = null;
    fireEvent.pointerDown(byId('cell-1-note'), { pointerId: 2, button: 0 });
    fireEvent.pointerUp(byId('cell-1-note'), { pointerId: 2, button: 0 });

    expect(onChange).toHaveBeenCalledTimes(2);
    expect(onChange).toHaveBeenLastCalledWith({
      anchor: coord(1, 'note'),
      focus: coord(1, 'note'),
      extraRanges: [],
      mode: 'cell',
    });
    expect(byId('dragging').textContent).toBe('false');
  });

  it('pointercancel ends the session so the following pointerup is a no-op', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 1, button: 0, clientX: 1, clientY: 1 });
    pointAt = byId('cell-1-id');
    fireEvent.pointerMove(surface(), { pointerId: 1, clientX: 2, clientY: 2 });
    fireEvent.pointerCancel(surface(), { pointerId: 1 });
    const afterCancel = onChange.mock.calls.length;

    fireEvent.pointerMove(surface(), { pointerId: 1, clientX: 4, clientY: 4 });
    fireEvent.pointerUp(surface(), { pointerId: 1, clientX: 4, clientY: 4 });

    expect(onChange.mock.calls.length).toBe(afterCancel);
    expect(byId('dragging').textContent).toBe('false');
  });

  it('focus leaving the grid (blur) ends the session without committing it', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 1, button: 0, clientX: 1, clientY: 1 });
    pointAt = byId('cell-1-id');
    fireEvent.pointerMove(surface(), { pointerId: 1, clientX: 2, clientY: 2 });

    fireEvent.focusOut(surface());
    expect(byId('dragging').textContent).toBe('false');
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.pointerUp(surface(), { pointerId: 1, clientX: 2, clientY: 2 });
    expect(onChange).not.toHaveBeenCalled();
  });

  it('hands pointer capture back when Escape cancels the session', () => {
    const setCapture = vi.fn();
    const releaseCapture = vi.fn();
    Object.defineProperty(HTMLElement.prototype, 'setPointerCapture', {
      configurable: true,
      writable: true,
      value: setCapture,
    });
    Object.defineProperty(HTMLElement.prototype, 'releasePointerCapture', {
      configurable: true,
      writable: true,
      value: releaseCapture,
    });
    Object.defineProperty(HTMLElement.prototype, 'hasPointerCapture', {
      configurable: true,
      writable: true,
      value: vi.fn(() => true),
    });

    render(<Harness />);
    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 7, button: 0 });
    expect(setCapture).toHaveBeenCalledWith(7);

    fireEvent.keyDown(surface(), { key: 'Escape' });
    expect(releaseCapture).toHaveBeenCalledWith(7);
  });

  it('does not release a capture the element never held', () => {
    const releaseCapture = vi.fn();
    Object.defineProperty(HTMLElement.prototype, 'setPointerCapture', {
      configurable: true,
      writable: true,
      value: vi.fn(),
    });
    Object.defineProperty(HTMLElement.prototype, 'releasePointerCapture', {
      configurable: true,
      writable: true,
      value: releaseCapture,
    });
    Object.defineProperty(HTMLElement.prototype, 'hasPointerCapture', {
      configurable: true,
      writable: true,
      value: vi.fn(() => false),
    });

    render(<Harness />);
    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 8, button: 0 });
    fireEvent.pointerUp(byId('cell-0-name'), { pointerId: 8, button: 0 });

    expect(releaseCapture).not.toHaveBeenCalled();
  });
});

describe('keyboard guards', () => {
  it('ignores a composing Escape and still handles the plain Escape that follows', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(byId('cell-0-name'), { pointerId: 1, button: 0 });
    expect(byId('mode').textContent).toBe('cell');
    expect(onChange).toHaveBeenCalledTimes(1);

    const composing = new KeyboardEvent('keydown', { key: 'Escape', isComposing: true });
    expect(composing.isComposing).toBe(true);
    fireEvent(surface(), composing);

    // Composition owns the key: the selection must survive and no edit may start.
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(byId('mode').textContent).toBe('cell');

    fireEvent.keyDown(surface(), { key: 'Escape' });
    expect(onChange).toHaveBeenCalledTimes(2);
    expect(onChange).toHaveBeenLastCalledWith(EMPTY_GRID_SELECTION);
    expect(byId('mode').textContent).toBe('none');
  });

  it('ignores a composing Escape emitted through the React synthetic shape', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(byId('cell-0-name'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(byId('cell-0-name'), { pointerId: 1, button: 0 });
    expect(onChange).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(surface(), { key: 'Escape', isComposing: true });

    expect(onChange).toHaveBeenCalledTimes(1);
    expect(byId('mode').textContent).toBe('cell');
  });
});
