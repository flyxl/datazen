/**
 * Shared Schema Diff E2E fixtures: wide schemas, multi-table diffs, setup/teardown.
 */
import { expect } from '@wdio/globals';
import {
  disconnectBackend,
  invokeBackend,
  parseQueryRows,
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
  try {
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
  } finally {
    await disconnectBackend(srcSession);
    await disconnectBackend(tgtSession);
  }
}

export async function setupPgWideSourceMinimalTarget(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  try {
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
  } finally {
    await disconnectBackend(srcSession);
    await disconnectBackend(tgtSession);
  }
}

export async function setupPgWideSourceMinimalMysqlTarget(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  try {
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
  } finally {
    await disconnectBackend(srcSession);
    await disconnectBackend(tgtSession);
  }
}

export async function setupMysqlWideSourceMinimalPgTarget(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  try {
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
  } finally {
    await disconnectBackend(srcSession);
    await disconnectBackend(tgtSession);
  }
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
  try {
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
  } finally {
    await disconnectBackend(srcSession);
    await disconnectBackend(tgtSession);
  }
}

export async function setupNotNullNoDefaultDiffFixture(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  try {
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
  } finally {
    await disconnectBackend(srcSession);
    await disconnectBackend(tgtSession);
  }
}

export async function setupDestructiveDiffFixture(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  try {
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
  } finally {
    await disconnectBackend(srcSession);
    await disconnectBackend(tgtSession);
  }
}

export async function countTableColumns(
  connectionId: string,
  table: string,
  dialect: 'postgresql' | 'mysql',
): Promise<number> {
  const session = await invokeBackend<string>('connect', { connectionId });
  try {
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
  } finally {
    await disconnectBackend(session);
  }
}

export async function columnExists(
  connectionId: string,
  table: string,
  column: string,
  dialect: 'postgresql' | 'mysql',
): Promise<boolean> {
  const session = await invokeBackend<string>('connect', { connectionId });
  try {
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
  } finally {
    await disconnectBackend(session);
  }
}

export async function columnNullable(
  connectionId: string,
  table: string,
  column: string,
): Promise<boolean> {
  const session = await invokeBackend<string>('connect', { connectionId });
  try {
    const result = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: session,
      sql: `SELECT is_nullable FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = '${table}' AND column_name = '${column}'`,
    });
    const rows = parseQueryRows(result);
    return String(rows[0]?.[0]).toUpperCase() === 'YES';
  } finally {
    await disconnectBackend(session);
  }
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
      // Release the session to free pool connections.
      await disconnectBackend(session);
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

export function pgEnterpriseStaffCreateSql(table: string): string {
  return `CREATE TABLE ${table} (
    org_id VARCHAR(32) NOT NULL,
    emp_no INT NOT NULL,
    emp_name VARCHAR(64) NOT NULL,
    email VARCHAR(128) NOT NULL,
    search_tag TEXT NOT NULL,
    job_desc TEXT NOT NULL DEFAULT '标准在册',
    biography TEXT,
    dept_code SMALLINT NOT NULL DEFAULT 10,
    base_salary NUMERIC(12, 2) NOT NULL DEFAULT 6500.00,
    bonus_rate REAL DEFAULT 0.15,
    annual_equity BIGINT DEFAULT 10000,
    is_active BOOLEAN NOT NULL DEFAULT true,
    hire_date DATE NOT NULL,
    shift_start TIME NOT NULL DEFAULT '09:00:00',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    device_uuid UUID,
    avatar_bin BYTEA,
    profile_json JSONB,
    PRIMARY KEY (org_id, emp_no)
  )`;
}

export async function setupPgEnterpriseStaffSourceEmptyMysqlTarget(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  try {
    await withSafeModeOff(async () => {
      await dropTableIfExists(srcSession, table);
      await dropTableIfExists(tgtSession, table);
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: pgEnterpriseStaffCreateSql(table),
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `CREATE UNIQUE INDEX uq_${table}_org_email ON ${table}(org_id, email)`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `CREATE INDEX idx_${table}_dept_hire ON ${table}(dept_code, hire_date)`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `CREATE INDEX idx_${table}_org_tag ON ${table}(org_id, search_tag)`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `INSERT INTO ${table} (
          org_id, emp_no, emp_name, email, search_tag, job_desc, biography,
          dept_code, base_salary, bonus_rate, annual_equity, is_active,
          hire_date, shift_start, device_uuid, avatar_bin, profile_json
        ) VALUES (
          'ORG_001', 1001, '张工', 'zhang@corp.internal', 'R&D/Core', '资深架构师', '从事分布式系统研发 10 年...',
          101, 28500.50, 0.25, 50000, true,
          '2024-03-15', '09:00:00', 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11',
          decode('89504E470D0A1A0A', 'hex'), '{"skills": ["Rust", "React", "K8s"], "tier": "L4"}'
        )`,
      });
    });
  } finally {
    await disconnectBackend(srcSession);
    await disconnectBackend(tgtSession);
  }
}

export function mysqlTenantOrdersCreateSql(table: string): string {
  return `CREATE TABLE ${table} (
    tenant_id VARCHAR(32) NOT NULL,
    order_id BIGINT NOT NULL,
    order_sn VARCHAR(64) NOT NULL,
    order_status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    payment_amount DECIMAL(10, 2) NOT NULL DEFAULT 0.00,
    is_settled TINYINT(1) NOT NULL DEFAULT 0,
    shipping_addr TEXT,
    metadata_json JSON,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (tenant_id, order_id)
  )`;
}

export async function setupMysqlTenantOrdersSourceEmptyPgTarget(
  srcConnectionId: string,
  tgtConnectionId: string,
  table: string,
): Promise<void> {
  const srcSession = await invokeBackend<string>('connect', { connectionId: srcConnectionId });
  const tgtSession = await invokeBackend<string>('connect', { connectionId: tgtConnectionId });
  try {
    await withSafeModeOff(async () => {
      await dropTableIfExists(srcSession, table);
      await dropTableIfExists(tgtSession, table);
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: mysqlTenantOrdersCreateSql(table),
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `CREATE UNIQUE INDEX uq_${table}_sn ON ${table}(tenant_id, order_sn)`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `CREATE INDEX idx_${table}_status_created ON ${table}(order_status, created_at)`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `INSERT INTO ${table} (
          tenant_id, order_id, order_sn, payment_amount, is_settled, metadata_json
        ) VALUES (
          'TENANT_CN_01', 90001, 'SN202609090001', 199.90, 0, '{"channel": "app"}'
        )`,
      });
    });
  } finally {
    await disconnectBackend(srcSession);
    await disconnectBackend(tgtSession);
  }
}

export async function fetchMysqlPrimaryKeys(
  connectionId: string,
  table: string,
): Promise<string[]> {
  const session = await invokeBackend<string>('connect', { connectionId });
  try {
    const result = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: session,
      sql: `SELECT COLUMN_NAME
            FROM information_schema.KEY_COLUMN_USAGE
            WHERE TABLE_SCHEMA = DATABASE()
              AND TABLE_NAME = '${table}'
              AND CONSTRAINT_NAME = 'PRIMARY'
            ORDER BY ORDINAL_POSITION`,
    });
    const rows = parseQueryRows(result);
    return rows.map((r) => String(r[0]));
  } finally {
    await disconnectBackend(session);
  }
}

export async function fetchMysqlIndexMap(
  connectionId: string,
  table: string,
): Promise<Record<string, { columns: string[]; isUnique: boolean }>> {
  const session = await invokeBackend<string>('connect', { connectionId });
  try {
    const result = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: session,
      sql: `SELECT INDEX_NAME, COLUMN_NAME, NON_UNIQUE
            FROM information_schema.STATISTICS
            WHERE TABLE_SCHEMA = DATABASE()
              AND TABLE_NAME = '${table}'
            ORDER BY INDEX_NAME, SEQ_IN_INDEX`,
    });
    const rows = parseQueryRows(result);
    const map: Record<string, { columns: string[]; isUnique: boolean }> = {};
    for (const r of rows) {
      const keyName = String(r[0]);
      const colName = String(r[1]);
      const isUnique = String(r[2]) === '0';
      if (!map[keyName]) {
        map[keyName] = { columns: [], isUnique };
      }
      map[keyName].columns.push(colName);
    }
    return map;
  } finally {
    await disconnectBackend(session);
  }
}

export async function fetchPgPrimaryKeys(connectionId: string, table: string): Promise<string[]> {
  const session = await invokeBackend<string>('connect', { connectionId });
  try {
    const result = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: session,
      sql: `SELECT a.attname
            FROM pg_index i
            JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS k(attnum, n) ON true
            JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.attnum
            WHERE i.indrelid = '${table}'::regclass AND i.indisprimary
            ORDER BY k.n`,
    });
    const rows = parseQueryRows(result);
    return rows.map((r) => String(r[0]));
  } finally {
    await disconnectBackend(session);
  }
}

export async function fetchPgIndexMap(
  connectionId: string,
  table: string,
): Promise<Record<string, { columns: string[]; isUnique: boolean }>> {
  const session = await invokeBackend<string>('connect', { connectionId });
  try {
    const result = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: session,
      sql: `SELECT i.relname AS index_name,
                   a.attname AS column_name,
                   ix.indisunique AS is_unique
            FROM pg_index ix
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS k(attnum, n) ON true
            JOIN pg_attribute a ON a.attrelid = ix.indrelid AND a.attnum = k.attnum
            WHERE ix.indrelid = '${table}'::regclass
            ORDER BY i.relname, k.n`,
    });
    const rows = parseQueryRows(result);
    const map: Record<string, { columns: string[]; isUnique: boolean }> = {};
    for (const r of rows) {
      const idxName = String(r[0]);
      const colName = String(r[1]);
      const isUnique = Boolean(r[2]);
      if (!map[idxName]) {
        map[idxName] = { columns: [], isUnique };
      }
      map[idxName].columns.push(colName);
    }
    return map;
  } finally {
    await disconnectBackend(session);
  }
}
