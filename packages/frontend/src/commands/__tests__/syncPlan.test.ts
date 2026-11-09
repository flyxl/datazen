import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { SyncProfile } from '../sync';

const invoke = vi.hoisted(() => vi.fn());
import { createBackendClient, setBackendClient } from '@datazen/backend-client';
// form-split R6: the invoke wrappers ride the bound backend transport now
// (same binding pattern as file.test.ts — the mock keeps its invoke shape).
// W8 刀3e web 收口：命令层改走 dataSync 域门面方法——假门面用**真门面**
// （createBackendClient）架在假 transport 上，域方法全部存在且经
// guard → transport.call；令牌铸造走 identity 域的 issueSubmissionToken，
// 同样落在 invokeMock 队列里（期望序列里的 'issueSubmissionToken' 行）。
const fakeTransport = {
  call: (c: string, p?: unknown) => (p === undefined ? invoke(c) : invoke(c, p)),
};
setBackendClient('test', createBackendClient('test', fakeTransport as never) as never);
import { syncCommands } from '../sync';

let jobSequence = 0;

/** saveSyncProfile / cancelDataSync 的用户资产令牌（域写面在传输层折叠）。 */
const submissionToken = { idempotencyKey: 'idem-save-profile', expiresAt: 1_700_000_000_000 };

function queueDurableCompare(preview: unknown): void {
  const jobId = `sync-prepare-${++jobSequence}`;
  const queued = {
    jobId,
    kind: 'dataSyncPrepare',
    state: 'queued',
    effectOutcome: null,
    error: null,
    progress: { committed: 0, unknown: 0 },
  };
  invoke.mockResolvedValueOnce(queued);
  invoke.mockResolvedValueOnce({ ...queued, state: 'succeeded' });
  invoke.mockResolvedValueOnce(preview);
}

describe('Data Sync immutable plan IPC', () => {
  beforeEach(() => {
    invoke.mockReset();
    jobSequence = 0;
  });

  it('persists profiles through dedicated IPC commands', async () => {
    const profile = {
      version: 1,
      id: 'profile-1',
      name: 'nightly',
      sourceConnectionId: 'src',
      targetConnectionId: 'tgt',
      sourceDatabase: 'app',
      targetDatabase: 'app',
      tables: [
        {
          sourceTable: 'users',
          targetTable: 'users',
          enabled: false,
          sourceFilter: {
            filters: [],
            recordset: { orderBy: 'id', start: { value: '1' } },
          },
        },
      ],
      options: { insert: true, update: true, delete: false },
      createdAt: '2026-09-21T00:00:00.000Z',
      updatedAt: '2026-09-21T00:00:00.000Z',
    } satisfies SyncProfile;
    invoke.mockResolvedValueOnce([]); // getSyncProfiles
    invoke.mockResolvedValueOnce(submissionToken); // issueSubmissionToken (saveUserAsset)
    await syncCommands.getSyncProfiles();
    await syncCommands.saveSyncProfile(profile);
    await syncCommands.deleteSyncProfile(profile.id);
    expect(invoke).toHaveBeenNthCalledWith(1, 'getSyncProfiles', {});
    expect(invoke).toHaveBeenNthCalledWith(2, 'issueSubmissionToken', {
      operation: 'saveUserAsset',
      handle: null,
    });
    expect(invoke).toHaveBeenNthCalledWith(3, 'saveSyncProfile', {
      profile,
      submissionToken,
    });
    expect(invoke).toHaveBeenNthCalledWith(4, 'deleteSyncProfile', {
      profileId: profile.id,
    });
  });

  it('sends saved table mappings only when inspecting a loaded profile', async () => {
    invoke.mockResolvedValueOnce([]);
    const mappings = [
      {
        sourceTable: 'users',
        targetTable: 'archive_users',
        enabled: false,
        sourceFilter: {
          filters: [],
          recordset: { limit: 10 },
        },
      },
    ];
    await syncCommands.inspectDataSync(
      'source-session',
      'target-session',
      'app',
      'app',
      'public',
      'public',
      mappings,
    );
    expect(invoke).toHaveBeenCalledWith('inspectDataSync', {
      sourceDbSessionId: 'source-session',
      targetDbSessionId: 'target-session',
      sourceDatabase: 'app',
      targetDatabase: 'app',
      sourceSchema: 'public',
      targetSchema: 'public',
      tables: mappings,
    });
  });

  it('keeps replacement SQL and row payloads out of preview and execute IPC', async () => {
    queueDurableCompare({
      planId: 'opaque-plan',
      selectionRevision: 1,
      tables: [
        {
          sourceTable: 'users',
          targetTable: 'users',
          status: 'MATCHED',
          rows: [
            {
              operation: 'INSERT',
              key: [1],
              sourceRow: [1, 'alice'],
              targetRow: null,
              changedColumns: [],
              selected: true,
            },
          ],
        },
      ],
    });
    await syncCommands.compareDataSync('source-session', 'target-session', ['users']);

    invoke.mockResolvedValueOnce([]);
    await syncCommands.generateDataSyncSql(
      'source-session',
      'target-session',
      [
        {
          sourceTable: 'users',
          targetTable: 'users',
          status: 'MATCHED',
          rows: [
            {
              operation: 'INSERT',
              key: [1],
              sourceRow: [1, 'alice'],
              targetRow: null,
              changedColumns: [],
              selected: true,
            },
          ],
        },
      ],
      { insert: true, update: true, delete: false },
    );
    expect(invoke).toHaveBeenLastCalledWith('generateDataSyncSql', {
      planId: 'opaque-plan',
      selection: {
        revision: 1,
        rows: [{ sourceTable: 'users', targetTable: 'users', operation: 'INSERT', key: [1] }],
      },
      options: { insert: true, update: true, delete: false },
    });

    invoke.mockResolvedValueOnce({ applied: 1, rolledBack: false });
    await syncCommands.executeDataSync(
      'target-session',
      [
        {
          table: 'users',
          operation: 'INSERT',
          sql: 'DROP TABLE users',
          previewSql: 'DROP TABLE users',
          parameters: [1, 'attacker supplied row'],
          rowKey: [1],
        },
      ],
      'job-1',
      'app',
    );
    expect(invoke).toHaveBeenLastCalledWith('executeDataSync', {
      request: {
        planId: 'opaque-plan',
        selection: {
          revision: 1,
          rows: [{ sourceTable: 'users', targetTable: 'users', operation: 'INSERT', key: [1] }],
        },
        options: { insert: true, update: true, delete: false },
        jobId: 'job-1',
      },
    });
    expect(JSON.stringify(invoke.mock.calls.at(-1))).not.toContain('DROP TABLE');
    expect(JSON.stringify(invoke.mock.calls.at(-1))).not.toContain('attacker supplied row');
  });

  it('sends only structured per-table filters with the compare request', async () => {
    queueDurableCompare({ planId: 'filtered-plan', selectionRevision: 1, tables: [] });
    await syncCommands.compareDataSync(
      'source-session',
      'target-session',
      ['users'],
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      { insert: true, update: true, delete: false },
      {
        users: {
          filters: [{ column: 'status', operator: 'eq', value: 'active' }],
          logic: 'and',
        },
      },
    );
    expect(invoke).toHaveBeenCalledWith(
      'startDataSyncPrepareJob',
      expect.objectContaining({
        filters: {
          users: {
            filters: [{ column: 'status', operator: 'eq', value: 'active' }],
            logic: 'and',
          },
        },
      }),
    );
    expect(JSON.stringify(invoke.mock.calls[0])).not.toContain('WHERE');
  });

  it('sends a lossless primary-key recordset inside the reviewed source scope', async () => {
    queueDurableCompare({ planId: 'recordset-plan', selectionRevision: 1, tables: [] });
    await syncCommands.compareDataSync(
      'source-session',
      'target-session',
      ['users'],
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      { insert: true, update: true, delete: false },
      {
        users: {
          filters: [],
          recordset: {
            orderBy: 'id',
            start: { value: '9223372036854775807', inclusive: true },
            end: { value: '9223372036854775808', inclusive: false },
            limit: 100,
          },
        },
      },
    );
    expect(invoke).toHaveBeenCalledWith(
      'startDataSyncPrepareJob',
      expect.objectContaining({
        filters: {
          users: {
            filters: [],
            recordset: {
              orderBy: 'id',
              start: { value: '9223372036854775807', inclusive: true },
              end: { value: '9223372036854775808', inclusive: false },
              limit: 100,
            },
          },
        },
      }),
    );

    queueDurableCompare({ planId: 'tuple-plan', selectionRevision: 2, tables: [] });
    await syncCommands.compareDataSync(
      'source-session',
      'target-session',
      ['events'],
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      { insert: true, update: true, delete: false },
      {
        events: {
          filters: [],
          recordset: {
            tupleRange: {
              columns: ['tenant_id', 'id'],
              start: { values: ['9223372036854775808', '01'], inclusive: false },
              end: { values: ['9223372036854775808', '99'], inclusive: true },
            },
          },
        },
      },
    );
    expect(invoke).toHaveBeenCalledWith(
      'startDataSyncPrepareJob',
      expect.objectContaining({
        filters: {
          events: {
            filters: [],
            recordset: {
              tupleRange: {
                columns: ['tenant_id', 'id'],
                start: { values: ['9223372036854775808', '01'], inclusive: false },
                end: { values: ['9223372036854775808', '99'], inclusive: true },
              },
            },
          },
        },
      }),
    );
  });

  it('loads an opaque page and preserves selected keys without row payloads in compare', async () => {
    queueDurableCompare({
      contractVersion: 1,
      pageSize: 100,
      planId: 'paged-plan',
      selectionRevision: 1,
      tables: [
        {
          sourceTable: 'users',
          targetTable: 'users',
          status: 'MATCHED',
          insertCount: 2,
          updateCount: 1,
          deleteCount: 0,
          unchangedCount: 40,
          rowCount: 43,
          pageSize: 100,
          firstCursor: 'v1.0.signature',
          hasMore: false,
        },
      ],
    });
    const preview = await syncCommands.compareDataSync('source-session', 'target-session', [
      'users',
    ]);
    expect(preview.tables[0].rows).toBeUndefined();

    invoke.mockResolvedValueOnce({
      contractVersion: 1,
      planId: 'paged-plan',
      sourceTable: 'users',
      targetTable: 'users',
      cursor: 'v1.0.signature',
      nextCursor: null,
      hasMore: false,
      pageSize: 100,
      rows: [
        {
          operation: 'INSERT',
          key: [7],
          sourceRow: [7, 'alice'],
          targetRow: null,
          changedColumns: [],
          selected: true,
        },
      ],
    });
    await syncCommands.getDataSyncComparisonPage('v1.0.signature', 'users', 'users');
    expect(invoke).toHaveBeenLastCalledWith('get_data_sync_comparison_page', {
      request: {
        planId: 'paged-plan',
        sourceTable: 'users',
        targetTable: 'users',
        cursor: 'v1.0.signature',
        limit: 100,
      },
    });

    invoke.mockResolvedValueOnce([]);
    await syncCommands.generateDataSyncSql(
      'source-session',
      'target-session',
      [preview.tables[0]],
      { insert: true, update: true, delete: false },
      undefined,
      undefined,
      undefined,
      undefined,
      [{ sourceTable: 'users', targetTable: 'users', operation: 'INSERT', key: [7] }],
    );
    expect(invoke).toHaveBeenLastCalledWith(
      'generateDataSyncSql',
      expect.objectContaining({
        selection: {
          revision: 1,
          rows: [{ sourceTable: 'users', targetTable: 'users', operation: 'INSERT', key: [7] }],
        },
      }),
    );
  });

  it('sends a table scope and one exclusion without materializing page keys', async () => {
    queueDurableCompare({
      contractVersion: 1,
      planId: 'scoped-plan',
      selectionRevision: 7,
      pageSize: 100,
      tables: [
        {
          sourceTable: 'users',
          targetTable: 'users',
          status: 'MATCHED',
          insertCount: 5000,
          updateCount: 0,
          deleteCount: 0,
          unchangedCount: 0,
          rowCount: 5000,
          pageSize: 100,
          firstCursor: null,
          hasMore: true,
        },
      ],
    });
    await syncCommands.compareDataSync('source-session', 'target-session', ['users']);

    invoke.mockResolvedValueOnce([]);
    await syncCommands.generateDataSyncSql(
      'source-session',
      'target-session',
      [
        {
          sourceTable: 'users',
          targetTable: 'users',
          status: 'MATCHED',
          rowCount: 5000,
          insertCount: 5000,
        },
      ],
      { insert: true, update: true, delete: false },
      undefined,
      undefined,
      undefined,
      undefined,
      [],
      [
        {
          sourceTable: 'users',
          targetTable: 'users',
          selectionMode: 'all',
          operations: ['INSERT'],
          excludedRows: [{ operation: 'INSERT', key: [42] }],
        },
      ],
    );
    expect(invoke).toHaveBeenLastCalledWith('generateDataSyncSql', {
      planId: 'scoped-plan',
      selection: {
        revision: 7,
        rows: [],
        scopes: [
          {
            sourceTable: 'users',
            targetTable: 'users',
            selectionMode: 'all',
            operations: ['INSERT'],
            excludedRows: [{ operation: 'INSERT', key: [42] }],
          },
        ],
      },
      options: { insert: true, update: true, delete: false },
    });
    expect(JSON.stringify(invoke.mock.calls.at(-1))).not.toContain('5000');
  });

  it.each([
    { state: 'succeeded', effect: 'completed', expected: 'committed' },
    { state: 'failed', effect: 'partiallyApplied', expected: 'partially_applied' },
  ])(
    'preserves $effect through the durable apply lifecycle',
    async ({ state, effect, expected }) => {
      queueDurableCompare({ planId: 'apply-plan', selectionRevision: 4, tables: [] });
      await syncCommands.compareDataSync('source-session', 'target-session', ['users']);

      invoke.mockResolvedValueOnce({
        jobId: 'apply-job',
        kind: 'dataSyncApply',
        state: 'queued',
        effectOutcome: null,
        error: null,
        progress: { committed: 0, unknown: 0 },
      });
      invoke.mockResolvedValueOnce({
        jobId: 'apply-job',
        kind: 'dataSyncApply',
        state,
        effectOutcome: effect,
        error: null,
        progress: { committed: 2, unknown: 0 },
      });
      invoke.mockResolvedValueOnce({
        job: {
          jobId: 'apply-job',
          kind: 'dataSyncApply',
          state,
          effectOutcome: effect,
          error: null,
          progress: { committed: 2, unknown: 0 },
        },
        recoveryTargets: [],
        domainResults: [{ stageId: 'apply', counters: [{ code: 'committed', value: 2 }] }],
      });

      const result = await syncCommands.executeDataSyncJob(
        'source-session',
        'target-session',
        'apply-job',
        { insert: true, update: true, delete: false },
        [{ sourceTable: 'users', targetTable: 'users', operation: 'INSERT', key: [7] }],
      );

      expect(result.applied).toBe(2);
      expect(result.outcome).toBe(expected);
      expect(result.rolledBack).toBe(false);
      expect(Boolean(result.error)).toBe(state !== 'succeeded');
      expect(invoke).toHaveBeenCalledWith('startDataSyncApplyJob', {
        sourceDbSessionId: 'source-session',
        targetDbSessionId: 'target-session',
        request: {
          planId: 'apply-plan',
          selection: {
            revision: 4,
            rows: [{ sourceTable: 'users', targetTable: 'users', operation: 'INSERT', key: [7] }],
          },
          options: { insert: true, update: true, delete: false },
          jobId: 'apply-job',
        },
      });
      expect(invoke).not.toHaveBeenCalledWith('generateDataSyncSql', expect.anything());
    },
  );
});

describe('Data Sync command-layer desktop-kept surfaces', () => {
  beforeEach(() => invoke.mockReset());

  it('generateDataSyncSql without an active comparison plan rejects with the guidance error', async () => {
    // 模块级比较计划被前序用例污染——resetModules 取全新模块钉「无计划」分支。
    vi.resetModules();
    const { syncCommands: fresh } = await import('../sync');
    await expect(
      fresh.generateDataSyncSql('source-session', 'target-session', [], {
        insert: true,
        update: false,
        delete: false,
      }),
    ).rejects.toThrow('data sync comparison plan is missing; compare again');
    expect(invoke).not.toHaveBeenCalled();
  });

  it('applyDataSync stays on the legacy rail with the desktop parameter bag', async () => {
    invoke.mockResolvedValueOnce({
      outcome: 'unknown',
      applied: 0,
      rolledBack: false,
      error: null,
    });
    await syncCommands.applyDataSync(
      'source-session',
      'target-session',
      ['users'],
      'apply-job',
      'src-db',
      'tgt-db',
      'public',
      'public',
      { insert: true, update: true, delete: false },
    );
    expect(invoke).toHaveBeenCalledWith('apply_data_sync', {
      sourceDbSessionId: 'source-session',
      targetDbSessionId: 'target-session',
      tables: ['users'],
      jobId: 'apply-job',
      sourceDatabase: 'src-db',
      targetDatabase: 'tgt-db',
      sourceSchema: 'public',
      targetSchema: 'public',
      options: { insert: true, update: true, delete: false },
    });
  });

  it('cancelDataSync folds a saveUserAsset token over the job id', async () => {
    invoke.mockResolvedValueOnce({ idempotencyKey: 'k', expiresAt: '2026-01-01T00:00:00Z' });
    invoke.mockResolvedValueOnce(true);

    await expect(syncCommands.cancelDataSync('job-9')).resolves.toBe(true);

    expect(invoke).toHaveBeenNthCalledWith(1, 'issueSubmissionToken', {
      operation: 'saveUserAsset',
      handle: null,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, 'cancelDataSync', {
      jobId: 'job-9',
      submissionToken: { idempotencyKey: 'k', expiresAt: '2026-01-01T00:00:00Z' },
    });
  });

  it('listDataSyncJobs and verifyDataSyncRecovery ride the domain read faces verbatim', async () => {
    invoke.mockResolvedValueOnce([{ job: { jobId: 'j-1' } }]);
    await expect(syncCommands.listDataSyncJobs()).resolves.toEqual([{ job: { jobId: 'j-1' } }]);
    expect(invoke).toHaveBeenLastCalledWith('listDataSyncJobs', {});

    const recovery = {
      jobId: 'j-1',
      sourceDbSessionId: 's-1',
      targetDbSessionId: 's-2',
      sourceDatabase: 'db',
      targetDatabase: 'db',
    };
    invoke.mockResolvedValueOnce({ job: { jobId: 'j-1' } });
    await syncCommands.verifyDataSyncRecovery(recovery);
    expect(invoke).toHaveBeenLastCalledWith('verifyDataSyncRecovery', { request: recovery });
  });

  it('compareDataSync and the comparison page reject without an active plan (fresh module)', async () => {
    vi.resetModules();
    // resetModules 连 backend-client 的绑定注册表一起重置——先重绑再取新模块。
    const bc = await import('@datazen/backend-client');
    bc.setBackendClient(
      'test',
      bc.createBackendClient('test', {
        call: (c: string, p?: unknown) => (p === undefined ? invoke(c) : invoke(c, p)),
      } as never) as never,
    );
    const { syncCommands: fresh } = await import('../sync');
    // compareDataSync 先比较后存计划（无前置守卫）；分页读才有 no-plan 拒绝。
    await expect(fresh.getDataSyncComparisonPage(null, 'users', 'users')).rejects.toThrow(
      'data sync comparison plan is missing; compare again',
    );
  });
});
