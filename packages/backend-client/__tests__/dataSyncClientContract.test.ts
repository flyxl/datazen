import { describe, expect, it } from 'vitest';

import { createBackendClient, createBackendGuard, toTimestamp } from '../src/index';
import {
  createDataSyncMethods,
  type DataSyncClient,
  type DataSyncMethodMap,
} from '../src/dataSyncClient';
import type {
  DataSyncCompareRequest,
  DataSyncInspectRequest,
  DataSyncRecoveryRequest,
  DataSyncRevalidateRequest,
  DataSyncRunRequest,
  DataSyncSelection,
  SyncOptions,
  SyncProfile,
} from '../src/types/dataSync';
import { createFakeTransport } from './client.test';

/**
 * The DataSync domain contract table (W8 刀3e web 收口, contract §6.16):
 * method × request shape × response shape, one row per method — all 22 of
 * them, because this table is the domain's coverage floor.
 *
 * The runner asserts the three contract facts that matter on the wire:
 * 1. the **method name** the transport receives (the MethodMap key);
 * 2. the **request bag** (desktop-IPC named-argument shape; token-folded on
 *    the two token-carrying writes, `{ profile }`-wrapped on save);
 * 3. the **response passthrough** (the facade adds nothing, drops nothing —
 *    HTTP-only envelopes like cancel's `{ cancelled }` are unwrapped by the
 *    web route decoders, not here).
 *
 * The client is built directly over the fake transport: the contract being
 * pinned is the domain slice. The facade spread is composition, pinned by
 * `client.test.ts`; the guard is the real one so a declared gap would fail
 * with the §13 code exactly as in production.
 */

interface ContractRow {
  /** Facade method exercised. */
  call: (client: DataSyncClient) => Promise<unknown>;
  /** Method name the transport must receive. */
  method: keyof DataSyncMethodMap;
  /** The exact payload the transport must receive. */
  payload: unknown;
  /** Canned response served for `method` (also asserted as the resolution). */
  response: unknown;
}

function mustNarrow<T>(narrowed: T | undefined): T {
  if (narrowed === undefined) throw new Error('fixture did not narrow');
  return narrowed;
}

const token = {
  idempotencyKey: 'idem-sync-1',
  expiresAt: mustNarrow(toTimestamp(1_700_000_000_000)),
};

const syncOptions: SyncOptions = { insert: true, update: true, delete: false };

const profile: SyncProfile = {
  version: 1,
  id: 'sp-1',
  name: 'nightly',
  sourceConnectionId: 'conn-src',
  targetConnectionId: 'conn-tgt',
  tables: [{ sourceTable: 'users', targetTable: 'users', enabled: true }],
  options: syncOptions,
  createdAt: '2026-11-08T00:00:00Z',
  updatedAt: '2026-11-08T00:00:00Z',
};

const task = {
  id: 'task-1',
  sourceConnectionId: 'conn-src',
  targetConnectionId: 'conn-tgt',
  tables: ['users'],
  completedTables: [],
  currentTable: null,
  currentTableOffset: 0,
  sourceRowCounts: { users: 42 },
  strategy: 'full',
  status: 'paused',
  errorMessage: null,
  createdAt: '2026-11-08T00:00:00Z',
  updatedAt: '2026-11-08T00:00:00Z',
  resumeState: 'unknown',
};

const runRecord = {
  id: 'run-1',
  operation: 'dataSync',
  status: 'failed',
  outcome: 'partially_applied',
  phase: 'execute',
  profileId: 'sp-1',
  profileRevision: '3',
  sourceConnectionId: 'conn-src',
  targetConnectionId: 'conn-tgt',
  startedAt: '2026-11-08T00:00:00Z',
  finishedAt: '2026-11-08T01:00:00Z',
  selectedCount: 7,
  committedCount: 2,
  failedCount: 5,
  conflictCount: 0,
  cancelled: false,
  rollbackOutcome: 'notRequired',
  errorSummary: 'commit outcome unknown',
};

const jobView = {
  jobId: 'job-1',
  kind: 'dataSyncPrepare',
  state: 'succeeded',
  stage: null,
  executionIds: [],
  artifactIds: [],
  createdAt: 1_700_000_000_000,
  updatedAt: 1_700_000_000_000,
  effectOutcome: 'completed',
  cancelRequested: false,
  pendingVerificationReason: null,
  error: null,
  progress: { read: 0, converted: 0, attempted: 0, committed: 2, unknown: 0 },
};

const jobDetails = {
  job: jobView,
  stateVersion: 2,
  commitBoundaries: [],
  domainResults: [
    {
      stageId: 'apply',
      resultCode: 'ok',
      outcomeCode: 'committed',
      counters: [{ code: 'committed', value: 2 }],
      items: [],
      artifactIds: [],
    },
  ],
  recoveryTargets: [],
};

const preview = {
  contractVersion: 1,
  planId: 'plan-1',
  selectionRevision: 3,
  pageSize: 100,
  tables: [
    {
      sourceTable: 'users',
      targetTable: 'users',
      status: 'MATCHED',
      insertCount: 1,
      updateCount: 0,
      deleteCount: 0,
      rowCount: 1,
      pageSize: 100,
      firstCursor: null,
      hasMore: false,
    },
  ],
};

const selection: DataSyncSelection = {
  revision: 3,
  rows: [{ sourceTable: 'users', targetTable: 'users', operation: 'INSERT', key: [7] }],
};

const runRequest: DataSyncRunRequest = {
  planId: 'plan-1',
  selection,
  options: syncOptions,
  jobId: 'job-2',
};

const compareRequest: DataSyncCompareRequest = {
  sourceDbSessionId: 'sess-1',
  targetDbSessionId: 'sess-2',
  tables: ['users'],
  jobId: 'data-sync-1',
  sourceDatabase: 'app',
  targetDatabase: 'app',
  sourceSchema: null,
  targetSchema: null,
  options: syncOptions,
  filters: {
    users: { filters: [{ column: 'status', operator: 'eq', value: 'active' }], logic: 'and' },
  },
};

const inspectRequest: DataSyncInspectRequest = {
  sourceDbSessionId: 'sess-1',
  targetDbSessionId: 'sess-2',
  sourceDatabase: 'app',
  targetDatabase: 'app',
  sourceSchema: 'public',
  targetSchema: 'public',
  tables: [{ sourceTable: 'users', targetTable: 'users', enabled: true }],
};

const revalidateRequest: DataSyncRevalidateRequest = {
  sourceDbSessionId: 'sess-1',
  targetDbSessionId: 'sess-2',
  tables: ['users'],
  sourceDatabase: 'app',
  targetDatabase: 'app',
  sourceSchema: null,
  targetSchema: null,
};

const recoveryRequest: DataSyncRecoveryRequest = {
  jobId: 'job-1',
  sourceDbSessionId: 'sess-1',
  targetDbSessionId: 'sess-2',
  sourceDatabase: 'app',
  targetDatabase: 'app',
  sourceSchema: 'public',
  targetSchema: 'public',
};

function runContractTable(rows: ContractRow[]): void {
  describe('DataSync domain contract table', () => {
    for (const row of rows) {
      it(`${String(row.method)}: name, bag and response are the contract`, async () => {
        const fake = createFakeTransport();
        fake.responses.set(row.method, row.response);
        const client = createDataSyncMethods(fake.transport, createBackendGuard('desktop', {}));

        await expect(row.call(client)).resolves.toEqual(row.response);
        expect(fake.calls).toHaveLength(1);
        expect(fake.calls[0]?.method).toBe(row.method);
        expect(fake.calls[0]?.payload).toEqual(row.payload);
      });
    }
  });
}

runContractTable([
  // ── assets & read faces (刀3e-3) ────────────────────────────────────────
  {
    call: (c) => c.getSyncTasks(),
    method: 'getSyncTasks',
    payload: {},
    response: [task],
  },
  {
    call: (c) => c.deleteSyncTask({ taskId: 'task-1' }),
    method: 'deleteSyncTask',
    payload: { taskId: 'task-1' },
    response: undefined,
  },
  {
    call: (c) => c.getSyncProfiles(),
    method: 'getSyncProfiles',
    payload: {},
    response: [profile],
  },
  {
    call: (c) => c.saveSyncProfile({ profile }, token),
    method: 'saveSyncProfile',
    payload: { profile, submissionToken: token },
    response: undefined,
  },
  {
    call: (c) => c.deleteSyncProfile({ profileId: 'sp-1' }),
    method: 'deleteSyncProfile',
    payload: { profileId: 'sp-1' },
    response: undefined,
  },
  {
    call: (c) =>
      c.classifyDataSyncPair({ sourceDatabaseType: 'postgresql', targetDatabaseType: 'mysql' }),
    method: 'classifyDataSyncPair',
    payload: { sourceDatabaseType: 'postgresql', targetDatabaseType: 'mysql' },
    response: { path: 'direct', supported: true, family: 'mysql', reason: null },
  },
  {
    call: (c) =>
      c.listMigrationRuns({
        filter: { operation: 'dataSync', status: 'failed' },
        offset: 25,
        limit: 25,
      }),
    method: 'listMigrationRuns',
    payload: { filter: { operation: 'dataSync', status: 'failed' }, offset: 25, limit: 25 },
    response: { items: [runRecord], total: 1, offset: 25, limit: 25 },
  },
  {
    call: (c) => c.getMigrationRun({ runId: 'run-1' }),
    method: 'getMigrationRun',
    payload: { runId: 'run-1' },
    response: runRecord,
  },
  {
    call: (c) => c.listDataSyncJobs(),
    method: 'listDataSyncJobs',
    payload: {},
    response: [jobDetails],
  },
  {
    call: (c) => c.getDataSyncJob({ jobId: 'job-1' }),
    method: 'getDataSyncJob',
    payload: { jobId: 'job-1' },
    response: jobView,
  },
  {
    call: (c) => c.getDataSyncJobDetails({ jobId: 'job-1' }),
    method: 'getDataSyncJobDetails',
    payload: { jobId: 'job-1' },
    response: jobDetails,
  },
  {
    call: (c) => c.getDataSyncJobPreview({ jobId: 'job-1' }),
    method: 'getDataSyncJobPreview',
    payload: { jobId: 'job-1' },
    response: preview,
  },
  {
    // The `{ cancelled }` envelope is an HTTP-route decoder concern; the
    // domain shape is the desktop IPC's bare boolean.
    call: (c) => c.cancelDataSync({ jobId: 'job-1' }, token),
    method: 'cancelDataSync',
    payload: { jobId: 'job-1', submissionToken: token },
    response: true,
  },

  // ── execution faces (刀3e-4b; token-free by ruling) ─────────────────────
  {
    call: (c) => c.inspectDataSync(inspectRequest),
    method: 'inspectDataSync',
    payload: inspectRequest,
    response: [
      {
        sourceTable: 'users',
        targetTable: 'users',
        status: 'MATCHED',
        incompatibleReason: null,
        columns: ['id', 'name'],
        columnTypes: ['integer', 'text'],
        primaryKeys: ['id'],
        unchangedCount: 41,
        rows: [],
        warnings: [],
      },
    ],
  },
  {
    call: (c) => c.revalidateDataSync(revalidateRequest),
    method: 'revalidateDataSync',
    payload: revalidateRequest,
    response: {
      ok: false,
      staleTables: [
        { sourceTable: 'users', targetTable: 'users', status: 'INCOMPATIBLE', reason: 'pk drift' },
      ],
    },
  },
  {
    call: (c) => c.compareDataSync(compareRequest),
    method: 'compareDataSync',
    payload: compareRequest,
    response: preview,
  },
  {
    call: (c) => c.startDataSyncPrepareJob(compareRequest),
    method: 'startDataSyncPrepareJob',
    payload: compareRequest,
    response: { ...jobView, state: 'queued' },
  },
  {
    call: (c) =>
      c.startDataSyncApplyJob({
        sourceDbSessionId: 'sess-1',
        targetDbSessionId: 'sess-2',
        request: runRequest,
      }),
    method: 'startDataSyncApplyJob',
    payload: { sourceDbSessionId: 'sess-1', targetDbSessionId: 'sess-2', request: runRequest },
    response: { ...jobView, jobId: 'job-2', kind: 'dataSyncApply', state: 'queued' },
  },
  {
    call: (c) => c.executeDataSync({ request: runRequest, profile: { id: 'sp-1', revision: '3' } }),
    method: 'executeDataSync',
    payload: { request: runRequest, profile: { id: 'sp-1', revision: '3' } },
    response: {
      applied: 2,
      rolledBack: false,
      affectedRows: 2,
      outcome: 'partially_applied',
      error: 'Some batches committed before execution stopped.',
    },
  },
  {
    call: (c) => c.generateDataSyncSql({ planId: 'plan-1', selection, options: syncOptions }),
    method: 'generateDataSyncSql',
    payload: { planId: 'plan-1', selection, options: syncOptions },
    response: [
      {
        table: 'users',
        operation: 'INSERT',
        sql: 'INSERT INTO users (id, name) VALUES ($1, $2)',
        previewSql: 'INSERT INTO users …',
        parameters: [7, 'alice'],
        rowKey: [7],
      },
    ],
  },
  {
    call: (c) => c.checkSyncConflicts({ taskId: 'task-1' }),
    method: 'checkSyncConflicts',
    payload: { taskId: 'task-1' },
    response: {
      hasConflicts: true,
      conflicts: [{ table: 'users', originalRows: 42, currentRows: 50 }],
    },
  },
  {
    call: (c) => c.verifyDataSyncRecovery(recoveryRequest),
    method: 'verifyDataSyncRecovery',
    payload: { request: recoveryRequest },
    response: jobDetails,
  },
]);

describe('dataSync domain optional-argument defaults', () => {
  it('listMigrationRuns without arguments sends the empty bag (no filter, no paging)', async () => {
    const fake = createFakeTransport();
    fake.responses.set('listMigrationRuns', { items: [], total: 0, offset: 0, limit: 50 });
    const client = createBackendClient('desktop', fake.transport);
    await expect(client.listMigrationRuns()).resolves.toEqual({
      items: [],
      total: 0,
      offset: 0,
      limit: 50,
    });
    expect(fake.calls[0]?.method).toBe('listMigrationRuns');
    expect(fake.calls[0]?.payload).toEqual({});
  });
});
