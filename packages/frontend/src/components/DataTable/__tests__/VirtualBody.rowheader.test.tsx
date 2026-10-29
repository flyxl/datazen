/**
 * DB-01 §4.4b: the row-number slot becomes a `rowheader` cell.
 *
 * A `role="row"` may only contain `gridcell` / `rowheader` / `columnheader` children, so the
 * bare `<button>` had to be wrapped. These assertions pin the three constraints of the
 * contract: R-1 (the inner button leaves the tab order), R-2 (the width and the border stay
 * on the rowheader cell) and R-3 (the row-number click still selects exactly one row).
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { VirtualBody } from '../VirtualBody';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

const ROWS = [
  [1, 'Alice'],
  [2, 'Bob'],
];

afterEach(cleanup);

function renderBody(overrides: Partial<React.ComponentProps<typeof VirtualBody>> = {}) {
  const props: React.ComponentProps<typeof VirtualBody> = {
    columns: COLS,
    rows: ROWS,
    rowHeight: 40,
    editingCell: null,
    selectedRows: new Set(),
    scrollElement: document.createElement('div'),
    onCellDoubleClick: vi.fn(),
    onCellEdit: vi.fn(),
    onCellEditCancel: vi.fn(),
    onRowSelect: vi.fn(),
    cellSelectionEnabled: true,
    ...overrides,
  };
  const { container } = render(<VirtualBody {...props} />);
  return { container, props };
}

describe('VirtualBody row header', () => {
  it('renders the gutter as a rowheader cell that still contains a button', () => {
    const { container } = renderBody();
    const row = container.querySelector('[data-dt-row-view="0"]')!;

    const gutter = row.firstElementChild as HTMLElement;
    expect(gutter.tagName).toBe('DIV');
    expect(gutter.getAttribute('role')).toBe('rowheader');
    // The button is not removed, only wrapped — it stays the click target.
    expect(gutter.querySelector('button')).not.toBeNull();
  });

  it('keeps the gutter button out of the tab order', () => {
    const { container } = renderBody();
    const button = container.querySelector('[role="rowheader"] button') as HTMLButtonElement;
    expect(button).not.toBeNull();
    expect(button.getAttribute('tabindex')).toBe('-1');
  });

  it('renders the row as a row and its cells as gridcells', () => {
    const { container } = renderBody();
    const row = container.querySelector('[data-dt-row-view="0"]')!;
    expect(row.getAttribute('role')).toBe('row');

    const cell = container.querySelector('[data-dt-row="0"][data-dt-col="id"]')!;
    expect(cell.getAttribute('role')).toBe('gridcell');
  });

  it('keeps the gutter width and border on the rowheader cell, not on the button', () => {
    const { container } = renderBody();
    const gutter = container.querySelector('[role="rowheader"]') as HTMLElement;
    const button = gutter.querySelector('button') as HTMLElement;

    expect(gutter.className).toContain('w-10');
    expect(gutter.className).toContain('border-r');
    expect(button.className).not.toContain('w-10');
    // The button fills the cell instead of sizing itself.
    expect(button.className).toContain('w-full');
  });

  it('selects the whole row exactly once when the gutter is clicked', () => {
    const { container, props } = renderBody();
    const button = container.querySelector('[role="rowheader"] button') as HTMLButtonElement;

    fireEvent.click(button, { ctrlKey: true });

    expect(props.onRowSelect).toHaveBeenCalledTimes(1);
    expect(props.onRowSelect).toHaveBeenCalledWith(0, { multi: true, range: false });
  });

  it('marks a selected row with aria-selected', () => {
    const { container } = renderBody({ selectedRows: new Set([1]) });
    const selected = container.querySelector('[data-dt-row-view="1"]')!;
    const other = container.querySelector('[data-dt-row-view="0"]')!;

    expect(selected.getAttribute('aria-selected')).toBe('true');
    expect(other.hasAttribute('aria-selected')).toBe(false);
  });

  it('renders no ARIA roles when the grid semantics are off', () => {
    const { container } = renderBody({ cellSelectionEnabled: false });
    const row = container.querySelector('[data-dt-row-view="0"]')!;

    expect(row.hasAttribute('role')).toBe(false);
    expect(container.querySelector('[role="rowheader"]')).toBeNull();
    expect(container.querySelector('[role="gridcell"]')).toBeNull();
  });
});
