/**
 * Independent verification of the foreign-key DOM contract (Wave 5 · track B).
 *
 * The contract (00-contracts §4.4, as corrected by `f31ea2315`):
 *
 * - `data-dt-fk` carries the **reference target identifier** — `customers.id`
 *   for a single-column key, `sales.parents.(x, y)` for a composite one — and
 *   it is on the **cell**;
 * - `data-dt-fk-click` is on the **arrow button**, and a cell that carries it
 *   always carries `data-dt-fk` too;
 * - a group with a missing member still marks its member cells, but draws no
 *   arrow at all (07 §4.1 `hiddenNull`);
 * - the arrow lands on the group's **first visible member in display order**,
 *   never in dictionary order and never in the original column order; hiding
 *   the first member moves the arrow, hiding the whole group removes it;
 * - an editor owns the cell (no arrow), and a resolution in flight disables
 *   the arrow.
 *
 * The wiring here is the **production** one: `TableView` filters the columns
 * before it hands them to the grid, so the grid sees only the *displayed*
 * columns and the positional row arrays are aligned to them. A test that hands
 * the grid every column would not exercise the path a user walks.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/react';
import { DataTable } from '../DataTable';
import { indexForeignKeys } from '../../../lib/fkNavigation';
import type { ForeignKeyInfo } from '../../../types';

afterEach(cleanup);

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({ columnWidths: [160, 160, 160, 160], onResizeStart: vi.fn() }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 28 })),
    totalHeight: rows.length * 28,
  }),
}));

const SINGLE: ForeignKeyInfo = {
  name: 'orders_customer_fk',
  columns: ['customer_id'],
  referencedTable: 'customers',
  referencedColumns: ['id'],
};

const COMPOSITE: ForeignKeyInfo = {
  name: 'orders_parent_fk',
  columns: ['parent_a', 'parent_b'],
  referencedTable: 'sales.parents',
  referencedColumns: ['x', 'y'],
};

interface Column {
  name: string;
}

interface GridOptions {
  /** The columns of the table, in the order the table declares them. */
  tableColumns: Column[];
  /** The columns the user sees, in the order the grid renders them. */
  displayedColumns: Column[];
  /** The rows, by column name. */
  rows: Record<string, unknown>[];
  foreignKeys: ForeignKeyInfo[];
  editingCell?: { row: number; col: string } | null;
  resolving?: ReadonlySet<string>;
}

/**
 * Render the grid the way `TableView` renders it.
 *
 * The two shapes that matter: the grid receives **only** the displayed
 * columns, and the row arrays are positional over exactly those columns.
 */
function renderGrid(options: GridOptions) {
  const fkIndex = indexForeignKeys(options.foreignKeys);
  const displayed = options.displayedColumns;
  const rowArrays = options.rows.map((row) =>
    displayed.map((column) => row[column.name] ?? null),
  );
  const { container } = render(
    <DataTable
      columns={displayed.map((column) => ({ id: column.name, name: column.name, type: 'integer' }))}
      rows={rowArrays}
      fkIndex={fkIndex}
      fkVisibleColumns={displayed.map((column) => column.name)}
      fkRows={options.rows}
      fkResolving={options.resolving}
      editingCell={options.editingCell ?? null}
    />,
  );
  return container;
}

function cellOf(container: HTMLElement, rowIndex: number, columnName: string): HTMLElement | null {
  const cell = container.querySelector(`[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`);
  return cell instanceof HTMLElement ? cell : null;
}

function arrows(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll('[data-dt-fk-click="true"]'));
}

function arrowOf(
  container: HTMLElement,
  rowIndex: number,
  columnName: string,
): HTMLElement | null {
  const cell = cellOf(container, rowIndex, columnName);
  if (!cell) return null;
  const arrow = cell.querySelector('[data-dt-fk-click="true"]');
  return arrow instanceof HTMLElement ? arrow : null;
}

describe('audit: data-dt-fk carries the reference target', () => {
  it('names the target of a single-column key', () => {
    const container = renderGrid({
      tableColumns: [{ name: 'id' }, { name: 'customer_id' }],
      displayedColumns: [{ name: 'id' }, { name: 'customer_id' }],
      rows: [{ id: 1, customer_id: 7 }],
      foreignKeys: [SINGLE],
    });

    expect(cellOf(container, 0, 'customer_id')?.getAttribute('data-dt-fk')).toBe('customers.id');
    // A column that is not a member carries nothing.
    expect(cellOf(container, 0, 'id')?.hasAttribute('data-dt-fk')).toBe(false);
  });

  it('names the table and the columns of a composite key', () => {
    const container = renderGrid({
      tableColumns: [{ name: 'id' }, { name: 'parent_a' }, { name: 'parent_b' }],
      displayedColumns: [{ name: 'id' }, { name: 'parent_a' }, { name: 'parent_b' }],
      rows: [{ id: 1, parent_a: 3, parent_b: 4 }],
      foreignKeys: [COMPOSITE],
    });

    expect(cellOf(container, 0, 'parent_a')?.getAttribute('data-dt-fk')).toBe(
      'sales.parents.(x, y)',
    );
    expect(cellOf(container, 0, 'parent_b')?.getAttribute('data-dt-fk')).toBe(
      'sales.parents.(x, y)',
    );
  });
});

describe('audit: an incomplete group marks but does not draw', () => {
  it('keeps the marker on the member cells and draws no arrow', () => {
    const container = renderGrid({
      tableColumns: [{ name: 'id' }, { name: 'parent_a' }, { name: 'parent_b' }],
      displayedColumns: [{ name: 'id' }, { name: 'parent_a' }, { name: 'parent_b' }],
      rows: [{ id: 1, parent_a: 3, parent_b: null }],
      foreignKeys: [COMPOSITE],
    });

    // The marker is on the member cells …
    expect(cellOf(container, 0, 'parent_a')?.getAttribute('data-dt-fk')).toBe(
      'sales.parents.(x, y)',
    );
    expect(cellOf(container, 0, 'parent_b')?.getAttribute('data-dt-fk')).toBe(
      'sales.parents.(x, y)',
    );
    // … and there is no arrow at all.
    expect(arrows(container)).toHaveLength(0);
  });
});

describe('audit: the arrow lands on the first visible member, in display order', () => {
  it('follows the display order, not the dictionary order and not the table order', () => {
    // The table declares (id, parent_a, parent_b); the user sees them in the
    // order (parent_b, parent_a, id) — the dictionary order would be
    // (parent_a, parent_b) and the table order is (parent_a, parent_b) too.
    const container = renderGrid({
      tableColumns: [{ name: 'id' }, { name: 'parent_a' }, { name: 'parent_b' }],
      displayedColumns: [{ name: 'parent_b' }, { name: 'parent_a' }, { name: 'id' }],
      rows: [{ id: 1, parent_a: 3, parent_b: 4 }],
      foreignKeys: [COMPOSITE],
    });

    expect(arrows(container)).toHaveLength(1);
    expect(arrowOf(container, 0, 'parent_b')).not.toBeNull();
    expect(arrowOf(container, 0, 'parent_a')).toBeNull();
  });

  /**
   * FKU-D1 (fixed).
   *
   * 07 §4.1 and §7 #14: hiding the first member of a composite key moves the
   * arrow to the next visible member — it must not make the group
   * unlocatable. The production wiring used to break that: `VirtualBody` built
   * the row record from the **displayed** columns only, so a hidden member
   * read as `undefined`, and `isFkCellNavigable` treated that as "the group is
   * incomplete" — the arrow disappeared.
   *
   * The fix: the grid reads the members' values from the **full** row records
   * (`fkRows`), so a hidden member keeps its value and the group stays
   * locatable. The pin used to be `it.fails`; it is a plain `it` now, and it
   * turns red the moment the behaviour breaks again.
   */
  it('moves to the next visible member when the first one is hidden', () => {
    // "parent_a" is hidden by the column filter: the grid never sees that
    // column, so the arrow has to move to "parent_b".
    const container = renderGrid({
      tableColumns: [{ name: 'id' }, { name: 'parent_a' }, { name: 'parent_b' }],
      displayedColumns: [{ name: 'id' }, { name: 'parent_b' }],
      rows: [{ id: 1, parent_a: 3, parent_b: 4 }],
      foreignKeys: [COMPOSITE],
    });

    expect(arrows(container)).toHaveLength(1);
    expect(arrowOf(container, 0, 'parent_b')).not.toBeNull();
    expect(arrowOf(container, 0, 'parent_a')).toBeNull();
  });

  it('draws nothing when the whole group is hidden, and does not fail', () => {
    const container = renderGrid({
      tableColumns: [{ name: 'id' }, { name: 'parent_a' }, { name: 'parent_b' }],
      displayedColumns: [{ name: 'id' }],
      rows: [{ id: 1, parent_a: 3, parent_b: 4 }],
      foreignKeys: [COMPOSITE],
    });

    expect(arrows(container)).toHaveLength(0);
    // The grid itself is still there — hiding a group is not an error.
    expect(container.querySelectorAll('[data-testid="data-table-cell"]').length).toBeGreaterThan(0);
  });
});

describe('audit: the arrow yields to the editor and to an in-flight resolution', () => {
  it('draws no arrow while the cell is edited', () => {
    const container = renderGrid({
      tableColumns: [{ name: 'id' }, { name: 'customer_id' }],
      displayedColumns: [{ name: 'id' }, { name: 'customer_id' }],
      rows: [{ id: 1, customer_id: 7 }],
      foreignKeys: [SINGLE],
      editingCell: { row: 0, col: 'customer_id' },
    });

    expect(arrowOf(container, 0, 'customer_id')).toBeNull();
  });

  it('disables the arrow while the resolution is in flight', () => {
    const container = renderGrid({
      tableColumns: [{ name: 'id' }, { name: 'customer_id' }],
      displayedColumns: [{ name: 'id' }, { name: 'customer_id' }],
      rows: [{ id: 1, customer_id: 7 }],
      foreignKeys: [SINGLE],
      resolving: new Set(['0:customer_id']),
    });

    const arrow = arrowOf(container, 0, 'customer_id');
    expect(arrow).not.toBeNull();
    expect((arrow as HTMLButtonElement).disabled).toBe(true);
  });
});

describe('audit: the marker of the click is the button, and its cell is marked', () => {
  it('puts data-dt-fk-click on a button inside a marked cell', () => {
    const container = renderGrid({
      tableColumns: [{ name: 'id' }, { name: 'customer_id' }],
      displayedColumns: [{ name: 'id' }, { name: 'customer_id' }],
      rows: [{ id: 1, customer_id: 7 }],
      foreignKeys: [SINGLE],
    });

    const cell = cellOf(container, 0, 'customer_id');
    expect(cell).not.toBeNull();
    // The cell carries the target …
    expect(cell?.getAttribute('data-dt-fk')).toBe('customers.id');
    // … and the **button** carries the click marker.
    const button = cell?.querySelector('[data-dt-fk-click="true"]');
    expect(button).not.toBeNull();
    expect(button?.tagName).toBe('BUTTON');
    expect(button?.getAttribute('type')).toBe('button');
    // The click marker is nowhere else in the grid.
    expect(container.querySelectorAll('[data-dt-fk-click]').length).toBe(1);
    // No cell carries the click marker itself.
    expect(container.querySelector('[data-dt-fk-click]')?.closest('[data-dt-row]')?.getAttribute('data-dt-col')).toBe(
      'customer_id',
    );
  });
});

describe('audit: the marker the E2E fixture would produce', () => {
  // The fixture of `e2e/specs/table-fk-navigation.ts`:
  //   customer_id INT REFERENCES _e2e_fk_parent(cust_no)
  //   FOREIGN KEY (parent_px, parent_py) REFERENCES _e2e_fk_comp_parent(px, py)
  it('names the table and the referenced column, not the column alone', () => {
    const single = indexForeignKeys([
      {
        name: '_e2e_fk_child_customer_id_fkey',
        columns: ['customer_id'],
        referencedTable: '_e2e_fk_parent',
        referencedColumns: ['cust_no'],
      },
    ]);
    expect(single.get('customer_id')?.targetLabel).toBe('_e2e_fk_parent.cust_no');

    const composite = indexForeignKeys([
      {
        name: '_e2e_fk_child_comp_fk',
        columns: ['parent_px', 'parent_py'],
        referencedTable: '_e2e_fk_comp_parent',
        referencedColumns: ['px', 'py'],
      },
    ]);
    expect(composite.get('parent_px')?.targetLabel).toBe('_e2e_fk_comp_parent.(px, py)');
  });
});
