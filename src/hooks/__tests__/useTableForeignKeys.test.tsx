/**
 * The foreign-key metadata of a table (DB-07).
 *
 * The grid asks the schema cache for the constraints of the table it shows.
 * The failure mode is the interesting one: metadata that cannot be read must
 * **degrade** the grid (no arrows), never break it.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { useTableForeignKeys } from '../useTableForeignKeys';

const mockGetCachedTableSchema = vi.fn();

vi.mock('../../lib/schemaCache', () => ({
  getCachedTableSchema: (...args: unknown[]) => mockGetCachedTableSchema(...args),
}));

const PARAMS = {
  dbSessionId: 'session-1',
  tableName: 'orders',
  database: 'app',
  schema: null,
};

describe('useTableForeignKeys', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('indexes the foreign keys of the table by column', async () => {
    mockGetCachedTableSchema.mockResolvedValue({
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
        {
          name: 'orders_parent_fk',
          columns: ['parent_a', 'parent_b'],
          referencedTable: 'parents',
          referencedColumns: ['x', 'y'],
        },
      ],
    });

    const { result } = renderHook(() => useTableForeignKeys(PARAMS));

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.error).toBeNull();
    expect(result.current.index.size).toBe(3);
    // Both members of the composite key point at the same group.
    expect(result.current.index.get('parent_a')).toBe(result.current.index.get('parent_b'));
    expect(result.current.index.get('customer_id')?.targetLabel).toBe('customers.id');
  });

  it('degrades silently when the metadata cannot be read', async () => {
    mockGetCachedTableSchema.mockRejectedValue(new Error('no permission for table orders'));

    const { result } = renderHook(() => useTableForeignKeys(PARAMS));

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.index.size).toBe(0);
    expect(result.current.error).toContain('no permission');
  });

  it('has no index before the metadata arrives', () => {
    mockGetCachedTableSchema.mockReturnValue(new Promise(() => {}));

    const { result } = renderHook(() => useTableForeignKeys(PARAMS));

    expect(result.current.loading).toBe(true);
    expect(result.current.index.size).toBe(0);
  });

  it('reads the metadata of the table it is given', async () => {
    mockGetCachedTableSchema.mockResolvedValue({
      tableName: 'orders',
      columns: [],
      primaryKeys: [],
      indexes: [],
      foreignKeys: [],
    });

    renderHook(() => useTableForeignKeys(PARAMS));

    await waitFor(() => expect(mockGetCachedTableSchema).toHaveBeenCalled());
    expect(mockGetCachedTableSchema).toHaveBeenCalledWith(
      'session-1',
      'orders',
      'app',
      null,
      undefined,
    );
  });
});
