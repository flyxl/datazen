/**
 * The foreign-key entry of a cell (DB-07).
 *
 * The grid renders the entry in exactly one place, so these are the tests of
 * the DOM contract: which cells carry the marker, when the arrow appears, and
 * that a click on the arrow is a jump and **not** a row selection or the
 * beginning of an edit.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { GridCell, type FkCellAffordance } from '../GridCell';

afterEach(cleanup);

const COLUMN = { id: 'customer_id', name: 'customer_id', type: 'integer' };

function fk(overrides: Partial<FkCellAffordance> = {}): FkCellAffordance {
  return { targetLabel: 'customers.id', navigable: true, resolving: false, ...overrides };
}

function renderCell(overrides: Partial<React.ComponentProps<typeof GridCell>> = {}) {
  const props: React.ComponentProps<typeof GridCell> = {
    rowIndex: 0,
    column: COLUMN,
    value: 7,
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
  return { cell, container, props };
}

describe('GridCell foreign-key entry', () => {
  it('marks a member of a foreign key with the target it points at', () => {
    const { cell } = renderCell({ fk: fk({ navigable: false }) });

    // The **value** of the attribute is the referenced target (00-contracts §4.4).
    expect(cell.getAttribute('data-dt-fk')).toBe('customers.id');
  });

  it('names the table and the columns of a composite target', () => {
    const { cell } = renderCell({ fk: fk({ targetLabel: 'sales.parents.(x, y)' }) });

    expect(cell.getAttribute('data-dt-fk')).toBe('sales.parents.(x, y)');
  });

  it('leaves a column that is not a member without the marker', () => {
    const { cell } = renderCell();

    expect(cell.hasAttribute('data-dt-fk')).toBe(false);
  });

  it('renders the arrow only when the cell may be followed', () => {
    const navigable = renderCell({ fk: fk() });
    expect(navigable.container.querySelector('[data-dt-fk-click="true"]')).not.toBeNull();

    const notNavigable = renderCell({ fk: fk({ navigable: false }) });
    expect(notNavigable.container.querySelector('[data-dt-fk-click="true"]')).toBeNull();
  });

  it('renders no arrow while the cell is edited', () => {
    const { container } = renderCell({ fk: fk(), editing: true });

    expect(container.querySelector('[data-dt-fk-click="true"]')).toBeNull();
  });

  it('names the target in the tooltip and the screen-reader label', () => {
    const { container } = renderCell({ fk: fk() });

    const arrow = container.querySelector('[data-dt-fk-click="true"]') as HTMLElement;
    expect(arrow.getAttribute('title')).toBe('Jump to customers.id');
    expect(arrow.getAttribute('aria-label')).toBe('Jump to customers.id');
  });

  it('names the table and the columns of a composite key', () => {
    const { container } = renderCell({
      fk: fk({ targetLabel: 'parents.(x, y)' }),
    });

    const arrow = container.querySelector('[data-dt-fk-click="true"]') as HTMLElement;
    expect(arrow.getAttribute('title')).toBe('Jump to parents (x, y)');
  });

  it('shows a spinner while the resolution is in flight', () => {
    const { container } = renderCell({ fk: fk({ resolving: true }) });

    const arrow = container.querySelector('[data-dt-fk-click="true"]') as HTMLButtonElement;
    expect(arrow.disabled).toBe(true);
    expect(arrow.getAttribute('title')).toBe('Resolving foreign key…');
  });

  it('reports the jump without starting an edit or a row selection', () => {
    const onFkActivate = vi.fn();
    const onCellDoubleClick = vi.fn();
    const { container } = renderCell({ fk: fk(), onFkActivate, onCellDoubleClick });

    const arrow = container.querySelector('[data-dt-fk-click="true"]') as HTMLElement;
    fireEvent.click(arrow);

    expect(onFkActivate).toHaveBeenCalledTimes(1);
    expect(onFkActivate).toHaveBeenCalledWith(0, 'customer_id', null);
    // A jump is not an edit.
    expect(onCellDoubleClick).not.toHaveBeenCalled();
  });

  it('does not let a double click on the arrow open the editor', () => {
    const onCellDoubleClick = vi.fn();
    const { container } = renderCell({ fk: fk(), onCellDoubleClick });

    const arrow = container.querySelector('[data-dt-fk-click="true"]') as HTMLElement;
    fireEvent.doubleClick(arrow);

    expect(onCellDoubleClick).not.toHaveBeenCalled();
  });

  it('keeps the cell DOM of a grid without foreign keys unchanged', () => {
    const { cell } = renderCell();

    expect(cell.getAttribute('data-testid')).toBe('data-table-cell');
    expect(cell.getAttribute('data-dt-row')).toBe('0');
    expect(cell.getAttribute('data-dt-col')).toBe('customer_id');
    expect(cell.hasAttribute('data-dt-fk')).toBe(false);
    expect(cell.hasAttribute('title')).toBe(false);
    expect(cell.querySelector('[data-dt-fk-click]')).toBeNull();
  });
});
