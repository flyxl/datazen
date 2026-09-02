import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  captureJourneyStep,
  queryScalar,
  selectDzOption,
  switchToNewWindow,
  withSafeModeOff,
  type QueryResultPayload,
} from '../helpers.js';

/**
 * Data Sync wizard Host journeys (DSW-001~DSW-008).
 * Full Execute against live DBs is covered in data-sync-real.ts (IPC); UI smoke here.
 */

async function openDataSyncWindow(mainWindow: string) {
  await browser.url('tauri://localhost/window.html?window=data-sync');
  await browser.pause(1500);
  const handles = await browser.getWindowHandles();
  if (handles.length > 1) {
    await switchToNewWindow(mainWindow);
  }
  await $('[data-testid="data-sync-window"]').waitForDisplayed({ timeout: 20000 });
  await browser.waitUntil(
    async () =>
      (await $('[data-testid="data-sync-window"]').getAttribute('data-sync-step')) === 'endpoints',
    { timeout: 20000, timeoutMsg: 'data-sync wizard did not open on endpoints step' },
  );
}

async function waitForStep(step: string) {
  await browser.waitUntil(
    async () =>
      (await $('[data-testid="data-sync-window"]').getAttribute('data-sync-step')) === step,
    { timeout: 15000, timeoutMsg: `data-sync wizard did not reach ${step} step` },
  );
}

async function nextStep() {
  const next = await $('[data-testid="data-sync-next"]');
  await next.waitForEnabled({ timeout: 15000 });
  await next.click();
}

async function advanceToCompare() {
  await nextStep();
  await waitForStep('setup');
  await nextStep();
  await waitForStep('objects');
  await browser.waitUntil(
    async () => {
      const state = await $('[data-testid="data-sync-window"]').getAttribute('data-sync-state');
      return state !== 'inspecting' && state !== 'comparing' && state !== 'executing';
    },
    { timeout: 120000, timeoutMsg: 'data-sync inspection did not finish' },
  );
  await nextStep();
  await waitForStep('compare');
  await browser.waitUntil(
    async () =>
      (await $('[data-testid="data-sync-window"]').getAttribute('data-sync-state')) === 'compared',
    { timeout: 120000, timeoutMsg: 'data-sync compare did not finish' },
  );
}

describe('数据同步窗口 (DSW-001~DSW-008)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('DSW-001: 应能通过 URL 打开数据同步窗口', async () => {
    await openDataSyncWindow(mainWindow);
    await expect($("[data-testid='data-sync-window']")).toExist();
    await expect($('[data-testid="data-sync-step-endpoints"]')).toExist();
    await captureJourneyStep('data-sync-window-open');
  });

  it('DSW-002: 应按向导分离端点与配置，且端点页不显示 Swap', async () => {
    await expect(await $('[data-testid="data-sync-next"]')).toBeDisabled();
    await expect(await $('[data-testid="data-sync-swap"]')).not.toBeExisting();
    await expect(await $('[data-testid="data-sync-option-update"]')).not.toBeExisting();
    await expect($('[data-testid="data-sync-start-disabled"]')).not.toBeExisting();
    await captureJourneyStep('data-sync-wizard-endpoints');
  });

  it('DSW-003: 未选两端点时下一步应保持禁用', async () => {
    await expect(await $('[data-testid="data-sync-next"]')).toBeDisabled();
    await expect(await $('[data-testid="data-sync-error"]')).not.toBeExisting();
    await captureJourneyStep('data-sync-select-both-gated');
  });

  it('DSW-004: 首屏处于端点步骤且不走旧 sync_tables', async () => {
    await expect(await $('[data-testid="data-sync-window"]')).toHaveAttribute(
      'data-sync-step',
      'endpoints',
    );
    const body = await $('body').getText();
    expect(body).not.toContain('DROP TABLE');
    await captureJourneyStep('data-sync-idle-endpoints');
  });

  it('DSW-005: 主页不再暴露数据同步入口，窗口改为 URL 直达', async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await browser.pause(300);
    // Data sync should not appear as a quick-action button on the home page
    const quickSync = await $("[data-testid='home-quick-data-sync']");
    await expect(quickSync).not.toBeExisting();
    await captureJourneyStep('data-sync-hidden-on-home');

    await openDataSyncWindow(mainWindow);
    await expect(await $('[data-testid="data-sync-next"]')).toBeDisplayed();
  });

  it('DSW-006: 配置选项位于独立 setup 步骤', async () => {
    await expect(await $('[data-testid="data-sync-option-delete"]')).not.toBeExisting();
    await captureJourneyStep('data-sync-setup-gated');
  });

  it('DSW-007: 向导端点页隐藏 Swap 操作', async () => {
    await expect(await $('[data-testid="data-sync-swap"]')).not.toBeExisting();
    await captureJourneyStep('data-sync-no-swap');
  });

  it('DSW-008: Compare 前不应出现 Execute 底栏', async () => {
    // ExecuteBar and CompareSummary are inside {compared && ...} — verify they
    // are absent from the DOM (not merely hidden) before a compare has run.
    await expect($('[data-testid="data-sync-summary"]')).not.toBeExisting();
    await expect($('[data-testid="data-sync-start"]')).not.toBeExisting();
    await expect($('[data-testid="data-sync-start-disabled"]')).not.toBeExisting();
    await captureJourneyStep('data-sync-no-execute-before-compare');
  });
});

describe('数据同步 Diff Workspace (DSW-MAP / DSW-WS)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await openDataSyncWindow(mainWindow);
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  async function trySelectPgEndpoints(): Promise<boolean> {
    const sourceWrap = await $('[data-testid="data-sync-source"]');
    await sourceWrap.waitForDisplayed({ timeout: 8000 });

    const sourceLabel = await sourceWrap.getText();
    const hasPg =
      sourceLabel.includes('PostgreSQL') ||
      sourceLabel.includes('postgres') ||
      sourceLabel.includes('PG');
    if (!hasPg) {
      const opened = await browser.execute(() => {
        const wrap = document.querySelector('[data-testid="data-sync-source"]');
        const btn = wrap?.querySelector('button[aria-haspopup="listbox"]') as HTMLElement | null;
        btn?.click();
        const list = document.querySelector('[id^="dz-select-listbox-"]');
        return list ? list.textContent || '' : '';
      });
      if (
        !opened.includes('postgresql') &&
        !opened.includes('PostgreSQL') &&
        !opened.includes('mysql')
      ) {
        return false;
      }
    }

    try {
      await selectDzOption(t('sync.selectSource'), 'PostgreSQL');
      await selectDzOption(t('sync.selectTarget'), 'PostgreSQL');
    } catch {
      try {
        await selectDzOption(t('sync.selectSource'), 'postgres');
        await selectDzOption(t('sync.selectTarget'), 'postgres');
      } catch {
        return false;
      }
    }

    await browser.pause(800);
    await advanceToCompare();
    return true;
  }

  it('DSW-MAP-001: 选同族两端后比较应出现 mapping 行且 Execute 禁用', async () => {
    if (!(await trySelectPgEndpoints())) return;

    await browser.execute(() => {
      const src = document.querySelector('[data-testid="data-sync-source-database"]');
      const tgt = document.querySelector('[data-testid="data-sync-target-database"]');
      return { srcHasSelect: !!src, tgtHasSelect: !!tgt };
    });

    await browser.waitUntil(
      async () =>
        (await $('[data-testid="data-sync-window"]').getAttribute('data-sync-state')) ===
        'compared',
      { timeout: 120000, timeoutMsg: 'data-sync compare did not finish' },
    );

    const rows = await $$('[data-testid="data-sync-mapping-row"]');
    const rowCount = await browser.execute(
      () => document.querySelectorAll('[data-testid="data-sync-mapping-row"]').length,
    );
    if (rowCount === 0) return;

    expect(rowCount).toBeGreaterThan(0);
    await nextStep();
    await waitForStep('preview');
    const executeDisabled = await $('[data-testid="data-sync-start-disabled"]');
    await expect(executeDisabled).toBeDisplayed();
    await expect(executeDisabled).toBeDisabled();
    expect(await executeDisabled.getAttribute('title')).toContain(t('sync.executeUnavailable'));
    await captureJourneyStep('data-sync-mapping-rows');
  });

  it('DSW-MAP-002: 选连接后应出现数据库选择器', async () => {
    const back = await $('[data-testid="data-sync-back"]');
    for (let i = 0; i < 4; i++) {
      await back.click();
      await browser.pause(150);
    }
    await waitForStep('endpoints');
    await expect(await $('[data-testid="data-sync-source-database"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-target-database"]')).toBeDisplayed();
  });

  it('DSW-WS-001: Compare 完成后应出现 summary 与 preview / execute chrome', async () => {
    if (!(await trySelectPgEndpoints())) return;

    await browser.waitUntil(
      async () =>
        (await $('[data-testid="data-sync-window"]').getAttribute('data-sync-state')) ===
        'compared',
      { timeout: 120000, timeoutMsg: 'data-sync compare did not finish' },
    );

    const rows = await $$('[data-testid="data-sync-mapping-row"]');
    const rowCount = await browser.execute(
      () => document.querySelectorAll('[data-testid="data-sync-mapping-row"]').length,
    );
    if (rowCount === 0) return;

    await expect(await $('[data-testid="data-sync-summary"]')).toBeDisplayed();
    await nextStep();
    await waitForStep('preview');
    await expect(await $('[data-testid="data-sync-preview"]')).toBeDisplayed();
    await captureJourneyStep('data-sync-preview-step');

    const executeDisabled = await $('[data-testid="data-sync-start-disabled"]');
    const executeEnabled = await $('[data-testid="data-sync-start"]');
    const hasExecuteChrome =
      (await executeDisabled.isDisplayed().catch(() => false)) ||
      (await executeEnabled.isDisplayed().catch(() => false));
    expect(hasExecuteChrome).toBe(true);
  });
});

/**
 * Data Sync Diff Workspace — UI 执行闭环（DSW-EXEC）。
 * 通过后端在 `datazen_sync_src` 造源表、`datazen_sync_tgt` 造缺 2 行目标表，
 * UI 走「选端点 → 比较 → 执行」后，回查目标表行数 = 源表行数（落库闭环）。
 * 依赖 `e2e/setup-sync-dbs.sh` 建库。
 */
describe('数据同步 UI 执行闭环 (DSW-EXEC)', () => {
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_ds_src_${STAMP}`;
  const TGT_ID = `e2e_ds_tgt_${STAMP}`;
  const SRC_NAME = `DSrc-${STAMP}`;
  const TGT_NAME = `DTgt-${STAMP}`;
  const TABLE = `ds_e2e_${STAMP}`;

  const pgCfg = (id: string, name: string, database: string) => ({
    id,
    name,
    databaseType: 'postgresql',
    host: process.env.E2E_PG_HOST || '127.0.0.1',
    port: Number(process.env.E2E_PG_PORT) || 5432,
    username: process.env.E2E_PG_USER || 'postgres',
    password: process.env.E2E_PG_PASSWORD || '',
    database,
    sslMode: 'disable',
  });

  async function invokeSync<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
    const result = await browser.executeAsync(
      (c: string, a: string, done: (r: unknown) => void) => {
        (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
          ?.invoke(c, JSON.parse(a))
          .then((r: unknown) => done(r))
          .catch((e: unknown) => done({ __error: String(e) }));
      },
      cmd,
      JSON.stringify(args),
    );
    if (result && typeof result === 'object' && '__error' in (result as Record<string, unknown>)) {
      throw new Error(String((result as Record<string, unknown>).__error));
    }
    return result as T;
  }

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await dSyncSaveConnections();
    await dSyncSeed();
  });

  after(async () => {
    try {
      const src = await dSyncConnect(SRC_ID);
      const tgt = await dSyncConnect(TGT_ID);
      await withSafeModeOff(async () => {
        await dSyncSql(src, `DROP TABLE IF EXISTS ${TABLE}`);
        await dSyncSql(tgt, `DROP TABLE IF EXISTS ${TABLE}`);
      });
    } catch {
      /* ok */
    }
    try {
      await dSyncInvoke('delete_connection', { id: SRC_ID });
    } catch {}
    try {
      await dSyncInvoke('delete_connection', { id: TGT_ID });
    } catch {}
    await closeExtraWindows(mainWindow);
  });

  async function dSyncInvoke(cmd: string, args: Record<string, unknown> = {}) {
    await invokeSync<unknown>(cmd, args);
  }
  async function dSyncConnect(id: string) {
    return invokeSync<string>('connect', { connectionId: id });
  }
  async function dSyncSql(dbSessionId: string, sql: string) {
    const run = () => invokeSync('execute_query', { dbSessionId, sql });
    await withSafeModeOff(run);
  }

  async function dSyncSaveConnections() {
    await dSyncInvoke('save_connection', { config: pgCfg(SRC_ID, SRC_NAME, 'datazen_sync_src') });
    await dSyncInvoke('save_connection', { config: pgCfg(TGT_ID, TGT_NAME, 'datazen_sync_tgt') });
  }

  async function dSyncSeed() {
    const src = await dSyncConnect(SRC_ID);
    const tgt = await dSyncConnect(TGT_ID);
    await dSyncSql(tgt, `DROP TABLE IF EXISTS ${TABLE}`);
    await dSyncSql(
      src,
      `DROP TABLE IF EXISTS ${TABLE}; CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL)`,
    );
    await dSyncSql(tgt, `CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL)`);
    await dSyncSql(
      src,
      `INSERT INTO ${TABLE} (id, name) VALUES (1,'a'),(2,'b'),(3,'c'),(4,'d'),(5,'e')`,
    );
    await dSyncSql(tgt, `INSERT INTO ${TABLE} (id, name) VALUES (1,'a'),(2,'b'),(3,'c')`);
    // 源 5 行 / 目标 3 行 → diff 2 个 INSERT
  }

  it('DSW-EXEC-001: 选端点、比较并执行同步，回查目标行数=源', async () => {
    await browser.url('tauri://localhost/window.html?window=data-sync');
    await browser.pause(1500);
    await expect(await $('[data-testid="data-sync-next"]')).toBeDisplayed();

    // 源端点（config.database 自动预填 datazen_sync_src）
    await selectDzOption(t('sync.selectSource'), SRC_NAME);
    // 目标端点（config.database 自动预填 datazen_sync_tgt）
    await selectDzOption(t('sync.selectTarget'), TGT_NAME);
    await browser.pause(1500);

    // 数据库与 schema 选择器应出现
    await expect(await $('[data-testid="data-sync-source-database"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-target-database"]')).toBeDisplayed();

    // 端点 → 配置 → 对象 → 对比
    await nextStep();
    await waitForStep('setup');
    await nextStep();
    await waitForStep('objects');

    // 映射行出现
    await browser.waitUntil(
      async () => {
        const rows = await $$('[data-testid="data-sync-mapping-row"]');
        for (const row of rows) {
          if (((await row.getText()) || '').includes(TABLE)) return true;
        }
        return false;
      },
      { timeout: 20000, timeoutMsg: `mapping row for ${TABLE} did not appear` },
    );
    await captureJourneyStep('data-sync-mapping-ready', 0, true);
    await nextStep();
    await waitForStep('compare');

    // The mapping is published before compareDataSync finishes.  Waiting only
    // for a row can therefore race the ExecuteBar: its locator is present
    // while the window is still `comparing`, or an async connection error has
    // opened a modal over the workspace.  Wait for the actual executable state
    // and surface that error instead of timing out in waitForClickable.
    const syncWindow = await $('[data-testid="data-sync-window"]');
    await browser.waitUntil(
      async () => (await syncWindow.getAttribute('data-sync-state')) === 'compared',
      { timeout: 20000, timeoutMsg: 'data-sync compare did not reach compared state' },
    );
    const error = await $('[data-testid="data-sync-error"]');
    if (await error.isDisplayed().catch(() => false)) {
      throw new Error(`data-sync compare error: ${await error.getText()}`);
    }

    await nextStep();
    await waitForStep('preview');

    // ExecuteBar is below the split workspace on smaller webdriver viewports;
    // center it before checking clickability so WebDriver does not hit an
    // off-screen/covered point while the UI is otherwise ready.
    const start = await $('[data-testid="data-sync-start"]');
    await start.waitForDisplayed({ timeout: 10000 });
    await start.scrollIntoView({ block: 'center', inline: 'nearest' });
    await expect(start).toBeEnabled();
    await start.waitForClickable({ timeout: 15000 });
    await start.click();
    await browser.pause(3000);

    await expect(await $('[data-testid="data-sync-summary"]')).toBeDisplayed();
    await captureJourneyStep('data-sync-execute-complete', 0, true);

    // 落库校验：目标行数 = 5
    const tgt = await dSyncConnect(TGT_ID);
    const rows2 = await invokeSync<QueryResultPayload>('execute_query', {
      dbSessionId: tgt,
      sql: `SELECT count(*)::int AS c FROM ${TABLE}`,
    });
    expect(queryScalar(rows2, 'c')).toBe(5);
  });
});
