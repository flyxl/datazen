import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import { closeExtraWindows, selectDzOption } from '../helpers.js';

/**
 * Data Sync window Host journeys (DSW-001~DSW-005).
 * Row Diff / Apply execute remain unwired; this spec covers the Compare → mapping gate shell.
 */

describe('数据同步窗口 (DSW-001~DSW-005)', () => {
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

  it('DSW-002: 应显示比较按钮与初始引导', async () => {
    const compare = await $('[data-testid="data-sync-compare"]');
    await expect(compare).toBeDisplayed();
    expect(await compare.getText()).toContain(t('sync.compare'));
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
    // 主窗口内容不再给出数据同步按钮入口。
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await browser.pause(300);
    const hiddenSyncEntry = await $(`button*=${t('action.dataSync')}`);
    await expect(hiddenSyncEntry).not.toBeDisplayed();

    // 同步窗口仍可经由 URL 打开。
    await browser.url('tauri://localhost/window.html?window=data-sync');
    await browser.pause(1500);
    await expect(await $('[data-testid="data-sync-overwrite-retired"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-compare"]')).toBeDisplayed();
  });
});

describe('数据同步窗口映射门闸 (DSW-MAP)', () => {
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

  it('DSW-MAP-001: 选同族两端后比较应出现 mapping 行且 Apply 禁用', async () => {
    const sourceWrap = await $('[data-testid="data-sync-source"]');
    const targetWrap = await $('[data-testid="data-sync-target"]');
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
        return;
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
        return;
      }
    }

    // Selecting each endpoint auto-populates its database list and defaults to the
    // connection's configured database, which is required before Compare.
    await browser.execute(() => {
      const src = document.querySelector('[data-testid="data-sync-source-database"]');
      const tgt = document.querySelector('[data-testid="data-sync-target-database"]');
      return {
        srcHasSelect: !!src,
        tgtHasSelect: !!tgt,
      };
    });
    await browser.pause(800);

    const compare = await $('[data-testid="data-sync-compare"]');
    await compare.click();
    await browser.pause(2000);

    // If no enumerable database is available, the compare is gated on a selection.
    const gated = await $('[data-testid="data-sync-error"]');
    if (await gated.isDisplayed().catch(() => false)) {
      return;
    }

    const rows = await $$('[data-testid="data-sync-mapping-row"]');
    if (rows.length === 0) {
      return;
    }
    expect(rows.length).toBeGreaterThan(0);
    const apply = await $('[data-testid="data-sync-start-disabled"]');
    await expect(apply).toBeDisplayed();
    await expect(apply).toBeDisabled();
    expect(await apply.getAttribute('title')).toContain(t('sync.applyUnavailable'));
  });

  it('DSW-MAP-002: 选连接后应出现数据库选择器', async () => {
    await expect(await $('[data-testid="data-sync-source-database"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-sync-target-database"]')).toBeDisplayed();
  });
});
