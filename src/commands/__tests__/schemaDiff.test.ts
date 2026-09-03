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
          statements: [{ sql: 'CREATE TABLE t()', risk: 'additive', rollbackSql: null, summary: '' }],
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
        { sql: 'ALTER TABLE users ADD COLUMN c int', risk: 'additive', rollbackSql: null, summary: '' },
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

describe('schemaDiffCommands wrappers', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  it('compareTableSchemas forwards session ids and table name', async () => {
    const diff = { table: 'users' } as unknown as TableSchemaDiff;
    invokeMock.mockResolvedValueOnce(diff);
    await expect(
      schemaDiffCommands.compareTableSchemas('src-1', 'tgt-1', 'users'),
    ).resolves.toBe(diff);
    expect(invokeMock).toHaveBeenCalledWith('compare_table_schemas', {
      sourceDbSessionId: 'src-1',
      targetDbSessionId: 'tgt-1',
      tableName: 'users',
    });
  });

  it('preparePlan normalizes IPC requirement tags into plan requirements', async () => {
    invokeMock.mockResolvedValueOnce({
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
    });
    await expect(
      schemaDiffCommands.preparePlan({
        sourceDbSessionId: 'src-2',
        targetDbSessionId: 'tgt-2',
        tableNames: ['a'],
        allowDestructive: false,
      }),
    ).resolves.toMatchObject({
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
    });
  });

  it('preparePlan forwards plan options with optional includeIndexes omitted key intact', async () => {
    invokeMock.mockResolvedValueOnce(samplePlan());
    await expect(
      schemaDiffCommands.preparePlan({
        sourceDbSessionId: 'src-2',
        targetDbSessionId: 'tgt-2',
        tableNames: ['a', 'b'],
        allowDestructive: true,
        includeIndexes: true,
      }),
    ).resolves.toMatchObject({ table: 'users' });
    expect(invokeMock).toHaveBeenCalledWith('prepare_schema_diff_plan', {
      sourceDbSessionId: 'src-2',
      targetDbSessionId: 'tgt-2',
      tableNames: ['a', 'b'],
      allowDestructive: true,
      includeIndexes: true,
    });
  });

  it('executeDeploy forwards deploy options and confirm token', async () => {
    const plan = samplePlan();
    const result = { status: 'committed', executedCount: 1 };
    invokeMock.mockResolvedValueOnce(result);
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
    });
  });

  it('executeDeploy omits optional keys when not provided', async () => {
    const plan = samplePlan();
    await schemaDiffCommands.executeDeploy({ targetDbSessionId: 'tgt-4', plan });
    expect(invokeMock).toHaveBeenCalledWith('execute_schema_diff_deploy', {
      targetDbSessionId: 'tgt-4',
      plan,
      useTransaction: undefined,
      confirmDestructive: undefined,
      jobId: undefined,
    });
  });
});
