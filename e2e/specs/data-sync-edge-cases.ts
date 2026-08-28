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
  withSafeModeOff,
} from '../helpers.js';

async function openDataSyncWindow() {
  await browser.url('tauri://localhost/window.html?window=data-sync');
  await browser.pause(1500);
  await $('[data-testid="data-sync-compare"]').waitForDisplayed({ timeout: 10000 });
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

  it('DS-EDGE-001: 未选端点比较应提示 selectBoth', async () => {
    await openDataSyncWindow();
    await $('[data-testid="data-sync-compare"]').click();
    await browser.pause(500);
    const err = await $('[data-testid="data-sync-error"]');
    await expect(err).toBeDisplayed();
    expect(await err.getText()).toContain(t('sync.selectBoth'));
    await captureStep('ds-edge-01-select-both');
    await dismissOkDialog();
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
    await $('[data-testid="data-sync-compare"]').click();
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

  it('DS-EDGE-003: 比较按钮在仅选源端点时仍提示 selectBoth', async () => {
    const STAMP = Date.now().toString(36);
    const ID = `e2e_ds_src_only_${STAMP}`;
    await invokeBackend('save_connection', {
      config: pgConfig(ID, `DS-SrcOnly-${STAMP}`, 'datazen_sync_src'),
    });

    await openDataSyncWindow();
    await selectDzOption(t('sync.selectSource'), `DS-SrcOnly-${STAMP}`);
    await browser.pause(500);
    await $('[data-testid="data-sync-compare"]').click();
    await browser.pause(500);
    const err = await $('[data-testid="data-sync-error"]');
    await expect(err).toBeDisplayed();
    expect(await err.getText()).toContain(t('sync.selectBoth'));
    await captureStep('ds-edge-03-source-only');
    await dismissOkDialog();

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

  it('DS-EDGE-005: 勾选 Delete 应弹出确认对话框', async () => {
    await openDataSyncWindow();
    const deleteOpt = await $('[data-testid="data-sync-option-delete"]');
    await deleteOpt.click();
    await browser.pause(400);
    const body = await $('body').getText();
    expect(body).toContain(t('sync.deleteConfirmTitle'));
    expect(body).toContain(t('sync.deleteConfirmBody'));
    await captureStep('ds-edge-05-delete-confirm');
    const cancel = await $(`button*=${t('common.cancel')}`);
    if (await cancel.isDisplayed()) {
      await cancel.click();
      await browser.pause(200);
    }
    expect(await deleteOpt.isSelected()).toBe(false);
  });

  it('DS-EDGE-006: Swap 端点不应触发错误对话框', async () => {
    await openDataSyncWindow();
    await $('[data-testid="data-sync-swap"]').click();
    await browser.pause(400);
    const err = await $('[data-testid="data-sync-error"]');
    expect(await err.isDisplayed().catch(() => false)).toBe(false);
    await captureStep('ds-edge-06-swap');
  });
});

describe('数据同步比较后边界 (DS-EDGE-POST)', () => {
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_ds_edge_src_${STAMP}`;
  const TGT_ID = `e2e_ds_edge_tgt_${STAMP}`;
  const SRC_NAME = `DS-Edge-Src-${STAMP}`;
  const TGT_NAME = `DS-Edge-Tgt-${STAMP}`;
  const TABLE = `ds_edge_${STAMP}`;

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
    await $('[data-testid="data-sync-compare"]').click();
    await browser.pause(2500);
    await $('[data-testid="data-sync-summary"]').waitForDisplayed({ timeout: 20000 });
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
    const insertOpt = await $('[data-testid="data-sync-option-insert"]');
    if (await insertOpt.isSelected()) {
      await insertOpt.click();
      await browser.pause(300);
    }
    await expect(await $('[data-testid="data-sync-start-disabled"]')).toBeDisplayed();
    expect(await $('[data-testid="data-sync-start-disabled"]').getAttribute('title')).toContain(
      t('sync.executeUnavailable'),
    );
    await captureStep('ds-edge-post-01-insert-off-execute-disabled');
    await insertOpt.click();
    await browser.pause(300);
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
    const toggled = await browser.execute((tableName: string) => {
      const rows = document.querySelectorAll('[data-testid="data-sync-mapping-row"]');
      for (const row of rows) {
        if ((row.textContent || '').includes(tableName)) {
          const cb = row.querySelector('input[type="checkbox"]') as HTMLInputElement | null;
          cb?.click();
          return true;
        }
      }
      return false;
    }, TABLE);
    expect(toggled).toBe(true);
    await browser.pause(400);
    await expect(await $('[data-testid="data-sync-start-disabled"]')).toBeDisplayed();
    await captureStep('ds-edge-post-03-mapping-unchecked');
    await browser.execute((tableName: string) => {
      const rows = document.querySelectorAll('[data-testid="data-sync-mapping-row"]');
      for (const row of rows) {
        if ((row.textContent || '').includes(tableName)) {
          const cb = row.querySelector('input[type="checkbox"]') as HTMLInputElement | null;
          cb?.click();
        }
      }
    }, TABLE);
    await browser.pause(300);
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
});
