/**
 * Data Transfer structure-object emission: secondary indexes and foreign keys.
 *
 * Covers the Host-generic UI path only (dialect-specific behaviour belongs to
 * `packages/drivers/<id>/e2e/`):
 *   - the DDL preview renders one block per secondary index and per foreign key,
 *   - every table block is emitted before every index block, and every index block
 *     before every foreign-key block,
 *   - the emitted DDL really creates the indexes and the constraint on the target,
 *   - a target-scoped object-name collision fails closed and names both sides.
 *
 * Requires the standard E2E databases (`datazen_sync_src`, `datazen_sync_tgt`,
 * `datazen_sync_mysql_tgt`) provisioned by `pnpm e2e`.
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import {
  advanceTransferWizardToPreview,
  clickTransferNext,
  closeExtraWindows,
  disconnectBackend,
  invokeBackend,
  openDataTransferWindow,
  queryScalar,
  selectDzOptionInWrap,
  withSafeModeOff,
  type QueryResultPayload,
} from '../helpers.js';

const SRC_TABLES = ['dt_obj_parent', 'dt_obj_child', 'dt_obj_other'];
const COLLIDE_TABLES = ['dt_obj_coll_a', 'dt_obj_coll_b'];

function pgConfig(id: string, name: string, database: string) {
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

function mysqlConfig(id: string, name: string, database: string) {
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

/** Drives source -> target -> structure mode -> table selection -> mapping -> preview. */
async function runStructureWizard(srcName: string, tgtName: string, tables: string[]) {
  await openDataTransferWindow();
  await selectDzOptionInWrap('data-transfer-source', srcName);
  await selectDzOptionInWrap('data-transfer-target', tgtName);
  await browser.pause(1500);
  await clickTransferNext();

  await (await $('[data-testid="data-transfer-mode-structure"]')).click();
  await browser.pause(300);
  const collationChoice = await $('[data-testid="data-transfer-use-target-default-collation"]');
  if ((await collationChoice.isExisting()) && !(await collationChoice.isSelected())) {
    await collationChoice.click();
  }
  await clickTransferNext();

  await browser.pause(2000);
  const pageText = (await $('body').getText()).slice(-1800);
  for (const table of tables) {
    const row = await $(`[data-testid="data-transfer-table-row"]*=${table}`);
    await row.waitForDisplayed({
      timeout: 15000,
      timeoutMsg: `Transfer table ${table} did not appear. Page state: ${pageText}`,
    });
    const checkbox = await row.$('input[type="checkbox"]');
    if (!(await checkbox.isSelected())) await checkbox.click();
  }
  await clickTransferNext();

  await $('[data-testid="data-transfer-mapping-step"]').waitForDisplayed({ timeout: 15000 });
  const createNew = await $('[data-testid="data-transfer-create-new-toggle"]');
  if (await createNew.isExisting()) {
    if (!(await createNew.isSelected())) await createNew.click();
    await (await $('[data-testid="data-transfer-target-table-input"]')).click();
    await browser.keys(['Tab']);
    await browser.pause(1500);
  }
  await advanceTransferWizardToPreview();
}

async function scalar(session: string, sql: string): Promise<number> {
  const rows = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId: session,
    sql,
  });
  return Number(queryScalar(rows, 'c'));
}

describe('数据传输结构对象发射 (DT-OBJ)', () => {
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const pgSrcId = `e2e_dt_obj_src_${STAMP}`;
  const pgTgtId = `e2e_dt_obj_pgtgt_${STAMP}`;
  const myTgtId = `e2e_dt_obj_mytgt_${STAMP}`;
  const pgSrcName = `DT-OBJ-PG-Src-${STAMP}`;
  const pgTgtName = `DT-OBJ-PG-Tgt-${STAMP}`;
  const myTgtName = `DT-OBJ-My-Tgt-${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });

    await invokeBackend('save_connection', {
      config: pgConfig(pgSrcId, pgSrcName, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: pgConfig(pgTgtId, pgTgtName, 'datazen_sync_tgt'),
    });
    await invokeBackend('save_connection', {
      config: mysqlConfig(myTgtId, myTgtName, 'datazen_sync_mysql_tgt'),
    });

    const srcSession = await invokeBackend<string>('connect', { connectionId: pgSrcId });
    const mySession = await invokeBackend<string>('connect', { connectionId: myTgtId });
    try {
      await withSafeModeOff(async () => {
        for (const table of [...SRC_TABLES, ...COLLIDE_TABLES]) {
          await invokeBackend('execute_query', {
            dbSessionId: srcSession,
            sql: `DROP TABLE IF EXISTS ${table} CASCADE`,
          });
        }
        for (const table of SRC_TABLES) {
          await invokeBackend('execute_query', {
            dbSessionId: mySession,
            sql: `DROP TABLE IF EXISTS ${table}`,
          });
        }

        const pg = (sql: string) =>
          invokeBackend('execute_query', { dbSessionId: srcSession, sql });
        await pg('CREATE TABLE dt_obj_parent (id INT PRIMARY KEY, label VARCHAR(64) NOT NULL)');
        await pg(`CREATE TABLE dt_obj_child (
            id INT PRIMARY KEY,
            parent_id INT NOT NULL,
            code VARCHAR(64) NOT NULL,
            CONSTRAINT dt_obj_child_parent_fk
              FOREIGN KEY (parent_id) REFERENCES dt_obj_parent(id)
          )`);
        await pg('CREATE INDEX idx_dt_obj_child_code ON dt_obj_child(code)');
        await pg('CREATE UNIQUE INDEX uq_dt_obj_child_parent ON dt_obj_child(parent_id)');
        await pg('CREATE TABLE dt_obj_other (id INT PRIMARY KEY, code VARCHAR(64) NOT NULL)');
        await pg('CREATE INDEX idx_dt_obj_other_code ON dt_obj_other(code)');

        // Two tables sharing one index name: rejected by a schema-scoped target.
        await pg('CREATE TABLE dt_obj_coll_a (id INT PRIMARY KEY, code VARCHAR(64) NOT NULL)');
        await pg('CREATE INDEX idx_dt_obj_shared ON dt_obj_coll_a(code)');
        await pg('CREATE TABLE dt_obj_coll_b (id INT PRIMARY KEY, code VARCHAR(64) NOT NULL)');
        await pg('CREATE INDEX idx_dt_obj_shared ON dt_obj_coll_b(code)');
      });
    } finally {
      await disconnectBackend(srcSession);
      await disconnectBackend(mySession);
    }
  });

  after(async () => {
    const pgIds = [pgSrcId, pgTgtId];
    for (const id of pgIds) {
      try {
        const session = await invokeBackend<string>('connect', { connectionId: id });
        try {
          await withSafeModeOff(async () => {
            await invokeBackend('execute_query', {
              dbSessionId: session,
              sql: `DO $$
                     DECLARE r record;
                     BEGIN
                       FOR r IN SELECT tablename FROM pg_tables
                                 WHERE schemaname = 'public'
                                   AND tablename LIKE 'dt\_obj\_%'
                       LOOP
                         EXECUTE format('DROP TABLE IF EXISTS %I CASCADE', r.tablename);
                       END LOOP;
                     END $$`,
            });
          });
        } finally {
          await disconnectBackend(session);
        }
      } catch {
        /* ok */
      }
    }
    try {
      const session = await invokeBackend<string>('connect', { connectionId: myTgtId });
      try {
        await withSafeModeOff(async () => {
          for (const table of SRC_TABLES) {
            await invokeBackend('execute_query', {
              dbSessionId: session,
              sql: `DROP TABLE IF EXISTS ${table}`,
            });
          }
        });
      } finally {
        await disconnectBackend(session);
      }
    } catch {
      /* ok */
    }
    for (const id of [...pgIds, myTgtId]) {
      try {
        await invokeBackend('delete_connection', { id });
      } catch {
        /* ok */
      }
    }
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('DT-OBJ-1: 结构预览按 表 → 二级索引 → 外键 发射，且 DDL 在目标库真正落库', async () => {
    await runStructureWizard(pgSrcName, myTgtName, SRC_TABLES);
    const preview = await $('[data-testid="data-transfer-preview"]');
    await preview.waitForDisplayed({ timeout: 20000 });

    const indexBlocks = await $$(
      '[data-testid^="data-transfer-ddl-preview-"][data-testid*="-index-"]',
    );
    const fkBlocks = await $$(
      '[data-testid^="data-transfer-ddl-preview-"][data-testid*="-foreignKey-"]',
    );
    expect(indexBlocks.length).toBe(3);
    expect(fkBlocks.length).toBe(1);

    const indexDdl: string[] = [];
    for (const block of indexBlocks) indexDdl.push(await block.getText());
    const allIndexDdl = indexDdl.join('\n');
    expect(allIndexDdl).toContain('idx_dt_obj_child_code');
    expect(allIndexDdl).toContain('uq_dt_obj_child_parent');
    expect(allIndexDdl).toContain('idx_dt_obj_other_code');
    expect(allIndexDdl).toContain('CREATE UNIQUE INDEX');

    const fkDdl = await fkBlocks[0].getText();
    expect(fkDdl).toContain('dt_obj_child_parent_fk');
    expect(fkDdl).toContain('REFERENCES');

    // DOM order must be: all tables, then all indexes, then all foreign keys.
    const blockIds: string[] = [];
    const blocks = await preview.$$('div[data-testid^="data-transfer-ddl-"]');
    for (const block of blocks) blockIds.push((await block.getAttribute('data-testid')) ?? '');
    const lastOf = (needle: string) =>
      blockIds.reduce((acc, id, i) => (id.includes(needle) ? i : acc), -1);
    const firstOf = (needle: string) => blockIds.findIndex((id) => id.includes(needle));
    expect(lastOf('-ddl-editor-')).toBeLessThan(lastOf('-index-'));
    expect(lastOf('-index-')).toBeLessThan(firstOf('-foreignKey-'));

    await (await $('[data-testid="data-transfer-execute"]')).click();
    await $('[data-testid="data-transfer-result"]').waitForDisplayed({ timeout: 60000 });

    const session = await invokeBackend<string>('connect', { connectionId: myTgtId });
    try {
      const indexes = await scalar(
        session,
        `SELECT COUNT(*) AS c FROM information_schema.statistics
          WHERE table_schema = DATABASE()
            AND table_name = 'dt_obj_child'
            AND index_name IN ('idx_dt_obj_child_code', 'uq_dt_obj_child_parent')`,
      );
      const constraints = await scalar(
        session,
        `SELECT COUNT(*) AS c FROM information_schema.referential_constraints
          WHERE constraint_schema = DATABASE() AND table_name = 'dt_obj_child'`,
      );
      expect(indexes).toBe(2);
      expect(constraints).toBe(1);
    } finally {
      await disconnectBackend(session);
    }
  });

  it('DT-OBJ-2: 目标库按 schema 命名空间判定重名时预览失败并指出冲突双方', async () => {
    await runStructureWizard(pgSrcName, pgTgtName, COLLIDE_TABLES);
    const errorBox = await $('[data-testid="data-transfer-preview-error"]');
    await errorBox.waitForDisplayed({ timeout: 20000 });
    const message = await errorBox.getText();
    expect(message).toContain('idx_dt_obj_shared');
    expect(message).toContain('collides between');
    expect(message).toContain('dt_obj_coll_a');
    expect(message).toContain('dt_obj_coll_b');
  });
});
