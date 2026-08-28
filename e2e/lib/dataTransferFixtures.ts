/**
 * Shared Data Transfer E2E fixtures: schemas, bulk data, setup/teardown.
 *
 * Row count chosen so default batch size (500) exercises multi-batch inserts.
 */
import { expect } from '@wdio/globals';
import {
  invokeBackend,
  queryScalar,
  withSafeModeOff,
  type QueryResultPayload,
} from '../helpers.js';

/** Fixed row count for comprehensive transfer E2E (5 batches @ 500). */
export const TRANSFER_E2E_ROW_COUNT = 2500;

export const PG_SYNC_DB = 'datazen_sync_src';
export const PG_SYNC_TGT_DB = 'datazen_sync_tgt';
export const MYSQL_SYNC_DB = 'datazen_sync_mysql_tgt';

export function pgConnectionConfig(id: string, name: string, database: string) {
  return {
    id,
    name,
    databaseType: 'postgresql',
    host: process.env.E2E_PG_HOST || '127.0.0.1',
    port: Number(process.env.E2E_PG_PORT) || 5432,
    username: process.env.E2E_PG_USER || 'postgres',
    password: process.env.E2E_PG_PASSWORD || '',
    database,
    sslMode: 'disable',
  };
}

export function mysqlConnectionConfig(id: string, name: string, database: string) {
  return {
    id,
    name,
    databaseType: 'mysql',
    host: process.env.E2E_MYSQL_HOST || '127.0.0.1',
    port: Number(process.env.E2E_MYSQL_PORT) || 3306,
    username: process.env.E2E_MYSQL_USER || 'root',
    password: process.env.E2E_MYSQL_PASSWORD || '',
    database,
    sslMode: 'disable',
  };
}

/** PostgreSQL wide-type fixture table (IR-mapped types for PG↔MySQL). */
export function pgWideTypesCreateSql(table: string): string {
  return `CREATE TABLE ${table} (
    id BIGINT PRIMARY KEY,
    small_qty SMALLINT NOT NULL,
    qty INT NOT NULL,
    big_qty BIGINT,
    amount DECIMAL(18,6) NOT NULL DEFAULT 0,
    rate REAL,
    weight DOUBLE PRECISION,
    code CHAR(8) NOT NULL DEFAULT 'CODE0001',
    label VARCHAR(128) NOT NULL DEFAULT 'unnamed',
    body TEXT,
    meta JSONB,
    payload JSON,
    active BOOLEAN NOT NULL DEFAULT true,
    born DATE,
    day_time TIME,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMP NOT NULL DEFAULT now(),
    uid UUID,
    blob_data BYTEA
  )`;
}

/** MySQL wide-type fixture table (aligned columns for data-mode transfer). */
export function mysqlWideTypesCreateSql(table: string): string {
  return `CREATE TABLE ${table} (
    id BIGINT PRIMARY KEY,
    small_qty SMALLINT NOT NULL,
    qty INT NOT NULL,
    big_qty BIGINT,
    amount DECIMAL(18,6) NOT NULL DEFAULT 0,
    rate FLOAT,
    weight DOUBLE,
    code CHAR(8) NOT NULL DEFAULT 'CODE0001',
    label VARCHAR(128) NOT NULL DEFAULT 'unnamed',
    body TEXT,
    meta JSON,
    payload JSON,
    active TINYINT(1) NOT NULL DEFAULT 1,
    born DATE,
    day_time TIME,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    uid CHAR(36),
    blob_data LONGBLOB
  )`;
}

export function pgWideTypesBulkInsertSql(table: string, count: number): string {
  return `INSERT INTO ${table} (
    id, small_qty, qty, big_qty, amount, rate, weight, code, label, body,
    meta, payload, active, born, day_time, created_at, updated_at, uid, blob_data
  )
  SELECT
    g,
    (g % 32767)::smallint,
    (g % 1000000)::int,
    g::bigint,
    (g * 1.234567)::decimal(18,6),
    (g * 0.1)::real,
    (g * 0.01)::double precision,
    ('C' || lpad((g % 10000000)::text, 7, '0'))::char(8),
    'label_' || g,
    'body text ' || g,
    jsonb_build_object('n', g, 'tag', 'pg'),
    json_build_object('n', g),
    (g % 2 = 0),
    DATE '2020-01-01' + ((g % 365))::int,
    TIME '08:30:00' + ((g % 3600) || ' seconds')::interval,
    TIMESTAMPTZ '2024-01-01' + (g || ' minutes')::interval,
    TIMESTAMP '2024-01-01' + (g || ' minutes')::interval,
    gen_random_uuid(),
    decode(lpad(to_hex(g), 8, '0'), 'hex')
  FROM generate_series(1, ${count}) AS g`;
}

export function mysqlWideTypesInsertValues(g: number): string {
  const amount = (g * 1.234567).toFixed(6);
  const code = `C${String(g % 10000000).padStart(7, '0')}`;
  return `(${g}, ${g % 32767}, ${g % 1000000}, ${g}, ${amount}, ${g * 0.1}, ${g * 0.01}, '${code}', 'label_${g}', 'body text ${g}', JSON_OBJECT('n', ${g}, 'tag', 'mysql'), JSON_OBJECT('n', ${g}), ${g % 2}, DATE_ADD('2020-01-01', INTERVAL ${g % 365} DAY), ADDTIME('08:30:00', SEC_TO_TIME(${g % 3600})), DATE_ADD('2024-01-01', INTERVAL ${g} MINUTE), DATE_ADD('2024-01-01', INTERVAL ${g} MINUTE), UUID(), UNHEX(LPAD(HEX(${g}), 8, '0')))`;
}

const MYSQL_INSERT_COLUMNS = `id, small_qty, qty, big_qty, amount, rate, weight, code, label, body, meta, payload, active, born, day_time, created_at, updated_at, uid, blob_data`;

export async function seedMysqlWideTypes(
  dbSessionId: string,
  table: string,
  count: number,
  batchSize = 250,
): Promise<void> {
  for (let start = 1; start <= count; start += batchSize) {
    const end = Math.min(start + batchSize - 1, count);
    const values = Array.from({ length: end - start + 1 }, (_, i) =>
      mysqlWideTypesInsertValues(start + i),
    ).join(',\n');
    await invokeBackend('execute_query', {
      dbSessionId,
      sql: `INSERT INTO ${table} (${MYSQL_INSERT_COLUMNS}) VALUES ${values}`,
    });
  }
}

export async function dropTableIfExists(dbSessionId: string, table: string): Promise<void> {
  await invokeBackend('execute_query', {
    dbSessionId,
    sql: `DROP TABLE IF EXISTS ${table}`,
  });
}

export async function setupPgSourceWideTable(
  connectionId: string,
  table: string,
  rowCount: number,
): Promise<void> {
  const session = await invokeBackend<string>('connect', { connectionId });
  await withSafeModeOff(async () => {
    await dropTableIfExists(session, table);
    await invokeBackend('execute_query', {
      dbSessionId: session,
      sql: pgWideTypesCreateSql(table),
    });
    await invokeBackend('execute_query', {
      dbSessionId: session,
      sql: pgWideTypesBulkInsertSql(table, rowCount),
    });
  });
}

export async function setupMysqlSourceWideTable(
  connectionId: string,
  table: string,
  rowCount: number,
): Promise<void> {
  const session = await invokeBackend<string>('connect', { connectionId });
  await withSafeModeOff(async () => {
    await dropTableIfExists(session, table);
    await invokeBackend('execute_query', {
      dbSessionId: session,
      sql: mysqlWideTypesCreateSql(table),
    });
    await seedMysqlWideTypes(session, table, rowCount);
  });
}

export async function setupEmptyTargetTable(
  connectionId: string,
  table: string,
  dialect: 'postgresql' | 'mysql',
): Promise<void> {
  const session = await invokeBackend<string>('connect', { connectionId });
  await withSafeModeOff(async () => {
    await dropTableIfExists(session, table);
    await invokeBackend('execute_query', {
      dbSessionId: session,
      sql: dialect === 'postgresql' ? pgWideTypesCreateSql(table) : mysqlWideTypesCreateSql(table),
    });
  });
}

export async function teardownTransferFixture(
  connectionIds: string[],
  table: string,
): Promise<void> {
  for (const connectionId of connectionIds) {
    try {
      const session = await invokeBackend<string>('connect', { connectionId });
      await withSafeModeOff(async () => {
        await dropTableIfExists(session, table);
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

export function expectedQtySum(rowCount: number): number {
  let sum = 0;
  for (let g = 1; g <= rowCount; g++) {
    sum += g % 1000000;
  }
  return sum;
}

export async function assertWideTableRowCount(
  connectionId: string,
  table: string,
  dialect: 'postgresql' | 'mysql',
  expected: number,
): Promise<void> {
  const session = await invokeBackend<string>('connect', { connectionId });
  const countSql =
    dialect === 'postgresql'
      ? `SELECT count(*)::int AS c FROM ${table}`
      : `SELECT COUNT(*) AS c FROM ${table}`;
  const rows = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId: session,
    sql: countSql,
  });
  expect(Number(queryScalar(rows, 'c'))).toBe(expected);

  const sumSql =
    dialect === 'postgresql'
      ? `SELECT sum(qty)::bigint AS s FROM ${table}`
      : `SELECT SUM(qty) AS s FROM ${table}`;
  const sumRows = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId: session,
    sql: sumSql,
  });
  expect(Number(queryScalar(sumRows, 's'))).toBe(expectedQtySum(expected));

  const metaSql =
    dialect === 'postgresql'
      ? `SELECT count(*)::int AS c FROM ${table} WHERE meta IS NOT NULL`
      : `SELECT COUNT(*) AS c FROM ${table} WHERE meta IS NOT NULL`;
  const metaRows = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId: session,
    sql: metaSql,
  });
  expect(Number(queryScalar(metaRows, 'c'))).toBe(expected);

  const amountSql =
    dialect === 'postgresql'
      ? `SELECT amount::float AS a FROM ${table} WHERE id = 100`
      : `SELECT CAST(amount AS DECIMAL(18,6)) AS a FROM ${table} WHERE id = 100`;
  const amountRows = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId: session,
    sql: amountSql,
  });
  expect(Number(queryScalar(amountRows, 'a'))).toBeCloseTo(100 * 1.234567, 3);
}
