/**
 * Shared Schema Diff E2E fixtures: wide schemas, multi-table diffs, setup/teardown.
 */
import { expect } from '@wdio/globals';
import {
  invokeBackend,
  queryScalar,
  withSafeModeOff,
  type QueryResultPayload,
} from '../helpers.js';
import {
  PG_SYNC_DB,
  PG_SYNC_TGT_DB,
  MYSQL_SYNC_DB,
  dropTableIfExists,
  mysqlConnectionConfig,
  mysqlWideTypesCreateSql,
  pgConnectionConfig,
  pgWideTypesCreateSql,
} from './dataTransferFixtures.js';

export { PG_SYNC_DB, PG_SYNC_TGT_DB, MYSQL_SYNC_DB, mysqlConnectionConfig, pgConnectionConfig };

/** Wide fixture column count (aligned with dataTransferFixtures). */
export const SCHEMA_DIFF_WIDE_COLUMN_COUNT = 19;

/** Multi-table batch compare count. */
export const SCHEMA_DIFF_MULTI_TABLE_COUNT = 10;

export function pgMinimalTargetCreateSql(table: string): string {
  return `CREATE TABLE ${table} (id BIGINT PRIMARY KEY)`;
}

export function mysqlMinimalTargetCreateSql(table: string): string {
  return `CREATE TABLE ${table} (id BIGINT PRIMARY KEY)`;
}

export function pgSimpleDiffCreateSql(table: string, extraCol: string): string {
  return `CREATE TABLE ${table} (
    id int PRIMARY KEY,
    name text NOT NULL,
    ${extraCol} text
  )`;
}

export function pgSimpleTargetCreateSql(table: string): string {
  return `CREATE TABLE ${table} (
    id int PRIMARY KEY,
    name text NOT NULL
  )`;
}

export function pgManyColumnsSourceSql(
  table: string,
  columnCount = SCHEMA_DIFF_WIDE_COLUMN_COUNT,
): string {
  const extraCols = Array.from(
    { length: columnCount - 1 },
    (_, i) => `col_${i} text NOT NULL DEFAULT ''`,
  ).join(',\n    ');
  return `CREATE TABLE ${table} (
    id BIGINT PRIMARY KEY,
    ${extraCols}
  )`;
}

export async function setupPgManyColumnsSourceMinimalTarget(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  await withSafeModeOff(async () => {
    await dropTableIfExists(srcSession, table);
    await dropTableIfExists(tgtSession, table);
    await invokeBackend('execute_query', {
      dbSessionId: srcSession,
      sql: pgManyColumnsSourceSql(table),
    });
    await invokeBackend('execute_query', {
      dbSessionId: tgtSession,
      sql: pgMinimalTargetCreateSql(table),
    });
  });
}

export async function setupPgWideSourceMinimalTarget(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  await withSafeModeOff(async () => {
    await dropTableIfExists(srcSession, table);
    await dropTableIfExists(tgtSession, table);
    await invokeBackend('execute_query', {
      dbSessionId: srcSession,
      sql: pgWideTypesCreateSql(table),
    });
    await invokeBackend('execute_query', {
      dbSessionId: tgtSession,
      sql: pgMinimalTargetCreateSql(table),
    });
  });
}

export async function setupPgWideSourceMinimalMysqlTarget(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  await withSafeModeOff(async () => {
    await dropTableIfExists(srcSession, table);
    await dropTableIfExists(tgtSession, table);
    await invokeBackend('execute_query', {
      dbSessionId: srcSession,
      sql: pgWideTypesCreateSql(table),
    });
    await invokeBackend('execute_query', {
      dbSessionId: tgtSession,
      sql: mysqlMinimalTargetCreateSql(table),
    });
  });
}

export async function setupMysqlWideSourceMinimalPgTarget(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  await withSafeModeOff(async () => {
    await dropTableIfExists(srcSession, table);
    await dropTableIfExists(tgtSession, table);
    await invokeBackend('execute_query', {
      dbSessionId: srcSession,
      sql: mysqlWideTypesCreateSql(table),
    });
    await invokeBackend('execute_query', {
      dbSessionId: tgtSession,
      sql: pgMinimalTargetCreateSql(table),
    });
  });
}

export async function setupMultiTableSchemaDiff(
  srcConnectionId: string,
  tgtConnectionId: string,
  tablePrefix: string,
  count: number,
): Promise<string[]> {
  const tables = Array.from({ length: count }, (_, i) => `${tablePrefix}_${i}`);
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  await withSafeModeOff(async () => {
    for (let i = 0; i < count; i++) {
      const table = tables[i];
      const extraCol = `extra_col_${i}`;
      await dropTableIfExists(srcSession, table);
      await dropTableIfExists(tgtSession, table);
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: pgSimpleDiffCreateSql(table, extraCol),
      });
      await invokeBackend('execute_query', {
        dbSessionId: tgtSession,
        sql: pgSimpleTargetCreateSql(table),
      });
    }
  });
  return tables;
}

export async function setupNotNullNoDefaultDiffFixture(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  await withSafeModeOff(async () => {
    await dropTableIfExists(srcSession, table);
    await dropTableIfExists(tgtSession, table);
    await invokeBackend('execute_query', {
      dbSessionId: srcSession,
      sql: `CREATE TABLE ${table} (id int PRIMARY KEY, status int NOT NULL)`,
    });
    await invokeBackend('execute_query', {
      dbSessionId: srcSession,
      sql: `INSERT INTO ${table} (id, status) VALUES (1, 1)`,
    });
    await invokeBackend('execute_query', {
      dbSessionId: tgtSession,
      sql: `CREATE TABLE ${table} (id int PRIMARY KEY)`,
    });
    await invokeBackend('execute_query', {
      dbSessionId: tgtSession,
      sql: `INSERT INTO ${table} (id) VALUES (1)`,
    });
  });
}

export async function setupDestructiveDiffFixture(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  await withSafeModeOff(async () => {
    await dropTableIfExists(srcSession, table);
    await dropTableIfExists(tgtSession, table);
    await invokeBackend('execute_query', {
      dbSessionId: srcSession,
      sql: pgSimpleTargetCreateSql(table),
    });
    await invokeBackend('execute_query', {
      dbSessionId: tgtSession,
      sql: `CREATE TABLE ${table} (
        id int PRIMARY KEY,
        name text NOT NULL,
        orphan_col text
      )`,
    });
  });
}

export async function countTableColumns(
  connectionId: string,
  table: string,
  dialect: 'postgresql' | 'mysql',
): Promise<number> {
  const session = await invokeBackend<string>('connect', { connectionId });
  if (dialect === 'postgresql') {
    const result = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: session,
      sql: `SELECT count(*)::int AS c FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = '${table}'`,
    });
    return Number(queryScalar(result, 'c'));
  }
  const result = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId: session,
    sql: `SELECT count(*) AS c FROM information_schema.columns
          WHERE table_schema = DATABASE() AND table_name = '${table}'`,
  });
  return Number(queryScalar(result, 'c'));
}

export async function columnExists(
  connectionId: string,
  table: string,
  column: string,
  dialect: 'postgresql' | 'mysql',
): Promise<boolean> {
  const session = await invokeBackend<string>('connect', { connectionId });
  if (dialect === 'postgresql') {
    const result = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: session,
      sql: `SELECT count(*)::int AS c FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = '${table}' AND column_name = '${column}'`,
    });
    return Number(queryScalar(result, 'c')) === 1;
  }
  const result = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId: session,
    sql: `SELECT count(*) AS c FROM information_schema.columns
          WHERE table_schema = DATABASE() AND table_name = '${table}' AND column_name = '${column}'`,
  });
  return Number(queryScalar(result, 'c')) === 1;
}

export async function columnNullable(
  connectionId: string,
  table: string,
  column: string,
): Promise<boolean> {
  const session = await invokeBackend<string>('connect', { connectionId });
  const result = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId: session,
    sql: `SELECT is_nullable FROM information_schema.columns
          WHERE table_schema = 'public' AND table_name = '${table}' AND column_name = '${column}'`,
  });
  return String(queryScalar(result, 'is_nullable')).toUpperCase() === 'YES';
}

export async function teardownSchemaDiffFixture(
  connectionIds: string[],
  tables: string | string[],
): Promise<void> {
  const tableList = Array.isArray(tables) ? tables : [tables];
  for (const connectionId of connectionIds) {
    try {
      const session = await invokeBackend<string>('connect', { connectionId });
      await withSafeModeOff(async () => {
        for (const table of tableList) {
          await dropTableIfExists(session, table);
        }
      });
    } catch {
      /* ok */
    }
  }
  for (const id of connectionIds) {
    try {
      await invokeBackend('delete_connection', { id });
    } catch {
      /* ok */
    }
  }
}

export function assertWideColumnCount(actual: number): void {
  expect(actual).toBeGreaterThanOrEqual(SCHEMA_DIFF_WIDE_COLUMN_COUNT - 2);
}
