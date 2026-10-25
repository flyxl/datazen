import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/react';
import { DataTable } from '../DataTable';

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
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 28 })),
    totalHeight: rows.length * 28,
  }),
}));

vi.mock('../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: vi.fn().mockResolvedValue(undefined),
}));

const COLS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'text' },
];

const ROWS: unknown[][] = [
  [1, 'first'],
  [2, 'second'],
];

function renderGrid(extra: Partial<React.ComponentProps<typeof DataTable>> = {}) {
  return render(
    <DataTable
      columns={COLS}
      rows={ROWS}
      onCellDoubleClick={() => {}}
      onCellEdit={() => {}}
      onCellEditCancel={() => {}}
      {...extra}
    />,
  );
}

/**
 * The DB-17 appearance settings, through the whole grid: every default must
 * be the DOM the grid rendered before the setting existed, and each switch
 * must change exactly its own channel.
 */
describe('DataTable grid appearance (DB-17)', () => {
  afterEach(cleanup);

  it('stripes the odd rows by default', () => {
    const { container } = renderGrid();
    const rows = [...container.querySelectorAll<HTMLElement>('[data-dt-row-view]')];
    expect(rows).toHaveLength(2);
    // Token-wise, not substring-wise: the hover tint (`hover:bg-…`) contains
    // the stripe's text inside its own token.
    expect(rows[0].className.split(/\s+/)).not.toContain('bg-surface-raised/50');
    expect(rows[1].className.split(/\s+/)).toContain('bg-surface-raised/50');
  });

  it('renders every row on the same background when alternatingRows is false', () => {
    const { container } = renderGrid({ alternatingRows: false });
    const rows = [...container.querySelectorAll<HTMLElement>('[data-dt-row-view]')];
    expect(rows[0].className.split(/\s+/)).not.toContain('bg-surface-raised/50');
    expect(rows[1].className.split(/\s+/)).not.toContain('bg-surface-raised/50');
    // The hover channel is untouched by the stripe switch.
    expect(rows[0].className.split(/\s+/)).toContain('hover:bg-surface-raised/50');
  });

  it('renders the row-number gutter by default', () => {
    const { container } = renderGrid();
    // The gutter's select button carries the row number as its text.
    const numbers = container.querySelectorAll('button[title="dataTable.selectRow"]');
    expect(numbers).toHaveLength(2);
    expect(numbers[0].textContent).toBe('1');
  });

  it('drops the gutter (and its select buttons) when showRowNumbers is false', () => {
    const { container } = renderGrid({ showRowNumbers: false });
    expect(container.querySelectorAll('button[title="dataTable.selectRow"]')).toHaveLength(0);
    // The cells themselves are untouched: still two columns of two rows.
    expect(container.querySelectorAll('[data-testid="data-table-cell"]')).toHaveLength(4);
  });

  it('keeps the scrollbars style-less by default', () => {
    const { container } = renderGrid();
    // The scroll container is the only overflow-auto element of the grid body.
    const scroller = container.querySelector('.overflow-auto')!;
    expect(scroller).toBeTruthy();
    expect(scroller.className).not.toContain('dt-scrollbar-autohide');
  });

  it('marks the scroll container for the auto-hide CSS when scrollbarAutoHide is on', () => {
    const { container } = renderGrid({ scrollbarAutoHide: true });
    const scroller = container.querySelector('.overflow-auto')!;
    expect(scroller.className).toContain('dt-scrollbar-autohide');
  });
});
