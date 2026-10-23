/**
 * Round-2 independent re-test (retest agent) — GC-D2 / GC-D3 / GC-D5.
 *
 * The implementer's own suite covers these, but this file asserts them **independently** and,
 * where the implementer has no coverage at all (the `capturePointer` warn path), from scratch.
 *
 *  - GC-D2: every **direct** child of the `role="grid"` surface must be a row or a rowgroup,
 *            in all three content forms (loading / empty / caller placeholder); the status
 *            layer stays a live region **inside** a gridcell.
 *  - GC-D3: the row-mode fallback fires from the row-number slot only — a row-background
 *            click must never publish a row selection, and a consumer that owns `onRowSelect`
 *            must receive nothing on the selection channel at all.
 *  - GC-D5: `setPointerCapture` failures — `NotFoundError` is silent, anything else warns
 *            **without** breaking the interaction.
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { useCallback, useState } from 'react';
import { DataTable } from '../DataTable';
import { EMPTY_GRID_SELECTION, type GridSelection } from '../../../stores/tableData/gridSelection';

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
];

interface HarnessProps {
  onSelectionChange?: (next: GridSelection) => void;
  onRowSelect?: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  rows?: unknown[][];
  loading?: boolean;
  emptyPlaceholder?: React.ReactNode;
}

function Harness({
  onSelectionChange,
  onRowSelect,
  rows = ROWS,
  loading = false,
  emptyPlaceholder,
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
      loading={loading}
      emptyPlaceholder={emptyPlaceholder}
      selectedRows={new Set()}
      onRowSelect={onRowSelect}
      onSelectAll={vi.fn()}
      primaryKeyColumns={['id']}
      gridSelection={selection}
      onGridSelectionChange={handleChange}
    />
  );
}

function surfaceOf(container: HTMLElement): HTMLElement {
  const found = container.querySelector('[data-dt-surface]');
  if (!(found instanceof HTMLElement)) throw new Error('grid surface not found');
  return found;
}

/** Every element child of the grid surface, with its role (null when the role is absent). */
function directChildrenRoles(container: HTMLElement): (string | null)[] {
  return Array.from(surfaceOf(container).children).map(
    (child) => (child instanceof HTMLElement ? child.getAttribute('role') : null),
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
  Reflect.deleteProperty(Element.prototype, 'setPointerCapture');
});

describe('GC-D2 · the grid only contains rows and row groups', () => {
  it('keeps every direct child a row/rowgroup while loading', () => {
    const { container } = render(<Harness loading={true} />);
    const roles = directChildrenRoles(container);

    expect(roles.length).toBeGreaterThan(0);
    for (const role of roles) {
      expect(['row', 'rowgroup']).toContain(role);
    }
    // The loading layer is a row of its own.
    expect(roles).toContain('row');
  });

  it('keeps every direct child a row/rowgroup for an empty table', () => {
    const { container } = render(<Harness rows={[]} />);
    const roles = directChildrenRoles(container);

    expect(roles.length).toBeGreaterThan(0);
    for (const role of roles) {
      expect(['row', 'rowgroup']).toContain(role);
    }
    expect(roles).toContain('row');
  });

  it('keeps every direct child a row/rowgroup for a caller-provided placeholder', () => {
    const { container } = render(
      <Harness emptyPlaceholder={<div data-testid="placeholder">nothing to show</div>} />,
    );
    const roles = directChildrenRoles(container);

    expect(roles.length).toBeGreaterThan(0);
    for (const role of roles) {
      expect(['row', 'rowgroup']).toContain(role);
    }
    // The placeholder is still rendered, just wrapped.
    expect(container.querySelector('[data-testid="placeholder"]')).not.toBeNull();
  });

  it('keeps the loading and empty status layers live regions inside a gridcell', () => {
    const loading = render(<Harness loading={true} />);
    const status = loading.container.querySelector('[role="status"]');
    expect(status).not.toBeNull();
    expect(status?.getAttribute('aria-live')).toBe('polite');
    // The live region sits inside a gridcell of a row.
    expect(status?.closest('[role="gridcell"]')).not.toBeNull();
    expect(status?.closest('[role="row"]')).not.toBeNull();
    cleanup();

    const empty = render(<Harness rows={[]} />);
    const emptyStatus = empty.container.querySelector('[role="status"]');
    expect(emptyStatus).not.toBeNull();
    expect(emptyStatus?.getAttribute('aria-live')).toBe('polite');
    expect(emptyStatus?.closest('[role="gridcell"]')).not.toBeNull();
    expect(emptyStatus?.closest('[role="row"]')).not.toBeNull();
  });
});

describe('GC-D3 · the row-mode fallback belongs to the row-number slot only', () => {
  it('does not publish a row selection when the row background is clicked', () => {
    const onSelectionChange = vi.fn();
    const onRowSelect = vi.fn();
    const { container } = render(
      <Harness onSelectionChange={onSelectionChange} onRowSelect={onRowSelect} />,
    );

    // The row container itself (its background, not a cell and not the gutter).
    const row = container.querySelector('[data-dt-row-view="0"]') as HTMLElement;
    fireEvent.click(row);

    expect(onRowSelect).toHaveBeenCalledTimes(1);
    expect(onSelectionChange).not.toHaveBeenCalled();
  });

  it('publishes nothing on the selection channel when the consumer owns onRowSelect', () => {
    const onSelectionChange = vi.fn();
    const onRowSelect = vi.fn();
    const { container } = render(
      <Harness onSelectionChange={onSelectionChange} onRowSelect={onRowSelect} />,
    );

    const gutter = container.querySelector('[role="rowheader"] button') as HTMLButtonElement;
    fireEvent.click(gutter);

    expect(onRowSelect).toHaveBeenCalledTimes(1);
    expect(onRowSelect).toHaveBeenCalledWith(0, { multi: false, range: false });
    expect(onSelectionChange).not.toHaveBeenCalled();
  });

  it('falls back to one geometry-free row selection from the gutter only', () => {
    const onSelectionChange = vi.fn();
    const { container } = render(<Harness onSelectionChange={onSelectionChange} />);

    const gutter = container.querySelector('[role="rowheader"] button') as HTMLButtonElement;
    fireEvent.click(gutter);

    expect(onSelectionChange).toHaveBeenCalledTimes(1);
    const emitted = onSelectionChange.mock.calls[0][0] as GridSelection;
    expect(emitted.mode).toBe('row');
    expect(emitted.anchor).toBeNull();
    expect(emitted.focus).toBeNull();
    expect(emitted.extraRanges).toEqual([]);
  });
});

describe('GC-D5 · pointer capture failures stay observable but never break the click', () => {
  it('warns once and still completes the selection for a non-NotFoundError', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const onSelectionChange = vi.fn();
    Object.defineProperty(Element.prototype, 'setPointerCapture', {
      configurable: true,
      writable: true,
      value: () => {
        throw new TypeError('capture exploded');
      },
    });

    const { container } = render(<Harness onSelectionChange={onSelectionChange} />);
    const target = cellOf(container, 0, 'id');
    fireEvent.pointerDown(target, { pointerId: 1, button: 0 });
    fireEvent.pointerUp(target, { pointerId: 1, button: 0 });

    expect(warn).toHaveBeenCalled();
    // The interaction was NOT aborted: the selection still landed.
    expect(onSelectionChange).toHaveBeenCalledTimes(1);
    expect((onSelectionChange.mock.calls[0][0] as GridSelection).mode).toBe('cell');
  });

  it('stays silent for NotFoundError and still completes the selection', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const onSelectionChange = vi.fn();
    Object.defineProperty(Element.prototype, 'setPointerCapture', {
      configurable: true,
      writable: true,
      value: () => {
        throw new DOMException('no active pointer', 'NotFoundError');
      },
    });

    const { container } = render(<Harness onSelectionChange={onSelectionChange} />);
    const target = cellOf(container, 0, 'id');
    fireEvent.pointerDown(target, { pointerId: 1, button: 0 });
    fireEvent.pointerUp(target, { pointerId: 1, button: 0 });

    expect(warn).not.toHaveBeenCalled();
    expect(onSelectionChange).toHaveBeenCalledTimes(1);
    expect((onSelectionChange.mock.calls[0][0] as GridSelection).mode).toBe('cell');
  });
});
