import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

import {
  dialectSupportsTransactionalDdl,
  exportPlanSql,
  planHasDestructive,
  rollbackCompletenessCounts,
  schemaDiffCommands,
  type SchemaDiffPlan,
  type SchemaDiffPrepareEnvelope,
} from '../schemaDiff';
import type { TableSchemaDiff } from '../../types';

function samplePlan(overrides: Partial<SchemaDiffPlan> = {}): SchemaDiffPlan {
  return {
    table: 'users',
    tables: ['users'],
    sourceDialect: 'postgres',
    targetDialect: 'mysql',
    sameDialect: false,
    statements: [],
    warnings: [],
    requirements: [],
    rollbackCompleteness: { complete: true, missing: [] },
    ...overrides,
  };
}

describe('schema diff pure helpers', () => {
  it('planHasDestructive flags destructive or rewrite statements only', () => {
    expect(planHasDestructive(samplePlan())).toBe(false);
    expect(
      planHasDestructive(
        samplePlan({
          statements: [
            { sql: 'CREATE TABLE t()', risk: 'additive', rollbackSql: null, summary: '' },
          ],
        }),
      ),
    ).toBe(false);
    expect(
      planHasDestructive(
        samplePlan({
          statements: [
            { sql: 'DROP TABLE t', risk: 'destructive', rollbackSql: null, summary: '' },
          ],
        }),
      ),
    ).toBe(true);
    expect(
      planHasDestructive(
        samplePlan({
          statements: [{ sql: 'ALTER TABLE t', risk: 'rewrite', rollbackSql: null, summary: '' }],
        }),
      ),
    ).toBe(true);
  });

  it('dialectSupportsTransactionalDdl covers postgres and sqlite only', () => {
    expect(dialectSupportsTransactionalDdl('postgresql')).toBe(true);
    expect(dialectSupportsTransactionalDdl('postgres')).toBe(true);
    expect(dialectSupportsTransactionalDdl('SQLite')).toBe(true);
    expect(dialectSupportsTransactionalDdl('mysql')).toBe(false);
    expect(dialectSupportsTransactionalDdl('mariadb')).toBe(false);
  });

  it('rollbackCompletenessCounts derives complete and missing counts', () => {
    const plan = samplePlan({
      statements: [
        { sql: 'A', risk: 'additive', rollbackSql: 'RA', summary: 'a' },
        { sql: 'B', risk: 'destructive', rollbackSql: null, summary: 'b' },
      ],
      rollbackCompleteness: { complete: false, missing: ['b'] },
    });
    expect(rollbackCompletenessCounts(plan)).toEqual({ complete: 1, missing: 1 });
  });

  it('exportPlanSql renders header and semicolon-terminated statements', () => {
    const plan = samplePlan({
      tables: ['users', 'orders'],
      sourceDialect: 'PostgreSQL',
      targetDialect: 'MySQL',
      statements: [
        {
          sql: 'ALTER TABLE users ADD COLUMN c int',
          risk: 'additive',
          rollbackSql: null,
          summary: '',
        },
        { sql: 'DROP INDEX idx', risk: 'destructive', rollbackSql: null, summary: '' },
      ],
    });
    const sql = exportPlanSql(plan);
    expect(sql).toContain('-- Schema Diff Deploy plan');
    expect(sql).toContain('-- tables: users, orders');
    expect(sql).toContain('-- PostgreSQL → MySQL');
    expect(sql).toContain('ALTER TABLE users ADD COLUMN c int;');
    expect(sql).toContain('DROP INDEX idx;');
    expect(sql.endsWith(';')).toBe(true);
  });
});

const prepareEnvelope = (plan: unknown): SchemaDiffPrepareEnvelope => ({
  plan: plan as SchemaDiffPlan,
  planId: (plan as { planId?: string }).planId ?? 'plan-1',
  selectionRevision: 1,
  planVersion: 1,
  handlerVersion: 1,
  checkpointVersion: 1,
  expiresAt: '2999-01-01T00:00:00Z',
  recoveryPolicy: 'readOnlyVerify',
});

const emptyProgress = { read: 0, converted: 0, attempted: 0, committed: 0, unknown: 0 };

function mockAcceptedPrepare(envelope: SchemaDiffPrepareEnvelope): void {
  invokeMock
    .mockResolvedValueOnce({
      jobId: 'job-prepare-1',
      kind: 'schemaDiffPrepare',
      state: 'queued',
      progress: emptyProgress,
    })
    .mockResolvedValueOnce({
      details: {
        job: {
          jobId: 'job-prepare-1',
          kind: 'schemaDiffPrepare',
          state: 'succeeded',
          progress: emptyProgress,
          error: null,
          artifactIds: [],
        },
        stateVersion: 2,
        planId: envelope.planId,
        planDigest: null,
        selectionRevision: envelope.selectionRevision,
        commitBoundaries: [],
        recovery: null,
        domainResults: [],
        recoveryTargets: [],
        targetBeforeFingerprint: null,
        recoveryPolicy: null,
      },
      prepared: envelope,
      planUnavailableAfterRestart: false,
      deployResult: null,
    });
}

function mockAcceptedApply(
  result: unknown = {
    status: 'committed',
    executedCount: 1,
    statementCount: 1,
    errors: [],
    statementResults: [],
  },
): void {
  invokeMock
    .mockResolvedValueOnce({
      jobId: 'job-apply-1',
      kind: 'schemaDiffApply',
      state: 'queued',
      progress: emptyProgress,
    })
    .mockResolvedValueOnce({
      details: {
        job: {
          jobId: 'job-apply-1',
          kind: 'schemaDiffApply',
          state: 'succeeded',
          progress: emptyProgress,
          error: null,
          artifactIds: [],
        },
        stateVersion: 2,
        planId: null,
        planDigest: null,
        selectionRevision: null,
        commitBoundaries: [],
        recovery: null,
        domainResults: [],
        recoveryTargets: [],
        targetBeforeFingerprint: null,
        recoveryPolicy: null,
      },
      prepared: null,
      planUnavailableAfterRestart: false,
      deployResult: result,
    });
}

describe('schemaDiffCommands wrappers', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  it('compareTableSchemas forwards endpoint-specific table names and schemas', async () => {
    const diff = { table: 'users' } as unknown as TableSchemaDiff;
    invokeMock.mockResolvedValueOnce(diff);
    await expect(
      schemaDiffCommands.compareTableSchemas(
        'src-1',
        'tgt-1',
        'source.users',
        'target.users',
        'source',
        'target',
      ),
    ).resolves.toBe(diff);
    expect(invokeMock).toHaveBeenCalledWith('compare_table_schemas', {
      sourceDbSessionId: 'src-1',
      targetDbSessionId: 'tgt-1',
      sourceTableName: 'source.users',
      targetTableName: 'target.users',
      sourceSchema: 'source',
      targetSchema: 'target',
    });
  });

  it('persists only reusable Schema Diff profile configuration through IPC', async () => {
    const profile = {
      version: 1 as const,
      id: 'profile-1',
      name: 'Production schema',
      sourceConnectionId: 'source',
      targetConnectionId: 'target',
      sourceDatabase: 'app',
      targetDatabase: 'app',
      sourceSchema: 'public',
      targetSchema: 'public',
      tables: ['public.users'],
      allowDestructive: false,
      includeIndexes: true,
      requireRollback: false,
      typeOverrides: [],
      createdAt: '2026-09-21T00:00:00.000Z',
      updatedAt: '2026-09-21T00:00:00.000Z',
    };
    await schemaDiffCommands.getProfiles();
    await schemaDiffCommands.saveProfile(profile);
    await schemaDiffCommands.deleteProfile(profile.id);
    expect(invokeMock).toHaveBeenNthCalledWith(1, 'get_schema_diff_profiles');
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'save_schema_diff_profile', { profile });
    expect(invokeMock).toHaveBeenNthCalledWith(3, 'delete_schema_diff_profile', {
      profileId: profile.id,
    });
  });

  it('preparePlan normalizes IPC requirement tags into plan requirements', async () => {
    mockAcceptedPrepare(
      prepareEnvelope({
        ...samplePlan(),
        requirements: [
          {
            backfill: {
              table: 'users',
              column: 'status',
              reason: 'Populate existing rows before enforcing NOT NULL.',
            },
          },
          {
            unsupported: {
              operation: 'users.meta',
              reason: 'Operation is not supported',
            },
          },
        ],
      }),
    );
    await expect(
      schemaDiffCommands.preparePlan({
        sourceDbSessionId: 'src-2',
        targetDbSessionId: 'tgt-2',
        tableNames: ['a'],
        allowDestructive: false,
      }),
    ).resolves.toMatchObject({
      plan: {
        requirements: [
          {
            kind: 'Backfill',
            table: 'users',
            column: 'status',
            reason: 'Populate existing rows before enforcing NOT NULL.',
          },
          {
            kind: 'Unsupported',
            table: 'users',
            column: 'meta',
            reason: 'Operation is not supported',
          },
        ],
      },
    });
  });

  it('preparePlan forwards plan options with optional includeIndexes omitted key intact', async () => {
    mockAcceptedPrepare(prepareEnvelope(samplePlan()));
    await expect(
      schemaDiffCommands.preparePlan({
        sourceDbSessionId: 'src-2',
        targetDbSessionId: 'tgt-2',
        tableNames: ['a', 'b'],
        allowDestructive: true,
        includeIndexes: true,
      }),
    ).resolves.toMatchObject({ plan: { table: 'users' } });
    expect(invokeMock).toHaveBeenCalledWith('prepare_schema_diff_plan', {
      sourceDbSessionId: 'src-2',
      targetDbSessionId: 'tgt-2',
      tableNames: ['a', 'b'],
      allowDestructive: true,
      includeIndexes: true,
    });
  });

  it('preparePlan forwards typeOverrides and preserves typeSuggestions', async () => {
    const planWithSug = samplePlan({
      typeSuggestions: [
        {
          table: 'demo_customers',
          column: 'region',
          sourceType: 'text',
          suggestedType: 'VARCHAR(255)',
          currentType: 'VARCHAR(255)',
          reason: 'Indexed column',
          isKeyOrIndexed: true,
        },
      ],
    });
    mockAcceptedPrepare(prepareEnvelope(planWithSug));
    const overrides = [{ table: 'demo_customers', column: 'region', targetType: 'VARCHAR(64)' }];
    const res = await schemaDiffCommands.preparePlan({
      sourceDbSessionId: 'src-1',
      targetDbSessionId: 'tgt-1',
      tableNames: ['demo_customers'],
      allowDestructive: false,
      typeOverrides: overrides,
    });
    expect(res.plan.typeSuggestions).toHaveLength(1);
    expect(invokeMock).toHaveBeenCalledWith('prepare_schema_diff_plan', {
      sourceDbSessionId: 'src-1',
      targetDbSessionId: 'tgt-1',
      tableNames: ['demo_customers'],
      allowDestructive: false,
      includeIndexes: undefined,
      typeOverrides: overrides,
    });
  });

  it('prepareUnifiedPlan sends exact mixed object identities with the table scope', async () => {
    mockAcceptedPrepare(prepareEnvelope(samplePlan({ planId: 'unified-review-1' })));
    const sourceObjects = [
      {
        kind: 'function' as const,
        schema: 'public',
        name: 'calculate_total',
        signature: 'integer, numeric',
      },
      {
        kind: 'trigger' as const,
        schema: 'public',
        name: 'audit_row',
        targetSchema: 'public',
        targetName: 'orders',
      },
    ];
    const targetObjects = [
      {
        kind: 'function' as const,
        schema: 'app',
        name: 'calculate_total',
        signature: 'integer, numeric',
      },
    ];

    await expect(
      schemaDiffCommands.prepareUnifiedPlan({
        sourceDbSessionId: 'source-session',
        targetDbSessionId: 'target-session',
        tableNames: ['public.orders'],
        targetTableNames: ['app.orders'],
        sourceSchema: 'public',
        targetSchema: 'app',
        allowDestructive: false,
        includeIndexes: true,
        typeOverrides: [{ table: 'orders', column: 'amount', targetType: 'DECIMAL(12,2)' }],
        sourceObjects,
        targetObjects,
      }),
    ).resolves.toMatchObject({ planId: 'unified-review-1' });

    expect(invokeMock).toHaveBeenCalledWith('prepare_schema_unified_plan', {
      sourceDbSessionId: 'source-session',
      targetDbSessionId: 'target-session',
      tableNames: ['public.orders'],
      targetTableNames: ['app.orders'],
      targetOnlyTableNames: undefined,
      sourceSchema: 'public',
      targetSchema: 'app',
      allowDestructive: false,
      includeIndexes: true,
      typeOverrides: [{ table: 'orders', column: 'amount', targetType: 'DECIMAL(12,2)' }],
      sourceObjects: [
        {
          kind: 'function',
          schema: 'public',
          name: 'calculate_total',
          signature: 'integer, numeric',
          targetSchema: null,
          targetName: null,
        },
        {
          kind: 'trigger',
          schema: 'public',
          name: 'audit_row',
          signature: null,
          targetSchema: 'public',
          targetName: 'orders',
        },
      ],
      targetObjects: [
        {
          kind: 'function',
          schema: 'app',
          name: 'calculate_total',
          signature: 'integer, numeric',
          targetSchema: null,
          targetName: null,
        },
      ],
    });
  });

  it('forwards explicit target-only selectors without changing source selectors', async () => {
    mockAcceptedPrepare(prepareEnvelope(samplePlan({ tables: ['users', 'archive'] })));
    await schemaDiffCommands.preparePlan({
      sourceDbSessionId: 'src-target-picker',
      targetDbSessionId: 'tgt-target-picker',
      tableNames: ['users'],
      targetOnlyTableNames: ['archive'],
      allowDestructive: false,
    });

    expect(invokeMock).toHaveBeenCalledWith('prepare_schema_diff_plan', {
      sourceDbSessionId: 'src-target-picker',
      targetDbSessionId: 'tgt-target-picker',
      tableNames: ['users'],
      allowDestructive: false,
      includeIndexes: undefined,
      typeOverrides: undefined,
      targetOnlyTableNames: ['archive'],
    });
  });

  it('prepareViewPlan forwards qualified selectors and normalizes requirements', async () => {
    invokeMock.mockResolvedValueOnce({
      ...samplePlan({ table: 'public.active_users', tables: ['public.active_users'] }),
      requirements: [],
    });
    await expect(
      schemaDiffCommands.prepareViewPlan({
        sourceDbSessionId: 'src-view',
        targetDbSessionId: 'tgt-view',
        objectNames: ['public.active_users'],
        allowDestructive: false,
      }),
    ).resolves.toMatchObject({ tables: ['public.active_users'] });
    expect(invokeMock).toHaveBeenCalledWith('prepare_schema_view_plan', {
      sourceDbSessionId: 'src-view',
      targetDbSessionId: 'tgt-view',
      objectNames: ['public.active_users'],
      allowDestructive: false,
    });
  });

  it('[tester] prepareRoutineTriggerPlan preserves a qualified object identity and destructive choice', async () => {
    invokeMock.mockResolvedValueOnce(
      samplePlan({
        table: 'function:public:lookup:integer',
        tables: ['function:public:lookup:integer'],
      }),
    );
    await expect(
      schemaDiffCommands.prepareRoutineTriggerPlan({
        sourceDbSessionId: 'src-routine',
        targetDbSessionId: 'tgt-routine',
        kind: 'function',
        objectNames: ['public.lookup(integer)'],
        allowDestructive: true,
      }),
    ).resolves.toMatchObject({ tables: ['function:public:lookup:integer'] });
    expect(invokeMock).toHaveBeenCalledWith('prepare_schema_routine_trigger_plan', {
      sourceDbSessionId: 'src-routine',
      targetDbSessionId: 'tgt-routine',
      kind: 'function',
      objectNames: ['public.lookup(integer)'],
      allowDestructive: true,
    });
  });

  it('[tester] prepareSequencePlan forwards only qualified selectors and destructive choice', async () => {
    invokeMock.mockResolvedValueOnce(
      samplePlan({
        table: 'sequence:public:orders_id_seq',
        tables: ['sequence:public:orders_id_seq'],
      }),
    );
    await expect(
      schemaDiffCommands.prepareSequencePlan({
        sourceDbSessionId: 'src-sequence',
        targetDbSessionId: 'tgt-sequence',
        objectNames: ['public.orders_id_seq'],
        allowDestructive: true,
      }),
    ).resolves.toMatchObject({ tables: ['sequence:public:orders_id_seq'] });
    expect(invokeMock).toHaveBeenCalledWith('prepare_schema_sequence_plan', {
      sourceDbSessionId: 'src-sequence',
      targetDbSessionId: 'tgt-sequence',
      objectNames: ['public.orders_id_seq'],
      allowDestructive: true,
    });
  });

  it('prepareTypePlan forwards qualified selectors and destructive choice', async () => {
    invokeMock.mockResolvedValueOnce(
      samplePlan({
        table: 'type:public:mood',
        tables: ['type:public:mood'],
      }),
    );
    await expect(
      schemaDiffCommands.prepareTypePlan({
        sourceDbSessionId: 'src-type',
        targetDbSessionId: 'tgt-type',
        objectNames: ['public.mood'],
        allowDestructive: true,
      }),
    ).resolves.toMatchObject({ tables: ['type:public:mood'] });
    expect(invokeMock).toHaveBeenCalledWith('prepare_schema_type_plan', {
      sourceDbSessionId: 'src-type',
      targetDbSessionId: 'tgt-type',
      objectNames: ['public.mood'],
      allowDestructive: true,
    });
  });

  it('executeDeploy forwards deploy options and confirm token', async () => {
    const plan = samplePlan();
    const result = { status: 'committed', executedCount: 1 };
    mockAcceptedApply(result);
    await expect(
      schemaDiffCommands.executeDeploy({
        targetDbSessionId: 'tgt-3',
        plan,
        useTransaction: true,
        confirmDestructive: 'DEPLOY',
      }),
    ).resolves.toMatchObject({ status: 'committed' });
    expect(invokeMock).toHaveBeenCalledWith('execute_schema_diff_deploy', {
      targetDbSessionId: 'tgt-3',
      plan,
      useTransaction: true,
      confirmDestructive: 'DEPLOY',
      jobId: undefined,
      targetDatabase: undefined,
      targetSchema: undefined,
    });
  });

  it('executeDeploy forwards the target catalog and schema', async () => {
    const plan = samplePlan();
    mockAcceptedApply();
    await schemaDiffCommands.executeDeploy({
      targetDbSessionId: 'tgt-5',
      plan,
      targetDatabase: 'analytics',
      targetSchema: 'reporting',
    });
    // Without these the backend cannot route a cross-database deploy on
    // PostgreSQL, which cannot reference another database in one statement.
    expect(invokeMock).toHaveBeenCalledWith(
      'execute_schema_diff_deploy',
      expect.objectContaining({
        targetDatabase: 'analytics',
        targetSchema: 'reporting',
      }),
    );
  });

  it('executeDeploy omits optional keys when not provided', async () => {
    const plan = samplePlan();
    mockAcceptedApply();
    await schemaDiffCommands.executeDeploy({ targetDbSessionId: 'tgt-4', plan });
    expect(invokeMock).toHaveBeenCalledWith('execute_schema_diff_deploy', {
      targetDbSessionId: 'tgt-4',
      plan,
      useTransaction: undefined,
      confirmDestructive: undefined,
      jobId: undefined,
      targetDatabase: undefined,
      targetSchema: undefined,
    });
  });
});

it('keeps the immutable plan identity and forwards required rollback to the backend', async () => {
  const plan = samplePlan({ planId: 'reviewed-plan-42' });
  mockAcceptedApply({
    status: 'unknown',
    executedCount: 0,
    statementCount: 0,
    errors: [],
    statementResults: [],
  });
  await schemaDiffCommands.executeDeploy({
    targetDbSessionId: 'target',
    plan,
    requireRollback: true,
    useTransaction: true,
  });
  expect(invokeMock).toHaveBeenCalledWith(
    'execute_schema_diff_deploy',
    expect.objectContaining({
      plan: expect.objectContaining({ planId: 'reviewed-plan-42' }),
      requireRollback: true,
    }),
  );
});

it('round-trips all requirement tags without losing table or column identity', async () => {
  invokeMock.mockReset();
  const requirements = [
    { backfill: { table: 'users', column: 'status', reason: 'populate first' } },
    { unsupported: { operation: 'users', reason: 'table unavailable' } },
    { unsupported: { operation: 'users.id', reason: 'column unavailable' } },
  ];
  mockAcceptedPrepare(prepareEnvelope({ ...samplePlan(), requirements }));
  const prepared = await schemaDiffCommands.preparePlan({
    sourceDbSessionId: 'src',
    targetDbSessionId: 'tgt',
    tableNames: ['users'],
    allowDestructive: false,
  });
  expect(prepared.plan.requirements).toEqual([
    { kind: 'Backfill', table: 'users', column: 'status', reason: 'populate first' },
    { kind: 'Unsupported', table: 'users', column: '', reason: 'table unavailable' },
    { kind: 'Unsupported', table: 'users', column: 'id', reason: 'column unavailable' },
  ]);
  mockAcceptedApply();
  await schemaDiffCommands.executeDeploy({ targetDbSessionId: 'tgt', plan: prepared.plan });
  expect(invokeMock).toHaveBeenCalledWith(
    'execute_schema_diff_deploy',
    expect.objectContaining({ plan: expect.objectContaining({ requirements }) }),
  );
});

it('cancels only the requested job and propagates cancellation failures', async () => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValueOnce(true).mockRejectedValueOnce(new Error('job not found'));
  await expect(schemaDiffCommands.cancelDeploy('job-42')).resolves.toBe(true);
  expect(invokeMock).toHaveBeenCalledWith('cancel_schema_diff_deploy', { jobId: 'job-42' });
  await expect(schemaDiffCommands.cancelDeploy('missing')).rejects.toThrow('job not found');
});
