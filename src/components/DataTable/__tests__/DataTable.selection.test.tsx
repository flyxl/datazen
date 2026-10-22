/**
 * DB-01 DOM wiring: the grid surface, the selected cells, the row header and the selection
 * summary are the four contracts the E2E suites and the later chapters rely on.
 *
 * Every case asserts the **rendered DOM**, not just the state: a selection that is correct
 * in the store but invisible in the DOM is not wired up.
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { useCallback, useState } from 'react';
import { DataTable } from '../DataTable';
import {
  EMPTY_GRID_SELECTION,
  type GridSelection,
} from '../../../stores/tableData/gridSelection';
import { t as translate } from '../../../locales/t';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({
    columnWidths: [160, 200],
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

const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

const ROWS = [
  [1, 'Alice'],
  [2, 'Bob'],
  [3, 'Cara'],
];

interface HarnessProps {
  onSelectionChange?: (next: GridSelection) => void;
  onRowSelect?: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  onDeleteRows?: (rowIndices: number[]) => void;
  selectedRows?: Set<number>;
  editingCell?: { row: number; col: string } | null;
  withSelectionProps?: boolean;
  /** Rows to render; defaults to the sample rows. */
  rows?: unknown[][];
  loading?: boolean;
  emptyPlaceholder?: React.ReactNode;
  /** Omit `onRowSelect` to exercise the grid-owned row-mode fallback. */
  withRowSelect?: boolean;
}

/** Controlled grid: the selection lives in the harness, the grid only reports changes. */
function Harness({
  onSelectionChange,
  onRowSelect,
  onDeleteRows,
  selectedRows,
  editingCell = null,
  withSelectionProps = true,
  rows = ROWS,
  loading = false,
  emptyPlaceholder,
  withRowSelect = true,
}: HarnessProps) {
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
      rows={rows}
      selectedRows={selectedRows}
      editingCell={editingCell}
      onRowSelect={withRowSelect ? onRowSelect : undefined}
      onSelectAll={vi.fn()}
      primaryKeyColumns={['id']}
      onDeleteRows={onDeleteRows}
      loading={loading}
      emptyPlaceholder={emptyPlaceholder}
      {...(withSelectionProps
        ? { gridSelection: selection, onGridSelectionChange: handleChange }
        : {})}
    />
  );
}

function cell(container: HTMLElement, rowIndex: number, columnName: string): HTMLElement {
  const found = container.querySelector(
    `[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`,
  );
  if (!(found instanceof HTMLElement)) throw new Error(`cell ${rowIndex}/${columnName} not found`);
  return found;
}

function surface(container: HTMLElement): HTMLElement {
  const found = container.querySelector('[data-dt-surface]');
  if (!(found instanceof HTMLElement)) throw new Error('grid surface not found');
  return found;
}

/**
 * The selected **cells**. The row containers carry `data-dt-selected` as well
 * (DB-02: the row highlight needs a locator of its own), so a cell locator
 * has to name the cell attributes.
 */
function selectedCells(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll('[data-dt-row][data-dt-selected="true"]'));
}

beforeEach(() => {
  // `tid` only renders the E2E locators in an E2E build; the summary testid is one of them.
  vi.stubEnv('VITE_E2E', '1');
});

afterEach(() => {
  cleanup();
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});

describe('DataTable selection surface', () => {
  it('marks the scroll container as the selection surface', () => {
    const { container } = render(<Harness />);
    const grid = surface(container);

    expect(grid.getAttribute('role')).toBe('grid');
    expect(grid.getAttribute('tabindex')).toBe('0');
    // 3 data rows + the header row, 2 data columns + the row-number column.
    expect(grid.getAttribute('aria-rowcount')).toBe('4');
    expect(grid.getAttribute('aria-colcount')).toBe('3');
  });

  it('keeps today\'s DOM when the selection props are not passed', () => {
    const { container } = render(<Harness withSelectionProps={false} />);

    expect(container.querySelector('[data-dt-surface]')).toBeNull();
    expect(container.querySelector('[role="grid"]')).toBeNull();
    expect(container.querySelector('[data-dt-selected]')).toBeNull();
    expect(container.querySelector('[role="gridcell"]')).toBeNull();
  });

  it('enters cell mode on a plain click and marks exactly one cell', () => {
    const onSelectionChange = vi.fn();
    const { container } = render(<Harness onSelectionChange={onSelectionChange} />);

    fireEvent.pointerDown(cell(container, 0, 'id'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(cell(container, 0, 'id'), { pointerId: 1, button: 0 });

    expect(onSelectionChange).toHaveBeenLastCalledWith({
      anchor: { rowIndex: 0, columnName: 'id' },
      focus: { rowIndex: 0, columnName: 'id' },
      extraRanges: [],
      mode: 'cell',
    });
    expect(selectedCells(container)).toHaveLength(1);
    expect(selectedCells(container)[0]).toBe(cell(container, 0, 'id'));
  });

  it('does not turn a cell click into a whole-row selection', () => {
    const onRowSelect = vi.fn();
    const { container } = render(<Harness onRowSelect={onRowSelect} />);

    fireEvent.pointerDown(cell(container, 1, 'name'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(cell(container, 1, 'name'), { pointerId: 1, button: 0 });
    fireEvent.click(cell(container, 1, 'name'));

    expect(onRowSelect).not.toHaveBeenCalled();
    expect(selectedCells(container)).toHaveLength(1);
  });

  it('extends the rectangle from the anchor on shift+click and updates the summary', () => {
    const { container, getByTestId } = render(<Harness />);

    fireEvent.pointerDown(cell(container, 0, 'id'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(cell(container, 0, 'id'), { pointerId: 1, button: 0 });
    fireEvent.pointerDown(cell(container, 1, 'name'), { pointerId: 1, button: 0, shiftKey: true });
    fireEvent.pointerUp(cell(container, 1, 'name'), { pointerId: 1, button: 0, shiftKey: true });

    expect(selectedCells(container)).toHaveLength(4);

    const summary = getByTestId('data-table-selection-summary');
    expect(summary.getAttribute('data-dt-selection-rows')).toBe('2');
    expect(summary.getAttribute('data-dt-selection-cols')).toBe('2');
    expect(summary.getAttribute('data-dt-selection-cells')).toBe('4');
  });

  it('points aria-activedescendant at the focused cell', () => {
    const { container } = render(<Harness />);

    fireEvent.pointerDown(cell(container, 2, 'name'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(cell(container, 2, 'name'), { pointerId: 1, button: 0 });

    const active = surface(container).getAttribute('aria-activedescendant');
    expect(active).toBeTruthy();
    expect(cell(container, 2, 'name').getAttribute('id')).toBe(active);
  });

  it('clears the selection on Escape', () => {
    const onSelectionChange = vi.fn();
    const { container } = render(<Harness onSelectionChange={onSelectionChange} />);

    fireEvent.pointerDown(cell(container, 0, 'id'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(cell(container, 0, 'id'), { pointerId: 1, button: 0 });
    expect(selectedCells(container)).toHaveLength(1);

    fireEvent.keyDown(surface(container), { key: 'Escape' });

    expect(onSelectionChange).toHaveBeenLastCalledWith(EMPTY_GRID_SELECTION);
    expect(container.querySelector('[data-dt-selected]')).toBeNull();
  });

  it('leaves Escape to the editor while a cell is being edited', () => {
    const onSelectionChange = vi.fn();
    const { container } = render(
      <Harness onSelectionChange={onSelectionChange} editingCell={{ row: 0, col: 'id' }} />,
    );

    fireEvent.pointerDown(cell(container, 0, 'id'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(cell(container, 0, 'id'), { pointerId: 1, button: 0 });
    const callsBefore = onSelectionChange.mock.calls.length;

    fireEvent.keyDown(surface(container), { key: 'Escape' });

    expect(onSelectionChange.mock.calls).toHaveLength(callsBefore);
  });

  it('keeps the row-number slot on the whole-row path', () => {
    const onRowSelect = vi.fn();
    const { container } = render(<Harness onRowSelect={onRowSelect} />);

    const gutter = container.querySelector('[role="rowheader"] button') as HTMLButtonElement;
    fireEvent.click(gutter);

    expect(onRowSelect).toHaveBeenCalledTimes(1);
    expect(onRowSelect).toHaveBeenCalledWith(0, { multi: false, range: false });
  });

  it('marks a selected row with aria-selected', () => {
    const { container } = render(<Harness selectedRows={new Set([1])} />);
    const row = container.querySelector('[data-dt-row-view="1"]')!;

    expect(row.getAttribute('aria-selected')).toBe('true');
  });

  it('deletes the rows covered by the cell range', () => {
    const onDeleteRows = vi.fn();
    const { container, getByTestId } = render(<Harness onDeleteRows={onDeleteRows} />);

    fireEvent.pointerDown(cell(container, 0, 'id'), { pointerId: 1, button: 0 });
    fireEvent.pointerUp(cell(container, 0, 'id'), { pointerId: 1, button: 0 });
    fireEvent.pointerDown(cell(container, 1, 'name'), { pointerId: 1, button: 0, shiftKey: true });
    fireEvent.pointerUp(cell(container, 1, 'name'), { pointerId: 1, button: 0, shiftKey: true });

    fireEvent.click(getByTestId('data-table-delete-rows'));

    expect(onDeleteRows).toHaveBeenCalledWith([0, 1]);
  });

  it('formats the summary text the way the E2E suite expects it', () => {
    // TC-SEL-002 asserts the toolbar text; the wording lives in the English domain pack.
    expect(translate('dataTable.selectionSummary', { rows: 4, cols: 3 })).toBe(
      '4 rows × 3 columns',
    );
  });
});

describe('DataTable ARIA grid structure', () => {
  /** Every direct child of the grid must be a row or a row group. */
  function expectOnlyRows(grid: HTMLElement) {
    const children = Array.from(grid.children);
    expect(children.length).toBeGreaterThan(0);
    for (const child of children) {
      expect(['row', 'rowgroup']).toContain(child.getAttribute('role'));
    }
  }

  it('keeps the grid children limited to rows and row groups', () => {
    const { container } = render(<Harness loading />);
    expectOnlyRows(surface(container));
  });

  it('keeps the grid children limited to rows when the table is empty', () => {
    const { container } = render(<Harness rows={[]} />);
    expectOnlyRows(surface(container));
  });

  it('keeps the grid children limited to rows when an empty placeholder is shown', () => {
    const { container } = render(
      <Harness emptyPlaceholder={<div data-testid="all-columns-hidden">hidden</div>} />,
    );
    expectOnlyRows(surface(container));
  });

  it('keeps the loading status a live region inside the grid', () => {
    const { container, getByRole } = render(<Harness loading />);
    const status = getByRole('status');

    expect(status.getAttribute('aria-live')).toBe('polite');
    // The announcement must survive: the live region stays inside the grid, wrapped in a
    // row/cell instead of hanging off the grid as a non-row child.
    expect(surface(container).contains(status)).toBe(true);
    expect(status.closest('[role="gridcell"]')).not.toBeNull();
  });

  it('keeps the empty state a live region inside the grid', () => {
    const { container, getByRole } = render(<Harness rows={[]} />);
    const status = getByRole('status');

    expect(status.getAttribute('aria-live')).toBe('polite');
    expect(surface(container).contains(status)).toBe(true);
  });
});

describe('DataTable row-mode fallback', () => {
  it('emits the row-mode selection from the row gutter only', () => {
    const onSelectionChange = vi.fn();
    const { container } = render(
      <Harness onSelectionChange={onSelectionChange} withRowSelect={false} />,
    );

    // The row background is not a row-selection entry point: the grid must not invent a
    // selection there, or a consumer that owns its own row set would lose it.
    fireEvent.click(container.querySelector('[data-dt-row-view="0"]')!);
    expect(onSelectionChange).not.toHaveBeenCalled();

    // The gutter is.
    const gutter = container.querySelector('[role="rowheader"] button') as HTMLButtonElement;
    fireEvent.click(gutter);
    expect(onSelectionChange).toHaveBeenCalledTimes(1);
    expect(onSelectionChange.mock.lastCall?.[0].mode).toBe('row');
  });
});
