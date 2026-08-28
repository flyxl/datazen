/**
 * Data Transfer column type mapping → Preview DDL (PG → MySQL, create-new).
 *
 * Requires `bash e2e/setup-sync-dbs.sh` (datazen_sync_src + datazen_sync_mysql_tgt).
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  invokeBackend,
  openDataTransferWindow,
  selectDzOption,
  withSafeModeOff,
} from '../helpers.js';

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

async function clickNext(label: string) {
  const next = await $('[data-testid="data-transfer-next"]');
  await next.waitForClickable({ timeout: 8000 });
  await next.click();
  await browser.pause(1200);
  await captureJourneyStep(label, 0, true);
}

async function getPreviewDdl(sourceTable: string): Promise<string> {
  return browser.execute((table) => {
    const wrap = document.querySelector(`[data-testid="data-transfer-ddl-editor-${table}"]`);
    const cm = wrap?.querySelector('.cm-content');
    return cm?.textContent ?? '';
  }, sourceTable);
}

describe('数据传输类型映射 Preview DDL (DT-TYPE-MAP)', () => {
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_dt_type_src_${STAMP}`;
  const TGT_ID = `e2e_dt_type_tgt_${STAMP}`;
  const SRC_NAME = `DT-Type-PG-${STAMP}`;
  const TGT_NAME = `DT-Type-MySQL-${STAMP}`;
  const TABLE = `dt_type_map_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });

    await invokeBackend('save_connection', {
      config: pgConfig(SRC_ID, SRC_NAME, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: mysqlConfig(TGT_ID, TGT_NAME, 'datazen_sync_mysql_tgt'),
    });

    const srcSession = await invokeBackend<string>('connect', { connectionId: SRC_ID });
    const tgtSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });

    await withSafeModeOff(async () => {
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `DROP TABLE IF EXISTS ${TABLE}`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: tgtSession,
        sql: `DROP TABLE IF EXISTS ${TABLE}`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `CREATE TABLE ${TABLE} (
          id bigint PRIMARY KEY,
          name text NOT NULL DEFAULT 'unnamed',
          created_at timestamptz NOT NULL DEFAULT now()
        )`,
      });
    });
  });

  after(async () => {
    try {
      const srcSession = await invokeBackend<string>('connect', { connectionId: SRC_ID });
      const tgtSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });
      await withSafeModeOff(async () => {
        await invokeBackend('execute_query', {
          dbSessionId: srcSession,
          sql: `DROP TABLE IF EXISTS ${TABLE}`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: tgtSession,
          sql: `DROP TABLE IF EXISTS ${TABLE}`,
        });
      });
    } catch {
      /* ok */
    }
    try {
      await invokeBackend('delete_connection', { id: SRC_ID });
    } catch {
      /* ok */
    }
    try {
      await invokeBackend('delete_connection', { id: TGT_ID });
    } catch {
      /* ok */
    }
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('DT-TYPE-001: 映射 VARCHAR(64)/DATETIME 应出现在 Preview CREATE TABLE', async () => {
    await openDataTransferWindow();
    await selectDzOption(t('transfer.pickConnection'), SRC_NAME);
    await selectDzOption(t('transfer.pickConnection'), TGT_NAME);
    await browser.pause(1500);

    await clickNext('dt-type-endpoints');

    const structureMode = await $('[data-testid="data-transfer-mode-structure"]');
    await structureMode.click();
    await browser.pause(300);
    await clickNext('dt-type-mode');

    await browser.pause(2000);
    const tableRow = await $(`[data-testid="data-transfer-table-row"]*=${TABLE}`);
    await tableRow.waitForDisplayed({ timeout: 10000 });
    const checkbox = await tableRow.$('input[type="checkbox"]');
    if (!(await checkbox.isSelected())) {
      await checkbox.click();
    }
    await clickNext('dt-type-objects');

    await $('[data-testid="data-transfer-mapping-step"]').waitForDisplayed({ timeout: 15000 });
    await $('[data-testid="data-transfer-column-editor"]').waitForDisplayed({ timeout: 15000 });

    const createNewToggle = await $('[data-testid="data-transfer-create-new-toggle"]');
    await createNewToggle.waitForDisplayed({ timeout: 8000 });
    if (!(await createNewToggle.isSelected())) {
      await createNewToggle.click();
    }
    const targetTableInput = await $('[data-testid="data-transfer-target-table-input"]');
    await targetTableInput.click();
    await browser.keys(['Tab']);
    await browser.pause(2000);

    const createdTypeInput = await $('[data-testid="data-transfer-target-type-created_at"]');
    await createdTypeInput.waitForDisplayed({ timeout: 15000 });
    await captureJourneyStep('dt-type-mapping-create-new', 0, true);
    const createdTypeValue = await createdTypeInput.getValue();
    expect(createdTypeValue.toUpperCase()).toContain('DATETIME');

    const nameTypeInput = await $('[data-testid="data-transfer-target-type-name"]');
    await nameTypeInput.waitForDisplayed({ timeout: 8000 });
    await captureJourneyStep('dt-type-mapping-with-types', 0, true);
    await nameTypeInput.clearValue();
    await nameTypeInput.setValue('VARCHAR(64)');
    await browser.pause(300);
    await captureJourneyStep('dt-type-mapping-edited', 0, true);

    await clickNext('dt-type-mapping-after');

    await $('[data-testid="data-transfer-preview"]').waitForDisplayed({ timeout: 15000 });
    await browser.pause(800);

    const ddl = await getPreviewDdl(TABLE);
    expect(ddl.length).toBeGreaterThan(10);
    expect(ddl.toUpperCase()).toContain('VARCHAR(64)');
    expect(ddl.toUpperCase()).toContain('DATETIME');
    expect(ddl).not.toContain('16383');
    expect(ddl.toUpperCase()).toContain('DEFAULT');

    await captureJourneyStep('dt-type-preview-ddl', 0, true);
  });
});
