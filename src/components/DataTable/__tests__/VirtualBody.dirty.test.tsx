/**
 * The dirty-cell rendering of the grid (DB-06 §9.2).
 *
 * The dirty state comes from `buildDirtyIndex` — the same derivation the panel
 * uses — so the test pins the **rendering** of a state the selectors really
 * produce, not a hand-made fixture.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/react';
import { VirtualBody } from '../VirtualBody';
import { buildDirtyIndex } from '../../../stores/tableData/dirtyIndex';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import type { TableState } from '../../../stores/tableData/types';
import type { PendingRowChange } from '../../../lib/tableChanges';
import type { ColumnSchema } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({
      index,
      key: String(index),
      start: index * 40,
    })),
    totalHeight: rows.length * 40,
  }),
}));

const COLUMNS: ColumnSchema[] = [
  { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
  { name: 'name', dataType: 'text', nullable: true },
  { name: 'score', dataType: 'integer', nullable: true },
];

const COLUMN_DEFS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'text' },
  { id: 'score', name: 'score', type: 'integer' },
];

const ROWS = [
  [1, 'Alice', 10],
  [2, 'Bob', 20],
];

afterEach(cleanup);

function panelWith(overrides: Partial<TableState>): TableState {
  return {
    ...emptyTableState(null),
    columns: COLUMNS,
    rows: [
      { id: 1, name: 'Alice', score: 10 },
      { id: 2, name: 'Bob', score: 20 },
    ],
    ...overrides,
  };
}

function renderBody(panel: TableState) {
  const dirtyByRow = buildDirtyIndex(panel).byRowIndex;
  return render(
    <VirtualBody
      columns={COLUMN_DEFS}
      rows={ROWS}
      rowHeight={40}
      editingCell={null}
      selectedRows={new Set()}
      scrollElement={document.createElement('div')}
      onCellDoubleClick={vi.fn()}
      onCellEdit={vi.fn()}
      onCellEditCancel={vi.fn()}
      onRowSelect={vi.fn()}
      dirtyByRow={dirtyByRow}
    />,
  );
}

describe('VirtualBody dirty rendering', () => {
  it('renders data-dt-dirty only on the changed cells', () => {
    const panel = panelWith({
      pendingChanges: new Map<string, PendingRowChange>([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: { name: 'Alice' },
            currentValues: { name: 'Alicia' },
            changedColumns: ['name'],
            deleteMarked: false,
          },
        ],
      ]),
    });

    const { container } = renderBody(panel);
    const dirty = container.querySelectorAll('[data-dt-dirty="true"]');
    expect(dirty).toHaveLength(1);
    expect(dirty[0].getAttribute('data-dt-col')).toBe('name');
    expect(dirty[0].getAttribute('data-dt-row')).toBe('0');
  });

  it('clean cell does not carry the attribute at all', () => {
    const panel = panelWith({
      pendingChanges: new Map<string, PendingRowChange>([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: { name: 'Alice' },
            currentValues: { name: 'Alicia' },
            changedColumns: ['name'],
            deleteMarked: false,
          },
        ],
      ]),
    });

    const { container } = renderBody(panel);
    const clean = container.querySelector('[data-dt-col="id"]');
    expect(clean?.hasAttribute('data-dt-dirty')).toBe(false);
    // Never "false" — the attribute is either "true" or absent (00-contracts §4.4).
    expect(container.querySelector('[data-dt-dirty="false"]')).toBeNull();
  });

  it('dirty cell exposes the original → next tooltip', () => {
    const panel = panelWith({
      pendingChanges: new Map<string, PendingRowChange>([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: { name: 'Alice' },
            currentValues: { name: 'Alicia' },
            changedColumns: ['name'],
            deleteMarked: false,
          },
        ],
      ]),
    });

    const { container } = renderBody(panel);
    const dirty = container.querySelector('[data-dt-dirty="true"]');
    expect(dirty?.getAttribute('title')).toContain('Alice');
    expect(dirty?.getAttribute('title')).toContain('Alicia');
    expect(dirty?.getAttribute('aria-label')).toContain('Alice');
    expect(dirty?.getAttribute('aria-label')).toContain('Alicia');
  });

  it('renders SQL NULL in the tooltip instead of an empty value', () => {
    const panel = panelWith({
      pendingChanges: new Map<string, PendingRowChange>([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: { name: 'Alice' },
            currentValues: { name: null },
            changedColumns: ['name'],
            deleteMarked: false,
          },
        ],
      ]),
    });

    const { container } = renderBody(panel);
    const dirty = container.querySelector('[data-dt-dirty="true"]');
    expect(dirty?.getAttribute('title')).toContain('NULL');
  });

  it('delete-marked row is struck through and its cells are not dirty', () => {
    const panel = panelWith({
      pendingChanges: new Map<string, PendingRowChange>([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: { name: 'Alice' },
            currentValues: { name: 'Alicia' },
            changedColumns: ['name'],
            deleteMarked: true,
          },
        ],
      ]),
    });

    const { container } = renderBody(panel);
    const row = container.querySelector('[data-dt-row-view="0"]');
    expect(row?.getAttribute('data-dt-row-state')).toBe('deleted');
    expect(row?.className).toContain('line-through');
    expect(container.querySelectorAll('[data-dt-dirty="true"]')).toHaveLength(0);
  });

  it('row-number gutter shows a dot for dirty and for deleted rows', () => {
    const panel = panelWith({
      pendingChanges: new Map<string, PendingRowChange>([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: { name: 'Alice' },
            currentValues: { name: 'Alicia' },
            changedColumns: ['name'],
            deleteMarked: false,
          },
        ],
        [
          '["id":2]',
          {
            rowIdentity: { id: 2 },
            originalValues: {},
            currentValues: {},
            changedColumns: [],
            deleteMarked: true,
          },
        ],
      ]),
    });

    const { container } = renderBody(panel);
    expect(container.querySelectorAll('[data-testid="row-dirty-dot"]')).toHaveLength(1);
    expect(container.querySelectorAll('[data-testid="row-deleted-dot"]')).toHaveLength(1);
    expect(container.querySelector('[data-dt-row-view="0"]')?.getAttribute('data-dt-row-state')).toBe(
      'dirty',
    );
  });

  it('no dirty attributes and no dots when dirtyByRow is omitted', () => {
    const { container } = render(
      <VirtualBody
        columns={COLUMN_DEFS}
        rows={ROWS}
        rowHeight={40}
        editingCell={null}
        selectedRows={new Set()}
        scrollElement={document.createElement('div')}
        onCellDoubleClick={vi.fn()}
        onCellEdit={vi.fn()}
        onCellEditCancel={vi.fn()}
        onRowSelect={vi.fn()}
      />,
    );

    expect(container.querySelectorAll('[data-dt-dirty]')).toHaveLength(0);
    expect(container.querySelectorAll('[data-dt-row-state]')).toHaveLength(0);
    expect(container.querySelectorAll('[data-testid="row-dirty-dot"]')).toHaveLength(0);
    expect(container.querySelectorAll('[data-testid="row-deleted-dot"]')).toHaveLength(0);
  });

  it('the number of painted cells equals the index count of the page (A2)', () => {
    const panel = panelWith({
      pendingChanges: new Map<string, PendingRowChange>([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: { name: 'Alice' },
            currentValues: { name: 'Alicia' },
            changedColumns: ['name'],
            deleteMarked: false,
          },
        ],
        [
          '["id":2]',
          {
            rowIdentity: { id: 2 },
            originalValues: { name: 'Bob', score: 20 },
            currentValues: { name: 'Bobby', score: 21 },
            changedColumns: ['name', 'score'],
            deleteMarked: false,
          },
        ],
      ]),
    });

    const counts = buildDirtyIndex(panel).counts;
    const { container } = renderBody(panel);

    expect(container.querySelectorAll('[data-dt-dirty="true"]').length).toBe(counts.cellsOnPage);
    expect(counts.cellsOnPage).toBe(3);
    // The panel count is the cross-page total; the grid paints the page.
    expect(counts.totalCells).toBe(3);
    expect(counts.cellsOffPage).toBe(0);
  });

  it('dirty marker does not replace the type color on the value span', () => {
    const panel = panelWith({
      pendingChanges: new Map<string, PendingRowChange>([
        [
          '["id":1]',
          {
            rowIdentity: { id: 1 },
            originalValues: { score: 10 },
            currentValues: { score: 11 },
            changedColumns: ['score'],
            deleteMarked: false,
          },
        ],
      ]),
    });

    const { container } = renderBody(panel);
    const dirty = container.querySelector('[data-dt-dirty="true"]');
    const value = dirty?.querySelector('span');
    expect(value?.className).toContain('text-dt-number');
  });
});
