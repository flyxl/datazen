/**
 * Independent verification of the target identifier and of the E2E
 * assertion's shape (Wave 5 · track A, round 3 — the FKU2-D1 follow-up).
 *
 * The E2E asserts the marker of a foreign-key cell with
 *
 * ```ts
 * expect(marked?.endsWith('_e2e_fk_parent.cust_no')).toBe(true);
 * ```
 *
 * This file checks that the assertion **holds for every driver shape**:
 *
 * - PostgreSQL reports the referenced table with its schema
 *   (`packages/drivers/postgres/src/schema.rs:308`,
 *   `qualified_pg_table_identity`), so the marker is
 *   `public._e2e_fk_parent.cust_no`;
 * - MySQL and SQLite report the bare name, so the marker is
 *   `_e2e_fk_parent.cust_no`.
 *
 * The assertion pins the **shape** — the referenced table and the referenced
 * column at the end — and leaves the schema prefix open in both directions.
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

/** The foreign key the E2E fixture creates, in the two driver shapes. */
function singleKey(driver: 'postgres' | 'mysql'): ForeignKeyInfo {
  return {
    name: '_e2e_fk_child_customer_id_fkey',
    columns: ['customer_id'],
    referencedTable:
      driver === 'postgres' ? 'public._e2e_fk_parent' : '_e2e_fk_parent',
    referencedColumns: ['cust_no'],
  };
}

function compositeKey(driver: 'postgres' | 'mysql'): ForeignKeyInfo {
  return {
    name: '_e2e_fk_child_comp_fk',
    columns: ['parent_px', 'parent_py'],
    referencedTable:
      driver === 'postgres' ? 'public._e2e_fk_comp_parent' : '_e2e_fk_comp_parent',
    referencedColumns: ['px', 'py'],
  };
}

function markerOf(
  foreignKey: ForeignKeyInfo,
  columnName: string,
  columns: string[],
  rowValues: (number | null)[],
  fullRecord: Record<string, unknown>,
): string | null {
  const fkIndex = indexForeignKeys([foreignKey]);
  const { container } = render(
    <DataTable
      columns={columns.map((name) => ({ id: name, name, type: 'integer' }))}
      rows={[rowValues]}
      foreignKeys={{
        index: fkIndex,
        rows: [fullRecord],
        visibleColumns: columns,
      }}
    />,
  );
  const cell = container.querySelector(`[data-dt-row="0"][data-dt-col="${columnName}"]`);
  return cell?.getAttribute('data-dt-fk') ?? null;
}

describe('audit: the E2E assertion shape holds on every driver', () => {
  it('holds for the single-column key on PostgreSQL', () => {
    const marked = markerOf(
      singleKey('postgres'),
      'customer_id',
      ['customer_id'],
      [7],
      { customer_id: 7 },
    );
    // The exact value PostgreSQL produces.
    expect(marked).toBe('public._e2e_fk_parent.cust_no');
    // …and the assertion the E2E uses.
    expect(marked?.endsWith('_e2e_fk_parent.cust_no')).toBe(true);
  });

  it('holds for the single-column key on MySQL and SQLite', () => {
    const marked = markerOf(
      singleKey('mysql'),
      'customer_id',
      ['customer_id'],
      [7],
      { customer_id: 7 },
    );
    expect(marked).toBe('_e2e_fk_parent.cust_no');
    expect(marked?.endsWith('_e2e_fk_parent.cust_no')).toBe(true);
  });

  it('holds for the composite key on both drivers', () => {
    for (const driver of ['postgres', 'mysql'] as const) {
      const marked = markerOf(
        compositeKey(driver),
        'parent_px',
        ['parent_px', 'parent_py'],
        [11, 22],
        { parent_px: 11, parent_py: 22 },
      );
      // The composite spelling is `table.(col, col)` — with a blank after the
      // comma (`targetLabelOf` in `src/lib/fkNavigation.ts`).
      expect(marked?.endsWith('_e2e_fk_comp_parent.(px, py)')).toBe(true);
      if (driver === 'postgres') {
        expect(marked).toBe('public._e2e_fk_comp_parent.(px, py)');
      } else {
        expect(marked).toBe('_e2e_fk_comp_parent.(px, py)');
      }
    }
  });

  it('never asserts the schema prefix in either direction', () => {
    // The prefix is a fact about the driver, so neither its presence nor its
    // absence may be asserted: the only thing both drivers agree on is the
    // tail.
    for (const driver of ['postgres', 'mysql'] as const) {
      const marked = markerOf(
        singleKey(driver),
        'customer_id',
        ['customer_id'],
        [7],
        { customer_id: 7 },
      );
      expect(marked).toContain('_e2e_fk_parent');
      expect(marked).toContain('cust_no');
      // The prefix is not asserted in either direction — the two shapes are
      // both accepted.
      expect(
        marked === 'public._e2e_fk_parent.cust_no' ||
          marked === '_e2e_fk_parent.cust_no',
      ).toBe(true);
    }
  });
});
