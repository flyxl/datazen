import { afterEach, describe, expect, it, vi } from 'vitest';

/**
 * The desktop half of the table-data port.
 *
 * `packages/backend-client` declares *what* travels; this file pins *which
 * Tauri command* each method reaches and *how* its payload is shaped on the
 * way out. The two shapes are not the same: the kernel-surface commands take
 * a single `request` argument, so their payload is nested under `request`,
 * while the table-data commands take named arguments (`context`, `changes`,
 * `plan`, `scope`, ...), so their payload is the argument bag itself.
 *
 * Getting this wrong does not fail a type check — both shapes are valid
 * `MethodMap` requests — it fails at the IPC boundary, with a Rust side that
 * cannot find the field it was promised. So the mapping is asserted here.
 */

const mock = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mock.invoke,
  Channel: class {
    onmessage = (_message: unknown) => {};
  },
}));

import { createDesktopBackendTransport } from '../tauriBackendTransport';
import type { Counter, Id, MethodMap, RowChangePlan } from '@datazen/backend-client';

afterEach(() => {
  vi.clearAllMocks();
});

const CONTEXT = {
  connectionId: 'conn-1',
  dbSessionId: 'sess-1',
  driverType: 'postgres',
  database: 'sales',
  schema: 'public',
  table: 'orders',
} as const;

const PLAN: RowChangePlan = {
  planId: 'plan-1',
  fingerprint: 'fp-1',
  configRevision: 7 as Counter,
  table: { ...CONTEXT },
  updates: [],
  deletes: [],
  inserts: [],
  warnings: [],
};

describe('desktop transport: table-data commands', () => {
  it('reads a table through the target-bound command, with the request nested', async () => {
    mock.invoke.mockResolvedValue({ columns: [], rows: [], page: 0, pageSize: 50 });
    const transport = createDesktopBackendTransport();

    await transport.call('getTableData', {
      context: { ...CONTEXT },
      manualHandle: null,
      page: 0,
      pageSize: 50,
      filters: undefined,
      sorts: undefined,
      skipCount: undefined,
      filterLogic: undefined,
    });

    // `get_table_data` also exists on the Rust side, but the grid reads
    // through the target-bound one; the port has to name the command the
    // grid actually uses.
    expect(mock.invoke).toHaveBeenLastCalledWith('get_table_data_at_target', {
      request: {
        context: { ...CONTEXT },
        manualHandle: null,
        page: 0,
        pageSize: 50,
        filters: undefined,
        sorts: undefined,
        skipCount: undefined,
        filterLogic: undefined,
      },
    });
  });

  it('previews a change through the flat argument bag the command declares', async () => {
    mock.invoke.mockResolvedValue(PLAN);
    const transport = createDesktopBackendTransport();

    await transport.call('previewPendingChanges', {
      context: { ...CONTEXT },
      changes: [],
    });

    // The Rust command takes `context` and `changes` as named arguments, so
    // the payload is the bag itself — nesting it would make the Rust side
    // look for a `request` field that does not exist.
    expect(mock.invoke).toHaveBeenLastCalledWith('preview_pending_changes', {
      context: { ...CONTEXT },
      changes: [],
    });
  });

  it('commits a change through the flat argument bag the command declares', async () => {
    mock.invoke.mockResolvedValue({
      planId: 'plan-1',
      fingerprint: 'fp-1',
      statements: [],
      affectedRows: 0,
    });
    const transport = createDesktopBackendTransport();

    await transport.call('commitPendingChanges', {
      dbSessionId: 'sess-1',
      manualHandle: null,
      plan: PLAN,
      fingerprint: 'fp-1',
    });

    expect(mock.invoke).toHaveBeenLastCalledWith('commit_pending_changes', {
      dbSessionId: 'sess-1',
      manualHandle: null,
      plan: PLAN,
      fingerprint: 'fp-1',
    });
  });

  it('estimates rows through the flat argument bag the command declares', async () => {
    mock.invoke.mockResolvedValue(null);
    const transport = createDesktopBackendTransport();

    await transport.call('estimateTableRows', {
      dbSessionId: 'sess-1',
      table: 'orders',
      database: 'sales',
      schema: 'public',
    });

    expect(mock.invoke).toHaveBeenLastCalledWith('estimate_table_rows', {
      dbSessionId: 'sess-1',
      table: 'orders',
      database: 'sales',
      schema: 'public',
    });
  });

  it('resolves a foreign key through the single-request command', async () => {
    mock.invoke.mockResolvedValue({ exists: true, candidates: [], truncated: false });
    const transport = createDesktopBackendTransport();

    const request = {
      dbSessionId: 'sess-1',
      table: 'orders',
      columns: ['customer_id'],
      values: [9],
    } satisfies MethodMap['resolveForeignKeyTargets']['request'];
    await transport.call('resolveForeignKeyTargets', request);

    expect(mock.invoke).toHaveBeenLastCalledWith('resolve_foreign_key_targets', { request });
  });

  it('writes a result cell through the single-request command', async () => {
    mock.invoke.mockResolvedValue(undefined);
    const transport = createDesktopBackendTransport();

    const request = {
      binding: {
        handle: { dbSessionId: 'sess-1' as Id, runtimeEpoch: 'epoch-1' as Id },
        resourceBindingId: 'res-1' as Id,
        executionId: 'exec-1' as Id,
      },
      statementIndex: 0,
      rowIndex: 2,
      column: 'note',
      value: 'edited',
    } satisfies MethodMap['commitQueryResultCell']['request'];
    await transport.call('commitQueryResultCell', request);

    expect(mock.invoke).toHaveBeenLastCalledWith('commit_query_result_cell', { request });
  });

  it('routes the three named-view commands to their own command names', async () => {
    mock.invoke.mockResolvedValue([]);
    const transport = createDesktopBackendTransport();

    const scope = {
      connectionId: 'conn-1',
      database: 'sales',
      schema: 'public',
      table: 'orders',
    };
    await transport.call('listNamedViews', { scope });
    expect(mock.invoke).toHaveBeenLastCalledWith('list_named_views', { scope });

    const view = {
      id: '',
      name: 'open orders',
      scope,
      filters: [],
      filterLogic: 'and' as const,
      sorts: [],
      visibleColumns: null,
      columnOrder: null,
      isDefault: false,
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    };
    await transport.call('saveNamedView', { view });
    expect(mock.invoke).toHaveBeenLastCalledWith('save_named_view', { view });

    await transport.call('deleteNamedView', { id: 'view-1' });
    expect(mock.invoke).toHaveBeenLastCalledWith('delete_named_view', { id: 'view-1' });
  });
});
