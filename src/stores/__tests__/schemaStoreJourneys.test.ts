import { projectRelationColumns } from '../schemaColumnLoader';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReadColumnsOutput, TableInfo } from '@datazen/driver-sdk';

const mocks = vi.hoisted(() => ({
  databases: vi.fn<() => Promise<string[]>>(),
  tables: vi.fn<() => Promise<TableInfo[]>>(),
  columns: vi.fn<() => Promise<ReadColumnsOutput>>(),
}));
vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getDatabases: mocks.databases,
    listTables: mocks.tables,
  },
}));
vi.mock('@datazen/driver-sdk', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/driver-sdk')>()),
  schemaClient: { readColumns: mocks.columns },
}));

import { renderHook, cleanup } from '@testing-library/react';
import { createEmptyConnectionSchema } from '../schemaStoreState';
import { useConnectionSchemaField, useSchemaStore } from '../schemaStore';
import { relationKey, syncSchemaTables } from '@datazen/driver-sdk';

function snapshot() {
  const state = useSchemaStore.getState().schemas.get('session') ?? createEmptyConnectionSchema();
  return {
    ...state,
    ...projectRelationColumns(
      state,
      'session',
      state.currentDatabase ?? '',
      state.currentSchema ?? undefined,
    ),
  };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

const tables = (name: string): TableInfo[] => [{ name, schema: null, tableType: 'table' }];

describe('schema directory and metadata journeys', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    useSchemaStore.getState().reset();
    useSchemaStore.setState({
      activeDbSessionId: 'session',
      schemas: new Map([['session', createEmptyConnectionSchema()]]),
    });
  });

  it.each(['old-first', 'new-first'])(
    'A → B keeps B regardless of completion order: %s',
    async (order) => {
      const a = deferred<TableInfo[]>();
      const b = deferred<TableInfo[]>();
      mocks.tables.mockReturnValueOnce(a.promise).mockReturnValueOnce(b.promise);
      const loadA = useSchemaStore.getState().switchDatabase('a', 'session');
      const loadB = useSchemaStore.getState().switchDatabase('b', 'session');
      expect(snapshot().loading).toBe(true);
      if (order === 'old-first') {
        a.resolve(tables('a_table'));
        await loadA;
        expect(snapshot().loading).toBe(true);
        b.resolve(tables('b_table'));
      } else {
        b.resolve(tables('b_table'));
        await loadB;
        a.resolve(tables('a_table'));
      }
      await Promise.all([loadA, loadB]);
      expect(snapshot().currentDatabase).toBe('b');
      expect(snapshot().tables).toEqual(tables('b_table'));
      expect(snapshot().loading).toBe(false);
    },
  );

  it('a stale failure cannot replace the selected database with an error', async () => {
    const a = deferred<TableInfo[]>();
    mocks.tables.mockReturnValueOnce(a.promise).mockResolvedValueOnce(tables('b_table'));
    const loadA = useSchemaStore.getState().switchDatabase('a', 'session');
    await useSchemaStore.getState().switchDatabase('b', 'session');
    a.reject(new Error('old database unavailable'));
    await loadA;
    expect(snapshot().error).toBeNull();
    expect(snapshot().currentDatabase).toBe('b');
  });

  it('background B refresh preserves A tables and columns, then B can be activated', () => {
    useSchemaStore.getState().setLoadedTables('a', tables('a_table'), 'session');
    const entry = useSchemaStore.getState().schemas.get('session')!;
    const ref = { database: 'a', schema: null, name: 'a_table' };
    useSchemaStore.setState({
      schemas: new Map([
        [
          'session',
          {
            ...entry,
            relationColumns: {
              [relationKey({ ...ref, dbSessionId: 'session' })]: {
                ref,
                columns: [{ name: 'id', dataType: 'integer', nullable: false }],
                primaryKeys: [],
              },
            },
          },
        ],
      ]),
    });
    useSchemaStore.getState().setLoadedTables('b', tables('b_table'), 'session', {
      pinCurrentDatabase: false,
    });
    expect(snapshot().currentDatabase).toBe('a');
    expect(snapshot().tables).toEqual(tables('a_table'));
    expect(snapshot().columnMap).toEqual({ a_table: ['id'] });
    const cachedB = snapshot().tableCatalogs.b;
    useSchemaStore.getState().setLoadedTables('b', cachedB, 'session');
    expect(snapshot().tables).toEqual(tables('b_table'));
    expect(snapshot().columnMap).toEqual({});
  });

  it('column responses from a replaced directory cannot repopulate its maps', async () => {
    const columns = deferred<ReadColumnsOutput>();
    mocks.columns.mockReturnValueOnce(columns.promise);
    useSchemaStore.getState().setLoadedTables('a', tables('users'), 'session');
    const load = useSchemaStore.getState().ensureColumns(['users'], 'session', 'a');
    useSchemaStore.getState().setLoadedTables('b', tables('users'), 'session');
    columns.resolve({
      results: [
        {
          status: 'ok',
          value: {
            ref: { database: 'a', schema: null, name: 'users' },
            columns: [{ name: 'old_column', dataType: 'text', nullable: true }],
            primaryKeys: [],
          },
        },
      ],
    });
    await load;
    expect(snapshot().columnMap).toEqual({});
    expect(snapshot().relationColumns).toEqual({});
    expect(snapshot().columnInflight.size).toBe(0);
  });

  it('disconnect while listing databases never resurrects the session', async () => {
    const databases = deferred<string[]>();
    mocks.databases.mockReturnValueOnce(databases.promise);
    const load = useSchemaStore.getState().loadForConnection('session', { skipLoadTables: true });
    useSchemaStore.getState().removeConnection('session');
    databases.resolve(['a']);
    await load;
    expect(useSchemaStore.getState().schemas.has('session')).toBe(false);
    expect(useSchemaStore.getState().activeDbSessionId).toBeNull();
  });

  it('SDK publication to background session leaves the active session unchanged', () => {
    useSchemaStore.getState().setLoadedTables('a', tables('active'), 'session');
    syncSchemaTables('b', tables('background'), 'other-session');
    expect(useSchemaStore.getState().activeDbSessionId).toBe('session');
    expect(snapshot().tables).toEqual(tables('active'));
    expect(useSchemaStore.getState().schemas.get('other-session')?.tables).toEqual(
      tables('background'),
    );
  });
});

afterEach(cleanup);

it('keeps bound session metadata independent of the active session and missing sessions empty', () => {
  const a = { ...createEmptyConnectionSchema(), tables: tables('active_table') };
  const b = { ...createEmptyConnectionSchema(), tables: tables('bound_table') };
  useSchemaStore.setState({
    schemas: new Map([
      ['active', a],
      ['bound', b],
    ]),
    activeDbSessionId: 'active',
  });
  const bound = renderHook(() => useConnectionSchemaField('bound', 'tables'));
  const missing = renderHook(() => useConnectionSchemaField('missing', 'tables'));
  expect(bound.result.current).toEqual(tables('bound_table'));
  expect(missing.result.current).toEqual([]);
  expect(useSchemaStore.getState().activeDbSessionId).toBe('active');
});
