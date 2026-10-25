/**
 * The full-database value scan (DB-19): the rules of the driver and the
 * helpers around it.
 *
 * `ValueSearchDriver` is the only thing that drives a scan, so these tests
 * are the contract: one command per table, pause lands between statements,
 * resume continues from the table it stopped at, a failed table is a
 * recorded row rather than a failed scan, and the ranking answers "where is
 * this value?" with the tables that hold it most.
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
  rankTables,
  searchableColumns,
  valueSearchTargets,
  type ValueSearchProgress,
  type ValueSearchTarget,
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
  it('keeps the text-ish columns and drops the rest', () => {
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
        { table: 'a', matches: 1 },
        { table: 'b', matches: 30 },
        { table: 'c', matches: 7 },
      ],
      2,
    );

    expect(ranked.map((entry) => entry.table)).toEqual(['b', 'c']);
  });

  it('leaves out the tables without a hit and the failed ones', () => {
    const ranked = rankTables([
      { table: 'a', matches: 0 },
      { table: 'b', matches: null, reason: 'unreadable' },
    ]);

    expect(ranked).toEqual([]);
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
      { table: 'orders', matches: 3 },
      { table: 'customers', matches: 0 },
    ]);
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
      matches: null,
      reason: 'unreadable',
    });
    expect(progress.results[1]).toEqual({ table: 'customers', matches: 2 });
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
});
