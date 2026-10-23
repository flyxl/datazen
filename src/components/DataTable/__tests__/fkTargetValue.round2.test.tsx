/**
 * Independent verification of the target identifier of a foreign key
 * (Wave 5 · track A, round 2 — the FKU-D3 follow-up).
 *
 * `data-dt-fk` carries the **reference target identifier** (00-contracts §4.4,
 * as corrected by `f31ea2315`). The identifier is built from the driver's own
 * metadata, and **the driver decides its shape**:
 *
 * - PostgreSQL reports `ForeignKeyInfo.referenced_table` as
 *   `"{schema}.{table}"` — `packages/drivers/postgres/src/schema.rs:308`
 *   (`qualified_pg_table_identity`), fed by the catalog's `ref_ns.nspname`
 *   (`packages/drivers/postgres/src/schema_table.rs:234-237`). A table in the
 *   `public` schema therefore reports `public.parents`;
 * - MySQL and SQLite report the **bare** table name
 *   (`packages/drivers/mysql/src/mysql/ddl_parser.rs:27`,
 *   `packages/drivers/sqlite/src/sqlite/schema.rs:361`).
 *
 * The front end never splits the name (`src/lib/fkNavigation.ts`
 * `targetLabelOf`), so the attribute is:
 *
 * - PostgreSQL:      `public.parents.cust_no`
 * - MySQL / SQLite:  `parents.cust_no`
 *
 * This file pins that shape per driver and shows why an assertion about the
 * **absence** of a schema prefix cannot be written: the prefix is a fact
 * about the driver the grid happens to be talking to, not a fact about the
 * foreign key.
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
  useColumnResize: () => ({ columnWidths: [160, 160], onResizeStart: vi.fn() }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 28 })),
    totalHeight: rows.length * 28,
  }),
}));

/**
 * The foreign key the E2E fixture creates:
 *
 *   customer_id INT REFERENCES _e2e_fk_parent(cust_no)
 *
 * The **only** difference between the two drivers is how `referenced_table`
 * is spelled.
 */
function foreignKeyOf(driver: 'postgres' | 'mysql'): ForeignKeyInfo {
  return {
    name: '_e2e_fk_child_customer_id_fkey',
    columns: ['customer_id'],
    referencedTable:
      driver === 'postgres' ? 'public._e2e_fk_parent' : '_e2e_fk_parent',
    referencedColumns: ['cust_no'],
  };
}

/** Render the grid and read the marker of the cell. */
function markerOf(driver: 'postgres' | 'mysql'): string | null {
  const fkIndex = indexForeignKeys([foreignKeyOf(driver)]);
  const { container } = render(
    <DataTable
      columns={[{ id: 'customer_id', name: 'customer_id', type: 'integer' }]}
      rows={[[7]]}
      foreignKeys={{
        index: fkIndex,
        rows: [{ customer_id: 7 }],
        visibleColumns: ['customer_id'],
      }}
    />,
  );
  const cell = container.querySelector('[data-dt-row="0"][data-dt-col="customer_id"]');
  return cell?.getAttribute('data-dt-fk') ?? null;
}

describe('audit: the target identifier is driver-shaped', () => {
  it('is the schema-qualified name on PostgreSQL', () => {
    // PostgreSQL's catalog query reports the referenced table's **namespace**,
    // so `public` is part of the name.
    expect(markerOf('postgres')).toBe('public._e2e_fk_parent.cust_no');
  });

  it('is the bare name on MySQL and SQLite', () => {
    expect(markerOf('mysql')).toBe('_e2e_fk_parent.cust_no');
  });

  it('cannot be asserted to be free of a schema prefix', () => {
    // The E2E runs on PostgreSQL (`connectSeededPgInWorkspace`), so the
    // attribute **does** contain "public" there. An assertion of the shape
    // `expect(marked).not.toContain('public')` is therefore false on the very
    // driver the E2E uses.
    const pg = markerOf('postgres');
    expect(pg).not.toBeNull();
    expect(pg).toContain('public');
    // So the E2E-style assertion `expect(marked).not.toContain('public')`
    // evaluates to **false** here — it would fail on the driver the E2E runs
    // on.
    expect(pg?.includes('public') === false).toBe(false);
    expect(pg?.includes('public')).toBe(true);
  });
});

describe('audit: the assertion shape that holds on every driver', () => {
  it('pins the table and the referenced column, and nothing driver-specific', () => {
    for (const driver of ['postgres', 'mysql'] as const) {
      const marked = markerOf(driver);
      // The two parts every driver spells the same way.
      expect(marked).toContain('_e2e_fk_parent');
      expect(marked).toContain('cust_no');
      // The **suffix** is the target identifier itself: it holds whether or
      // not the driver prefixes the schema.
      expect(marked?.endsWith('_e2e_fk_parent.cust_no')).toBe(true);
      // And the schema prefix is never asserted in either direction.
    }
  });
});
