import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useCallback, useState } from 'react';
import { EMPTY_GRID_SELECTION, type CellCoord, type GridSelection } from '../../stores/tableData/gridSelection';
import { useGridSelection } from '../useGridSelection';

const COLUMNS = ['name', 'id', 'note'];
const ROW_COUNT = 3;

interface HarnessProps {
  onChange?: (next: GridSelection) => void;
  onRowGutterSelect?: (rowIndex: number, opts: { multi: boolean; range: boolean }) => void;
  enabled?: boolean;
  editingCell?: { row: number; col: string } | null;
}

/**
 * Minimal grid surface: real DOM cells with the same `data-dt-row` / `data-dt-col` contract
 * the virtual body renders, plus the row-gutter and blank-filler shapes from DB-01.
 */
function Harness({ onChange, onRowGutterSelect, enabled, editingCell = null }: HarnessProps) {
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
    onRowGutterSelect,
    enabled,
    editingCell,
  });

  return (
    <div>
      <div data-testid="surface" ref={setScrollElement} {...grid.surfaceProps}>
        <div data-testid="row-0">
          <div role="rowheader">
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
          <div data-testid="cell-0-note" data-dt-row={0} data-dt-col="note" data-dt-editing="true">
            <input data-testid="editor-0-note" />
          </div>
        </div>
        <div data-testid="row-1">
          <div data-testid="cell-1-name" data-dt-row={1} data-dt-col="name">
            Bob
          </div>
          <div data-testid="cell-1-id" data-dt-row={1} data-dt-col="id">
            2
          </div>
          <div data-testid="cell-1-note" data-dt-row={1} data-dt-col="note">
            x
          </div>
        </div>
        <div data-testid="cell-2-id" data-dt-row={2} data-dt-col="id">
          3
        </div>
        <div data-testid="surface-filler" data-dt-surface="true" />
      </div>
      <span data-testid="mode">{selection.mode}</span>
      <span data-testid="selected-name">{String(grid.isCellSelected(0, 'name'))}</span>
      <span data-testid="selected-id">{String(grid.isCellSelected(1, 'id'))}</span>
      <span data-testid="dragging">{String(grid.dragging)}</span>
      <span data-testid="active-descendant">{grid.activeDescendantId ?? ''}</span>
      <span data-testid="focus-key">{grid.focusKey ?? ''}</span>
    </div>
  );
}

const coord = (rowIndex: number, columnName: string): CellCoord => ({ rowIndex, columnName });

function surface() {
  return screen.getByTestId('surface');
}

function cell(testId: string) {
  return screen.getByTestId(testId);
}

function click(testId: string, init: Record<string, unknown> = {}) {
  const target = cell(testId);
  fireEvent.pointerDown(target, { pointerId: 1, button: 0, ...init });
  fireEvent.pointerUp(target, { pointerId: 1, button: 0, ...init });
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
});

describe('useGridSelection pointer interactions', () => {
  it('selects a single cell on a plain click and exposes the focus id/key', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    click('cell-0-name');

    expect(onChange).toHaveBeenLastCalledWith({
      anchor: coord(0, 'name'),
      focus: coord(0, 'name'),
      extraRanges: [],
      mode: 'cell',
    });
    expect(screen.getByTestId('mode').textContent).toBe('cell');
    expect(screen.getByTestId('selected-name').textContent).toBe('true');
    expect(screen.getByTestId('focus-key').textContent).toBe('0:name');
    expect(screen.getByTestId('active-descendant').textContent).toMatch(/-r0c0$/);
  });

  it('extends from the existing anchor on Shift+click', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    click('cell-0-name');
    click('cell-2-id', { shiftKey: true });

    expect(onChange).toHaveBeenLastCalledWith({
      anchor: coord(0, 'name'),
      focus: coord(2, 'id'),
      extraRanges: [],
      mode: 'cell',
    });
  });

  it('toggles a non-contiguous extra range on Cmd/Ctrl+click', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    click('cell-0-name');
    click('cell-1-note', { metaKey: true });
    expect(onChange.mock.lastCall?.[0].extraRanges).toEqual([
      { start: coord(1, 'note'), end: coord(1, 'note') },
    ]);

    click('cell-1-note', { metaKey: true });
    expect(onChange.mock.lastCall?.[0].extraRanges).toEqual([]);
  });

  it('commits a dragged rectangle once, on pointerup', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(cell('cell-0-name'), { pointerId: 1, button: 0 });
    vi.mocked(document.elementFromPoint).mockReturnValue(cell('cell-1-id'));
    fireEvent.pointerMove(surface(), { pointerId: 1, clientX: 120, clientY: 40 });

    expect(screen.getByTestId('dragging').textContent).toBe('true');
    // Nothing is written to the store while the pointer is still down.
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.pointerUp(surface(), { pointerId: 1, clientX: 120, clientY: 40 });

    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith({
      anchor: coord(0, 'name'),
      focus: coord(1, 'id'),
      extraRanges: [],
      mode: 'cell',
    });
    expect(screen.getByTestId('dragging').textContent).toBe('false');
  });

  it('keeps the last valid cell when the drag leaves the grid', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(cell('cell-0-name'), { pointerId: 1, button: 0 });
    vi.mocked(document.elementFromPoint).mockReturnValue(cell('cell-1-id'));
    fireEvent.pointerMove(surface(), { pointerId: 1, clientX: 120, clientY: 40 });
    // Pointer leaves the grid: no cell under the point, so the last valid one is kept.
    vi.mocked(document.elementFromPoint).mockReturnValue(null);
    fireEvent.pointerUp(surface(), { pointerId: 1, clientX: 240, clientY: 60 });

    expect(onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ anchor: coord(0, 'name'), focus: coord(1, 'id') }),
    );
  });

  it('clears the selection on a blank-surface click', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    click('cell-0-name');
    const blank = screen.getByTestId('surface-filler');
    fireEvent.pointerDown(blank, { pointerId: 7, button: 0 });
    fireEvent.pointerUp(blank, { pointerId: 7, button: 0 });

    expect(onChange).toHaveBeenLastCalledWith(EMPTY_GRID_SELECTION);
    expect(screen.getByTestId('mode').textContent).toBe('none');
  });

  it('hands row-gutter clicks to the whole-row path without touching the region', () => {
    const onChange = vi.fn();
    const onRowGutterSelect = vi.fn();
    render(<Harness onChange={onChange} onRowGutterSelect={onRowGutterSelect} />);

    fireEvent.pointerDown(screen.getByTestId('gutter-0'), {
      pointerId: 1,
      button: 0,
      metaKey: true,
    });

    expect(onRowGutterSelect).toHaveBeenCalledWith(0, { multi: true, range: false });
    expect(onChange).not.toHaveBeenCalled();
  });

  it('ignores pointerdown on an open editor', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    fireEvent.pointerDown(screen.getByTestId('editor-0-note'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(screen.getByTestId('editor-0-note'), { pointerId: 1, button: 0 });

    expect(onChange).not.toHaveBeenCalled();
  });

  it('does nothing when disabled', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} enabled={false} />);

    click('cell-0-name');

    expect(onChange).not.toHaveBeenCalled();
  });
});

describe('useGridSelection keyboard and focus', () => {
  it('Escape clears the selection', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    click('cell-0-name');
    fireEvent.keyDown(surface(), { key: 'Escape' });

    expect(onChange).toHaveBeenLastCalledWith(EMPTY_GRID_SELECTION);
    expect(screen.getByTestId('mode').textContent).toBe('none');
  });

  it('Escape belongs to the editor while a cell is being edited', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} editingCell={{ row: 0, col: 'name' }} />);

    click('cell-0-name');
    const callsBeforeEscape = onChange.mock.calls.length;
    fireEvent.keyDown(surface(), { key: 'Escape' });

    expect(onChange.mock.calls).toHaveLength(callsBeforeEscape);
  });

  it('lets focus leave the grid without clearing the selection', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    click('cell-0-name');
    const callsBeforeBlur = onChange.mock.calls.length;
    fireEvent.blur(surface());

    expect(onChange.mock.calls).toHaveLength(callsBeforeBlur);
    expect(screen.getByTestId('mode').textContent).toBe('cell');
  });

  it('reports containment for selected cells only', () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);

    expect(screen.getByTestId('selected-id').textContent).toBe('false');
    click('cell-1-id');
    expect(screen.getByTestId('selected-id').textContent).toBe('true');
    expect(screen.getByTestId('selected-name').textContent).toBe('false');
  });
});
