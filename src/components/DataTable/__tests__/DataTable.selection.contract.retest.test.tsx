/**
 * Independent re-test of the Wave-2 `grid-cell` DOM contract.
 *
 * This file is written by the RETEST agent, not by the implementer, and asserts the contract
 * from `docs/prd/data-browsing-design/00-contracts.md` §4.4 against the **rendered DOM**:
 *
 *  - `[data-dt-surface]`  : the scroll container, fixed value `""`, carrying the whole ARIA grid host set
 *  - `[data-dt-selected]` : `"true"` / attribute absent (never `"false"`)
 *  - `[role="rowheader"]` : the row-number slot (outer div owns width/border, inner button leaves the tab order)
 *  - the summary counts    : `data-dt-selection-rows|cols|cells` must be the *covered* counts
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { useCallback, useState } from 'react';
import { DataTable } from '../DataTable';
import { EMPTY_GRID_SELECTION, type GridSelection } from '../../../stores/tableData/gridSelection';
import { t as translate } from '../../../locales/t';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({
    columnWidths: [160, 200, 200],
    onResizeStart: vi.fn(),
  }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

// Three data columns (id / code / name) and ten rows, so "covered" and "page" counts differ.
const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'code', name: 'code', type: 'varchar' },
  { id: 'name', name: 'name', type: 'varchar' },
];

const ROWS = Array.from({ length: 10 }, (_, i) => [i + 1, `code_${i}`, `row_${i}`]);

interface HarnessProps {
  onSelectionChange?: (next: GridSelection) => void;
  onRowSelect?: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  selectedRows?: Set<number>;
}

function Harness({ onSelectionChange, onRowSelect, selectedRows }: HarnessProps) {
  const [selection, setSelection] = useState<GridSelection>(EMPTY_GRID_SELECTION);
  const handleChange = useCallback(
    (next: GridSelection) => {
      setSelection(next);
      onSelectionChange?.(next);
    },
    [onSelectionChange],
  );
  return (
    <DataTable
      columns={COLS}
      rows={ROWS}
      selectedRows={selectedRows}
      onRowSelect={onRowSelect}
      onSelectAll={vi.fn()}
      primaryKeyColumns={['id']}
      gridSelection={selection}
      onGridSelectionChange={handleChange}
    />
  );
}

function cellOf(container: HTMLElement, rowIndex: number, columnName: string): HTMLElement {
  const found = container.querySelector(
    `[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`,
  );
  if (!(found instanceof HTMLElement)) throw new Error(`cell ${rowIndex}/${columnName} missing`);
  return found;
}

beforeEach(() => {
  vi.stubEnv('VITE_E2E', '1');
});

afterEach(() => {
  cleanup();
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});

describe('contract · [data-dt-surface]', () => {
  it('is a single element whose value is the fixed empty string (not "true")', () => {
    const { container } = render(<Harness />);
    const surfaces = container.querySelectorAll('[data-dt-surface]');

    // Exactly one: the scroll container. The virtual filler must NOT carry it too, or
    // `document.querySelector('[data-dt-surface]')` becomes ambiguous (01 §3.7).
    expect(surfaces).toHaveLength(1);
    const surface = surfaces[0] as HTMLElement;
    expect(surface.hasAttribute('data-dt-surface')).toBe(true);
    expect(surface.getAttribute('data-dt-surface')).toBe('');
    expect(surface.getAttribute('data-dt-surface')).not.toBe('true');
  });

  it('is the element that carries the ARIA grid host set', () => {
    const { container } = render(<Harness />);
    const surface = container.querySelector('[data-dt-surface]') as HTMLElement;

    expect(surface.getAttribute('role')).toBe('grid');
    expect(surface.getAttribute('tabindex')).toBe('0');
    expect(surface.getAttribute('aria-rowcount')).toBe(String(ROWS.length + 1));
    expect(surface.getAttribute('aria-colcount')).toBe(String(COLS.length + 1));
  });

  it('adds the whole ARIA role set atomically (grid / row / gridcell / rowheader / columnheader)', () => {
    const { container } = render(<Harness />);

    expect(container.querySelector('[role="grid"]')).not.toBeNull();
    expect(container.querySelector('[role="rowgroup"]')).not.toBeNull();
    expect(container.querySelectorAll('[role="row"]').length).toBeGreaterThan(0);
    expect(container.querySelectorAll('[role="gridcell"]').length).toBeGreaterThan(0);
    expect(container.querySelectorAll('[role="rowheader"]').length).toBe(ROWS.length);
    expect(container.querySelectorAll('[role="columnheader"]').length).toBe(COLS.length + 1);
  });
});

describe('contract · [data-dt-selected]', () => {
  it('is absent (never "false") on an unselected cell', () => {
    const { container } = render(<Harness />);
    const plain = cellOf(container, 0, 'id');

    expect(plain.hasAttribute('data-dt-selected')).toBe(false);
    expect(plain.getAttribute('data-dt-selected')).toBeNull();
    expect(plain.getAttribute('data-dt-selected')).not.toBe('false');
  });

  it('is exactly "true" on a selected cell and on nothing else', () => {
    const { container } = render(<Harness />);

    fireEvent.pointerDown(cellOf(container, 1, 'code'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(cellOf(container, 1, 'code'), { pointerId: 1, button: 0 });

    const marked = container.querySelectorAll('[data-dt-selected]');
    expect(marked).toHaveLength(1);
    expect(marked[0].getAttribute('data-dt-selected')).toBe('true');
    expect(marked[0]).toBe(cellOf(container, 1, 'code'));
  });
});

describe('contract · [role="rowheader"]', () => {
  it('keeps width and border on the rowheader cell and the button out of the tab order', () => {
    const { container } = render(<Harness />);
    const gutter = container.querySelector('[role="rowheader"]') as HTMLElement;
    const button = gutter.querySelector('button') as HTMLButtonElement;

    expect(gutter.tagName).toBe('DIV');
    expect(gutter.className).toContain('w-10');
    expect(gutter.className).toContain('border-r');
    expect(button.className).not.toContain('w-10');
    expect(button.getAttribute('tabindex')).toBe('-1');
    expect(button.hasAttribute('tabindex')).toBe(true);
  });

  it('keeps the row number, the title and the single-shot click', () => {
    const onRowSelect = vi.fn();
    const { container } = render(<Harness onRowSelect={onRowSelect} />);
    const button = container.querySelectorAll('[role="rowheader"] button')[2] as HTMLButtonElement;

    expect(button.textContent).toBe('3');
    expect(button.getAttribute('title')).toBe('dataTable.selectRow');

    fireEvent.click(button);
    expect(onRowSelect).toHaveBeenCalledTimes(1);
    expect(onRowSelect).toHaveBeenCalledWith(2, { multi: false, range: false });
  });
});

describe('contract · selection summary counts', () => {
  it('reports the covered 4x3 rectangle as 4 / 3 / 12, not the page size', () => {
    const { container, getByTestId } = render(<Harness />);

    // (row 2, id) -> (row 5, name): 4 rows x 3 columns.
    fireEvent.pointerDown(cellOf(container, 2, 'id'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(cellOf(container, 2, 'id'), { pointerId: 1, button: 0 });
    fireEvent.pointerDown(cellOf(container, 5, 'name'), { pointerId: 1, button: 0, shiftKey: true });
    fireEvent.pointerUp(cellOf(container, 5, 'name'), { pointerId: 1, button: 0, shiftKey: true });

    const summary = getByTestId('data-table-selection-summary');
    expect(summary.getAttribute('data-dt-selection-rows')).toBe('4');
    expect(summary.getAttribute('data-dt-selection-cols')).toBe('3');
    expect(summary.getAttribute('data-dt-selection-cells')).toBe('12');
    // The wording comes from the English domain pack (the component test mocks `t` to the
    // key, so the real string is checked through the locale helper).
    expect(translate('dataTable.selectionSummary', { rows: 4, cols: 3 })).toBe(
      '4 rows × 3 columns',
    );
    // The page itself has 10 rows x 3 columns — the counts must be the covered ones.
    expect(container.querySelectorAll('[data-dt-selected="true"]')).toHaveLength(12);
  });

  it('reports the covered counts, not the grid dimensions, for a 2x2 rectangle', () => {
    const { container, getByTestId } = render(<Harness />);

    fireEvent.pointerDown(cellOf(container, 0, 'id'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(cellOf(container, 0, 'id'), { pointerId: 1, button: 0 });
    fireEvent.pointerDown(cellOf(container, 1, 'code'), { pointerId: 1, button: 0, shiftKey: true });
    fireEvent.pointerUp(cellOf(container, 1, 'code'), { pointerId: 1, button: 0, shiftKey: true });

    const summary = getByTestId('data-table-selection-summary');
    expect(summary.getAttribute('data-dt-selection-rows')).toBe('2');
    expect(summary.getAttribute('data-dt-selection-cols')).toBe('2');
    expect(summary.getAttribute('data-dt-selection-cells')).toBe('4');
  });
});

describe('contract · row-mode fallback does not fight the paired write', () => {
  it('uses the consumer row path when onRowSelect is provided (no second row selection)', () => {
    const onSelectionChange = vi.fn();
    const onRowSelect = vi.fn();
    const { container } = render(
      <Harness onSelectionChange={onSelectionChange} onRowSelect={onRowSelect} />,
    );

    const button = container.querySelector('[role="rowheader"] button') as HTMLButtonElement;
    fireEvent.click(button);

    expect(onRowSelect).toHaveBeenCalledTimes(1);
    // The grid must NOT publish a second, competing row-mode selection.
    expect(onSelectionChange).not.toHaveBeenCalled();
  });

  it('falls back to one geometry-free row selection when no onRowSelect exists', () => {
    const onSelectionChange = vi.fn();
    const { container } = render(<Harness onSelectionChange={onSelectionChange} />);

    const button = container.querySelector('[role="rowheader"] button') as HTMLButtonElement;
    fireEvent.click(button);

    expect(onSelectionChange).toHaveBeenCalledTimes(1);
    const emitted = onSelectionChange.mock.calls[0][0] as GridSelection;
    expect(emitted.mode).toBe('row');
    // I2: row mode carries no geometry at all.
    expect(emitted.anchor).toBeNull();
    expect(emitted.focus).toBeNull();
    expect(emitted.extraRanges).toEqual([]);
  });
});
