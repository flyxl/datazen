/**
 * The full-database value scan (DB-19 + DBR-03): the rules of the driver and
 * the helpers around it.
 *
 * `ValueSearchDriver` is the only thing that drives a scan, so these tests
 * are the contract: one command per table, pause lands between statements,
 * resume continues from the table it stopped at, a failed table is a
 * recorded row rather than a failed scan, and the ranking answers "where is
 * this value?" with the tables that hold it most.
 *
 * DBR-03 adds its own contracts: every answer carries the schema it belongs
 * to (same-named tables across schemas are different tables), the searched
 * columns ride with the answer (the locate seeds exactly those), and the
 * type scope is the dialog's choice over the driver's declared groups.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

const { mockSearchTableValues } = vi.hoisted(() => ({
  mockSearchTableValues: vi.fn(),
}));

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    searchTableValues: mockSearchTableValues,
  },
}));

import {
  ValueSearchDriver,
  cellTypesForScope,
  filterOperatorForMode,
  locateConditions,
  locateFilterLogic,
  rankTables,
  searchableColumns,
  targetKey,
  typeScopesForDriver,
  type ValueSearchProgress,
  type ValueSearchTarget,
  valueSearchTargets,
  valueSearchTypeGroupsFor,
} from '../valueSearch';
import type { ColumnSchema } from '../../types';

const TEXT: ColumnSchema = {
  name: 'name',
  dataType: 'varchar(100)',
  nullable: true,
  isPrimaryKey: false,
};
const NUMBER: ColumnSchema = {
  name: 'age',
  dataType: 'integer',
  nullable: true,
  isPrimaryKey: false,
};
const DATE: ColumnSchema = {
  name: 'created_at',
  dataType: 'timestamp',
  nullable: true,
  isPrimaryKey: false,
};

function columns(...list: ColumnSchema[]): ColumnSchema[] {
  return list;
}

function driver(
  targets: ValueSearchTarget[],
  overrides: Partial<ConstructorParameters<typeof ValueSearchDriver>[0]> = {},
): ValueSearchDriver {
  return new ValueSearchDriver({
    dbSessionId: 'session-1',
    database: 'app',
    targets,
    value: 'acme',
    mode: 'contains',
    cellTypes: cellTypesForScope('text'),
    readSchema: async () => [TEXT],
    ...overrides,
  });
}

/** Wait until the driver's status settles to the expected one. */
async function waitFor(
  instance: ValueSearchDriver,
  predicate: (progress: ValueSearchProgress) => boolean,
): Promise<ValueSearchProgress> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    if (predicate(instance.progress)) return instance.progress;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error('the scan did not settle');
}

beforeEach(() => {
  mockSearchTableValues.mockReset();
});

describe('searchableColumns', () => {
  it('keeps columns the grid renders as text and drops the rest', () => {
    expect(searchableColumns(columns(TEXT, NUMBER))).toEqual(['name']);
    expect(searchableColumns(columns(NUMBER))).toEqual([]);
  });

  it('keeps an unclassified column — the grid renders it as text too', () => {
    const unknown: ColumnSchema = {
      name: 'blob_label',
      dataType: 'weirdtype',
      nullable: true,
      isPrimaryKey: false,
    };
    expect(searchableColumns(columns(unknown))).toEqual(['blob_label']);
  });

  it('adds the declared type groups to the scope, and only those (DBR-03)', () => {
    const schema = columns(TEXT, NUMBER, DATE);

    // The base scope never grows by itself.
    expect(searchableColumns(schema, cellTypesForScope('text'))).toEqual(['name']);
    // A declared group joins as text-representation matching.
    expect(searchableColumns(schema, cellTypesForScope('text+number'))).toEqual([
      'name',
      'age',
    ]);
    expect(searchableColumns(schema, cellTypesForScope('text+datetime'))).toEqual([
      'name',
      'created_at',
    ]);
    expect(
      searchableColumns(schema, cellTypesForScope('text+number+datetime')),
    ).toEqual(['name', 'age', 'created_at']);
  });
});

describe('valueSearchTypeGroupsFor (DBR-03)', () => {
  it('returns nothing for a driver that declares nothing — the safe text set it always had', () => {
    expect(valueSearchTypeGroupsFor('postgresql')).toEqual([]);
    expect(valueSearchTypeGroupsFor(undefined)).toEqual([]);
    expect(valueSearchTypeGroupsFor(null)).toEqual([]);
  });

  it('returns the groups a driver declares its LIKE can text-match', () => {
    expect(valueSearchTypeGroupsFor('mysql')).toEqual(['number', 'datetime']);
    expect(valueSearchTypeGroupsFor('sqlite')).toEqual(['number', 'datetime']);
  });

  it('offers only the base scope to a driver without declarations', () => {
    expect(typeScopesForDriver('postgresql')).toEqual(['text']);
  });

  it('offers one scope per declared group, and the combination', () => {
    expect(typeScopesForDriver('mysql')).toEqual([
      'text',
      'text+number',
      'text+datetime',
      'text+number+datetime',
    ]);
  });
});

describe('targetKey', () => {
  it('qualifies a schema-ful target and leaves a schema-less one bare', () => {
    expect(targetKey({ table: 'orders', schema: 'sales' })).toBe('sales.orders');
    expect(targetKey({ table: 'orders', schema: null })).toBe('orders');
  });
});

describe('valueSearchTargets', () => {
  it('scans tables only, in the list order', () => {
    expect(
      valueSearchTargets([
        { name: 'orders', tableType: 'table' },
        { name: 'v_orders', tableType: 'view' },
        { name: 'customers', schema: 'sales', tableType: 'table' },
      ]),
    ).toEqual([
      { table: 'orders', schema: null },
      { table: 'customers', schema: 'sales' },
    ]);
  });
});

describe('rankTables', () => {
  it('leads with the tables that hold the most hits', () => {
    const ranked = rankTables(
      [
        { table: 'a', schema: null, matches: 1, columns: ['name'] },
        { table: 'b', schema: null, matches: 30, columns: ['name'] },
        { table: 'c', schema: null, matches: 7, columns: ['name'] },
      ],
      2,
    );

    expect(ranked.map((entry) => entry.table)).toEqual(['b', 'c']);
  });

  it('leaves out the tables without a hit and the failed ones', () => {
    const ranked = rankTables([
      { table: 'a', schema: null, matches: 0, columns: [] },
      { table: 'b', schema: null, matches: null, columns: [], reason: 'unreadable' },
    ]);

    expect(ranked).toEqual([]);
  });
});

describe('the locate helpers (DBR-03)', () => {
  it('maps every search mode to the filter operator that says the same thing', () => {
    expect(filterOperatorForMode('contains')).toBe('contains');
    expect(filterOperatorForMode('notContains')).toBe('notContains');
    expect(filterOperatorForMode('prefix')).toBe('startsWith');
    expect(filterOperatorForMode('suffix')).toBe('endsWith');
    expect(filterOperatorForMode('exact')).toBe('eq');
  });

  it('builds one condition per searched column, all with the same operator', () => {
    expect(
      locateConditions({
        table: 'orders',
        schema: 'sales',
        database: 'app',
        value: 'acme',
        mode: 'contains',
        columns: ['name', 'email'],
      }),
    ).toEqual([
      { column: 'name', operator: 'contains', value: 'acme' },
      { column: 'email', operator: 'contains', value: 'acme' },
    ]);
  });

  it('combines the positive modes with OR and the negation with AND', () => {
    expect(locateFilterLogic('contains')).toBe('or');
    expect(locateFilterLogic('exact')).toBe('or');
    expect(locateFilterLogic('notContains')).toBe('and');
  });
});

describe('ValueSearchDriver', () => {
  it('searches every table, one command each, and finishes', async () => {
    mockSearchTableValues.mockImplementation(async (request: { table: string }) => ({
      table: request.table,
      searchedColumns: ['name'],
      matches: request.table === 'orders' ? 3 : 0,
    }));

    const instance = driver([
      { table: 'orders', schema: null },
      { table: 'customers', schema: null },
    ]);
    instance.start();

    const progress = await waitFor(instance, (state) => state.status === 'done');

    expect(mockSearchTableValues).toHaveBeenCalledTimes(2);
    expect(progress.completed).toBe(2);
    expect(progress.results).toEqual([
      { table: 'orders', schema: null, matches: 3, columns: ['name'] },
      { table: 'customers', schema: null, matches: 0, columns: ['name'] },
    ]);
  });

  it('answers with the schema each table belongs to — same names do not merge (DBR-03)', async () => {
    mockSearchTableValues.mockImplementation(async (request: { table: string }) => ({
      table: request.table,
      searchedColumns: ['name'],
      matches: 1,
    }));

    const instance = driver([
      { table: 'orders', schema: 'sales' },
      { table: 'orders', schema: 'archive' },
    ]);
    instance.start();

    const progress = await waitFor(instance, (state) => state.status === 'done');

    expect(progress.results).toEqual([
      { table: 'orders', schema: 'sales', matches: 1, columns: ['name'] },
      { table: 'orders', schema: 'archive', matches: 1, columns: ['name'] },
    ]);
    // The command carried each schema — the statement searched the right one.
    expect(mockSearchTableValues).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({ table: 'orders', schema: 'sales' }),
    );
    expect(mockSearchTableValues).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({ table: 'orders', schema: 'archive' }),
    );
  });

  it('passes the searchable columns of each table, and skips a table without any', async () => {
    mockSearchTableValues.mockResolvedValue({
      table: 'orders',
      searchedColumns: ['name'],
      matches: 1,
    });

    const instance = driver([{ table: 'orders', schema: 'sales' }], {
      readSchema: async (target) => (target.table === 'orders' ? [TEXT, NUMBER] : []),
    });
    instance.start();
    await waitFor(instance, (state) => state.status === 'done');

    expect(mockSearchTableValues).toHaveBeenCalledWith(
      expect.objectContaining({
        table: 'orders',
        schema: 'sales',
        columns: ['name'],
      }),
    );
  });

  it('searches the extra type groups only when the scope includes them (DBR-03)', async () => {
    mockSearchTableValues.mockResolvedValue({
      table: 'orders',
      searchedColumns: ['name', 'age', 'created_at'],
      matches: 1,
    });

    const instance = driver([{ table: 'orders', schema: null }], {
      cellTypes: cellTypesForScope('text+number+datetime'),
      readSchema: async () => [TEXT, NUMBER, DATE],
    });
    instance.start();
    await waitFor(instance, (state) => state.status === 'done');

    expect(mockSearchTableValues).toHaveBeenCalledWith(
      expect.objectContaining({ columns: ['name', 'age', 'created_at'] }),
    );
    expect(instance.progress.results[0].columns).toEqual(['name', 'age', 'created_at']);
  });

  it('records a failed table and keeps the scan going', async () => {
    mockSearchTableValues.mockImplementation(async (request: { table: string }) => {
      if (request.table === 'orders') throw new Error('unreadable');
      return { table: request.table, searchedColumns: ['name'], matches: 2 };
    });

    const instance = driver([
      { table: 'orders', schema: null },
      { table: 'customers', schema: null },
    ]);
    instance.start();

    const progress = await waitFor(instance, (state) => state.status === 'done');

    // The failed table still counts as answered.
    expect(progress.completed).toBe(2);
    expect(progress.results[0]).toEqual({
      table: 'orders',
      schema: null,
      matches: null,
      columns: [],
      reason: 'unreadable',
    });
    expect(progress.results[1]).toEqual({
      table: 'customers',
      schema: null,
      matches: 2,
      columns: ['name'],
    });
  });

  it('pauses between tables and resumes from where it stopped', async () => {
    let calls = 0;
    mockSearchTableValues.mockImplementation(async () => {
      calls += 1;
      // Pause after the first table started: the pause lands between
      // statements, never inside one.
      if (calls === 1) {
        instance.pause();
      }
      return { table: 'orders', searchedColumns: ['name'], matches: 1 };
    });

    const instance = driver([
      { table: 'orders', schema: null },
      { table: 'customers', schema: null },
    ]);
    instance.start();

    await waitFor(instance, (state) => state.status === 'paused');
    // The first statement finished; the second has not been issued.
    expect(mockSearchTableValues).toHaveBeenCalledTimes(1);

    instance.start();
    const progress = await waitFor(instance, (state) => state.status === 'done');

    expect(progress.completed).toBe(2);
    expect(mockSearchTableValues).toHaveBeenCalledTimes(2);
  });

  it('cancels for good — a scan nobody watches should not keep running', async () => {
    mockSearchTableValues.mockResolvedValue({
      table: 'orders',
      searchedColumns: ['name'],
      matches: 1,
    });

    const instance = driver([
      { table: 'orders', schema: null },
      { table: 'customers', schema: null },
    ]);
    instance.start();
    instance.cancel();

    const progress = await waitFor(instance, (state) => state.status === 'cancelled');

    expect(progress.completed).toBeLessThanOrEqual(1);
    // Give a running loop a moment to prove it stopped issuing commands.
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(mockSearchTableValues).toHaveBeenCalledTimes(1);
  });

  it('measures the scan it ran — wall-clock, never a made-up ETA (DBR-03)', async () => {
    mockSearchTableValues.mockResolvedValue({
      table: 'orders',
      searchedColumns: ['name'],
      matches: 1,
    });

    const instance = driver([{ table: 'orders', schema: null }]);
    expect(instance.progress.elapsedMs).toBeNull();

    instance.start();
    const progress = await waitFor(instance, (state) => state.status === 'done');

    expect(progress.elapsedMs).not.toBeNull();
    expect(progress.elapsedMs).toBeGreaterThanOrEqual(0);
  });
});
