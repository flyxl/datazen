import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent } from '@testing-library/react';
import { SchemaTree } from '../SchemaTree';
import { useSchemaStore } from '../../../../stores/schemaStore';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../../plugins/generated', () => ({
  PLUGIN_DB_ENTRIES: {},
  PLUGIN_SQL_DIALECTS: {},
  getPluginSchemaTree: () => undefined,
  getPluginConnectionForm: () => undefined,
  getPluginValidator: () => undefined,
}));

/** jsdom has no layout; render all virtual rows so tree content is visible. */
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count, estimateSize }: {
    count: number;
    estimateSize?: (i: number) => number;
  }) => {
    const sizeOf = (i: number) => estimateSize?.(i) ?? 32;
    let offset = 0;
    const items = Array.from({ length: count }, (_, index) => {
      const size = sizeOf(index);
      const start = offset;
      offset += size;
      return { index, key: index, start, size, end: start + size };
    });
    return {
      getTotalSize: () => offset || count * 32,
      getVirtualItems: () => items,
    };
  },
}));

const mockGetDatabases = vi.fn();
const mockGetTables = vi.fn();
const mockUseDatabase = vi.fn();

vi.mock('../../../../commands/database', () => ({
  databaseCommands: {
    getDatabases: (...args: unknown[]) => mockGetDatabases(...args),
    getTables: (...args: unknown[]) => mockGetTables(...args),
    useDatabase: (...args: unknown[]) => mockUseDatabase(...args),
    getColumns: vi.fn().mockResolvedValue([]),
  },
}));

afterEach(() => {
  cleanup();
  useSchemaStore.getState().reset();
  vi.clearAllMocks();
});

beforeEach(() => {
  mockGetDatabases.mockResolvedValue(['db_a', 'db_b']);
  mockGetTables.mockResolvedValue([
    { name: 'users', tableType: 'TABLE', schema: null, rowCount: null },
  ]);
  mockUseDatabase.mockResolvedValue(undefined);
  useSchemaStore.getState().reset();
});

const baseProps = {
  connectionId: 'conn-1',
  selectedTable: null as string | null,
  searchQuery: '',
  onSelectTable: vi.fn(),
};

describe('SchemaTree routing', () => {
  it('routes mysql to MultiDatabaseSchemaTree when length > 1', async () => {
    mockGetDatabases.mockResolvedValueOnce(['datazen_test', 'mysql', 'information_schema']);

    const { findByText, queryByText } = render(
      <SchemaTree {...baseProps} databaseType="mysql" />,
    );

    expect(await findByText('datazen_test')).toBeInTheDocument();
    expect(await findByText('mysql')).toBeInTheDocument();
    expect(await findByText('information_schema')).toBeInTheDocument();
    // Multi-db tree has no "Tables" section header
    expect(queryByText(/schemaTree\.tables/)).not.toBeInTheDocument();
    await waitFor(() => {
      expect(useSchemaStore.getState().isMultiDatabase).toBe(true);
    });
  });

  it('routes mysql to MultiDatabaseSchemaTree when length === 1', async () => {
    mockGetDatabases.mockResolvedValueOnce(['only_db']);

    const { findByText, queryByText } = render(
      <SchemaTree {...baseProps} databaseType="mysql" />,
    );

    expect(await findByText('only_db')).toBeInTheDocument();
    expect(queryByText(/schemaTree\.tables/)).not.toBeInTheDocument();
    await waitFor(() => {
      expect(useSchemaStore.getState().isMultiDatabase).toBe(false);
    });
  });

  it('routes postgresql to StandardSchemaTree even with multiple databases', async () => {
    mockGetDatabases.mockResolvedValueOnce(['db1', 'db2']);
    mockGetTables.mockResolvedValueOnce([
      { name: 'orders', tableType: 'TABLE', schema: 'public', rowCount: null },
    ]);

    const { findByText, queryByText } = render(
      <SchemaTree {...baseProps} databaseType="postgresql" />,
    );

    expect(await findByText(/schemaTree\.tables/)).toBeInTheDocument();
    expect(await findByText('orders')).toBeInTheDocument();
    // Sibling DBs are not listed as expandable multi-db nodes
    expect(queryByText('db2')).not.toBeInTheDocument();
    await waitFor(() => {
      expect(useSchemaStore.getState().isMultiDatabase).toBe(false);
      expect(useSchemaStore.getState().databases).toEqual(['db1', 'db2']);
    });
  });

  it('expands a mysql database node and loads tables', async () => {
    mockGetDatabases.mockResolvedValueOnce(['alpha', 'beta']);
    mockGetTables.mockResolvedValueOnce([
      { name: 't1', tableType: 'TABLE', schema: null, rowCount: null },
      { name: 't2', tableType: 'TABLE', schema: null, rowCount: null },
    ]);

    const { findByText, getByText } = render(
      <SchemaTree {...baseProps} databaseType="mysql" />,
    );

    const dbBtn = await findByText('alpha');
    fireEvent.click(dbBtn.closest('button')!);

    await waitFor(() => {
      expect(mockUseDatabase).toHaveBeenCalledWith('conn-1', 'alpha');
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'alpha');
    });

    expect(await findByText('t1')).toBeInTheDocument();
    expect(getByText('t2')).toBeInTheDocument();
  });
});
