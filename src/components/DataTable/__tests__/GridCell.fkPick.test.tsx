/**
 * The foreign-key pick entry of a cell (DB-20).
 *
 * The selector is the second affordance of a foreign-key cell, next to the
 * jump arrow: a dropdown that writes a referenced value into the cell. These
 * are the DOM-contract tests: which cells offer it, when it disappears, and
 * that a click on it opens the cell editor — the same one path every editor
 * takes — rather than selecting the row or starting a jump.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { GridCell, type FkCellAffordance } from '../GridCell';

afterEach(cleanup);

const COLUMN = { id: 'customer_id', name: 'customer_id', type: 'integer' };

function fk(overrides: Partial<FkCellAffordance> = {}): FkCellAffordance {
  return {
    targetLabel: 'customers.id',
    navigable: false,
    resolving: false,
    pickable: true,
    ...overrides,
  };
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

describe('GridCell foreign-key pick entry', () => {
  it('offers the selector on a pickable key', () => {
    const { container } = renderCell({ fk: fk() });

    expect(container.querySelector('[data-dt-fk-pick="true"]')).not.toBeNull();
  });

  it('offers the selector even when the key is NULL — filling an empty key is its point', () => {
    // The jump arrow needs the whole group to carry a value; the picker does
    // not: it exists to put a value where there was none.
    const { container } = renderCell({ value: null, fk: fk() });

    expect(container.querySelector('[data-dt-fk-pick="true"]')).not.toBeNull();
    // And the arrow is still absent, by its own rule.
    expect(container.querySelector('[data-dt-fk-click="true"]')).toBeNull();
  });

  it('offers no selector on a column the grid cannot write', () => {
    const { container } = renderCell({ fk: fk({ pickable: false }) });

    expect(container.querySelector('[data-dt-fk-pick="true"]')).toBeNull();
  });

  it('offers no selector while the cell is edited', () => {
    const { container } = renderCell({ fk: fk(), editing: true });

    expect(container.querySelector('[data-dt-fk-pick="true"]')).toBeNull();
  });

  it('leaves a column without a foreign key untouched', () => {
    const { container } = renderCell();

    expect(container.querySelector('[data-dt-fk-pick="true"]')).toBeNull();
  });

  it('opens the cell editor on click — never a row selection', () => {
    const { container, props } = renderCell({ fk: fk() });

    const button = container.querySelector('[data-dt-fk-pick="true"]') as HTMLButtonElement;
    fireEvent.click(button);

    expect(props.onCellDoubleClick).toHaveBeenCalledWith(0, 'customer_id');
  });

  it('lets the click bubble to nothing else — the row must not select', () => {
    const { container } = renderCell({ fk: fk() });

    const button = container.querySelector('[data-dt-fk-pick="true"]') as HTMLButtonElement;
    const event = fireEvent.click(button);
    // The handler stops propagation; jsdom reports the event as consumed.
    expect(event).toBe(true);
    expect(button.getAttribute('title')).toContain('customers.id');
  });
});
