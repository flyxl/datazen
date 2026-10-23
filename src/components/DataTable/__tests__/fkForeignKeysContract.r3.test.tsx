/**
 * Independent verification of the `foreignKeys` contract (Wave 5 · track A,
 * round 3 — the FKU2-D1 follow-up).
 *
 * The four loose props became **one** object, because the pieces belong
 * together: the arrow of a composite key is decided by the values of **all**
 * of its members — including a member the user has hidden — and only the
 * **full** row records carry those values.
 *
 * Two properties have to hold:
 *
 * 1. **the type makes a half-filled object impossible to write** — a grid
 *    that gets the index but not the records would silently lose the arrow of
 *    every composite key with a hidden member (FKU-D1);
 * 2. **the object is reference-stable** — the grid is `memo`-ised on its
 *    props, so a fresh object on every render would re-render every visible
 *    cell of the page on every scroll.
 *
 * The first one is a **compile-time** property, so it is pinned with
 * `@ts-expect-error`: if `rows` ever became optional, the directive would be
 * unused and `tsc` would report TS2578 ("Unused '@ts-expect-error' directive")
 * — the typecheck gate would go red.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/react';
import { useEffect, useState } from 'react';
import { DataTable, type DataTableProps } from '../DataTable';
import { useTableForeignKeyNavigation } from '../../../windows/connection/useTableForeignKeyNavigation';
import type { FkGroupAffordance } from '../../../lib/fkNavigation';

afterEach(cleanup);

vi.mock('../../../lib/schemaCache', () => ({
  getCachedTableSchema: async () => ({
    tableName: 'orders',
    columns: [],
    primaryKeys: [],
    indexes: [],
    foreignKeys: [
      {
        name: 'orders_customer_fk',
        columns: ['customer_id'],
        referencedTable: 'customers',
        referencedColumns: ['id'],
      },
    ],
  }),
}));

vi.mock('../../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 28 })),
    totalHeight: rows.length * 28,
  }),
}));

// ---------------------------------------------------------------------------
// 1. The type makes a half-filled object impossible to write.
// ---------------------------------------------------------------------------

/**
 * The pin: `rows` and `visibleColumns` are **required** inside the object.
 *
 * If they ever became optional, this directive would be unused and `tsc`
 * would report TS2578 — the typecheck gate would catch the loosening.
 */
// @ts-expect-error — `rows` is required: without the full records a hidden
// member looks like a member without a value and the arrow is lost.
export const MISSING_ROWS: DataTableProps['foreignKeys'] = {
  index: new Map<string, FkGroupAffordance>(),
  visibleColumns: [],
};

/**
 * The same for `visibleColumns`: the arrow needs to know the **display
 * order** to land on the first visible member.
 */
// @ts-expect-error — `visibleColumns` is required: the arrow lands on the
// group's first **visible** member, in display order.
export const MISSING_VISIBLE_COLUMNS: DataTableProps['foreignKeys'] = {
  index: new Map<string, FkGroupAffordance>(),
  rows: [],
};

/** `index` is required as well. */
// @ts-expect-error — `index` is required: without it there is no group to ask.
export const MISSING_INDEX: DataTableProps['foreignKeys'] = {
  rows: [],
  visibleColumns: [],
};

/** The whole object is optional — a table without foreign keys passes none. */
export const NO_FOREIGN_KEYS: DataTableProps['foreignKeys'] = undefined;

// ---------------------------------------------------------------------------
// 2. The object is reference-stable.
// ---------------------------------------------------------------------------

/**
 * The inputs, hoisted so their identity is stable — exactly as `TableView`
 * does it: `rows` is the store's array and `displayedColumns` is a
 * `useMemo`. An inline literal would change identity on every render and
 * would (correctly) produce a new object.
 */
const PROBE_ROWS = [{ id: 1, customer_id: 7 }];
const PROBE_COLUMNS = [{ name: 'id' }, { name: 'customer_id' }];

function Probe({ onForeignKeys }: { onForeignKeys: (value: unknown) => void }) {
  const navigation = useTableForeignKeyNavigation({
    panelId: 'panel-orders',
    dbSessionId: 'session-1',
    database: 'app',
    tableName: 'orders',
    schema: null,
    connectionId: 'conn-1',
    databaseType: 'postgresql',
    rows: PROBE_ROWS,
    displayedColumns: PROBE_COLUMNS,
    rowHeight: 28,
    activeCell: null,
  });
  const [tick, setTick] = useState(0);
  useEffect(() => {
    onForeignKeys(navigation.foreignKeys);
  });
  return (
    <button type="button" data-testid="rerender" onClick={() => setTick(tick + 1)}>
      {tick}
    </button>
  );
}

describe('audit: the foreignKeys object', () => {
  it('keeps its identity across re-renders', async () => {
    const seen: unknown[] = [];
    const view = render(<Probe onForeignKeys={(value) => seen.push(value)} />);
    // Let the metadata arrive and the effect run.
    await new Promise((resolve) => setTimeout(resolve, 0));
    const before = seen[seen.length - 1];
    expect(before).toBeDefined();

    // A re-render of the owner must not produce a new object.
    view.container.querySelector('[data-testid="rerender"]')?.dispatchEvent(
      new MouseEvent('click', { bubbles: true }),
    );
    await new Promise((resolve) => setTimeout(resolve, 0));
    const after = seen[seen.length - 1];

    expect(after).toBe(before);
  });
});

describe('audit: the grid takes the affordances as one object', () => {
  it('renders the arrow when the object carries the full records', () => {
    const index = new Map<string, FkGroupAffordance>([
      [
        'customer_id',
        {
          localColumns: ['customer_id'],
          targetColumns: ['id'],
          targetLabel: 'customers.id',
          constraintName: 'orders_customer_fk',
          referenceTable: 'customers',
        },
      ],
    ]);
    const { container } = render(
      <DataTable
        columns={[{ id: 'customer_id', name: 'customer_id', type: 'integer' }]}
        rows={[[7]]}
        foreignKeys={{
          index,
          rows: [{ customer_id: 7 }],
          visibleColumns: ['customer_id'],
        }}
      />,
    );
    expect(container.querySelectorAll('[data-dt-fk-click="true"]')).toHaveLength(1);
    expect(
      container.querySelector('[data-dt-row="0"][data-dt-col="customer_id"]')?.getAttribute('data-dt-fk'),
    ).toBe('customers.id');
  });
});
