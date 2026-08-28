/**
 * Data Transfer comprehensive E2E: wide column types + multi-batch row volume.
 *
 * Uses e2e/lib/dataTransferFixtures.ts (2500 rows, 19 columns).
 * Preflight: bash e2e/setup-data-transfer-e2e.sh (also run via pnpm e2e:data-transfer).
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  invokeBackend,
  openDataTransferWindow,
  selectDzOption,
} from '../helpers.js';
import {
  TRANSFER_E2E_ROW_COUNT,
  PG_SYNC_DB,
  PG_SYNC_TGT_DB,
  MYSQL_SYNC_DB,
  assertWideTableRowCount,
  mysqlConnectionConfig,
  pgConnectionConfig,
  setupEmptyTargetTable,
  setupMysqlSourceWideTable,
  setupPgSourceWideTable,
  teardownTransferFixture,
} from '../lib/dataTransferFixtures.js';

async function clickNext(label: string) {
  const next = await $('[data-testid="data-transfer-next"]');
  await next.waitForClickable({ timeout: 10000 });
  await next.click();
  await browser.pause(1200);
  await captureJourneyStep(label, 0, true);
}

async function runDataTransferWizard(sourceName: string, targetName: string, tableName: string) {
  await openDataTransferWindow();
  await selectDzOption(t('transfer.pickConnection'), sourceName);
  await selectDzOption(t('transfer.pickConnection'), targetName);
  await browser.pause(1500);
  await clickNext('dt-comp-endpoints');

  await (await $('[data-testid="data-transfer-mode-data"]')).click();
  await browser.pause(300);
  await clickNext('dt-comp-setup');

  await browser.pause(2000);
  const tableRow = await $(`[data-testid="data-transfer-table-row"]*=${tableName}`);
  await tableRow.waitForDisplayed({ timeout: 20000 });
  const checkbox = await tableRow.$('input[type="checkbox"]');
  if (!(await checkbox.isSelected())) await checkbox.click();
  await clickNext('dt-comp-objects');

  for (let i = 0; i < 10; i++) {
    const execute = await $('[data-testid="data-transfer-execute"]');
    if (await execute.isExisting().catch(() => false)) break;
    const next = await $('[data-testid="data-transfer-next"]');
    if (!(await next.isEnabled())) await browser.pause(1000);
    await clickNext(`dt-comp-advance-${i}`);
  }

  const execute = await $('[data-testid="data-transfer-execute"]');
  await execute.waitForClickable({ timeout: 20000 });
  await execute.click();
  await browser.pause(Math.min(15000, 3000 + TRANSFER_E2E_ROW_COUNT * 4));

  const result = await $('[data-testid="data-transfer-result"]');
  await result.waitForDisplayed({ timeout: 60000 });
  expect(await result.getText()).not.toContain(t('transfer.error'));
}

describe('数据传输限制说明 (DT-LIM)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('DT-LIM-001: 应通过弹窗显示当前版本限制说明', async () => {
    await openDataTransferWindow({ dismissLimitations: false });
    const dialog = await $('[data-testid="data-transfer-limitations-dialog"]');
    await dialog.waitForDisplayed({ timeout: 8000 });
    const panel = await dialog.$('[data-testid="data-transfer-limitations"]');
    await panel.waitForDisplayed({ timeout: 8000 });
    const dialogText = await dialog.getText();
    expect(dialogText).toContain(t('transfer.limitations.title'));
    expect(dialogText).toContain(t('transfer.limitations.noFkIndexes'));
  });
});

describe(`PG→MySQL 宽类型 ${TRANSFER_E2E_ROW_COUNT} 行 (DT-COMP-PG-MYSQL)`, function () {
  this.timeout(180000);
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_dt_comp_pg_${STAMP}`;
  const TGT_ID = `e2e_dt_comp_mysql_${STAMP}`;
  const SRC_NAME = `DT-Comp-PG-${STAMP}`;
  const TGT_NAME = `DT-Comp-MySQL-${STAMP}`;
  const TABLE = `dt_comp_pg_mysql_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(SRC_ID, SRC_NAME, PG_SYNC_DB),
    });
    await invokeBackend('save_connection', {
      config: mysqlConnectionConfig(TGT_ID, TGT_NAME, MYSQL_SYNC_DB),
    });
    await setupPgSourceWideTable(SRC_ID, TABLE, TRANSFER_E2E_ROW_COUNT);
    await setupEmptyTargetTable(TGT_ID, TABLE, 'mysql');
  });

  after(async () => {
    await teardownTransferFixture([SRC_ID, TGT_ID], TABLE);
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it(`DT-COMP-001: 应迁移 ${TRANSFER_E2E_ROW_COUNT} 行宽类型数据`, async () => {
    await runDataTransferWizard(SRC_NAME, TGT_NAME, TABLE);
    await assertWideTableRowCount(TGT_ID, TABLE, 'mysql', TRANSFER_E2E_ROW_COUNT);
  });
});

describe(`MySQL→PG 宽类型 ${TRANSFER_E2E_ROW_COUNT} 行 (DT-COMP-MYSQL-PG)`, function () {
  this.timeout(180000);
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_dt_comp_mysql_src_${STAMP}`;
  const TGT_ID = `e2e_dt_comp_pg_tgt_${STAMP}`;
  const SRC_NAME = `DT-Comp-MySQL-Src-${STAMP}`;
  const TGT_NAME = `DT-Comp-PG-Tgt-${STAMP}`;
  const TABLE = `dt_comp_mysql_pg_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: mysqlConnectionConfig(SRC_ID, SRC_NAME, MYSQL_SYNC_DB),
    });
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(TGT_ID, TGT_NAME, PG_SYNC_TGT_DB),
    });
    await setupMysqlSourceWideTable(SRC_ID, TABLE, TRANSFER_E2E_ROW_COUNT);
    await setupEmptyTargetTable(TGT_ID, TABLE, 'postgresql');
  });

  after(async () => {
    await teardownTransferFixture([SRC_ID, TGT_ID], TABLE);
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it(`DT-COMP-002: 应迁移 ${TRANSFER_E2E_ROW_COUNT} 行宽类型数据`, async () => {
    await runDataTransferWizard(SRC_NAME, TGT_NAME, TABLE);
    await assertWideTableRowCount(TGT_ID, TABLE, 'postgresql', TRANSFER_E2E_ROW_COUNT);
  });
});
