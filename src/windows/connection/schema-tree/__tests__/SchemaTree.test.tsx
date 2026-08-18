import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent } from '@testing-library/react';
import { SchemaTree } from '../SchemaTree';
import { useSchemaStore } from '../../../../stores/schemaStore';

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../../plugins/generated', () => {
  const sqlMulti = {
    label: 'SQL',
    shortLabel: 'SQL',
    iconBg: 'bg-blue-600',
    iconColor: 'text-blue-400',
    defaultPort: 5432,
    defaultHost: '127.0.0.1',
    defaultUser: '',
    quoteChar: '"',
    connectionMode: 'server',
    supportsSSH: true,
    supportsSSL: true,
    supportsBackup: true,
    supportsTables: true,
    isKeyValue: false,
    supportsSQL: true,
    category: 'sql',
    connectionView: 'sql',
    sqlDialect: 'postgresql',
    databaseFieldType: 'name',
    connectionForm: 'standard',
    supportsExplain: true,
    hasMultiDatabase: true,
  };
  const DRIVER_DB_ENTRIES = {
    postgresql: { ...sqlMulti, label: 'PostgreSQL', quoteChar: '"', sqlDialect: 'postgresql' },
    mysql: {
      ...sqlMulti,
      label: 'MySQL',
      quoteChar: '`',
      sqlDialect: 'mysql',
      defaultPort: 3306,
    },
  };
  return {
    DRIVER_DB_ENTRIES,
    PLUGIN_DB_ENTRIES: DRIVER_DB_ENTRIES,
    PLUGIN_SQL_DIALECTS: {},
    getPluginSchemaTree: () => undefined,
    getPluginConnectionForm: () => undefined,
    getPluginConnectionAdvanced: () => undefined,
    getPluginValidator: () => undefined,
    getPluginClipboardParsers: () => [],
  };
});

/** jsdom has no layout; render all virtual rows so tree content is visible. */
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({
    count,
    estimateSize,
  }: {
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
    getDatabaseObjects: vi.fn().mockResolvedValue([]),
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
    { name: 'users', tableType: 'table', schema: null, rowCount: null },
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
  it('shouldUseMultiDatabaseTree ignores domain field as logical lock', async () => {
    const { shouldUseMultiDatabaseTree } = await import('../SchemaTree');
    expect(
      shouldUseMultiDatabaseTree(
        { hasMultiDatabase: true, databaseFieldType: 'domain' },
        'afi-ph-useraccount-dbreader.aku',
      ),
    ).toBe(true);
    expect(
      shouldUseMultiDatabaseTree(
        { hasMultiDatabase: true, databaseFieldType: 'name' },
        'datazen_test',
      ),
    ).toBe(false);
    expect(
      shouldUseMultiDatabaseTree({ hasMultiDatabase: true, databaseFieldType: 'name' }, undefined),
    ).toBe(true);
    expect(
      shouldUseMultiDatabaseTree({ hasMultiDatabase: false, databaseFieldType: 'domain' }, 'x'),
    ).toBe(false);
  });

  it('routes mysql with initialDatabase to StandardSchemaTree (single DB)', async () => {
    mockGetDatabases.mockResolvedValueOnce(['datazen_test', 'mysql', 'information_schema']);

    const { findByText, queryByText } = render(
      <SchemaTree {...baseProps} databaseType="mysql" initialDatabase="datazen_test" />,
    );

    expect(await findByText('datazen_test')).toBeInTheDocument();
    expect(queryByText('information_schema')).not.toBeInTheDocument();
    expect(queryByText('mysql')).not.toBeInTheDocument();
    await waitFor(() => {
      expect(useSchemaStore.getState().isMultiDatabase).toBe(false);
      expect(useSchemaStore.getState().databases).toEqual(['datazen_test']);
      expect(mockGetTables).toHaveBeenCalled();
    });
    expect(await findByText(/schemaTree\.tables/)).toBeInTheDocument();
  });

  it('routes mysql without initialDatabase to MultiDatabaseSchemaTree when length > 1', async () => {
    mockGetDatabases.mockResolvedValueOnce(['datazen_test', 'mysql', 'information_schema']);

    const { findByText, queryByText } = render(<SchemaTree {...baseProps} databaseType="mysql" />);

    expect(await findByText('datazen_test')).toBeInTheDocument();
    expect(await findByText('mysql')).toBeInTheDocument();
    expect(await findByText('information_schema')).toBeInTheDocument();
    // Multi-db tree has no "Tables" section header
    expect(queryByText(/schemaTree\.tables/)).not.toBeInTheDocument();
    await waitFor(() => {
      expect(useSchemaStore.getState().isMultiDatabase).toBe(true);
    });
  });

  it('routes mysql without initialDatabase to MultiDatabaseSchemaTree when length === 1', async () => {
    mockGetDatabases.mockResolvedValueOnce(['only_db']);

    const { findByText, queryByText } = render(<SchemaTree {...baseProps} databaseType="mysql" />);

    expect(await findByText('only_db')).toBeInTheDocument();
    expect(queryByText(/schemaTree\.tables/)).not.toBeInTheDocument();
    await waitFor(() => {
      expect(useSchemaStore.getState().isMultiDatabase).toBe(false);
    });
  });

  it('routes postgresql without initialDatabase to MultiDatabaseSchemaTree when length > 1', async () => {
    mockGetDatabases.mockResolvedValueOnce(['db1', 'db2']);

    const { findByText, queryByText } = render(
      <SchemaTree {...baseProps} databaseType="postgresql" />,
    );

    expect(await findByText('db1')).toBeInTheDocument();
    expect(await findByText('db2')).toBeInTheDocument();
    expect(queryByText(/schemaTree\.tables/)).not.toBeInTheDocument();
    await waitFor(() => {
      expect(useSchemaStore.getState().isMultiDatabase).toBe(true);
      expect(useSchemaStore.getState().databases).toEqual(['db1', 'db2']);
    });
  });

  it('routes postgresql with initialDatabase to StandardSchemaTree', async () => {
    mockGetDatabases.mockResolvedValueOnce(['db1', 'db2']);

    const { findByText, queryByText } = render(
      <SchemaTree {...baseProps} databaseType="postgresql" initialDatabase="db2" />,
    );

    expect(await findByText('db2')).toBeInTheDocument();
    expect(queryByText('db1')).not.toBeInTheDocument();
    await waitFor(() => {
      expect(useSchemaStore.getState().isMultiDatabase).toBe(false);
      expect(useSchemaStore.getState().databases).toEqual(['db2']);
    });
    expect(await findByText(/schemaTree\.tables/)).toBeInTheDocument();
  });

  it('expands a postgresql database node and loads tables', async () => {
    mockGetDatabases.mockResolvedValueOnce(['db1', 'db2']);
    mockGetTables.mockResolvedValueOnce([
      { name: 'orders', tableType: 'table', schema: 'public', rowCount: null },
    ]);

    const { findByText } = render(<SchemaTree {...baseProps} databaseType="postgresql" />);

    const dbBtn = await findByText('db1');
    fireEvent.click(dbBtn.closest('button')!);

    await waitFor(() => {
      expect(mockUseDatabase).toHaveBeenCalledWith('conn-1', 'db1');
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'db1');
      expect(useSchemaStore.getState().currentDatabase).toBe('db1');
      expect(useSchemaStore.getState().tables.map((t) => t.name)).toEqual(['orders']);
    });

    const publicSchema = await findByText('public');
    fireEvent.click(publicSchema.closest('button')!);

    const tablesCategory = await findByText('schemaTree.tables');
    fireEvent.click(tablesCategory.closest('button')!);

    expect(await findByText('orders')).toBeInTheDocument();
  });

  it('expands a mysql database node and loads tables', async () => {
    mockGetDatabases.mockResolvedValueOnce(['alpha', 'beta']);
    mockGetTables.mockResolvedValueOnce([
      { name: 't1', tableType: 'table', schema: null, rowCount: null },
      { name: 't2', tableType: 'table', schema: null, rowCount: null },
    ]);

    const { findByText, getByText } = render(<SchemaTree {...baseProps} databaseType="mysql" />);

    const dbBtn = await findByText('alpha');
    fireEvent.click(dbBtn.closest('button')!);

    await waitFor(() => {
      expect(mockUseDatabase).toHaveBeenCalledWith('conn-1', 'alpha');
      expect(mockGetTables).toHaveBeenCalledWith('conn-1', 'alpha');
      expect(useSchemaStore.getState().currentDatabase).toBe('alpha');
      expect(useSchemaStore.getState().tables.map((t) => t.name)).toEqual(['t1', 't2']);
    });

    const tablesCategory = await findByText('schemaTree.tables');
    fireEvent.click(tablesCategory.closest('button')!);

    expect(await findByText('t1')).toBeInTheDocument();
    expect(getByText('t2')).toBeInTheDocument();
  });

  it('removes a dropped table from the multi-db sidebar without remounting', async () => {
    mockGetDatabases.mockResolvedValueOnce(['alpha', 'beta']);
    mockGetTables.mockResolvedValueOnce([
      { name: 't1', tableType: 'table', schema: null, rowCount: null },
      { name: 't2', tableType: 'table', schema: null, rowCount: null },
    ]);

    const { findByText, queryByText } = render(<SchemaTree {...baseProps} databaseType="mysql" />);

    fireEvent.click((await findByText('alpha')).closest('button')!);

    await waitFor(() => {
      expect(useSchemaStore.getState().tables.length).toBeGreaterThan(0);
    });

    const tablesCategory = await findByText('schemaTree.tables');
    fireEvent.click(tablesCategory.closest('button')!);

    expect(await findByText('t2')).toBeInTheDocument();

    useSchemaStore.getState().removeRelation('t2');

    await waitFor(() => {
      expect(queryByText('t2')).not.toBeInTheDocument();
    });
    expect(await findByText('t1')).toBeInTheDocument();
  });

  it('groups tables by schema when multiple schemas exist (PostgreSQL)', async () => {
    mockGetDatabases.mockResolvedValueOnce(['mydb']);
    mockGetTables.mockResolvedValueOnce([
      { name: 'users', tableType: 'table', schema: 'public', rowCount: null },
      { name: 'audit_log', tableType: 'table', schema: 'audit', rowCount: null },
      { name: 'active_users', tableType: 'view', schema: 'public', rowCount: null },
    ]);

    const { findByText, getByText, getAllByText } = render(
      <SchemaTree {...baseProps} databaseType="postgresql" />,
    );

    const dbBtn = await findByText('mydb');
    fireEvent.click(dbBtn.closest('button')!);

    expect(await findByText('public')).toBeInTheDocument();
    expect(getByText('audit')).toBeInTheDocument();

    fireEvent.click(getByText('public').closest('button')!);
    const publicTablesBtn = getAllByText('schemaTree.tables')[0];
    fireEvent.click(publicTablesBtn.closest('button')!);
    expect(await findByText('users')).toBeInTheDocument();

    const publicViewsBtn = getAllByText('schemaTree.views')[0];
    fireEvent.click(publicViewsBtn.closest('button')!);
    expect(await findByText('active_users')).toBeInTheDocument();

    fireEvent.click(getByText('audit').closest('button')!);
    const auditTablesBtn = getAllByText('schemaTree.tables')[1];
    fireEvent.click(auditTablesBtn.closest('button')!);
    expect(await findByText('audit_log')).toBeInTheDocument();
  });

  it('shows public schema layer even when only public schema exists', async () => {
    mockGetDatabases.mockResolvedValueOnce(['mydb']);
    mockGetTables.mockResolvedValueOnce([
      { name: 'users', tableType: 'table', schema: 'public', rowCount: null },
      { name: 'orders', tableType: 'table', schema: 'public', rowCount: null },
    ]);

    const { findByText } = render(<SchemaTree {...baseProps} databaseType="postgresql" />);

    const dbBtn = await findByText('mydb');
    fireEvent.click(dbBtn.closest('button')!);

    expect(await findByText('public')).toBeInTheDocument();

    fireEvent.click((await findByText('public')).closest('button')!);
    fireEvent.click((await findByText('schemaTree.tables')).closest('button')!);
    expect(await findByText('users')).toBeInTheDocument();
    expect(await findByText('orders')).toBeInTheDocument();
  });
});
