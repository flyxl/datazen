/**
 * Independent verification of the hidden-member rule (Wave 5 · track A,
 * round 2 — the FKU-D1 follow-up).
 *
 * A composite foreign key is located as a **whole**: one member without a
 * value makes the group unlocatable. But a member that is merely **hidden**
 * still has a value, and hiding a column must **move** the arrow, not lose
 * it (07 §4.1, §7 #14).
 *
 * The distinction the grid has to make:
 *
 * - a member that is hidden **and has a value** ⇒ the group is complete ⇒ the
 *   arrow lands on the next **visible** member;
 * - a member whose value is **NULL** ⇒ the group is incomplete ⇒ no arrow;
 * - a member that is hidden **and the full record does not carry it** ⇒ the
 *   grid does not guess: it treats it as missing ⇒ no arrow;
 * - the **whole group** hidden ⇒ no arrow, and no error.
 *
 * The fixture is the production shape: the grid receives only the displayed
 * columns and the positional rows over exactly those columns, while the full
 * records arrive with the foreign keys (`foreignKeys.rows`).
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

const COMPOSITE: ForeignKeyInfo = {
  name: 'orders_parent_fk',
  columns: ['parent_a', 'parent_b'],
  referencedTable: 'parents',
  referencedColumns: ['x', 'y'],
};

interface GridOptions {
  /** The columns the grid renders (the visible ones). */
  displayed: string[];
  /** The positional rows, over the displayed columns only. */
  rowValues: (number | null)[];
  /** The full row records the grid gets through `foreignKeys.rows`. */
  fullRecord?: Record<string, unknown> | null;
  /**
   * Hand down **no** foreign keys at all — a grid that does not know about
   * them.
   *
   * The props are all-or-nothing on purpose: a grid that got the index but
   * not the records could not tell a hidden member from one without a value,
   * so the type makes that combination impossible to write (FKU-D1).
   */
  withoutForeignKeys?: boolean;
}

function renderGrid(options: GridOptions) {
  const fkIndex = indexForeignKeys([COMPOSITE]);
  const { container } = render(
    <DataTable
      columns={options.displayed.map((name) => ({ id: name, name, type: 'integer' }))}
      rows={[options.rowValues]}
      {...(options.withoutForeignKeys
        ? {}
        : {
            foreignKeys: {
              index: fkIndex,
              rows: [options.fullRecord ?? {}],
              visibleColumns: options.displayed,
            },
          })}
    />,
  );
  return container;
}

function arrows(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll('[data-dt-fk-click="true"]'));
}

function arrowOf(
  container: HTMLElement,
  rowIndex: number,
  columnName: string,
): HTMLElement | null {
  const cell = container.querySelector(`[data-dt-row="${rowIndex}"][data-dt-col="${columnName}"]`);
  if (!(cell instanceof HTMLElement)) return null;
  const arrow = cell.querySelector('[data-dt-fk-click="true"]');
  return arrow instanceof HTMLElement ? arrow : null;
}

describe('audit: a hidden member with a value moves the arrow', () => {
  it('lands the arrow on the next visible member', () => {
    // "parent_a" is hidden, "parent_b" is visible, and **both have a value**.
    const container = renderGrid({
      displayed: ['id', 'parent_b'],
      rowValues: [1, 4],
      fullRecord: { id: 1, parent_a: 3, parent_b: 4 },
    });

    expect(arrows(container)).toHaveLength(1);
    expect(arrowOf(container, 0, 'parent_b')).not.toBeNull();
    expect(arrowOf(container, 0, 'parent_a')).toBeNull();
  });

  it('still refuses when another member of the group is NULL', () => {
    // "parent_a" is hidden but has a value; "parent_b" is visible and NULL.
    const container = renderGrid({
      displayed: ['id', 'parent_b'],
      rowValues: [1, null],
      fullRecord: { id: 1, parent_a: 3, parent_b: null },
    });

    expect(arrows(container)).toHaveLength(0);
  });
});

describe('audit: a member without a value makes the whole group unlocatable', () => {
  it('draws no arrow when a visible member is NULL', () => {
    const container = renderGrid({
      displayed: ['id', 'parent_a', 'parent_b'],
      rowValues: [1, 3, null],
      fullRecord: { id: 1, parent_a: 3, parent_b: null },
    });

    expect(arrows(container)).toHaveLength(0);
    // The member cells are still marked: they belong to the key.
    const cell = container.querySelector('[data-dt-row="0"][data-dt-col="parent_a"]');
    expect(cell?.getAttribute('data-dt-fk')).toBe('parents.(x, y)');
  });

  it('draws no arrow when a hidden member is absent from the full record', () => {
    // The grid does not guess: a member it cannot read is a member without a
    // value.
    const container = renderGrid({
      displayed: ['id', 'parent_b'],
      rowValues: [1, 4],
      fullRecord: { id: 1, parent_b: 4 },
    });

    expect(arrows(container)).toHaveLength(0);
  });
});

describe('audit: a wholly hidden group', () => {
  it('draws no arrow, marks nothing and does not fail', () => {
    const container = renderGrid({
      displayed: ['id'],
      rowValues: [1],
      fullRecord: { id: 1, parent_a: 3, parent_b: 4 },
    });

    expect(arrows(container)).toHaveLength(0);
    // The member cells are **not on screen**, so there is nothing to mark.
    expect(container.querySelector('[data-dt-fk]')).toBeNull();
    // The grid is still there.
    expect(container.querySelectorAll('[data-testid="data-table-cell"]').length).toBeGreaterThan(0);
  });
});

describe('audit: the grid that does not hand down the full records', () => {
  it('cannot tell a hidden member from a missing one', () => {
    // Without the foreign keys the grid only sees the displayed columns, so
    // a hidden member is indistinguishable from one without a value: no
    // arrow. That is the documented fallback ("the grid does not guess") —
    // and it is why the owner of the grid has to hand the full records down.
    const container = renderGrid({
      displayed: ['id', 'parent_b'],
      rowValues: [1, 4],
      withoutForeignKeys: true,
    });

    expect(arrows(container)).toHaveLength(0);
  });
});
