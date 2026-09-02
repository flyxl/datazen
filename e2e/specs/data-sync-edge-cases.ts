/**
 * Data Sync Diff Workspace — validation errors, edge cases, and post-compare UI.
 * Every step captures a screenshot when run with `--screenshot` (`pnpm e2e:data-sync`).
 *
 * PG-backed cases require `e2e/setup-sync-dbs.sh` (`datazen_sync_src` / `datazen_sync_tgt`).
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  connectConfig,
  executeQuery,
  invokeBackend,
  selectDzOption,
  selectDzOptionInWrap,
  withSafeModeOff,
} from '../helpers.js';
import {
  advanceDataSyncToPreview,
  advanceDataSyncToSetup,
  compareDataSyncObjects,
  inspectDataSyncObjects,
  moveDataSyncBackTo,
} from './journeys/dataSyncJourneyHelpers.js';

async function openDataSyncWindow() {
  await browser.url('tauri://localhost/window.html?window=data-sync');
  await browser.pause(1500);
  await $('[data-testid="data-sync-window"]').waitForDisplayed({ timeout: 10000 });
  await browser.waitUntil(
    async () =>
      (await $('[data-testid="data-sync-window"]').getAttribute('data-sync-step')) === 'endpoints',
    { timeout: 10000, timeoutMsg: 'data-sync wizard did not open on endpoints step' },
  );
}

async function captureStep(label: string) {
  await captureJourneyStep(label, 0, true);
}

async function dismissOkDialog() {
  const ok = await $(`button*=${t('common.ok')}`);
  if (await ok.isDisplayed().catch(() => false)) {
    await ok.click();
    await browser.pause(200);
  }
}

async function getSyncState(): Promise<string | null> {
  const win = await $('[data-testid="data-sync-window"]');
  return win.getAttribute('data-sync-state');
}

async function waitForSyncStateNotBusy(timeoutMsg: string) {
  await browser.waitUntil(
    async () => {
      const state = await getSyncState();
      return state !== 'inspecting' && state !== 'comparing' && state !== 'executing';
    },
    { timeout: 15000, timeoutMsg },
  );
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

describe('数据同步边界与异常 (DS-EDGE)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('DS-EDGE-001: 未选端点时下一步应禁用', async () => {
    await openDataSyncWindow();
    await expect(await $('[data-testid="data-sync-next"]')).toBeDisabled();
    await expect(await $('[data-testid="data-sync-error"]')).not.toBeExisting();
    await captureStep('ds-edge-01-select-both');
  });

  it('DS-EDGE-002: 同源同库比较应提示 cannotSameDb', async () => {
    const STAMP = Date.now().toString(36);
    const ID = `e2e_ds_same_${STAMP}`;
    const NAME = `DS-Same-${STAMP}`;
    await invokeBackend('save_connection', {
      config: pgConfig(ID, NAME, 'datazen_sync_src'),
    });

    await openDataSyncWindow();
    await selectDzOption(t('sync.selectSource'), NAME);
    await selectDzOption(t('sync.selectTarget'), NAME);
    await browser.pause(800);
    await advanceDataSyncToSetup();
    await $('[data-testid="data-sync-next"]').click();
    await browser.pause(500);
    const err = await $('[data-testid="data-sync-error"]');
    await expect(err).toBeDisplayed();
    expect(await err.getText()).toContain(t('sync.cannotSameDb'));
    await captureStep('ds-edge-02-same-endpoint');
    await dismissOkDialog();

    try {
      await invokeBackend('delete_connection', { id: ID });
    } catch {
      /* ok */
    }
  });

  it('DS-EDGE-003: 仅选源端点时下一步应禁用', async () => {
    const STAMP = Date.now().toString(36);
    const ID = `e2e_ds_src_only_${STAMP}`;
    await invokeBackend('save_connection', {
      config: pgConfig(ID, `DS-SrcOnly-${STAMP}`, 'datazen_sync_src'),
    });

    await openDataSyncWindow();
    await selectDzOption(t('sync.selectSource'), `DS-SrcOnly-${STAMP}`);
    await browser.pause(500);
    await expect(await $('[data-testid="data-sync-next"]')).toBeDisabled();
    await expect(await $('[data-testid="data-sync-error"]')).not.toBeExisting();
    await captureStep('ds-edge-03-source-only');

    try {
      await invokeBackend('delete_connection', { id: ID });
    } catch {
      /* ok */
    }
  });

  it('DS-EDGE-004: PG 源下 MySQL 目标在列表中标记为 unsupportedPair', async () => {
    const STAMP = Date.now().toString(36);
    const PG_ID = `e2e_ds_pg_src_${STAMP}`;
    const MY_ID = `e2e_ds_my_tgt_${STAMP}`;
    await invokeBackend('save_connection', {
      config: pgConfig(PG_ID, `DS-PG-${STAMP}`, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: {
        id: MY_ID,
        name: `DS-MySQL-${STAMP}`,
        databaseType: 'mysql',
        host: process.env.E2E_MYSQL_HOST || '127.0.0.1',
        port: Number(process.env.E2E_MYSQL_PORT) || 3306,
        username: process.env.E2E_MYSQL_USER || 'root',
        password: process.env.E2E_MYSQL_PASSWORD || '',
        database: process.env.E2E_MYSQL_DB || 'datazen_test',
        sslMode: 'disable',
      },
    });

    await openDataSyncWindow();
    await selectDzOption(t('sync.selectSource'), `DS-PG-${STAMP}`);
    await browser.pause(500);

    await browser.execute(() => {
      const wrap = document.querySelector('[data-testid="data-sync-target"]');
      const btn = wrap?.querySelector('button[aria-haspopup="listbox"]') as HTMLElement | null;
      btn?.click();
    });
    await browser.waitUntil(
      async () => {
        const list = await $('#dz-select-listbox');
        return list.isDisplayed().catch(() => false);
      },
      { timeout: 5000, timeoutMsg: 'target listbox did not open' },
    );
    const listText = await $('#dz-select-listbox').getText();
    expect(listText).toContain('DS-MySQL');
    expect(listText).toContain(t('common.unsupportedPair'));
    await captureStep('ds-edge-04-mysql-unsupported-label');

    for (const id of [PG_ID, MY_ID]) {
      try {
        await invokeBackend('delete_connection', { id });
      } catch {
        /* ok */
      }
    }
  });

  it('DS-EDGE-005: 端点页应把 Delete 选项放到后续 setup 步骤', async () => {
    await openDataSyncWindow();
    await expect(await $('[data-testid="data-sync-option-delete"]')).not.toBeExisting();
    await captureStep('ds-edge-05-delete-gated');
  });

  it('DS-EDGE-006: 向导端点页不显示 Swap 操作', async () => {
    await openDataSyncWindow();
    await expect(await $('[data-testid="data-sync-swap"]')).not.toBeExisting();
    await captureStep('ds-edge-06-no-swap');
  });

  it('DS-EDGE-012: 比较过程中 Cancel 应中止并提示 compare cancelled', async () => {
    const STAMP = Date.now().toString(36);
    const SRC_ID = `e2e_ds_cancel_src_${STAMP}`;
    const TGT_ID = `e2e_ds_cancel_tgt_${STAMP}`;
    await invokeBackend('save_connection', {
      config: pgConfig(SRC_ID, `DS-Cancel-Src-${STAMP}`, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: pgConfig(TGT_ID, `DS-Cancel-Tgt-${STAMP}`, 'datazen_sync_tgt'),
    });

    await openDataSyncWindow();
    await selectDzOption(t('sync.selectSource'), `DS-Cancel-Src-${STAMP}`);
    await selectDzOption(t('sync.selectTarget'), `DS-Cancel-Tgt-${STAMP}`);
    await browser.pause(1500);
    await advanceDataSyncToSetup();
    await inspectDataSyncObjects();
    await $('[data-testid="data-sync-next"]').click();
    await browser.waitUntil(
      async () =>
        (await $('[data-testid="data-sync-window"]').getAttribute('data-sync-step')) === 'compare',
      { timeout: 10000, timeoutMsg: 'data-sync compare step did not open' },
    );
    const cancel = await $('[data-testid="data-sync-cancel"]');
    const sawCancel = await cancel
      .waitForDisplayed({ timeout: 8000 })
      .then(() => true)
      .catch(() => false);
    expect(sawCancel).toBe(true);
    await cancel.click();
    await waitForSyncStateNotBusy('compare cancel did not restore idle/compared state');
    const state = await getSyncState();
    expect(state).not.toBe('inspecting');
    expect(state).not.toBe('comparing');
    await expect(await $('[data-testid="data-sync-next"]')).toBeDisplayed();
    expect(await $('[data-testid="data-sync-next"]').getAttribute('disabled')).toBe(null);
    expect(await $('body').getText()).toContain(t('sync.compareCancelled'));
    await captureStep('ds-edge-12-compare-cancelled');

    for (const id of [SRC_ID, TGT_ID]) {
      try {
        await invokeBackend('delete_connection', { id });
      } catch {
        /* ok */
      }
    }
  });

  it('DS-EDGE-013: 只读目标应禁用 Execute 并显示 targetReadOnly', async () => {
    const STAMP = Date.now().toString(36);
    const SRC_ID = `e2e_ds_ro_src_${STAMP}`;
    const TGT_ID = `e2e_ds_ro_tgt_${STAMP}`;
    const TABLE = `e2e_ds_ro_${STAMP}`;
    await invokeBackend('save_connection', {
      config: pgConfig(SRC_ID, `DS-RO-Src-${STAMP}`, 'datazen_sync_src'),
    });
    const SETUP_TGT_ID = `e2e_ds_ro_setup_${STAMP}`;
    await invokeBackend('save_connection', {
      config: pgConfig(SETUP_TGT_ID, `DS-RO-Setup-${STAMP}`, 'datazen_sync_tgt'),
    });
    await invokeBackend('save_connection', {
      config: {
        ...pgConfig(TGT_ID, `DS-RO-Tgt-${STAMP}`, 'datazen_sync_tgt'),
        readOnly: true,
      },
    });

    const srcSession = await connectConfig(SRC_ID);
    const setupTgtSession = await connectConfig(SETUP_TGT_ID);
    await withSafeModeOff(async () => {
      await executeQuery(srcSession, `DROP TABLE IF EXISTS ${TABLE}`);
      await executeQuery(setupTgtSession, `DROP TABLE IF EXISTS ${TABLE}`);
      await executeQuery(
        srcSession,
        `CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL)`,
      );
      await executeQuery(
        setupTgtSession,
        `CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL)`,
      );
      await executeQuery(
        srcSession,
        `INSERT INTO ${TABLE} (id, name) VALUES (1,'a'),(2,'b'),(3,'c')`,
      );
      await executeQuery(setupTgtSession, `INSERT INTO ${TABLE} (id, name) VALUES (1,'a')`);
    });

    await openDataSyncWindow();
    await selectDzOption(t('sync.selectSource'), `DS-RO-Src-${STAMP}`);
    await selectDzOption(t('sync.selectTarget'), `DS-RO-Tgt-${STAMP}`);
    await browser.pause(1500);
    await advanceDataSyncToSetup();
    await inspectDataSyncObjects();
    await compareDataSyncObjects();
    await $('[data-testid="data-sync-summary"]').waitForDisplayed({ timeout: 20000 });
    await advanceDataSyncToPreview();

    await expect(await $('[data-testid="data-sync-start-disabled"]')).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('sync.targetReadOnly'));
    await captureStep('ds-edge-13-readonly-target');

    try {
      await withSafeModeOff(async () => {
        await executeQuery(srcSession, `DROP TABLE IF EXISTS ${TABLE}`);
        await executeQuery(setupTgtSession, `DROP TABLE IF EXISTS ${TABLE}`);
      });
    } catch {
      /* ok */
    }
    for (const id of [SRC_ID, TGT_ID, SETUP_TGT_ID]) {
      try {
        await invokeBackend('delete_connection', { id });
      } catch {
        /* ok */
      }
    }
  });

  it('DS-EDGE-015: 变更源连接应清除 mapping', async () => {
    const STAMP = Date.now().toString(36);
    const SRC_ID = `e2e_ds_chg_src_${STAMP}`;
    const ALT_SRC_ID = `e2e_ds_chg_alt_${STAMP}`;
    const TGT_ID = `e2e_ds_chg_tgt_${STAMP}`;
    const TABLE = `e2e_ds_chg_${STAMP}`;
    await invokeBackend('save_connection', {
      config: pgConfig(SRC_ID, `DS-Chg-Src-${STAMP}`, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: pgConfig(ALT_SRC_ID, `DS-Chg-Alt-${STAMP}`, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: pgConfig(TGT_ID, `DS-Chg-Tgt-${STAMP}`, 'datazen_sync_tgt'),
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
      await executeQuery(srcSession, `INSERT INTO ${TABLE} (id, name) VALUES (1,'a'),(2,'b')`);
      await executeQuery(tgtSession, `INSERT INTO ${TABLE} (id, name) VALUES (1,'a')`);
    });

    await openDataSyncWindow();
    await selectDzOption(t('sync.selectSource'), `DS-Chg-Src-${STAMP}`);
    await selectDzOption(t('sync.selectTarget'), `DS-Chg-Tgt-${STAMP}`);
    await browser.pause(1500);
    await advanceDataSyncToSetup();
    await inspectDataSyncObjects();
    await compareDataSyncObjects();
    await $('[data-testid="data-sync-summary"]').waitForDisplayed({ timeout: 20000 });
    const rowCountBefore = await browser.execute(
      () => document.querySelectorAll('[data-testid="data-sync-mapping-row"]').length,
    );
    expect(rowCountBefore).toBeGreaterThan(0);

    await moveDataSyncBackTo('endpoints');
    await selectDzOptionInWrap('data-sync-source', `DS-Chg-Alt-${STAMP}`);
    await browser.pause(800);
    expect(await getSyncState()).toBe('idle');
    const rowCountAfter = await browser.execute(
      () => document.querySelectorAll('[data-testid="data-sync-mapping-row"]').length,
    );
    expect(rowCountAfter).toBe(0);
    await expect(await $('[data-testid="data-sync-step-endpoints"]')).toBeDisplayed();
    await captureStep('ds-edge-15-source-change-clears-mapping');

    try {
      await withSafeModeOff(async () => {
        await executeQuery(srcSession, `DROP TABLE IF EXISTS ${TABLE}`);
        await executeQuery(tgtSession, `DROP TABLE IF EXISTS ${TABLE}`);
      });
    } catch {
      /* ok */
    }
    for (const id of [SRC_ID, ALT_SRC_ID, TGT_ID]) {
      try {
        await invokeBackend('delete_connection', { id });
      } catch {
        /* ok */
      }
    }
  });
});

describe('数据同步比较后边界 (DS-EDGE-POST)', () => {
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_ds_edge_src_${STAMP}`;
  const TGT_ID = `e2e_ds_edge_tgt_${STAMP}`;
  const SRC_NAME = `DS-Edge-Src-${STAMP}`;
  const TGT_NAME = `DS-Edge-Tgt-${STAMP}`;
  const TABLE = `e2e_ds_edge_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
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

    await openDataSyncWindow();
    await selectDzOption(t('sync.selectSource'), SRC_NAME);
    await selectDzOption(t('sync.selectTarget'), TGT_NAME);
    await browser.pause(1500);
    await advanceDataSyncToSetup();
    await inspectDataSyncObjects();
    await compareDataSyncObjects();
    await captureStep('ds-edge-post-00-compared');
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
    for (const id of [SRC_ID, TGT_ID]) {
      try {
        await invokeBackend('delete_connection', { id });
      } catch {
        /* ok */
      }
    }
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('DS-EDGE-007: 关闭 Insert 后 Execute 应禁用（仅 INSERT diff）', async () => {
    await moveDataSyncBackTo('setup');
    const insertOpt = await $('[data-testid="data-sync-option-insert"]');
    if (await insertOpt.isSelected()) {
      await insertOpt.click();
      await browser.pause(300);
    }
    await inspectDataSyncObjects();
    await compareDataSyncObjects();
    await advanceDataSyncToPreview();
    await expect(await $('[data-testid="data-sync-start-disabled"]')).toBeDisplayed();
    expect(await $('[data-testid="data-sync-start-disabled"]').getAttribute('title')).toContain(
      t('sync.executeUnavailable'),
    );
    await captureStep('ds-edge-post-01-insert-off-execute-disabled');
    await moveDataSyncBackTo('setup');
    await $('[data-testid="data-sync-option-insert"]').click();
    await browser.pause(300);
    await inspectDataSyncObjects();
    await compareDataSyncObjects();
  });

  it('DS-EDGE-008: 表搜索应过滤左侧列表', async () => {
    const search = await $('input[type="search"]');
    await search.waitForDisplayed({ timeout: 10000 });
    await search.setValue(TABLE);
    await browser.pause(500);
    expect(await $('body').getText()).toContain(TABLE);
    await search.setValue('__no_such_table_xyz__');
    await browser.pause(500);
    expect(await $('body').getText()).toContain(t('sync.noTablesMatch'));
    await search.setValue('');
    await browser.pause(200);
    await captureStep('ds-edge-post-02-table-search');
  });

  it('DS-EDGE-009: 取消映射表勾选后 Execute 应禁用', async () => {
    const unchecked = await browser.execute((tableName: string) => {
      let count = 0;
      const rows = document.querySelectorAll('[data-testid="data-sync-mapping-row"]');
      for (const row of rows) {
        const cb = row.querySelector('input[type="checkbox"]') as HTMLInputElement | null;
        if (!cb || cb.disabled || !cb.checked) continue;
        cb.click();
        count += 1;
        if ((row.textContent || '').includes(tableName)) break;
      }
      return count;
    }, TABLE);
    expect(unchecked).toBeGreaterThan(0);
    await browser.pause(400);
    await expect(await $('[data-testid="data-sync-next"]')).toBeDisabled();
    await captureStep('ds-edge-post-03-mapping-unchecked');
    await moveDataSyncBackTo('objects');
    await browser.execute(() => {
      document
        .querySelectorAll('[data-testid="data-sync-mapping-row"] input[type="checkbox"]')
        .forEach((cb) => {
          const input = cb as HTMLInputElement;
          if (!input.disabled && !input.checked) input.click();
        });
    });
    await browser.pause(300);
    await compareDataSyncObjects();
  });

  it('DS-EDGE-010: Copy report 按钮应可点击', async () => {
    const copyBtn = await $('[data-testid="data-sync-copy-report"]');
    await expect(copyBtn).toBeDisplayed();
    await copyBtn.click();
    await browser.pause(200);
    await captureStep('ds-edge-post-04-copy-report');
  });

  it('DS-EDGE-011: Insert 过滤器应只显示有 INSERT diff 的表', async () => {
    const insertFilter = await $(`button*=${t('sync.filter.insert')}`);
    await insertFilter.click();
    await browser.pause(300);
    const body = await $('body').getText();
    expect(body).toContain(TABLE);
    await captureStep('ds-edge-post-05-insert-filter');
    const allFilter = await $(`button*=${t('sync.filter.all')}`);
    await allFilter.click();
    await browser.pause(200);
  });

  it('DS-EDGE-014: 执行后应显示 executeDone 成功 banner', async () => {
    await advanceDataSyncToPreview();
    const start = await $('[data-testid="data-sync-start"]');
    await start.waitForClickable({ timeout: 20000 });
    await start.click();
    await browser.waitUntil(
      async () => {
        const cancel = await $('[data-testid="data-sync-cancel"]');
        return !(await cancel.isDisplayed().catch(() => false));
      },
      { timeout: 120000, timeoutMsg: 'execute did not finish' },
    );
    const err = await $('[data-testid="data-sync-error"]');
    if (await err.isDisplayed().catch(() => false)) {
      throw new Error(`execute error: ${await err.getText()}`);
    }
    const doneBanner = await $('[data-testid="data-sync-execute-done"]');
    await expect(doneBanner).toBeDisplayed();
    expect(await doneBanner.getText()).toContain(t('sync.executeDone'));
    expect(await getSyncState()).toBe('done');
    await captureStep('ds-edge-post-06-execute-done');
  });
});
