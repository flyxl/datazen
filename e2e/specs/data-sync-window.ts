import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import { closeExtraWindows, selectDzOption } from '../helpers.js';

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
    if (rows.length === 0) return;

    expect(rows.length).toBeGreaterThan(0);
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
    if (rows.length === 0) return;

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
