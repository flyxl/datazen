/**
 * Data Transfer comprehensive E2E: wide column types + multi-batch row volume.
 *
 * Uses e2e/lib/dataTransferFixtures.ts (25000 rows, 19 columns).
 * Preflight: bash e2e/setup-data-transfer-e2e.sh (also run via pnpm e2e:data-transfer).
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  invokeBackend,
  openDataTransferWindow,
  selectDzOptionInWrap,
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

/** 失败现场的取证：向导停在哪个步骤、Next 是否在位/可用、可见哪些 testid。 */
async function describeTransferState(): Promise<string> {
  return browser.execute(() => {
    const button = document.querySelector(
      '[data-testid="data-transfer-next"]',
    ) as HTMLButtonElement | null;
    const testIds = Array.from(document.querySelectorAll('[data-testid]'))
      .map((node) => node.getAttribute('data-testid') ?? '')
      .filter((id) => id.startsWith('data-transfer'))
      .slice(0, 40)
      .join(',');
    return [
      `next=${button ? `present disabled=${button.disabled}` : 'absent'}`,
      `testids=${testIds}`,
      `text=${(document.body.innerText ?? '').replace(/\s+/g, ' ').slice(0, 1200)}`,
    ].join(' || ');
  });
}

/**
 * 点 Next 之前先等它**真的可用**。
 *
 * 向导的 Next 由 `canNext && !loading` 决定：端点步要先等双向探测（pairing）返回，
 * 每步 Next 又会触发一次异步准备，期间按钮被禁用。固定 10s 的 `waitForClickable`
 * 会把「探测还没回来」当成「按钮不可点」直接失败，实测同一用例时而红在端点步、
 * 时而红在推进循环里。这里换成显式条件等待（默认 60s），失败时把现场一起抛出。
 */
async function clickNext(label: string, timeout = 60000) {
  let reachedExecute = false;
  try {
    await browser.waitUntil(
      async () => {
        // 到了执行步就没有 Next 了：这是**到达**而不是失败。推进途中向导可能
        // 自己走完映射→预览（prepare 回来后自动前进），所以判断要放进等待里，
        // 不能只在点击前看一次——实测整包跑时正是这个窗口把用例判红。
        if (
          await $('[data-testid="data-transfer-execute"]')
            .isExisting()
            .catch(() => false)
        ) {
          reachedExecute = true;
          return true;
        }
        const button = await $('[data-testid="data-transfer-next"]');
        if (!(await button.isExisting().catch(() => false))) return false;
        return (await button.isEnabled().catch(() => false)) === true;
      },
      { timeout, timeoutMsg: 'data-transfer-next 一直不可用' },
    );
  } catch (error) {
    throw new Error(
      `ID:data-transfer-diverse-types|${label}|${(error as Error).message} :: ${await describeTransferState()}`,
    );
  }
  const next = await $('[data-testid="data-transfer-next"]');
  if (reachedExecute && !(await next.isExisting().catch(() => false))) {
    await captureJourneyStep(label, 0, true);
    return;
  }
  await next.click();
  await browser.pause(1200);
  await captureJourneyStep(label, 0, true);
}

/**
 * 一直推进到执行步。每轮先看执行按钮在不在——向导到预览步时 Next 已经没了，
 * 再等它就是等一个不存在的元素（实测超时信息里 `next=absent`，页面停在预览步）。
 */
async function advanceToExecute(label: string, timeoutMs = 120000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  let clicks = 0;
  for (;;) {
    if (
      await $('[data-testid="data-transfer-execute"]')
        .isExisting()
        .catch(() => false)
    )
      return;
    if (Date.now() > deadline) {
      throw new Error(
        `ID:data-transfer-diverse-types|${label}|${timeoutMs}ms 内没到执行步 :: ${await describeTransferState()}`,
      );
    }
    const next = await $('[data-testid="data-transfer-next"]');
    const ready =
      (await next.isExisting().catch(() => false)) && (await next.isEnabled().catch(() => false));
    if (ready) {
      await clickNext(`${label}-${clicks}`);
      clicks += 1;
      continue;
    }
    await browser.pause(500);
  }
}

async function runDataTransferWizard(sourceName: string, targetName: string, tableName: string) {
  await openDataTransferWindow();
  await selectDzOptionInWrap('data-transfer-source', sourceName);
  await selectDzOptionInWrap('data-transfer-target', targetName);
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

  // 对象步 → 映射步 → 预览 → 执行。到执行步 Next 就不存在了，所以循环的
  // 终止条件必须是「执行按钮出现」而不是「点满 N 次」：盲点会把已经到位的
  // 向导再点一次，随后等一个不存在的 Next 直到超时。
  await advanceToExecute('dt-comp-advance');

  const execute = await $('[data-testid="data-transfer-execute"]');
  const pageText = (await $('body').getText()).slice(-1800);
  await execute.waitForClickable({
    timeout: 20000,
    timeoutMsg: `Transfer execute was disabled. Page state: ${pageText}`,
  });
  await execute.click();
  await browser.pause(Math.min(30000, 3000 + Math.floor(TRANSFER_E2E_ROW_COUNT / 200) * 100));

  const result = await $('[data-testid="data-transfer-result"]');
  await result.waitForDisplayed({ timeout: 120000 });
  await browser.waitUntil(async () => (await result.getAttribute('data-completed')) === 'true', {
    timeout: 180000,
    timeoutMsg: 'Wide-type transfer did not reach a verified completed result',
  });
  expect(await result.getAttribute('data-verdict-severity')).toBe('ok');
  expect(await result.getText()).not.toContain(t('transfer.error'));
}

describe('数据传输限制说明 (DT-LIM)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });
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
  this.timeout(300000);
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
  this.timeout(300000);
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
