import { describe, expect, it } from 'vitest';

import {
  ApiError,
  createBackendClient,
  toTableValue,
  type BackendTransport,
  type ChangeWarning,
  type CommitPendingChangesRequest,
  type CommitPendingChangesResponse,
  type Counter,
  type EstimateTableRowsRequest,
  type Id,
  type MethodMap,
  type PendingRowChange,
  type PlannedStatement,
  type PreviewPendingChangesRequest,
  type ResolveForeignKeyTargetsRequest,
  type ResultCellWrite,
  type RowChangePlan,
  type SessionHandle,
  type TableChangeContext,
  type TableDataRequest,
  type TableDataResult,
  type UnsupportedMethodKind,
} from '../src/index';

/* ------------------------------------------------------------------ */
/* Compile-time parity with the desktop mirror.                       */
/*                                                                     */
/* The desktop frontend keeps its own copy of these shapes in          */
/* `src/lib/tableChanges.ts`. Two copies drift, and a drift here is    */
/* not a cosmetic difference: the same pending change would preview    */
/* on the desktop and commit differently on the web, or the insert    */
/* flag would be dropped on one side only.                            */
/*                                                                     */
/* The assertion below is a *type* assertion, so it is checked by     */
/* `tsc --noEmit` and not by a test run: a shape that stops matching  */
/* is a typecheck failure, long before a test could notice.           */
/* ------------------------------------------------------------------ */

type DesktopTableChangeContext = import('../../../src/lib/tableChanges').TableChangeContext;
type DesktopPendingRowChange = import('../../../src/lib/tableChanges').PendingRowChange;
type DesktopPlannedStatement = import('../../../src/lib/tableChanges').PlannedStatement;
type DesktopChangeWarning = import('../../../src/lib/tableChanges').ChangeWarning;
type DesktopRowChangePlan = import('../../../src/lib/tableChanges').RowChangePlan;
type DesktopCommitStatementResult = import('../../../src/lib/tableChanges').CommitStatementResult;
type DesktopCommitPendingChangesResponse =
  import('../../../src/lib/tableChanges').CommitPendingChangesResponse;

/** `true` only when `A` and `B` are mutually assignable. */
type MutuallyAssignable<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;

/**
 * `true` only when `A` and `B` declare exactly the same keys.
 *
 * Mutual assignability alone is not enough, and the gap is not academic: an
 * **optional** field that exists on one side and not the other is assignable
 * in *both* directions, so a mirror that silently dropped `insertMarked?`
 * would still pass an assignability check. The key comparison is what makes
 * that failure loud.
 */
type SameKeys<A, B> = [Exclude<keyof A, keyof B>] extends [never]
  ? [Exclude<keyof B, keyof A>] extends [never]
    ? true
    : false
  : false;

/**
 * `true` only when the two shapes are the same shape: same keys, and each
 * assignable to the other (which is what catches a required field that
 * became optional, or a field whose type drifted).
 */
type Parity<A, B> = MutuallyAssignable<A, B> extends true
  ? SameKeys<A, B> extends true
    ? true
    : false
  : false;

/** Fails to compile when the checked condition is not `true`. */
type Assert<T extends true> = T;

export type ContextParity = Assert<Parity<DesktopTableChangeContext, TableChangeContext>>;
export type PendingChangeParity = Assert<Parity<DesktopPendingRowChange, PendingRowChange>>;
export type PlannedStatementParity = Assert<Parity<DesktopPlannedStatement, PlannedStatement>>;
export type ChangeWarningParity = Assert<Parity<DesktopChangeWarning, ChangeWarning>>;
export type RowChangePlanParity = Assert<Parity<DesktopRowChangePlan, RowChangePlan>>;
export type CommitStatementParity = Assert<
  Parity<DesktopCommitStatementResult, import('../src/index').CommitStatementResult>
>;
export type CommitResponseParity = Assert<
  Parity<DesktopCommitPendingChangesResponse, CommitPendingChangesResponse>
>;
/**
 * The table read result is mirrored too, from the desktop's `src/types`.
 *
 * It matters for the same reason: the same read must mean the same thing on
 * both sides of the port — in particular whether "the count was not taken"
 * arrives as `null` or as a missing key.
 */
export type TableDataResultParity = Assert<
  Parity<import('../../../src/types').TableDataResult, TableDataResult>
>;

/* ------------------------------------------------------------------ */
/* Fixtures.                                                           */
/* ------------------------------------------------------------------ */

const CONTEXT: TableChangeContext = {
  connectionId: 'conn-1',
  dbSessionId: 'sess-1',
  driverType: 'postgres',
  database: 'sales',
  schema: 'public',
  table: 'orders',
};

const INSERT_CHANGE: PendingRowChange = {
  rowIdentity: {},
  originalValues: {},
  currentValues: { sku: 'A-1', note: null },
  changedColumns: ['sku', 'note'],
  deleteMarked: false,
  insertMarked: true,
};

const UPDATE_CHANGE: PendingRowChange = {
  rowIndex: 3,
  rowIdentity: { id: 7 },
  originalValues: { id: 7, note: 'old' },
  currentValues: { id: 7, note: 'new' },
  changedColumns: ['note'],
  deleteMarked: false,
};

const PLANNED: PlannedStatement = {
  rowIdentity: { id: 7 },
  originalValues: { id: 7, note: 'old' },
  currentValues: { id: 7, note: 'new' },
  changedColumns: ['note'],
  sqlTemplate: 'UPDATE orders SET note = $1 WHERE id = $2',
  parameterSummary: ['new', '7'],
};

const PLAN: RowChangePlan = {
  planId: 'plan-1',
  fingerprint: 'fp-1',
  configRevision: 7 as Counter,
  table: CONTEXT,
  updates: [PLANNED],
  deletes: [],
  inserts: [
    {
      ...PLANNED,
      rowIdentity: {},
      originalValues: {},
      currentValues: { sku: 'A-1', note: null },
      changedColumns: ['sku', 'note'],
      sqlTemplate: 'INSERT INTO orders (sku, note) VALUES ($1, $2)',
    },
  ],
  warnings: [],
};

/**
 * A transport that records every call.
 *
 * The point is to assert on the *wire* — the method name and the payload —
 * and not on the facade's return value, because a facade that quietly
 * renamed or reshaped an argument would still receive the canned answer and
 * pass.
 */
function createFakeTransport(
  options: { unsupportedMethods?: Partial<Record<keyof MethodMap, UnsupportedMethodKind>> } = {},
) {
  const calls: Array<{ method: keyof MethodMap; payload: unknown }> = [];
  const responses = new Map<keyof MethodMap, unknown>();

  const transport: BackendTransport = {
    unsupportedMethods: options.unsupportedMethods,
    async call<K extends keyof MethodMap>(method: K, payload: MethodMap[K]['request']) {
      calls.push({ method, payload });
      if (!responses.has(method)) throw new Error(`fake transport has no response for ${method}`);
      return responses.get(method) as MethodMap[K]['response'];
    },
  };

  return { transport, calls, responses };
}

/* ------------------------------------------------------------------ */
/* The wire shapes the table surface travels in.                       */
/* ------------------------------------------------------------------ */

describe('table data wire shapes', () => {
  it('carries the insert flag, the insert statements and the insert operation', () => {
    // The three fields a change set gained when new rows became writable.
    // A mirror that forgot any one of them makes an insert look like an
    // update on one side of the port.
    expect(INSERT_CHANGE.insertMarked).toBe(true);
    expect(PLAN.inserts).toHaveLength(1);
    expect(PLAN.inserts?.[0]?.changedColumns).toEqual(['sku', 'note']);

    const response: CommitPendingChangesResponse = {
      planId: 'plan-1',
      fingerprint: 'fp-1',
      statements: [
        { operation: 'insert', rowIdentity: {}, affectedRows: 1 },
        { operation: 'update', rowIdentity: { id: 7 }, affectedRows: 1 },
        { operation: 'delete', rowIdentity: { id: 8 }, affectedRows: 1 },
      ],
      affectedRows: 3,
    };
    expect(response.statements.map((statement) => statement.operation)).toEqual([
      'insert',
      'update',
      'delete',
    ]);
  });

  it('keeps an insert-free plan free of the insert field', () => {
    // The backend omits `insertMarked` when it is false, so a plan without
    // inserts must not carry an empty insert list either: the fingerprint of
    // an insert-free plan has to stay byte-identical to the payload that
    // predates the flag.
    const insertFree: RowChangePlan = { ...PLAN, inserts: undefined };
    expect(insertFree.inserts).toBeUndefined();
    expect(JSON.parse(JSON.stringify(insertFree)).inserts).toBeUndefined();
  });

  it('accepts a table read whose row count was not taken', () => {
    // The backend's count is optional, so the key is always there and the
    // value may be `null`. A type that said "the key may be missing" would
    // be a different — and wrong — claim about the wire, and a caller that
    // did `result.totalRows ?? 0` would still be right only by accident.
    const uncounted: TableDataResult = {
      columns: [{ name: 'id', dataType: 'int4', nullable: false }],
      rows: [[7]],
      totalRows: null,
      page: 0,
      pageSize: 50,
    };
    expect(uncounted.totalRows).toBeNull();
    expect(JSON.parse(JSON.stringify(uncounted)).totalRows).toBeNull();

    const counted: TableDataResult = { ...uncounted, totalRows: 1 };
    expect(counted.totalRows).toBe(1);
  });

  it('narrows a cell value to what the wire can carry', () => {
    expect(toTableValue(null)).toBeNull();
    expect(toTableValue('text')).toBe('text');
    expect(toTableValue(42)).toBe(42);
    expect(toTableValue(false)).toBe(false);
    expect(toTableValue({ a: 1 })).toEqual({ a: 1 });
    expect(toTableValue([1, 'two'])).toEqual([1, 'two']);

    // A value the wire cannot carry is refused, not silently dropped.
    expect(toTableValue(undefined)).toBeUndefined();
    expect(toTableValue(Number.NaN)).toBeUndefined();
    expect(toTableValue(Number.POSITIVE_INFINITY)).toBeUndefined();
    expect(toTableValue(() => undefined)).toBeUndefined();
    expect(toTableValue(Symbol('nope'))).toBeUndefined();
  });
});

/* ------------------------------------------------------------------ */
/* Every table-data method is registered and delegated.                */
/* ------------------------------------------------------------------ */

describe('BackendClient table-data surface', () => {
  it('routes a table read under its own method name and passes the request through', async () => {
    const fake = createFakeTransport();
    const result: TableDataResult = {
      columns: [{ name: 'id', dataType: 'int4', nullable: false }],
      rows: [[7]],
      totalRows: 1,
      page: 0,
      pageSize: 50,
    };
    fake.responses.set('getTableData', result);

    const client = createBackendClient('desktop', fake.transport);
    const request: TableDataRequest = {
      context: CONTEXT,
      manualHandle: null,
      page: 0,
      pageSize: 50,
      filters: [{ column: 'note', operator: 'eq', value: 'old' }],
      sorts: [{ column: 'id', descending: true }],
      skipCount: false,
      filterLogic: 'and',
    };
    await expect(client.getTableData(request)).resolves.toEqual(result);

    expect(fake.calls).toEqual([{ method: 'getTableData', payload: request }]);
  });

  it('routes a change preview under its own method name', async () => {
    const fake = createFakeTransport();
    fake.responses.set('previewPendingChanges', PLAN);

    const client = createBackendClient('desktop', fake.transport);
    const request: PreviewPendingChangesRequest = {
      context: CONTEXT,
      changes: [UPDATE_CHANGE, INSERT_CHANGE],
    };
    await expect(client.previewPendingChanges(request)).resolves.toEqual(PLAN);

    expect(fake.calls).toEqual([{ method: 'previewPendingChanges', payload: request }]);
  });

  it('routes a change commit under its own method name, with the plan and its fingerprint', async () => {
    const fake = createFakeTransport();
    const response: CommitPendingChangesResponse = {
      planId: 'plan-1',
      fingerprint: 'fp-1',
      statements: [{ operation: 'insert', rowIdentity: {}, affectedRows: 1 }],
      affectedRows: 1,
    };
    fake.responses.set('commitPendingChanges', response);

    const client = createBackendClient('desktop', fake.transport);
    const request: CommitPendingChangesRequest = {
      dbSessionId: 'sess-1',
      manualHandle: null,
      plan: PLAN,
      fingerprint: 'fp-1',
    };
    await expect(client.commitPendingChanges(request)).resolves.toEqual(response);

    expect(fake.calls).toEqual([{ method: 'commitPendingChanges', payload: request }]);
  });

  it('reports an unusable estimate as null rather than as a failure', async () => {
    const fake = createFakeTransport();
    fake.responses.set('estimateTableRows', null);

    const client = createBackendClient('desktop', fake.transport);
    const request: EstimateTableRowsRequest = {
      dbSessionId: 'sess-1',
      table: 'orders',
      database: 'sales',
      schema: 'public',
    };
    await expect(client.estimateTableRows(request)).resolves.toBeNull();

    expect(fake.calls).toEqual([{ method: 'estimateTableRows', payload: request }]);
  });

  it('routes a foreign-key resolution under its own method name', async () => {
    const fake = createFakeTransport();
    const resolution = {
      target: {
        database: 'sales',
        schema: 'public',
        table: 'customers',
        columns: ['id'],
        dataTypes: ['int4'],
        primaryKeys: ['id'],
        unique: true,
        candidateLimit: 50,
      },
      exists: true,
      candidates: [[9]],
      truncated: false,
    };
    fake.responses.set('resolveForeignKeyTargets', resolution);

    const client = createBackendClient('desktop', fake.transport);
    const request: ResolveForeignKeyTargetsRequest = {
      dbSessionId: 'sess-1',
      database: 'sales',
      schema: 'public',
      table: 'orders',
      columns: ['customer_id'],
      values: [9],
    };
    await expect(client.resolveForeignKeyTargets(request)).resolves.toEqual(resolution);

    expect(fake.calls).toEqual([{ method: 'resolveForeignKeyTargets', payload: request }]);
  });

  it('routes a result-cell write under its own method name', async () => {
    const fake = createFakeTransport();
    fake.responses.set('commitQueryResultCell', undefined);

    const client = createBackendClient('desktop', fake.transport);
    const request: ResultCellWrite = {
      binding: {
        handle: { dbSessionId: 'sess-1' as Id, runtimeEpoch: 'epoch-1' as Id },
        resourceBindingId: 'res-1' as Id,
        executionId: 'exec-1' as Id,
      },
      statementIndex: 0,
      rowIndex: 2,
      column: 'note',
      value: 'edited',
    };
    await expect(client.commitQueryResultCell(request)).resolves.toBeUndefined();

    expect(fake.calls).toEqual([{ method: 'commitQueryResultCell', payload: request }]);
  });

  it('declares the named-view methods on the transport as the desktop-only ones', async () => {
    // The three named-view commands are registered in the port even though a
    // browser build cannot serve them: the port has to know they exist, and
    // the gap has to be declared rather than discovered at call time.
    const fake = createFakeTransport();
    const client = createBackendClient('desktop', fake.transport);
    expect(typeof client.listNamedViews).toBe('function');
    expect(typeof client.saveNamedView).toBe('function');
    expect(typeof client.deleteNamedView).toBe('function');
  });
});

/* ------------------------------------------------------------------ */
/* The capability gap.                                                 */
/* ------------------------------------------------------------------ */

describe('a backend that cannot serve a method', () => {
  /**
   * A browser build's gaps. `capability` is the right kind here: the saved
   * views live in the *host's* persistent store, and a browser build has no
   * host store at all — not "not yet", but "not ever, in this form factor".
   */
  const WEB_UNSUPPORTED: Partial<Record<keyof MethodMap, UnsupportedMethodKind>> = {
    listNamedViews: 'capability',
    saveNamedView: 'capability',
    deleteNamedView: 'capability',
  };

  it('fails a named-view call with CapabilityUnsupported instead of an empty list', async () => {
    const fake = createFakeTransport({ unsupportedMethods: WEB_UNSUPPORTED });
    const client = createBackendClient('web', fake.transport);

    // The failure must be explicit. An empty list here would read as "you
    // have no saved views", and the UI would offer to save one that can
    // never be read back.
    await expect(client.listNamedViews({})).rejects.toBeInstanceOf(ApiError);
    await expect(client.listNamedViews({})).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
    });
  });

  it('never reaches the transport for an unsupported method', async () => {
    const fake = createFakeTransport({ unsupportedMethods: WEB_UNSUPPORTED });
    const client = createBackendClient('web', fake.transport);

    await expect(client.saveNamedView({ view: {} as never })).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
    });
    await expect(client.deleteNamedView({ id: 'view-1' })).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
    });

    expect(fake.calls).toHaveLength(0);
  });

  it('names the method and the backend in the failure', async () => {
    const fake = createFakeTransport({ unsupportedMethods: WEB_UNSUPPORTED });
    const client = createBackendClient('web', fake.transport);

    await expect(client.listNamedViews({})).rejects.toThrow(/listNamedViews/);
    await expect(client.listNamedViews({})).rejects.toThrow(/web/);
  });

  it('tells a permanent capability gap from a command that is not implemented yet', async () => {
    const fake = createFakeTransport({ unsupportedMethods: WEB_UNSUPPORTED });
    const web = createBackendClient('web', fake.transport);

    // A form factor that can never serve the method says so in the wording:
    // the feature is out of reach here, and a caller must hide it.
    await expect(web.listNamedViews({})).rejects.toThrow(/is not supported by the web backend/);

    // A backend that simply has not implemented it yet says that instead, so
    // a caller can tell "coming soon" from "never possible".
    const pending = createFakeTransport({ unsupportedMethods: { estimateTableRows: 'notImplemented' } });
    const desktop = createBackendClient('desktop', pending.transport);
    await expect(
      desktop.estimateTableRows({ dbSessionId: 'sess-1', table: 'orders' }),
    ).rejects.toThrow(/is not implemented by the desktop backend yet/);
  });

  it('carries the kind of the gap on the error, so a caller need not read the message', async () => {
    const fake = createFakeTransport({ unsupportedMethods: WEB_UNSUPPORTED });
    const client = createBackendClient('web', fake.transport);

    await expect(client.listNamedViews({})).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
      unsupportedKind: 'capability',
    });

    const pending = createFakeTransport({ unsupportedMethods: { estimateTableRows: 'notImplemented' } });
    const desktop = createBackendClient('desktop', pending.transport);
    await expect(
      desktop.estimateTableRows({ dbSessionId: 'sess-1', table: 'orders' }),
    ).rejects.toMatchObject({ unsupportedKind: 'notImplemented' });
  });

  it('keeps the kind out of the wire payload, which stays the §6.3 shape', async () => {
    const fake = createFakeTransport({ unsupportedMethods: WEB_UNSUPPORTED });
    const client = createBackendClient('web', fake.transport);

    const error = await client.listNamedViews({}).catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).toPayload()).toEqual({
      code: 'CapabilityUnsupported',
      message: 'listNamedViews is not supported by the web backend.',
      requestId: '',
      retryDisposition: 'never',
    });
  });

  it('still serves the methods the backend does support', async () => {
    const fake = createFakeTransport({ unsupportedMethods: WEB_UNSUPPORTED });
    fake.responses.set('getTableData', {
      columns: [],
      rows: [],
      page: 0,
      pageSize: 50,
    } as TableDataResult);

    const client = createBackendClient('web', fake.transport);
    await expect(
      client.getTableData({ context: CONTEXT, page: 0, pageSize: 50 }),
    ).resolves.toMatchObject({ page: 0 });

    // A capability gap is a property of one method, not of the whole
    // backend: the table read still works.
    expect(fake.calls.map((call) => call.method)).toEqual(['getTableData']);
  });
});

/* ------------------------------------------------------------------ */
/* The journey: a frontend write goes out and comes back.              */
/* ------------------------------------------------------------------ */

describe('journey: a table write travels out through the port and comes back', () => {
  it('previews, commits, and refuses the desktop-only view commands on the web side', async () => {
    const fake = createFakeTransport({
      unsupportedMethods: {
        listNamedViews: 'capability',
        saveNamedView: 'capability',
        deleteNamedView: 'capability',
      },
    });
    fake.responses.set('previewPendingChanges', PLAN);
    fake.responses.set('commitPendingChanges', {
      planId: 'plan-1',
      fingerprint: 'fp-1',
      statements: [
        { operation: 'insert', rowIdentity: {}, affectedRows: 1 },
        { operation: 'update', rowIdentity: { id: 7 }, affectedRows: 1 },
      ],
      affectedRows: 2,
    } satisfies CommitPendingChangesResponse);

    const client = createBackendClient('web', fake.transport);

    // 1. The frontend stages a change and asks for a plan.
    const plan = await client.previewPendingChanges({
      context: CONTEXT,
      changes: [UPDATE_CHANGE, INSERT_CHANGE],
    });
    expect(plan.planId).toBe('plan-1');
    expect(plan.inserts).toHaveLength(1);
    expect(fake.calls[0]?.method).toBe('previewPendingChanges');

    // 2. The frontend commits the exact plan it was handed, guarded by the
    //    fingerprint that plan carries. Nothing else travels.
    const response = await client.commitPendingChanges({
      dbSessionId: CONTEXT.dbSessionId,
      plan,
      fingerprint: plan.fingerprint,
    });
    expect(response.statements.map((statement) => statement.operation)).toEqual([
      'insert',
      'update',
    ]);
    expect(fake.calls[1]).toEqual({
      method: 'commitPendingChanges',
      payload: { dbSessionId: 'sess-1', plan, fingerprint: 'fp-1' },
    });

    // 3. The same session asks for its saved views. The web backend has no
    //    persistent store for them, so the call fails loudly instead of
    //    answering "no views".
    await expect(client.listNamedViews({})).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
    });

    // 4. The failed capability probe left the wire untouched: the transport
    //    saw the two write calls and nothing else.
    expect(fake.calls.map((call) => call.method)).toEqual([
      'previewPendingChanges',
      'commitPendingChanges',
    ]);
  });

  it('rejects a commit whose plan the backend refuses, without inventing a result', async () => {
    const rejection = { code: 'InvalidArgument', message: 'stale plan', requestId: 'r-1' };
    const transport: BackendTransport = {
      async call<K extends keyof MethodMap>(method: K, _payload: MethodMap[K]['request']) {
        if (method === 'commitPendingChanges') throw rejection;
        throw new Error(`unexpected method ${method}`);
      },
    };
    const client = createBackendClient('web', transport);

    await expect(
      client.commitPendingChanges({
        dbSessionId: 'sess-1',
        plan: PLAN,
        fingerprint: 'fp-1',
      }),
    ).rejects.toMatchObject({ code: 'InvalidArgument', requestId: 'r-1' });
  });

  it('keeps the manual session handle on the read, so a pinned read stays pinned', async () => {
    const fake = createFakeTransport();
    fake.responses.set('getTableData', {
      columns: [],
      rows: [],
      page: 0,
      pageSize: 50,
    } as TableDataResult);

    const client = createBackendClient('web', fake.transport);
    const handle: SessionHandle = { dbSessionId: 'sess-9' as Id, runtimeEpoch: 'epoch-9' as Id };
    await client.getTableData({
      context: CONTEXT,
      manualHandle: handle,
      page: 1,
      pageSize: 25,
    });

    expect(fake.calls[0]?.payload).toMatchObject({ manualHandle: handle, page: 1, pageSize: 25 });
  });
});
