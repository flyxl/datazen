/**
 * DB-02: the React wiring of the grid keyboard.
 *
 * The hook is exercised through a real keydown on a real element — the same
 * path the grid uses — and every assertion is about a callback the grid
 * already has. No store, no mock: the hook adds no action of its own.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render } from '@testing-library/react';
import { useGridKeyboardNav, type UseGridKeyboardNavOptions } from '../useGridKeyboardNav';
import { EMPTY_GRID_SELECTION, type GridSelection } from '../../stores/tableData/gridSelection';

const COLUMNS = ['id', 'name'];

/** The options of a grid that has every action. */
function baseOptions(): UseGridKeyboardNavOptions {
  return {
    columnOrder: COLUMNS,
    rowCount: 10,
    selection: EMPTY_GRID_SELECTION,
    lastSelectedIndex: null,
    editing: false,
    viewportRowCount: 20,
    enabled: true,
    onGridSelectionChange: vi.fn(),
    onCellDoubleClick: vi.fn(),
    onRowGutterSelect: vi.fn(),
    onToggleSelectAll: vi.fn(),
  };
}

/** A grid surface with the hook's handler on it. */
function Harness({ options }: { options: UseGridKeyboardNavOptions }) {
  const nav = useGridKeyboardNav(options);
  return <div data-testid="surface" onKeyDown={nav.handleKeyDown} />;
}

function surface(container: HTMLElement): HTMLElement {
  const found = container.querySelector('[data-testid="surface"]');
  if (!(found instanceof HTMLElement)) throw new Error('surface not found');
  return found;
}

afterEach(cleanup);

describe('useGridKeyboardNav', () => {
  it('claims Cmd+A without writing anything when the grid has no select-all action', () => {
    const onGridSelectionChange = vi.fn();
    const options = {
      ...baseOptions(),
      onGridSelectionChange,
      // The action is missing — the grid must not fall back to a write of its
      // own (02 §4.6): a row-mode write would clear the selected rows.
      onToggleSelectAll: undefined,
    } as unknown as UseGridKeyboardNavOptions;

    const { container } = render(<Harness options={options} />);
    // `fireEvent` reports `false` when the default was prevented.
    expect(fireEvent.keyDown(surface(container), { key: 'a', metaKey: true })).toBe(true);

    expect(onGridSelectionChange).not.toHaveBeenCalled();
  });

  it('asks for the row to be scrolled into view when the row selection moves', () => {
    const onRowGutterSelect = vi.fn();
    const onRequestScrollIntoView = vi.fn();
    const selection: GridSelection = {
      anchor: null,
      focus: null,
      extraRanges: [],
      mode: 'row',
    };
    const options: UseGridKeyboardNavOptions = {
      ...baseOptions(),
      selection,
      lastSelectedIndex: 3,
      onRowGutterSelect,
      onRequestScrollIntoView,
    };

    const { container } = render(<Harness options={options} />);
    fireEvent.keyDown(surface(container), { key: 'ArrowDown' });

    expect(onRowGutterSelect).toHaveBeenCalledWith(4, { multi: false, range: false });
    // The row highlight has to stay visible: the row mode has no cell cursor,
    // so the scroll request is the only thing that follows the row.
    expect(onRequestScrollIntoView).toHaveBeenCalledTimes(1);
    expect(onRequestScrollIntoView).toHaveBeenCalledWith({ rowIndex: 4, columnName: 'id' });
  });

  it('hands Cmd+A to the select-all action and writes no selection of its own', () => {
    const onGridSelectionChange = vi.fn();
    const onToggleSelectAll = vi.fn();
    const options: UseGridKeyboardNavOptions = {
      ...baseOptions(),
      onGridSelectionChange,
      onToggleSelectAll,
    };

    const { container } = render(<Harness options={options} />);
    expect(fireEvent.keyDown(surface(container), { key: 'a', metaKey: true })).toBe(false);

    expect(onToggleSelectAll).toHaveBeenCalledTimes(1);
    expect(onGridSelectionChange).not.toHaveBeenCalled();
  });

});
