import { describe, expect, it, vi } from 'vitest';
import {
  buildDataTableContextMenuItems,
  serializeDataTableRowsAsTsv,
  type DataTableContextMenuLabels,
} from '../dataTableContextMenu';

const labels: DataTableContextMenuLabels = {
  copyCell: 'Copy Cell',
  copySelectedRows: 'Copy Selected Rows',
  export: 'Export',
};

function ids(items: ReturnType<typeof buildDataTableContextMenuItems>): string[] {
  return items.map((i) => (i.kind === 'item' ? i.id : i.kind));
}

describe('serializeDataTableRowsAsTsv', () => {
  it('joins cells with tabs and rows with newlines; nullish becomes empty', () => {
    expect(
      serializeDataTableRowsAsTsv([
        [1, 'a', null],
        [2, undefined, 'b'],
      ]),
    ).toBe('1\ta\t\n2\t\tb');
  });

  it('returns empty string for no rows', () => {
    expect(serializeDataTableRowsAsTsv([])).toBe('');
  });
});

describe('buildDataTableContextMenuItems', () => {
  it('includes all items when cell, rows, and export are available', () => {
    const onCopyCell = vi.fn();
    const onCopySelectedRows = vi.fn();
    const onExport = vi.fn();
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: { onCopyCell, onCopySelectedRows, onExport },
      cellText: 'hello',
      selectedRowTexts: ['1\ta', '2\tb'],
      exportEnabled: true,
    });
    expect(ids(items)).toEqual(['copy-cell', 'copy-selected-rows', 'export']);
    for (const it of items) {
      if (it.kind === 'item') it.action();
    }
    expect(onCopyCell).toHaveBeenCalledOnce();
    expect(onCopySelectedRows).toHaveBeenCalledOnce();
    expect(onExport).toHaveBeenCalledOnce();
  });

  it('omits copy-cell when cellText is missing or empty', () => {
    const base = {
      labels,
      handlers: { onCopyCell: vi.fn(), onExport: vi.fn() },
      exportEnabled: true,
    };
    expect(ids(buildDataTableContextMenuItems({ ...base, cellText: null }))).toEqual(['export']);
    expect(ids(buildDataTableContextMenuItems({ ...base, cellText: '' }))).toEqual(['export']);
    expect(ids(buildDataTableContextMenuItems({ ...base }))).toEqual(['export']);
  });

  it('omits copy-selected-rows when selectedRowTexts is missing or empty', () => {
    const base = {
      labels,
      handlers: { onCopySelectedRows: vi.fn(), onExport: vi.fn() },
      exportEnabled: true,
    };
    expect(ids(buildDataTableContextMenuItems({ ...base, selectedRowTexts: null }))).toEqual([
      'export',
    ]);
    expect(ids(buildDataTableContextMenuItems({ ...base, selectedRowTexts: [] }))).toEqual([
      'export',
    ]);
    expect(ids(buildDataTableContextMenuItems({ ...base }))).toEqual(['export']);
  });

  it('omits export when exportEnabled is false or omitted', () => {
    const withCell = buildDataTableContextMenuItems({
      labels,
      handlers: { onCopyCell: vi.fn(), onExport: vi.fn() },
      cellText: 'x',
      exportEnabled: false,
    });
    expect(ids(withCell)).toEqual(['copy-cell']);

    const omitted = buildDataTableContextMenuItems({
      labels,
      handlers: { onCopyCell: vi.fn(), onExport: vi.fn() },
      cellText: 'x',
    });
    expect(ids(omitted)).toEqual(['copy-cell']);
  });

  it('skips items whose handlers are missing', () => {
    const items = buildDataTableContextMenuItems({
      labels,
      handlers: {},
      cellText: 'x',
      selectedRowTexts: ['a'],
      exportEnabled: true,
    });
    expect(ids(items)).toEqual([]);
  });

  it('returns empty when nothing is available', () => {
    expect(
      ids(
        buildDataTableContextMenuItems({
          labels,
          handlers: {
            onCopyCell: vi.fn(),
            onCopySelectedRows: vi.fn(),
            onExport: vi.fn(),
          },
        }),
      ),
    ).toEqual([]);
  });
});
