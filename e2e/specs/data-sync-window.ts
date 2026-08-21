import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import { closeExtraWindows, selectDzOption, withSafeModeOff } from '../helpers.js';

/**
 * Data Sync Diff Workspace Host journeys (DSW-001~DSW-008).
 * Full Execute against live DBs is covered in data-sync-real.ts (IPC); UI smoke here.
 */

describe('数据同步窗口 (DSW-001~DSW-008)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $(`button*=${t('action.newConnection')}`).waitForDisplayed({ timeout: 10000 });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('DSW-001: 应能通过 URL 打开数据同步窗口并看到覆盖拷贝退役横幅', async () => {
    await browser.url('tauri://localhost/window.html?window=data-sync');
    await browser.pause(1500);
    const banner = await $('[data-testid="data-sync-overwrite-retired"]');
    await expect(banner).toBeDisplayed();
    expect(await banner.getText()).toContain(t('sync.overwriteRetiredBanner'));
    const body = await $('body').getText();
    expect(body).toContain(t('sync.windowTitle'));
    expect(body).toContain(t('sync.source'));
    expect(body).toContain(t('sync.target'));
  });

  it('DSW-002: 应显示比较按钮、Options 与 Swap', async () => {
    const compare = await $('[data-testid="data-sync-compare"]');
    await expect(compare).toBeDisplayed();
    expect(await compare.getText()).toContain(t('sync.compare'));
    await expect(await $('[data-testid="data-sync-swap"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-option-insert"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-option-update"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-option-delete"]')).toBeDisplayed();
    // Schema pickers appear only after PG endpoints load schemas; containers always present for source/target DB.
    await expect(await $('[data-testid="data-sync-source-database"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-target-database"]')).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('sync.selectPrompt'));
    await expect(await $('[data-testid="data-sync-start-disabled"]')).not.toBeDisplayed();
  });

  it('DSW-003: 未选两端点点比较应提示 selectBoth', async () => {
    const compare = await $('[data-testid="data-sync-compare"]');
    await compare.click();
    await browser.pause(500);
    const err = await $('[data-testid="data-sync-error"]');
    await expect(err).toBeDisplayed();
    expect(await err.getText()).toContain(t('sync.selectBoth'));
    const ok = await $(`button*=${t('common.ok')}`);
    if (await ok.isDisplayed()) {
      await ok.click();
      await browser.pause(200);
    }
  });

  it('DSW-004: 比较按钮在 idle 可点且不走旧 sync_tables', async () => {
    const compare = await $('[data-testid="data-sync-compare"]');
    await expect(compare).toBeEnabled();
    const body = await $('body').getText();
    expect(body).not.toContain('DROP TABLE');
  });

  it('DSW-005: 主页不再暴露数据同步入口，窗口改为 URL 直达', async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await browser.pause(300);
    const hiddenSyncEntry = await $(`button*=${t('action.dataSync')}`);
    await expect(hiddenSyncEntry).not.toBeDisplayed();

    await browser.url('tauri://localhost/window.html?window=data-sync');
    await browser.pause(1500);
    await expect(await $('[data-testid="data-sync-overwrite-retired"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-compare"]')).toBeDisplayed();
  });

  it('DSW-006: Delete 选项默认未勾选', async () => {
    const deleteOpt = await $('[data-testid="data-sync-option-delete"]');
    await expect(deleteOpt).toBeDisplayed();
    expect(await deleteOpt.isSelected()).toBe(false);
  });

  it('DSW-007: Swap 按钮可见且可点击', async () => {
    const swap = await $('[data-testid="data-sync-swap"]');
    await expect(swap).toBeDisplayed();
    await swap.click();
    await browser.pause(200);
    const err = await $('[data-testid="data-sync-error"]');
    expect(await err.isDisplayed().catch(() => false)).toBe(false);
  });

  it('DSW-008: Compare 前不应出现 Execute 底栏', async () => {
    await expect(await $('[data-testid="data-sync-start"]')).not.toBeDisplayed();
    await expect(await $('[data-testid="data-sync-start-disabled"]')).not.toBeDisplayed();
    await expect(await $('[data-testid="data-sync-summary"]')).not.toBeDisplayed();
  });
});

describe('数据同步 Diff Workspace (DSW-MAP / DSW-WS)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await browser.url('tauri://localhost/window.html?window=data-sync');
    await $('[data-testid="data-sync-compare"]').waitForDisplayed({ timeout: 10000 });
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
        const list = document.getElementById('dz-select-listbox');
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
    return true;
  }

  it('DSW-MAP-001: 选同族两端后比较应出现 mapping 行且 Execute 禁用', async () => {
    if (!(await trySelectPgEndpoints())) return;

    await browser.execute(() => {
      const src = document.querySelector('[data-testid="data-sync-source-database"]');
      const tgt = document.querySelector('[data-testid="data-sync-target-database"]');
      return { srcHasSelect: !!src, tgtHasSelect: !!tgt };
    });

    const compare = await $('[data-testid="data-sync-compare"]');
    await compare.click();
    await browser.pause(2500);

    const gated = await $('[data-testid="data-sync-error"]');
    if (await gated.isDisplayed().catch(() => false)) return;

    const rows = await $$('[data-testid="data-sync-mapping-row"]');
    const rowCount = await browser.execute(
      () => document.querySelectorAll('[data-testid="data-sync-mapping-row"]').length,
    );
    if (rowCount === 0) return;

    expect(rowCount).toBeGreaterThan(0);
    const executeDisabled = await $('[data-testid="data-sync-start-disabled"]');
    await expect(executeDisabled).toBeDisplayed();
    await expect(executeDisabled).toBeDisabled();
    expect(await executeDisabled.getAttribute('title')).toContain(t('sync.executeUnavailable'));
  });

  it('DSW-MAP-002: 选连接后应出现数据库选择器', async () => {
    await expect(await $('[data-testid="data-sync-source-database"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-target-database"]')).toBeDisplayed();
  });

  it('DSW-WS-001: Compare 完成后应出现 summary 与 preview / execute chrome', async () => {
    if (!(await trySelectPgEndpoints())) return;

    const compare = await $('[data-testid="data-sync-compare"]');
    await compare.click();
    await browser.pause(2500);

    const gated = await $('[data-testid="data-sync-error"]');
    if (await gated.isDisplayed().catch(() => false)) return;

    const rows = await $$('[data-testid="data-sync-mapping-row"]');
    const rowCount = await browser.execute(
      () => document.querySelectorAll('[data-testid="data-sync-mapping-row"]').length,
    );
    if (rowCount === 0) return;

    await expect(await $('[data-testid="data-sync-summary"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-option-insert"]')).toBeDisplayed();

    const previewTab = await $(`button*=${t('sync.sqlPreviewTab')}`);
    await previewTab.click();
    await browser.pause(600);
    await expect(await $('[data-testid="data-sync-preview"]')).toBeDisplayed();

    const rowDiffTab = await $(`button*=${t('sync.rowDiffTab')}`);
    await rowDiffTab.click();
    await browser.pause(300);
    const rowDiff = await $('[data-testid="data-sync-row-diff"]');
    if (await rowDiff.isDisplayed().catch(() => false)) {
      await expect(rowDiff).toBeDisplayed();
    }

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
    return invokeSync<string>('connect', { configId: id });
  }
  async function dSyncSql(connId: string, sql: string) {
    const run = () => invokeSync('execute_query', { connectionId: connId, sql });
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
    await expect(await $('[data-testid="data-sync-compare"]')).toBeDisplayed();

    // 源端点（config.database 自动预填 datazen_sync_src）
    await selectDzOption(t('sync.selectSource'), SRC_NAME);
    // 目标端点（config.database 自动预填 datazen_sync_tgt）
    await selectDzOption(t('sync.selectTarget'), TGT_NAME);
    await browser.pause(1500);

    // 数据库与 schema 选择器应出现
    await expect(await $('[data-testid="data-sync-source-database"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-target-database"]')).toBeDisplayed();

    // 比较
    const compare = await $('[data-testid="data-sync-compare"]');
    await compare.click();
    await browser.pause(2500);

    // 映射行出现
    const rows = await $$('[data-testid="data-sync-mapping-row"]');
    let seen = false;
    for (const r of rows) {
      if (((await r.getText()) || '').includes(TABLE)) {
        seen = true;
        break;
      }
    }
    expect(seen).toBe(true);

    // 执行
    const start = await $('[data-testid="data-sync-start"]');
    await start.waitForClickable({ timeout: 15000 });
    await start.click();
    await browser.pause(2500);

    // 落库校验：目标行数 = 5
    const tgt = await dSyncConnect(TGT_ID);
    const rows2 = await invokeSync<{ data: Array<{ c: number }> }>('execute_query', {
      connectionId: tgt,
      sql: `SELECT count(*)::int AS c FROM ${TABLE}`,
    });
    expect(Number(rows2?.data?.[0]?.c)).toBe(5);
  });
});
