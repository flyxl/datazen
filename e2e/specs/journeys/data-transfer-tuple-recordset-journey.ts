/**
 * Composite-key Data Transfer journey. The equal non-ASCII text components
 * deliberately let the numeric key determine endpoint order without assuming
 * a host-side text collation.
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  clickTransferNext,
  closeExtraWindows,
  connectBackend,
  disconnectBackend,
  invokeBackend,
  openDataTransferWindow,
  parseQueryRows,
  selectDzOptionInWrap,
  withSafeModeOff,
  type QueryResultPayload,
} from '../../helpers.js';

type DriverType = 'postgresql' | 'mysql';

interface Route {
  label: string;
  sourceType: DriverType;
  sourceDatabase: string;
  targetType: DriverType;
  targetDatabase: string;
}

const routes: Route[] = [
  {
    label: 'PostgreSQL to MySQL',
    sourceType: 'postgresql',
    sourceDatabase: 'datazen_sync_src',
    targetType: 'mysql',
    targetDatabase: 'datazen_sync_mysql_tgt',
  },
  {
    label: 'MySQL to PostgreSQL',
    sourceType: 'mysql',
    sourceDatabase: 'datazen_sync_mysql_tgt',
    targetType: 'postgresql',
    targetDatabase: 'datazen_sync_tgt',
  },
];

function connectionConfig(type: DriverType, id: string, name: string, database: string) {
  if (type === 'postgresql') {
    return {
      id,
      name,
      databaseType: type,
      host: process.env.E2E_PG_HOST || '127.0.0.1',
      port: Number(process.env.E2E_PG_PORT) || 5432,
      username: process.env.E2E_PG_USER || 'postgres',
      password: process.env.E2E_PG_PASSWORD || '',
      database,
      sslMode: 'disable',
    };
  }
  return {
    id,
    name,
    databaseType: type,
    host: process.env.E2E_MYSQL_HOST || '127.0.0.1',
    port: Number(process.env.E2E_MYSQL_PORT) || 3306,
    username: process.env.E2E_MYSQL_USER || 'root',
    password: process.env.E2E_MYSQL_PASSWORD || '',
    database,
    sslMode: 'disable',
  };
}

function createTableSql(type: DriverType, table: string): string {
  const tenant =
    type === 'mysql' ? 'VARCHAR(64) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin' : 'VARCHAR(64)';
  const payload = type === 'mysql' ? 'LONGTEXT' : 'TEXT';
  return `CREATE TABLE ${table} (tenant ${tenant} NOT NULL, seq BIGINT NOT NULL, payload ${payload} NOT NULL, PRIMARY KEY (tenant, seq))`;
}

function insertFixtureSql(table: string): string {
  return `INSERT INTO ${table} (tenant, seq, payload) VALUES
    ('a-雪', -3, 'before-start'),
    ('a-雪', -2, 'inside-negative'),
    ('a-雪', 0, 'inside-zero'),
    ('a-雪', 9223372036854775806, 'inside-large'),
    ('a-雪', 9223372036854775807, 'after-end'),
    ('z-外', 0, 'outside-text-prefix')`;
}

function queryText(value: unknown): string {
  if (Array.isArray(value) && value.every((item) => typeof item === 'number')) {
    return new TextDecoder().decode(Uint8Array.from(value));
  }
  return String(value);
}

async function fixtureTableExists(
  dbSessionId: string,
  type: DriverType,
  table: string,
): Promise<boolean> {
  const sql =
    type === 'postgresql'
      ? `SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name = '${table}'`
      : `SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = '${table}'`;
  const rows = parseQueryRows(
    await invokeBackend<QueryResultPayload>('execute_query', { dbSessionId, sql }),
  );
  const rawCount = rows[0]?.[0];
  if (rawCount === undefined || rawCount === null) {
    throw new Error(`fixture catalog query returned no count for ${table}`);
  }
  const count = Number(queryText(rawCount));
  if (!Number.isSafeInteger(count) || count < 0) {
    throw new Error(`fixture catalog query returned an invalid count for ${table}`);
  }
  return count > 0;
}

describe('Data Transfer composite tuple recordset journeys', () => {
  for (const route of routes) {
    describe(route.label, () => {
      let mainWindow: string;
      let sourceSession: string | undefined;
      let targetSession: string | undefined;
      const stamp = `${Date.now().toString(36)}_${route.sourceType}`;
      const sourceId = `e2e_dt_tuple_src_${stamp}`;
      const targetId = `e2e_dt_tuple_tgt_${stamp}`;
      const sourceName = `DT-Tuple-Source-${stamp}`;
      const targetName = `DT-Tuple-Target-${stamp}`;
      const table = `dt_tuple_${stamp}`;

      before(async () => {
        mainWindow = await browser.getWindowHandle();
        await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });
        await invokeBackend('save_connection', {
          config: connectionConfig(route.sourceType, sourceId, sourceName, route.sourceDatabase),
        });
        await invokeBackend('save_connection', {
          config: connectionConfig(route.targetType, targetId, targetName, route.targetDatabase),
        });
        sourceSession = await connectBackend(sourceId);
        targetSession = await connectBackend(targetId);
        await withSafeModeOff(async () => {
          await invokeBackend('execute_query', {
            dbSessionId: sourceSession!,
            sql: `DROP TABLE IF EXISTS ${table}`,
          });
          await invokeBackend('execute_query', {
            dbSessionId: targetSession!,
            sql: `DROP TABLE IF EXISTS ${table}`,
          });
          await invokeBackend('execute_query', {
            dbSessionId: sourceSession!,
            sql: createTableSql(route.sourceType, table),
          });
          await invokeBackend('execute_query', {
            dbSessionId: sourceSession!,
            sql: insertFixtureSql(table),
          });
          await invokeBackend('execute_query', {
            dbSessionId: targetSession!,
            sql: createTableSql(route.targetType, table),
          });
          const sourceHex =
            route.sourceType === 'postgresql'
              ? "encode(convert_to(tenant, 'UTF8'), 'hex')"
              : 'HEX(tenant)';
          const sourceRows = await invokeBackend<QueryResultPayload>('execute_query', {
            dbSessionId: sourceSession!,
            sql: `SELECT tenant, ${sourceHex} AS tenant_hex, CAST(seq AS ${route.sourceType === 'postgresql' ? 'TEXT' : 'CHAR'}) AS seq FROM ${table} ORDER BY tenant, seq`,
          });
          const sourceTuples = parseQueryRows(sourceRows).map(
            (row) => `${queryText(row[0])}:${queryText(row[1]).toLowerCase()}:${queryText(row[2])}`,
          );
          expect([...sourceTuples].sort()).toEqual(
            [
              'a-雪:612de99baa:-3',
              'a-雪:612de99baa:-2',
              'a-雪:612de99baa:0',
              'a-雪:612de99baa:9223372036854775806',
              'a-雪:612de99baa:9223372036854775807',
              'z-外:7a2de5a496:0',
            ].sort(),
          );
        });
      });

      after(async () => {
        const cleanupErrors: string[] = [];
        const cleanupSessions: string[] = [];
        try {
          await withSafeModeOff(async () => {
            for (const endpoint of [
              { label: 'source', id: sourceId, type: route.sourceType },
              { label: 'target', id: targetId, type: route.targetType },
            ] as const) {
              let cleanupSession: string | undefined;
              try {
                // The transfer window may have released the sessions created in
                // before(); ask the connection manager for a live session before
                // attempting fixture cleanup.
                cleanupSession = await connectBackend(endpoint.id);
                cleanupSessions.push(cleanupSession);
                await invokeBackend('execute_query', {
                  dbSessionId: cleanupSession,
                  sql: `DROP TABLE IF EXISTS ${table}`,
                });
                if (await fixtureTableExists(cleanupSession, endpoint.type, table)) {
                  throw new Error(`fixture table ${table} still exists after DROP`);
                }
              } catch (error) {
                cleanupErrors.push(
                  `${endpoint.label} (${endpoint.type}): ${error instanceof Error ? error.message : String(error)}`,
                );
              }
            }
          });
        } catch (error) {
          cleanupErrors.push(
            `Safe Mode cleanup wrapper: ${error instanceof Error ? error.message : String(error)}`,
          );
        }
        for (const [label, session] of [
          ...cleanupSessions.map(
            (session, index) => [`cleanup session ${index + 1}`, session] as const,
          ),
          ...(sourceSession ? [['original source session', sourceSession] as const] : []),
          ...(targetSession ? [['original target session', targetSession] as const] : []),
        ]) {
          try {
            await disconnectBackend(session);
          } catch (error) {
            cleanupErrors.push(
              `disconnect ${label}: ${error instanceof Error ? error.message : String(error)}`,
            );
          }
        }
        try {
          await invokeBackend('delete_connection', { id: sourceId });
        } catch {
          /* The connection may already have been removed. */
        }
        try {
          await invokeBackend('delete_connection', { id: targetId });
        } catch {
          /* The connection may already have been removed. */
        }
        await closeExtraWindows(mainWindow);
        await browser.switchToWindow(mainWindow);
        if (cleanupErrors.length > 0) {
          throw new Error(`fixture cleanup failed:\n${cleanupErrors.join('\n')}`);
        }
      });

      it('transfers only the selected inclusive/exclusive composite key range', async () => {
        await openDataTransferWindow();
        await selectDzOptionInWrap('data-transfer-source', sourceName);
        await selectDzOptionInWrap('data-transfer-target', targetName);
        await clickTransferNext();

        const dataMode = await $('[data-testid="data-transfer-mode-data"]');
        await dataMode.scrollIntoView();
        await dataMode.waitForClickable({ timeout: 10000 });
        await dataMode.click();
        await clickTransferNext();

        await $('[data-testid="data-transfer-table-row"]').waitForDisplayed({ timeout: 15000 });
        expect(await $('body').getText()).toContain(table);
        await clickTransferNext();

        const enableRecordset = await $('[data-testid="data-transfer-recordset-enable"]');
        await enableRecordset.waitForDisplayed({ timeout: 10000 });
        await enableRecordset.click();
        const tupleEditor = await $('[data-testid="data-transfer-recordset-tuple-editor"]');
        await tupleEditor.waitForDisplayed({ timeout: 5000 });
        await expect(
          await $('[data-testid="data-transfer-recordset-tuple-collation-hint"]'),
        ).toBeDisplayed();

        await $('[data-testid="data-transfer-recordset-tuple-start-0"]').setValue('a-雪');
        await $('[data-testid="data-transfer-recordset-tuple-start-1"]').setValue('-3');
        await $('[data-testid="data-transfer-recordset-tuple-start-inclusive"]').click();
        await $('[data-testid="data-transfer-recordset-tuple-end-0"]').setValue('a-雪');
        await $('[data-testid="data-transfer-recordset-tuple-end-1"]').setValue(
          '9223372036854775806',
        );
        await clickTransferNext({ timeout: 30000, pauseMs: 2500 });

        const preview = await $('[data-testid="data-transfer-preview"]');
        const pageText = (await $('body').getText()).slice(-1800);
        await preview.waitForDisplayed({
          timeout: 20000,
          timeoutMsg: `Tuple transfer preview did not appear. Page state: ${pageText}`,
        });
        const previewText = await preview.getText();
        expect(previewText).toContain('a-雪');
        expect(previewText).toContain('9223372036854775806');
        expect(previewText).toMatch(/Estimated rows|预计行数/);
        expect(previewText).toContain('3');

        await $('[data-testid="data-transfer-execute"]').click();
        const result = await $('[data-testid="data-transfer-result"]');
        await result.waitForDisplayed({ timeout: 30000 });
        await browser.waitUntil(
          async () => (await result.getAttribute('data-completed')) === 'true',
          {
            timeout: 90000,
            timeoutMsg: 'Tuple-range transfer did not finish with a verified result',
          },
        );
        expect(await result.getAttribute('data-verdict-severity')).toBe('ok');
        expect(await result.getText()).not.toContain(t('transfer.error'));

        const target = await connectBackend(targetId);
        try {
          const tenantHex =
            route.targetType === 'postgresql'
              ? "encode(convert_to(tenant, 'UTF8'), 'hex')"
              : 'HEX(tenant)';
          const cast = route.targetType === 'postgresql' ? 'TEXT' : 'CHAR';
          const tenantType =
            route.targetType === 'postgresql' ? 'pg_typeof(tenant)::text AS tenant_type, ' : '';
          const rows = await invokeBackend<QueryResultPayload>('execute_query', {
            dbSessionId: target,
            sql: `SELECT tenant AS tenant_value, ${tenantType}${tenantHex} AS tenant_hex, CAST(seq AS ${cast}) AS seq FROM ${table} ORDER BY tenant, seq`,
          });
          const tenantHexIndex = route.targetType === 'postgresql' ? 2 : 1;
          const targetRows = parseQueryRows(rows);
          expect(targetRows.map((row) => queryText(row[0]))).toEqual(['a-雪', 'a-雪', 'a-雪']);
          if (route.targetType === 'postgresql') {
            expect(targetRows.map((row) => queryText(row[1]))).toEqual([
              'character varying',
              'character varying',
              'character varying',
            ]);
          }
          const actual = targetRows.map(
            (row) =>
              `${queryText(row[tenantHexIndex]).toLowerCase()}:${queryText(row[tenantHexIndex + 1])}`,
          );
          expect(actual).toEqual([
            '612de99baa:-2',
            '612de99baa:0',
            '612de99baa:9223372036854775806',
          ]);
        } finally {
          await disconnectBackend(target);
        }
      });
    });
  }
});
