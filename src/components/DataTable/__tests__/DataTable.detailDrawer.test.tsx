/**
 * The colour sites of **one** grid (DB-05).
 *
 * The header label and the cell value of a column must read the same
 * driver's declaration — and so must the **detail drawer**, which shows the
 * very same column next to the grid it belongs to.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render } from '@testing-library/react';
import { DataTable } from '../DataTable';
import { DetailPanel } from '../DetailPanel';
import { buildColumnEditorInfo } from '../../../lib/cellEditorKind';
import type { CellTypeHints } from '../../../lib/cellTypes';

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

const MYSQL: CellTypeHints = {
  overrides: [{ match: 'exact', pattern: 'tinyint(1)', cellType: 'bool' }],
};

const COLUMNS = [{ name: 'flag', dataType: 'tinyint(1)', nullable: true }];
const COLUMN_DEFS = [{ id: 'flag', name: 'flag', type: 'tinyint(1)' }];

afterEach(cleanup);

describe('every colour site of one grid reads the same declaration', () => {
  it('the header and the cell agree, and the detail drawer is the third site', () => {
    const columnEditorInfo = buildColumnEditorInfo(COLUMNS, MYSQL);
    const { container } = render(
      <div>
        <DataTable
          columns={COLUMN_DEFS}
          rows={[[1]]}
          cellTypeHints={MYSQL}
          cellEditing={{ panelId: 'panel-detail-drawer', columnEditorInfo, readOnlyReason: null }}
          onCellEditCancel={() => undefined}
        />
        <DetailPanel
          open
          columns={COLUMN_DEFS}
          row={{ flag: 1 }}
          rowIndex={0}
          cellTypeHints={MYSQL}
        />
      </div>,
    );

    const headerLabel = container.querySelector('[title="tinyint(1)"]');
    const cell = container.querySelector('[data-dt-row="0"][data-dt-col="flag"]');
    const cellValue = cell?.querySelector('span');

    // The header and the cell read the declaration.
    expect(headerLabel?.className).toContain('text-dt-bool');
    expect(cellValue?.className).toContain('text-dt-bool');

    // The detail drawer shows the same column — and reads the same
    // declaration, so it does not answer a different question than the
    // grid next to it.
    const drawerLabel = [...container.querySelectorAll('span')].find(
      (node) => node.textContent === 'tinyint(1)',
    );
    expect(drawerLabel?.className).toContain('text-dt-bool');
  });
});
