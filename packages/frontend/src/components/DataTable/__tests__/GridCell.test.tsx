import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { GridCell } from '../GridCell';
import { resolveDataTableCellFromEvent } from '../../../lib/dataTableContextMenu';

afterEach(cleanup);

const COLUMN = { id: 'id', name: 'id', type: 'integer' };

function renderCell(overrides: Partial<React.ComponentProps<typeof GridCell>> = {}) {
  const props: React.ComponentProps<typeof GridCell> = {
    rowIndex: 0,
    column: COLUMN,
    value: 1,
    width: 160,
    selected: false,
    editing: false,
    onCellDoubleClick: vi.fn(),
    onCellEdit: vi.fn(),
    onCellEditCancel: vi.fn(),
    ...overrides,
  };
  const { container } = render(<GridCell {...props} />);
  const cell = container.firstElementChild as HTMLElement;
  return { cell, props };
}

describe('GridCell DOM contract', () => {
  it('keeps the existing cell attributes on the outermost element', () => {
    const { cell } = renderCell({ rowIndex: 3, column: { id: 'name', name: 'name' } });

    expect(cell.getAttribute('data-testid')).toBe('data-table-cell');
    expect(cell.getAttribute('data-dt-row')).toBe('3');
    expect(cell.getAttribute('data-dt-col')).toBe('name');
    // The context-menu resolver must still find the cell from its outermost element.
    expect(resolveDataTableCellFromEvent(cell)).toEqual({ rowIndex: 3, columnName: 'name' });
  });

  it('renders the normalised type name as data-dt-cell-type', () => {
    const { cell } = renderCell({ cellType: 'number' });
    expect(cell.getAttribute('data-dt-cell-type')).toBe('number');
  });

  it('omits the type attribute when no type is known', () => {
    const { cell } = renderCell();
    expect(cell.hasAttribute('data-dt-cell-type')).toBe(false);
  });

  it('marks a selected cell with data-dt-selected and omits it otherwise', () => {
    const selected = renderCell({ selected: true });
    expect(selected.cell.getAttribute('data-dt-selected')).toBe('true');

    const plain = renderCell({ selected: false });
    expect(plain.cell.hasAttribute('data-dt-selected')).toBe(false);
  });

  it('marks the focus corner with data-dt-active and omits it otherwise', () => {
    const active = renderCell({ active: true });
    expect(active.cell.getAttribute('data-dt-active')).toBe('true');

    const plain = renderCell({ active: false });
    expect(plain.cell.hasAttribute('data-dt-active')).toBe(false);
  });

  it('adds the grid roles only when the grid semantics are on', () => {
    const grid = renderCell({ selected: true, gridSemantics: true });
    expect(grid.cell.getAttribute('role')).toBe('gridcell');
    expect(grid.cell.getAttribute('aria-selected')).toBe('true');

    const plain = renderCell({ selected: true, gridSemantics: false });
    expect(plain.cell.hasAttribute('role')).toBe(false);
    expect(plain.cell.hasAttribute('aria-selected')).toBe(false);
  });

  it('delegates the value rendering to CellRenderer', () => {
    const { cell, props } = renderCell({
      editing: true,
      value: 'Alice',
      column: { id: 'name', name: 'name', type: 'varchar' },
    });
    const input = cell.querySelector('input');
    expect(input).not.toBeNull();
    expect(input?.getAttribute('data-testid')).toBe('table-edit-input');

    fireEvent.change(input!, { target: { value: 'Bob' } });
    fireEvent.keyDown(input!, { key: 'Enter' });
    expect(props.onCellEdit).toHaveBeenCalledWith(0, 'name', 'Bob');
  });

  it('reports a double click with the row and column name', () => {
    const { cell, props } = renderCell({ rowIndex: 2, column: { id: 'name', name: 'name' } });
    fireEvent.doubleClick(cell);
    expect(props.onCellDoubleClick).toHaveBeenCalledWith(2, 'name');
  });
});
