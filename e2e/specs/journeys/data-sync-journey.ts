/**
 * Data Sync Diff Workspace full user journey — direct URL entry (hidden in v0.1.0 UI).
 *
 * Flow: Open → options → endpoints → compare → mapping → preview → execute → verify rows.
 * Requires PostgreSQL sync DBs (`e2e/setup-sync-dbs.sh`).
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  connectConfig,
  executeQuery,
  invokeBackend,
  queryScalar,
  selectDzOption,
  withSafeModeOff,
} from '../../helpers.js';

async function openDataSyncWindow() {
  await browser.url('tauri://localhost/window.html?window=data-sync');
  await browser.pause(1500);
  await $('[data-testid="data-sync-compare"]').waitForDisplayed({ timeout: 10000 });
}

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

describe('数据同步完整用户旅程 (DS-JOURNEY)', () => {
  /**
   * Documented user flow (single sequential session; screenshot each step with --screenshot):
   * 1. Open Data Sync window via direct URL
   * 2. Toggle sync options (insert/update/delete) and database selectors
   * 3. Compare without endpoints → selectBoth error
   * 4. Select source/target PG sync connections
   * 5. Run compare → mapping rows appear
   * 6. Review summary and SQL preview tab
   * 7. Switch to row diff detail panel
   * 8. Execute sync and verify target row count matches source
   */
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_ds_j_src_${STAMP}`;
  const TGT_ID = `e2e_ds_j_tgt_${STAMP}`;
  const SRC_NAME = `DS-J-Src-${STAMP}`;
  const TGT_NAME = `DS-J-Tgt-${STAMP}`;
  const TABLE = `ds_journey_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });

    await invokeBackend('save_connection', {
      config: pgConfig(SRC_ID, SRC_NAME, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: pgConfig(TGT_ID, TGT_NAME, 'datazen_sync_tgt'),
    });

    const srcSession = await connectConfig(SRC_ID);
    const tgtSession = await connectConfig(TGT_ID);

    await withSafeModeOff(async () => {
      await executeQuery(srcSession, `DROP TABLE IF EXISTS ${TABLE}`);
      await executeQuery(tgtSession, `DROP TABLE IF EXISTS ${TABLE}`);
      await executeQuery(
        srcSession,
        `CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL)`,
      );
      await executeQuery(
        tgtSession,
        `CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL)`,
      );
      await executeQuery(
        srcSession,
        `INSERT INTO ${TABLE} (id, name) VALUES (1,'a'),(2,'b'),(3,'c'),(4,'d'),(5,'e')`,
      );
      await executeQuery(
        tgtSession,
        `INSERT INTO ${TABLE} (id, name) VALUES (1,'a'),(2,'b'),(3,'c')`,
      );
    });
  });

  after(async () => {
    try {
      const srcSession = await connectConfig(SRC_ID);
      const tgtSession = await connectConfig(TGT_ID);
      await withSafeModeOff(async () => {
        await executeQuery(srcSession, `DROP TABLE IF EXISTS ${TABLE}`);
        await executeQuery(tgtSession, `DROP TABLE IF EXISTS ${TABLE}`);
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

  it('Step 1: 通过 URL 打开数据同步窗口', async () => {
    await openDataSyncWindow();
    await expect(await $('[data-testid="data-sync-compare"]')).toBeDisplayed();
    await captureJourneyStep('ds-journey-01-window-open', 0, true);
  });

  it('Step 2: 切换同步选项并显示数据库选择器', async () => {
    const updateOpt = await $('[data-testid="data-sync-option-update"]');
    await updateOpt.click();
    await browser.pause(300);
    await expect(await $('[data-testid="data-sync-source-database"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-target-database"]')).toBeDisplayed();
    await captureJourneyStep('ds-journey-02-options-toggled', 0, true);
  });

  it('Step 3: 未选两端点比较应提示 selectBoth', async () => {
    const compare = await $('[data-testid="data-sync-compare"]');
    await compare.click();
    await browser.pause(500);
    const err = await $('[data-testid="data-sync-error"]');
    await expect(err).toBeDisplayed();
    expect(await err.getText()).toContain(t('sync.selectBoth'));
    await captureJourneyStep('ds-journey-03-select-both-error', 0, true);
    const ok = await $(`button*=${t('common.ok')}`);
    if (await ok.isDisplayed()) {
      await ok.click();
      await browser.pause(200);
    }
  });

  it('Step 4: 选择源/目标同步连接', async () => {
    await selectDzOption(t('sync.selectSource'), SRC_NAME);
    await selectDzOption(t('sync.selectTarget'), TGT_NAME);
    await browser.pause(1500);
    await expect(await $('[data-testid="data-sync-source-database"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-target-database"]')).toBeDisplayed();
    await captureJourneyStep('ds-journey-04-endpoints-selected', 0, true);
  });

  it('Step 5: 比较后应出现映射行', async () => {
    const compare = await $('[data-testid="data-sync-compare"]');
    await compare.click();
    await browser.pause(2500);

    const rows = await $$('[data-testid="data-sync-mapping-row"]');
    let seen = false;
    for (const r of rows) {
      if (((await r.getText()) || '').includes(TABLE)) {
        seen = true;
        break;
      }
    }
    expect(seen).toBe(true);
    await expect(await $('[data-testid="data-sync-summary"]')).toBeDisplayed();
    await captureJourneyStep('ds-journey-05-mapping-rows', 0, true);
  });

  it('Step 6: 查看 SQL 预览标签页', async () => {
    const previewTab = await $(`button*=${t('sync.sqlPreviewTab')}`);
    await previewTab.click();
    await browser.pause(600);
    await expect(await $('[data-testid="data-sync-preview"]')).toBeDisplayed();
    await captureJourneyStep('ds-journey-06-sql-preview', 0, true);
  });

  it('Step 7: 切换到行级差异详情', async () => {
    const rowDiffTab = await $(`button*=${t('sync.rowDiffTab')}`);
    await rowDiffTab.click();
    await browser.pause(400);
    await expect(await $('[data-testid="data-sync-row-diff"]')).toBeDisplayed();
    await captureJourneyStep('ds-journey-07-row-diff', 0, true);
  });

  it('Step 8: 执行同步并验证目标行数', async () => {
    const start = await $('[data-testid="data-sync-start"]');
    await start.waitForClickable({ timeout: 15000 });
    await start.click();
    await browser.pause(3000);

    await expect(await $('[data-testid="data-sync-summary"]')).toBeDisplayed();
    await captureJourneyStep('ds-journey-08-execute-complete', 0, true);

    const tgtSession = await connectConfig(TGT_ID);
    const rows = await executeQuery(tgtSession, `SELECT count(*)::int AS c FROM ${TABLE}`);
    expect(queryScalar(rows, 'c')).toBe(5);
    await captureJourneyStep('ds-journey-09-rows-verified', 0, true);
  });
});
